import { copyGoldenFixture } from "./golden-fixture";
import { afterAll, afterEach, beforeEach, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { existsSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

// RM-107 integration slice: execute the migrated GET/create/PATCH paths against
// an isolated generated target. The frozen 44-case harness remains RM-109.
const root = mkdtempSync(join(tmpdir(), "jadpo-golden-routes-"));
const project = join(root, "application");
const previousSqlitePath = Bun.env.SQLITE_PATH;
const previousDatabaseUrl = Bun.env.DATABASE_URL;
let database: Database | undefined;
let observedSqlStatements: string[] | undefined;
const sqlitePrototype = Database.prototype as any;
const originalPrepare = sqlitePrototype.prepare;
sqlitePrototype.prepare = function (this: Database, sql: string, ...options: unknown[]) {
  const statement = originalPrepare.call(this, sql, ...options);
  if (observedSqlStatements === undefined) return statement;
  return new Proxy(statement, {
    get(target, property) {
      const member = Reflect.get(target, property, target);
      if (property === "all" || property === "get") {
        return (...values: unknown[]) => {
          observedSqlStatements?.push(sql);
          return member.apply(target, values);
        };
      }
      return typeof member === "function" ? member.bind(target) : member;
    },
  });
};
copyGoldenFixture(project);
const compiler = Bun.env.JADPO_BIN ?? resolve("jadpo/target/debug/jadpo");
const build = Bun.spawnSync([compiler, "build", project], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Golden route fixture failed to build:\n${build.stdout}\n${build.stderr}`);
const dependency = resolve(Bun.env.JADPO_JWT_DEPENDENCY_DIR ?? "build/validation/jwt-dependencies");
if (!existsSync(join(dependency, "node_modules/jose/package.json"))) throw new Error("Install the pinned JWT dependency before running this suite");
symlinkSync(join(dependency, "node_modules"), join(project, "build/target/node_modules"), "dir");
Bun.env.SQLITE_PATH = join(root, "golden.sqlite");
delete Bun.env.DATABASE_URL;
const app = await import(pathToFileURL(join(project, "build/target/app.ts")).href);
database = new Database(Bun.env.SQLITE_PATH, { strict: true });
const db = database;
const owner = "00000000-0000-4000-8000-000000000001";
const otherOwner = "00000000-0000-4000-8000-000000000002";
function dbAll<T = Record<string, unknown>>(sql: string, ...values: unknown[]): T[] {
  const statement = db.prepare(sql);
  try { return statement.all(...values) as T[]; } finally { statement.finalize(); }
}
function dbGet<T = Record<string, unknown>>(sql: string, ...values: unknown[]): T | null {
  const statement = db.prepare(sql);
  try { return statement.get(...values) as T | null; } finally { statement.finalize(); }
}
function dbRun(sql: string, ...values: unknown[]): void {
  const statement = db.prepare(sql);
  try { statement.run(...values); } finally { statement.finalize(); }
}
const environment = {
  DATABASE_URL: "https://db.test/golden",
  SESSION_SIGNING_KEY: Buffer.alloc(32, 71).toString("base64url"),
  BROWSER_ORIGIN: "https://todo.test",
  OIDC_ISSUER: "https://issuer.test",
  OIDC_AUDIENCE: "todo",
  MAIL_API_KEY: Buffer.alloc(32, 72).toString("base64url"),
  MAIL_SENDER: "todo@example.test",
};
let now = 0;
let testGate: Promise<void> = Promise.resolve();
let releaseTest: (() => void) | undefined;
beforeEach(async () => {
  const previous = testGate;
  let release!: () => void;
  testGate = new Promise<void>(resolve => { release = resolve; });
  await previous;
  releaseTest = release;
  try {
    await app.initializeApplication(environment);
    db.exec('DELETE FROM todo; DELETE FROM "__jadpo_auth_sessions"; DELETE FROM "user";');
    now = Date.now();
    for (const [id, subject] of [[owner, "owner"], [otherOwner, "other-owner"]]) {
      dbRun('INSERT INTO "user" (id, authentication_subject, email, status, created_at, disabled_at) VALUES (?, ?, ?, ?, ?, ?)', id, subject, `${subject}@example.test`, "active", now, null);
    }
  } catch (error) {
    releaseTest();
    releaseTest = undefined;
    throw error;
  }
});
afterEach(() => {
  releaseTest?.();
  releaseTest = undefined;
});
afterAll(() => {
  database?.close();
  if (previousSqlitePath === undefined) delete Bun.env.SQLITE_PATH;
  else Bun.env.SQLITE_PATH = previousSqlitePath;
  if (previousDatabaseUrl === undefined) delete Bun.env.DATABASE_URL;
  else Bun.env.DATABASE_URL = previousDatabaseUrl;
  rmSync(root, { recursive: true, force: true });
});

const host = () => app.authenticationHost();
const issue = (subject = "owner") => host().issue("api_bearer", subject, now + 3_600_000, now);
const request = (path: string, credential: string, method = "GET", body?: unknown) => new Request(`https://todo.test${path}`, {
  method,
  headers: { authorization: `Bearer ${credential}`, ...(body === undefined ? {} : { "content-type": "application/json" }) },
  ...(body === undefined ? {} : { body: JSON.stringify(body) }),
});
const browserRequest = (path: string, credential: { setCookie: string; csrfToken: string }, method = "GET", body?: unknown) => new Request(`https://todo.test${path}`, {
  method,
  headers: {
    cookie: credential.setCookie.split(";", 1)[0],
    origin: "https://todo.test",
    "x-jadpo-csrf": credential.csrfToken,
    ...(body === undefined ? {} : { "content-type": "application/json" }),
  },
  ...(body === undefined ? {} : { body: JSON.stringify(body) }),
});

async function captureSqliteReads<T>(work: () => Promise<T>): Promise<{ result: T; statements: string[] }> {
  const statements: string[] = [];
  observedSqlStatements = statements;
  try {
    return { result: await work(), statements };
  } finally {
    observedSqlStatements = undefined;
  }
}

function insertTodo(id: string, ownerId: string, title: string, status: "open" | "completed", createdAt: number, dueAt: number | null = null, deletedAt: number | null = null) {
  dbRun("INSERT INTO todo (id, owner_id, title, status, due_at, reminder_sent_at, created_at, updated_at, deleted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)", id, ownerId, title, status, dueAt, null, createdAt, createdAt, deletedAt);
}

test("create, read and patch preserve the declared owner and public projection", async () => {
  const credential = await issue();
  const create = await app.handleRequest(request("/todos", credential.credential, "POST", { title: "first" }));
  expect(create.status).toBe(201);
  const created = await create.json();
  expect(created).toMatchObject({ title: "first", status: "open", due_at: null });
  expect(created).not.toHaveProperty("owner_id");
  const stored = dbGet("SELECT owner_id, title, status, due_at, reminder_sent_at FROM todo WHERE id = ?", created.id);
  expect(stored).toEqual({ owner_id: owner, title: "first", status: "open", due_at: null, reminder_sent_at: null });

  const read = await app.handleRequest(request(`/todos/${created.id}`, credential.credential));
  expect(read.status).toBe(200);
  expect(await read.json()).toEqual(created);

  const patch = await app.handleRequest(request(`/todos/${created.id}`, credential.credential, "PATCH", { title: "changed" }));
  expect(patch.status).toBe(200);
  expect(await patch.json()).toMatchObject({ id: created.id, title: "changed", status: "open", due_at: null });
  expect(dbGet("SELECT owner_id, title, status, due_at FROM todo WHERE id = ?", created.id))
    .toEqual({ owner_id: owner, title: "changed", status: "open", due_at: null });
});

test("list pages use stable owner-scoped keysets and exclude deleted or foreign rows", async () => {
  const credential = await issue();
  const tie = now - 60_000;
  const ids = [
    "00000000-0000-4000-8000-000000000011",
    "00000000-0000-4000-8000-000000000012",
    "00000000-0000-4000-8000-000000000013",
    "00000000-0000-4000-8000-000000000014",
    "00000000-0000-4000-8000-000000000015",
  ];
  for (const [index, id] of ids.entries()) insertTodo(id, owner, `item-${index + 1}`, "open", tie);
  insertTodo("00000000-0000-4000-8000-000000000016", owner, "deleted", "open", tie, null, tie);
  insertTodo("00000000-0000-4000-8000-000000000020", otherOwner, "foreign", "open", tie);

  const firstResponse = await app.handleRequest(request("/todos?page_size=2", credential.credential));
  expect(firstResponse.status, JSON.stringify(await firstResponse.clone().json())).toBe(200);
  const first = await firstResponse.json();
  expect(first.items.map((item: any) => item.id)).toEqual([ids[4], ids[3]]);
  expect(first.next).toEqual({ created_at: new Date(tie).toISOString(), id: ids[3] });
  expect(first.items.every((item: any) => !("owner_id" in item))).toBe(true);

  const cursor = encodeURIComponent(JSON.stringify(first.next));
  const secondResponse = await app.handleRequest(request(`/todos?page_size=2&after=${cursor}`, credential.credential));
  expect(secondResponse.status).toBe(200);
  const second = await secondResponse.json();
  expect(second.items.map((item: any) => item.id)).toEqual([ids[2], ids[1]]);
  expect(second.next).toEqual({ created_at: new Date(tie).toISOString(), id: ids[1] });

  const thirdResponse = await app.handleRequest(request(`/todos?page_size=2&after=${encodeURIComponent(JSON.stringify(second.next))}`, credential.credential));
  expect(thirdResponse.status).toBe(200);
  const third = await thirdResponse.json();
  expect(third.items.map((item: any) => item.id)).toEqual([ids[0]]);
  expect(third.next).toBeNull();

  const repeated = await app.handleRequest(request("/todos?page_size=2", credential.credential));
  expect(await repeated.json()).toEqual(first);
});

test("list filters compose, defaults are bounded, and generated SQL has an indexed one-query keyset plan", async () => {
  const credential = await issue();
  const older = now - 120_000;
  const newer = now - 60_000;
  insertTodo("00000000-0000-4000-8000-000000000031", owner, "old open", "open", older, older);
  insertTodo("00000000-0000-4000-8000-000000000032", owner, "new open", "open", newer, newer);
  insertTodo("00000000-0000-4000-8000-000000000033", owner, "old completed", "completed", older, older);
  insertTodo("00000000-0000-4000-8000-000000000034", owner, "no due date", "open", older);

  const filtered = await app.handleRequest(request(`/todos?status=open&due_before=${encodeURIComponent(new Date(now - 90_000).toISOString())}`, credential.credential));
  expect(filtered.status, JSON.stringify(await filtered.clone().json())).toBe(200);
  const filteredPage = await filtered.json();
  expect(filteredPage.items.map((item: any) => item.id)).toEqual(["00000000-0000-4000-8000-000000000031"]);
  expect(filteredPage.next).toBeNull();

  const defaults = await app.handleRequest(request("/todos", credential.credential));
  expect(defaults.status).toBe(200);
  expect((await defaults.json()).items).toHaveLength(4);
  expect((await app.handleRequest(request("/todos?page_size=101", credential.credential))).status).toBe(422);

  const inventory = JSON.parse(readFileSync(join(project, "build/persistence/entities.json"), "utf8"));
  const plan = inventory.query_plans.find((candidate: any) => candidate.entity === "Todo" && candidate.strategy === "indexed_keyset_page");
  expect(plan).toMatchObject({ query_count: 1, offset: false, fetch_limit: "page_size_plus_one" });
  expect(plan.index).toBe(
    "todo_page_owner_id_created_at_id_where_deleted_at_is_null_idx",
  );
  expect(plan.sqlite_after_sql).toContain('("created_at", "id") < (?4, ?5)');
  expect(plan.sqlite_after_sql).not.toContain("OFFSET");
  const explain = dbAll<{ detail: string }>(`EXPLAIN QUERY PLAN ${plan.sqlite_after_sql}`, owner, "open", now, now - 30_000, "00000000-0000-4000-8000-000000000099", 26);
  expect(explain.some(row => row.detail.includes(plan.index))).toBe(true);
  expect(explain.some(row => row.detail.includes("TEMP B-TREE"))).toBe(false);
  expect(dbAll<{ name: string }>("PRAGMA index_list('todo')").some(row => row.name === plan.index)).toBe(true);
});

test("foreign and missing reads are indistinguishable and perform no write", async () => {
  const ownerCredential = await issue();
  const create = await app.handleRequest(request("/todos", ownerCredential.credential, "POST", { title: "private" }));
  const created = await create.json();
  const otherCredential = await issue("other-owner");
  for (const id of [created.id, "00000000-0000-4000-8000-000000000099"]) {
    const response = await app.handleRequest(request(`/todos/${id}`, otherCredential.credential));
    expect(response.status).toBe(404);
    expect((await response.json()).error.code).toBe("todo_not_found");
  }
  const before = dbGet("SELECT title, updated_at FROM todo WHERE id = ?", created.id);
  const rejected = await app.handleRequest(request(`/todos/${created.id}`, otherCredential.credential, "PATCH", { title: "stolen" }));
  expect(rejected.status).toBe(404);
  expect((await rejected.json()).error.code).toBe("todo_not_found");
  expect(dbGet("SELECT title, updated_at FROM todo WHERE id = ?", created.id)).toEqual(before);
});

test("self todo collection returns bounded ordered TodoView values with two data reads", async () => {
  const otherCredential = await issue("other-owner");
  const zero = await captureSqliteReads(() => app.handleRequest(request(`/users/${otherOwner}/todos`, otherCredential.credential)));
  expect(zero.result.status).toBe(200);
  expect(await zero.result.json()).toEqual({ user_id: otherOwner, todos: [] });
  expect(zero.statements.filter(sql => /SELECT\b[\s\S]*FROM\s+"user"[\s\S]*WHERE\s+"id"\s*=/i.test(sql))).toHaveLength(1);
  expect(zero.statements.filter(sql => /SELECT\b[\s\S]*FROM\s+"todo"/i.test(sql))).toHaveLength(1);

  insertTodo("00000000-0000-4000-8000-000000000901", otherOwner, "one item", "open", now - 1000);
  const one = await app.handleRequest(request(`/users/${otherOwner}/todos`, otherCredential.credential));
  expect(one.status).toBe(200);
  const oneBody = await one.json();
  expect(oneBody.user_id).toBe(otherOwner);
  expect(oneBody.todos.map((item: any) => item.id)).toEqual(["00000000-0000-4000-8000-000000000901"]);
  expect(oneBody).not.toHaveProperty("email");
  expect(oneBody.todos[0]).not.toHaveProperty("owner_id");

  const ownerCredential = await issue();
  const active: Array<{ id: string; createdAt: number }> = [];
  for (let index = 0; index < 105; index += 1) {
    const id = `00000000-0000-4000-8000-${String(1000 + index).padStart(12, "0")}`;
    const createdAt = now - 500_000 + Math.floor(index / 2);
    active.push({ id, createdAt });
    insertTodo(id, owner, `active-${index}`, "open", createdAt);
  }
  for (let index = 0; index < 8; index += 1) {
    insertTodo(
      `00000000-0000-4000-8000-${String(2000 + index).padStart(12, "0")}`,
      owner,
      `deleted-${index}`,
      "open",
      now - 700_000 + index,
      null,
      now - 1000,
    );
  }
  insertTodo("00000000-0000-4000-8000-000000000099", otherOwner, "foreign", "open", now - 900_000);

  const many = await captureSqliteReads(() => app.handleRequest(request(`/users/${owner}/todos`, ownerCredential.credential)));
  expect(many.result.status, JSON.stringify(await many.result.clone().json())).toBe(200);
  const body = await many.result.json();
  const expected = active.sort((left, right) => left.createdAt - right.createdAt || left.id.localeCompare(right.id));
  expect(body.user_id).toBe(owner);
  expect(body.todos.map((item: any) => item.id)).toEqual(expected.slice(0, 100).map(item => item.id));
  expect(body.todos).toHaveLength(100);
  expect(body).not.toHaveProperty("email");
  expect(body.todos.every((item: any) => Object.keys(item).sort().join(",") === "created_at,due_at,id,status,title,updated_at")).toBe(true);
  expect(body.todos.some((item: any) => item.title.startsWith("deleted-") || item.title === "foreign")).toBe(false);

  const dataReads = many.statements.filter(sql => /SELECT\b[\s\S]*FROM\s+"user"[\s\S]*WHERE\s+"id"\s*=|SELECT\b[\s\S]*FROM\s+"todo"/i.test(sql));
  expect(dataReads).toHaveLength(2);
  const todoRead = dataReads.find(sql => /FROM\s+"todo"/i.test(sql))!;
  expect(todoRead).toContain('"deleted_at" IS NULL');
  expect(todoRead).toContain('ORDER BY "created_at" ASC, "id" ASC');
  expect(todoRead).toContain("LIMIT");
});

test("self todo collection conceals another or missing user before querying children", async () => {
  const credential = await issue();
  for (const userId of [otherOwner, "00000000-0000-4000-8000-000000000099"]) {
    const captured = await captureSqliteReads(() => app.handleRequest(request(`/users/${userId}/todos`, credential.credential)));
    expect(captured.result.status).toBe(404);
    expect((await captured.result.json()).error.code).toBe("user_not_found");
    expect(captured.statements.some(sql => /FROM\s+"todo"/i.test(sql))).toBe(false);
  }
});

test("patch distinguishes omitted due date, explicit null, supplied due date and empty input", async () => {
  const credential = await issue();
  const due = new Date(now + 60_000).toISOString();
  const create = await app.handleRequest(request("/todos", credential.credential, "POST", { title: "reminder", due_at: due }));
  const created = await create.json();
  dbRun("UPDATE todo SET reminder_sent_at = ? WHERE id = ?", now, created.id);

  const omitted = await app.handleRequest(request(`/todos/${created.id}`, credential.credential, "PATCH", { title: "renamed" }));
  expect(omitted.status).toBe(200);
  expect(dbGet("SELECT due_at, reminder_sent_at FROM todo WHERE id = ?", created.id)).toEqual({ due_at: now + 60_000, reminder_sent_at: now });

  const cleared = await app.handleRequest(request(`/todos/${created.id}`, credential.credential, "PATCH", { due_at: null }));
  expect(cleared.status).toBe(200);
  expect(dbGet("SELECT due_at, reminder_sent_at FROM todo WHERE id = ?", created.id)).toEqual({ due_at: null, reminder_sent_at: null });

  const empty = await app.handleRequest(request(`/todos/${created.id}`, credential.credential, "PATCH", {}));
  expect(empty.status).toBe(422);
  expect((await empty.json()).error.code).toBe("empty_patch");
});

test("invalid create and patch dates are rejected without writes", async () => {
  const credential = await issue();
  const before = dbGet<{ count: number }>("SELECT count(*) AS count FROM todo");
  const past = await app.handleRequest(request("/todos", credential.credential, "POST", { title: "past", due_at: new Date(now - 1).toISOString() }));
  expect(past.status).toBe(422);
  expect((await past.json()).error.code).toBe("due_date_in_past");
  expect(dbGet("SELECT count(*) AS count FROM todo")).toEqual(before);

  const create = await app.handleRequest(request("/todos", credential.credential, "POST", { title: "valid" }));
  const created = await create.json();
  const rowBefore = dbGet("SELECT title, due_at, updated_at FROM todo WHERE id = ?", created.id);
  const patch = await app.handleRequest(request(`/todos/${created.id}`, credential.credential, "PATCH", { due_at: new Date(now - 1).toISOString() }));
  expect(patch.status).toBe(422);
  expect((await patch.json()).error.code).toBe("due_date_in_past");
  expect(dbGet("SELECT title, due_at, updated_at FROM todo WHERE id = ?", created.id)).toEqual(rowBefore);
});

test("HTTP delete soft-hides a todo and fresh self-disable retains it while closing authentication", async () => {
  const browser = await host().issue("browser_session", "owner", now + 300_000, now);
  const bounded = await issue();
  expect(bounded.expires).toBeLessThanOrEqual(now + 300_000);

  const otherCredential = await issue("other-owner");
  const foreignTodo = "00000000-0000-4000-8000-000000000777";
  insertTodo(foreignTodo, owner, "private", "open", now - 1_000);
  const foreignDelete = await app.handleRequest(request(`/todos/${foreignTodo}`, otherCredential.credential, "DELETE"));
  expect(foreignDelete.status).toBe(404);
  expect((await foreignDelete.json()).error.code).toBe("todo_not_found");
  expect(dbGet("SELECT deleted_at, title FROM todo WHERE id = ?", foreignTodo)).toEqual({ deleted_at: null, title: "private" });

  const create = await app.handleRequest(request("/todos", bounded.credential, "POST", { title: "retained" }));
  expect(create.status).toBe(201);
  const todo = await create.json();
  const deleteResponse = await app.handleRequest(browserRequest(`/todos/${todo.id}`, browser, "DELETE"));
  expect(deleteResponse.status).toBe(204);
  expect(await deleteResponse.text()).toBe("");
  const deletedRow = dbGet<{ deleted_at: number; title: string; status: string; updated_at: number }>(
    "SELECT deleted_at, title, status, updated_at FROM todo WHERE id = ?", todo.id,
  );
  expect(deletedRow).toMatchObject({ title: "retained", status: "open" });
  expect(deletedRow?.deleted_at).toBeGreaterThan(0);
  expect(deletedRow?.updated_at).toBe(deletedRow?.deleted_at);
  expect((await (await app.handleRequest(request("/todos", bounded.credential))).json()).items.map((item: any) => item.id))
    .not.toContain(todo.id);

  const hiddenGet = await app.handleRequest(request(`/todos/${todo.id}`, bounded.credential));
  expect(hiddenGet.status).toBe(404);
  expect((await hiddenGet.json()).error.code).toBe("todo_not_found");
  const hiddenPatch = await app.handleRequest(request(`/todos/${todo.id}`, bounded.credential, "PATCH", { title: "after-delete" }));
  expect(hiddenPatch.status).toBe(404);
  expect((await hiddenPatch.json()).error.code).toBe("todo_not_found");
  const repeatedDelete = await app.handleRequest(browserRequest(`/todos/${todo.id}`, browser, "DELETE"));
  expect(repeatedDelete.status).toBe(404);
  expect((await repeatedDelete.json()).error.code).toBe("todo_not_found");
  expect(dbGet("SELECT deleted_at, title, status, updated_at FROM todo WHERE id = ?", todo.id)).toEqual(deletedRow);

  const foreignDisable = await app.handleRequest(browserRequest(`/users/${otherOwner}`, browser, "DELETE"));
  expect(foreignDisable.status).toBe(404);
  expect((await foreignDisable.json()).error.code).toBe("user_not_found");
  expect(dbGet<{ status: string; disabled_at: number | null }>('SELECT status, disabled_at FROM "user" WHERE id = ?', otherOwner))
    .toEqual({ status: "active", disabled_at: null });

  const disable = await app.handleRequest(browserRequest(`/users/${owner}`, browser, "DELETE"));
  expect(disable.status).toBe(204);
  expect(await disable.text()).toBe("");
  expect(dbGet<{ status: string; disabled_at: number }>('SELECT status, disabled_at FROM "user" WHERE id = ?', owner))
    .toMatchObject({ status: "disabled" });
  expect(dbGet<{ disabled_at: number }>('SELECT disabled_at FROM "user" WHERE id = ?', owner)?.disabled_at).toBeGreaterThan(0);
  expect(dbGet("SELECT id FROM todo WHERE id = ?", todo.id)).toEqual({ id: todo.id });

  const repeatedDisable = await app.handleRequest(browserRequest(`/users/${owner}`, browser, "DELETE"));
  expect(repeatedDisable.status).toBe(403);
  expect((await repeatedDisable.json()).error.code).toBe("user_disabled");
  await expect(host().refresh("browser_session", browser.credential, now + 1)).rejects.toMatchObject({
    code: "principal_inactive",
    declaredFailure: "UserDisabled",
  });
  const boundedRequest = request("/todos", bounded.credential);
  expect(await host().authenticate(boundedRequest, false, now + 1)).toMatchObject({ kind: "user" });
  await expect(host().authenticate(boundedRequest, false, bounded.expires)).rejects.toMatchObject({ code: "invalid_credentials" });
});

test("concurrent HTTP patch and delete leave one atomic hidden todo state", async () => {
  const credential = await issue();
  const create = await app.handleRequest(request("/todos", credential.credential, "POST", { title: "race" }));
  expect(create.status).toBe(201);
  const todo = await create.json();

  const [deleted, patched] = await Promise.all([
    app.handleRequest(request(`/todos/${todo.id}`, credential.credential, "DELETE")),
    app.handleRequest(request(`/todos/${todo.id}`, credential.credential, "PATCH", { title: "raced" })),
  ]);
  expect([204, 404]).toContain(deleted.status);
  expect([200, 404]).toContain(patched.status);
  expect(dbGet<{ deleted_at: number | null; title: string; status: string }>(
    "SELECT deleted_at, title, status FROM todo WHERE id = ?", todo.id,
  )).toMatchObject({ status: "open" });
  expect(dbGet<{ deleted_at: number | null }>("SELECT deleted_at FROM todo WHERE id = ?", todo.id)?.deleted_at).toBeGreaterThan(0);
  expect(["race", "raced"]).toContain(dbGet<{ title: string }>("SELECT title FROM todo WHERE id = ?", todo.id)?.title);
  const after = await app.handleRequest(request(`/todos/${todo.id}`, credential.credential));
  expect(after.status).toBe(404);
});
