// Cloudflare Workers entry: the same Hono application behind the Workers
// fetch and scheduled handlers. A Worker can't reuse a connection another
// request opened, so each request gets its own Store over the Hyperdrive
// binding (Cloudflare's pattern for postgres-js), closed once it has answered;
// configuration, the application, its limits and its key cache last the
// isolate.
import { AsyncLocalStorage } from "node:async_hooks";
import { loadConfig, ConfigError } from "./config.ts";
import type { Config } from "./config.ts";
import { Store } from "./db/store.ts";
import type { StoreOptions } from "./db/store.ts";
import { createApplication } from "./app.ts";
import { Egress, fetchTransport } from "./egress.ts";
import { Jobs } from "./jobs.ts";
import {
  AppError,
  newRequestId,
  publicFailure,
  safeLogRecord,
} from "./errors.ts";
import type { Logger } from "./errors.ts";
import type { Clock } from "./runtime.ts";

export type Hyperdrive = { readonly connectionString: string };
export type WorkerEnvironment = Readonly<Record<string, unknown>>;
export type ExecutionContext = {
  waitUntil(promise: Promise<unknown>): void;
};

export const workerStoreOptions: StoreOptions = {
  connections: 2,
  transactionClients: 2,
  prepare: false,
  fetchTypes: false,
};

// Workerd writes console.error lines as they are; process.stderr is prefixed.
export const workerLog: Logger = (event) => {
  console.error(JSON.stringify(safeLogRecord(event)));
};

function settings(env: WorkerEnvironment): Record<string, string | undefined> {
  const values: Record<string, string | undefined> = {};
  for (const [name, value] of Object.entries(env)) {
    if (typeof value === "string") values[name] = value;
    else if (
      name === "DATABASE_URL" &&
      value !== null &&
      typeof value === "object" &&
      "connectionString" in value &&
      typeof value.connectionString === "string"
    )
      values[name] = value.connectionString;
  }
  return values;
}

export function createWorker(
  options: {
    clock?: Clock;
    egress?: Egress;
    logger?: Logger;
    store?: (url: string) => Store;
  } = {},
) {
  const clock = options.clock ?? Date.now;
  const logger = options.logger ?? workerLog;
  const egress = options.egress ?? new Egress(fetchTransport);
  const open =
    options.store ??
    ((url: string) => new Store(url, undefined, workerStoreOptions));
  const current = new AsyncLocalStorage<Store>();
  // Everything built once reaches the request's own Store through this.
  const store = new Proxy({} as Store, {
    get(_target, property) {
      const active = current.getStore();
      if (!active) throw new AppError("unavailable");
      const value: unknown = Reflect.get(active, property, active);
      return typeof value === "function"
        ? (value as (...args: unknown[]) => unknown).bind(active)
        : value;
    },
  });
  let built:
    | {
        config: Config;
        application: ReturnType<typeof createApplication>;
        jobs: Jobs;
      }
    | undefined;
  function build(env: WorkerEnvironment) {
    if (built) return built;
    const config = loadConfig(settings(env));
    const application = createApplication({
      config,
      store,
      clock,
      egress,
      logger,
    });
    const jobs = new Jobs({ config, store, clock, egress, logger });
    built = { config, application, jobs };
    return built;
  }
  async function withStore<T>(
    env: WorkerEnvironment,
    ctx: ExecutionContext,
    body: (parts: NonNullable<typeof built>) => Promise<T>,
  ): Promise<T> {
    const parts = build(env);
    const request = open(parts.config.databaseUrl);
    try {
      return await current.run(request, () => body(parts));
    } finally {
      ctx.waitUntil(request.close());
    }
  }
  function refused(error: unknown): Response {
    const requestId = newRequestId();
    if (error instanceof ConfigError)
      console.error(
        JSON.stringify({
          event: "configuration_refused",
          problems: error.problems,
        }),
      );
    else
      logger({
        event: "configuration_refused",
        request_id: requestId,
        code: "unavailable",
      });
    const safe = publicFailure(new AppError("unavailable"), requestId);
    return Response.json(safe.body, {
      status: safe.status,
      headers: { "x-request-id": requestId },
    });
  }
  return {
    withStore,
    build,
    fetch: async (
      request: Request,
      env: WorkerEnvironment,
      ctx: ExecutionContext,
    ): Promise<Response> => {
      try {
        build(env);
      } catch (error) {
        return refused(error);
      }
      // Cloudflare sets CF-Connecting-IP to the caller's address.
      const peer = request.headers.get("cf-connecting-ip") ?? "unknown";
      return await withStore(env, ctx, ({ application }) =>
        Promise.resolve(application.app.fetch(request, { peer })),
      );
    },
    scheduled: async (
      _controller: unknown,
      env: WorkerEnvironment,
      ctx: ExecutionContext,
    ): Promise<void> => {
      await withStore(env, ctx, ({ jobs }) => jobs.tick());
    },
  };
}

const worker = createWorker();
export default { fetch: worker.fetch, scheduled: worker.scheduled };
