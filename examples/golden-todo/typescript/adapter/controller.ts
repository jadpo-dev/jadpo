// Private JSONL fixture process. Never imported by the production server.
import { createInterface } from "node:readline";
import { createServer } from "node:net";
import { request as rawRequest } from "node:http";
import { execFileSync, spawnSync, spawn } from "node:child_process";
import { writeFileSync, mkdirSync, unlinkSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { bind } from "../src/host.ts";
import { SignJWT, exportJWK, generateKeyPair } from "jose";
import ts from "typescript";
import { loadConfig } from "../src/config.ts";
import { Store, databaseCode } from "../src/db/store.ts";
import { createApplication, apiDocument } from "../src/app.ts";
import { digest, sign, open } from "../src/auth.ts";
import { Egress } from "../src/egress.ts";
import type { Transport } from "../src/egress.ts";
import { Jobs } from "../src/jobs.ts";

const here = resolve(fileURLToPath(new URL("..", import.meta.url)));
const environment = { ...process.env };
const config = loadConfig(environment);
let now = 1768478400000;
let statements: string[] = [];
let databaseOut = false;
let issuerOut = false;
let hook: { match: string; sql: string } | undefined;
const psql = environment.GOLDEN_PSQL ?? "";
if (!psql)
  throw Error("Private controller requires the resolved local psql tool");
function text(value: unknown): string {
  if (typeof value !== "string") throw Error("Expected fixture text");
  return value;
}
function statement(query: string, parameters: readonly unknown[]): string {
  return query.replace(/\$(\d+)/g, (match, index: string) => {
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
const store = new Store(config.databaseUrl, (query, parameters) => {
  statements.push(statement(query, parameters));
  if (hook && /^update "todo"/i.test(query) && query.includes(hook.match)) {
    const mutation = hook;
    hook = undefined;
    execFileSync(
      psql,
      [
        "-q",
        "-v",
        "ON_ERROR_STOP=1",
        "-d",
        config.databaseUrl,
        "-c",
        mutation.sql,
      ],
      { stdio: "ignore" },
    );
  }
});
const pair = await generateKeyPair("EdDSA");
const publicKey = {
  ...(await exportJWK(pair.publicKey)),
  kid: "fixture-ed25519",
  alg: "EdDSA",
  use: "sig",
};
let answers: string[] = [];
let calls: { key: string; body: unknown }[] = [];
let receipts: { accepted_at: string }[] = [];
const accepted = new Map<
  string,
  { body: string; receipt: { accepted_at: string } }
>();
let mutation: string | undefined;
const transport: Transport = async (input, _scope) => {
  if (
    input.url.startsWith("https://identity.example/") ||
    input.url.startsWith("https://keys.example/")
  ) {
    if (issuerOut) throw new Error("fixture dependency unavailable");
    if (input.url.endsWith("/.well-known/openid-configuration"))
      return {
        status: 200,
        body: JSON.stringify({
          issuer: config.issuer,
          jwks_uri: "https://keys.example/jwks",
        }),
      };
    if (input.url === "https://keys.example/jwks")
      return { status: 200, body: JSON.stringify({ keys: [publicKey] }) };
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
    await store.client.unsafe(sql);
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
const egress = new Egress(transport);
const logs: unknown[] = [];
const { app, auth, limits } = createApplication({
  config,
  store,
  clock: () => now,
  egress,
  logger: (event) => {
    logs.push(event);
  },
});
// Outages are injected outside the application, as a transport/dependency fault.
const server = await bind((request, peer) => app.fetch(request, peer), 0);
const port = server.port;
async function http(command: Record<string, unknown>) {
  const headers = command.headers as [string, string][];
  return await new Promise<{
    status: number;
    headers: [string, string][];
    body: string;
  }>((resolve, reject) => {
    const request = rawRequest(
      {
        hostname: "127.0.0.1",
        port,
        path: String(command.path),
        method: String(command.method),
        headers: [
          ...headers,
          ["host", `127.0.0.1:${port}`],
          [
            "content-length",
            String(Buffer.byteLength(text(command.body ?? ""))),
          ],
        ].flat(),
      },
      (response) => {
        const chunks: Buffer[] = [];
        response.on("data", (chunk: Buffer) => {
          chunks.push(chunk);
        });
        response.on("error", reject);
        response.on("end", () => {
          const pairs: [string, string][] = [];
          for (let i = 0; i < response.rawHeaders.length; i += 2)
            pairs.push([
              response.rawHeaders[i] ?? "",
              response.rawHeaders[i + 1] ?? "",
            ]);
          resolve({
            status: response.statusCode ?? 0,
            headers: pairs,
            body: Buffer.concat(chunks).toString("utf8"),
          });
        });
      },
    );
    request.on("error", reject);
    request.end(text(command.body ?? ""));
  });
}
function policyProbe() {
  mkdirSync(resolve(here, "build"), { recursive: true });
  const path = resolve(here, "build/owner-probe.ts");
  writeFileSync(
    path,
    "import type { TodoService } from '../src/todos.ts';\nimport type { Principal } from '../src/schemas.ts';\nimport type { RequestScope } from '../src/runtime.ts';\ndeclare const service:TodoService, principal:Principal, scope:RequestScope;\nvoid service.patch(principal,crypto.randomUUID(),{owner_id:crypto.randomUUID()},scope);\n",
  );
  try {
    const configuration = ts.readConfigFile(
      resolve(here, "tsconfig.json"),
      (path) => ts.sys.readFile(path),
    );
    const parsed = ts.parseJsonConfigFileContent(
      configuration.config,
      ts.sys,
      here,
    );
    const program = ts.createProgram([path], parsed.options);
    const diagnostics = ts.getPreEmitDiagnostics(program);
    const rejected = diagnostics.some(
      (d) =>
        d.code === 2353 &&
        ts
          .flattenDiagnosticMessageText(d.messageText, " ")
          .includes("owner_id"),
    );
    return {
      releaseEquivalentSuccess: !rejected,
      disposition: rejected ? "requires_human_decision" : "incomplete",
      rule: rejected ? "type.field_update_forbidden" : "probe_failed",
      diagnostics: diagnostics.map((d) => ({
        code: d.code,
        message: ts.flattenDiagnosticMessageText(d.messageText, " "),
      })),
    };
  } finally {
    unlinkSync(path);
  }
}
let startupVerified = false;
async function positiveStartup(): Promise<void> {
  if (startupVerified) return;
  const [queued] = await store.client.unsafe(
    "SELECT (SELECT count(*) FROM todo)+(SELECT count(*) FROM jadpo_deliveries) AS work",
  );
  if (Number(queued?.work) !== 0)
    throw Error("Positive startup requires an empty private work fixture");
  const reservation = createServer();
  await new Promise<void>((resolve) => {
    reservation.listen(0, "127.0.0.1", resolve);
  });
  const address = reservation.address();
  if (!address || typeof address === "string")
    throw Error("Positive startup port unavailable");
  const port = address.port;
  await new Promise<void>((resolve) => {
    reservation.close(() => {
      resolve();
    });
  });
  const child = spawn(process.execPath, [resolve(here, "src/server.ts")], {
    env: { ...environment, PORT: String(port) },
    stdio: "ignore",
  });
  const exited = new Promise<void>((resolve) => {
    child.once("exit", () => {
      resolve();
    });
  });
  try {
    const until = performance.now() + 10000;
    while (performance.now() < until && child.exitCode === null) {
      try {
        const healthy = await new Promise<boolean>((resolve) => {
          const outgoing = rawRequest(
            { hostname: "127.0.0.1", port, path: "/health/live", timeout: 200 },
            (incoming) => {
              const chunks: Buffer[] = [];
              incoming.on("data", (chunk: Buffer) => {
                chunks.push(chunk);
              });
              incoming.on("end", () => {
                resolve(
                  incoming.statusCode === 200 &&
                    Buffer.concat(chunks).toString("utf8") ===
                      '{"status":"live"}',
                );
              });
              incoming.on("error", () => {
                resolve(false);
              });
            },
          );
          outgoing.on("timeout", () => {
            outgoing.destroy();
            resolve(false);
          });
          outgoing.on("error", () => {
            resolve(false);
          });
          outgoing.end();
        });
        if (healthy) {
          startupVerified = true;
          return;
        }
      } catch {
        /* Child may still be importing its pinned dependencies. */
      }
      await new Promise<void>((resolve) => {
        setTimeout(resolve, 50);
      });
    }
    throw Error("Positive control: production server did not serve liveness");
  } finally {
    if (child.exitCode === null) child.kill("SIGTERM");
    const timeout = setTimeout(() => {
      if (child.exitCode === null) child.kill("SIGKILL");
    }, 3000);
    try {
      await exited;
    } finally {
      clearTimeout(timeout);
    }
  }
}
async function preflight(command: Record<string, unknown>) {
  await positiveStartup();
  const changes = command.environment as Record<string, string | null>;
  const env: Record<string, string | undefined> = { ...environment, PORT: "0" };
  for (const [key, value] of Object.entries(changes))
    if (value === null) delete env[key];
    else env[key] = value;
  const result = spawnSync(process.execPath, [resolve(here, "src/server.ts")], {
    env,
    encoding: "utf8",
    timeout: 10000,
  });
  const output = result.stdout + result.stderr;
  const diagnostics = output.split("\n").flatMap((line) => {
    try {
      return [
        JSON.parse(line) as {
          problems?: { variable: string; problem: string }[];
        },
      ];
    } catch {
      return [];
    }
  });
  const problems = diagnostics.flatMap((item) => item.problems ?? []);
  return {
    refused: result.status !== null && result.status !== 0,
    handlerRan: result.error?.message.includes("ETIMEDOUT") ?? false,
    declarations: problems.map((item) => item.variable),
    diagnosticCodes: problems.map((item) => item.problem),
    safe: ![
      config.mailApiKey,
      config.signingKey,
      "password@",
      ...Object.entries(changes)
        .filter(([key]) => key === "DATABASE_URL")
        .map(([, v]) => v ?? "")
        .filter(Boolean),
    ].some((secret) => output.includes(secret)),
  };
}
async function dispatch(command: Record<string, unknown>): Promise<unknown> {
  switch (command.op) {
    case "clock":
      now = Number(command.at);
      return {};
    case "worker_reset":
      auth.clearKeys();
      limits.reset();
      hook = undefined;
      return {};
    case "digest":
      return { digest: digest(String(command.text)) };
    case "sign":
      return { signed: sign(String(command.payload), config.signingKey) };
    case "open":
      return { payload: open(String(command.signed), [config.signingKey]) };
    case "jwt":
      return {
        token: await new SignJWT({ email: "attacker@example.test" })
          .setProtectedHeader({ alg: "EdDSA", kid: publicKey.kid })
          .setSubject(String(command.subject))
          .setIssuer(config.issuer)
          .setAudience(config.audience)
          .setIssuedAt(Math.floor(now / 1000))
          .setExpirationTime(Math.floor(now / 1000) + 300)
          .sign(pair.privateKey),
      };
    case "sql":
      try {
        const result = await store.client.unsafe(String(command.sql)).values();
        const cells: readonly (readonly unknown[])[] = result;
        return {
          rows: cells.map((row) =>
            row.map((value, index) =>
              result.columns[index]?.type === 20 &&
              Number.isSafeInteger(Number(value))
                ? Number(value)
                : value,
            ),
          ),
        };
      } catch (error) {
        return {
          error:
            databaseCode(error) === "23503"
              ? "Conflict: relationship_conflict"
              : "StoreFailure: unavailable",
        };
      }
    case "request":
      return await http(command);
    case "tick":
      await jobs.tick();
      return {};
    case "concurrent_tick":
      await Promise.all([jobs.tick(), jobs.tick()]);
      return {};
    case "schedule":
      return { items: await jobs.schedule(String(command.name)) };
    case "drain":
      await jobs.drain();
      return {};
    case "sweep":
      await jobs.sweep();
      return {};
    case "deliver_one":
      return { delivered: await jobs.deliverOne() };
    case "mail":
      if (command.clear) {
        calls = [];
        receipts = [];
        accepted.clear();
        answers = [];
        mutation = undefined;
      }
      if (command.answers) answers = [...(command.answers as string[])];
      if (command.mutate) mutation = text(command.mutate);
      return { calls, accepted: accepted.size, receipts };
    case "mutation_hook":
      hook = { match: String(command.match), sql: String(command.sql) };
      return {};
    case "outage":
      if (command.database !== undefined) {
        databaseOut = Boolean(command.database);
        execFileSync(
          psql,
          [
            "-q",
            "-v",
            "ON_ERROR_STOP=1",
            "-d",
            String(environment.CONTROL_DATABASE_URL),
            "-c",
            `ALTER DATABASE "${new URL(config.databaseUrl).pathname.slice(1)}" ALLOW_CONNECTIONS ${databaseOut ? "false" : "true"}`,
          ],
          { stdio: "ignore" },
        );
        if (databaseOut)
          execFileSync(
            psql,
            [
              "-q",
              "-v",
              "ON_ERROR_STOP=1",
              "-d",
              String(environment.CONTROL_DATABASE_URL),
              "-c",
              `SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='${new URL(config.databaseUrl).pathname.slice(1)}' AND pid<>pg_backend_pid()`,
            ],
            { stdio: "ignore" },
          );
      }
      if (command.issuer !== undefined) {
        issuerOut = Boolean(command.issuer);
        auth.clearKeys();
      }
      return {};
    case "preflight":
      return await preflight(command);
    case "policy_probe":
      return policyProbe();
    case "api_record":
      return { record: JSON.stringify(apiDocument()) };
    case "jwks":
      auth.clearKeys();
      return {};
    default:
      throw Error(
        "unsupported private controller operation: " + String(command.op),
      );
  }
}
const jobs = new Jobs({
  config,
  store,
  clock: () => now,
  egress,
  logger: (event) => {
    logs.push(event);
  },
});
try {
  for await (const line of createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  })) {
    statements = [];
    try {
      const command = JSON.parse(line) as Record<string, unknown>;
      const result = await dispatch(command);
      process.stdout.write(
        JSON.stringify({
          result,
          statements,
          transaction_begins: statements.filter((s) =>
            s.toUpperCase().startsWith("BEGIN"),
          ).length,
        }) + "\n",
      );
    } catch (error) {
      process.stdout.write(
        JSON.stringify({
          error: error instanceof Error ? error.message : "controller failure",
          statements,
        }) + "\n",
      );
    }
  }
} finally {
  await server.close();
  await store.close();
}
