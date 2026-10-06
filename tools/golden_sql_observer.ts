// Checkpoint-only observation adapter. The generated SQL import is redirected
// here; the exact original and instrumented files are retained and hashed.
import { SQL as NativeSQL } from "bun";

export type StatementObservation = { sql: string; operation: string; numericParameters: Record<string, number>; maintenanceCutoff?: string; maintenanceRowIdentity?: string; relationshipRowIdentity?: string; relationshipOwnerIdentity?: string; reminderRowIdentity?: string };
let active: StatementObservation[] | undefined;
let authenticationAttempts = 0;
let principals: unknown[] = [];
let credentialsIssued = 0;
let sessionStorageUnavailable = false;
let storageReads: { operation: string; outcome: string }[] = [];
let jwtVerifications: unknown[] = [];
let authorityPrincipals: unknown[] = [];
export function recordResolvedAuthorityPrincipal(principal: any) { if (active) authorityPrincipals.push({ kind: principal.kind, subject: principal.subject, authenticationStrength: principal.authenticationStrength, values: { ...principal.values } }); }
export function resolvedAuthorityPrincipals() { return authorityPrincipals; }
export function setSessionStorageUnavailable(value: boolean) { sessionStorageUnavailable = value; }
export function authenticationFaultConfiguration() { return { sessionStorageUnavailable }; }
export function authenticationStorageReads() { return storageReads; }
export function observeAuthenticationStorageRead() {
  if (active) storageReads.push({ operation: "session.get", outcome: sessionStorageUnavailable ? "faulted" : "delegated" });
  if (sessionStorageUnavailable) throw new Error("Checkpoint session dependency unavailable");
}
export function recordJwtVerification(value: unknown) { if (active) jwtVerifications.push(value); }
export function observedJwtVerifications() { return jwtVerifications; }
export function recordAuthenticationAttempt() { if (active) authenticationAttempts++; }
export function authenticationAttemptCount() { return authenticationAttempts; }
// Observe the actual normalized principal returned by the native boundary.
// These closed fields contain fixture identities, never credential material.
export function recordAuthenticatedPrincipal(principal: any) {
  if (active) principals.push({ kind: principal.kind, subject: principal.subject,
    authenticationStrength: principal.authenticationStrength, values: { ...principal.values } });
}
export function authenticatedPrincipals() { return principals; }
export function recordCredentialIssued() { if (active) credentialsIssued++; }
export function issuedCredentialCount() { return credentialsIssued; }
export function beginSqlObservation() { active = []; authenticationAttempts = 0; principals = []; credentialsIssued = 0; storageReads = []; jwtVerifications = []; authorityPrincipals = []; return active; }
export function endSqlObservation() { active = undefined; }
export function recordStatement(sql: string, parameters: unknown[] = []) {
  if (!active) return;
  // Never retain arbitrary bind values or literal credentials in evidence.
  const safe = sql.replace(/'(?:''|[^'])*'/gu, "'<literal>'");
  // Retain bounded integer limit/offset observations; strings, dates, secrets
  // and arbitrary driver parameter payloads remain absent from raw evidence.
  const numericParameters = Object.fromEntries(parameters.flatMap((value, index) => Number.isInteger(value) && Number(value) >= 0 && Number(value) <= 1000 ? [[String(index + 1), value as number]] : []));
  // The compiler-owned retention predicate has one typed Instant bind. Retain
  // only that value on its exact SELECT/DELETE shape, never arbitrary binds.
  let maintenanceCutoff: string | undefined;
  if (/^(?:SELECT "id", "deleted_at" FROM "todo"|DELETE FROM "todo")/u.test(safe) && /"deleted_at" <= [?$]1/u.test(safe)) {
    const value = parameters[0];
    if (typeof value === "number" && Number.isSafeInteger(value) || typeof value === "string" && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z$/u.test(value)) {
      const date = new Date(value); if (Number.isFinite(date.getTime())) maintenanceCutoff = date.toISOString();
    }
  }
  const maintenanceRowIdentity = /^DELETE FROM "todo" WHERE "id" = [?$]2 AND \(/u.test(safe) && typeof parameters[1] === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/u.test(parameters[1]) ? parameters[1] : undefined;
  // Exact original relationship operations retain only typed fixture UUIDs;
  // auth/service SQL and arbitrary strings never enter this observation.
  const fixtureUuid = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/u.test(value);
  const relationshipInsert = /^INSERT INTO todo \(id, owner_id,/u.test(safe);
  const relationshipDelete = /^DELETE FROM "user" WHERE id = [?$]1$/u.test(safe);
  const relationshipRowIdentity = (relationshipInsert || relationshipDelete) && fixtureUuid(parameters[0]) ? parameters[0] : undefined;
  const relationshipOwnerIdentity = relationshipInsert && fixtureUuid(parameters[1]) ? parameters[1] : undefined;
  // Capture only the exact native sent-mark source identity, never auth binds.
  const reminderRowIdentity = /^UPDATE "todo" SET "reminder_sent_at" = [?$]1, "updated_at" = [?$]2 WHERE "id" = [?$]3 AND "reminder_sent_at" IS NULL RETURNING \*$/u.test(safe) && typeof parameters[2] === "string" && /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/u.test(parameters[2]) ? parameters[2] : undefined;
  active.push({ sql: safe, operation: /^\s*([a-z]+)/iu.exec(safe)?.[1].toUpperCase() ?? "UNKNOWN", numericParameters, ...(maintenanceCutoff ? { maintenanceCutoff } : {}), ...(maintenanceRowIdentity ? { maintenanceRowIdentity } : {}), ...(relationshipRowIdentity ? { relationshipRowIdentity } : {}), ...(relationshipOwnerIdentity ? { relationshipOwnerIdentity } : {}), ...(reminderRowIdentity ? { reminderRowIdentity } : {}) });
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
