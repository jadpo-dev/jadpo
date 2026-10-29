// Private fixture Worker for the shared contract under workerd. Never imported
// by the production server. It runs src/worker.ts's own entry, per-request
// Store and Workers options with a fixture clock, statement observation and
// injected issuer/mail answers; adapter/run.py serves SQL fixtures, signing
// and static probes from the Node controller beside it.
import { SignJWT, exportJWK, generateKeyPair } from "jose";
import { Store } from "../src/db/store.ts";
import { Egress } from "../src/egress.ts";
import type { Transport } from "../src/egress.ts";
import { createWorker, workerStoreOptions } from "../src/worker.ts";
import type { ExecutionContext, WorkerEnvironment } from "../src/worker.ts";

let now = 1768478400000;
let statements: string[] = [];
let issuerOut = false;
let hook: { match: string; sql: string } | undefined;
let answers: string[] = [];
let calls: { key: string; body: unknown }[] = [];
let receipts: { accepted_at: string }[] = [];
const accepted = new Map<
  string,
  { body: string; receipt: { accepted_at: string } }
>();
let mutation: string | undefined;
let fixtureUrl = "";
let issuer = "";

function statement(query: string, parameters: readonly unknown[]): string {
  return query.replace(/\$(\d+)/g, (_match, index: string) => {
    const value = parameters[Number(index) - 1];
    if (value === null) return "NULL";
    if (typeof value === "number" || typeof value === "bigint")
      return String(value);
    return (
      "'" +
      (typeof value === "string"
        ? value
        : (JSON.stringify(value) ?? "")
      ).replaceAll("'", "''") +
      "'"
    );
  });
}
// Fixture SQL runs on a connection of its own, outside the observed Store.
async function fixtureSql(sql: string) {
  const store = new Store(fixtureUrl, undefined, workerStoreOptions);
  try {
    await store.client.unsafe(sql);
  } finally {
    await store.close();
  }
}
type Query = PromiseLike<unknown> & {
  values(): Query;
  cancel(): void;
  catch(reject: (error: unknown) => unknown): Promise<unknown>;
  finally(done: () => void): Promise<unknown>;
};
type Unsafe = (text: string, parameters?: unknown, options?: unknown) => Query;
// The Node controller runs a concurrent edit synchronously before the matched
// UPDATE is sent; here the UPDATE waits for the same edit to commit.
function hooked(client: { unsafe: Unsafe }) {
  const original = client.unsafe.bind(client);
  client.unsafe = (text, parameters, options) => {
    if (!(hook && /^update "todo"/i.test(text) && text.includes(hook.match)))
      return original(text, parameters, options);
    const edit = hook.sql;
    hook = undefined;
    let values = false;
    let cancelled = false;
    let inner: Query | undefined;
    let started: Promise<unknown> | undefined;
    const run = () =>
      (started ??= fixtureSql(edit).then(() => {
        inner = original(text, parameters, options);
        if (values) inner = inner.values();
        if (cancelled) inner.cancel();
        return inner;
      }));
    const query: Query = {
      values() {
        values = true;
        return query;
      },
      cancel() {
        cancelled = true;
        inner?.cancel();
      },
      then(resolve, reject) {
        return run().then(resolve, reject);
      },
      catch(reject) {
        return run().catch(reject);
      },
      finally(done) {
        return run().finally(done);
      },
    };
    return query;
  };
}
function observedStore(url: string): Store {
  const store = new Store(
    url,
    (query, parameters) => {
      statements.push(statement(query, parameters));
    },
    workerStoreOptions,
  );
  for (const client of [store.client, ...store.transactions.clients])
    hooked(client as unknown as { unsafe: Unsafe });
  return store;
}

// Workers refuse random values at module scope, so the issuer key is made
// on the first command.
let keys:
  | {
      pair: Awaited<ReturnType<typeof generateKeyPair>>;
      publicKey: Record<string, unknown>;
    }
  | undefined;
async function issuerKeys() {
  if (keys) return keys;
  const pair = await generateKeyPair("EdDSA");
  keys = {
    pair,
    publicKey: {
      ...(await exportJWK(pair.publicKey)),
      kid: "fixture-ed25519",
      alg: "EdDSA",
      use: "sig",
    },
  };
  return keys;
}
const transport: Transport = async (input) => {
  if (
    input.url.startsWith("https://identity.example/") ||
    input.url.startsWith("https://keys.example/")
  ) {
    if (issuerOut) throw new Error("fixture dependency unavailable");
    if (input.url.endsWith("/.well-known/openid-configuration"))
      return {
        status: 200,
        body: JSON.stringify({ issuer, jwks_uri: "https://keys.example/jwks" }),
      };
    if (input.url === "https://keys.example/jwks")
      return {
        status: 200,
        body: JSON.stringify({ keys: [(await issuerKeys()).publicKey] }),
      };
  }
  if (input.url !== "https://mail.example.invalid/v1/reminders")
    throw new Error("undeclared fixture egress");
  const key = input.headers["idempotency-key"];
  if (!key || !input.body) throw new Error("missing keyed mail");
  const payload: unknown = JSON.parse(input.body);
  calls.push({ key, body: payload });
  if (mutation) {
    const sql = mutation;
    mutation = undefined;
    await fixtureSql(sql);
  }
  const answer = answers.shift() ?? "accepted";
  if (answer === "unauthorized")
    return { status: 401, body: "private-provider-secret" };
  if (answer === "unavailable")
    return { status: 503, body: "private-provider-secret" };
  let receipt = accepted.get(key);
  if (receipt && receipt.body !== input.body)
    return { status: 409, body: "private-provider-secret" };
  if (!receipt) {
    receipt = {
      body: input.body,
      receipt: { accepted_at: new Date(now).toISOString() },
    };
    accepted.set(key, receipt);
    receipts.push(receipt.receipt);
  }
  if (answer === "lost") throw new Error("fixture acknowledgement lost");
  return { status: 200, body: JSON.stringify(receipt.receipt) };
};
const logs: unknown[] = [];
const worker = createWorker({
  clock: () => now,
  egress: new Egress(transport),
  logger: (event) => {
    logs.push(event);
  },
  store: observedStore,
});

async function request(
  command: Record<string, unknown>,
  env: WorkerEnvironment,
  ctx: ExecutionContext,
) {
  const body = typeof command.body === "string" ? command.body : "";
  const headers = new Headers(command.headers as [string, string][]);
  // As workerd's socket does for every caller in the benchmark.
  headers.set("cf-connecting-ip", "127.0.0.1");
  const response = await worker.fetch(
    new Request("http://127.0.0.1" + String(command.path), {
      method: String(command.method),
      headers,
      ...(body ? { body } : {}),
    }),
    env,
    ctx,
  );
  const pairs: [string, string][] = [];
  response.headers.forEach((value, name) => {
    if (name !== "set-cookie") pairs.push([name, value]);
  });
  for (const cookie of response.headers.getSetCookie())
    pairs.push(["set-cookie", cookie]);
  return {
    status: response.status,
    headers: pairs,
    body: await response.text(),
  };
}
async function dispatch(
  command: Record<string, unknown>,
  env: WorkerEnvironment,
  ctx: ExecutionContext,
): Promise<unknown> {
  const jobs = (
    body: (parts: ReturnType<typeof worker.build>) => Promise<unknown>,
  ) => worker.withStore(env, ctx, body);
  switch (command.op) {
    case "clock":
      now = Number(command.at);
      return {};
    case "worker_reset": {
      const { application } = worker.build(env);
      application.auth.clearKeys();
      application.limits.reset();
      hook = undefined;
      return {};
    }
    case "jwt": {
      const { pair } = await issuerKeys();
      return {
        token: await new SignJWT({ email: "attacker@example.test" })
          .setProtectedHeader({ alg: "EdDSA", kid: "fixture-ed25519" })
          .setSubject(String(command.subject))
          .setIssuer(issuer)
          .setAudience(worker.build(env).config.audience)
          .setIssuedAt(Math.floor(now / 1000))
          .setExpirationTime(Math.floor(now / 1000) + 300)
          .sign(pair.privateKey),
      };
    }
    case "request":
      return await request(command, env, ctx);
    case "tick":
      await jobs(({ jobs }) => jobs.tick());
      return {};
    case "concurrent_tick":
      await jobs(({ jobs }) => Promise.all([jobs.tick(), jobs.tick()]));
      return {};
    case "schedule":
      return {
        items: await jobs(({ jobs }) => jobs.schedule(String(command.name))),
      };
    case "drain":
      await jobs(({ jobs }) => jobs.drain());
      return {};
    case "sweep":
      await jobs(({ jobs }) => jobs.sweep());
      return {};
    case "deliver_one":
      return { delivered: await jobs(({ jobs }) => jobs.deliverOne()) };
    case "mail":
      if (command.clear) {
        calls = [];
        receipts = [];
        accepted.clear();
        answers = [];
        mutation = undefined;
      }
      if (command.answers) answers = [...(command.answers as string[])];
      if (typeof command.mutate === "string") mutation = command.mutate;
      return { calls, accepted: accepted.size, receipts };
    case "mutation_hook":
      hook = { match: String(command.match), sql: String(command.sql) };
      return {};
    case "outage":
      if (command.issuer !== undefined) {
        issuerOut = Boolean(command.issuer);
        worker.build(env).application.auth.clearKeys();
      }
      return {};
    case "jwks":
      worker.build(env).application.auth.clearKeys();
      return {};
    default:
      throw Error(
        "unsupported private workerd controller operation: " +
          String(command.op),
      );
  }
}
export default {
  async fetch(
    incoming: Request,
    env: WorkerEnvironment,
    ctx: ExecutionContext,
  ): Promise<Response> {
    const { config } = worker.build(env);
    fixtureUrl = config.databaseUrl;
    issuer = config.issuer;
    statements = [];
    try {
      const command = (await incoming.json()) as Record<string, unknown>;
      const result = await dispatch(command, env, ctx);
      return Response.json({
        result,
        statements,
        transaction_begins: statements.filter((s) =>
          s.toUpperCase().startsWith("BEGIN"),
        ).length,
      });
    } catch (error) {
      return Response.json({
        error: error instanceof Error ? error.message : "controller failure",
        statements,
      });
    }
  },
};
