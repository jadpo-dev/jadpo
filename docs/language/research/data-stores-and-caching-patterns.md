# Data stores and caching: what real systems do

**Status:** research input, 2026-10-12. This decides nothing. It feeds
[area 5](../05-data.md) (where entities are stored, `cache` blocks) and the
area 6 question of how a query asks for a copy.

**Labels.** *evidence*: a measurement, incident report or post-mortem.
*documented*: the project's or vendor's own docs. *practitioner*: an
engineering blog, talk, forum thread or issue from people running the thing.
*opinion*: a stated position. Paragraphs marked **Jadpo reading** are this
document's own inference and count as opinion.

## 1. Several stores in one application

### How frameworks say where a model lives

| System | How a model names its store | What holds across stores |
|---|---|---|
| Rails | An abstract base class calls `connects_to` with writing and reading roles, and models inherit from it. The guide says to connect one abstract class per database, not each model, because every connection spec multiplies open connections. ([documented](https://guides.rubyonrails.org/active_record_multiple_databases.html)) | Associations across databases need `disable_joins: true`. Rails then issues separate queries and sorts in memory for ordered or limited queries. The guide says nothing about transactions across databases. ([documented](https://guides.rubyonrails.org/active_record_multiple_databases.html)) |
| Django | Routers decide at run time (`db_for_read`, `db_for_write`, `allow_relation`, `allow_migrate`). They are asked in order, and `default` is the fallback. ([documented](https://docs.djangoproject.com/en/5.1/topics/db/multi-db/)) | Foreign keys and many-to-many relations "must be internal to a single database". Unless a router says otherwise, only relations within one database are allowed. `auth` must live with `contenttypes`. ([documented](https://docs.djangoproject.com/en/5.1/topics/db/multi-db/)) |
| Ecto | The schema doesn't name a store. The call site picks the repo (`Repo.insert`, `Repo.replica().get!`), and replicas are `read_only: true` repos. ([documented](https://ecto.hexdocs.pm/replicas-and-dynamic-repositories.html)) | Not covered. A write inside a transaction is invisible to other connections until it commits, so a replica can't see sandboxed test writes. ([documented](https://ecto.hexdocs.pm/replicas-and-dynamic-repositories.html)) |
| Prisma | `@@schema("name")` on each model, for several schemas inside **one** PostgreSQL, CockroachDB or SQL Server database. SQLite and MySQL aren't supported. ([documented](https://www.prisma.io/docs/orm/prisma-schema/data-model/multi-schema)) A schema file has exactly one `datasource`. ([documented](https://www.prisma.io/docs/orm/reference/prisma-schema-reference)) | Relations and foreign keys work across schemas because they stay inside one database. ([documented](https://www.prisma.io/docs/orm/prisma-schema/data-model/multi-schema)) |
| Ash | `data_layer:` on `use Ash.Resource`. ([documented](https://ash.hexdocs.pm/relationships.html)) Each data layer declares what it can do through `can?/2`: `:transact`, `{:join, resource}`, `{:aggregate, kind}`, `:through_relationship` and so on. ([documented](https://ash.hexdocs.pm/Ash.DataLayer.html)) ETS is "not recommended for production". ([documented](https://ash.hexdocs.pm/Ash.DataLayer.Ets.html)) | A relationship is loaded by filtering the target with keys from the source records. Non-SQL layers rely on manual relationships or calculations. The maintainer describes the goal as "a superset of multiple kinds of storage layers". ([practitioner](https://elixirforum.com/t/need-help-adding-relationship-support-to-sanity-datalayer/59176)) |
| Cloudflare | Each store is a separate binding with its own API. ([documented](https://developers.cloudflare.com/workers/platform/storage-options/)) | Nothing spans stores. Each one has its own consistency model (table below). |

**Jadpo reading.** Every framework here stops at the database boundary for
foreign keys, joins and transactions. None offers a transaction across two
stores. Ash is the one precedent for a planner that asks a store what it
supports and refuses a query the store can't run. That is the shape a
compiler-checked store declaration would take.

### Each store has its own consistency model (Cloudflare as a worked example)

| Store | Consistency | Source |
|---|---|---|
| Workers KV | Eventually consistent. A write "may take up to 60 seconds or more" to show in other locations, and even in the same location this is "not guaranteed". Missing keys are cached too, so a newly created key is as slow to appear as an updated one. | [documented](https://developers.cloudflare.com/kv/concepts/how-kv-works/) |
| D1 | SQLite. Statements in a `batch()` run as one transaction and roll back together if one fails. Outside a batch each statement auto-commits. | [documented](https://developers.cloudflare.com/d1/worker-api/d1-database/) |
| D1 transactions | Interactive `BEGIN`/`COMMIT` is reported as rejected, for example by Drizzle's `transaction()`. | [practitioner](https://github.com/drizzle-team/drizzle-orm/issues/2463) |
| D1 replicas | The Sessions API uses bookmarks to give sequential consistency, including read-your-writes across replicas. Without a session, every query goes to the primary. | [documented](https://developers.cloudflare.com/d1/best-practices/read-replication/) |
| Durable Objects | Strongly consistent. Each object is single-threaded and has one live instance per ID. | [documented](https://developers.cloudflare.com/workers/platform/storage-options/) |
| R2 | Strongly consistent per object. | [documented](https://developers.cloudflare.com/workers/platform/storage-options/) |

Fowler's polyglot-persistence argument is to pick storage by how the data is
used. He names the costs: another interface to learn per store, harder tuning,
and the end of a single source of transactional truth
([opinion](https://martinfowler.com/bliki/PolyglotPersistence.html)).

### "SQLite locally, Postgres in production"

This matters directly, because area 5 currently says "the same project runs on
SQLite locally and PostgreSQL in production".

- Twelve-factor uses this exact pairing as its example of a parity gap: "The twelve-factor developer resists the urge to use different backing services" ([opinion](https://12factor.net/dev-prod-parity)).
- Django: SQLite "has many differences from other databases", so for anything substantial, develop on the production database ([documented](https://docs.djangoproject.com/en/stable/topics/install/)).
- SQLite's own quirks list includes flexible typing, no boolean or datetime type, foreign keys off by default, NULLs allowed in primary keys, double-quoted strings accepted as literals, "dubious SQL" accepted without error, and ASCII-only case folding ([documented](https://www.sqlite.org/quirks.html)). STRICT tables (3.37.0) restore type checking ([documented](https://www.sqlite.org/stricttables.html)).
- SQLite allows one writer at a time ([documented](https://www.sqlite.org/whentouse.html)), so local runs won't surface write-contention and locking behaviour.
- D1 is not plain SQLite either: atomic writes go through `batch()` (above).
- Practitioner reports describe queries that "don't line up" and bugs that only appear in production ([practitioner](https://dev.to/johntellsall/re-using-sqlite-for-dev-and-postgres-for-production-18e4)).

No quantified incident data turned up. *Still needs checking.*

**Jadpo reading.** Most of the usual harm is that hand-written SQL means
different things on each engine. Jadpo emits all the SQL, so it can generate
only what both engines share, turn on STRICT tables and the foreign-key pragma
on SQLite, and run one conformance suite on both. What generation can't
remove is concurrency, isolation, collation and lock behaviour. So the
defensible position is SQLite for fast local loops, with CI and every
acceptance gate on PostgreSQL. "Runs the same" should not be promised.

## 2. Kinds of cache and how frameworks support them

| Kind | How it's supported | What invalidates it |
|---|---|---|
| Per-request memo | Rails' SQL query cache lives for one action ([documented](https://guides.rubyonrails.org/caching_with_rails.html)). DataLoader keeps one instance per request ([documented](https://github.com/graphql/dataloader)). Laravel's `Cache::memo()` ([documented](https://laravel.com/docs/12.x/cache)). | The request ends. |
| Single record | Shopify IdentityCache only reads through the cache when code calls `fetch*`; plain finders are unchanged ([practitioner](https://github.com/Shopify/identity_cache)). Apollo normalises by `__typename:id`, so updating one entity updates every query that references it ([documented](https://www.apollographql.com/docs/react/caching/overview)). | IdentityCache: an `after_commit` hook. Apollo: the mutation's response. |
| Query or result set | Prisma Accelerate takes `cacheStrategy: { ttl, swr, tags }` on each read ([documented](https://www.prisma.io/docs/accelerate/caching)). Hasura's `@cached` caches whole responses ([documented](https://hasura.io/docs/2.0/caching/how-it-works/)). | Accelerate: TTL plus `$accelerate.invalidate(tags)`, which is paid-plan only ([practitioner](https://www.prisma.io/blog/announcing-on-demand-cache-invalidation-for-prisma-accelerate)). Hasura: **TTL only**, with no automatic invalidation when a mutation runs ([documented](https://hasura.io/docs/2.0/caching/how-it-works/)). |
| Lists after a write | Apollo: a new object "isn't automatically added to any list fields". You write an `update` function or use `refetchQueries` ([documented](https://www.apollographql.com/docs/react/data/mutations)). | Manual. |
| Fragment / Russian doll | Rails keys fragments on `model/id-updated_at` plus a template digest. Nested fragments need `touch: true` so a child change expires its parent ([documented](https://guides.rubyonrails.org/caching_with_rails.html)). | A new key (versioned keys, no delete). |
| Aggregate / counter | Rails `counter_cache`. If you add it to an existing table without backfilling, `size` "may return incorrect results", and `reset_counters` repairs drift ([documented](https://guides.rubyonrails.org/association_basics.html)). | Callbacks on the counted rows. |
| Materialised view | Postgres: the data "is not always current" until `REFRESH`. `CONCURRENTLY` needs a unique index ([documented](https://www.postgresql.org/docs/current/rules-materializedviews.html), [documented](https://www.postgresql.org/docs/current/sql-refreshmaterializedview.html)). | An explicit or scheduled refresh. |
| Tagged | Laravel `Cache::tags()`, not supported on the file, database or DynamoDB drivers ([documented](https://laravel.com/docs/12.x/cache)). | Flushing a tag. |
| HTTP / CDN | `Cache-Control`, `Vary`, `stale-while-revalidate` ([RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html), [RFC 5861](https://www.rfc-editor.org/rfc/rfc5861.html)). Fastly `Surrogate-Key` ([documented](https://www.fastly.com/documentation/guides/full-site-delivery/purging/working-with-surrogate-keys/)). Cloudflare `Cache-Tag`, about 1,000 tags in 16 KB ([documented](https://developers.cloudflare.com/cache/how-to/purge-cache/purge-by-tags/)). | Purging by key or tag, or TTL. |
| Client-side (in-process) | Redis tracking sends invalidation messages for the keys a client has read ([documented](https://redis.io/docs/latest/develop/reference/client-side-caching/)). | Server push. |

**Write strategies.** In cache-aside the app reads the cache, falls back to
the store on a miss and fills the cache. On a write it updates the store
*then* deletes the cache entry. Deleting first opens a window in which a
reader can cache the old value. Cache-aside "doesn't guarantee consistency".
Write-through updates both at once, so readers see the new value straight
away. Microsoft advises against caching sensitive data in a shared cache at
all ([documented](https://learn.microsoft.com/en-us/azure/architecture/patterns/cache-aside)).
Facebook deletes rather than sets on write "because deletes are idempotent"
([evidence](https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final170_update.pdf)).

## 3. Invalidation and how it fails

**Invalidation sources.**

- **From the commit log.** Facebook's mcsqueal daemons tail each MySQL commit log and broadcast the deletes. SQL statements carry the keys to invalidate. Only 4% of deletes actually removed a cached item ([evidence](https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final170_update.pdf)).
- **Change data capture.** Debezium can evict a Hibernate second-level cache from the Postgres change stream. It skips the app's own transactions by tracking `txid_current()`, and it accepts "a very short time window" of staleness ([practitioner](https://debezium.io/blog/2018/12/05/automating-cache-invalidation-with-change-data-capture/)).
- **Outbox.** The outbox router uses the event ID to "remove duplicate messages", which implies at-least-once delivery ([documented](https://debezium.io/documentation/reference/stable/transformations/outbox-event-router.html)).
- **TTL only.** Hasura's default is 60 s ([documented](https://hasura.io/docs/2.0/caching/how-it-works/)), and Redis advises a maximum TTL even on tracked keys as a guard against bugs ([documented](https://redis.io/docs/latest/develop/reference/client-side-caching/)).
- **Lost invalidations happen.** IdentityCache says it "is never going to be 100% consistent, since cache invalidations can be lost" ([practitioner](https://github.com/Shopify/identity_cache)).

**Failure modes.**

| Failure | Mechanism | Mitigation seen in practice |
|---|---|---|
| Stale set (fill races invalidation) | A fill reads x=42, x becomes 43, the invalidation lands first, then the late fill writes 42, and nothing corrects it. | Facebook's leases are a 64-bit token per miss, voided by a delete ([evidence](https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final170_update.pdf)). Version numbers reject older fills, but that fails if the newer entry was already evicted ([evidence](https://engineering.fb.com/2022/06/08/core-infra/cache-made-consistent/)). Redis uses a "caching-in-progress" placeholder ([documented](https://redis.io/docs/latest/develop/reference/client-side-caching/)). |
| Thundering herd / stampede | A hot key is invalidated or expires, and every reader recomputes it. | Leases issue one token per key per 10 s while other readers wait. The peak database rate fell from 17K/s to 1.3K/s ([evidence](https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final170_update.pdf)). Rails `race_condition_ttl` ([documented](https://guides.rubyonrails.org/caching_with_rails.html)). Laravel atomic locks and `Cache::flexible` stale-while-revalidate ([documented](https://laravel.com/docs/12.x/cache)). XFetch probabilistic early expiry ([evidence](https://mail.vldb.org/pvldb/volumes/8/paper/Optimal%20Probabilistic%20Cache%20Stampede%20Prevention)). |
| Read-your-writes after a write | A replica or cache lags behind the primary. | Rails sends a user's reads to the writer for 2 s after they write ([documented](https://guides.rubyonrails.org/active_record_multiple_databases.html)). GitLab sticks reads to the primary for up to 30 s, or until the replicas catch up ([documented](https://docs.gitlab.com/administration/postgresql/database_load_balancing/)). D1 uses bookmarks. Facebook uses remote markers ([evidence](https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final170_update.pdf)). In TAO the leader returns a changeset to refresh the writer's tier ([evidence](https://www.usenix.org/system/files/conference/atc13/atc13-bronson.pdf)). |
| Inconsistent invalidation across result sets | A record changes but the lists, counts and parents that contain it don't. | Apollo lists need manual updates. Rails needs `touch: true`. Hasura falls back to TTL (all in the table above). |
| Negative caching | "Not found" is cached, so a new record stays invisible. | KV caches missing keys ([documented](https://developers.cloudflare.com/kv/concepts/how-kv-works/)). IdentityCache warns not to pass uncommitted IDs to jobs ([practitioner](https://github.com/Shopify/identity_cache)). |
| Invalidation feedback loop | In the 2010 Facebook outage, clients treated database errors as invalid values and deleted cache keys. That sent more queries to an already failing cluster, and the site was down for about 2.5 h ([evidence](https://engineering.fb.com/2010/09/23/uncategorized/more-details-on-today-s-outage/)). | Treat errors as distinct from missing data, and cap invalidation. |
| Cold start | An empty cache sends the full load to the store. | Facebook's cold-cluster warm-up uses a 2 s hold-off on deletes, and Gutter pools absorb failed servers ([evidence](https://www.usenix.org/system/files/conference/nsdi13/nsdi13-final170_update.pdf)). |
| Cache poisoning | An unkeyed input such as `X-Forwarded-Host` changes the response, and that response is stored for everyone. | Key every input, or strip it. "The most robust defense against cache poisoning is to disable caching" ([practitioner](https://portswigger.net/research/practical-web-cache-poisoning)). |

Meta measures consistency from the client side. Its Polaris tool raised TAO
from six to ten nines of cache writes consistent within five minutes. It
calls cache inconsistency "almost as bad as data loss"
([evidence](https://engineering.fb.com/2022/06/08/core-infra/cache-made-consistent/)).

## 4. Authorisation and shared caches

### Incidents

| Incident | What happened | Source |
|---|---|---|
| Steam, 25 Dec 2015 | During a DoS attack, a caching configuration "incorrectly cached web traffic for authenticated users". Store pages for about 34k users were served to others for 90 minutes, showing billing addresses, purchase history, emails and partial card numbers. | [evidence](https://store.steampowered.com/news/19852/) |
| Railway, 30 Mar 2026 | A CDN configuration change turned on caching for domains that had the CDN off. GET responses on about 0.05% of those domains were cached for 52 minutes, and users "may have seen pages intended for other users". | [evidence](https://blog.railway.com/p/incident-report-march-30-2026-authenticated-user-data-cached) |
| ChatGPT, 20 Mar 2023 | A redis-py bug: cancelled requests corrupted pooled connections, which returned another user's cached data. Chat titles were exposed, plus payment details for 1.2% of active Plus subscribers in a nine-hour window. OpenAI's fix added checks "that the data returned by our Redis cache matches the requesting user". | [evidence](https://openai.com/blog/march-20-chatgpt-outage), reported by [The Hacker News](https://thehackernews.com/2023/03/openai-reveals-redis-bug-behind-chatgpt.html) |
| GitHub, Mar 2021 | Not a cache, but the same class of bug: a background thread kept a reference to an earlier request's environment hash, which Unicorn reused, and wrote one user's session cookie into another user's response. | [evidence](https://github.blog/security/vulnerability-research/how-we-found-and-fixed-a-rare-race-condition-in-our-session-handling/) |
| Web cache deception (PayPal and others, 2017) | `/account/x.css` is served dynamically by the origin, and the CDN caches it as static. | [practitioner](https://portswigger.net/web-security/web-cache-deception) |

Klarna's May 2021 exposure is often described as a CDN caching fault, but
Klarna's own report, as quoted by others, says "configuration change". Not
verified as a cache incident ([practitioner](https://www.verdict.co.uk/klarna-incident/)).

### Why the defaults don't protect you

- RFC 9111 stops a shared cache from reusing a response to a request with an `Authorization` header, unless `public`, `s-maxage` or `must-revalidate` allows it ([documented](https://www.rfc-editor.org/rfc/rfc9111.html)). Cookie-authenticated requests get no such default. **Jadpo reading:** that gap is the one Steam's configuration fell through.
- Cloudflare doesn't cache a response that has `Set-Cookie`, or `Cache-Control: private`/`no-store`/`max-age=0`. HTML and JSON aren't cached by default, but a "cache everything" rule overrides that ([documented](https://developers.cloudflare.com/cache/concepts/default-cache-behavior/)).
- Django's `cache_page` "does not automatically vary by authentication, sessions, cookies". The docs warn that URL-keyed caches "could expose incorrect or sensitive data" ([documented](https://docs.djangoproject.com/en/stable/topics/cache/)).
- DataLoader: "It may be dangerous to use one cache across many users" ([documented](https://github.com/graphql/dataloader)).

### Patterns that work

1. **Cache data before authorisation and check on every read.** TAO caches nodes and edges, which are shared. It runs privacy checks "each time the content is viewed", because "the results of privacy checks cannot" be reused ([evidence](https://www.usenix.org/system/files/conference/atc13/atc13-bronson.pdf)).
2. **Put what policy reads into the key.** Hasura's key includes the role and only the session variables "used in the permissions of the query" ([documented](https://hasura.io/docs/2.0/caching/how-it-works/)).
3. **Mark responses `private` or `no-store`, and `Vary` on what they depend on** ([documented](https://www.rfc-editor.org/rfc/rfc9111.html), [documented](https://docs.djangoproject.com/en/stable/topics/cache/)). Also make sure CDN rules respect the origin's `Cache-Control` ([practitioner](https://portswigger.net/web-security/web-cache-deception)).
4. **Check the owner on the way out.** OpenAI's fix, above.

## 5. Consistency guarantees and how they're expressed

| Guarantee | Meaning | How systems express it |
|---|---|---|
| Strong / linearisable | You read the latest committed write. | Cosmos *Strong*; DynamoDB `ConsistentRead: true`, which costs twice an eventual read and isn't available on GSIs or streams ([documented](https://learn.microsoft.com/en-us/azure/cosmos-db/consistency-levels), [documented](https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/HowItWorks.ReadConsistency.html)). |
| Bounded staleness | At most K versions or T seconds behind. | Cosmos minimums are 10 writes or 5 s in one region, and 100,000 or 300 s across regions ([documented](https://learn.microsoft.com/en-us/azure/cosmos-db/consistency-levels)). A cache TTL bounds age only if invalidations can't be lost. |
| Session (read-your-writes, monotonic reads) | Your own writes are visible to you. | Cosmos session tokens, D1 bookmarks, Rails and GitLab sticky reads. Cosmos calls Session "the most widely used" level ([documented](https://learn.microsoft.com/en-us/azure/cosmos-db/consistency-levels)). |
| Consistent prefix | You never see writes out of order. Transactional batches appear together. | Cosmos ([documented](https://learn.microsoft.com/en-us/azure/cosmos-db/consistency-levels)). |
| Eventual | No bound. | DynamoDB's default, KV. Cosmos suggests it for counts like likes ([documented](https://learn.microsoft.com/en-us/azure/cosmos-db/consistency-levels)). |

**Jadpo reading.** These are the right words for cached reads. Each one is a
property of the read, not of the cache. Cosmos and DynamoDB let a single
request ask for a stronger level than the default, so the guarantee goes on
the read, not on the entity.

## 6. Design principles

1. **One authority per entity, and every other store is a copy.** No framework surveyed gives transactions or foreign keys across stores (§1). Area 5's "never a second place to write" matches every precedent.
2. **A store declares its capabilities, and the compiler refuses queries the store can't run.** Ash's `can?/2` is the precedent. The D1 batch-only and KV models show how far stores diverge (§1).
3. **Develop on one engine, but gate on the production engine.** Twelve-factor, Django and SQLite's quirks all point this way (§1). Generate only the shared SQL, with STRICT tables and foreign keys on.
4. **Store data before authorisation and run policy on every read, including cache hits.** TAO (§4). The policy decision itself is never cached in a shared cache.
5. **If a shared cache holds results after authorisation, its key includes every input the policy reads, and nothing else.** Hasura (§4). Extra inputs fragment the cache, and missing ones leak data.
6. **Responses are `private, no-store` unless the compiler proves they don't depend on the principal.** Steam and Railway (§4) show that CDN configuration fails open.
7. **Check ownership at the boundary as a second line of defence.** ChatGPT's fix (§4).
8. **Delete on write, never set, and fill under a lease or version.** Facebook's leases and Meta's versioning (§3).
9. **Invalidate from the commit, not from application code.** mcsqueal and Debezium (§3). IdentityCache's lost invalidations show that hooks miss writes.
10. **Derive invalidation from read dependencies.** A cached list, count or parent must be invalidated by writes to every entity it read. Apollo, Rails `touch` and Hasura's TTL-only approach show what happens when this is manual (§2–3).
11. **Every cached read states its guarantee:** strong, session or bounded(T), using the Cosmos vocabulary (§5). Session is the default after the caller's own writes (Rails, GitLab, D1).
12. **Coalesce misses and serve stale data while revalidating.** Leases cut the peak database load 13× (§3).
13. **Cache "not found" only with a short TTL and with invalidation on create** (KV, IdentityCache, §3).
14. **Keep a TTL even on invalidated entries** as a bound on bugs (Redis, IdentityCache, §3).
15. **Measure consistency in production,** as Polaris does (§3). It's the only way to know the guarantee holds.

## 7. Risks a compiler-managed cache must handle itself

**Jadpo reading**, each tied to the evidence above:

- **Principal leakage.** The compiler sees the policy, so it can compute the cache key's principal and tenant inputs. It should refuse a shared cache of post-authorisation results that lacks them. *Steam, Railway, Hasura.*
- **Policy drift.** When a policy changes, every entry cached under the old one must be invalidated. Include a policy version in keys. *Inference from the Hasura key design, unverified elsewhere.*
- **Invalidation reach.** For each cached query, derive the set of entities and fields it read, including joins, aggregates and filters. Invalidate on any committed write to them, including deletes and inserts that newly match a filter. *Apollo lists, Rails `touch`.*
- **Stale fills.** Generate the lease/version protocol, not a plain get-then-set. *Meta and Facebook.*
- **Ordering.** Delete after commit, never before, and never inside an uncommitted transaction. Skip cached reads inside a transaction. *Microsoft, IdentityCache.*
- **Read-your-writes.** After a write, the same session's reads bypass or wait for copies. *Rails, GitLab, D1, Cosmos.*
- **Lost invalidations.** Use an outbox or log-driven pipeline with at-least-once delivery, plus a maximum TTL. *Debezium, IdentityCache, Redis.*
- **Stampede and cold start.** Coalesce misses and add early expiry. *Facebook leases, XFetch.*
- **Errors aren't values.** Never cache, or invalidate because of, a failed store read. *Facebook 2010.*
- **Unkeyed inputs.** Every input that shapes the response, including headers, locale and feature flags, is in the key or rejected. *Cache poisoning, web cache deception.*
- **Engine divergence.** Generated SQL stays in the shared subset, and the production engine runs in CI. *SQLite quirks, D1.*
- **Shared state between requests.** Request-scoped memos are created per request and never shared across principals. *DataLoader, GitHub 2021.*
