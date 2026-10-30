---
title: Reads and writes
area: 6
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Reads and writes
<!-- topic: reads-and-writes -->

How a program reads and changes stored records: named queries, filters,
order and pages, totals, related records, the operations every entity
gets, what a refused write reports, and how an action stays
all-or-nothing while other requests change the same records. Where
records are stored, and how caches copy them, is on [Data](05-data.md);
who may read or change them is on [Policy](09-policy.md).

The examples follow the golden todo app's `planning` domain: `User`,
`Project` and `Todo`. Each example marked `check` compiles with this
checkout's `jadpo`, and its tests run against PostgreSQL. What isn't built
yet is listed in [What is built](#what-is-built).

## Named queries
<!-- topic: named-queries -->

Every read of stored records is a named query. An entity is a kind of
stored record, like a table with types ([Entities](05-data.md#entities-and-the-id)).
A query is one read of an entity: which records, in what order, and how
many. Each query lives in a file of its own in its domain's `queries/`
folder ([Files and names](01-project-layout-and-modules.md#files-and-names)).
You call it by name, as you call an action, the function that changes
records ([Functions and actions](03-logic.md#functions-and-actions)).

This query gives an owner's todos a page at a time, newest first. Its
`filter` parameter is a `ListTodos`, an `Input` type a route fills from the
request: an optional `status`, an optional `due_before`, a `page_size`
that defaults to 25 and an optional `after`, the cursor the previous page
returned:

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

`from:` names the entity, `where:` the records it keeps, `order:` their
order and `page:` how many come back at once and where the page starts.
`where_present:` applies its condition only when the value on the right
is there: when the request leaves out `status`, every status comes back.
The [Filters](#filters) and [Order and pages](#order-and-pages) topics
cover each section in full.

### How many records come back

The result type says how many records come back, so there is no separate
word for it. A query for exactly one record also needs `missing:`, which
names the failure it gives when no record matches
([`missing:`](04-failures.md#a-read-that-finds-nothing-missing)):

| Result type | Returns | Needs |
|---|---|---|
| `TodoView` | exactly one record | a `where:` on a unique field, and `missing:` |
| `TodoView?` | zero or one | a `where:` on a unique field |
| `List<TodoView>` | up to the number `limit:` sets | `order:` and `limit:` |
| `Page<TodoView>` | one page and a cursor for the next | `order:` and `page:` |
| `FeatureCollection<Point, TodoPin>` | a map layer of up to the number `limit:` sets, each record a point on the map ([A query giving a map layer](#a-query-giving-a-map-layer)) | `geometry:`, `order:` and `limit:` |

Some apps are shared by several tenants, such as companies, whose records
are kept apart ([Tenancy](09-policy.md#tenancy)). There a `unique:` rule
holds within each tenant: two companies can each have a project called
Launch ([A unique rule holds within its tenant](09-policy.md#a-unique-rule-holds-within-its-tenant)).
So a unique field alone doesn't pick one record. A query for one record
compares the record's tenancy reference in its `where:` as well as the
unique field, unless it compares a reference to another record of the
same tenant, which already picks the tenant.

### What a result holds

A result's element type is the entity, or a type built from its fields.
`type TodoView = Output { pick: [Todo.id, Todo.title] }` is a type for
output that picks two of `Todo`'s fields
([Building a type from other types](02-values-and-types.md#building-a-type-from-other-types)).
The compiler reads only the fields that type needs.

That type can't have a rule across its fields, such as "`ends_at` is
after `starts_at`" ([Rules across fields](02-values-and-types.md#rules-across-fields)).
This holds even when each field's own limits would make the rule always
true. The same goes for every other type that holds a query's records,
including a `Nearest<T>`'s item ([Nearest first](#nearest-first)), a map layer's properties,
an aggregate's rows and the related records a query brings with it. A
result type with such a rule is the error
[`query.result_rule`](../errors/query.result_rule.md).

The reason is that Jadpo checks a rule whenever a value is made, and a
broken rule is a failure your code handles with `attempt`
([Passing a failure on](04-failures.md#passing-a-failure-on-attempt)). A
read has no `attempt` around it, and the database can't check the rule as
it reads. So a stored record that broke the rule could only be a
[defect](04-failures.md#defects), a bug rather than a failure the caller
handles. Instead, read into a type with the same fields and no rule, then
build the type with the rule from it in code, inside an `attempt`.

Records the entity's [lifecycle](05-data.md#lifecycle) hides, such as
soft-deleted todos, are left out of every query.

A query writes nothing and calls no actions
([Functions and actions](03-logic.md#functions-and-actions)). Call it by
its bare name in its own domain. From another domain, put the domain's
name first, `planning.todos_for_owner(...)`, which works when the query
is `public`.

### The statement it sends

`jadpo sql todos_for_owner` prints the SQL statement the query sends. It
is one `SELECT` with the query's conditions, an `ORDER BY`, and a `LIMIT`
one more than the page's size, so the runtime knows whether a next page
exists. It selects only the columns the result type needs, plus the
`order:` fields a page cursor needs and any field a
[policy](09-policy.md) condition checks outside the database.

The statement's text is fixed when the project is compiled. A
`where_present:` condition and the cursor are written
`($n IS NULL OR ...)`, so the same query always sends the same statement.

A query called inside an action reads in the action's transaction. Called
from a test, it runs in a short transaction of its own.

## Filters
<!-- topic: filters -->

Each `where:` line is one condition, and a record must meet all of them.
A condition can use comparisons, `and`, `or`, `not`, `== none` for an
empty field, and `in` with a list. Fields are named with their entity, as
`Todo.status`, so `where: Todo.id == todo_id` can't be misread as
comparing a value with itself.

`where_present:` applies its condition only when the value on the right
is present. That is how a request's optional filters are written: below,
`filter.due_before` narrows the list only when the request gave it.

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
named first, compared with a value. So a condition can't compare two
fields, can't compare with a value worked out in the query, and can't
apply a function to a field, except for the operations below.

The values a condition compares with are parameters and their fields,
literals, enum values, lists of those, and `clock.now`. A
[constant](03-logic.md#constants) works wherever a literal does, since its
value is fixed when the project is compiled. `<` and the other orderings
take numbers, times, dates and durations, as they do in code.

A field may be passed to three kinds of operation:

- **`ai.distance`**, compared with a bound
  ([The nearest-neighbour query](21-ai.md#the-nearest-neighbour-query)).
- **A `geo.` test**, `geo.within`, `geo.contains` or `geo.intersects`. It
  takes a geometry field first and a parameter after, on a `where:` line
  of its own or joined by `and`. A GiST index, PostgreSQL's index for
  shapes, finds the records, and Jadpo's own code decides each one, so the
  query gives the same answer as code
  ([Queries](20-geospatial.md#queries)).
- **A standard operation with an exact SQL form**, which stands where the
  field would ([Operations in conditions](#operations-in-conditions),
  below).

A field that is `none` meets no comparison except `!=`, and `not` keeps
that meaning, so SQL's `NULL` never shows through. A `where:` line can't
compare with a value that may be absent: that is what `where_present:` is
for.

The compiler creates the indexes the conditions and order need
([Indexes](05-data.md#indexes)).

### Operations in conditions

A standard operation may stand where a field does when it has an exact
SQL form ([SQL forms](19-standard-library.md#sql-forms)). The field is its
first argument, and every other argument is written out: a literal, a
constant, or an enum value such as `Rounding.half_even`. The database
works the operation out with that form, which gives exactly what the
operation gives in code. So the query keeps the same records `list.filter`
would keep with the same operation.

The operation's result is compared with a value as a field is: by `==`,
`!=`, `in` and, for numbers, times, dates and durations, the orderings. An
operation that gives a `Bool` is the condition on its own.

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

This test runs on PostgreSQL and shows the database agreeing with code: a
title is counted in code points, as `text.length` counts it, however many
bytes it takes, and a year ends where `temporal.year` says it does:

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

The rules for an operation in a condition:

- **Every other argument is written out:** a literal, a constant, an enum
  value, a rounding mode, `decimal.round`'s `scale:` or
  `text.pad_start`'s `fill:`. The compiler holds each to the limits code
  holds it to (a zero divisor, a negative count, empty text to find), and
  writes it into the statement's text. That way the index and the
  statement hold the same expression, and PostgreSQL matches them. A value
  given when the query runs would change what the index holds, so a
  parameter as another argument is `query.condition_unsupported`.
- **Only an exact form, and one an index can hold.** An operation without
  a SQL form is `query.condition_unsupported`: `text.lower`, for example,
  whose result PostgreSQL's collation would decide. So is an operation
  whose form an index can't hold. `map.count` and
  `temporal.period_between` read a subquery, and `hash.sha256` and
  `encoding.to_utf8` of text call `convert_to`, which PostgreSQL marks as
  stable rather than fixed, since it reads the database's encoding. A
  range's operations take a range written out, which a query can't write.
- **The field first, once.** The field is the operation's first argument,
  given as it is. An operation of another operation's result, or with the
  field anywhere else, is `query.condition_unsupported`.
- **Compared as a field is.** A literal the result is compared with is
  read as a value of the result's type. The `not` of an operation that
  gives a `Bool` is written `IS FALSE`, since PostgreSQL turns
  `NOT (a = b)` into `a <> b` before it looks for an index. `== none` is
  `type.incomparable`, since an operation always gives a value.
- **`none`.** A record whose field is `none` meets no condition on an
  operation of that field, `not` included (the SQL is
  `"field" IS NOT NULL AND ...`), since code can't call the operation on
  `none`. `ai.distance` keeps its plain statement, since pgvector's
  operators give `NULL` for `NULL`.
- **Anywhere a comparison stands:** under `not` and `or`, on a
  `where_present:` line, in an include's `where:` and in an aggregate
  query, since the database works the operation out exactly. `order:`
  still takes fields only. A `geo.` test, which an index finds and code
  decides, still stands on its own line or under `and`, and not in
  includes or aggregates.
- **The index.** The compiler derives one B-tree index on each
  expression, however many queries and includes compare it, as it derives
  one for a field's test ([Indexes](05-data.md#indexes)). It is named
  `<table>_<column>_<operation>_idx`, with its digest added when another
  expression of the same operation and field needs a different name. Like
  every derived index, it is partial, holding only the records
  `visible:` keeps. The schema keeps it as an index's `expression`,
  beside the column it reads, and a migration creates it with
  `CREATE INDEX ... ((expression))`.

Comparing a field of money or a range, or one kept whole as JSON, isn't
built yet ([What is built](#what-is-built)).

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
<!-- topic: order-and-pages -->

`order:` lists fields, each with `asc` or `desc`. When the order could tie,
the compiler adds `id` as the last field, so records with equal values
never swap places between pages.

Every list has a bound: `limit:` is a number, or a value whose type has a
`max:`.

### Pages

A page is found by keyset pagination: each page starts after the last
record of the page before, by its `order:` values, rather than by
counting past an offset. `Page<T>` is a standard type holding
`items: List<T>` and `next: PageCursor?`. `next` is an opaque cursor,
which the client sends back as `after:` to get the next page. A request's
input for a page looks like this:

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

`Page<T>`, `PageCursor` and `PageSize` (an `Int` from 1 to 100) are built
in. A page size must have an upper limit, so an app that needs a different
maximum declares its own `Int` type with a `max:`.

There is no offset, because a large offset is slow, and it skips or
repeats records when rows are added.

A cursor records which query made it, so a cursor from another query is
refused as invalid input: the runtime's 422 `invalid_input`. A page's
order fields must be required fields, since the next page starts after
the last record's values.

### What can be sorted

Text sorts by code point, as Jadpo compares it, whatever the database's
collation. An enum, a `Bool` or a structured field can't be sorted by,
since the database and Jadpo would disagree on their order.

### Nearest first

A vector field is ordered by nearness instead: `order: [F nearest v]`.
That is the only order of a query returning `List<Nearest<T>>`, and it
goes with `ai.distance(F, v)` compared with a bound in `where:`
([The nearest-neighbour query](21-ai.md#the-nearest-neighbour-query)).

A point field is ordered by distance, nearest first, with
`order: [geo.distance(F, p) asc]`. That must be the list's only order, and
it goes with `where: geo.within(F, p, distance: d)` on the same field and
point, which bounds what is sorted. Records are sorted by the distance
`geo.distance` gives, then by `id` ([Queries](20-geospatial.md#queries)).

## Counts, sums and groups
<!-- topic: counts-sums-and-groups -->

A query can count and add up the records it finds instead of returning
them. `aggregate:` names each total the database works out, and `group:`
splits the records into groups, giving one row per group. This API
service reports its calls by month, counts them by how they were replied
to, and totals an account's calls:

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

`monthly_calls` gives one row for each month that has calls.
`reply_tallies` gives one for each reply that some call got, so `refused`
appears only once a call is refused.

Both blocks are shaped like `include:`: each entry is named after the
result field it fills. Each row is a value of an `Output` (or `Object`)
type. Every field of that type is a group or an aggregate of the same
name, and every group and aggregate is a field of it. Nothing else of the
entity is read, since the records in a group have no one value of any
other field. `count`, `sum`, `min`, `max` and `average` are words of
`aggregate:` only, so they take no namespace and mean nothing elsewhere.

`jadpo sql monthly_calls` prints the one statement it sends, on two
levels. A grouped subquery works out the groups and totals: its `SELECT`
of each group and aggregate, the conditions, and `GROUP BY` the groups.
The outer statement selects, orders and limits them, so an average names
its sum and its count once. A `Money` sum's currency is the one its type
fixes, sent as a parameter.

A query otherwise computes nothing, and aggregates are the exception. A
time bucket and an average are written with their standard operations'
[SQL forms](19-standard-library.md#sql-forms), and the rest are
PostgreSQL's `count`, `sum`, `min` and `max`. So an aggregate query is
always one statement, however many records it counts.

### The aggregates

- **`count()`** is how many records there are, an `Int`.
- **`sum(F)`** adds up a field built on `Int`, `Decimal` or `Duration`, a
  quantity, or `Money` whose type fixes `currency:`. Two currencies can't
  be added, and a sum of nothing needs a currency for its zero. A
  percentage isn't summed. A sum gives what `+` gives: the field's
  representation (`Int`, `Decimal`, `Duration`, the quantity or `Money`),
  so a money or quantity sum keeps its kind.
- **`min(F)` and `max(F)`** give the smallest and largest value of a field,
  as the field's own type. The field is one a query can sort by (numbers,
  times, dates, durations, quantities, percentages, and text by code
  point), other than an identity, since PostgreSQL has no `min` of a
  `uuid`.
- **`average(F, scale:, rounding:)`** of a field built on `Int`, or on
  `Decimal` with at most `scale:` places, is a `Decimal` with exactly
  `scale:` places: the exact mean, rounded once as `rounding:` says. Both
  are written out, a whole number from 0 to 38 and a `Rounding`, as
  `decimal.round`'s are, since an average rarely comes out exact and the
  rounding is part of what the report says. An average of durations,
  quantities or money isn't offered until an application needs one.

Each aggregate gives exactly what code working from the records gives:

- `count()` is `list.count`.
- `sum` is `list.sum`, `+` for durations and quantities, or `money.sum`
  with the type's currency.
- `min` and `max` are the values `list.min_by` and `list.max_by` find.
- `average` is `decimal.divide` of the sum by the count, at `scale:`
  places, with `rounding:`. It goes through `decimal.divide`'s SQL form,
  which works the quotient out exactly as the runtime does, where
  PostgreSQL's own `avg` and `/` round to places they choose.

A whole number, a duration and money's minor units are summed exactly and
cast back to `bigint`, so a sum past 64 bits stops the read. A decimal
past 38 digits fails as it is read. Both are defects in code.

`sum`, `min`, `max` and `average` skip a field's `none` values, as a list
of the values that are there would be summed. `count()` counts records,
whatever their fields hold; to count a field's values, add a condition
(`where: ApiCall.wait != none`). A group whose values are all `none` sums
to zero, and its `min`, `max` and `average` are `none`.

### The groups

A group is a field, or a time bucket: `temporal.year`, `temporal.month` or
`temporal.day` of a field built on `CalendarDate`, each an `Int` through
its [SQL form](19-standard-library.md#sql-forms). The field itself is the
day, and a year and a month together are a month.

A group field is built on `Bool`, `Int`, `Text`, `Uuid` (a reference among
them), `CalendarDate` or a plain enum, types whose stored values are equal
exactly when Jadpo's are. A field that may be `none` puts its records
without a value in a group of their own, `none`.

These can't be groups:

- **Decimals, quantities and percentages**, since `numeric` equality puts
  `1.0` and `1.00` in one group, and only one of them could be shown.
- **Instants, durations, money and structured values.** An instant has no
  bucket either: its day needs a zone, and PostgreSQL's time zone database
  can differ from the one built into every Jadpo build
  ([Zones and local times](10-configuration-and-time.md#zones-and-local-times)).
  So no SQL form of `temporal.in_zone` gives exactly what code gives.
  Until that is decided, a report keeps the day as a `CalendarDate` field.

### The result

Each group and aggregate fills the result's field of its name. That field
takes its type, or one it widens to, as a stored value fills a field
(`calls_made: Int`, `largest_call: ApiCall.tokens_spent`). A field that can
be `none` needs a `T?`: a group of a field that may be `none`, and `min`,
`max` and `average` of such a field or with no `group:`.

**With `group:`,** the query returns `List<T>` with `order:` and `limit:`,
one row per group that has records. So no group is empty: a month without
calls gives no row, as SQL's `GROUP BY` gives none, and filling the gaps
is up to the caller.

`order:` names groups and aggregates by their own names, never the
entity's fields. The compiler adds each group `order:` doesn't name as a
tie-break, ascending, since the groups tell the rows apart. A group sorts
in Jadpo's own order ([Values and types](02-values-and-types.md)): numbers
by value, text by code point (`COLLATE "C"`), days in time order, `false`
before `true`, and an enum's variants in their declared order
(`array_position` of the codes it is stored as). An aggregate sorts when
its type is ordered, which a `Money` sum isn't. A total that may be `none`
sorts where PostgreSQL puts `NULL`, as a list query's order field that
may be `none` does.

**Without `group:`,** the query returns `T`, one row over every record it
finds, and takes no `order:`, `limit:` or `missing:`, since there is
always the row. When nothing matches, `count()` is 0, a sum is zero (in
the type's currency for `Money`, at its scale for a `Decimal`), and `min`,
`max` and `average` are `none`.

### What an aggregate query doesn't take

An aggregate query returns `List<T>` or `T`, never `T?` or `Page<T>`, and
reads one entity. It takes no `include:`, no `page:`, no `missing:` and no
`nearest` order, since a page of groups would need a cursor over groups
and nothing has asked for one.

Its conditions are a query's usual `where:` and `where_present:` lines.
Policy and the lifecycle's `visible:` limit the records it counts, as they
limit any query's. Every field a group or an aggregate reads must be one
every caller may read, as an output's fields must be
([Field restrictions](09-policy.md#field-restrictions),
`policy.output_field_unproved`).

## A query giving a map layer
<!-- topic: a-query-giving-a-map-layer -->

A query can return its records as a map layer: a
`FeatureCollection<G, P>` ([Interop](20-geospatial.md#interop)), where `G`
is the geometry type and `P` the type of each feature's properties. A
`GET` route can then send the layer a mapping library fetches, as plain
RFC 7946 GeoJSON ([Output and status](07-http.md#output-and-status)).
Here are a bakery map's layer, and the bakeries within reach of a door:

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

The result type names the properties type, as `List<Nearest<T>>` names its
records. `geometry:` names the stored field that is each feature's
geometry, a `G` every record has. It has to be named because an entity may
hold several geometry fields, and the result type can't say which. `P`
must be an `Object` type, as a feature's properties always are
([Interop](20-geospatial.md#interop)). Its fields are filled from the
record's fields of the same names, as any result type's are.

The query picks fields and computes nothing, so it is a bounded list like
any other: `order:` and `limit:`, with `geo.` conditions and an order by
`geo.distance` if it needs them ([Queries](20-geospatial.md#queries)).

- **`geometry:`** names a field of the entity read, of exactly the layer's
  own geometry type, and never `none`, since a feature always has a
  geometry. So a `Point?` field, a `Polygon` field in a layer of `Point`s,
  or any field in a layer of `Geometry` (which would be a value built from
  it) is `query.geometry_field`. A field the entity doesn't have is
  `query.unknown_field`, as in a condition.
- **One bounded list.** `order:` and `limit:` are required. `page:` is
  refused, since a `FeatureCollection` has no cursor, and so are
  `missing:`, `include:`, `group:` and `aggregate:`, since a layer's
  properties are each record's own fields, one feature to a record.
- **Its properties.** Each field of `P` must be the record's field of that
  name, or the query is `query.result_field`. A feature's identity goes in
  its properties, so `P` picks `id` where the map needs it.
- **Field policy.** A layer is sent as it is, so it is held to what an
  `Output` is held to
  ([Field restrictions](09-policy.md#field-restrictions)). Every caller who
  may read the records must be allowed to read each property, the
  geometry, and every field the conditions and order test, or the query is
  `policy.output_field_unproved`. This is proved where the query is
  declared, so an action returning the layer carries nothing more.
- **Only what a `GET` can ask for.** A `GET`'s input is its query string
  ([One input](07-http.md#one-input)), which can't hold a `BoundingBox`.
  So a layer of only what is in view would still need a body. The teaching
  example `21-veg-box-rounds` sends its whole drop point layer with
  `GET /drop-points`.

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
  help: https://jadpo.dev/docs/errors/query.result_field

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
  help: https://jadpo.dev/docs/errors/policy.output_field_unproved

check failed: 1 error, 1 file
```

## Children and parents
<!-- topic: children-and-parents -->

An `include:` section adds related records to the result, each under a
field of the result type. This board shows a project with its owner and
its open todos:

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

Naming one of this entity's own references (`Project.owner_id`) includes
the record it points to, its parent. Naming another entity's reference to
this one (`Todo.project_id`) includes the records that point here, its
children. Children are a list, which needs `order:` and `limit:` in a
block, or at most one record when that reference is unique. A parent never
declares its children ([Children](05-data.md#children)).

Each include costs one more database query, however many records the
result holds, never one per record. A parent or a unique child is read by
`= ANY` of the keys the records hold. A list is read through a `LATERAL`
subquery that orders and limits each record's list on its own.

Includes nest up to two levels and stay inside the domain. They reach
another [store](05-data.md#stores) only to fetch records by `id`. A parent
is included even when its lifecycle hides it, because the reference still
holds.

## The operations every entity gets
<!-- topic: the-operations-every-entity-gets -->

Every entity gets these operations, called through the entity's name, as
`Todo.get(todo_id)`. Only actions in the entity's own domain may call the
ones that write. Other domains call that domain's public actions instead.

| Operation | Does |
|---|---|
| `Todo.get(todo_id)` | reads one visible record, or fails `TodoNotFound` |
| `Todo.create { set: { ... } }` | adds a record |
| `Todo.update(todo) { set: { ... } }` | changes fields of a record loaded earlier in the action |
| `Todo.update(todo) { patch: input }` | changes only the fields a request supplied |
| `Todo.update(todo_id) { set: { ... } }` | changes the record with that `id` in one step (counters, below) |
| `Todo.delete(todo)` | removes the record, or runs the lifecycle's `delete` transition |
| `User.disable(user)` | runs a lifecycle transition, named after it |

An `update`, `delete` or transition takes either the record the action
loaded earlier or its `id`. By `id`, it fails with the entity's not-found
failure when no record has that `id`.

An update gives its fields in `set:`, or the request's changes with
`patch:`, never both. `when_supplied:` goes with `patch:` (both are below).

A create and an update give back the record as stored; a delete gives
nothing. Each fills the fields the store owns: a new version 7 UUID as the
`id` on create (`Ctx::new_id`, from the invocation's seed), and the
operation's time in each generated field, `on_create` once and `on_change`
on every update.

Every write joins the action's transaction. In the compiler's graph of the
program, a `writes` edge runs from the action or function to the entity.

In this action, `attempt User.get(owner_id)` gives the user, or passes on
`UserNotFound`. `Project.create` sets every field but `id` and the
generated ones:

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

### Transitions

A transition ([Lifecycle](05-data.md#lifecycle)) is an update of the
fields its `set:` gives. Like an update, it runs on the record loaded
earlier or by `id`, and gives back the record as stored; a `delete`
transition gives nothing. Its block takes only `conflict:`, since what it
changes is fixed in the lifecycle.

Like every update and delete, a transition reaches only a record that
`visible:` lets through, so a hidden record is the entity's not-found
failure. A `when:` that asks more than `visible:` can refuse a record that
is there. Map that refusal in `conflict:`, as you would a rule, keyed by
the transition. Here `UserDisabled` is what the action fails with when
`disable`'s `when:` doesn't hold:

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
WHERE "id" = $3 AND "stage" = $4`, with the stages bound as their column
holds them, `'disabled'` and `'active'`
([Every boundary through the contract](19-standard-library.md#every-boundary-through-the-contract)).
When it changes no row, a second statement asks whether the user is there
and visible, which tells `UserDisabled` from `UserNotFound`.
`jadpo sql disable_user` shows both.

### The not-found failure

Nothing else is an entity operation: `Todo.fetch(todo_id)` is
`semantic.unknown_callee`. `Todo.title(value)` still builds a value of the
field's type ([Creating values](02-values-and-types.md#creating-values)).

`TodoNotFound` is the domain's failure of that name in `failures/`. When
the domain declares none, the compiler provides one: kind `not_found`,
public, with no fields, placed at the entity in the graph. When a declared
one has a field typed `<Entity>.id`, a read or write that fails fills it
with the `id` it looked for. A write that can't fail needs no `attempt`
([Attempt](03-logic.md#attempt)).

This domain declares no `TodoNotFound`, so the compiler provides it:

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

### Patches from a request

A patch takes an `Input` whose fields are optional and match the
entity's. A field the request left out keeps its value, and one sent as
`none` is cleared ([Values that may be missing](02-values-and-types.md#values-that-may-be-missing)).
`when_supplied:` sets further fields when a given field was supplied: here
a new `due_at` also clears `reminder_sent_at`. A patch with no fields
changes nothing and succeeds, as a JSON Merge Patch does, so there is no
`empty:` failure.

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

The statement's text is fixed: each field of the input is written
`CASE WHEN <supplied> THEN <value> ELSE <column> END`, and an `on_change`
field changes only when a field was supplied.

A value built without one of its optional `Input` fields remembers that
the field was left out. Only a patch reads that, and the value still
equals one given `none`.

A field whose type can't be `none` (`title` above) can't be cleared, so
`none` given to it is the same as leaving it out, and the patch keeps the
stored value. That isn't an error, since passing an optional value on
into an optional field, such as a page's `next` cursor, is ordinary code.

## When a write is refused
<!-- topic: when-a-write-is-refused -->

A write can be refused for two reasons, and a lifecycle transition for a
third, when its `when:` doesn't hold
([Transitions](#transitions), above):

- **The record isn't there.** That reports the entity's not-found
  failure, `TodoNotFound`. It includes a reference that names a missing
  record: a bad `project_id` is `ProjectNotFound`.
- **A database rule refuses it:** a unique rule, or a `restricted`
  reference blocking a delete.

Map each rule a write can break in its `conflict:` block, keyed by the
rule: `Project.name_per_owner` (a named unique rule), `User.email` (a
unique field) or `Todo.project_id` (a reference that blocks deleting a
project). The compiler knows which rules every write can break, so a
missing mapping is a compile error, and there is no catch-all. The
database's own error is kept as the failure's internal cause
([Failures](04-failures.md)).

Here creating a project can break the unique rule on its owner and name,
and closing one is blocked while todos still point at it. `owner.id` comes
from a loaded record, so the owner is surely there and the create can't
fail with `UserNotFound`:

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

What a write can break follows from what it writes:

- a create: every unique rule of its entity;
- an update: the unique rules over the fields it sets;
- a delete: each `restricted` reference to the record, or to a record its
  delete cascades to.

A reference's value surely names a record that is there when it is read
from a record loaded earlier in the action (`owner.id`, or a loaded
record's own reference to that entity), or when it is `none`. Any other
value may name a record that isn't there, so the write can fail with that
entity's not-found failure, which `attempt` passes on. A loaded record is
surely there because the transaction's snapshot holds it, and a
concurrent delete of it is a concurrent change
([All-or-nothing actions and concurrent changes](#all-or-nothing-actions-and-concurrent-changes)).

The runtime maps the store's constraint name back to the rule, since
Jadpo names every constraint itself ([Data](05-data.md)).

### A failed action inside another

An action whose failure its caller handles with `match` runs inside a
savepoint, a point in the transaction it can be rolled back to. When it
fails, everything it wrote is undone, a refused write among them, and the
caller carries on in a transaction the failure didn't end. An action is
all-or-nothing whoever calls it.

An action that passes its failure on with `attempt` needs no savepoint:
the failure ends its caller too, up to a `match` that undoes both, or up
to the outermost action, which rolls back.

## All-or-nothing actions and concurrent changes
<!-- topic: all-or-nothing-actions-and-concurrent-changes -->

Every action is all-or-nothing
([Functions and actions](03-logic.md#functions-and-actions)): its writes,
and those of the public actions it calls, commit together or not at all.
You write nothing for this.

A write to a record loaded earlier in the action checks that nobody
changed the record in between. If someone did, the runtime throws the
attempt away and runs the whole action again, with fresh reads and a
fresh `clock.now`:

```text
request A: loads todo 7                 request B: loads todo 7
request A: updates todo 7, commits
                                        request B: updates todo 7: changed since loaded
                                        request B: runs again, loads the new todo 7, updates
```

Running it again is safe because an action's outside call comes before its
writes and carries an idempotency key, a value that lets the provider
recognise a repeat. So a rerun repeats the call harmlessly
([Integrations](11-integrations.md)). There are two exceptions:

- **A changing call with no key.** Once it has gone out, the action isn't
  run again. It stops as `outcome_unknown`, for an operator to look at,
  since sending it again could act twice (see "Parked, never sent again
  blindly" in
  [The seam every outside call goes through](16-built-in-services.md#the-seam-every-outside-call-goes-through)).
- **An action with several changing calls.** Once its record of the calls
  exists, a rerun replays the recorded calls and keeps the record's
  `clock.now` ([Several outside calls](#several-outside-calls), below).

After three attempts within a second, the retry budget, the request fails
`unavailable` and the client may try again.

To catch a client that edits a stale copy across two requests, use HTTP's
`If-Match` ([HTTP](07-http.md)).

### Isolation levels

On PostgreSQL, an action that loads a record runs at the repeatable read
isolation level, where the whole transaction sees one snapshot of the
database. The runtime already retries its "could not serialize" error. An
action that loads no record runs at read committed instead, where each
statement sees what was committed before it began
([Counters and other one-step updates](#counters-and-other-one-step-updates),
below).

At repeatable read, the snapshot is taken at the action's first
statement, and a record it loaded is the record as of then. An `update`
or `delete` of a row another transaction changed since then is
PostgreSQL's serialisation failure, which the driver reports as a
concurrent change.

The runtime then rolls the transaction back, throws away the attempt's
events, and runs the whole action again from its first line. Its reads
see the other transaction's change, and its new records get new
identities. The budget is three attempts.

A record handed to an action from outside its transaction, as when a test
passes in one an earlier action returned, is checked only for its row
still being there.


### Outside calls hold no transaction

No transaction is open while an action waits on a provider. What an
action reads before its outside call runs in a short read-only snapshot,
which closes before the call goes out. The transaction opens after the
call returns, and the action's writes and any later reads run in it.
Reads between two outside calls get a snapshot of their own.

A write to a record loaded before the call checks that its row is
unchanged since it was loaded, by comparing PostgreSQL's row version,
`xmin`. A change is a concurrent change like any other: the whole action
runs again, and the call repeats harmlessly under its idempotency key.

So a slow provider holds no database connection, and the time it takes
never widens the window in which another request's change forces a rerun.
The guarantee is the one repeatable read gives: every record the action
writes is checked against the copy it loaded.

This is done where every outside call goes through Jadpo's runtime
([The seam every outside call goes through](16-built-in-services.md#the-seam-every-outside-call-goes-through)):

- **Reads before a call are a snapshot.** An action's transaction still
  begins at its first store operation, at its isolation level. If the
  action has run only reads when a call goes out, that transaction is the
  snapshot. It reads the row versions of the records it loaded (one
  statement per table, `xmin` by `id`) and rolls back, before the call is
  sent. Once the action has written, a call is refused
  (`call_in_transaction`), so no transaction is ever open while a call is
  out. An action with no call keeps its one transaction.
- **The transaction after the call.** The next store operation begins a
  new transaction at the same level, in the same store. Reads after a call
  run in it. If another call comes before any write, those reads were a
  snapshot of their own, and it closes the same way.
- **The row version check.** An update, delete or transition of a row
  loaded in a snapshot that a call closed compares the row's version in
  the write's own `WHERE`. A successful write locks the row and checks it
  in one statement, once per transaction. When no row is changed, a
  `FOR NO KEY UPDATE` probe, run only on that failure, tells whether the
  row has a different version or is gone, which is a concurrent change.
  It runs before the usual refusals for policy, a field, a lifecycle
  condition or a missing row.
- **Nested actions and writes by `id`.** A failed nested action's
  savepoint restores only the version checks whose locks survive its
  rollback. A write by `id` is checked too when the action loaded that row
  before the call, as repeatable read checks any write to a row changed
  since the snapshot. A row is held to the version the action first
  loaded, so a row read again after the call, or in a second snapshot, is
  still checked against that first version.
- **A change reruns the action.** A concurrent change throws the attempt
  away and runs the action again, within the budget of three attempts. Its
  keyed calls take the same steps again, so each repeats under the key it
  was first sent with ([Integrations](11-integrations.md)). A change found
  at the commit of a route's or a delivery's held transaction does the
  same. A changing call without a key that went out stops the action as
  `outcome_unknown` instead. A reference to a record loaded before a call,
  deleted while the call was out, is a concurrent change too, so the
  rerun's fresh read finds the record gone.
- **A batch's items.** An item of a run whose `each:` action makes outside
  calls is a batch of its own
  ([How a run is kept](12-jobs-and-events.md#how-a-run-is-kept)). Its
  calls go out with no transaction open, as any action's do, and its
  writes and the run's position commit after them under these rules. A
  write placed before a later call is refused at compile time
  ([Several outside calls](#several-outside-calls), below).

### Several outside calls

An action may make up to 8 changing calls, one after another
([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)).
For reads and writes this changes three things and keeps the rest:

- **Writes come after the last call.** The action's transaction opens only
  once its last outside call has returned; a write placed before a later
  call is a compile error naming the call it must follow. No transaction
  is open during any call, as above, and reads between two calls each get
  a snapshot of their own.
- **The runtime keeps its record apart from the action's transaction.**
  Each step is
  recorded before it is sent and its answer after, each in a short
  transaction of the runtime's own in the store the action writes; the
  record is deleted in the action's commit, so a committed action leaves
  none behind.
- **A rerun replays, with the record's time.** When a concurrent change
  reruns the action, recorded steps are replayed without being sent and
  `clock.now` is the time the record holds, not a fresh one; the reads are
  fresh. If a fresh read changes a request the action would send, the
  rerun stops as `diverged` before sending anything, held for an operator
  ([Jobs and events](12-jobs-and-events.md)), rather than sending a
  different request under a used key.
  Database reads are never recorded, so this check is what keeps a rerun
  honest.

The writes still commit together after every call has answered. So a
write that is refused for good, by a unique rule or a restricted
reference, after a step took effect is warned of at compile time, naming
the fixes: check with a read before the first call, record what happened
and finish in a subscriber, or undo it as another step
([Jobs and events](12-jobs-and-events.md)).

### A write before a call

A write placed before a later outside call is
[`data.write_before_call`](../errors/data.write_before_call.md). It is
reported at the write and names the last call the write comes before, so
checked code never reaches the runtime's `call_in_transaction` refusal,
which stays as the guard. Each part of the rule takes the most cautious
reading:

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
- **Not declinable.** The order is never a judgement call
  ([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)):
  run as written, the call would be refused.

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

## Counters and other one-step updates
<!-- topic: counters-and-other-one-step-updates -->

When a new value depends only on the stored one, the database works it out
in one step and nothing needs checking. A view counter is the usual case:

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

On the right of `set:`, `Video.view_count` means the value stored at the
moment of the write, while `video.view_count` would be the copy loaded
earlier. Such an update takes the record's `id` instead of a loaded
record. Its values may use only parameters, constants
([Constants](03-logic.md#constants)), `clock.now` and the record's stored
fields, so no increment is ever lost.

The statement is `UPDATE "video" SET "view_count" = ("view_count" + $1), ...`,
and no row is `VideoNotFound`.

The database works a stored field out only with `+` and `-`, which give
the same result in SQL as in Jadpo (`+` joining text is written `||`).
They must sit inside a construction the declared ranges prove:
`ViewCount(...)` here, since a count of at least 0 plus 1 is at least 0.
Anything else needs the record loaded first.

A decimal added to a stored field may not have more decimal places than
the field's `scale`, since PostgreSQL would round the sum its own way
(half away from zero) rather than Jadpo's. A literal counts by its own
places and any other value by its type's `scale`. A value whose type sets
no `scale` is refused (`data.stored_decimal_scale`).

### Which isolation level an action runs at

Which isolation level an action runs at follows from what it does. You
write nothing for it: the compiler decides it from the graph.

An action that loads no record, neither itself nor through any action it
calls, runs at PostgreSQL's read committed level. Of two requests counting
the same row at once, the second waits for the first, adds to its count
and commits, and neither reruns.

An action that loads a record runs at repeatable read
([Isolation levels](#isolation-levels)). So a counter racing another
request on a row it loaded reruns the action, as any concurrent change
does.

An action loads a record when it:

- reads one: `get`, or a query, with what it includes;
- is handed one: a parameter whose type holds a record, such as
  `video: Video`, or a list or object holding one;
- writes one back: an `update`, `delete` or transition of a record, rather
  than of its `id`;
- runs a transition whose `when:` can refuse a visible record, since
  telling that refusal from a missing record takes a second statement,
  which must see the first one's snapshot.

A create, and an update, delete or other transition by `id`, counters
among them, loads nothing: the store checks a reference a create sets in
the same statement. Calling a function never loads a record.

`jadpo check` decides each action and fixture from these and the actions
and fixtures it calls. It writes the level into the manifest as
`isolation` (`read_committed` or `repeatable_read`), and the runtime
begins the transaction at it (`BEGIN ISOLATION LEVEL READ COMMITTED` or
`REPEATABLE READ`). An action another action calls joins that action's
transaction at its level, which already counts what the callee does. Each
action a test calls begins its own.

So `view_video` above runs at read committed. Two requests counting one
video at once both commit at their first attempt, the second waiting for
the first's lock and adding to its count. A counter in an action that
read the video first still reruns the second. Both are checked live
against PostgreSQL in `crates/jadpo-cli/tests/writes.rs`.

What runs outside an action, such as a read a test makes itself,
migrations and purge, runs at repeatable read.

## Reading in a loop
<!-- topic: reading-in-a-loop -->

A query or `Todo.get` inside a `for` loop is a compile error, because it
would read once per item. Read the whole set with one query using `in`
instead:

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

Writes inside a loop are allowed: the list is bounded, and every write
joins the action's transaction.

An outside call inside a loop isn't allowed ([Loops](03-logic.md#loops)).
A loop of calls is `batches.each`
([How a run is kept](12-jobs-and-events.md#how-a-run-is-kept)). A batch
that should keep its partial progress is a schedule or subscriber
handling one item per action.

## One store per action
<!-- topic: one-store-per-action -->

An action writes to at most one [store](05-data.md#stores), counting the
public actions it calls. It may read other stores, but those reads are
outside its transaction. A read that decides a write (a policy, lifecycle
or invariant check) must come from the store being written.

Across stores there are no joins, filters or ordering. An include reaches
another store only to fetch records by `id`.

Moving an entity to another store can break an action that never names
it, so the error names the cause:
[`store.action_spans_stores`](../errors/store.action_spans_stores.md)
points at the write or the call that brings in the second store.

At run time, an action's transaction is begun in the store its first store
operation names. Reading another store outside the transaction isn't
built yet. For now, an action that reads a second store, itself or through
an action it calls, is `toolchain.not_built_yet` at the read or call that
brings it in, rather than failing as it runs:

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
  help: https://jadpo.dev/docs/errors/toolchain.not_built_yet

check failed: 1 error, 5 files
```

## Where a read comes from
<!-- topic: where-a-read-comes-from -->

Reads inside an action, policy and authentication always read the store,
inside the action's transaction, so they are current.

A display read is a query a route runs directly to show data
([What a route runs](07-http.md#what-a-route-runs)). It may be served from
the cache its query file declares ([Caches](05-data.md#caches)). A client
still sees its own writes: every write response carries a revision, and a
request carrying a revision newer than the cache skips it. The build lists
every query call with where it reads from.

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

Caches aren't built yet: they need the Redis driver.

## Entities in Redis
<!-- topic: queries-on-entities-in-redis -->

An entity in a Redis store
([Entities in Redis](05-data.md#entities-in-redis)) is read and written
through the same queries and operations as any entity. Redis keeps no
indexes of its own, though, so only what Jadpo can keep a key or a set for
is allowed:

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

None of this is built yet: there is no Redis driver.

## Reads across domains
<!-- topic: reads-across-domains -->

A query reads its own domain's entities only, and includes follow
references inside the domain. Another domain's data comes from its public
queries, called through that domain like its public actions. Reading a
list by ids keeps it to one query:

```jadpo fragment
var owners: List<OwnerView> = identity.users_by_ids(owner_ids)
```

## What is built
<!-- topic: reads-and-writes-built -->

| Feature | Status |
|---|---|
| Everything else on this page | Built. |
| Reading another store outside an action's transaction | Not built yet: it is `toolchain.not_built_yet` ("One store per action"). |
| Caches and entities in Redis | Not built yet: there is no Redis driver ("Where a read comes from", "Entities in Redis"). |
| A rerun's time bound | Not built yet: the runtime counts attempts only, so of "three attempts within a second" only the three is enforced ("All-or-nothing actions and concurrent changes"). |
| Comparing money, ranges and fields kept as JSON in a query | Not built yet: a field of money or a range, or one kept whole as its JSON (an object, a list, a set, a map or an enum with data), isn't compared in a query, its operations included (`toolchain.not_built_yet`), though `list.count` of a list would otherwise have a form ("Operations in conditions"). |

## Errors

Every error reads and writes can produce, each with an example that
triggers it and the compiler's actual output.

### Queries

- [`query.unknown_section`](../errors/query.unknown_section.md)
- [`query.repeated_section`](../errors/query.repeated_section.md)
- [`query.section_required`](../errors/query.section_required.md)
- [`query.section_unexpected`](../errors/query.section_unexpected.md)
- [`query.from_not_entity`](../errors/query.from_not_entity.md)
- [`query.other_domain`](../errors/query.other_domain.md)
- [`query.result_type`](../errors/query.result_type.md)
- [`query.result_field`](../errors/query.result_field.md)
- [`query.result_rule`](../errors/query.result_rule.md)
- [`query.geometry_field`](../errors/query.geometry_field.md)
### Conditions

- [`query.unknown_field`](../errors/query.unknown_field.md)
- [`query.condition_unsupported`](../errors/query.condition_unsupported.md)
- [`query.value_unsupported`](../errors/query.value_unsupported.md)
- [`query.optional_value`](../errors/query.optional_value.md)
- [`query.not_unique`](../errors/query.not_unique.md)
### Order, limits and pages

- [`query.order_direction`](../errors/query.order_direction.md)
- [`query.order_field`](../errors/query.order_field.md)
- [`query.limit_range`](../errors/query.limit_range.md)
- [`query.limit_unbounded`](../errors/query.limit_unbounded.md)
- [`query.page_setting`](../errors/query.page_setting.md)
### Includes

- [`query.include_unknown`](../errors/query.include_unknown.md)
- [`query.include_result`](../errors/query.include_result.md)
- [`query.include_depth`](../errors/query.include_depth.md)
### Groups and aggregates

- [`query.group_key`](../errors/query.group_key.md)
- [`query.aggregate_unknown`](../errors/query.aggregate_unknown.md)
- [`query.aggregate_argument`](../errors/query.aggregate_argument.md)
- [`query.aggregate_field`](../errors/query.aggregate_field.md)
### Entity operations

- [`data.read_in_loop`](../errors/data.read_in_loop.md)
- [`data.lifecycle_owned_field`](../errors/data.lifecycle_owned_field.md)
- [`data.lifecycle_hard_delete`](../errors/data.lifecycle_hard_delete.md)
- [`data.set_generated_field`](../errors/data.set_generated_field.md)
### Writes

A write that isn't an entity's is `data.write_not_entity`. A section a
write doesn't take (`missing:`, `where:`, `empty:`) is
`data.write_section_unexpected`, and an update with nothing to change is
`data.write_section_required`. A field given twice or unknown in `set:` is
`type.duplicate_field` or `type.unknown_field`, from
[Values and types](02-values-and-types.md). The conflict mappings are
below. A write placed before a later outside call is
`data.write_before_call`.

- [`data.write_before_call`](../errors/data.write_before_call.md)
- [`data.write_not_entity`](../errors/data.write_not_entity.md)
- [`data.write_section_unexpected`](../errors/data.write_section_unexpected.md)
- [`data.write_section_required`](../errors/data.write_section_required.md)
- [`data.update_by_id_value`](../errors/data.update_by_id_value.md)
- [`data.stored_value_outside_update`](../errors/data.stored_value_outside_update.md)
- [`data.stored_decimal_scale`](../errors/data.stored_decimal_scale.md)
- [`data.stored_value_unproven`](../errors/data.stored_value_unproven.md)
### Conflicts

- [`data.conflict_unmapped`](../errors/data.conflict_unmapped.md)
- [`data.conflict_unknown`](../errors/data.conflict_unknown.md)
- [`data.conflict_cannot_break`](../errors/data.conflict_cannot_break.md)
### Patches

- [`data.patch_not_input`](../errors/data.patch_not_input.md)
- [`data.patch_field_unknown`](../errors/data.patch_field_unknown.md)
- [`data.patch_field_required`](../errors/data.patch_field_required.md)
- [`data.patch_overlap`](../errors/data.patch_overlap.md)
### Stores

- [`store.action_spans_stores`](../errors/store.action_spans_stores.md)
