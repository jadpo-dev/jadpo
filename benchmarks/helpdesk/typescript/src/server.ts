// The production entry: checks its settings, serves HTTP on PORT (3000) and
// ticks the schedules once a minute.
import { serve } from "@hono/node-server";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { ConfigError, loadConfig } from "./config.ts";
import { Store } from "./db/store.ts";
import { createApplication } from "./app.ts";
import { Jobs } from "./jobs.ts";
import { structuredLog } from "./errors.ts";

export async function start(
  environment: Readonly<Record<string, string | undefined>>,
) {
  const config = loadConfig(environment);
  const port = environment.PORT === undefined ? 3000 : Number(environment.PORT);
  if (!Number.isInteger(port) || port < 0 || port > 65535)
    throw new ConfigError([{ variable: "PORT", problem: "setting_invalid" }]);
  const store = new Store(config.databaseUrl);
  const { app, desk } = createApplication({ config, store });
  const jobs = new Jobs(desk, Date.now, structuredLog);
  const server = serve({
    fetch: app.fetch,
    port,
    hostname: environment.HOST ?? "127.0.0.1",
  });
  await new Promise<void>((done, failed) => {
    server.once("listening", done);
    server.once("error", failed);
  });
  let running: Promise<unknown> | undefined;
  const interval = setInterval(() => {
    running ??= jobs.tick().finally(() => {
      running = undefined;
    });
  }, 60000);
  interval.unref();
  return {
    async close() {
      clearInterval(interval);
      await new Promise<void>((done) => server.close(() => done()));
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
      void application.close();
    };
    process.once("SIGTERM", stop);
    process.once("SIGINT", stop);
  } catch (error) {
    process.stderr.write(
      JSON.stringify({
        event: "configuration_refused",
        problems: error instanceof ConfigError ? error.problems : [],
      }) + "\n",
    );
    process.exitCode = 1;
  }
}
