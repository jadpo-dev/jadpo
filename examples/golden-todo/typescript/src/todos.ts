import { v7 as uuidv7 } from "uuid";
import { and, asc, desc, eq, lt, or } from "drizzle-orm";
import type { z } from "zod";
import type { Store, Database } from "./db/store.ts";
import { todos, users } from "./db/schema.ts";
import { AppError } from "./errors.ts";
import { todoScope, userId, self } from "./policy.ts";
import type { Clock, RequestScope } from "./runtime.ts";
import {
  Cursor,
  TodoPage,
  TodoView,
  UserWithTodos,
  CreateTodo,
  PatchTodo,
} from "./schemas.ts";
import type {
  Principal,
  PublicTodo,
  TodoChanges,
  ListTodos,
} from "./schemas.ts";

const projection = {
  id: todos.id,
  title: todos.title,
  status: todos.status,
  due_at: todos.due_at,
  created_at: todos.created_at,
  updated_at: todos.updated_at,
};
const iso = (value: string) => new Date(value).toISOString();
function view(
  row:
    | typeof todos.$inferSelect
    | {
        id: string;
        title: string;
        status: string;
        due_at: string | null;
        created_at: string;
        updated_at: string;
      },
): PublicTodo {
  return TodoView.parse({
    id: row.id,
    title: row.title,
    status: row.status,
    due_at: row.due_at === null ? null : iso(row.due_at),
    created_at: iso(row.created_at),
    updated_at: iso(row.updated_at),
  });
}
function decodeCursor(value?: string) {
  if (!value) return undefined;
  try {
    return Cursor.parse(
      JSON.parse(Buffer.from(value, "base64url").toString("utf8")),
    );
  } catch {
    throw new AppError("invalid_input");
  }
}
export class TodoService {
  readonly store: Store;
  readonly clock: Clock;
  constructor(store: Store, clock: Clock) {
    this.store = store;
    this.clock = clock;
  }
  private due(due: string | null | undefined): void {
    if (due !== null && due !== undefined && Date.parse(due) < this.clock())
      throw new AppError("due_date_in_past");
  }
  async create(
    principal: Principal,
    input: z.infer<typeof CreateTodo>,
    scope: RequestScope,
  ) {
    input = CreateTodo.parse(input);
    const owner = userId(principal);
    this.due(input.due_at);
    return await this.store.transaction(scope, async (db) => {
      const now = new Date(this.clock()).toISOString();
      const [row] = await db
        .insert(todos)
        .values({
          id: uuidv7(),
          owner_id: owner,
          title: input.title,
          status: "open",
          due_at: input.due_at ?? null,
          reminder_revision: 1n,
          created_at: now,
          updated_at: now,
        })
        .returning(projection);
      if (!row) throw new AppError("internal_fault");
      return view(row); // Parse before transaction commits; excess/private fields cannot escape.
    });
  }
  async get(principal: Principal, id: string, scope: RequestScope) {
    scope.check();
    const [row] = await this.store.db
      .select(projection)
      .from(todos)
      .where(and(todoScope(principal), eq(todos.id, id)))
      .limit(1);
    scope.check();
    if (!row) throw new AppError("todo_not_found");
    return view(row);
  }
  async list(
    principal: Principal,
    input: z.infer<typeof ListTodos>,
    scope: RequestScope,
  ) {
    const cursor = decodeCursor(input.after);
    scope.check();
    const rows = await this.store.db
      .select(projection)
      .from(todos)
      .where(
        and(
          todoScope(principal),
          input.status ? eq(todos.status, input.status) : undefined,
          input.due_before ? lt(todos.due_at, input.due_before) : undefined,
          cursor
            ? or(
                lt(todos.created_at, cursor.created_at),
                and(
                  eq(todos.created_at, cursor.created_at),
                  lt(todos.id, cursor.id),
                ),
              )
            : undefined,
        ),
      )
      .orderBy(desc(todos.created_at), desc(todos.id))
      .limit(input.page_size + 1);
    scope.check();
    const items = rows.slice(0, input.page_size).map(view);
    const last = items.at(-1);
    const next =
      rows.length > input.page_size && last
        ? Buffer.from(
            JSON.stringify({ created_at: last.created_at, id: last.id }),
          ).toString("base64url")
        : null;
    return TodoPage.parse({ items, next });
  }
  async patch(
    principal: Principal,
    id: string,
    changes: TodoChanges,
    scope: RequestScope,
  ) {
    changes = PatchTodo.parse(changes);
    this.due(changes.due_at);
    return await this.store.transaction(scope, async (db) => {
      const [before] = await db
        .select()
        .from(todos)
        .where(and(todoScope(principal), eq(todos.id, id)))
        .limit(1);
      if (!before) throw new AppError("todo_not_found");
      if (Object.keys(changes).length === 0) return view(before);
      const dueSupplied = Object.hasOwn(changes, "due_at");
      const values = {
        ...changes,
        updated_at: new Date(this.clock()).toISOString(),
        ...(dueSupplied
          ? {
              reminder_revision: before.reminder_revision + 1n,
              reminder_sent_at: null,
              reminder_queued_at: null,
            }
          : {}),
      };
      const [after] = await db
        .update(todos)
        .set(values)
        .where(and(todoScope(principal), eq(todos.id, id)))
        .returning(projection);
      if (!after) throw new AppError("todo_not_found");
      return view(after);
    });
  }
  async remove(
    principal: Principal,
    id: string,
    scope: RequestScope,
  ): Promise<void> {
    await this.store.transaction(scope, async (db) => {
      const now = new Date(this.clock()).toISOString();
      const rows = await db
        .update(todos)
        .set({ deleted_at: now, updated_at: now })
        .where(and(todoScope(principal), eq(todos.id, id)))
        .returning({ id: todos.id });
      if (!rows.length) throw new AppError("todo_not_found");
    });
  }
  async userTodos(principal: Principal, id: string, scope: RequestScope) {
    self(principal, id, "read");
    scope.check();
    const [parent] = await this.store.db
      .select({ id: users.id })
      .from(users)
      .where(eq(users.id, id))
      .limit(1);
    if (!parent) throw new AppError("user_not_found");
    scope.check();
    const children = await this.store.db
      .select(projection)
      .from(todos)
      .where(todoScope(principal))
      .orderBy(asc(todos.created_at), asc(todos.id))
      .limit(100);
    scope.check();
    return UserWithTodos.parse({ id: parent.id, todos: children.map(view) });
  }
  async disable(
    principal: Principal,
    id: string,
    scope: RequestScope,
  ): Promise<void> {
    self(principal, id, "delete");
    await this.store.transaction(scope, async (db) => {
      const changed = await db
        .update(users)
        .set({ status: "disabled" })
        .where(eq(users.id, id))
        .returning({ id: users.id });
      if (!changed.length) throw new AppError("user_not_found");
    });
  }
}
// Restricted public type used by the actual negative compiler probe.
export type MutationDatabase = Database;
