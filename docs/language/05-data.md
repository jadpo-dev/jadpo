---
title: Data
area: 5
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Data
<!-- topic: data -->

How a Jadpo program declares the records it keeps and how their schema
changes. Reading and writing records is area 6; who may read or change one
is area 9.

Caches, entities in Redis and seeds are not built yet, and their examples
are proposals.

Examples use the golden todo app's `planning` domain, plus a shop where Redis
is needed.

## Stores
<!-- topic: stores -->

Stores are connections declared in `app.jadpo`, in its `connections:` block
(area 11). Each store's driver is fixed in source, so every environment
runs the same engine. A D1 driver is not built yet.
Every entity lives in exactly one store, `main` unless its file says
`store: name`; anything never saved is a `type`. An entity in Redis must
declare its expiry ("Entities in Redis", below). An action writes to at
most one store; work across stores goes through an event and a subscriber.

**No database unless something needs one.**
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
(area 10); `jadpo migrate apply` and `status` read it from the environment
over `.env.local`, or take the database's URL with `--url`. The other
settings of a connection are area 11's.

### PostgreSQL over TLS

A database connection always uses TLS, the server's certificate checked
against Mozilla's root certificates compiled into Jadpo and for the name
its address gives, except to this machine: a Unix socket, `localhost`,
`127.0.0.1` or `::1`, which are reached without it. The connection decides,
in `app.jadpo` where a review sees it, never its address:

- `authority: config.<name>`, a `Certificate` setting (area 19, read from
  its PEM) holding the private authority its server's certificate is
  signed by: Amazon RDS's root for its region, Cloud SQL's server CA. Only
  that authority is trusted, and the connection uses TLS to this machine
  too, but for a Unix socket, which PostgreSQL never secures.
- `tls: false`, for a server elsewhere that offers no TLS, on a private
  network. The network report (area 11) lists the connection as reached
  without TLS.

```jadpo check project=database-tls file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
            authority: config.database_authority
        }
        legacy: {
            driver: Driver.postgres
            url: config.legacy_url
            tls: false
        }
    }
}
```

```jadpo check project=database-tls file=config.jadpo
config {
    database_authority: Certificate
    database_url: PostgresUrl
    legacy_url: PostgresUrl
}
```

An address that sets TLS itself, `sslmode`, `sslrootcert` or another of
libpq's `ssl…` settings, is refused as the app starts, by `jadpo config
check` and by `jadpo migrate` (area 10's `configuration.value_invalid`),
and a `Certificate` that isn't one certificate's PEM the same way. A server elsewhere that
offers no TLS, or whose certificate the check refuses, is a fault naming
the server, never retried as a database that is down for now. Each
attempt at a server, from the socket to its being ready, waits at most
the address's `connect_timeout`, or 10 seconds when it sets none (or 0,
which libpq reads as forever), then the next host is tried; a deployable
waiting for its database, or ticking, still stops at SIGTERM. `jadpo
migrate`, `jadpo schedules`, `jadpo runs`, `jadpo deliveries` and `jadpo
ai usage` reach a database as its connection says, with `--url` too.

A database connection takes no other setting
(`connection.key_invalid`):

```jadpo check project=e-database-key file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-database-key file=app.jadpo expect=connection.key_invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
            sslmode: "require"
        }
    }
}
```

```text output
error: A connection with `driver: Driver.postgres` takes no `sslmode:`
 --> app.jadpo:6:13
  |
6 |             sslmode: "require"
  |             ^^^^^^^ not a database setting
  reason: A connection to a PostgreSQL database gives `url:`, a `PostgresUrl` setting, and may give `authority:`, a `Certificate` setting its server's certificate is checked against, or `tls: false` for a server elsewhere that offers no TLS.
  next: Remove `sslmode:` (agent)
  rule: connection.key_invalid
  help: docs/errors/connection.key_invalid.md

check failed: 1 error, 2 files
```

`authority:` reads a `Certificate` setting (`connection.value_invalid`):

```jadpo check project=e-database-authority file=config.jadpo hidden
config {
    database_authority: Text
    database_url: PostgresUrl
}
```

```jadpo check project=e-database-authority file=app.jadpo expect=connection.value_invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
            authority: config.database_authority
        }
    }
}
```

```text output
error: `main`'s `authority:` must be a `Certificate` setting
 --> app.jadpo:6:24
  |
6 |             authority: config.database_authority
  |                        ^^^^^^^^^^^^^^^^^^^^^^^^^ `database_authority` is `Text`
  reason: `authority:` is the authority its server's certificate is checked against, as PEM: a `Certificate` setting, `config.<name>`, read from each deployment's environment.
  next: Declare `database_authority: Certificate` in `config.jadpo` (agent)
  rule: connection.value_invalid
  help: docs/errors/connection.value_invalid.md

check failed: 1 error, 2 files
```

`tls:` is only ever written `false` (`connection.value_invalid`):

```jadpo check project=e-database-tls-true file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-database-tls-true file=app.jadpo expect=connection.value_invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
            tls: true
        }
    }
}
```

```text output
error: `main`'s `tls:` is written only as `false`
 --> app.jadpo:6:18
  |
6 |             tls: true
  |                  ^^^^ the default already
  reason: A database connection uses TLS unless it says `tls: false`, for a server elsewhere that offers none; it is decided in `app.jadpo`, where a review sees it, never by a setting.
  next: Remove `tls: true` (agent)
  rule: connection.value_invalid
  help: docs/errors/connection.value_invalid.md

check failed: 1 error, 2 files
```

An authority is never checked without TLS (`connection.key_invalid`):

```jadpo check project=e-database-both file=config.jadpo hidden
config {
    database_authority: Certificate
    database_url: PostgresUrl
}
```

```jadpo check project=e-database-both file=app.jadpo expect=connection.key_invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
            authority: config.database_authority
            tls: false
        }
    }
}
```

```text output
error: `main` names an authority and goes without TLS
 --> app.jadpo:6:13
  |
6 |             authority: config.database_authority
  |             ^^^^^^^^^ never checked
  reason: `authority:` is what the server's certificate is checked against, and a connection with `tls: false` never asks for one.
  next: Remove `authority:`, or `tls: false` (human)
  rule: connection.key_invalid
  help: docs/errors/connection.key_invalid.md

check failed: 1 error, 2 files
```

### PostgreSQL through Hyperdrive

A Worker reaches PostgreSQL through Cloudflare's Hyperdrive, which keeps
the connections to the database open between requests that a Worker,
opening its own for each, can't. The connection names its binding through
its `url:` setting: the setting's variable (`DATABASE_URL`, or its
`binding:`) is, on Workers, a Hyperdrive binding, so the same `app.jadpo`
runs natively and on Workers with nothing added. `jadpo build` writes that
binding into a Workers deployable's Wrangler file in place of a secret,
with the id of the Hyperdrive configuration it is given in
`JADPO_HYPERDRIVE_<variable>` (area 14, "A Workers deployable").

- **The address is the binding's.** Its user, password and database at
  the host the Workers runtime makes for it, which `connect()` reaches
  inside Cloudflare. A text binding of that name is still an address the
  Worker reaches directly, as natively.
- **TLS is Hyperdrive's.** The Worker reaches Hyperdrive without TLS:
  Hyperdrive's own address for the binding says `sslmode=disable`, its
  socket offers no TLS, and the connection doesn't leave Cloudflare.
  Hyperdrive uses TLS to the database, checking the server's certificate
  against the public authorities by default, as Jadpo does; a connection
  whose `authority:` names a private authority needs it uploaded to
  Hyperdrive and the configuration created with it
  (`--ca-certificate-id`, `--sslmode verify-full`). A connection a Worker
  makes directly uses TLS through `startTls()`, checked against the
  platform's roots, and a certificate refused there is the same fault
  naming the server as natively, though Workers reports it only as the
  first bytes go out.
- **Caching off.** Hyperdrive caches reads and doesn't invalidate them on
  a write, and Jadpo's reads are current (area 6), so its configuration
  is created with caching off (`--caching-disabled`). A Worker can't see
  a configuration's caching, so `jadpo build` says so when it writes a
  binding with no id; checking it on a real deploy is not built yet.
- **No session state.** Hyperdrive pools by transaction, so Jadpo's
  statements are unnamed, each transaction sets its isolation level in
  its own `BEGIN`, and leases are rows, never advisory locks, as on every
  target.

## Entities and the `id`
<!-- topic: entities-and-the-id -->

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
<!-- topic: fields-and-their-settings -->

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
<!-- topic: unique-rules -->

`unique: true` in a field's block makes that field unique. A rule over
several fields is named in the entity's `unique: { }` section with its fields
as a list; its fields must exist, be distinct and be required, so every
engine agrees on what a duplicate is. The name is what a write's conflict
mapping refers to (area 6). On a tenant's records a rule holds within each
tenant, its key holding the tenancy reference too (area 9, ["A unique rule
holds within its tenant"](09-policy.md#a-unique-rule-holds-within-its-tenant)).

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
<!-- topic: references-and-on-delete -->

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
<!-- topic: children -->

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
<!-- topic: lifecycle -->

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

The details:

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
  `Duration` setting (`config.soft_delete_retention`, area 10),
  whose value the app started with. A record is purged once `visible:` doesn't hold and its
  `since:` field is older than the run's time less `after:`; each batch
  deletes at most 500 records, oldest first, in its own transaction.
  References that `cascade` and those `cleared` do what they do for any
  delete; a record a `restricted` reference holds, directly or through
  any chain of cascades (one that comes back to its own entity, a folder
  inside a folder, followed as deep as the data goes),
  stays until nothing holds it, and never stops the rest going.
  Purge is the compiler's own operation, outside every action and policy:
  the runtime's purge entry point runs it, and area 12's built-in hourly
  purge schedule calls it (area 12, "The purge schedule").
  Each run reports how many records each batch deleted, which the
  schedule logs.

## Indexes
<!-- topic: indexes -->

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

An index is a list of fields, or a block of `fields:` and `kind:`.
`kind: IndexKind.geographic` makes a GiST index on one geometry field
([area 20](20-geospatial.md), "Indexes"), and `kind: IndexKind.hnsw`, with
`m:` and `ef_construction:` for tuning, makes pgvector's HNSW index on one
vector field ([area 21](21-ai.md), "The index"); the other kinds are not
built yet. The queries' indexes are derived: for each query, and each list
it includes, the fields it always tests with `==` or `in`, then its
order's fields, unless an index or unique key the table already has serves
them; a reference's own index goes once a query's index starts with its
column. The index a query of an entity with a lifecycle needs is partial,
holding only the records `visible:` lets queries see
(`WHERE "deleted_at" IS NULL`), unless `visible:` reads `clock.now`, which
changes as time passes; a reference's own index then stays, since deleting
the record it points at still finds hidden records. A text field a query
sorts by is indexed in code-point order (`"title" COLLATE "C"`), the order
a query sorts text in (area 6), so the sorted read uses the index; its name
ends `_sorted_idx`, apart from an index on the same columns that tests them
with `==`. A geometry field a `geo.` condition tests gets a GiST index, and
a `Point` field a box filter tests one on the point cast to `geometry`
([area 20](20-geospatial.md), "Indexes").

A derived index records why it exists, the first query alphabetically
that needs it, but the migration diff compares only what defines it: its
table, columns or expression, kind, partial condition, operator class,
code-point columns and settings. A query that sorts first needs no
migration, and `jadpo check` and `jadpo migrate new` find nothing to do;
the next migration written for another change records the new reason in
its snapshot, with no step for the index. Snapshots keep their format, so
every existing one reads.

## Caches
<!-- topic: caches -->

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
  category pages a product moved out of and into) is not designed yet.
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
  every Jadpo type maps cleanly: money as `{"amount": 1250, "currency":
  "GBP"}`, large integers as strings, sets as arrays, enums tagged.
  Money's one shape is its minor units and its currency's code, here as on
  the wire (area 19, "`Money` and the operator table").

A record that lives in Redis rather than being copied there (a cart, a
sign-up in progress) is an entity in a Redis store instead ("Entities in
Redis", below). Which services can hold a cache (Redis-compatible ones, or
`Driver.memory` with a warning), and what a cache may hold and never holds,
are area 16's ("Caches: drivers and what they hold").

## Entities in Redis
<!-- topic: entities-in-redis -->

An entity may live in a Redis store, for data that is short-lived and
harmless to lose (question 15). It must
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
without one, is a compile error. Not built yet: these come with the Redis
driver.

## Storage types
<!-- topic: storage-types -->

Each column is the store engine's native type for the field's type; the
source never names one. Structured fields, recursive ones included, are
stored whole as `jsonb` in the API's JSON, which caches and Redis entities
hold too. The database enforces types, required fields, keys and references;
the application checks each field's own rules. Renaming an enum variant
changes stored values, so it needs a migration.

Each migration's snapshot records what every field
allows beyond its column type, an enum's variants and the field's bounds,
lengths and patterns, so changing them is drift `jadpo check` reports like
any other. A change that allows more (a variant added, a longer
`max_length`) is a migration with nothing to run. One that allows less (a
variant renamed or removed, a shorter `max_length`, a narrower range)
needs a decision: `backfill:` gives the records already stored with a
value the field no longer allows one it does, and the migration's SQL
replaces exactly those. A new `pattern:` isn't built yet, since
PostgreSQL's regular expressions aren't Jadpo's; `jadpo migrate new` says
so. An older snapshot that doesn't record these has them recorded by the
next migration, with nothing to decide.

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
| `Duration` | `bigint`, in milliseconds |
| `Enum` without data | `text` holding the variant name; a standard enum holds its code, `GBP`, `Europe/London` |
| `Money` (area 19) | two columns, always: `<field>_amount bigint` of minor units and `<field>_currency char(3)`, with a check constraint holding the currency to the one a type fixes |
| `ZonedTime` (area 10) | two columns: `<field>_instant timestamptz(3)` and `<field>_zone text` holding its zone's code |
| `Enum` with data, `Object`, `List`, `Set`, `Map` | `jsonb` |

A standard type whose data entry lists named columns is stored in those,
as `Money` is, rather than as `jsonb` (area 19, "`Money` and the operator table", and "Every
boundary through the contract"). Every standard object and choice type is
stored in named columns laid out from its parts, `<field>_<part>`, a part
that may be `none` a column that may be `NULL` (an amount of money in two,
another standard object in its own, a choice's variant's name in
`<field>_kind` and then each part its variants hold); a `ZonedTime` in
`<field>_instant` and `<field>_zone`; a range in PostgreSQL's range type of
its ends, a `LocalTime` in `time` and a `LocalDateTime` in `timestamp`
without a zone, each one column named for the field. Every standard enum
holds its code (`Europe/London`, `GET`), as `Currency` does. A quantity's
column carries a comment naming its kind and SI unit
(`Length in metres (m)`), and a rate type's fixed `from:` and `to:` get
check constraints, as a `Money` type's `currency:` does. A query doesn't
compare or order by a standard type stored in named columns yet. Only the
standard collections (`RichText`, `RichTextBlock`, `LocalisedText`) stay
structured `jsonb`, holding their contract's JSON, which the route codec
writes and reads back; a query doesn't compare one yet.

A geometry (`Point` and the other five, or `Geometry`) is one PostGIS
`geography` column, untyped and held to its geometry type and to WGS84 by a
check constraint; the first migration with one creates the extension
(area 20, "Storage").

**A stored value is checked when it is read,** by its field's validator,
the one a write and a backfill check it by: the field's own rules, then
its type's along its chain, a built-in type's own check (`Email`, `Url`)
and a standard object's rules across its parts. Nothing checks it twice,
and a value a write would keep is never refused on read. Every read that
makes a stored value checks it: a query's records, `get`, a parent's, a
child's and children's includes, a nearest query's records, a map layer's
properties and geometry, a `geo.` condition's recheck, the record a
`create` or an `update` gives back, every stored field it holds, and each
field a backfill worked out in Jadpo reads. An aggregate makes a stored
value only for a group by a field and for `min` and `max`, which are
checked; a count, a sum, an average and a time bucket are worked out, and
a sum past a field's `max:` is no defect. A field kept as JSON is read by
its codec with its type's and the field's rules ("Structured values",
below), not again; a field a migration retires has no declaration left,
and a backfill reads it as its built-in type alone.

A row written around Jadpo that breaks a rule is a defect naming the
field and the rule as written, never the value (`` a stored `Tool.stars`
that breaks `max: 5` ``), so the fault behind a 500 or a stopped backfill
names what to fix and leaks nothing; `none` breaks no rule. A backfill
stops as it does on any defect, naming the record's key (`it read a
stored …`). `scale:` can't break on read: a decimal with `scale:` is kept
in a `numeric(p, s)` column, which rounds anything written into it to its
places, so ranges, lengths and patterns are the rules such a row breaks.
No `CHECK` constraint holds them in the database yet. The check runs in
the runtime's one read path (`jadpo_runtime::stored_value`), which both
executors call; the plan gives each selected column its field's validator
(`Select::held`, `Returned::held`).

### Structured values

An application's enum with data, object, list, set and map is kept in one
`jsonb` column as exactly what `json.write` writes for its type, its
contract JSON, as the standard collections are, and read back as a
request's body is, with every rule of its type and the field's own: one
codec, the route's, with the plan numbering the field's type in its JSON
table. A set and a map's keys are written in their canonical order, and a
variant without data as its name, as on the wire. PostgreSQL keeps
`jsonb` in its own key order and spacing, so the text isn't byte for byte
what was written, but it reads back `==`.

```jadpo check project=structured-fields
type Tag = Text {
    max_length: 20
}

type ParcelWeight = Mass {
    min: 0g
    max: 100kg
}

/// What is printed on a parcel.
type Label = Object {
    line: Tag
    note: Tag?
}

/// How a parcel leaves.
type Delivery = Enum {
    pickup
    courier: {
        company: Tag
        weight: ParcelWeight
    }
}

entity Parcel {
    id: Uuid
    label: Label
    tags: List<Tag>
    weights: Map<Tag, ParcelWeight>
    delivery: Delivery
    second_label: Label?  // `NULL` when it is `none`
}
```

A parcel going by courier keeps `{"tag": "courier", "company": "Swift",
"weight": {"value": "0.92", "unit": "kg"}}` in `delivery`, one to be picked
up `"pickup"`, and its `weights` `{"box": {"value": "0.12", "unit": "kg"},
"loaf": {"value": "0.8", "unit": "kg"}}`, keys in their canonical order.

- **What a stored value holds:** what that JSON holds, values built on the
  built-in and standard types, enums, and `Object`s made of them, in
  lists, sets and maps, a type that holds itself included. An `Input`, an
  `Output`, an event, a failure, an email, a message or a prompt inside
  one, a map keyed by a type with no single text form, a format and a
  plural count are `data.field_without_json` (under "Errors"). A secret
  value, an API token or a service key, an entity and text for people
  are refused inside a stored value as in a column of their own
  (`type.secret_exposed`, `type.auth_credential_stored`,
  `data.entity_in_field`, `type.presentation_text_persistence`). A
  `PasswordHash` is kept, as a field of its own is; every `Input` and
  `Output` that would carry one off the server is refused already (area
  19).
- **What doesn't read is a fault.** A row edited outside Jadpo whose JSON
  breaks a rule, isn't its type or is past the limits below is a fault
  when it is read, named by the rule and where in the value it broke
  (`max` on `weight`), never the value.
- **`none` is the column's `NULL`,** never JSON's `null`, which in the
  column is a row written outside Jadpo and a fault. Inside the value, a
  part that is `none` is JSON's `null`, as the wire writes it.
- **No query compares or sorts one,** as for the standard collections. A
  condition on such a field is `toolchain.not_built_yet` and an order by
  one `query.order_field`; the field is read whole wherever its record
  is. No `unique:` or index covers one (`toolchain.not_built_yet`), since
  PostgreSQL compares `jsonb` by its own rules rather than Jadpo's `==`;
  for the same reason a lifecycle tests one only with `== none` or `!=
  none`, and a transition or `initial:` sets one only to `none`
  (`data.lifecycle_condition`, `data.lifecycle_value`). Any other value is
  written by an action, through the codec, and an update by `id` sets one
  only from a value it is given, since the database doesn't work one out
  in one step (`toolchain.not_built_yet`, as for money).
- **An id inside is a value, not a reference.** A `User.id` inside a
  stored value has no foreign key and no `on_delete:`, as an event's
  payload has none (area 12); a link the database must keep is a field of
  its own, or a link entity (question 1).
- **Policy is the field's.** A field's `policy:` covers its whole stored
  value, as any field's does; a field of an `Object` takes no `policy:`
  (`type.unknown_setting`), and a part is read only by reading its field,
  so area 9's proofs hold it as they hold the field.
- **Limits.** A stored value is at most 1 MiB of JSON, the most a
  request's body may be (area 7), nested at most 128 arrays and objects
  deep, the most a request's body is read to, so whatever is kept could
  have arrived in a request and always reads back. Writing a larger or a
  deeper one is a fault naming the limit, as is reading one written
  outside Jadpo; PostgreSQL's own limit on `jsonb` is far above both.
- **Encodings.** The column holds the contract's JSON, so a standard type
  inside it is pinned by that type's golden file and the release's
  encoding (area 19, "Changing a standard type's encoding"). Like an
  entity's columns, a JSON column records no encoding of its own: a
  release that changed what a standard type's JSON holds would come with
  the migration that rewrites stored values.
- **Event payloads go through the same table.** Every event type is
  numbered in the plan's JSON table, by the identity key the outbox's
  `event` column records (`JsonTable::events`), so the outbox writes and
  reads a payload through the one codec, with the type's
  rules applied on read (area 12, "A payload is the contract's JSON"). No
  job takes arguments yet (a schedule's `run:` takes none); a batch's saved
  values are numbered in the same table by their types (area 12).

**Migrating them.** Each migration's snapshot records what a JSON column's
type reads (its `allows` holds `json`): the value's shape (a value built
on a built-in or standard type, with the bounds, lengths, patterns and
places along its chain and any other rule as text; a part that may be
`none`; a list, a set or a map with its lengths) and each of the
application's objects and enums it names, by name, with an object's
fields and rules across its fields and an enum's variants and their
fields. The field's own settings count, as they do for any column, so
changing the stored type, or the field's own rules, is drift like any
other.

A change runs nothing only when the new type can be shown to read every
value the old one wrote: the same built-in or standard type and places,
every other rule the same, no bound or length tighter or new, no pattern
new, a part made optional but never required, a list never made a set,
rules across fields only removed, variants only added, each with the
fields it had. An object's JSON is read member for member, every field
required and no other allowed, so a field added, even one that may be
`none`, or a field removed may read less. A type renamed reads as its
shape. Any other change may read less: the migration checks every stored
value in batches, before anything else in the migration, and `jadpo
migrate apply` refuses it while any doesn't read. A `backfill:` rewrites
those values instead, and needs writing only then:

```jadpo check project=structured-migration file=app.jadpo hidden
application {}
```

```jadpo check project=structured-migration file=domains/shipping/types/tag.jadpo hidden
type Tag = Text {
    max_length: 20
}
```

```jadpo check project=structured-migration file=domains/shipping/types/label.jadpo hidden
use shipping/types { Tag }

/// What is printed on a parcel.
type Label = Object {
    line: Tag
    note: Tag?
}
```

```jadpo check project=structured-migration file=domains/shipping/entities/parcel.jadpo hidden migrate=initial
use shipping/types { Label }

entity Parcel {
    id: Uuid
    label: Label
}
```

```jadpo check project=structured-migration file=domains/shipping/types/tag.jadpo migrate=shorten_tags
type Tag = Text {
    max_length: 12
}
```

```jadpo check project=structured-migration file=database/migrations/20261020T1401_shorten_tags/changes.jadpo migrate=finish
// changes.jadpo
/// Shortens tags, relabelling parcels whose line no longer fits.
migration {
    Parcel.label: {
        backfill: Label {
            line: "Unlabelled"
            note: none
        }
    }
}
```

Without the decision, the migration's SQL reads each batch of labels
(`-- check Parcel.label: in Jadpo, 1000 rows a batch, …`); with it, it
rewrites those the new `Tag` doesn't read (`-- rewrite Parcel.label: …`)
and leaves the rest as they are.

- **The check** comes before anything else in the store's section, so a
  refusal applies nothing of the migration (the migrations before it
  stay). It reads the rows with a value, `NULL` being `none`, 1000 a
  batch in key order, each batch its own read-committed transaction
  under `jadpo_migrations`' lock, locking no row and changing nothing,
  and reads each value with the codec of the field's type now, as a
  query reads it. It records nothing as it goes: a run stopped during it
  starts it again, and a refusal clears the migration's progress, so the
  next run reads every value again. The refusal is
  `migration.apply_failed`, naming how many values don't read and the
  first rule broken and where (`` `max_length` on `line` ``), never a
  value.
- **The decision may be left out.** No decision is required
  (`NeededKind::Unreadable`): `jadpo migrate new` finishes the migration
  and writes a `changes.jadpo` holding only a comment saying how to give
  one. Writing `backfill:` there and running `jadpo migrate new` again
  replaces that field's check with a rewrite.
- **The rewrite** runs in batches as a backfill worked out in Jadpo does
  ("Computed backfills", below), recorded in the resume marker: each
  batch reads the rows with a value, locked `FOR NO KEY UPDATE`, reads
  each with the new type, works the backfill out for those that don't
  read and writes them back through the codec, leaving the rest as they
  are. The backfill may be written out or worked out from the row, as any
  backfill in Jadpo may; a narrowed column's refit outside JSON still
  takes only a value written out. A backfill still reads no field held as
  JSON, its own included, whose old type is gone.
- **Values written meanwhile.** A server still on the old build may write
  a value the new type doesn't read after the check or the rewrite has
  passed its row; as with a narrowed column's refit, no lock holds writes
  to the whole table out for that long, and such a value is a fault when
  read, as any stored value that doesn't read is.
- **A required structured field added to a table with rows** takes a
  `backfill:` as any other field does. Its value, written out (a
  construction such as `Label { … }`, or a variant) or worked out from the
  row, has no SQL form, so it is always worked out in Jadpo and written by
  the codec of the field's type, as a write keeps it, then made required
  in the same last steps as any backfill.
- **Only the latest migration** is read with the project's code, as for
  backfills: an earlier migration's check or rewrite with rows to read
  stops `jadpo migrate apply` (`Checking Parcel.label for <migration>
  needs the build it was written in`).
- **An older snapshot** that recorded nothing of what a JSON column read
  has the next migration check its stored values rather
  than recording the type with nothing to do, since nothing shows that
  the values stored then read now; on an empty table the check reads
  nothing.
- **Output.** `jadpo migrate new` lists "check stored values in batches in
  Jadpo" or "rewrite values no longer read in batches in Jadpo"; the live
  line says `Checking Parcel.label in Jadpo  2000 of 5000 rows`; the
  result says `1200 rows checked in 2 batches in Jadpo` or `3 rows
  rewritten in 2 batches in Jadpo`, and `--json` gives each its `work`.

## Migrations
<!-- topic: migrations -->

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
  which writes the registry, is not built yet (area 14); a field renamed by
  hand reads as one removed and one added, and the removed one's decision
  says so.
- **Decisions only where needed**: a value for existing rows of a required
  field, or retiring a field. Removing a field takes two migrations,
  `FieldChange.retired` (code stops using it, the column stays, now
  nullable, for servers still running the old build) and later
  `FieldChange.dropped`, which needs explicit acceptance (area 18; not built
  yet). Removing an entity, moving one between stores and changing a
  column's type other than widening it aren't built yet: `jadpo migrate
  new` says so.
- **Backfills can be worked out** from each row's own fields, for a new
  field that reshapes data the entity already holds. See "Computed
  backfills" below.
- **Forgetting is caught at once**: `jadpo check` fails while the entities
  and the latest snapshot disagree, naming what changed and the command to
  run, so the editor, the build and CI all stop. Loose files, with no
  `app.jadpo`, have no migrations, so the check is a project's.
- **`jadpo migrate apply`** runs pending migrations as a deploy step, each in
  a transaction, one per store, and records each in the store's
  `jadpo_migrations` table; it refuses a database that applied a migration
  the project doesn't have or one that changed since. `jadpo migrate status`
  lists the migrations and which a database has applied. A native
  deployable built by `jadpo build` refuses to start against a database that hasn't applied every migration
  it was built with, or applied one it doesn't have or that changed
  since, naming the store and the migrations (area 14, "A native
  deployable").
- **The test database.** `jadpo test` gives each action test its own copy
  of a template built from the migrations, on the server
  `JADPO_TEST_DATABASE_URL` names or a disposable one, with copies
  prepared in the background; `--keep-failed` keeps a failed test's.

### Computed backfills

A backfill can be an expression over each row's own stored fields, so a
migration can reshape data rather than only fill it with one value:

```jadpo check project=computed-backfill file=app.jadpo hidden
application {}
```

```jadpo check project=computed-backfill file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
    legacy_flag: Bool
}
```

```jadpo check project=computed-backfill file=domains/planning/types/priority.jadpo hidden
type Priority = Enum {
    low
    normal
    high
}
```

```jadpo check project=computed-backfill file=domains/planning/entities/todo.jadpo hidden migrate=priority_from_flag
use planning/types { Priority }

entity Todo {
    id: Uuid
    priority: Priority
}
```

```jadpo check project=computed-backfill file=domains/planning/types/legacy_flag.jadpo hidden
type LegacyFlag = Bool {}
```

```jadpo check project=computed-backfill file=domains/planning/libraries/priorities.jadpo
// domains/planning/libraries/priorities.jadpo
use planning/types { LegacyFlag, Priority }

library priorities {
    function from_flag(legacy_flag: LegacyFlag) -> Priority {
        if (legacy_flag) {
            return Priority.high
        }
        return Priority.normal
    }
}
```

```jadpo check project=computed-backfill file=database/migrations/20261020T1401_priority_from_flag/changes.jadpo migrate=finish
// changes.jadpo
/// Replaces the legacy flag with a priority worked out from it.
migration {
    Todo.priority: {
        backfill: priorities.from_flag(LegacyFlag(Todo.legacy_flag))
    }
    Todo.legacy_flag: FieldChange.retired
}
```

- `Todo.legacy_flag` means each row's stored value, as on the right of an
  update's `set:` (area 6). The expression may use the entity's fields as
  they stand before this migration, including one this migration retires,
  constants and pure functions (`priorities` is a library, area 1). A
  constant is a `constant` declaration (area 3), read through its library (`priorities.default_priority`).
- **What it may not read.** Another entity's field; a field the same
  migration adds, which no stored record has a value for yet; and a
  setting, directly or through a function it calls, a `crypto`
  operation's `key:` included, since `jadpo migrate apply` is given none
  and each deployment sets its own. Each is
  `migration.backfill_outside_row` (examples under "Errors"). The clock,
  the random generator, queries and actions are refused as in any
  function.
- **A retired field** is read as the built-in type its column holds
  (`boolean`, `text`, `uuid`, a whole number, `numeric`, `timestamptz`,
  `date`), since its declared type goes with it, so
  `LegacyFlag(Todo.legacy_flag)` gives the function the named type it
  takes. A retired enum or structured field isn't offered.
- **Its type** is the field's declared type, as for any value set on the
  field (area 6); a field with rules of its own takes a construction,
  which is refused unless it is proven safe. Nothing in a backfill may
  fail, since a row whose value failed would have none: a call or
  construction nothing handles, or an `attempt` passing a failure on, is
  `migration.backfill_can_fail`. A defect (an overflow, say) still stops
  the batch: it rolls back, the column stays consistent, the run stops at
  that row, and `jadpo migrate apply` names the record's key and the
  fault, never the row's values.
- **A literal or an enum's variant** (`Priority.normal`) is filled by one
  statement in the migration's transaction. Any other value is computed:
  the migration adds the column as optional and fills existing rows in
  batches, each in its own transaction, so a large table is never locked
  for the whole backfill. A last short step fills rows that servers still
  on the old build wrote meanwhile, and makes the column required.
- When every operation in the expression has a SQL form, each batch is one
  `UPDATE`; otherwise `jadpo migrate apply` reads each batch, works the
  values out in Jadpo and writes them back. The generated SQL says which.
- **A field that allows less** (a variant removed, a shorter `max_length`)
  is refitted only with a value written out; a computed one is not built
  yet.
- **Only the latest migration's backfills are checked,** since `jadpo
  check` reads only its decisions; what an earlier one calls ran with the
  code it was written against.
- Freeform SQL in migrations stays out: a backfill is checked Jadpo, so a
  migration can't write a value its field's rules forbid.

**By `UPDATE`.** For the example above, the migration's SQL adds
`priority` as optional, then a line saying how the backfill runs (`--
backfill Todo.priority: by UPDATE, 1000 rows a batch, each batch its own
transaction`), a statement counting the rows left and one batch, whose
`UPDATE` sets `"priority" = (CASE WHEN "todo"."legacy_flag" THEN 'high'
ELSE 'normal' END)`. Functions are written in place, up to 16 calls deep,
their parameters replaced by their arguments' SQL and their `var`s by
their values; an `if` chain whose branches return, and a `match` on a
plain variant, `true` and `false`, a literal, `none` or `some`, become
`CASE`, the last arm its `else`. A `var mut`, a loop or a statement that
only runs for its effect has no SQL form. Each standard operation's form
is area 19's ("SQL forms": a whole number is always `bigint`, a decimal
keeps its places, a defect fails the statement); they and the operators'
and conditionals' forms are tested against the runtime on live
PostgreSQL.

A construction keeps its value as the runtime keeps it. Building a type
with a fixed `scale` keeps exactly that many places, rounded half to even
(`decimal.round`'s form, then a cast to `numeric(38, scale)`, which stops
the statement past 38 digits where the runtime faults):
`Amount(Whole(cost))` with `Whole` at `scale: 0` keeps 2.50 as 2 and 3.50
as 4, as `jadpo test` does, and a decimal is stored at its field's places,
rounded half to even, as the runtime stores one. No other rule changes a
value built on `Bool`, `Int`, `Decimal`, `Text`, `Bytes`, `Uuid`, a date,
an instant or a duration: ranges, lengths and patterns only refuse, a
backfill's constructions are proved not to, and text isn't normalised as
it is built. A standard type that changes a value as it is built (`Html`
made safe, a coordinate kept as its double, a secret) has no SQL form, so
a backfill building one runs in Jadpo.

**In Jadpo.** A backfill with an operation that has no SQL form
(`text.upper`, a `var mut`) is worked out by `jadpo migrate apply` in the
interpreter, with values identical to the runtime's. Its line in the
migration's SQL says so (`-- backfill Todo.priority: in Jadpo, 1000 rows
a batch, …`) and names a digest of the code it runs: the backfill's body,
every function it reaches (where each is written left out), the constants
they read, every type they name along their chains with its fields and
variants (rules and scales included), and the toolchain's version, whose
runtime works the values out. A change to any of them makes `jadpo check`
ask for the migration to be written again, as a change to a backfill by
`UPDATE` does, and a database that ran the old code refuses the new one
as changed, so a resumed backfill can't fill some rows one way and the
rest another. The code it runs is the project's as `jadpo migrate apply`
finds it, which must check without errors once a row needs it.

Each batch reads up to 1000 rows still without a value past where the
last ended, in key order, `FOR NO KEY UPDATE`, so a request on the old
build can't change a field the value is worked out from before the batch
commits, and writes the values back in one `UPDATE … FROM unnest($1,
$2)` filling only rows still without one. The step that locks the table
reads and fills the rows written meanwhile the same way. It reads values
held in one column (the built-in types, an enum without data, and a
standard type held in one, such as a percentage or a quantity, read and
bound as a query and a write do, `jadpo_std::read::Column::of_builtin`),
and fills those or a structured value held as JSON, through the codec; a
backfill in Jadpo that fills a value held in several columns, or reads a
field held so or as JSON, is refused as not built yet.

Only the latest migration's backfills are worked out with the project's
code. An earlier migration's backfill in Jadpo that has rows to fill
stops `jadpo migrate apply` (`Filling Todo.priority for <migration> needs
the build it was written in`), keeping every step that committed, so it
is applied with the build that wrote it and then this one. On an empty
table nothing needs working out, so a new database takes every migration
at once. `jadpo dev` applies migrations with the same interpreter; `jadpo
test` builds its database on empty tables, so nothing needs it there.

**Batches, locks and resuming.** Every step of such a migration takes
`jadpo_migrations`' lock first, as every migration does. A batch is up to
1000 rows, its own transaction at read committed, in key order from where
the last ended (`"id" > $1 ORDER BY "id" LIMIT 1000`), filling only the
rows still without a value, so a batch run twice fills nothing twice; it
takes only its rows' locks. A batch that meets a concurrent change (a
deadlock with a request writing the same rows) is tried again, up to
three times, then the run stops.

Every backfill fills before any column is required. A store's section
that fills a backfill in batches runs every backfill's batches first, in
the order their columns are added. One step then locks each table against
writes, never reads (`SHARE ROW EXCLUSIVE`), fills the rows written
meanwhile for each column, and adds a check per column that it has a
value. A backfill written out as one value beside them fills the existing
rows in the section's first step, and again in that step the rows written
meanwhile, and is required with the rest; alone, it is one statement in
the migration's transaction.

The last steps lock out writes and make no read wait on a scan. The check
is added `NOT VALID`, reading no row, so every row written from then on
is held to it. The next step, a transaction of its own, proves it
(`VALIDATE CONSTRAINT`), reading every row under `SHARE UPDATE
EXCLUSIVE`, which lets reads and writes go on. The last makes each column
required, which PostgreSQL 12 and later do without reading a row once a
proved check says every row has a value, drops the check and records the
migration. Adding the check, requiring the column and dropping the check
each take `ACCESS EXCLUSIVE` for a moment, which waits for reads already
running and holds new ones back meanwhile; no form of `SET NOT NULL`
avoids that. `postgres.sql` opens each such step with a `-- step:` line
saying what it does, and names the check `<table>_<column>_filled`.

The resume marker is a row in `jadpo_migration_steps` (the migration's
name and digest, its next step and the key the last batch ended at),
written in the transaction of the step it records, so a run stopped
anywhere, killed included, leaves the column consistent and carries on
from the step after the last one committed. The table is the runtime's
own, made the first time such a migration runs; a migration's row goes in
the transaction that records it as applied; one whose digest changed
since is refused as changed.

**Progress.** On a terminal, the live line says how far each backfill has
got (`Filling Todo.priority  2000 of 5000 rows`, or `Filling
Todo.priority in Jadpo  2000 of 5000 rows`); in plain output and CI
nothing is printed while it runs, rather than a line per batch. The
result lists each backfill under its migration with the rows it filled,
its batches and how (`2500 rows filled in 3 batches by UPDATE`, or `2500
rows filled in 3 batches in Jadpo (slower than SQL)`), and whether it
carried on from an earlier run; `--json` gives them as `backfills`, each
with its `how` (`"in Jadpo"`). `jadpo migrate new` lists "fill existing
rows in batches in Jadpo" for one worked out in Jadpo.

A migration's `postgres.sql` holds a section per store, each opened by a
`-- store: <name>` line, and names every table, column, constraint and index
as the snapshot does: a table after its entity's born name in
`lower_snake_case` (`Todo` is `todo`), a column after its field, keys and
indexes `<table>_<name>_key`, `_fkey` and `_idx`, cut with a digest past
PostgreSQL's 63 bytes. Each store that holds an entity also gets the
runtime's system tables, prefixed `jadpo_` (below).

## System tables
<!-- topic: system-tables -->

Tables the runtime needs are generated, prefixed `jadpo_`, and part of the
migration snapshots like any entity, so drift in them fails `jadpo check`
too. The ones that must commit with an action's own writes go in each
store an action writes, so an action's outbox and delivery records sit
beside the writes they commit with; the compiler gives them to every store
that holds an entity, since any of them can be written:

| Table | Holds | Behaviour |
|---|---|---|
| `jadpo_outbox` | Each event an action emitted, with its trace context and how deep in a chain it is | Area 12 |
| `jadpo_deliveries` | Each event's delivery to each subscriber: status, attempts, next attempt, last failure, and while a circuit breaker holds it, its connection and time held | Area 12 |
| `jadpo_handled_deliveries` | Each delivery a subscriber's action handled, committed with its writes in the store it writes | Area 12 |
| `jadpo_idempotency_keys` | Each idempotent request's key, digest and stored response | Area 7 |
| `jadpo_operator_actions` | What an operator did to a delivery with `jadpo deliveries`: when, which action, the item and its state then, the operator's name and the database role | Area 12 |
| `jadpo_circuit_breakers` | Each connection's circuit breaker over the deliveries beside it: its state, the run of failures, when it opened, its pauses and probe, and when its alert was raised | Area 12 |

A subscriber's record that it handled a delivery goes in the store the
subscriber writes, committing with its writes, and refers to no table in
another store (area 12, "Delivery: at least once, safe to repeat").
The record of an action with several changing calls (`jadpo_step_records`:
each run's key, action, build, step shape, first start, the generation
of the runner holding it, whether it diverged and when the alert on
unfinished records was raised for it; and `jadpo_steps`: each step's
operation, its request's fingerprint, when it was sent and its sealed
answer; area 12) goes in the store such an action writes, or
`events: { store: }`'s, only where one is kept. A resumable single keyed
changing call with an existing record store uses these tables for dispatch
metadata only, with no request fingerprints or encrypted answers, so the
provider's safe key window also bounds its retries (area 11, "Keys and
windows").

Browser sessions and API tokens (`jadpo_sessions`: each one's user, the
verifiers of its token and a browser session's CSRF token, and when it
started and ends; area 8) go only in the
store the principal's user entity lives in, and service keys
(`jadpo_service_keys`: each key's service, its verifier, and when it was
issued and expires) only in the store its service entity lives
in, since authentication reads a credential with its caller's record in
one statement. Passkeys (`jadpo_passkeys`: each one's user, its
credential ID, the user handle its device keeps and the
`WebAuthnCredential` it is; and `jadpo_passkey_challenges`: each ceremony
under way, by its challenge's verifier, with what it is bound to and when
it expires; area 8) and magic links
(`jadpo_sign_in_requests`: one row per address, by its digest, with the
latest link's and code's verifiers and the counts the limits read; area
8) go beside the sessions they start.
Schedule state, each schedule's lease included, goes in the store
`schedules: { store: }` names in `app.jadpo`, or the only store when there
is one (area 12): `jadpo_schedules` (each schedule's next due time, lease
and last tick) and `jadpo_schedule_runs` (each run and skipped tick),
created when a schedule is declared, with `jadpo_operator_actions` for
what operators did to those runs. Other features that only keep state
name their store the same way (`flags: { store: main }`), and with
several stores a missing one is a compile error naming the feature. Other
runtime state (multi-step call records and batch runs beside the outbox,
OAuth grants, poll state, and webhook and queue de-duplication) follows the same rules and is always in a database, never
a cache (area 16). Every store holding runtime state also keeps `jadpo_housekeeping`, the
owner, lease and last completed hourly cleanup pass (area 12), including a store holding only AI usage. It is generated and
migrated with the other system tables. The manifest
holds a `system_table` node for each, and `jadpo_migrations`, which
`jadpo migrate apply` keeps, is the runtime's own and in no snapshot.
A system table that keeps a value beyond a request, the outbox's payload,
a passkey's credential and a batch run's saved values and `key:`, records the encoding it was written in, an
`encoding` `smallint`, so a later release reads it by the form it has
(area 19, "Changing a standard type's encoding"). The runtime always
writes one, but the column may be `NULL`, for a row kept before encodings
were recorded: a migration adds it to a table that may hold rows already,
with no default, so the same column is the same everywhere. A passkey
with none is read in the form passkeys were first kept in.
A project whose migrations predate the column takes one migration adding
it when `jadpo check` asks (`migration.out_of_date`).

Each system table is defined once, in `jadpo_std::system`, which the
checker turns into each store's tables and the runtime's statements are
written against; a name no table has is no table, never another.
`jadpo_runtime::statements` lists every statement the runtime runs on
them, a test fails when one in its source isn't listed, and a live test
prepares each on PostgreSQL against the tables a migration makes, so a
statement and its table can't drift apart unseen. Statements a plan builds
on a system table with an application's own, a session read with its
user's record, are the plan's and are tested with it.

## Seeds
<!-- topic: seeds -->

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
<!-- topic: where-things-live -->

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

## What is built
<!-- topic: data-built -->

Each feature not built yet is `toolchain.not_built_yet` where it is used.

| Feature | Status |
|---|---|
| Entities, their fields, references, unique rules, indexes and stores | Built. |
| Migrations, computed backfills and the test database | Built. |
| PostgreSQL over TLS and through Hyperdrive | Built. |
| Lifecycles | Built. |
| Structured values stored as JSON, and their migrations | Built. |
| The check of every stored value on read | Built. |
| Manifest nodes | Built: a node for each entity's placement, unique rule, index and reference, with `references` edges between entities, one for each system table and migration snapshot, and one for each lifecycle and transition (`lifecycle Todo`, `transition Todo.delete`, with its `when:`, whether it can refuse a record and the fields it sets). |
| Other schema changes in `jadpo migrate new` | Not built yet: removing an entity, moving one between stores, dropping a retired column (it needs acceptances, area 18) and other column type changes. |
| A worked-out backfill for a field that allows less | Not built yet. |
| Entities in Redis and record caches (`caches:`) | Not built yet. |
| Index kinds other than geographic and HNSW | Not built yet. |
| A database implied only when something needs one (question 14) | Not built yet: it comes with the first feature that can run without one. |
| Seeds and `jadpo seed` | Not built yet. |
| `jadpo rename` | Not built yet: it writes the identity registry (area 14). |

## Errors

Every error this area can produce, with an example that triggers it and the
compiler's actual output; the errors of reading and writing records,
including a write to a field a lifecycle owns, are area 6's. An example's hidden
files and the migrations its `jadpo migrate new` steps write are in the
page's source.

- [`data.identity_missing`](../errors/data.identity_missing.md)
- [`data.identity_type`](../errors/data.identity_type.md)
- [`data.entity_in_field`](../errors/data.entity_in_field.md)
- [`data.field_without_json`](../errors/data.field_without_json.md)
- [`data.generated_type`](../errors/data.generated_type.md)
- [`data.on_delete_required`](../errors/data.on_delete_required.md)
- [`data.on_delete_not_reference`](../errors/data.on_delete_not_reference.md)
- [`data.cleared_required_reference`](../errors/data.cleared_required_reference.md)
- [`data.reference_cycle`](../errors/data.reference_cycle.md)
- [`data.reference_across_stores`](../errors/data.reference_across_stores.md)
- [`data.unknown_field`](../errors/data.unknown_field.md)
- [`data.repeated_field`](../errors/data.repeated_field.md)
- [`data.field_list_required`](../errors/data.field_list_required.md)
- [`data.unique_nullable_field`](../errors/data.unique_nullable_field.md)
- [`data.unique_single_field`](../errors/data.unique_single_field.md)
- [`data.duplicate_rule`](../errors/data.duplicate_rule.md)
- [`data.store_unknown`](../errors/data.store_unknown.md)
- [`data.store_required`](../errors/data.store_required.md)
- [`data.unknown_section`](../errors/data.unknown_section.md)
- [`data.reserved_table_name`](../errors/data.reserved_table_name.md)
- [`data.name_too_long`](../errors/data.name_too_long.md)
- [`data.lifecycle_section`](../errors/data.lifecycle_section.md)
- [`data.lifecycle_condition`](../errors/data.lifecycle_condition.md)
- [`data.lifecycle_unknown_field`](../errors/data.lifecycle_unknown_field.md)
- [`data.lifecycle_value`](../errors/data.lifecycle_value.md)
- [`data.lifecycle_initial_required`](../errors/data.lifecycle_initial_required.md)
- [`data.lifecycle_transition_name`](../errors/data.lifecycle_transition_name.md)
- [`data.lifecycle_purge_visible`](../errors/data.lifecycle_purge_visible.md)
- [`data.lifecycle_purge_after`](../errors/data.lifecycle_purge_after.md)
- [`data.lifecycle_purge_since`](../errors/data.lifecycle_purge_since.md)
- [`migration.out_of_date`](../errors/migration.out_of_date.md)
- [`migration.record_invalid`](../errors/migration.record_invalid.md)
- [`migration.decision_required`](../errors/migration.decision_required.md)
- [`migration.decision_invalid`](../errors/migration.decision_invalid.md)
- [`migration.decision_unknown`](../errors/migration.decision_unknown.md)
- [`migration.unfinished`](../errors/migration.unfinished.md)
- [`migration.backfill_can_fail`](../errors/migration.backfill_can_fail.md)
- [`migration.backfill_outside_row`](../errors/migration.backfill_outside_row.md)
