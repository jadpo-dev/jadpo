import { expect } from "vitest";
import { z } from "zod";
import { sql } from "drizzle-orm";
import { Idempotency } from "../src/idempotency.ts";
import { RequestScope } from "../src/runtime.ts";
import type { Store } from "../src/db/store.ts";

export async function idempotencyNegatives(store: Store) {
  await store.client.unsafe("TRUNCATE todo_request_receipts");
  let now = 1768478400000;
  const receipts = new Idempotency(store, () => now);
  const principal = {
    kind: "user",
    id: "00000000-0000-4000-8000-000000000001",
    subject: "alice",
  } as const;
  const scopes: RequestScope[] = [];
  const scope = () => {
    const lifetime = new RequestScope(new AbortController().signal, 3000);
    scopes.push(lifetime);
    return lifetime;
  };
  const execute = (
    key: string,
    lifetime: RequestScope,
    action: () => Promise<unknown>,
    who = principal,
  ) =>
    store.run(lifetime, () =>
      receipts.execute({
        principal: who,
        key,
        route: "PATCH /users/{id}",
        decoded: { status: "disabled" },
        status: 200,
        output: z.strictObject({ ok: z.literal(true) }),
        scope: lifetime,
        requestId: crypto.randomUUID(),
        action,
      }),
    );
  try {
    let enter!: () => void;
    let unblock!: () => void;
    const entered = new Promise<void>((resolve) => {
      enter = resolve;
    });
    const barrier = new Promise<void>((resolve) => {
      unblock = resolve;
    });
    const first = execute("concurrent", scope(), async () => {
      await store.transaction(scopes[0]!, async (db) => {
        await db.execute(sql`UPDATE "user" SET status='disabled'`);
      });
      enter();
      await barrier;
      return { ok: true };
    });
    await entered;
    await expect(
      execute("concurrent", scope(), () => Promise.resolve({ ok: true })),
    ).rejects.toThrow("Conflict.");
    unblock();
    await first;
    await expect(
      execute("concurrent", scope(), () => {
        throw Error("Replay executed mutation");
      }),
    ).resolves.toEqual({ ok: true });
    await store.client.unsafe("UPDATE \"user\" SET status='active'");
    let kept: { status: number; body: string } | undefined;
    const changedScope = scope();
    await store.run(changedScope, () =>
      receipts.execute({
        principal,
        key: "concurrent",
        route: "PATCH /users/{id}",
        decoded: { status: "disabled" },
        status: 202,
        output: z.strictObject({ changed: z.literal(true) }),
        scope: changedScope,
        requestId: crypto.randomUUID(),
        onReplay: (response) => {
          kept = response;
        },
        action: () => {
          throw Error("Changed build reran action");
        },
      }),
    );
    expect(kept).toEqual({ status: 200, body: '{"ok":true}' });
    // A cancelled attempt must keep neither the mutation nor a completed receipt.
    const abort = new AbortController();
    const cancelled = new RequestScope(abort.signal, 3000);
    scopes.push(cancelled);
    await expect(
      execute("cancelled", cancelled, async () => {
        await store.transaction(cancelled, async (db) => {
          await db.execute(sql`UPDATE "user" SET status='disabled'`);
        });
        abort.abort();
        return { ok: true };
      }),
    ).rejects.toThrow();
    expect(
      (await store.client.unsafe('SELECT status FROM "user"'))[0]?.status,
    ).toBe("active");
    expect(
      (
        await store.client.unsafe(
          "SELECT count(*) AS n FROM todo_request_receipts WHERE key='cancelled' AND response_status IS NOT NULL",
        )
      )[0]?.n,
    ).toBe("0");
    // A replaced lease fences the old runner's mutation and response together.
    let began!: () => void;
    let resume!: () => void;
    const begun = new Promise<void>((resolve) => {
      began = resolve;
    });
    const blocked = new Promise<void>((resolve) => {
      resume = resolve;
    });
    let calls = 0;
    const stale = scope();
    const old = execute("lease", stale, async () => {
      await store.transaction(stale, async (db) => {
        await db.execute(sql`UPDATE "user" SET status='disabled'`);
      });
      if (++calls === 1) {
        began();
        await blocked;
      }
      return { ok: true };
    });
    await begun;
    now += 16000;
    await execute("lease", scope(), () => Promise.resolve({ ok: true }));
    resume();
    await expect(old).rejects.toThrow("Unavailable.");
    expect(
      (await store.client.unsafe('SELECT status FROM "user"'))[0]?.status,
    ).toBe("active");
  } finally {
    scopes.forEach((lifetime) => {
      lifetime.close();
    });
  }
}
