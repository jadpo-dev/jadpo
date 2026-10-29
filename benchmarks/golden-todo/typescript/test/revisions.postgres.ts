import { expect } from "vitest";
import { eq } from "drizzle-orm";
import { Jobs } from "../src/jobs.ts";
import { TodoService } from "../src/todos.ts";
import { Egress, type OutboundRequest } from "../src/egress.ts";
import { loadConfig } from "../src/config.ts";
import { RequestScope } from "../src/runtime.ts";
import { todos } from "../src/db/schema.ts";
import type { Store } from "../src/db/store.ts";

export async function revisionNegatives(store: Store, url: string) {
  const user = "00000000-0000-4000-8000-000000000001";
  const principal = { kind: "user", id: user, subject: "alice" } as const;
  const config = loadConfig({
    DATABASE_URL: url,
    BROWSER_ORIGIN: "https://todo.example.test",
    OIDC_ISSUER: "https://identity.example/tenant/",
    OIDC_AUDIENCE: "golden-todo",
    SESSION_SIGNING_KEY: "x".repeat(43),
    MAIL_API_KEY: "private-provider-secret",
  });
  for (const base of [2147483648n, 9007199254740993n, 9223372036854775804n]) {
    let now = Date.parse("2026-01-15T12:00:00Z");
    const id = crypto.randomUUID();
    const calls: OutboundRequest[] = [];
    const jobs = new Jobs({
      config,
      store,
      clock: () => now,
      logger: () => {},
      egress: new Egress((request) => {
        calls.push(request);
        return Promise.resolve({
          status: 200,
          body: JSON.stringify({ accepted_at: new Date(now).toISOString() }),
        });
      }),
    });
    const service = new TodoService(store, () => now);
    const patch = async (due: string) => {
      const scope = new RequestScope(new AbortController().signal);
      try {
        await service.patch(principal, id, { due_at: due }, scope);
      } finally {
        scope.close();
      }
    };
    await store.client.unsafe(
      "TRUNCATE todo,jadpo_deliveries,todo_provider_breakers",
    );
    await store.client.unsafe(
      `INSERT INTO todo(id,owner_id,title,status,due_at,reminder_revision,created_at,updated_at) VALUES ('${id}','${user}','Original','open','2026-01-15T13:00:00Z',${base},'2026-01-15T12:00:00Z','2026-01-15T12:00:00Z')`,
    );
    // A→B→A before due advances the exact integer each time, including >2^53.
    await patch("2026-01-15T14:00:00Z");
    await patch("2026-01-15T13:00:00Z");
    const [row] = await store.db
      .select({ revision: todos.reminder_revision })
      .from(todos)
      .where(eq(todos.id, id));
    expect(String(row?.revision)).toBe(String(base + 2n));
    now = Date.parse("2026-01-15T14:01:00Z");
    await jobs.schedule();
    const queued = await store.client.unsafe(
      "SELECT payload FROM jadpo_deliveries WHERE subscriber='prepare_delivery'",
    );
    expect(queued[0]?.payload).toEqual({
      todo_id: id,
      revision: String(base + 2n),
    });
    await jobs.deliverOne();
    const prepared = await store.client.unsafe(
      "SELECT payload FROM jadpo_deliveries WHERE subscriber='send_reminder'",
    );
    expect(prepared[0]?.payload).toEqual({
      to: "alice@example.test",
      title: "Original",
      todo_id: id,
      revision: String(base + 2n),
    });
    await patch("2026-01-15T15:00:00Z");
    await jobs.drain();
    expect(calls).toHaveLength(0); // Old prepared revision cannot send or mark receipt.
    now = Date.parse("2026-01-15T16:00:00Z");
    await jobs.schedule();
    await jobs.drain();
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe("https://mail.example.invalid/v1/reminders");
    expect(calls[0]?.body).toBe(
      JSON.stringify({
        to: "alice@example.test",
        title: "Original",
        todo_id: id,
        revision: String(base + 3n),
      }),
    );
    const [final] = await store.client.unsafe(
      "SELECT reminder_revision::text AS revision, reminder_sent_at IS NOT NULL AS sent FROM todo",
    );
    expect(final).toMatchObject({ revision: String(base + 3n), sent: true });
  }
}
