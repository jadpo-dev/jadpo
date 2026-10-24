# Data store and caching scenarios

**Purpose:** test cases a Jadpo model of stores, placement and caching must
survive. Input to area 5 open questions 2, 3 and 14 and to area 6's cache
reads; it does not propose syntax.
**Status:** research, 2026-10-12. The six applications are those in
[layout scenarios](layout-scenarios.md); the data needs below are designed
estimates, not measured workloads.

## The direction being tested

Richard's proposal, not a decision: an application declares one or more
stores (postgres, sqlite, d1, redis, later graph); each entity lives in
exactly one store, with a default implied; engines declare capabilities and
the compiler refuses what a store cannot honour; a cache is a copy of
authoritative data in another store, never written directly and kept correct
by the compiler after commits; writes read the authority, plain display
queries may read caches. Caches must obey the same authorisation rules and be
wiped or updated when data changes.

Fixed points it has to fit:

- Entity model §6, §9: one authority per fact;
  freshness levels authoritative, read-your-writes, bounded, eventual; the
  compiler may pick a stronger source, never a weaker one; freshness is not
  authorisation, so derived data is revalidated against authority unless its
  revocation contract is proved; durable projection uses a change record in
  the authority's transaction.
- [Area 3](../03-logic.md) question 15: every action is all-or-nothing and
  reads are current unless they read a cache, declared where the cache is.
- Authentication plan: no security or
  availability guarantee depends on a cache being present.

Three tensions are already visible. [Area 5](../05-data.md) question 2
recommends that `entity` means "stored in the application's database", while
the direction allows entities that live only in Redis or are never saved.
Question 3 recommends deleting cache syntax until an adapter exists.
Question 14 recommends one `database:` in `app.jadpo`, which a multi-store
declaration replaces.

Terms used below: **placement** is which store holds an entity's authority;
a **cache** is a same-shape copy; a **projection** is a reshaped copy (search
index, graph, edge replica, materialised total); **ephemeral** data has its
authority in a store that may lose it (TTL, eviction); **external** data is
owned by another system and copied in.

---

## Scenario 1: Golden todo

The model must cost nothing here: four entities, no store declarations.

| Need | Shape | What it tests |
|---|---|---|
| All four entities in the default store; SQLite locally, PostgreSQL live | Placement, implied | A zero-config default; question 14's single engine |
| Rate limit on `/auth/exchange` | Ephemeral counter, never saved | Whether non-entity counters exist in the model at all |
| ServiceCredential lookup on every authenticated request | Single-record cache | `immediate` revocation forbids serving a revoked key from cache; `bounded` permits a declared delay |
| `GET /todos` cursor pages per owner | Per-user list cache | One new todo shifts every later page; the owner must see it on the next list (read-your-own-writes) |
| Soft-deleted todo | Visibility change | A cached list must drop it on delete; purge must remove every copy, not just the row |
| Overdue todos | Time-based membership | Becomes overdue with no commit, so commit-driven invalidation never fires |
| `User.with_todos` | Cross-entity cached result | Invalidated by Todo writes although the query is owned by User |
| Tests | Cache absent vs present | The same suite must pass with no cache, and prove cache equals authority |

---

## Scenario 2: E-commerce

| Need | Shape | What it tests |
|---|---|---|
| Guest Cart and CartLine | Placement: Redis with TTL is tempting | Checkout reads cart and reserves stock atomically; a Redis cart turns checkout into a durable workflow, so moving one entity changes far-away actions |
| StockReservation with expiry | Primary, time-limited | Must share InventoryItem's transaction; expiry by job, not eviction |
| WebhookReceipt dedupe | Primary, or Redis `SET NX` | A Redis dedupe lost before the payment write commits replays or drops a payment |
| Login and checkout rate limits | Ephemeral counters | Loss is acceptable; never read by a write |
| `product_listing` by category, filter, sort, page | Shared list cache, many result sets | One price change invalidates every listing containing that product across every filter combination |
| `product_detail` during a flash sale | Hot shared record | Stampede when the entry expires; cold cache after deploy |
| "Only 3 left" stock badge | Hot aggregate | Bounded staleness for display, authority at checkout; changes too often for invalidation to pay |
| Customer-group price lists | Shared cache, segmented | The key must include the group, or one group's price leaks to another |
| Draft and unpublished products | Policy on shared cache | Admin and storefront call the same query; a draft must never enter the public copy |
| Product search and facet counts | Projection to a search engine | Policy on results; facets counted before or after visibility rules |
| `order_history` after checkout | Per-user list | Confirmation page then order list must show the new order |
| `daily_sales`, `low_stock_report` | Computed and stored (nightly rollup) | Is a rollup a projection the compiler maintains, or an entity a job writes? |
| Tax rates, shipping quotes, exchange rates | External data with TTL | No commit to invalidate on; authority is the provider |
| Cached `Money` values | Encoding | Exact decimal must survive the cache round trip (area 5 already found SQLite loses it) |

---

## Scenario 3: Multi-tenant B2B SaaS

| Need | Shape | What it tests |
|---|---|---|
| Every entity scoped to Organisation | Tenant isolation | Cache keys must carry the tenant by construction; a key bug is a cross-tenant breach |
| Member removed, team changed, CustomRole edited | Permission change | `project_board` cached for that user still lists projects; one role edit affects every member holding it. The invalidation source is a policy input, not the cached entity |
| `browser_session` | Placement: Redis or primary | Losing a session logs the user out (safe); losing a revocation in a deny-list design is unsafe |
| ImpersonationSession | Primary only | Must be auditable and durable; refusing an ephemeral store for it |
| ApiKey lookup per request; `rotate_api_key` | Single-record cache on the auth path | Revocation bound versus `immediate` mode |
| OrganisationSettings on every request | Hot shared-per-tenant record | Toggling "SSO required" is a security change that must apply now |
| UsageRecord API-call counters | Redis increment, flushed by `aggregate_usage` | Redis is the authority for unflushed counts; loss is lost revenue. A second authority disguised as a cache |
| AuditEntry on nearly every action | Placement: primary or a separate append store | In the same transaction it is atomic; in another store every action becomes a durable workflow |
| `seat_usage`, member and project counts | Stored aggregates | Must agree with authority for billing; maintained incrementally or recomputed |
| `staff_org_search` across tenants | Cross-tenant read | Tenant-scoped caches cannot serve it; staff policy differs |
| EU tenant data in an EU database | Placement by tenant | "Each entity lives in exactly one store" breaks when Task lives in one of N regional stores |
| `delete_organisation` | Erasure | Every cache, projection and search entry for the tenant must go, provably |
| Back-office and public API deployables | Shared caches | A back-office write must invalidate the public API's caches |
| Plan entitlements from BillingProvider | External data | Cached for every request; webhook-driven refresh |

---

## Scenario 4: Booking marketplace

| Need | Shape | What it tests |
|---|---|---|
| `search_listings` by place, dates, guests, price | Projection with geo and availability | Eventually consistent display, but a booking must check authority; how stale may "available" be? |
| `listing_calendar`, `quote_stay` | Computed per listing and date range | Key explosion by range; invalidated by PricingRule, AvailabilityBlock and Booking writes |
| Booking request holds (`purge_stale_holds`) | Primary with expiry | Double-booking prevention needs a constraint in one transaction; a Redis lock is tempting and must be refused as a correctness mechanism |
| ExternalCalendar sync every 30 minutes from host URLs | External data stored as blocks | A copy of another system that also blocks bookings: cache, entity, or authority? |
| Typing indicators, online presence | Ephemeral, never saved | Not an entity, not a cache; has no authority anywhere else |
| Inbox unread counts | Per-user hot aggregate | Every message write updates two users' counts |
| `recompute_rankings` | Computed and stored leaderboard | Sorted set in Redis rebuilt by job: projection, or derived entity? |
| `review_summary` with blind reviews | Stored average, time-based visibility | Reviews hidden until both submit or the window closes; the cached average must not include hidden ones |
| Host dashboard and earnings | Per-host aggregates in money | Exact totals; payouts read authority |
| Listing photos and renditions | Metadata in primary, bytes in object storage | Blob store as a store kind; metadata row exists before the rendition does |
| Geocoding results | External data with licence limits | Provider terms cap how long results may be kept |
| Public `.ics` calendar feed | HTTP or CDN caching | Whether response caching (`Cache-Control`) is in the model and derived from policy |
| "Today" per listing time zone | Time boundaries | Expiry computed from a zone, not UTC midnight |

---

## Scenario 5: Editorial content

| Need | Shape | What it tests |
|---|---|---|
| `article_by_slug`, `section_front` for anonymous readers | Shared, very hot; edge copy in D1/KV or CDN | A projection into a store with weaker capabilities and replica lag; which queries may run at the edge |
| Scheduled publish and embargo lifting | Time-based visibility | A copy built before 09:00 must not show the article early and must show it at 09:00; expiry derived from the next visibility boundary |
| Unpublish or legal takedown | Erasure with a deadline | Must leave every cache, search index, edge copy and CDN within a stated bound; CDN purge is an external call after commit |
| Search projection with an external mapping | Projection; `reconcile_search_index` | A mapping change is a migration of a projection: rebuild, dual-write window, cut-over |
| Drafts, review queue, preview tokens | Policy on shared caches | Preview must bypass the public copy; a draft must never be written into it |
| ArticleVersion, `version_diff` | Immutable | Cacheable forever if the compiler proves immutability; large values |
| SlugRedirect | Hot, rarely changes | Slug change must invalidate the old slug and add a redirect together |
| Retagging or moving an article | Fan-out invalidation | One write touches tag pages, section fronts, author page, sitemap and RSS |
| Sitemap and RSS | Large computed output | Rebuilt artefact, not a query result; thousands of entries |
| `editorial_dashboard` counts by state | Per-role aggregate | Different counts per role; same query |
| Editor saves then reloads | Read-your-own-writes | Across two requests, needing a revision token |
| MediaAsset uploads and transcodes | Metadata plus blobs | Asset visible before renditions exist |

---

## Scenario 6: 72-entity platform

| Need | Shape | What it tests |
|---|---|---|
| Ledger in PostgreSQL, Session in Redis, catalogue in search, events in a warehouse | Several authorities | About 20 cross-domain workflows: the compiler must refuse `atomic` across stores and require durable workflows |
| LedgerAccount balances | Computed and stored from LedgerEntry | Must be exact and authoritative for postings; display may read a copy |
| Session with TTL; Consent durable | Ephemeral versus durable in one domain | Placement decided per entity with different loss tolerance |
| Price by PriceBook, segment, currency and time | Segmented, time-based cache | Promotion start and end; key dimensions grow multiplicatively |
| Partner rate limits per ApiClient | Ephemeral counters | Shared across four deployables |
| Feed and ExportJob over millions of rows | Large list | Needs a consistent snapshot or replica, not a cache |
| PostgreSQL read replica | Same-engine copy with lag | Is a replica a cache under the same freshness rules? |
| Four deployables | Store connections and invalidation | Each deployable connects to some stores; a worker's write must reach the API's caches |
| Rolling deploy with v1 and v2 builds | Cached shape change | Two builds share Redis with different shapes; keys versioned by schema digest; area 5's retire-then-drop meets caches |
| Redis outage at peak | Cache failure | Fall back to authority without overloading it; typed availability failure; no guarantee depends on the cache |
| Cache flush or cold region | Cold start | Warm-up and request coalescing |
| Fraud and recommendation graph | Projection to graph; later graph as authority | Grant and Role in a graph for authorisation would make policy read a projection |
| ERP stock levels | External authority mirrored | Reconciliation against a system Jadpo does not own |
| Storefront catalogue at the edge | Edge projection | Policy evaluation at the edge; writes routed to origin |
| Team A caches Team B's entity | Cache ownership | Declared in the owning dossier or by the consuming query? |
| 2,500 tests | Testing | Deterministic cache adapter, injected lag, outage tests, cache-equals-authority checks |
| Operations | Observability | Lag, hit rate, rebuild and reconciliation commands per cache |

---

## The hardest data and caching questions, ranked

Ranked by how many scenarios hit each and how badly a wrong answer fails:
data leaks and lost writes rank above slow pages.

| # | Question | Scenarios | Why it is hard |
|---|---|---|---|
| 1 | **Permission changes.** When a membership, role or tenant changes, which cached results become wrong, and are caches stored before policy (re-filtered per request) or after (per principal)? | 1–6 | The trigger is a policy input, not the cached entity; pre-policy wastes work, post-policy multiplies copies |
| 2 | **Result-set invalidation.** Which cached lists, pages, searches and totals does one write affect? | 1–6 | The compiler sees predicates, so it can compute this, but precision versus over-invalidation and page shifts are unsolved |
| 3 | **Cache keys** derived from tenant, principal and segment by the compiler, never authored, with proof of no cross-tenant reuse | 2, 3, 5, 6 | A single wrong key is a breach; segmentation multiplies entries |
| 4 | **Time-based visibility.** Embargoes, overdue items, promotion windows, review windows change results with no commit | 1, 2, 4, 5, 6 | Commit-driven invalidation never fires; expiry must come from the next boundary in the predicate |
| 5 | **Non-database authorities.** May an entity live only in Redis (session, cart, counter), and what loss does that admit? | 2, 3, 4, 6 | Contradicts area 5 question 2; loss tolerance must be declared and checked |
| 6 | **Engine capabilities and refusals.** Which capabilities (transactions, unique, range, ordering, TTL, durability, isolation) are listed, and what is refused where? | 1–6 | Defines the whole store model; too coarse admits bugs, too fine is unusable |
| 7 | **Placement changes meaning.** Moving Cart to Redis turns checkout from atomic into a durable workflow | 2, 3, 6 | A one-line placement edit causes compile errors far away; must be explained, not mysterious |
| 8 | **Auth-path caches.** Credentials, sessions and tenant settings under `immediate` and `bounded` revocation | 1, 3, 6 | Highest-traffic reads; any lag is a security property |
| 9 | **Erasure reaches every copy** within a stated bound: purge, org deletion, takedown, including search, edge and CDN | 1, 3, 5, 6 | Legal deadlines; CDN purge is an external effect after commit |
| 10 | **Read-your-own-writes across requests.** How a route carries a revision token, and what the second device sees | 1, 2, 3, 5 | Inside one action it is solved; across requests it needs client cooperation |
| 11 | **Second authorities disguised as caches.** Buffered counters flushed by a job are authoritative until flushed | 3, 4, 6 | The cache rule "never written directly" is broken by design; needs its own name or a refusal |
| 12 | **Stored aggregates.** Counts, money totals, averages, leaderboards: incremental or recomputed, exact or approximate, projection or entity field? | 2, 3, 4, 5, 6 | Billing and ledger totals must be exact; dashboards need not be |
| 13 | **Which reads may use a copy.** How the compiler knows a query is "plain display" when the same query serves a route and a workflow | 1–6 | Area 3 puts the choice on the cache; area 6 must decide per query or per call |
| 14 | **Outage behaviour.** Fall back to authority, shed load, or fail with a typed error | 1–6 | Fallback can overload the authority the cache protected |
| 15 | **Ephemeral, never-saved data.** Presence, typing, rate limits: entity, value, or a separate kind? | 2, 3, 4, 6 | No authority to copy from; fits neither entity nor cache |
| 16 | **External data.** Rates, tax, geocoding, entitlements, iCal: TTL, refresh, licence limits | 2, 3, 4, 5, 6 | No commit signal; authority is outside Jadpo; terms limit retention |
| 17 | **Policy over search and graph projections** with their own query capabilities, without raw queries | 2, 4, 5, 6 | Candidates come from the projection; release needs revalidation against authority |
| 18 | **Edge reads** from D1/KV replicas: which queries and policies run there, and how lag is bounded | 5, 6 (2, 4) | Different engine capabilities and no writes at the edge |
| 19 | **Cached shape changes** across a rolling deploy and projection reindexing | 5, 6 (all) | Two builds share one cache; area 5's two-step removal must extend to copies |
| 20 | **Stampede and cold start.** Coalescing, warm-up, flash sales | 2, 4, 5, 6 | Runtime concern the compiler must still generate correctly |
| 21 | **Per-tenant or regional placement.** One entity across N stores of the same schema | 3, 6 | Breaks "exactly one store" |
| 22 | **Atomic audit.** Audit in the action's transaction or in a separate store | 3, 6 | Separate store makes every action a durable workflow |
| 23 | **Unit of caching.** Record, query result, or HTTP response; is CDN caching in the model? | 2, 4, 5 | Response caching bypasses policy unless derived from it |
| 24 | **Cross-deployable invalidation.** A worker or back-office write reaching another deployable's caches | 3, 6 | Delivery must cross process and deployable boundaries |
| 25 | **Blob stores.** Object storage as a store kind; metadata and bytes consistency | 4, 5, 6 | Not rows; upload completes outside any transaction |
| 26 | **Large lists and exports.** Cursor caching, deep pages, consistent snapshots | 2, 3, 5, 6 | Caching deep pages is waste; exports need snapshots |
| 27 | **Testing.** Deterministic adapter, lag and outage injection, cache-equals-authority checks | 1–6 | Without it, cache bugs surface only in production |
| 28 | **Default store at zero config**, and where stores are declared (replacing question 14's `database:`) | 1–6 | The small app must not pay for the large one |
| 29 | **Read replicas** as same-engine copies under freshness rules | 3, 6 | Replica lag is the commonest staleness in practice |
| 30 | **Who declares a cache.** The owning dossier or the consuming query, across teams | 2, 6 | Area 3 says "where the cache is"; ownership at scale disagrees |
| 31 | **Hot, fast-changing values.** Stock badges, view counters, unread counts | 2, 4, 6 | Invalidation costs more than the read it saves |
| 32 | **Immutable data cached forever.** Versions, ledger entries | 5, 6 | Cheap win if the compiler can prove immutability |
| 33 | **Operations.** Lag, hit rate, rebuild and reconciliation per copy | 6 (all) | §9.2 promises them; nothing yet generates them |

Questions 1–10 bite in most scenarios and decide whether the model is safe;
they should be settled before any cache syntax returns. Questions 5, 6, 7 and
21 decide the store model itself and are the most expensive to reverse once
applications depend on them.
