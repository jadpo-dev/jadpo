// Sign-in, in the credential formats Jadpo's runtime issues: an API token
// (`Authorization: Bearer jadpo_token_<secret>.<signed claims>`) or a browser
// session (the `desk_session` cookie, with the CSRF header on writes). Both
// are rows of `jadpo_sessions`, keyed by the SHA-256 of the secret. The signed
// claims let a request skip the session lookup for up to five minutes; the
// person's own record is read every time, so a disabled user is refused.
import { createHash, createHmac, timingSafeEqual } from "node:crypto";
import { and, eq, gt, isNotNull, isNull } from "drizzle-orm";
import { z } from "zod";
import type { Config } from "./config.ts";
import type { Store } from "./db/store.ts";
import { sessions, users } from "./db/schema.ts";
import { AppError } from "./errors.ts";

export const COOKIE = "desk_session";
const CLAIMS_LIFETIME_MS = 300000;

/** Who is calling: always a signed-in user of this desk. */
export type Principal = Readonly<{ id: string; subject: string }>;
export type Authentication = {
  principal: Principal;
  renewal?: readonly [string, string];
};

const compactId = z.string().regex(/^[0-9a-f]{32}$/);
const Claims = z.strictObject({
  enc: z.literal(1),
  m: z.enum(["browser_session", "api_token"]),
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
type Method = Claims["m"];

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

type Selected = { kind: "cookie" | "bearer"; value: string };
/** The one credential a request carries: none is 401
 * `authentication_required`, two are `ambiguous_credentials`. */
export function selectCredential(headers: Headers): Selected {
  const cookies = (headers.get("cookie") ?? "")
    .split(";")
    .map((item) => item.trim())
    .filter((item) => item.split("=")[0] === COOKIE);
  const authorization = headers.get("authorization");
  const presented =
    cookies.length +
    (authorization === null ? 0 : authorization.split(",").length);
  if (presented > 1) throw new AppError("ambiguous_credentials");
  if (!presented) throw new AppError("authentication_required");
  if (cookies.length) {
    const value = cookies[0]?.slice(COOKIE.length + 1);
    if (!value) throw new AppError("invalid_credentials");
    return { kind: "cookie", value };
  }
  const bearer = /^Bearer ([^\s,]+)$/i.exec(authorization ?? "")?.[1];
  if (!bearer || bearer.length > 8192)
    throw new AppError("invalid_credentials");
  return { kind: "bearer", value: bearer };
}

const expanded = (id: string) =>
  `${id.slice(0, 8)}-${id.slice(8, 12)}-${id.slice(12, 16)}-${id.slice(16, 20)}-${id.slice(20)}`;
const compact = (id: string) => id.replaceAll("-", "");

export class AuthenticationService {
  readonly config: Config;
  readonly store: Store;
  readonly clock: () => number;
  constructor(config: Config, store: Store, clock: () => number) {
    this.config = config;
    this.store = store;
    this.clock = clock;
  }

  async authenticate(request: Request): Promise<Authentication> {
    const credential = selectCredential(request.headers);
    const method: Method =
      credential.kind === "cookie" ? "browser_session" : "api_token";
    const value = credential.value;
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
    if (claims && equal(claims.v, verifier)) {
      this.csrf(request, method, claims.c);
      // The session needs no lookup, but the person must still be active.
      const id = expanded(claims.p);
      await this.active(id);
      return { principal: { id, subject: claims.s.slice(1) } };
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
            gt(sessions.ends_at, new Date(this.clock())),
            method === "browser_session"
              ? isNotNull(sessions.csrf_verifier)
              : isNull(sessions.csrf_verifier),
          ),
        )
        .limit(1);
    } catch {
      throw new AppError("authentication_unavailable");
    }
    const row = rows[0];
    if (!row) throw new AppError("invalid_credentials");
    if (row.status !== "active") throw new AppError("user_disabled");
    this.csrf(request, method, row.csrf ?? undefined);
    const principal: Principal = { id: row.id, subject: row.subject };
    // Renew the signed claims, so the next requests skip this lookup.
    const ends = row.ends.getTime();
    const payload: Claims = {
      enc: 1,
      m: method,
      r: compact(row.row),
      v: verifier,
      p: compact(row.id),
      s: "t" + row.subject,
      e: Math.min(this.clock() + CLAIMS_LIFETIME_MS, ends),
      ...(row.csrf ? { c: row.csrf } : {}),
    };
    const renewed =
      secret + "." + sign(JSON.stringify(payload), this.config.signingKey);
    const renewal: readonly [string, string] =
      method === "browser_session"
        ? [
            "set-cookie",
            `${COOKIE}=${renewed}; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=${Math.max(0, Math.floor((ends - this.clock()) / 1000))}`,
          ]
        : ["x-jadpo-token", renewed];
    return { principal, renewal };
  }

  private async active(id: string): Promise<void> {
    let rows;
    try {
      rows = await this.store.db
        .select({ status: users.status })
        .from(users)
        .where(eq(users.id, id))
        .limit(1);
    } catch {
      throw new AppError("authentication_unavailable");
    }
    const row = rows[0];
    if (!row) throw new AppError("invalid_credentials");
    if (row.status !== "active") throw new AppError("user_disabled");
  }

  private claims(value: string, method: Method): Claims | undefined {
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
        result.data.e > this.clock() + CLAIMS_LIFETIME_MS ||
        !result.data.s.startsWith("t")
      )
        return undefined;
      if ((method === "browser_session") !== Boolean(result.data.c))
        return undefined;
      return result.data;
    } catch {
      return undefined;
    }
  }

  // A browser session's writes need the page's own origin and the CSRF
  // proof whose digest the session keeps.
  private csrf(request: Request, method: Method, verifier?: string): void {
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
}
