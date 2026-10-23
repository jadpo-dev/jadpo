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

> Decided on 2026-10-12. Built in chunk 3.2: queries (filters, order, pages, includes and the columns a result needs), `Entity.get`, the rule against reads in a loop, and `jadpo sql`. Built in chunk 3.3: `create`, `update` (with `set:` or `patch:`, of a loaded record or by `id`) and `delete`, `conflict:` mappings, the not-found failure the compiler provides, counters, one store per action and the rerun after a concurrent change. The examples marked `check` compile with this checkout's `jadpo`, and the runtime runs them against PostgreSQL. Built in chunk 3.4: lifecycles, so `visible:` hides records from every read and write, transitions run as operations, and `delete` runs the lifecycle's `delete` transition. Built in chunk 3.7: each action's isolation level, read committed or repeatable read, decided from what it does. Built in chunk 5.6c: aggregate queries, `count`, `sum`, `min`, `max` and `average` grouped by a field or a time bucket, each one statement ("Counts, sums and groups"). Built in chunk 5.5u8: a query giving a map layer, a `FeatureCollection<G, P>` a `GET` route sends ("A query giving a map layer"). Built in chunk 5.8c: a result type with a rule across its fields is refused (`query.result_rule`). Built in chunk 6.3b: a standard operation with an exact SQL form in a condition, with an index on its expression ("Operations in conditions"). The archived compiler's syntax is collapsed beneath each section that changed. The query syntax is compared with other frameworks in [the query proposals](research/06-reads-and-writes-proposals.md).

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
| `FeatureCollection<Point, TodoPin>` | a map layer of up to a fixed number, each record a feature ("A query giving a map layer") | `geometry:`, `order:` and `limit:` |

The element type is the entity or a type derived from it
(`type TodoView = Output { pick: [Todo.id, Todo.title] }`, area 2); the compiler
reads only the fields that type needs. It names no rule across its fields
(area 2), proved by the declared ranges or not, and neither does whatever
else stands for its records: a `Nearest<T>`'s item, a map layer's
properties, an aggregate's rows or the records an include brings
([`query.result_rule`](#query-result-rule)). A rule is checked whenever a
value is made, and a broken one is a failure an `attempt` maps; a read has
no `attempt` around it and the database can't check the rule as it reads,
so a stored record that broke it could only be a defect. A type with the
same fields and no rule fills a result, and code builds the type with the
rule from it where an `attempt` maps the failure. Records hidden by the entity's
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
named first, compared with a value. Two fields compared and a value
worked out in the query are refused, and so is a function applied to a
field, but for the operations below; values
are parameters and their fields, literals, enum values, lists of those and
`clock.now`. A constant (area 3, [Constants](03-logic.md#constants)) is
accepted wherever a literal is, since it is a value fixed while compiling
(decided 2026-10-16, built in chunk 5.3e). `<` and the other orderings take numbers, times, dates and
durations, as they do in a body (area 3). A field may be passed to three
kinds of operation. `ai.distance` (area 21) is compared with a bound. A
`geo.` test (`geo.within`, `geo.contains` or `geo.intersects`) takes a
geometry field first and a parameter after, on a `where:` line of its
own or joined by `and`, and a GiST index serves it: the index finds the
records and Jadpo's own code decides each, so the query gives the code's
answer ([area 20](20-geospatial.md), "Queries", built in chunk 5.5u5).
And a standard operation with an exact SQL form stands where the field
would ("Operations in conditions", below). A field that is `none` meets no
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

### Operations in conditions

A standard operation may stand where a field does when area 19 gives it
an exact SQL form ([area 19](19-standard-library.md), "SQL forms"): the
field is its first argument, and every other argument is written out, a
literal, a constant or an enum value such as `Rounding.half_even`. The
database works the operation out with that form, which gives exactly what
the operation gives in code, so the query keeps the records `list.filter`
would keep with the same operation (decided 2026-10-19, built in chunk
6.3b). Its result is compared with a value as a field's is, by `==`, `!=`,
`in` and, for numbers, times, dates and durations, the orderings; an
operation that gives a `Bool` is the condition itself.

```jadpo check project=operation-conditions file=app.jadpo hidden
application {}
```

```jadpo check project=operation-conditions file=domains/library/types/book_title.jadpo hidden
/// A book's title, as printed on its cover.
type BookTitle = Text {
    max_length: 200
}
```

```jadpo check project=operation-conditions file=domains/library/types/publication_year.jadpo hidden
/// A year a book can have been first published in.
type PublicationYear = Int {
    min: 1450
    max: 2100
}
```

```jadpo check project=operation-conditions file=domains/library/types/publication_day.jadpo hidden
/// The day a book was first published.
type PublicationDay = CalendarDate {}
```

```jadpo check project=operation-conditions file=domains/library/entities/book.jadpo
use library/types { BookTitle, PublicationDay }

/// A book in the catalogue.
entity Book {
    id: Uuid
    title: BookTitle
    published_on: PublicationDay
}
```

```jadpo check project=operation-conditions file=domains/library/types/book_card.jadpo hidden
use library/entities { Book }

/// A book in a list.
type BookCard = Output {
    pick: [Book.id, Book.title]
}
```

```jadpo check project=operation-conditions file=domains/library/actions/add_book.jadpo hidden
use library/entities { Book }
use library/types { BookTitle, PublicationDay }

/// Adds a book to the catalogue.
action add_book(title: BookTitle, published_on: PublicationDay) -> Book {
    return Book.create {
        set: {
            title: title
            published_on: published_on
        }
    }
}
```

```jadpo check project=operation-conditions file=domains/library/queries/spine_titles.jadpo
use library/entities { Book }
use library/types { BookCard }

/// The books whose title fits on a spine label.
query spine_titles() -> List<BookCard> {
    from: Book
    where: text.length(Book.title) <= 24
    order: [Book.title asc]
    limit: 50
}
```

```jadpo check project=operation-conditions file=domains/library/queries/published_in.jadpo migrate=initial
use library/entities { Book }
use library/types { BookCard, PublicationYear }

/// The books first published in a year.
query published_in(year_published: PublicationYear) -> List<BookCard> {
    from: Book
    where: temporal.year(Book.published_on) == year_published
    order: [Book.title asc]
    limit: 100
}
```

A title is counted in code points, as `text.length` counts it, however
many bytes it takes, and a year ends where `temporal.year` says, run on
PostgreSQL:

```jadpo check project=operation-conditions file=domains/library/queries/spine_titles.test.jadpo
use library/entities { Book }
use library/types { BookCard, BookTitle }
use library/actions { add_book }
use library/queries { published_in }

test "a query keeps the books its operation keeps in code" {
    var dune: Book = add_book(BookTitle("Dune"), "1965-08-01")
    var cafe: Book = add_book(BookTitle("Café Society à la Plage!"), "1965-12-31")
    var left_hand: Book = add_book(BookTitle("The Left Hand of Darkness"), "1969-03-01")
    var spine: List<BookCard> = spine_titles()
    assert text.join(list.map(spine, card -> card.title), ", ") == text.join(
        [cafe.title, dune.title],
        ", "
    )
    assert text.length(cafe.title) == 24
    assert text.length(left_hand.title) == 25
    var sixty_five: List<BookCard> = published_in(1965)
    assert list.count(sixty_five) == 2
    assert list.count(published_in(1969)) == 1
}
```

- **Every other argument is written out:** a literal, a constant, an enum
  value, a rounding mode, `decimal.round`'s `scale:` or `text.pad_start`'s
  `fill:`. Each is held while compiling to what code holds it to (a zero
  divisor, a negative count, empty text to find) and written into the
  statement's text, so the index and the statement hold the same
  expression and PostgreSQL matches them. A value given when the query
  runs would change what the index holds, so a parameter as another
  argument is `query.condition_unsupported`.
- **Only an exact form, and one an index can hold.** An operation without
  a SQL form (`text.lower`, whose result PostgreSQL's collation would
  decide) is `query.condition_unsupported`, and so is one whose form an
  index can't hold: `map.count` and `temporal.period_between` read a
  subquery, and `hash.sha256` and `encoding.to_utf8` of text call
  `convert_to`, which PostgreSQL marks stable since it reads the
  database's encoding. A range's operations take a range written out,
  which a query can't write. A field of money or a range, or one kept
  whole as its JSON (an object, a list, a set, a map or an enum with
  data), isn't compared in a query yet, its operations included
  (`toolchain.not_built_yet`), though `list.count` of a list would
  otherwise have a form.
- **The field first, once.** The field is the operation's first argument,
  given as it is; an operation of another operation's result, or with the
  field anywhere else, is `query.condition_unsupported`.
- **Compared as a field is.** A literal the result is compared with is
  decoded for the result's type. The `not` of an operation that gives a
  `Bool` is written `IS FALSE`, since PostgreSQL turns `NOT (a = b)` into
  `a <> b` before it looks for an index. `== none` is `type.incomparable`:
  an operation gives a value whenever it is called.
- **`none`.** A record whose field is `none` meets no condition on an
  operation of it, `not` included (`"field" IS NOT NULL AND ...`), as code
  can't call the operation on `none`. `ai.distance` keeps its statement,
  since pgvector's operators give `NULL` for `NULL`.
- **Anywhere a comparison stands.** Under `not` and `or`, on a
  `where_present:` line, in an include's `where:` and in an aggregate
  query, since the database works the operation out exactly; `order:`
  still takes fields only. A `geo.` test, which an index finds and code
  decides, still stands on its own line or under `and`, outside includes
  and aggregates.
- **The index.** The compiler derives one B-tree on each expression,
  however many queries and includes compare it, as it derives one for a
  field's test (area 5, "Indexes"): `<table>_<column>_<operation>_idx`,
  another expression of the same operation and field named apart by its
  digest, and partial for `visible:` as every derived index is. The schema
  keeps it as an index's `expression`, beside the column it reads, and a
  migration creates it with `CREATE INDEX ... ((expression))`.

The statements (`jadpo sql`) and the indexes derived for them:

```sql
-- spine_titles
SELECT "id", "title"
FROM "book"
WHERE char_length("title")::bigint <= $1
ORDER BY "title" COLLATE "C" ASC, "id" ASC
LIMIT 50

-- published_in
SELECT "id", "title"
FROM "book"
WHERE (CASE WHEN extract(year FROM "published_on") < 0 THEN extract(year FROM "published_on") + 1 ELSE extract(year FROM "published_on") END)::bigint = $1
ORDER BY "title" COLLATE "C" ASC, "id" ASC
LIMIT 100

CREATE INDEX "book_published_on_year_idx" ON "book" (((CASE WHEN extract(year FROM "published_on") < 0 THEN extract(year FROM "published_on") + 1 ELSE extract(year FROM "published_on") END)::bigint));
CREATE INDEX "book_title_length_idx" ON "book" ((char_length("title")::bigint));
```

A live test (`jadpo-cli`'s `operation_conditions`) runs such queries on
PostgreSQL, edge cases among them, and checks each gives the code's answer
through its index.

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

A vector field is ordered by nearness instead, `order: [F nearest v]`,
the only order of a query returning `List<Nearest<T>>`, with
`ai.distance(F, v)` compared with a bound in `where:` (area 21, "The
nearest-neighbour query", built in chunk 5.5v1b).
A point field is ordered by distance, nearest first, with
`order: [geo.distance(F, p) asc]`, the only order of a list, beside
`where: geo.within(F, p, distance: d)` on the same field and point, which
bounds what is sorted; records are sorted by the distance `geo.distance`
gives, then `id` (area 20, question 4, built in chunk 5.5u5).

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

## Counts, sums and groups

A query can count and add up the records it finds instead of returning
them. `aggregate:` names each total the database works out, and `group:`
splits the records into groups, one row per group. Both blocks are shaped
like `include:`, each entry named after the result field it fills. Each
row is a value of an `Output` (or `Object`) type whose every field is a
group or an aggregate of the same name, and every entry a field; nothing
else of the entity is read, since the records in a group have no one value
of any other field. `count`, `sum`, `min`, `max` and `average` are words of
`aggregate:` only, so they take no namespace and name nothing elsewhere. An API service reports its
calls by month, counts them by how they were replied, and totals an
account's calls:

```jadpo check project=aggregates file=app.jadpo hidden
application {}
```

```jadpo check project=aggregates file=domains/metering/types/call_day.jadpo hidden
/// The day a call was made, in the account's own zone.
type CallDay = CalendarDate {}
```

```jadpo check project=aggregates file=domains/metering/types/spent_tokens.jadpo hidden
/// The tokens a call spent.
type SpentTokens = Int {
    min: 0
}
```

```jadpo check project=aggregates file=domains/metering/types/call_charge.jadpo hidden
/// What a call was charged, in pounds.
type CallCharge = Money {
    currency: Currency.gbp
}
```

```jadpo check project=aggregates file=domains/metering/types/call_wait.jadpo hidden
/// How long a caller waited for an answer.
type CallWait = Duration {}
```

```jadpo check project=aggregates file=domains/metering/types/reply_kind.jadpo hidden
/// How a call was replied.
type ReplyKind = Enum {
    replied
    throttled
    refused
}
```

```jadpo check project=aggregates file=domains/metering/entities/api_call.jadpo migrate=initial
use metering/types { CallCharge, CallDay, CallWait, ReplyKind, SpentTokens }

/// One call an account made to the API.
entity ApiCall {
    id: Uuid
    made_on: CallDay
    tokens_spent: SpentTokens
    charge: CallCharge
    reply: ReplyKind
    wait: CallWait?
}
```

```jadpo check project=aggregates file=domains/metering/types/month_of_calls.jadpo
use metering/entities { ApiCall }

/// A month of calls on an invoice.
type MonthOfCalls = Output {
    call_year: Int
    call_month: Int
    calls_made: Int
    tokens_billed: Int
    amount_billed: Money
    largest_call: ApiCall.tokens_spent
    mean_tokens: Decimal
}
```

```jadpo check project=aggregates file=domains/metering/types/reply_tally.jadpo
use metering/types { ReplyKind }

/// How many calls were replied one way.
type ReplyTally = Output {
    reply: ReplyKind
    calls_made: Int
}
```

```jadpo check project=aggregates file=domains/metering/types/call_summary.jadpo
use metering/entities { ApiCall }

/// An account's calls since a day, all together.
type CallSummary = Output {
    calls_made: Int
    amount_billed: Money
    slowest_wait: ApiCall.wait
    mean_tokens: Decimal?
}
```

```jadpo check project=aggregates file=domains/metering/queries/monthly_calls.jadpo
use metering/entities { ApiCall }
use metering/types { CallDay, MonthOfCalls }

/// Calls made since `from_day`, by month, the latest month first.
query monthly_calls(from_day: CallDay) -> List<MonthOfCalls> {
    from: ApiCall
    where: ApiCall.made_on >= from_day
    group: {
        call_year: temporal.year(ApiCall.made_on)
        call_month: temporal.month(ApiCall.made_on)
    }
    aggregate: {
        calls_made: count()
        tokens_billed: sum(ApiCall.tokens_spent)
        amount_billed: sum(ApiCall.charge)
        largest_call: max(ApiCall.tokens_spent)
        mean_tokens: average(ApiCall.tokens_spent, scale: 2, rounding: Rounding.half_even)
    }
    order: [call_year desc, call_month desc]
    limit: 24
}
```

```jadpo check project=aggregates file=domains/metering/queries/reply_tallies.jadpo
use metering/entities { ApiCall }
use metering/types { CallDay, ReplyTally }

/// How many calls since `from_day` were answered each way, the most first.
query reply_tallies(from_day: CallDay) -> List<ReplyTally> {
    from: ApiCall
    where: ApiCall.made_on >= from_day
    group: {
        reply: ApiCall.reply
    }
    aggregate: {
        calls_made: count()
    }
    order: [calls_made desc]
    limit: 3
}
```

```jadpo check project=aggregates file=domains/metering/queries/totals_since.jadpo
use metering/entities { ApiCall }
use metering/types { CallDay, CallSummary }

/// Every call since `from_day`, all together.
query totals_since(from_day: CallDay) -> CallSummary {
    from: ApiCall
    where: ApiCall.made_on >= from_day
    aggregate: {
        calls_made: count()
        amount_billed: sum(ApiCall.charge)
        slowest_wait: max(ApiCall.wait)
        mean_tokens: average(ApiCall.tokens_spent, scale: 2, rounding: Rounding.half_even)
    }
}
```

```jadpo check project=aggregates file=domains/metering/actions/record_call.jadpo hidden
use metering/entities { ApiCall }
use metering/types { CallCharge, CallDay, CallWait, ReplyKind, SpentTokens }

/// Records a call the API replied.
action record_call(
    made_on: CallDay,
    tokens_spent: SpentTokens,
    charge: CallCharge,
    reply: ReplyKind,
    wait: CallWait?
) -> ApiCall {
    return ApiCall.create {
        set: {
            made_on: made_on
            tokens_spent: tokens_spent
            charge: charge
            reply: reply
            wait: wait
        }
    }
}
```

```jadpo check project=aggregates file=domains/metering/actions/record_call.test.jadpo
use metering/entities { ApiCall }
use metering/types { CallCharge, CallSummary, MonthOfCalls, ReplyKind, ReplyTally }
use metering/queries { monthly_calls, reply_tallies, totals_since }

test "calls add up by month, and each reply is counted" {
    var one_penny: CallCharge = CallCharge(money.from_minor(1, Currency.gbp))
    var calls_recorded: List<ApiCall> = [
        record_call("2026-08-31", 120, one_penny, ReplyKind.replied, 2s),
        record_call("2026-09-01", 100, one_penny, ReplyKind.replied, none),
        record_call("2026-09-30", 41, one_penny, ReplyKind.throttled, 5s),
        record_call("2026-09-30", 0, one_penny, ReplyKind.replied, none)
    ]
    assert list.count(calls_recorded) == 4
    var by_month: List<MonthOfCalls> = monthly_calls("2026-08-01")
    assert list.map(by_month, month -> month.call_month) == [9, 8]
    assert list.map(by_month, month -> month.calls_made) == [3, 1]
    assert list.map(by_month, month -> month.tokens_billed) == [141, 120]
    assert list.map(by_month, month -> money.minor(month.amount_billed)) == [3, 1]
    assert list.map(by_month, month -> month.largest_call) == [100, 120]
    assert list.map(by_month, month -> month.mean_tokens) == [47.00, 120.00]
    var tallies: List<ReplyTally> = reply_tallies("2026-08-01")
    assert list.map(tallies, tally -> tally.reply == ReplyKind.replied) == [true, false]
    assert list.map(tallies, tally -> tally.calls_made) == [3, 1]
    var since_september: CallSummary = totals_since("2026-09-01")
    assert since_september.calls_made == 3
    assert since_september.slowest_wait == 5s
    assert since_september.mean_tokens == 47.00
}

test "a summary of no calls counts nothing" {
    var nothing: CallSummary = totals_since("2026-01-01")
    assert nothing.calls_made == 0
    assert nothing.amount_billed == money.from_minor(0, Currency.gbp)
    assert nothing.slowest_wait == none
    assert nothing.mean_tokens == none
}
```

`monthly_calls` gives one row for each month that has calls, and
`reply_tallies` one for each reply that some call got, `refused` among
them only once a call is refused. `jadpo sql monthly_calls` prints the one
statement it sends, on two levels: a grouped subquery works out the groups
and totals (its `SELECT` of each group and aggregate, the conditions and
`GROUP BY` the groups), and the outer statement selects, orders and limits
them, so an average names its sum and its count once. A `Money` sum's
currency is the one its type fixes, sent as a parameter.

- **The aggregates.** `count()` is how many records there are, an `Int`.
  `sum(F)` adds up a field built on `Int`, `Decimal` or `Duration`, a
  quantity, or `Money` whose type fixes `currency:` (two currencies can't
  be added, and a sum of nothing needs a currency for its zero); a
  percentage isn't summed. It gives what `+` gives, the field's representation (`Int`,
  `Decimal`, `Duration`, the quantity or `Money`), so Money and quantity
  sums keep their kind. `min(F)` and `max(F)` give the smallest and
  largest value of a field a query can sort by, other than an identity,
  since PostgreSQL has no `min` of a `uuid` (numbers, times, dates,
  durations, quantities, percentages and text, by code point), as the
  field's own type. `average(F, scale:, rounding:)`
  of a field built on `Int`, or on `Decimal` with at most `scale:` places,
  is a `Decimal` with exactly `scale:` places: the exact mean rounded once
  as `rounding:` says. Both are written out, a whole number from 0 to 38
  and a `Rounding`, as `decimal.round`'s are, since an average rarely
  comes out exact and the rounding is part of what the report says. An
  average of durations, quantities or money isn't offered until an
  application needs one.
- **The same as code.** Each aggregate gives exactly what code working
  from the records gives: `count()` is `list.count`; `sum` is `list.sum`,
  `+` for durations and quantities, or `money.sum` with the type's
  currency; `min` and `max` are the value `list.min_by` and `list.max_by`
  find; `average` is `decimal.divide` of the sum, at `scale:` places, by
  the count, with `rounding:`, through `decimal.divide`'s SQL form, which
  works the quotient out exactly as the runtime does, where PostgreSQL's
  own `avg` and `/` round to places they choose. A whole number, a
  duration and money's minor units are summed exactly and cast back to
  `bigint`, so a sum past 64 bits stops the read, and a decimal past 38
  digits fails as it is read: both are defects in code.
- **`none` is left out.** `sum`, `min`, `max` and `average` skip a field's
  `none` values, as a list of the values that are there would be summed;
  `count()` counts records, whatever their fields hold, and counting a
  field's values is a condition (`where: ApiCall.wait != none`). A group whose
  values are all `none` sums to zero, and its `min`, `max` and `average`
  are `none`.
- **The groups.** A group is a field, or a time bucket: `temporal.year`,
  `temporal.month` or `temporal.day` of a field built on `CalendarDate`,
  each an `Int` through its SQL form (area 19). The field itself is the
  day; a year and a month together are a month. A group field is built on
  `Bool`, `Int`, `Text`, `Uuid` (a reference among them), `CalendarDate`
  or a plain enum, whose stored values are equal exactly when Jadpo's are;
  a field that may be `none` puts its records without a value in a group
  of their own, `none`. Decimals, quantities and percentages aren't
  groups, since `numeric` equality puts `1.0` and `1.00` in one group and
  only one of them could be shown, and nor are instants, durations, money
  and structured values. An instant has no bucket: its day needs a zone,
  and PostgreSQL's time zone database can differ from the one built into
  every Jadpo build (area 10), so no SQL form of `temporal.in_zone` gives
  exactly what code gives; until that is decided, a report keeps the day
  as a `CalendarDate` field.
- **The result type.** Each group and aggregate fills the result's field of
  its name, which takes its type or one it widens to, as a stored value
  fills a field (`calls_made: Int`, `largest_call: ApiCall.tokens_spent`).
  A field that can be `none` needs a `T?`: a group of a field that may be
  `none`, and `min`, `max` and `average` of one, or with no `group:`.
- **With `group:`,** the query returns `List<T>` with `order:` and
  `limit:`, one row per group that has records, so no group is empty: a
  month without calls gives no row, as SQL's `GROUP BY` gives none, and
  filling gaps is the caller's. `order:` names groups and aggregates
  by their names, never the entity's fields, and the compiler adds each
  group it doesn't name as the tie-break, ascending, since the groups
  tell the rows apart. A group sorts in Jadpo's own order (area 2):
  numbers by value, text by code point (`COLLATE "C"`), days in time,
  `false` before `true` and an enum's variants in their declared order
  (`array_position` of the codes it is stored as); an aggregate sorts
  when its type is ordered, which a `Money` sum isn't. A total that may
  be `none` sorts where PostgreSQL puts `NULL`, as a list query's
  nullable order field does.
- **Without `group:`,** the query returns `T`, one row over every record it
  finds, and takes no `order:`, `limit:` or `missing:`: there is always the
  row. When nothing matches, `count()` is 0, a sum is zero (in the type's
  currency for `Money`, at its scale for a `Decimal`), and `min`, `max` and
  `average` are `none`.
- **What it doesn't take.** An aggregate query returns `List<T>` or `T`,
  never `T?` or `Page<T>`, and reads one entity: no `include:`, no
  `page:`, no `missing:` and no `nearest` order, since a page of groups
  would need a cursor over groups and nothing has asked for one. Its conditions are a
  query's usual `where:` and `where_present:` lines, and policy and the
  lifecycle's `visible:` limit the records it counts as they limit any
  query's. Every field a group or an aggregate reads is one every caller
  may read, as an output's fields are (area 9,
  `policy.output_field_unproved`).

Aggregates are the decided exception to "a query computes nothing"
(area 19, questions 21 and 28): a time bucket and an average are written
with their standard operations' SQL forms, and the rest are PostgreSQL's
`count`, `sum`, `min` and `max`, so an aggregate query is always one
statement, however many records it counts.

## A query giving a map layer

A query can return its records as a map layer, a
`FeatureCollection<G, P>` ([area 20](20-geospatial.md), "Interop"), so a
`GET` route sends the layer a mapping library fetches, as plain RFC 7946
GeoJSON (area 7, "A map layer as output"). The result type names the
properties type, as `List<Nearest<T>>` names its records. `geometry:`
names the stored field each feature's geometry is, a `G` every record
has, since an entity may hold several and the result type can't say
which; `P`, an `Object` type as area 20 requires of a feature's
properties, is each feature's properties, its fields filled from the
record's fields of the same names, as any result type's are. The query picks
fields and computes nothing, so it is a bounded list like any other:
`order:` and `limit:`, with area 20's `geo.` conditions and its order by
`geo.distance` if it needs them. A bakery map's layer, and the bakeries
within reach of a door:

```jadpo check project=layers file=app.jadpo hidden
application {}
```

```jadpo check project=layers file=domains/baking/types/bakery_name.jadpo hidden
/// A bakery's name on its sign.
type BakeryName = Text {
    max_length: 80
}
```

```jadpo check project=layers file=domains/baking/types/search_reach.jadpo hidden
/// How far a customer looks for a bakery.
type SearchReach = Length {
    min: 0metre
    max: 50km
}
```

```jadpo check project=layers file=domains/baking/entities/bakery.jadpo hidden
use baking/types { BakeryName }

/// A shop that bakes and delivers.
entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point

    policy: {
        read: [Access.public]
    }
}
```

```jadpo check project=layers file=domains/baking/actions/open_bakery.jadpo hidden
use baking/entities { Bakery }
use baking/types { BakeryName }

/// Opens a bakery at a spot.
action open_bakery(name: BakeryName, spot: Point) -> Bakery {
    return Bakery.create {
        set: {
            name: name
            spot: spot
        }
    }
}
```

```jadpo check project=layers file=domains/baking/types/bakery_pin.jadpo
use baking/entities { Bakery }
use baking/types { BakeryName }

/// What a bakery's pin on the map says: its identity goes in its
/// properties.
type BakeryPin = Object {
    id: Bakery.id
    name: BakeryName
}
```

```jadpo check project=layers file=domains/baking/queries/bakery_map.jadpo
use baking/entities { Bakery }
use baking/types { BakeryPin }

/// Every bakery, as the map's layer.
query bakery_map() -> FeatureCollection<Point, BakeryPin> {
    from: Bakery
    geometry: Bakery.spot
    order: [Bakery.name asc]
    limit: 500
}
```

```jadpo check project=layers file=domains/baking/queries/bakeries_near_map.jadpo
use baking/entities { Bakery }
use baking/types { BakeryPin, SearchReach }

/// The bakeries within reach of a door, nearest first, as a layer.
query bakeries_near_map(
    doorstep: Point,
    max_trip: SearchReach
) -> FeatureCollection<Point, BakeryPin> {
    from: Bakery
    geometry: Bakery.spot
    where: geo.within(Bakery.spot, doorstep, distance: max_trip)
    order: [geo.distance(Bakery.spot, doorstep) asc]
    limit: 20
}
```

```jadpo check project=layers file=domains/baking/routes/bakeries.jadpo
use baking/types { BakeryPin }
use baking/queries { bakery_map }

/// Bakeries, for the map.
routes Bakeries {
    prefix: /bakeries

    /// Every bakery, as GeoJSON.
    GET / {
        authentication: RouteAuthentication.none
        output: FeatureCollection<Point, BakeryPin>
        run: bakery_map()
    }
}
```

```jadpo check project=layers file=domains/baking/routes/bakeries.test.jadpo
use baking/entities { Bakery }
use baking/types { BakeryPin }
use baking/actions { open_bakery }
use baking/queries { bakeries_near_map, bakery_map }

test "the map fetches the bakeries as GeoJSON" {
    var soho: Bakery = open_bakery(
        "Soho",
        Point {
            latitude: 51.5136
            longitude: -0.1365
            altitude: none
        }
    )
    var bermondsey: Bakery = open_bakery(
        "Bermondsey",
        Point {
            latitude: 51.4979
            longitude: -0.0637
            altitude: none
        }
    )
    var response: Response = request {
        method: HttpMethod.get
        path: "/bakeries"
    }
    assert response.status == 200
    assert response.output == bakery_map()
    assert response.output == FeatureCollection {
        features: [
            Feature {
                geometry: bermondsey.spot
                properties: BakeryPin {
                    id: bermondsey.id
                    name: bermondsey.name
                }
            },
            Feature {
                geometry: soho.spot
                properties: BakeryPin {
                    id: soho.id
                    name: soho.name
                }
            }
        ]
    }
    var near_soho: FeatureCollection<Point, BakeryPin> = bakeries_near_map(soho.spot, 2km)
    assert list.map(near_soho.features, pin -> pin.properties.name) == ["Soho"]
}
```

```jadpo check project=layers file=api/main.jadpo_record hidden migrate=initial
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

GET /bakeries
    authentication none
    output FeatureCollection<Point, BakeryPin>
    status 200

GET /openapi.json
    generated openapi
    authentication none
    status 200

object BakeryPin
    id: Uuid
    name: Text { max_length: 80 }
```

`GET /bakeries` answers `{"type":"FeatureCollection","features":[...]}`,
each feature `{"type":"Feature","geometry":{"type":"Point","coordinates":[-0.0637,51.4979]},"properties":{"id":"...","name":"Bermondsey"}}`.
`jadpo sql bakery_map` prints its one statement, a `SELECT` of the
properties' columns and the geometry's.

- **What it takes.** `geometry:` names a field of the entity read, of
  exactly the layer's own geometry type and never `none`: a feature always
  has one, so a `Point?` field, a `Polygon` field in a layer of `Point`s,
  or one in a layer of `Geometry` (which would be a value built from it),
  is `query.geometry_field`, and a field the entity hasn't is
  `query.unknown_field`, as in a condition. A layer is one bounded list:
  `order:` and `limit:` are required, and `page:` (a `FeatureCollection`
  has no cursor), `missing:`, `include:`, `group:` and `aggregate:` are
  refused, since a layer's properties are each record's own fields, one
  feature to a record.
- **Its properties.** Each field of `P` is the record's field of that
  name, `query.result_field` otherwise; a feature's identity goes in its
  properties, so `P` picks `id` where the map needs it.
- **Field policy.** A layer is sent as it is, so it is held as an
  `Output` is (area 9, "Field restrictions"): every caller who may read
  the records may read each property and the geometry, and every field
  its conditions and order test, or the query is
  `policy.output_field_unproved`. This is proved where the query is
  declared, so an action returning the layer carries nothing more.
- **Only what a `GET` can ask for.** A `GET`'s input is its query string,
  which can't hold a `BoundingBox` (area 7), so a layer of only what is in
  view would still need a body; example 21 sends its whole drop point
  layer with `GET /drop-points`.

A property the record has no field for:

```jadpo check project=e-layer-properties expect=query.result_field
type BakeryName = Text {
    max_length: 80
}

entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point
}

type BakeryPin = Object {
    bakery_id: Bakery.id
    name: BakeryName
}

query bakery_map() -> FeatureCollection<Point, BakeryPin> {
    from: Bakery
    geometry: Bakery.spot
    order: [Bakery.name asc]
    limit: 500
}
```

```text output
error: `BakeryPin.bakery_id` isn't a field of `Bakery`
 --> main.jadpo:16:23
   |
16 | query bakery_map() -> FeatureCollection<Point, BakeryPin> {
   |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `bakery_id` can't be filled
  reason: A map layer fills its features' properties from `Bakery`'s fields of the same name (`id`, `name` and `spot`); nothing gives `bakery_id`.
  next: Remove `bakery_id` from `BakeryPin`, or name it after a field of `Bakery` (agent)
  rule: query.result_field
  help: docs/language/06-reads-and-writes.html#query-result-field

check failed: 1 error, 1 file
```

A property only the server may read:

```jadpo check project=e-layer-restricted expect=policy.output_field_unproved
type BakeryName = Text {
    max_length: 80
}

type PhoneText = Text {
    max_length: 20
}

entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point
    owner_phone: PhoneText {
        policy: FieldAccess.server_only
    }

    policy: {
        read: [Access.public]
    }
}

type BakeryPin = Object {
    name: BakeryName
    owner_phone: PhoneText
}

query bakery_map() -> FeatureCollection<Point, BakeryPin> {
    from: Bakery
    geometry: Bakery.spot
    order: [Bakery.name asc]
    limit: 500
}
```

```text output
error: `bakery_map` sends `Bakery.owner_phone`, which not every caller may read
 --> main.jadpo:27:23
   |
27 | query bakery_map() -> FeatureCollection<Point, BakeryPin> {
   |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ carries `owner_phone`
  reason: Anyone `Bakery`'s `read:` lets in may call `bakery_map`, anyone at all among them, and no client may read it, as it is `FieldAccess.server_only` `owner_phone`. An output has one shape for everyone, so a field some callers may not read can't be in it.
  next: Leave `owner_phone` out of `BakeryPin`, the features' properties (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 1 file
```

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

Built since chunk 3.3, the lifecycle's transitions since chunk 3.4. An
`update`, `delete` or transition takes the record loaded earlier in the action, or its
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

A transition (area 5, "Lifecycle") is an update of the fields its `set:`
gives, run on the record loaded earlier or by `id`, which gives back the
record as stored; through `delete`, it gives nothing. Its block takes only
`conflict:`, since what it changes is fixed in the lifecycle. Like every
update and delete it reaches only a record `visible:` lets through, so a
hidden one is the entity's not-found failure. A `when:` that asks more than
`visible:` can refuse a record that is there, and that refusal is mapped in
`conflict:` like a rule, keyed by the transition (chosen in chunk 3.4):

```jadpo check project=transition file=planning.jadpo hidden
type UserName = Text {
    max_length: 80
}

type UserStage = Enum {
    active
    disabled
}

entity User {
    id: Uuid
    name: UserName
    stage: UserStage
    disabled_at: Instant?

    lifecycle: {
        initial: {
            stage: UserStage.active
        }
        transitions: {
            disable: {
                when: stage == UserStage.active
                set: {
                    stage: UserStage.disabled
                    disabled_at: clock.now
                }
            }
        }
    }
}

type UserDisabled = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=transition file=disable_user.jadpo
// domains/planning/actions/disable_user.jadpo
action disable_user(user_id: User.id) fails UserNotFound, UserDisabled -> User {
    return attempt User.disable(user_id) {
        conflict: {
            User.disable: UserDisabled  // its `when:` doesn't hold
        }
    }
}
```

The statement is `UPDATE "user" SET "stage" = $1, "disabled_at" = $2
WHERE "id" = $3 AND "stage" = $4`, the stages bound as their column holds
them (`'disabled'`, `'active'`; area 19, "Every boundary through the contract"); when
it changes no row, a second
statement asks whether the user is there and visible, which tells
`UserDisabled` from `UserNotFound`. `jadpo sql disable_user` shows both.

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
reads; it still equals one given `none`. A field whose type isn't nullable
(`title` above) can't be cleared, so `none` given to it is the same as
leaving it out and the patch keeps the stored value (chosen in chunk 3.10:
a check error can't tell, since forwarding an optional value such as a
page's `next` cursor into an optional field is ordinary code).

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

A write can be refused for two reasons, and a lifecycle transition for a
third: its `when:` doesn't hold (above). The record is not there: that
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
An action whose failure its caller handles with `match` runs inside a
savepoint (chosen in chunk 3.10): when it fails, everything it wrote is
undone, a refused write among them, and the caller carries on in a
transaction the failure didn't end, since an action is all-or-nothing
whoever calls it. One that passes its failure on with `attempt` needs none:
the failure ends its caller too, up to a `match` that undoes both or the
outermost action, which rolls back.

## All-or-nothing actions and concurrent changes

Every action is all-or-nothing (area 3): its writes, and those of the public
actions it calls, commit together or not at all. A write to a record loaded
earlier in the action checks that nobody changed it in between. If someone
did, the runtime throws the attempt away and runs the whole action again,
with fresh reads and a fresh `clock.now`. That is safe because an action's
outside call comes before its writes and carries an idempotency key, so a
rerun repeats it harmlessly (area 11). A changing call with no key is the
exception: once it has gone out, the action isn't run again but stops as
`outcome_unknown`, for an operator, since sending it again could act twice
(area 16, "Parked, never sent again blindly"; chunk 7.9a). An action with
several changing calls is the other: once its record exists, a rerun
replays the recorded calls and keeps the record's `clock.now` ("Several
outside calls", below). After three
attempts within a second, the existing retry budget, the request fails
`unavailable` and the client may try again. Authors write nothing for this.

```text
request A: loads todo 7                 request B: loads todo 7
request A: updates todo 7, commits
                                        request B: updates todo 7: changed since loaded
                                        request B: runs again, loads the new todo 7, updates
```

On PostgreSQL this is the repeatable-read isolation level, whose "could not
serialize" error the runtime already retries; an action that loads no
record runs at read committed instead ("Counters and other one-step
updates", below). Catching a client that edits a stale copy across two
requests is HTTP's `If-Match` (area 7).

Built in chunk 3.3. An action that loads a record runs at repeatable read
(every action did until chunk 3.7), so its snapshot is taken at its first
statement and a record it loaded is the record as of then; an `update` or `delete` of a row another transaction
changed since is PostgreSQL's serialisation failure, which the driver
reports as a concurrent change. The runtime then rolls the transaction
back, throws away the attempt's events, and runs the whole action again
from its first line: its reads see the other transaction's change, and its
new records get new identities. The budget is three attempts; the second
bound, on time, isn't built: the boundary counts attempts only. A record
handed to an action from outside its transaction (a test passing one an
earlier action returned) is checked only as far as its row is still there.


### Outside calls hold no transaction

No transaction is open while an action waits on a provider (Richard,
2026-10-15). What an action reads before its outside call runs in a short
read-only snapshot that closes before the call goes out; the transaction
opens after the call returns, and the action's writes and any later reads
run in it. Reads between two outside calls get a snapshot of their own. A
write to a record loaded before the call checks that its row is unchanged
since it was loaded, by comparing PostgreSQL's row version (`xmin`, which
Entity Framework's PostgreSQL provider uses the same way), and a change is
a concurrent change like any other: the whole action runs again, and the
call repeats harmlessly under its idempotency key (area 11).

So a slow provider holds no database connection, and the time it takes
never widens the window in which another request's change forces a rerun.
The guarantee is the one repeatable read gives: every record the action
writes is checked against the copy it loaded.

Built in chunk 7.4, at the capability seam every outside call goes
through:

- **The snapshot is what the action has only read.** An action's
  transaction still begins at its first store operation, at its level. If
  it has run nothing but reads when a call goes out, it is the snapshot:
  it reads the row versions of the records it loaded (one statement per
  table, `xmin` by `id`) and rolls back, before the call is sent. Once it
  has written, a call is refused as before (`call_in_transaction`), so no
  transaction is ever open while a call is out. An action with no call
  keeps its one transaction, unchanged.
- **The transaction after the call.** The next store operation begins a
  new transaction at the same level, in the same store. Reads after a call
  run in it; if another call comes before any write, they were a snapshot
  of their own and close the same way.
- **The row version check.** An update, delete or transition of a row
  loaded in a snapshot a call closed compares that row's version in the
  write's own `WHERE` (chunk 9.1b4). A successful write locks it and checks
  it in one statement, once per transaction. When no row is changed, a
  failure-only `FOR NO KEY UPDATE` probe distinguishes a different version
  or no row, a concurrent change, before the ordinary policy, field,
  lifecycle-condition or missing-row refusal. A failed nested action's
  savepoint restores only version checks whose locks survive its rollback. A write by `id` is held to it too when the action loaded that
  row before the call, as repeatable read holds any write to a row changed
  since the snapshot. A row is held to the version the action first
  loaded, so one read again after the call, or in a second snapshot, is
  still checked against it.
- **A change reruns the action.** A concurrent change throws the attempt
  away and runs the action again, within the existing budget of three
  attempts. Its keyed calls take the same steps again, so each repeats
  under the key it was first sent with (area 11); a change found at the
  commit of a route's or a delivery's held transaction does the same. An
  unkeyed change that went out stops it as `outcome_unknown` instead
  (chunk 7.9a). A reference to a record
  loaded before a call that was deleted while the call was out is a
  concurrent change too, so the rerun's fresh read finds the record gone.
- **A batch's items** (chunk 7.4b). An item of a run whose `each:` action
  makes outside calls is a batch of its own (area 12, "How a run is
  kept"), so its calls go out with no transaction open, as any action's
  do, and its writes and the run's position commit after them under these
  rules. A write placed before a later call is refused at compile time
  ("Several outside calls", below).

### Several outside calls

Decided 2026-10-16 (Richard). An action may make up to 8 changing calls in
order (area 12, "Several changing calls in one action"). For reads and
writes this changes three things and keeps the rest:

- **Writes come after the last call.** The action's transaction opens only
  once its last outside call has returned; a write placed before a later
  call is a compile error naming the call it must follow. No transaction
  is open during any call, as above, and reads between two calls each get
  a snapshot of their own.
- **The runtime's record is not the action's transaction.** Each step is
  recorded before it is sent and its answer after, each in a short
  transaction of the runtime's own in the store the action writes; the
  record is deleted in the action's commit, so a committed action leaves
  none behind.
- **A rerun replays, with the record's time.** When a concurrent change
  reruns the action, recorded steps are replayed without being sent and
  `clock.now` is the time the record holds, not a fresh one; the reads are
  fresh. If a fresh read changes a request the action would send, the
  rerun stops as `diverged` before sending anything, held for an operator
  (area 12), rather than sending a different request under a used key.
  Database reads are never recorded, so this check is what keeps a rerun
  honest.

Because the writes still commit together after every call has answered,
a write refused for good (a unique rule, a restricted reference) after a
step took effect is warned at compile time, naming the fixes: check with a
read before the first call, record what happened and finish in a
subscriber, or undo as another step (area 12).

Built in chunk 7.4b, for the order: a write placed before a later outside
call is [`data.write_before_call`](#data-write-before-call), reported at
the write and naming the last call it comes before, so checked code never
reaches the runtime's `call_in_transaction`, which stays as the guard.
Chosen there, each the most conservative reading:

- **What writes.** An entity's write; a session or key operation that
  changes one; starting a run (`batches.each` and the file helpers keep
  its row) or cancelling one; and calling an action or fixture that does
  any of these, itself or through an action it calls, since it joins the
  caller's transaction. `emit` writes nothing until the action commits,
  and `batches.status` only reads.
- **What calls.** A provider's operation, a store's `put` or `read`, and
  calling an action or fixture that makes one, itself or through an action
  it calls. A run's `each:` and `finished:` are the run's, never the
  action that starts it.
- **Each way through on its own.** A write on one way through an `if` or
  a `match` and a call on another never meet, and a `return`, `reject` or
  `propagate` ends its way; a loop's body counts as running more than
  once, so a write in it comes before a call after the loop or in its next
  turn.
- **Not declinable.** The order is never a judgement call (area 1,
  "Declining a judgement call"): run as written, the call would be
  refused.

A bakery keeps the price sheet a supplier sends again, marking it
checked: the call first, then one write after it.

```jadpo check project=write-after-call file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=write-after-call file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=write-after-call file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=write-after-call file=domains/bakery/entities/price_sheet.jadpo hidden
/// A supplier's price sheet, kept as it was sent, and whether it was
/// checked.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }
    checked: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
    }
}
```

```jadpo check project=write-after-call file=domains/bakery/actions/recheck_prices.jadpo migrate=initial
use app { sheets }
use bakery/entities { PriceSheet }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Keeps the sheet a supplier sent again, then marks it checked: the
/// call first, the write after it.
action recheck_prices(
    price_sheet: PriceSheet,
    sheet_text: SheetText
) fails SheetNotKept -> PriceSheet {
    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return PriceSheet.update(price_sheet) {
        set: {
            sheet: sheet
            checked: true
        }
    }
}
```

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
parameters, constants, `clock.now` and the record's stored fields, so no
increment is ever lost. A constant here is a `constant` declaration
(area 3, decided 2026-10-16, built in chunk 5.3e).

Which isolation level an action runs at follows from what it does (Richard,
2026-10-15). An action that loads no record, neither itself nor through any
action it calls, runs at PostgreSQL's read committed level: of two requests
counting the same row at once, the second waits for the first, adds to its
count and commits, and neither reruns. An action that loads a record runs at
repeatable read (see "All-or-nothing actions and concurrent changes"), so a
counter racing another request on a row it loaded reruns the action, as any
concurrent change does. The compiler decides this from the graph; authors
write nothing.

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
in Jadpo (`+` joining text is written `||`, since chunk 3.10), under constructions the declared ranges prove (`ViewCount(...)`
here, since a count of at least 0 plus 1 is at least 0); anything else needs
the record loaded first. A decimal added to a stored field may not have more
decimal places than the field's `scale`, since PostgreSQL would round the
sum its own way (half away from zero) rather than Jadpo's: a literal by its
own places, any other value by its type's `scale`, and one whose type sets
none is refused (`data.stored_decimal_scale`, built in chunk 3.4).

Built in chunk 3.7. An action loads a record when it reads one (`get`, or
a query with what it includes), is handed one (a parameter whose type
holds a record, such as `video: Video`, or a list or object holding one),
writes one back (an `update`, `delete` or transition of a record rather
than its `id`), or runs a transition whose `when:` can refuse a visible
record, since telling that refusal from a missing record takes a second
statement, which must see the first one's snapshot. A create, and an
update, delete or other transition by `id`, counters among them, loads
nothing: the store checks a reference a create sets in the same statement.
Calling a function never does. `jadpo check` decides each action and
fixture from these and the actions and fixtures it calls, writes it into
the manifest as `isolation` (`read_committed` or `repeatable_read`), and
the runtime begins the transaction at it (`BEGIN ISOLATION LEVEL READ
COMMITTED` or `REPEATABLE READ`). An action another action calls joins that
action's transaction at its level, which already counts what the callee
does; each action a test calls begins its own. So `view_video` above runs
at read committed: two requests counting one video at once both commit at
their first attempt, the second waiting for the first's lock and adding to
its count, while a counter in an action that read the video first still
reruns the second (both checked live against PostgreSQL in
`crates/jadpo-cli/tests/writes.rs`). What runs outside an action, such as
a read a test makes itself, migrations and purge, runs at repeatable
read.

## Reading in a loop

A query or `Todo.get` inside a `for` loop is a compile error, because it
would read once per item. Read the whole set with one query using `in`.
Writes inside a loop are allowed: the list is bounded and every write joins
the action's transaction. An outside call inside a loop is not (area 3,
decided 2026-10-16): a loop of calls is `batches.each` (area 12). A batch that should keep partial progress is a
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
operation names. Reading another store outside the transaction is not
built yet, so since chunk 3.10 an action that reads a second store, itself
or through an action it calls, is `toolchain.not_built_yet` at the read or
call that brings it in, rather than a fault as it runs:

```jadpo check project=e-second-store-read file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        catalogue: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-second-store-read file=domains/ordering/types/product_name.jadpo hidden
type ProductName = Text {
    max_length: 80
}
```

```jadpo check project=e-second-store-read file=domains/ordering/entities/product.jadpo hidden
use ordering/types { ProductName }

entity Product {
    id: Uuid
    name: ProductName

    store: catalogue
}
```

```jadpo check project=e-second-store-read file=domains/ordering/entities/order.jadpo hidden migrate=initial
use ordering/types { ProductName }

entity Order {
    id: Uuid
    product_name: ProductName
}
```

```jadpo check project=e-second-store-read file=domains/ordering/actions/order_product.jadpo expect=toolchain.not_built_yet
use ordering/entities { Order, Product, ProductNotFound }

action order_product(product_id: Product.id) fails ProductNotFound -> Order {
    var product: Product = attempt Product.get(product_id)
    return Order.create {
        set: {
            product_name: product.name
        }
    }
}
```

```text output
error: Reading a second store in `order_product` isn't built yet
 --> domains/ordering/actions/order_product.jadpo:4:36
  |
4 |     var product: Product = attempt Product.get(product_id)
  |                                    ^^^^^^^^^^^^^^^^^^^^^^^ store `catalogue`
  reason: An action may read another store outside its transaction (area 6), but this compiler runs all of an action's store operations in the one transaction its first begins; `order_product` uses `Order` (store `main`), and here it reads `Product` (store `catalogue`).
  next: Read `Product` before `order_product` runs, and pass in what it needs, for now (agent)
  rule: toolchain.not_built_yet
  help: docs/language/06-reads-and-writes.html#one-store-per-action

check failed: 1 error, 5 files
```

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

## Entities in Redis

Decided 2026-10-16 (Richard). An entity in a Redis store (area 5) is read
and written through the same queries and operations as any entity, but
Redis keeps no indexes of its own, so only what Jadpo can keep a key or a
set for is allowed:

| Read or write | Allowed on a Redis entity |
|---|---|
| `get`, `create`, `update` and `delete` by `id` | Yes |
| An update by `id` adding to a stored whole number (`Counter.views + 1`) | Yes, as one Redis `increment` |
| A lifecycle and its transitions | No: a compile error; `expires_after:` takes its place |
| A query whose `where:` is equality on a `unique:` field | Yes: one record, from the key Jadpo keeps per unique field |
| A query whose `where:` is equality on one field in `indexes:` | Yes: a page, in creation order, from the set Jadpo keeps per value; counting the same |
| Policy grants that are equality on the record's own fields | Yes, checked after reading |
| Joins, `include`, ranges (`<`, `>`), `in`, `or`, ordering by any other field, `matches`, grants beyond simple equality | No: a compile error naming the reason and PostgreSQL as the alternative |

- **Expired records are hidden** from every read and update, as
  `visible:` hides records: a record past its `expires_after:` (counted
  from its last write) is not found, even in the moment before Redis
  removes it.
- **Concurrent change** is checked through a revision Jadpo keeps on each
  record: a write to a record loaded earlier fails as a concurrent change
  if the revision moved, and the action reruns as any action does.
- **One store per action** applies as everywhere: an action writing a Redis
  entity writes nothing in another store.

These are built with the Redis driver, after the first build.

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
  reason: A query's block holds `from:`, `geometry:`, `where:`, `where_present:`, `group:`, `aggregate:`, `order:`, `limit:`, `page:`, `include:` and `missing:`. Did you mean `order:`?
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
use catalogue/entities { Book }

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
use catalogue/entities { Book }

/// Every copy.
query shelf() -> List<Book> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `shelf` reads `Book`, an entity of domain `catalogue`
 --> domains/lending/queries/shelf.jadpo:5:11
  |
5 |     from: Book
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
  reason: A query's result is `Book` or an object type its fields fill (`type BookView = Output { pick: [...] }`), as `T` for exactly one record, `T?` for at most one, `List<T>` or `Page<T>`, or a map layer, `FeatureCollection<G, P>`.
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

### `query.result_rule`

```jadpo check project=e-result-rule expect=query.result_rule
type LeafCount = Int {
    min: 1
    max: 5000
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ThickBook = Output {
    pick: [Book.id, Book.leaves]
    rule thick: leaves > 1000
}

query thick_books() -> List<ThickBook> {
    from: Book
    order: [Book.id asc]
    limit: 20
}
```

```text output
error: `thick_books` returns `ThickBook`, whose rule `thick` no read can check
 --> main.jadpo:16:24
   |
16 | query thick_books() -> List<ThickBook> {
   |                        ^^^^^^^^^^^^^^^ `ThickBook` has rules
  reason: A rule across an object's fields is checked whenever a value is made, and a value that breaks it is a failure an `attempt` maps; a query makes `ThickBook` from stored records with no `attempt` around it, so its result type has no rules across its fields.
  next: Return a type without rules, and build `ThickBook` from it with `attempt` where its rules are checked (agent)
  rule: query.result_rule
  help: docs/language/06-reads-and-writes.html#query-result-rule

check failed: 1 error, 1 file
```

A query that needs only the thick books says so in `where:`, and a type
with the rule is built from its result where a failure can be mapped.

### `query.geometry_field`

```jadpo check project=e-geometry-field expect=query.geometry_field
type BakeryName = Text {
    max_length: 80
}

entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point
    delivery_area: Polygon
}

type BakeryPin = Object {
    id: Bakery.id
    name: BakeryName
}

query bakery_map() -> FeatureCollection<Point, BakeryPin> {
    from: Bakery
    geometry: Bakery.delivery_area
    order: [Bakery.name asc]
    limit: 500
}
```

```text output
error: `Bakery.delivery_area` isn't a `Point`, so it can't be each feature's geometry
 --> main.jadpo:19:15
   |
19 |     geometry: Bakery.delivery_area
   |               ^^^^^^^^^^^^^^^^^^^^ a `Polygon`
  reason: `FeatureCollection<Point, BakeryPin>`'s features each hold a `Point`, and a stored value goes into the layer as it is, so `geometry:` names a field of that type.
  next: Use `Bakery.spot` (agent)
  rule: query.geometry_field
  help: docs/language/06-reads-and-writes.html#query-geometry-field

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
  reason: `>` orders numbers, times, dates, durations and quantities, as it does in a body, and `Book.title` is `Title`.
  next: Compare `Book.title` with `==`, `!=` or `in` (agent)
  rule: query.condition_unsupported
  help: docs/language/06-reads-and-writes.html#query-condition-unsupported

check failed: 1 error, 1 file
```

A standard operation stands where a field does only when it has an exact
SQL form ("Operations in conditions"). `text.lower` has none,
since PostgreSQL's would follow the database's collation:

```jadpo check project=e-condition-no-form expect=query.condition_unsupported
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

query books_titled(title: Book.title) -> List<Book> {
    from: Book
    where: text.lower(Book.title) == title
    order: [Book.title asc]
    limit: 20
}
```

```text output
error: `text.lower(Book.title) == title` isn't a condition a query can use
 --> main.jadpo:12:12
   |
12 |     where: text.lower(Book.title) == title
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not an indexable condition
  reason: `text.lower` has no SQL form, an expression PostgreSQL works out exactly as the code does (area 19, "SQL forms"), so the database can't decide this condition.
  next: Compare `Book.title` itself, or keep what the condition needs in a field of its own (agent)
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
  reason: A query's values are its parameters and their fields, literals, constants, enum values, lists of those and `clock.now`; anything worked out from them is computed before the call.
  next: Work the value out in the action, and pass it as a parameter (agent)
  rule: query.value_unsupported
  help: docs/language/06-reads-and-writes.html#query-value-unsupported

check failed: 1 error, 1 file
```

A constant counts as a literal here (decided 2026-10-16, built in chunk
5.3e), so this is accepted. Arithmetic on a constant inside the query is
still worked out, and still this error: give the result a constant of its
own.

```jadpo check project=query-constant
type PageCount = Int {
    min: 1
    max: 5000
}

entity Manual {
    id: Uuid
    page_count: PageCount
}

constant longest_manual: PageCount = 900

query long_manuals() -> List<Manual> {
    from: Manual
    where: Manual.page_count > longest_manual
    order: [Manual.page_count asc]
    limit: 20
}
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
  reason: A query sorts by numbers, times, dates, durations, quantities, percentages, text and identities, which the database and Jadpo order the same way; `Book.available` is `Bool`.
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

A value's type must keep it at 1 or more too, since PostgreSQL refuses a
negative `LIMIT` and 0 reads nothing (chunk 3.10):

```jadpo check project=e-limit-range-type expect=query.limit_range
type Title = Text {
    max_length: 80
}

type ShelfSize = Int {
    max: 50
}

entity Book {
    id: Uuid
    title: Title
}

query shelf(size: ShelfSize) -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: size
}
```

```text output
error: `size` can be below 1, so the limit could read nothing
 --> main.jadpo:17:12
   |
17 |     limit: size
   |            ^^^^ `ShelfSize` has no `min:`
  reason: `limit:` is how many records come back at most, so it is at least 1, and a value's type must say so with `min: 1` or more, as the built-in `PageSize` (1 to 100) does.
  next: Give `ShelfSize` `min: 1`, or type the value as `PageSize` (agent)
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

entity Territory {
    id: Uuid
    name: Name
}

entity Publisher {
    id: Uuid
    territory_id: Territory.id {
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

type TerritoryView = Output {
    pick: [Territory.name]
}

type PublisherView = Output {
    territory: TerritoryView
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
                        territory: Publisher.territory_id
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

### Groups and aggregates

### `query.group_key`

```jadpo check project=e-group-key expect=query.group_key
type Title = Text {
    max_length: 80
}

entity Loan {
    id: Uuid
    title: Title
    lent_at: Instant
}

type LoansByYear = Output {
    lent_year: Int
    loans_made: Int
}

query loans_by_year() -> List<LoansByYear> {
    from: Loan
    group: {
        lent_year: temporal.year(Loan.lent_at)
    }
    aggregate: {
        loans_made: count()
    }
    order: [lent_year desc]
    limit: 20
}
```

```text output
error: `temporal.year` groups by a date, and `Loan.lent_at` is `Instant`
 --> main.jadpo:19:34
   |
19 |         lent_year: temporal.year(Loan.lent_at)
   |                                  ^^^^^^^^^^^^ not a date
  reason: A time bucket is worked out by the database, which gives exactly what code gives only for a date: an instant has a day only in a zone, and PostgreSQL's time zone database can differ from the one built into Jadpo.
  next: Keep the day as a `CalendarDate` field, and group by a time bucket of it (agent)
  rule: query.group_key
  help: docs/language/06-reads-and-writes.html#query-group-key

check failed: 1 error, 1 file
```

### `query.aggregate_unknown`

```jadpo check project=e-aggregate-unknown expect=query.aggregate_unknown
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfLength = Output {
    mean_leaves: Decimal?
}

query shelf_length() -> ShelfLength {
    from: Book
    aggregate: {
        mean_leaves: avg(Book.leaves, scale: 1, rounding: Rounding.half_up)
    }
}
```

```text output
error: `avg` isn't an aggregate
 --> main.jadpo:17:22
   |
17 |         mean_leaves: avg(Book.leaves, scale: 1, rounding: Rounding.half_up)
   |                      ^^^ not an aggregate
  reason: An aggregate is `count()`, `sum(F)`, `min(F)`, `max(F)` or `average(F, scale:, rounding:)`, each `F` a field of `Book`. Did you mean `average`?
  next: Use `average` (agent)
  rule: query.aggregate_unknown
  help: docs/language/06-reads-and-writes.html#query-aggregate-unknown

check failed: 1 error, 1 file
```

### `query.aggregate_argument`

```jadpo check project=e-aggregate-argument expect=query.aggregate_argument
type LeafCount = Int {
    min: 1
}

entity Book {
    id: Uuid
    leaves: LeafCount
}

type ShelfLength = Output {
    mean_leaves: Decimal?
}

query shelf_length() -> ShelfLength {
    from: Book
    aggregate: {
        mean_leaves: average(Book.leaves)
    }
}
```

```text output
error: `average` needs `scale:` and `rounding:`
 --> main.jadpo:17:22
   |
17 |         mean_leaves: average(Book.leaves)
   |                      ^^^^^^^^^^^^^^^^^^^^ wrong arguments
  reason: An average rarely comes out exact, so it says how many places it keeps and how the last one is rounded, both written out: `average(Book.<field>, scale: 2, rounding: Rounding.half_even)`.
  next: Add `scale:` and `rounding:` (agent)
  rule: query.aggregate_argument
  help: docs/language/06-reads-and-writes.html#query-aggregate-argument

check failed: 1 error, 1 file
```

### `query.aggregate_field`

```jadpo check project=e-aggregate-field expect=query.aggregate_field
type Fine = Money {}

entity Loan {
    id: Uuid
    fine: Fine
}

type FinesOwed = Output {
    owed: Money
}

query fines_owed() -> FinesOwed {
    from: Loan
    aggregate: {
        owed: sum(Loan.fine)
    }
}
```

```text output
error: `sum` adds up money in one currency, and `Loan.fine`'s type doesn't fix one
 --> main.jadpo:15:19
   |
15 |         owed: sum(Loan.fine)
   |                   ^^^^^^^^^ `Fine`
  reason: Amounts in two currencies don't add up (`currency_mismatch`), and a sum of no records needs a currency for its zero, so the field's type fixes its currency: `Fine` doesn't.
  next: Give `Fine` a `currency:` (agent)
  rule: query.aggregate_field
  help: docs/language/06-reads-and-writes.html#query-aggregate-field

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

### `data.lifecycle_owned_field`

Only a lifecycle's transitions change the fields it owns: every field a transition sets or `initial:` gives:

```jadpo check project=e-lifecycle-owned-field expect=data.lifecycle_owned_field
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                set: {
                    deleted_at: clock.now
                }
            }
        }
    }
}

action hide_todo(todo: Todo) -> Todo {
    return Todo.update(todo) {
        set: {
            deleted_at: clock.now
        }
    }
}
```

```text output
error: `Todo.update` sets `deleted_at`, which only `Todo`'s lifecycle changes
 --> main.jadpo:20:13
   |
20 |             deleted_at: clock.now
   |             ^^^^^^^^^^ owned by the lifecycle
  reason: `Todo`'s lifecycle owns `deleted_at`: only its transition `delete` changes it, so the record's stages always move as the lifecycle says.
  next: Remove `deleted_at`, and call `Todo.delete(todo)` to change it (agent)
  rule: data.lifecycle_owned_field
  help: docs/language/06-reads-and-writes.html#data-lifecycle-owned-field

check failed: 1 error, 1 file
```

### `data.lifecycle_hard_delete`

Code never removes a record that has a lifecycle: `delete` runs its `delete` transition, and an entity with none can't be deleted:

```jadpo check project=e-lifecycle-hard-delete expect=data.lifecycle_hard_delete
type ArticleTitle = Text {
    max_length: 120
}

entity Article {
    id: Uuid
    title: ArticleTitle
    published_at: Instant?

    lifecycle: {
        visible: published_at != none and published_at <= clock.now
    }
}

action remove_article(article: Article) -> Unit {
    Article.delete(article)
}
```

```text output
error: `Article.delete` would remove a record `Article`'s lifecycle keeps
 --> main.jadpo:16:5
   |
16 |     Article.delete(article)
   |     ^^^^^^^^^^^^^^ a lifecycle keeps it
  reason: Code never removes a record that has a lifecycle: `delete` runs its `delete` transition, and `Article`'s lifecycle has none. Only purge, the compiler's own, removes such records, once they have been hidden long enough.
  next: Add a `delete` transition to `Article`'s lifecycle, saying what deleting one changes (human)
  rule: data.lifecycle_hard_delete
  help: docs/language/06-reads-and-writes.html#data-lifecycle-hard-delete

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
mappings are below. Since chunk 7.4b a write placed before a later outside
call is `data.write_before_call`.

### `data.write_before_call`

No transaction is open while an outside call is out, so an action's
writes come after its last call ("Several outside calls"). The error is
at the write, naming the last call it comes before; one made through an
action it calls names that action and the call it makes:

```jadpo check project=e-write-before-call file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-write-before-call file=domains/bakery/entities/price_sheet.jadpo hidden migrate=initial
/// A supplier's price sheet, kept as it was sent, and whether it was
/// checked.
entity PriceSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
        types: [FileType.csv]
    }
    checked: Bool

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
    }
}
```

```jadpo check project=e-write-before-call file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-write-before-call file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-write-before-call file=domains/bakery/actions/recheck_prices.jadpo expect=data.write_before_call
use app { sheets }
use bakery/entities { PriceSheet }
use bakery/types { SheetText }
use bakery/failures { SheetNotKept }

/// Marks a supplier's price sheet checked, then keeps the sheet they sent
/// again: the write comes before the call.
action recheck_prices(
    price_sheet: PriceSheet,
    sheet_text: SheetText
) fails SheetNotKept -> PriceSheet {
    var checked: PriceSheet = PriceSheet.update(price_sheet) {
        set: {
            checked: true
        }
    }
    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return PriceSheet.update(checked) {
        set: {
            sheet: sheet
        }
    }
}
```

```text output
error: `PriceSheet.update` writes before the call to `sheets.put`
 --> domains/bakery/actions/recheck_prices.jadpo:12:31
   |
12 |     var checked: PriceSheet = PriceSheet.update(price_sheet) {
   |                               ^^^^^^^^^^^^^^^^^ writes here
17 |     var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {
   |                               ---------- the call it must follow
  reason: No transaction is open while an outside call is out, so an action writes only after its last call; run as written, `sheets.put` would be refused.
  next: Move `PriceSheet.update` after `sheets.put` (agent)
  rule: data.write_before_call
  help: docs/language/06-reads-and-writes.html#data-write-before-call

check failed: 1 error, 5 files
```

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

### `data.stored_decimal_scale`

The database rounds a sum its own way, so a decimal added to a stored field
keeps no more places than the field does:

```jadpo check project=e-stored-decimal-scale expect=data.stored_decimal_scale
type Balance = Decimal {
    scale: 2
}

entity Wallet {
    id: Uuid
    balance: Balance
}

action add_interest(wallet_id: Wallet.id) fails WalletNotFound -> Wallet {
    return attempt Wallet.update(wallet_id) {
        set: {
            balance: Balance(Wallet.balance + 0.005)
        }
    }
}
```

```text output
error: `Wallet.update` adds a decimal with more places than `Wallet.balance` keeps
 --> main.jadpo:13:47
   |
13 |             balance: Balance(Wallet.balance + 0.005)
   |                                               ^^^^^ 3 decimal places
  reason: The database works `Wallet.balance` out in one step, and keeps it at `scale: 2`; `0.005` has 3, and PostgreSQL would round the result half away from zero where Jadpo rounds half to even, so the stored value could differ from Jadpo's.
  next: Give the value at most 2 decimal places, or a type with `scale: 2` or fewer (agent)
  rule: data.stored_decimal_scale
  help: docs/language/06-reads-and-writes.html#data-stored-decimal-scale

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
  reason: A `conflict:` arm is keyed by the rule the store refuses, written `Entity.name`: a field with `unique: true`, a rule named in `unique:`, a `restricted` reference that refuses a delete, or a lifecycle transition whose `when:` can refuse it. `Customer.create` can break `Customer.email`. Did you mean `Customer.email`?
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
use ordering/entities { Cart }

action clear_cart(cart: Cart) -> Unit {
    Cart.delete(cart)
}
```

```jadpo check project=e-action-spans-stores file=domains/ordering/actions/checkout.jadpo expect=store.action_spans_stores
use ordering/entities { Cart, Order }
use ordering/actions { clear_cart }

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
 --> domains/ordering/actions/checkout.jadpo:8:5
  |
8 |     clear_cart(cart)
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
| Counts and sums in queries, and totals kept on an entity in the same transaction (computed fields, from the stores mapping §1.8) | 6 | Deferred. Amended 2026-10-17 by area 19 (questions 21 and 28): aggregate queries (`count`, `sum`, `min`, `max`, `average`, grouped by a field or a time bucket, using standard operations with SQL forms) are decided in principle; designed and built in chunk 5.6c ("Counts, sums and groups"); totals kept on an entity stay deferred |
| `expiry:` on lossy-store entities hides expired records from queries and updates like `visible:` | 5 | Answered by area 5 question 15 (2026-10-16): `expires_after:` |
| Several changing calls: the record, replay and `diverged` | 12 | Question 16 |

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
15. **Can a one-step update refuse when the stored value would break a
    rule?** Raised 2026-10-15 by an outside review. Taking stock
    (`Product.stock - quantity`) can't be a one-step update today, because
    the declared ranges can't prove the result stays at 0 or above, so the
    action loads the product first, and buyers racing on one product rerun
    each other; in a flash sale that uses up the three-attempt budget.
    *Recommendation:* a stored-value construction in `set:` may map the
    rules it can break, as any construction does (area 2):
    `stock: Stock(Product.stock - quantity) { min: OutOfStock }`. The
    database checks the condition in the same statement (`WHERE "stock" >=
    $1`), and no row updated is the mapped failure, so the update stays one
    step and never reruns. Open.
16. **What do several outside calls in one action change about reads and
    writes?** (from area 12, 2026-10-16) *Decided 2026-10-16 (Richard):*
    the writes come after the last call, with no transaction open during
    any call; the runtime's step record is written in short transactions
    of its own and deleted in the action's commit; a rerun after a
    concurrent change replays recorded steps with the record's `clock.now`,
    stopping as `diverged` if a fresh read changes a request.
17. **Which queries does an entity in Redis support?** (from area 5
    question 15) *Decided 2026-10-16 (Richard):* get, create, update and
    delete by `id`; lookup by a `unique:` field; listing and counting by
    equality on one field in `indexes:`, in creation order; revision
    checks; policy grants of simple equality on the record's own fields,
    checked after reading. Joins and `include`, ranges, other orderings,
    `matches` and other grants are compile errors naming PostgreSQL.

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
| `none` to a field that can't be cleared | Chosen in chunk 3.10: `none` given to an optional `Input` field whose type isn't nullable is the same as leaving it out, so a patch keeps the stored value | 2026-10-15 |
| Refusals in a called action | Chosen in chunk 3.3, widened in chunk 3.10: an action whose failure its caller handles with `match` runs inside a savepoint, so its failure undoes all it wrote, a refusal among it, and the caller carries on | 2026-10-15 |
| Transitions as operations | Chosen in chunk 3.4: `Entity.<transition>(record or id)`, an update of the fields its `set:` gives, giving back the record (nothing through `delete`); its block takes only `conflict:`; it reaches only visible records | 2026-10-15 |
| A transition's `when:` refusing | Chosen in chunk 3.4: a `when:` that asks more than `visible:` can refuse a record that is there, a conflict mapped in `conflict:` keyed by the transition (`User.disable: UserDisabled`), with no catch-all; one that asks only what `visible:` asks can't, since a hidden record is not found | 2026-10-15 |
| Counter decimals | A decimal added to or subtracted from a stored field in a counter update may not have more decimal places than the field's `scale`; built in chunk 3.4 as `data.stored_decimal_scale`, a value whose type sets no `scale` counting as too many (Richard) | 2026-10-15 |
| Isolation by what an action does | An action that loads no record, itself or through any action it calls, runs at read committed, so concurrent counters on one row both commit without a rerun; any other action runs at repeatable read. Corrects "as the action's first statement it never reruns", which PostgreSQL refuses at repeatable read (Richard, from an outside review). Built in chunk 3.7 | 2026-10-15 |
| What loads a record | Chosen in chunk 3.7: reading one (`get`, a query and its includes); being handed one (a parameter whose type holds a record, which area 5 calls a loaded record, so a record authentication loads for an action counts too); writing one back (an `update`, `delete` or transition of a record rather than its `id`, wherever the record came from); and a transition whose `when:` can refuse a visible record, whose second statement must share the first one's snapshot. A create (its references checked by the store in the same statement), an update, delete or unguarded transition by `id`, and a counter load nothing | 2026-10-15 |
| Isolation of fixtures and tests | Chosen in chunk 3.7: a fixture is decided as an action is; each action or fixture a test calls begins its own transaction at its own level; a read or write a test runs itself, and whatever else runs outside an action (migrations, purge), runs at repeatable read | 2026-10-15 |
| Outside calls and the transaction | No transaction is open during an outside call: reads before it run in a read-only snapshot closed before the call, the transaction opens after it, and a write to a record loaded before the call checks its row version, a change rerunning the action (Richard, from an outside review) | 2026-10-15 |
| Several outside calls | Writes after the last call; no transaction during calls; the step record in the runtime's own short transactions, deleted in the commit; a rerun replays with the record's `clock.now` and stops as `diverged` when a fresh read changes a request; a write refusable after a step took effect is warned (Richard, area 12) | 2026-10-16 |
| Writes before calls | Chosen in chunk 7.4b: a write placed before a later outside call, on any way through the body, directly or through an action it calls, is `data.write_before_call`, not declinable, at the write and naming the last call it comes before; starting or cancelling a run writes, and a run's `each:` and `finished:` are the run's | 2026-10-20 |
| Entities in Redis | Get, create, update and delete by `id`; lookup by a `unique:` field; equality on one indexed field, paged in creation order, and its count; revision checks; simple-equality grants checked after reading; everything else a compile error naming PostgreSQL; expired records hidden (Richard) | 2026-10-16 |
| Redis counters and lifecycles | Counter updates by `id` allowed as one Redis `increment`; no lifecycles on a Redis entity (decided for Richard) | 2026-10-16 |
| Constants in queries and updates | A `constant` (area 3) is a value fixed while compiling, so a query accepts one wherever it accepts a literal (`query.value_unsupported` names constants among a query's values), and the "constants" an update by `id` may use are these. Arithmetic in a query stays refused, on constants too. Built in chunk 5.3e (Richard) | 2026-10-16 |
| Aggregate queries | `group:` and `aggregate:` blocks shaped like `include:`; `count()`, `sum`, `min`, `max` and `average(F, scale:, rounding:)`, each what code gives; groups of exactly-comparing fields and time buckets of dates; `List<T>` with `group:`, `T` without; one statement. Built in chunk 5.6c, as "Counts, sums and groups" sets out (area 19, questions 21 and 28) | 2026-10-18 |
| Standard operations in `where:` (Phase 5 review) | Allowed where the operation has an exact SQL form, with a derived expression index, once conditions share one lowering (chunk 6.3b); refused as `query.condition_unsupported` until then (Richard, as recommended). Built in chunk 6.3b, as "Operations in conditions" sets out | 2026-10-19 |
| A query giving a map layer (Phase 5 review) | A query may return its records as a `FeatureCollection<G, P>`, naming the geometry field and the properties type, so a `GET` route can send a layer; nothing is computed beyond picking fields (Richard, as recommended after chunk 5.5u7). Built in chunk 5.5u8 | 2026-10-19 |
