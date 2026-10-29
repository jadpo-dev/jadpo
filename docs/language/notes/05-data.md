---
title: Data, design notes
area: 5
---

# Data: design notes

Why [Data](../05-data.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Status

An entity's `policy:` and `membership:` are area 9's.

## Carried to later areas

| Item | Area | Status |
|---|---|---|
| Reading and writing records: `query`, `create`, `update`, `patch`, `delete`, includes by the child's reference (question 6), `transition:` in an update, conflict mapping by unique rule name, and concurrent changes (area 6, "All-or-nothing actions and concurrent changes") | 6 | Input to area 6 |
| Reading from a cache or projection (question 3) | 6 | Input to area 6 |
| Whether children are hidden when their parent is soft-deleted; they stay visible, and a required include of a hidden parent reads as missing | 6, 9 | Open |
| Field-level policy in the field's block (`role:`, `immutable:`, `policy: { }`), entity `policy: { }`, `membership`, and purge as a compiler-only authority | 9 | Input to area 9 |
| Persisting presentation text (`type.presentation_text_persistence`) and `Instant` precision | 10 | Built: milliseconds, and an entity field holding `PresentationText` refused |
| Scheduling the purge worker | 12 | Input to area 12 |
| Test fixtures versus seeds | 13 | Input to area 13 |
| `migration.*` rules raised only by `jadpo schema` commands (snapshot, diff, decision, plan and SQL errors, registry read and write failures, `migration.identity_registry_missing`, `migration.identity_registry_exists`, `migration.identity_additions_have_removal`, rename errors), `index.accept_*` and `index.recommendation_*` other than the check warning, `cli.schema_*`, and `toolchain.target_store_not_implemented` (raised by `jadpo build`); none can be triggered by `jadpo check` on source | 14 | Input to area 14 |
| The `jadpo migrate` and rename commands themselves (question 11) | 14 | Input to area 14 |
| GDPR support as its own review: personal-data marking on fields, erasure across every store and cache, export of everything held about a person, retention, and a build report of where personal data lives, since strong out-of-the-box compliance helps adoption | 15 | Moved to area 15 |

## Decisions

| Topic | Decision |
|---|---|
| Identity (question 1) | One field, `id`, based on `Uuid`, generated as version 7 on create (or the version its named type pins), never supplied by a caller; only seeds and migration backfills may set one. No compound identity: a many-to-many link is its own entity with a compound unique rule. Mapping an existing database with composite keys is out of scope |
| Stores (question 2) | Every entity lives in exactly one declared store, `main` unless its file says `store: name`; anything never saved is a `type`; an entity in an engine that can lose data (Redis) declares an expiry. An action writes to at most one store; work across stores goes through an event and a subscriber |
| Caches (question 3) | Record, query-result and computed caches beside what they copy, with the Redis driver: derived keys including tenant and policy inputs, compiler-driven invalidation, a required `keep:` of at most a day. Projections come with the first search, graph or edge engine. Display reads from routes may use caches; reads inside actions, policy and authentication never do. Erasing personal data wipes its cached copies immediately |
| Compound unique rules (question 4) | A named list in the entity's `unique: { }` section, `tenant_owner: [tenant, owner_email]`; the name stays because conflict mapping refers to it |
| References (question 5) | Any field typed `X.id` is a reference: the compiler creates the foreign key and index, and `on_delete:` goes in the field's block with no default |
| Children (question 6) | A parent does not declare its children: a query includes them by naming the child's reference field, and gets a list, or at most one record when that field is unique (area 6) |
| Lifecycle form (question 7) | The general lifecycle, in the colon form: `visible:`, `transitions: { delete: { when: ... set: { } } }` and `purge: { after: ... since: ... }`, with no soft-delete shorthand. Its details are under [Lifecycle](../05-data.md#lifecycle) |
| Storage types (question 8) | Each store uses its engine's native column types (on PostgreSQL `uuid`, `numeric`, `timestamptz`, `jsonb`, integer widths from declared ranges); structured fields are `jsonb`; caches and Redis entities store the same JSON as API responses |
| Indexes (question 9) | The compiler creates every index its named queries need, multi-column and partial ones included, and lists each with the query that needs it. Lookups from outside the application use `indexes: { by_tenant_created: [tenant, created_at] }`; special kinds take a `kind:` option where the store supports it. Every index change appears in its migration, and the build includes a schema report |
| Receivers (question 10) | No `self`: parameters are ordinary (`todo: Todo` for a snapshot, `todo_id: Todo.id` for a reference) and written actions are called by name, `complete_todo(todo_id)` |
| Migration workflow (question 11) | `jadpo migrate new <purpose>` writes the next migration, one section per store; `jadpo check` fails while source and the latest migration disagree; `jadpo migrate apply` runs pending migrations as a deploy step, each in a transaction; the application refuses to start against a database that does not match; moving an entity between stores needs explicit acceptance |
| Removing data (question 12) | Two migrations: retire (the code stops using it, the column stays), then drop in a later one, which needs explicit acceptance and shows in the approval review (area 18). Removing an entity works the same way |
| Seeds (question 13) | A `seed { }` file in `database/seeds/` creates data by calling the application's own actions; `jadpo seed <name>` refuses a production database; reference data production needs is a migration backfill |
| Store declaration (question 14) | Stores are connections declared in `app.jadpo`; each store's driver is fixed in source, so every environment uses the same engine. No database is required unless something uses one: `main` on PostgreSQL is implied only when needed, and where a feature needs a database and none can hold it, `jadpo check` is an error naming the feature (sign-in, events and the outbox, idempotency keys, webhook de-duplication, flags, audit, any database entity) |
| Redis entities (question 15) | `expires_after:` on the entity, beside `store:`, measured from the record's last write, required on a Redis entity and refused elsewhere; expired records hidden as area 6 says. A Redis entity supports get, create, update and delete by `id` (counters as one Redis `increment`), lookup by a `unique:` field, listing and counting by equality on one field in `indexes:` (pages in creation order), and concurrent-change checks through a revision; no lifecycle, expiry taking its place; anything else is a compile error naming PostgreSQL |
| PostgreSQL over TLS | A database connection always uses TLS, checked against Mozilla's root certificates compiled in, except to this machine (a Unix socket, `localhost`, `127.0.0.1`, `::1`), as area 16 decides for SMTP. A connection may name a private authority, `authority: config.<name>`, a `Certificate` setting read from one certificate's PEM, trusted alone and used to this machine too but over a Unix socket. A server elsewhere without TLS is reached only with `tls: false`, the narrowest form, a word written in source and never a setting, which the network report lists. `sslmode` in the URL is refused, since the connection decides. A local server is reached without TLS rather than offered it, since a local server's own certificate, Debian's snakeoil among them, would fail the check |
| PostgreSQL through Hyperdrive | On Workers a database connection's address setting's variable names a Hyperdrive binding, so `app.jadpo` gains nothing (a text binding of that name stays a direct address). The Worker reaches Hyperdrive without TLS, since the binding's address says `sslmode=disable` and its socket offers none; TLS to the database is Hyperdrive's, by default checked against the public authorities, a private `authority:` uploaded to Hyperdrive with `--sslmode verify-full`. TLS through `startTls()` applies to a direct connection only. Caching off (`--caching-disabled`) is required and stated, since a Worker can't see it; `jadpo build` writes the binding with the id `JADPO_HYPERDRIVE_<variable>` gives, or without one, which `wrangler deploy` refuses. Each choice is reversible |
| Connecting, and a statement that may have run | An attempt at a server waits the address's `connect_timeout`, or 10 seconds when it sets none or 0, stricter than libpq's forever. An autocommitted statement whose connection dropped after any of it was written is `unavailable` to its caller, never sent again by the driver, since the server may have committed it |
| Layout | From area 1: an entity lives in its own file under its domain's `entities/` folder; `database/` holds the identity registry, timestamped migration folders and `seeds/` |
| Keyword | From area 4: a declaration with its own folder keeps its keyword (`entity`); anything in `types/` is a `type` |
| Field settings | From area 2: a field's settings go in a block after its type, one per line, including entity facts (`unique`, `generated`, the reference); only `Input` has `optional` and `default` |
| Colon rule | From area 3: entity sections are `name: { ... }` (`lifecycle: { }`, `policy: { }`) |
| Setting options and lists | From area 2: closed options are written `Type.value` (`on_delete: OnDelete.restricted`); lists are `[ ]` |
| Calls and settings | From areas 1 and 3: the dot names the owner (`Todo.get(id)`); actions have no settings and every write checks for concurrent change |
| Mapping form | From area 4: write conflicts map with `condition: Failure`, and a hidden record reports the same `not_found` failure |
| Cache outages | An unreachable cache never fails a request: reads fall back to the database after a short timeout, failed clears are retried, bounded by `keep:`, and readiness ignores caches |
| Cache configuration | Opt-in: `caches: { records: { store: keep: } }` on an entity, `cache: { store: keep: }` on a query; by default a change clears every cached result reading that entity; entries are one JSON value each |
| Query and action placement | Every query lives in its domain's `queries/` folder and every action in `actions/`, one per file, whatever it reads; entity files hold the data definition, a query's cache sits in its file, and the generated operations (`Todo.get`, create, update, delete) stay on the entity |
| Generated fields | `generated: Generated.on_create` (set once when created) or `Generated.on_change` (set when created and on every change) |
| Lifecycle conditions | `visible:` and `when:` name the record's own fields bare and may use comparisons, `and`, `or`, `none` checks and `clock.now`; never other records or the caller |
| What a lifecycle owns, deleting and purge | See [Lifecycle](../05-data.md#lifecycle): every field a transition sets or `initial:` gives changes only through transitions; `delete` runs the `delete` transition; purge deletes hidden records in batches of at most 500 |
| Stores for system features | Never assumed: there may be no database or several. A feature that only needs somewhere to keep state names its store explicitly (`flags: { store: main }`, `schedules: { store: main }`, `sessions:`, `privacy:`, access records, the erasure log); a missing one is a compile error naming the feature. Tables that must commit with the action's own writes (event outbox, delivery records, idempotency keys, audit history) go in each store that actions write to, placed by the compiler for correctness; the build report lists where every system table lives |
| System tables | Tables the runtime needs are generated, prefixed `jadpo_`, and part of the migration snapshots like any entity; drift in them fails `jadpo check`; see [System tables](../05-data.md#system-tables) |
| Migration snapshots | Each migration folder holds a generated `snapshot.json`, generated SQL and `changes.jadpo` only for decisions; the compiler diffs consecutive snapshots; `jadpo check` fails with the command to run when entities and the latest snapshot disagree |
| Table names | A table is named after its entity's born name in singular snake_case (`Todo` is `todo`, `LoanRecord` is `loan_record`), never pluralised: no English inflection rules to get wrong, and the code and the database use the same word |
| Computed backfills | `backfill:` may be an expression over each row's own stored fields, constants and pure functions, including a field this migration retires; filled in batches, by `UPDATE` where the expression has a SQL form and in Jadpo otherwise, then made required; no freeform SQL. See [Computed backfills](../05-data.md#computed-backfills) |
| A field's rules on read | A stored value is checked against every rule its field's type has when it is read; a row written around Jadpo that breaks one is a defect naming the rule and never the value; `CHECK` constraints may follow |
