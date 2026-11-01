---
title: Data
area: 5
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Data
<!-- topic: data -->

How you declare the records your app keeps, where they are stored, and how
their shape changes over time. Reading and changing records is
[Reads and writes](06-reads-and-writes.md); who may read or change one is
[Policy](09-policy.md).

Examples use the golden todo app's `planning` domain, plus a shop where Redis
is needed.

## Stores
<!-- topic: stores -->

A store is somewhere your app keeps records: a PostgreSQL database, or a
Redis server. Every entity lives in exactly one store, `main` unless its
file says `store: <name>`. With no database declared, `main` is a
PostgreSQL database and you don't declare it yourself. Anything your app
never saves is a `type`, not an entity.

You declare other stores as connections in `app.jadpo`, in its
`connections:` block (see [Integrations](11-integrations.md) for
connections in general). Each store's driver is written in the source, so
every environment, from your machine to production, runs the same engine.
Here a shop keeps its orders in a database apart from its catalogue:

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

An action writes to at most one store. Work that spans two stores goes
through an event and a subscriber ([Jobs and events](12-jobs-and-events.md)).
An entity in Redis must say when its records expire
([Entities in Redis](#entities-in-redis)).

**No database unless something needs one.** An app that uses no database
has none: no URL to configure, no migrations, nothing for `jadpo dev` to
start. `main` on PostgreSQL is implied only once something needs a
database. These need one:

- any entity stored in a database;
- sign-in and sessions ([Identity](08-identity.md));
- events and the outbox, the table that holds each event until it is
  delivered ([Jobs and events](12-jobs-and-events.md));
- idempotency keys ([Idempotent writes](07-http.md#idempotent-writes));
- webhook de-duplication ([Integrations](11-integrations.md));
- flags and audit ([Built-in services](16-built-in-services.md)).

Where something needs a database and none can hold it, `jadpo check`
reports an error naming the feature that needs one and the connection to
add. That happens when the app declares databases but none called `main`,
or names a Redis store for something only a database can keep.

**The database's address.** A database connection's address is a setting
of the type its driver needs: `url: config.database_url`, with
`database_url: PostgresUrl` in `config.jadpo`
([Database and other connection URLs](10-configuration-and-time.md#database-and-other-connection-urls)).
`jadpo migrate apply` and `jadpo migrate status` read it from the
environment, then `.env.local`, or take the URL with `--url`. A
connection's other settings are described under
[Integrations](11-integrations.md).

Not built yet: the "only once something needs one" part, which comes with
the first feature that can run without a database, and the D1 driver
([Actions on D1](#actions-on-d1)).

### PostgreSQL over TLS

A database connection always uses TLS. Jadpo checks the server's
certificate against Mozilla's root certificates, which are compiled into
Jadpo, and checks that it is for the host name in the address. The one
exception is this machine: a Unix socket, `localhost`, `127.0.0.1` or
`::1` is reached without TLS.

The connection, in `app.jadpo` where a review sees it, decides anything
else. Its address never does. There are two options.

**Your own certificate authority: `authority:`.** Use it when your
database's certificate is signed by a private authority rather than a
public one, such as Amazon RDS's root for its region or Cloud SQL's server
CA. `authority: config.<name>` names a `Certificate` setting, read from
its PEM ([Standard library](19-standard-library.md)). Only that authority
is trusted, and the connection then uses TLS to this machine too, except
over a Unix socket, which PostgreSQL never secures.

**No TLS: `tls: false`.** Use it for a server elsewhere, on a private
network, that offers no TLS.
[The network report](11-integrations.md#the-network-report) lists the
connection as reached without TLS.

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

An address that sets TLS itself, with `sslmode`, `sslrootcert` or another
of libpq's `ssl…` settings, is refused as the app starts, by `jadpo config
check` and by `jadpo migrate`, with the error
`configuration.value_invalid`
([Configuration and time](10-configuration-and-time.md)). A `Certificate`
setting that isn't the PEM of exactly one certificate is refused the same
way.

A server elsewhere that offers no TLS, or whose certificate the check
refuses, is a fault naming the server. It is never retried as if the
database were only down for now.

Each attempt at a server, from opening the socket to the server being
ready, waits at most the address's `connect_timeout`. When the address
sets none, or sets 0 (which libpq reads as "forever"), it waits 10
seconds. Then the next host is tried. A deployable (a program `jadpo
build` makes, such as a server or a Worker) that is waiting for its
database, or running a schedule, still stops when it is sent SIGTERM.

`jadpo migrate`, `jadpo schedules`, `jadpo runs`, `jadpo deliveries` and
`jadpo ai usage` reach a database the way its connection says, and take
`--url` too.

A database connection takes no setting other than `url:`, `authority:`
and `tls:` (`connection.key_invalid`):

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
  help: https://jadpo.dev/docs/errors/connection.key_invalid

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
  help: https://jadpo.dev/docs/errors/connection.value_invalid

check failed: 1 error, 2 files
```

`tls:` is only ever written `false`, since TLS is already the default
(`connection.value_invalid`):

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
  help: https://jadpo.dev/docs/errors/connection.value_invalid

check failed: 1 error, 2 files
```

An authority is never checked without TLS, so a connection can't have
both (`connection.key_invalid`):

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
  help: https://jadpo.dev/docs/errors/connection.key_invalid

check failed: 1 error, 2 files
```

### PostgreSQL through Hyperdrive

On Cloudflare Workers, your app reaches PostgreSQL through Cloudflare's
Hyperdrive, and you add nothing to `app.jadpo` for it. A Worker opens a
new connection for each request; Hyperdrive keeps connections to the
database open between requests, which a Worker can't.

The connection's `url:` setting names the binding. On Workers, that
setting's variable (`DATABASE_URL`, or the setting's `binding:`) is a
Hyperdrive binding, so the same `app.jadpo` runs natively and on Workers.
`jadpo build` writes that binding into a Workers deployable's Wrangler
file in place of a secret. It takes the id of the Hyperdrive configuration
from `JADPO_HYPERDRIVE_<variable>`
([A Workers deployable](14-tooling.md#a-workers-deployable)).

- **The address is the binding's.** It holds the database's user,
  password and name at a host the Workers runtime makes for it, which
  `connect()` reaches inside Cloudflare. A text binding of that name is
  still an address the Worker reaches directly, as it would natively.
- **TLS is Hyperdrive's.** The Worker reaches Hyperdrive without TLS:
  Hyperdrive's own address for the binding says `sslmode=disable`, its
  socket offers no TLS, and the connection doesn't leave Cloudflare.
  Hyperdrive uses TLS to the database, checking the server's certificate
  against the public authorities by default, as Jadpo does. A connection
  whose `authority:` names a private authority needs that authority
  uploaded to Hyperdrive, and the configuration created with it
  (`--ca-certificate-id`, `--sslmode verify-full`).
- **A direct connection.** A connection a Worker makes directly, not
  through Hyperdrive, uses TLS through `startTls()`, checked against the
  platform's roots. A certificate refused there is the same fault naming
  the server as natively, though Workers reports it only once the first
  bytes go out.
- **Caching off.** Hyperdrive caches reads and doesn't clear them when a
  write changes the data, and Jadpo's reads are always current
  ([Reads and writes](06-reads-and-writes.md)). So the configuration must
  be created with caching off (`--caching-disabled`). A Worker can't see a
  configuration's caching, so `jadpo build` says so when it writes a
  binding with no id.
- **No session state.** Hyperdrive hands a Worker a connection for one
  transaction at a time, so Jadpo's statements are unnamed, each transaction sets
  its isolation level in its own `BEGIN`, and leases are rows, never
  advisory locks. That is the same on every target.

Not built yet: checking a configuration's caching on a real deploy.

### Actions on D1

D1 is Cloudflare's SQLite database. It has no interactive transaction: it
refuses `BEGIN TRANSACTION`. The one way to commit several statements
together is a batch, which runs them in order as one SQL transaction and
rolls the whole batch back when any of them fails. So an action on D1
runs its reads first and then sends all its writes at once:

- **Reads first.** The action's reads run before any of its writes, each
  a statement of its own, and every record it loads is read with its row
  version, a number that changes whenever the row does. Reads go to the
  primary database, as D1's do outside its Sessions API, which an action
  doesn't use, so they are current
  ([Reads and writes](06-reads-and-writes.md)).
- **Writes as one batch.** The action's writes, those of the public
  actions it calls, and its outbox and delivery records go out together as
  one batch, so they commit together or not at all
  ([Logic](03-logic.md)).
- **Each write checked against the version it read.** On PostgreSQL a
  write compares the row version PostgreSQL keeps, `xmin`
  ([Outside calls hold no transaction](06-reads-and-writes.md#outside-calls-hold-no-transaction)).
  SQLite has no `xmin`, so each entity's table on D1 keeps a row version
  of its own in a column, which every write to the row changes. An update,
  delete or transition compares the version the action read. A statement
  in a batch that changes no row isn't a failure, so the driver makes a
  version that doesn't match fail the batch, which then rolls back whole.
- **A conflict reruns the action.** A version that doesn't match is a
  concurrent change like any other. The whole action runs again with
  fresh reads, within the same budget of three attempts, and its keyed
  calls repeat under their keys
  ([All-or-nothing actions and concurrent changes](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)).
- **What can't be written that way is refused.** For a D1 store only,
  `jadpo check` refuses an action whose writes can't all wait until the
  end, since a batch can only roll back whole. That is a write whose input
  depends on an earlier write in the same action, a read after a write,
  or a write refusal the action catches and carries on from. The same
  action on PostgreSQL is accepted.

A batch counts as one query against D1's limit per invocation, but each
statement in it keeps D1's own limits: 100 bound parameters and 100 KB of
SQL. The whole batch has 30 seconds.

Not built yet: D1 itself. This is the rule its driver is built to.

## Entities and the `id`
<!-- topic: entities-and-the-id -->

An entity is a kind of stored record with a lasting identity. Each one is
declared in its own file under its domain's `entities/` folder:

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

Every entity's identity is one field, `id`, based on `Uuid`. Jadpo
generates it as a version 7 UUID when the record is created, or as the
version its named type pins, and callers never supply it. Other code
refers to the field as `Project.id`.

There is no compound identity. A link between two entities is an entity
of its own, with its own `id` and a unique rule over the two references.

An entity value is a snapshot that never changes: reading one of its
fields never touches the database.

## Fields and their settings
<!-- topic: fields-and-their-settings -->

A field's settings, such as `generated:`, `unique:` and `on_delete:`, go
in a block after its type, one per line:

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

`generated:` makes a field that Jadpo sets from the operation's clock:
`Generated.on_create` sets it once, when the record is created, and
`Generated.on_change` sets it on create and on every change. A generated
field is a required `Instant`. Code never assigns it, and it is never in
an `Input`.

A field that may be empty is written `T?`, like `due_at` above. Only an
`Input` has `optional:` and `default:`. A starting value that Jadpo sets,
rather than the caller, is the lifecycle's `initial:`
([Lifecycle](#lifecycle)).

## Unique rules
<!-- topic: unique-rules -->

`unique: true` in a field's block means no two records may hold the same
value in that field. A rule over several fields goes in the entity's
`unique: { }` section, with a name and its fields as a list:

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

Here one user may own only one project of a given name. A rule's fields
must exist, be different fields, and be required, so every engine agrees
on what a duplicate is. A write that breaks the rule refers to it by its
name when it maps the conflict to a failure
([Conflicts](06-reads-and-writes.md#conflicts)).

On a tenant's records, a rule holds within each tenant: its key holds the
tenancy reference too
([A unique rule holds within its tenant](09-policy.md#a-unique-rule-holds-within-its-tenant)).

## References and `on_delete`
<!-- topic: references-and-on-delete -->

Any field typed `X.id` is a reference to a record of `X`. The database
gets a foreign key and an index for it. Its block must say what happens
when the record it refers to is deleted; there is no default.

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

Here a user who still has todos can't be deleted, and a todo whose project
is deleted stays, outside any project.

| `on_delete:` | When the referenced record is deleted | Allowed on |
|---|---|---|
| `OnDelete.restricted` | The delete fails with a conflict while any record refers to it | any reference |
| `OnDelete.cascade` | Records that refer to it are deleted in the same transaction | any reference |
| `OnDelete.cleared` | The reference is set to `none` | an optional reference (`Project.id?`) |

A cycle of required references is an error, because neither record could
be created first.

## Children
<!-- topic: children -->

A parent doesn't declare its children. The relationship is stated once,
on the child's reference. A query that wants a project's todos names that
field, here in an `include:`:

```jadpo fragment
include: {
    todos: Todo.project_id
}
```

It gets a list, since `project_id` isn't unique, or at most one record
when the field is unique. How a query includes records is
[Includes](06-reads-and-writes.md#includes).

A many-to-many link, such as a project's members, is an entity with two
references and a unique rule over both.

## Lifecycle
<!-- topic: lifecycle -->

A lifecycle describes the stages a record goes through and when it drops
out of normal view. The common case is soft delete: deleting a todo hides
it everywhere at once but keeps it for 30 days, so it can be investigated
or restored, and then removes it for good.

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

A lifecycle has three parts:

- **`visible:`** says which records normal reads can see. Every read, list
  and count leaves out records that aren't visible, and reading one by
  `id` reports `not_found`. Nothing else has to remember to filter them.
- **`transitions:`** are named, allowed stage changes, and the only way to
  change the fields a lifecycle owns. Each has a name, an optional `when:`
  that must be true for it to run, and a fixed `set:` saying what it
  changes. The generated operation `Todo.delete(todo_id)` runs the
  `delete` transition.
- **`purge:`** deletes hidden records for good once they have been hidden
  for `after:`, counted from the `since:` field. It runs in small, audited
  batches. Code can't hard-delete a record that has a lifecycle.

`visible:` and `when:` are conditions on the record's own fields. They may
use comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), `and`, `or`, `none`
checks and `clock.now`, so scheduled publishing works. A starting value
that Jadpo sets goes in `initial:`; here every new story starts as a
draft:

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

The conditions can't mention other records or who is asking. That is
[Policy](09-policy.md), and keeping lifecycle conditions this simple lets
them become database indexes.

The details:

- **Bare fields.** A lifecycle's conditions name the record's fields bare,
  as above (`deleted_at`, not `Todo.deleted_at`); a qualified one is
  `data.lifecycle_condition`, with the fix. `in` and `not` aren't among
  the operators; the opposite comparison says the same.
- **What a lifecycle owns.** Every field a transition sets, and every
  field `initial:` gives, changes only through the transitions. A
  `create`, `update` or patch that gives one is
  `data.lifecycle_owned_field`
  ([Reads and writes](06-reads-and-writes.md)). A new record gets its
  `initial:` value, or `none`; so a required field a transition sets needs
  an `initial:` value (`data.lifecycle_initial_required`). Values in
  `set:` and `initial:` are fixed: literals, enum values, `none` and
  `clock.now`.
- **Delete.** `Todo.delete` runs the lifecycle's `delete` transition,
  which changes the record and keeps it. An entity with a lifecycle but no
  `delete` transition can't be deleted by code at all. Neither can a
  record whose removal would cascade to one with a lifecycle
  (`data.lifecycle_hard_delete`). Only purge, retention and erasing a
  person ([Erasure](15-personal-data.md#erasure)) remove such records.
- **Transitions see visible records only.** A transition is an update, so
  it reaches a record only while `visible:` holds. A hidden record is the
  entity's not-found failure, and restoring one needs a decision of its
  own. A `when:` that asks only what `visible:` asks can't refuse a record
  that is there. Any other `when:` can, and its callers map that refusal
  in `conflict:` ([Conflicts](06-reads-and-writes.md#conflicts)).
- **How long purge waits.** `after:` takes a duration written in place
  (`30d`) or a `Duration` setting (`config.soft_delete_retention`,
  [Configuration and time](10-configuration-and-time.md)), whose value is
  the one the app started with. A record is purged once `visible:`
  doesn't hold and its `since:` field is older than the run's time less
  `after:`.
- **How purge deletes.** Each batch deletes at most 500 records, oldest
  first, in its own transaction. References that `cascade` and those
  `cleared` do what they do for any delete. A record that a `restricted`
  reference holds stays until nothing holds it, and never stops the rest
  going. That includes a hold through any chain of cascades, even one that
  comes back to its own entity (a folder inside a folder), followed as
  deep as the data goes.
- **Who runs purge.** Purge is Jadpo's own operation, outside every action
  and policy. The runtime's purge entry point runs it, and the built-in
  hourly purge schedule calls it
  ([The purge schedule](12-jobs-and-events.md#the-purge-schedule)). Each
  run reports how many records each batch deleted, which the schedule
  logs.
- **Retention beside it.** A `retention:` on the entity, or on one of its
  fields, deletes records or clears a value once their time is up,
  whether `visible:` holds or not
  ([Retention](15-personal-data.md#retention)). The same schedule applies
  it, and a field a lifecycle owns or reads can't have one, since clearing
  it would change which records are hidden.

## Indexes
<!-- topic: indexes -->

You don't write the indexes your queries need. Jadpo sees every query, so
it creates every index they need, including indexes over several columns
and partial ones for `visible:`. A page of a user's visible todos, newest
first, gets `(owner_id, created_at desc, id desc)` where `deleted_at` is
empty. Every index change appears in its migration, and in the build's
schema report beside the query that needed it.

### Indexes for lookups from outside the app

Use an `indexes:` section for lookups made from outside the application,
such as a reporting export, that no query of yours needs:

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

### Special kinds of index

A special kind of index takes a `kind:` option, where the store supports
it:

- `kind: IndexKind.geographic` makes a GiST index on one geometry field
  ([Indexes](20-geospatial.md#indexes) under Geospatial).
- `kind: IndexKind.hnsw` makes pgvector's HNSW index on one vector field,
  with `m:` and `ef_construction:` for tuning
  ([The vector index](21-ai.md#the-vector-index) under AI).

A full-text index isn't a kind: it is the entity's `search:` section,
which gives the index, its column of words and its migration from one
declaration ([Search](16-built-in-services.md#search)), and
`kind: IndexKind.full_text` says so (`type.setting_value`). JSON indexes
are a kind too.

Not built yet: index kinds other than geographic and HNSW.

### How the derived indexes are chosen

For each query, and each list it includes, an index holds the fields it
always tests with `==` or `in`, then its order's fields. Jadpo doesn't add
one when an index or unique key the table already has serves them. A
reference's own index goes once a query's index starts with its column.

- **Partial indexes.** The index a query needs on an entity with a
  lifecycle holds only the records `visible:` lets queries see
  (`WHERE "deleted_at" IS NULL`). The exception is a `visible:` that reads
  `clock.now`, which changes as time passes. A reference's own index then
  stays, since deleting the record it points at still finds hidden
  records.
- **Sorted text.** A text field a query sorts by is indexed in code-point
  order (`"title" COLLATE "C"`), the order a query sorts text in
  ([Order and pages](06-reads-and-writes.md#order-and-pages)), so the
  sorted read uses the index. Its name ends `_sorted_idx`, to keep it apart
  from an index on the same columns that tests them with `==`.
- **Geometry.** A geometry field a `geo.` condition tests gets a GiST
  index, and a `Point` field a box filter tests gets one on the point cast
  to `geometry` ([Indexes](20-geospatial.md#indexes) under Geospatial).

A derived index records why it exists: the first query, alphabetically,
that needs it. The migration diff compares only what defines the index:
its table, columns or expression, kind, partial condition, operator class,
code-point columns and settings. So a query that sorts first needs no
migration, and `jadpo check` and `jadpo migrate new` find nothing to do.
The next migration written for another change records the new reason in
its snapshot, with no step for the index. Snapshots keep their format, so
every existing one reads.

## Caches
<!-- topic: caches -->

Nothing is cached unless you add it. A cache keeps a copy of data in a
Redis store, so busy pages read it quickly. You can cache single records,
on an entity, or a query's results, on the query:

```jadpo fragment
entity Product {
    ...
    caches: {
        records: {
            store: cache
            keep: 1h
        }
    }
}

query category_page(category_id: Category.id, page: PageRequest) -> Page<ProductCard> {
    from: Product
    where: Product.category_id == category_id
    cache: {
        store: cache
        keep: 10m
    }
}
```

`caches: { records: }` on an entity caches single products, looked up by
`id`. `cache:` on a query caches that query's results. Each names its
`store:`, a Redis store declared in `app.jadpo`, and `keep:`, the oldest
an entry can be.

- **Writes always go to the database**, never to the cache, and code never
  names cache keys.
- **The running app keeps caches current.** Jadpo generates the code that,
  after every committed change, clears each cached entry the change could
  affect. A change to a `Product` clears every cached result of every
  query that reads products: some extra database reads, but never a stale
  list.
- **Only pages read caches.** Reads inside actions, policy checks and
  sign-in always read the database, so a decision is never made from a
  stale copy.
- **A cache that can't be reached never fails a request**, anywhere in the
  language. Reads go to the database after a short timeout, a clear that
  fails is retried in the background, and `keep:` bounds how stale an
  entry can get meanwhile. Readiness ([Health](07-http.md#health)) doesn't
  depend on a cache.
- **Permissions apply on every read.** Cached results are keyed by tenant
  and by the details policy depends on, so one user's results are never
  served to another. Erasing personal data clears cached copies at once.
- **Each entry is stored as one JSON value**, the same JSON the API sends,
  so every Jadpo type maps cleanly: money as `{"amount": 1250, "currency":
  "GBP"}`, large integers as strings, sets as arrays, enums tagged. Money
  has one shape, its minor units and its currency's code, here as on the
  wire ([Money and payments](19-standard-library.md#money-and-payments)).

A record that lives in Redis, rather than being copied there, such as a
cart or a sign-up in progress, is an entity in a Redis store instead
([Entities in Redis](#entities-in-redis)). Which services can hold a cache
(Redis-compatible ones, or `Driver.memory` with a warning), and what a
cache may and may never hold, are in
[Caches: drivers and what they hold](16-built-in-services.md#caches-drivers-and-what-they-hold).

Not built yet: caches. Clearing only the affected entries, for example
the category pages a product moved out of and into, isn't designed yet.

## Entities in Redis
<!-- topic: entities-in-redis -->

An entity may live in a Redis store, for data that is short-lived and
harmless to lose. It must expire: `expires_after:` gives how long a record
lives, counted from its last write.

app.jadpo:

```jadpo fragment
application {
    connections: {
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}
```

domains/accounts/entities/sign_up_draft.jadpo:

```jadpo fragment
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

Expired records are hidden from every read and update, as records
`visible:` hides are
([Entities in Redis](06-reads-and-writes.md#entities-in-redis) under
Reads and writes). An `expires_after:` on an entity in a database, or a
Redis entity without one, is a compile error.

A Redis entity supports:

- **get, create, update and delete by `id`**, the generated operations
  every entity has. Counters are included: an update by `id` that adds to
  a stored whole number (`SignUpDraft.attempts + 1`) is one Redis
  `increment`.
- **lookup by a `unique:` field**, for which Jadpo keeps one key per
  unique field.
- **listing and counting by equality on one field** declared in
  `indexes:`, for which Jadpo keeps a set per value. Pages come in
  creation order.
- **concurrent-change checks**, through a revision Jadpo keeps on each
  record, as every write checks ([Logic](03-logic.md)).

Anything else is a compile error that names the reason and suggests
PostgreSQL instead: joins and `include`, ranges, ordering by any other
field, `matches`, an index over several fields, and policy grants beyond
equality on the record's own fields. So is a `lifecycle:`, since expiry
takes its place.

A record is stored as one JSON value, the same JSON the API sends, like a
cache entry.

Not built yet: entities in Redis, which come with the Redis driver.

## Storage types
<!-- topic: storage-types -->

You never name a column type. Each column is the store engine's own type
for the field's type:

| Field type | PostgreSQL |
|---|---|
| `Uuid` and types based on it | `uuid` |
| `Int` | `smallint`, `integer` or `bigint`, from the declared range |
| `Decimal { scale: 2 }` | `numeric(p, 2)` |
| `Bool`, `Text`, `Bytes` | `boolean`, `text`, `bytea` |
| `Instant`, `CalendarDate` | `timestamptz(3)`, `date` |
| `Duration` | `bigint`, in milliseconds |
| `Enum` without data | `text` holding the variant name; a standard enum holds its code, `GBP`, `Europe/London` |
| `Money` ([Money and payments](19-standard-library.md#money-and-payments)) | two columns, always: `<field>_amount bigint` of minor units and `<field>_currency char(3)`, with a check constraint holding the currency to the one a type fixes |
| `ZonedTime` ([Configuration and time](10-configuration-and-time.md)) | two columns: `<field>_instant timestamptz(3)` and `<field>_zone text` holding its zone's code |
| `Enum` with data, `Object`, `List`, `Set`, `Map` | `jsonb` |

Structured fields, recursive ones included, are stored whole as `jsonb`,
in the API's JSON, which caches and Redis entities hold too
([Structured values](#structured-values)).

The database enforces types, required fields, keys and references. The
application checks each field's own rules. Renaming an enum variant
changes stored values, so it needs a migration.

### Standard types in columns

A standard type whose data entry lists named columns is stored in those,
as `Money` is, rather than as `jsonb`
([Every boundary through the contract](19-standard-library.md#every-boundary-through-the-contract)).
Every standard object and choice type is stored in named columns laid out
from its parts, `<field>_<part>`:

- a part that may be `none` is a column that may be `NULL`;
- an amount of money takes two columns, and another standard object its
  own;
- a choice's variant's name goes in `<field>_kind`, then each part its
  variants hold;
- a `ZonedTime` goes in `<field>_instant` and `<field>_zone`.

A range is one column of PostgreSQL's range type of its ends, a
`LocalTime` is `time`, and a `LocalDateTime` is `timestamp` without a zone,
each named for the field. Every standard enum holds its code
(`Europe/London`, `GET`), as `Currency` does. A quantity's column carries
a comment naming its kind and SI unit (`Length in metres (m)`). A rate
type's fixed `from:` and `to:` get check constraints, as a `Money` type's
`currency:` does.

Only the standard collections (`RichText`, `RichTextBlock`,
`LocalisedText`) stay structured `jsonb`, holding their contract's JSON,
which the route codec writes and reads back.

A geometry (`Point` and the other five, or `Geometry`) is one PostGIS
`geography` column. It is untyped, and a check constraint holds it to its
geometry type and to WGS84. The first migration with one creates the
extension ([Storage](20-geospatial.md#storage) under Geospatial).

### Changing what a field allows

Each migration's snapshot records what every field allows beyond its
column type: an enum's variants, and the field's bounds, lengths and
patterns. So changing them is drift that `jadpo check` reports like any
other.

- **A change that allows more**, such as a variant added or a longer
  `max_length`, is a migration with nothing to run.
- **A change that allows less**, such as a variant renamed or removed, a
  shorter `max_length` or a smaller range, needs a decision. `backfill:`
  gives the records already stored, whose value the field no longer
  allows, one it does. The migration's SQL replaces exactly those.

Here `large` leaves `BoxSize`, and parcels stored as large become medium:

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

database/migrations/20261020T1401_drop_large/changes.jadpo:

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

An older snapshot that doesn't record what fields allow has it recorded by
the next migration, with nothing to decide.

### A stored value is checked when it is read

Every stored value is checked when it is read, by its field's validator:
the same check a write and a backfill use. That is the field's own rules,
then its type's along its chain, a built-in type's own check (`Email`,
`Url`), and a standard object's rules across its parts. Nothing checks a
value twice, and a value a write would keep is never refused on read.

Every read that makes a stored value checks it: a query's records, `get`,
each kind of include (a parent, a child, children), a nearest query's
records, a map layer's properties and geometry, a `geo.` condition's
recheck, and the record a `create` or an `update` gives back, with every
stored field it holds. So does each field a backfill worked out in Jadpo
reads.

An aggregate makes a stored value only for a group by a field and for
`min` and `max`, which are checked. A count, a sum, an average and a time
bucket are worked out, not stored, so a sum past a field's `max:` is no
defect. A field kept as JSON is read by its codec with its type's and the
field's rules ([Structured values](#structured-values)), and not checked
again. A field a migration retires has no declaration left, so a backfill
reads it as its built-in type alone.

**A row that breaks a rule.** A row written around Jadpo that breaks a
rule is a defect naming the field and the rule as written, never the
value: `` a stored `Tool.stars` that breaks `max: 5` ``. So the fault
behind a 500 or a stopped backfill names what to fix and leaks nothing.
`none` breaks no rule. A backfill stops as it does on any defect, naming
the record's key (`it read a stored …`).

`scale:` can't break on read. A decimal with `scale:` is kept in a
`numeric(p, s)` column, which rounds anything written into it to its
places, so ranges, lengths and patterns are the rules such a row can
break.

Not built yet: a new `pattern:` on a stored field, since PostgreSQL's
regular expressions aren't Jadpo's (`jadpo migrate new` says so);
comparing or ordering by a standard type stored in named columns, or by a
standard collection, in a query; and `CHECK` constraints holding fields'
rules in the database.

## Structured values
<!-- topic: structured-values -->

A field whose type is one of your app's enums with data, objects, lists,
sets or maps is kept in one `jsonb` column. It holds exactly what
`json.write` writes for its type, its contract JSON, as the standard
collections do. It is read back as a request's body is, with every rule
of its type and of the field: one codec, the route's.

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

`second_label` is `NULL` in the database when it is `none`. A parcel going
by courier keeps `{"tag": "courier", "company": "Swift", "weight":
{"value": "0.92", "unit": "kg"}}` in `delivery`, and one to be picked up
keeps `"pickup"`. Its `weights` are `{"box": {"value": "0.12", "unit":
"kg"}, "loaf": {"value": "0.8", "unit": "kg"}}`, the keys in their
canonical order.

A set, and a map's keys, are written in their canonical order, and a
variant without data as its name, as on the wire. PostgreSQL keeps
`jsonb` in its own key order and spacing, so the text isn't byte for byte
what was written, but it reads back `==`.

- **What a stored value may hold:** what that JSON holds. That is values
  built on the built-in and standard types, enums, and `Object`s made of
  them, in lists, sets and maps, a type that holds itself included.
- **What it may not hold.** An `Input`, an `Output`, an event, a failure,
  an email, a message or a prompt inside one, a map keyed by a type with
  no single text form, a format and a plural count are each
  `data.field_without_json` (under "Errors"). A secret value, an API token
  or a service key, an entity and text for people are refused inside a
  stored value as they are in a column of their own
  (`type.secret_exposed`, `type.auth_credential_stored`,
  `data.entity_in_field`, `type.presentation_text_persistence`). A
  `PasswordHash` is kept, as a field of its own is; every `Input` and
  `Output` that would carry one off the server is already refused
  ([Standard library](19-standard-library.md)).
- **A value that doesn't read is a fault.** A row edited outside Jadpo
  whose JSON breaks a rule, isn't its type, or is past the limits below,
  is a fault when it is read. The fault names the rule and where in the
  value it broke (`max` on `weight`), never the value.
- **`none` is the column's `NULL`,** never JSON's `null`. A JSON `null` in
  the column is a row written outside Jadpo, and a fault. Inside the
  value, a part that is `none` is JSON's `null`, as the wire writes it.
- **No query compares or sorts one,** as for the standard collections; the
  field is read whole wherever its record is. An order by one is
  `query.order_field`. A lifecycle tests one only with `== none` or `!=
  none`, and a transition or `initial:` sets one only to `none`
  (`data.lifecycle_condition`, `data.lifecycle_value`), since PostgreSQL
  compares `jsonb` by its own rules rather than Jadpo's `==`. Any other
  value is written by an action, through the codec.
- **An id inside is a value, not a reference.** A `User.id` inside a
  stored value has no foreign key and no `on_delete:`, as an event's
  payload has none ([Jobs and events](12-jobs-and-events.md)). A link the
  database must keep is a field of its own, or a link entity.
- **Policy is the field's.** A field's `policy:` covers its whole stored
  value, as any field's does. A field of an `Object` takes no `policy:`
  (`type.unknown_setting`), and a part is read only by reading its field,
  so [Policy](09-policy.md)'s proofs hold it as they hold the field.
- **Limits.** A stored value is at most 1 MiB of JSON, the most a
  request's body may be ([HTTP](07-http.md)), nested at most 128 arrays and
  objects deep, the most a request's body is read to. So whatever is kept
  could have arrived in a request and always reads back. Writing a larger
  or deeper one is a fault naming the limit, as is reading one written
  outside Jadpo. PostgreSQL's own limit on `jsonb` is far above both.
- **Encodings.** The column holds the contract's JSON, so a standard type
  inside it is pinned by that type's golden file and the release's
  encoding
  ([Changing a standard type's encoding](19-standard-library.md#changing-a-standard-types-encoding)).
  Like an entity's columns, a JSON column records no encoding of its own:
  a release that changed what a standard type's JSON holds would come with
  the migration that rewrites stored values.
- **Event payloads use the same codec.** Every event type is numbered in
  the plan's JSON table, by the identity key the outbox's `event` column
  records, so the outbox writes and reads a payload through the one codec,
  with the type's rules applied on read
  ([Jobs and events](12-jobs-and-events.md)). No job takes arguments yet
  (a schedule's `run:` takes none). A batch's saved values are numbered in
  the same table by their types.

### Migrating structured values

Each migration's snapshot records what a JSON column's type reads (its
`allows` holds `json`):

- the value's shape: a value built on a built-in or standard type, with
  the bounds, lengths, patterns and places along its chain and any other
  rule as text; a part that may be `none`; a list, a set or a map with its
  lengths;
- each of your app's objects and enums it names, by name, with an
  object's fields and rules across its fields, and an enum's variants and
  their fields.

The field's own settings count, as they do for any column. So changing the
stored type, or the field's own rules, is drift like any other.

**When a change runs nothing.** A change runs nothing only when the new
type can be shown to read every value the old one wrote: the same built-in
or standard type and places, every other rule the same, no bound or length
tighter or new, no pattern new, a part made optional but never required, a
list never made a set, rules across fields only removed, and variants only
added, each with the fields it had. An object's JSON is read member for
member, every field required and no other allowed, so adding a field (even
one that may be `none`) or removing one may read less. A type renamed
reads as its shape.

**Any other change may read less.** The migration checks every stored
value in batches, before anything else in the migration, and `jadpo
migrate apply` refuses it while any value doesn't read. A `backfill:`
rewrites those values instead; you need to write one only then. Here `Tag`
gets shorter, and parcels whose label no longer fits are relabelled:

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

database/migrations/20261020T1401_shorten_tags/changes.jadpo:

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
(`-- check Parcel.label: in Jadpo, 1000 rows a batch, …`). With it, it
rewrites those the new `Tag` doesn't read (`-- rewrite Parcel.label: …`)
and leaves the rest as they are.

- **The check** comes before anything else in the store's section, so a
  refusal applies nothing of the migration; the migrations before it stay.
  It reads the rows with a value (`NULL` is `none`), 1000 a batch in key
  order. Each batch is its own read-committed transaction under
  `jadpo_migrations`' lock, locking no row and changing nothing, and reads
  each value with the codec of the field's type now, as a query reads it.
  It records nothing as it goes: a run stopped during it starts it again,
  and a refusal clears the migration's progress, so the next run reads
  every value again. The refusal is `migration.apply_failed`, naming how
  many values don't read and the first rule broken and where (``
  `max_length` on `line` ``), never a value.
- **The decision may be left out.** No decision is required: `jadpo
  migrate new` finishes the migration and writes a `changes.jadpo` holding
  only a comment saying how to give one. Writing `backfill:` there and
  running `jadpo migrate new` again replaces that field's check with a
  rewrite.
- **The rewrite** runs in batches, as a backfill worked out in Jadpo does
  ([Computed backfills](#computed-backfills)), recorded in the resume
  marker. Each batch reads the rows with a value, locked `FOR NO KEY
  UPDATE`, reads each with the new type, works the backfill out for those
  that don't read, and writes them back through the codec, leaving the
  rest as they are. The backfill may be written out or worked out from the
  row, as any backfill in Jadpo may; a narrowed column's refit outside
  JSON still takes only a value written out. A backfill still reads no
  field held as JSON, its own included, whose old type is gone.
- **Values written meanwhile.** A server still on the old build may write
  a value the new type doesn't read after the check or the rewrite has
  passed its row. As with a narrowed column's refit, no lock holds writes
  to the whole table out for that long, and such a value is a fault when
  read, as any stored value that doesn't read is.
- **A required structured field added to a table with rows** takes a
  `backfill:`, as any other field does. Its value, written out (a
  construction such as `Label { … }`, or a variant) or worked out from the
  row, has no SQL form. So it is always worked out in Jadpo and written by
  the codec of the field's type, as a write keeps it, then made required
  in the same last steps as any backfill.
- **Only the latest migration** is read with the project's code, as for
  backfills. An earlier migration's check or rewrite with rows to read
  stops `jadpo migrate apply` (`Checking Parcel.label for <migration>
  needs the build it was written in`).
- **An older snapshot** that recorded nothing of what a JSON column read
  has the next migration check its stored values, rather than recording
  the type with nothing to do, since nothing shows that the values stored
  then read now. On an empty table the check reads nothing.
- **Output.** `jadpo migrate new` lists "check stored values in batches in
  Jadpo" or "rewrite values no longer read in batches in Jadpo". The live
  line says `Checking Parcel.label in Jadpo  2000 of 5000 rows`. The
  result says `1200 rows checked in 2 batches in Jadpo` or `3 rows
  rewritten in 2 batches in Jadpo`, and `--json` gives each its `work`.

Not built yet: a condition on a structured field (`toolchain.not_built_yet`);
a `unique:` or index over one (`toolchain.not_built_yet`), since PostgreSQL
compares `jsonb` by its own rules; and an update by `id` that works a
structured value out in the database rather than setting one it is given
(`toolchain.not_built_yet`, as for money).

## Migrations
<!-- topic: migrations -->

Entities describe the schema you want. A migration records each step from
one version of the database to the next. `jadpo migrate new add_priority`
compares the entities with the previous migration's snapshot and writes
the next folder:

```text
database/migrations/20261020T1400_add_priority/
  snapshot.json        generated: the whole schema at this point
  postgres.sql         generated: the difference from the previous snapshot
  changes.jadpo        only when a decision is needed
```

When a change needs a decision, `jadpo migrate new` writes `changes.jadpo`
with a line to fill in for each one, such as `Todo.priority: {}`. `jadpo
check` names what is missing until it is written, and running `jadpo
migrate new` again finishes the migration, writing the decisions into its
SQL. Here a required `priority` is added, so existing rows need a value,
and the legacy flag is retired:

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

database/migrations/20261020T1401_add_priority/changes.jadpo:

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

- **Forgetting is caught at once.** `jadpo check` fails while the entities
  and the latest snapshot disagree, naming what changed and the command to
  run, so the editor, the build and CI all stop. Loose files, with no
  `app.jadpo`, have no migrations, so the check is a project's.
- **Decisions only where needed:** a value for existing rows of a
  required field, or retiring a field or an entity.
- **Removing data takes two migrations**, and a person
  ([Removing data](#removing-data)).
- **Renames keep their data.** The identity registry, a file Jadpo keeps
  under `database/`, records what each renamed thing was born as
  ([Records and manifest](18-records-and-manifest.md)). Tables and columns
  keep the names they were born with, so renaming a field keeps its
  column. A field renamed by hand, without the registry, reads as one
  removed and one added, and the removed one's decision says so.
- **Field order is no part of the schema.** PostgreSQL can't move a
  column, so each column keeps its place, a new field's column goes last
  wherever the field is written, and fields written in another order are
  nothing to migrate.
- **Backfills can be worked out** from each row's own fields, for a new
  field that reshapes data the entity already holds
  ([Computed backfills](#computed-backfills)).
- **`jadpo migrate apply`** runs pending migrations as a deploy step, each
  in a transaction, one per store, and records each in the store's
  `jadpo_migrations` table. It refuses a database that applied a
  migration the project doesn't have, or one that changed since. `jadpo
  migrate status` lists the migrations and which a database has applied.
- **A native deployable checks its database.** A native deployable built
  by `jadpo build` refuses to start against a database that hasn't applied
  every migration it was built with, or that applied one it doesn't have
  or that changed since, naming the store and the migrations
  ([A native deployable](14-tooling.md#a-native-deployable)).
- **The test database.** `jadpo test` gives each action test its own copy
  of a template built from the migrations, on the server
  `JADPO_TEST_DATABASE_URL` names or a disposable one, with copies
  prepared in the background. `--keep-failed` keeps a failed test's copy.

A migration's `postgres.sql` holds a section per store, each opened by a
`-- store: <name>` line. It names every table, column, constraint and
index as the snapshot does: a table after its entity's born name in
`lower_snake_case` (`Todo` is `todo`), a column after its field, and keys
and indexes `<table>_<name>_key`, `_fkey` and `_idx`, cut short with a
digest past PostgreSQL's 63 bytes. Each store that holds an entity also
gets the runtime's [system tables](#system-tables), prefixed `jadpo_`.

Not built yet: `jadpo rename`, which writes the identity registry
([Tooling](14-tooling.md)), and changing a column's type other than
widening it. `jadpo migrate new` says so for each.

### Removing data

Nothing a migration removes goes without a person, and nothing goes at
the first step. When a field or an entity leaves the source, it is
retired: code stops using it, and its column or table stays with its
data, for servers still running the old build. Dropping it for good is a
later migration of its own, which `jadpo migrate new` writes only when
you name what goes:

```jadpo check project=removing file=app.jadpo hidden
application {}
```

```jadpo check project=removing file=domains/planning/entities/todo.jadpo hidden migrate=initial
entity Todo {
    id: Uuid
    title: Text
    legacy_flag: Bool
}
```

```jadpo check project=removing file=domains/planning/entities/todo.jadpo hidden migrate=retire_legacy_flag
entity Todo {
    id: Uuid
    title: Text
}
```

```jadpo check project=removing file=database/migrations/20261020T1401_retire_legacy_flag/changes.jadpo hidden migrate=finish
migration {
    Todo.legacy_flag: FieldChange.retired
}
```

```sh run project=removing
jadpo migrate new drop_legacy_flag --drop Todo.legacy_flag
```

```text output
  compared the entities with 20261020T1401_retire_legacy_flag

  Todo
    - legacy_flag  boolean  dropped: the column and its data go for good

Wrote database/migrations/20261020T1402_drop_legacy_flag/
    snapshot.json  the whole schema at this point
    postgres.sql   1 statement: drop column
    changes.jadpo  every decision made

decision: 1 change waits for a person's acceptance
  next: A person accepts it: `jadpo acceptance new <purpose>` writes the draft for them. (human)
```

Its `changes.jadpo` says the same, `Todo.legacy_flag: FieldChange.dropped`,
and `postgres.sql` holds the one statement. `jadpo check` then fails until
an acceptance this change adds lists `Todo.legacy_flag:
DataChange.column_dropped` with a person's reason
([Acceptances](18-records-and-manifest.md#acceptances)), and
`jadpo approval` says what goes in plain words: the store, the table, each
column and its type.

| What leaves | First step | Needs a person | Second step | Needs a person |
|---|---|---|---|---|
| A field | Its column retired, `Todo.legacy_flag: FieldChange.retired` | | `--drop Todo.legacy_flag`, `FieldChange.dropped` | `DataChange.column_dropped` |
| An entity | Its table retired, `Todo: EntityChange.retired` | `DataChange.entity_removed` | `--drop Todo`, `EntityChange.dropped` | `DataChange.table_dropped` |
| An entity moved to another store | A new table there, the old one retired, nothing to write | `DataChange.entity_moved` | `--drop Todo` | `DataChange.table_dropped` |

- **A retired entity is out of reach.** Its table keeps its keys and
  indexes for servers on the old build, but no code reads it, and erasure
  and retention no longer reach its records, which is why retiring an
  entity needs an acceptance when retiring a field doesn't. It refers to
  nothing any more, so its rows don't hold back a delete of what they
  named.
- **Land the two steps in separate deploys.** A drop in the same deploy as
  its retirement takes the column from servers still running the old
  build. Nothing checks this for you: the acceptance is where it is
  weighed.
- **What was retired stays retired.** An entity can't take back a retired
  table, nor a field a retired column, until a migration has dropped it. A
  field or entity renamed by hand reads as one removed and one added, so
  its decision says to give it back its name.
- **Moving copies nothing.** A migration runs against one store at a
  time, so an entity moved to another starts empty there and its records
  stay behind, retired, until you copy them across and drop them.
- **Files are let go of.** Dropping a `File` column or a table with one
  takes a holder from each file its rows hold, in the same transaction,
  and the file's bytes go as any file's do once no record holds them
  ([Deleting and erasing](16-built-in-services.md#deleting-and-erasing)),
  as long as a `File` field still names their store of files. While it is
  only retired, its rows still hold them.
- **The audit trail stays.** No migration removes an entry of
  `jadpo_audit`. A dropped field's values stay in its entries until their
  retention ends, where no caller is shown them
  ([Retention and erasure](16-built-in-services.md#retention-and-erasure)).
  Nothing purges the entries of an entity that is gone, and the report says
  so.
- **System tables stay.** A table of Jadpo's own that nothing asks for any
  more, such as the audit trail's once no entity is audited, stays, with
  nothing to decide and no migration written. It may hold what can't be
  got back.
- **It can't run without its acceptance.** `jadpo migrate apply` refuses
  a project with a removal no acceptance lists with a reason
  ([`migration.drop_not_accepted`](../errors/migration.drop_not_accepted.md)),
  whatever git says, before it reaches a database. So do `jadpo seed` and
  `jadpo dev` on a database they didn't start themselves. A test's database
  and one `jadpo dev` starts are thrown away, so they run it. Editing a
  migration already on the before side so that it removes something needs
  an acceptance too, and a statement that removes what the snapshots keep
  is [`migration.record_invalid`](../errors/migration.record_invalid.md).

## Computed backfills
<!-- topic: computed-backfills -->

A backfill can be an expression over each row's own stored fields, so a
migration can reshape data rather than only fill it with one value. Here
each todo's priority is worked out from its legacy flag, by a function in
a library:

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

database/migrations/20261020T1401_priority_from_flag/changes.jadpo:

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

### What a backfill may read

`Todo.legacy_flag` means each row's stored value, as on the right of an
update's `set:` ([Reads and writes](06-reads-and-writes.md)). The
expression may use:

- the entity's fields as they stand before this migration, including one
  this migration retires;
- constants, a `constant` declaration read through its library
  (`priorities.default_priority`, [Constants](03-logic.md#constants));
- pure functions, such as those of the `priorities` library
  ([Project layout and modules](01-project-layout-and-modules.md)).

**What it may not read.** Another entity's field; a field the same
migration adds, which no stored record has a value for yet; and a setting,
directly or through a function it calls, a `crypto` operation's `key:`
included, since `jadpo migrate apply` is given none and each deployment
sets its own. Each is `migration.backfill_outside_row` (examples under
"Errors"). The clock, the random generator, queries and actions are
refused, as in any function.

**A retired field** is read as the built-in type its column holds
(`boolean`, `text`, `uuid`, a whole number, `numeric`, `timestamptz`,
`date`), since its declared type goes with it. So
`LegacyFlag(Todo.legacy_flag)` gives the function the named type it
takes. A retired enum or structured field isn't offered.

**Its type** is the field's declared type, as for any value set on the
field ([Reads and writes](06-reads-and-writes.md)). A field with rules of
its own takes a construction, which is refused unless it is proven safe.

**Nothing in a backfill may fail,** since a row whose value failed would
have none. A call or construction nothing handles, or an `attempt` passing
a failure on, is `migration.backfill_can_fail`. A defect, such as an
overflow, still stops the batch: it rolls back, the column stays
consistent, the run stops at that row, and `jadpo migrate apply` names the
record's key and the fault, never the row's values.

**Only the latest migration's backfills are checked,** since `jadpo check`
reads only its decisions. What an earlier one calls ran with the code it
was written against.

There is no freeform SQL in migrations. A backfill is checked Jadpo, so a
migration can't write a value its field's rules forbid.

### How a backfill runs

A literal or an enum's variant (`Priority.normal`) is filled by one
statement in the migration's transaction. Any other value is computed: the
migration adds the column as optional and fills existing rows in batches,
each in its own transaction, so a large table is never locked for the
whole backfill. A last short step fills rows that servers still on the old
build wrote meanwhile, and makes the column required.

When every operation in the expression has a SQL form, each batch is one
`UPDATE`. Otherwise `jadpo migrate apply` reads each batch, works the
values out in Jadpo and writes them back. The generated SQL says which.

**By `UPDATE`.** For the example above, the migration's SQL adds
`priority` as optional, then a line saying how the backfill runs (`--
backfill Todo.priority: by UPDATE, 1000 rows a batch, each batch its own
transaction`), a statement counting the rows left, and one batch, whose
`UPDATE` sets `"priority" = (CASE WHEN "todo"."legacy_flag" THEN 'high'
ELSE 'normal' END)`.

Functions are written in place, up to 16 calls deep: their parameters are
replaced by their arguments' SQL and their `var`s by their values. An `if`
chain whose branches return becomes `CASE`, the last arm its `else`. So
does a `match` on a plain variant, `true` and `false`, a literal, `none` or
`some`. A `var mut`, a loop, or a statement that only runs for its effect
has no SQL form. Each standard operation's form is in
[SQL forms](19-standard-library.md#sql-forms): a whole number is always
`bigint`, a decimal keeps its places, and a defect fails the statement.
These forms, and those of the operators and conditionals, are tested
against the runtime on live PostgreSQL.

A construction keeps its value as the runtime keeps it. Building a type
with a fixed `scale` keeps exactly that many places, rounded half to even
(`decimal.round`'s form, then a cast to `numeric(38, scale)`, which stops
the statement past 38 digits where the runtime faults). So
`Amount(Whole(cost))`, with `Whole` at `scale: 0`, keeps 2.50 as 2 and
3.50 as 4, as `jadpo test` does, and a decimal is stored at its field's
places, rounded half to even, as the runtime stores one. No other rule
changes a value built on `Bool`, `Int`, `Decimal`, `Text`, `Bytes`,
`Uuid`, a date, an instant or a duration: ranges, lengths and patterns
only refuse, a backfill's constructions are proved not to, and text isn't
normalised as it is built. A standard type that changes a value as it is
built (`Html` made safe, a coordinate kept as its double, a secret) has no
SQL form, so a backfill building one runs in Jadpo.

**In Jadpo.** A backfill with an operation that has no SQL form
(`text.upper`, a `var mut`) is worked out by `jadpo migrate apply` in the
interpreter, with values identical to the runtime's. Its line in the
migration's SQL says so (`-- backfill Todo.priority: in Jadpo, 1000 rows a
batch, …`) and names a digest of the code it runs. The digest covers the
backfill's body, every function it reaches (where each is written left
out), the constants they read, every type they name along their chains
with its fields and variants (rules and scales included), and the
toolchain's version, whose runtime works the values out.

A change to any of them makes `jadpo check` ask for the migration to be
written again, as a change to a backfill by `UPDATE` does. A database that
ran the old code refuses the new one as changed, so a resumed backfill
can't fill some rows one way and the rest another. The code it runs is
the project's as `jadpo migrate apply` finds it, which must check without
errors once a row needs it.

Each batch reads up to 1000 rows still without a value, past where the
last ended, in key order, `FOR NO KEY UPDATE`, so a request on the old
build can't change a field the value is worked out from before the batch
commits. It writes the values back in one `UPDATE … FROM unnest($1, $2)`
filling only rows still without one. The step that locks the table reads
and fills the rows written meanwhile the same way. It reads values held in
one column (the built-in types, an enum without data, and a standard type
held in one, such as a percentage or a quantity, read and bound as a
query and a write do), and fills those, or a structured value held as
JSON, through the codec.

Only the latest migration's backfills are worked out with the project's
code. An earlier migration's backfill in Jadpo that has rows to fill stops
`jadpo migrate apply` (`Filling Todo.priority for <migration> needs the
build it was written in`), keeping every step that committed, so it is
applied with the build that wrote it and then this one. On an empty table
nothing needs working out, so a new database takes every migration at
once. `jadpo dev` applies migrations with the same interpreter; `jadpo
test` builds its database on empty tables, so nothing needs it there.

### Batches, locks and resuming

Every step of such a migration takes `jadpo_migrations`' lock first, as
every migration does. A batch is up to 1000 rows, its own transaction at
read committed, in key order from where the last ended (`"id" > $1 ORDER
BY "id" LIMIT 1000`). It fills only the rows still without a value, so a
batch run twice fills nothing twice, and it takes only its rows' locks. A
batch that meets a concurrent change (a deadlock with a request writing
the same rows) is tried again, up to three times, then the run stops.

Every backfill fills before any column is required. A store's section that
fills a backfill in batches runs every backfill's batches first, in the
order their columns are added. One step then locks each table against
writes, never reads (`SHARE ROW EXCLUSIVE`), fills the rows written
meanwhile for each column, and adds a check per column that it has a
value. A backfill written out as one value beside them fills the existing
rows in the section's first step, and again in that step the rows written
meanwhile, and is required with the rest. Alone, it is one statement in
the migration's transaction.

The last steps lock out writes and make no read wait on a scan. The check
is added `NOT VALID`, reading no row, so every row written from then on is
held to it. The next step, a transaction of its own, proves it (`VALIDATE
CONSTRAINT`), reading every row under `SHARE UPDATE EXCLUSIVE`, which lets
reads and writes go on. The last makes each column required, which
PostgreSQL 12 and later do without reading a row once a proved check says
every row has a value, then drops the check and records the migration.
Adding the check, requiring the column and dropping the check each take
`ACCESS EXCLUSIVE` for a moment, which waits for reads already running and
holds new ones back meanwhile; no form of `SET NOT NULL` avoids that.
`postgres.sql` opens each such step with a `-- step:` line saying what it
does, and names the check `<table>_<column>_filled`.

The resume marker is a row in `jadpo_migration_steps`: the migration's
name and digest, its next step, and the key the last batch ended at. It is
written in the transaction of the step it records, so a run stopped
anywhere, killed included, leaves the column consistent and carries on
from the step after the last one committed. The table is the runtime's
own, made the first time such a migration runs. A migration's row goes in
the transaction that records it as applied; one whose digest changed
since is refused as changed.

**Progress.** On a terminal, the live line says how far each backfill has
got (`Filling Todo.priority  2000 of 5000 rows`, or `Filling Todo.priority
in Jadpo  2000 of 5000 rows`). In plain output and CI nothing is printed
while it runs, rather than a line per batch. The result lists each
backfill under its migration with the rows it filled, its batches and how
(`2500 rows filled in 3 batches by UPDATE`, or `2500 rows filled in 3
batches in Jadpo (slower than SQL)`), and whether it carried on from an
earlier run. `--json` gives them as `backfills`, each with its `how` (`"in
Jadpo"`). `jadpo migrate new` lists "fill existing rows in batches in
Jadpo" for one worked out in Jadpo.

Not built yet: a computed backfill for a field that allows less (a
variant removed, a shorter `max_length`), which is refitted only with a
value written out; and a backfill in Jadpo that fills a value held in
several columns, or reads a field held so or as JSON.

## System tables
<!-- topic: system-tables -->

The runtime needs tables of its own, for example to hold events until
they are delivered. Jadpo generates them, names them with the prefix
`jadpo_`, and keeps them in the migration snapshots like any entity, so
drift in them fails `jadpo check` too.

### Tables in every store that holds an entity

The tables that must commit with an action's own writes go in each store
an action writes, so an action's outbox and
delivery records sit beside the writes they commit with. Jadpo gives them
to every store that holds an entity, since any of them can be written:

| Table | Holds | Described in |
|---|---|---|
| `jadpo_outbox` | Each event an action emitted, with its trace context and how deep in a chain it is | [Jobs and events](12-jobs-and-events.md) |
| `jadpo_deliveries` | Each event's delivery to each subscriber: status, attempts, next attempt, last failure, and while a circuit breaker holds it, its connection and time held | [Jobs and events](12-jobs-and-events.md) |
| `jadpo_handled_deliveries` | Each delivery a subscriber's action handled, committed with its writes in the store it writes | [Jobs and events](12-jobs-and-events.md) |
| `jadpo_idempotency_keys` | Each idempotent request's key, digest and stored response | [Idempotent writes](07-http.md#idempotent-writes) |
| `jadpo_operator_actions` | What an operator did to a delivery with `jadpo deliveries`: when, which action, the item and its state then, the operator's name and the database role | [Jobs and events](12-jobs-and-events.md) |
| `jadpo_circuit_breakers` | Each connection's circuit breaker over the deliveries beside it: its state, the run of failures, when it opened, its pauses and probe, and when its alert was raised | [Jobs and events](12-jobs-and-events.md) |

A subscriber's record that it handled a delivery goes in the store the
subscriber writes, committing with its writes, and refers to no table in
another store
([Delivery: at least once, safe to repeat](12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)).

An action with several changing calls keeps a record of them
([Several changing calls in one action](12-jobs-and-events.md#several-changing-calls-in-one-action)):

- `jadpo_step_records`: each run's key, action, build, step shape, first
  start, the generation of the runner holding it, whether it diverged,
  and when the alert on unfinished records was raised for it;
- `jadpo_steps`: each step's operation, its request's fingerprint, when it
  was sent and its sealed answer.

They go in the store such an action writes, or `events: { store: }`'s,
only where one is kept. A resumable single keyed changing call with an
existing record store uses these tables for dispatch details only, with
no request fingerprints or encrypted answers, so the provider's safe key
window also bounds its retries ([Integrations](11-integrations.md)).

### Tables for sign-in

Sign-in reads a credential with its caller's record in one statement, so
these tables go in the store of the entity they belong to
([Identity](08-identity.md)). Secrets in them are kept only as verifiers:
a verifier is a one-way digest that Jadpo can check a secret against, but
can't turn back into the secret.

- `jadpo_sessions`, browser sessions and API tokens: each one's user, the
  verifiers of its token and a browser session's CSRF token, and when it
  started and ends. Only in the store the principal's user entity lives
  in.
- `jadpo_service_keys`: each key's service, its verifier, and when it was
  issued and expires. Only in the store its service entity lives in.
- `jadpo_passkeys`: each passkey's user, its credential ID, the user
  handle its device keeps and the `WebAuthnCredential` it is; and
  `jadpo_passkey_challenges`: each ceremony under way, by its challenge's
  verifier, with what it is bound to and when it expires. Beside the
  sessions they start.
- `jadpo_sign_in_requests`, for magic links: one row per address, by its
  digest, with the latest link's and code's verifiers and the counts the
  limits read. Beside the sessions they start.
- `jadpo_sign_in_links`: each way in and subject a first sign-in linked to
  a user, when `on_first_sign_in:` decides an external token's first
  sign-in ([A person the app has not seen](08-identity.md#a-person-the-app-has-not-seen)).
  Only in the store the principal's user entity lives in.

### The audit trail's table

An audited entity's history commits with the changes it records, so it
goes in the store the entity lives in, created when an entity there has
an `audit:` ([Audit trail](16-built-in-services.md#audit-trail)):

- `jadpo_audit`: each change to an audited record, by the entity's born
  name, the record's `id` and its place among the record's changes; when
  it was made, what kind of change, who made it, through which action,
  request and trace, and each changed field before and after as text,
  with the encoding that was written in. Rows are only ever added, then
  deleted when the entity's audit retention ends, or with their record
  when erasure deletes it.

### Tables for schedules and other state

Schedule state, each schedule's lease included, goes in the store
`schedules: { store: }` names in `app.jadpo`, or the only store when there
is one ([Schedules](12-jobs-and-events.md#schedules)):

- `jadpo_schedules`: each schedule's next due time, lease and last tick (a
  time the schedule was due to run);
- `jadpo_schedule_runs`: each run and skipped tick.

They are created when a schedule is declared, with
`jadpo_operator_actions` for what operators did to those runs.

Other features that only keep state name their store the same way
(`flags: { store: main }`), and with several stores a missing one is a
compile error naming the feature. The flags keep `jadpo_flags`, each
flag's state, and `jadpo_flag_changes`, each change `jadpo flags` made
and who made it ([Feature flags](16-built-in-services.md#feature-flags)). Other runtime state (multi-step call
records and batch runs beside the outbox, OAuth grants, poll state, and
webhook and queue de-duplication) follows the same rules and is always in
a database, never a cache
([Built-in services](16-built-in-services.md)).

Every store holding runtime state, including a store holding only AI
usage, also keeps `jadpo_housekeeping`: the owner, lease and last completed
hourly cleanup pass ([Jobs and events](12-jobs-and-events.md)). It is
generated and migrated with the other system tables.

A store whose entities have `File` fields keeps `jadpo_files`, how many
fields hold each stored file, which a trigger on each table with a `File`
column keeps as rows change; it is how a file's bytes are deleted once no
record holds them ([Deleting and erasing](16-built-in-services.md#deleting-and-erasing)).
Its migration makes the trigger, and counts the files existing rows hold.

The manifest holds a `system_table` node for each.
`jadpo_migrations`, which `jadpo migrate apply` keeps, is the runtime's
own and in no snapshot.

### Encodings in system tables

A system table that keeps a value beyond a request records the encoding
it was written in, an `encoding` `smallint`, so a later release reads it
by the form it has
([Changing a standard type's encoding](19-standard-library.md#changing-a-standard-types-encoding)).
Those values are the outbox's payload, a passkey's credential, and a
batch run's saved values and `key:`.

The runtime always writes an encoding, but the column may be `NULL`, for a
row kept before encodings were recorded. A migration adds it to a table
that may hold rows already, with no default, so the same column is the
same everywhere. A passkey with none is read in the form passkeys were
first kept in. A project whose migrations predate the column takes one
migration adding it when `jadpo check` asks (`migration.out_of_date`).

## Seeds
<!-- topic: seeds -->

A seed creates data for development by calling your app's own actions, so
validation, lifecycle and policy apply to it. Each seed is a file in
`database/seeds/`.

database/seeds/local_demo.jadpo:

```jadpo fragment
/// A user with a few todos, for local development.
seed {
    var alice: User = attempt planning.register_user("alice@example.com")
    attempt planning.create_todo(alice.id, "Buy milk")
    attempt planning.create_todo(alice.id, "Book dentist")
}
```

A seed is named by its file: `seed { }` in `local_demo.jadpo` is
`local_demo`. It is checked like a test, and its calls run as a test's do
([Calls with no caller](13-testing.md#calls-with-no-caller)): one that
passes a caller runs as them, under policy, and one that passes none runs
without it, which is how a seed makes the first user who holds a role.

`jadpo seed <name>` runs one
([`jadpo seed`](14-tooling.md#jadpo-seed)). It refuses a production
database: data production needs is a migration backfill. A production
database is any that isn't on this machine: every server the store's
address names must be a Unix socket, `localhost`, `127.0.0.1` or `::1`,
the ones [PostgreSQL over TLS](#postgresql-over-tls) also treats as this
machine. Nothing is run against any store until every store passes
(`seed.production_refused`).

A seed runs as a request does, at one time read from the clock and with
the machine's randomness, so each run makes new identities. Each action it
calls commits on its own, so a seed that stops at a failure or a fault
keeps what its earlier calls committed (`seed.failed`). Mail lands in the
dev inbox and provider calls are faked, as under `jadpo dev`, and what its
actions emit is delivered before the command ends.

## Where things live
<!-- topic: where-things-live -->

An entity's file holds only its data definition: its store, fields,
unique rules, indexes, lifecycle, policy and record cache. Every entity
gets generated operations: `Todo.get`, create, update and delete.

Everything else has a folder of its own in the domain, one declaration per
file: every query you write in `queries/`, every action in `actions/` and
every failure in `failures/`.

```jadpo fragment
// domains/planning/failures/todo_not_found.jadpo
type TodoNotFound = Failure {
    kind: FailureKind.not_found
    message: "Todo not found."
}

// domains/planning/actions/complete_todo.jadpo
action complete_todo(todo_id: Todo.id) fails TodoNotFound -> Todo {
    var todo: Todo = attempt Todo.get(todo_id)
    ...
}
```

`Todo.get(todo_id)` above is one of the generated operations. A written
action is called by its name, `complete_todo(todo_id)`, or
`planning.complete_todo(todo_id)` from another domain when it is public.

There is no `self`. A parameter `todo: Todo` is a snapshot of the record,
and `todo_id: Todo.id` is a reference to it.

## What is built
<!-- topic: data-built -->

Each feature not built yet is `toolchain.not_built_yet` where it is used.

| Feature | Status |
|---|---|
| Entities, their fields, references, unique rules, indexes and stores | Built. |
| Migrations, computed backfills and the test database | Built. |
| PostgreSQL over TLS and through Hyperdrive | Built. Checking a Hyperdrive configuration's caching on a real deploy is not built yet. |
| D1, and actions on it as reads first and one checked batch of writes | Not built yet. |
| Lifecycles | Built. |
| Structured values stored as JSON, and their migrations | Built. Conditions on them, a `unique:` or index over them, and an update by `id` that works one out in the database are not built yet. |
| The check of every stored value on read | Built. `CHECK` constraints holding fields' rules in the database are not built yet. |
| Standard types stored in named columns, and the standard collections | Built. Comparing or ordering by them in a query is not built yet. |
| Manifest nodes | Built: a node for each entity's placement, unique rule, index and reference, with `references` edges between entities, one for each system table and migration snapshot, and one for each lifecycle and transition (`lifecycle Todo`, `transition Todo.delete`, with its `when:`, whether it can refuse a record and the fields it sets). |
| Removing data | Built: retiring a field or an entity, moving an entity to another store and `--drop`, each needing its [acceptance](18-records-and-manifest.md#acceptances) ([Removing data](#removing-data)). Copying an entity's records to its new store is yours to do. |
| Other schema changes in `jadpo migrate new` | Not built yet: a new `pattern:` on a stored field, and other column type changes. |
| A worked-out backfill for a field that allows less | Not built yet. |
| A backfill in Jadpo that fills a value held in several columns, or reads a field held so or as JSON | Not built yet. |
| Entities in Redis and record caches (`caches:`) | Not built yet. Clearing only the cached entries a change affects isn't designed yet. |
| Index kinds other than geographic and HNSW | Not built yet. |
| A database implied only when something needs one | Not built yet: it comes with the first feature that can run without one. |
| Seeds and `jadpo seed` | Built. |
| `jadpo rename` | Not built yet: it writes the identity registry ([Tooling](14-tooling.md)). |

## Errors

Every error this page's features can produce, with an example that
triggers it and the compiler's actual output. The errors of reading and
writing records, including a write to a field a lifecycle owns, are under
[Reads and writes](06-reads-and-writes.md). An example's hidden files and
the migrations its `jadpo migrate new` steps write are in the page's
source.

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
- [`migration.drop_not_accepted`](../errors/migration.drop_not_accepted.md)
- [`migration.unfinished`](../errors/migration.unfinished.md)
- [`migration.backfill_can_fail`](../errors/migration.backfill_can_fail.md)
- [`migration.backfill_outside_row`](../errors/migration.backfill_outside_row.md)
