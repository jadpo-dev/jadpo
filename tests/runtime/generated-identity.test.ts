import { afterAll, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const previousDatabaseUrl = Bun.env.DATABASE_URL;
const previousSqlitePath = Bun.env.SQLITE_PATH;
delete Bun.env.DATABASE_URL;
delete Bun.env.SQLITE_PATH;

const root = mkdtempSync(join(tmpdir(), "jadpo-generated-identity-"));
const databasePath = join(root, "generated-identity.sqlite");
Bun.env.SQLITE_PATH = databasePath;
const compiler = Bun.env.JADPO_BIN ?? new URL("../../jadpo/target/debug/jadpo", import.meta.url).pathname;
const source = readFileSync(new URL("../compile/pass/137_generated_identity.jadpo", import.meta.url), "utf8");
writeFileSync(join(root, "app.jadpo"), source);
const build = Bun.spawnSync([compiler, "build", root], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) {
  throw new Error(`Generated identity fixture failed to build:\n${build.stdout}\n${build.stderr}`);
}
const app = await import(pathToFileURL(join(root, "build/target/app.ts")).href);

afterAll(() => {
  if (previousDatabaseUrl === undefined) delete Bun.env.DATABASE_URL;
  else Bun.env.DATABASE_URL = previousDatabaseUrl;
  if (previousSqlitePath === undefined) delete Bun.env.SQLITE_PATH;
  else Bun.env.SQLITE_PATH = previousSqlitePath;
  rmSync(root, { recursive: true, force: true });
});

async function createTodo(title: string) {
  return app.handleRequest(new Request("https://identity.test/todos", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ title }),
  }));
}

describe("compiler-generated entity identities", () => {
  test("creates distinct UUID identities without accepting an ID from input", async () => {
    const first = await createTodo("first");
    const second = await createTodo("second");
    expect(first.status).toBe(200);
    expect(second.status).toBe(200);
    const firstTodo = await first.json() as { id: string; title: string };
    const secondTodo = await second.json() as { id: string; title: string };
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    expect(firstTodo.id).toMatch(uuid);
    expect(firstTodo.title).toBe("first");
    expect(secondTodo.id).toMatch(uuid);
    expect(secondTodo.title).toBe("second");
    expect(firstTodo.id).not.toBe(secondTodo.id);

    const forged = await app.handleRequest(new Request("https://identity.test/todos", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ id: firstTodo.id, title: "forged" }),
    }));
    expect(forged.status).toBe(422);

    const database = new Database(databasePath, { readonly: true, strict: true });
    try {
      expect(database.query("SELECT id, title FROM todo ORDER BY title").all()).toEqual([
        { id: firstTodo.id, title: "first" },
        { id: secondTodo.id, title: "second" },
      ]);
    } finally {
      database.close();
    }
  });
});
