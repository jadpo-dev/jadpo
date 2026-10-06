import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { SQL } from "bun";

const databaseUrl = Bun.env.DATABASE_URL;
if (!databaseUrl) {
  throw new Error("DATABASE_URL is required for the PostgreSQL runtime test");
}

const sql = new SQL(databaseUrl);
const { handleRequest } = await import(
  "../../examples/persistence-seed/build/target/app.ts"
);
const { persistence } = await import(
  "../../examples/persistence-seed/build/target/persistence.ts"
);

let server: ReturnType<typeof Bun.serve>;

beforeAll(async () => {
  await sql`DROP INDEX IF EXISTS customer_email_unique_test`;
  await sql`DELETE FROM "atomic_item"`;
  await sql`DELETE FROM "account"`;
  await sql`DELETE FROM "note"`;
  await sql`DELETE FROM "todo"`;
  await sql`DELETE FROM "user"`;
  await sql`DELETE FROM "customer"`;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const port = 30_000 + Math.floor(Math.random() * 20_000);
    try {
      server = Bun.serve({ port, fetch: handleRequest });
      return;
    } catch {
      // A parallel process won this port; try another bounded candidate.
    }
  }
  throw new Error("could not allocate a PostgreSQL-test HTTP port");
});

afterAll(async () => {
  server?.stop(true);
  await sql.close();
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

async function accountRows(): Promise<
  Array<{ id: string; handle: string; tenant: string; owner_email: string }>
> {
  return (await sql`
    SELECT "id", "handle", "tenant", "owner_email"
    FROM "account"
    ORDER BY "id"
  `) as Array<{
    id: string;
    handle: string;
    tenant: string;
    owner_email: string;
  }>;
}

async function rows(): Promise<Array<{ id: string; email: string }>> {
  return (await sql`
    SELECT "id", "email"
    FROM "customer"
    ORDER BY "id"
  `) as Array<{ id: string; email: string }>;
}

describe("generated PostgreSQL persistence path", () => {
  test("loads multiple bounded collections without Cartesian multiplication", async () => {
    const first = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fc001",
      email: "multi-first@example.com",
      group: "multi-include",
    };
    const second = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fc002",
      email: "multi-second@example.com",
      group: "multi-include",
    };
    await persistence.create_User(first);
    await persistence.create_User(second);
    for (let index = 1; index <= 3; index += 1) {
      await persistence.create_Todo({
        id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7fc10${index}`,
        owner_id: first.id,
        title: `Todo ${index}`,
      });
      await persistence.create_Note({
        id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7fc20${index}`,
        owner_id: first.id,
        body: `Note ${index}`,
      });
    }
    await persistence.create_Note({
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fc301",
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
      first_id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa101",
      first_key: "same-key",
      second_id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa102",
      second_key: "same-key",
    };

    const failed = await createAtomicPair(pair);
    expect(failed.status).toBe(409);
    expect(await failed.json()).toMatchObject({
      error: { code: "atomic_pair_conflict" },
    });
    const [afterFailure] = (await sql`
      SELECT COUNT(*)::int AS count FROM "atomic_item"
    `) as Array<{ count: number }>;
    expect(afterFailure.count).toBe(0);

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
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa111",
      handle: "account-source",
      tenant: "tenant-one",
      owner_email: "source@example.com",
    };
    const second = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7fa112",
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
    expect(await accountRows()).toContainEqual(updated);

    const compoundFailure = await updateAccount({
      id: updated.id,
      handle: "must-not-commit",
      owner_email: second.owner_email,
    });
    expect(compoundFailure.status).toBe(409);
    expect(await compoundFailure.json()).toMatchObject({
      error: { code: "account_tenant_owner_taken" },
    });
    expect(await accountRows()).toContainEqual(updated);
  });

  test("enforces and indexes the generated User-to-Todo relationship", async () => {
    let orphanRejected = false;
    try {
      await sql`
        INSERT INTO "todo" ("id", "owner_id", "title")
        VALUES (
          '018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d01',
          '018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d99',
          'Orphan'
        )
      `;
    } catch {
      orphanRejected = true;
    }
    expect(orphanRejected).toBe(true);

    const userId = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d02";
    await sql`INSERT INTO "user" ("id", "email", "group") VALUES (${userId}, 'relationship@example.com', 'relationship')`;
    await sql`
      INSERT INTO "todo" ("id", "owner_id", "title")
      VALUES ('018f47a2-5b7c-4d91-8ba2-1d3c5e7f9d03', ${userId}, 'Prove relationship')
    `;
    const indexes = (await sql`
      SELECT indexname
      FROM pg_indexes
      WHERE schemaname = current_schema() AND tablename = 'todo'
    `) as Array<{ indexname: string }>;
    expect(indexes.map((index) => index.indexname)).toContain(
      "todo_owner_id_idx",
    );

    await sql`DELETE FROM "user" WHERE "id" = ${userId}`;
    const remaining = await sql`SELECT "id" FROM "todo" WHERE "owner_id" = ${userId}`;
    expect(remaining).toHaveLength(0);
  });

  test("loads a validated parent and deterministically ordered inverse collection", async () => {
    const userId = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e01";
    const user = {
      id: userId,
      email: "ordered-todos@example.com",
      group: "ordered",
    };
    await sql`INSERT INTO "user" ("id", "email", "group") VALUES (${userId}, 'ordered-todos@example.com', 'ordered')`;
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
    for (const todo of todos) {
      await sql`
        INSERT INTO "todo" ("id", "owner_id", "title")
        VALUES (${todo.id}, ${todo.owner_id}, ${todo.title})
      `;
    }

    const response = await listUserTodos(userId);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({
      parent: user,
      todos: [...todos].reverse(),
    });

    const page = (await persistence.query_required_User_with_todos_by_id_order_by_id_asc(
      userId,
      1,
      1,
    )) as { todos: typeof todos };
    expect(page.todos.map((todo) => todo.id)).toEqual([todos[0].id]);
  });

  test("returns an empty inverse collection and a typed missing-parent failure", async () => {
    const user = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9e06",
      email: "empty-todos@example.com",
      group: "empty",
    };
    await sql`INSERT INTO "user" ("id", "email", "group") VALUES (${user.id}, ${user.email}, ${user.group})`;

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

  test("paginates parents before joining independently bounded children", async () => {
    const group = "parent-page";
    const users = [1, 2, 3].map((number) => ({
      id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7faa0${number}`,
      email: `parent-page-${number}@example.com`,
      group,
    }));
    for (const user of users) {
      await sql`INSERT INTO "user" ("id", "email", "group") VALUES (${user.id}, ${user.email}, ${user.group})`;
    }
    const todos = [1, 2, 3].map((number) => ({
      id: `018f47a2-5b7c-4d91-8ba2-1d3c5e7fab0${number}`,
      owner_id: users[0].id,
      title: `First user ${number}`,
    }));
    for (const todo of todos) {
      await sql`INSERT INTO "todo" ("id", "owner_id", "title") VALUES (${todo.id}, ${todo.owner_id}, ${todo.title})`;
    }

    const response = await listGroupUserTodos(group);
    expect(response.status).toBe(200);
    const body = (await response.json()) as Array<{
      parent: (typeof users)[number];
      todos: typeof todos;
    }>;
    expect(body.map((entry) => entry.parent.id)).toEqual([
      users[0].id,
      users[1].id,
    ]);
    expect(body[0].todos).toHaveLength(2);
    expect(body[1].todos).toEqual([]);
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
    expect(await rows()).toEqual([]);
  });

  test("creates, decodes, and returns a validated entity", async () => {
    const customer = {
      id: "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9a01",
      email: "person@example.com",
    };
    const response = await createCustomer(customer);

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual(customer);
    expect(await rows()).toEqual([customer]);
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
    expect(await rows()).toContainEqual(updated);
  });

  test("rolls back an update whose predicate matches multiple rows", async () => {
    const id = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b03";
    const response = await updateCustomer({
      id,
      email: "must-not-commit@example.com",
    });

    expect(response.status).toBe(500);
    const matches = (await rows()).filter((row) => row.id === id);
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
    await sql`
      CREATE UNIQUE INDEX customer_email_unique_test
      ON "customer" ("email")
      WHERE "email" != 'duplicate@example.com'
    `;

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
    expect(await rows()).toContainEqual(first);
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
    expect(await rows()).not.toContainEqual(customer);
  });

  test("rolls back a delete whose predicate matches multiple rows", async () => {
    const id = "018f47a2-5b7c-4d91-8ba2-1d3c5e7f9b03";
    const response = await deleteCustomer(id);

    expect(response.status).toBe(500);
    expect((await rows()).filter((row) => row.id === id)).toHaveLength(2);
  });

  test("normalises raw PostgreSQL driver errors at the adapter boundary", async () => {
    const { persistence, PersistenceFault } = await import(
      "../../examples/persistence-seed/build/target/persistence.ts"
    );
    await sql`DROP TABLE "customer"`;

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
