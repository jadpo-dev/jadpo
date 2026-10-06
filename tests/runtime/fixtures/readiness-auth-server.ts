import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { pathToFileURL } from "node:url";

const applicationPath = process.argv.at(-1);
if (applicationPath === undefined) throw new Error("generated application path is required");

let userSchemaInitializations = 0;
const databasePrototype = Database.prototype as any;
const originalExec = databasePrototype.exec;
databasePrototype.exec = function (statement: string, ...values: unknown[]) {
  if (statement.includes('CREATE TABLE IF NOT EXISTS "user"')) userSchemaInitializations += 1;
  return originalExec.call(this, statement, ...values);
};

const application = await import(pathToFileURL(applicationPath).href);
await application.initializeApplication(Bun.env);

application.createDeploymentServer(Number(Bun.env.JADPO_DEPLOYMENT_PORT));
Bun.serve({
  hostname: "127.0.0.1",
  port: Number(Bun.env.PORT ?? "0"),
  async fetch(request) {
    const url = new URL(request.url);
    // This test-only host harness seeds an authority record and issues a
    // session only after the parent test observes readiness recovery.
    if (request.method === "POST" && url.pathname === "/__test/issue") {
      if (Bun.env.DATABASE_URL) {
        const database = new SQL(Bun.env.DATABASE_URL, { prepare: false });
        try {
          await database.unsafe('INSERT INTO "user" ("id", "authentication_subject", "enabled") VALUES ($1, $2, true)',
            ["00000000-0000-4000-8000-000000000007", "readiness-user"]);
        } finally {
          await database.close();
        }
      } else {
        const database = new Database(Bun.env.SQLITE_PATH!, { strict: true });
        try {
          database.prepare('INSERT INTO "user" ("id", "authentication_subject", "enabled") VALUES (?, ?, 1)')
            .run("00000000-0000-4000-8000-000000000007", "readiness-user");
        } finally {
          database.close();
        }
      }
      const now = Date.now();
      const issued = await application.authenticationHost().issue(
        "api_bearer",
        "readiness-user",
        now + 60 * 60 * 1000,
        now,
      );
      return Response.json({ credential: issued.credential });
    }
    if (request.method === "GET" && url.pathname === "/__test/readiness-stats") {
      return Response.json({ userSchemaInitializations });
    }
    return application.handleRequest(request);
  },
});
