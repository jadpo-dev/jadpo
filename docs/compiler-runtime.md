# Jadpo compiler and runtime architecture

**Status:** the build approach is decided (§1.1–1.2, 2026-10-13); the
architecture (§2) was written in roadmap chunk 0.4 and records what each
chunk has built since.

## 1. Architecture follows the semantic model

The implementation should not begin with a polished grammar. The intended
pipeline is conceptually:

```text
source
  -> parser
  -> syntax tree
  -> semantic model
  -> type and boundary validation
  -> policy checker
  -> effect/audit graph
  -> generated artifacts
  -> executable target
```

The valuable component is the semantic model and its checks. The parser and
target are replaceable implementation choices.

### 1.1 Graph-centred compiler (decided, 2026-10-13)

**The v0 compiler (read 2026-10-13, now archived):** it built a semantic
graph (`build_semantic_graph` in `jadpo/crates/semantic/src/lib.rs`) of
declarations, call edges, refinements and external effects, keyed by name,
with no control flow inside callables. It fed diagnostics, the language
service and approval reachability, which is why the approval report called
its paths "may-call, not feasible". Code generation (`core/src/target.rs`,
about 14,000 lines) read the syntax tree directly and consulted the graph in
about ten places. So the graph and the generated program were two separate
readings of the source.

**Options:**

1. **Graph as a by-product (v0).** No rewrite, and golden-todo progress
   continues. But the graph can disagree with what runs, and every
   graph feature (source maps, spans named by node, path IDs, thresholds,
   `why`, path coverage) is added once to the graph and again to the
   generator. A second target (E09) starts again from the syntax tree.
2. **One typed graph that everything reads.** Parse, resolve and type-check
   into a graph of declarations, edges and per-callable control flow, each
   node with a stable identity; code generation, reports, the language
   service, source maps and path numbering all read it. What the review
   shows is what runs, by construction, and each new feature is written
   once. Costs: moving the generator onto it, keeping the passing golden
   cases green throughout, and the risk of fixing the graph's shape while
   areas are still in review.
3. **The graph as the source** (Unison's content-addressed code). Rejected:
   models, git and reviewers work in text.

**Decided (2026-10-13, §1.2):** option 2, built as a new compiler core
rather than by moving the existing generator over.

Why a rebuild rather than a migration (read, 2026-10-13):

- The language review's decisions for areas 1–4 alone change nearly every
  syntax form, so the parser and type checker would be largely rewritten
  in rework anyway.
- The v0 generator read the syntax tree, the opposite of option 2; writing
  a generator for the graph is less work than moving 14,000 lines onto it.
- The v0 compiler was tuned to the golden app: the formatter hard-coded the
  reminder payload fields (`core/src/formatter.rs`, `todo_title`,
  `due_at`), and only the golden service was accepted, matched line by
  line. Its passing golden cases overstated what worked.

What the rebuild keeps: the language specifications, whose compiled
examples and error coverage (`tools/language_docs.py`) become the feature
tests; the golden contract and `tools/verify.py`; the diagnostics
catalogue; and the v0 runtime (outbox and delivery, JWT
authentication, migration identity) as reference design. The old compiler
was archived before the rebuild began (roadmap chunk 0.2, tag
`archive/compiler-v0`).

How it is built, so it does not repeat the failure:

1. Feature by feature from the area specifications; each area's examples
   and errors pass before the next. The golden todo is the final
   integration check, not the target being steered towards.
2. A verifier check fails when compiler source contains identifiers from
   the example applications, catching special cases mechanically.
3. The graph is the spine from the first commit: parse, resolve and
   type-check into the graph with stable identities; everything else reads
   the graph.

**Features the graph must be designed for.** These are proposals in the
language areas, and the graph's shape has to carry them even where they
are built later:

| Feature | Needs from the graph | Where |
|---|---|---|
| `.jadpo` source maps and mapped stacks | Source range per node, generated line per node | Area 17 question 1 |
| Incidents across builds | Stable identity for actions, queries, routes | Area 17 question 2 |
| Spans named by declaration | Node per route, action, query, integration call, delivery | Area 16 question 10 |
| Path IDs | Control flow inside callables; stable branch identity; loop and recursion rule | Area 16 question 13 |
| Threshold-driven telemetry and generated monitoring rules | Thresholds attached to nodes and paths; build manifest | Area 16 questions 14–16 |
| `jadpo why` | Policy, lifecycle, concealment and cache decisions as nodes with sources | Area 17 question 4 |
| Path coverage, production in review, query cost per path | Path enumeration; queries per path | Area 17 questions 11, 12 and 14 |
| Production replay | Every effect boundary (clock, random, integration, store read) as a node | Area 17 question 13 |
| Exact review paths | Branch-level reachability instead of may-call | Area 17 question 9 |
| Application graph, console and MCP | The graph itself as versioned JSON | Area 17 question 8; roadmap E11 |
| Second target (WASM, native) | A target-independent graph | Roadmap E09 |

**What else it enables during development:**

- **Context for agents.** The exact declarations a task touches, from the
  graph rather than a text search: smaller prompts and fewer wrong edits.
- **Test selection.** Run only the tests whose paths reach changed nodes.
- **Incremental checking.** Recheck only affected nodes (the query-based
  approach of rust-analyzer), for a faster editor and agent loop.
- **Compiler-performed refactors.** Rename, move and split as graph
  operations that are correct by construction.
- **Merging parallel agents' work** by declaration identity rather than
  lines; disjoint subgraphs cannot conflict.
- **Exact review paths.** With control flow, the approval report can name
  the branch that reaches a field instead of "may-call".
- **More than one target** (E09) from the same graph.

### 1.2 Impact of the finished language review (2026-10-13)

Written after all eighteen review areas were decided and after Richard
decided to drop the TypeScript/Bun target for native Rust and WebAssembly
([decision register](decision-register.md#runtime-targets--2026-10-13)).
Nothing in the review needs reopening. What changes is the build plan.

**1. The rework option no longer holds up.** Its one advantage was keeping
the passing golden cases green on the existing compiler. Those cases run on
the TypeScript/Bun generator, which is being dropped, so keeping them green
protects work that will be thrown away. With areas 1–4 changing nearly every
syntax form and the generator tuned to the golden app (§1.1), rework is now
a rebuild by another name. Recommendation: decide option 2, the graph-centred
rebuild, now.

**2. Development needs a second way to run the graph.** The decided testing
target is a watch-mode rerun in under a second (roadmap item 3). Compiling
generated Rust cannot meet it: recompiling an 816-line generated fixture
took a median 8.61 s with cached dependencies, release mode
([native experiment](experiments/native-rust-experiment-results.md#startup-build-cost-and-artifacts)),
and a real application is larger. Recommendation: two executors over one
graph.

- **Interpreted** inside `jadpo` for `dev`, `test`, `call`, `test new
  --from` and replay: the graph runs directly, so a save is rechecked and
  rerun in milliseconds, and the timeline, `why` and traces come from the
  interpreter's own record of what it did.
- **Compiled** for deployables: graph to Rust, then native or `wasm32`
  through rustc.
- **One shared runtime crate** both use, so only callable bodies differ:
  HTTP, sessions and OAuth, policy evaluation, store drivers, the outbox,
  scheduler and leases, idempotency, rate limits, telemetry, frame mapping,
  email rendering, flags and the privacy flows.
- **Divergence is the risk,** and the guard is mechanical: CI runs every
  test in both modes and fails on any difference. A debug-mode Cranelift
  backend was considered and rejected: still seconds per change.

**3. The graph carries much more than §1.1 lists.** Every feature below was
decided in the review and needs nodes or edges of its own, so the graph's
shape must allow for them from the first commit, even where they are built
later:

| From | Needs in the graph |
|---|---|
| Area 9 policy | Grants per effect and field, the four kinds of "who", tenancy reach, OAuth scopes; compiled into each query's conditions |
| Area 5 data | Lifecycle conditions, cache entries and which writes invalidate them, stores per entity, system-table placement by the stores actions write |
| Area 15 personal data | Marks, the person tree through references, erasure outcomes per reference and field, retention, unreachable-data errors |
| Area 12 events | Emit, subscriber and schedule edges, trace propagation, `Access.system` |
| Area 11 packages | Provider packages and capability mappings as lazily loaded subgraphs, cached per package digest; connections; hosts for the network report |
| Area 14 deployables | Reachability per deployable and its target; drivers checked against targets |
| Area 16 services | Flag reads as branch points (tests must cover each value), email templates and translation keys as typed nodes, rate limits, thresholds |
| Area 18 records | Stable identity (born name plus renames), canonical JSON, graph diff as a first-class operation |

**4. Properties the graph needs.**

- **Stable identity** from the born name and the rename registry (area 18),
  so incidents, coverage, acceptances and stored deliveries survive renames.
- **Deterministic, canonical serialisation,** because incidents rebuild a
  deployed build's manifest from its commit (area 18).
- **Incremental, query-based checking** (rust-analyzer's model) for the
  editor, `--watch` and `--affected`.
- **Lazy subgraphs** for packages, so a large provider costs only what is
  called (area 11).
- **Diff** as one operation feeding the approval report, acceptances,
  package upgrade plans, `check`'s gate and the review view.
- **Per-callable control flow** for path IDs, path coverage and exact review
  paths; a concrete syntax tree kept beside it for the formatter.

**5. The first build needs a scope.** The review decided far more than the
golden todo needs: an OAuth provider, privacy self-service, flags, audit,
search, files, SMS, publishing, the registry, MCP, the console and review
view, replay and path coverage. Building all of it before the golden todo
runs end to end would repeat the long wait. Recommendation: the first build
is the graph core, both executors and what the golden todo exercises
(entities, queries, actions, routes, sign-in, policy, configuration, events,
schedules, email through the `email` capability, tests, migrations and
PostgreSQL), on the native target first and then Workers. The rest follows
in the staged order each area already records, each still designed into
the graph from the start.

**6. Proposed order,** each step finished when its area pages' examples and
errors pass:

1. Graph core: parser and concrete syntax tree, resolution, types and
   failures (areas 1–4), stable identity, the manifest, `jadpo check`, the
   CLI shell in the agreed design language, the verifier check that bans
   example-application names in compiler source.
2. Interpreter and the test runner (area 13), so everything after is tested
   at the target speed.
3. Data: entities, migrations, queries and writes on PostgreSQL (areas 5, 6).
4. HTTP, identity and policy (areas 7–9).
5. Configuration, time and translations (area 10).
6. Events, schedules and the outbox (area 12).
7. Packages, capabilities and email (areas 11, 16).
8. Compiled deployables: native, then WebAssembly on Workers, with both
   executors passing the same tests.
9. The golden todo end to end on both targets.
10. Then, in staged order: personal data (15), the rest of built-in services
    (16), debugging and review (17), the approval report and acceptances (18),
    and the roadmap's product work (console, MCP, review view, registry).

**Risks to watch:** the size of the shared runtime crate; interpreter and
compiled divergence (guarded by running every test both ways); PostgreSQL
from Workers needing Cloudflare's connection pooling; and scope creep, which
step 5's cut exists to stop.

## 2. Architecture (chunk 0.4, 2026-10-13)

Each subsection gives the decision, the reason and the main alternative
rejected, so a chunk can start building without redesigning. Where an area
page's body and its Decisions table differ, the table wins: `connections:`
replaces area 5's `stores:` (area 11), email types and templates live in
`emails/` (area 16), and tests use an in-memory cache (area 13). The v0
compiler's architecture is kept in [the archive](archive/compiler-runtime-v0.md)
for reference only.

### 2.1 Workspace and crates

One Cargo workspace at the repository root (`Cargo.toml`, `crates/`), where
`tools/verify.py` already runs `cargo test --workspace`. Edition 2024, with
an exact stable toolchain pinned in `rust-toolchain.toml` (salsa 0.28 needs
Rust 1.88 or later).

| Crate | Responsibility | wasm32 |
|---|---|---|
| `jadpo-std` | Shared leaf: built-in types, exact numbers, value rules and their validators, the standard library's enums Rust interprets, the typed operation table; the plans both executors run, as plain data (type and failure plans, constants, read, write and purge plans) | yes |
| `jadpo-diag` | Rule catalogue, diagnostic model, edits | |
| `jadpo-syntax` | Lexer, parser, lossless syntax tree, typed AST, `jadpo.ungram` | |
| `jadpo-fmt` | Formatter | |
| `jadpo-sql` | Every PostgreSQL text the compiler writes: conditions and their one lowering (an exact SQL form, or an index search the runtime rechecks), statements, aggregates, identity lookups and table definitions, rendered from what the graph, the checker and the plan decide | |
| `jadpo-graph` | Node and edge model, identity keys, canonical JSON, diff | |
| `jadpo-db` | Incremental database: inputs, project layout, packages and lock, identity registry, the before side from git | |
| `jadpo-check` | Resolution, types, failures, effects, policy, data and layout rules, as queries that build the graph | |
| `jadpo-plan` | What both executors need besides control flow: each statement's parts with policy compiled in, which `jadpo-sql` writes, codecs and validators per type, route, schedule and subscriber tables, system-table placement, migration diffs | |
| `jadpo-interp` | The interpreter | |
| `jadpo-codegen` | Rust source from graph and plan; drives cargo | |
| `jadpo-value` | Runtime values, JSON and row codecs, validation, failure envelope, standard-library implementations | yes |
| `jadpo-runtime` | Everything in §2.6, behind a `Host` trait | yes |
| `jadpo-drivers` | PostgreSQL, Redis, SMTP, S3 and package (JSON over HTTP) drivers over a host-supplied transport; a cargo feature each | yes |
| `jadpo-host-native` | tokio, hyper, rustls; timers, delivery workers, local disk and inbox | |
| `jadpo-host-workers` | workers-rs: fetch, scheduled, `waitUntil`, sockets, bindings | only |
| `jadpo-term` | The CLI design language: rich, plain and JSON output | |
| `jadpo-test` | Test runner, PostgreSQL harness, differential comparison | |
| `jadpo-lsp` | Language server | |
| `jadpo-cli` | The `jadpo` binary: commands, help, completion | |

Built so far: every crate above but `jadpo-codegen` and
`jadpo-host-workers`; `jadpo-plan` began in chunk 2.10 with type plans,
failure shapes, constants and the frame table (§2.5), and `jadpo-drivers`
(PostgreSQL only) and `jadpo-host-native` in chunk 3.1 (§2.6). Since chunk
3.2 `jadpo-plan` also holds the SQL each query and `get` runs
(`QueryPlan`, `EntityPlan`), since chunk 3.3 each write site's
(`WriteSitePlan`), and since chunk 3.4 each lifecycle's purge
(`PurgePlan`), as the read and write plans `jadpo_std::read` and
`jadpo_std::write` define: plain data (statement text, where each
parameter comes from, how each column becomes a value and a row a
record) that `jadpo-plan` decides and `jadpo-runtime` runs, so both
executors send the same statements and build the same values, and compiled
code needs nothing from a compiler crate to run them.

Since chunk 4.1 the plan also holds the route table
(`jadpo_std::http::RouteTable`, `Plan::routes`): each route's method and
full path, where its input comes from (the query string, the JSON body,
and the fields read from headers), its success status and deadline, the
action it runs by callable number or the query's read plan, and how each
argument is made from the placeholders and the input; and a JSON codec
(`jadpo_std::http::Codec`) for every type that crosses the boundary,
numbered like the type plans: a scalar with its validator and whether an
`Int` is a JSON number, an object's fields in declared order, an enum's
variants, collections, `T?` and a page. The checker reads each route group
into a model without positions (`jadpo_check::routes`: `group_read` per
group, `table_problems` across them, `api_table` from `app.jadpo`), which
the graph (route and API nodes, `runs` and `uses` edges), the plan, the
API record (`routes::record`) and OpenAPI (`routes::openapi`, written by
`jadpo check` to `build/openapi/<api>.json`) all read; the record and
OpenAPI render types through one view of the graph (`routes::wire`).

Since chunk 5.1 the plan also holds the settings
(`jadpo_std::config::ConfigPlan`, `Plan::config`): each setting's name, the
environment variable it is read from, whether it is secret, the codec its
value is read and checked by (numbered with the route table's, sharing the
type plans' validators), its default and test value in the spelling a
variable has, and how `jadpo test` makes a secret; and for each database
connection the setting holding its address. A body's `config.<name>` is its
own graph expression (`Expr::Config`, decoded to the setting's number), and
a test's `config:` the values the test's callable plan gives that test.

Since chunk 5.5g the plan also holds the codecs `json.read` and
`json.write` run with (`jadpo_std::http::JsonTable`, `Plan::json`): the
route table's kind of codec for each type a call reads or writes, built by
the same code, numbered per call through the site's `Decoded::Json`, so a
document in memory is read and written exactly as a request and a
response are (area 19, "Data formats").
Since chunk 5.5w2b the table also numbers the type of each field holding
a JSON-held standard value (`RichText`, `RichTextBlock`, `LocalisedText`,
`FieldModel::json`): its column is `Column::Json(n)` and its write's
parameter `WriteSource::Json`, and the runtime's `store` is given the
table with the program's validators and shapes (`JsonCodecs`, as
`json.read` is given them), so its `jsonb` holds what `json.write` writes
and reads back as `json.read` reads. Since chunk 5.9b that is every field
stored as JSON, an application's enum with data, object, list, set or map
among them (`FieldModel::json` holds its type), within the 1 MiB and 128
levels a request's body is read to (`JSON_KEPT_LIMIT`, `JSON_KEPT_DEPTH`),
and the table numbers each event type too, by its identity key
(`JsonTable::events`), for the outbox's payload.
Since chunk 5.5w3 a codec of a standard type a built-in type holds reads
and writes it through the contract's JSON (`jadpo_value::standard::json`),
by its entry's `Json`: its canonical text, or an object of the members
`Json::Parts` lists (`Money`, `ZonedTime`, the quantities), which
OpenAPI's schema reads too (`routes::wire::scalar_schema`), so neither
names a built-in type; `jadpo_plan::standard_codecs` makes any standard
type's codecs over an empty graph, for the per-type harness.
Since chunk 5.5w3b the storage codec is the shared leaf's
(`jadpo_std::standard::storage::bind`, with `SqlValue`): the runtime's
`to_sql` reads a value as the codec's `Stored` view and calls it, and
`jadpo_sql::condition` decodes a literal for its field
(`jadpo_std::constant::literal`, moved from `jadpo-plan`, given the
field's built-in type and scale on `Column`) and makes its value with
it. A statement binds that value as a parameter through `Bind::stored`
(`Source::Stored`, `WriteSource::Stored`, `PurgeSource::Stored`, which the
runtime sends as they are); where a statement takes none (a partial
index's predicate, a migration's backfill, the identity statements with
fixed parameters) `jadpo_sql::literal::sql` writes the same value as SQL
text (area 19, "Chosen in chunk 5.5w3b").
Since chunk 5.5q the table marks the operations that read the generator
(`Op::reads_generator`, `random`'s five): the one exception to every
operation being pure. The checker refuses them outside actions and tests
(`type.random_context`, as `clock.now`'s `type.clock_context`), and each
executor runs them through `jadpo_value::random::run`, given the
invocation's source of random bytes, rather than `jadpo_value::run`.
Since chunk 5.5q2 `crypto`'s operations run the same way, through
`jadpo_value::crypto::run`, given the invocation's random bytes (for
`encrypt`'s nonce and `hash_password`'s salt, which read the generator)
and the key its `key: config.<name>` names. A key is never a value: the
executor passes the setting's number in the key's place, and the
operation is handed the setting's `KeyRing` (current and previous), which
`jadpo_runtime::config::key_ring` makes from the checked settings for
either executor.
Since chunk 5.5r `crypto.verify_jwt` runs there too, given the
invocation's clock as well (`Op::reads_clock`, its one operation, which the
checker refuses outside actions and tests as `type.clock_context`), in
`jadpo_value::jwt`: the token types are entries in
`jadpo_std::standard::security`, whose `decode_base64url` both sides
read, and the kinds of key a `Jwk` holds, their algorithms and the
rules across its parts are `jadpo_std::standard::jwk`'s (chunk 5.5r4),
and its conditions are `RuleKey`s that only an `attempt` settles
(`RuleKey::only_when_run`). With `as:` (chunk 5.5r4) the plan gives the
call the codec of the type `as:` names, as it gives `json.read`'s, and
`jwt::verify_jwt_as` reads the token's own claims through it. A new `FormatKey`,
`FirstPart`, shows an object by the first of its named parts it has.
Since chunk 5.5r2 the secret values (`Password`, `ApiKey`, `BearerToken`,
`TotpSecret`, `SecretText`, and `Jwt`) are entries held as text whose
text form, JSON, format and storage say read and never written
(`TextForm::Secret`, `Json::Secret`, `FormatKey::Redacted`,
`Storage::Never`; `security::secret_value` names them). The runtime holds
one as `Value::Secret` (`jadpo_value::secret`): `Validator::store` makes
it from text whose chain names a secret, so construction, a request's
JSON, `json.read` and settings all give one, the plan keeps a literal as
`Constant::Secret`, and `text.parse` reads one through its text form.
`Display` and `Debug` write it redacted, so every report and test screen
does; `==` (`ops::equal`, and `PartialEq`) compares it in constant time;
the operations made for one read it through `Secret::expose`.
`jadpo_check::types::secrets` decides where one may go: the declaration
check, the data model, `json.write`, every standard operation's arguments,
the operators and construction report `type.secret_exposed`. Credentials
the runtime makes draw on `Ctx::secret_random_bytes`, never on a
recording's tape.
Since chunk 5.5r3 a standard type may be built on `Bytes` (`Holds::Bytes`,
`Hash`, `Hmac` and `EncryptedValue`): `standard::declared` gives it as
`Bytes` whose rules name it as their `standard`, which
`Rules::check_bytes` checks through the entry's `Validator::accepts_bytes`,
so the type table, the plan's chain, the wire and storage treat it as
`Bytes` (Base64 in JSON, `bytea`), and `TextForm::Base64` is its canonical
text. The checker proves a construction from the operation that makes its
bytes (`security::made_by`, read in the rule verdict), so
`Hash(hash.sha256(x))` needs no arm. `Value`'s `PartialEq` and the
canonical order compare every `Bytes` in constant time. `OneTimeCode`
joined the secret values, and a `Bytes` part of a standard object is
bounded by its length (`Part::Bounded` on `Bytes`).
Since chunk 5.5r5 `Certificate` is a standard object read only from what
it parses (`standard::certificates::read_only`): its canonical text and
JSON are its PEM (`TextForm::Pem`), which `jadpo_value::certificate` reads
strictly, the DER checked as DER by its own walk and then read by
`x509-parser` (pure Rust, so it builds for wasm32), the parts made from
the DER and written back from its `der` part. The checker refuses an
object literal of one (`type.constructor_input`), so no executor ever
holds parts its DER doesn't say.
Since chunk 5.5g2 an `attempt` arm that maps `json.read`'s `mismatch` to
a failure declaring `problem: DecodeProblem` carries that field's place
(`RuleArm::problem` in the graph, `Raise::problem` in the plan), and the
executor fills it with the first problem the codec found, a built-in
object like `IssuedToken` (`jadpo_std::http::builtin_object_fields`).
Since chunk 5.5t a rule across an object's fields (area 2) is a `rule`
node, a child of its type, whose typed body is its condition checked with
the type's fields as parameters (`types::rule_callables`); `jadpo-plan`
lowers each into `jadpo_std::across::CrossRule`, plain data on the
object type's `ValidatorPlan`, and `jadpo_value::across` runs it, so the
interpreter's builds, the codecs' objects and compiled code check one
implementation. A broken one is `RuleKey::Named` with the rule's name,
which `RuleArm::named` maps; the checker's proof reads the condition as a
positionless `types::across::Cond` with comparisons known around the
construction (`known::Relation`).
Since chunk 5.5t2 a standard object type's rules across its parts are
`jadpo_std::standard::across`'s `StandardRule`s, each one check over what
is known of the parts (`Known`, `Given`, `Parts`): the checker reads a
literal by it (`types/body/standard_rules.rs`) and proves a rule ordering
two parts through the same `Cond` proof; the plan gives a standard
object's validator a `CrossRule` per rule whose check is
`RuleExpr::Standard`, which `jadpo_value::across` runs over the value's
fields, so builds, `text.parse`, the codecs and compiled code read one
implementation. `jadpo_value::across::broken_standard` names the rule a
value sent as its text breaks. The checks it replaced (`order_rules`,
`part_rules`, `events::broken`, the media and geo ones) are gone, and no
text reader checks a rule across parts any more.

Since chunk 3.12 everything the runtime needs of a plan is that kind of
plain data in `jadpo-std`: type plans (`jadpo_std::types`: a type's
validator chain, a shape's fields in declared order with what a field
left out holds, a variant's place, a failure's code, kind, status,
message and sectioned fields), constants (`Constant`), and on the read
and write plans the failure a missing record raises (`MissingPlan`) and
what each refused constraint raises (`Refusal`: the site's `conflict:`
arm, a reference's not-found failure, or a fault). `jadpo-plan` decides
and numbers them; the runtime raises from them (§2.5), and Phase 4's
codecs and display reads and Phase 8's code generator read them without
the interpreter. PostgreSQL text is written in one leaf crate,
`jadpo-sql`, which depends only on `jadpo-std`: the condition model and
its renderer (below), the plan's reads, writes and purges, and a
migration's table definitions, each a function given the columns,
conditions and values its caller chose, so the graph, the checker and the
plan keep no SQL text of their own.

Since chunk 3.1 the physical schema lives where both sides of the compiler
reach it: `jadpo_graph::schema` holds the model (stores, tables, columns,
keys, unique constraints, foreign keys and indexes, each with the name the
database knows it by and the born name it holds), a migration's
`snapshot.json` in canonical JSON, and the structural diff between two
schemas; `jadpo_graph::migration` works out from two schemas which changes
need a person's decision, the snapshot the next migration records, and its
SQL, per store. `jadpo-check` builds the schema the entities ask for, with
each store's system tables (`data::schema`), and compares it with the
latest snapshot, so `jadpo check` and `jadpo migrate new` read one answer.
That is the "migration diffs" and "system-table placement" the table gives
`jadpo-plan`, moved down because `jadpo check` needs them. Since chunk 3.2
the schema also holds the indexes the queries need (area 5, "Indexes"):
for each query, and each list it includes, the fields it always tests with
`==` or `in`, in the order its lines name them, then its order's fields,
unless an index or unique key the table already has serves them; a
reference's own index goes once a query's index starts with its column.
Their columns are ascending, which PostgreSQL also reads backwards. Since
chunk 3.4 an index of an entity with a lifecycle holds only the records
its `visible:` lets through (`Index::predicate`, a partial index's `WHERE`
in the snapshot and the migration), unless `visible:` reads `clock.now`,
and a reference's own index then stays. Conditions become SQL in one
place, `jadpo_sql::condition` (in `jadpo_check::data` until chunk 3.12),
which the plans' statements and the indexes' predicates both use, through
a `Bind` that says where a parameter goes and the `Columns` an entity's
model gives it. What
each write can break (`data::write_rules`: an entity's unique rules, its
references, and the restricting references a delete reaches through every
`cascade`) is a per-entity query the checker and the plan both read, so a
body depends only on the entities it writes.

**Dependency direction.** Compiler crates never depend on runtime crates,
except `jadpo-interp`, which links the runtime it shares with compiled
code. Runtime crates never depend on compiler crates, and a generated
application depends only on runtime crates and the shared leaf. The one
shared crate, `jadpo-std`, depends on no workspace crate, so both sides may
depend on it: what a valid value is, and what each standard-library
operation takes and gives, is decided once (decided 2026-10-13, Phase 1
review). The layers are a table in the root `Cargo.toml`
(`[workspace.metadata.layers]`); a workspace test reads it and `cargo
metadata` and fails on anything a crate reaches, directly or through other
crates, against these rules, on salsa anywhere but `jadpo-db` and
`jadpo-check`, and, since chunk 3.12, on `jadpo-sql` reaching anything
but the shared leaf. *Rejected:* v0's single `core` crate, which grew to
everything and let the generator read the syntax tree.

**The standard library** (`list`, `temporal`, `decimal` and the rest)
is declared in Jadpo stub files embedded in `jadpo-check`, for programs to
call, and listed, typed, in `jadpo-std`'s operation table: each operation's
parameters with their kinds, its result, and the one Rust function in
`jadpo-value` that runs it, so its semantics exist once for both executors.
A test holds the stubs to the table on arity and kinds, and `jadpo-value`
matches on the table's operations exhaustively. An operation the language
names but no chunk has built yet is `toolchain.not_built_yet` where it is
called. In the stubs a single capital letter is a type variable, fixed by
the arguments at each call; `Number` is any type built on `Int` or
`Decimal`, and a built-in representation such as `Int` in a parameter is
any type built on it, since operations work on the representation as
operators do (so `integer.divide` takes values built on `Int`, and
`decimal.divide` numbers). A parameter the table marks `never_zero` (the
divisions' `divisor`) is checked as `/`'s divisor is: a literal isn't `0` and
a value's type excludes zero (chunk 1.9). A type variable no argument fixes is a checked
unknown: it accepts anything where it stands, but the container around it
is still checked, so a `List<T>` parameter takes only a list. Since chunk
5.2 the table also says what the stubs can't: a parameter of kind `OneOf`
(a stub's `T`) takes only values built on the types it lists, and a result
of that kind keeps the argument's named type; an `optional` parameter (a
stub's `T?`) may be left out and is given `none`; an option `only_for`
one built-in type means nothing for others (`type.temporal_option_unused`);
a `Clock` parameter takes only a clock time written out; since chunk
5.2b an option that is also `needed` must be given when it applies
(`zone:` for an `Instant`, `type.temporal_zone_required`), and a `Format`
parameter takes only a format written out, which the checker reads into
`jadpo_std::TimeFormatSpec` and lowers as its text; and an operation's
`conditions` are what it fails on unless an option written out settles
them, mapped by the call's `attempt` as a construction's broken rules are
(the operation returns `Fault::Broke(key)`, which the interpreter raises as
the arm's failure). Since chunk 5.5b (area 19, "Lists") a `never_negative`
parameter (a count or a position) is checked as a divisor is, a literal
not below `0` and a value's type with a `min` of at least `0`
(`type.argument_negative`); an `ordered` inline function's result must be
a kind `<` orders, or text (`type.ordering_operand`); an `implicit`
parameter is never written: the checker lowers it from the call's types,
as `list.sum`'s `zero` of its items' representation, and a result of kind
`Number` is that representation; a `Map` result is a stub's `Map<K, V>`;
and `MOVED_NAMESPACES` lists a namespace that moved whole (`collection`,
reserved with no operations), whose calls the type checker reports with
an edit by their first value's type, since resolution can't know it.
Since chunk 5.5c a `Set` parameter or result is a stub's `Set<T>`, and a
standard object type an operation takes (`InstantRange`, kind `Std`) can
be written as an object literal, which the plan builds from its data
entry's fields (area 19, "Sets", "Maps" and "Ranges"). Since chunk 5.5c2
a stub's `Function<K, V, U>` is an inline function of two parameters,
`(key, value) -> ...` (the graph's `Inline` holds its parameters in
order, and `jadpo_value::run`'s `each` is given the values as a slice);
and a `Range` parameter (a stub's `Range<T>`) takes a `Range<T>` or a
standard range type, all of a call's ranges one range type, which a
`Range` result is. `Range<T>` is the first standard type with a type
argument: `Holds::Range` in its entry, the checker's `Ty::Opaque("Range",
[T])`, one plan shape for every `T` and a codec per `T`.
Since chunk 5.5k (area 19, "Time and calendar additions") a standard
object's fields are `Part`s: a built-in type, a whole number with a range,
or another standard type. The type table reads them as fields
(`types::standard_fields`), a range as the field's own rules, so an
object literal proves it and `LocalTime.hour(...)` builds one, which the
plan's validator checks from the entry. An object whose JSON is its
canonical text has `Codec::Standard`, read and written through
`jadpo_value::standard`; `jadpo_value::calendar` holds the time group's
texts, `CalendarPeriod`'s operator rows (which `ops::add`, `subtract`
and `multiply` reach by the value's shape) and `temporal.period_between`,
and `ops::compare` orders a record its entry marks ordered, part by part.
Since chunk 5.5k2 a standard type held as text is a stub's kind
`Std` too (`RecurrenceRule`, `OpeningHours`), the runtime giving the
operation its text; `jadpo_std::standard::recurrence` reads an RFC 5545
rule and expands it in local time, and `opening_hours` reads OpenStreetMap's
hours, each shared by the type's validator and by `jadpo_value::temporal`'s
`occurrences` and `open_at`, which put the local times in the zone with
jiff. A `within` parameter (`occurrences`' `count`) is a whole number
proved between two bounds as a count's sign is. Since chunk 5.5k4 a
`Clock` parameter also takes a `LocalTime`, given to the operation as its
record; a `Local` parameter (`resolve`'s first, a stub's `T`) takes a value
built on `CalendarDate` or a `LocalDateTime`, `resolve`'s `at:` being an
option `needed` for a date and `only_for` it; a `OneOf` text literal
takes the first listed time it is written as; and `occurrences`' result is
`List<CalendarDate>` when its `from:` is a date, the runtime matching on
the value it is given.
Since chunk 5.5d (area 19, "Numbers") a `written_out` parameter
(`decimal.round`'s `scale:`) takes only a whole number from 0 to 18
written at the call, and an operation whose `OneOf` result shares its `T`
with two parameters (`min`, `max`) takes values of one type there, as `+`
does, giving that type, or the representation for a plain number.
Since chunk 5.5f (area 19, "Text") a `never_empty` parameter is text
checked as a divisor is (a literal not `""`, a value's type with a
`min_length` of at least 1, `type.argument_empty`); a `character`
parameter (`text.pad_start`'s `fill:`) takes only a text literal of one
code point; a `bounds_length` parameter is the text a result is never
longer than (`at_most_length_of`), which construction proofs read as that
argument's `max_length`; and a stub's `List<Text>` takes a list of any
values built on `Text`. `jadpo-value`'s `text` module runs the text
operations in code points, case mapping, collation (`list.sort_by`'s
`locale:`) and the slug's decomposition over ICU4X, and the codecs compose
every text a request carries to NFC before its rules run, in the one
function both executors decode through.
`temporal` runs on jiff with its bundled IANA database,
in `jadpo-value` only, so every target reads the same zones and none reads
the platform's (area 10, "Zones and local times"). The time types'
`text.format` (area 19; `temporal.format` until chunk 5.5a) and
`format_relative` write text for people in `jadpo-value`'s `present` module
over ICU4X with Unicode CLDR compiled in (pure Rust, pinned exactly, built
for wasm32 like the rest; area 10, "Chosen in chunk 5.3"): the format
arrives as `TimeFormatSpec` text, the locale as the `Locale` variant, and
the locales an application supports (`locales { }`, `LocalePlan`) reach the
wire through the codecs, which accept only those, and reach `jadpo-value`'s
`run` as its `locales` argument, which `locale.from_accept_language`
(chunk 5.3c; `locale` module, a pure function of the header and the plan)
chooses among; a `nullable` parameter (a stub's `T?` that a call must still
pass) is the one the table marks apart from an `optional` one. Translations
(area 10, "Translations as messages", chunk 5.4c) are read in `jadpo-check`'s
`translations` module from the project's own source: a `type X = Message`
in a `translations/` folder (`FieldsBase::Message`, with `PluralCount` a
built-in beside `PresentationText`), and one unnamed `translations { }` in
each language file, named for its locale (`SINGLETONS` in the grammar,
`Folder::Translations`). `declared` reads what can be worded in a scope (a
domain's or `shared`'s): its messages, the enums of the scope that a
language file words, and the emails; `language` reads one language file by
itself, and `check` compares a scope's files with its subjects and the
application's `locales { }`, giving the project's `TranslationPlan`: for
each message or enum, its parameters and its wording (`jadpo-std`'s
`Template`, `{field}` pieces only) in each locale that has it complete. A
call is typed by a per-name `callable` query that sees only the message's
parameters, so editing wording checks no body again. The plan reaches the
interpreter's `Callee::Translation` (and generated code, which will write
the text with the same function): `jadpo_value::present::translate` walks
the fallback chain of an `in_progress:` locale (`locale::fallback_chain`:
itself, its regional siblings with CLDR's likely one first, the default),
picks the wording by the enum value or by the locale's CLDR plural form,
and writes numbers the locale's way, over `icu_plurals` and `icu_decimal`
beside `icu_datetime`. The plural forms each locale needs are a table in
`jadpo-std`, which `jadpo-value`'s tests hold equal to ICU4X's data. Each
message and enum is a `translation_key` graph node digested with its
wording in every language file, so a save of one reaches the tests that call
it. Chunk 5.4d adds, in the same module: failures as subjects keyed by code
(`declared`), required when a route's `fails` lists them (`failures`, which
reads the route model), their wording carried in `Analysis` and written by
`jadpo-cli` as `build/openapi/<api>.failures.<locale>.json` beside the
OpenAPI documents; `sources`, each folder's `sources.jadpo_record` against
the wording (`Standings`, `updated`) and `translations: { review: }` in
`app.jadpo`; `outstanding`, the gaps and unused messages the commands list;
and `definitions`, go to definition from a call. Chunk 5.4e adds `jadpo`,
Jadpo's own messages: a catalogue in `jadpo-std` (`jadpo_text`) of each
message with its feature, fields and shipped wording, turned into
`Subject`s the root `translations/` folder's `jadpo: { }` group (the
`RootTranslations` place, read by `language` into `Group`s) is checked
against by the same `Ctx` as a folder's, a feature being on when a route
(and its authentication) calls for it; the root folder's record is `sources`'
`ROOT` scope, and the failures it words join each API's export. The pseudo-locale
`Locale.en_xa` is a `Locale` variant that `fallback_chain` sends straight to
the default locale, and `present::pseudo` accents and lengthens the text of
the wording while the values it writes stay as they are.

Prompts (area 21, "Prompts as files", chunk 5.5v2) are read in
`jadpo-check`'s `prompts` module: a `type X = Prompt` (`Base::Prompt`,
`FieldsBase::Prompt`, `Folder::Prompts` in a domain or `shared/`) and its
text, `<name>.prompt` beside it, which `jadpo-db` reads as a record
(`is_prompt_text`; in loose files any `.prompt`) and `source_texts` hands
to the renderer so its errors quote it. `jadpo-std`'s `prompt` module
parses the two sections into `Piece`s, fills them (`fill`) and escapes a
data block's closing tags (`escape`), for text and for JSON; `analysis`
decides how each field is sent (`Form::Closed`, `Text` or `Json`, or the
conversation's turns), reports the `prompt.` rules and gives each checked
prompt a `PromptEntry` with its digest, a SHA-256 over its two files. The
graph gets a `prompt` node per prompt (`Detail::Prompt { version }`) and a
`uses` edge from each callable that renders it. `prompts.render(value)`,
a member of the generated `prompts` namespace, is checked from the value's
type alone and lowered to `Callee::Prompt(name)`; `jadpo-plan` numbers a
JSON codec for each field the text writes (`Decoded::Prompt`) and carries
the `PromptPlan`, and `jadpo_value::prompt::render` builds the
`List<ChatMessage>` for both executors. `jadpo ai show` (`jadpo-cli`'s
`ai`) prints a prompt's messages from `outline`, unfilled.
Enums that Rust code interprets (`Rounding`, `TextFormat`,
`UuidVersion`, `Weekday`, `Dst`, `InvalidDay`, `ComponentStyle`, `TextDirection`, `TranslationReview`, and `Zone` and `Locale` from their tables) are
declared once, in `jadpo-std`. The namespaces the compiler fills from the
project or the platform (`clock`, `config`, `translations`, `sessions`,
`oauth`) list in `std/names.txt` the members built so far (`clock.now`,
and a test's `clock.advance`);
any other member is `toolchain.not_built_yet`. `config.<name>` is the
exception: its members are the project's own settings, read from
`config { }` (chunk 5.1, `jadpo_check::config`), so `names.txt` lists none.

**The standard types** (area 19, "Every standard type meets one
contract"; chunk 5.5a) are the extension point chunks 5.5b to 5.5s add
type groups through, with no new code path. Each is two things:

- a stub declaration in `crates/jadpo-check/std/standard.jadpo`, global,
  which `StdNames` reads (`StandardStub`): `= Enum { ... }` for a closed
  list (its variants, or none when its table in `jadpo-std` is long),
  `= Object { ... }` for a value with named fields, or its own name for a
  type built into the language (`jadpo_std::Builtin`);
- a data entry in `jadpo_std::standard::STANDARD_TYPES` (`StandardType`):
  what it holds (`Holds`), its canonical text (`TextForm`), its JSON shape
  (`Json`), its storage column, the locale data it is formatted with
  (`FormatKey`) and the `text.format` options it takes and needs, whether
  `<` takes it, its named `Validator` (IBAN's mod-97, Luhn and the like are
  added to that enum, one function each), the shape `type.standard_exists`
  matches (`FieldMatch`), its examples, and the release that added it,
  which `type.standard_clash` names (chunk 5.5a2).

Every baseline operation has one implementation over the entries:
`jadpo_value::standard` (`text.from`, `text.parse`, `text.format`, with
CLDR names in `present::names`), the plan's codecs (`Made::StdObject` for
an object), `jadpo_check::data::columns::column_type`, the checker's
`ordered`, and `types/body/contract.rs`, which decides which values the
three calls take, the options a type needs and what `text.parse` can fail
with (`malformed` and the rules of the type `as:` names; the interpreter
builds that type once the text is read). `jadpo_check::types::decl`
matches `type.standard_exists`. Two tests hold everything to the entries:
`crates/jadpo-check/tests/standard_types.rs` (the stub against the entry,
and what each call and `<` accept) and
`crates/jadpo-interp/tests/standard_types.rs` (text both ways, the
format for people, JSON, storage, `==` and order, per example, through
the codecs the plan's builder makes, `jadpo_plan::standard_codecs`, since
chunk 5.5w3). A new
`TextForm` or `FormatKey` variant is the only code a group may need, once,
for every type that shares it. Operations that move are listed in
`jadpo_std::MOVED` and reported as `semantic.standard_operation_moved`.
Since chunk 5.5h an entry's storage is one column or named columns
(`jadpo_std::standard::Storage`): a field of such a type (`Money`) is
stored in `<column>_<part>` columns, always all of them, read through
`Column::Compound` and `Column::Part` and written through
`WriteSource::Part`, with `jadpo_value::standard::to_parts` and
`from_parts` the one codec, and the part a type can fix (`Money`'s
currency) held to its value by a check constraint, which the physical
schema, snapshots and migrations now carry (`schema::Check`). Standard
enums are spelt on the wire and in settings by their entry's codes
(`Spelling::Codes`). `Money` itself is a `Builtin` (`Value::Money`, minor
units and a currency), its operators in `jadpo_value::ops` and `money`;
the checker carries what it knows of an amount's currency
(`types/body/currency.rs`, with conditions' currency classes in
`known.rs`), an `attempt` covers a chain of operators that can fail
`currency_mismatch`, and the plan gives each `*` or `/` of an amount its
type's `rounding:` (`Decoded::Rounding`).
Since chunk 5.5i a standard type can be held as text (`Holds::Text`, the
payment and identity numbers in `jadpo_std::standard::identifiers`): its
stub is `= Text {}`, the checker's type table declares it as a type built
on `Text` whose one rule is `Rules::standard`, its own check (its pattern
and named validator, broken as `format`), and the plan's chain, the wire
and OpenAPI read a `TypeRef::Std` of it as text with that rule
(`standard::text_rules`), so construction, JSON and storage need nothing
new. `text.format` is given the type's name by an implicit argument the
checker fills (`standard`), since the runtime holds only the text, and
shows it grouped or masked to its last four (`FormatKey::Grouped`,
`FormatKey::LastFour`).
Since chunk 5.5j (area 19, "Physical quantities") each of the 17
quantities with `DataSize` and `Bitrate` is `Builtin::Quantity(kind)`
(`jadpo_std::QuantityKind`), held as `Value::Quantity`, a mantissa in the
kind's SI base unit at its scale. `jadpo_std::quantity` is the one table
of units (each kind's unit enum, its suffix, its size as an exact fraction
and an offset for temperatures) and of the products and quotients, with
the exact conversions and the table's arithmetic both the checker and
`jadpo-value`'s `units` module use; `unit_suffixes()` reads it for the
editor. A suffixed literal is `Const::Quantity` and `Literal::Quantity`
(any suffix but a duration's), checked by `literal::check`, which also
reports a bare number as `type.unit_required`; `types/body/quantities.rs`
types the `units` operations and `text.format`'s `unit:`, whose stubs
write `Quantity` and `QuantityUnit` (`Kind::Quantity`,
`Kind::QuantityUnit`). A quantity is stored in one `numeric(38, <scale>)`
column (`Column::Quantity`), sent as `{ "value": "2.5", "unit": "kg" }`,
and shown by `present::units` over ICU4X's CLDR unit names where it has
them.
Since chunk 5.5j2 `Percentage` is `Builtin::Percentage`, held as
`Value::Percentage`, the exact `Decimal` fraction (20% is 0.20), with its
operator rows in `jadpo_value::ops` and the checker's `ops.rs`
(`percentage_arithmetic`, and money's and quantities' rows taking it).
The lexer reads digits touching `%` as one `UNIT_NUMBER` token, which the
checker reads as `Const::Percentage` and the graph holds as
`Literal::Percentage`; a number, `%` and a name or number all touching is
`LexErrorKind::PercentOrRemainder`, `syntax.percent_ambiguous`.
`jadpo_std::percentage` holds the literal's and the three types' texts.
A construction into a type built on `Percentage` takes a `Decimal` ratio,
which `Validator::store` turns into a percentage. It is stored in one
`numeric` column (`Column::Percentage`), sent as its text, `"20%"`, and
shown by `present::percent` over ICU4X's percent formatter; a message
writes it through `translation::Style::Percent`. `Ratio` and `Fraction`
are standard objects held as text on the wire and in storage, as the
time objects are.
Since chunk 5.5l a closed list (question 22) is a standard enum with a
`ClosedList` in `jadpo_std::standard::places`: its authority, its other
code systems (`CodeForm`, each a list of codes in the variants' order,
which `text.parse` and `text.from` take as `form:`, checked in
`types/body/contract.rs`) and its retired variants, which the checker
refuses where code writes one (`type.variant_retired`) and `Enum.all`
leaves out. `jadpo_std::enums()` and `variant_ordinal` read every standard
enum from its entry, so a list is its table alone; the tables are
generated once into `places_data`, `person_names_data` and
`addresses_data`, which the example-name ban skips as authority data
(`// @generated`). `Enum.all` lowers to a list of the variants it lists.
A standard object's `Part` may be a line of text (`Part::Line`, its
pattern its rules), `Optional` or a `List`; the checker reads such a part
as a nullable field or a list, the plan as the field itself (`TypeRef::Field`,
whose chain is the part's rules), and an object with `Storage::Columns` is
stored a column per part, through the same `Column::Compound`,
`WriteSource::Part`, `to_parts` and `from_parts` as `Money`, a part that
may be `none` a column that may be `NULL` and a list a `text[]`, which
the PostgreSQL driver now reads. `jadpo_value::present::contact` writes a
person's name by CLDR's person-name patterns and an address by
libaddressinput's formats, and `present::names` the CLDR names of places,
languages and scripts through ICU4X's display names.
Since chunk 5.5l2 `jadpo_std::standard::phones` validates, reads and
formats phone numbers from libphonenumber's metadata (`phones_data`,
generated once, its patterns compiled on first use), and
`standard::postal` checks and reads postal codes by each country's
pattern (`postal_codes_data`). `text.parse` takes `country:` (a `Country`
value; `country_option` in `types/body/contract.rs` refuses it for other
types), which `jadpo-value`'s `parse_in` hands to those modules. A postal
code fitting its address's country is a rule across the address's parts,
`standard::across` since chunk 5.5t2.
Since chunk 5.5i2 the cards (`jadpo_std::standard::card_types`) and the
identity documents (`standard::documents`) are entries like the rest: a
number's rule reads its document's kind and country (`standard::across::fits`
since chunk 5.5t2), and each country's forms are tables
of a pattern and an optional check digit (`CountryForm`), with bounds for
a country that has none. `Part::Matching` is a part of text matching a
pattern (a masked card's last four). `money.mask` (`Op::MoneyMask`) makes
a `MaskedCardNumber` from a `PaymentCardNumber` by its issuer range
(`card_types::brand_of`).
Since chunk 5.5n a standard type may be held as a whole number
(`Holds::Whole`, `HttpStatus` and `NetworkPort`): the checker's type table
declares it as a type built on `Int` with its range as its rules, which
name it as their `standard`, as a type held as text's do
(`jadpo_std::standard::declared` gives either), so the plan's chain, the
wire and OpenAPI read it as an `Int` with that range, sent as a JSON number
(`Json::Number`) and stored in an `integer` column; `text.format` is told
its name as a type held as text's is. A type held as text may be a
narrower form of another (`standard::WITHIN`: a `UrlPath` is a
`RelativeUrl`), built on it in the type table. A standard object's part
named with a reserved word's trailing `_` is sent without it
(`standard::wire_name`, `ProblemDetails`' `type_`), and one sent as RFC
9457 sends a problem (`Json::Members`) leaves out its parts that are
`none`, which read as `none` when left out, through the plan's `absent`.
`jadpo_std::standard::web` holds the web types' validators (UTS 46 by the
`idna` crate, the only dependency `jadpo-std` gained besides `regex`;
RFC 3986's grammar and resolution; RFC 6265's cookies; Emoji 18.0's list
in `emoji_data`), and `jadpo_value::web` runs `web`'s operations over it.
Since chunk 5.5n2 `jadpo_std::standard::software` holds the software
types' validators and SemVer 2.0.0's grammar and precedence, with
node-semver's ranges desugared to comparators there; `SpdxLicence` reads
the SPDX licence list from the `spdx` crate (0.13.6, Apache-2.0, the list
3.29.0 under CC0), `jadpo-std`'s third dependency, and its expression
grammar is jadpo-std's own. `SemanticVersion` is a standard object whose
canonical text is SemVer's (`TextForm::SemanticVersion`), stored a column
per part; `<` on it is its precedence (`jadpo_value::ops::compare`, build
metadata ignored), and the one order (`jadpo_value::order`) sorts it by
precedence then build metadata, so `sort_by` agrees with `<`.
`jadpo_value::semver` runs `semver.satisfies` (`Op::SemverSatisfies`).
Since chunk 5.5o `jadpo_std::standard::markup` is the HTML sanitiser
every `Html` value is built with: ammonia over html5ever, with Jadpo's
allow-list as data there, its output always a fixed point, and markup
nested past 512 elements kept as text. ammonia and html5ever are
`jadpo-std`'s fourth and fifth dependencies, so the checker proves an
`Html` literal (`Validator::Html`, a literal already safe) with the code
the runtime builds values with. `Html` is held as text (`Holds::Text`,
`TextForm::Html`). Its construction is a transformation, not a refusal:
`jadpo_value::Validator::store` sanitises text whose chain names it, as
it turns a ratio into a percentage. Every construction, a write's input,
a request's JSON and the probes go through `store`, and `text.parse`
reads it through `TextForm::Html`. The checker's `verdict` never counts
`format` breakable for it, and `parse_fallible` reads it as text, so
neither needs an `attempt`. `jadpo_value::markup` renders CommonMark with
pulldown-cmark (`jadpo-value` only) and sanitises the result for
`markup.to_html`; `markup.to_plain_text` and `FormatKey::PlainText` read
the sanitised text's tokens.
Since chunk 5.5o2 a standard type may be a named collection: a list
(`Holds::List`, `RichText`) or a map (`Holds::Map`, `LocalisedText`) of
parts. The checker's type table builds each on `List<...>` or
`Map<...>` as it builds an application's named collection, the plan
gives it a list or map codec of its parts, and a map keyed by a standard
enum is written by each key's wire text (a locale's tag). A part may now
be a standard choice or object sent as its fields, which the plan
already built recursively. `jadpo_std::standard::rich_text` holds the
entries, the link pattern and `render`, which writes blocks and spans as
HTML the sanitiser keeps unchanged; `jadpo_value::markup` reads a
`RichText` value into those blocks for `markup.rich_to_html`
(`Op::MarkupRichToHtml`), which sanitises the result, and
`jadpo_value::locale::pick` walks area 10's fallback chain over a
`LocalisedText` for `text.pick` (`Op::TextPick`) and `text.format`, with
the application's default locale from the `LocalePlan` `run` is given.
Since chunk 5.5p `jadpo_std::standard::colour` holds `Colour` and its
hex text, and `jadpo_std::standard::media` the media types, the
WebCodecs codec strings, SMPTE drop-frame and WebVTT rules and
`media.fit`'s whole-number arithmetic. Generic text forms carry objects
whose parts are numbers joined by a separator (`TextForm::Wholes`,
`TextForm::Decimals`), an enum's code then text (`TextForm::CodeThenText`)
and a WebVTT cue (`TextForm::Cue`); `Timecode` has no text form and is
shown by `FormatKey::Timecode`. `jadpo_value::colour` runs `colour`'s five
operations by CSS Color 4's conversions in fixed point at 18 places over
`maths`' correctly rounded functions, so every target gives the same
channels, and `jadpo_value::media` runs `media.fit`; a timecode's and a
cue's rules are rules across their parts (chunk 5.5t2). A shape's `FieldMatch` may be `optional`
(`Colour`'s alpha), and a stub parameter written `Percentage` or a
quantity kind takes any type built on it.
Since chunk 5.5m2 the records and measures are entries in
`jadpo_std::standard::records` and `standard::health`. Two text forms came
with them: `TextForm::Joined`, two parts joined by a separator, the first
an enum's code, a standard type's text or a pattern's (`CodeThenText` is
its case with a space and an enum, through the same
`jadpo_value::media::read_joined`), and `TextForm::FlightNumber`, IATA's
designator, in `jadpo_value::measures`. Three `FormatKey`s show measures
in the unit a reader expects: `UnitsIn` (a quantity in a listed unit at
listed places, or in `unit:`'s), `Measures` (an object's or a choice's
quantity parts, each in its kind's listed unit or `unit:`'s, the unit
written once where it says) and `Dosage`; `present::units` now splits a
quantity into its number (`number_in`, exact or rounded to places) and
its unit (`with_unit`), so a unit can follow several numbers. A type built
on a quantity with `UnitsIn` (`HeartRate`) is named to `text.format` by
the checker as a type held as text is, since the runtime holds only the
quantity, and `StandardType::unit_kind` gives the kind `unit:` names a
unit of. A rating's and a blood pressure's parts are ordered by rules
across them (chunk 5.5t2). A `Part::Bounded` on a `Duration`
keeps its bounds in milliseconds as `min_duration` and `max_duration`.
Four units joined the table with no literal suffix (`mmHg`, `bpm`,
`mgdL` and the glucose millimole). `CodeForm::Isbn10` is a form read and
never written (`standard::reads_form`), which `text.parse` turns into the
13 digits through `standard::isbn_from_isbn10`.
Since chunk 5.5s the geospatial types are entries in
`jadpo_std::standard::geo`, with H3's index check there. A geometry is
sent as GeoJSON (`Json::GeoJson`): the plan gives it a `Codec::Standard`,
which `jadpo_value::json` hands to `jadpo_value::geo` to read and write,
the one place a position's `[longitude, latitude]` exists; it checks
ranges, lengths, closure and members, rewinds rings to the right-hand rule
on the way in and out, and reports a refusal at its place in
`coordinates`. `Geometry` is a choice the plan sends the same way, never
tagged. A geometry's storage is `Storage::Geography`, whose column type is
`geography` (chunk 5.5u4, below). Since chunk 5.5u6 `Feature<G, P>` and
`FeatureCollection<G, P>` are standard types with two type arguments, as
`Range<T>` has one: `Holds::Feature` and `Holds::FeatureCollection`, the
checker's `Ty::Opaque("Feature", [G, P])` (its fields in
`types::body::features`), one plan shape each, and `Codec::Feature` and
`Codec::FeatureCollection` holding their arguments' codecs, which
`jadpo_value::json` reads and writes as RFC 7946's objects
(`Json::Feature`). WKT is read and written by `jadpo_value::geo::wkt`,
the one reader, which reports a refusal at its character. A type built on
`Decimal` with bounds (`Latitude`) has `TextForm::Decimal` and is named to
`text.format` by the checker, so `FormatKey::Coordinate` can show its
hemisphere; a compound type's column may now be `numeric`
(`EastingNorthing`).
Since chunk 5.5s2 the AI types are entries in `jadpo_std::standard::ai`.
`Vector` is a named collection of `Part::Float`, a single-precision float
held as the decimal `standard::ai::float` gives; its type rules
(`standard::collection_rules`: 1 to 16,000 items, `standard` naming it)
make `Validator::store` keep every item as that float, as it makes
`Html` safe, so the checker's `verdict` never counts `format` breakable
for it. The plan sends one as a `Codec::List` of `Codec::Float`, a JSON
number each. A type built on `Vector` takes `dimensions:` (`Subject::Vector`
in the checker's constraints, `Rules::dimensions`, `RuleKey::Dimensions`),
checked by `Rules::check_length` before the lengths it decides, proved by
`verdict` from a list's counted items, and shown in the API record and as
OpenAPI's `minItems` and `maxItems`. A vector's storage is
`Storage::Vector`, a pgvector `vector` column. `FormatKey::Vector` and
`FormatKey::ByLabel` show vectors and labelled parts.
Since chunk 5.5v1 a vector field is stored: the data model gives any
type whose rule chain is `Vector`'s a `vector(n)` column, `n` its
`dimensions:` (`vector` without), with no length checked beside it, and an
`Embedding` three columns (its `ModelId`'s two and a `vector`). The
column is `SqlType::Vector` and `Column::Vector`; the runtime holds it as
`SqlValue::Vector`, each item's float by its bits, and the PostgreSQL
driver sends it in pgvector's binary format with no type number (the
server infers it from the column) and knows a column of it by its type's
name, since the extension's number differs in each database. `distance:`
(`Rules::distance`, a `VectorDistance`) says how a type's vectors are
compared; `ai.distance` (`Op::AiDistance`, `jadpo_value::vector`) is
given its metric by the checker, as `text.format` its standard type, and
works it out as pgvector does (`jadpo_std::standard::ai::distance`). The
migration diff writes `CREATE EXTENSION IF NOT EXISTS "vector"` first in
the first migration with a vector column (`migration::extensions`);
`jadpo_runtime::migrate` refuses a database that doesn't offer an
extension a pending migration creates before running anything
(`MigrateError::ExtensionMissing`, `migration.extension_missing`), and
`has_extensions` is the start-up check, which `jadpo dev` runs.
Since chunk 5.5u4 a geometry field is stored: the data model gives a
field holding one of the six geometries or a `Geometry` one untyped
`geography` column (`FieldModel::geometry` names which), with a check
constraint (`schema::Check { geography: true }`) holding it to SRID 4326
and its `GeometryType`s; a `BoundingBox` and a `Circle` are named columns
(`Storage::Columns`), a circle's centre a point's `geography` with the
same check (`storage::geometry_at`). The column is `SqlType::Geography`
and `Column::Geography`; the runtime holds it as `SqlValue::Geography`, a
`jadpo_std::geography::Shape` of `Coordinates`, each a double by its bits,
which `jadpo_value::geo::shape` and `from_shape` make from a value and
back without rewinding a ring; the storage codec binds one
(`Stored::Shape`) and refuses one whose positions mix with and without
altitude. The PostgreSQL driver sends and reads it as EWKB, with no type
number, and knows the column by its type's name; the compiler writes it
as EWKT (`jadpo_sql::literal`). `migration::extensions` adds `postgis`
for a `geography` column, and `jadpo_runtime::migrate` also creates each
extension a pending migration needs in a savepoint and undoes it, so a
role that can't create PostGIS is refused before anything runs
(`MigrateError::ExtensionNotCreatable`, `migration.extension_missing`).
An `indexes:` entry's block takes `fields:` and `kind:`
(`ListRule::kind`, `IndexKind`); `IndexKind.geographic` is a GiST index
(`IndexMethod` `gist`, no operator class) on one geometry field.
Since chunk 5.5k3 `CalendarEvent` is a standard object whose canonical
text is an RFC 5545 `VEVENT` (`TextForm::Event`), written and read by
`jadpo_std::standard::events`, which also holds the rules across its parts
(days or times, their order, a recurrence from the event's start), named
rules since chunk 5.5t2. A part may be another standard object
(`Part::Standard("LocalDateTime")`), sent as its canonical text and stored
as it in a `text` column (`to_parts`), or text over several lines
(`Part::Paragraph`); a `date` part reads back from PostgreSQL as a date.
Since chunk 5.5m the codes of commerce, health, travel and the other
published standards are held as text in `jadpo_std::standard::codes`, with
their check digits as named validators (GS1's mod 10, ISO 6346, ISO 7064
MOD 11-2, Verhoeff, a VIN's and an MPAN's), and their closed lists are
standard enums in `standard::lists`, whose `ClosedList`s `closed_list`
reads beside the places'. A list CLDR names none of is shown by its
authority's English wording through chunk 5.5i2's `FormatKey::Names`;
no other code was needed.
Since chunk 5.5i3 a standard type may be a choice of other types
(`Holds::Choice`, `Discount` and `UnitPrice` first): its stub is `= Enum
{ ... }` with each variant's fields written as the built-in types its
parts are built on (`StandardStub::payloads`), and its entry lists each
variant's parts. The checker's type table declares it as an enum with
data whose variants' fields come from the parts (`standard_field`), so
construction, `match`, a binding's fields and literal proofs are area 2's
own; `jadpo_std::enums()` lists it, and the plan gives each variant the
shape and the parts' codecs of its entry (`standard_variant_fields`,
`Routes::choice_fields`, keyed `field <Choice>.<variant>.<field>`, whose
chain `standard_part` reads), so the interpreter builds and reads a
`Value::Variant` with fields, and JSON is area 2's tagged form
(`Json::Tagged`, `Codec::Enum` with payload variants). OpenAPI describes
it as `oneOf` its tagged variants. A later choice (`BankAccount`, chunk
5.5i4) is a stub and an entry; its UK modulus check is
`jadpo_std::standard::modulus`, which reads Vocalink's tables as text and
which nothing runs until they can ship. A standard type may also be built
on a built-in type with bounds of its own (`Holds::Bounded`, `TaxRate`): `standard::declared`
gives its built-in type and its bounds, as it gives a type held as text
or as a whole number its rules, so the table, the plan's chain and the
wire read it as that refinement; its bounds name no `standard`, so a
plain `Percentage` builds one by its bounds alone. A part may keep bounds
too (`Part::Bounded`). `BasisPoints` is a whole number (`Holds::Whole`),
whose text now takes a `-`. Two `FormatKey`s came with them: `Parts`
(named parts each shown as their own type is, joined) and `Suffixed` (a
number and a fixed unit, `25 bp`).
`money.apply_discount` (`Op::MoneyApplyDiscount`) takes a `Discount` off
a `Price` in `jadpo_value::money`.

**wasm32.** The runtime crates and the shared leaf build for
`wasm32-unknown-unknown`, checked by the gate, which reads the list from
the layer table, less the crates `[workspace.metadata.targets]` lists as
native only (`jadpo-host-native`); `jadpo-drivers` builds with every driver
feature on. `rust-toolchain.toml` pins the target with the toolchain. The
maths on `Decimal` runs there too, not only builds (chunk 5.5e): the gate
builds `jadpo-value`'s `decimal_maths` example for wasm32 and runs it under
Node (`tools/run_wasm.mjs`), holding it to the reference values the native
test checks. Compiler crates do no I/O below `jadpo-db`, which
keeps a browser playground possible later without gating on it now.

### 2.2 Source to graph

**Lexer and parser.** Hand-written. The lexer has modes for route paths
(`GET /todos/{todo_id}`), durations (`15m`) and template text (`{{ }}`).
The recursive-descent parser emits start, finish and token events that a
sink builds into a [rowan](https://crates.io/crates/rowan) green tree, as
rust-analyzer does: lossless, comments and whitespace included, for the
formatter and the language server. The sink attaches each comment to a
token by one rule (leading to the next token, trailing to the last token
on its line), as [Biome](https://biomejs.dev/internals/architecture) does,
so the formatter needs none of the comment-placement repairs Ruff's
AST-based formatter carries. The parser recovers at declaration and member
boundaries, never panics, and is fuzzed. An `if` with its `else if`s and
`else` is one flat list of branches in the tree and in the typed body, so a
chain's length is unlimited and never nests; real nesting (brackets,
blocks, template tags) is limited to 96 levels, past which the rest is
skipped with one error rather than recursing; a byte-order mark opening a
file is whitespace, and a line ended by a carriage return alone is an error
(chunk 1.9). The grammar is one
`jadpo.ungram`; the typed AST is generated from it and committed, with a
test that fails when they drift. *Rejected:* tree-sitter or LALR
generators, with weaker recovery and messages, and a second lossless tree
still needed.

**Habits from other languages** (chunk 3.8, decision register
2026-10-15). `jadpo_syntax::habits` is the catalogue of constructs
carried over from TypeScript and JavaScript, Python, Rust, Go, Java and
C#, Kotlin and Swift, and SQL (`let`, `fn`, `&&`, `===`, `;`, `#`, `T[]`,
`??`, `switch`, `import … from` and the rest), each with its rule, where
it is found and the words of its error; what it leaves out, and why, is
in its module comment. The lexer and the parser report each with its edit
and carry on as if the Jadpo form were written: a word Jadpo spells
differently is read as Jadpo's word, and one it doesn't have is kept in
the tree as `SKIPPED` trivia, read as nothing. When that is all the Jadpo
form changes, the error is clean, and a project whose syntax errors are
all clean (`Parse::checkable`) is checked too, leaving out other problems
on those lines; any other syntax error still comes alone. Names another
language gives a type or a value (`string`, `Vec`, `null`, `None`) are
the checker's, when they resolve to nothing (`type.foreign_type`,
`type.foreign_value`). Behind it all, a syntax error never brings a
second on its line: a parse keeps the first error a line. A corpus of
broken files (`crates/jadpo-syntax/tests/broken/`), the parser's fuzz
test, which also checks that clean edits converge on a file with no
errors, and a jadpo-check test that checks each clean habit against its
edited source hold it. The language server shows these errors and their
edits like any other, offering no quick fix for them yet; its code
actions are "Make this a setting" and the swap for a standard type's
finding (below), and the declines.

**Formatter.** Syntax tree to a document algebra to a printer, owning
indentation, blank lines and member order (areas 1 and 14). Every example
is checked to be idempotent and to parse to the same AST after formatting.
Since chunk 1.9 the formatter also reads its own output back: when it
doesn't parse to the same tokens, nodes and comments, or doesn't format to
itself, it writes nothing and reports `format.unsafe`, a formatter mistake;
a fuzz test feeds it the parser fuzz test's edits of every example. Since
chunk 3.5 it moves `use` lines to the top of the file and sorts them and
their names (`jadpo_fmt::imports`), reading the path alone; the canonical
form sorts them the same way, so the self-check and the digests take their
order for layout. Adding or removing an import needs the project's names,
which the formatter doesn't have, so `jadpo fmt` first applies the edits
of the file's import problems against a loaded project, round by round
until none is left (`jadpo_check::fix_imports`, in `jadpo-cli` until chunk
3.12, now where the language server can reuse it), then formats. Since chunk
5.5h3 it writes a file's declarations before their use (area 1, "Files and
names"; chunk 3.9 had put the primary declaration first): `jadpo fmt` reads
which declaration is the primary from the file's path and item tree
(`jadpo_db::primary`, the same answer the layout rules use) and passes its
name to `format_file_in_order`. That orders the declarations
(`jadpo_fmt::order`) by the names each writes: a declaration goes after
every one it uses, a group that uses each other stays in written order, and
among what is ready the choice is by kind and then by the order a walk from
the primary first reaches them, the primary's group last. Each moves with
the comments the formatter prints with it, below the opening comments and
`use` lines, before formatting, so the self-check compares the moved text
with its formatted form.

**Item trees.** Each file is summarised into its declarations and
signatures without bodies (rust-analyzer's `ItemTree`), so editing a body
never invalidates another file's resolution.

**Constants** (area 3, built in chunk 5.3e). A `constant` is an item like
a helper function, in the item tree with its type as its signature's
result and, in a library, among the library's members in source order. Its
name follows a helper function's scope and identity (a private one beside
its use is unique within its domain and keyed `constant billing.name`; a
library's is keyed `constant library.name` under its library). It is a
callable with a body: `typed_body` types its value as a `return` of its
type (`Context::Constant`, pure, where a construction never needs
`attempt`), and lowers it to a body of one `return`. A read is its own
graph expression, `Expr::Constant`, decoded to the constant's callable, so
both executors run that body where it is read; it can't fail. The value
itself is worked out once for the whole project while compiling
(`jadpo_check::constants::constant_table`), in exact arithmetic that
follows `jadpo-value`'s, from literals, enum values, other constants,
constructions, objects, lists and operators: every construction is
checked against its type's rules there, a call, `config` or `clock` is
`constant.value_not_fixed`, a loop is `constant.cycle`, and two constants
of one type holding one value are `constant.similar_exists`, a
declinable error (area 1, question 11; chunk 5.4b). A query
reads the worked-out value as a literal (or a plain variant, or a list of
them), and the graph's constant node, the manifest and the library
catalogue carry it as Jadpo writes it.

**Resolution.** One project-wide name table, because names are unique
across the project and across kinds (area 1), except a private function or
type, which is unique only within its domain (decided 2026-10-16, built in
chunk 5.3d). The table holds every declaration of a name with the scope
it is unique in (`Symbol::scope`), and a name resolves among its file's
domain's private names and the project-wide ones, which never overlap: a
`Lookup` made for a file (`Lookup::for_file`) reads names as that file
writes them. A private name declared in more than one scope is
*contested* (`jadpo_db::contested`, usually empty, so what reads it is
rarely recomputed), and the type and signature tables know it as
`<domain>.<name>`, which no declaration can be called: each file's shapes
and signatures are read through it before the tables are built, so a
`Ty::Named` always names one type, even one that reaches another domain
through a public signature, and is shown as written. Duplicates and folded
names are checked within a scope (`naming.rs`); a private type an API's
record names (`routes::api_named`) stays unique across the project. A
key carries the domain of a function or type declared in one (area 18,
question 8), and lookups read such an identity, `billing.Address`, as
the name it is. A dotted prefix resolves to
its owner: domain, library, entity, connection, package or standard
namespace. Visibility is the domain unless `public`. Layout rules (folder,
kind, file name, promotion) are checked from paths. Since chunk 2.1 a
file's resolution query resolves its declarations only (types, fields,
signatures, a query's parameters); a body's names are resolved by the walk
that types it (`typed_body`), at each node it checks and over the same
scopes, so name errors and unused-variable errors come from one walk, and
a changed signature no longer re-resolves the files that call it. Since
chunk 2.7 a call names one owner (a call through three names is reported,
except a connection's, which is not built yet), a collection type is never
a constructor, and a call that reaches nothing is always reported, so no
call checks clean and then faults as something that can't be called.

**Imports** (chunk 3.5, area 1). `imports::import_problems`, a query per
file, compares the names a file writes with its `use` lines. What a name
can be is read from where it stands, as written: an upper-case name first
in a path or chain is a type, entity, failure or event; a bare call's
callee is a function, action, query or fixture of the file's own domain;
the name before a dot is a domain, library or connection. So it reads only
its own tree and, through the per-name lookups, the entries of the names it
writes and imports: no other file's text, and no typed body. A name is
brought in from the folder its symbol's file is in (`<domain>/<folder>`,
`shared/<folder>`, a root file), a domain by `use <domain>`. What
resolution reports (an unknown name, a private one, domain code in
`shared/`, a bare call into another domain) is never asked for as an
import, so one mistake gives one error. Each problem carries its edit;
`import_edits` gathers the ones that don't overlap for `jadpo fmt`.
Since chunk 3.9 `usage_problems` also reports each type or helper
function beside a file's primary declaration that the primary doesn't
reach through the names its declarations write, `layout.unused_declaration`;
it reads the file's own tree and, per supporting declaration, the per-name
users, so one another file uses is left to `layout.promotion_required`;
while what the primary reaches writes a name that names nothing (an
unknown type, or an unknown callee in its typed body), it reports none.
`layout.promotion_required` on a type carries the edits that move it
(`imports::move_edits`): out of its file with its attached comments, into
a new file below the `use` lines it needs, and imported back into the old
file when that still uses it; `jadpo fmt --layout` applies them one move at
a time, checking again after each. The language server's lookup reads a
`use` line's names as the declarations they bring in.

**Making a constant a setting.** `jadpo_check::setting_move` (chunk 5.3f,
area 10) is the code `jadpo config move` and the editor's "Make this a
setting" share: `plan` reads the checked project and decides the setting,
its `default:`, every read (from each body's resolved names, and the
syntax outside bodies) and every refusal, editing nothing; `apply` edits
the loaded texts, fixes imports, formats what was formatted and checks
the result, undoing a move whose result wouldn't check. The command
writes the files it gives back; the language server sends them as one
workspace edit and puts its own texts back with `restore`.

**Fixing a standard type's finding.** `jadpo_check::fixes` (chunk
5.5a2, area 19's question 4) is the code `jadpo fix` and the editor's
"Swap for the standard ..." share, built as `setting_move` is: `findings`
reads `type.standard_clash` and `type.standard_exists` from the check,
`plan` decides each edit (a reference by its name in the files that can
see the declaration, a matched field by what each body name resolved to
and by `Type.field` as written, a `use` line, the declaration's file
moved for a rename) and every refusal, and `apply` makes them, fixes
imports, formats what was formatted and checks the result, undoing a fix
that adds an error by rule and headline. Each standard type's data entry
carries the `release` that added it, which the clash names.

**Types.** Nominal and bidirectional: the expected type flows into
literals (area 2). Widening is the only implicit conversion; the standard
library's signatures are the only generics. An `Error` type absorbs
anything built from it, so one mistake gives one error. Body checks
follow: definite assignment, unused variables, `attempt` placement, failure
sets, effect rules per callable kind, reads in loops, one store per action,
calls before writes.

**Diagnostics.** The catalogue is a `rules!` table in `jadpo-diag` (rule
id, owning area, whether it is declinable, summary, who decides, help
anchor; every finding is an error, area 1's question 11), used through
typed constants, so a misspelt rule does not compile. A diagnostic carries
the rule, primary and secondary spans, reason, next step with optional
edits, and who decides (`agent` or `human`). `jadpo-term` renders it rich
or plain per the design language, writing control characters and
bidirectional overrides from source as `\u{...}` so they can't restyle the
terminal or reorder code (chunk 1.9); `--json` carries `schemaVersion`,
project-relative paths, and line and column beside byte offsets. The
catalogue is part of `jadpo help --json`, which the docs check reads.
*Rejected:* v0's `build.rs` scan of emitter strings, which let codes drift.

### 2.3 The graph

**Representation.** Per-kind arenas with typed `u32` ids
([la-arena](https://docs.rs/la-arena/latest/la_arena/)) and interned names,
rebuilt rather than mutated and shared behind `Arc`; nothing is deleted in
place, so generational keys would be wasted. In-memory ids are never
serialised. *Rejected:* a general property graph with string labels, which
loses the typed shape every consumer relies on. Built in chunk 1.7 as one
key-sorted node list with typed `NodeId`s and a typed detail per kind; since
chunk 1.8 a detail's types and rules are typed too (`TypeRef` names declared
types, entities, fields and variants by identity key; rules are
`jadpo-std`'s `Rules`), and the manifest prints them as Jadpo writes them. Since chunk 2.7 each field
records its place among its owner's fields (`order`), copied fields where
the line that copied them stands, and every reader takes fields in that
order rather than by key or source offset.
Each callable's typed body (below) is its own arena, which both executors
walk (§2.5).

| Node group | Kinds |
|---|---|
| Structure | domain, library, file, deployable |
| Types | type (base `Object`, `Enum`, `Input`, `Output`, `Failure`, `Event`, `EmailMessage` or a scalar refinement), field, variant, constraint |
| Data | entity, unique rule, index, reference, lifecycle (visible, transition, purge), cache, store placement, system table, personal mark, retention, person |
| Behaviour | function, action, query, fixture, seed, test, site, branch |
| Triggers | API, route group, route, schedule, subscriber, generated route (sign-in, webhooks, health, privacy) |
| Access | principal, authentication method, sign-in provider, grant (entity, effect, who), field restriction, tenancy, action exception, OAuth scope |
| Outside | connection, package, resource, operation, capability mapping, host |
| Services | config field, locale set, translation key, email template, flag, rate limit, threshold |
| Records | API record, migration snapshot, registry entry, acceptance, lock entry |

**Edges:** calls, reads and writes (entity, fields, operation), emits,
subscribes, runs (trigger to action or query), raises, maps (failure to
failure), references, includes, grants, scopes (tenancy), invalidates
(write to cache), uses (type, config, connection, translation key, flag),
provides (package to capability), reaches (deployable to node) and covers
(test to node).

**Bodies.** Each callable has a typed body: an arena of statements and
expressions, each with a site number and source range. Effects are their
own expression kinds (store read, store write, emit, integration call,
clock, new identity, flag read), which replay, `why` and coverage need.
Queries are declarative nodes, not bodies. A control-flow graph derived
per callable gives Ball–Larus path numbers, branch coverage and exact
review paths, with loop back edges cut as area 16 decides. Executors walk
the structured body; reports read the control-flow graph.

Built in chunk 1.8 (`jadpo_graph::body`), for functions, actions,
fixtures, library functions, tests and seeds: one salsa query per callable
(`typed_body`) checks it and lowers what the check decided, so the body,
the diagnostics and everything read from the body agree. Sites are numbered
in source order (a site's number is its index), each with its range, line
and type; a literal's type is the type it goes into. A call names what it
reaches: a function, action, query, fixture or library function by key, a
standard-library operation by the operation table's `Op`, a constructor by
its type, an object or variant by key, or an entity operation (area 6). A
construction lists the rules it can break, its `attempt` the failure each
rule maps to (an arm per rule, `invalid:` for the rest), and the body lists the failures that can leave it.
Since chunk 2.6 a construction's proof reads the declared ranges and what
the `if` conditions around it say (area 2, "Conditions as proof"): the
walk keeps a bound per compared number as it goes, from each branch's
condition and from what holds after an `if` whose other branches leave,
forgetting a `var mut`'s bounds where it is assigned and around a loop
that assigns it. The bounds live only in the walk, so `typed_body` still
depends only on the names it reads, and the body lists each construction
a condition proves with the comparisons that prove it, which the errors at
a site that flips point at. A target with a fixed `scale` has its proven
range rounded as the value will be. Locals are
numbered with parameters first, and a table records what every name in the
body refers to. Every reader goes through it: the graph's `calls`, `uses`
and `raises` edges, usage and promotion (area 1), action cycles (area 3)
and the language server's hover and definition, so a name means one thing
everywhere. The graph's `site` nodes are the sites the runtime can name in
a frame, a failure or a fault (calls, constructions that can fail,
`reject`, `propagate`, arithmetic); its `branch` nodes are each way through
an `if` chain (each condition's block, and the `else` or the way past it,
written or not) and each arm of a `match`. Both are keyed
`<kind> <callable>.<number>` under their callable and numbered per build,
so a diff leaves them to the callable's own change (area 18). Effect kinds,
the control-flow graph and path numbers come with their areas.

Since chunk 3.1 the data nodes are built (area 5): an entity carries its
store and table; under it sit a `store_placement` node, a `unique_rule`
node for each compound rule and an `index` node for each declared index
(each with its fields and the name the database knows it by, keyed
`unique_rule Todo.name_per_owner`), and under each reference field a
`reference` node with its target and `on_delete`, beside a `references`
edge from the entity to the one it points at. The project's
`system_table` nodes (`system_table main.jadpo_outbox`) and its
`migration_snapshot` nodes, one per migration folder with the digest of
its `snapshot.json`, come from no source file and are added when the graph
is assembled (`graph::records`).

Since chunk 3.4 an entity's lifecycle is read once into a
`LifecycleModel` on its `EntityModel` (`jadpo_check::data::lifecycle`):
`visible:` and each transition's `when:` as the queries' `Condition`s on
the record's own fields, `initial:` and each transition's `set:` as fixed
values, whether a `when:` can refuse a visible record, the fields the
lifecycle owns, and `purge:`'s period and field. Under the entity sit a
`lifecycle Todo` node (its `visible:` and purge, as written) and a
`transition Todo.delete` node for each transition (its `when:`, whether
it is guarded, and the fields it sets), which a `conflict:` arm mapping a
transition's refusal names. `data::write_rules` adds what a body's writes
need: whether the entity has a lifecycle, the fields it owns, its
transitions, and a reference through which removing a record would
cascade to an entity with a lifecycle.

Since chunk 3.2 a query is a built node (area 6): `jadpo_check::data::queries`
reads each declaration once into a `QueryModel` (its entity, parameters,
how many records it returns, the record's fields from the entity's fields
and its includes, its conditions, its order with `id` added as the
tie-break, its limit or page and its `missing:` failure), which the
checks, the derived indexes, the graph and `jadpo-plan` all read. The
graph gives a query a `reads` edge to its entity, an `includes` edge to
each entity it includes and a `raises` edge to its `missing:` failure; a
body calling `Entity.get` gets a `reads` edge too.

Since chunk 2.10 each site says whether it can pause (§2.5): an effect (a
store read or write, `emit`, an integration call, `clock.now`, a new
identity) or a call to an action, query or fixture, which the runtime
enters through its action boundary; the body says whether any site does.
Functions never pause (area 3). A construction's rules and its `attempt`'s
arms name rules by `jadpo_std::RuleKey`, which a broken rule carries too,
so nothing reads a rule back out of its text. Each variant records its
place among its enum's variants (`order`, printed in the manifest), which
sets order variants by. `typed_body` returns its body behind `Arc`, and the
graph query its graph, so a reader keeps a reference, never a copy.

**Identity.** A node's key is its kind and born name through the registry
(`action complete_todo`, `field Todo.title`), not qualified by domain, so
promotion to `shared/` keeps it (built in chunk 1.7; area 18, question 8).
The registry's lines are a field of the project's salsa input, so a rename
it records re-keys exactly what it touches (chunk 1.8). Its digest is BLAKE3 over its canonical
form without positions or comments, so moving code changes neither. Sites
and branches are numbered per build in source order (area 18). Every node
records its project-relative file and byte range; the manifest prints
lines.

**Canonical JSON.** A dedicated writer rather than `serde_json`'s map
order: sorted keys, nodes sorted by key, edges as sorted lists, two-space
indent, LF, no floats or timestamps, a top-level `format` (area 18), its
schema in `jadpo help --json`.

**Diff.** One operation: match nodes by key, compare digests, then let each
kind classify its change into the decided kinds (`ApiChange`, `DataChange`,
`PolicyChange`, `DomainChange`, `PackageChange`, `ThresholdChange`), other
behaviour, and newly reachable nodes. The before side is read from the
base commit through `git cat-file --batch` into an in-memory file set,
checked by the same compiler and cached in `build/before/<commit>/`.

**Packages.** A package's index is always parsed, a resource file only when
the app calls into it. A checked package subgraph is cached on disk by
package digest and compiler version (`~/.cache/jadpo/packages/`), so an
unchanged package costs nothing after its first check (area 11).

### 2.4 Incremental engine

**Decision:** [salsa](https://github.com/salsa-rs/salsa) 0.28, pinned
exactly and confined to `jadpo-db` and `jadpo-check`. It gives early
cut-off (a recomputed result equal to the old one stops invalidation),
durability levels, and the cancellation the language server needs when a
keystroke arrives mid-check; rust-analyzer and Astral's ty run on it. Its
costs are heavy macros, breaking releases and
[cancellation by unwinding](https://docs.rs/salsa/latest/salsa/enum.Cancelled.html),
so the `jadpo` binary keeps `panic = "unwind"`. *Rejected:* a hand-rolled
per-module cache, as
[Gleam](https://gleam.run/news/v0.26-incremental-compilation-and-deno/)
keeps by modification time: enough for `check`, too coarse for the editor,
and invalidation is where hand-rolled caches go wrong.

| Query | Keyed by | Recomputed when |
|---|---|---|
| `parse` | file | its text changes |
| `item_tree` | file | its declarations or signatures change, not bodies |
| `symbols` | project | an item tree changes |
| `signature`, `type_def`, `name_facts`, `failure_info` (per-name lookups) | name | that name's entry in its project table changes |
| `typed_body` (resolved, typed, diagnostics) | callable | its file's text, or a name it reads, changes |
| `body_facts` (records it reads, creates, changes and deletes, fields set, calls, queries, session operations; no positions) | callable | what its typed body does past its own values changes; never for a comment (chunk 4.11) |
| `import_problems` | file | its text, or the entry of a name it writes or imports, changes |
| `effects` | callable | its body or a callee's effects change |
| `entity_read`, `query_read` (model, positions, problems) | entity, query | its file's text, or a name it reads, changes |
| `entity_model`, `query_model`, `write_rules` (no positions) | entity, query | that model, or a model it reads, changes; never for a comment |
| `data_problems`, `query_problems` | file | a read of what the file declares changes |
| `schema`, `migration_problems` | project | any entity's or query's model changes |
| `identity`, `api_table` | project | `app.jadpo`'s or `authentication.jadpo`'s declarations change (chunks 4.1 and 4.3) |
| `group_read` (model, positions, problems), `groups`, `group_names` | route group, project | the group's file, or a name it reads, changes |
| `route_at` (what a test's `request` reads of its route) | route, by label | that route's key, placeholders, input, output or need of a caller changes (chunk 4.11) |
| `route_problems`, `table_problems`, `signin_problems`, `record_bodies`, `record_problems` | file, project | a group's model, the identity, or a type a route sends changes; the record waits on every route reading cleanly |
| `policy_read`, `membership_read` | entity | that entity's `policy:` or `membership:`, or a name it reads, changes (chunk 4.4) |
| `entity_policy` (no positions) | entity | that entity's resolved policy changes; never for a comment (chunk 4.11) |
| `root_read`, `root_policy`, `memberships`, `access`, `policy_clean` | project | root `policy { }`, or any entity's policy or membership, changes; `root_policy` holds no positions |
| `policy_problems`, `query_policy_problems` | file | a read of what the file declares, or the policy of an entity its queries read, changes |
| `acts_of` | action, by name | the `body_facts` of its declarations change |
| `reach_problems` | file | its route groups, the `acts_of` of an action they reach, or the policy of an entity they reach, changes (chunk 4.11) |
| `flow_summary` (what flows to its result and its outputs, by parameter; no positions) | callable | its typed body, a callee's summary, or the policy of an entity it reaches, changes; calls that go round are a cycle, solved by fixpoint iteration (chunk 4.11) |
| `flow_problems` | file | its bodies, a summary they call, or the policy of an entity they reach, changes |
| `client_fields` | failure | that failure's `client:` block changes |
| `graph`, `diagnostics` | project, file | anything above; assembly only |

The plan (SQL, codecs, sites; `Plan::build` in `jadpo-plan`) is not a
query: it is built whole from these results each time a run needs it
(see "The plan is built whole" below).

**Per-name lookups (chunk 1.8).** The project tables (symbols, types,
signatures, failures, who uses what) are rebuilt whenever any declaration
changes, which is cheap; nothing that checks a file reads them whole.
Each check reads them through per-name queries that copy one name's entry
out, so when a table is rebuilt and the entry comes out equal, salsa's
early cut-off stops there and the check depends only on the names it reads.
Suggestions on a mistake ("did you mean") read a table whole, so only code
that already found a mistake depends on every name. In a project of 2,003
files (`crates/jadpo-check/tests/scale.rs`, release build), before and
after: a cold check took 850 ms and takes 82 ms (the naming rules no longer
compare every pair of declarations); a rule on a type one file uses
rechecked all 2,003 files' bodies in 18 ms and now rechecks that file in
5 ms; an uncalled function's signature rechecked 2,003 bodies in 17 ms and
now rechecks its own file in 6 ms; a new file nothing reads rechecked
2,004 files' bodies, resolution and naming in 833 ms and now checks only
itself in 12 ms. The test asserts the counts, not the times. Since chunk
3.5 each action file in it imports `Count` and `import_problems` is among
the checks counted: a body edit, a rule on a type one file uses and a new
file each recheck one file's imports, and a rule on the type all 2,001
files import rechecks the imports of its own file only, since its entry
in the name table doesn't change (release build: cold 128 ms, a body edit
8 ms, that rule 39 ms with its 2,001 bodies).

**Entities and queries by name (chunk 3.13).** Areas 5 and 6 are read
the same way. `entity_read` and `query_read` read one declaration, by
name, into its model and its problems; `entity_model` and `query_model`
copy the model out, and the model holds no positions (where each part is
written is kept beside it, for the problems), so a comment or a blank
line changes the read but not the model, and early cut-off stops there.
Each file's `data_problems` and `query_problems` collect the problems of
what it declares. What crosses entities (references to another store,
cycles of required references, the references a delete reaches) is
worked out from the models (`entity_links`, read per entity through
`links_of`), and the graph's parts, the one-store rule, the write rules
and the migration check read the models by name too. The schema, the
migration check and the plan read every model, so only a change to a
model reruns them. The scale test now has 2,303 files: beside the
actions, 100 entities, each with a query reading it and an action
creating its records. Before and after, in a release build (checking,
then building the graph): a comment above an entity reran every file's
`data_problems` (2,303), 100 graph parts and 100 entities' write rules,
with the query models, the schema and the migration check, in 16 + 10 ms,
and now rechecks its own file only, in 10 + 6 ms; a comment above a query
reran every file's `query_problems` and 100 graph parts in 13 + 10 ms,
and now rechecks its own file, in 10 + 7 ms; a rule on one of an entity's
fields reran every file's `data_problems`, 101 graph parts and 100 write
rules in 18 + 12 ms, and now rechecks the three files that name the
entity (it, its query and the action that creates it, whose signatures
read its fields), the one body that writes it, its write rules and the
schema, in 14 + 7 ms; a body edit in an action that writes rechecks its
own file, as before (11 + 7 ms). A new file nothing reads also reran
every file's one-store check, which read the whole table of what each
action writes; each file now reads its own actions' entries, and the new
file is the only one checked (39 ms to check before, 25 ms now). The test
asserts the counts.

**Policy, routes and flow by name (chunk 4.11).** Areas 7 to 9's checks
read what they need the same way. What a body does past its own values
(the records it reads with `get`, creates, changes and deletes, the fields
each write sets, the queries, actions, fixtures and functions it calls
and its session operations) is one query per callable, `body_facts`,
holding no positions; isolation (`file_loading`), the one-store rule
(`store_reaches`), policy's reach (`acts_of`) and the lines the flow
check reads under `actions:` all build on it, where each walked the typed
bodies itself. Each entity's policy is read by name (`entity_policy`,
through the `Policies` view, which the proofs' `Prover` reads lazily, an
entity and a membership's roles at a time), so a check depends on the
entities it proves against, not on `access` whole. Each route group's
file checks its own routes (`reach_problems`, from the project-wide
`policy::reach::route_problems`). The flow check is a summary per action
and function (`flow_summary`), in terms of its parameters, which a caller
reads, and each file's problems walk that file's bodies (`flow_problems`),
where one project-wide `flow_report` walked every body again on any edit.
Calls that go round (a function calling itself, or functions calling
each other) are a cycle of summaries, which salsa solves by fixpoint
iteration: a summary starts empty, each round is joined with the last, so
it only grows, and a part followed deeper than eight fields, which only a
recursive type gives, is widened to everything inside it, so the rounds
end. The summary a caller then reads holds everything that may flow
through the cycle, never less; before, a call back into a function being
walked read as nothing, so `pick(first, second, swap)`, giving back
`second` only through its call to itself, seemed never to give back what
it was passed second (crates/jadpo-check/tests/policy.rs). A test body
reads the route a `request` names, by label (`route_at`), and the whole
table only to list the paths when there is none, where every test body
read every route. The scale test now has 2,376 files: beside the above, a
principal signing in with a browser session, ten entities with policies,
each with a field only its owner reads, a query, an action creating its
records and a route group running both, with two tests each, one sending
a request, and a route anyone may call. Before and after, by what ran
again: a comment above an action that writes a note walked every action's
and function's flow again (`flow_report`), and now checks its own file,
its flow included, and nothing else; a field's own policy on one entity
rechecked every query file (110), every route and every action's flow,
and now rechecks the four files that read that entity's policy (its own,
its query's, the action creating its records and its routes'); a route no
test sends a request to rechecked all 21 test bodies and now none; a route
one test requests rechecks that test's body alone. A test file also
checked that its subject is beside it by reading every file's path, so a
new file rechecked the layout of every test file; it now reads its
subject's path alone. Every page's and example's diagnostics, positions
included, were compared before and after and are the same. The test
asserts the counts.

**The plan is built whole.** `Plan::build` is not incremental: each run
that executes the project (`jadpo test`, and each good save under
`--watch`) builds the plan again from the checker's memoised results
(the typed bodies, the graph, the models), numbering callables,
constants and type plans and decoding every site in one pass. Reusing an
unchanged callable's or entity's piece would need those numbers to stay
the same from one build to the next, since a decoded site names its
callee, constants and type plans by number: a plan relinked from
memoised pieces, in `jadpo-plan` and the interpreter that reads it. For
the scale test's project, after a body edit, it takes 5 ms against 11 ms
to check and 6 ms to build the graph (release, measured once, not
asserted), spread across collecting the bodies, the queries' and
entities' SQL, the write sites and decoding the sites, none over 2 ms, so
no one part is worth memoising alone. It is worth revisiting if
watch-mode timings show it.

Inputs carry durability: standard library and packages high, root files
medium, domain files low, so a keystroke never revalidates packages.
`jadpo lsp`, `jadpo dev` and `jadpo test --watch` keep one database for
their lifetime; `jadpo check` starts cold with only the package cache.
Affected tests are those that reach a node whose digest changed, by
reverse reachability over `calls`, `runs`, `reads`, `uses` and `covers`:
changing a type's rule re-runs the tests that reach code using the type.

**Watch mode (chunk 2.3).** `jadpo test --watch` (`jadpo_test::watch`)
keeps one database and compares each good check's node digests with the
last run's; reachability also climbs from a field, variant or site to what
holds it, since what uses a record uses its fields. A broken build runs
nothing and keeps the last run as the baseline, so the next good save runs
everything changed since. Saves are found by polling modification times
and sizes (every 40 ms, or ten times a look's cost) and gathered until the
files are quiet for 50 ms: no platform file-event API or dependency, and
the same on every file system. On a generated project of 230 files and
312 tests in 12 domains (`crates/jadpo-cli/tests/watch.rs`), saving a
library's first step rechecks that file's 10 bodies and resolves 1 file,
and reruns the 26 tests that reach it, in 5 ms release and 39 ms debug;
from the save on disk to the results written by the real loop, poll and
quiet period included, took 61–71 ms release and 97–111 ms debug against
the 1-second target. The gate asserts the counts and a 1.25-second
ceiling: the target plus 250 ms for a loaded CI machine, whose slowest save
took 138 ms in a debug build (chunk 2.8).

Since chunk 2.10 the session keeps the last run's graph, shared rather than
copied, and `watch::follow` classifies every edge kind: followed backwards
where the source depends on the target (`calls`, `reads`, `writes`,
`emits`, `runs`, `raises`, `references`, `includes`, `uses`, `covers`), so
a change climbs to whatever reaches the source; attached forwards where the
source changes how the target behaves (`subscribes`, `maps`, `grants`,
`scopes`, `invalidates`, `provides`), so a changed or removed source counts
as a change to its target; and irrelevant (`reaches`). A changed subscriber
is thereby a change to its event, which reaches the actions that emit it
and their tests. The match names every kind, and a test runs each through
a small graph, so a kind added later can't be left out.

### 2.5 Executors

**One contract.** Both executors consume the graph's typed bodies plus
`jadpo-plan`, and every effect and standard-library operation goes through
one per-invocation context in `jadpo-runtime`:

| `Ctx` operation | Used for |
|---|---|
| `read`, `write` (plan, parameters) | Queries and entity operations; SQL, policy and lifecycle come from the plan |
| `emit(event)` | Outbox row in the action's transaction |
| `call(operation, input)` | Integration call with idempotency key, retries and failure mapping |
| `now`, `new_id`, `flag`, `config` | Effects with one source each, recorded for replay |
| `enter(callable, site)`, `leave`, `branch(increment)` | Frames and path numbers |
| `fail(failure)`, `fault(kind, site)` | Failures and defects with their `.jadpo` place |
| `assertion_begin`, `record(site, value)`, `assertion_end` | A failed `assert`'s parts, recorded the same way by either executor (chunk 2.10) |

The runtime, not the executor, owns the action boundary: it opens the
transaction at the first store operation, runs the body (a closure either
executor supplies), reruns it on a concurrent change within the budget
(area 6), commits, then leaves the outbox for the runner (since chunk
6.1a; §2.6) and clears caches. Limits
(recursion depth, events per action, event size, chain depth) are runtime
constants. Only control flow and value representation differ between the
executors, so divergence can only come from those.

Built in chunk 2.10: `jadpo_runtime::action` runs an `ActionBody` either
executor supplies, inside the running action's boundary when there is one
(an action another action calls joins its transaction, area 1), otherwise
in a boundary of its own, one per action or fixture a test calls (a fixture
is a test-only action, area 13). The transaction begins at the body's first
store operation (`Ctx::transaction`); a concurrent change runs the whole
body again, up to `RERUN_ATTEMPTS` (3) times, then fails `unavailable`; a
body that fails rolls back; and what it emitted is delivered only after it
commits. The second of area 6's budget waits for the host's clock (no host
reads one yet; chunk 5.2 brings time), and caches come with area 5. Since
chunk 3.10 an action called inside another whose failure the caller
handles (`ActionBody::caught`: the call is an outcome `match`'s subject or
an `assert … fails`'s) runs inside a savepoint: when it fails, the
transaction returns to it, undoing its writes, a refused statement among
them, and its events; when it began the transaction itself, the
transaction is rolled back and the caller's next store operation begins
another. (Chunk 3.3 savepointed only each refusable write, which kept a
failed action's earlier writes.)

Since chunk 3.7 the boundary begins its transaction at the isolation level
the body names (`ActionBody::isolation`), which both executors take from
the plan: an action or fixture that loads no record, itself or through any
action it calls, runs at read committed, and every other at repeatable
read (area 6, "Counters and other one-step updates"). `jadpo-check` decides
it: each file's actions and fixtures are read once from what their bodies
do, without positions (`file_loading`, from `body_facts` since chunk 4.11:
whether one reads a record with
`get` or a query, is handed one in a parameter, writes one back rather than
by `id`, or runs a transition whose `when:` can refuse a visible record,
and which actions and fixtures it calls), and the project's decision closes
that over the calls (`isolation`), as `written_stores` does for stores. The
graph carries it on each action and fixture node, the manifest writes it as
`isolation`, and `CallablePlan::isolation` hands it to the executors, so
the interpreter and compiled code begin the same transaction. An action
called inside another joins its boundary at the caller's level, which
already counts the callee; a read or write run outside any action (a test's
own read, later a route's display read) runs at repeatable read, since a
read loads what it reads and its includes need one snapshot, which the
runtime's store entry point decides (since chunk 3.12; the interpreter did
until then).

**Interpreter.** Functions are pure (area 3) and run synchronously. Only
actions, queries and tests are `async`, and the checker knows which
expressions can suspend, so only those frames are boxed futures
(`Box::pin`, allowed for recursive `async fn` since
[Rust 1.77](https://blog.rust-lang.org/2024/03/21/Rust-1.77.0/)). Values
are an `Rc`-shared `Value` enum, read and written by codecs driven by the
same type plans the generator uses. It runs on `jadpo-host-native`, so
`dev` and `test` exercise the real HTTP, driver and outbox code.
*Rejected:* compiling to bytecode first; a tree walker over checked bodies
is fast enough when the real cost is I/O.

Built in chunk 2.1 (`jadpo-interp`), for pure logic: `Program::load` reads
a project that checked without errors (each callable's typed body, the
graph's types, fields, variants and failures, and the places of every
site), and `Program::call` runs one callable with argument values,
synchronously, giving its value, the declared failure that left it, a
fault, or a test's failed assertion. Operators and the standard library
run through `jadpo-value`'s one implementation: arithmetic is checked, so
overflow is the `int_overflow` or `decimal_overflow` fault; `Int` `/`
drops the remainder; a `Decimal` `/` is exact to 18 places past its
operands and, like any computed decimal, is rounded half to even where a
type with a fixed `scale` stores it (since chunk 2.7 the scaled dividend is
worked out as wide as it needs, so large operands keep all 18 places, and
a quotient is `decimal_overflow` only when it is itself past 38 digits or
38 places; decimals compare exactly whatever their scales, through the one
comparison `jadpo-std` gives the checker's bounds too); constructions run their rules only
where the checker couldn't prove them from the declared ranges and the
conditions around them, and a broken one becomes the
failure its arm names, or `invalid:`'s (a validated built-in type's own
check, such as `Email`'s, is the `format` rule); an object literal never
fails. An outcome `match` and `assert … fails` work out their call's
arguments first, so only the call's own failures reach their arms. Effects later areas build (`emit`)
end in the `not_built_yet` fault where they are reached; since chunk 3.2
queries and `get` run, since chunk 3.3 `create`, `update` and `delete`
(below), since chunk 3.4 a lifecycle's transitions, which lower to the
same `Write` node with the transition's key, and `clock.now` in an action reads the invocation's clock
(`Ctx::now`), the one generated fields and a query's `clock.now` read. A call walks the Rust stack a few frames per
Jadpo frame, so recursion to the depth limit runs inside
`on_large_stack`; hosts, `async` frames and codecs come with their areas.

Reshaped in chunk 2.10 for Phase 3. One walker runs every body
(`jadpo_interp`'s machine): it keeps its own stacks of tasks, values and
activations instead of recursing on Rust's, so it can stop at any site the
typed body says pauses and carry on from exactly there. Functions run on it
synchronously; a call to an action or fixture hands back to a small async
driver, which enters the runtime's action boundary with the callee's body,
run on a fresh machine, and resumes the caller with the result. Only that
nested call's future is boxed, so a body that never pauses allocates none,
and a recursion reaches the depth limit without growing the thread's stack
(only an inline function inside a collection operation recurses on it, so
`on_large_stack` now gives 64 MiB). The context is handed to each step and
never kept. Everything the walk needs besides control flow comes from
`jadpo-plan`: callables numbered with their labels, files and lines; every
site decoded once (a literal's constant, a call's callee, the shape and
slots a build fills, the failure a `reject` or an arm raises, where a
report hangs the site's value); type plans with declared field order,
validator chains, field defaults, set and list kinds and each variant's
place; and failure shapes with their sections. `Prepared` (the plan, its
validators compiled and the frame table) is shared by every thread behind
`Arc`; a `Program` is one thread's view, with the plan's constants and
shapes decoded into `Rc` values, and `Program::codec` builds a value of a
type from its parts the way a body does. Sets and maps keep the one
canonical order area 2 decides ("Set order"), which is `Value`'s `Ord` in
`jadpo-value`: by value, numbers numerically, text by code point, variants
in declared order, objects and tagged unions field by field.

Since chunk 3.2 a site that pauses names its target: a callable, a
query's plan, an entity's `get` or, since chunk 3.3, a write's plan. A
write (one typed `Write` node for `create`, `update` and `delete`) works
out its inputs first, in the order its plan lists them (its record, then
the values its block gives, each kept the way its field's type keeps it),
then pauses. Since chunk 3.12 the runtime has one entry point for both,
`jadpo_runtime::store(ctx, StoreOp::Read | StoreOp::Write, failures,
args, site)`: inside the running action's transaction, or, called outside
any action (by a test, later by a route's display read), in a boundary of
its own, as a one-statement action. It gives back `Stored::Value` (what
it read, or the record written), `Stored::Failed` (the failure the plan
names when a read for exactly one record finds none, `missing:` or
`<Entity>NotFound` for `get`, or a write by `id` finds no record, its
fields from the arguments; a reference's not-found failure with the
store's refusal as its cause; `unavailable` or an invalid cursor),
`Stored::Conflict { arm, cause }` (a refusal the site's `conflict:` arm
maps), or `Stored::Fault`. The executor is left the arm dispatch: it
raises the arm's failure, working out that arm's fields only then, with
the store's refusal as its internal cause. Until chunk 3.12 the
interpreter turned a missing record and a refusal into failures itself.

**Code generation.** `quote!` into a `syn` tree, printed with
[prettyplease](https://github.com/dtolnay/prettyplease): deterministic,
never fails to format, about twenty times faster than rustfmt, and
readable when the executors differ. One module per domain; a struct or
enum per type with codecs from the type plan; functions as `fn`, actions
and queries as `async fn(&mut Ctx, …)`; checked arithmetic that returns a
fault, never panics. *Rejected:* string templates, whose malformed output
only rustc catches.

**Keeping rustc time down.** The runtime crates are compiled once per
Jadpo version and target and cached, so a build compiles only the thin
application crate. Generated code has no generics, because monomorphised
copies drive the LLVM IR rustc must optimise
([compile-time guide](https://nnethercote.github.io/perf-book/compile-times.html)).
Tests run interpreted; compiled builds happen only in the differential
run (dev profile) and for deployment (release). The differential run
compiles every corpus project as a module of one crate: one rustc
invocation for the corpus.

**Frames.** Both executors keep a shadow stack of `(callable, site)` in
the context; each site maps to a `.jadpo` file and line through a table
from the graph (a static array in compiled code). A fault renders its
frames from that stack, with `runtime: <component>` for the runtime's own
work, so nothing depends on unwinding or debug information, which Workers
does not keep (area 18). Path numbers come from the same branch increments
in both. Built in chunk 2.1: `jadpo-runtime`'s `Ctx` keeps the stack
(`enter(callable, site)`, `leave`), `fault` and `fail` capture it, and
`Places` turns a frame into `file:line  label`; past `MAX_DEPTH` (1,000)
frames a call is the `recursion_limit` fault at its site. A failure keeps
the stack where it was raised, so the path it travelled, and the failure
it was mapped from as its cause (area 4). A `Report` renders a fault, a
failure or a failed assertion in the shape compiler errors have: the
headline (`fault` or `failed`), the `.jadpo` site with its code frame, then
`reason`, `next` and the frames, or a failure's code, kind, fields, cause
and path. Since chunk 2.10 outcomes and reports live in `jadpo-runtime`,
and a report is read from `Places` (each site's line, bytes and anchor)
and the sources alone, so it is the same whichever executor ran the call;
`Places::collapse` counts a recursion once for every stack a person reads.

**The test runner.** Built in chunk 2.2 (`jadpo-test`, `jadpo test`): it
finds the tests through the graph, takes a test's kind from its `calls`
edges (an action, query or fixture makes it an action test, and since
chunk 3.10 so does a `reads` edge, an entity's `get` called straight from
the test), picks them by
file, `<file>:<line>`, `--name`, `--kind` and `--failed`, and runs them
through `Program::call` in a Fisher–Yates order from a SplitMix64 order
seed, so `--seed` reproduces a run. A failed `assert` keeps the value of
each sub-expression its frame worked out (the interpreter records them while
the condition runs, leaving out literals, callees and inline functions),
each anchored at its operator, field name or start, so the report hangs
them under the source power-assert style; `==` on records, lists, sets and
maps adds a diff by field, index or key. A failure or fault that ends a
test is reported with its `.jadpo` site, frames and path. Each outcome has
a stable code (`test.assertion_failed`, `test.unexpected_failure`,
`test.fault`) and a rerun command with the seed; the run is `test.failed`,
and nothing to run is `test.no_tests`. A project keeps its last run's
failures in `build/test/last-run.json` for `--failed`. Since chunk 2.10 the
runner calls the runtime's `Executor` trait (the interpreter now, compiled
code in Phase 8) and gives each test its own environment (`TestEnv`: a
seed from the order seed and the test's key, the test clock, and the
places of the test database and fakes) in a context it makes; tests run on
a thread per core, each with its own view of the one prepared program, and
their results come back in the seed's order. Since chunk 3.1 an action test
in a project with tables runs on `jadpo_test::database::TestHost`, driven by
the native host's runtime on its thread, and gets its own copy of the test
database the first time it uses a store (§2.7). The trace, fakes and clock
control come with their phases. Since chunk 4.1 a test that sends a
`request` through a route is a route test (`--kind route`): the typed
body's `Request` site pauses like a call into the boundary, and the
interpreter builds the request a client would send from the route table
(`jadpo_runtime::http::test_request`), hands it to the runtime's HTTP
component in the test's own context and database, and reads the response
back as a `Response` value (`response_value`); a defect behind a 500 ends
the test with that fault. Since chunk 4.10 all of that is the runtime's
`http::route_test`, which takes the test's `Idempotency-Key` and caller
too, so the interpreter only hands over the site's values. Schedule tests (`--kind
schedule`) are `toolchain.not_built_yet`.

**Differential test.** Every test runs interpreted and compiled, as Gleam
runs one
[language test project](https://raw.githubusercontent.com/gleam-lang/gleam/main/test/language/Makefile)
on every target. Per test it compares outcome, failure code and internal
fields, assertion values, the effect sequence (SQL and row counts, events,
provider calls), path numbers and the final database diff; any difference
fails the gate and prints both sides. From Phase 8, random well-typed
function bodies add fuzzed comparisons, as Cranelift does against its
interpreter.

### 2.6 The runtime

Built so far (chunk 2.1): what pure execution needs from the context,
in `jadpo-runtime` (frames, failures, faults, limits); `jadpo-value`'s
values are `Rc`-shared, with time values held as numbers. Chunk 2.10 adds
the shape Phase 3 plugs into: the context carries its host and its
environment (`Env`: seed and clock); `Host` gives each capability as a
trait, so far a `Database` and its `Transaction`, and the delivery of a
committed action's events, with `MemoryHost`, which holds no store; the
action boundary (§2.5); and `block_on`, which drives one invocation's
non-`Send` future on its own thread until the native host's current-thread
runtime takes over with the store. The components below come with their
areas.

Built in chunk 3.1: a store's transaction runs SQL (`query`, `execute`,
`batch`) with `SqlValue` parameters and rows. `jadpo-drivers` reaches a
server through a `Transport` the host supplies (open a stream to an
`Endpoint`, TCP or a Unix socket; spawn a connection's task on the same
thread), so one driver serves both targets. Its `postgres` feature gives
`Postgres`, a `Database` over tokio-postgres: a few idle connections kept
between transactions, each transaction at repeatable read (since chunk
3.7, at the level it is asked for, set in the transaction's own `BEGIN`
and never for the session), every statement
unnamed (`query_typed`, each parameter's type taken from its value), and
its own binary codecs for the runtime's types. One function classifies
errors by SQLSTATE: unique, foreign-key and exclusion violations are a
conflict with the server's constraint name; serialisation failures and
deadlocks a concurrent change; connection exceptions (class 08), shutdowns
(57P01–57P03), too many connections, a closed connection and any I/O
failure unavailable; since chunk 3.10 a statement the server finds
malformed or out of its rules (class 42 but for a missing privilege, table
or column, class 22, class 0A, a not-null or check violation) a
`Statement` error, which the runtime reports as a defect in Jadpo rather
than asking a person to check the database; anything else a fault with its
SQLSTATE and message.
Connections are unencrypted until TLS is built. `jadpo-host-native` gives
`Runtime` (one current-thread tokio runtime with a `LocalSet`),
`NativeTransport` over tokio's TCP and Unix sockets, and `postgres(url)`.

Built in chunk 3.2: the runtime runs a read plan (one statement,
then one per include, each with the keys of the records it is included in
as one array parameter, so never a statement per record), decodes each
column into a value and each row into a record, and for a page fetches one
more row than its size and writes the next cursor: the query's tag and the
last record's order values, as URL-safe base64, which a query refuses
(422 `invalid_input`) unless it made it. `create` inserts a record with a
new version 7 identity (`Ctx::new_id`) and the clock for generated fields,
and builds the record from the values it sent. Since chunk 3.12 an
identity's random bits come from the host (`Host::random`, the platform's
secure source: the operating system's, or the Web Crypto API on Workers)
when the invocation has no seed, as in a deployable; a test's environment
keeps its seed (`Env::seed`), whose SplitMix generator makes the same
identities on every run but can be run backwards, so it serves tests
only. A host with no randomness asked for an identity without a seed is a
defect at the write. Since chunk 5.5q a test's seed comes from its key
alone, not the run's order seed, so a rerun gives it the same values
whatever order it runs in (area 19, question 19); every other random byte
an invocation is given (`random`'s values, the tokens signing in makes)
comes through `Ctx::random_bytes`: a replayed run's recorded bytes
(`Ctx::replay_random`), else the host's, else the seed's, kept while
recording (`Ctx::record_random`), and none at all, never a weaker source,
when there is neither a generator nor a seed. The native host reads the
operating system's generator through `getrandom`. A store's refusal becomes what the
runtime raises: `unavailable`, a rerun after a concurrent change, and
otherwise the `Store` fault with the driver's reason. Since chunk 5.6c
an aggregate query (area 6, "Counts, sums and groups") is read into a
`QueryModel` whose `grouping` holds its groups and aggregates, and
`jadpo-plan` writes it as one statement (`jadpo_sql::statement::grouped`):
a grouped subquery selecting each entry, then an outer one that orders,
limits and works out what needs several totals (an average from its sum
and count, through `decimal.divide`'s SQL form). Its rows are ordinary
read-plan records, so the runtime reads them as it reads any query's.
Since chunk 6.3b a query's condition on a standard operation of a field
is one model, `jadpo_sql::Condition::Call` (the operation, the field it
is called on, its other arguments and how its result is held, and what
the condition asks of the result), lowered one way for every operation
by `jadpo_sql::condition::lowering`: `Exact`, its SQL form
(`jadpo_sql::form`) compared in the statement, for area 19's exact forms
(area 6, "Operations in conditions") and `ai.distance` as pgvector works
it out (area 21); or `Search`, a search an index serves a margin wider
than the condition, for the `geo.` tests (area 20), whose rows the
runtime holds to the operation. The read plan says what it holds them to
as `jadpo_std::read::Recheck`: each `Check` an operation and its operands
(a column of the row, a value from the read's arguments, a constant), run
by `jadpo_value::run`, the code a function body's call runs, and an order
by an operation's result (`geo.distance`), with the refetch-when-short
loop `jadpo_runtime::read` keeps for any of them. An exact operation's
other arguments are written into its expression, so
`jadpo_sql::condition::index_expression` gives the same text for the
B-tree the checker derives on it (`jadpo_graph::schema::Index::expression`,
`jadpo_sql::table::create_expression_index`), and
`jadpo_sql::form::indexable` says which forms PostgreSQL can index.
`jadpo-plan` decides each statement's parts and where each value comes
from, and writes no SQL text of its own: the vector distances and
`hnsw.` settings, `ST_Distance`, an aggregate's parts
(`jadpo_sql::aggregate`), the identity lookups (`jadpo_sql::identity`)
and the policy joins are `jadpo-sql`'s, which `jadpo-plan`'s `sql_text`
test holds it to. The checker's query reader is split by concern under
`jadpo_check::data::queries`: reading a declaration, conditions, values,
operations, order and pages, includes, `missing:`, vectors, geospatial
conditions and aggregates.

Built in chunk 3.3: the runtime runs a write plan
(`jadpo_std::write`, one per write site, planned in `jadpo-plan`'s
`writes`). A create inserts every column, with a new identity and the
clock in its generated fields; an update sets what `set:` gives and every
`on_change` field, on the loaded record's `id` or the `id` it is given;
both return the row (`RETURNING`), decoded with the read plans' codecs. A
patch writes each field of its input as `CASE WHEN <supplied> THEN <value>
ELSE <column> END`, so its text is fixed and a field left out keeps its
value (a record built without an optional `Input` field remembers it,
`Record::omitted`; since chunk 3.10 `none` given to one whose type isn't
nullable counts as left out, `FieldPlan::none_left_out`, so a patch never
sends `NULL` to a required column). An update by `id` works out a stored field
(`Post.view_count + 1`) in the statement, with `+` and `-` only (`+` on
text is `||`, since chunk 3.10). A delete
removes the row; by `id`, no row is `Missing`, and of a loaded record it is
a fault, since a record changed by another transaction is a concurrent
change and one this transaction can't see was deleted by it, or loaded
outside it. The plan lists the constraints a refusal can name: each
`conflict:` arm's rule (a unique field's or named rule's key, or a
restricting reference's foreign key, reached through every `cascade`), and
each reference the write sets, which raises that entity's not-found failure
unless its value was read from a record loaded earlier, when the record
deleted since is a fault. `Transaction` gained `savepoint`,
`release_savepoint` and `rollback_to_savepoint`; the PostgreSQL driver
forgets a refused statement's error once the transaction returns to its
savepoint, so the transaction can still commit. Every transaction is at
repeatable read (since chunk 3.7, every action that loads a record), so an
update or delete of a row another transaction
changed since the first statement is PostgreSQL's serialisation failure,
classified as a concurrent change: the boundary rolls back and runs the
whole action again. A live test forces one from a second connection and
proves the rerun reads afresh, keeps only the second attempt's rows and
loses no counter increment (`crates/jadpo-cli/tests/writes.rs`).

Built in chunk 3.7: `Database::begin_at(isolation)` begins a transaction
at the action's level (`begin` stays repeatable read, for migrations,
purge and tools), and the PostgreSQL driver sends `BEGIN ISOLATION LEVEL
READ COMMITTED` or `REPEATABLE READ`, so no session state outlives the
transaction (Hyperdrive pools by transaction). At read committed a
counter that is its action's first statement waits for another
transaction's lock on its row, then adds to the committed count, so two
at once both commit at their first attempt; a deadlock is still a
concurrent change. A live test races two requests on one row, each on its
own connection, holding the first's lock until the second is seen waiting
for it: the counter commits twice with one attempt each, and a counter
after a read reruns the second (`crates/jadpo-cli/tests/writes.rs`).
The driver also sends `time` values and one-dimensional arrays.
`Host::database(store)` gives each store by its connection's name, and an
action's transaction is begun in the store its first store operation
names; asking for a second store in one action is a fault, since an action
writes one store (area 5), and since chunk 3.10 the checker refuses an
action that reads a second store (`toolchain.not_built_yet`) until reads
outside the transaction are built. `jadpo_runtime::migrate` and `standing` apply a
store's pending migrations (each `jadpo_std::Migration`, as the checker
reads it from the migration folders) and say where it stands, recording each in the
store's `jadpo_migrations` table under the table's lock (a row lock, never
an advisory one) and refusing a store that applied a migration the project
doesn't have, one that changed since, or one out of order; `jadpo migrate
apply` and the test database both run them. Since chunk 3.10 a snapshot's
column also records what its field allows beyond the column type
(`Column::allows`: an enum's variants, the field's bounds, lengths and
patterns), so allowing less is a migration whose `backfill:` decision
replaces the stored values no longer allowed (`NeededKind::Refit`), and
an index keeps the text columns a query sorts by in code-point order
(`Index::collate`). Since chunk 5.6b a computed backfill (area 5) is
checked as a body of its own (`types::backfill_callables`, the latest
migration's `changes.jadpo` only), written as SQL from its typed body by
`data::backfill`, through `jadpo_sql::form`'s operation, operator and
`case` forms, and decided as `Decided::Computed`: `migration_sql` writes a
`-- backfill <field>: by UPDATE` line, a count and one batch, then the
last step's lock, fill and `SET NOT NULL`. `jadpo_std::Migration::steps`
splits a section there, and `jadpo_runtime::migrate_reporting` runs each
step in its own read-committed transaction under `jadpo_migrations`'
lock, recording how far it got in `jadpo_migration_steps` in the same
transaction, so a stopped run carries on; `Progress` tells `jadpo migrate
apply` each batch. Since chunk 5.6b2 a computed backfill with no SQL form
is decided as `Decided::InJadpo` (`data::backfill::InJadpo`: its body as
a function of the stored fields it reads, `as_function`, and the digest
of the code it runs, `fingerprint`): its marker says `in Jadpo` and is
followed by a count, a locked read and a write, and the last step holds
a `-- fill` read and write (`Step::Mixed`). `Plan::with_backfills`
plans the latest migration's as callables of their own, and `jadpo
migrate apply` hands `jadpo_runtime::migrate_with` a `Backfills` that
runs them in the interpreter, reading and binding through the storage
codec. Since chunk 6.1a that `Backfills` is the runtime's own
(`runner::InJadpo`, built from `Program::backfills`): each batch's rows
run on the runner's per-item mode through the `Executor`, and each value
is kept and checked by its field's validator (`BackfillPlan::validator`)
rather than rounded by the command, so compiled code will run them the
same way. Since chunk 5.9c a JSON column's `Allows::json` records what its
type reads (`jadpo_graph::stored_json::Reads`, built by
`data::columns::json_reads`), and `stored_json::reads_every` decides
whether a change only widens it; one that may not is
`NeededKind::Unreadable`, a decision that may be left out:
`migration_sql` writes a `-- check` step first in the section, or, with
a backfill, a `-- rewrite` step (`jadpo_std::Work`). The plan numbers
each such field's type (`Plan::migration_json`), and `Backfills::unreadable`
reads each stored value with its codec; a refused check is
`MigrateError::Unreadable`. A structured field's backfill is always
`Decided::InJadpo`, its value written through the codec.

Built in chunk 3.4: `jadpo-plan`'s `Planner::access` adds an entity's
`visible:` to every query, `get`, child include, update, delete and
transition, never to the parent an include reads, which stays readable
while hidden (area 6, question 10); `clock.now` in it is the operation's
clock, sent as a parameter. A transition's statement is an `UPDATE` of the
fields its `set:` gives (fixed values and `clock.now` parameters, since
chunk 5.5w3b) and every `on_change` field, with its `when:` in the `WHERE`
when it can refuse a visible record; a create fills each owned field from
`initial:`, or `NULL`. A guarded transition's plan carries a `Guard`: when
the update changes no row, the runtime asks with its `SELECT 1`
whether the record is there and visible, and a row is the refusal its
`conflict:` arm maps (listed among the plan's refusals as `when:
Entity.transition`), otherwise the not-found failure or the fault as for
any write. `jadpo_runtime::purge(host, plan, now, batches)` is purge's
entry point, outside every action: batch after batch, each its own transaction, one
`DELETE ... WHERE "id" IN (SELECT ... LIMIT 500 FOR UPDATE SKIP LOCKED)
RETURNING "id"` of hidden records whose `since:` field is older than `now`
less `after:`, oldest first, leaving out with `NOT EXISTS` any a
`restricted` reference holds, directly or through a cascade, until a batch
comes back short (since chunk 3.10 through every chain of cascades, and,
when one leads back to an entity already on it, such as a folder inside a
folder, through a recursive query that follows the cascades as deep as
the data goes); it returns each batch's count (`Purged`). Since chunk 3.12
a run stops after `batches` batches (`PURGE_BATCHES`, 20, for the
schedule) and says whether it `finished`, so a Workers cron run can't
overrun however much is due, and the next run carries on; the budget is a
batch count, since no host reads a clock yet (chunk 5.2). Area 12's
hourly purge schedule will call it (chunk 6.2); a live test calls it
directly (`crates/jadpo-cli/tests/lifecycle.rs`), stopping one run at its
budget.

Built in chunk 4.1: the HTTP component (`jadpo_runtime::http`). A host
hands it a `Request` (method, path, query string, headers, body, the
client's address) and sends the `Response` it gives. It counts the request
against the default rate limits (`RateLimits`: 120 a minute per address,
and since chunk 4.3 600 a minute per signed-in caller, fixed windows in a bounded amount of memory, the oldest
counters dropped first; since chunk 4.10 the host counts them,
`Host::rate_limit`), routes by the route table (a fixed segment beats
a placeholder; 404 `route_not_found`, 405 `method_not_allowed` with
`Allow`; `HEAD` as `GET` without the body, `OPTIONS` with `Allow`),
decodes each placeholder and the input with the route's codecs
(`jadpo_value::json`: each problem as `{ field, rule }` by its path in the
input, 422 `invalid_input`; a body that isn't JSON, 400
`invalid_request`), runs the action through the executor, inside the
runtime's action boundary, or the query through `store`, and writes the
output, or the failure envelope with the client fields' codecs, the status
the failure's kind sets and `request_id`, which `x-request-id` repeats; a
fault is 500 `internal_fault` and travels on the response for the host's
log. The component is the same for both executors and every host; route
tests (§2.5) drive it with a request built from typed values.
`jadpo-host-native` serves it (`http::serve`): one listener whose thread
hands each connection to a worker chosen by the client's address, and one
current-thread tokio runtime per worker, each with its own handler, stores
and rate-limit counters, so an address is always counted on one thread;
hyper reads and writes HTTP/1.1. `NativeHost` gives each store by name,
the operating system's randomness and tokio's timer. `jadpo dev` runs the
interpreter on it, swapping in each save that checks cleanly.

Built in chunk 4.2: the request guarantees, in the same component. A body
over `BODY_LIMIT` (1 MB) is 413 before routing; the native host stops
holding a body there and marks the request `too_large`. Health
(`/health/live`, `/health/ready`) is answered before the rate limits, the
readiness check racing `SELECT 1` on each store against `Host::sleep`.
Each route races what it runs against `Host::sleep(deadline)`: when the
timer wins, the work is dropped with its open transaction (`Ctx::abandon`;
a dropped transaction closes its connection, which aborts it) and the
answer is 504. So that a request that committed is never answered 504, the
route asks the boundary to hold an action's commit (`Ctx::hold`): the
boundary leaves a succeeding body's transaction and outbox open, and the
component commits them (`commit_held`) after the race, first writing the
request's idempotency key into the same transaction. Keys
(`http::idempotency`) are claimed before the action runs (in a short
transaction of their own until chunk 4.10, one statement since) and kept
in the `jadpo_idempotency_keys` table of
the store the action writes, as the plan's `RoutePlan::keys` names it; the
SQL for that table is the runtime's own, like `jadpo_migrations`'.

Built in chunk 4.3: sign-in (area 8). `jadpo-check`'s `identity` module
reads `application { }`, `principal { }` and `authentication { }` once per
project into an `Identity` (each variant's entity, subject and `active:`
condition, each declared method), read lazily by the route groups and the
bodies that name a caller, so an edit to it checks again only those. The
route model says whether a route needs a caller (`Authentication`) and
which kind it selects; `Principal`, `Principal.user` and
`Principal.service` are types whose fields (`id`, `subject`, `strength`)
the identity gives; `Principal.user(record)` and `sessions.start` and
`.end` are callees of their own (`Callee::Caller`, `Callee::Session`) and
a request's `as:` is the last value of `Expr::Request`. `jadpo-plan`
writes the one statement each kind of caller is looked up with: the
session by its token's verifier, joined laterally to the user's record
with its `active:` and lifecycle's `visible:`, into the route table's
`IdentityPlan`. The HTTP component (`http::identity`) authenticates a
route's request after routing and before its input, inside the deadline:
one credential, the session's lookup, the CSRF check on a cookie write,
the kind the route selects; the caller rides on `Ctx` for what the route
runs and for `sessions.end`. `sessions.start` writes `jadpo_sessions` in
the action's transaction and records a `SessionEffect` on the boundary,
which a held commit hands back so the response sets or clears the cookies
once the action commits. A signed-in caller counts against a caller's
limit (`RateLimits::allow_caller`; `Host::rate_limit` with `CALLER_LIMIT`
since chunk 4.10), and keys its idempotency as `user <id>`.

Built in chunk 4.3b: API tokens and service keys (area 8). The identity
reads `api_token:` and `service_key:`, and `jadpo_std::http::SessionOp`
names every generated credential operation: `sessions.start`, `.end`,
`.issue_token`, `.tokens` and `.revoke_token`, and the service entity's
`issue_key`, `keys` and `revoke_key`, each a `Callee::Session` that only
an action calls and that reads or writes the store its table lives in
(`jadpo_sessions` beside the users, `jadpo_service_keys` beside the
services, each a system table in that store's schema). `ApiToken` and
`ServiceKey` are built-in text types no entity may store; `IssuedToken`
and `IssuedKey` are built-in values whose fields (`jadpo_std::http::
ISSUED_FIELDS`) code reads. `jadpo-plan` writes each kind of caller's
lookup (`PrincipalPlan::lookup`: a session or token with its user, a key
with its service), and the HTTP component reads a bearer credential by
its prefix (`jadpo_token_`, `jadpo_key_`), where the route's API accepts
its method, refusing a disabled service with `service_disabled` and the
wrong kind for the route with `not_permitted`; a service caller is keyed
`service <id>`. An action that issues a token or key marks its boundary
(`Ctx::issued`), and its response is never stored for an idempotent
repeat. A route test's `as:` keeps a real credential of the kind the
route's API accepts (`http::identity::test_credentials`).

Built in chunk 5.1b: signed sessions (area 8). The identity reads
`revocation_delay:`, each method's `signing_key:` (a `SigningKey`
setting) and a browser session's `origin:` (a `Url` setting) into the
`IdentityPlan` by setting name, and each lookup returns when its session
or key ends. `http::signed` is the runtime's host-agnostic signing:
`Settings::read` derives each method's key (BLAKE3 `derive_key`) from the
values the application started with, current and `_PREVIOUS`, held by the
program and handed to the HTTP component in `App::auth`, never in a value
code reads; a signed part is base64url JSON `Claims` and a keyed BLAKE3
tag, compared in constant time, the same natively and on wasm32. With a
delay, `http::identity::authenticate` signs a request in from a good
signed part without a statement, unless the route is `fresh`; otherwise it
runs the lookup and returns a `Renewed` credential, which the response
carries in `Set-Cookie` or `X-Jadpo-Token` unless the action started or
ended the session. The plan appends a `RoutePlan` with `Run::Exchange` for
`POST /auth/exchange` in the first API that accepts service keys, whose
sign-in signs the token the response gives; the API record, OpenAPI and
`jadpo routes` list it from `Identity::exchanges`.

Built in chunk 5.5z: passkeys (area 8, "Passkeys"). The identity reads
`sign_in: { passkey: }` into `Identity::passkey`, kept only with a browser
session and its `origin:`; the schema adds `jadpo_passkeys` and
`jadpo_passkey_challenges` beside `jadpo_sessions`. The plan appends one
`RoutePlan` per `jadpo_std::http::PasskeyStep` under `path:`, with
`Run::Passkey`, in the first API that accepts browser sessions, and writes
`PasskeyPlan::lookup`, a passkey by its credential ID joined laterally to
its user's record and `active:`. The HTTP component answers them in
`http::passkeys`, after signing in and with no idempotency key: it keeps
each challenge server-side by its BLAKE3 verifier, bound to a
`<cookie>_passkey` cookie's verifier or the session, takes it once with
`DELETE … RETURNING` before the answer is checked, and starts the session
in the transaction that writes the passkey's new counter, only if the
counter is still the one read. The ceremonies are a host capability,
`Host::passkeys` (`jadpo_runtime::Passkeys`), with the configured origin
and plain data across it, so the runtime crate stays free of them on
wasm32: the native host and a test's host carry `webauthn-rs` 0.5.5,
whose OpenSSL doesn't build for wasm32, so a Workers host waits for its
pure-Rust release; `jadpo_host_native::test_authenticator` (feature
`test-authenticator`, tests only) is a software device the live tests
sign in with.

Built in chunk 4.4: policy (area 9). `jadpo_check::policy` reads each
entity's `policy:` and `membership:` by name (`policy_read`,
`membership_read`) and root `policy { }` once (`root_read`), resolving
every entry of a grant list into a `Grant` (a field pointing at the
caller, a membership role through the reference it applies through, or
`Access`), each entity's tenancy and the fields only a listed action
changes; `access` gathers them for the plan and the report, and
`policy::reach` follows what each action and query reaches (`acts_of`,
by name from `body_facts`, like the store rules) to check each route
group's file (`reach_problems`). `jadpo-plan`'s `policy`
module compiles the grants into every statement at the one place
`Planner::access` already joined the lifecycle: a read's and an
update's or delete's `WHERE`, and a create's `INSERT ... SELECT ...
WHERE` over the new record's values (`jadpo_sql::policy`), with the
caller as parameters (`jadpo_std::policy::CallerPart`, `Source::Caller`,
`WriteSource::Caller`); each statement policy limits also keeps its text
without policy (`unpoliced`), which a test's call without a caller runs
(`Ctx::set_unpoliced`) and which tells a hidden record from a missing
one (since chunk 4.10 a read of one record selects its grant instead). A
write's `PolicyRefusal` says what writing no row means, a listed
action's record-free line becomes a `Gate` checked as it starts
(`jadpo_runtime::admit`, by the action boundary since chunk 4.10), and the identity plan's `GatedStatements` limit
listing, issuing and revoking credentials by the holder's grants. The
runtime raises `not_permitted` (`jadpo_runtime::not_permitted`) and marks
a concealed not-found failure `concealed_by: policy`; the interpreter sets
the caller of each call a test, fixture or seed makes from the
`Principal` it passes (since chunk 4.10 the runtime's `call_as`, with the
parameter the plan names). `jadpo check` writes `build/report/permissions.md`
and `.json` (`policy::report`).

Built in chunk 4.4b: field restrictions (area 9). `policy_read` also reads
each field's own `policy:` into `EntityPolicy::fields` (`FieldPolicy`:
`FieldAccess.server_only`, or the lists it narrows), refusing one that
widens its record (`policy.field_widens_entity`). `policy::proof` decides
who may read what from the grants alone: what is known of a caller on a
record (`Fact`: a grant they pass, a membership held in the record, the
tenant), moved along a reference (`Prover::along`), for each check they
passed to reach it (`Reached`). `policy::outputs` proves each query's
output fields and the parent every include reads; `policy::flow` walks
every action's typed body for where each value came from (`Prov`, flow-
insensitive per local, with a summary per action of what its result carries
and which parameters reach an output it builds) and checks every `Output`
and failure `client:` field it builds and every create's user fields
(`policy.role_field_create_forbidden`); `policy::reach` refuses a route
whose action always sets a field nobody may change. The plan adds each
restricted field's grants to the statement that changes it
(`Planner::field_policy`, beside `granted`, whose tenancy is now
`in_tenant`), and an update's `PolicyRefusal::Probe` asks again under the
record's grants (`FieldProbe`) to tell a field's refusal, 403
`not_permitted`, from a hidden record. The report gives each restricted
field a row and `fields` in its JSON, and `graph::policy_nodes` adds
`grant`, `field_restriction`, `tenancy` and `action_exception` nodes with
`grants` and `scopes` edges.

Built in chunk 4.4c: listing tokens and keys from a `GET` (area 8). A
route's `Run` has a `generated` field: `sessions.tokens` or the service
entity's `keys`, which no declaration holds, so it has no `Runs` edge. The
checker reads the call in `routes::Reader::generated_run` (one argument,
the holder's `id`; any other method is `route.run_kind`, a write is
`route.run_invalid`), types it `List<IssuedToken>` or `List<IssuedKey>`,
and `policy::reach` gives the route the holder's `read`
(`session_act`). `IssuedToken` and `IssuedKey` are `routes::wire::Shape::
Issued`: an object of `ISSUED_FIELDS` with no node in the graph, which the
record and OpenAPI write as a section and a schema of their own and
`jadpo-plan` gives a shape (`Types::issued_shape`) and an object codec.
The table's `Run::Listing { keys }` is run by `http::identity::list`, a
one-statement read in a boundary of its own at repeatable read that calls
the same `issued` an action does, so the plan's policy-limited statement
(`GatedStatements`) decides the list.

Built in chunk 4.7: three Phase 4 decisions (areas 9 and 13).
`jadpo_check::policy::Access::kept_in_tenant` says when a reference stays
inside the tenant; `Prover::along` moves the tenant, and a role held
through the record's tenancy, along it, so an include of a tenant's parent
proves. `Planner::kept_in_tenant` compiles each such reference a write sets
into its own statement as `jadpo_sql::policy::outside_tenant` (`NOT
EXISTS` a record with that `id` in another tenant), in the policed and the
unpoliced text alike, and `WritePlan::kept_in_tenant` tells the runtime
that writing no row without policy is `not_permitted` too, a create's or a
found record's. `policy::outputs` proves each field a query's or an
include's `where:`, `where_present:` and `order:` tests, and
`policy::flow` a `get`'s `id` and a transition's `when:`, like an output.
A route test's `request { }` takes `idempotency_key:` (graph
`Expr::Request::idempotency_key`), and the interpreter's `request` sends it,
or a fresh one from the test's seed, with every request but a `GET` (the
runtime's `http::test_request` since chunk 4.10).

Built in chunk 4.8: the Phase 4 review's policy findings (areas 8 and 9).
`ActionGrants::gate` decides who a listed line lets call its action as it
starts (`LineGate`), from the entities its own body writes
(`policy::reach::own_writes`, `line_gate`): the checker refuses a route
whose line lets nobody in, `policy::flow` counts on it, and the plan's
`gates` compiles it, refusing every caller when it can't build one, as
`http::identity::policed` refuses a credential statement with no policed
twin. `policy::flow` walks functions as well as actions; `Src::Decided`
carries what an `if`, a `match`, a loop or a filter tested into every value
made under it (`Walk::pc`), `Src::Record::given` keeps what a write set
in the record it gives back, `Src::Param` names the part of a parameter a
summary sends, and each value that finds a record is checked like an
output (`Walk::lookup`). `Prover::field_readable` asks the record's `read:`
too, except of a created record (`Fact::Fresh`) or its `id`; an update's
record is proved once for each record its target may be (`targets`);
`Fact::MaybeHidden` keeps a membership read as a parent from proving a
role, `Prover::proves_on` proves a list naming every role held in the
tenant from the tenant alone, and an optional `role:` proves none.
`policy::outputs` checks each include's reference (`Unproved::Via`). The
plan founds a tenant only while `jadpo_sql::policy::unheld` holds for every
membership held in it (`Planner::nobody_in`), adds the caller's membership
in the new tenant to a move (`Planner::moved_to`), and asks a guarded
transition's probe with its fields' grants, so a field's refusal is 403
before the condition's 409. `types::decl::holds_principal` follows named
types.

Built in chunk 4.9: the Phase 4 review's request findings (areas 6, 7, 8
and 13), each in the runtime or the native host. A route sets the
request's deadline on `Ctx` (`set_deadline`, read from
`Host::monotonic_ms`, which a test's host leaves `None`); the
interpreter's machine checks `Ctx::out_of_time` once every 4,096 tasks and
stops with `FaultKind::DeadlineExceeded`, and the component answers any
body that finished past the deadline as one the timer stopped: abandoned,
its claim given up, 504. A held commit that finds a concurrent change runs
the action again within `RERUN_ATTEMPTS` and the same deadline; a claim
that lapsed before the commit is `unavailable`. A route anyone may call
counts a request with a credential against its address before it runs;
`boundary::caught` restores the session effect and `issued` with the
outbox when a handled action fails. Before routing, a header value that
isn't printable ASCII is 400 (the native host passes every value, as
`from_utf8_lossy` reads it); a route refuses a query string or a body it
doesn't read as `unknown_field`, and reads a body only with a JSON
`Content-Type`. `jadpo_value::json` leaves an omitted `Input` field out
when it writes one, so a route test's body is the one a client sends. A
page's bad cursor answers `invalid_input` naming its field
(`http::cursor_field`). Natively, hyper has tokio's timer and a 10-second
header timeout, a body has 30 seconds, the listener holds at most 1,024
connections (`http::MAX_CONNECTIONS`), and each worker's `Postgres` opens
at most 10 connections (`MAX_OPEN`), a transaction past that waiting for
one to go idle or close. `jadpo-check` derives each route's OpenAPI
responses from what it runs (`openapi::facts`: policy reach through calls,
`types::used_stores` for 503, `types::written_stores` and
`types::unkept_responses` for what an `Idempotency-Key` keeps), writes
`Time` and a mixed enum's plain variants as they are sent, and puts each
API's sign-in methods (`apis::accepted_methods`) and each route's caller
kind in the record.

Built in chunk 4.10: the Phase 4 review's runtime findings. What compiled
code must share is the runtime's or the plan's, as chunk 3.12 made reads
and writes. The action boundary checks an action's gate before its body
runs, whichever executor supplies the body (`ActionBody::gate` and
`site`, from `CallablePlan::gate`); a test's, fixture's or seed's call
runs as the argument the callee's plan names as its caller, the parameter
typed `Principal`, `Principal.user` or `Principal.service`
(`CallablePlan::caller`, `QueryPlan::caller`, run by `jadpo_runtime::
call_as`), never a value's type name; a route test's request, its
`Idempotency-Key` and its credentials are built by `http::test_request`
and sent by `http::route_test`; and a deployable hands each request to
`http::serve_request(app, executor, host, worker, request)`, which makes
the context under the host's wall clock (`Host::now_ms`), the request id
and the answer, as `jadpo dev` now does (`http::Worker` is what the
component keeps between one worker's requests). Rate limits are a host
capability (`Host::rate_limit(key, limit, now)`, a future, as Redis or a
Workers binding needs): natively `NativeHost` counts in its worker's
bounded memory, `RateLimits`, and a test's host counts nothing. On Workers
there is no native default: area 16 keeps counts in memory only where
memory lasts, and its question 6 requires a Redis connection or
Cloudflare's rate-limiting binding there, with the isolate's memory
counting only while that can't be reached, which is the one reading both
give. Round trips: the sign-in lookup, an idempotency key's claim (one
`INSERT ... ON CONFLICT` that also gives back what the key is kept with)
and its release, the response a failed action keeps, and `SELECT 1` each
run as one statement on their own (`Database::query_once`, autocommit),
and the PostgreSQL driver sends a transaction's `BEGIN` with its first
statement, both written before either answer is read; an idle connection
found closed at that first round trip ran nothing, so it is sent again on
another, as `begin` did. A `get` or a query for one record that policy
limits selects its grant as a column (`Select::permitted`) and filters
only on what it asks and the record's visibility, so one statement tells
a hidden record (`concealed_by: policy`) from one that isn't there; rows
the caller may not read are dropped before its includes run. Counted at
the transport against live PostgreSQL once connections are open
(`crates/jadpo-cli/tests/round_trips.rs`): a signed-in write with an
`Idempotency-Key` took 10 round trips and takes 5 (the lookup, the claim,
`BEGIN` with the insert, the response kept, `COMMIT`), and a signed-in
`GET` policy hides took 7 and takes 3 (the lookup, `BEGIN` with the read,
`ROLLBACK`); the write's repeat takes 2.
`/health/ready` is answered from the worker's last check for a second
(`Host::monotonic_ms`), so probes cost each store at most one statement a
second per worker. The interpreter's machine gives the thread back each
time it checks a request's deadline (`Step::Yield`,
`jadpo_runtime::yield_now`), so a route that computes without waiting no
longer holds the requests its worker serves until its deadline; an inline
function inside a collection operation still only stops there.

**The client's address** (chunk 5.1c). The address a request counts as,
for the default rate limits, `jadpo dev`'s request lines and the request
timeline, is chosen in the runtime from plan data and not by a host:
`application { client_address: }` is `RouteTable::client_address`, read
with the settings the app started with into `http::address::ClientAddress`
(`ClientAddress::read(plan, values, Target)`), which `App::client` carries.
`serve_request` sets the chosen address on the context (`Ctx::client`) and on
the response (`Response::client`) before anything else runs, and
`by_address` counts that. A host only gives the connection's address
(`Request::address`; natively the peer's), so the Workers host uses the same
code with `Target::Workers`, where the default is `cloudflare`
(`CF-Connecting-IP`) and `read` refuses every other source (`NotAccepted`)
for the deployable to report as a start-up problem (chunk 8.2). With
`hops: N` the client is the N-th address from the right of `X-Forwarded-For`
(every line joined) or of `Forwarded`'s `for` parameters; a header that is
short, invalid or missing gives the connection's address, never a guess, and
addresses are kept in one spelling. Natively the listener still sends a
connection to the worker its peer address hashes to, so a client behind
one proxy is counted on one thread; clients behind several proxies can be
counted on several, as clients on several addresses always were.

**Configuration** (chunk 5.1). Starting is one function, `jadpo_runtime::
config::load`, given the plan's settings, their compiled validators and a
source of variables (`Variables`, a host capability: `Host::variable` is the
process's environment natively and would be the Worker's bindings on
Workers). It reads only the variables the settings name, decodes each with
the setting's codec, and gives the checked values or every setting that
stops the start, as the setting, its variable and the rule it breaks, never
the value; `check` gives the same readings as `set`, `default`, `missing`
or `invalid` for `jadpo config check`. A test run reads `test:` values,
defaults and secrets it makes at random (`Mode::Test`). The checked values
are text, `Send`, so every worker thread decodes them into its own values
(`Program::with_config`), and a secret's text is held there and nowhere
else (its `Debug` leaves it out); the interpreter reads a setting like a
constant. Nothing here is interpreter-only, so the compiled deployables
(Phase 8) call the same function with the Worker's bindings.

**The system caller and the runner** (chunk 6.1a). Background work,
area 12's schedules and subscribers, acts as the system (area 9,
`Access.system`): `Ctx::system` holds it, with the tenant a subscriber's
event came from, and only the runner sets it (`Ctx::become_system` is
the crate's own); `set_caller`, `set_unpoliced` and every request the
HTTP component answers clear it. Policy binds it as two caller parts,
`CallerPart::System` (whether the system runs the statement) and
`CallerPart::SystemTenant` (its tenant or `NULL`): the plan compiles
`Access.system` to the first, and on a tenant's records adds
`jadpo_sql::policy::system_in_tenant` beside the memberships, so a
request, for which they are `false` and `NULL`, is never let in by
them. A listed action's `Gate` says whether its line names the system
(`Gate::system`) beside the callers it lets in (`Callers`).
`jadpo_runtime::runner` drives an `Executor` for work no request waits
on, each `Job` (a callable, its arguments and the system it runs as) in
a boundary of its own:

- **deferred**: a committed action's outbox is left on the context as
  `Deferred::Events` (`Ctx::defer`), never delivered in the request
  (`Host::deliver` is gone); `serve_request` moves what a request left
  to `Response::deferred`, and the host hands it to `runner::after_response`,
  which gives `Host::defer` a future running `run_deferred` once the
  response has been handed back: natively the HTTP worker takes it
  (`Handler::take_deferred`, `NativeHost::take_deferred`) and spawns it
  on its own thread's `LocalSet`, where it shares the worker's program,
  so no background thread needs a program of its own; on Workers,
  `waitUntil` (chunk 8.2). Each run gets its own context under the
  host's clock and no seed, as the system, raced against
  `RUN_LIMIT_MS` (15 minutes, area 12) with a deadline the interpreter
  yields at; what it sets off runs after it;
- **inline**: `runner::settle` runs work in the invocation that left it,
  then gives the context back to its caller (6.3's `settle:`, a test's
  delivery after each commit);
- **per item**: `runner::items` runs a callable once per item, and
  inside a `runner::Batch` (a transaction of its own, begun as it opens)
  each item's action joins it in a savepoint and is undone alone when it
  fails, as `boundary::Undo` undoes a caught call; the run writes its
  position in `Batch::transaction` before `Batch::commit`, so a stopped
  run resumes after the last committed batch (6.4).

A host with nothing to run deferred work on (a test's, a tool's) drops
it, and the outbox keeps what it held. Schedules (6.2) hand their runs
to the same runner.

**Events** (chunk 6.1b). The checker reads each subscriber
(`jadpo_check::events`: `subscribers_in` per file, `subscribers` for
the plan; `on:` an event type, `run:` an action of its own domain with
`event` paths as its arguments, `retry:`, and the event's field holding
the tenant's id), which actions emit through what they call
(`emitting`, from a `Fact::Emits` in each body's facts) and where each
keeps its outbox (`outbox_store`: the store it writes, else
`events: { store: }`, else `event.store_required`); a subscriber whose
action reaches a grant naming `Access.public` but not `Access.system`
is `policy.system_grant_missing` (`policy::reach::public_only`). The
plan holds the subscriber table (`jadpo_std::subscriber::EventsPlan`,
`Plan::events`) and each emitting action's `CallablePlan::outbox`. The
interpreter's `emit` writes the event's payload through the JSON table
and hands the runtime an `Event` with its subscribers' keys
(`Ctx::emit`, refusing past 1,000 events or 64 KB). The boundary seals
a succeeding body (`boundary::seal`): in its transaction, beginning one
in `ActionBody::outbox` when it has none, it writes one `jadpo_outbox`
row per event and one `jadpo_deliveries` row per subscriber (two
statements, `unnest`ed arrays), and for a subscriber's delivery its
handled record (`jadpo_handled_deliveries`); once it commits, the
deliveries are `Deferred::Deliveries`. `runner::deliver` claims a
delivery's record in one statement, finds its handled record or runs
the subscriber's action as the system limited to the event's tenant,
in a new span of the event's trace (`outbox::Trace`, `Ctx::trace`,
which the HTTP component starts or continues from `traceparent`), and
records `succeeded`, `pending` with a backoff (`outbox::backoff`, full
jitter) or `failed`; `runner::sweep` delivers what is due in a store,
for 6.2's minute tick. The statements are `jadpo_runtime::outbox`'s,
on its own system tables, as the idempotency keys' are.

**Schedules and the tick** (chunk 6.2). The checker reads each schedule
(`jadpo_check::schedules`: `every:` and `run:`, an action of its own
domain taking nothing) and where their state is kept
(`schedules_settings`: `schedules: { store: }` or the only database, else
`schedule.store_required`); the plan holds the schedule table
(`jadpo_std::schedule::SchedulesPlan`, `Plan::schedules`, which
`Executor::schedules` gives the runtime) with the stores holding the
runtime's own tables. `jadpo_runtime::schedules::tick(executor, host,
now)` is the one entry point every host calls: it makes and touches each
schedule's state row (`jadpo_schedules`), takes the lease of each one due
in one conditional statement, runs them side by side as the system with
no tenant under the tick's clock and a new trace, records each run and
skipped tick (`jadpo_schedule_runs`) and delivers what they emitted; then
in each store it calls `runner::sweep` and deletes expired idempotency
keys and events every subscriber has had, and runs `schedules::HOOKS`,
which picks up batch runs whose lease lapsed (`batches::resume`). Natively the HTTP server's first
worker runs a timer that starts the tick at each whole minute of UTC, or
each shortest `every:` (`Handler::tick`, `schedules::tick_every_ms`);
`jadpo dev` ticks. On Workers the cron trigger calls the same function
(chunk 8.2).

**Batch runs** (chunk 6.4, area 12's "Chosen in chunk 6.4"). The
checker reads `batches.each(from:, each:, finished:, key:)`
(`types::body::batches`) into a `Callee::Batch` call whose `from:` is
its query's call and whose `each:` and `finished:` are inline functions
calling an action; the facts give the action a `Fact::Batch` in place of
those calls, which are the run's, so the one-store rule counts the
query's store and the actions' writes and policy reaches nothing for the
caller. `jadpo-plan` decodes each such site into
`jadpo_std::batch::BatchPlan` (`Plan::batches`): the query's read plan,
the store, the batch's isolation, the JSON-table number of each value
the run saves and where each argument of `each:`'s and `finished:`'s
action comes from (`BatchArg::Item(path)` or `Captured`), and the sites
the executor works out to start it (`Decoded::Batch`). The runtime's
`batches` module keeps runs in the query's store (`jadpo_batch_runs`,
`_problems`, `_cancels`, placed by `data::schema` beside a store some
batch walks): `start` writes the run's row in the action's transaction
and leaves `Deferred::Batch` for after the response; `run` claims the
run with a lease and walks it a page at a time on `runner::Batch`, each
page's items through `runner::items` (an item undone alone on a failure
or a fault, `boundary::Undo`), then `finished:`; `resume` picks up runs
whose lease lapsed, for 6.2's minute tick; `status` and `cancel` read the
row and keep a cancel. An executor gives the runner its batches through
`Executor::batches`, so compiled code runs the same runner.

**Single-threaded by design.** Runtime futures need not be `Send`. A
Workers isolate is single-threaded anyway; natively, `jadpo-host-native`
runs one current-thread tokio runtime per core, each with its own
connection pools and rate-limit counters, behind one listener that hands
each connection to the worker its client's address hashes to, so an
address is counted on one thread. Work that never waits gives its
thread back as it checks the deadline, so one request can't hold the
others a worker serves. One code path serves both targets,
with `Rc` instead of `Arc` and no locks on the request path. *Rejected:*
multi-threaded tokio with `Send` bounds switched off on wasm32, which
doubles the async signatures and lets bugs hide on one target.

| Component | Does | On Workers |
|---|---|---|
| HTTP | Router from the route table, input decoding, body limit, deadline, health, idempotency keys, failure envelope, `request_id` | fetch event |
| Identity | Sessions, CSRF, API tokens, service keys, OIDC sign-in, revocation delay | same |
| Policy | Principal, tenancy and memberships; predicates arrive compiled in the plan's SQL and the runtime binds the caller | same |
| Transactions | Read committed or repeatable read by what the action does, revision checks, rerun within the budget | same |
| Idempotency | A key claimed in one statement before the action; its response stored in the action's transaction, or in one statement for a failure | same |
| Outbox | Delivery, retries, circuit breaker, dead letters, run by the runner after the response (chunk 6.1a), natively as a task on the worker's thread | `waitUntil` after the response, swept by the minute tick |
| Scheduler | Due times in the schedule's state row from its first tick, leases, run records, tick adapters; native timer | Cloudflare cron |
| Rate limits, caches | Rate limits through `Host::rate_limit`: Redis when one is named, otherwise each worker's bounded memory; cache reads with timeout, invalidation after commit | Redis or Cloudflare's rate-limiting binding required (area 16, question 6); the isolate's memory only while it can't be reached |
| Telemetry | Spans per declaration, metrics, path numbers, threshold sampling, OTLP | export in `waitUntil` |
| Email | Templates, translations, the `email` capability | SMTP on 587 or a package |
| Flags, privacy | Evaluation; export and erasure flows | same |
| Development | Request timeline, local inbox and disk, published-event log | none |

**Capability drivers.** One trait per capability: `Database` (begin,
query, execute, commit, rollback, and one statement on its own),
`Cache`, `Email`, `Sms`, `Files`, `Publishing` and `ScheduleTick` (ticks
and an optional lease); rate limits and the wall clock are `Host`
methods (chunk 4.10). Drivers
classify their own errors into the runtime's fixed set (conflict with the
constraint name, serialisation failure, unavailable, fault), so executors
and targets agree; the experiments found exactly this classification
drifting between adapters
([shared conformance](experiments/shared-rust-conformance-results.md)).
Package-backed capabilities use one generic driver that runs the package's
declared protocol, credentials, idempotency header, retries and error
mapping through `Host::fetch`; packages never generate driver code. Drivers
list their targets for `jadpo check` (area 11), and test fakes implement
the same traits.

**What differs on Workers.**

- No threads, no timers between requests, no disk: delivery runs in
  `waitUntil`, the minute cron sweeps the outbox and ticks every schedule.
- PostgreSQL goes through Hyperdrive over `connect()` sockets with
  tokio-postgres, as in workers-rs's
  [example](https://raw.githubusercontent.com/cloudflare/workers-rs/main/examples/tokio-postgres/src/lib.rs),
  one connection per request. Hyperdrive
  [pools by transaction and caches reads](https://developers.cloudflare.com/hyperdrive/concepts/how-hyperdrive-works/)
  without invalidating them on writes, so Jadpo requires its caching off
  (reads are current, area 6), uses unnamed statements (`query_typed`),
  keeps no session state outside a transaction and takes leases with rows,
  never advisory locks. Rows reach the module as wire bytes rather than
  JavaScript values, which should remove the conversion that held the
  experiments to about 79% of Bun
  ([optimisation results](experiments/wasm-optimization-results.md)): an
  inference until the performance spike (chunk 8.3) measures it.
- Six connections at once per request
  ([limits](https://developers.cloudflare.com/workers/platform/limits/)),
  counted by the runtime across database, providers and SMTP.
- A panic aborts the instance, so runtime crates deny `unwrap`, `expect`
  and `panic!` by lint, and faults are values; the crate-graph test holds
  every shared and runtime crate to the lint (chunk 1.9), and since chunk
  3.12 to clippy's `indexing_slicing`, `unreachable` and `unimplemented`
  too.
- `wasm32-unknown-unknown` with wasm-bindgen; Cloudflare documents no
  component-model support and only experimental WASI. Each deployable links only the drivers it reaches, which keeps
  start-up inside the one-second limit.
- Application and runtime are one trusted module, the "trusted
  first-party" mode of the
  [host trust decision](experiments/wasm-host-trust-decision.md); the
  guest-and-monitor designs are not carried forward.

### 2.7 Tests and the gate

**Specification examples become tests.** A chunk turns its areas'
`fragment` blocks into `check` blocks as it builds them; `check` blocks are
compiled by the new `jadpo`, while unmarked blocks stay frozen v0 results
until the chunk removes them. An `expect=rule.id` block must give exactly
one diagnostic, with that rule, as `jadpo check --json` reports it, and exit
1 (0 for a warning); it is followed by a ` ```text output ` block holding the
exact plain output, compared byte for byte and rewritten by
`language_docs.py --bless`, so each `## Errors` entry shows the new
compiler's real output (Phase 1's "done when"). A page of a built area owns
exactly the rules whose catalogue entry names its area, and each needs a
`check` block; a frozen v0 result never counts for it (chunk 1.9). Projects with
`test` declarations also run under `jadpo test --json` and are exported to
`build/spec-corpus/` for the differential run (since chunk 2.2: a `check`
project with tests must pass them, one marked `expect=test.<code>` must
fail exactly one test with that code and show `jadpo test`'s plain output,
both with `--seed 1`; since chunk 3.11 each project runs `jadpo test
--json` once, and only one that shows a failure also runs the plain
command for its output block; `language_docs.py --export-corpus` writes each
project and its `test.json`, and `build/` is not committed).

**Teaching examples** (chunk 2.4; decision "Examples teach every feature",
2026-10-14). Each project in `examples/` (a folder holding `app.jadpo`;
`golden-todo/` holds only the golden contract) is copied to a temporary
folder by `tools/examples.py`, which fails unless `jadpo check --json`
reports no diagnostics at all, `jadpo fmt --check` passes, and `jadpo test
--json --seed 1` runs at least one test with every test passing; each
project also needs its own `README.md` and a line in `examples/README.md`.
`--export-corpus` writes each project and its `test.json` to
`build/spec-corpus/examples/<project>/`, beside the pages' projects, and
the determinism test and format idempotence cover the examples too. The
coverage test (`crates/jadpo-check/tests/examples.rs`) loads each project,
reads its parse trees, graph and typed bodies, and names any built
feature no example uses. The features come from the compiler's own tables
(the grammar's node kinds, graph node and edge kinds by status and area,
type bases, constraints, input field settings, failure members and
declarable kinds, the copying lines, the operation table and the enums its
operations take, the generated namespaces' built members, the layout's
root files and folders) and from exhaustive matches over the typed body;
an exemption names a feature that can't appear in runnable code yet, with
its reason, and fails once an example uses that feature. The examples are
brought up to date once per phase, by an examples chunk at its end
(Richard, 2026-10-15), so in an ordinary chunk a built feature no example
uses is printed as a `note:` under the step and the step passes;
`python3 tools/verify.py --require-examples` (`JADPO_REQUIRE_EXAMPLES=1`
for the test alone) fails on it, as the examples chunk runs it, and so does
a CI run started by hand with `require-examples` ticked. Everything else
about the examples blocks every chunk: each still checks, formats, passes
its tests and its README's Try-next edits, and the coverage test's own
lists (exemptions, built features) must stay in step with the compiler.

**Proof holes** (chunk 2.7). Compiled code never checks a construction
the checker proved follows its type's rules, so a hole in a proof would
pass an invalid value silently. `Program::with_proof_checks` checks every
proven construction again as it runs (`Type(value)`, an object literal's
fields, a list written into a type with lengths) and turns a broken rule
into a defect; `crates/jadpo-interp/tests/proofs.rs` runs every project of
the specification corpus that checks clean that way, its tests as written
and every callable whose parameters are numbers, text, UUIDs or flags, or
lists and sets of them, with values its types allow at and near the edges
of their rules
(`Program::probe_arguments`), and fails on any defect. The gate runs it as
its own step.

**PostgreSQL harness.** `JADPO_TEST_DATABASE_URL`, or a disposable server
from the local tools (`initdb`, `pg_ctl`) on a Unix socket with `fsync`
off. Since chunk 3.1, `jadpo_test::postgres::Server` finds the tools on
`PATH`, then where `pg_config --bindir` says, then in the newest of the
usual installs (Postgres.app, `/usr/lib/postgresql/*` on Debian and
Ubuntu, Homebrew's `postgresql@*`, `/usr/local/pgsql`), and fails saying
how to get a server when there is none. It makes a cluster in a new
temporary folder (user `jadpo`, trust authentication, UTF-8, C locale),
listening only on a socket in that folder with durability off and 300
connections, and stops it and removes the folder when dropped or, failing
that, once the process that started it ends. The driver's live tests
(`crates/jadpo-test/tests/postgres.rs`) share one server per process and
create a database each; `JADPO_SKIP_POSTGRES=1` skips them, as the
quick gate's `compiler tests` step does, and the `live PostgreSQL` step runs
them. Migrations build one template per migration digest; per-test copies
(`CREATE DATABASE … TEMPLATE`) are prepared in the background (area 13).
Built in chunk 3.1 (`jadpo_test::database`), made to scale in chunk 3.11:
the server is started the first time a test uses a store and kept for the
process; each store's template is named after the digest of its
migrations' names and digests (`jadpo_tpl_<store>_<digest>`), built under
a name of its own and renamed, so processes sharing a server, at once or
one after another, build it once and never see half of one. Four worker
threads keep copies ready, four beyond those tests hold but never more
than the run's action tests can use. A copy a test gives back is put back
as the template has it, with its tables truncated (all but
`jadpo_migrations`) and its sequences restarted, and used again; a
template whose migrations left rows in a table or moved a sequence can't
be put back that way, so its used copies are dropped instead, sixteen at
a time or when the run ends, since every drop waits for a checkpoint of
the whole server. When a run ends its copies are not dropped but left,
emptied, under `jadpo_free_<template hash>_…` names, and a later run on
the same server (a later `jadpo test`, or the next test binary) takes one
by renaming it to a name of its own before it makes a new one. Every
database a process creates is named with a tag of its own (its id and
start time), so names never collide. A test whose database couldn't be
made faults at the store operation with the reason. `--keep-failed` keeps
a failed test's copy and names it in the report (`data: kept as …`). On
this machine (ten cores, alone on a fresh server) the server, the template
and the first two copies take about 1.5 s; eight more copies, each used,
about 0.05 s in all (3.7 s before chunk 3.11); and sixty tests that each
write, on ten threads, about 1 s (8.4 s). Copies are made only for action
tests in a project with tables, and a test that never uses a store starts
nothing.

**One server per gate run** (chunk 3.11). `tools/verify.py` starts
`jadpo-test-server` (a binary of `jadpo-test` that starts the disposable
server above, prints its admin URL and keeps it until its input closes),
beside the steps that don't need it, and gives every step that uses
PostgreSQL that URL as `JADPO_TEST_DATABASE_URL`: the live steps and, in
the full profile, the language pages and the teaching examples. So no test
binary and no `jadpo test` call starts a server of its own, and the live
steps run at once rather than taking turns. When `JADPO_TEST_DATABASE_URL`
is already set (CI) that server is used instead. Under `--profile quick`
nothing starts PostgreSQL: the pages and `tools/examples.py` get
`JADPO_SKIP_POSTGRES=1`, check and format a project with migrations as
usual, and print `skipped: jadpo test for …` instead of running its
tests (today only `examples/10-library-loans`), which the gate shows under
the step.

**No live test skips unseen** (chunk 5.8). Every Rust test with a live
PostgreSQL part asks one helper whether it runs, `jadpo_test::live()`
(`crates/jadpo-test/src/live.rs`), and the gate finds those test files by
that call: each gets the run's server, in a step of the `POSTGRES` table
in `tools/verify.py` or, if the table leaves it out, a step of its own, so
no list can be forgotten. `JADPO_SKIP_POSTGRES=1` skips a live part with a
`skipped:` line; the full profile's `compiler tests` step also sets
`JADPO_REQUIRE_LIVE=1`, so a live test that reaches it, one the gate failed
to give a server, fails rather than skipping. A server too old for one
test (Postgres.app's pgvector 0.7.0 for a nearest query) is
`jadpo_test::skip_for_version`: skipped and shown under its step on a
developer's machine. Under CI (`CI` set) both panic, and any step whose
output has a `skipped:` line fails, since CI's image has every extension
and version the tests need. `tools/verify.py` checks these promises
itself before it runs (`self_test`).

**Teaching examples and pages with migrations.** A teaching example
commits its migrations like any project. A language page's `check`
project can carry `migrate=<purpose>` on a block: once the blocks up to it
are written, `tools/language_docs.py` runs `jadpo migrate new <purpose>`
with `SOURCE_DATE_EPOCH` fixed per step (the reproducible-builds variable,
which `jadpo migrate new` names folders by), so a page shows projects with
migrations without holding their generated files.

**Example-name ban.** `tools/example_names.py` extracts every declared name
from the language pages' code blocks and from `examples/` each time it
runs, minus the committed allowlist of generic words
(`tools/example-identifiers-allowed.txt`: `id`, `status`); the extracted
lists are never committed (chunk 5.8), and `--list` prints them. The gate
fails if a banned name appears in any case style as an identifier or string
in `crates/*/src/`. An example entity whose name is an allowed word (`Order`,
`Session`) is banned as a type name instead, and may not stand
as a name in a string literal without spaces (`"Order"`), the sign of
special-casing, while the word stays usable in identifiers and prose
(Richard, 2026-10-13). Since chunk 1.9 that holds for every example type,
not only entities, except the few the language itself also names
(`tools/example-type-names-allowed.txt`), and the standard library's Jadpo
stubs and names list are scanned too. Tests, including inline
`#[cfg(test)] mod` blocks (the attribute itself, not a comment mentioning
it), may use them.

| Phase | `tools/verify.py` adds |
|---|---|
| 1 | `cargo fmt --check`, `clippy -D warnings`, `cargo test`, the dependency test, the name ban, docs `check` blocks, format idempotence over every example, wasm32 build of the runtime crates |
| 2 | `jadpo test` over the spec corpus; a watch-mode latency test with a ceiling of the 1-second target plus a CI allowance (both built: chunks 2.2 and 2.3; tightened in 2.8); the teaching examples checked, formatted and tested, and the coverage test (chunk 2.4); every proved construction in the corpus checked again as it runs (chunk 2.7) |
| 3 | Live PostgreSQL (`--profile quick` skips it, and starts no server): the driver, the test database and `jadpo migrate apply` and `status` (built: chunk 3.1); queries, `get` and `create` through `jadpo test` (chunk 3.2); writes, conflicts and counters through `jadpo test`, and a concurrent change forced from a second connection (chunk 3.3); lifecycles and their transitions through `jadpo test`, and purge through its entry point (chunk 3.4). A golden SQL file per query and write shape (`crates/jadpo-plan/tests/sql/`) holds the statements for review |
| 8 | The differential run; the golden todo built for wasm32 and smoke-tested in local workerd |
| 10 | `--require-golden`: every golden contract case on both targets |

**How the gate runs** (2026-10-15). `tools/verify.py` builds the workspace
and every test binary once (`cargo test --workspace --no-run`, which also
builds `jadpo` and `jadpo-test-server`), starts the run's PostgreSQL
server (above), then runs the steps concurrently: each test binary
straight from that build, from its package's folder, a processor's worth
at a time; the Python checks beside them, given that `jadpo` through
`JADPO_BIN` and checking their projects concurrently; cargo's own steps
(clippy and the wasm32 build) one after another (chunk 3.12 dropped the
documentation-test step: no crate has a documentation example, so it ran
none); and
the watch-mode latency test last, alone, since it measures time. Each
crate's integration tests are one binary (since chunk 3.11: `autotests =
false`, one `[[test]]` called `integration`, and `tests/main.rs` declaring
each test file as a module, which the crate-graph test checks, so no file
is left out), so the workspace links eleven integration binaries rather
than fifty-odd; a named step runs its test files by name
filter (`proofs::`), fails if a filter matches no test, and `compiler
tests` skips those files, so no test runs twice. Each step prints `ok` or
`FAILED` as it finishes, then the failures' output with a command that
reruns each failing binary or file (`cargo test -p jadpo-cli --test
integration queries::`). `--profile quick` leaves out the live PostgreSQL
steps and the wasm32 build and skips `jadpo test` for projects with
migrations, so it starts no PostgreSQL. On a ten-core Mac with nothing to
rebuild, the full gate takes about 14 s and quick about 11 s (15 s and
12 s before chunk 3.11, measured alternately on a machine busy with other
builds); touching a crate low in the graph costs about 8 s of building
(12 to 15 s before), and that full gate about 27 s (31 to 35 s). `--group lint|docs|tests|wasm32` runs one
CI job's share, and `--require-examples` makes the coverage test fail on a
feature no example uses (above).

**CI.** GitHub Actions on Linux runs the gate's four groups as parallel
jobs, each with the pinned toolchain and its own cargo cache: `lint`
(formatting, clippy, the name ban, the editor grammar), `docs` (the pages
and the teaching examples), `tests` (every test, with the live PostgreSQL
steps running at once on one PostgreSQL container named by
`JADPO_TEST_DATABASE_URL`) and `wasm32`; `docs` and `tests` each build and
start their own container from `tools/postgres/Dockerfile`, PostgreSQL 16
with pgvector and PostGIS at pinned versions (chunk 5.5v1; area 21,
"Chosen in chunk 5.5v1"), while the toolchain installs. A local run's
disposable server comes from the machine's own PostgreSQL tools, which
need pgvector for the vector tests and PostGIS for the geography tests
(Postgres.app ships both). The
`supported-language` job needs all four and passes only when each did; it
is the check branch protection requires. Benchmarks (Phase 9) are a
separate command, not the gate.

### 2.8 Determinism and the build stamp

- **Same source, same bytes** in `build/`: no hash-map order in output,
  project-relative paths, no timestamps, per-build numbering in source
  order. A gate test checks each corpus project twice and compares.
- **The build id digests inputs, not the binary:** the canonical subgraph
  the deployable reaches, the compiler version, the target and the lock
  (area 18). `jadpo incident` therefore needs only `check` of the stamped
  commit with the stamped compiler, not rustc.
- **Toolchain pinning.** Each Jadpo release pins one Rust toolchain and
  ships the runtime crates' `Cargo.lock`. `jadpo build` writes a
  `rust-toolchain.toml` into the generated crate, builds `--locked` with
  `--remap-path-prefix` and a fixed `SOURCE_DATE_EPOCH`
  ([reproducible Rust](https://reproducible-builds.org/docs/rust/)), and
  pins wasm-bindgen and wasm-opt through `WASM_BINDGEN_BIN` and
  `WASM_OPT_BIN`, since `worker-build` otherwise downloads unpinned ones.
  Bit-identical binaries are a goal checked in CI, not a dependency of
  incidents.
- **The stamp** (build id, commit, modified flag, compiler version,
  deployable, target) is a constant in the binary and on every log event.

### 2.9 Risks and the first chunks

| Risk | Guard |
|---|---|
| Interpreter and compiled code diverge | One plan, one runtime, codecs from the same type plans, the differential run |
| The runtime grows without seams | Components behind traits; capability features per deployable |
| salsa's breaking releases and advisories | Exact pin, two crates, upgraded in its own change |
| workers-rs is 0.x | Only `jadpo-host-workers` touches it; our own JavaScript glue where it falls short |
| PostgreSQL from Workers, Wasm slower than Bun | Hyperdrive with caching off, unnamed statements and each transaction's isolation level set in its own `BEGIN`, and typed codecs, measured in the performance spike (chunk 8.3), before the Phase 9 benchmarks |
| Graph shape fixed too early | Kinds for later areas reserved now; `format` changes only on removals or changed meaning |

**1.1 Workspace and CLI shell.** `rust-toolchain.toml`, the root
`Cargo.toml`, `jadpo-diag`, `jadpo-term` and `jadpo-cli`, and the
dependency test with the whole intended graph. `jadpo-term` first: output
modes, colour and symbol detection, the grid, JSON envelopes. Then a
declarative command table that generates `--help` with examples, `jadpo
help --json`, Tab completion and nearest-command suggestions; commands
appear as they are built. The name ban and the Phase 1 gate steps.

**1.2 Parser, syntax tree and formatter.** `jadpo.ungram` from the decided
syntax of areas 1–4, with other areas' fragments as parse-only tests; the
lexer, event parser and rowan tree; generated AST; the formatter with
idempotence tests; `jadpo fmt`; the syntax errors' new output on the area
pages.

**1.3 Project layout and domains.** `jadpo-db` with salsa inputs for files
and layout; discovery by walking up to `app.jadpo`; item trees and the
symbol table; area 1's layout, naming, visibility and resolution errors,
each example converted to a `check` block; `jadpo check` printing them.

### 2.10 Questions for Richard

1. **Which store holds an action's outbox rows?** Area 12 says events go
   through "the main store's outbox"; area 5 (same day) puts the outbox,
   delivery records and idempotency keys "in each store that actions write
   to". They disagree when an action writes a store other than `main`, and
   neither covers an action that writes nothing (or only Redis) but emits
   or takes an idempotency key. *Recommendation:* area 5's rule, because an
   outbox row must commit with the writes it announces; an action writing
   no PostgreSQL store uses a store named in `application { events: {
   store: main } }`, required only when such an action exists, as area 5
   names stores for other system features. The runtime sweeps every store
   holding an outbox.
   *Decided 2026-10-13 (Richard):* area 5's rule, with nothing defaulting to
   a store called `main` or any other: the outbox, delivery records and
   idempotency keys go in each store an action writes (correctness requires
   it, and the build report shows where), and an action that writes no store
   but emits or takes an idempotency key needs the store named explicitly in
   `events: { store: <connection> }`; without it, a compile error names the
   action and the setting.
2. **Where do `flags.jadpo` and the per-domain monitoring file live?** Area
   1 fixes the root entries and a domain's folders and makes any other
   `.jadpo` file an error; area 16 adds a root `flags.jadpo` and "one
   monitoring file per domain" without saying where. *Recommendation:* add
   `flags.jadpo` to area 1's fixed root, and allow one `monitoring.jadpo`
   directly in a domain folder. *Decided 2026-10-13 (Richard):* as recommended.
