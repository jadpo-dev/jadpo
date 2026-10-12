---
title: Reads and writes
area: 6
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - data.atomic_domain_mismatch
  - data.consistency_invalid
  - data.consistency_required
  - data.freshness_invalid
  - data.mutation_guard_invalid
  - data.mutation_owner_required
  - data.query_freshness_required
  - data.raw_query_outside_named_query
  - data.value_receiver_guard_required
  - syntax.expected_delete_action
  - syntax.expected_order_direction
  - syntax.expected_patch_input
  - syntax.expected_supplied
  - syntax.expected_supplied_field
  - syntax.expected_update_body
  - syntax.mutation_conflict_required
  - syntax.query_page_after_required
  - syntax.query_page_cursor_required
  - syntax.query_page_predicate_operator
  - syntax.query_page_project_required
  - type.conflict_duplicate_binding
  - type.conflict_unknown_constraint
  - type.create_not_entity
  - type.delete_not_entity
  - type.include_duplicate_relationship
  - type.include_mixed_cardinality
  - type.include_order_not_deterministic
  - type.include_parent_pagination_required
  - type.include_required_parent
  - type.include_result_mismatch
  - type.include_result_not_output
  - type.include_result_shape
  - type.include_unknown_relationship
  - type.inverse_one_cardinality
  - type.inverse_one_required_query
  - type.inverse_one_result_shape
  - type.inverse_one_single
  - type.mutation_nullable_field_unsupported
  - type.mutation_unknown_predicate_field
  - type.nested_include_cardinality
  - type.nested_include_first_hop
  - type.nested_include_nullable_first_hop
  - type.nested_include_required_query
  - type.nested_include_result_shape
  - type.nested_include_second_hop
  - type.nested_include_single
  - type.parent_include_nullable_reference
  - type.parent_include_required_query
  - type.parent_include_result_shape
  - type.parent_include_single
  - type.parent_include_unknown_reference
  - type.patch_condition_binding
  - type.patch_condition_unknown_field
  - type.patch_derived_overlap
  - type.patch_field_not_optional
  - type.patch_field_required
  - type.patch_input_binding
  - type.patch_not_input
  - type.patch_unknown_field
  - type.query_not_entity
  - type.query_nullable_field_unsupported
  - type.query_order_not_deterministic
  - type.query_page_shape
  - type.query_pagination_constant_required
  - type.query_pagination_range
  - type.query_unknown_field
  - type.query_unknown_order_field
  - type.update_duplicate_field
  - type.update_field_required
  - type.update_not_entity
  - type.update_unknown_field
---

# Reads and writes

How a program reads and changes stored records: named queries, filters,
order and pages, children and parents, the operations every entity gets,
what a failed write reports, and how an action stays all-or-nothing while
other requests change the same records. Where records are stored, and how
caches copy them, is area 5; who may read or change them is area 9.

> Proposed. Code on this page shows the language after the rework, applying the decisions from areas 1 to 5 and this page's recommendations; none of it compiles yet. Today's compiled syntax is collapsed beneath each section. The query syntax is compared with other frameworks in [the query proposals](research/06-reads-and-writes-proposals.md).

The examples follow the golden todo app's `planning` domain: `User`,
`Project` and `Todo`.

## Named queries

Every read is a named query, one per file in its domain's `queries/` folder
(decided in areas 1 and 5). A query is one read: which entity, which
records, in what order, and how many. The result type says how many records
come back, so there is no separate word for it:

| Result type | Returns | Needs |
|---|---|---|
| `TodoView` | exactly one record | a `where:` on a unique field, and `missing:` |
| `TodoView?` | zero or one | a `where:` on a unique field |
| `List<TodoView>` | up to a fixed number | `order:` and `limit:` |
| `Page<TodoView>` | one page and a cursor for the next | `order:` and `page:` |

The element type is the entity or a type derived from it
(`type TodoView = Output from Todo { pick: [...] }`, area 2); the compiler
reads only the fields that type needs. Records hidden by the entity's
lifecycle are left out of every query. A query writes nothing and calls no
actions (area 3). It is called like an action: bare in its own domain,
`planning.todos_for_owner(...)` from another domain when it is `public`.

```jadpo fragment
// domains/planning/queries/todos_for_owner.jadpo
/// The owner's visible todos, newest first, one page at a time.
query todos_for_owner(owner_id: User.id, filter: ListTodos) -> Page<TodoView> {
    from: Todo
    where: Todo.owner_id == owner_id
    where_present: Todo.status == filter.status
    order: [Todo.created_at desc]
    page: {
        size: filter.page_size
        after: filter.after
    }
}
```

::: today

Queries sit inside the entity with a required `freshness:` setting. The body
is ordinary statements around a read expression that names its own
cardinality (`required`, `optional`, `many` or `page`):

```jadpo project=today-query
failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

entity Todo {
    id: Uuid { generated: identity }
    title: Text

    identity: id

    persistence {
        store: primary
        role: authority
    }

    query by_id(todo_id: Todo.id)
    freshness: authoritative
        fails TodoNotFound
        -> Todo
    {
        return attempt query required Todo {
            where: id == todo_id
            missing: TodoNotFound
        }
    }
}
```

:::

## Filters

Each `where:` line is one condition, and a record must meet all of them.
Fields are named with their entity, `Todo.status`, so `where: Todo.id ==
todo_id` cannot be misread as comparing a value with itself. A condition can
use comparisons, `and`, `or`, `not`, `== none` for an empty field, and `in`
with a list. `where_present:` applies its condition only when the value on
the right is present, which is how a request's optional filters are written.

```jadpo fragment
where: Todo.owner_id == owner_id
where: Todo.due_at == none or Todo.due_at > clock.now
where: Todo.status in [TodoStatus.open, TodoStatus.waiting]
where_present: Todo.due_at <= filter.due_before
```

A condition must be one the database can use an index for; a function
applied to a field is refused. The compiler creates the indexes (area 5).

::: today

Only page queries can test more than one field, by chaining `and:`, and
optional filters use `matches optional` and `<= optional`. Every other query
accepts exactly one `field == value`, and a nullable field cannot be tested
at all. The page example is under the next section.

:::

## Order and pages

`order:` lists fields, each with `asc` or `desc`. When the order could tie,
the compiler adds `id` as the last field, so equal values never swap places
between pages. Every list is bounded: `limit:` is a number, or a value whose
type has a `max:`. A page uses keyset pagination: `Page<T>` is a standard
type holding `items` and `next`, an opaque `PageCursor` the client sends
back as `after:` to get the next page. There is no offset, because a large
offset is slow and skips or repeats records when rows are added.

```jadpo fragment
type ListTodos = Input {
    status: TodoStatus {
        optional: true
    }
    due_before: Instant {
        optional: true
    }
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}
```

A cursor records which query made it, so a cursor from another query is
rejected as invalid input.

::: today

The page shape is spelled out by hand: a cursor type repeating the order
fields, a page type, `project:`, and `cursor:` written like a constructor
call. `limit:` may come from the request here but must be a literal
everywhere else, and `offset:` is required on lists and includes.

```jadpo project=today-page
enum TodoStatus {
    open
    completed
}

type PageSize = Int {
    min: 1
    max: 100
}

entity Todo {
    id: Uuid { generated: identity }
    owner_id: Uuid
    status: TodoStatus
    created_at: Instant generated { on: create }

    identity: id

    persistence {
        store: primary
        role: authority
    }

    query page_todos(owner_id: Todo.owner_id, input: ListTodos)
    freshness: authoritative
        -> TodoPage
    {
        return attempt query page Todo -> TodoPage {
            where: owner_id == owner_id
            and: status matches optional input.status
            order_by: created_at desc, id desc
            after: optional input.after
            limit: input.page_size
            project: TodoView
            cursor: TodoCursor(created_at, id)
        }
    }
}

input ListTodos {
    status: Todo.status optional
    page_size: PageSize default 25
    after: TodoCursor optional
}

value TodoCursor {
    created_at: Todo.created_at
    id: Todo.id
}

output TodoView {
    id: Todo.id
    status: Todo.status
}

output TodoPage {
    items: List<TodoView>
    next: TodoCursor?
}
```

:::

## Children and parents

An `include:` section adds related records to the result, each under a
field of the result type. Naming one of this entity's own references
(`Project.owner_id`) includes the record it points to. Naming another
entity's reference to this one (`Todo.project_id`) includes the records
that point here: a list, which needs `order:` and `limit:` in a block, or at
most one record when that reference is unique. Parents never declare their
children (area 5).

```jadpo fragment
// domains/planning/queries/project_board.jadpo
query project_board(project_id: Project.id) fails ProjectNotFound -> ProjectBoardView {
    from: Project
    where: Project.id == project_id
    missing: ProjectNotFound
    include: {
        owner: Project.owner_id
        todos: {
            via: Todo.project_id
            where: Todo.status == TodoStatus.open
            order: [Todo.created_at asc]
            limit: 50
        }
    }
}

type ProjectBoardView = Output from Project {
    pick: [id, name]
    owner: UserView
    todos: List<TodoView>
}
```

Each include costs one more database query however many records the result
holds, never one per record. Includes nest up to two levels, stay inside the
domain, and reach another store only to fetch records by `id`. A parent is
included even when its lifecycle hides it, because the reference still
holds (question 10).

::: today

The parent declares each child list as an `inverse`, the include writes
five settings on one line, and the result has a `parent:` field holding the
whole record:

```jadpo project=today-include
failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

entity User {
    id: Uuid { generated: identity }

    identity: id

    persistence {
        store: primary
        role: authority
        inverse todos: many Todo via: Todo.owner_id
    }

    query with_todos(user_id: User.id)
    freshness: authoritative
        fails UserNotFound
        -> UserTodos
    {
        return attempt query required User {
            where: id == user_id
            include: todos into: UserTodos order_by: id asc limit: 100 offset: 0
            missing: UserNotFound
        }
    }
}

entity Todo {
    id: Uuid { generated: identity }
    owner_id: User.id

    identity: id

    persistence {
        store: primary
        role: authority
        references owner_id: User.id as: owner on_delete: cascade
    }
}

output UserTodos {
    parent: User
    todos: List<Todo>
}
```

:::

## The operations every entity gets

Every entity gets these operations, called through the entity (area 1).
Only actions in the entity's own domain may call the ones that write; other
domains call that domain's public actions.

| Operation | Does |
|---|---|
| `Todo.get(todo_id)` | reads one visible record, or fails `TodoNotFound` |
| `Todo.create { set: { ... } }` | adds a record |
| `Todo.update(todo) { set: { ... } }` | changes fields of a record loaded earlier in the action |
| `Todo.update(todo) { patch: input }` | changes only the fields a request supplied |
| `Todo.delete(todo)` | removes the record, or runs the lifecycle's `delete` transition |
| `User.disable(user)` | runs a lifecycle transition, named after it |

`TodoNotFound` is the domain's failure of that name in `failures/`, or one
the compiler provides with kind `not_found` when the domain declares none.
A write that cannot fail needs no `attempt` (area 3).

```jadpo fragment
// domains/planning/actions/reschedule_todo.jadpo
action reschedule_todo(todo_id: Todo.id, due_at: DueAt) fails TodoNotFound, DueDateInPast -> TodoView {
    var todo: Todo = attempt Todo.get(todo_id)
    if (due_at <= clock.now) {
        reject DueDateInPast
    }
    var changed: Todo = Todo.update(todo) {
        set: {
            due_at: due_at
            reminder_sent_at: none
        }
    }
    return TodoView(changed)
}
```

A patch takes an `Input` whose fields are optional and match the entity's:
a field the request left out keeps its value, and one sent as `none` is
cleared (area 2). `when_supplied:` sets further fields when a given field
was supplied. A patch with no fields changes nothing and succeeds, as JSON
Merge Patch does, so there is no `empty:` failure.

```jadpo fragment
var changed: Todo = Todo.update(todo) {
    patch: changes
    when_supplied: {
        due_at: {
            reminder_sent_at: none
        }
    }
}
```

::: today

Writes are expressions that only an action inside the entity may use.
`update` and `delete` find their record with `where:` and must map
`missing:` and `conflict:` every time; `create` puts its mapping after the
closing brace:

```jadpo project=today-write
failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

failure TodoConflict {
    kind: Conflict
    code: "todo_conflict"
}

entity Todo {
    id: Uuid { generated: identity }
    title: Text
    due_at: Instant?
    reminder_sent_at: Instant?

    identity: id

    persistence {
        store: primary
        role: authority
    }

    action add(title: Todo.title)
        fails TodoConflict
        -> Todo
    {
        return attempt create Todo {
            title: title
            due_at: none
            reminder_sent_at: none
        } conflict: TodoConflict
    }

    action edit(todo_id: Todo.id, input: EditTodo)
        fails TodoNotFound, EmptyPatch, TodoConflict
        -> Todo
    {
        return attempt update required Todo {
            where: id == todo_id
            patch: input
            empty: EmptyPatch
            set: {
                reminder_sent_at: none when input.due_at supplied
            }
            missing: TodoNotFound
            conflict: TodoConflict
        }
    }
}

input EditTodo {
    title: Todo.title optional
    due_at: Todo.due_at optional
}
```

:::

## When a write is refused

A write can be refused for two reasons. The record is not there: that
reports the entity's not-found failure, `TodoNotFound`, including a
reference that names a missing record (`ProjectNotFound` for a bad
`project_id`). Or a database rule refuses it: a unique rule, or a `restrict`
reference blocking a delete. Each rule a write can break must be mapped in
its `conflict:` block, keyed by the rule: `Project.name_per_owner` (a named
unique rule), `User.email` (a unique field), `Todo.project_id` (a reference
that blocks deleting a project). The compiler knows which rules every write
can break, so a missing mapping is a compile error and there is no
catch-all. The database's own error is kept as the internal cause (area 4).

```jadpo fragment
var project: Project = attempt Project.create {
    set: {
        owner_id: owner.id
        name: name
    }
    conflict: {
        Project.name_per_owner: ProjectNameTaken
    }
}

attempt Project.delete(project) {
    conflict: {
        Todo.project_id: ProjectHasTodos
    }
}
```

## All-or-nothing actions and concurrent changes

Every action is all-or-nothing (area 3): its writes, and those of the public
actions it calls, commit together or not at all. A write to a record loaded
earlier in the action checks that nobody changed it in between. If someone
did, the runtime throws the attempt away and runs the whole action again,
with fresh reads and a fresh `clock.now`. That is safe because an action
calls no outside service before it commits (areas 11 and 12). After three
attempts within a second, the existing retry budget, the request fails
`unavailable` and the client may try again. Authors write nothing for this.

```text
request A: loads todo 7                 request B: loads todo 7
request A: updates todo 7, commits
                                        request B: updates todo 7: changed since loaded
                                        request B: runs again, loads the new todo 7, updates
```

On PostgreSQL this is the repeatable-read isolation level, whose "could not
serialize" error the runtime already retries. Catching a client that edits
a stale copy across two requests is HTTP's `If-Match` (area 7).

::: today

An action that calls two entities' writing actions must say
`consistency: atomic`, and an entity action that writes a record it was
handed needs `guard: reload` or `guard: revision`. Nothing checks for
concurrent change on PostgreSQL (see Status).

```jadpo project=today-atomic
entity Project {
    id: Uuid { generated: identity }

    identity: id

    persistence {
        store: primary
        role: authority
    }

    action open() -> Project {
        return attempt create Project {}
    }
}

entity Board {
    id: Uuid { generated: identity }

    identity: id

    persistence {
        store: primary
        role: authority
    }

    action open() -> Board {
        return attempt create Board {}
    }
}

action open_project_with_board()
consistency: atomic
    -> Board
{
    var project = Project.open()
    return Board.open()
}
```

:::

## Counters and other one-step updates

When a new value depends only on the stored one, the database works it out
in one step and nothing needs checking. On the right of `set:`,
`Post.view_count` means the value stored at the moment of the write, while
`post.view_count` would be the copy loaded earlier. Such an update takes the
record's `id` instead of a loaded record, and its values may use only
parameters, constants, `clock.now` and the record's stored fields, so it
never reruns the action.

```jadpo fragment
attempt Post.update(post_id) {
    set: {
        view_count: Post.view_count + 1
        last_viewed_at: clock.now
    }
}
```

Today `set:` cannot read the stored value: `Post.view_count` there is an
unknown name.

## Reading in a loop

A query or `Todo.get` inside a `for` loop is a compile error, because it
would read once per item. Read the whole set with one query using `in`.
Writes inside a loop are allowed: the list is bounded and every write joins
the action's transaction. A batch that should keep partial progress is a
schedule or subscriber handling one item per action (area 3).

```jadpo fragment
// domains/planning/queries/todos_by_ids.jadpo
query todos_by_ids(todo_ids: List<Todo.id>) -> List<TodoView> {
    from: Todo
    where: Todo.id in todo_ids
    order: [Todo.created_at asc]
    limit: 100
}
```

## One store per action

An action writes to at most one store (area 5), counting the public actions
it calls. It may read other stores, but those reads are outside its
transaction, and a read that decides a write (a policy, lifecycle or
invariant check) must come from the store being written. Across stores
there are no joins, filters or ordering; an include reaches another store
only to fetch records by `id`. Moving an entity to another store can break
an action that never names it, so the error names the cause:

```text
error[store.action_spans_stores]: checkout writes two stores
  --> domains/ordering/actions/checkout.jadpo:12
   | writes Order (store main) and Cart (store carts)
   = fix: clear the cart in a subscriber to OrderEvent.placed, or keep Cart in main
```

Today only the main store runs, and `consistency: atomic` across two stores
is rejected (`data.atomic_domain_mismatch`).

## Where a read comes from

Reads inside an action, policy and authentication always read the store,
inside the action's transaction, so they are current. A display read is a
query a route runs directly to show data (area 7): it may be served from the
cache its query file declares (area 5). A client still sees its own writes:
every write response carries a revision, and a request carrying a revision
newer than the cache skips it. The build lists every query call with where
it reads from.

| Call | Reads from |
|---|---|
| Inside an action, policy or authentication | the store, in the action's transaction |
| A route's display read, after this client wrote | the store |
| A route's display read, otherwise | the query's cache when declared, up to its `keep:` |

```jadpo fragment
route GET /todos {
    input: ListTodos
    output: Page<TodoView>
    run: todos_for_owner(current_principal.user_id, input)
}
```

## Reads across domains

A query reads its own domain's entities only, and includes follow
references inside the domain. Another domain's data comes from its public
queries, called through that domain like its public actions; reading a list
by ids keeps it to one query.

```jadpo fragment
var owners: List<OwnerView> = identity.users_by_ids(owner_ids)
```

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### Queries

### `type.query_not_entity`

```jadpo project=e-query-not-entity expect=type.query_not_entity
type FilterValue = Text {}

input Filter {
    value: FilterValue
}

action find(value: FilterValue) -> Filter? {
    return attempt query optional Filter {
        where: value == value
    }
}
```

### `type.query_unknown_field`

```jadpo project=e-query-unknown-field expect=type.query_unknown_field
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action find(name: Item.name) -> Item? {
    return attempt query optional Item {
        where: title == name
    }
}
```

### `type.query_nullable_field_unsupported`

```jadpo project=e-query-nullable-field-unsupported expect=type.query_nullable_field_unsupported
entity Customer {
    email: Text?
}

action find_customer(email: Customer.email) -> Customer? {
    return attempt query optional Customer {
        where: email == email
    }
}
```

### `type.query_unknown_order_field`

```jadpo project=e-query-unknown-order-field expect=type.query_unknown_order_field
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action list(name: Item.name) -> List<Item> {
    return attempt query many Item {
        where: name == name
        order_by: created_at asc
    }
}
```

### `type.query_order_not_deterministic`

Goes under question 3, because the compiler would add `id` itself:

```jadpo project=e-query-order-not-deterministic expect=type.query_order_not_deterministic
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action list(name: Item.name) -> List<Item> {
    return attempt query many Item {
        where: name == name
        order_by: name asc
    }
}
```

### `syntax.expected_order_direction`

```jadpo project=e-expected-order-direction expect=syntax.expected_order_direction
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action list(name: Item.name) -> List<Item> {
    return attempt query many Item {
        where: name == name
        order_by: id newest
    }
}
```

### `type.query_pagination_constant_required`

Goes under question 3, which lets a type with a `max:` bound a list:

```jadpo project=e-query-pagination-constant-required expect=type.query_pagination_constant_required
type PageSize = Int {
    min: 1
    max: 100
}

entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action list(name: Item.name, page_size: PageSize) -> List<Item> {
    return attempt query many Item {
        where: name == name
        order_by: id asc
        limit: page_size
        offset: 0
    }
}
```

### `type.query_pagination_range`

```jadpo project=e-query-pagination-range expect=type.query_pagination_range
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action list(name: Item.name) -> List<Item> {
    return attempt query many Item {
        where: name == name
        order_by: id asc
        limit: 0
        offset: 0
    }
}
```

### Pages

### `syntax.query_page_after_required`

```jadpo project=e-query-page-after-required expect=syntax.query_page_after_required
entity Todo {
    id: Uuid identity
}

input ListTodos {
    id: Uuid optional
    after: TodoCursor optional
}

value TodoCursor {
    id: Todo.id
}

output TodoView {
    id: Todo.id
}

output TodoPage {
    items: List<TodoView>
    next: TodoCursor?
}

action list(input: ListTodos) -> TodoPage {
    return attempt query page Todo -> TodoPage {
        where: id == input.id
        order_by: id desc
        limit: 10
        project: TodoView
        cursor: TodoCursor(id)
    }
}
```

### `syntax.query_page_project_required`

```jadpo project=e-query-page-project-required expect=syntax.query_page_project_required
entity Todo {
    id: Uuid identity
}

input ListTodos {
    id: Uuid optional
    after: TodoCursor optional
}

value TodoCursor {
    id: Todo.id
}

output TodoView {
    id: Todo.id
}

output TodoPage {
    items: List<TodoView>
    next: TodoCursor?
}

action list(input: ListTodos) -> TodoPage {
    return attempt query page Todo -> TodoPage {
        where: id == input.id
        order_by: id desc
        after: optional input.after
        limit: 10
        cursor: TodoCursor(id)
    }
}
```

### `syntax.query_page_cursor_required`

```jadpo project=e-query-page-cursor-required expect=syntax.query_page_cursor_required
entity Todo {
    id: Uuid identity
}

input ListTodos {
    id: Uuid optional
    after: TodoCursor optional
}

value TodoCursor {
    id: Todo.id
}

output TodoView {
    id: Todo.id
}

output TodoPage {
    items: List<TodoView>
    next: TodoCursor?
}

action list(input: ListTodos) -> TodoPage {
    return attempt query page Todo -> TodoPage {
        where: id == input.id
        order_by: id desc
        after: optional input.after
        limit: 10
        project: TodoView
    }
}
```

### `syntax.query_page_predicate_operator`

```jadpo project=e-query-page-predicate-operator expect=syntax.query_page_predicate_operator
entity Todo {
    id: Uuid identity
}

input ListTodos {
    id: Uuid optional
    after: TodoCursor optional
}

value TodoCursor {
    id: Todo.id
}

output TodoView {
    id: Todo.id
}

output TodoPage {
    items: List<TodoView>
    next: TodoCursor?
}

action list(input: ListTodos) -> TodoPage {
    return attempt query page Todo -> TodoPage {
        where: id != input.id
        order_by: id desc
        after: optional input.after
        limit: 10
        project: TodoView
        cursor: TodoCursor(id)
    }
}
```

### `type.query_page_shape`

```jadpo project=e-query-page-shape expect=type.query_page_shape
entity Todo {
    id: Uuid identity
}

input ListTodos {
    id: Uuid optional
    after: TodoCursor optional
}

value TodoCursor {
    id: Todo.id
}

output TodoView {
    id: Todo.id
}

output TodoPage {
    items: List<Text>
    next: TodoCursor?
}

action list(input: ListTodos) -> TodoPage {
    return attempt query page Todo -> TodoPage {
        where: id == input.id
        order_by: id desc
        after: optional input.after
        limit: 10
        project: TodoView
        cursor: TodoCursor(id)
    }
}
```

### Child lists

### `type.include_unknown_relationship`

```jadpo project=e-include-unknown-relationship expect=type.include_unknown_relationship
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    todos: List<Todo>
}

action load(user_id: User.id) fails UserNotFound -> UserTodos {
    return attempt query required User {
        where: id == user_id
        include: tasks into: UserTodos order_by: id asc limit: 10 offset: 0
        missing: UserNotFound
    }
}
```

### `type.include_duplicate_relationship`

```jadpo project=e-include-duplicate-relationship expect=type.include_duplicate_relationship
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    todos: List<Todo>
}

action load(user_id: User.id) fails UserNotFound -> UserTodos {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserTodos order_by: id asc limit: 10 offset: 0
        include: todos into: UserTodos order_by: id desc limit: 10 offset: 0
        missing: UserNotFound
    }
}
```

### `type.include_mixed_cardinality`

```jadpo project=e-include-mixed-cardinality expect=type.include_mixed_cardinality
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    todos: List<Todo>
}

action load(user_id: User.id) fails UserNotFound -> UserTodos {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserTodos order_by: id asc limit: 10 offset: 0
        include: profile optional into: UserTodos
        missing: UserNotFound
    }
}
```

### `type.include_order_not_deterministic`

Goes under question 3, like the rule above:

```jadpo project=e-include-order-not-deterministic expect=type.include_order_not_deterministic
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    todos: List<Todo>
}

action load(user_id: User.id) fails UserNotFound -> UserTodos {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserTodos order_by: title asc limit: 10 offset: 0
        missing: UserNotFound
    }
}
```

### `type.include_parent_pagination_required`

```jadpo project=e-include-parent-pagination-required expect=type.include_parent_pagination_required
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    todos: List<Todo>
}

action load(user_id: User.id) -> List<UserTodos> {
    return attempt query many User {
        where: id == user_id
        order_by: id asc
        include: todos into: UserTodos order_by: id asc limit: 10 offset: 0
    }
}
```

### `type.include_required_parent`

```jadpo project=e-include-required-parent expect=type.include_required_parent
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    todos: List<Todo>
}

action load(user_id: User.id) -> UserTodos {
    return attempt query optional User {
        where: id == user_id
        include: todos into: UserTodos order_by: id asc limit: 10 offset: 0
    }
}
```

### `type.include_result_mismatch`

```jadpo project=e-include-result-mismatch expect=type.include_result_mismatch
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    todos: List<Todo>
}

output OtherUserTodos {
    parent: User
    todos: List<Todo>
}

action load(user_id: User.id) fails UserNotFound -> UserTodos {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserTodos order_by: id asc limit: 10 offset: 0
        include: todos into: OtherUserTodos order_by: id asc limit: 10 offset: 0
        missing: UserNotFound
    }
}
```

### `type.include_result_not_output`

```jadpo project=e-include-result-not-output expect=type.include_result_not_output
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

action load(user_id: User.id) fails UserNotFound -> Text {
    return attempt query required User {
        where: id == user_id
        include: todos into: Text order_by: id asc limit: 10 offset: 0
        missing: UserNotFound
    }
}
```

### `type.include_result_shape`

```jadpo project=e-include-result-shape expect=type.include_result_shape
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserTodos {
    parent: User
    items: List<Todo>
}

action load(user_id: User.id) fails UserNotFound -> UserTodos {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserTodos order_by: id asc limit: 10 offset: 0
        missing: UserNotFound
    }
}
```

### One child

### `type.inverse_one_cardinality`

```jadpo project=e-inverse-one-cardinality expect=type.inverse_one_cardinality
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserProfile {
    parent: User
    profile: Profile?
}

action load(user_id: User.id) fails UserNotFound -> UserProfile {
    return attempt query required User {
        where: id == user_id
        include: profile into: UserProfile order_by: id asc limit: 1 offset: 0
        missing: UserNotFound
    }
}
```

### `type.inverse_one_required_query`

```jadpo project=e-inverse-one-required-query expect=type.inverse_one_required_query
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserProfile {
    parent: User
    profile: Profile?
}

action load(user_id: User.id) -> UserProfile {
    return attempt query optional User {
        where: id == user_id
        include: profile optional into: UserProfile
    }
}
```

### `type.inverse_one_result_shape`

```jadpo project=e-inverse-one-result-shape expect=type.inverse_one_result_shape
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserProfile {
    parent: User
    profile: Profile
}

action load(user_id: User.id) fails UserNotFound -> UserProfile {
    return attempt query required User {
        where: id == user_id
        include: profile optional into: UserProfile
        missing: UserNotFound
    }
}
```

### `type.inverse_one_single`

```jadpo project=e-inverse-one-single expect=type.inverse_one_single
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}

entity Todo {
    id: Uuid identity
    title: Text
    owner_id: User.id references User.id on_delete cascade
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

failure UserNotFound {
    kind: NotFound
    code: "user_not_found"
}

output UserProfile {
    parent: User
    profile: Profile?
}

action load(user_id: User.id) fails UserNotFound -> UserProfile {
    return attempt query required User {
        where: id == user_id
        include: profile optional into: UserProfile
        include: profile optional into: UserProfile
        missing: UserNotFound
    }
}
```

### Parents

### `type.parent_include_unknown_reference`

```jadpo project=e-parent-include-unknown-reference expect=type.parent_include_unknown_reference
entity User {
    id: Uuid identity
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
}

output TodoOwner {
    parent: Todo
    owner: User
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwner {
    return attempt query required Todo {
        where: id == todo_id
        include: author required into: TodoOwner
        missing: TodoNotFound
    }
}
```

### `type.parent_include_nullable_reference`

```jadpo project=e-parent-include-nullable-reference expect=type.parent_include_nullable_reference
entity User {
    id: Uuid identity
}

entity PatchItem {
    id: Uuid identity
    reviewer_id: User.id? references User.id on_delete set_null
}

input FindPatchItem {
    id: PatchItem.id
}

output PatchItemReviewer {
    parent: PatchItem
    reviewer_id: User
}

failure PatchItemNotFound {
    kind: NotFound code: "patch_item_not_found" }

action get_patch_item_reviewer(input: FindPatchItem) -> PatchItemReviewer
    fails PatchItemNotFound
{
    return attempt query required PatchItem {
        where: id == input.id
        include: reviewer_id required into: PatchItemReviewer
        missing: PatchItemNotFound
    }
}
```

### `type.parent_include_required_query`

```jadpo project=e-parent-include-required-query expect=type.parent_include_required_query
entity User {
    id: Uuid identity
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
}

output TodoOwner {
    parent: Todo
    owner: User
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) -> TodoOwner {
    return attempt query optional Todo {
        where: id == todo_id
        include: owner required into: TodoOwner
    }
}
```

### `type.parent_include_result_shape`

```jadpo project=e-parent-include-result-shape expect=type.parent_include_result_shape
entity User {
    id: Uuid identity
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
}

output TodoOwner {
    parent: Todo
    owner: User?
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwner {
    return attempt query required Todo {
        where: id == todo_id
        include: owner required into: TodoOwner
        missing: TodoNotFound
    }
}
```

### `type.parent_include_single`

```jadpo project=e-parent-include-single expect=type.parent_include_single
entity User {
    id: Uuid identity
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
}

output TodoOwner {
    parent: Todo
    owner: User
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwner {
    return attempt query required Todo {
        where: id == todo_id
        include: owner required into: TodoOwner
        include: owner required into: TodoOwner
        missing: TodoNotFound
    }
}
```

### Two-level includes

### `type.nested_include_first_hop`

```jadpo project=e-nested-include-first-hop expect=type.nested_include_first_hop
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
    inverse todos: many Todo via Todo.owner_id
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
    reviewer_id: User.id? references User.id as reviewer on_delete set_null
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

output OwnerProfile {
    parent: User
    profile: Profile?
}

output TodoOwnerProfile {
    parent: Todo
    owner: OwnerProfile
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwnerProfile {
    return attempt query required Todo {
        where: id == todo_id
        include: unknown.profile optional into: TodoOwnerProfile
        missing: TodoNotFound
    }
}
```

### `type.nested_include_nullable_first_hop`

```jadpo project=e-nested-include-nullable-first-hop expect=type.nested_include_nullable_first_hop
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
    inverse todos: many Todo via Todo.owner_id
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
    reviewer_id: User.id? references User.id as reviewer on_delete set_null
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

output OwnerProfile {
    parent: User
    profile: Profile?
}

output TodoOwnerProfile {
    parent: Todo
    owner: OwnerProfile
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwnerProfile {
    return attempt query required Todo {
        where: id == todo_id
        include: reviewer.profile optional into: TodoOwnerProfile
        missing: TodoNotFound
    }
}
```

### `type.nested_include_second_hop`

```jadpo project=e-nested-include-second-hop expect=type.nested_include_second_hop
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
    inverse todos: many Todo via Todo.owner_id
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
    reviewer_id: User.id? references User.id as reviewer on_delete set_null
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

output OwnerProfile {
    parent: User
    profile: Profile?
}

output TodoOwnerProfile {
    parent: Todo
    owner: OwnerProfile
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwnerProfile {
    return attempt query required Todo {
        where: id == todo_id
        include: owner.todos optional into: TodoOwnerProfile
        missing: TodoNotFound
    }
}
```

### `type.nested_include_cardinality`

```jadpo project=e-nested-include-cardinality expect=type.nested_include_cardinality
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
    inverse todos: many Todo via Todo.owner_id
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
    reviewer_id: User.id? references User.id as reviewer on_delete set_null
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

output OwnerProfile {
    parent: User
    profile: Profile?
}

output TodoOwnerProfile {
    parent: Todo
    owner: OwnerProfile
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwnerProfile {
    return attempt query required Todo {
        where: id == todo_id
        include: owner.profile required into: TodoOwnerProfile
        missing: TodoNotFound
    }
}
```

### `type.nested_include_required_query`

```jadpo project=e-nested-include-required-query expect=type.nested_include_required_query
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
    inverse todos: many Todo via Todo.owner_id
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
    reviewer_id: User.id? references User.id as reviewer on_delete set_null
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

output OwnerProfile {
    parent: User
    profile: Profile?
}

output TodoOwnerProfile {
    parent: Todo
    owner: OwnerProfile
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) -> TodoOwnerProfile {
    return attempt query optional Todo {
        where: id == todo_id
        include: owner.profile optional into: TodoOwnerProfile
    }
}
```

### `type.nested_include_result_shape`

```jadpo project=e-nested-include-result-shape expect=type.nested_include_result_shape
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
    inverse todos: many Todo via Todo.owner_id
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
    reviewer_id: User.id? references User.id as reviewer on_delete set_null
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

output OwnerProfile {
    parent: User
    profile: Profile?
}

output TodoOwnerProfile {
    parent: Todo
    owner: User
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwnerProfile {
    return attempt query required Todo {
        where: id == todo_id
        include: owner.profile optional into: TodoOwnerProfile
        missing: TodoNotFound
    }
}
```

### `type.nested_include_single`

```jadpo project=e-nested-include-single expect=type.nested_include_single
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
    inverse todos: many Todo via Todo.owner_id
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
    reviewer_id: User.id? references User.id as reviewer on_delete set_null
}

entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}

output OwnerProfile {
    parent: User
    profile: Profile?
}

output TodoOwnerProfile {
    parent: Todo
    owner: OwnerProfile
}

failure TodoNotFound {
    kind: NotFound
    code: "todo_not_found"
}

action load(todo_id: Todo.id) fails TodoNotFound -> TodoOwnerProfile {
    return attempt query required Todo {
        where: id == todo_id
        include: owner.profile optional into: TodoOwnerProfile
        include: owner.profile optional into: TodoOwnerProfile
        missing: TodoNotFound
    }
}
```

### Writes

### `type.create_not_entity`

```jadpo project=e-create-not-entity expect=type.create_not_entity
type DraftName = Text {}

input Draft {
    name: DraftName
}

action create_draft(name: DraftName) -> Draft {
    return attempt create Draft {
        name: name
    }
}
```

### `type.update_not_entity`

```jadpo project=e-update-not-entity expect=type.update_not_entity
type DraftId = Uuid {}

input Draft {
    id: DraftId
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action change(id: DraftId) fails ItemNotFound, ItemConflict -> Draft {
    return attempt update required Draft {
        where: id == id
        set: {
            id: id
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.delete_not_entity`

```jadpo project=e-delete-not-entity expect=type.delete_not_entity
type DraftId = Uuid {}

input Draft {
    id: DraftId
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action delete_draft(id: DraftId) fails ItemNotFound, ItemConflict -> Draft {
    return attempt delete required Draft {
        where: id == id
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `syntax.expected_update_body`

```jadpo project=e-expected-update-body expect=syntax.expected_update_body
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action change(id: Item.id) fails ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.update_field_required`

```jadpo project=e-update-field-required expect=type.update_field_required
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action change(id: Item.id) fails ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        set: {}
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.update_unknown_field`

```jadpo project=e-update-unknown-field expect=type.update_unknown_field
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action change(id: Item.id, name: Item.name) fails ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        set: {
            title: name
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.update_duplicate_field`

```jadpo project=e-update-duplicate-field expect=type.update_duplicate_field
entity Customer {
    id: Uuid
    email: Text
}

failure CustomerNotFound {
    kind: NotFound code: "customer_not_found" }
failure CustomerConflict {
    kind: Conflict code: "customer_conflict" }

action update_customer(id: Customer.id, email: Customer.email) -> Customer
    fails CustomerNotFound, CustomerConflict
{
    return attempt update required Customer {
        where: id == id
        set: {
            email: email
            email: (email)
        }
        missing: CustomerNotFound
        conflict: CustomerConflict
    }
}
```

### `type.mutation_unknown_predicate_field`

```jadpo project=e-mutation-unknown-predicate-field expect=type.mutation_unknown_predicate_field
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action change(id: Item.id, name: Item.name) fails ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: code == id
        set: {
            name: name
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.mutation_nullable_field_unsupported`

```jadpo project=e-mutation-nullable-field-unsupported expect=type.mutation_nullable_field_unsupported
type Nickname = Text {}

entity Item {
    id: Uuid identity
    name: Text
    nickname: Nickname?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action change(nickname: Nickname, name: Item.name) fails ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: nickname == nickname
        set: {
            name: name
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `syntax.mutation_conflict_required`

Goes under question 6: only rules a write can break are mapped.

```jadpo project=e-mutation-conflict-required expect=syntax.mutation_conflict_required
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

action remove(id: Item.id) fails ItemNotFound -> Item {
    return attempt delete required Item {
        where: id == id
        missing: ItemNotFound
    }
}
```

### `type.conflict_duplicate_binding`

```jadpo project=e-conflict-duplicate-binding expect=type.conflict_duplicate_binding
entity Item {
    id: Uuid identity
}

failure FirstConflict {
    kind: Conflict
    code: "first_conflict"
}

failure SecondConflict {
    kind: Conflict
    code: "second_conflict"
}

action create_item(id: Item.id) fails FirstConflict, SecondConflict -> Item {
    return attempt create Item {
        id: id
    }
    conflict: FirstConflict
    conflict: SecondConflict
}
```

### `type.conflict_unknown_constraint`

```jadpo project=e-conflict-unknown-constraint expect=type.conflict_unknown_constraint
entity Membership {
    id: Uuid identity
    tenant: Text
    email: Text
    constraint tenant_email: unique(tenant, email)
}

failure MembershipConflict {
    kind: Conflict code: "membership_conflict" }

action create_membership(id: Membership.id, tenant: Membership.tenant, email: Membership.email) -> Membership
    fails MembershipConflict
{
    return attempt create Membership {
        id: id
        tenant: tenant
        email: email
    }
    conflict Membership.no_such_constraint: MembershipConflict
}
```

### `syntax.expected_delete_action`

Raised by a reference's `on_delete` value, which area 5 keeps:

```jadpo project=e-expected-delete-action expect=syntax.expected_delete_action
entity User {
    id: Uuid identity
}

entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id on_delete archive
}
```

### Patches

### `syntax.expected_patch_input`

```jadpo project=e-expected-patch-input expect=syntax.expected_patch_input
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

input PatchItem {
    name: Item.name optional
}

action patch_item(id: Item.id, input: PatchItem) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: 123
        empty: EmptyPatch
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.patch_input_binding`

```jadpo project=e-patch-input-binding expect=type.patch_input_binding
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

input PatchItem {
    name: Item.name optional
}

action patch_item(id: Item.id, input: PatchItem) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: input.name
        empty: EmptyPatch
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.patch_not_input`

```jadpo project=e-patch-not-input expect=type.patch_not_input
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

action patch_item(id: Item.id, item: Item) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: item
        empty: EmptyPatch
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.patch_field_required`

```jadpo project=e-patch-field-required expect=type.patch_field_required
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

input PatchItem {}

action patch_item(id: Item.id, input: PatchItem) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: input
        empty: EmptyPatch
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.patch_field_not_optional`

```jadpo project=e-patch-field-not-optional expect=type.patch_field_not_optional
entity Customer {
    id: Uuid identity
    email: Text
}

input PatchCustomer {
    email: Customer.email
}

failure EmptyCustomerPatch {
    kind: InvalidValue code: "empty_customer_patch" }
failure CustomerNotFound {
    kind: NotFound code: "customer_not_found" }
failure CustomerConflict {
    kind: Conflict code: "customer_conflict" }

action patch_customer(id: Customer.id, input: PatchCustomer) -> Customer
    fails EmptyCustomerPatch, CustomerNotFound, CustomerConflict
{
    return attempt update required Customer {
        where: id == id
        patch: input
        empty: EmptyCustomerPatch
        missing: CustomerNotFound
        conflict: CustomerConflict
    }
}
```

### `type.patch_unknown_field`

```jadpo project=e-patch-unknown-field expect=type.patch_unknown_field
entity Customer {
    id: Uuid identity
    email: Text
}

input PatchCustomer {
    display_name: Text optional
}

failure EmptyCustomerPatch {
    kind: InvalidValue code: "empty_customer_patch" }
failure CustomerNotFound {
    kind: NotFound code: "customer_not_found" }
failure CustomerConflict {
    kind: Conflict code: "customer_conflict" }

action patch_customer(id: Customer.id, input: PatchCustomer) -> Customer
    fails EmptyCustomerPatch, CustomerNotFound, CustomerConflict
{
    return attempt update required Customer {
        where: id == id
        patch: input
        empty: EmptyCustomerPatch
        missing: CustomerNotFound
        conflict: CustomerConflict
    }
}
```

### `type.patch_derived_overlap`

```jadpo project=e-patch-derived-overlap expect=type.patch_derived_overlap
entity Customer {
    id: Uuid identity
    email: Text
}

input PatchCustomer {
    email: Customer.email optional
}

failure EmptyCustomerPatch {
    kind: InvalidValue code: "empty_customer_patch" }
failure CustomerNotFound {
    kind: NotFound code: "customer_not_found" }
failure CustomerConflict {
    kind: Conflict code: "customer_conflict" }

action patch_customer(id: Customer.id, input: PatchCustomer) -> Customer
    fails EmptyCustomerPatch, CustomerNotFound, CustomerConflict
{
    return attempt update required Customer {
        where: id == id
        patch: input
        empty: EmptyCustomerPatch
        set: {
            email: input.email when input.email supplied
        }
        missing: CustomerNotFound
        conflict: CustomerConflict
    }
}
```

### `syntax.expected_supplied_field`

```jadpo project=e-expected-supplied-field expect=syntax.expected_supplied_field
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

input PatchItem {
    name: Item.name optional
}

action patch_item(id: Item.id, input: PatchItem) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: input
        empty: EmptyPatch
        set: {
            nickname: none when 123
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `syntax.expected_supplied`

```jadpo project=e-expected-supplied expect=syntax.expected_supplied
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

input PatchItem {
    name: Item.name optional
}

action patch_item(id: Item.id, input: PatchItem) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: input
        empty: EmptyPatch
        set: {
            nickname: none when input.name changed
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.patch_condition_binding`

```jadpo project=e-patch-condition-binding expect=type.patch_condition_binding
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

input PatchItem {
    name: Item.name optional
}

action patch_item(id: Item.id, input: PatchItem, other: PatchItem) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: input
        empty: EmptyPatch
        set: {
            nickname: none when other.name supplied
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### `type.patch_condition_unknown_field`

```jadpo project=e-patch-condition-unknown-field expect=type.patch_condition_unknown_field
entity Item {
    id: Uuid identity
    name: Text
    nickname: Text?
}

failure ItemNotFound {
    kind: NotFound
    code: "item_not_found"
}

failure ItemConflict {
    kind: Conflict
    code: "item_conflict"
}

failure EmptyPatch {
    kind: InvalidValue
    code: "empty_patch"
}

input PatchItem {
    name: Item.name optional
}

action patch_item(id: Item.id, input: PatchItem) fails EmptyPatch, ItemNotFound, ItemConflict -> Item {
    return attempt update required Item {
        where: id == id
        patch: input
        empty: EmptyPatch
        set: {
            nickname: none when input.title supplied
        }
        missing: ItemNotFound
        conflict: ItemConflict
    }
}
```

### Ownership, transactions and freshness

### `data.raw_query_outside_named_query`

Goes under question 1: an action has no read expression to write.

```jadpo project=e-raw-query-outside-named-query expect=data.raw_query_outside_named_query
entity Customer {
    id: Uuid
    identity: id
    persistence {
        store: primary
        role: authority
    }
}

action load_customer(id: Customer.id) -> Customer? {
    return attempt query optional Customer {
        where: id == id
    }
}
```

### `data.mutation_owner_required`

Changes meaning under question 5: the owning domain's actions may write.

```jadpo project=e-mutation-owner-required expect=data.mutation_owner_required
entity Session {
    id: Uuid
    identity: id

    persistence {
        store: primary
        role: authority
    }
}

action save_session(id: Session.id) -> Session {
    return attempt create Session {
        id: id
    }
}
```

### `data.query_freshness_required`

Goes: area 3 removed callable settings.

```jadpo project=e-query-freshness-required expect=data.query_freshness_required
entity Customer {
    id: Uuid
    identity: id

    persistence {
        store: primary
        role: authority
    }

    query by_id(id: Customer.id) -> Customer? {
        return attempt query optional Customer {
            where: id == id
        }
    }
}
```

### `data.freshness_invalid`

Goes with `freshness:`.

```jadpo project=e-freshness-invalid expect=data.freshness_invalid
entity Customer {
    id: Uuid
    identity: id

    persistence {
        store: primary
        role: authority
    }

    query by_id(id: Customer.id)
    freshness: cached
        -> Customer?
    {
        return attempt query optional Customer {
            where: id == id
        }
    }
}
```

### `data.consistency_required`

Goes: every action is all-or-nothing (area 3).

```jadpo project=e-consistency-required expect=data.consistency_required
entity Customer {
    id: Uuid
    identity: id
    persistence {
        store: primary
        role: authority
    }
    action register(id: Customer.id) -> Customer {
        return attempt create Customer { id: id }
    }
}

entity Account {
    id: Uuid
    identity: id
    persistence {
        store: primary
        role: authority
    }
    action open(id: Account.id) -> Account {
        return attempt create Account { id: id }
    }
}

action register_both(customer_id: Customer.id, account_id: Account.id) -> Account {
    var customer = Customer.register(customer_id)
    return Account.open(account_id)
}
```

### `data.consistency_invalid`

Goes with `consistency:`.

```jadpo project=e-consistency-invalid expect=data.consistency_invalid
type Count = Int {}

action bump(n: Count)
consistency: eventual
    -> Count
{
    return n
}
```

### `data.atomic_domain_mismatch`

Becomes "an action writes two stores" (area 5).

```jadpo project=e-atomic-domain-mismatch expect=data.atomic_domain_mismatch
entity Customer {
    id: Uuid
    identity: id
    persistence {
        store: primary
        role: authority
    }
    action register(id: Customer.id) -> Customer {
        return attempt create Customer { id: id }
    }
}

entity LedgerEntry {
    id: Uuid
    identity: id
    persistence {
        store: ledger
        role: authority
    }
    action record(id: LedgerEntry.id) -> LedgerEntry {
        return attempt create LedgerEntry { id: id }
    }
}

action register_and_record(customer_id: Customer.id, entry_id: LedgerEntry.id)
    consistency: atomic
    -> LedgerEntry
{
    var customer = Customer.register(customer_id)
    return LedgerEntry.record(entry_id)
}
```

### `data.value_receiver_guard_required`

Goes: area 5 removed `self` and `guard:`, and every write checks for concurrent change.

```jadpo project=e-value-receiver-guard-required expect=data.value_receiver_guard_required
failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found_guard"
}

failure CustomerConflict {
    kind: Conflict
    code: "customer_conflict_guard"
}

entity Customer {
    id: Uuid
    email: Email
    identity: id
    persistence {
        store: primary
        role: authority
    }

    action change_email(self: value, email: Customer.email)
        fails CustomerNotFound, CustomerConflict
        -> Customer
    {
        return attempt update required Customer {
            where: id == self.id
            set: { email: email }
            missing: CustomerNotFound
            conflict: CustomerConflict
        }
    }
}
```

### `data.mutation_guard_invalid`

Goes with `guard:`.

```jadpo project=e-mutation-guard-invalid expect=data.mutation_guard_invalid
failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found"
}

failure CustomerConflict {
    kind: Conflict
    code: "customer_conflict"
}

entity Customer {
    id: Uuid
    email: Email
    identity: id

    persistence {
        store: primary
        role: authority
    }

    action change_email(self: value, email: Customer.email)
    guard: optimistic
        fails CustomerNotFound, CustomerConflict
        -> Customer
    {
        return attempt update required Customer {
            where: id == self.id
            set: { email: email }
            missing: CustomerNotFound
            conflict: CustomerConflict
        }
    }
}
```

## Review

### Status

Found by compiling with `jadpo check` and by building the golden todo app
and reading the generated TypeScript and SQL; nothing was run against a live
database.

- **No concurrent-change check on PostgreSQL (read from generated code).**
  An action's transaction runs at PostgreSQL's default isolation (read
  committed) with no revision on records. `update` re-reads its row under
  `FOR UPDATE`, so the write itself is consistent, but a decision taken on an
  earlier read can rest on a value another request has since changed (a lost
  update). SQLite starts every action with `BEGIN IMMEDIATE`, which
  serialises writers, so the two engines behave differently.
- **Only page queries can filter on more than one field (compiled).** `and:`
  in a `many` query gives four unrelated syntax errors; `required`,
  `optional` and `many` accept exactly one `field == value`.
- **A `many` query without `limit:` reads every matching row (compiled).**
- **`limit:` follows two rules (compiled):** a page takes it from the
  request; a `many` query or include needs a literal, and `offset:` is
  required beside it.
- **Top-level actions can neither read nor write (compiled).** Both are
  errors (`data.raw_query_outside_named_query`,
  `data.mutation_owner_required`), so the decided `actions/` folder cannot
  hold the golden app's actions until the rework lands.
- **The golden app re-checks visibility by hand (read from generated SQL).**
  Every read already adds `deleted_at IS NULL`, so `Todo.by_id`'s `match` on
  `deleted_at` never fires, and `patch_todo` binds `var existing` and never
  uses it.
- **`create` puts its conflict mapping outside the braces;** inside, it is
  read as a field (verified by the syntax review).

### Consistency with the rest of the language

- **Qualified fields.** This page writes `Todo.status` in every condition,
  while area 5's decided lifecycle writes `visible: deleted_at == none` and
  `when: deleted_at == none` with bare fields. One rule for every condition
  on a record would mean qualifying those too.
- **`set:` everywhere.** Create, update, counters and lifecycle transitions
  all give field values in a `set:` block.
- **Two not-found mappings.** `Todo.get` reports the entity's own failure,
  while a query maps `missing:` itself, because a query can be about
  something other than one record (`ProjectBoardView`).
- **`Page<T>` is a standard generic type,** like `List<T>`; area 3 ruled out
  generics only in your own code.
- **`order:` replaces `order_by:`** and is a list, following area 2's list
  decision.
- **The route example runs a query,** which area 3's table lists as
  something a route may not do; area 7 decides.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| A route may run a query directly for a display read; the write revision header and cookie (`jadpo-revision`) and whether non-browser clients echo it; `If-Match` checked against a record's revision with `precondition_failed` | 7 | Input to area 7 |
| Policy filters in queries, the cache policy key, concealment of hidden and unpermitted records alike | 9 | Input to area 9 |
| Children of a hidden parent (question 10): the policy side, such as whether a hidden owner's todos stay readable by others | 9 | Depends on question 10 |
| Exports and reports that read a large set: a schedule paging through a query, and whether a consistent snapshot read is needed | 12 | Input to area 12 |
| Asserting in tests how many records and database queries a query can cost | 13 | Input to area 13 |
| The build report: each query call's source (store or cache), its row bound and query count, and the indexes it needs | 14 | Input to area 14 |
| Counts and sums in queries, and totals kept on an entity in the same transaction (computed fields, from the stores mapping §1.8) | 6 | Deferred |
| `expiry:` on lossy-store entities hides expired records from queries and updates like `visible:` | 5 | Follows from a decision |

### Open questions

1. **What does a query look like?** Today a query is a callable with a
   statement body around a read expression with its own cardinality word,
   five clause grammars, and hand-written cursor and page types (the syntax
   review's top-10 items 3 and 4). *Recommendation:* a query is one
   declarative read: `from:`, stacked `where:` lines, `order:`, `limit:` or
   `page:`, `include:`, `missing:`; how many records it returns follows from
   its result type. Composition happens in actions. This is Ecto's and
   Prisma's shape rather than a SQL-like expression language; see
   [the query proposals](research/06-reads-and-writes-proposals.md) for the
   alternatives.
2. **How are filters written?** *Recommendation:* fields qualified with
   their entity; each `where:` line must hold; comparisons, `and`, `or`,
   `not`, `== none` and `in`; `where_present:` for a request's optional
   filters, replacing `matches optional` and `<= optional`; only conditions
   an index can serve.
3. **How are order and pages written?** *Recommendation:* `order: [field
   direction]` with `id` added as the tie-break, so
   `type.query_order_not_deterministic` goes; every list bounded by a
   literal or a type's `max:`; keyset pages only, through the standard
   `Page<T>` and an opaque `PageCursor`; no offset. Relay and Prisma
   both recommend cursors over offsets for the same reasons.
4. **How are children and parents included?** *Recommendation:* an
   `include:` block whose entries name a reference field: this entity's own
   reference gives the parent, another entity's reference gives children (a
   list needing `order:` and `limit:`, or one record when the reference is
   unique). Nesting up to two levels, within the domain, and across stores
   only by `id`. Other domains' data comes from their public queries.
5. **Which operations does every entity get?** *Recommendation:* `get`,
   `create`, `update` (with `set:` or `patch:`), `delete`, and one per
   lifecycle transition, called through the entity; only actions in the
   owning domain may call the writing ones, which replaces today's rule
   that only entity-owned actions write.
6. **What do failed writes report?** *Recommendation:* a missing record
   reports the entity's `<Entity>NotFound`, provided by the compiler unless
   the domain declares it; every unique rule and `restrict` reference a
   write can break is mapped in `conflict:`, keyed by the rule, with no
   catch-all and no `TodoMutationConflict`. Rails and Django raise one
   generic error per constraint type and leave mapping to the developer;
   here the compiler knows the rules, so it can insist.
7. **Does an empty patch fail?** *Recommendation:* no: it changes nothing
   and succeeds, as JSON Merge Patch (RFC 7396) specifies, so `empty:` and
   `failure.patch_empty_not_invalid_value` go. Conditional writes become
   `when_supplied: { field: { ... } }`, replacing `x when input.f supplied`.
8. **What happens when a record changed since it was loaded?**
   *Recommendation:* the runtime reruns the whole action within the
   existing retry budget, then fails `unavailable`; on PostgreSQL through
   repeatable-read isolation. Rails (`lock_version`) and Entity Framework
   raise an error the developer must handle; Jadpo can rerun because an
   action has no outside effects before it commits.
9. **How are counters updated?** *Recommendation:* an update by `id` whose
   values use `Entity.field` for the stored value
   (`Post.view_count + 1`), computed by the database in one step and never
   rerun; PostgreSQL's own documentation recommends exactly this for
   increments.
10. **Are children hidden with their parent?** Today they stay visible, and
    a required include of a hidden parent reads as missing.
    *Recommendation:* visibility never spreads: a child stays visible until
    its own lifecycle hides it, and an include of its parent returns the
    parent even when hidden, because the reference still holds. The action
    that hides a parent runs its children's transitions when they should go
    too. Rails' `paranoia` gem hides deleted parents from associations by
    default and the resulting `nil` parents are its most common complaint.
11. **Are reads inside a loop allowed?** *Recommendation:* no: a query or
    `get` inside `for` is a compile error, and the fix is one query with
    `in`. Rails needed `strict_loading` to catch the same "one read per
    item" mistake after the fact.
12. **Where may a cache serve a read?** *Recommendation:* only a query a
    route runs directly; every read inside an action, policy and
    authentication goes to the store, and a client that just wrote skips
    caches by its revision. This makes the call site, not a `freshness:`
    setting, decide, as area 3 decided.
13. **Do policy conditions shape query plans and indexes, and is PostgreSQL
    row-level security generated?** (carried from area 9) Area 9 sends
    generated query plans, indexes for policy conditions, and row-level
    security as optional defence in depth generated from the same policy to
    this page. Which of these does the compiler generate?
14. **How do caches behave in tests?** (carried from area 13) Once the Redis
    adapter exists, do tests read through declared caches or straight from
    the store, and how does a test check a cached read?

### Decisions

Nothing on this page is decided yet. The rows marked "Carried" were decided
in earlier areas and bind this page.

| Topic | Decision | Date |
|---|---|---|
| Query placement | Carried from areas 1 and 5 (Richard): every query lives in its domain's `queries/` folder, one per file; generated operations stay on the entity | 2026-10-12 |
| Action placement | Carried from areas 1 and 5 (Richard): every action lives in `actions/`, one per file; written actions are called by name | 2026-10-12 |
| Inverses | Carried from area 5: parents do not declare children; a query includes children by naming the child's reference field | 2026-10-12 |
| Receivers | Carried from area 5: no `self`; `todo: Todo` is a loaded record and `todo_id: Todo.id` a reference; `guard:` goes | 2026-10-12 |
| Callable settings | Carried from area 3: none; every action is all-or-nothing, writes check for concurrent change, reads are current unless they read a declared cache | 2026-10-12 |
| Mapping form | Carried from area 4: `condition: Failure` inside the operation's block; the original failure kept as the internal cause | 2026-10-12 |
| Stores and caches | Carried from area 5: an action writes at most one store; caches are copies for display reads only, with a required `keep:` | 2026-10-12 |
| Loops | Carried from area 3: `for` over a `List` only | 2026-10-12 |
| Query shape | | |
| Filters | | |
| Order and pages | | |
| Includes | | |
| Entity operations | | |
| Write failures | | |
| Empty patch | | |
| Concurrent change | | |
| Counters | | |
| Hidden parents | | |
| Reads in loops | | |
| Cache reads | | |
| Policy in query plans | | |
| Caches in tests | | |
