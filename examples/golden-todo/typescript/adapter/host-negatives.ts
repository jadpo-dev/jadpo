// End-to-end client disconnect controls, invoked separately on Node and Bun.
import assert from "node:assert/strict";
import { request } from "node:http";
import { setTimeout as pause } from "node:timers/promises";
import { Store } from "../src/db/store.ts";
import { loadConfig } from "../src/config.ts";
import { createApplication } from "../src/app.ts";
import { bind } from "../src/host.ts";
import { digest, sign } from "../src/auth.ts";

const config = loadConfig(process.env);
const store = new Store(config.databaseUrl);
const user = "00000000-0000-4000-8000-000000000001";
const todo = "00000000-0000-4000-8000-000000000011";
const now = Date.now();
await store.client.unsafe(
  `INSERT INTO "user" VALUES ('${user}','alice','active','alice@example.test')`,
);
await store.client.unsafe(
  `INSERT INTO todo(id,owner_id,title,status,reminder_revision,created_at,updated_at) VALUES ('${todo}','${user}','Original','open',1,now(),now())`,
);
const secret = "jadpo_token_" + "1".repeat(64);
const claims = {
  enc: 1,
  m: "api_token",
  r: crypto.randomUUID().replaceAll("-", ""),
  v: digest(secret),
  p: user.replaceAll("-", ""),
  s: "talice",
  e: now + 300000,
};
const token = secret + "." + sign(JSON.stringify(claims), config.signingKey);
const { app } = createApplication({
  config,
  store,
  clock: () => now,
  logger: () => {},
});
const server = await bind((request, peer) => app.fetch(request, peer), 0);
const lock = await store.client.reserve();
let locked = false;
try {
  await lock.unsafe("BEGIN");
  locked = true;
  await lock.unsafe(`SELECT id FROM todo WHERE id='${todo}' FOR UPDATE`);
  const body = JSON.stringify({ title: "Cancelled" });
  const outgoing = request(
    {
      hostname: "127.0.0.1",
      port: server.port,
      path: "/todos/" + todo,
      method: "PATCH",
      headers: {
        authorization: "Bearer " + token,
        "content-type": "application/json",
        "content-length": Buffer.byteLength(body),
        "idempotency-key": "disconnect",
      },
    },
    (incoming) => {
      incoming.resume();
    },
  );
  outgoing.on("error", () => {});
  outgoing.end(body);
  const blocked = async () => {
    const rows = await store.client.unsafe(
      "SELECT count(*) AS n FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'update \"todo\"%'",
    );
    return Number(rows[0]?.n) > 0;
  };
  const until = performance.now() + 3000;
  while (!(await blocked()) && performance.now() < until) await pause(10);
  assert(
    await blocked(),
    "positive control: PATCH must be waiting for a real row lock",
  );
  outgoing.destroy();
  const cancelledBy = performance.now() + 2000;
  while ((await blocked()) && performance.now() < cancelledBy) await pause(10);
  assert(
    !(await blocked()),
    "client disconnect must cancel the active PostgreSQL query",
  );
  await lock.unsafe("ROLLBACK");
  locked = false;
  await pause(50);
  assert.equal(
    (await store.client.unsafe("SELECT title FROM todo"))[0]?.title,
    "Original",
  );
  assert.equal(
    Number(
      (
        await store.client.unsafe(
          "SELECT count(*) AS n FROM todo_request_receipts WHERE response_status IS NOT NULL",
        )
      )[0]?.n,
    ),
    0,
  );
  const send = async (
    json: string,
    key: string,
    method = "POST",
    path = "/todos",
  ) =>
    await new Promise<{ status: number; body: string }>((resolve, reject) => {
      const outgoing = request(
        {
          hostname: "127.0.0.1",
          port: server.port,
          path,
          method,
          headers: {
            authorization: "Bearer " + token,
            "content-type": "application/json",
            "content-length": Buffer.byteLength(json),
            "idempotency-key": key,
          },
        },
        (incoming) => {
          const chunks: Buffer[] = [];
          incoming.on("data", (chunk: Buffer) => {
            chunks.push(chunk);
          });
          incoming.on("end", () => {
            resolve({
              status: incoming.statusCode ?? 0,
              body: Buffer.concat(chunks).toString("utf8"),
            });
          });
          incoming.on("error", reject);
        },
      );
      outgoing.on("error", reject);
      outgoing.end(json);
    });
  const create = (due: string | null | undefined, key = "native-replay") =>
    send(
      JSON.stringify({
        title: "e\u0301".repeat(200),
        ...(due !== undefined ? { due_at: due } : {}),
      }),
      key,
    );
  const first = await create(undefined);
  const replay = await create(null);
  assert.equal(first.status, 201);
  assert.deepEqual(replay, first);
  assert.equal(
    (JSON.parse(first.body) as { title: string }).title,
    "é".repeat(200),
  );
  const offset = await create(
    "2026-12-01T13:00:00.100+01:00",
    "native-instant",
  );
  const utc = await create("2026-12-01T12:00:00.1Z", "native-instant");
  assert.equal(offset.status, 201);
  assert.deepEqual(utc, offset);
  assert.equal(
    (JSON.parse(utc.body) as { due_at: string }).due_at,
    "2026-12-01T12:00:00.100Z",
  );
  const target = "/todos/" + (JSON.parse(utc.body) as { id: string }).id;
  const omittedPatch = await send("{}", "native-patch", "PATCH", target);
  const nullPatch = await send(
    '{"due_at":null}',
    "native-patch",
    "PATCH",
    target,
  );
  assert.equal(omittedPatch.status, 200);
  assert.equal(nullPatch.status, 422);
  assert.equal(
    (JSON.parse(nullPatch.body) as { error: { code: string } }).error.code,
    "idempotency_key_reused",
  );
  process.stdout.write(
    JSON.stringify({
      host: "Bun" in globalThis ? "bun" : "node",
      clientDisconnectRollback: true,
      unicodeAndDurableReplay: true,
    }) + "\n",
  );
} finally {
  if (locked) await lock.unsafe("ROLLBACK");
  lock.release();
  await server.close();
  await store.close();
}
