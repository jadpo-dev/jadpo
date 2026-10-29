import { and, asc, eq, gt, isNull, lt, sql } from "drizzle-orm";
import { z } from "zod";
import type { Store } from "./db/store.ts";
import type { Database } from "./db/store.ts";
import {
  deliveries,
  scheduleClaims,
  providerBreakers,
  scheduleRuns,
  todos,
  users,
} from "./db/schema.ts";
import { AppError } from "./errors.ts";
import type { Logger } from "./errors.ts";
import { RequestScope } from "./runtime.ts";
import type { Clock } from "./runtime.ts";
import type { Egress } from "./egress.ts";
import type { Config } from "./config.ts";
import { MailPayload, QueuedEvent } from "./schemas.ts";
import { MailProvider, ProviderFailure } from "./provider.ts";
import { digest } from "./auth.ts";

export const deliveryKey = (id: string) =>
  "jd1_" + digest("golden-todo:reminder-mail:v1:send:" + id);
export const retryDisposition = (first: number | null, now: number) =>
  first !== null && now - first >= 23 * 3600000 ? "outcome_unknown" : "pending";
class SendHeld extends Error {
  readonly state: "held" | "outcome_unknown";
  readonly until: number;
  constructor(state: "held" | "outcome_unknown", until: number) {
    super("Delivery held.");
    this.state = state;
    this.until = until;
  }
}
type Delivery = typeof deliveries.$inferSelect;
export class Jobs {
  readonly store: Store;
  readonly clock: Clock;
  readonly egress: Egress;
  readonly config: Config;
  readonly logger: Logger;
  readonly provider: MailProvider;
  constructor(options: {
    store: Store;
    clock: Clock;
    egress: Egress;
    config: Config;
    logger: Logger;
  }) {
    this.store = options.store;
    this.clock = options.clock;
    this.egress = options.egress;
    this.config = options.config;
    this.logger = options.logger;
    this.provider = new MailProvider(options.egress, options.config.mailApiKey);
  }
  private async providerPermit(
    row: Delivery,
    scope: RequestScope,
  ): Promise<void> {
    await this.store.transaction(scope, async (db) => {
      await db
        .insert(providerBreakers)
        .values({ name: "reminder_mail" })
        .onConflictDoNothing();
      const [breaker] = await db
        .select()
        .from(providerBreakers)
        .where(eq(providerBreakers.name, "reminder_mail"))
        .for("update");
      if (!breaker) throw new AppError("unavailable");
      if (breaker.open_until === null) return;
      if (Date.parse(breaker.open_until) > this.clock())
        throw new SendHeld("held", Date.parse(breaker.open_until));
      if (breaker.probe_until && Date.parse(breaker.probe_until) > this.clock())
        throw new SendHeld("held", Date.parse(breaker.probe_until));
      await db
        .update(providerBreakers)
        .set({ probe_token: row.lease_token, probe_until: row.lease_until })
        .where(eq(providerBreakers.name, "reminder_mail"));
    });
  }
  private async providerOutcome(
    row: Delivery,
    unavailable: boolean,
    scope: RequestScope,
  ): Promise<void> {
    await this.store.transaction(scope, async (db) => {
      const [breaker] = await db
        .select()
        .from(providerBreakers)
        .where(eq(providerBreakers.name, "reminder_mail"))
        .for("update");
      if (!breaker) return;
      if (breaker.probe_token && breaker.probe_token !== row.lease_token)
        return;
      if (!unavailable)
        await db
          .update(providerBreakers)
          .set({
            failures: 0,
            open_until: null,
            backoff_ms: 60000,
            probe_token: null,
            probe_until: null,
          })
          .where(eq(providerBreakers.name, "reminder_mail"));
      else {
        const failures = breaker.failures + 1;
        const opened = failures >= 5;
        await db
          .update(providerBreakers)
          .set({
            failures,
            open_until: opened
              ? new Date(this.clock() + breaker.backoff_ms).toISOString()
              : null,
            backoff_ms: opened
              ? Math.min(600000, breaker.backoff_ms * 2)
              : breaker.backoff_ms,
            probe_token: null,
            probe_until: null,
          })
          .where(eq(providerBreakers.name, "reminder_mail"));
      }
    });
  }
  private async work<T>(body: (scope: RequestScope) => Promise<T>): Promise<T> {
    const scope = new RequestScope(new AbortController().signal, 30000);
    try {
      return await this.store.run(scope, () => body(scope));
    } finally {
      scope.close();
    }
  }
  private eligible(now: string, revision?: string, id?: string) {
    return and(
      eq(todos.status, "open"),
      isNull(todos.deleted_at),
      isNull(todos.reminder_sent_at),
      lt(todos.due_at, now),
      eq(users.status, "active"),
      revision === undefined
        ? undefined
        : eq(todos.reminder_revision, BigInt(revision)),
      id === undefined ? undefined : eq(todos.id, id),
    );
  }
  private async enqueue(
    db: Database,
    subscriber: string,
    payload: unknown,
    now: string,
  ): Promise<void> {
    const id = crypto.randomUUID();
    await db.insert(deliveries).values({
      id,
      subscriber,
      payload,
      status: "pending",
      next_attempt_at: now,
      provider_key: deliveryKey(id),
      created_at: now,
    });
  }
  async schedule(name = "overdue_reminders"): Promise<number> {
    return await this.work(async (scope) => {
      if (name !== "overdue_reminders" && name !== "Todo.purge")
        throw new AppError("invalid_input");
      if (name === "Todo.purge") return await this.purge(scope);
      const amount = await this.store.transaction(scope, async (db) => {
        const now = new Date(this.clock()).toISOString();
        const candidates = await db
          .select({ id: todos.id, revision: todos.reminder_revision })
          .from(todos)
          .innerJoin(users, eq(users.id, todos.owner_id))
          .where(and(this.eligible(now), isNull(todos.reminder_queued_at)))
          .orderBy(asc(todos.created_at), asc(todos.id))
          .limit(500);
        for (const row of candidates) {
          const changed = await db
            .update(todos)
            .set({ reminder_queued_at: now })
            .where(
              and(
                eq(todos.id, row.id),
                eq(todos.reminder_revision, row.revision),
                isNull(todos.reminder_queued_at),
              ),
            )
            .returning({ id: todos.id });
          if (changed.length)
            await this.enqueue(
              db,
              "prepare_delivery",
              QueuedEvent.parse({
                todo_id: row.id,
                revision: row.revision.toString(),
              }),
              now,
            );
        }
        return candidates.length;
      });
      await this.store.db.insert(scheduleRuns).values({
        id: crypto.randomUUID(),
        schedule: name,
        started_at: new Date(this.clock()).toISOString(),
        outcome: "succeeded",
        items: amount,
      });
      return amount;
    });
  }
  private async purge(scope: RequestScope): Promise<number> {
    let amount = 0;
    const cutoff = new Date(
      this.clock() - this.config.retentionMs,
    ).toISOString();
    for (let batch = 0; batch < 20; batch++) {
      const rows = await this.store.transaction(
        scope,
        async (db) =>
          await db.execute(
            sql`DELETE FROM "todo" WHERE "id" IN (SELECT "id" FROM "todo" WHERE "deleted_at" < ${cutoff} ORDER BY "deleted_at", "id" LIMIT 500) RETURNING "id"`,
          ),
      );
      amount += rows.length;
      if (rows.length < 500) break;
    }
    await this.store.db.insert(scheduleRuns).values({
      id: crypto.randomUUID(),
      schedule: "Todo.purge",
      started_at: new Date(this.clock()).toISOString(),
      outcome: "succeeded",
      items: amount,
    });
    return amount;
  }
  async tick(): Promise<void> {
    for (const [name, interval] of [
      ["overdue_reminders", 60000],
      ["Todo.purge", 3600000],
      ["RequestReceipts.cleanup", 60000],
    ] as const) {
      const bucket = Math.floor(this.clock() / interval);
      const execute = await this.work(
        async (scope) =>
          await this.store.transaction(scope, async (db) => {
            const inserted = await db
              .insert(scheduleClaims)
              .values({ name, bucket })
              .onConflictDoNothing()
              .returning({ name: scheduleClaims.name });
            if (inserted.length) return false; // Startup records the current interval; the next tick executes it.
            const claim = await db
              .update(scheduleClaims)
              .set({ bucket })
              .where(
                and(
                  eq(scheduleClaims.name, name),
                  lt(scheduleClaims.bucket, bucket),
                ),
              )
              .returning({ name: scheduleClaims.name });
            return claim.length === 1;
          }),
      );
      if (execute) {
        if (name === "RequestReceipts.cleanup")
          await this.work(async (scope) => {
            for (let batch = 0; batch < 10; batch++) {
              const now = new Date(this.clock()).toISOString();
              const removed = await this.store.transaction(
                scope,
                async (db) =>
                  await db.execute(
                    sql`DELETE FROM "todo_request_receipts" WHERE ctid IN (SELECT ctid FROM "todo_request_receipts" WHERE "expires_at"<=${now} OR "response_status" IS NULL AND "lease_until"<=${now} ORDER BY "expires_at" LIMIT 1000) RETURNING "key"`,
                  ),
              );
              if (removed.length < 1000) break;
            }
          });
        else await this.schedule(name);
      }
    }
    await this.drain();
  }
  private async claim(scope: RequestScope): Promise<Delivery | undefined> {
    return await this.store.transaction(scope, async (db) => {
      const now = new Date(this.clock()).toISOString();
      const rows = await db.execute(
        sql`UPDATE "jadpo_deliveries" SET "status"='running', "lease_token"=${crypto.randomUUID()}, "lease_until"=${new Date(this.clock() + 30000).toISOString()}, "attempts"="attempts"+1 WHERE "id"=(SELECT "id" FROM "jadpo_deliveries" WHERE ("status" IN ('pending','held') OR "status"='running' AND "lease_until" < ${now}) AND "next_attempt_at" <= ${now} ORDER BY "created_at", "subscriber", "id" FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *`,
      );
      const row = rows[0];
      if (!row) return undefined;
      // Raw statement is required for atomic SKIP LOCKED leasing; validate the result.
      return deliveryRow.parse(row);
    });
  }
  private fenced(row: Delivery) {
    return and(
      eq(deliveries.id, row.id),
      eq(deliveries.status, "running"),
      eq(deliveries.lease_token, row.lease_token ?? ""),
      gt(deliveries.lease_until, new Date(this.clock()).toISOString()),
    );
  }
  private async prepare(row: Delivery, scope: RequestScope): Promise<void> {
    const event = QueuedEvent.parse(row.payload);
    await this.store.transaction(scope, async (db) => {
      const now = new Date(this.clock()).toISOString();
      const [current] = await db
        .select({
          id: todos.id,
          title: todos.title,
          revision: todos.reminder_revision,
          email: users.email,
        })
        .from(todos)
        .innerJoin(users, eq(users.id, todos.owner_id))
        .where(this.eligible(now, event.revision, event.todo_id))
        .limit(1);
      // Completion fencing happens in the same transaction as the second outbox event.
      const claimed = await db
        .update(deliveries)
        .set({ status: "handled", lease_until: null, lease_token: null })
        .where(this.fenced(row))
        .returning({ id: deliveries.id });
      if (!claimed.length) throw new AppError("unavailable");
      if (current)
        await this.enqueue(
          db,
          "send_reminder",
          MailPayload.parse({
            to: current.email,
            title: current.title,
            todo_id: current.id,
            revision: current.revision.toString(),
          }),
          now,
        );
    });
  }
  private async send(row: Delivery, scope: RequestScope): Promise<void> {
    const payload = MailPayload.parse(row.payload);
    const first =
      row.first_attempt_at === null ? null : Date.parse(row.first_attempt_at);
    if (retryDisposition(first, this.clock()) === "outcome_unknown") {
      await this.store.transaction(scope, async (db) => {
        await db
          .update(deliveries)
          .set({
            status: "outcome_unknown",
            lease_token: null,
            lease_until: null,
          })
          .where(this.fenced(row));
      });
      return;
    }
    await this.providerPermit(row, scope);
    const firstAttempt =
      row.first_attempt_at ?? new Date(this.clock()).toISOString();
    // The current eligibility snapshot is released before provider IO.
    const current = await this.store.transaction(scope, async (db) => {
      const [todo] = await db
        .select({ id: todos.id, version: sql<string>`"todo".xmin::text` })
        .from(todos)
        .innerJoin(users, eq(users.id, todos.owner_id))
        .where(
          this.eligible(
            new Date(this.clock()).toISOString(),
            payload.revision,
            payload.todo_id,
          ),
        )
        .limit(1);
      if (!todo) {
        await db
          .update(deliveries)
          .set({ status: "handled", lease_token: null, lease_until: null })
          .where(this.fenced(row));
        return undefined;
      }
      const changes = await db
        .update(deliveries)
        .set({
          first_attempt_at: firstAttempt,
        })
        .where(this.fenced(row))
        .returning({ id: deliveries.id });
      if (!changes.length) throw new AppError("unavailable");
      return {
        ...todo,
        firstAttempt: firstAttempt,
      };
    });
    if (!current) return;
    scope.check();
    if (!row.lease_until || Date.parse(row.lease_until) <= this.clock()) return;
    if (
      retryDisposition(Date.parse(current.firstAttempt), this.clock()) ===
      "outcome_unknown"
    ) {
      await this.store.transaction(scope, async (db) => {
        await db
          .update(deliveries)
          .set({
            status: "outcome_unknown",
            lease_token: null,
            lease_until: null,
          })
          .where(this.fenced(row));
      });
      return;
    }
    let receipt;
    try {
      receipt = await this.provider.send(
        payload,
        row.provider_key,
        scope,
        async () => {
          if (
            retryDisposition(Date.parse(firstAttempt), this.clock()) ===
            "outcome_unknown"
          )
            throw new SendHeld("outcome_unknown", this.clock());
          const [lease] = await this.store.db
            .select({
              token: deliveries.lease_token,
              until: deliveries.lease_until,
              status: deliveries.status,
            })
            .from(deliveries)
            .where(eq(deliveries.id, row.id))
            .limit(1);
          scope.check();
          if (
            retryDisposition(Date.parse(firstAttempt), this.clock()) ===
            "outcome_unknown"
          )
            throw new SendHeld("outcome_unknown", this.clock());
          if (
            !lease ||
            lease.token !== row.lease_token ||
            lease.status !== "running" ||
            !lease.until ||
            Date.parse(lease.until) <= this.clock()
          )
            throw new AppError("unavailable");
        },
      );
      await this.providerOutcome(row, false, scope);
    } catch (error) {
      if (error instanceof ProviderFailure)
        await this.providerOutcome(row, !error.permanent, scope);
      throw error;
    }
    await this.store.transaction(scope, async (db) => {
      const claimed = await db
        .update(deliveries)
        .set({
          status: "handled",
          lease_token: null,
          lease_until: null,
          last_failure: null,
        })
        .where(this.fenced(row))
        .returning({ id: deliveries.id });
      if (!claimed.length) throw new AppError("unavailable");
      // xmin prevents an acknowledgement from marking a row edited during IO.
      const updated = await db
        .update(todos)
        .set({ reminder_sent_at: receipt.accepted_at })
        .where(
          and(
            eq(todos.id, current.id),
            eq(todos.reminder_revision, BigInt(payload.revision)),
            sql`"todo".xmin::text = ${current.version}`,
          ),
        )
        .returning({ id: todos.id });
      if (!updated.length) throw new AppError("unavailable");
    });
  }
  async deliverOne(): Promise<boolean> {
    return await this.work(async (scope) => {
      const row = await this.claim(scope);
      if (!row) return false;
      try {
        const first =
          row.first_attempt_at === null
            ? null
            : Date.parse(row.first_attempt_at);
        if (
          row.subscriber === "send_reminder" &&
          retryDisposition(first, this.clock()) === "outcome_unknown"
        )
          throw new SendHeld("outcome_unknown", this.clock());
        if (
          row.attempts > 5 ||
          this.clock() - Date.parse(row.created_at) >= 86400000
        ) {
          await this.store.transaction(scope, async (db) => {
            await db
              .update(deliveries)
              .set({
                status: "failed",
                attempts: row.attempts - 1,
                lease_token: null,
                lease_until: null,
                last_failure: row.last_failure ?? {
                  code: "deadline_exceeded",
                  delivery_id: row.id,
                },
              })
              .where(this.fenced(row));
          });
          return true;
        }
        if (row.subscriber === "prepare_delivery")
          await this.prepare(row, scope);
        else if (row.subscriber === "send_reminder")
          await this.send(row, scope);
        else throw new AppError("internal_fault");
      } catch (error) {
        if (error instanceof SendHeld) {
          await this.store.transaction(scope, async (db) => {
            await db
              .update(deliveries)
              .set({
                status: error.state,
                attempts: Math.max(
                  0,
                  row.attempts - (error.state === "held" ? 1 : 0),
                ),
                lease_token: null,
                lease_until: null,
                next_attempt_at: new Date(error.until).toISOString(),
              })
              .where(
                and(
                  eq(deliveries.id, row.id),
                  eq(deliveries.lease_token, row.lease_token ?? ""),
                  eq(deliveries.status, "running"),
                ),
              );
          });
          return true;
        }
        const defect =
          !(error instanceof ProviderFailure) &&
          (!(error instanceof AppError) || error.code === "internal_fault");
        const permanent =
          defect || (error instanceof ProviderFailure && error.permanent);
        const failure = {
          code: defect
            ? "internal_fault"
            : permanent
              ? "provider_refused"
              : "unavailable",
          delivery_id: row.id,
        };
        this.logger({
          event: "job_failed",
          request_id: row.id,
          code: failure.code,
        });
        const delay = Math.max(
          1000,
          Math.floor(
            Math.random() * Math.min(21600000, 30000 * 2 ** (row.attempts - 1)),
          ),
        );
        const exhausted =
          row.attempts >= 5 ||
          this.clock() + delay - Date.parse(row.created_at) >= 86400000;
        await this.store.transaction(scope, async (db) => {
          await db
            .update(deliveries)
            .set({
              status: permanent || exhausted ? "failed" : "pending",
              lease_until: null,
              lease_token: null,
              last_failure: failure,
              next_attempt_at: new Date(this.clock() + delay).toISOString(),
            })
            .where(this.fenced(row));
        });
      }
      return true;
    });
  }
  async drain(): Promise<void> {
    for (let work = 0; work < 3000; work++)
      if (!(await this.deliverOne())) return;
  }
  async sweep(): Promise<void> {
    await this.drain();
  }
}
const deliveryRow = z.object({
  id: z.uuid(),
  subscriber: z.string(),
  payload: z.unknown(),
  status: z.string(),
  lease_token: z.uuid().nullable(),
  lease_until: z.string().nullable(),
  attempts: z.number().int(),
  first_attempt_at: z.string().nullable(),
  next_attempt_at: z.string(),
  provider_key: z.string(),
  last_failure: z.unknown().nullable(),
  created_at: z.string(),
});
