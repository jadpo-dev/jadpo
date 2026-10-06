import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { SQL } from "bun";
import { Database } from "bun:sqlite";

const postgresUrl = Bun.env.JADPO_LIFECYCLE_DATABASE_URL;
delete Bun.env.DATABASE_URL;
delete Bun.env.SQLITE_PATH;
const root = mkdtempSync(join(tmpdir(), "jadpo-lifecycle-"));
const databasePath = join(root, "lifecycle.sqlite");
if (postgresUrl) Bun.env.DATABASE_URL = postgresUrl;
else Bun.env.SQLITE_PATH = databasePath;
Bun.env.JADPO_LIFECYCLE_RETENTION = "30d";

const { persistence, configureLifecycleRetention } = await import(
  "../../tests/compile/pass/lifecycle/build/target/persistence.ts"
);
const { handleRequest, runLifecycleMaintenance } = await import(
  "../../tests/compile/pass/lifecycle/build/target/app.ts"
);
const postgres = postgresUrl ? new SQL({ url: postgresUrl, prepare: false }) : null;

afterAll(async () => {
  delete Bun.env.SQLITE_PATH;
  delete Bun.env.DATABASE_URL;
  delete Bun.env.JADPO_LIFECYCLE_RETENTION;
  rmSync(root, { recursive: true, force: true });
  await postgres?.close();
});

function instant(value: unknown): string {
  if (value instanceof Date) return value.toISOString();
  if (typeof value === "number") return new Date(value).toISOString();
  return String(value);
}

async function seedTodos(
  ids: readonly string[],
  userId: string,
  deletedAt: string | null,
  status = "open",
): Promise<void> {
  if (postgres) {
    const parameters: unknown[] = [];
    const values = ids.map((id, index) => {
      const offset = index * 6;
      parameters.push(
        id,
        userId,
        `retained-${index}`,
        status,
        deletedAt,
        deletedAt ?? "2026-10-03T14:00:00.000Z",
      );
      return `($${offset + 1}, $${offset + 2}, $${offset + 3}, $${offset + 4}, $${offset + 5}, $${offset + 6})`;
    });
    await postgres.unsafe(
      `INSERT INTO "todo" ("id", "user_id", "title", "status", "deleted_at", "updated_at") VALUES ${values.join(", ")}`,
      parameters,
    );
    return;
  }
  const database = new Database(databasePath, { strict: true });
  try {
    database.exec("PRAGMA foreign_keys = ON; BEGIN IMMEDIATE");
    const insert = database.prepare(
      'INSERT INTO "todo" ("id", "user_id", "title", "status", "deleted_at", "updated_at") VALUES (?1, ?2, ?3, ?4, ?5, ?6)',
    );
    const deletedAtMilliseconds = deletedAt === null ? null : Date.parse(deletedAt);
    const updatedAtMilliseconds = Date.parse(
      deletedAt ?? "2026-10-03T14:00:00.000Z",
    );
    ids.forEach((id, index) =>
      insert.run(
        id,
        userId,
        `retained-${index}`,
        status,
        deletedAtMilliseconds,
        updatedAtMilliseconds,
      ),
    );
    database.exec("COMMIT");
  } catch (error) {
    database.exec("ROLLBACK");
    throw error;
  } finally {
    database.close();
  }
}

async function seedTodo(
  id: string,
  userId: string,
  title: string,
  status: string,
  deletedAt: string | null,
  updatedAt: string,
): Promise<void> {
  if (postgres) {
    await postgres.unsafe(
      'INSERT INTO "todo" ("id", "user_id", "title", "status", "deleted_at", "updated_at") VALUES ($1, $2, $3, $4, $5, $6)',
      [id, userId, title, status, deletedAt, updatedAt],
    );
    return;
  }
  const database = new Database(databasePath, { strict: true });
  try {
    database.exec("PRAGMA foreign_keys = ON");
    database
      .query('INSERT INTO "todo" ("id", "user_id", "title", "status", "deleted_at", "updated_at") VALUES (?1, ?2, ?3, ?4, ?5, ?6)')
      .run(
        id,
        userId,
        title,
        status,
        deletedAt === null ? null : Date.parse(deletedAt),
        Date.parse(updatedAt),
      );
  } finally {
    database.close();
  }
}

async function expiredTodoIds(): Promise<string[]> {
  if (postgres) {
    const rows = await postgres`SELECT id FROM todo WHERE deleted_at IS NOT NULL AND deleted_at < '2021-01-01T00:00:00Z' ORDER BY deleted_at ASC, id ASC`;
    return rows.map((row) => String(row.id));
  }
  const database = new Database(databasePath, { readonly: true, strict: true });
  try {
    const rows = database
      .query('SELECT id FROM "todo" WHERE deleted_at IS NOT NULL AND deleted_at < ?1 ORDER BY deleted_at ASC, id ASC')
      .all(Date.parse("2021-01-01T00:00:00.000Z")) as Array<{ id: string }>;
    return rows.map((row) => row.id);
  } finally {
    database.close();
  }
}

async function todoExists(id: string): Promise<boolean> {
  if (postgres) {
    const rows = await postgres`SELECT id FROM todo WHERE id = ${id}`;
    return rows.length === 1;
  }
  const database = new Database(databasePath, { readonly: true, strict: true });
  try {
    const row = database
      .query('SELECT id FROM "todo" WHERE id = ?1')
      .get(id);
    return row !== null && row !== undefined;
  } finally {
    database.close();
  }
}

async function todoRecord(
  id: string,
): Promise<{ title: string; status: string; deleted_at: unknown; updated_at: unknown } | null> {
  if (postgres) {
    const rows = await postgres`SELECT title, status, deleted_at, updated_at FROM todo WHERE id = ${id}`;
    return (rows[0] as typeof rows[number] | undefined) ?? null;
  }
  const database = new Database(databasePath, { readonly: true, strict: true });
  try {
    return database
      .query('SELECT title, status, deleted_at, updated_at FROM "todo" WHERE id = ?1')
      .get(id) as { title: string; status: string; deleted_at: unknown; updated_at: unknown } | null;
  } finally {
    database.close();
  }
}

async function insertTodoReference(id: string, todoId: string): Promise<void> {
  if (postgres) {
    await postgres`INSERT INTO todo_reference (id, todo_id) VALUES (${id}, ${todoId})`;
    return;
  }
  const database = new Database(databasePath, { strict: true });
  try {
    database.exec("PRAGMA foreign_keys = ON");
    database
      .query('INSERT INTO "todo_reference" (id, todo_id) VALUES (?1, ?2)')
      .run(id, todoId);
  } finally {
    database.close();
  }
}

async function removeTodoReference(id: string): Promise<void> {
  if (postgres) {
    await postgres`DELETE FROM todo_reference WHERE id = ${id}`;
    return;
  }
  const database = new Database(databasePath, { strict: true });
  try {
    database.query('DELETE FROM "todo_reference" WHERE id = ?1').run(id);
  } finally {
    database.close();
  }
}

describe("entity lifecycle transition lowering", () => {
  test("fails closed when direct purge runs before retention is initialized", async () => {
    await expect(
      persistence
        .withOperationTime(new Date().toISOString())
        .retention_purge_Todo(() => {}),
    ).rejects.toThrow();
  });

  test("creates lifecycle-owned initial values through generated routes", async () => {
    const create = (path: string, body: Record<string, unknown>) =>
      handleRequest(
        new Request(`https://lifecycle.test${path}`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(body),
        }),
      );

    const userResponse = await create("/lifecycle/users", {
      name: "Route-created owner",
      batch_group: "route-created",
    });
    expect(userResponse.status).toBe(200);
    const user = (await userResponse.json()) as Record<string, unknown>;
    expect(user).toMatchObject({ status: "active", disabled_at: null });

    const todoResponse = await create("/lifecycle/todos", {
      user_id: user.id,
      title: "Route-created Todo",
      status: "open",
    });
    expect(todoResponse.status).toBe(200);
    const todo = (await todoResponse.json()) as Record<string, unknown>;
    expect(todo).toMatchObject({ status: "open", deleted_at: null });
    expect(await persistence.query_optional_User_by_id(user.id)).toMatchObject({
      status: "active",
      disabled_at: null,
    });
    expect(await persistence.query_optional_Todo_by_id(todo.id)).toMatchObject({
      status: "open",
      deleted_at: null,
    });

    const forgedUser = await create("/lifecycle/users", {
      name: "Forged owner",
      batch_group: "route-created",
      status: "disabled",
      disabled_at: new Date().toISOString(),
    });
    const forgedTodo = await create("/lifecycle/todos", {
      user_id: user.id,
      title: "Forged Todo",
      status: "open",
      deleted_at: new Date().toISOString(),
    });
    expect(forgedUser.status).toBe(422);
    expect(forgedTodo.status).toBe(422);
  });

  test("rejects host persistence attempts to override lifecycle initial state", async () => {
    const userId = crypto.randomUUID();
    const createdAt = new Date().toISOString();

    await expect(
      persistence.withOperationTime(createdAt).create_User({
        id: userId,
        name: "Host-forged disabled owner",
        batch_group: "host-boundary",
        status: "disabled",
        disabled_at: createdAt,
        updated_at: createdAt,
      }),
    ).rejects.toThrow();
    expect(await persistence.query_optional_User_by_id(userId)).toBeNull();

    const activeUserId = crypto.randomUUID();
    await persistence.withOperationTime(createdAt).create_User({
      id: activeUserId,
      name: "Host-created active owner",
      batch_group: "host-boundary",
      status: "active",
      disabled_at: null,
      updated_at: createdAt,
    });

    const todoId = crypto.randomUUID();
    await expect(
      persistence.withOperationTime(createdAt).create_Todo({
        id: todoId,
        user_id: activeUserId,
        title: "Host-forged deleted Todo",
        status: "open",
        deleted_at: createdAt,
        updated_at: createdAt,
      }),
    ).rejects.toThrow();
    expect(await persistence.query_optional_Todo_by_id(todoId)).toBeNull();
  });

  test("binds direct host purge eligibility to configured retention", async () => {
    configureLifecycleRetention({ soft_delete_retention: "PT720H" });
    const userId = crypto.randomUUID();
    const todoId = crypto.randomUUID();
    const now = Date.now();
    const createdAt = new Date(now - 20 * 86_400_000).toISOString();
    const deletedAt = new Date(now - 10 * 86_400_000).toISOString();
    await persistence.withOperationTime(createdAt).create_User({
      id: userId,
      name: "Retention boundary owner",
      batch_group: "host-purge-boundary",
      status: "active",
      disabled_at: null,
      updated_at: createdAt,
    });
    await seedTodo(todoId, userId, "Young retained Todo", "open", deletedAt, deletedAt);

    const forgedAttemptTime = new Date(now + 365 * 86_400_000).toISOString();
    await persistence
      .withOperationTime(forgedAttemptTime)
      .retention_purge_Todo(() => {});
    expect(await todoExists(todoId)).toBeTrue();
  });

  test("uses initialized retention instead of a different process environment value", async () => {
    expect(Bun.env.JADPO_LIFECYCLE_RETENTION).toBe("30d");
    configureLifecycleRetention({ soft_delete_retention: "PT120H" });

    const userId = crypto.randomUUID();
    const todoId = crypto.randomUUID();
    const now = Date.now();
    const createdAt = new Date(now - 20 * 86_400_000).toISOString();
    const deletedAt = new Date(now - 10 * 86_400_000).toISOString();
    await persistence.withOperationTime(createdAt).create_User({
      id: userId,
      name: "Initialized retention owner",
      batch_group: "host-purge-config",
      status: "active",
      disabled_at: null,
      updated_at: createdAt,
    });
    await seedTodo(todoId, userId, "Beyond initialized retention", "open", deletedAt, deletedAt);

    await persistence
      .withOperationTime(new Date(now).toISOString())
      .retention_purge_Todo(() => {});
    expect(await todoExists(todoId)).toBeFalse();

    configureLifecycleRetention({ soft_delete_retention: "PT720H" });
  });

  test("atomically disables one visible identity and conceals it afterward", async () => {
    const id = crypto.randomUUID();
    const createdAt = "2026-10-03T10:00:00.000Z";
    const disabledAt = "2026-10-03T10:05:00.000Z";

    const created = (await persistence
      .withOperationTime(createdAt)
      .create_User({
        id,
        name: "Ada",
        batch_group: "default",
        status: "active",
        disabled_at: null,
        updated_at: createdAt,
      })) as Record<string, unknown>;
    expect(created.status).toBe("active");
    expect(instant(created.updated_at)).toBe(createdAt);

    const changed = (await persistence
      .withOperationTime(disabledAt)
      .transition_required_User_by_id_disable(id)) as Record<string, unknown>;
    expect(changed.status).toBe("disabled");
    expect(instant(changed.disabled_at)).toBe(disabledAt);
    expect(instant(changed.updated_at)).toBe(disabledAt);

    expect(
      await persistence
        .withOperationTime("2026-10-03T10:06:00.000Z")
        .transition_required_User_by_id_disable(id),
    ).toBeNull();
    expect(await persistence.query_optional_User_by_id(id)).toBeNull();
    expect(
      await persistence.update_required_User_by_id_set_name(id, "Grace"),
    ).toBeNull();
    expect("delete_required_User_by_id" in persistence).toBeFalse();
    expect("update_required_User_by_id_set_status" in persistence).toBeFalse();

    let storedName: string;
    if (postgres) {
      const rows = await postgres`SELECT name FROM "user" WHERE id = ${id}`;
      storedName = String(rows[0]?.name);
    } else {
      const database = new Database(databasePath, { readonly: true, strict: true });
      try {
        storedName = (database
          .query('SELECT name FROM "user" WHERE id = ?1')
          .get(id) as { name: string }).name;
      } finally {
        database.close();
      }
    }
    expect(storedName).toBe("Ada");
  });

  test("serializes a transition against an ordinary update", async () => {
    const id = crypto.randomUUID();
    const createdAt = "2026-10-03T11:00:00.000Z";
    const disabledAt = "2026-10-03T11:01:00.000Z";
    await persistence.withOperationTime(createdAt).create_User({
      id,
      name: "Ada",
      batch_group: "default",
      status: "active",
      disabled_at: null,
      updated_at: createdAt,
    });

    const [changed, renamed] = await Promise.all([
      persistence
        .withOperationTime(disabledAt)
        .transition_required_User_by_id_disable(id),
      persistence
        .withOperationTime("2026-10-03T11:00:30.000Z")
        .update_required_User_by_id_set_name(id, "Grace"),
    ]);
    expect(changed).not.toBeNull();
    expect(
      renamed === null || (renamed as Record<string, unknown>).name === "Grace",
    ).toBeTrue();
    expect(await persistence.query_optional_User_by_id(id)).toBeNull();

    let row: { name: string; status: string; disabled_at: unknown };
    if (postgres) {
      const rows = await postgres`SELECT name, status, disabled_at FROM "user" WHERE id = ${id}`;
      row = rows[0] as typeof row;
    } else {
      const database = new Database(databasePath, { readonly: true, strict: true });
      try {
        row = database
          .query('SELECT name, status, disabled_at FROM "user" WHERE id = ?1')
          .get(id) as typeof row;
      } finally {
        database.close();
      }
    }
    expect(row.status).toBe("disabled");
    expect(instant(row.disabled_at)).toBe(disabledAt);
    expect(["Ada", "Grace"]).toContain(row.name);
  });

  test("keeps the logical delete effect while racing an ordinary Todo update", async () => {
    const userId = crypto.randomUUID();
    const todoId = crypto.randomUUID();
    const createdAt = "2026-10-03T12:00:00.000Z";
    const deletedAt = "2026-10-03T12:01:00.000Z";
    await persistence.withOperationTime(createdAt).create_User({
      id: userId,
      name: "Ada",
      batch_group: "default",
      status: "active",
      disabled_at: null,
      updated_at: createdAt,
    });
    await persistence.withOperationTime(createdAt).create_Todo({
      id: todoId,
      user_id: userId,
      title: "Seed",
      status: "open",
      deleted_at: null,
      updated_at: createdAt,
    });

    const completed = (await persistence
      .withOperationTime("2026-10-03T12:00:30.000Z")
      .update_required_Todo_by_id_set_status(todoId, "completed")) as Record<string, unknown>;
    expect(completed.status).toBe("completed");
    expect(completed.deleted_at).toBeNull();
    expect(await persistence.query_optional_Todo_by_id(todoId)).not.toBeNull();

    const [deleted, renamed] = await Promise.all([
      persistence
        .withOperationTime(deletedAt)
        .transition_required_Todo_by_id_delete(todoId),
      persistence.update_required_Todo_by_id_set_title(todoId, "Raced"),
    ]);
    expect(deleted).not.toBeNull();
    expect(
      renamed === null || (renamed as Record<string, unknown>).title === "Raced",
    ).toBeTrue();
    expect(await persistence.query_optional_Todo_by_id(todoId)).toBeNull();
    expect(
      await persistence.update_required_Todo_by_id_set_title(todoId, "After delete"),
    ).toBeNull();
    expect(
      await persistence
        .withOperationTime("2026-10-03T12:02:00.000Z")
        .transition_required_Todo_by_id_delete(todoId),
    ).toBeNull();

    const repeatedDelete = await handleRequest(
      new Request(`https://lifecycle.test/lifecycle/todos/${todoId}/delete`, {
        method: "POST",
      }),
    );
    expect(repeatedDelete.status).toBe(404);
    expect((await repeatedDelete.json()).error.code).toBe("todo_missing");
    const nestedRename = await handleRequest(
      new Request(`https://lifecycle.test/lifecycle/todos/${todoId}/rename`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ title: "Nested after delete" }),
      }),
    );
    expect(nestedRename.status).toBe(404);
    expect((await nestedRename.json()).error.code).toBe("todo_missing");

    const row = await todoRecord(todoId);
    expect(row?.status).toBe("completed");
    expect(row?.title).toBe(renamed === null ? "Seed" : "Raced");
    expect(instant(row?.deleted_at)).toBe(deletedAt);
    expect(instant(row?.updated_at)).toBe(deletedAt);
  });

  test("retains visible child Todos when their User is disabled", async () => {
    const userId = crypto.randomUUID();
    const todoId = crypto.randomUUID();
    const createdAt = "2026-10-03T13:00:00.000Z";
    await persistence.withOperationTime(createdAt).create_User({
      id: userId,
      name: "Ada",
      batch_group: "default",
      status: "active",
      disabled_at: null,
      updated_at: createdAt,
    });
    await persistence.withOperationTime(createdAt).create_Todo({
      id: todoId,
      user_id: userId,
      title: "Keep me",
      status: "open",
      deleted_at: null,
      updated_at: createdAt,
    });

    await persistence
      .withOperationTime("2026-10-03T13:01:00.000Z")
      .transition_required_User_by_id_disable(userId);

    if (postgres) {
      await expect(
        postgres.begin(async (transaction) => {
          await transaction`SET LOCAL lock_timeout = '2s'`;
          await transaction`DELETE FROM "user" WHERE id = ${userId}`;
        }),
      ).rejects.toThrow("violates foreign key constraint");
    } else {
      const database = new Database(databasePath, { strict: true });
      try {
        database.exec("PRAGMA foreign_keys = ON");
        expect(() =>
          database.query('DELETE FROM "user" WHERE id = ?1').run(userId),
        ).toThrow();
      } finally {
        database.close();
      }
    }

    expect(await persistence.query_optional_Todo_by_id(todoId)).not.toBeNull();
    expect(await persistence.query_optional_User_by_id(userId)).toBeNull();
    expect(
      await persistence.query_required_Todo_with_owner_required_by_id(todoId),
    ).toBeNull();
    expect((await todoRecord(todoId))?.title).toBe("Keep me");
  });

  test("filters disabled owners before the bounded joined page and required child read", async () => {
    const old = "2020-01-01T00:00:00.000Z";
    const disabledUsers = Array.from(
      { length: 501 },
      (_, index) => `00000000-0000-4000-8001-${index.toString(16).padStart(12, "0")}`,
    );
    const activeUser = "00000000-0000-4000-8001-000000000501";
    const users = [...disabledUsers, activeUser];
    const todos = users.map((_, index) =>
      `00000000-0000-4000-8002-${index.toString(16).padStart(12, "0")}`,
    );

    if (postgres) {
      const userParameters: unknown[] = [];
      const userValues = users.map((id, index) => {
        const offset = index * 6;
        userParameters.push(
          id,
          `reminder-owner-${index}`,
          "reminders",
          index === users.length - 1 ? "active" : "disabled",
          index === users.length - 1 ? null : old,
          old,
        );
        return `($${offset + 1}, $${offset + 2}, $${offset + 3}, $${offset + 4}, $${offset + 5}, $${offset + 6})`;
      });
      await postgres.unsafe(
        `INSERT INTO "user" ("id", "name", "batch_group", "status", "disabled_at", "updated_at") VALUES ${userValues.join(", ")}`,
        userParameters,
      );
      const todoParameters: unknown[] = [];
      const todoValues = todos.map((id, index) => {
        const offset = index * 6;
        todoParameters.push(id, users[index], "reminder", "open", null, old);
        return `($${offset + 1}, $${offset + 2}, $${offset + 3}, $${offset + 4}, $${offset + 5}, $${offset + 6})`;
      });
      await postgres.unsafe(
        `INSERT INTO "todo" ("id", "user_id", "title", "status", "deleted_at", "updated_at") VALUES ${todoValues.join(", ")}`,
        todoParameters,
      );
    } else {
      const database = new Database(databasePath, { strict: true });
      try {
        database.exec("PRAGMA foreign_keys = ON; BEGIN IMMEDIATE");
        const insertUser = database.prepare(
          'INSERT INTO "user" ("id", "name", "batch_group", "status", "disabled_at", "updated_at") VALUES (?1, ?2, ?3, ?4, ?5, ?6)',
        );
        const insertTodo = database.prepare(
          'INSERT INTO "todo" ("id", "user_id", "title", "status", "deleted_at", "updated_at") VALUES (?1, ?2, ?3, ?4, ?5, ?6)',
        );
        users.forEach((id, index) =>
          insertUser.run(
            id,
            `reminder-owner-${index}`,
            "reminders",
            index === users.length - 1 ? "active" : "disabled",
            index === users.length - 1 ? null : Date.parse(old),
            Date.parse(old),
          ),
        );
        todos.forEach((id, index) =>
          insertTodo.run(id, users[index], "reminder", "open", null, Date.parse(old)),
        );
        database.exec("COMMIT");
      } catch (error) {
        database.exec("ROLLBACK");
        throw error;
      } finally {
        database.close();
      }
    }

    const capturedSql: string[] = [];
    const originalPrepare = Database.prototype.prepare;
    Database.prototype.prepare = function (sql: string, ...parameters: any[]) {
      if (sql.includes('WITH "parent_page"') || sql.includes('WITH "child_page"')) {
        capturedSql.push(sql);
      }
      return originalPrepare.call(this, sql, ...parameters);
    } as typeof originalPrepare;
    let rows: unknown[];
    try {
      rows = await persistence.query_many_User_with_todos_by_batch_group_order_by_id_asc_include_order_by_id_asc_paginated(
        "reminders",
        500,
        0,
        500,
        0,
      );
    } finally {
      Database.prototype.prepare = originalPrepare;
    }

    expect(rows).toHaveLength(1);
    const [row] = rows as Array<{ parent: { id: string; status: string }; todos: Array<{ id: string; user_id: string }> }>;
    expect(row.parent).toMatchObject({ id: activeUser, status: "active" });
    expect(row.todos).toHaveLength(1);
    expect(row.todos[0]).toMatchObject({ id: todos[users.length - 1], user_id: activeUser });
    expect(JSON.stringify(rows)).not.toContain(disabledUsers[0]);
    if (!postgres) {
      expect(capturedSql).toHaveLength(1);
      const parentLimit = capturedSql[0].indexOf('ORDER BY "id" ASC LIMIT ?2');
      expect(capturedSql[0].indexOf('AND ("status" = \'active\')')).toBeGreaterThan(-1);
      expect(capturedSql[0].indexOf('AND ("status" = \'active\')')).toBeLessThan(parentLimit);
    }

    const joinedSql: string[] = [];
    const originalPrepareForReminder = Database.prototype.prepare;
    if (!postgres) {
      Database.prototype.prepare = function (sql: string, ...parameters: any[]) {
        if (sql.includes('WITH "child_page"')) joinedSql.push(sql);
        return originalPrepareForReminder.call(this, sql, ...parameters);
      } as typeof originalPrepareForReminder;
    }
    let reminderResponse: Response;
    try {
      reminderResponse = await handleRequest(
        new Request("https://lifecycle.test/lifecycle/reminders", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ title: "reminder" }),
        }),
      );
    } finally {
      Database.prototype.prepare = originalPrepareForReminder;
    }
    expect(reminderResponse.status).toBe(200);
    const reminderRows = (await reminderResponse.json()) as Array<{
      parent: { id: string };
      owner: { id: string; status: string };
    }>;
    expect(reminderRows).toHaveLength(1);
    expect(reminderRows[0]).toMatchObject({
      parent: { id: todos[users.length - 1] },
      owner: { id: activeUser, status: "active" },
    });
    expect(JSON.stringify(reminderRows)).not.toContain(disabledUsers[0]);
    if (!postgres) {
      expect(joinedSql).toHaveLength(1);
      const joinedQuery = joinedSql[0];
      const pageLimit = joinedQuery.indexOf("LIMIT ?2");
      const childVisibility = joinedQuery.indexOf('AND ("status" = \'active\')');
      const ownerVisibility = joinedQuery.indexOf(
        'AND EXISTS (SELECT 1 FROM "user" WHERE "user"."id" = "todo"."user_id" AND ("status" = \'active\')',
      );
      expect(joinedQuery).toContain("INNER JOIN \"user\" AS \"owner_page\"");
      expect(childVisibility).toBeGreaterThan(-1);
      expect(ownerVisibility).toBeGreaterThan(-1);
      expect(childVisibility).toBeLessThan(pageLimit);
      expect(ownerVisibility).toBeLessThan(pageLimit);
    }
  });

  test("filters deleted Todos before the bounded visible page", async () => {
    const userId = "00000000-0000-4000-8004-000000000001";
    const createdAt = new Date(Date.now() - 48 * 60 * 60 * 1000).toISOString();
    const recentlyDeletedAt = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString();
    const deletedIds = Array.from(
      { length: 100 },
      (_, index) => `00000000-0000-4000-8004-${(index + 2).toString().padStart(12, "0")}`,
    );
    const visibleIds = Array.from(
      { length: 100 },
      (_, index) => `00000000-0000-4000-8004-${(index + 102).toString().padStart(12, "0")}`,
    );
    await persistence.withOperationTime(createdAt).create_User({
      id: userId,
      name: "Page owner",
      batch_group: "visible-page",
      status: "active",
      disabled_at: null,
      updated_at: createdAt,
    });
    await seedTodos(deletedIds, userId, recentlyDeletedAt);
    await seedTodos(visibleIds, userId, null, "open");

    const capturedSql: string[] = [];
    const originalPrepare = Database.prototype.prepare;
    if (!postgres) {
      Database.prototype.prepare = function (sql: string, ...parameters: any[]) {
        if (sql.includes('FROM "todo"') && sql.includes('LIMIT ?2')) capturedSql.push(sql);
        return originalPrepare.call(this, sql, ...parameters);
      } as typeof originalPrepare;
    }
    let rows: unknown[];
    try {
      rows = await persistence.query_many_Todo_by_user_id_order_by_id_asc_paginated(
        userId,
        100,
        0,
      );
    } finally {
      Database.prototype.prepare = originalPrepare;
    }

    expect(rows).toHaveLength(100);
    expect((rows as Array<{ id: string; status: string }>).map((row) => row.id)).toEqual(
      visibleIds,
    );
    expect((rows as Array<{ status: string }>).every((row) => row.status === "open")).toBeTrue();
    if (!postgres) {
      expect(capturedSql).toHaveLength(1);
      const visibility = capturedSql[0].indexOf('AND ("deleted_at" IS NULL)');
      const pageLimit = capturedSql[0].indexOf("LIMIT ?2");
      expect(visibility).toBeGreaterThan(-1);
      expect(visibility).toBeLessThan(pageLimit);
    }
  });

  test("purges only expired post-transition rows in audited batches of 500", async () => {
    Bun.env.JADPO_LIFECYCLE_RETENTION = "-1d";
    await expect(runLifecycleMaintenance()).rejects.toThrow(
      "Retention must be positive: config.soft_delete_retention",
    );
    Bun.env.JADPO_LIFECYCLE_RETENTION = "30d";

    const ids = Array.from(
      { length: 501 },
      (_, index) => `00000000-0000-4000-8000-${index.toString(16).padStart(12, "0")}`,
    );
    const userId = crypto.randomUUID();
    const old = "2020-01-01T00:00:00.000Z";
    await persistence.withOperationTime(old).create_User({
      id: userId,
      name: "Purge owner",
      batch_group: "purge",
      status: "active",
      disabled_at: null,
      updated_at: old,
    });
    await seedTodos(ids, userId, old);
    const liveId = crypto.randomUUID();
    const youngId = crypto.randomUUID();
    const recent = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString();
    await seedTodo(
      liveId,
      userId,
      "live-business-state",
      "open",
      null,
      old,
    );
    await seedTodo(youngId, userId, "young-deletion", "open", recent, recent);
    await insertTodoReference(
      "ffffffff-ffff-4fff-8fff-ffffffffffff",
      ids[499],
    );

    const auditEvents: Array<Record<string, unknown>> = [];
    const originalError = console.error;
    console.error = (...values: unknown[]) => {
      try {
        auditEvents.push(JSON.parse(values.map(String).join(" ")) as Record<string, unknown>);
      } catch {
        // Ignore unrelated console output.
      }
    };
    try {
      await expect(runLifecycleMaintenance()).rejects.toThrow();
    } finally {
      console.error = originalError;
    }
    expect(auditEvents).toHaveLength(1);
    expect(auditEvents[0]).toMatchObject({
      kind: "maintenance_audit",
      eventName: "retention.purge.batch",
      operation: "retention_purge(Todo)",
      entity: "Todo",
      retentionBinding: "config.soft_delete_retention",
      selectedCount: 500,
      committedCount: 0,
      bound: 500,
      outcome: "rolled_back",
    });
    expect(auditEvents[0].rowIdentityTokens).toHaveLength(500);
    expect(JSON.stringify(auditEvents[0])).not.toContain(ids[499]);
    expect(await expiredTodoIds()).toEqual(ids);
    expect(await todoExists(liveId)).toBeTrue();
    expect(await todoExists(youngId)).toBeTrue();

    await removeTodoReference("ffffffff-ffff-4fff-8fff-ffffffffffff");
    const captureAudit = async () => {
      const events: Array<Record<string, unknown>> = [];
      console.error = (...values: unknown[]) => {
        try {
          events.push(JSON.parse(values.map(String).join(" ")) as Record<string, unknown>);
        } catch {
          // Ignore unrelated console output.
        }
      };
      try {
        await runLifecycleMaintenance();
      } finally {
        console.error = originalError;
      }
      return events[0];
    };
    const firstBatch = await captureAudit();
    expect(firstBatch).toMatchObject({
      selectedCount: 500,
      committedCount: 500,
      bound: 500,
      outcome: "committed",
    });
    expect(await expiredTodoIds()).toEqual([ids[500]]);
    const finalBatch = await captureAudit();
    expect(finalBatch).toMatchObject({
      selectedCount: 1,
      committedCount: 1,
      bound: 500,
      outcome: "committed",
    });
    expect(await expiredTodoIds()).toEqual([]);
    expect(await todoExists(liveId)).toBeTrue();
    expect(await todoExists(youngId)).toBeTrue();
  });

  test("serializes competing retention-eligibility changes with purge", async () => {
    const ownerId = crypto.randomUUID();
    const todoId = crypto.randomUUID();
    const old = "1900-01-01T00:00:00.000Z";
    await persistence.withOperationTime(old).create_User({
      id: ownerId,
      name: "Eligibility race owner",
      batch_group: "purge-eligibility-race",
      status: "active",
      disabled_at: null,
      updated_at: old,
    });
    await seedTodos([todoId], ownerId, old);

    const events: Array<Record<string, unknown>> = [];
    const originalError = console.error;
    console.error = (...values: unknown[]) => {
      try {
        const event = JSON.parse(values.map(String).join(" ")) as Record<string, unknown>;
        if (event.kind === "maintenance_audit") events.push(event);
      } catch {
        // Ignore unrelated console output.
      }
    };

    if (postgres) {
      try {
        const maintenance = runLifecycleMaintenance();
        const update = postgres.unsafe(
          'UPDATE "todo" SET "deleted_at" = NULL WHERE "id" = $1 RETURNING "id"',
          [todoId],
        );
        const [_, updatedRows] = await Promise.all([maintenance, update]);
        expect(updatedRows.length === 1 || updatedRows.length === 0).toBeTrue();
        if (updatedRows.length === 1) {
          expect(await todoRecord(todoId)).toMatchObject({ deleted_at: null });
          expect(events[0]).toMatchObject({ selectedCount: 0, committedCount: 0 });
        } else {
          expect(await todoExists(todoId)).toBeFalse();
          expect(events[0]).toMatchObject({ selectedCount: 1, committedCount: 1 });
        }
      } finally {
        console.error = originalError;
      }
    } else {
      const contender = new Database(databasePath, { strict: true });
      let updateBlocked = false;
      let updateChanges = 0;
      const maintenance = runLifecycleMaintenance();
      try {
        contender.exec("PRAGMA busy_timeout = 0");
        try {
          const result = contender
            .query('UPDATE "todo" SET "deleted_at" = NULL WHERE "id" = ?1')
            .run(todoId);
          updateChanges = result.changes;
        } catch (error) {
          if (!String(error).includes("locked")) throw error;
          updateBlocked = true;
        }
        await maintenance;
        if (updateBlocked) {
          const result = contender
            .query('UPDATE "todo" SET "deleted_at" = NULL WHERE "id" = ?1')
            .run(todoId);
          expect(result.changes).toBe(0);
          expect(await todoExists(todoId)).toBeFalse();
          expect(events[0]).toMatchObject({ selectedCount: 1, committedCount: 1 });
        } else if (updateChanges === 1) {
          expect(await todoRecord(todoId)).toMatchObject({ deleted_at: null });
          expect(events[0]).toMatchObject({ selectedCount: 0, committedCount: 0 });
        } else {
          expect(await todoExists(todoId)).toBeFalse();
          expect(events[0]).toMatchObject({ selectedCount: 1, committedCount: 1 });
        }
      } finally {
        contender.close();
        console.error = originalError;
      }
    }

    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({
      kind: "maintenance_audit",
      entity: "Todo",
      bound: 500,
      outcome: "committed",
    });
  });
});
