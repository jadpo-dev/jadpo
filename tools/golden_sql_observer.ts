// Checkpoint-only observation adapter. The generated SQL import is redirected
// here; the exact original and instrumented files are retained and hashed.
import { SQL as NativeSQL } from "bun";

export type StatementObservation = { sql: string; operation: string };
let active: StatementObservation[] | undefined;
let authenticationAttempts = 0;
let authenticatedPrincipal: { kind: "user" | "service"; subject: string; identityId: string } | undefined;
let captureTodoLimitBindings = false;
let todoLimitBindings: number[] = [];
export function recordAuthenticationAttempt() { if (active) authenticationAttempts++; }
export function authenticationAttemptCount() { return authenticationAttempts; }
export function recordAuthenticatedPrincipal(principal: any) {
  if (!active || !principal || typeof principal.subject !== "string" || typeof principal.values !== "object") return;
  if (principal.kind === "user" && typeof principal.values.user_id === "string") {
    authenticatedPrincipal = { kind: "user", subject: principal.subject, identityId: principal.values.user_id };
  } else if (principal.kind === "service" && typeof principal.values.service_id === "string") {
    authenticatedPrincipal = { kind: "service", subject: principal.subject, identityId: principal.values.service_id };
  }
}
export function authenticatedPrincipalObservation() { return authenticatedPrincipal ?? null; }
export function beginSqlObservation(captureTodoLimit = false) {
  active = [];
  authenticationAttempts = 0;
  authenticatedPrincipal = undefined;
  captureTodoLimitBindings = captureTodoLimit;
  todoLimitBindings = [];
  return active;
}
export function endSqlObservation() { active = undefined; }
export function todoLimitBindingObservation() { return [...todoLimitBindings]; }
export function recordStatement(sql: string, values: unknown[] = []) {
  if (!active) return;
  // Never retain bind values or literal credentials in observation evidence.
  const safe = sql.replace(/'(?:''|[^'])*'/gu, "'<literal>'");
  active.push({ sql: safe, operation: /^\s*([a-z]+)/iu.exec(safe)?.[1].toUpperCase() ?? "UNKNOWN" });
  if (captureTodoLimitBindings && /\bfrom\s+"?todo"?\b/iu.test(safe)) {
    const limit = /\blimit\s+(\$(\d+)|\?(\d*)|(\d+))/iu.exec(safe);
    if (limit) {
      let boundValues = values;
      if (boundValues.length === 1 && Array.isArray(boundValues[0])) boundValues = boundValues[0];
      const parameterIndex = limit[2] ? Number(limit[2]) - 1
        : limit[3] ? Number(limit[3]) - 1
        : limit[4] ? -1
        : [...safe.slice(0, limit.index).matchAll(/\$\d+|\?\d*/gu)].length;
      const value = parameterIndex === -1 ? Number(limit[4]) : boundValues[parameterIndex];
      if (Number.isSafeInteger(value) && value > 0) todoLimitBindings.push(value);
    }
  }
}
function observeConnection(connection: any): any {
  return new Proxy(connection, {
    apply(target, receiver, arguments_) {
      const strings = arguments_[0];
      if (!Array.isArray(strings)) throw new Error("Unsupported SQL dispatch");
      recordStatement(strings.join("?"), arguments_.slice(1));
      return Reflect.apply(target, target, arguments_);
    },
    get(target, property) {
      if (property === "unsafe") return (sql: string, ...values: unknown[]) => {
        recordStatement(sql, values);
        return target.unsafe(sql, ...values);
      };
      if (property === "begin") return async (...arguments_: any[]) => {
        const callback = arguments_.pop();
        if (typeof callback !== "function") throw new Error("Unsupported transaction callback");
        // A callback is entered after BEGIN. Bun.SQL rolls back when the
        // callback rejects, so retain that transaction outcome; successful
        // native resolution acknowledges COMMIT. Failed transaction control is
        // intentionally unsupported.
        const result = await target.begin(...arguments_, async (transaction: any) => {
          recordStatement("BEGIN");
          try { return await callback(observeConnection(transaction)); }
          catch (error) { recordStatement("ROLLBACK"); throw error; }
        });
        recordStatement("COMMIT");
        return result;
      };
      // Refuse unobserved protocol/API expansion. No selected case exercises
      // reserved/file/distributed SQL or transaction retry semantics.
      if (["transaction", "reserve", "file", "beginDistributed"].includes(String(property))) {
        return () => { throw new Error("Unsupported checkpoint SQL dispatch"); };
      }
      const value = Reflect.get(target, property, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}
export const SQL: typeof NativeSQL = new Proxy(NativeSQL, {
  construct(target, arguments_) { return observeConnection(Reflect.construct(target, arguments_)); },
});
