---
title: Data
area: 5
status: decided
checked-with: this checkout's jadpo on every gate run (archived examples: v0 3ce094d0)
---

# Data

How a Jadpo program declares the records it keeps and how their schema
changes. Reading and writing records is area 6; who may read or change one
is area 9.

> Decided on 2026-10-12. Built in chunk 3.1: entities, their fields, references, unique rules, indexes and stores, migrations and the test database; lifecycles in chunk 3.4. The examples marked `check` compile with this checkout's `jadpo`. Caches (with the Redis driver) and seeds come later, and their examples are proposals. The reasoning behind the store and cache rules is in [the stores and caching mapping](research/data-stores-and-caching-mapping.md); where it differs, this page wins.

Examples use the golden todo app's `planning` domain, plus a shop where Redis
is needed.

## Stores

Stores are connections declared in `app.jadpo` (area 11 merged area 5's
`stores:` into one `connections:` block). Each store's driver is fixed in
source, so every environment runs the same engine (D1 is on the roadmap).
Every entity lives in exactly one store, `main` unless its file says
`store: name`; anything never saved is a `type`. An entity in Redis must
declare its expiry ("Entities in Redis", below). An action writes to at
most one store; work across stores goes through an event and a subscriber.

**No database unless something needs one** (decided 2026-10-16, Richard).
An app that uses no database has none: no URL to configure, no
migrations, nothing for `jadpo dev` to start. With no database declared,
`main` on PostgreSQL is implied only once something needs a database. What
needs one: any entity stored in a database, sign-in and sessions (area 8),
events and the outbox (area 12), idempotency keys (area 7), webhook
de-duplication (area 11), flags and audit (area 16). Where something needs
a database and none can hold it (the app declares databases but none
called `main`, or names a Redis store for something only a database can
keep), `jadpo check` is an error naming the feature that needs one and the
connection to add.

```jadpo check project=stores file=app.jadpo
// app.jadpo in a shop that keeps its orders apart from its catalogue
application {
    connections: {
        catalogue: {
            driver: Driver.postgres
        }
        orders: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=stores file=domains/shop/entities/product.jadpo
/// Something the shop sells.
entity Product {
    id: Uuid
    name: Text

    store: catalogue
}
```

```jadpo check project=stores file=domains/shop/entities/order.jadpo migrate=initial
/// A customer's order.
entity Order {
    id: Uuid
    ordered_at: Instant {
        generated: Generated.on_create
    }

    store: orders
}
```

A database connection's address is a setting of the type its driver needs,
`url: config.database_url` with `database_url: PostgresUrl` in `config.jadpo`
(area 10, built in chunk 5.1); `jadpo migrate apply` and `status` read it from
the environment over `.env.local`, or take the database's URL with `--url`.
The other settings of a connection come with area 11.

## Entities and the `id`

An entity is a stored record with a lasting identity, in its own file under
its domain's `entities/` folder. Its identity is one field, `id`, based on
`Uuid`, generated as version 7 on create (or the version its named type
pins) and never supplied by callers; other code refers to it as `Todo.id`.
There is no compound identity: a link between two entities is an entity with
its own `id` and a compound unique rule. An entity value is an immutable
snapshot; reading a field never touches the database.

```jadpo check project=entities file=user.jadpo hidden
entity User {
    id: Uuid
}

type ProjectName = Text {
    max_length: 80
}
```

```jadpo check project=entities file=project.jadpo
// domains/planning/entities/project.jadpo
/// A group of todos with a shared goal.
entity Project {
    id: Uuid  // version 7, generated on create
    owner_id: User.id {
        on_delete: OnDelete.restricted
    }
    name: ProjectName
}
```

## Fields and their settings

A field's settings (`generated:`, `unique:`, `on_delete:`) go in a block
after its type, one per line. A nullable field is `T?`; only `Input` has
`optional:` and `default:`, and a starting value the compiler owns is the
lifecycle's `initial:`. A generated field is a required `Instant` set from
the operation's clock, on create or on every change; code never assigns it
and it is never in an `Input`.

```jadpo check project=fields
entity Todo {
    id: Uuid
    due_at: Instant?
    created_at: Instant {
        generated: Generated.on_create  // set once, when the record is created
    }
    updated_at: Instant {
        generated: Generated.on_change  // set on create and on every change
    }
}
```

## Unique rules

`unique: true` in a field's block makes that field unique. A rule over
several fields is named in the entity's `unique: { }` section with its fields
as a list; its fields must exist, be distinct and be required, so every
engine agrees on what a duplicate is. The name is what a write's conflict
mapping refers to (area 6).

```jadpo check project=unique
type ProjectName = Text {
    max_length: 80
}

entity User {
    id: Uuid
    email: Email {
        unique: true  // one user per address
    }
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
```

## References and `on_delete`

Any field typed `X.id` is a reference, with a foreign key and index. Its
block must say what happens when the referenced record is deleted; there is
no default. A cycle of required references is an error, because neither
record could be created first.

| `on_delete:` | When the referenced record is deleted | Allowed on |
|---|---|---|
| `OnDelete.restricted` | The delete fails with a conflict while any record refers to it | any reference |
| `OnDelete.cascade` | Records that refer to it are deleted in the same transaction | any reference |
| `OnDelete.cleared` | The reference is set to `none` | an optional reference (`Project.id?`) |

```jadpo check project=references
entity User {
    id: Uuid
}

entity Project {
    id: Uuid
}

entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted  // a user with todos cannot be deleted
    }
    project_id: Project.id? {
        on_delete: OnDelete.cleared  // the todo stays; it leaves the project
    }
}
```

## Children

A parent does not declare its children; the relationship is stated once, on
the child's reference. A query wanting a project's todos names that field and
gets a list, or at most one record when the field is unique. A many-to-many
link, such as project members, is an entity with two references and a unique
rule over both.

```jadpo fragment
// in a query: area 6 settles the include syntax
include: {
    todos: Todo.project_id          // a list: project_id is not unique
}
```

## Lifecycle

A lifecycle describes the stages a record goes through and when it drops
out of normal view. The common case is soft delete: deleting a todo hides it
everywhere at once but keeps it for 30 days, so it can be investigated or
restored, and then removes it for good.

```jadpo check project=lifecycle
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none  // which records normal reads can see
        transitions: {
            delete: {  // a named, allowed stage change
                when: deleted_at == none  // only allowed when this is true
                set: {
                    deleted_at: clock.now  // what the change sets
                }
            }
        }
        purge: {  // permanent deletion of hidden records
            after: 30d  // how long a hidden record is kept
            since: deleted_at  // counted from when it was hidden
        }
    }
}
```

- **`visible:`** Every read, list and count leaves out records that are not
  visible, and reading one by `id` reports `not_found`. Nothing else needs to
  remember to filter them.
- **`transitions:`** The only way to change the fields a lifecycle owns. Each
  has a name, an optional `when:` and a fixed `set:`. The generated
  operation `Todo.delete(todo_id)` runs it.
- **`purge:`** The application deletes hidden records once they have been
  hidden for the configured time, in small audited batches. Code cannot
  hard-delete a record that has a lifecycle.

`visible:` and `when:` are conditions on the record's own fields: comparisons
(`==`, `!=`, `<`, `<=`, `>`, `>=`), `and`, `or`, `none` checks and
`clock.now`, so scheduled publishing works. A starting value the compiler
owns is given in `initial:`:

```jadpo check project=lifecycle-publishing
type HeadlineText = Text {
    max_length: 120
}

type ReviewStage = Enum {
    draft
    in_review
    approved
}

entity Story {
    id: Uuid
    headline: HeadlineText
    stage: ReviewStage
    published_at: Instant?

    lifecycle: {
        visible: published_at != none and published_at <= clock.now
        initial: {
            stage: ReviewStage.draft  // every new story starts as a draft
        }
        transitions: {
            submit: {
                when: stage == ReviewStage.draft
                set: {
                    stage: ReviewStage.in_review
                }
            }
        }
    }
}
```

They cannot mention other records or who is asking: that is policy (area 9),
which keeps lifecycle conditions simple enough to become database indexes.

Built in chunk 3.4, with these readings of the decisions above (recorded
below):

- **Bare fields.** A lifecycle's conditions name the record's fields bare,
  as written above (`deleted_at`, not `Todo.deleted_at`); a qualified one is
  `data.lifecycle_condition` with the fix. `in` and `not` aren't among the
  decided operators, and the opposite comparison says the same.
- **What a lifecycle owns.** Every field a transition sets, and every field
  `initial:` gives, changes only through the transitions: a `create`,
  `update` or patch that gives one is `data.lifecycle_owned_field` (area 6).
  A new record gets its `initial:` value, or `none`; a required field a
  transition sets needs an `initial:` value
  (`data.lifecycle_initial_required`). Values in `set:` and `initial:` are
  fixed: literals, enum values, `none` and `clock.now`.
- **Delete.** `Todo.delete` runs the lifecycle's `delete` transition, which
  changes the record and keeps it. An entity with a lifecycle but no
  `delete` transition can't be deleted by code at all, and neither can a
  record whose removal would cascade to one with a lifecycle
  (`data.lifecycle_hard_delete`): only purge removes such records.
- **Transitions see visible records only.** A transition is an update, so
  it reaches a record only while `visible:` holds; a hidden one is the
  entity's not-found failure, and a restore needs a decision of its own.
  A `when:` that asks only what `visible:` asks already can't refuse a
  record that is there; any other `when:` can, and its callers map that
  refusal in `conflict:` (area 6).
- **Purge.** `after:` takes a duration written in place (`30d`) or a
  `Duration` setting (`config.soft_delete_retention`, area 10, chunk 5.1),
  whose value the app started with. A record is purged once `visible:` doesn't hold and its
  `since:` field is older than the run's time less `after:`; each batch
  deletes at most 500 records, oldest first, in its own transaction.
  References that `cascade` and those `cleared` do what they do for any
  delete; a record a `restricted` reference holds, directly or through
  any chain of cascades (one that comes back to its own entity, a folder
  inside a folder, followed as deep as the data goes, since chunk 3.10),
  stays until nothing holds it, and never stops the rest going.
  Purge is the compiler's own operation, outside every action and policy:
  the runtime's purge entry point runs it, and area 12's built-in hourly
  purge schedule will call it (chunk 6.2); until then nothing runs it on a
  timer. Each run reports how many records each batch deleted, which the
  schedule logs.

## Indexes

The compiler sees every read, so it creates every index its queries need,
multi-column and partial (for `visible:`) included; a page of a user's
visible todos, newest first, gets `(owner_id, created_at desc, id desc)`
where `deleted_at` is empty. An `indexes:` section declares lookups made from
outside the application; special kinds (full-text, JSON, geographic) take a
`kind:` option where the store supports it. Every index change appears in its
migration and in the build's schema report, beside the query that needed it.

```jadpo check project=indexes
entity Todo {
    id: Uuid
    due_at: Instant?

    indexes: {
        by_due_at: [due_at]  // for the reporting export; no query needs it
    }
}
```

Built in chunk 3.1: an index is a list of fields; a `kind:` option is not
built yet. Since chunk 3.2 the queries' indexes are derived: for each
query, and each list it includes, the fields it always tests with `==` or
`in`, then its order's fields, unless an index or unique key the table
already has serves them; a reference's own index goes once a query's index
starts with its column. Since chunk 3.4 the index a query of an entity with a
lifecycle needs is partial, holding only the records `visible:` lets queries
see (`WHERE "deleted_at" IS NULL`), unless `visible:` reads `clock.now`,
which changes as time passes; a reference's own index then stays, since
deleting the record it points at still finds hidden records.
Since chunk 3.10 a text field a query sorts by is indexed in code-point
order (`"title" COLLATE "C"`), the order a query sorts text in (area 6),
so the sorted read uses the index; its name ends `_sorted_idx`, apart from
an index on the same columns that tests them with `==`.

## Caches

A cache keeps a copy of data in a Redis store so busy pages read it quickly.
Nothing is cached unless you add it, on an entity (single records) or on a
query (its results):

```jadpo fragment
entity Product {
    ...
    caches: {
        records: {                     // single products, looked up by id
            store: cache               // a Redis store declared in app.jadpo
            keep: 1h                   // the oldest an entry can be
        }
    }
}

query category_page(category_id: Category.id, page: PageRequest) -> Page<ProductCard> {
    from: Product
    where: Product.category_id == category_id
    cache: {                           // this query's results
        store: cache
        keep: 10m
    }
}
```

- **Writes always go to the database**, never to the cache; code never names
  cache keys.
- **The running application keeps caches current.** The compiler generates
  the code that, after every committed change, clears each cached entry the
  change could affect. By default a change to a `Product` clears every cached
  result of every query that reads products: some extra database reads, but
  never a stale list. Clearing only the affected entries (for example the
  category pages a product moved out of and into) is a later optimisation.
- **Only pages read caches.** Reads inside actions, policy checks and
  authentication always read the database, so a decision is never made from
  a stale copy.
- **A cache that cannot be reached never fails a request**, anywhere in the
  language. Reads go to the database after a short timeout; a clear that
  fails is retried in the background, and `keep:` bounds how stale an entry
  can get meanwhile; readiness (area 7) does not depend on a cache.
- **Permissions are applied on every read**, and cached results are keyed by
  tenant and the details policy depends on, so one user's results are never
  served to another. Erasing personal data clears cached copies immediately.
- **Each entry is stored as one JSON value**, the same JSON the API sends, so
  every Jadpo type maps cleanly: money as `"12.50"`, large integers as
  strings, sets as arrays, enums tagged.

A record that lives in Redis rather than being copied there (a cart, a
sign-up in progress) is an entity in a Redis store instead ("Entities in
Redis", below). Which services can hold a cache (Redis-compatible ones, or
`Driver.memory` with a warning), and what a cache may hold and never holds,
are area 16's ("Caches: drivers and what they hold").

## Entities in Redis

Decided 2026-10-16 (Richard), answering question 15. An entity may live in
a Redis store, for data that is short-lived and harmless to lose. It must
expire: `expires_after:` gives how long a record lives, measured from its
last write, and expired records are hidden from every read and update as
records `visible:` hides are (area 6).

```jadpo fragment
// app.jadpo
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}

// domains/accounts/entities/sign_up_draft.jadpo
/// A sign-up someone has started and not yet finished.
entity SignUpDraft {
    id: Uuid
    email: Email {
        unique: true
    }
    step: SignUpStep
    attempts: Int

    store: drafts
    expires_after: 2h
    indexes: {
        by_step: [step]
    }
}
```

A Redis entity supports:

- **get, create, update and delete by `id`**, the generated operations every
  entity has, counters included: an update by `id` adding to a stored whole
  number (`SignUpDraft.attempts + 1`) is one Redis `increment`;
- **lookup by a `unique:` field**, for which Jadpo keeps one key per unique
  field;
- **listing and counting by equality on one field** declared in
  `indexes:`, for which Jadpo keeps a set per value; pages come in creation
  order;
- **concurrent-change checks** through a revision Jadpo keeps on each
  record, as every write checks (area 3).

Everything else is a compile error naming the reason and PostgreSQL as the
alternative: joins and `include`, ranges, ordering by any other field,
`matches`, an index over several fields, policy grants beyond equality
on the record's own fields (area 6, "Entities in Redis"), and a
`lifecycle:`, since expiry takes its place. A record
is stored as one JSON value, the same JSON the API sends, like a cache
entry. An `expires_after:` on an entity in a database, or a Redis entity
without one, is a compile error. These are built with the Redis driver,
after the first build.

## Storage types

Each column is the store engine's native type for the field's type; the
source never names one. Structured fields, recursive ones included, are
stored whole as `jsonb` in the API's JSON, which caches and Redis entities
hold too. The database enforces types, required fields, keys and references;
the application checks each field's own rules. Renaming an enum variant
changes stored values, so it needs a migration.

Built in chunk 3.10: each migration's snapshot records what every field
allows beyond its column type, an enum's variants and the field's bounds,
lengths and patterns, so changing them is drift `jadpo check` reports like
any other. A change that allows more (a variant added, a longer
`max_length`) is a migration with nothing to run. One that allows less (a
variant renamed or removed, a shorter `max_length`, a narrower range)
needs a decision: `backfill:` gives the records already stored with a
value the field no longer allows one it does, and the migration's SQL
replaces exactly those. A new `pattern:` isn't built yet, since
PostgreSQL's regular expressions aren't Jadpo's; `jadpo migrate new` says
so. A snapshot written before chunk 3.10 didn't record these, so the next
migration records them with nothing to decide.

```jadpo check project=narrowing file=app.jadpo hidden
application {}
```

```jadpo check project=narrowing file=domains/shipping/types/box_size.jadpo hidden
type BoxSize = Enum {
    small
    medium
    large
}
```

```jadpo check project=narrowing file=domains/shipping/entities/parcel.jadpo hidden migrate=initial
use shipping/types { BoxSize }

entity Parcel {
    id: Uuid
    size: BoxSize
}
```

```jadpo check project=narrowing file=domains/shipping/types/box_size.jadpo migrate=drop_large
type BoxSize = Enum {
    small
    medium
}
```

```jadpo check project=narrowing file=database/migrations/20261020T1401_drop_large/changes.jadpo migrate=finish
// changes.jadpo
/// Stops sending large boxes.
migration {
    Parcel.size: {
        backfill: BoxSize.medium  // parcels stored as large become medium
    }
}
```

The migration's SQL is `UPDATE "parcel" SET "size" = 'medium' WHERE
"size" NOT IN ('medium', 'small')`.

| Field type | PostgreSQL |
|---|---|
| `Uuid` and types based on it | `uuid` |
| `Int` | `smallint`, `integer` or `bigint`, from the declared range |
| `Decimal { scale: 2 }` | `numeric(p, 2)` |
| `Bool`, `Text`, `Bytes` | `boolean`, `text`, `bytea` |
| `Instant`, `CalendarDate` | `timestamptz(3)`, `date` |
| `Enum` without data | `text` holding the variant name |
| `Enum` with data, `Object`, `List`, `Set`, `Map` | `jsonb` |

## Migrations

Entities describe the schema you want; a migration records each step from
one version of the database to the next. `jadpo migrate new add_priority`
compares the entities with the previous migration's snapshot and writes the
next folder:

```text
database/migrations/20261020T1400_add_priority/
  snapshot.json        generated: the whole schema at this point
  postgres.sql         generated: the difference from the previous snapshot
  changes.jadpo        only when a decision is needed
```

When a change needs a decision, `jadpo migrate new` writes `changes.jadpo`
with a line to fill in for each one, `Todo.priority: {}`; `jadpo check`
names what is missing until it is written, and `jadpo migrate new` run
again finishes the migration, writing the decisions into its SQL.

```jadpo check project=migrations file=app.jadpo hidden
application {}
```

```jadpo check project=migrations file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
    legacy_flag: Bool
}
```

```jadpo check project=migrations file=domains/planning/types/priority.jadpo hidden
type Priority = Enum {
    low
    normal
    high
}
```

```jadpo check project=migrations file=domains/planning/entities/todo.jadpo hidden migrate=add_priority
use planning/types { Priority }

entity Todo {
    id: Uuid
    priority: Priority
}
```

```jadpo check project=migrations file=database/migrations/20261020T1401_add_priority/changes.jadpo migrate=finish
// changes.jadpo
/// Adds a priority to every todo and retires the legacy flag.
migration {
    Todo.priority: {
        backfill: Priority.normal  // existing rows need a value
    }
    Todo.legacy_flag: FieldChange.retired
}
```

- **Renames keep their data**: the identity registry (area 18) records what
  each renamed thing was born as, and tables and columns keep the names they
  were born with, so renaming a field keeps its column. `jadpo rename`,
  which writes the registry, comes with area 14's tooling; until then a
  field renamed by hand reads as one removed and one added, and the removed
  one's decision says so.
- **Decisions only where needed**: a value for existing rows of a required
  field, or retiring a field. Removing a field takes two migrations,
  `FieldChange.retired` (code stops using it, the column stays, now
  nullable, for servers still running the old build) and later
  `FieldChange.dropped`, which needs explicit acceptance (area 18; not built
  yet). Removing an entity, moving one between stores and changing a
  column's type other than widening it aren't built yet: `jadpo migrate
  new` says so.
- **Backfills can be worked out** from each row's own fields (Richard,
  2026-10-15), for a new field that reshapes data the entity already holds.
  See "Computed backfills" below; not built yet.
- **Forgetting is caught at once**: `jadpo check` fails while the entities
  and the latest snapshot disagree, naming what changed and the command to
  run, so the editor, the build and CI all stop. Loose files, with no
  `app.jadpo`, have no migrations, so the check is a project's.
- **`jadpo migrate apply`** runs pending migrations as a deploy step, each in
  a transaction, one per store, and records each in the store's
  `jadpo_migrations` table; it refuses a database that applied a migration
  the project doesn't have or one that changed since. `jadpo migrate status`
  lists the migrations and which a database has applied. Until
  configuration gives each connection its URL (area 10), both take it with
  `--url`. The application's refusal to start against a database that
  does not match comes with the compiled deployables (Phase 8).

### Computed backfills

A backfill can be an expression over each row's own stored fields, so a
migration can reshape data rather than only fill it with one value
(Richard, 2026-10-15):

```jadpo fragment
// domains/planning/libraries/priorities.jadpo
library priorities {
    function from_flag(legacy_flag: Bool) -> Priority {
        if (legacy_flag) {
            return Priority.high
        }
        return Priority.normal
    }
}

// database/migrations/20261101T0900_priority_from_flag/changes.jadpo
/// Replaces the legacy flag with a priority worked out from it.
migration {
    Todo.priority: {
        backfill: priorities.from_flag(Todo.legacy_flag)
    }
    Todo.legacy_flag: FieldChange.retired
}
```

- `Todo.legacy_flag` means each row's stored value, as on the right of an
  update's `set:` (area 6). The expression may use the entity's fields as
  they stand before this migration, including one this migration retires,
  constants and pure functions (`priorities` is a library, area 1); never a
  query, another entity or `clock.now`. It is typed against the field it
  fills, like any value.
- The migration adds the column as optional and fills existing rows in
  batches, each in its own transaction, so a large table is never locked
  for the whole backfill. A last short transaction fills rows that servers
  still on the old build wrote meanwhile, and makes the column required.
- When every operation in the expression has a SQL form, each batch is one
  `UPDATE`; otherwise `jadpo migrate apply` reads each batch, works the
  values out in the runtime and writes them back. The generated SQL says
  which.
- Freeform SQL in migrations stays out: a backfill is checked Jadpo, so a
  migration can't write a value its field's rules forbid.

Built in chunk 5.6.

A migration's `postgres.sql` holds a section per store, each opened by a
`-- store: <name>` line, and names every table, column, constraint and index
as the snapshot does: a table after its entity's born name in
`lower_snake_case` (`Todo` is `todo`), a column after its field, keys and
indexes `<table>_<name>_key`, `_fkey` and `_idx`, cut with a digest past
PostgreSQL's 63 bytes. Each store that holds an entity also gets the
runtime's system tables, prefixed `jadpo_` (below).

## System tables

Tables the runtime needs are generated, prefixed `jadpo_`, and part of the
migration snapshots like any entity, so drift in them fails `jadpo check`
too. The ones that must commit with an action's own writes go in every
store that holds an entity, since any of them can be written:

| Table | Holds | Behaviour |
|---|---|---|
| `jadpo_outbox` | Each event an action emitted, with its trace context | Area 12, chunk 6.1 |
| `jadpo_deliveries` | Each event's delivery to each subscriber: status, attempts, next attempt, last failure | Area 12, chunk 6.1 |
| `jadpo_idempotency_keys` | Each idempotent request's key, digest and stored response | Area 7, built in chunk 4.2 |

Browser sessions and API tokens (`jadpo_sessions`: each one's user, the
verifiers of its token and a browser session's CSRF token, and when it
started and ends; area 8, built in chunks 4.3 and 4.3b) go only in the
store the principal's user entity lives in, and service keys
(`jadpo_service_keys`: each key's service, its verifier, and when it was
issued and expires; chunk 4.3b) only in the store its service entity lives
in, since authentication reads a credential with its caller's record in
one statement.
Schedule state and the stores system features name explicitly
(`schedules: { store: main }`) join them with their chunks. The runtime
state decided on 2026-10-16 (multi-step call records and batch runs beside
the outbox, OAuth grants, poll state, and webhook and queue
de-duplication) follows the same rules and is always in a database, never
a cache (area 16). The manifest
holds a `system_table` node for each, and `jadpo_migrations`, which
`jadpo migrate apply` keeps, is the runtime's own and in no snapshot.

## Seeds

A seed in `database/seeds/` creates development data by calling the
application's own actions, so validation, lifecycle and policy apply.
`jadpo seed <name>` refuses a production database; data production needs is
a migration backfill.

```jadpo fragment
// database/seeds/local_demo.jadpo
/// A user with a few todos, for local development.
seed {
    var alice: User = attempt planning.register_user("alice@example.com")
    attempt planning.create_todo(alice.id, "Buy milk")
    attempt planning.create_todo(alice.id, "Book dentist")
}
```

Seeds are not built yet.

## Where things live

An entity's file holds only its data definition (store, fields, unique
rules, indexes, lifecycle, policy, record cache) and gets generated
operations: `Todo.get`, create, update and delete. Every query you write is
in the domain's `queries/` folder, every action in `actions/` and every
failure in `failures/`, one per file. Written actions are called by name,
`complete_todo(todo_id)`, or `planning.complete_todo(todo_id)` from another
domain when public. There is no `self`: `todo: Todo` is a snapshot,
`todo_id: Todo.id` a reference.

```jadpo fragment
// domains/planning/failures/todo_not_found.jadpo
type TodoNotFound = Failure {
    kind: FailureKind.not_found
    message: "Todo not found."
}

// domains/planning/actions/complete_todo.jadpo
action complete_todo(todo_id: Todo.id) fails TodoNotFound -> Todo {
    var todo: Todo = attempt Todo.get(todo_id)      // generated operation
    ...
}
```

## Errors

Every error this area can produce, with an example that triggers it and the
new compiler's actual output; the errors of reading and writing records,
including a write to a field a lifecycle owns, are area 6's. An example's hidden
files and the migrations its `jadpo migrate new` steps write are in the
page's source.

### `data.identity_missing`

```jadpo check project=e-identity-missing expect=data.identity_missing
entity Note {
    body: Text
}
```

```text output
error: `Note` has no `id` field
 --> main.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ no `id`
  reason: Every entity's identity is one field, `id`, based on `Uuid`: generated when a record is created and never supplied by callers, so other code can refer to it as `Entity.id`.
  next: Add `id: Uuid` as the entity's first field (agent)
  rule: data.identity_missing
  help: docs/language/05-data.html#data-identity-missing

check failed: 1 error, 1 file
```

### `data.identity_type`

```jadpo check project=e-identity-type expect=data.identity_type
entity Note {
    id: Text
}
```

```text output
error: `Note.id` must be a required `Uuid`
 --> main.jadpo:2:5
  |
2 |     id: Text
  |     ^^ not a `Uuid`
  reason: An entity's `id` is a `Uuid`, or a named type built on one; links between entities are references to it.
  next: Write `id: Uuid` (agent)
  rule: data.identity_type
  help: docs/language/05-data.html#data-identity-type

check failed: 1 error, 1 file
```

### `data.entity_in_field`

```jadpo check project=e-entity-in-field expect=data.entity_in_field
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    keeper: User
}
```

```text output
error: `Todo.keeper` holds a whole `User`
 --> main.jadpo:7:13
  |
7 |     keeper: User
  |             ^^^^ a `User` record
  reason: An entity value is a snapshot, never stored inside another record; a field refers to a record by its identity, `User.id`, which the database keeps as a foreign key.
  next: Store `User.id` instead (agent)
  rule: data.entity_in_field
  help: docs/language/05-data.html#data-entity-in-field

check failed: 1 error, 1 file
```

### `data.generated_type`

```jadpo check project=e-generated-type expect=data.generated_type
entity Note {
    id: Uuid
    created_on: CalendarDate {
        generated: Generated.on_create
    }
}
```

```text output
error: `Note.created_on` can't be generated
 --> main.jadpo:4:9
  |
4 |         generated: Generated.on_create
  |         ^^^^^^^^^ not an `Instant`
  reason: A generated field is a required `Instant` set from the operation's clock, on create or on every change; code never assigns it.
  next: Type `created_on` as `Instant`, or remove `generated:` (agent)
  rule: data.generated_type
  help: docs/language/05-data.html#data-generated-type

check failed: 1 error, 1 file
```

### `data.on_delete_required`

```jadpo check project=e-on-delete-required expect=data.on_delete_required
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    owner_id: User.id
}
```

```text output
error: `Todo.owner_id` doesn't say what deleting its `User` does
 --> main.jadpo:7:5
  |
7 |     owner_id: User.id
  |     ^^^^^^^^ refers to a `User`
  reason: Any field typed `User.id` is a reference with a foreign key, and its block must say what happens when the `User` is deleted: `OnDelete.restricted` refuses the delete, `OnDelete.cascade` deletes this record too, and `OnDelete.cleared` sets an optional reference to `none`. There is no default.
  next: Add `on_delete:` to `owner_id`'s block (human)
  rule: data.on_delete_required
  help: docs/language/05-data.html#data-on-delete-required

check failed: 1 error, 1 file
```

### `data.on_delete_not_reference`

```jadpo check project=e-on-delete-not-reference expect=data.on_delete_not_reference
entity Note {
    id: Uuid
    title: Text {
        on_delete: OnDelete.cascade
    }
}
```

```text output
error: `Note.title` isn't a reference
 --> main.jadpo:4:20
  |
4 |         on_delete: OnDelete.cascade
  |                    ^^^^^^^^^^^^^^^^ nothing to delete
  reason: `on_delete:` says what deleting a referenced record does, so it belongs only on a field typed `X.id`.
  next: Remove `on_delete:` (agent)
  rule: data.on_delete_not_reference
  help: docs/language/05-data.html#data-on-delete-not-reference

check failed: 1 error, 1 file
```

### `data.cleared_required_reference`

```jadpo check project=e-cleared-required expect=data.cleared_required_reference
entity Project {
    id: Uuid
}

entity Todo {
    id: Uuid
    project_id: Project.id {
        on_delete: OnDelete.cleared
    }
}
```

```text output
error: `Todo.project_id` can't be cleared
 --> main.jadpo:8:20
  |
8 |         on_delete: OnDelete.cleared
  |                    ^^^^^^^^^^^^^^^^ the field can't be `none`
  reason: `OnDelete.cleared` sets the reference to `none` when the `Project` is deleted, which only an optional reference (`Project.id?`) can hold.
  next: Make it `Project.id?`, or use `OnDelete.restricted` or `OnDelete.cascade` (agent)
  rule: data.cleared_required_reference
  help: docs/language/05-data.html#data-cleared-required-reference

check failed: 1 error, 1 file
```

### `data.reference_cycle`

Neither record could be created first, whatever happens on delete:

```jadpo check project=e-reference-cycle expect=data.reference_cycle
entity Team {
    id: Uuid
    lead_id: Member.id {
        on_delete: OnDelete.restricted
    }
}

entity Member {
    id: Uuid
    team_id: Team.id {
        on_delete: OnDelete.restricted
    }
}
```

```text output
error: Required references form a cycle through `Member`
 --> main.jadpo:10:5
   |
 3 |     lead_id: Member.id {
   |     ------- and this one
10 |     team_id: Team.id {
   |     ^^^^^^^ required reference
  reason: `Member.team_id` and `Team.lead_id` are both required, so no record could be created first: each needs one that already exists.
  next: Make one of these references optional (`X.id?`) (human)
  rule: data.reference_cycle
  help: docs/language/05-data.html#data-reference-cycle

check failed: 1 error, 1 file
```

### `data.reference_across_stores`

```jadpo check project=e-across-stores file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-across-stores file=domains/planning/entities/user.jadpo hidden
entity User {
    id: Uuid
}
```

```jadpo check project=e-across-stores file=domains/planning/entities/old_todo.jadpo expect=data.reference_across_stores
use planning/entities { User }

entity OldTodo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    store: archive
}
```

```text output
error: `OldTodo.owner_id` refers to a `User` in another store
 --> domains/planning/entities/old_todo.jadpo:5:5
  |
5 |     owner_id: User.id {
  |     ^^^^^^^^ `OldTodo` is in `archive`
  reason: A reference is a foreign key, which a database keeps only between its own tables; an action writes one store, and work across stores goes through an event and a subscriber.
  next: Keep `OldTodo` and `User` in one store, or store the id as a plain `Uuid` (human)
  rule: data.reference_across_stores
  help: docs/language/05-data.html#data-reference-across-stores

check failed: 1 error, 3 files
```

### `data.unknown_field`

```jadpo check project=e-unknown-field expect=data.unknown_field
entity Project {
    id: Uuid
    name: Text

    unique: {
        name_per_owner: [owner_id, name]
    }
}
```

```text output
error: `Project` has no field `owner_id`
 --> main.jadpo:6:26
  |
6 |         name_per_owner: [owner_id, name]
  |                          ^^^^^^^^ no such field
  reason: The unique rule `name_per_owner` names fields of `Project`, its own.
  next: Remove `owner_id` (agent)
  rule: data.unknown_field
  help: docs/language/05-data.html#data-unknown-field

check failed: 1 error, 1 file
```

### `data.repeated_field`

```jadpo check project=e-repeated-field expect=data.repeated_field
entity Todo {
    id: Uuid
    due_at: Instant

    indexes: {
        by_due_at: [due_at, due_at]
    }
}
```

```text output
error: The index `by_due_at` lists `due_at` twice
 --> main.jadpo:6:29
  |
6 |         by_due_at: [due_at, due_at]
  |                             ^^^^^^ listed again
  reason: Each field counts once in an index.
  next: Remove the second `due_at` (agent)
  rule: data.repeated_field
  help: docs/language/05-data.html#data-repeated-field

check failed: 1 error, 1 file
```

### `data.field_list_required`

```jadpo check project=e-field-list expect=data.field_list_required
entity Project {
    id: Uuid
    keeper: Text
    name: Text

    unique: {
        name_per_keeper: keeper
    }
}
```

```text output
error: The unique rule `name_per_keeper` isn't a list of fields
 --> main.jadpo:7:26
  |
7 |         name_per_keeper: keeper
  |                          ^^^^^^ not a list
  reason: Each unique rule is a list of the entity's fields in brackets: `name_per_keeper: [field, other]`.
  next: Write `name_per_keeper: [field, ...]` (agent)
  rule: data.field_list_required
  help: docs/language/05-data.html#data-field-list-required

check failed: 1 error, 1 file
```

### `data.unique_nullable_field`

```jadpo check project=e-unique-nullable expect=data.unique_nullable_field
entity Account {
    id: Uuid
    tenant: Text
    handle: Text?

    unique: {
        tenant_handle: [tenant, handle]
    }
}
```

```text output
error: `Account.handle` can be `none`
 --> main.jadpo:7:33
  |
7 |         tenant_handle: [tenant, handle]
  |                                 ^^^^^^ optional field
  reason: The fields of a unique rule over several fields must be required, so every engine agrees on what a duplicate is.
  next: Make `handle` required, or leave it out of `tenant_handle` (agent)
  rule: data.unique_nullable_field
  help: docs/language/05-data.html#data-unique-nullable-field

check failed: 1 error, 1 file
```

### `data.unique_single_field`

```jadpo check project=e-unique-single expect=data.unique_single_field
entity Account {
    id: Uuid
    handle: Text

    unique: {
        only_handle: [handle]
    }
}
```

```text output
error: The unique rule `only_handle` has one field
 --> main.jadpo:6:9
  |
6 |         only_handle: [handle]
  |         ^^^^^^^^^^^ one field
  reason: The `unique:` section holds rules over several fields; a single field is made unique in its own block, `handle: ... { unique: true }`.
  next: Move it to `handle`'s block as `unique: true` (agent)
  rule: data.unique_single_field
  help: docs/language/05-data.html#data-unique-single-field

check failed: 1 error, 1 file
```

### `data.duplicate_rule`

The same two fields in another order are the same rule:

```jadpo check project=e-duplicate-rule expect=data.duplicate_rule
entity Account {
    id: Uuid
    tenant: Text
    handle: Text

    unique: {
        tenant_handle: [tenant, handle]
        handle_tenant: [handle, tenant]
    }
}
```

```text output
error: The unique rule `handle_tenant` repeats `tenant_handle`
 --> main.jadpo:8:9
  |
7 |         tenant_handle: [tenant, handle]
  |         ------------- first here
8 |         handle_tenant: [handle, tenant]
  |         ^^^^^^^^^^^^^ same fields
  reason: Both cover the same fields, in whatever order, so they are one rule; a write's conflict names one of them (area 6).
  next: Remove `handle_tenant` (agent)
  rule: data.duplicate_rule
  help: docs/language/05-data.html#data-duplicate-rule

check failed: 1 error, 1 file
```

### `data.store_unknown`

```jadpo check project=e-store-unknown file=app.jadpo hidden
application {}
```

```jadpo check project=e-store-unknown file=domains/notes/entities/note.jadpo expect=data.store_unknown
entity Note {
    id: Uuid

    store: archive
}
```

```text output
error: `archive` isn't a database `Note` can live in
 --> domains/notes/entities/note.jadpo:4:12
  |
4 |     store: archive
  |            ^^^^^^^ not a database
  reason: `app.jadpo` declares no connection called `archive`; an entity's `store:` names a connection with `driver: Driver.postgres`, or `main` when `app.jadpo` declares no database.
  next: Name a database connection, or declare one in `app.jadpo` (agent)
  rule: data.store_unknown
  help: docs/language/05-data.html#data-store-unknown

check failed: 1 error, 2 files
```

### `data.store_required`

`app.jadpo` declares a database, so `main` is no longer implied:

```jadpo check project=e-store-required file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
        }
    }
}
```

```jadpo check project=e-store-required file=domains/notes/entities/note.jadpo expect=data.store_required
entity Note {
    id: Uuid
}
```

```text output
error: `Note` names no store, and there is no `main`
 --> domains/notes/entities/note.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ which store?
  reason: An entity lives in `main` unless its file says `store: <connection>`, and `app.jadpo` declares no database called `main`; its databases are `archive`.
  next: Add `store: archive` to `Note` (human)
  rule: data.store_required
  help: docs/language/05-data.html#data-store-required

check failed: 1 error, 2 files
```

### `data.unknown_section`

An entity has no `identity:` item:

```jadpo check project=e-unknown-section expect=data.unknown_section
entity Note {
    id: Uuid

    identity: id
}
```

```text output
error: `identity` isn't something an entity holds
 --> main.jadpo:4:5
  |
4 |     identity: id
  |     ^^^^^^^^ not a field or a section
  reason: An entity's file holds only its data definition: fields (`name: Type`), and the sections `store:`, `unique:`, `indexes:`, `lifecycle:`, `caches:`, `policy:` and `membership:`. Its identity is always the field `id`; actions, queries and failures go in the domain's own folders.
  next: Remove it from `Note`, or move it where it belongs (agent)
  rule: data.unknown_section
  help: docs/language/05-data.html#data-unknown-section

check failed: 1 error, 1 file
```

### `data.reserved_table_name`

```jadpo check project=e-reserved-table expect=data.reserved_table_name
entity JadpoAudit {
    id: Uuid
}
```

```text output
error: `JadpoAudit`'s table would be called `jadpo_audit`
 --> main.jadpo:1:8
  |
1 | entity JadpoAudit {
  |        ^^^^^^^^^^ a system table's prefix
  reason: Tables whose names start with `jadpo_` are the runtime's own (the outbox, event deliveries, idempotency keys), so no entity's table may take that prefix.
  next: Rename `JadpoAudit` (agent)
  rule: data.reserved_table_name
  help: docs/language/05-data.html#data-reserved-table-name

check failed: 1 error, 1 file
```

### `data.name_too_long`

```jadpo check project=e-name-too-long expect=data.name_too_long
entity Note {
    id: Uuid
    the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest: Instant?
}
```

```text output
error: `Note.the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest` is too long a name for a column
 --> main.jadpo:3:5
  |
3 |     the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest: Instant?
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 67 characters
  reason: Its column would be called `the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest`, and PostgreSQL keeps names of at most 63 bytes, so a longer one would be cut and could clash with another.
  next: Choose a shorter name (agent)
  rule: data.name_too_long
  help: docs/language/05-data.html#data-name-too-long

check failed: 1 error, 1 file
```

### `data.lifecycle_section`

A lifecycle holds `visible:`, `initial:`, `transitions:` and `purge:`, each once; a transition holds `when:` and `set:`, and `purge:` holds `after:` and `since:`:

```jadpo check project=e-lifecycle-section expect=data.lifecycle_section
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        hidden: deleted_at != none
    }
}
```

```text output
error: `hidden` isn't part of a lifecycle
 --> main.jadpo:7:9
  |
7 |         hidden: deleted_at != none
  |         ^^^^^^ not a section here
  reason: A lifecycle holds `visible:`, `initial:`, `transitions:` and `purge:`.
  next: Remove `hidden` from the lifecycle (agent)
  rule: data.lifecycle_section
  help: docs/language/05-data.html#data-lifecycle-section

check failed: 1 error, 1 file
```

### `data.lifecycle_condition`

A lifecycle condition tests the record's own fields against fixed values, never another record or who is asking:

```jadpo check project=e-lifecycle-condition expect=data.lifecycle_condition
entity Todo {
    id: Uuid
    owner_id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none and owner_id == current_user.id
    }
}
```

```text output
error: `current_user.id` isn't something a lifecycle condition can compare with
 --> main.jadpo:7:53
  |
7 |         visible: deleted_at == none and owner_id == current_user.id
  |                                                     ^^^^^^^^^^^^^^^ from outside the record
  reason: A lifecycle condition compares the record's own fields with fixed values: a literal, an enum value or `clock.now`. It never mentions other records or who is asking, which is policy (area 9), so it can join every statement's `WHERE` and an index.
  next: Compare with a literal, an enum value or `clock.now` (agent)
  rule: data.lifecycle_condition
  help: docs/language/05-data.html#data-lifecycle-condition

check failed: 1 error, 1 file
```

### `data.lifecycle_unknown_field`

Every field a lifecycle names is one of the record's own:

```jadpo check project=e-lifecycle-unknown-field expect=data.lifecycle_unknown_field
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deletd_at == none
    }
}
```

```text output
error: `Todo` has no field `deletd_at`
 --> main.jadpo:6:18
  |
6 |         visible: deletd_at == none
  |                  ^^^^^^^^^ not declared
  reason: A lifecycle names the fields of the record it is written in, and `Todo`'s fields are `id` and `deleted_at`. Did you mean `deleted_at`?
  next: Use `deleted_at` instead of `deletd_at` (agent)
  rule: data.lifecycle_unknown_field
  help: docs/language/05-data.html#data-lifecycle-unknown-field

check failed: 1 error, 1 file
```

### `data.lifecycle_value`

A transition sets each field to a fixed value, the same every time it runs:

```jadpo check project=e-lifecycle-value expect=data.lifecycle_value
entity Todo {
    id: Uuid
    deleted_at: Instant?
    edited_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                set: {
                    deleted_at: edited_at
                }
            }
        }
    }
}
```

```text output
error: `edited_at` is another field of the record
 --> main.jadpo:11:33
   |
11 |                     deleted_at: edited_at
   |                                 ^^^^^^^^^ a field, not a value
  reason: A transition sets each field to a fixed value (a literal, an enum value, `none` or `clock.now`), the same every time it runs; copying one field into another isn't one.
  next: Use a literal, an enum value, `none` or `clock.now` (agent)
  rule: data.lifecycle_value
  help: docs/language/05-data.html#data-lifecycle-value

check failed: 1 error, 1 file
```

### `data.lifecycle_initial_required`

A required field only transitions change gets its first value from `initial:`:

```jadpo check project=e-lifecycle-initial-required expect=data.lifecycle_initial_required
type OrderStage = Enum {
    placed
    shipped
}

entity Order {
    id: Uuid
    stage: OrderStage

    lifecycle: {
        transitions: {
            ship: {
                when: stage == OrderStage.placed
                set: {
                    stage: OrderStage.shipped
                }
            }
        }
    }
}
```

```text output
error: `Order.stage` has no starting value
 --> main.jadpo:8:5
   |
 8 |     stage: OrderStage
   |     ^^^^^ required, and only transitions set it
12 |             ship: {
   |             ---- set here
  reason: Only the lifecycle's transitions change `stage`, so a create can't give it; a required field gets its first value from `initial:`, and an optional one starts as `none`.
  next: Add `stage: <value>` to the lifecycle's `initial:` (agent)
  rule: data.lifecycle_initial_required
  help: docs/language/05-data.html#data-lifecycle-initial-required

check failed: 1 error, 1 file
```

### `data.lifecycle_transition_name`

A transition is an operation of its entity, so it can't take another operation's name or a field's:

```jadpo check project=e-lifecycle-transition-name expect=data.lifecycle_transition_name
entity Todo {
    id: Uuid
    archived_at: Instant?

    lifecycle: {
        transitions: {
            update: {
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}
```

```text output
error: A transition can't be called `update`
 --> main.jadpo:7:13
  |
7 |             update: {
  |             ^^^^^^ name in use
  reason: `Todo.update` is already an operation every entity has, so a transition can't take its name; `delete` is the one a transition may take over.
  next: Name the transition after the change it makes, such as `archive` (agent)
  rule: data.lifecycle_transition_name
  help: docs/language/05-data.html#data-lifecycle-transition-name

check failed: 1 error, 1 file
```

### `data.lifecycle_purge_visible`

Purge deletes records once they have been hidden for a while, so the lifecycle says which records are hidden:

```jadpo check project=e-lifecycle-purge-visible expect=data.lifecycle_purge_visible
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        transitions: {
            delete: {
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

```text output
error: `Todo`'s lifecycle purges records but hides none
 --> main.jadpo:13:9
   |
13 |         purge: {
   |         ^^^^^ no `visible:` above
  reason: Purge permanently deletes records once they have been hidden for a while, so the lifecycle must say which records are hidden, with `visible:`.
  next: Add `visible: <condition>` to the lifecycle (agent)
  rule: data.lifecycle_purge_visible
  help: docs/language/05-data.html#data-lifecycle-purge-visible

check failed: 1 error, 1 file
```

### `data.lifecycle_purge_after`

A hidden record is kept for a positive duration before purge deletes it:

```jadpo check project=e-lifecycle-purge-after expect=data.lifecycle_purge_after
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 0d
            since: deleted_at
        }
    }
}
```

```text output
error: `after: 0d` isn't a positive duration
 --> main.jadpo:8:20
  |
8 |             after: 0d
  |                    ^^ not a positive duration
  reason: `after:` is how long a hidden record is kept before purge deletes it, a duration such as `30d` or `12h`, longer than nothing; a hidden record would be deleted the moment it is hidden.
  next: Write a duration, such as `after: 30d` (agent)
  rule: data.lifecycle_purge_after
  help: docs/language/05-data.html#data-lifecycle-purge-after

check failed: 1 error, 1 file
```

### `data.lifecycle_purge_since`

`since:` names the time field purge counts from:

```jadpo check project=e-lifecycle-purge-since expect=data.lifecycle_purge_since
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 30d
            since: title
        }
    }
}
```

```text output
error: `title` isn't a time
 --> main.jadpo:14:20
   |
14 |             since: title
   |                    ^^^^^ a `TodoTitle`
  reason: `since:` names the `Instant` field purge counts from: a record goes once it has been hidden for `after:` since that time.
  next: Name an `Instant` field of the record (agent)
  rule: data.lifecycle_purge_since
  help: docs/language/05-data.html#data-lifecycle-purge-since

check failed: 1 error, 1 file
```

### `migration.out_of_date`

A project with an entity and no migration yet:

```jadpo check project=e-out-of-date file=app.jadpo hidden
application {}
```

```jadpo check project=e-out-of-date file=domains/notes/entities/note.jadpo expect=migration.out_of_date
entity Note {
    id: Uuid
    body: Text
}
```

```text output
error: No migration creates these entities' tables yet
 --> domains/notes/entities/note.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ no table yet
  reason: `jadpo check` compares the database schema the entities ask for with the migrations in `database/migrations/`, and there are none: new entity `Note`, new system table for deliveries, new system table for idempotency keys and new system table for outbox.
  next: Run `jadpo migrate new initial` (agent)
  rule: migration.out_of_date
  help: docs/language/05-data.html#migration-out-of-date

check failed: 1 error, 2 files
```

### `migration.record_invalid`

A snapshot edited by hand:

```jadpo check project=e-record-invalid file=app.jadpo hidden
application {}
```

```jadpo check project=e-record-invalid file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-record-invalid file=database/migrations/20261020T1400_initial/snapshot.json expect=migration.record_invalid
{"format": 1}
```

```text output
error: `database/migrations/20261020T1400_initial/snapshot.json` can't be read
 --> database/migrations/20261020T1400_initial/snapshot.json
  reason: A migration's records are written by `jadpo migrate new` and never edited by hand; this one can't be used: it has no `stores` object.
  next: Restore it from version control (human)
  rule: migration.record_invalid
  help: docs/language/05-data.html#migration-record-invalid

check failed: 1 error, 2 files
```

### `migration.decision_required`

`stars` is new and required; `jadpo migrate new add_stars` wrote the migration and left the decision to fill in:

```jadpo check project=e-decision-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-required file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-required file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-required file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-required file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_required
migration {
    Note.stars: {}
}
```

```text output
error: Migration `20261020T1401_add_stars` needs a decision about `Note.stars`
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:2:5
  |
2 |     Note.stars: {}
  |     ^^^^^^^^^^ decide here
  reason: `Note.stars` is required, and the `Note` records already stored have no value for it.
  next: Set `backfill:` for `Note.stars`, such as `backfill: 1`, then run `jadpo migrate new` to finish the migration (human)
  rule: migration.decision_required
  help: docs/language/05-data.html#migration-decision-required

check failed: 1 error, 4 files
```

### `migration.decision_invalid`

```jadpo check project=e-decision-invalid file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-invalid file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-invalid file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-invalid file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-invalid file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_invalid
migration {
    Note.stars: {
        backfill: 9
    }
}
```

```text output
error: The decision about `Note.stars` doesn't fit its change
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:3:19
  |
3 |         backfill: 9
  |                   ^ doesn't fit
  reason: This value can't fill `Note.stars`: it is above the field's `max`.
  next: Write `Note.stars: { backfill: <value> }` with a value of its type (agent)
  rule: migration.decision_invalid
  help: docs/language/05-data.html#migration-decision-invalid

check failed: 1 error, 4 files
```

### `migration.decision_unknown`

```jadpo check project=e-decision-unknown file=app.jadpo hidden
application {}
```

```jadpo check project=e-decision-unknown file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-decision-unknown file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-decision-unknown file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-decision-unknown file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.decision_unknown
migration {
    Note.stars: {
        backfill: 3
    }
    Note.body: FieldChange.retired
}
```

```text output
error: Nothing about `Note.body` needs a decision
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:5:5
  |
5 |     Note.body: FieldChange.retired
  |     ^^^^^^^^^ no such change
  reason: A migration's `changes.jadpo` holds only the decisions its own changes need, and nothing `20261020T1401_add_stars` changes about `Note.body` does: a decision is a value for existing rows of a required field or for rows a field no longer allows, or retiring a field that is gone.
  next: Remove `Note.body` (agent)
  rule: migration.decision_unknown
  help: docs/language/05-data.html#migration-decision-unknown

check failed: 1 error, 4 files
```

### `migration.unfinished`

The decision is written, but the SQL was written before it:

```jadpo check project=e-unfinished file=app.jadpo hidden
application {}
```

```jadpo check project=e-unfinished file=domains/notes/entities/note.jadpo hidden migrate=initial
entity Note {
    id: Uuid
}
```

```jadpo check project=e-unfinished file=domains/notes/types/stars.jadpo hidden
type Stars = Int {
    min: 1
    max: 5
}
```

```jadpo check project=e-unfinished file=domains/notes/entities/note.jadpo hidden migrate=add_stars
use notes/types { Stars }

entity Note {
    id: Uuid
    stars: Stars
}
```

```jadpo check project=e-unfinished file=database/migrations/20261020T1401_add_stars/changes.jadpo expect=migration.unfinished
migration {
    Note.stars: {
        backfill: 3
    }
}
```

```text output
error: Migration `20261020T1401_add_stars` doesn't hold its decisions yet
 --> database/migrations/20261020T1401_add_stars/changes.jadpo:1:1
  |
1 | migration {
  | ^^^^^^^^^ decided
  reason: Its decisions are all written in `changes.jadpo`, but `database/migrations/20261020T1401_add_stars/postgres.sql` was written before them, so it doesn't carry them out.
  next: Run `jadpo migrate new` to finish it (agent)
  rule: migration.unfinished
  help: docs/language/05-data.html#migration-unfinished

check failed: 1 error, 4 files
```

## Review

### Status

Built in chunk 3.1 (run, with the examples above and `crates/jadpo-check`,
`jadpo-graph` and `jadpo-cli`'s tests):

- **Entities** are checked as decided: the `id` (a required `Uuid`), each
  field's column type, `generated:`, `unique: true`, references with a
  required `on_delete:`, cycles of required references, compound `unique:`
  rules, `indexes:`, `store:` against `app.jadpo`'s connections (with `main`
  on PostgreSQL implied when none is a database), and the names the
  database keeps. The manifest holds a node for each entity's placement,
  unique rule, index and reference, with `references` edges between
  entities, and one for each system table and migration snapshot.
- **Migrations**: `jadpo migrate new` writes the snapshot, the SQL and the
  decisions to make, and finishes the migration once they are written;
  `jadpo check` fails on drift and on a migration still waiting for its
  decisions; `jadpo migrate apply` and `status` run against the database
  `--url` names. Adding entities, fields, unique rules, indexes and
  references, backfilling required fields, retiring removed fields and
  widening whole-number and decimal columns are migrated.
- **The test database**: `jadpo test` gives each action test its own copy
  of a template built from the migrations, on the server
  `JADPO_TEST_DATABASE_URL` names or a disposable one, with copies
  prepared in the background; `--keep-failed` keeps a failed test's.

Built in chunk 3.4: **lifecycles**, as the Lifecycle section says. Each
lifecycle and transition is a node in the manifest (`lifecycle Todo`,
`transition Todo.delete`, with its `when:`, whether it can refuse a record
and the fields it sets); `visible:` joins every query, `get`, update,
delete and transition at one place in the plan; queries' indexes are
partial; and purge is one statement per batch, run by the runtime's purge
entry point, tested against live PostgreSQL with cascades, restricted
references and batches (`crates/jadpo-cli/tests/lifecycle.rs`).

Not built yet, and said so where it is used (`toolchain.not_built_yet`):

- Removing an entity, moving one between stores, dropping a retired column
  (it needs acceptances, area 18) and other column type changes, in
  `jadpo migrate new`.
- Entities in Redis (`expires_after:`, decided 2026-10-16, question 15),
  record caches (`caches:`), and indexes with a `kind:` option. A database
  implied only when something needs one (question 14, revised 2026-10-16)
  comes with the first feature that can run without one.
- Seeds and `jadpo seed` are not built. An entity's `policy:` and
  `membership:` are area 9's, built in chunk 4.4.
- A lifecycle's `purge: { after: config.<setting> }` waits for configuration
  (chunk 5.1), and the hourly purge schedule for schedules (chunk 6.2).
- `jadpo rename`, which writes the identity registry (area 18), comes with
  area 14's tooling; until then a field renamed by hand reads as one
  removed and one added.
- Each connection's URL comes from configuration (area 10); until then
  `jadpo migrate` takes it with `--url`, and the application's refusal to
  start against a database that doesn't match comes with the compiled
  deployables (Phase 8).
- Structured fields (an enum with data, an object, a list, a set or a
  map) on an entity: their `jsonb` holds the API's JSON, which comes with
  area 7, so until then the field is refused where it is declared rather
  than every read and write of it failing as it runs (chunk 3.10):

```jadpo check project=e-structured-field expect=toolchain.not_built_yet
type Tag = Text {
    max_length: 20
}

entity Parcel {
    id: Uuid
    tags: List<Tag>
}
```

```text output
error: Storing a `List<Tag>` field isn't built yet
 --> main.jadpo:7:11
  |
7 |     tags: List<Tag>
  |           ^^^^^^^^^ structured, stored as `jsonb`
  reason: Area 5 stores an enum with data, an object, a list, a set or a map as `jsonb` holding the API's JSON, and that JSON comes with area 7; until then this compiler can't read or write `Parcel.tags`.
  next: Store what `Parcel.tags` holds in fields of its own, or as records of an entity that refers to `Parcel`, for now (agent)
  rule: toolchain.not_built_yet
  help: docs/language/05-data.html#storage-types

check failed: 1 error, 1 file
```

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Reading and writing records: `query`, `create`, `update`, `patch`, `delete`, includes by the child's reference (question 6), `transition:` in an update, conflict mapping by unique rule name, mutation ownership (`data.mutation_owner_required`), `freshness`, `consistency` and `guard` (`data.*_freshness*`, `data.consistency_*`, `data.mutation_guard_invalid`, `data.value_receiver_guard_required`, `data.raw_query_outside_named_query`, `data.atomic_domain_mismatch`) | 6 | Input to area 6 |
| Reading from a cache or projection (question 3) | 6 | Input to area 6 |
| Whether children are hidden when their parent is soft-deleted; today they stay visible and a required include of a hidden parent reads as missing | 6, 9 | Open |
| Field-level policy in the field's block (`role:`, `immutable:`, `policy: { }`), entity `policy: { }`, `membership`, and purge as a compiler-only authority | 9 | Input to area 9 |
| Persisting presentation text (`type.presentation_text_persistence`) and `Instant` precision | 10 | Input to area 10 |
| Scheduling the purge worker | 12 | Input to area 12 |
| Test fixtures versus seeds | 13 | Input to area 13 |
| `migration.*` rules raised only by `jadpo schema` commands (snapshot, diff, decision, plan and SQL errors, registry read and write failures, `migration.identity_registry_missing`, `migration.identity_registry_exists`, `migration.identity_additions_have_removal`, rename errors), `index.accept_*` and `index.recommendation_*` other than the check warning, `cli.schema_*`, and `toolchain.target_store_not_implemented` (raised by `jadpo build`); none can be triggered by `jadpo check` on source | 14 | Input to area 14 |
| The `jadpo migrate` and rename commands themselves (question 11) | 14 | Input to area 14 |
| GDPR support as its own review: personal-data marking on fields, erasure across every store and cache, export of everything held about a person, retention, and a build report of where personal data lives (Richard: strong out-of-the-box compliance helps adoption) | 15 | Moved to area 15 |

### Open questions

1. **Is the identity always one generated field called `id`?** *Decided
   2026-10-12:* yes: `id`, based on `Uuid`, generated as version 7 on create
   (or the version its named type pins), never supplied by a caller; only
   seeds and migration backfills may set one. No compound identity: a
   many-to-many link is its own entity with a compound unique rule. Mapping
   an existing database with composite keys is out of scope.
2. **Is every entity stored in the application's database?** *Decided
   2026-10-12:* every entity lives in exactly one declared store, `main`
   unless its file says `store: name`; the `persistence` block goes; anything
   never saved is a `type`; an entity in an engine that can lose data (Redis)
   must declare an expiry. An action writes to at most one store; work across
   stores goes through an event and a subscriber.
3. **Keep `cache` and `projection` before any store can run them?** *Decided
   2026-10-12:* today's syntax is removed. Caches return with the Redis
   adapter: record, query result and computed caches beside what they copy,
   derived keys including tenant and policy inputs, compiler-driven
   invalidation, a required `keep:` of at most a day. Projections return with
   the first search, graph or edge engine. Display reads from routes may use
   caches; reads inside actions, policy and authentication never do. Erasing
   personal data wipes its cached copies immediately.
4. **How are compound unique rules written?** *Decided 2026-10-12:* a named
   list in the entity's `unique: { }` section, `tenant_owner: [tenant,
   owner_email]`; the name stays because conflict mapping refers to it.
5. **Is every field typed `X.id` a reference, with `on_delete:` required?**
   *Decided 2026-10-12:* yes: the compiler creates the foreign key and index,
   `on_delete:` goes in the field's block with no default, and the `as:` name
   goes.
6. **Should a parent declare its children?** *Decided 2026-10-12:* no: a
   query includes children by naming the child's reference field, and gets a
   list, or at most one record when that field is unique. Area 6 settles the
   include syntax.
7. **Keep the lifecycle general, in the colon form?** *Decided 2026-10-12:*
   yes: `visible:`, `transitions: { delete: { when: ... set: { } } }` and
   `purge: { after: ... since: ... }`, with no soft-delete shorthand for now.
8. **How are structured fields stored, and which column types?** *Decided
   2026-10-12:* each store uses its engine's native column types (on
   PostgreSQL `uuid`, `numeric`, `timestamptz`, `jsonb`, integer widths from
   declared ranges); structured fields are `jsonb`; caches and Redis entities
   store the same JSON as API responses.
9. **Who decides indexes?** *Decided 2026-10-12:* the compiler creates every
   index its named queries need, multi-column ones included, and lists each
   with the query that needs it. Lookups outside the application use
   `indexes: { by_tenant_created: [tenant, created_at] }`; special kinds
   (full-text, JSON, partial, geographic) take a `kind:` option where the
   store supports it; a partial index for `visible:` is derived. Every index
   change appears in its migration, and the build includes a schema report.
   The recommendation warning and the source-editing command go.
10. **Do entity operations need a `self` receiver?** *Decided 2026-10-12:* no.
    Parameters are ordinary (`todo: Todo` for a snapshot, `todo_id: Todo.id`
    for a reference) and written actions are called by name,
    `complete_todo(todo_id)`. `self: ref`, `self: value`, `Todo.Ref` and
    `guard:` go.
11. **How are migrations created and applied?** *Decided 2026-10-12:*
    `jadpo migrate new <purpose>` writes the next migration, one per store;
    `jadpo check` fails while source and the latest migration disagree;
    `jadpo migrate apply` runs pending migrations as a deploy step, each in a
    transaction, and the application refuses to start against a database
    that does not match; moving an entity between stores needs explicit
    acceptance.
12. **What does removing data need?** *Decided 2026-10-12:* two migrations:
    `retire` (the code stops using it, the column stays), then `drop` in a
    later one, shown in the approval review. Removing an entity works the
    same way.
13. **What is a seed, and can it run in production?** *Decided 2026-10-12:* a
    `seed { }` file in `database/seeds/` that creates data by calling the
    application's own actions; `jadpo seed <name>` refuses a production
    database. Reference data production needs is a migration backfill.
14. **Where is the database itself declared?** *Decided 2026-10-12:* in
    `app.jadpo`, `application { stores: { main: { engine: Engine.postgres url:
    config.database_url } } }`, with `main` on PostgreSQL implied when nothing
    is declared; each store's engine is fixed in source, so every environment
    uses the same engine. D1 is on the roadmap.
    *Revised 2026-10-16 (Richard):* no database is required unless
    something uses one; the implied PostgreSQL `main` becomes "only when
    needed", and where a feature needs a database and none can hold it,
    `jadpo check` is an error naming the feature (sign-in, events and the
    outbox, idempotency keys, webhook de-duplication, flags, audit, any
    database entity).
15. **How is a Redis entity's expiry written?** (carried from area 6) An
    entity in Redis must declare an expiry, and area 6 has expired records
    hidden from queries and updates like `visible:`, but this page gives the
    `expiry:` setting no syntax. What does it look like, and where does it go?
    *Decided 2026-10-16 (Richard):* `expires_after: 2h` on the entity,
    beside `store:`, measured from the record's last write, expired records
    hidden as area 6 says. A Redis entity supports get, create, update and
    delete by `id`, lookup by a `unique:` field, listing and counting by
    equality on one field in `indexes:` (pages in creation order), and
    concurrent-change checks through a revision; anything else is a compile
    error naming PostgreSQL.
16. **A `lifecycle { }` block on an entity written `id: Uuid identity` is
    silently ignored today** (carried from area 8). Does it need its own fix?
    *Recommendation:* no; it is moot once the rework lands, because the
    trailing `identity` form goes.

### Decisions

Decided by Richard on 2026-10-12. Built in chunk 3.1 except where Status says otherwise.

| Topic | Decision | Date |
|---|---|---|
| Layout | Carried from area 1: an entity lives in its own file under its domain's `entities/` folder; `database/` holds `identities.json` (one sorted entry per line), timestamped migration folders and `seeds/` | 2026-10-12 |
| Keyword | Carried from area 4: a declaration with its own folder keeps its keyword (`entity`); anything in `types/` is a `type` | 2026-10-12 |
| Field settings | Carried from area 2: a field's settings go in a block after its type, one per line, including entity facts (`unique`, `generated`, the reference); only `Input` has `optional` and `default` | 2026-10-12 |
| Colon rule | Carried from area 3: entity sections are `name: { ... }` (`lifecycle: { }`, `policy: { }`) | 2026-10-12 |
| Setting options and lists | Carried from area 2: closed options are lower_snake_case names (`on_delete: restrict`); lists are `[ ]` (Superseded 2026-10-12: options are written `Type.value`; see area 2 question 17.) | 2026-10-12 |
| UUID versions | Carried from area 2: new identities are version 7; a named type can pin another version | 2026-10-12 |
| Building types | Carried from area 2: `type X = Output { include: [...] pick: [...] omit: [...] }` read top to bottom; no `include` on an `Input`; generated fields never in an `Input` | 2026-10-13 |
| Calls and settings | Carried from areas 1 and 3: the dot names the owner (`Todo.get(id)`); actions have no settings and every write checks for concurrent change | 2026-10-12 |
| Mapping form | Carried from area 4: write conflicts map with `condition: Failure`, a hidden record reports the same `not_found` failure | 2026-10-12 |
| Identity | One field `id`, `Uuid`, generated as version 7, never supplied by callers; no composite identity: many-to-many links are entities with their own `id` and a compound unique rule | 2026-10-12 |
| Stores | Every entity lives in one declared store (`main` by default); no `persistence` block; never-saved things are types; Redis entities declare an expiry; an action writes at most one store | 2026-10-12 |
| Cache outages | Every cache degrades gracefully: an unreachable cache never fails a request; reads fall back to the database after a short timeout; failed clears are retried, bounded by `keep:`; readiness ignores caches (Richard) | 2026-10-12 |
| Caches | Today's syntax removed; record, query-result and computed caches return with Redis, beside what they copy; derived keys including tenant and policy inputs; compiler-driven invalidation; required `keep:` up to a day; display reads only; erasure wipes cached copies immediately | 2026-10-12 |
| Compound unique rules | A named list in the entity's `unique: { }` section, `tenant_owner: [tenant, owner_email]`; the name is used by failure mapping | 2026-10-12 |
| References | Any field typed `X.id` is a reference: the compiler creates the foreign key and index; `on_delete:` is required in the field's block with no default; no `as:` name | 2026-10-12 |
| Inverses | Parents do not declare children; a query includes children by naming the child's reference field (syntax in area 6) | 2026-10-12 |
| Lifecycle form | The general lifecycle stays, in the colon form (`visible:`, `transitions:`, `purge: { after: since: }`); no soft-delete shorthand for now | 2026-10-12 |
| Storage types | Native column types per engine; structured fields as `jsonb`; caches use API JSON | 2026-10-12 |
| Indexes | Derived from named queries, including multi-column; `indexes: { name: [fields] }` for outside lookups; special kinds via a `kind:` option when the store supports it; indexes shown in migrations and in a generated schema report | 2026-10-12 |
| Receivers | No `self`; ordinary parameters (`todo: Todo`, `todo_id: Todo.id`) and written actions called by name, such as `complete_todo(todo_id)`; `Todo.Ref` and `guard:` go | 2026-10-12 |
| Migration workflow | `jadpo migrate new` / `apply`, one migration per store, check fails on drift, app refuses a mismatched database, moving stores needs acceptance | 2026-10-12 |
| Removing data | Two migrations: retire (code stops using it, column stays), then drop; a drop needs explicit acceptance (the shared mechanism carried to area 7) | 2026-10-12 |
| Seeds | `database/seeds/` files create data through the application's own actions and refuse to run against production; reference data production needs is a migration backfill | 2026-10-12 |
| Store declaration | Stores declared in `app.jadpo`; PostgreSQL `main` implied; same engine in every environment; D1 on the roadmap (Revised 2026-10-16: `main` implied only when something needs a database; an error naming the feature where one is needed and none can hold it) | 2026-10-12 |
| Query placement | Every query lives in its domain's `queries/` folder, one per file, whatever it reads; entity files hold the data definition; generated operations (`Todo.get`, create, update, delete) stay on the entity; a query's cache sits in its file (Richard, after scenarios) | 2026-10-12 |
| Action placement | Every action lives in its domain's `actions/` folder, one per file; written actions are called by name (`complete_todo(todo_id)`); generated operations stay on the entity (Richard) | 2026-10-12 |
| Generated fields | `generated: Generated.on_create` (set once when created) or `Generated.on_change` (set when created and on every change) (Richard) | 2026-10-12 |
| Redis entity expiry syntax | `expires_after:` on the entity, measured from the last write, required on a Redis entity and refused elsewhere; expired records hidden; get, create, update and delete by `id`, lookup by a `unique:` field, listing and counting by equality on one indexed field in creation order, revision checks; anything else a compile error naming PostgreSQL (Richard) | 2026-10-16 |
| `lifecycle { }` ignored on `id: Uuid identity` | | |
| Lifecycle conditions | `visible:` and `when:` may use comparisons, `and`, `or`, `none` checks and `clock.now` on the record's own fields; never other records or the caller (Richard) | 2026-10-12 |
| Cache configuration | Opt-in: `caches: { records: { store: keep: } }` on an entity, `cache: { store: keep: }` on a query; by default a change clears every cached result reading that entity; precise clearing later; entries are one JSON value each (Richard) | 2026-10-12 |
| Stores for system features | Never assumed (Richard, 2026-10-13): there may be no database or several. A feature that only needs somewhere to keep state names its store explicitly (`flags: { store: main }`, `schedules: { store: main }`, `sessions:`, `privacy:`, access records, the erasure log); a missing one is a compile error naming the feature. Tables that must commit with the action's own writes (event outbox, delivery records, idempotency keys, audit history) go in each store that actions write to, placed by the compiler for correctness; the build report lists where every system table lives | 2026-10-13 |
| System tables | Tables the runtime needs (schedule state, event outbox, delivery records, idempotency keys, sessions) are generated, prefixed `jadpo_`, and part of the migration snapshots like any entity; drift in them fails `jadpo check` (Richard, from area 12) | 2026-10-13 |
| Migration snapshots | Each migration folder holds a generated `snapshot.json`, generated SQL and `changes.jadpo` only for decisions; the compiler diffs consecutive snapshots; `jadpo check` fails with the command to run when entities and the latest snapshot disagree (Richard) | 2026-10-12 |
| Table names | A table is named after its entity's born name in singular snake_case (`Todo` → `todo`, `LoanRecord` → `loan_record`), never pluralised: no English inflection rules to get wrong, and the code and the database use the same word. Built in chunk 3.1 (Richard) | 2026-10-14 |
| Lifecycle conditions on bare fields | Chosen in chunk 3.4: `visible:` and `when:` name the record's fields bare, as this page writes them; `in` and `not` aren't among the decided operators | 2026-10-15 |
| What a lifecycle owns | Chosen in chunk 3.4: every field a transition sets or `initial:` gives changes only through transitions; a new record takes its `initial:` value or `none`, and a required field a transition sets needs one; `set:` and `initial:` values are literals, enum values, `none` and `clock.now` | 2026-10-15 |
| Deleting a record with a lifecycle | Chosen in chunk 3.4: `delete` runs the `delete` transition; with none, code can't delete the record, nor remove one whose delete would cascade to an entity with a lifecycle; only purge removes them | 2026-10-15 |
| Purge's period and batches | Chosen in chunk 3.4: `after:` takes a duration in place, or a `Duration` setting (chunk 5.1; `jadpo_runtime::purge_after` is given its value); a batch deletes at most 500 hidden records, oldest `since:` first, each batch its own transaction; a record a `restricted` reference holds stays without stopping the rest; the runtime's entry point runs it, area 12's hourly schedule calls it (chunk 6.2) | 2026-10-15 |
| Partial indexes | Chosen in chunk 3.4: the indexes an entity's queries need hold only the records `visible:` lets through, unless it reads `clock.now`; a reference's own index stays beside a partial one | 2026-10-15 |
| Computed backfills | `backfill:` may be an expression over each row's own stored fields (`Todo.legacy_flag`), constants and pure functions, including a field this migration retires; filled in batches, one `UPDATE` each where the expression has a SQL form and by `jadpo migrate apply` otherwise, then made required; no freeform SQL (Richard, from an outside review) | 2026-10-15 |
| Redis entity updates | Updates by `id` include counters, done as one Redis `increment`; no lifecycles on a Redis entity, expiry taking their place (decided for Richard) | 2026-10-16 |
