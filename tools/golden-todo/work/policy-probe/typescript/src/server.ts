import { bind } from "./host.ts";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { loadConfig, ConfigError } from "./config.ts";
import { Store } from "./db/store.ts";
import { createApplication } from "./app.ts";
import { Egress } from "./egress.ts";
import { Jobs } from "./jobs.ts";
import { structuredLog } from "./errors.ts";

export async function start(
  environment: Readonly<Record<string, string | undefined>>,
) {
  // Validate every declaration before allocating resources or listening.
  const config = loadConfig(environment);
  const store = new Store(config.databaseUrl);
  const egress = new Egress();
  const { app } = createApplication({ config, store, egress });
  const jobs = new Jobs({
    config,
    store,
    clock: Date.now,
    egress,
    logger: structuredLog,
  });
  const port = environment.PORT === undefined ? 3000 : Number(environment.PORT);
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    await store.close();
    throw new ConfigError([{ variable: "PORT", problem: "setting_invalid" }]);
  }
  const server = await bind(
    (request, peer) => app.fetch(request, peer),
    port,
    environment.HOST ?? "127.0.0.1",
  );
  let running: Promise<void> | undefined;
  const interval = setInterval(() => {
    if (running) return;
    running = jobs
      .tick()
      .catch(() => {
        structuredLog({
          event: "job_failed",
          request_id: crypto.randomUUID(),
          code: "unavailable",
        });
      })
      .finally(() => {
        running = undefined;
      });
  }, 1000);
  interval.unref();
  return {
    server,
    async close() {
      clearInterval(interval);
      await server.close();
      await running;
      await store.close();
    },
  };
}
if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
) {
  try {
    const application = await start(process.env);
    const stop = () => {
      void application.close().then(
        () => {
          process.exitCode = 0;
        },
        () => {
          process.exitCode = 1;
        },
      );
    };
    process.once("SIGTERM", stop);
    process.once("SIGINT", stop);
  } catch (error) {
    if (error instanceof ConfigError)
      process.stderr.write(
        JSON.stringify({
          event: "configuration_refused",
          problems: error.problems,
        }) + "\n",
      );
    else
      structuredLog({
        event: "configuration_refused",
        request_id: crypto.randomUUID(),
        code: "unavailable",
      });
    process.exitCode = 1;
  }
}
