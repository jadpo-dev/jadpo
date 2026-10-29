import { and, eq, gt, sql } from "drizzle-orm";
import type { z } from "zod";
import type { Store, Database } from "./db/store.ts";
import { idempotency } from "./db/schema.ts";
import { digest } from "./auth.ts";
import { AppError, publicFailure } from "./errors.ts";
import { RequestScope } from "./runtime.ts";
import type { Clock } from "./runtime.ts";
import { IdempotencyKey } from "./schemas.ts";
import type { Principal } from "./schemas.ts";

function canonical(value: unknown): string {
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (value !== null && typeof value === "object")
    return (
      "{" +
      Object.entries(value)
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .filter(([, value]) => value !== undefined)
        .map(([key, value]) => JSON.stringify(key) + ":" + canonical(value))
        .join(",") +
      "}"
    );
  return JSON.stringify(value) ?? "null";
}
export class Idempotency {
  readonly store: Store;
  readonly clock: Clock;
  constructor(store: Store, clock: Clock) {
    this.store = store;
    this.clock = clock;
  }
  async execute(options: {
    principal: Principal;
    key: string | null;
    route: string;
    decoded: unknown;
    status: number;
    output: z.ZodType;
    scope: RequestScope;
    requestId: string;
    onReplay?: (response: { status: number; body: string }) => void;
    action: () => Promise<unknown>;
  }): Promise<unknown> {
    const { key, scope, action, output } = options;
    if (key === null) return await action();
    if (!IdempotencyKey.safeParse(key).success)
      throw new AppError("invalid_request");
    const caller = options.principal.kind + " " + options.principal.id;
    const fingerprint = digest(
      canonical({ route: options.route, input: options.decoded }),
    );
    const token = crypto.randomUUID();
    const now = new Date(this.clock()).toISOString();
    const until = new Date(
      this.clock() + scope.remaining() + 5000,
    ).toISOString();
    scope.check();
    const claimed = await this.store.db.execute(
      sql`INSERT INTO "todo_request_receipts" ("caller","key","fingerprint","token","lease_until","expires_at") VALUES (${caller},${key},${fingerprint},${token},${until},${new Date(this.clock() + 86400000).toISOString()}) ON CONFLICT ("caller","key") DO UPDATE SET "fingerprint"=EXCLUDED."fingerprint","token"=EXCLUDED."token","lease_until"=EXCLUDED."lease_until","expires_at"=EXCLUDED."expires_at","response_status"=NULL,"response_body"=NULL WHERE "todo_request_receipts"."expires_at"<=${now} OR "todo_request_receipts"."response_status" IS NULL AND "todo_request_receipts"."lease_until"<=${now} RETURNING "token"`,
    );
    scope.check();
    if (!claimed.length) {
      const [saved] = await this.store.db
        .select()
        .from(idempotency)
        .where(and(eq(idempotency.caller, caller), eq(idempotency.key, key)))
        .limit(1);
      scope.check();
      if (!saved) throw new AppError("unavailable");
      if (saved.fingerprint !== fingerprint)
        throw new AppError("idempotency_key_reused");
      if (saved.response_status === null || saved.response_body === null)
        throw new AppError("conflict");
      options.onReplay?.({
        status: saved.response_status,
        body: saved.response_body,
      });
      return saved.response_body === ""
        ? undefined
        : (JSON.parse(saved.response_body) as unknown);
    }
    const predicate = () =>
      and(
        eq(idempotency.caller, caller),
        eq(idempotency.key, key),
        eq(idempotency.token, token),
        gt(idempotency.lease_until, new Date(this.clock()).toISOString()),
      );
    const finish = async (db: Database, status: number, body: string) => {
      const rows = await db
        .update(idempotency)
        .set({ response_status: status, response_body: body })
        .where(predicate())
        .returning({ token: idempotency.token });
      if (!rows.length) throw new AppError("unavailable");
    };
    try {
      return await this.store.transaction(scope, async (db) => {
        const result = output.parse(await action());
        scope.check();
        await finish(
          db,
          options.status,
          result === undefined ? "" : JSON.stringify(result),
        );
        return result;
      });
    } catch (error) {
      const stable =
        error instanceof AppError &&
        ![
          "unavailable",
          "authentication_unavailable",
          "internal_fault",
          "deadline_exceeded",
        ].includes(error.code);
      // Cleanup is bounded and awaited even when the original request is cancelled.
      // It can only touch this runner's still-pending claim, never another lease.
      const cleanup = new RequestScope(new AbortController().signal, 1000);
      try {
        await this.store.transaction(cleanup, async (db) => {
          if (stable && !scope.signal.aborted) {
            const safe = publicFailure(error, options.requestId);
            await finish(db, safe.status, JSON.stringify(safe.body));
          } else
            await db
              .delete(idempotency)
              .where(
                and(
                  eq(idempotency.caller, caller),
                  eq(idempotency.key, key),
                  eq(idempotency.token, token),
                ),
              );
        });
      } finally {
        cleanup.close();
      }
      throw error;
    }
  }
}
