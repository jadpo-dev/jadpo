# Data stores and caching: proposed model

**Status:** research proposal, 2026-10-12; decides nothing. Tests Richard's
direction against [the patterns](data-stores-and-caching-patterns.md) and
[scenarios](data-stores-and-caching-scenarios.md), and recommends answers to
[area 5](../05-data.md) questions 2, 3, 8, 11 and 14. Code uses the decided
syntax and is illustrative; none of it compiles today. *Still needs checking*
marks claims not verified against a running engine.

The direction holds up. Two rules make it safe: an action writes only one
store, and the call site decides whether a read may use a copy. Keys and
invalidation are always derived.

## 1. The model

### 1.1 Stores

Stores are declared once, in `app.jadpo`, inside the unnamed `application { }`
singleton. A small app declares nothing and gets one store called `main`:

```jadpo fragment
// Implied when app.jadpo declares no stores. Writing it out changes nothing.
application {
    stores: {
        main: {
            engine: postgres
            url: config.database_url
        }
    }
}
```

A larger app:

```jadpo fragment
application {
    stores: {
        main: {
            engine: postgres
            url: config.database_url
            replicas: config.database_replica_urls    // optional; read-only copies
        }
        cache: {
            engine: redis
            url: config.redis_url
        }
        sessions: {
            engine: redis
            url: config.session_redis_url
        }
    }
}
```

| Rule | Detail |
|---|---|
| S1 | A store is a name, an `engine:` (closed option) and a connection taken from `config`. Raw environment variables are never read. |
| S2 | The engine is fixed in source. It is not a config value, so the same engine runs in development, tests, CI and production. A missing or different engine at start-up is a start-up failure, not a fallback. |
| S3 | The default store is always named `main`. With no `stores:` it is implied as above. If `stores:` is declared without `main`, every entity must name its store. |
| S4 | `config.database_url` must exist in `config.jadpo` when `main` is implied. `jadpo new` writes it; leaving it out is an error with a one-step fix. |
| S5 | `replicas:` declares same-engine read-only copies of a SQL store. They follow the cache read rules in §1.7, not separate ones. |
| S6 | A store an adapter cannot yet run is a compile error (`store.engine_not_available`), never a silent no-op. |

**The shared-SQL compromise** (patterns §1: SQLite locally, Postgres as the
gate) costs more than it saves. It limits generated SQL to what both engines
share, which rules out exclusion constraints for double-booking (scenario 4),
`jsonb`, row locks and range types. Concurrency, isolation and collation
bugs still surface only in CI or production. Its one real benefit, fast
tests, area 13 can get on Postgres itself: template databases, a rolled-back
transaction per test, or PGlite (Postgres as WebAssembly, in-process;
*still needs checking*: one connection, so no lock contention). Adopt
Richard's rule. An app may still choose SQLite, but then uses it everywhere.

### 1.2 Engines and capabilities

Capabilities belong to the engine and are built into the compiler. Apps cannot
declare or override them. The compiler refuses any feature that relies on a
capability the entity's store lacks (`store.capability_missing`). The error
names the entity, the feature, the store, its engine and the missing
capability.

| Capability | postgres | sqlite | d1 | redis | later: search, graph, objects |
|---|---|---|---|---|---|
| `durability` | durable | durable | durable | **lossy** (Jadpo cannot verify persistence settings) | per engine |
| `transactions` | full | full, one writer | **one batch** (*still needs checking* that a guarded batch can abort) | optimistic on one node (WATCH/MULTI) | none |
| `unique` on non-id fields | yes | yes | yes | no | no |
| `references` (foreign keys) | yes | yes (pragma on) | yes | no | no |
| `ordering`, `range_queries`, `aggregates` | yes | yes | yes | id lookup only | per engine |
| `row_locks`, `exclusion` | yes | no | no | no | no |
| `expiry` (native time to live) | no (runtime sweep) | no (sweep) | no (sweep) | yes | n/a |
| `atomic_increment` | yes (`count + 1`) | yes | yes | yes | no |
| `cache_target` (may hold caches) | no | no | no | yes | no |
| `full_text` index kind | yes | FTS5 | FTS5 | no | search: yes |
| Several app instances | yes | **no**: one process | yes | yes | yes |

v1 builds `postgres` and `redis` (cache use only). `sqlite` keeps today's
generator. `d1`, `search`, `graph` and `objects` stay unavailable (S6) until
each has an adapter.

### 1.3 Placement

An entity names its store only when it isn't `main`:

```jadpo fragment
entity BrowserSession {
    id: Uuid
    user_id: User.id {
        on_delete: cascade            // crosses stores: applied after commit (X5)
    }
    last_seen_at: Instant {
        generated: create_or_change
    }

    store: sessions
    expiry: {
        after: config.session_idle_timeout
        since: last_seen_at
    }
}
```

| Rule | Detail |
|---|---|
| P1 | Every entity is stored in exactly one store; no `store:` means `main`. No `persistence` block or `role:`. |
| P2 | Anything never saved (a request's actor, presence, typing) is a `type`, not an entity. |
| P3 | An entity in a `lossy` store must declare `expiry:`, has no `purge:` or unique rules beyond `id`, and no durable entity may reference it. |
| P4 | `expiry: { after: since: }` works in any store: an expired record is hidden at once (time-based visibility, C9), then removed by native time-to-live or the runtime sweep. Stock reservations and booking holds need no hand-written job. |
| P5 | Placement is per entity, never per domain. Changing it is a migration with an explicit decision (§4). |
| P6 | No per-tenant or per-region placement: residency means a deployment per region. Partitioned stores can be added later without changing P1. |

### 1.4 Across stores

| Rule | Detail |
|---|---|
| X1 | **An action writes at most one store**, counting public actions it calls (they share its transaction, area 1) and computed fields (§1.8). |
| X2 | Work across stores is an event plus a subscriber (area 12). No two-phase commit. |
| X3 | An action may read another store; the build lists those reads as outside its transaction. |
| X4 | Reads that guard a write (policy, lifecycle, invariants) must be in the write's store (CONSISTENCY-001 §8 rule 7). |
| X5 | A reference crosses stores only with `on_delete: cascade`, applied after commit from the change record. |
| X6 | No joins, filters, ordering or aggregates across stores; an include across stores is a batched lookup by id. |
| X7 | Audit entries, change records and idempotency keys are written in the written store's transaction, so audit is always atomic. |

Moving an entity therefore breaks code that never mentions it (scenario
question 7), so the error names the cause:

```text
error[store.action_spans_stores]: checkout writes two stores
  --> domains/ordering/actions/checkout.jadpo:12
   | writes Order (store main) and Cart (store carts)
   | Cart moved to store carts at domains/ordering/entities/cart.jadpo:9
   = fix: clear the cart in a subscriber to OrderEvent.placed, or keep Cart in main
```

X1 also refuses the scenarios' tempting shortcuts with no special rule: a
Redis `SET NX` webhook dedupe beside a Postgres payment, a Redis booking lock,
and a separate audit store each make an action write two stores.

### 1.5 Copies: one mechanism

Every copy is maintained from the **change record**, a runtime table written
in the same transaction as the change (today `__jadpo_changes`). Extend it
with a global `sequence` (order, watermarks, revision tokens), the before and
after values of every field a copy partitions on (so moving a product between
categories invalidates both), and pruning once every consumer has passed it.

One **invalidator** per store (elected by advisory lock) reads the record in
sequence order, applies each change to every cache idempotently and at least
once, and records the last sequence applied as the **watermark**. Writers
also invalidate after their own commit as a fast path. Because invalidation
comes from the store, not the writing process, a worker's or back office's
writes reach every deployable's caches (scenario question 24).

### 1.6 Caches

A cache is a copy in a `cache_target` store. Application code never writes or
names it. A cache is a declaration beside what it copies: inside the entity
for its records and its queries, or in the `queries/` file for a top-level
query. `from:` keeps its one meaning, "built from".

```jadpo fragment
entity Product {
    id: Uuid
    slug: Slug {
        unique: true
    }
    category_id: Category.id {
        on_delete: restrict
    }
    price: Money
    published_at: Instant?

    lifecycle: {
        visible: published_at != none and published_at <= clock.now
        ...
    }

    query listing(category_id: Category.id, page: ProductPageRequest) -> ProductPage { ... }
    query stock_badge(product_id: Product.id) -> StockBadge { ... }

    /// Product records by id and by slug.
    cache records {
        from: Product
        store: cache
        keep: 1h
    }

    /// Category pages. Every relevant committed write wipes them.
    cache listing_pages {
        from: Product.listing
        store: cache
        keep: 10m
    }

    /// "Only 3 left": may be up to 5 seconds old. Writes do not wipe it.
    cache stock_badges {
        from: Product.stock_badge
        store: cache
        keep: 5s
        refresh: on_expiry
    }
}
```

**Kinds.** A **record** cache (`from:` the entity) holds records before
policy, by `id` and each `unique` field, and serves display reads and
includes by those keys. A **query result** cache (`from:` a query returning
entities or projections) and a **computed** cache (`from:` a query returning a
count, total or summary) hold the result after policy, one entry per argument
set and policy key.

**Settings.** `keep:` is required, at most `1d`: the oldest any entry can be,
which backstops lost invalidations and bounds erasure. `refresh:` is
`on_change` (default) or `on_expiry`; `on_expiry` is never wiped by writes, so
it is bounded staleness of `keep:`, listed in the build. There is no
`strategy:`, `delivery:` or key setting: one strategy, always durable, keys
always derived.

**Rules.**

| Rule | Detail |
|---|---|
| **C1 Keys** | Never written by hand: cache name, digests of the cached shape, the policy and any `config` the query reads, canonical arguments, and the **policy key** (exactly the principal, tenant and segment inputs the query's policy reads). Anything unkeyable refuses the cache (`cache.unkeyable_input`). |
| **C2 Tenancy** | For a tenant-scoped entity, the tenant is the first key and scope component, by construction. |
| **C3 Who reads** | Only callers of the query, through its policy. Record caches hold data before policy, and policy runs on every hit (TAO). Query and computed caches hold results after policy, keyed by the policy key, and each returned record's owner or tenant is checked against the caller before release (OpenAI's fix). Ops commands show metadata, never values. |
| **C4 Scopes** | Derived from the equality filters of the query and its policy, e.g. `Product:category_id=c`, `Membership:user_id=u`: the leading columns of the index the compiler already derives for that query. No equality filter means the whole entity within its tenant. Joins, includes, aggregates and every entity the policy reads add scopes. A write touches the scopes of its old and new values. |
| **C5 Generations** | Each scope has a generation in the cache store, set to the higher of its current value and the change sequence (idempotent). An entry records the generations it was filled under; a read fetches entry and current generations together, and any mismatch is a miss. Record caches also delete the record's entry, so erased data leaves. |
| **C6 Stale fills** | A fill reads generations *before* reading the authority, so a commit during the fill leaves an entry that is never served (Meta's versioning). |
| **C7 Stampede** | One fill at a time per key (per instance, and across instances via a self-expiring fill lock). Others wait briefly, then read the authority under a per-cache concurrency limit. Only `on_expiry` entries are served while refreshing; wiped or time-expired entries never are. No warm-up in v1. |
| **C8 Errors are not values** | A failed read is never cached and never invalidates (Facebook 2010). A missing generation counts as "changed now". |
| **C9 Time boundaries** | Where `clock.now` appears in the query, `visible:`, `expiry:` or policy, the fill also asks the authority for the next instant any record enters or leaves the result (an overdue todo leaving, an embargoed article arriving). The entry expires at that instant or `keep:`, whichever is sooner. Supported: `field <op> clock.now ± duration` and `clock.today(zone)`; anything else is refused (`cache.time_boundary_unknown`). |
| **C10 Read-your-writes** | In a request: after an action commits, reads of the scopes it touched go to the authority. Across requests: write responses carry the commit sequence (`jadpo-revision` header and short-lived cookie); a request carrying a revision newer than the watermark skips caches and replicas. A second device sees the measured invalidation lag. |
| **C11 Outage** | Reads fall back to the authority under the C7 limit, then fail `unavailable`. After reconnecting, caches are bypassed until the watermark reaches the change-record head. Start-up refuses a Redis eviction policy that could evict generation keys. |
| **C12 Permission changes** | Policy inputs are scopes (C4), so a membership, role or tenant-setting write wipes every result that depended on it. Record caches re-run policy against authority. A changed policy changes the digest, so old entries are never read. |
| **C13 Never cached** | Anything authentication reads (`immediate` reads authority; `bounded` uses signed credentials), entities in lossy stores, queries whose policy key can't be derived, and unbounded results or exports. |
| **C14 Erasure** | A purge, `delete_organisation` or takedown makes every copy unreadable once the invalidator applies it, and deletes record entries. Query entries leave within `keep:`; tenant erasure also deletes by tenant prefix. The build reports the bound: invalidation lag plus `keep:`. |
| **C15 Deploys** | Shape and policy digests let v1 and v2 builds share Redis without reading each other's entries. Generations are keyed by identity-registry ids, so renames keep them. Caches never need migrations. |
| **C16 Immutable data** | An entity with no updates and no transitions is recognised as immutable; its record cache has nothing to invalidate. |

### 1.7 Read consistency without callable settings

Queries carry no `freshness:` (area 3: no callable settings). The **call site**
decides, so a query used by a route and by an action is handled correctly in
both:

| Where the query is called | Source | Guarantee |
|---|---|---|
| Inside an action, directly or through a public action or subscriber | Authority, in the action's transaction | Authoritative |
| Policy, authentication, lifecycle and invariant checks | Authority | Authoritative |
| From a route or another query, outside any action, after a write earlier in the same request | Authority for the touched scopes | Read your writes |
| From a route or another query, outside any action, otherwise | `on_change` cache, a replica or the authority. Copies are used only when the request's revision is not newer than their watermark (C10). | Read your writes for the writer. Others see bounded lag, measured, not promised. |
| As above, through an `on_expiry` cache | That cache | Stale by at most `keep:` |

The compiler may pick a stronger source and never a weaker one. The build
lists each call site with its source. There is no "eventual" level until a
search or graph projection exists.

### 1.8 Computed values stored on an entity

An exact total that a write must rely on, such as a ledger balance, seat count
or unread count, is a field the compiler maintains in the **same transaction**
as the rows it summarises:

```jadpo fragment
entity LedgerAccount {
    id: Uuid
    balance: Money {
        computed: {
            sum: LedgerEntry.amount
            by: LedgerEntry.account_id
        }
    }
    entry_count: Int {
        computed: {
            count: LedgerEntry
            by: LedgerEntry.account_id
        }
    }
}
```

Only `count` and `sum`, with `by:` naming a reference to this entity in the
same store; the source's `visible:` filter applies. Refused: sources whose
visibility depends on `clock.now`, and `min`, `max` and `average` (store
`sum` and `count`, divide in a function). Like generated fields they are
never assigned or in an `Input`, are exempt from mutation ownership, and
count as writes for X1. The build flags each as a hot-row risk, and adding
one backfills it in its migration (avoiding Rails' stale counters).

Dashboards and leaderboards are **computed caches** of an ordinary query.
Nightly rollups that are reported as facts, such as `daily_sales`, are
**entities** written by a schedule.

### 1.9 Search, blobs, external data, ephemeral data

| Need | Model |
|---|---|
| Search in v1 | A `full_text` index kind in the entity's own store; no copy needed |
| Search engine, graph, edge (later) | A **projection**: a reshaped copy maintained from the change record, named explicitly by the queries that read it, never substituted by the compiler, never guarding a write; candidates are re-checked against authority policy |
| Blobs (later) | `objects` engine and a `File` field: bytes uploaded to a pending key before the row commits, orphans swept, bytes deleted after purge |
| External data you act on (calendar blocks, entitlements) | An entity written by a schedule or webhook |
| External data you display (rates, geocoding) | An integration response cache with `keep:` capped by the licence (area 11) |
| Rate limits | Runtime-owned state declared on routes, in a store with `atomic_increment` and `expiry` if declared, else `main`; loss acceptable |
| Presence, typing | Types sent as realtime events (area 12); "who is online" is a lossy entity with `expiry:` |
| Buffered usage counters | Two honest authorities: a counter entity updated with `count + 1`, and a schedule writing `UsageRecord`; both in `main` until load is measured |

## 2. The 33 questions

Area: 5 data, 6 reads/writes, 9 policy, 10 config, 11 integrations, 12 events,
13 testing, 14 tooling.

| # | Question | Answer | Area |
|---|---|---|---|
| 1 | Permission changes | Record caches hold data from before policy, and policy re-runs on every hit. Query caches hold results after policy, keyed by the policy key, with policy inputs as invalidation scopes (C3, C12). | 9 |
| 2 | Result-set invalidation | Scopes from equality filters and policy reads; old and new values; generations (C4, C5). Over-invalidation is accepted. | 5 |
| 3 | Cache keys | Derived, never written by hand. Tenant first. Unkeyable inputs refuse the cache (C1, C2). | 9 |
| 4 | Time-based visibility | The fill computes the next boundary, and the entry expires then. Unknown forms are refused (C9). | 5 |
| 5 | Non-database authorities | Allowed in lossy stores with `expiry:`. No durable references to them, not used to guard writes (P3, X4). Built after caches. | 5 |
| 6 | Capabilities and refusals | Engine table (§1.2) and `store.capability_missing`. | 5 |
| 7 | Placement changes meaning | X1. The error names the placement line. | 5 |
| 8 | Auth-path caches | Never cached: `immediate` reads authority, `bounded` uses signed credentials (C13). | 9 |
| 9 | Erasure reaches every copy | Unreadable at invalidation, gone within `keep:`. Tenant prefix delete. CDN purge is an integration after commit (C14). | 5, 11 |
| 10 | Read-your-writes across requests | `jadpo-revision` header and cookie against the watermark (C10). | 6 |
| 11 | Second authorities disguised as caches | Refused. Model them as two entities and a schedule (§1.9). | 5 |
| 12 | Stored aggregates | Exact: `computed:` in the same transaction. Display: computed cache. Rollups: schedule-written entity (§1.8). | 5, 6 |
| 13 | Which reads may use a copy | The call site decides (§1.7). No settings. | 6 |
| 14 | Outage behaviour | Fall back under a concurrency limit, then `unavailable`. Bypass caches until caught up (C11). | 5 |
| 15 | Never-saved data | Types (P2). Rate limits are runtime-owned. | 5, 10 |
| 16 | External data | Acted on: entity. Displayed: integration cache with a licence cap. | 11 |
| 17 | Policy over search and graph | Projections. Candidates are checked against authority (§1.9). | 9 |
| 18 | Edge reads | Later. Projection into `d1`, only queries that don't depend on the caller, no writes. | 5 (later) |
| 19 | Shapes changing across deploys | Shape and policy digests in keys. Projections rebuild on change (C15). | 5 |
| 20 | Stampede and cold start | Single-flight, fill lock, concurrency limit. No warm-up in v1 (C7). | 5 |
| 21 | Per-tenant or regional placement | One deployment per region. Partitioned stores later (P7). | 5 |
| 22 | Atomic audit | Runtime audit rows in the written store's transaction (X7). | 9 |
| 23 | Unit of caching; CDN | Record, query and computed. Responses are `private, no-store` unless the compiler proves they don't depend on the caller. | 9 |
| 24 | Cross-deployable invalidation | Invalidator per store reads the change record (§1.5). | 12 |
| 25 | Blob stores | `objects` engine and `File` field, later (§1.9). | 5, 11 |
| 26 | Large lists and exports | Not cached. Exports read a read-only snapshot of the authority or a replica. | 6 |
| 27 | Testing | Memory cache engine with the same protocol. Injected lag and outage. A check mode compares every cached read with the authority. The suite passes with caches off. | 13 |
| 28 | Default store at zero config | Implied `main` on postgres from `config.database_url` (S3, S4). | 5 |
| 29 | Read replicas | `replicas:` on the store, under the same watermark rule as caches (S5, C10). | 5, 6 |
| 30 | Who declares a cache | The owner of what it copies, in the same file. Consumers cache their own queries. | 5 |
| 31 | Hot, fast-changing values | `refresh: on_expiry` with a short `keep:`, or no cache plus `count + 1`. | 5, 6 |
| 32 | Immutable data | Recognised automatically, no invalidation (C16). | 5 |
| 33 | Operations | `jadpo cache status, rebuild, flush, verify`. Metrics: hit rate, watermark lag, mismatches found by sampling. | 14 |

## 3. Mapping onto what exists

**CONSISTENCY-001 ([entity-query-model](../../entity-query-model.md) §6–§9)**

| Item | Verdict | Under this model |
|---|---|---|
| One authority per fact; copies never written by the app (§7, §9.1) | **Keep** | P1, §1.6 |
| Change record in the authority's transaction (§9.2 items 1–2) | **Keep** | §1.5, extended |
| At-least-once, idempotent, ordered delivery with replay, watermark, lag and reconciliation (§9.2 items 3–6) | **Keep** | Invalidator, generations, `jadpo cache verify` |
| Local atomic; durable workflow (§9 table) | **Keep** | X1; area 1's subscribers (X2) |
| Durable projection | **Keep, narrowed** | Caches now, projections later |
| Prepared atomic, two-phase commit (§8 rule 6, §9 table) | **Delete** | Not planned. `atomic` across stores stays an error. |
| Four freshness levels on each query (§6) | **Change** | Derived from the call site (§1.7). `eventual` is dropped until projections exist. |
| "Stronger, never weaker"; freshness is not authorisation; revision token (§6) | **Keep** | §1.7, C3, C10 |
| "Persistence is optional; entityhood is not persistence" (§1, §2; DATA-007) | **Reverse** | P1, P2 |
| `persistence { store: role: }`, `cache x { store: from: strategy: delivery: }`, `projection` | **Change / delete** | `store:` section; the new `cache` form; projection syntax removed until an engine exists |

**Compiler today** (read from `jadpo/crates/core/src/target.rs` and area 5's
Status)

| Today | Verdict |
|---|---|
| `__jadpo_changes` with a revision per entity, written in the write's transaction | **Keep.** Add a global sequence and partition values. The `MAX(revision)+1` insert fails under concurrent writes to one entity, and needs checking before invalidation depends on it. |
| Postgres if `DATABASE_URL` is set, SQLite otherwise; tests forced onto SQLite | **Delete.** The engine comes from the store (S2), and tests use the app's engine (area 13). |
| `freshness:` required on every query | **Delete** (§1.7) |
| `cache` and `projection` parse, check and emit nothing; `strategy:` accepts any word | **Delete** now. The new `cache` returns with the Redis adapter. |
| `toolchain.target_store_not_implemented` at build | **Change** to `store.engine_not_available` at check (S6) |
| Indexes derived from query shapes, including the partial composite index for todos | **Keep and reuse.** The same analysis yields invalidation scopes (C4). |
| Migrations with `postgres.sql` and `sqlite.sql` for every app | **Change** to one SQL file per SQL store (§4) |
| Lifecycle runtime evidence on both databases (RM-206) | **Keep.** Each engine remains tested, but no single app uses two engines. |

## 4. Recommended answers for area 5

**Question 2, reworded: where is each entity stored?** *Recommendation:*
every entity is stored in exactly one declared store, `main` unless its
dossier says `store: name`; the `persistence` block, `role:` and
`store: primary` go, and DATA-007's optional persistence is reversed.
Something with identity that is never saved is a `type`. A lossy-store entity
must declare `expiry:` and cannot be referenced by a durable one. An action
writes at most one store; work across stores is a subscriber.
`expiry: { after: since: }` is available in every store.

**Question 3, reworded: what is a cache, and is `projection` kept?**
*Recommendation:* delete today's `cache` and `projection` syntax now. Bring
`cache` back in the §1.6 form (`from:`, `store:`, required `keep:`,
`refresh:`; keys, scopes and time boundaries always derived) together with
the Redis adapter and generation protocol; until then `cache` is a compile
error. `projection` stays a concept and returns with the first `search`,
`graph` or edge engine.

**Question 8: column types.** *Recommendation:* as area 5 recommends, but
each store emits only its own engine's types, so no app needs a two-engine
mapping. Postgres: `uuid`, `boolean`, `numeric(p, s)`, `date`,
`timestamptz(3)`, integer width from the range, structured fields as `jsonb`
in the boundary's JSON. SQLite and D1 keep their mapping (STRICT tables,
`Decimal` as a scaled integer) for apps that choose them. Redis entities and
every cache hold the boundary JSON, where `Decimal` and large `Int` are
strings, so `Money` stays exact (scenario 2). Field rules are checked on write
and read. Engine-only features are allowed where the store has the
capability, such as an exclusion rule against double-booking.

**Question 11: migrations.** *Recommendation:* area 5's shape, with
`jadpo migrate new <purpose>` writing `schema.json`, one `<store>.sql` per SQL
store, and `changes.jadpo` when a decision is needed. `jadpo migrate apply`
applies each store in its own transaction (one batch on D1); a build refuses
to start if any store disagrees. Lossy stores have no migration files, but a
shape change on a lossy entity needs `BrowserSession: discard` in
`changes.jadpo`, because it logs everyone out. Moving an entity between
stores needs `Cart: { move_to: carts }` with `discard` in v1; copying data is
a later area 14 tool. Caches never need migrations (C15).

**Question 14, reworded: where are stores declared?** *Recommendation:* in
`app.jadpo`, `application { stores: { main: { engine: postgres url: config.database_url } } }`,
with `main` on Postgres implied when nothing is declared. `engine:` is a
closed option fixed in source, so each store uses one engine everywhere; the
connection always comes from configuration; tables, columns and indexes come
from entities. Fast Postgres test databases are area 13's question (PGlite
and template databases to evaluate).

**Carry to area 6 (reads and writes):** include syntax across stores (X6)
and reporting reads outside the transaction (X3); how the build shows each
call site's source (§1.7); the `jadpo-revision` contract and whether
non-browser clients must echo it; keyset pagination for cached lists; a
snapshot read form for exports; `count + 1` updates beside `computed:`
fields; `expiry:` as visibility in `query` and `update`.

**Carry to area 9 (policy):** extracting the policy key from a policy and
proving nothing is missing; tenant-first keys and scopes as a compile-time
guarantee (C2); revocation contracts and "nothing authentication reads is
cached"; lossy entities in policy only where a missing record means deny;
no caches for staff reads across tenants; whether the approval review shows
which caches a policy change resets; `Cache-Control` derived from policy
(`private, no-store` unless proved independent of the caller); concealment
on cache hits.

## 5. Hard to reverse, and open questions

**Hard to reverse once apps depend on it**

| Decision | Why it sticks | Recommendation |
|---|---|---|
| An entity is stored in exactly one store; DATA-007 is reversed | Every dossier, migration and report assumes it | Adopt |
| The engine is fixed in source; no dev/prod split | Every app's local setup and the test harness depend on it | Adopt; area 13 must deliver fast Postgres tests |
| An action writes one store | It shapes every workflow; relaxing it later is additive, tightening it later is not | Adopt |
| Change-record format and the global sequence | Invalidation, tokens, replicas and projections all read it | Settle the schema before the first cache ships |
| `jadpo-revision` on the HTTP contract | Clients depend on it | Ship with the first cache, opaque |
| Queries read copies by call site, with no settings | It sets what every existing query call means | Adopt |
| `main` defaults to Postgres | Every small app's development setup | See question 1 below |

**Decided here, reversible:** Redis entities (sessions, carts) come after
caches. Caches carry the highest-ranked value and risk; sessions and carts
work in `main` with `expiry:` meanwhile.

**Open questions for Richard** (each changes what gets built first)

1. **Should the implied `main` use Postgres or D1?** The runtime targets Bun
   and Postgres today, and D1's batch-only transactions may not support area
   3's concurrent-change check (*still needs checking*); your other products
   run on Cloudflare Workers. *Recommendation:* Postgres, with D1 declared
   explicitly once its adapter exists. Choosing D1 makes a Workers runtime and
   the batch-guard proof the first build.
2. **Is "unreadable at once, gone within `keep:` (at most a day)" acceptable
   for legal erasure?** Physically removing query-cache entries at once needs
   an index from each scope to its keys: more storage and code.
   *Recommendation:* accept it, and show the bound in the build report.

Remaining uncertainty (D1 batch aborts, PGlite for tests, the
`MAX(revision)+1` race) affects only its own engine or harness, not these
answers.
