// Checkpoint-only observation adapter. The generated SQL import is redirected
// here; the exact original and instrumented files are retained and hashed.
import { SQL as NativeSQL } from "bun";

export type StatementObservation = { sql: string; operation: string; numericParameters: Record<string, number> };
let active: StatementObservation[] | undefined;
let authenticationAttempts = 0;
export function recordAuthenticationAttempt() { if (active) authenticationAttempts++; }
export function authenticationAttemptCount() { return authenticationAttempts; }
export function beginSqlObservation() { active = []; authenticationAttempts = 0; return active; }
export function endSqlObservation() { active = undefined; }
export function recordStatement(sql: string, parameters: unknown[] = []) {
  if (!active) return;
  // Never retain bind values or literal credentials in observation evidence.
  const safe = sql.replace(/'(?:''|[^'])*'/gu, "'<literal>'");
  // Retain bounded integer limit/offset observations; strings, dates, secrets
  // and arbitrary driver parameter payloads remain absent from raw evidence.
  const numericParameters = Object.fromEntries(parameters.flatMap((value, index) => Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 1000 ? [[String(index + 1), value as number]] : []));
  active.push({ sql: safe, operation: /^\s*([a-z]+)/iu.exec(safe)?.[1].toUpperCase() ?? "UNKNOWN", numericParameters });
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
        recordStatement(sql, Array.isArray(values[0]) ? values[0] : values);
        return target.unsafe(sql, ...values);
      };
      if (property === "begin") return async (...arguments_: any[]) => {
        const callback = arguments_.pop();
        if (typeof callback !== "function") throw new Error("Unsupported transaction callback");
        // A callback is entered after BEGIN, and native resolution acknowledges
        // COMMIT. Failed transaction control is intentionally unsupported.
        const result = await target.begin(...arguments_, async (transaction: any) => {
          recordStatement("BEGIN");
          return callback(observeConnection(transaction));
        });
        recordStatement("COMMIT");
        return result;
      };
      // Refuse unobserved protocol/API expansion. No selected case exercises
      // reserved/file/distributed SQL or transaction rollback/retry semantics.
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
