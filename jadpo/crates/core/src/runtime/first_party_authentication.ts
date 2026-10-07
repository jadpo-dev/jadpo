// Compiler-owned first-party credential boundary. No authored login endpoints.
import { authenticateRequest, AuthenticationFault, firstPartyAuthenticationStrength, type AuthPrincipal, type ResolutionResult, type ValidationResult } from "./authentication.ts";

type Settings = Readonly<{ name: string; principal?: "user" | "service"; mode: "signed" | "opaque" | "api_key"; cookie: string | null; audience: string; origin: string | null; secret: string; previousSecret?: string; maximumDelayMs: number }>;
type Session = Readonly<{ id: string; strategy: string; subject: string; userId: string; expires: number; verifier: string; keyId: string; revoked: boolean }>;
type ServiceCredential = Readonly<{ id: string; strategy: string; subject: string; serviceId: string; expires: number; verifier: string; keyId: string; revoked: boolean }>;
export type AuthenticationStorage = Readonly<{
  get(id: string): Promise<Session | null>;
  put(session: Session): Promise<void>;
  revoke(id: string): Promise<void>;
  getServiceCredential(id: string): Promise<ServiceCredential | null>;
  putServiceCredential(credential: ServiceCredential, now: number): Promise<void>;
  revokeServiceCredential(id: string, now: number): Promise<void>;
}>;
type Key = Readonly<{ id: string; key: CryptoKey }>;
type Configured = Settings & { keys: readonly Key[] };
type UserPrincipal = Extract<AuthPrincipal, { kind: "user" }>;
type Verified = { principal: AuthPrincipal; sessionId: string; credential: string; key: Key; strategy: Configured; serviceCredential?: ServiceCredential };
// Native compiler-worker handoff only. Neither an authored principal nor a
// serialized object is credential provenance. No raw key is retained here.
type DeliveryCredentialFacts = Readonly<{ id: string; strategy: string; subject: string; serviceId: string; verifier: string; keyId: string }>;
type DeliveryIssuer = { storage: AuthenticationStorage; owner: { active: boolean } };
const deliveryCredentialHosts = new WeakMap<object, DeliveryIssuer>();
const deliveryCredentialProofs = new WeakMap<object, DeliveryIssuer & { facts: DeliveryCredentialFacts }>();
// Privileged native operator provenance is separate from worker credentials.
// Only the selected current host/storage owner may mint or read these grants.
const deliveryOperatorGrants = new WeakMap<object, DeliveryIssuer>();
// __JADPO_RECONCILIATION_AUTHORITY_START__
// Accepted-proof reconciliation is a separate privileged purpose. Worker and
// known-no-effect replay provenance can never substitute for this grant.
const deliveryReceiptReconciliationGrants = new WeakMap<object, DeliveryIssuer>();
export function readDeliveryAcceptedEvidenceOwner(storage: AuthenticationStorage, selectedHost: unknown): void {
  const issuer = typeof selectedHost === "object" && selectedHost !== null ? deliveryCredentialHosts.get(selectedHost) : undefined;
  if (issuer === undefined || issuer.storage !== storage || !issuer.owner.active) return invalid();
}
export function readDeliveryReceiptReconciliationGrant(grant: unknown, storage: AuthenticationStorage, selectedHost: unknown): void {
  const record = typeof grant === "object" && grant !== null ? deliveryReceiptReconciliationGrants.get(grant) : undefined;
  const issuer = typeof selectedHost === "object" && selectedHost !== null ? deliveryCredentialHosts.get(selectedHost) : undefined;
  if (record === undefined || issuer === undefined || record.storage !== storage || issuer.storage !== storage
      || record.owner !== issuer.owner || !record.owner.active) return invalid();
}
// __JADPO_RECONCILIATION_AUTHORITY_END__
export function readDeliveryOperatorGrant(grant: unknown, storage: AuthenticationStorage, selectedHost: unknown): void {
  const record = typeof grant === "object" && grant !== null ? deliveryOperatorGrants.get(grant) : undefined;
  const issuer = typeof selectedHost === "object" && selectedHost !== null ? deliveryCredentialHosts.get(selectedHost) : undefined;
  if (record === undefined || issuer === undefined || record.storage !== storage || issuer.storage !== storage
      || record.owner !== issuer.owner || !record.owner.active) return invalid();
}
// The generated application supplies its private currently published host.
// A structurally identical host, or another real factory on the same storage,
// cannot substitute for that exact native issuer. There is no issuer setter.
export function readDeliveryCredentialProof(proof: unknown, storage: AuthenticationStorage, selectedHost: unknown): DeliveryCredentialFacts {
  const record = typeof proof === "object" && proof !== null ? deliveryCredentialProofs.get(proof) : undefined;
  const issuer = typeof selectedHost === "object" && selectedHost !== null ? deliveryCredentialHosts.get(selectedHost) : undefined;
  if (record === undefined || issuer === undefined || record.storage !== storage || issuer.storage !== storage
      || record.owner !== issuer.owner || !record.owner.active) return invalid();
  return record.facts;
}
export type ServiceCredentialAudit = Readonly<{ schemaVersion: 1; kind: "authentication_audit"; eventName: "service_credential.issued" | "service_credential.exchanged" | "service_credential.refreshed" | "service_credential.revoked"; strategy: string; serviceId: string; credentialId: string; occurredAt: number }>;
const encoder = new TextEncoder();
const identifier = /^[A-Za-z0-9_-]{1,128}$/u;
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;
const invalid = (): never => { throw new AuthenticationFault("invalid_credentials", 401); };
const misconfigured = (): never => { throw new AuthenticationFault("authentication_misconfigured", 503); };
function encode(bytes: ArrayBuffer | Uint8Array): string {
  return btoa(String.fromCharCode(...new Uint8Array(bytes))).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/u, "");
}
function decode(value: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/u.test(value)) return invalid();
  let bytes: Uint8Array;
  try { bytes = Uint8Array.from(atob(value.replaceAll("-", "+").replaceAll("_", "/")), c => c.charCodeAt(0)); } catch { return invalid(); }
  if (encode(bytes) !== value) return invalid();
  return bytes;
}
async function keyFromSecret(secret: string): Promise<Key> {
  let bytes: Uint8Array;
  try { bytes = decode(secret); } catch { return misconfigured(); }
  if (bytes.length !== 32) return misconfigured();
  const id = encode(await crypto.subtle.digest("SHA-256", bytes)).slice(0, 22);
  const key = await crypto.subtle.importKey("raw", bytes, { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]);
  return { id, key };
}
async function sign(key: Key, message: string): Promise<string> {
  return encode(await crypto.subtle.sign("HMAC", key.key, encoder.encode(message)));
}
async function verifies(key: Key, message: string, signature: string): Promise<boolean> {
  const bytes = decode(signature);
  return bytes.length === 32 && await crypto.subtle.verify("HMAC", key.key, bytes, encoder.encode(message));
}
function sessionRecord(value: Session | null): Session | null {
  if (value === null) return null;
  if (typeof value !== "object" || Array.isArray(value)
      || Object.keys(value).sort().join(",") !== "expires,id,keyId,revoked,strategy,subject,userId,verifier"
      || typeof value.id !== "string" || !identifier.test(value.id) || typeof value.strategy !== "string" || !identifier.test(value.strategy)
      || typeof value.subject !== "string" || value.subject.length === 0 || value.subject.length > 512
      || !uuid.test(value.userId) || !Number.isSafeInteger(value.expires)
      || typeof value.revoked !== "boolean" || typeof value.verifier !== "string" || typeof value.keyId !== "string") {
    throw new AuthenticationFault("authority_invariant", 500);
  }
  return value;
}
function identity(subject: string, userId: string, strength: string): UserPrincipal {
  return Object.freeze({ kind: "user", subject, authenticationStrength: firstPartyAuthenticationStrength("user", strength), values: Object.freeze({ user_id: userId }) }) as UserPrincipal;
}
function serviceIdentity(subject: string, serviceId: string, strength: string): AuthPrincipal {
  return Object.freeze({ kind: "service", subject, authenticationStrength: firstPartyAuthenticationStrength("service", strength), values: Object.freeze({ service_id: serviceId }) }) as AuthPrincipal;
}
function serviceCredentialRecord(value: ServiceCredential | null): ServiceCredential | null {
  if (value === null) return null;
  if (typeof value !== "object" || Array.isArray(value)
      || Object.keys(value).sort().join(",") !== "expires,id,keyId,revoked,serviceId,strategy,subject,verifier"
      || typeof value.id !== "string" || !identifier.test(value.id) || typeof value.strategy !== "string" || !identifier.test(value.strategy)
      || typeof value.subject !== "string" || value.subject.length === 0 || value.subject.length > 512
      || typeof value.serviceId !== "string" || !uuid.test(value.serviceId) || !Number.isSafeInteger(value.expires)
      || typeof value.revoked !== "boolean" || typeof value.verifier !== "string" || !/^[A-Za-z0-9_-]{43}$/u.test(value.verifier)
      || typeof value.keyId !== "string" || !/^[A-Za-z0-9_-]{22}$/u.test(value.keyId)) {
    throw new AuthenticationFault("authority_invariant", 500);
  }
  return value;
}
async function storageCall<T>(operation: () => Promise<T>): Promise<T> {
  try { return await operation(); } catch { throw new AuthenticationFault("authentication_unavailable", 503); }
}
function milliseconds(value: number): number {
  if (!Number.isSafeInteger(value) || value < 0) throw new AuthenticationFault("authority_invariant", 500);
  return value;
}

export async function createFirstPartyAuthentication(
  settings: readonly Settings[], storage: AuthenticationStorage,
  resolve: (strategy: string, subject: string, strength: string, principalKind?: "user" | "service") => Promise<ResolutionResult>,
  external?: Readonly<{ validate(strategy: string, credential: string, operationNow: number): Promise<ValidationResult> }>,
  audit: (event: ServiceCredentialAudit) => void = event => console.error(JSON.stringify(event)),
) {
  const deliveryProofOwner = { active: true };
  function serviceAudit(eventName: ServiceCredentialAudit["eventName"], record: ServiceCredential, now: number) {
    // Only compiler-created, closed identity facts enter this sink. Never pass
    // the credential record, subject, verifier, token, provider data or errors.
    try { audit(Object.freeze({ schemaVersion: 1, kind: "authentication_audit", eventName, strategy: record.strategy, serviceId: record.serviceId, credentialId: record.id, occurredAt: now })); }
    catch { throw new AuthenticationFault("authentication_unavailable", 503); }
  }
  const configured: Configured[] = [];
  for (const item of settings) {
    if (!identifier.test(item.name) || typeof item.audience !== "string" || item.audience.length === 0 || item.audience.length > 256
        || !Number.isSafeInteger(item.maximumDelayMs) || item.maximumDelayMs < 0
        || (item.mode === "signed" && item.maximumDelayMs === 0)
        || (item.principal === "service" && (item.cookie !== null || item.mode === "opaque"))
        || (item.mode === "api_key" && item.principal !== "service")
        || configured.some(existing => existing.name === item.name && existing.mode === item.mode && existing.principal === item.principal)) return misconfigured();
    if (item.cookie !== null) {
      if (!/^[A-Za-z0-9_-]{1,128}$/u.test(item.cookie) || item.origin === null) return misconfigured();
      try { const origin = new URL(item.origin); if (origin.protocol !== "https:" || origin.origin !== item.origin) return misconfigured(); } catch { return misconfigured(); }
    }
    const keys = [await keyFromSecret(item.secret)];
    if (item.previousSecret !== undefined) {
      const previous = await keyFromSecret(item.previousSecret);
      if (previous.id === keys[0].id) return misconfigured();
      keys.push(previous);
    }
    configured.push({ ...item, secret: "", previousSecret: undefined, keys });
  }
  const config = (name: string, kind: "user" | "service" = "user", mode?: Settings["mode"]): Configured => configured.find(item => item.name === name && (item.principal ?? "user") === kind && (mode === undefined || item.mode === mode)) ?? misconfigured();
  async function authority(strategy: Configured, subject: string, principalId?: string): Promise<AuthPrincipal> {
    let result: ResolutionResult;
    const kind = strategy.principal ?? "user";
    // Exchange changes representation, not the originating authentication proof.
    const strength = kind === "service" ? "api_key" : strategy.mode;
    try { result = await resolve(strategy.name, subject, strength, kind); } catch { throw new AuthenticationFault("authentication_unavailable", 503); }
    if (result.kind === "inactive") throw new AuthenticationFault("principal_inactive", 401, result.failureName);
    if (result.kind === "missing") return invalid();
    if (result.kind === "unavailable") throw new AuthenticationFault("authentication_unavailable", 503);
    if (result.kind !== "active" || result.principal.kind !== kind || result.principal.subject !== subject
        || (principalId !== undefined && result.principal.values[kind === "user" ? "user_id" : "service_id"] !== principalId)) throw new AuthenticationFault("authority_invariant", 500);
    return result.principal;
  }
  async function liveSession(verified: Verified, now: number): Promise<Session> {
    if (verified.principal.kind !== "user") return invalid();
    const session = sessionRecord(await storageCall(() => storage.get(verified.sessionId)));
    if (session === null || session.id !== verified.sessionId || session.revoked || session.expires <= now
        || session.strategy !== verified.strategy.name || session.subject !== verified.principal.subject
        || session.userId !== verified.principal.values.user_id) return invalid();
    return session;
  }
  async function liveServiceCredential(verified: Verified, now: number): Promise<ServiceCredential> {
    const record = serviceCredentialRecord(await storageCall(() => storage.getServiceCredential(verified.sessionId)));
    if (verified.principal.kind !== "service" || record === null || record.id !== verified.sessionId || record.revoked || record.expires <= now
        || record.strategy !== verified.strategy.name || record.subject !== verified.principal.subject
        || record.serviceId !== verified.principal.values.service_id) return invalid();
    const origin = config(verified.strategy.name, "service", "api_key");
    if (!origin.keys.some(key => key.id === record.keyId)) return invalid();
    return record;
  }
  async function verify(strategy: Configured, credential: string, now: number): Promise<Verified> {
    if (credential.length > 4096) return invalid();
    if (strategy.principal === "service") return verifyService(strategy, credential, now);
    if (strategy.mode === "opaque") {
      const parts = credential.split(".");
      if (parts.length !== 3 || parts[0] !== "jdo1" || !identifier.test(parts[1]) || decode(parts[2]).length !== 32) return invalid();
      const session = sessionRecord(await storageCall(() => storage.get(parts[1])));
      if (session === null || session.id !== parts[1] || session.revoked || session.expires <= now || session.strategy !== strategy.name) return invalid();
      const key = strategy.keys.find(key => key.id === session.keyId);
      if (!key || !(await verifies(key, `opaque:${strategy.audience}:${credential}`, session.verifier))) return invalid();
      return { principal: identity(session.subject, session.userId, "opaque"), sessionId: session.id, credential, key, strategy };
    }
    const parts = credential.split(".");
    if (parts.length !== 4 || parts[0] !== "jds1") return invalid();
    const key = strategy.keys.find(key => key.id === parts[1]);
    if (!key || !(await verifies(key, parts.slice(0, 3).join("."), parts[3]))) return invalid();
    let payload: unknown;
    try { payload = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(decode(parts[2]))); } catch { return invalid(); }
    // Fixed tuple: version, audience, strategy, session, subject, identity, issued, expires.
    if (!Array.isArray(payload) || payload.length !== 8 || payload[0] !== 1
        || payload[1] !== strategy.audience || payload[2] !== strategy.name || typeof payload[3] !== "string" || !identifier.test(payload[3])
        || typeof payload[4] !== "string" || payload[4].length === 0 || payload[4].length > 512
        || typeof payload[5] !== "string" || !uuid.test(payload[5])
        || !Number.isSafeInteger(payload[6]) || !Number.isSafeInteger(payload[7])
        || payload[6] < 0 || payload[6] > now || payload[7] <= now || payload[7] <= payload[6]
        || payload[7] - payload[6] > strategy.maximumDelayMs) return invalid();
    return { principal: identity(payload[4], payload[5], "signed"), sessionId: payload[3], credential, key, strategy };
  }
  async function verifyService(strategy: Configured, credential: string, now: number): Promise<Verified> {
    const parts = credential.split(".");
    if (strategy.mode === "api_key") {
      if (parts.length !== 3 || parts[0] !== "jdk1" || !identifier.test(parts[1]) || decode(parts[2]).length !== 32) return invalid();
      const record = serviceCredentialRecord(await storageCall(() => storage.getServiceCredential(parts[1])));
      if (record === null || record.id !== parts[1] || record.revoked || record.expires <= now || record.strategy !== strategy.name) return invalid();
      const key = strategy.keys.find(key => key.id === record.keyId);
      if (!key || !(await verifies(key, `service:${strategy.audience}:${credential}`, record.verifier))) return invalid();
      return { principal: serviceIdentity(record.subject, record.serviceId, "api_key"), sessionId: record.id, credential, key, strategy, serviceCredential: record };
    }
    if (strategy.mode !== "signed" || parts.length !== 4 || parts[0] !== "jdx1") return invalid();
    const key = strategy.keys.find(key => key.id === parts[1]);
    if (!key || !(await verifies(key, parts.slice(0, 3).join("."), parts[3]))) return invalid();
    let payload: unknown;
    try { payload = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(decode(parts[2]))); } catch { return invalid(); }
    // Closed service envelope: version, audience, strategy, originating credential,
    // subject, service identity, issued, expiry, principal kind, strength.
    if (!Array.isArray(payload) || payload.length !== 10 || payload[0] !== 1 || payload[8] !== "service" || payload[9] !== "api_key"
        || payload[1] !== strategy.audience || payload[2] !== strategy.name || typeof payload[3] !== "string" || !identifier.test(payload[3])
        || typeof payload[4] !== "string" || payload[4].length === 0 || payload[4].length > 512
        || typeof payload[5] !== "string" || !uuid.test(payload[5])
        || !Number.isSafeInteger(payload[6]) || !Number.isSafeInteger(payload[7])
        || payload[6] < 0 || payload[6] > now || payload[7] <= now || payload[7] <= payload[6]
        || payload[7] - payload[6] > strategy.maximumDelayMs) return invalid();
    return { principal: serviceIdentity(payload[4], payload[5], "api_key"), sessionId: payload[3], credential, key, strategy };
  }
  function credentialConfig(name: string, credential: string): Configured {
    const prefix = credential.split(".", 1)[0];
    const entry = configured.find(item => item.name === name && (
      prefix === "jdk1" ? item.principal === "service" && item.mode === "api_key" :
      prefix === "jdx1" ? item.principal === "service" && item.mode === "signed" :
      prefix === "jdo1" ? (item.principal ?? "user") === "user" && item.mode === "opaque" :
      prefix === "jds1" ? (item.principal ?? "user") === "user" && item.mode === "signed" : false));
    return entry ?? invalid();
  }
  async function serviceBearer(strategy: Configured, record: ServiceCredential, now: number) {
    const expires = Math.min(record.expires, now + strategy.maximumDelayMs);
    const key = strategy.keys[0];
    const payload = encode(encoder.encode(JSON.stringify([1, strategy.audience, strategy.name, record.id, record.subject, record.serviceId, now, expires, "service", "api_key"])));
    const message = `jdx1.${key.id}.${payload}`;
    return { credential: `${message}.${await sign(key, message)}`, expires, credentialId: record.id, serviceId: record.serviceId };
  }
  async function credentialFor(strategy: Configured, session: Session, now: number, opaque?: string) {
    const key = strategy.keys[0];
    const expires = strategy.mode === "signed" ? Math.min(session.expires, now + strategy.maximumDelayMs) : session.expires;
    let credential = opaque;
    if (strategy.mode === "signed") {
      const payload = encode(encoder.encode(JSON.stringify([1, strategy.audience, strategy.name, session.id, session.subject, session.userId, now, expires])));
      const message = `jds1.${key.id}.${payload}`;
      credential = `${message}.${await sign(key, message)}`;
    }
    if (!credential) return invalid();
    const csrfToken = await sign(key, `csrf:${strategy.name}:${credential}`);
    const setCookie = strategy.cookie === null ? null : `${strategy.cookie}=${credential}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=${Math.max(0, Math.floor((expires - now) / 1000))}`;
    return { credential, csrfToken, setCookie, expires };
  }
  const host = Object.freeze({
    // Internal host integration only: caller must already have authenticated the
    // human. No Jadpo action, HTTP route, password or account provisioning API.
    async issue(strategyName: string, subject: string, expires: number, operationNow: number) {
      const now = milliseconds(operationNow); milliseconds(expires);
      if (expires <= now || subject.length === 0 || subject.length > 512) return invalid();
      const strategy = config(strategyName);
      const principal = await authority(strategy, subject);
      if (principal.kind !== "user") return invalid();
      const userId = principal.values.user_id;
      if (typeof userId !== "string" || !uuid.test(userId)) throw new AuthenticationFault("authority_invariant", 500);
      const id = crypto.randomUUID();
      const opaque = strategy.mode === "opaque" ? `jdo1.${id}.${encode(crypto.getRandomValues(new Uint8Array(32)))}` : undefined;
      const session: Session = { id, strategy: strategy.name, subject, userId, expires, revoked: false, keyId: strategy.keys[0].id,
        verifier: opaque ? await sign(strategy.keys[0], `opaque:${strategy.audience}:${opaque}`) : "" };
      await storageCall(() => storage.put(session));
      return credentialFor(strategy, session, now, opaque);
    },
    // Trusted provisioning boundary: this is never exposed as an authored
    // callable or generated route. Only this return reveals the random secret.
    async issueServiceCredential(strategyName: string, subject: string, expires: number, operationNow: number) {
      const now = milliseconds(operationNow); milliseconds(expires);
      if (expires <= now || typeof subject !== "string" || subject.length === 0 || subject.length > 512) return invalid();
      const strategy = config(strategyName, "service", "api_key");
      const principal = await authority(strategy, subject);
      if (principal.kind !== "service" || typeof principal.values.service_id !== "string" || !uuid.test(principal.values.service_id)) throw new AuthenticationFault("authority_invariant", 500);
      const id = crypto.randomUUID();
      const credential = `jdk1.${id}.${encode(crypto.getRandomValues(new Uint8Array(32)))}`;
      const record: ServiceCredential = { id, strategy: strategy.name, subject, serviceId: principal.values.service_id, expires, revoked: false,
        keyId: strategy.keys[0].id, verifier: await sign(strategy.keys[0], `service:${strategy.audience}:${credential}`) };
      await storageCall(() => storage.putServiceCredential(record, now));
      serviceAudit("service_credential.issued", record, now);
      return { credential, credentialId: id, serviceId: record.serviceId, expires };
    },
    // Verify the exact selected direct api-key strategy through the ordinary
    // crypto/storage boundary BEFORE entering the worker source transaction.
    // Admission must still re-read live declared authority in that transaction;
    // this opaque proof never supplies an entity, role or dispatch grant.
    async prepareDeliveryCredential(strategyName: string, credential: string, operationNow: number): Promise<object> {
      if (!deliveryProofOwner.active) return invalid();
      const now = milliseconds(operationNow);
      const strategy = config(strategyName, "service", "api_key");
      const verified = await verify(strategy, credential, now);
      const record = verified.serviceCredential;
      if (record === undefined) return invalid();
      await authority(strategy, record.subject, record.serviceId);
      if (!deliveryProofOwner.active) return invalid();
      const proof = Object.freeze(Object.create(null));
      const facts = Object.freeze({ id: record.id, strategy: record.strategy, subject: record.subject,
        serviceId: record.serviceId, verifier: record.verifier, keyId: record.keyId });
      deliveryCredentialProofs.set(proof, { storage, owner: deliveryProofOwner, facts });
      return proof;
    },
    // Configuration reinitialization invalidates old host proofs, including
    // ones awaiting an authority transaction. This only withdraws authority.
    invalidateDeliveryCredentialProofs(): void { deliveryProofOwner.active = false; },
    // Same trusted administrative boundary as native issue/revoke: the host
    // caller already owns privileged operator authorization. No authored API.
    issueDeliveryOperatorGrant(): object {
      if (!deliveryProofOwner.active) return invalid();
      const grant = Object.freeze({});
      deliveryOperatorGrants.set(grant, { storage, owner: deliveryProofOwner });
      return grant;
    },
    revokeDeliveryOperatorGrant(grant: unknown): void {
      readDeliveryOperatorGrant(grant, storage, host);
      deliveryOperatorGrants.delete(grant as object);
    },
// __JADPO_RECONCILIATION_AUTHORITY_START__
    issueDeliveryReceiptReconciliationGrant(): object {
      if (!deliveryProofOwner.active) return invalid();
      const grant = Object.freeze({});
      deliveryReceiptReconciliationGrants.set(grant, { storage, owner: deliveryProofOwner });
      return grant;
    },
    revokeDeliveryReceiptReconciliationGrant(grant: unknown): void {
      readDeliveryReceiptReconciliationGrant(grant, storage, host);
      deliveryReceiptReconciliationGrants.delete(grant as object);
    },
// __JADPO_RECONCILIATION_AUTHORITY_END__
    async exchangeServiceCredential(strategyName: string, credential: string, operationNow: number) {
      const now = milliseconds(operationNow);
      const direct = config(strategyName, "service", "api_key");
      const signed = config(strategyName, "service", "signed");
      const verified = await verify(direct, credential, now);
      // verifyService already loaded and checked the declared credential. Reuse
      // that exact private record rather than issuing a second credential lookup.
      const record = verified.serviceCredential;
      if (record === undefined) throw new AuthenticationFault("authority_invariant", 500);
      await authority(direct, record.subject, record.serviceId);
      const bearer = await serviceBearer(signed, record, now);
      serviceAudit("service_credential.exchanged", record, now);
      return bearer;
    },
    async revokeServiceCredential(strategyName: string, credentialId: string, operationNow: number) {
      milliseconds(operationNow);
      const strategy = config(strategyName, "service", "api_key");
      if (typeof credentialId !== "string" || !identifier.test(credentialId)) return invalid();
      const record = serviceCredentialRecord(await storageCall(() => storage.getServiceCredential(credentialId)));
      if (record === null || record.strategy !== strategy.name) return invalid();
      await storageCall(() => storage.revokeServiceCredential(credentialId, operationNow));
      serviceAudit("service_credential.revoked", record, operationNow);
      return { credentialId: record.id, serviceId: record.serviceId };
    },
    async authenticate(request: Request, freshAuthority: boolean, operationNow: number, mutates = false): Promise<AuthPrincipal> {
      const now = milliseconds(operationNow);
      let verified: Verified | null = null;
      let externallyVerified = false;
      const principal = await authenticateRequest(request, { freshAuthority }, {
        async validate(name, credential) {
          const segments = credential.split(".");
          if (external && segments.length === 3 && !["jdo1", "jdk1", "jds1", "jdx1"].includes(segments[0])) {
            externallyVerified = true;
            return external.validate(name, credential, now);
          }
          try { verified = await verify(credentialConfig(name, credential), credential, now); return { kind: "valid", identity: { principal: verified.principal, authorityRequired: verified.strategy.mode === "opaque" || verified.strategy.mode === "api_key" } }; }
          catch (error) { if (error instanceof AuthenticationFault && error.code === "invalid_credentials") return { kind: "invalid" }; throw error; }
        },
        async resolve() {
          if (!verified) throw new AuthenticationFault("authority_invariant", 500);
          try {
            if (verified.principal.kind === "service") await liveServiceCredential(verified, now);
            else await liveSession(verified, now);
            const id = verified.principal.kind === "service" ? verified.principal.values.service_id : verified.principal.values.user_id;
            const principal = await authority(verified.strategy, verified.principal.subject, id as string);
            return { kind: "active", principal };
          } catch (error) {
            if (error instanceof AuthenticationFault && error.code === "invalid_credentials") return { kind: "missing" };
            if (error instanceof AuthenticationFault && error.code === "principal_inactive") return { kind: "inactive", failureName: error.declaredFailure! };
            throw error;
          }
        },
      });
      if (externallyVerified) return principal;
      if (verified === null) throw new AuthenticationFault("authority_invariant", 500);
      const selected = verified as Verified;
      if (selected.strategy.cookie !== null && (mutates || !["GET", "HEAD", "OPTIONS"].includes(request.method))) {
        const token = request.headers.get("x-jadpo-csrf");
        if (request.headers.get("origin") !== selected.strategy.origin || request.headers.get("sec-fetch-site") === "cross-site"
            || token === null || token.length > 128) throw new AuthenticationFault("csrf_rejected", 403);
        let valid = false;
        try { valid = await verifies(selected.key, `csrf:${selected.strategy.name}:${selected.credential}`, token); } catch { /* invalid encoding */ }
        if (!valid) throw new AuthenticationFault("csrf_rejected", 403);
      }
      return principal;
    },
    async refresh(strategyName: string, credential: string, operationNow: number) {
      const now = milliseconds(operationNow);
      const strategy = credentialConfig(strategyName, credential);
      const verified = await verify(strategy, credential, now);
      if (verified.principal.kind === "service") {
        if (strategy.mode !== "signed") return invalid();
        const record = await liveServiceCredential(verified, now);
        await authority(strategy, record.subject, record.serviceId);
        serviceAudit("service_credential.refreshed", record, now);
        return serviceBearer(strategy, record, now);
      }
      const session = await liveSession(verified, now);
      await authority(strategy, session.subject, session.userId);
      // Opaque credentials retain their original verifier/key until reissued.
      if (strategy.mode === "opaque") return invalid();
      return credentialFor(strategy, session, now);
    },
    async revoke(strategyName: string, credential: string, operationNow: number) {
      const now = milliseconds(operationNow);
      const verified = await verify(credentialConfig(strategyName, credential), credential, now);
      if (verified.principal.kind === "service") {
        const record = await liveServiceCredential(verified, now);
        await storageCall(() => storage.revokeServiceCredential(verified.sessionId, now));
        serviceAudit("service_credential.revoked", record, operationNow);
      }
      else await storageCall(() => storage.revoke(verified.sessionId));
    },
  });
  deliveryCredentialHosts.set(host, { storage, owner: deliveryProofOwner });
  return host;
}
