# Query syntax: proposals

Input to [area 6, Reads and writes](../06-reads-and-writes.md), questions 1 to
4. The syntax review ranks the query syntax third and fourth of its top ten
changes: it is the part of Jadpo with the most special cases.

## What is complicated today

One read of the golden todo app, a page of a user's todos, needs four
declarations and eleven clauses:

```text
query page_todos(input: ListTodos, principal: Principal.user)
freshness: authoritative
    -> TodoPage
{
    return attempt query page Todo -> TodoPage {
        where: owner_id == Todo.owner_id(principal.user_id)
        and: deleted_at == none
        and: status matches optional input.status
        and: due_at <= optional input.due_before
        order_by: created_at desc, id desc
        after: optional input.after
        limit: input.page_size
        project: TodoView
        cursor: TodoCursor(created_at, id)
    }
}
// plus TodoCursor, TodoPage and TodoView declared by hand
```

The difficulty is in how many rules there are, not in any one of them:

- **Two levels.** A query is a callable with a statement body, and inside it
  sits a read expression with its own cardinality word (`required`,
  `optional`, `many`, `page`). Each of the four has its own grammar: only
  `page` takes more than one condition, only `required` takes `missing:`,
  only `many` and `page` take an order.
- **Three spellings of "optional".** `matches optional`, `<= optional` and
  `after: optional` each mean "skip this when the request left it out".
- **Hand-written plumbing.** The cursor type repeats the order fields, the
  page type is the same two fields every time, and `project:` names a type
  the result type already implies.
- **Bare fields.** `where: id == id` compares a column with a parameter of
  the same name and reads as always true.
- **Includes.** Four kinds (child list, one child, parent, two hops), each
  with its own result-shape rule and `parent:` field; 24 of the area's 71
  compiler errors are about includes.
- **Paging.** `limit:` may come from a request in a page but must be a
  literal in a list, `offset:` is required in lists, and a list without
  `limit:` is unbounded.

## How other frameworks do it

| Framework | Shape of one read | Optional filters | Pagination | Related records |
|---|---|---|---|---|
| Ecto (Elixir) | `from t in Todo, where: ..., order_by: ..., limit: ...`; several `where:` are joined with `and` | Build the query in steps | Offset; keyset via libraries | `preload:`, one extra query per association |
| Prisma (TypeScript) | `findMany({ where, orderBy, take, cursor, include })` | Pass `undefined` to drop a condition | Cursor; needs a unique tie-break such as `id` | `include:`, nested, with its own `where`, `orderBy`, `take` |
| EdgeQL (Gel) | `select Todo { title } filter ... order by ... limit ...` | Coalescing expressions | Offset | Nested shapes, each with its own `filter`, `order by`, `limit` |
| Rails | `Todo.where(...).order(...).limit(...)` chains | `where(...) if param` in code | Offset; cursor via gems | `includes`; N+1 caught by `strict_loading` |
| GraphQL Relay | Fields with arguments | Arguments | Opaque cursor connections (`first`, `after`, `pageInfo`) | Nested fields |

Three lessons recur. Ecto shows that a keyword list of clauses, with
repeated `where:` meaning "and", reads well and stays declarative. Prisma and
Relay show that cursor pagination needs a unique tie-break and that the
cursor is best opaque to the client. EdgeQL and Prisma show related records
as a nested block with the same clauses as the outer read.

## Options

### A. Finish today's direction

The syntax review's successor candidate keeps the two levels and moves the
cardinality into the block:

```text
var page: TodoPage = attempt query(Todo) {
    cardinality: page
    into: TodoPage
    where: Todo.owner_id == owner_id
    where_present: Todo.status == input.status
    order_by: [Todo.created_at desc, Todo.id desc]
    after: input.after
    limit: input.page_size
    project: TodoView
    cursor: TodoCursor
}
```

Every clause is now `name: value`, but the reader still learns a callable
around a read, four cardinalities with different required clauses, and the
cursor and page types.

### B. A query is one declarative read (recommended)

The query file is the read. Its result type says how many records come back,
the compiler builds the page and cursor, and composition happens in actions:

```text
query todos_for_owner(owner_id: User.id, filter: ListTodos) -> Page<TodoView> {
    from: Todo
    where: Todo.owner_id == owner_id
    where_present: Todo.status == filter.status
    where_present: Todo.due_at <= filter.due_before
    order: [Todo.created_at desc]
    page: {
        size: filter.page_size
        after: filter.after
    }
}
```

| Result type | Returns | Needs |
|---|---|---|
| `T` | exactly one | `where:` on a unique field, `missing:` |
| `T?` | zero or one | `where:` on a unique field |
| `List<T>` | bounded list | `order:`, `limit:` |
| `Page<T>` | a page and its cursor | `order:`, `page:` |

Includes use the same clauses one level down:

```text
include: {
    owner: Project.owner_id
    todos: {
        via: Todo.project_id
        where: Todo.status == TodoStatus.open
        order: [Todo.created_at asc]
        limit: 50
    }
}
```

What it removes: the statement body, the cardinality words, `and:`,
`matches optional`, `<= optional`, `after: optional`, `project:`, `cursor:`,
hand-written cursor and page types, `offset:`, `freshness:`, `inverse`
declarations, the `parent:` result field, and the deterministic-order errors
(the compiler adds `id`).

What it costs: a read that needs logic (combining two reads, computing a
summary) moves into the action that calls it. That is where it belongs
anyway: area 3 lets routes call one action, and actions already compose.
Counts and sums need a later clause (`count:`), which is deferred.

### C. An expression language

Reads as expressions anywhere in a query body, in the style of LINQ or
EdgeQL:

```text
return Todo
    .where(todo -> todo.owner_id == owner_id)
    .order(todo -> todo.created_at, desc)
    .page(filter.page_size, filter.after)
```

The most flexible, and the most familiar to agents trained on TypeScript and
C#. It is also the hardest to review and to analyse: the compiler must prove
from arbitrary expressions which indexes a read needs, which records a cache
depends on, and that every read is bounded, and area 3 allows inline
functions only as standard-library collection arguments.

## Trade-offs

| | A. Successor | B. Declarative read | C. Expressions |
|---|---|---|---|
| Concepts to learn | Callable, read, 4 cardinalities, cursor and page types | One block; result type sets the count | Method chain and lambdas |
| Reviewer reads | Clauses inside code | Clauses only | Code |
| Compiler derives indexes, cache scopes, row bounds | Yes | Yes, directly | Only for a restricted subset |
| Logic inside a read | Yes | No; in the calling action | Yes |
| Change from today | Rename clauses | Rewrite every query once | New sub-language |

## Recommendation

Option B. It turns the largest special-case area of the language into one
block whose clauses are all `name: value`, it gives the compiler everything
it needs for indexes (area 5), caches (area 5) and the build report (area
14) without analysis, and its only loss, logic inside a read, has a home in
actions. Supporting choices, each an open question on the page:

1. Qualified fields in every condition; stacked `where:` lines; comparisons,
   `and`, `or`, `not`, `== none` and `in`; `where_present:` for optional
   filters.
2. `order:` as a list with `id` added as the tie-break; every list bounded
   by a literal or a type's `max:`; keyset `Page<T>` with an opaque
   `PageCursor` that records its query; no offset.
3. Includes keyed by a reference field: this entity's reference gives the
   parent, another entity's reference gives children. Two levels at most,
   one extra database query per include, within the domain and store.
4. A query or `get` inside a `for` loop is a compile error; read the set
   with `in`.

## Sources

- Ecto query keyword syntax, repeated `where` and `preload`:
  <https://ecto.hexdocs.pm/Ecto.Query.html>
- Prisma pagination, cursors and the unique tie-break:
  <https://www.prisma.io/docs/orm/prisma-client/queries/pagination>
- EdgeQL nested shapes with their own filter, order and limit:
  <https://docs.geldata.com/reference/edgeql/select>
- GraphQL cursor connections:
  <https://relay.dev/graphql/connections.htm>
- Rails `strict_loading` against one query per record:
  <https://guides.rubyonrails.org/active_record_querying.html#strict-loading>
- Rails optimistic locking (`lock_version`, `StaleObjectError`):
  <https://api.rubyonrails.org/classes/ActiveRecord/Locking/Optimistic.html>
- PostgreSQL isolation levels, relative updates (`balance + 100`) and
  retrying serialization failures:
  <https://www.postgresql.org/docs/current/transaction-iso.html>
- JSON Merge Patch, where an empty patch changes nothing:
  <https://www.rfc-editor.org/rfc/rfc7396>
