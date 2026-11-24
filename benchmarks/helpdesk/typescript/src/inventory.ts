// The routes this app serves, as {method, path} with `{name}` placeholders.
// `npm run inventory` prints them as JSON.
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { routes } from "./routes.ts";

export function inventory() {
  return {
    routes: routes.map((route) => ({
      method: route.method,
      path: route.path,
      status: route.status,
      input: route.body ? "body" : route.query ? "query" : "none",
    })),
  };
}

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
)
  process.stdout.write(JSON.stringify(inventory(), null, 2) + "\n");
