// Private JSONL fixture process the hidden tests drive: one command per
// stdin line, one answer per stdout line. Never imported by the app.
import { createInterface } from "node:readline";
import { request as rawRequest } from "node:http";
import { execFileSync } from "node:child_process";
import { serve } from "@hono/node-server";
import { loadConfig } from "../src/config.ts";
import { Store, databaseCode } from "../src/db/store.ts";
import { createApplication } from "../src/app.ts";
import { digest, open, sign } from "../src/auth.ts";
import { Jobs } from "../src/jobs.ts";

const environment = { ...process.env };
const config = loadConfig(environment);
const psql = environment.GOLDEN_PSQL ?? "";
if (!psql) throw Error("The controller needs the local psql tool");
let now = 1768478400000;
const clock = () => now;
let statements: string[] = [];

function text(value: unknown): string {
  if (typeof value !== "string") throw Error("Expected fixture text");
  return value;
}
function statement(query: string, parameters: readonly unknown[]): string {
  return query.replace(/\$(\d+)/g, (_match, index: string) => {
    const value = parameters[Number(index) - 1];
    if (value === null) return "NULL";
    if (typeof value === "number" || typeof value === "bigint")
      return String(value);
    const shown =
      typeof value === "string"
        ? value
        : value instanceof Date
          ? value.toISOString()
          : (JSON.stringify(value) ?? "");
    return "'" + shown.replaceAll("'", "''") + "'";
  });
}
const store = new Store(config.databaseUrl, (query, parameters) => {
  statements.push(statement(query, parameters));
});

// Every line the application writes joins the logs: stdout is the protocol.
const logs: unknown[] = [];
for (const name of ["log", "info", "warn", "error", "debug"] as const) {
  console[name] = (...parts: unknown[]) => {
    logs.push(
      parts
        .map((part) => (typeof part === "string" ? part : JSON.stringify(part)))
        .join(" "),
    );
  };
}
const logger = (event: unknown) => {
  logs.push(event);
};
const { app, desk } = createApplication({ config, store, clock, logger });
const jobs = new Jobs(desk, clock, logger);

const server = serve({ fetch: app.fetch, port: 0, hostname: "127.0.0.1" });
await new Promise<void>((resolve, reject) => {
  server.once("listening", resolve);
  server.once("error", reject);
});
const address = server.address();
if (!address || typeof address === "string") throw Error("No listener");
const port = address.port;

async function http(command: Record<string, unknown>) {
  const headers = command.headers as [string, string][];
  const body = text(command.body ?? "");
  return await new Promise<{
    status: number;
    headers: [string, string][];
    body: string;
  }>((resolve, reject) => {
    const outgoing = rawRequest(
      {
        hostname: "127.0.0.1",
        port,
        path: String(command.path),
        method: String(command.method),
        headers: [
          ...headers,
          ["host", `127.0.0.1:${port}`],
          ["content-length", String(Buffer.byteLength(body))],
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
    outgoing.on("error", reject);
    outgoing.end(body);
  });
}

function control(sql: string) {
  execFileSync(
    psql,
    [
      "-q",
      "-v",
      "ON_ERROR_STOP=1",
      "-d",
      String(environment.CONTROL_DATABASE_URL),
      "-c",
      sql,
    ],
    { stdio: "ignore" },
  );
}
const databaseName = new URL(config.databaseUrl).pathname.slice(1);

async function dispatch(command: Record<string, unknown>): Promise<unknown> {
  switch (command.op) {
    case "clock":
      now = Number(command.at);
      return {};
    case "worker_reset":
      jobs.reset();
      return {};
    case "logs":
      return { logs: logs.splice(0) };
    case "digest":
      return { digest: digest(String(command.text)) };
    case "sign":
      return { signed: sign(String(command.payload), config.signingKey) };
    case "open":
      return { payload: open(String(command.signed), [config.signingKey]) };
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
                : value instanceof Date
                  ? value.toISOString()
                  : value,
            ),
          ),
        };
      } catch (error) {
        return {
          error:
            databaseCode(error) === "23503"
              ? "Conflict: relationship_conflict"
              : databaseCode(error) === "23505"
                ? "Conflict: unique_violation"
                : "StoreFailure: unavailable",
        };
      }
    case "request":
      return await http(command);
    case "tick":
      return { ran: await jobs.tick() };
    case "drain":
      // The desk emits no events: nothing waits to be delivered.
      return {};
    case "outage":
      // A database outage is injected outside the application.
      if (command.database !== undefined) {
        const out = Boolean(command.database);
        control(
          `ALTER DATABASE "${databaseName}" ALLOW_CONNECTIONS ${out ? "false" : "true"}`,
        );
        if (out)
          control(
            `SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='${databaseName}' AND pid<>pg_backend_pid()`,
          );
      }
      return {};
    default:
      throw Error("unsupported controller operation: " + String(command.op));
  }
}

try {
  for await (const line of createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  })) {
    statements = [];
    try {
      const result = await dispatch(
        JSON.parse(line) as Record<string, unknown>,
      );
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
  await new Promise<void>((resolve) => server.close(() => resolve()));
  await store.close();
}
