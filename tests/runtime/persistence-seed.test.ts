import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const temporaryDirectory = mkdtempSync(join(tmpdir(), "jadpo-persistence-"));
const databasePath = join(temporaryDirectory, "application.sqlite");
delete Bun.env.DATABASE_URL;
Bun.env.SQLITE_PATH = databasePath;

const { handleRequest } = await import(
  "../../examples/persistence-seed/build/target/app.ts"
);
const { persistence } = await import(
  "../../examples/persistence-seed/build/target/persistence.ts"
);

let server: ReturnType<typeof Bun.serve>;

beforeAll(() => {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const port = 30_000 + Math.floor(Math.random() * 20_000);
    try {
      server = Bun.serve({ port, fetch: handleRequest });
      return;
    } catch {
      // A parallel process won this port; try another bounded candidate.
    }
  }
  throw new Error("could not allocate a persistence-test HTTP port");
});

afterAll(() => {
  server?.stop(true);
  rmSync(temporaryDirectory, { recursive: true, force: true });
});

async function createCustomer(body: unknown): Promise<Response> {
  return fetch(new URL("/customers", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function findCustomer(id: string): Promise<Response> {
  return fetch(new URL("/customers/find", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id }),
  });
}

async function requireCustomer(id: string): Promise<Response> {
  return fetch(new URL("/customers/require", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id }),
  });
}

async function updateCustomer(body: unknown): Promise<Response> {
  return fetch(new URL("/customers/update", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function createPatchItem(body: unknown): Promise<Response> {
  return fetch(new URL("/patch-items", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function patchItem(id: string, changes: unknown): Promise<Response> {
  return fetch(new URL("/patch-items/patch", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id, changes }),
  });
}

async function getPatchItemReviewer(id: string): Promise<Response> {
  return fetch(new URL("/patch-items/reviewer", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id }),
  });
}

async function deleteCustomer(id: string): Promise<Response> {
  return fetch(new URL("/customers/delete", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id }),
  });
}

async function listUserTodos(userId: string): Promise<Response> {
  return fetch(new URL("/users/todos", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id: userId }),
  });
}

async function getTodoOwner(todoId: string): Promise<Response> {
  return fetch(new URL("/todos/owner", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id: todoId }),
  });
}

async function getTodoOwnerProfile(todoId: string): Promise<Response> {
  return fetch(new URL("/todos/owner-profile", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id: todoId }),
  });
}

async function getUserProfile(userId: string): Promise<Response> {
  return fetch(new URL("/users/profile", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id: userId }),
  });
}

async function listGroupUserTodos(group: string): Promise<Response> {
  return fetch(new URL("/users/todos/page", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ group }),
  });
}

async function listUserActivity(userId: string): Promise<Response> {
  return fetch(new URL("/users/activity", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ id: userId }),
  });
}

async function listGroupUserActivity(group: string): Promise<Response> {
  return fetch(new URL("/users/activity/page", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ group }),
  });
}

async function createAtomicPair(body: unknown): Promise<Response> {
  return fetch(new URL("/atomic-pairs", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function createAccount(body: unknown): Promise<Response> {
  return fetch(new URL("/accounts", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function updateAccount(body: unknown): Promise<Response> {
  return fetch(new URL("/accounts/update", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

function accountRows(): Array<{
  id: string;
  handle: string;
  tenant: string;
  owner_email: string;
}> {
  const database = new Database(databasePath, { readonly: true, strict: true });
  try {
    return database
      .query("SELECT id, handle, tenant, owner_email FROM account ORDER BY id")
      .all() as Array<{
      id: string;
      handle: string;
      tenant: string;
      owner_email: string;
    }>;
  } finally {
    database.close();
  }
}

function rows(): Array<{ id: string; email: string }> {
  const database = new Database(databasePath, { readonly: true, strict: true });
  try {
    return database
      .query("SELECT id, email FROM customer ORDER BY id")
      .all() as Array<{ id: string; email: string }>;
  } finally {
    database.close();
  }
}

describe("generated SQLite persistence path", () => {
  test("preserves omission separately from explicit none in a fixed-shape patch", async () => {
    const original = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa090",
      title: "Original title",
      note: "Original note",
      marker: "Original marker",
      reviewer_id: null,
    };
    const created = await createPatchItem(original);
    expect(created.status).toBe(200);
    expect(await created.json()).toEqual(original);

    const absentReviewer = await getPatchItemReviewer(original.id);
    expect(absentReviewer.status).toBe(200);
    expect(await absentReviewer.json()).toEqual({
      parent: original,
      reviewer_id: null,
    });

    const reviewer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa091",
      email: "patch-reviewer@example.com",
      group: "patch-reviewers",
    };
    await persistence.create_User(reviewer);
    const reviewedItem = {
      ...original,
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa092",
      reviewer_id: reviewer.id,
    };
    const reviewedCreated = await createPatchItem(reviewedItem);
    expect(reviewedCreated.status).toBe(200);
    const includedReviewer = await getPatchItemReviewer(reviewedItem.id);
    expect(includedReviewer.status).toBe(200);
    expect(await includedReviewer.json()).toEqual({
      parent: reviewedItem,
      reviewer_id: reviewer,
    });

    const empty = await patchItem(original.id, {});
    expect(empty.status).toBe(422);
    expect(await empty.json()).toMatchObject({
      error: { code: "empty_patch_item" },
    });

    const cleared = await patchItem(original.id, { note: null });
    expect(cleared.status).toBe(200);
    expect(await cleared.json()).toEqual({
      ...original,
      note: null,
    });

    const titleOnly = await patchItem(original.id, { title: "Changed title" });
    expect(titleOnly.status).toBe(200);
    expect(await titleOnly.json()).toEqual({
      ...original,
      title: "Changed title",
      note: null,
      marker: null,
    });

    const both = await patchItem(original.id, {
      title: "Final title",
      note: "Final note",
    });
    expect(both.status).toBe(200);
    expect(await both.json()).toEqual({
      ...original,
      title: "Final title",
      note: "Final note",
      marker: null,
    });
  });

  test("loads multiple bounded collections without Cartesian multiplication", async () => {
    const first = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fb001",
      email: "multi-first@example.com",
      group: "multi-include",
    };
    const second = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fb002",
      email: "multi-second@example.com",
      group: "multi-include",
    };
    await persistence.create_User(first);
    await persistence.create_User(second);
    for (let index = 1; index <= 3; index += 1) {
      await persistence.create_Todo({
        id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7fb10${index}`,
        owner_id: first.id,
        title: `Todo ${index}`,
      });
      await persistence.create_Note({
        id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7fb20${index}`,
        owner_id: first.id,
        body: `Note ${index}`,
      });
    }
    await persistence.create_Note({
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fb301",
      owner_id: second.id,
      body: "Second user note",
    });

    const required = await listUserActivity(first.id);
    expect(required.status).toBe(200);
    expect(await required.json()).toMatchObject({
      parent: first,
      todos: [{ title: "Todo 1" }, { title: "Todo 2" }],
      notes: [{ body: "Note 1" }, { body: "Note 2" }],
    });

    const page = await listGroupUserActivity("multi-include");
    expect(page.status).toBe(200);
    const activity = (await page.json()) as Array<{
      parent: typeof first;
      todos: unknown[];
      notes: unknown[];
    }>;
    expect(activity).toHaveLength(2);
    expect(activity[0]).toMatchObject({ parent: first });
    expect(activity[0].todos).toHaveLength(2);
    expect(activity[0].notes).toHaveLength(2);
    expect(activity[1]).toMatchObject({ parent: second, todos: [] });
    expect(activity[1].notes).toHaveLength(1);
  });

  test("automatically rolls back every mutation in a failed action", async () => {
    const pair = {
      first_id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa001",
      first_key: "same-key",
      second_id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa002",
      second_key: "same-key",
    };

    const failed = await createAtomicPair(pair);
    expect(failed.status).toBe(409);
    expect(await failed.json()).toMatchObject({
      error: { code: "atomic_pair_conflict" },
    });

    const database = new Database(databasePath, { readonly: true, strict: true });
    try {
      const afterFailure = database
        .query("SELECT COUNT(*) AS count FROM atomic_item")
        .get() as { count: number };
      expect(afterFailure.count).toBe(0);
    } finally {
      database.close();
    }

    const succeeded = await createAtomicPair({
      ...pair,
      first_key: "first-key",
      second_key: "second-key",
    });
    expect(succeeded.status).toBe(200);
    expect(await succeeded.json()).toEqual({
      first: { id: pair.first_id, key: "first-key" },
      second: { id: pair.second_id, key: "second-key" },
    });
  });

  test("updates several fields atomically in one fixed-shape mutation", async () => {
    const first = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa011",
      handle: "account-source",
      tenant: "tenant-one",
      owner_email: "source@example.com",
    };
    const second = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa012",
      handle: "account-target",
      tenant: "tenant-one",
      owner_email: "target@example.com",
    };
    expect((await createAccount(first)).status).toBe(200);
    expect((await createAccount(second)).status).toBe(200);

    const updated = {
      ...first,
      handle: "account-updated",
      owner_email: "updated@example.com",
    };
    const succeeded = await updateAccount({
      id: updated.id,
      handle: updated.handle,
      owner_email: updated.owner_email,
    });
    expect(succeeded.status).toBe(200);
    expect(await succeeded.json()).toEqual(updated);

    const failed = await updateAccount({
      id: updated.id,
      handle: second.handle,
      owner_email: "must-not-commit@example.com",
    });
    expect(failed.status).toBe(409);
    expect(await failed.json()).toMatchObject({
      error: { code: "account_handle_taken" },
    });
    expect(accountRows()).toContainEqual(updated);

    const compoundFailure = await updateAccount({
      id: updated.id,
      handle: "must-not-commit",
      owner_email: second.owner_email,
    });
    expect(compoundFailure.status).toBe(409);
    expect(await compoundFailure.json()).toMatchObject({
      error: { code: "account_tenant_owner_taken" },
    });
    expect(accountRows()).toContainEqual(updated);
  });

  test("enforces generated identity, uniqueness, and lookup indexes", () => {
    const database = new Database(databasePath, { strict: true });
    try {
      const columns = database
        .query("PRAGMA table_info('account')")
        .all() as Array<{ name: string; pk: number }>;
      expect(columns.find((column) => column.name === "id")?.pk).toBe(1);

      const indexes = database
        .query("PRAGMA index_list('account')")
        .all() as Array<{ name: string; unique: number }>;
      expect(indexes).toContainEqual(
        expect.objectContaining({
          name: "account_owner_email_idx",
          unique: 0,
        }),
      );
      expect(indexes.some((index) => index.unique === 1)).toBe(true);

      const insert = database.query(
        "INSERT INTO account (id, handle, tenant, owner_email) VALUES (?1, ?2, ?3, ?4)",
      );
      insert.run(
        "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9c01",
        "first-handle",
        "schema-tenant",
        "first@example.com",
      );
      expect(() =>
        insert.run(
          "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9c01",
          "second-handle",
          "schema-tenant",
          "second@example.com",
        ),
      ).toThrow();
      expect(() =>
        insert.run(
          "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9c02",
          "first-handle",
          "schema-tenant",
          "third@example.com",
        ),
      ).toThrow();
    } finally {
      database.close();
    }
  });

  test("enforces and indexes the generated User-to-Todo relationship", async () => {
    const database = new Database(databasePath, { strict: true });
    try {
      const todoIndexes = database
        .query("PRAGMA index_list('todo')")
        .all() as Array<{ name: string; unique: number }>;
      expect(todoIndexes).toContainEqual(
        expect.objectContaining({ name: "todo_owner_id_idx", unique: 0 }),
      );

      let orphanRejected = false;
      try {
        await persistence.create_Todo({
          id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d01",
          owner_id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d99",
          title: "Orphan",
        });
      } catch {
        orphanRejected = true;
      }
      expect(orphanRejected).toBe(true);

      const userId = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d02";
      await persistence.create_User({
        id: userId,
        email: "relationship@example.com",
        group: "relationship",
      });
      await persistence.create_Todo({
        id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d03",
        owner_id: userId,
        title: "Prove relationship",
      });
      await persistence.delete_required_User_by_id(userId);

      const remaining = database
        .query("SELECT COUNT(*) AS count FROM todo WHERE owner_id = ?1")
        .get(userId) as { count: number };
      expect(remaining.count).toBe(0);
    } finally {
      database.close();
    }
  });

  test("loads an optional inverse only when its owning reference is unique", async () => {
    const user = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d11",
      email: "profile@example.com",
      group: "profile",
    };
    await persistence.create_User(user);

    const absent = await getUserProfile(user.id);
    expect(absent.status).toBe(200);
    expect(await absent.json()).toEqual({ parent: user, profile: null });

    const todo = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d14",
      owner_id: user.id,
      title: "Nested relationship",
    };
    await persistence.create_Todo(todo);
    const nestedAbsent = await getTodoOwnerProfile(todo.id);
    expect(nestedAbsent.status).toBe(200);
    expect(await nestedAbsent.json()).toEqual({
      parent: todo,
      owner: { parent: user, profile: null },
    });

    const profile = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d12",
      user_id: user.id,
      display_name: "Profile Owner",
    };
    await persistence.create_UserProfile(profile);

    const present = await getUserProfile(user.id);
    expect(present.status).toBe(200);
    expect(await present.json()).toEqual({ parent: user, profile });

    const nestedPresent = await getTodoOwnerProfile(todo.id);
    expect(nestedPresent.status).toBe(200);
    expect(await nestedPresent.json()).toEqual({
      parent: todo,
      owner: { parent: user, profile },
    });

    let duplicateRejected = false;
    try {
      await persistence.create_UserProfile({
        ...profile,
        id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d13",
      });
    } catch {
      duplicateRejected = true;
    }
    expect(duplicateRejected).toBe(true);

    const manifest = await Bun.file(
      new URL(
        "../../examples/persistence-seed/build/persistence/entities.json",
        import.meta.url,
      ),
    ).json();
    expect(manifest.query_plans).toContainEqual(
      expect.objectContaining({
        parent: "User",
        relationship: "profile",
        child: "UserProfile",
        strategy: "bounded_optional_inverse",
        query_count: 2,
        parent_cardinality: "required",
        child_cardinality: "optional",
        foreign_field: "UserProfile.user_id",
        target_field: "User.id",
      }),
    );
    expect(manifest.query_plans).toContainEqual(
      expect.objectContaining({
        root: "Todo",
        path: ["owner", "profile"],
        leaf: "UserProfile",
        strategy: "bounded_nested_lookup",
        query_count: 3,
        maximum_depth: 2,
        root_cardinality: "required",
        middle_cardinality: "required",
        leaf_cardinality: "optional",
        leaf_target_field: "User.id",
      }),
    );
  });

  test("loads a validated parent and ordered inverse collection with a bounded plan", async () => {
    const userId = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e01";
    const user = {
      id: userId,
      email: "ordered-todos@example.com",
      group: "ordered",
    };
    await persistence.create_User({
      ...user,
    });
    const todos = [
      {
        id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e03",
        owner_id: userId,
        title: "Third",
      },
      {
        id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e02",
        owner_id: userId,
        title: "Second",
      },
    ];
    for (const todo of todos) await persistence.create_Todo(todo);

    const ownerResponse = await getTodoOwner(todos[0].id);
    expect(ownerResponse.status).toBe(200);
    expect(await ownerResponse.json()).toEqual({
      parent: todos[0],
      owner: user,
    });

    const otherUserId = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e04";
    await persistence.create_User({
      id: otherUserId,
      email: "other@example.com",
      group: "other",
    });
    await persistence.create_Todo({
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e05",
      owner_id: otherUserId,
      title: "Must not leak",
    });

    const response = await listUserTodos(userId);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      parent: user,
      todos: [...todos].reverse(),
    });

    const firstPage = (await persistence.query_required_User_with_todos_by_id_order_by_id_asc(
      userId,
      1,
      0,
    )) as { todos: typeof todos };
    const secondPage = (await persistence.query_required_User_with_todos_by_id_order_by_id_asc(
      userId,
      1,
      1,
    )) as { todos: typeof todos };
    expect(firstPage.todos.map((todo) => todo.id)).toEqual([todos[1].id]);
    expect(secondPage.todos.map((todo) => todo.id)).toEqual([todos[0].id]);

    const emptyResponse = await listUserTodos(otherUserId);
    expect(emptyResponse.status).toBe(200);
    expect((await emptyResponse.json()).todos).toHaveLength(1);

    const manifest = await Bun.file(
      new URL(
        "../../examples/persistence-seed/build/persistence/entities.json",
        import.meta.url,
      ),
    ).json();
    expect(manifest.query_plans).toContainEqual(
      expect.objectContaining({
        parent: "User",
        relationship: "todos",
        strategy: "bounded_batch",
        query_count: 2,
        child_sql: expect.stringContaining("LIMIT $2 OFFSET $3"),
      }),
    );
    expect(manifest.query_plans).toContainEqual(
      expect.objectContaining({
        parent: "User",
        relationship: "todos",
        strategy: "parent_page_join",
        query_count: 1,
        parent_pagination_before_join: true,
        parameters: {
          predicate: 1,
          parent_limit: 2,
          parent_offset: 3,
          child_limit: 4,
          child_offset: 5,
        },
      }),
    );
    expect(manifest.query_plans).toContainEqual(
      expect.objectContaining({
        child: "Todo",
        relationship: "owner",
        parent: "User",
        strategy: "bounded_parent_lookup",
        child_cardinality: "required",
        parent_cardinality: "required",
      }),
    );
    expect(manifest.query_plans).toContainEqual(
      expect.objectContaining({
        child: "PatchItem",
        relationship: "reviewer_id",
        parent: "User",
        strategy: "bounded_parent_lookup",
        query_count: 2,
        child_cardinality: "required",
        parent_cardinality: "optional",
        foreign_field: "PatchItem.reviewer_id",
        target_field: "User.id",
      }),
    );
  });

  test("returns an empty inverse collection and a typed missing-parent failure", async () => {
    const user = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e06",
      email: "empty-todos@example.com",
      group: "empty",
    };
    await persistence.create_User(user);

    const empty = await listUserTodos(user.id);
    expect(empty.status).toBe(200);
    expect(await empty.json()).toEqual({ parent: user, todos: [] });

    const missing = await listUserTodos(
      "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e99",
    );
    expect(missing.status).toBe(404);
    expect(await missing.json()).toMatchObject({
      error: { code: "user_not_found" },
    });
  });

  test("paginates parents before joining their independently bounded children", async () => {
    const group = "parent-page";
    const users = [1, 2, 3].map((number) => ({
      id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7faa0${number}`,
      email: `parent-page-${number}@example.com`,
      group,
    }));
    for (const user of users) await persistence.create_User(user);
    const firstUserTodos = [1, 2, 3].map((number) => ({
      id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7fab0${number}`,
      owner_id: users[0].id,
      title: `First user ${number}`,
    }));
    for (const todo of firstUserTodos) await persistence.create_Todo(todo);
    await persistence.create_Todo({
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fac01",
      owner_id: users[1].id,
      title: "Second user",
    });

    const response = await listGroupUserTodos(group);
    expect(response.status).toBe(200);
    const body = (await response.json()) as Array<{
      parent: (typeof users)[number];
      todos: typeof firstUserTodos;
    }>;
    expect(body.map((entry) => entry.parent.id)).toEqual([
      users[0].id,
      users[1].id,
    ]);
    expect(body[0].todos.map((todo) => todo.id)).toEqual([
      firstUserTodos[0].id,
      firstUserTodos[1].id,
    ]);
    expect(body[1].todos).toHaveLength(1);

    const nextPage = (await persistence.query_many_User_with_todos_by_group_order_by_id_asc_include_order_by_id_asc_paginated(
      group,
      2,
      2,
      2,
      0,
    )) as Array<{ parent: (typeof users)[number]; todos: unknown[] }>;
    expect(nextPage.map((entry) => entry.parent.id)).toEqual([users[2].id]);
    expect(nextPage[0].todos).toEqual([]);
  });

  test("rejects invalid input without writing a row", async () => {
    const response = await createCustomer({
      id: "not-a-uuid",
      email: "not-an-email",
    });

    expect(response.status).toBe(422);
    expect(await response.json()).toMatchObject({
      error: { code: "invalid_input" },
    });
    expect(rows()).toEqual([]);
  });

  test("creates, decodes, and returns a validated entity", async () => {
    const customer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9a01",
      email: "person@example.com",
    };
    const response = await createCustomer(customer);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(customer);
    expect(rows()).toEqual([customer]);
  });

  test("returns null when an optional query has no row", async () => {
    const response = await findCustomer(
      "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b01",
    );

    expect(response.status).toBe(200);
    expect(await response.json()).toBeNull();
  });

  test("returns a validated entity when an optional query has one row", async () => {
    const customer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b02",
      email: "query@example.com",
    };
    expect((await createCustomer(customer)).status).toBe(200);

    const response = await findCustomer(customer.id);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(customer);
  });

  test("contains an optional-query cardinality violation", async () => {
    const customer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b03",
      email: "duplicate@example.com",
    };
    expect((await createCustomer(customer)).status).toBe(200);
    expect((await createCustomer(customer)).status).toBe(200);

    const response = await findCustomer(customer.id);

    expect(response.status).toBe(500);
    const body = await response.json();
    expect(body).toMatchObject({
      error: { code: "internal_fault" },
    });
    expect(JSON.stringify(body)).not.toContain("optional query");
  });

  test("maps a missing required query to its declared not-found failure", async () => {
    const response = await requireCustomer(
      "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b04",
    );

    expect(response.status).toBe(404);
    const body = await response.json();
    expect(body).toMatchObject({
      error: {
        code: "customer_not_found",
        message: "Customer not found.",
      },
    });
    expect(JSON.stringify(body)).not.toContain("customer_id");
  });

  test("returns a validated entity from a required query", async () => {
    const customer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b05",
      email: "required@example.com",
    };
    expect((await createCustomer(customer)).status).toBe(200);

    const response = await requireCustomer(customer.id);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(customer);
  });

  test("maps a missing required update to not-found", async () => {
    const response = await updateCustomer({
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b06",
      email: "missing-update@example.com",
    });

    expect(response.status).toBe(404);
    expect(await response.json()).toMatchObject({
      error: { code: "customer_not_found" },
    });
  });

  test("updates and returns exactly one validated entity", async () => {
    const customer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b07",
      email: "before-update@example.com",
    };
    expect((await createCustomer(customer)).status).toBe(200);

    const updated = { ...customer, email: "after-update@example.com" };
    const response = await updateCustomer(updated);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(updated);
    expect(rows()).toContainEqual(updated);
  });

  test("rolls back an update whose predicate matches multiple rows", async () => {
    const id = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b03";
    const response = await updateCustomer({
      id,
      email: "must-not-commit@example.com",
    });

    expect(response.status).toBe(500);
    const matches = rows().filter((row) => row.id === id);
    expect(matches).toHaveLength(2);
    expect(matches.every((row) => row.email === "duplicate@example.com")).toBe(
      true,
    );
  });

  test("maps create and update constraint violations to their declared conflict", async () => {
    const first = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b08",
      email: "conflict-source@example.com",
    };
    const second = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b09",
      email: "conflict-target@example.com",
    };
    expect((await createCustomer(first)).status).toBe(200);
    expect((await createCustomer(second)).status).toBe(200);
    const database = new Database(databasePath, { strict: true });
    database.exec(
      "CREATE UNIQUE INDEX customer_email_unique_test ON customer(email) WHERE email != 'duplicate@example.com'",
    );
    database.close();

    const createResponse = await createCustomer({
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9c03",
      email: second.email,
    });
    expect(createResponse.status).toBe(409);
    expect(await createResponse.json()).toMatchObject({
      error: { code: "customer_mutation_conflict" },
    });

    const response = await updateCustomer({
      id: first.id,
      email: second.email,
    });

    expect(response.status).toBe(409);
    expect(await response.json()).toMatchObject({
      error: { code: "customer_mutation_conflict" },
    });
    expect(rows()).toContainEqual(first);
  });

  test("maps a missing required delete to not-found", async () => {
    const response = await deleteCustomer(
      "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b10",
    );

    expect(response.status).toBe(404);
    expect(await response.json()).toMatchObject({
      error: { code: "customer_not_found" },
    });
  });

  test("deletes and returns exactly one validated entity", async () => {
    const customer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b11",
      email: "delete-me@example.com",
    };
    expect((await createCustomer(customer)).status).toBe(200);

    const response = await deleteCustomer(customer.id);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(customer);
    expect(rows()).not.toContainEqual(customer);
  });

  test("rolls back a delete whose predicate matches multiple rows", async () => {
    const id = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b03";
    const response = await deleteCustomer(id);

    expect(response.status).toBe(500);
    expect(rows().filter((row) => row.id === id)).toHaveLength(2);
  });

  test("normalises raw SQLite driver errors at the adapter boundary", async () => {
    const { persistence, PersistenceFault } = await import(
      "../../examples/persistence-seed/build/target/persistence.ts"
    );
    const database = new Database(databasePath, { strict: true });
    database.exec("DROP TABLE customer");
    database.close();

    try {
      await persistence.query_optional_Customer_by_id(
        "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b06",
      );
      throw new Error("expected persistence query to fail");
    } catch (error) {
      expect(error).toBeInstanceOf(PersistenceFault);
      expect((error as InstanceType<typeof PersistenceFault>).operation).toBe(
        "query.Customer.id",
      );
      expect((error as InstanceType<typeof PersistenceFault>).kind).toBe(
        "driver",
      );
      expect((error as Error).cause).toBeDefined();
    }
  });
});
