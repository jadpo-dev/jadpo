import { expect } from "vitest";
import { Jobs } from "../src/jobs.ts";
import { Egress } from "../src/egress.ts";
import { loadConfig } from "../src/config.ts";
import type { Store } from "../src/db/store.ts";
export async function jobNegatives(store: Store, url: string) {
  const start = 1768478400000;
  let now = start;
  let healthy = false;
  let calls = 0;
  const config = loadConfig({
    DATABASE_URL: url,
    BROWSER_ORIGIN: "https://todo.example.test",
    OIDC_ISSUER: "https://identity.example/tenant/",
    OIDC_AUDIENCE: "golden-todo",
    SESSION_SIGNING_KEY: "x".repeat(43),
    MAIL_API_KEY: "private-provider-secret",
  });
  const egress = new Egress(() => {
    calls++;
    return Promise.resolve(
      healthy
        ? {
            status: 200,
            body: JSON.stringify({ accepted_at: new Date(now).toISOString() }),
          }
        : { status: 503, body: "private-provider-secret" },
    );
  });
  const jobs = new Jobs({
    config,
    store,
    clock: () => now,
    egress,
    logger: () => {},
  });
  const seed = async (amount: number) => {
    await store.client.unsafe(
      "TRUNCATE todo,jadpo_deliveries,todo_provider_breakers",
    );
    for (let i = 0; i < amount; i++)
      await store.client.unsafe(
        `INSERT INTO todo(id,owner_id,title,status,due_at,reminder_revision,created_at,updated_at) VALUES ('${crypto.randomUUID()}','00000000-0000-4000-8000-000000000001','Original','open','2026-01-15T11:00:00Z',1,'2026-01-15T12:00:00Z','2026-01-15T12:00:00Z')`,
      );
  };
  await seed(6);
  await jobs.schedule();
  await jobs.drain();
  expect(calls).toBe(15);
  expect(
    (
      await store.client.unsafe(
        "SELECT count(*) AS n FROM jadpo_deliveries WHERE status='held'",
      )
    )[0]?.n,
  ).toBe("1");
  healthy = true;
  now += 60000;
  await jobs.drain();
  expect(calls).toBe(21);
  expect(
    (
      await store.client.unsafe(
        "SELECT count(*) AS n FROM todo WHERE reminder_sent_at IS NOT NULL",
      )
    )[0]?.n,
  ).toBe("6");
  await seed(1);
  healthy = false;
  calls = 0;
  now = start;
  await jobs.schedule();
  await jobs.drain();
  for (let attempt = 1; attempt < 5; attempt++) {
    now += 3600000;
    await jobs.drain();
  }
  expect(calls).toBe(15);
  const [delivery] = await store.client.unsafe(
    "SELECT status,attempts FROM jadpo_deliveries WHERE subscriber='send_reminder'",
  );
  expect(delivery?.status).toBe("failed");
  expect(delivery?.attempts).toBe(5);
  now += 3600000;
  await jobs.drain();
  expect(calls).toBe(15);
}
