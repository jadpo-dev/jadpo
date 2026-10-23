import {
  createHash,
  createHmac,
  timingSafeEqual,
  X509Certificate,
} from "node:crypto";
import { and, eq, gt, isNull, isNotNull } from "drizzle-orm";
import {
  createLocalJWKSet,
  decodeProtectedHeader,
  jwtVerify,
  importJWK,
  errors,
} from "jose";
import type { JSONWebKeySet, JWK } from "jose";
import { z } from "zod";
import { uniqueJson } from "./strict-json.ts";
import type { Config } from "./config.ts";
import type { Store } from "./db/store.ts";
import { users, services, sessions, serviceKeys } from "./db/schema.ts";
import { AppError } from "./errors.ts";
import type { Principal } from "./schemas.ts";
import { RequestScope, type Clock } from "./runtime.ts";
import type { Egress } from "./egress.ts";
import { publicHttps, ResponseTooLarge } from "./egress.ts";

const compactId = z.string().regex(/^[0-9a-f]{32}$/);
const Claims = z.strictObject({
  enc: z.literal(1),
  m: z.enum(["browser_session", "api_token", "service_key"]),
  r: compactId,
  v: z.string().regex(/^[0-9a-f]{64}$/),
  p: compactId,
  s: z.string().min(1).max(1025),
  e: z.number().int().positive(),
  c: z
    .string()
    .regex(/^[0-9a-f]{64}$/)
    .optional(),
});
type Claims = z.infer<typeof Claims>;
export function digest(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}
function equal(left: string, right: string): boolean {
  const a = Buffer.from(left);
  const b = Buffer.from(right);
  return a.length === b.length && timingSafeEqual(a, b);
}
export function sign(payload: string, key: string): string {
  const encoded = Buffer.from(payload).toString("base64url");
  return (
    encoded +
    "." +
    createHmac("sha256", key).update(encoded).digest("base64url")
  );
}
export function open(
  signed: string,
  keys: readonly string[],
): string | undefined {
  if (signed.length > 8192) return undefined;
  const [encoded, tag, extra] = signed.split(".");
  if (!encoded || !tag || extra !== undefined) return undefined;
  if (
    !keys.some((key) =>
      equal(tag, createHmac("sha256", key).update(encoded).digest("base64url")),
    )
  )
    return undefined;
  return Buffer.from(encoded, "base64url").toString("utf8");
}
export type Selected = { kind: "cookie" | "bearer"; value: string };
export function selectCredential(headers: Headers): Selected {
  const cookies = (headers.get("cookie") ?? "")
    .split(";")
    .map((item) => item.trim())
    .filter((item) => item.split("=")[0] === "todo_session");
  const authorization = headers.get("authorization");
  const presented =
    cookies.length +
    (authorization === null ? 0 : authorization.split(",").length);
  if (presented > 1) throw new AppError("ambiguous_credentials");
  if (!presented) throw new AppError("authentication_required");
  if (cookies.length) {
    const value = cookies[0]?.slice("todo_session=".length);
    if (!value) throw new AppError("invalid_credentials");
    return { kind: "cookie", value };
  }
  const bearer = /^Bearer ([^\s,]+)$/i.exec(authorization ?? "")?.[1];
  if (!bearer || bearer.length > 8192)
    throw new AppError("invalid_credentials");
  return { kind: "bearer", value: bearer };
}
const expanded = (id: string) =>
  id.slice(0, 8) +
  "-" +
  id.slice(8, 12) +
  "-" +
  id.slice(12, 16) +
  "-" +
  id.slice(16, 20) +
  "-" +
  id.slice(20);
const compact = (id: string) => id.replaceAll("-", "");
export type Authentication = {
  principal: Principal;
  renewal?: readonly [string, string];
};

export class AuthenticationService {
  readonly config: Config;
  readonly store: Store;
  readonly clock: Clock;
  readonly egress: Egress;
  #keys: ReturnType<typeof createLocalJWKSet> | undefined;
  #keysUntil = 0;
  #refreshAt = -Infinity;
  #refreshing: Promise<void> | undefined;
  #refreshError: AppError | undefined;
  constructor(config: Config, store: Store, clock: Clock, egress: Egress) {
    this.config = config;
    this.store = store;
    this.clock = clock;
    this.egress = egress;
  }
  clearKeys(): void {
    this.#keys = undefined;
    this.#keysUntil = 0;
    this.#refreshAt = -Infinity;
    this.#refreshError = undefined;
  }
  async authenticate(
    request: Request,
    scope: RequestScope,
    fresh = false,
  ): Promise<Authentication> {
    const credential = selectCredential(request.headers);
    scope.check();
    const value = credential.value;
    if (
      credential.kind === "bearer" &&
      value.startsWith("jadpo_service_token_")
    ) {
      const claims = this.claims(
        value.slice("jadpo_service_token_".length),
        "service_key",
      );
      if (!claims) throw new AppError("invalid_credentials");
      if (fresh) return await this.service(claims.v, scope, false);
      return {
        principal: {
          kind: "service",
          id: expanded(claims.p),
          subject: claims.s.slice(1),
        },
      };
    }
    if (credential.kind === "bearer" && value.startsWith("jadpo_key_")) {
      if (!/^jadpo_key_[0-9a-f]{64}$/.test(value))
        throw new AppError("invalid_credentials");
      return await this.service(digest(value), scope, false);
    }
    const method =
      credential.kind === "cookie" ? "browser_session" : "api_token";
    if (credential.kind === "bearer" && !value.startsWith("jadpo_token_"))
      return await this.external(value, scope);
    const split = value.indexOf(".");
    const secret = split < 0 ? value : value.slice(0, split);
    const pattern =
      method === "browser_session"
        ? /^[0-9a-f]{64}$/
        : /^jadpo_token_[0-9a-f]{64}$/;
    if (!pattern.test(secret)) throw new AppError("invalid_credentials");
    const verifier = digest(secret);
    const claims =
      split < 0 ? undefined : this.claims(value.slice(split + 1), method);
    if (!fresh && claims && equal(claims.v, verifier)) {
      this.csrf(request, method, claims.c);
      return {
        principal: {
          kind: "user",
          id: expanded(claims.p),
          subject: claims.s.slice(1),
        },
      };
    }
    let rows;
    try {
      rows = await this.store.db
        .select({
          id: users.id,
          subject: users.authentication_subject,
          status: users.status,
          row: sessions.id,
          csrf: sessions.csrf_verifier,
          ends: sessions.ends_at,
        })
        .from(sessions)
        .innerJoin(users, eq(users.id, sessions.principal_id))
        .where(
          and(
            eq(sessions.verifier, verifier),
            gt(sessions.ends_at, new Date(this.clock()).toISOString()),
            method === "browser_session"
              ? isNotNull(sessions.csrf_verifier)
              : isNull(sessions.csrf_verifier),
          ),
        )
        .limit(1);
    } catch {
      throw new AppError("authentication_unavailable");
    }
    scope.check();
    const row = rows[0];
    if (!row) throw new AppError("invalid_credentials");
    if (row.status !== "active") throw new AppError("user_disabled");
    this.csrf(request, method, row.csrf ?? undefined);
    const principal: Principal = {
      kind: "user",
      id: row.id,
      subject: row.subject,
    };
    if (claims && equal(claims.v, verifier)) return { principal };
    const payload: Claims = {
      enc: 1,
      m: method,
      r: compact(row.row),
      v: verifier,
      p: compact(row.id),
      s: "t" + row.subject,
      e: Math.min(this.clock() + 300000, Date.parse(row.ends)),
      ...(row.csrf ? { c: row.csrf } : {}),
    };
    const renewed =
      secret + "." + sign(JSON.stringify(payload), this.config.signingKey);
    const renewal: readonly [string, string] =
      method === "browser_session"
        ? [
            "set-cookie",
            `todo_session=${renewed}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=${Math.max(0, Math.floor((Date.parse(row.ends) - this.clock()) / 1000))}`,
          ]
        : ["x-jadpo-token", renewed];
    return { principal, renewal };
  }
  private claims(value: string, method: Claims["m"]): Claims | undefined {
    const payload = open(value, [
      this.config.signingKey,
      ...(this.config.previousKey ? [this.config.previousKey] : []),
    ]);
    if (!payload) return undefined;
    try {
      const result = Claims.safeParse(JSON.parse(payload));
      if (
        !result.success ||
        result.data.m !== method ||
        result.data.e <= this.clock() ||
        result.data.e > this.clock() + 300000 ||
        !result.data.s.startsWith("t")
      )
        return undefined;
      if (
        (method === "browser_session" && !result.data.c) ||
        (method !== "browser_session" && result.data.c)
      )
        return undefined;
      return result.data;
    } catch {
      return undefined;
    }
  }
  private csrf(request: Request, method: string, verifier?: string): void {
    if (
      method !== "browser_session" ||
      ["GET", "HEAD", "OPTIONS"].includes(request.method)
    )
      return;
    const proof = request.headers.get("x-jadpo-csrf");
    if (
      request.headers.get("origin") !== this.config.browserOrigin ||
      !proof ||
      !verifier ||
      !equal(digest(proof), verifier)
    )
      throw new AppError("csrf_rejected");
  }
  async exchange(request: Request, scope: RequestScope) {
    const selected = selectCredential(request.headers);
    if (new URL(request.url).search || (await request.text()))
      throw new AppError("invalid_input");
    if (
      selected.kind !== "bearer" ||
      !/^jadpo_key_[0-9a-f]{64}$/.test(selected.value)
    ) {
      await this.authenticate(request, scope, true);
      throw new AppError("not_permitted");
    }
    return await this.service(digest(selected.value), scope, true);
  }
  private async service(
    verifier: string,
    scope: RequestScope,
    exchange: boolean,
  ) {
    let rows;
    try {
      rows = await this.store.db
        .select({
          id: services.id,
          subject: services.name,
          status: services.status,
          row: serviceKeys.id,
          ends: serviceKeys.ends_at,
        })
        .from(serviceKeys)
        .innerJoin(services, eq(services.id, serviceKeys.principal_id))
        .where(
          and(
            eq(serviceKeys.verifier, verifier),
            gt(serviceKeys.ends_at, new Date(this.clock()).toISOString()),
          ),
        )
        .limit(1);
    } catch {
      throw new AppError("authentication_unavailable");
    }
    scope.check();
    const row = rows[0];
    if (!row) throw new AppError("invalid_credentials");
    if (row.status !== "active") throw new AppError("service_disabled");
    const principal: Principal = {
      kind: "service",
      id: row.id,
      subject: row.subject,
    };
    if (!exchange) return { principal };
    const expires = Math.min(this.clock() + 300000, Date.parse(row.ends));
    const payload: Claims = {
      enc: 1,
      m: "service_key",
      r: compact(row.row),
      v: verifier,
      p: compact(row.id),
      s: "t" + row.subject,
      e: expires,
    };
    return {
      principal,
      output: {
        token:
          "jadpo_service_token_" +
          sign(JSON.stringify(payload), this.config.signingKey),
        expires_at: new Date(expires).toISOString(),
      },
    };
  }
  private async external(
    token: string,
    scope: RequestScope,
  ): Promise<Authentication> {
    if (
      token.length > 8192 ||
      !/^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(token)
    )
      throw new AppError("invalid_credentials");
    let header;
    try {
      uniqueJson(
        new TextDecoder("utf-8", { fatal: true }).decode(
          Buffer.from(token.split(".")[0] ?? "", "base64url"),
        ),
      );
      header = decodeProtectedHeader(token);
    } catch {
      throw new AppError("invalid_credentials");
    }
    if (
      !["EdDSA", "RS256", "ES256"].includes(header.alg ?? "") ||
      (header.kid !== undefined &&
        (typeof header.kid !== "string" ||
          !/^[\x21-\x7e]{1,256}$/.test(header.kid))) ||
      header.crit !== undefined ||
      (header.cty !== undefined && typeof header.cty !== "string") ||
      header.cty?.toUpperCase() === "JWT"
    )
      throw new AppError("invalid_credentials");
    if (!this.#keys || this.#keysUntil <= this.clock())
      await this.refresh(scope);
    let payload;
    try {
      payload = await this.verifyExternal(token);
    } catch (error) {
      if (
        error &&
        typeof error === "object" &&
        "code" in error &&
        [
          "ERR_JWKS_NO_MATCHING_KEY",
          "ERR_JWS_SIGNATURE_VERIFICATION_FAILED",
        ].includes(String(error.code))
      ) {
        await this.refresh(scope);
        try {
          payload = await this.verifyExternal(token);
        } catch {
          throw new AppError("invalid_credentials");
        }
      } else throw new AppError("invalid_credentials");
    }
    scope.check();
    if (!payload.sub || payload.sub.length > 1024)
      throw new AppError("invalid_credentials");
    let rows;
    try {
      rows = await this.store.db
        .select({
          id: users.id,
          subject: users.authentication_subject,
          status: users.status,
        })
        .from(users)
        .where(eq(users.authentication_subject, payload.sub))
        .limit(1);
    } catch {
      throw new AppError("authentication_unavailable");
    }
    scope.check();
    const row = rows[0];
    if (!row) throw new AppError("sign_in_refused");
    if (row.status !== "active") throw new AppError("user_disabled");
    return { principal: { kind: "user", id: row.id, subject: row.subject } };
  }
  private async verifyExternal(token: string) {
    if (!this.#keys) throw new AppError("authentication_unavailable");
    const options = {
      algorithms: ["EdDSA", "RS256", "ES256"],
      issuer: this.config.issuer,
      audience: this.config.audience,
      requiredClaims: ["sub", "iss", "aud", "exp"],
      currentDate: new Date(this.clock()),
      clockTolerance: 30,
    };
    let verified;
    try {
      verified = await jwtVerify(token, this.#keys, options);
    } catch (error) {
      if (!(error instanceof errors.JWKSMultipleMatchingKeys)) throw error;
      for await (const key of error) {
        try {
          verified = await jwtVerify(token, key, options);
          break;
        } catch {
          /* Try every matching public key, never a token-supplied key. */
        }
      }
      if (!verified) throw new AppError("invalid_credentials");
    }
    const result = verified.payload;
    uniqueJson(
      new TextDecoder("utf-8", { fatal: true }).decode(
        Buffer.from(token.split(".")[1] ?? "", "base64url"),
      ),
    );
    for (const name of ["exp", "nbf", "iat"] as const) {
      const value = result[name];
      if (
        value !== undefined &&
        (!Number.isSafeInteger(value) || Math.abs(value) > 8640000000000)
      )
        throw new AppError("invalid_credentials");
    }
    if (result.iat !== undefined && result.iat > this.clock() / 1000 + 30)
      throw new AppError("invalid_credentials");
    return result;
  }
  private async refresh(scope: RequestScope): Promise<void> {
    scope.check();
    if (this.#refreshing) {
      // A waiter's cancellation must not cancel the request owning this refresh.
      const pending = this.#refreshing;
      return await new Promise<void>((resolve, reject) => {
        const cancelled = () => {
          cleanup();
          reject(new AppError("unavailable"));
        };
        scope.signal.addEventListener("abort", cancelled, { once: true });
        const cleanup = () => {
          scope.signal.removeEventListener("abort", cancelled);
        };
        void pending.then(
          () => {
            cleanup();
            resolve();
          },
          (error: unknown) => {
            cleanup();
            reject(
              error instanceof Error
                ? error
                : new AppError("authentication_unavailable"),
            );
          },
        );
        if (scope.signal.aborted) cancelled();
      });
    }
    if (this.clock() - this.#refreshAt < 30000) {
      if (this.#refreshError) throw this.#refreshError;
      if (!this.#keys || this.#keysUntil <= this.clock())
        throw new AppError("authentication_unavailable");
      return;
    }
    // Reserve before awaiting, including cold/expired/failing/cancelled requests.
    this.#refreshAt = this.clock();
    this.#refreshError = new AppError("authentication_unavailable");
    const pending = (async () => {
      try {
        const discovery =
          this.config.issuer.replace(/\/$/, "") +
          "/.well-known/openid-configuration";
        const metadata = z
          .object({
            issuer: z.literal(this.config.issuer),
            jwks_uri: z.string(),
          })
          .parse(await this.fetchDocument(discovery, 65536, scope));
        // fetchDocument permits a distinct public HTTPS JWKS host.
        const keys = await readPublicKeys(
          await this.fetchDocument(metadata.jwks_uri, 262144, scope),
        );
        scope.check();
        this.#keys = createLocalJWKSet(keys);
        this.#keysUntil = this.clock() + 300000;
        this.#refreshError = undefined;
      } catch (error) {
        this.#refreshError =
          error instanceof AppError &&
          [
            "authentication_unavailable",
            "unavailable",
            "deadline_exceeded",
          ].includes(error.code)
            ? new AppError("authentication_unavailable")
            : new AppError("authentication_misconfigured");
        throw this.#refreshError;
      } finally {
        this.#refreshAt = this.clock();
      }
    })();
    this.#refreshing = pending;
    try {
      await pending;
    } finally {
      this.#refreshing = undefined;
    }
  }
  private async fetchDocument(
    url: string,
    maximum: number,
    parent: RequestScope,
  ): Promise<unknown> {
    // Validate configuration separately from transport failures; no raw value escapes.
    try {
      publicHttps(url);
    } catch {
      throw new AppError("authentication_misconfigured");
    }
    const scope = new RequestScope(
      parent.signal,
      Math.min(3000, parent.remaining()),
    );
    try {
      let answer;
      try {
        answer = await this.egress.send(
          {
            url,
            method: "GET",
            headers: { accept: "application/json" },
            maxResponseBytes: maximum,
          },
          scope,
        );
      } catch (error) {
        throw new AppError(
          error instanceof ResponseTooLarge
            ? "authentication_misconfigured"
            : "authentication_unavailable",
        );
      }
      if (answer.status !== 200)
        throw new AppError("authentication_unavailable");
      return uniqueJson(answer.body, maximum);
    } finally {
      scope.close();
    }
  }
}

const jwkReader = z
  .object({
    kty: z.enum(["OKP", "RSA", "EC"]),
    crv: z.enum(["Ed25519", "P-256"]).nullish(),
    x: z.string().nullish(),
    y: z.string().nullish(),
    n: z.string().nullish(),
    e: z.string().nullish(),
    kid: z
      .string()
      .regex(/^[\x21-\x7e]{1,256}$/)
      .nullish(),
    alg: z.enum(["EdDSA", "RS256", "ES256"]).nullish(),
    use: z.literal("sig").nullish(),
    x5c: z.array(z.string()).nullish(),
  })
  .passthrough();
function publicBytes(text: string): Buffer {
  if (!/^[A-Za-z0-9_-]+$/.test(text)) throw Error("Invalid public key");
  const bytes = Buffer.from(text, "base64url");
  if (bytes.toString("base64url") !== text) throw Error("Invalid public key");
  return bytes;
}
export async function readPublicKeys(value: unknown): Promise<JSONWebKeySet> {
  const set = z
    .strictObject({ keys: z.array(z.unknown()).min(1).max(64) })
    .parse(value);
  const result: JSONWebKeySet = { keys: [] };
  for (const entry of set.keys) {
    // OIDC encryption-only members never enter the signing key reader.
    if (
      typeof entry === "object" &&
      entry !== null &&
      "use" in entry &&
      entry.use === "enc"
    )
      continue;
    const raw = jwkReader.parse(entry);
    if (
      ["d", "p", "q", "dp", "dq", "qi", "oth", "k", "key_ops"].some((name) =>
        Object.hasOwn(raw, name),
      )
    )
      throw Error("Private or restricted key");
    const algorithm =
      raw.kty === "OKP" ? "EdDSA" : raw.kty === "RSA" ? "RS256" : "ES256";
    if (raw.alg && raw.alg !== algorithm)
      throw Error("Incoherent key algorithm");
    if (raw.kty === "OKP") {
      if (
        raw.crv !== "Ed25519" ||
        !raw.x ||
        publicBytes(raw.x).length !== 32 ||
        raw.y ||
        raw.n ||
        raw.e
      )
        throw Error("Invalid Ed25519 public key");
    }
    if (raw.kty === "EC") {
      if (
        raw.crv !== "P-256" ||
        !raw.x ||
        !raw.y ||
        publicBytes(raw.x).length !== 32 ||
        publicBytes(raw.y).length !== 32 ||
        raw.n ||
        raw.e
      )
        throw Error("Invalid P256 public key");
    }
    if (raw.kty === "RSA") {
      if (!raw.n || !raw.e || raw.crv || raw.x || raw.y)
        throw Error("Invalid RSA public key");
      const n = publicBytes(raw.n);
      const e = publicBytes(raw.e);
      const bits = n.length * 8 - Math.clz32(n[0] ?? 0) + 24;
      let exponent = 0;
      for (const byte of e) exponent = exponent * 256 + byte;
      if (
        n[0] === 0 ||
        bits < 2048 ||
        bits > 4096 ||
        !((n.at(-1) ?? 0) & 1) ||
        !e.length ||
        e.length > 4 ||
        e[0] === 0 ||
        exponent < 3 ||
        exponent % 2 !== 1
      )
        throw Error("Invalid RSA public key");
    }
    const key: JWK = {
      kty: raw.kty,
      alg: algorithm,
      ...Object.fromEntries(
        ["kid", "crv", "x", "y", "n", "e", "use"].flatMap((name) =>
          raw[name] === null || raw[name] === undefined
            ? []
            : [[name, raw[name]]],
        ),
      ),
    };
    await importJWK(key, algorithm); // Includes point-on-curve validation for every EC member.
    if (raw.x5c?.length) {
      for (const certificate of raw.x5c)
        if (
          !/^[A-Za-z0-9+/]+={0,2}$/.test(certificate) ||
          Buffer.from(certificate, "base64").toString("base64") !== certificate
        )
          throw Error("Invalid certificate");
      const certified = new X509Certificate(
        Buffer.from(raw.x5c[0] ?? "", "base64"),
      ).publicKey.export({ format: "jwk" });
      for (const field of ["kty", "crv", "x", "y", "n", "e"] as const)
        if (certified[field] !== key[field])
          throw Error("Certificate public key mismatch");
    }
    result.keys.push(key);
  }
  if (!result.keys.length) throw Error("No signing keys");
  return result;
}
