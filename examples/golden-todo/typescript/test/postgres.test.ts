import { afterAll, beforeEach, describe, expect, it } from "vitest";
import { sql } from "drizzle-orm";
import { Store } from "../src/db/store.ts";
import { RequestScope } from "../src/runtime.ts";
import { jobNegatives } from "./jobs.postgres.ts";
import { idempotencyNegatives } from "./idempotency.postgres.ts";
import { httpNegatives } from "./http.postgres.ts";
import { revisionNegatives } from "./revisions.postgres.ts";
import { resolverNegatives } from "./auth-resolver.postgres.ts";
import { externalNegatives } from "./auth.postgres.ts";
import { Egress } from "../src/egress.ts";
import { TodoService } from "../src/todos.ts";

// npm test runs fast unit tests; adapter/run.py --unit requires and supplies the
// isolated real database. Running this file explicitly without it is an error.
const url = process.env.TEST_DATABASE_URL;
if (!url)
  throw Error(
    "TEST_DATABASE_URL required; use adapter/run.py --unit with an explicit contract source.",
  );
describe.sequential("real PostgreSQL mutation/lifetime boundaries", () => {
  const store = new Store(url ?? "postgres://127.0.0.1/unavailable");
  const scope = () => new RequestScope(new AbortController().signal, 3000);
  const user = "00000000-0000-4000-8000-000000000001";
  const todo = "00000000-0000-4000-8000-000000000011";
  beforeEach(async () => {
    await store.client.unsafe(
      'TRUNCATE "todo", "user", "service", "jadpo_sessions", "jadpo_service_keys" CASCADE',
    );
    await store.client.unsafe(
      `INSERT INTO "user" VALUES ('${user}','alice','active','alice@example.test')`,
    );
  });
  afterAll(async () => {
    await store.close();
  });
  it("creates default entity IDs as UUIDv7 and persists the same identifier", async () => {
    const lifetime = scope();
    try {
      const service = new TodoService(store, Date.now);
      const created = await service.create(
        { kind: "user", id: user, subject: "alice" },
        { title: "Generated", due_at: null },
        lifetime,
      );
      expect(created.id).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
      );
      expect((await store.client.unsafe("SELECT id FROM todo"))[0]?.id).toBe(
        created.id,
      );
    } finally {
      lifetime.close();
    }
  });
  it("verifies the strict external JOSE and local provisioning boundary for every allowed algorithm", async () => {
    await externalNegatives(store, url);
  }, 20000);
  it("bounds and coalesces issuer resolution with safe failure classifications", async () => {
    await resolverNegatives(store, url);
  });
  it("preserves signed64 revision storage, ABA fences, outbox and exact provider bytes", async () => {
    await revisionNegatives(store, url);
  });
  it("enforces HTTP decoding boundaries over a real socket", async () => {
    await httpNegatives(store, url);
  });
  it("fences idempotency replay, concurrent claims, cancelled writes and expired runners", async () => {
    await idempotencyNegatives(store);
  });
  it("enforces durable subscriber budgets and connection circuit holds", async () => {
    await jobNegatives(store, url);
  }, 15000);
  it("rolls back a mutation when its actual public response is invalid", async () => {
    await store.client.unsafe(
      `INSERT INTO todo(id,owner_id,title,status,reminder_revision,created_at,updated_at) VALUES ('${todo}','${user}','Original','corrupt',1,now(),now())`,
    );
    const lifetime = scope();
    try {
      await expect(
        new TodoService(store, Date.now).patch(
          { kind: "user", id: user, subject: "alice" },
          todo,
          { title: "Edited" },
          lifetime,
        ),
      ).rejects.toThrow();
    } finally {
      lifetime.close();
    }
    const rows = await store.client.unsafe("SELECT title FROM todo");
    expect(rows[0]?.title).toBe("Original");
  });
  it("cancels active SQL and awaits rollback before returning the connection", async () => {
    const abort = new AbortController();
    const lifetime = new RequestScope(abort.signal, 3000);
    const started = performance.now();
    const execution = store.transaction(lifetime, async (db) => {
      await db.execute(sql`UPDATE "user" SET status='disabled'`);
      await db.execute(sql`SELECT pg_sleep(5)`);
    });
    const timer = setTimeout(() => {
      abort.abort();
    }, 100);
    try {
      await expect(execution).rejects.toThrow();
    } finally {
      clearTimeout(timer);
      lifetime.close();
    }
    expect(performance.now() - started).toBeLessThan(2000);
    const rows = await store.client.unsafe('SELECT status FROM "user"');
    expect(rows[0]?.status).toBe("active");
  });
  it("removes a cancelled saturated-pool waiter without ever beginning its action", async () => {
    const holds = Array.from({ length: 5 }, () => scope());
    let unblock!: () => void;
    const waiting = new Promise<void>((resolve) => {
      unblock = resolve;
    });
    let entered = 0;
    let ready!: () => void;
    const allEntered = new Promise<void>((resolve) => {
      ready = resolve;
    });
    const executions = holds.map((lifetime) =>
      store.transaction(lifetime, async () => {
        if (++entered === 5) ready();
        await waiting;
      }),
    );
    await allEntered;
    const abort = new AbortController();
    const cancelled = new RequestScope(abort.signal, 3000);
    let ran = false;
    const queued = store.transaction(cancelled, () => {
      ran = true;
      return Promise.resolve();
    });
    abort.abort();
    await expect(queued).rejects.toThrow();
    unblock();
    await Promise.all(executions);
    expect(ran).toBe(false);
    cancelled.close();
    holds.forEach((lifetime) => {
      lifetime.close();
    });
  });
  it("refuses a CPU-only deadline overrun before committing a mutation", async () => {
    const lifetime = new RequestScope(new AbortController().signal, 20);
    try {
      await expect(
        store.transaction(lifetime, async (db) => {
          await db.execute(sql`UPDATE "user" SET status='disabled'`);
          const until = performance.now() + 40;
          while (performance.now() < until) {
            /* synchronous work */
          }
        }),
      ).rejects.toThrow("Request deadline exceeded.");
    } finally {
      lifetime.close();
    }
    const rows = await store.client.unsafe('SELECT status FROM "user"');
    expect(rows[0]?.status).toBe("active");
  });
  it("retries the complete serializable transaction after a real concurrent change", async () => {
    await store.client.unsafe(
      `INSERT INTO todo(id,owner_id,title,status,reminder_revision,created_at,updated_at) VALUES ('${todo}','${user}','0','open',1,now(),now())`,
    );
    let arrived = 0;
    let go!: () => void;
    const barrier = new Promise<void>((resolve) => {
      go = resolve;
    });
    let attempts = 0;
    const lifetimes = [scope(), scope()];
    try {
      await Promise.all(
        lifetimes.map((lifetime) =>
          store.transaction(lifetime, async (db) => {
            attempts++;
            const rows = await db.execute(
              sql`SELECT title FROM todo WHERE id=${todo}`,
            );
            if (++arrived === 2) go();
            if (arrived <= 2) await barrier;
            const title = String(Number(rows[0]?.title) + 1);
            await db.execute(
              sql`UPDATE todo SET title=${title} WHERE id=${todo}`,
            );
          }),
        ),
      );
    } finally {
      lifetimes.forEach((lifetime) => {
        lifetime.close();
      });
    }
    expect(attempts).toBe(3);
    const rows = await store.client.unsafe("SELECT title FROM todo");
    expect(rows[0]?.title).toBe("2");
  });
  it("refuses provider IO inside a real mutation transaction before transport and rolls back", async () => {
    let reached = false;
    const egress = new Egress(() => {
      reached = true;
      return Promise.resolve({ status: 200, body: "{}" });
    });
    const lifetime = scope();
    try {
      await expect(
        store.transaction(lifetime, async (db) => {
          await db.execute(sql`UPDATE "user" SET status='disabled'`);
          await egress.send(
            {
              url: "https://mail.example.invalid/v1/reminders",
              method: "POST",
              headers: {},
            },
            lifetime,
          );
        }),
      ).rejects.toThrow("Internal fault.");
    } finally {
      lifetime.close();
    }
    expect(reached).toBe(false);
    const rows = await store.client.unsafe('SELECT status FROM "user"');
    expect(rows[0]?.status).toBe("active");
  });
});
