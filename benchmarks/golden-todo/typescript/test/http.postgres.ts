import { expect } from "vitest";
import { bind } from "../src/host.ts";
import { request } from "node:http";
import { createApplication } from "../src/app.ts";
import { loadConfig } from "../src/config.ts";
import { digest, sign } from "../src/auth.ts";
import type { Store } from "../src/db/store.ts";

export async function httpNegatives(store: Store, url: string) {
  const now = 1768478400000;
  const key = "x".repeat(43);
  const id = "00000000000040008000000000000001";
  const secret = "1".repeat(64);
  const token = "jadpo_token_" + secret;
  const claims = {
    enc: 1,
    m: "api_token",
    r: "00000000000040008000000000000010",
    v: digest(token),
    p: id,
    s: "talice",
    e: now + 300000,
  };
  const bearer = token + "." + sign(JSON.stringify(claims), key);
  const config = loadConfig({
    DATABASE_URL: url,
    BROWSER_ORIGIN: "https://todo.example.test",
    OIDC_ISSUER: "https://identity.example/tenant/",
    OIDC_AUDIENCE: "golden-todo",
    SESSION_SIGNING_KEY: key,
    MAIL_API_KEY: "private-provider-secret",
  });
  const { app, admission } = createApplication({
    config,
    store,
    clock: () => now,
    logger: () => {},
  });
  const server = await bind((request, peer) => app.fetch(request, peer), 0);
  const port = server.port;
  const call = async (
    method: string,
    path: string,
    body = "",
    contentType = "application/json",
    authorization = "Bearer " + bearer,
    idempotencyKey?: string,
  ) =>
    await new Promise<{
      status: number;
      body: string;
      headers: Record<string, string | string[] | undefined>;
    }>((resolve, reject) => {
      const outgoing = request(
        {
          hostname: "127.0.0.1",
          port,
          method,
          path,
          headers: {
            authorization,
            ...(idempotencyKey ? { "idempotency-key": idempotencyKey } : {}),
            "content-type": contentType,
            "content-length": Buffer.byteLength(body),
            "x-forwarded-for":
              "192.0.2." + String(Math.floor(Math.random() * 254) + 1),
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
              headers: incoming.headers,
              body: Buffer.concat(chunks).toString("utf8"),
            });
          });
          incoming.on("error", reject);
        },
      );
      outgoing.on("error", reject);
      outgoing.end(body);
    });
  try {
    expect((await call("POST", "/todos", "{")).status).toBe(400);
    expect(
      (await call("POST", "/todos", '{"title":"safe"}', "text/plain")).status,
    ).toBe(400);
    expect((await call("POST", "/todos", "x".repeat(1048577))).status).toBe(
      413,
    );
    expect((await call("GET", "/todos", "unexpected")).status).toBe(422);
    expect((await call("GET", "/todos?unknown=value")).status).toBe(422);
    expect((await call("GET", "/todos?page_size=1&page_size=2")).status).toBe(
      422,
    );
    expect(
      (
        await call(
          "GET",
          "/todos",
          "",
          "application/json",
          "Bearer " + bearer + ", Bearer invalid",
        )
      ).status,
    ).toBe(401);
    expect(
      (await call("POST", "/todos", '{"title":"one","\\u0074itle":"two"}'))
        .status,
    ).toBe(400);
    expect(
      (await call("POST", "/todos", '{"title":"safe","extra":{"a":1,"a":2}}'))
        .status,
    ).toBe(400);
    expect(
      (
        await call(
          "POST",
          "/todos",
          " ".repeat(20000) + '{"title":"Whitespace"}',
        )
      ).status,
    ).toBe(201);
    const valid = await call(
      "POST",
      "/todos",
      JSON.stringify({ title: "e\u0301".repeat(200) }),
      "application/vnd.todo+json",
    );
    expect(valid.status).toBe(201);
    expect((JSON.parse(valid.body) as { title: string }).title).toBe(
      "é".repeat(200),
    );
    const once = await call(
      "POST",
      "/todos",
      '{"title":"Once"}',
      "application/json",
      "Bearer " + bearer,
      "once",
    );
    const replay = await call(
      "POST",
      "/todos",
      '{ "title": "Once", "due_at": null }',
      "application/json",
      "Bearer " + bearer,
      "once",
    );
    expect(once.status).toBe(201);
    expect(replay.status).toBe(201);
    expect(replay.body).toBe(once.body);
    expect(
      (
        await call(
          "POST",
          "/todos",
          '{"title":"Different"}',
          "application/json",
          "Bearer " + bearer,
          "once",
        )
      ).status,
    ).toBe(422);
    const rows = await store.client.unsafe(
      "SELECT count(*) AS amount FROM todo WHERE title='Once'",
    );
    expect(Number(rows[0]?.amount)).toBe(1);
    const item = JSON.parse(once.body) as { id: string };
    expect(
      (
        await call(
          "PATCH",
          "/todos/" + item.id,
          "{}",
          "application/json",
          "Bearer " + bearer,
          "presence",
        )
      ).status,
    ).toBe(200);
    expect(
      (
        await call(
          "PATCH",
          "/todos/" + item.id,
          '{"due_at":null}',
          "application/json",
          "Bearer " + bearer,
          "presence",
        )
      ).status,
    ).toBe(422);
    expect((await call("HEAD", "/todos")).body).toBe("");
    expect((await call("OPTIONS", "/todos")).status).toBe(204);
    expect((await call("POST", "/health/live")).status).toBe(405);
    let refused;
    for (let attempt = 0; attempt < 121; attempt++) {
      refused = await call(
        "GET",
        "/todos",
        "",
        "application/json",
        "Bearer invalid",
      );
      if (refused.status === 429) break;
      expect(refused.status).toBe(401);
    }
    expect(refused?.status).toBe(429);
    expect(Number(refused?.headers["retry-after"])).toBeGreaterThan(0);
    expect(refused?.headers.ratelimit).toBeTruthy();
    const live = await call("GET", "/health/live");
    expect(live.status).toBe(200);
    expect(live.headers["cache-control"]).toBe("no-store");
    expect(live.headers["x-request-id"]).toMatch(/^req_[a-f0-9]{16}$/);
    expect(live.headers.ratelimit).toBeUndefined();
    expect(admission.active).toBe(0);
  } finally {
    await server.close();
  }
}
