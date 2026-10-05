import { afterAll, expect, test } from "bun:test";
import { SQL } from "bun";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const databaseUrl = Bun.env.DATABASE_URL;
if (!databaseUrl) throw new Error("DATABASE_URL is required for the PostgreSQL keyset-page test");

const root = mkdtempSync(join(tmpdir(), "jadpo-golden-page-postgres-"));
const project = join(root, "application");
const source = resolve("examples/golden-todo-migration");
cpSync(source, project, { recursive: true, filter: path => path !== join(source, "build") });
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Golden PostgreSQL page fixture failed to build:\n${build.stdout}\n${build.stderr}`);

const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
if (!existsSync(join(dependency, "node_modules/jose/package.json"))) throw new Error("Install the pinned JWT dependency before running this suite");
symlinkSync(join(dependency, "node_modules"), join(project, "build/target/node_modules"), "dir");

Bun.env.DATABASE_URL = databaseUrl;
delete Bun.env.SQLITE_PATH;
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
const sql = new SQL({ url: databaseUrl, prepare: false });
const environment = {
  DATABASE_URL: "https://db.test/golden",
  SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test",
  OIDC_ISSUER: "https://issuer.test",
  OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"),
  MAIL_SENDER: "todo@example.test",
};
const owner = "00000000-0000-4000-8000-000000000001";
const otherOwner = "00000000-0000-4000-8000-000000000002";
const now = new Date("2026-01-02T00:00:00.000Z");

function request(path: string, credential: string, method = "GET", body?: unknown): Request {
  return new Request(`https://todo.test${path}`, {
    method,
    headers: { authorization: `Bearer ${credential}`, ...(body === undefined ? {} : { "content-type": "application/json" }) },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
}

function browserRequest(path: string, credential: { setCookie: string; csrfToken: string }, method = "GET"): Request {
  return new Request(`https://todo.test${path}`, {
    method,
    headers: {
      cookie: credential.setCookie.split(";", 1)[0],
      origin: "https://todo.test",
      "x-jadpo-csrf": credential.csrfToken,
    },
  });
}

async function insertTodo(id: string, ownerId: string, title: string, status: string, createdAt: Date, dueAt: Date | null, deletedAt: Date | null = null): Promise<void> {
  await sql.unsafe(
    'INSERT INTO "todo" ("id", "owner_id", "title", "status", "due_at", "reminder_sent_at", "created_at", "updated_at", "deleted_at") VALUES ($1, $2, $3, $4, $5, NULL, $6, $6, $7)',
    [id, ownerId, title, status, dueAt?.toISOString() ?? null, createdAt.toISOString(), deletedAt?.toISOString() ?? null],
  );
}

afterAll(async () => {
  await sql.close();
  rmSync(root, { recursive: true, force: true });
});

test("PostgreSQL keyset pages preserve scope and use the composite index", async () => {
  await app.initializeApplication(environment);
  await sql`DELETE FROM "service_credential"`;
  await sql`DELETE FROM "__jadpo_auth_service_credentials"`;
  await sql`DELETE FROM "service"`;
  await sql`DELETE FROM "todo"`;
  await sql`DELETE FROM "__jadpo_auth_sessions"`;
  await sql`DELETE FROM "user"`;

  await sql.unsafe(
    'INSERT INTO "user" ("id", "authentication_subject", "email", "status", "created_at", "disabled_at") VALUES ($1, $2, $3, $4, $5, NULL), ($6, $7, $8, $4, $5, NULL)',
    [owner, "owner", "owner@example.test", "active", now.toISOString(), otherOwner, "other-owner", "other-owner@example.test"],
  );
  await sql.unsafe(`
    INSERT INTO "todo" ("id", "owner_id", "title", "status", "due_at", "reminder_sent_at", "created_at", "updated_at", "deleted_at")
    SELECT '10000000-0000-4000-8000-' || lpad(item::text, 12, '0'), $1, 'bulk-' || item::text,
      CASE WHEN item % 2 = 0 THEN 'open' ELSE 'completed' END,
      TIMESTAMPTZ '2026-02-01 00:00:00+00', NULL,
      TIMESTAMPTZ '2026-01-02 00:00:00+00' - item * INTERVAL '1 second',
      TIMESTAMPTZ '2026-01-02 00:00:00+00' - item * INTERVAL '1 second', NULL
    FROM generate_series(1, 8000) AS item
  `, [owner]);
  const tie = new Date("2026-01-03T00:00:00.000Z");
  const tieIds = [1, 2, 3, 4, 5].map(index => `20000000-0000-4000-8000-${String(index).padStart(12, "0")}`);
  for (const [index, id] of tieIds.entries()) await insertTodo(id, owner, `tie-${index + 1}`, "open", tie, new Date("2026-01-01T00:00:00.000Z"));
  await insertTodo("20000000-0000-4000-8000-000000000099", owner, "deleted", "open", tie, new Date("2026-01-01T00:00:00.000Z"), tie);
  await insertTodo("30000000-0000-4000-8000-000000000001", otherOwner, "foreign", "open", tie, new Date("2026-01-01T00:00:00.000Z"));
  for (let index = 0; index < 8; index += 1) {
    await insertTodo(
      `40000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`,
      owner,
      `old-deleted-${index}`,
      "open",
      new Date("2026-01-01T00:00:00.000Z"),
      null,
      new Date("2026-01-01T12:00:00.000Z"),
    );
  }
  await insertTodo(
    "50000000-0000-4000-8000-000000000001",
    otherOwner,
    "old-foreign",
    "open",
    new Date("2026-01-01T00:00:00.000Z"),
    null,
  );
  await sql`ANALYZE "todo"`;

  const host = app.authenticationHost();
  const ownerCredential = await host.issue("api_bearer", "owner", Date.now() + 3_600_000, Date.now());
  const firstResponse = await app.handleRequest(request(`/todos?status=open&due_before=${encodeURIComponent("2026-01-02T00:00:00.000Z")}&page_size=2`, ownerCredential.credential));
  expect(firstResponse.status).toBe(200);
  const first = await firstResponse.json();
  expect(first.items.map((item: any) => item.id)).toEqual([tieIds[4], tieIds[3]]);
  expect(first.next).toEqual({ created_at: tie.toISOString(), id: tieIds[3] });

  const secondResponse = await app.handleRequest(request(`/todos?status=open&due_before=${encodeURIComponent("2026-01-02T00:00:00.000Z")}&page_size=2&after=${encodeURIComponent(JSON.stringify(first.next))}`, ownerCredential.credential));
  expect(secondResponse.status).toBe(200);
  const second = await secondResponse.json();
  expect(second.items.map((item: any) => item.id)).toEqual([tieIds[2], tieIds[1]]);
  const thirdResponse = await app.handleRequest(request(`/todos?status=open&due_before=${encodeURIComponent("2026-01-02T00:00:00.000Z")}&page_size=2&after=${encodeURIComponent(JSON.stringify(second.next))}`, ownerCredential.credential));
  expect(thirdResponse.status).toBe(200);
  const third = await thirdResponse.json();
  expect(third.items.map((item: any) => item.id)).toEqual([tieIds[0]]);
  expect(third.next).toBeNull();

  const relationshipResponse = await app.handleRequest(request(`/users/${owner}/todos`, ownerCredential.credential));
  expect(relationshipResponse.status).toBe(200);
  const relationship = await relationshipResponse.json();
  const expectedChildren = await sql.unsafe(
    'SELECT "id" FROM "todo" WHERE "owner_id" = $1 AND "deleted_at" IS NULL ORDER BY "created_at" ASC, "id" ASC LIMIT 100',
    [owner],
  );
  expect(relationship.user_id).toBe(owner);
  expect(relationship.todos.map((item: any) => item.id)).toEqual(expectedChildren.map((item: any) => item.id));
  expect(relationship.todos).toHaveLength(100);
  expect(relationship).not.toHaveProperty("email");
  expect(relationship.todos.every((item: any) => Object.keys(item).sort().join(",") === "created_at,due_at,id,status,title,updated_at")).toBe(true);
  expect(relationship.todos.some((item: any) => item.title.startsWith("old-deleted-") || item.title === "old-foreign")).toBe(false);

  const inventory = JSON.parse(readFileSync(join(project, "build/persistence/entities.json"), "utf8"));
  const plan = inventory.query_plans.find((candidate: any) => candidate.entity === "Todo" && candidate.strategy === "indexed_keyset_page");
  expect(plan).toMatchObject({ query_count: 1, offset: false, fetch_limit: "page_size_plus_one" });
  expect(plan.postgres_after_sql).toContain('("created_at", "id") < ($4, $5)');
  expect(plan.postgres_first_sql).toContain("$2::TEXT IS NULL");
  expect(plan.postgres_first_sql).toContain("$3::TIMESTAMPTZ(3) IS NULL");
  const indexMetadata = await sql`SELECT indexdef FROM pg_indexes WHERE indexname = ${plan.index}`;
  expect(indexMetadata[0]?.indexdef).toContain('(owner_id, created_at DESC, id DESC) WHERE (deleted_at IS NULL)');
  const explainRows = await sql.unsafe(
    `EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ${plan.postgres_after_sql}`,
    [owner, "open", null, "2026-01-01T22:53:20.000Z", "10000000-0000-4000-8000-0000000004000", 26],
  );
  const planRoot = (explainRows[0]["QUERY PLAN"] as Array<{ Plan: Record<string, any> }>)[0].Plan;
  const planNodes = (node: Record<string, any>): Record<string, any>[] => [node, ...(node.Plans ?? []).flatMap(planNodes)];
  const nodes = planNodes(planRoot);
  expect(nodes.some(node => node["Index Name"] === plan.index)).toBe(true);
  expect(nodes.some(node => node["Node Type"] === "Sort")).toBe(false);
  const pageScan = nodes.find(node => node["Index Name"] === plan.index)!;
  expect(pageScan["Actual Rows"]).toBeLessThanOrEqual(26);

  const relationshipPlan = inventory.query_plans.find((candidate: any) => candidate.entity === "Todo" && candidate.strategy === "indexed_keyset_page" && candidate.postgres_first_sql.includes('ORDER BY "created_at" ASC'));
  expect(relationshipPlan).toMatchObject({ query_count: 1, offset: false, fetch_limit: "page_size_plus_one" });
  expect(relationshipPlan.postgres_first_sql).toContain('"deleted_at" IS NULL');
  expect(relationshipPlan.postgres_first_sql).toContain('ORDER BY "created_at" ASC, "id" ASC');
  expect(relationshipPlan.postgres_first_sql.split(" FROM ")[0]).not.toContain('"owner_id"');
  const targetSource = readFileSync(join(project, "build/target/app.ts"), "utf8");
  expect(targetSource).toContain("persistence.query_required_User_by_id(user_id)");
  expect(targetSource).toContain('persistence.query_page_rows("query.Todo.page"');

  const browserIssuedAt = Date.now();
  const browser = await host.issue("browser_session", "owner", browserIssuedAt + 300_000, browserIssuedAt);
  expect(ownerCredential.expires).toBeLessThanOrEqual(browserIssuedAt + 300_000);
  const createdResponse = await app.handleRequest(request("/todos", ownerCredential.credential, "POST", { title: "postgres lifecycle" }));
  expect(createdResponse.status).toBe(201);
  const created = await createdResponse.json();

  const deleteResponse = await app.handleRequest(browserRequest(`/todos/${created.id}`, browser, "DELETE"));
  expect(deleteResponse.status).toBe(204);
  expect(await deleteResponse.text()).toBe("");
  const deleted = await sql`SELECT "deleted_at", "title", "status", "updated_at", ("deleted_at" = "updated_at") AS "timestamp_matches" FROM "todo" WHERE "id" = ${created.id}`;
  expect(deleted[0]?.deleted_at).toBeTruthy();
  expect(deleted[0]).toMatchObject({ title: "postgres lifecycle", status: "open", timestamp_matches: true });
  const visiblePage = await app.handleRequest(request("/todos", ownerCredential.credential));
  expect(visiblePage.status).toBe(200);
  expect((await visiblePage.json()).items.map((item: any) => item.id)).not.toContain(created.id);
  for (const [method, body] of [["GET", undefined], ["PATCH", { title: "after delete" }], ["DELETE", undefined]] as const) {
    const response = await app.handleRequest(request(`/todos/${created.id}`, ownerCredential.credential, method, body));
    expect(response.status).toBe(404);
    expect((await response.json()).error.code).toBe("todo_not_found");
  }
  expect(await sql`SELECT "deleted_at", "title", "status", "updated_at", ("deleted_at" = "updated_at") AS "timestamp_matches" FROM "todo" WHERE "id" = ${created.id}`).toEqual(deleted);

  const foreignDisable = await app.handleRequest(browserRequest(`/users/${otherOwner}`, browser, "DELETE"));
  expect(foreignDisable.status).toBe(404);
  expect((await foreignDisable.json()).error.code).toBe("user_not_found");
  expect(await sql`SELECT "status", "disabled_at" FROM "user" WHERE "id" = ${otherOwner}`)
    .toEqual([{ status: "active", disabled_at: null }]);

  const disabled = await app.handleRequest(browserRequest(`/users/${owner}`, browser, "DELETE"));
  expect(disabled.status).toBe(204);
  expect(await disabled.text()).toBe("");
  const disabledUser = await sql`SELECT "status", "disabled_at" FROM "user" WHERE "id" = ${owner}`;
  expect(disabledUser).toHaveLength(1);
  expect(disabledUser[0]?.status).toBe("disabled");
  expect(disabledUser[0]?.disabled_at).not.toBeNull();
  expect(await sql`SELECT "id" FROM "todo" WHERE "id" = ${created.id}`).toEqual([{ id: created.id }]);
  const repeatedDisable = await app.handleRequest(browserRequest(`/users/${owner}`, browser, "DELETE"));
  expect(repeatedDisable.status).toBe(403);
  expect((await repeatedDisable.json()).error.code).toBe("user_disabled");
  await expect(host.refresh("browser_session", browser.credential, browserIssuedAt + 1)).rejects.toMatchObject({
    code: "principal_inactive",
    declaredFailure: "UserDisabled",
  });
  const boundedRequest = request("/todos", ownerCredential.credential);
  expect(await host.authenticate(boundedRequest, false, browserIssuedAt + 1)).toMatchObject({ kind: "user" });
  await expect(host.authenticate(boundedRequest, false, ownerCredential.expires)).rejects.toMatchObject({ code: "invalid_credentials" });
});
