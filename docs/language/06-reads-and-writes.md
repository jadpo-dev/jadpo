---
title: Reads and writes
area: 6
status: decided
checked-with: this checkout's jadpo on every gate run (archived examples: v0 3ce094d0)
---

# Reads and writes

How a program reads and changes stored records: named queries, filters,
order and pages, children and parents, the operations every entity gets,
what a failed write reports, and how an action stays all-or-nothing while
other requests change the same records. Where records are stored, and how
caches copy them, is area 5; who may read or change them is area 9.

> Decided on 2026-10-12. Built in chunk 3.2: queries (filters, order, pages, includes and the columns a result needs), `Entity.get`, the rule against reads in a loop, and `jadpo sql`. Built in chunk 3.3: `create`, `update` (with `set:` or `patch:`, of a loaded record or by `id`) and `delete`, `conflict:` mappings, the not-found failure the compiler provides, counters, one store per action and the rerun after a concurrent change. The examples marked `check` compile with this checkout's `jadpo`, and the runtime runs them against PostgreSQL. Lifecycles come with chunk 3.4, so their examples are proposals. The archived compiler's syntax is collapsed beneath each section that changed. The query syntax is compared with other frameworks in [the query proposals](research/06-reads-and-writes-proposals.md).

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
(`type TodoView = Output { pick: [Todo.id, Todo.title] }`, area 2); the compiler
reads only the fields that type needs. Records hidden by the entity's
lifecycle are left out of every query. The generated SQL selects only the
columns the result type needs, plus the `order:` fields a page cursor needs
and any field a policy condition checks outside the database. A query writes nothing and calls no
actions (area 3). It is called like an action: bare in its own domain,
`planning.todos_for_owner(...)` from another domain when it is `public`.

```jadpo check project=named file=planning.jadpo hidden
type UserName = Text {
    max_length: 80
}

type ProjectName = Text {
    max_length: 80
}

type TodoTitle = Text {
    max_length: 200
}

type TodoStatus = Enum {
    open
    waiting
    done
}

entity User {
    id: Uuid
    name: UserName
}

entity Project {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id? {
        on_delete: OnDelete.cleared
    }
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }
}

type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status]
}

type UserView = Output {
    pick: [User.id, User.name]
}
```

```jadpo check project=named file=list_todos.jadpo hidden
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

```jadpo check project=named file=todos_for_owner.jadpo
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

`jadpo sql todos_for_owner` prints the statement it sends: one `SELECT` of
the result's columns and the order's, the conditions, `ORDER BY` and a
`LIMIT` one more than the page's size, so the runtime knows whether a next
page exists. Its text is fixed when the project is compiled: a
`where_present:` condition and the cursor are written `($n IS NULL OR
...)`, so the same query always sends the same statement. A query called
inside an action reads in the action's transaction; called from a test, it
runs in a short transaction of its own.

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

```jadpo check project=filters file=planning.jadpo hidden
type UserName = Text {
    max_length: 80
}

type ProjectName = Text {
    max_length: 80
}

type TodoTitle = Text {
    max_length: 200
}

type TodoStatus = Enum {
    open
    waiting
    done
}

entity User {
    id: Uuid
    name: UserName
}

entity Project {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id? {
        on_delete: OnDelete.cleared
    }
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }
}

type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status]
}

type UserView = Output {
    pick: [User.id, User.name]
}
```

```jadpo check project=filters file=list_todos.jadpo hidden
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

```jadpo check project=filters file=due_todos.jadpo
query due_todos(owner_id: User.id, filter: ListTodos) -> List<TodoView> {
    from: Todo
    where: Todo.owner_id == owner_id
    where: Todo.due_at == none or Todo.due_at > clock.now
    where: Todo.status in [TodoStatus.open, TodoStatus.waiting]
    where_present: Todo.due_at <= filter.due_before
    order: [Todo.created_at desc]
    limit: 50
}
```

A condition must be one the database can use an index for: a field,
named first, compared with a value. A function applied to a field, two
fields compared, and a value worked out in the query are refused; values
are parameters and their fields, literals, enum values, lists of those and
`clock.now`. `<` and the other orderings take numbers, times, dates and
durations, as they do in a body (area 3). A field that is `none` meets no
comparison but `!=`, and `not` keeps that meaning, so SQL's `NULL` never
shows through. A `where:` line can't compare with a value that may be
absent; that is what `where_present:` is for. The compiler creates the
indexes the conditions and order need (area 5).

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
type holding `items: List<T>` and `next: PageCursor?`, an opaque cursor the
client sends back as `after:` to get the next page. `Page<T>`, `PageCursor`
and `PageSize` (an `Int` from 1 to 100) are built in; an app that needs a
different maximum declares its own `Int` type with a `max:`, because a page
size must have an upper limit. There is no offset, because a large
offset is slow and skips or repeats records when rows are added.

```jadpo check project=page-input file=types.jadpo hidden
type TodoStatus = Enum {
    open
    done
}
```

```jadpo check project=page-input
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
rejected as invalid input: the runtime's 422 `invalid_input`. A page's
order fields are required ones, since the next page starts after the last
record's values. Text sorts by code point, as Jadpo compares it, whatever
the database's collation; an enum, a `Bool` or a structured field can't be
sorted by, since the database and Jadpo would disagree on their order.

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

```jadpo check project=board file=planning.jadpo hidden
type UserName = Text {
    max_length: 80
}

type ProjectName = Text {
    max_length: 80
}

type TodoTitle = Text {
    max_length: 200
}

type TodoStatus = Enum {
    open
    waiting
    done
}

entity User {
    id: Uuid
    name: UserName
}

entity Project {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id? {
        on_delete: OnDelete.cleared
    }
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }
}

type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status]
}

type UserView = Output {
    pick: [User.id, User.name]
}
```

```jadpo check project=board file=failures.jadpo hidden
type BoardNotFound = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=board file=project_board.jadpo
// domains/planning/queries/project_board.jadpo
query project_board(project_id: Project.id) fails BoardNotFound -> ProjectBoardView {
    from: Project
    where: Project.id == project_id
    missing: BoardNotFound
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

type ProjectBoardView = Output {
    pick: [Project.id, Project.name]
    owner: UserView
    todos: List<TodoView>
}
```

Each include costs one more database query however many records the result
holds, never one per record: a parent or a unique child is read by `= ANY`
of the keys the records hold, and a list through a `LATERAL` subquery that
orders and limits each record's list on its own. Includes nest up to two levels, stay inside the
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
| `Todo.update(todo_id) { set: { ... } }` | changes the record with that `id` in one step (counters, below) |
| `Todo.delete(todo)` | removes the record, or runs the lifecycle's `delete` transition |
| `User.disable(user)` | runs a lifecycle transition, named after it |

Built since chunk 3.3: every operation but the lifecycle's transitions,
which come with chunk 3.4; calling one is `toolchain.not_built_yet`. An
`update` or `delete` takes the record loaded earlier in the action, or its
`id`: by `id` it fails with the entity's not-found failure when no record
has it (chosen in chunk 3.3, since area 5's lifecycle writes
`Todo.delete(todo_id)` and this table `Todo.delete(todo)`). An update gives
its fields in `set:` or the request's changes with `patch:`, never both, and
`when_supplied:` goes with `patch:` (chosen in chunk 3.3). A create and an
update give back the record as stored; a delete gives nothing. Each fills
the store's own fields: a new version 7 `id` (`Ctx::new_id`, from the
invocation's seed) on create, and the operation's clock in each generated
field, `on_create` once and `on_change` on every update. Every write joins
the action's transaction, and the graph has a `writes` edge from the
callable to the entity.

```jadpo check project=operations file=planning.jadpo hidden
type ProjectName = Text {
    max_length: 80
}

entity User {
    id: Uuid
}

entity Project {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName
}

type UserNotFound = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=operations file=start_project.jadpo
// domains/planning/actions/start_project.jadpo
action start_project(owner_id: User.id, name: Project.name) fails UserNotFound -> Project {
    var owner: User = attempt User.get(owner_id)  // the user, or UserNotFound
    return Project.create {
        set: {
            owner_id: owner.id  // every field but `id` and the generated ones
            name: name
        }
    }
}
```

Nothing else is an entity operation: `Todo.fetch(todo_id)` is
`semantic.unknown_callee` (checked since chunk 1.9), while `Todo.title(value)`
still builds a value of the field's type (area 2).

`TodoNotFound` is the domain's failure of that name in `failures/`, or one
the compiler provides with kind `not_found` when the domain declares none:
public, with no fields, placed at the entity in the graph (chosen in chunk
3.3). A read or write gives each field typed `<Entity>.id` of a declared one
the `id` it looked for. A write that cannot fail needs no `attempt` (area
3).

```jadpo check project=reschedule file=planning.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

type DueAt = Instant {}

entity Todo {
    id: Uuid
    title: TodoTitle
    due_at: DueAt?
    reminder_sent_at: Instant?
}

type DueDateInPast = Failure {
    kind: FailureKind.invalid_value
}
```

```jadpo check project=reschedule file=reschedule_todo.jadpo
// domains/planning/actions/reschedule_todo.jadpo
action reschedule_todo(todo_id: Todo.id, due_at: DueAt) fails TodoNotFound, DueDateInPast -> Todo {
    var todo: Todo = attempt Todo.get(todo_id)  // TodoNotFound: provided
    if (due_at <= clock.now) {
        reject DueDateInPast
    }
    return Todo.update(todo) {
        set: {
            due_at: due_at
            reminder_sent_at: none
        }
    }
}
```

A patch takes an `Input` whose fields are optional and match the entity's:
a field the request left out keeps its value, and one sent as `none` is
cleared (area 2). `when_supplied:` sets further fields when a given field
was supplied. A patch with no fields changes nothing and succeeds, as JSON
Merge Patch does, so there is no `empty:` failure.

```jadpo check project=patch file=planning.jadpo hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
    due_at: Instant?
    reminder_sent_at: Instant?
}
```

```jadpo check project=patch file=edit_todo.jadpo
type TodoChanges = Input {
    pick: [Todo.title, Todo.due_at]
    optional: all
}

action edit_todo(todo: Todo, amendment: TodoChanges) -> Todo {
    return Todo.update(todo) {
        patch: amendment
        when_supplied: {
            due_at: {
                reminder_sent_at: none
            }
        }
    }
}
```

The statement's text is fixed: each field of the input is written `CASE
WHEN <supplied> THEN <value> ELSE <column> END`, and an `on_change` field
changes only when one was supplied. A value built without one of its
optional `Input` fields remembers it was left out, which only a patch
reads; it still equals one given `none`.

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

```jadpo check project=refused file=planning.jadpo hidden
type ProjectName = Text {
    max_length: 80
}

entity User {
    id: Uuid
}

entity Project {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName

    unique: {
        name_per_owner: [owner_id, name]
    }
}

entity Todo {
    id: Uuid
    project_id: Project.id {
        on_delete: OnDelete.restricted
    }
}

type ProjectNameTaken = Failure {
    kind: FailureKind.conflict
}

type ProjectHasTodos = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=refused file=start_project.jadpo
action start_project(owner: User, name: Project.name) fails ProjectNameTaken -> Project {
    return attempt Project.create {
        set: {
            owner_id: owner.id  // a loaded record's id: the owner is there
            name: name
        }
        conflict: {
            Project.name_per_owner: ProjectNameTaken
        }
    }
}
```

```jadpo check project=refused file=close_project.jadpo
action close_project(project: Project) fails ProjectHasTodos -> Unit {
    attempt Project.delete(project) {
        conflict: {
            Todo.project_id: ProjectHasTodos
        }
    }
}
```

What a write can break is worked out from what it writes: a create, every
unique rule of its entity; an update, the unique rules over the fields it
sets; a delete, each `restricted` reference to the record, or to a record
its delete cascades to. A reference's value names a record that is surely
there when it is read from a record loaded earlier in the action
(`owner.id`, or a loaded record's own reference to that entity) or is
`none`; any other value may name one that isn't, so the write can fail with
that entity's not-found failure, which `attempt` passes on (chosen in chunk
3.3: the transaction's snapshot holds a record loaded in it, and a
concurrent delete of it is a concurrent change). The runtime maps the
store's constraint name back to the rule (area 5 names every constraint).
A write that can be refused, run by an action another action called, runs
inside a savepoint, so a caller that handles the failure with `match`
carries on in a transaction the refusal didn't end.

## All-or-nothing actions and concurrent changes

Every action is all-or-nothing (area 3): its writes, and those of the public
actions it calls, commit together or not at all. A write to a record loaded
earlier in the action checks that nobody changed it in between. If someone
did, the runtime throws the attempt away and runs the whole action again,
with fresh reads and a fresh `clock.now`. That is safe because an action's
outside call comes before its writes and carries an idempotency key, so a
rerun repeats it harmlessly (area 11). After three
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

Built in chunk 3.3. Every action's transaction runs at repeatable read, so
its snapshot is taken at its first statement and a record it loaded is the
record as of then; an `update` or `delete` of a row another transaction
changed since is PostgreSQL's serialisation failure, which the driver
reports as a concurrent change. The runtime then rolls the transaction
back, throws away the attempt's events, and runs the whole action again
from its first line: its reads see the other transaction's change, and its
new records get new identities. The budget is three attempts; the second
bound waits for a host clock, which comes with time (chunk 5.2). A record
handed to an action from outside its transaction (a test passing one an
earlier action returned) is checked only as far as its row is still there.

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
`Video.view_count` means the value stored at the moment of the write, while
`video.view_count` would be the copy loaded earlier. Such an update takes the
record's `id` instead of a loaded record, and its values may use only
parameters, constants, `clock.now` and the record's stored fields, so it
never reruns the action.

```jadpo check project=counter file=video.jadpo hidden
type ViewCount = Int {
    min: 0
}

entity Video {
    id: Uuid
    view_count: ViewCount
    last_viewed_at: Instant?
}
```

```jadpo check project=counter file=view_video.jadpo
action view_video(video_id: Video.id) fails VideoNotFound -> Video {
    return attempt Video.update(video_id) {
        set: {
            view_count: ViewCount(Video.view_count + 1)
            last_viewed_at: clock.now
        }
    }
}
```

Built in chunk 3.3: the statement is `UPDATE "video" SET "view_count" =
("view_count" + $1), ...`, and no row is `VideoNotFound`. The database works
a stored field out with `+` and `-`, which give the same result in SQL as
in Jadpo, under constructions the declared ranges prove (`ViewCount(...)`
here, since a count of at least 0 plus 1 is at least 0); anything else needs
the record loaded first. One correction to the paragraph above: at
repeatable read, PostgreSQL refuses to write a row another transaction
changed after this action's first statement, so a counter inside an action
that read first does rerun when two requests race; no increment is lost
either way, and one that is the action's first statement never reruns.

## Reading in a loop

A query or `Todo.get` inside a `for` loop is a compile error, because it
would read once per item. Read the whole set with one query using `in`.
Writes inside a loop are allowed: the list is bounded and every write joins
the action's transaction. A batch that should keep partial progress is a
schedule or subscriber handling one item per action (area 3).

```jadpo check project=by-ids file=planning.jadpo hidden
type UserName = Text {
    max_length: 80
}

type ProjectName = Text {
    max_length: 80
}

type TodoTitle = Text {
    max_length: 200
}

type TodoStatus = Enum {
    open
    waiting
    done
}

entity User {
    id: Uuid
    name: UserName
}

entity Project {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    name: ProjectName
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
    project_id: Project.id? {
        on_delete: OnDelete.cleared
    }
    title: TodoTitle
    status: TodoStatus
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }
}

type TodoView = Output {
    pick: [Todo.id, Todo.title, Todo.status]
}

type UserView = Output {
    pick: [User.id, User.name]
}
```

```jadpo check project=by-ids file=todos_by_ids.jadpo
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
`store.action_spans_stores`, below, points at the write or the call that
brings in the second store.

Built in chunk 3.3: the check, counting every action an action calls. At
run time an action's transaction is begun in the store its first store
operation names, and any operation on another store in the same action is
a fault; reading another store outside the transaction is not built yet.

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

Every error this area's reads and writes can produce, with an example
that triggers it and the new compiler's actual output. The last section
keeps the archived compiler's errors for settings the rework dropped.

### Queries

### `query.unknown_section`

```jadpo check project=e-unknown-section expect=query.unknown_section
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order_by: [Book.id asc]
    limit: 20
}
```

```text output
error: `order_by` isn't something a query holds
 --> main.jadpo:15:5
   |
15 |     order_by: [Book.id asc]
   |     ^^^^^^^^ not a section
  reason: A query's block holds `from:`, `where:`, `where_present:`, `order:`, `limit:`, `page:`, `include:` and `missing:`. Did you mean `order:`?
  next: Use `order:` (agent)
  rule: query.unknown_section
  help: docs/language/06-reads-and-writes.html#query-unknown-section

check failed: 1 error, 1 file
```

### `query.repeated_section`

```jadpo check project=e-repeated-section expect=query.repeated_section
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
    limit: 50
}
```

```text output
error: `limit:` is given twice
 --> main.jadpo:17:5
   |
16 |     limit: 20
   |     ----- first given here
17 |     limit: 50
   |     ^^^^^ given again
  reason: A query has one `limit:`; only `where:` and `where_present:` lines stack, each one more condition.
  next: Remove one `limit:` (agent)
  rule: query.repeated_section
  help: docs/language/06-reads-and-writes.html#query-repeated-section

check failed: 1 error, 1 file
```

### `query.section_required`

```jadpo check project=e-section-required expect=query.section_required
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
}
```

```text output
error: `books_titled` returns `List<Book>` but has no `limit:`
 --> main.jadpo:12:7
   |
12 | query books_titled(title: Book.title) -> List<Book> {
   |       ^^^^^^^^^^^^ no `limit:`
  reason: Every list is bounded: `limit:` is a number, or a value whose type has a `max:`.
  next: Add `limit: <number>`, or return `Page<T>` to read it a page at a time (agent)
  rule: query.section_required
  help: docs/language/06-reads-and-writes.html#query-section-required

check failed: 1 error, 1 file
```

### `query.section_unexpected`

```jadpo check project=e-section-unexpected expect=query.section_unexpected
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query book_by_id(book_id: Book.id) -> Book? {
    from: Book
    where: Book.id == book_id
    order: [Book.title asc]
}
```

```text output
error: A query returning `Book?` takes no `order:`
 --> main.jadpo:15:5
   |
15 |     order: [Book.title asc]
   |     ^^^^^ not for this result
  reason: `Book?` is at most one record, found by a unique field, so there is nothing to order or limit.
  next: Remove `order:` (agent)
  rule: query.section_unexpected
  help: docs/language/06-reads-and-writes.html#query-section-unexpected

check failed: 1 error, 1 file
```

### `query.from_not_entity`

```jadpo check project=e-from-not-entity expect=query.from_not_entity
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query titles(title: Book.title) -> List<Book> {
    from: Title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `from:` names `Title`, which isn't an entity
 --> main.jadpo:13:11
   |
13 |     from: Title
   |           ^^^^^ not an entity
  reason: `Title` is a type; a query reads stored records, so `from:` names an entity declared in the domain's `entities/` folder.
  next: Name the entity the query reads (agent)
  rule: query.from_not_entity
  help: docs/language/06-reads-and-writes.html#query-from-not-entity

check failed: 1 error, 1 file
```

### `query.other_domain`

```jadpo check project=e-other-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-other-domain file=domains/lending/actions/check_out.jadpo hidden
/// Lends a copy.
action check_out(book: Book) -> Unit {}
```

```jadpo check project=e-other-domain file=domains/catalogue/entities/book.jadpo hidden migrate=initial
/// A copy on the shelves.
public entity Book {
    id: Uuid
}
```

```jadpo check project=e-other-domain file=domains/lending/queries/shelf.jadpo expect=query.other_domain
/// Every copy.
query shelf() -> List<Book> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `shelf` reads `Book`, an entity of domain `catalogue`
 --> domains/lending/queries/shelf.jadpo:3:11
  |
3 |     from: Book
  |           ^^^^ domain `catalogue`
  reason: A query reads its own domain's entities only; another domain's data comes from that domain's public queries, so `catalogue` decides what it shares.
  next: Call a public query of `catalogue`, or move `shelf` into `catalogue` (human)
  rule: query.other_domain
  help: docs/language/06-reads-and-writes.html#query-other-domain

check failed: 1 error, 4 files
```

### `query.result_type`

```jadpo check project=e-result-type expect=query.result_type
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query titles(title: Book.title) -> List<Title> {
    from: Book
    where: Book.title == title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `titles` reads `Book` records but returns `List<Title>`
 --> main.jadpo:12:36
   |
12 | query titles(title: Book.title) -> List<Title> {
   |                                    ^^^^^^^^^^^ not records of the entity
  reason: A query's result is `Book` or an object type its fields fill (`type BookView = Output { pick: [...] }`), as `T` for exactly one record, `T?` for at most one, `List<T>` or `Page<T>`.
  next: Return `Book`, or an `Output` type that picks the fields you need (agent)
  rule: query.result_type
  help: docs/language/06-reads-and-writes.html#query-result-type

check failed: 1 error, 1 file
```

### `query.result_field`

```jadpo check project=e-result-field expect=query.result_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookCard = Output {
    pick: [Book.id, Book.title]
    author: Title
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `BookCard.author` isn't a field of `Book`, or an include
 --> main.jadpo:17:18
   |
17 | query cards() -> List<BookCard> {
   |                  ^^^^^^^^^^^^^^ `author` can't be filled
  reason: A query fills its result's fields from `Book`'s fields of the same name (`id`, `title`, `subtitle` and `available`) and from its includes; nothing gives `author`.
  next: Remove `author` from `BookCard`, or include records under it (agent)
  rule: query.result_field
  help: docs/language/06-reads-and-writes.html#query-result-field

check failed: 1 error, 1 file
```

### Conditions

### `query.unknown_field`

```jadpo check project=e-unknown-field expect=query.unknown_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: Book.titel == title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `Book` has no field `titel`
 --> main.jadpo:14:17
   |
14 |     where: Book.titel == title
   |                 ^^^^^ not declared
  reason: `Book`'s fields are `id`, `title`, `subtitle` and `available`. Did you mean `title`?
  next: Use `title` instead of `titel` (agent)
  rule: query.unknown_field
  help: docs/language/06-reads-and-writes.html#query-unknown-field

check failed: 1 error, 1 file
```

### `query.condition_unsupported`

```jadpo check project=e-condition-unsupported expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query books_after(title: Book.title) -> List<Book> {
    from: Book
    where: Book.title > title
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `Book.title > title` isn't a condition a query can use
 --> main.jadpo:14:12
   |
14 |     where: Book.title > title
   |            ^^^^^^^^^^^^^^^^^^ not an indexable condition
  reason: `>` orders numbers, times, dates and durations, as it does in a body, and `Book.title` is `Title`.
  next: Compare `Book.title` with `==`, `!=` or `in` (agent)
  rule: query.condition_unsupported
  help: docs/language/06-reads-and-writes.html#query-condition-unsupported

check failed: 1 error, 1 file
```

### `query.value_unsupported`

```jadpo check project=e-value-unsupported expect=query.value_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

query longer_manuals(manual_pages: Manual.page_count) -> List<Manual> {
    from: Manual
    where: Manual.page_count > manual_pages * 2
    order: [Manual.page_count asc]
    limit: 20
}
```

```text output
error: `manual_pages * 2` is worked out, and a query computes nothing
 --> main.jadpo:24:32
   |
24 |     where: Manual.page_count > manual_pages * 2
   |                                ^^^^^^^^^^^^^^^^ computed
  reason: A query's values are its parameters and their fields, literals, enum values, lists of those and `clock.now`; anything worked out from them is computed before the call.
  next: Work the value out in the action, and pass it as a parameter (agent)
  rule: query.value_unsupported
  help: docs/language/06-reads-and-writes.html#query-value-unsupported

check failed: 1 error, 1 file
```

### `query.optional_value`

```jadpo check project=e-optional-value expect=query.optional_value
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type FindBooks = Input {
    title: Title {
        optional: true
    }
}

query find_books(filter: FindBooks) -> List<Book> {
    from: Book
    where: Book.title == filter.title
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `filter.title` may be absent, so this `where:` line can't always be met
 --> main.jadpo:20:26
   |
20 |     where: Book.title == filter.title
   |                          ^^^^^^^^^^^^ may be `none`
  reason: A `where:` line applies to every call; a condition that should apply only when its value is there, as a request's optional filter, is written with `where_present:`.
  next: Write `where_present:` for this condition (agent)
  rule: query.optional_value
  help: docs/language/06-reads-and-writes.html#query-optional-value

check failed: 1 error, 1 file
```

### `query.not_unique`

```jadpo check project=e-not-unique expect=query.not_unique
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query book_titled(title: Book.title) -> Book? {
    from: Book
    where: Book.title == title
}
```

```text output
error: A query returning `Book?` could match several records
 --> main.jadpo:14:5
   |
14 |     where: Book.title == title
   |     ^^^^^ not a unique field
  reason: `Book?` is at most one record, so a `where:` line must test a unique field with `==`: `Book.id`.
  next: Test a unique field with `==`, or return `List<T>` (agent)
  rule: query.not_unique
  help: docs/language/06-reads-and-writes.html#query-not-unique

check failed: 1 error, 1 file
```

### Order, limits and pages

### `query.order_direction`

```jadpo check project=e-order-direction expect=query.order_direction
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.title]
    limit: 20
}
```

```text output
error: `Book.title` says no direction
 --> main.jadpo:14:13
   |
14 |     order: [Book.title]
   |             ^^^^^^^^^^ `asc` or `desc`?
  reason: Each field of `order:` says which way it sorts: `asc` from the smallest, `desc` from the largest.
  next: Write `Book.title asc` or `Book.title desc` (agent)
  rule: query.order_direction
  help: docs/language/06-reads-and-writes.html#query-order-direction

check failed: 1 error, 1 file
```

### `query.order_field`

```jadpo check project=e-order-field expect=query.order_field
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.available desc]
    limit: 20
}
```

```text output
error: Records can't be sorted by `Book.available`
 --> main.jadpo:14:13
   |
14 |     order: [Book.available desc]
   |             ^^^^^^^^^^^^^^^^^^^ `Bool`
  reason: A query sorts by numbers, times, dates, durations, text and identities, which the database and Jadpo order the same way; `Book.available` is `Bool`.
  next: Sort by another field (agent)
  rule: query.order_field
  help: docs/language/06-reads-and-writes.html#query-order-field

check failed: 1 error, 1 file
```

### `query.limit_range`

```jadpo check project=e-limit-range expect=query.limit_range
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: 0
}
```

```text output
error: `limit: 0` would read nothing
 --> main.jadpo:15:12
   |
15 |     limit: 0
   |            ^ below 1
  reason: `limit:` is how many records come back at most, so it is at least 1.
  next: Give `limit:` a number from 1 (agent)
  rule: query.limit_range
  help: docs/language/06-reads-and-writes.html#query-limit-range

check failed: 1 error, 1 file
```

### `query.limit_unbounded`

```jadpo check project=e-limit-unbounded expect=query.limit_unbounded
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type Count = Int {
    min: 1
}

query shelf(count: Count) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: count
}
```

```text output
error: `count` has no maximum, so the limit has no bound
 --> main.jadpo:19:12
   |
19 |     limit: count
   |            ^^^^^ `Count` has no `max:`
  reason: Every list is bounded: `limit:` is a number, or a value whose type has a `max:`, such as the built-in `PageSize` (1 to 100).
  next: Type the value as `PageSize`, or as an `Int` type of your own with a `max:` (agent)
  rule: query.limit_unbounded
  help: docs/language/06-reads-and-writes.html#query-limit-unbounded

check failed: 1 error, 1 file
```

### `query.page_setting`

```jadpo check project=e-page-setting expect=query.page_setting
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> Page<Book> {
    from: Book
    order: [Book.title asc]
    page: {
        size: 20
    }
}
```

```text output
error: `page:` has no `after:`
 --> main.jadpo:15:5
   |
15 |     page: {
   |     ^^^^ no `after:`
  reason: A page continues `after:` the cursor the previous page gave in its `next`; on the first call that cursor is absent.
  next: Add `after: ...` to the block (agent)
  rule: query.page_setting
  help: docs/language/06-reads-and-writes.html#query-page-setting

check failed: 1 error, 1 file
```

### Includes

### `query.include_unknown`

```jadpo check project=e-include-unknown expect=query.include_unknown
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type ShelfCard = Output {
    pick: [Book.id]
    name: Title
}

query cards() -> List<ShelfCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
    include: {
        name: Book.title
    }
}
```

```text output
error: `Book.title` isn't a reference to or from `Book`
 --> main.jadpo:22:15
   |
22 |         name: Book.title
   |               ^^^^^^^^^^ not a reference
  reason: An include follows a reference: one of `Book`'s own gives the record it points at, and another entity's reference to `Book` gives the records that point here. No reference leads to or from `Book`.
  next: Add a reference field, or read the records with a query of their own (agent)
  rule: query.include_unknown
  help: docs/language/06-reads-and-writes.html#query-include-unknown

check failed: 1 error, 1 file
```

### `query.include_result`

```jadpo check project=e-include-result expect=query.include_result
type Name = Text {
    max_length: 80
}

entity Author {
    id: Uuid
    name: Name
}

entity Book {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
    title: Name
}

type BookCard = Output {
    pick: [Book.id, Book.title]
    author: List<Author>
}

query cards() -> List<BookCard> {
    from: Book
    order: [Book.title asc]
    limit: 20
    include: {
        author: Book.author_id
    }
}
```

```text output
error: `BookCard.author` is `List<Author>`, which the include `author` can't fill
 --> main.jadpo:28:9
   |
28 |         author: Book.author_id
   |         ^^^^^^ needs `Author`
  reason: `author` follows a required reference, so the one record it points at is always there. Its field is `Author`, where `Author` may also be an object type derived from it.
  next: Declare `author: Author` in `BookCard` (agent)
  rule: query.include_result
  help: docs/language/06-reads-and-writes.html#query-include-result

check failed: 1 error, 1 file
```

### `query.include_depth`

```jadpo check project=e-include-depth expect=query.include_depth
type Name = Text {
    max_length: 80
}

entity Country {
    id: Uuid
    name: Name
}

entity Publisher {
    id: Uuid
    country_id: Country.id {
        on_delete: OnDelete.restricted
    }
}

entity Author {
    id: Uuid
    publisher_id: Publisher.id {
        on_delete: OnDelete.restricted
    }
}

entity Book {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
}

type CountryView = Output {
    pick: [Country.name]
}

type PublisherView = Output {
    country: CountryView
}

type AuthorView = Output {
    publisher: PublisherView
}

type BookView = Output {
    pick: [Book.id]
    author: AuthorView
}

query books() -> List<BookView> {
    from: Book
    order: [Book.id asc]
    limit: 20
    include: {
        author: {
            via: Book.author_id
            include: {
                publisher: {
                    via: Author.publisher_id
                    include: {
                        country: Publisher.country_id
                    }
                }
            }
        }
    }
}
```

```text output
error: Includes nest at most two levels
 --> main.jadpo:58:21
   |
58 |                     include: {
   |                     ^^^^^^^ a third level
  reason: Each include is one more database query, and a query's includes nest up to two levels so its cost stays easy to see; a deeper shape is a second query called from the action.
  next: Read the deeper records with a query of their own (human)
  rule: query.include_depth
  help: docs/language/06-reads-and-writes.html#query-include-depth

check failed: 1 error, 1 file
```

### Entity operations

### `data.read_in_loop`

```jadpo check project=e-read-in-loop expect=data.read_in_loop
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

type BookNotFound = Failure {
    kind: FailureKind.not_found
}

type Count = Int {
    min: 0
}

action available_count(book_ids: List<Book.id>) fails BookNotFound -> Count {
    var mut count: Count = 0
    for book_id in book_ids {
        var book: Book = attempt Book.get(book_id)
        if (book.available) {
            count = Count(count + 1)
        }
    }
    return count
}
```

```text output
error: `Book.get` reads once for every item of the loop
 --> main.jadpo:23:34
   |
23 |         var book: Book = attempt Book.get(book_id)
   |                                  ^^^^^^^^ inside `for`
  reason: A read inside a loop makes one database query per item. Read the whole set with one query, testing the field with `in` a list of the values, before the loop.
  next: Read the records with one query using `in` before the loop (agent)
  rule: data.read_in_loop
  help: docs/language/06-reads-and-writes.html#data-read-in-loop

check failed: 1 error, 1 file
```

### `data.set_generated_field`

```jadpo check project=e-set-generated-field expect=data.set_generated_field
type Title = Text {
    max_length: 80
}

entity Note {
    id: Uuid
    title: Title
    written_at: Instant {
        generated: Generated.on_create
    }
}

action jot_down(title: Note.title, at: Note.written_at) -> Note {
    return Note.create {
        set: {
            title: title
            written_at: at
        }
    }
}
```

```text output
error: `Note.create` sets `written_at`, which the store generates
 --> main.jadpo:17:13
   |
17 |             written_at: at
   |             ^^^^^^^^^^ generated
  reason: `Note.written_at` is generated from the operation's clock; code never assigns it.
  next: Remove `written_at` from `set:` (agent)
  rule: data.set_generated_field
  help: docs/language/06-reads-and-writes.html#data-set-generated-field

check failed: 1 error, 1 file
```

### Writes

Since chunk 3.3. The archived compiler's write errors are gone with its
syntax: a write that isn't an entity's is `data.write_not_entity`; a
section a write doesn't take (`missing:`, `where:`, `empty:`) is
`data.write_section_unexpected`; an update with nothing to change is
`data.write_section_required`; a field given twice or unknown in `set:` is
area 2's `type.duplicate_field` or `type.unknown_field`; and the conflict
mappings are below.

### `data.write_not_entity`

Only an entity is stored, so only an entity has `create`, `update` and
`delete`:

```jadpo check project=e-write-not-entity expect=data.write_not_entity
type DraftName = Text {
    max_length: 80
}

type Draft = Input {
    name: DraftName
}

action save_draft(name: DraftName) -> Draft {
    return Draft.create {
        set: {
            name: name
        }
    }
}
```

```text output
error: `Draft` isn't an entity, so it has no `create`
 --> main.jadpo:10:12
   |
10 |     return Draft.create {
   |            ^^^^^^^^^^^^ an input
  reason: Only an entity is stored, so only an entity has `create`, `update` and `delete`; `Draft` is an input, a value built with `Draft { ... }`.
  next: Write the entity this value is stored as, or build the value with `Draft { ... }` (agent)
  rule: data.write_not_entity
  help: docs/language/06-reads-and-writes.html#data-write-not-entity

check failed: 1 error, 1 file
```

### `data.write_section_unexpected`

A create takes `set:` and `conflict:`, an update `set:` or `patch:` (with
`when_supplied:`) and `conflict:`, a delete `conflict:`, each once. A
record that isn't there reports the entity's not-found failure itself:

```jadpo check project=e-write-section-unexpected expect=data.write_section_unexpected
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemMissing = Failure {
    kind: FailureKind.not_found
}

action rename(item_id: Item.id, name: Item.name) fails ItemNotFound -> Item {
    return attempt Item.update(item_id) {
        set: {
            name: name
        }
        missing: ItemMissing
    }
}
```

```text output
error: `Item.update` takes no `missing:`
 --> main.jadpo:19:9
   |
19 |         missing: ItemMissing
   |         ^^^^^^^ not a section here
  reason: A record that isn't there reports `ItemNotFound` itself; `missing:` maps a query's "nothing matched".
  next: Remove `missing: ...` (agent)
  rule: data.write_section_unexpected
  help: docs/language/06-reads-and-writes.html#data-write-section-unexpected

check failed: 1 error, 1 file
```

### `data.write_section_required`

```jadpo check project=e-write-section-required expect=data.write_section_required
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action touch(item: Item) -> Item {
    return Item.update(item)
}
```

```text output
error: `Item.update` doesn't say what it changes
 --> main.jadpo:11:12
   |
11 |     return Item.update(item)
   |            ^^^^^^^^^^^ nothing to change
  reason: An update gives the fields it changes in `set:`, or the request's changes with `patch:`.
  next: Add `set: { ... }` with the fields it changes (agent)
  rule: data.write_section_required
  help: docs/language/06-reads-and-writes.html#data-write-section-required

check failed: 1 error, 1 file
```

### `data.update_by_id_value`

An update by `id` is worked out by the database in one step, so its values
use only parameters, constants, `clock.now` and the record's stored fields,
and a stored field only with `+` and `-`:

```jadpo check project=e-update-by-id-value expect=data.update_by_id_value
type Count = Int {
    min: 0
}

entity Counter {
    id: Uuid
    hits: Count
}

action hit(counter_id: Counter.id) fails CounterNotFound -> Counter {
    var counter: Counter = attempt Counter.get(counter_id)
    return attempt Counter.update(counter_id) {
        set: {
            hits: Count(counter.hits + 1)
        }
    }
}
```

```text output
error: `Counter.update` by `id` sets a value from `counter`, a variable
 --> main.jadpo:14:25
   |
14 |             hits: Count(counter.hits + 1)
   |                         ^^^^^^^ a variable
  reason: An update by `id` is worked out by the database in one step, so its values use only the action's parameters, constants, `clock.now` and `Counter`'s stored fields; a value read earlier could have changed by the time it writes.
  next: Load the record and update it, `Counter.update(counter) { set: { ... } }`, or pass the value in as a parameter (agent)
  rule: data.update_by_id_value
  help: docs/language/06-reads-and-writes.html#data-update-by-id-value

check failed: 1 error, 1 file
```

### `data.stored_value_outside_update`

`Counter.hits` is the value stored when an update by `id` runs, so only
that update reads it:

```jadpo check project=e-stored-value-outside-update expect=data.stored_value_outside_update
type Count = Int {
    min: 0
}

entity Counter {
    id: Uuid
    hits: Count
}

action hit(counter: Counter) -> Counter {
    return Counter.update(counter) {
        set: {
            hits: Count(Counter.hits + 1)
        }
    }
}
```

```text output
error: `Counter.hits` is a stored value, read only by an update by `id`
 --> main.jadpo:13:25
   |
13 |             hits: Count(Counter.hits + 1)
   |                         ^^^^^^^^^^^^ stored value
  reason: `Counter.hits` is the value stored in the database when an update by `id` runs, which only that update's `set:` reads; elsewhere a record's field is read from the record loaded earlier.
  next: Read it from a loaded record, `counter.hits`, or use it in `Counter.update(counter_id) { set: { ... } }` (agent)
  rule: data.stored_value_outside_update
  help: docs/language/06-reads-and-writes.html#data-stored-value-outside-update

check failed: 1 error, 1 file
```

### `data.stored_value_unproven`

Nothing checks a value the database works out before it is stored, so the
declared ranges must prove it:

```jadpo check project=e-stored-value-unproven expect=data.stored_value_unproven
type Stock = Int {
    min: 0
    max: 1000
}

entity Product {
    id: Uuid
    stock: Stock
}

action restock(product_id: Product.id) fails ProductNotFound -> Product {
    return attempt Product.update(product_id) {
        set: {
            stock: Stock(Product.stock + 1)
        }
    }
}
```

```text output
error: `Product.update` by `id` could store a value that breaks `max: 1000`
 --> main.jadpo:14:20
   |
14 |             stock: Stock(Product.stock + 1)
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^ worked out by the database
  reason: The database works out a value from `Product`'s stored fields when it writes, so nothing checks it before it is stored: the declared ranges must prove it follows its type's rules.
  next: Load the record and build the value with `attempt` before `Product.update`, or loosen the type's rules (human)
  rule: data.stored_value_unproven
  help: docs/language/06-reads-and-writes.html#data-stored-value-unproven

check failed: 1 error, 1 file
```

### Conflicts

### `data.conflict_unmapped`

Each unique rule and restricting reference a write can break needs an arm
in `conflict:`; there is no catch-all:

```jadpo check project=e-conflict-unmapped expect=data.conflict_unmapped
entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
}

action register(email: Customer.email) -> Customer {
    return Customer.create {
        set: {
            email: email
        }
    }
}
```

```text output
error: `Customer.create` can break `Customer.email`, and nothing maps it
 --> main.jadpo:9:12
  |
9 |     return Customer.create {
  |            ^^^^^^^^^^^^^^^ can be refused
  reason: A write is refused when it would give two records the same values a unique rule covers; it needs an arm in `conflict:` naming the `conflict` failure the caller gets, since there is no catch-all.
  next: Add `conflict: { Customer.email: SomeFailure }` to the block (agent)
  rule: data.conflict_unmapped
  help: docs/language/06-reads-and-writes.html#data-conflict-unmapped

check failed: 1 error, 1 file
```

### `data.conflict_unknown`

```jadpo check project=e-conflict-unknown expect=data.conflict_unknown
entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
}

type EmailTaken = Failure {
    kind: FailureKind.conflict
}

action register(email: Customer.email) fails EmailTaken -> Customer {
    return attempt Customer.create {
        set: {
            email: email
        }
        conflict: {
            Customer.mail: EmailTaken
        }
    }
}
```

```text output
error: `Customer.mail` names no unique rule or reference
 --> main.jadpo:18:13
   |
18 |             Customer.mail: EmailTaken
   |             ^^^^^^^^^^^^^ not a rule
  reason: A `conflict:` arm is keyed by the rule the store refuses, written `Entity.name`: a field with `unique: true`, a rule named in `unique:`, or a `restricted` reference that refuses a delete. `Customer.create` can break `Customer.email`. Did you mean `Customer.email`?
  next: Use `Customer.email` instead of `Customer.mail` (agent)
  rule: data.conflict_unknown
  help: docs/language/06-reads-and-writes.html#data-conflict-unknown

check failed: 1 error, 1 file
```

### `data.conflict_cannot_break`

An update can break only the unique rules over the fields it sets:

```jadpo check project=e-conflict-cannot-break expect=data.conflict_cannot_break
type CustomerName = Text {
    max_length: 80
}

entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
    name: CustomerName
}

type EmailTaken = Failure {
    kind: FailureKind.conflict
}

action rename(customer: Customer, name: Customer.name) fails EmailTaken -> Customer {
    return attempt Customer.update(customer) {
        set: {
            name: name
        }
        conflict: {
            Customer.email: EmailTaken
        }
    }
}
```

```text output
error: `Customer.update` can't break `Customer.email`, so it takes no arm
 --> main.jadpo:23:13
   |
23 |             Customer.email: EmailTaken
   |             ^^^^^^^^^^^^^^ never runs
  reason: `Customer.update` sets none of the fields `Customer.email` covers, so it can't break it.
  next: Remove `Customer.email: ...` (agent)
  rule: data.conflict_cannot_break
  help: docs/language/06-reads-and-writes.html#data-conflict-cannot-break

check failed: 1 error, 1 file
```

A conflict arm naming a failure of another kind is area 4's
`failure.mapping_kind`, an arm given twice `failure.duplicate_mapping`, and
`invalid:` in a write's block `failure.invalid_outside_construction`.

### Patches

### `data.patch_not_input`

```jadpo check project=e-patch-not-input expect=data.patch_not_input
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

action copy_name(item: Item, from: Item) -> Item {
    return Item.update(item) {
        patch: from
    }
}
```

```text output
error: `patch:` takes an `Input`, and this is `Item`
 --> main.jadpo:12:16
   |
12 |         patch: from
   |                ^^^^ this is `Item`
  reason: A patch changes only the fields a request supplied, so it takes an `Input` whose fields are optional and named after `Item`'s: one left out keeps its value, and one sent as `none` is cleared.
  next: Pass an `Input` such as `type PatchItem = Input { pick: [...]  optional: all }` (agent)
  rule: data.patch_not_input
  help: docs/language/06-reads-and-writes.html#data-patch-not-input

check failed: 1 error, 1 file
```

### `data.patch_field_unknown`

Each field of a patch's input is one of the entity's, and `when_supplied:`
names the input's fields:

```jadpo check project=e-patch-field-unknown expect=data.patch_field_unknown
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemChanges = Input {
    title: ItemName {
        optional: true
    }
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

```text output
error: `ItemChanges` has `title`, which `Item` doesn't
 --> main.jadpo:18:16
   |
18 |         patch: amendment
   |                ^^^^^^^^^ `ItemChanges`
  reason: A patch sets each field its input supplies on the field of `Item` with the same name, so every field of `ItemChanges` is one of `Item`'s.
  next: Remove `title` from `ItemChanges` (agent)
  rule: data.patch_field_unknown
  help: docs/language/06-reads-and-writes.html#data-patch-field-unknown

check failed: 1 error, 1 file
```

### `data.patch_field_required`

```jadpo check project=e-patch-field-required expect=data.patch_field_required
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
}

type ItemChanges = Input {
    pick: [Item.name]
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
    }
}
```

```text output
error: `ItemChanges.name` isn't optional, so a patch can't leave it out
 --> main.jadpo:16:16
   |
16 |         patch: amendment
   |                ^^^^^^^^^ `ItemChanges`
  reason: A patch changes only the fields a request supplied, so each field of its input is optional: one left out keeps its value. A required `name` would have to be sent every time.
  next: Make `name` optional in `ItemChanges` (`optional: true`, or `optional: all`), or set it with `set:` (agent)
  rule: data.patch_field_required
  help: docs/language/06-reads-and-writes.html#data-patch-field-required

check failed: 1 error, 1 file
```

### `data.patch_overlap`

```jadpo check project=e-patch-overlap expect=data.patch_overlap
type ItemName = Text {
    max_length: 80
}

entity Item {
    id: Uuid
    name: ItemName
    nickname: ItemName?
}

type ItemChanges = Input {
    pick: [Item.name, Item.nickname]
    optional: all
}

action edit(item: Item, amendment: ItemChanges) -> Item {
    return Item.update(item) {
        patch: amendment
        when_supplied: {
            name: {
                nickname: none
            }
        }
    }
}
```

```text output
error: `nickname` is set by the patch and by `when_supplied:`
 --> main.jadpo:21:17
   |
21 |                 nickname: none
   |                 ^^^^^^^^ set twice
  reason: `ItemChanges` supplies `nickname` already, so a value here would compete with the request's.
  next: Remove `nickname` from `when_supplied:`, or from `ItemChanges` (agent)
  rule: data.patch_overlap
  help: docs/language/06-reads-and-writes.html#data-patch-overlap

check failed: 1 error, 1 file
```

### Stores

### `store.action_spans_stores`

An action writes one store, counting the actions it calls, so moving an
entity can break an action that never names it:

```jadpo check project=e-action-spans-stores file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        carts: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/entities/order.jadpo hidden
entity Order {
    id: Uuid
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/entities/cart.jadpo hidden migrate=initial
entity Cart {
    id: Uuid

    store: carts
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/actions/clear_cart.jadpo hidden
action clear_cart(cart: Cart) -> Unit {
    Cart.delete(cart)
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/actions/checkout.jadpo expect=store.action_spans_stores
action checkout(cart: Cart) -> Order {
    var order: Order = Order.create {
        set: {}
    }
    clear_cart(cart)
    return order
}
```

```text output
error: `checkout` writes two stores
 --> domains/ordering/actions/checkout.jadpo:5:5
  |
5 |     clear_cart(cart)
  |     ^^^^^^^^^^^^^^^^ store `carts`
  reason: An action's writes, and those of the actions it calls, commit as one, in one store's transaction; `checkout` writes `Order` (store `main`), and here it calls `clear_cart`, which writes `Cart` (store `carts`).
  next: Write `Cart` in a subscriber to an event `checkout` emits, or keep `Cart` in store `main` (human)
  rule: store.action_spans_stores
  help: docs/language/06-reads-and-writes.html#store-action-spans-stores

check failed: 1 error, 5 files
```

### Ownership, transactions and freshness

### `data.raw_query_outside_named_query`

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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

> The archived compiler's error: the rework drops entity-owned operations, `freshness:` and `consistency:`, so this mistake no longer exists in the new language.

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
   alternatives. *Decided 2026-10-12:* yes, as recommended.
2. **How are filters written?** *Recommendation:* fields qualified with
   their entity; each `where:` line must hold; comparisons, `and`, `or`,
   `not`, `== none` and `in`; `where_present:` for a request's optional
   filters, replacing `matches optional` and `<= optional`; only conditions
   an index can serve. *Decided 2026-10-12:* yes, as recommended.
3. **How are order and pages written?** *Recommendation:* `order: [field
   direction]` with `id` added as the tie-break, so
   `type.query_order_not_deterministic` goes; every list bounded by a
   literal or a type's `max:`; keyset pages only, through the standard
   `Page<T>` and an opaque `PageCursor`; no offset. Relay and Prisma
   both recommend cursors over offsets for the same reasons. *Decided 2026-10-12:* yes, as recommended.
4. **How are children and parents included?** *Recommendation:* an
   `include:` block whose entries name a reference field: this entity's own
   reference gives the parent, another entity's reference gives children (a
   list needing `order:` and `limit:`, or one record when the reference is
   unique). Nesting up to two levels, within the domain, and across stores
   only by `id`. Other domains' data comes from their public queries. *Decided 2026-10-12:* yes, as recommended.
5. **Which operations does every entity get?** *Recommendation:* `get`,
   `create`, `update` (with `set:` or `patch:`), `delete`, and one per
   lifecycle transition, called through the entity; only actions in the
   owning domain may call the writing ones, which replaces today's rule
   that only entity-owned actions write. *Decided 2026-10-12:* yes, as recommended.
6. **What do failed writes report?** *Recommendation:* a missing record
   reports the entity's `<Entity>NotFound`, provided by the compiler unless
   the domain declares it; every unique rule and `restrict` reference a
   write can break is mapped in `conflict:`, keyed by the rule, with no
   catch-all and no `TodoMutationConflict`. Rails and Django raise one
   generic error per constraint type and leave mapping to the developer;
   here the compiler knows the rules, so it can insist. *Decided 2026-10-12:* yes, as recommended.
7. **Does an empty patch fail?** *Recommendation:* no: it changes nothing
   and succeeds, as JSON Merge Patch (RFC 7396) specifies, so `empty:` and
   `failure.patch_empty_not_invalid_value` go. Conditional writes become
   `when_supplied: { field: { ... } }`, replacing `x when input.f supplied`. *Decided 2026-10-12:* yes, as recommended.
8. **What happens when a record changed since it was loaded?**
   *Recommendation:* the runtime reruns the whole action within the
   existing retry budget, then fails `unavailable`; on PostgreSQL through
   repeatable-read isolation. Rails (`lock_version`) and Entity Framework
   raise an error the developer must handle; Jadpo can rerun because an
   action has no outside effects before it commits. *Decided 2026-10-12:* yes, as recommended.
9. **How are counters updated?** *Recommendation:* an update by `id` whose
   values use `Entity.field` for the stored value
   (`Post.view_count + 1`), computed by the database in one step and never
   rerun; PostgreSQL's own documentation recommends exactly this for
   increments. *Decided 2026-10-12:* yes, as recommended.
10. **Are children hidden with their parent?** Today they stay visible, and
    a required include of a hidden parent reads as missing.
    *Recommendation:* visibility never spreads: a child stays visible until
    its own lifecycle hides it, and an include of its parent returns the
    parent even when hidden, because the reference still holds. The action
    that hides a parent runs its children's transitions when they should go
    too. Rails' `paranoia` gem hides deleted parents from associations by
    default and the resulting `nil` parents are its most common complaint. *Decided 2026-10-12:* yes, as recommended.
11. **Are reads inside a loop allowed?** *Recommendation:* no: a query or
    `get` inside `for` is a compile error, and the fix is one query with
    `in`. Rails needed `strict_loading` to catch the same "one read per
    item" mistake after the fact. *Decided 2026-10-12:* yes, as recommended.
12. **Where may a cache serve a read?** *Recommendation:* only a query a
    route runs directly; every read inside an action, policy and
    authentication goes to the store, and a client that just wrote skips
    caches by its revision. This makes the call site, not a `freshness:`
    setting, decide, as area 3 decided. *Decided 2026-10-12:* yes, as recommended.
13. **Do policy conditions shape query plans and indexes, and is PostgreSQL
    row-level security generated?** (carried from area 9) Area 9 sends
    generated query plans, indexes for policy conditions, and row-level
    security as optional defence in depth generated from the same policy to
    this page. Which of these does the compiler generate?
    *Decided 2026-10-12 (row-level security only):* not now. Policy compiled
    into each query's `WHERE` is the protection; row-level security comes
    later as an opt-in setting on a PostgreSQL store, generated from the same
    policy. *Decided 2026-10-12 (the rest):* fields a policy condition
    filters on get indexes, as `where:` fields do.
14. **How do caches behave in tests?** (carried from area 13) Once the Redis
    adapter exists, do tests read through declared caches or straight from
    the store, and how does a test check a cached read? *Decided 2026-10-12:*
    `jadpo test` supplies an in-memory cache that behaves like Redis and is
    emptied between tests, so tests run production's code paths; a test
    checks a cached read by reading, writing elsewhere and reading again.

### Decisions

The rows marked "Carried" were decided in earlier areas and bind this page.

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
| Query shape | One declarative read: `from:`, stacked `where:`, `order:`, `limit:` or `page:`, `include:`, `missing:`; the result type sets how many records come back | 2026-10-12 |
| Filters | Fields qualified with their entity; every `where:` line must hold; comparisons, `and`, `or`, `not`, `== none`, `in`; `where_present:` for optional filters; only conditions an index can serve | 2026-10-12 |
| Selected columns | The generated SQL selects only the columns the result type needs, plus `order:` fields for the cursor and fields a policy checks outside the database | 2026-10-12 |
| Page types | `Page<T>` (items and next), `PageCursor` and `PageSize` (1 to 100) are built in; an app may declare its own `Int` type with a `max:` for a different limit | 2026-10-12 |
| Row-level security | Not now; later an opt-in setting on a PostgreSQL store, generated from policy. The compiled `WHERE` is the protection | 2026-10-12 |
| Order and pages | `order: [field direction]` with `id` added as the tie-break; every list bounded by a literal or a type's `max:`; keyset pages only, no offset | 2026-10-12 |
| Includes | `include:` names a reference field: own reference gives the parent, another entity's gives children (ordered and limited); two levels, within the domain, across stores only by `id` | 2026-10-12 |
| Entity operations | `get`, `create`, `update` (`set:` or `patch:`), `delete` and one per lifecycle transition; only actions in the owning domain call the writing ones | 2026-10-12 |
| Write failures | Missing record gives `<Entity>NotFound` (provided unless declared); every unique rule and restricted reference a write can break mapped in `conflict:`, no catch-all | 2026-10-12 |
| Empty patch | Succeeds and changes nothing (RFC 7396); conditional writes use `when_supplied: { }` | 2026-10-12 |
| Concurrent change | The runtime reruns the whole action within the retry budget, then fails `unavailable`; repeatable-read isolation on PostgreSQL | 2026-10-12 |
| Counters | An update by `id` using `Entity.field` for the stored value (`Post.view_count + 1`), done by the database in one step | 2026-10-12 |
| Hidden parents | Visibility never spreads; an include of a hidden parent returns it; the action that hides a parent runs its children's transitions when they should go too | 2026-10-12 |
| Reads in loops | A query or `get` inside `for` is a compile error; the fix is one query with `in` | 2026-10-12 |
| Cache reads | Only a query a route runs directly may be served from a cache; reads in actions, policy and authentication go to the store; a client that just wrote skips caches | 2026-10-12 |
| Policy in query plans | Policy compiled into each query's `WHERE`; fields a policy condition filters on get indexes; row-level security later, opt-in | 2026-10-12 |
| Caches in tests | `jadpo test` supplies an in-memory cache that behaves like Redis, emptied between tests | 2026-10-12 |
| Write targets | Chosen in chunk 3.3: `update` and `delete` take the record loaded earlier or its `id`; by `id` no record is the entity's not-found failure; a delete gives nothing | 2026-10-14 |
| Update sections | Chosen in chunk 3.3: `set:` or `patch:`, never both; `when_supplied:` only with `patch:`, never setting a field the patch has or one another entry sets | 2026-10-14 |
| Provided not-found failure | Chosen in chunk 3.3: kind `not_found`, public, no fields, placed at its entity | 2026-10-14 |
| References that hold | Chosen in chunk 3.3: a reference's value read from a record loaded earlier (its `id`, or its reference to the same entity) or `none` names a record that is there; any other value may not, and the write can fail with that entity's not-found failure | 2026-10-14 |
| Stored values | Chosen in chunk 3.3: an update by `id` works a stored field out only with `+` and `-`, under constructions the declared ranges prove | 2026-10-14 |
| Refusals in a called action | Chosen in chunk 3.3: a write that can be refused, run by an action another action called, runs inside a savepoint, so a caller handling the failure carries on | 2026-10-14 |
