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
| `jadpo-std` | Shared leaf: built-in types, exact numbers, value rules and their validators, the standard types' data entries and storage codec, the standard library's enums Rust interprets, the typed operation table; the plans both executors run, as plain data (type and failure plans, constants, read, write and purge plans, the route, settings, JSON, translation and prompt tables) | yes |
| `jadpo-diag` | Rule catalogue, diagnostic model, edits | |
| `jadpo-syntax` | Lexer, parser, lossless syntax tree, typed AST, `jadpo.ungram` | |
| `jadpo-fmt` | Formatter | |
| `jadpo-sql` | Every PostgreSQL text the compiler writes: conditions and their one lowering (an exact SQL form, or an index search the runtime rechecks), statements, aggregates, identity lookups and table definitions, rendered from what the graph, the checker and the plan decide | |
| `jadpo-graph` | Node and edge model, identity keys, canonical JSON, diff; the physical schema, its snapshots and migration diffs | |
| `jadpo-db` | Incremental database: source inputs, project layout, the lock and the identity registry; the only compiler crate that does I/O (the before side from git comes later) | |
| `jadpo-check` | Resolution, types, failures, effects, policy, data and layout rules, as queries that build the graph | |
| `jadpo-plan` | What both executors need besides control flow: numbered callables and their frame tables, type and failure plans, constants, each read and write site's statements with policy compiled in (written by `jadpo-sql`), codecs per type, and the route, settings, JSON, translation and prompt tables; schedule and subscriber tables come with area 12 | |
| `jadpo-interp` | The interpreter | |
| `jadpo-codegen` | Rust source from graph and plan; drives cargo | |
| `jadpo-value` | Runtime values, operators, JSON codecs, validation, standard-library implementations | yes |
| `jadpo-runtime` | Everything in §2.6, behind a `Host` trait | yes |
| `jadpo-drivers` | Capability drivers over a host-supplied transport, a cargo feature each: PostgreSQL and the package driver (JSON over HTTP, chunk 7.2c) now; Redis, SMTP and S3 with their areas | yes |
| `jadpo-host-native` | tokio and hyper: the HTTP listener, timers, the background work a response leaves, passkey ceremonies, the disk driver `jadpo dev` keeps files with (chunk 6.4b), HTTP for the package driver (chunk 7.2c); the inbox with email | |
| `jadpo-host-workers` | workers-rs: fetch, scheduled, `waitUntil`, sockets, bindings | only |
| `jadpo-term` | The CLI design language: rich, plain and JSON output | |
| `jadpo-test` | Test runner, PostgreSQL harness, field-by-field value diffs; the differential comparison with compiled code | |
| `jadpo-lsp` | Language server | |
| `jadpo-cli` | The `jadpo` binary: commands, help, completion | |

Built so far: every crate above but `jadpo-codegen` and
`jadpo-host-workers`, which come with Phase 8.

**`jadpo-std`** decides once what both sides need: what a valid value is
(`Rules` and the validators the checker runs on literals and the runtime on
values), what each standard-library operation takes and gives
(`OPERATIONS`), and each standard type's data entry (`standard`, area 19:
its text form, JSON shape, storage and validator), with the storage codec
that binds a value to its column (`standard::storage::bind`). The plans the
runtime runs are its plain data, so compiled code needs nothing from a
compiler crate.

**`jadpo-sql`** writes every PostgreSQL text, each a function given the
columns, conditions and values its caller chose, so the graph, the checker
and the plan keep no SQL of their own. A condition becomes SQL in one place
(`condition`), which the plan's statements and the checker's partial indexes
both use; `literal` writes a value as SQL text where a statement takes no
parameters (a partial index's predicate, a backfill), as the storage codec
made it.

**`jadpo-graph`** holds the application graph (§2.3) and the physical
schema (`schema`: stores, tables, columns, keys, constraints and indexes,
each with its database name and born name), a migration's `snapshot.json`
and the structural diff between two schemas; `migration` works out which
changes need a person's decision and writes each store's SQL. The schema
sits here, not in `jadpo-plan`, because `jadpo check` and `jadpo migrate
new` both read it.

**`jadpo-db` and `jadpo-check`** are the incremental compiler (§2.2, §2.4),
the only crates that use salsa. `jadpo-check` builds the schema the
entities ask for, system tables included, and compares it with the latest
snapshot. The standard library is declared for programs in Jadpo stubs
embedded in `jadpo-check` (`std/`), which a test holds to `jadpo-std`'s
operation table on arity and kinds; a namespace the compiler fills
(`clock`, `sessions`, `oauth`) lists its built members in `std/names.txt`,
and any other member is `toolchain.not_built_yet`.

**`jadpo-plan`** reads a checked project once and numbers what both
executors run (§2.5): each callable and site, each type's validator chain
and codecs, each statement, and the tables above. A rule across an object's
fields lowers to a `CrossRule`, which `jadpo-value` runs, so builds, codecs
and compiled code check one implementation.

**`jadpo-value`** implements each standard-library operation once, as one
Rust function both executors call; `run` matches on the table's `Op`
exhaustively. Its pure-Rust dependencies build for wasm32 with their data
compiled in (jiff with its bundled IANA zones, ICU4X with Unicode CLDR), so
every target reads the same zones and locale data and none reads the
platform's. Operations that read the generator or the clock run only in
actions and tests, given the invocation's random bytes or clock.

**`jadpo-runtime`, `jadpo-drivers` and the hosts** are §2.6: the runtime
owns the action boundary and the components, a driver implements one
capability over a transport the host supplies, and a host is the only code
that knows its platform. The PostgreSQL driver sends pgvector and PostGIS
values in their binary forms with no type number, knowing those columns by
their type's name, and refuses to encode a geometry whose positions mix
with and without altitude, since EWKB flags a third coordinate once for the
whole geometry; storage itself binds a geometry without asking, because the
checker and every reader refuse such a mix (area 20, "Altitudes are all or
nothing").

**`jadpo-term`, `jadpo-test`, `jadpo-lsp` and `jadpo-cli`** are tooling:
the CLI design language's screens, the test runner (§2.5, §2.7), the
language server, and the binary that nothing depends on.

**Dependency direction.** Compiler crates never depend on runtime crates,
except `jadpo-interp`, which links the runtime it shares with compiled
code. Runtime crates never depend on compiler crates, and a generated
application depends only on runtime crates and the shared leaf. The one
shared crate, `jadpo-std`, depends on no workspace crate, so both sides may
depend on it: what a valid value is, and what each standard-library
operation takes and gives, is decided once (decided 2026-10-13, Phase 1
review). The layers are a table in the root `Cargo.toml`
(`[workspace.metadata.layers]`: shared, compiler, runtime and tooling, with
every workspace crate in one). A workspace test
(`crates/jadpo-cli/tests/crate_graph.rs`) reads it and `cargo metadata` and
fails on anything a crate reaches, directly or through other crates,
against these rules: compiler crates reach no tooling crate either, nothing
depends on `jadpo-cli`, salsa is confined to `jadpo-db` and `jadpo-check`,
and `jadpo-sql` is a leaf that reaches only the shared crate.
Dev-dependencies are not part of the graph. *Rejected:* v0's single `core`
crate, which grew to everything and let the generator read the syntax tree.

**wasm32.** The runtime crates and the shared leaf build for
`wasm32-unknown-unknown`, checked by the gate, which reads the list from
the layer table, less the crates `[workspace.metadata.targets]` lists as
`native_only`; `jadpo-drivers` builds with every driver feature on.
`rust-toolchain.toml` pins the target with the toolchain. The maths on `Decimal` runs there too, not only builds:
the gate builds `jadpo-value`'s `decimal_maths` example for wasm32 and runs
it under Node (`tools/run_wasm.mjs`), holding it to the reference values the
native test checks. Compiler crates do no I/O below `jadpo-db`, which keeps
a browser playground possible later without gating on it now.

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
| Types | type (base `Object`, `Enum`, `Input`, `Output`, `Failure`, `Event`, `EmailMessage`, `EmailPartial` or a scalar refinement), field, variant, constraint |
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
unchanged package costs nothing after its first check (area 11). Built in
chunk 7.1c2: an installed package's resources wait in the project's
`lazy` input until the app writes `<connection>.<resource>`, and the cache
holds the package files `jadpo check` found clean, which it then skips
(area 11, "Chosen in chunk 7.1c2").

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
| `effects::run(ctx, effects, effect, values)` | `clock.now`, `random`, `crypto` with its key, `json.read` and `json.write` with the plan's codec, and `emit` (chunk 7.0b) |
| `effects::call(ctx, call)` | Every call that leaves the process, through the capability seam (chunk 7.0c, §2.6) |

**Operations with effects** (chunk 7.0b). Everything a body does that
reads the invocation rather than only its values runs in one runtime
function, `jadpo_runtime::effects::run`, which both executors call, so
the interpreter keeps no copy and compiled code (Phase 8) re-implements
none of it. The effect is named, never told by its source text:
`Effect::Now` for `clock.now`, which the graph lowers to its own
`Expr::Now` (a site that pauses, as it was); `Effect::Std { op, json }`
for the standard operations `effects::is_effect` names, which
`jadpo_value::run` refuses: `random`'s, from the invocation's random
bytes, kept on a recording's tape; `crypto`'s but `read_certificate`,
given the key ring its `key:` names (`effects::Keys`, read from the
settings), its random bytes (secret for an encryption's nonce) and, for
`verify_jwt`, its clock; and `json.read`, `json.write` and `verify_jwt`'s
`as:` claims, through the codec the plan numbers for their site (`json`);
and `Effect::Emit { event }`, whose payload is written as its type's
contract JSON and kept, with its subscribers, in the running action's
outbox. What they need of the program comes as `Effects` (the plan's JSON
table with the program's validators and shapes, the events' numbers and
subscribers, the settings and the keys), which the interpreter builds
from its plan and compiled code will hold as statics. A refusal comes
back as `Refused::Broke` (a condition the call's `attempt` maps, with how
a report names it and, for `mismatch`, its `DecodeProblem`),
`Refused::Fault` or `Refused::Defect`, which the executor raises at its
site. `the_interpreter_runs_effects_only_through_the_runtime` holds the
interpreter to it: none of its sources may name what runs an effect (the
clock, random bytes, `crypto::run`, a key ring, the JSON reader, the
outbox), and each such name must still be the entry point's. Since
chunk 7.0c an outside call is another effect, run by `effects::call`,
asynchronous since it waits on the outside (§2.6, "The capability
seam"); the interpreter may not reach a driver (`.capability(`) of its
own either.

**The context's parts** (chunk 7.0b). `Ctx`'s fields are private and
reached through methods. Its background-run state is one struct,
`Background` (the system and its tenant, the subscriber's delivery whose
handled record commits with the action, the chain and the trace, the
work left for after the response, and the events kept), which the
runner and the boundary set through `Ctx::background_mut`; its
capabilities another, `Capabilities` (the host, through which its stores,
passkey ceremonies and random bytes are reached, the identities made from
them and the recording tape), reached through `Ctx::capabilities` and the
methods that draw on them (`random_bytes`, `secret_random_bytes`,
`new_id`, `transaction`); since chunk 7.0c it also keeps the log of the
invocation's outside calls and what a recording keeps of their answers
(§2.6, "The capability seam"). The action boundary's own state is
`boundary::Boundary`, read and written field by field only in
`boundary.rs`; the runner's batches use its methods. The request's own state (its caller, client address, deadline
and a test's settings), the frames and the assertion being recorded stay
on `Ctx`. Its public methods are unchanged.

The runtime, not the executor, owns the action boundary: it opens the
transaction at the first store operation (and again after an outside call
closed what the action had only read, since chunk 7.4; §2.6), runs the
body (a closure either executor supplies), reruns it on a concurrent
change within the budget (area 6), commits, then leaves the outbox for
the runner (since chunk 6.1a; §2.6) and clears caches. Limits
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
commits. The second of area 6's budget isn't built: the boundary counts
attempts only. Caches come with area 5. Since
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
`test.fault`, and since chunk 6.11 `test.subscriber_failed`) and a rerun command with the seed; the run is `test.failed`,
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
too, so the interpreter only hands over the site's values. Since chunk
6.2c a test that runs a schedule, `schedule.run()`, is a schedule test
(`--kind schedule`): its `Schedule` site pauses like a call into the
boundary, and the interpreter calls the schedule's action through it as
the system with no tenant (`jadpo_runtime::schedules::test_run`), at the
test's clock. Since chunk 6.11 what a test's step emits is delivered
before its next statement: the runner turns it on for a test with a
database (`Ctx::deliver_between_steps`, with an order from the run's seed
and the test's key), `clock.advance` pauses like a step, and after each
step the executor calls `jadpo_runtime::test_steps::after_step`, which
draws each delivery due in the outbox at the test's clock from all those
due, delivers it as the runner does, and ends the test with the failure
of one that failed for good, `Ctx::undelivered` naming it for the
report's `test.subscriber_failed` (area 13, "Caches and events").

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
batch count. Area 12's
hourly purge schedule calls it since chunk 6.2c, through the executor's
`purge`, which gives the plan and its period; a live test calls it
directly (`crates/jadpo-cli/tests/lifecycle.rs`), stopping one run at its
budget, and another runs it on the tick
(`crates/jadpo-cli/tests/purge_schedule.rs`).

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
the operating system's randomness and tokio's timer. Since chunk 6.9
what a host does with a request and a tick is the runtime's
(`jadpo_runtime::serve::Serving`, over an `Application`, an executor
that gives its route table and codecs): it answers with `serve_request`,
hands what the response left to `runner::after_response`, runs the tick,
and tells an `Observer` of each request and tick. The native server's
handler that serves an application is `jadpo_host_native::http::AppHandler`,
`Serving` on a `NativeHost` with that host's deferred work, tracked ticks
and drain; Workers' fetch and scheduled events call the same `Serving`
(chunk 8.2). `jadpo dev` runs the interpreter on it, swapping in each
save that checks cleanly, and only observes it to print its lines. Since chunk
6.7 a stopped server drains (`Server::stop`): it accepts nothing more,
its timer starts no more ticks, and each worker waits, at most
`DRAIN_GRACE` (10 seconds), for its handler's background work
(`Handler::drain`, `NativeHost::drain`, which counts what it runs with
`NativeHost::track`) to stop and give back what it holds, its budget
ended through `Host::stopping`. `stop_signals` turns SIGTERM and SIGINT
into that stop, a second ending the process at once; `jadpo dev` stops
on them.

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

Built in chunk 7.3b: magic links (area 8, "Magic links"). The identity
reads `sign_in: { magic_link: }` into `Identity::magic_link` and
`on_first_sign_in:` into `Identity::first_sign_in`, whose action
`first_sign_in_action` checks once every signature is known; the schema
adds `jadpo_sign_in_requests` beside `jadpo_sessions`. The plan appends a
`RoutePlan` per `jadpo_std::http::MagicLinkStep` with `Run::MagicLink`
in the first API that accepts browser sessions, writes
`MagicLinkPlan::lookup` and `lookup_id` (a user by its address or `id`,
with its `active:`), numbers the first sign-in's callable, and adds
Jadpo's own email to the email plan (`EmailPlan::builtin`) with its
subscriber. The HTTP component answers the routes in
`http::magic_links`, with no idempotency key; the email's delivery makes
the link and the code (`magic_links::mint`) and `email::message` writes
it from Jadpo's messages, wrapped in the app's layout.

Built in chunk 7.3b2: the app's own sign-in email and sending (area 16,
levels 4 and 5). `MagicLink::email` and `send` are read as written and
checked by `sign_in_email` and `sign_in_send_action`; `MagicLinkEmail`'s
fields come from `jadpo_std::email` (`types::builtin_fields`, which
`pick:` copies from), and `SignInCode` is a `Builtin`. The plan sets
`MagicLinkEmailPlan::own` (the app email's key) and `send` (the action's
callable); with `send` the email's subscriber targets the action, with or
without an email connection. `email::message` fills the app's email from
the minted link and code and renders it as any email; the runner gives a
`send` action the `MagicLinkRequest` `magic_links::request_value` makes
for each attempt (`runner::run_job` runs it as a subscriber's action).

Built in chunk 7.3c: mail packages, partials and `email.sent` (area 16,
"Chosen in chunk 7.3c"). `jadpo_check::mail` reads each package's
`provides/email.jadpo` into a `Mapping` (`mail::mappings`) and checks
each email's `File` fields' sizes against it
(`mail::attachment_problems`); `emails::settings` takes a package's
connection that has one, and the plan carries it as
`EmailsPlan::package` (`PackageEmail`, its input as `MailInput`). The
runner's email delivery calls `email::send_package`, which builds the
operation's input as the provider's JSON, reads it with the input's
codec and makes the call through `provider::call`, unless the host's
driver for the connection `catches_mail()`, as a test's `Fake` does. A
partial (`Base::EmailPartial`, `emails::PartialFacts`) has template
callables keyed `email partial <name> html|text`; `{{ include }}` is
checked by `Body::include` (`IncludeFact`) and lowered to a
`Callee::Function` call given the including template's `locale` first,
its output joined unescaped; `emails::include_cycles` refuses a cycle.
`email.sent` is the generated namespace `email`'s member, checked by
`Body::sent_emails` and lowered to `Expr::SentEmails`, which
`Effect::SentEmails` answers from the email connection's
`Capability::caught()` as `SentEmail` records (`email::sent_email`).

Built in chunk 7.3b3: the app's own magic-link routes. `MagicLink::
generate_routes` is read, and `Identity::magic_link_routes` is what the
generated routes, the API record, OpenAPI and `jadpo routes` list.
`magic_links.request`, `.redeem` and `.redeem_code` are `SessionOp`s
(`magic_link_op`), checked by `magic_link_call`; `SignInToken` is a
`Builtin` and `MagicLinkRedeemed` a built-in object. The interpreter's
`session` hands them to `http::magic_links::{request, redeem,
redeem_code}`, which run the generated routes' own `Site` steps: `ask`
in the action's transaction with the email as the action's event, and
`by_token`, `by_code` and `person` in transactions of their own, with
`on_first_sign_in:`'s action run outside the waiting boundary
(`Ctx::take_boundary`). A `Refused` becomes the route's response or the
operation's failure (`Failure::retry_after` gives `Retry-After`); the
binding cookie travels as `Ctx::sign_in_cookie` in and `Binding` out
(`Held::binding`).

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
  `Deferred::Deliveries` (`Ctx::defer`), and a batch run it started as
  `Deferred::Batch`, never run in the request; `serve_request` moves what a request left
  to `Response::deferred`, and the host hands it to `runner::after_response`,
  which gives `Host::defer` a future running it once the
  response has been handed back: natively the HTTP worker takes it
  (`Handler::take_deferred`, `NativeHost::take_deferred`) and spawns it
  on its own thread's `LocalSet`, where it shares the worker's program,
  so no background thread needs a program of its own; on Workers,
  `waitUntil` (chunk 8.2). Each piece gets its own context under the
  host's clock and no seed, as the system; what it sets off runs after
  it;
- **while a route waits**: `runner::settle_within` runs a route's
  follow-on work before it answers (6.3's `settle:`), then gives the
  context back to its caller;
- **per item**: `runner::items` runs a callable once per item, and
  inside a `runner::Batch` (a transaction of its own, begun as it opens)
  each item's action joins it in a savepoint and is undone alone when it
  fails, as `boundary::Undo` undoes a caught call; the run writes its
  position in `Batch::transaction` before `Batch::commit`, so a stopped
  run resumes after the last committed batch (6.4).

A host with nothing to run deferred work on (a test's, a tool's) drops
it, and the outbox keeps what it held. Schedules (6.2) run their actions
through the same runner.

**Within a host's budget** (chunk 6.7, area 12's "Running background
work").
Each invocation of background work (`budget::Invocation`: after a
response, the tick, or work handed on) runs within the budget its host
gives it (`Host::budget_ms`, at most `RUN_LIMIT_MS`, 15 minutes; the
runtime's default and the native host's is all of it, a test's host may
give less, Workers' come in 8.2). `budget::Budget` measures it on the
host's monotonic clock and keeps a tenth back, at most 5 seconds, for
giving claims back: `runner::drive` takes a piece only while it isn't
`spent`, and what it doesn't reach stays in the store (a delivery
`pending`, a batch run on its row), handed on by `after_response` to a
fresh invocation (`Host::hand_on`, natively a task of its own) when the
invocation ran something, else left for the tick. One helper runs an
action within it, `runner::within_budget`: the body raced against
`Budget::cut` (the run limit or what is left before the time kept back,
whichever is sooner, or the host shutting down, `Host::stopping`),
never the commit (`held`, chunk 6.6a), with the interpreter's deadline
set to the same limit; work that isn't an action (a purge, a batch's
page) runs within it through `Budget::run`. A delivery or a schedule's
run the limit stops ran out of time (`deadline_exceeded`), settled
against its budget; one the host's shutdown stops is given back as it
was. One lease helper, `lease::{claim, release, until}`, serves the
three kinds of claimed row (`outbox::Leased`, `schedules::Leased`,
`batches`' own), each lease lasting what is left of the claimer's
budget and `lease::MARGIN_MS`, a minute; a batch run renews its own with
each page's commit.

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

**Delivering once, within limits** (chunk 6.6a). `boundary::seal`
writes a delivery's handled record after the outbox rows, in whichever
store the transaction is in by then, and a subscriber's plan names the
store its action writes or else its outbox store, so an action that
only emits keeps one. `runner::attempt_delivery` claims at the host's
time (a lost lease's last failure becoming `lease_expired`), gives up
without running a delivery claimed over its budget or past `within:`
(`outbox::GIVE_UP`), and runs the action within the invocation's budget
(chunk 6.7), or, while a route waits, against its `within:`, which
gives the delivery back. Either races only the action's body
(`runner::held`, `Ctx::hold` and `commit_held`, as a route does), never
its commit.
`runner::drive` reads the host's clock per piece, so the sweep never
runs late work at the tick's time. A batch run's row keeps the chain
depth it started at (`depth`), which each page's context takes; each
item resets the boundary's event allowance (`Boundary::allowance`); and
`batches::clean`, a tick hook, deletes runs that ended 30 days ago with
their problems and cancels.

**Schedules and the tick** (chunk 6.2). The checker reads each schedule
(`jadpo_check::schedules`: `every:` and `run:`, an action of its own
domain taking nothing) and where their state is kept
(`schedules_settings`: `schedules: { store: }` or the only database, else
`schedule.store_required`); the plan holds the schedule table
(`jadpo_std::schedule::SchedulesPlan`, `Plan::schedules`, which
`Executor::schedules` gives the runtime) with the stores holding the
runtime's own tables, and the stores sessions and service keys are kept
in. `jadpo_runtime::schedules::tick(executor, host,
now)` is the one entry point every host calls: it makes and touches each
schedule's state row (`jadpo_schedules`), takes the lease of each one due
in one conditional statement, runs them side by side as the system with
no tenant under the tick's clock and a new trace, records each run and
skipped tick (`jadpo_schedule_runs`) and delivers what they emitted; then
in each store it calls `runner::sweep` and deletes expired idempotency
keys; events every subscriber has had, with their delivery records and,
in the same statement, the handled records kept beside them, then the
rest of those handled records in each other store by the deliveries'
ids (chunk 6.10); and ended sessions, tokens and service keys where
they are kept; then it runs `schedules::HOOKS`,
which picks up batch runs whose lease lapsed (`batches::resume`). The
whole tick runs within the budget its host gives a tick (chunk 6.7),
each step starting only while it isn't spent, its runs' actions through
`runner::within_budget`. Natively the HTTP server's first
worker runs a timer that starts the tick at each whole minute of UTC, or
each shortest `every:` (`Handler::tick`, `schedules::tick_every_ms`);
`jadpo dev` ticks. Since chunk 6.9 the tick raises the alerts too
(`watch::raise`): once its schedules have run, it works out which hold
for each schedule and records them on its state row (`alerts`, each id
with when it was raised) in one conditional statement, so of several
instances' ticks, or Workers invocations, the one that changed the
record raises what newly holds (`Ticked::raised`), once until it stops
holding. Readers (`jadpo schedules`, `jadpo why`) still work the alerts
out from the rows when they read, since a stopped application raises
nothing; `jadpo why`'s answer is decided by the runtime as data
(`schedules::why`), which the CLI says in words. On Workers the cron trigger calls the same function
(chunk 8.2).

**Batch runs** (chunk 6.4, area 12's "How a run is kept"). The
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
or a fault, `boundary::Undo`), then `finished:`, while its invocation's
budget lasts; once that is spent it gives its lease back and is handed
on to a fresh invocation (`run_within`, `Ran::HandedOn`; chunk 6.7);
`resume` picks up runs whose lease lapsed, for 6.2's minute tick; `status` and `cancel` read the
row and keep a cancel. An executor gives the runner its batches through
`Executor::batches`, so compiled code runs the same runner. In an
application with a tenant the plan's `BatchTenant` names which saved
values hold the tenant's id, and each page runs as the system limited to
the one tenant they name, or to none (`outbox::named_tenant`, shared
with a subscriber's delivery; chunk 6.4c). `start` keeps that tenant on
the run's row, and a `key:` is unique per call, tenant and key (chunk
6.6c). The checker proves each value naming the tenant came from a record
of it the action reached (`policy::flow::tenants`, as an event's tenant
field is), so a caller never aims a run or a delivery at a tenant they
don't belong to.

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
| Transactions | Read committed or repeatable read by what the action does, a snapshot closed before each outside call and row versions checked after it (chunk 7.4), a batch's item that calls a batch of its own (chunk 7.4b), rerun within the budget | same |
| Idempotency | A key claimed in one statement before the action; its response stored in the action's transaction, or in one statement for a failure | same |
| Outbox | Delivery, retries, dead letters and (7.2b) a circuit breaker, run by the runner after the response (chunk 6.1a), natively as a task on the worker's thread | `waitUntil` after the response, swept by the minute tick |
| Scheduler | Due times in the schedule's state row from its first tick, leases, run records; native timer, and tick adapters (8.2) | Cloudflare cron |
| Rate limits, caches | Rate limits through `Host::rate_limit`: Redis when one is named, otherwise each worker's bounded memory; cache reads with timeout, invalidation after commit | Redis or Cloudflare's rate-limiting binding required (area 16, question 6); the isolate's memory only while it can't be reached |
| Telemetry | Spans per declaration, metrics, path numbers, threshold sampling, OTLP | export in `waitUntil` |
| Email | Templates, translations, the `email` capability | SMTP on 587 or a package |
| AI | `ai.generate`: the prompt rendered, the schema, decoding and repair; the built-in `Driver.openai_compatible` and the `ai` fake (chunk 5.5v3); `ai.embed` and `ai.embed_all`, the fake's embeddings and prompts' recordings (chunk 5.5v4); `ai.stream` read piece by piece and sent as server-sent events (chunk 5.5v6) | same, through Workers' `fetch` |
| Receiving | Queues fetched on the tick through a package, each message kept once as its event (chunk 7.7c) | Cloudflare Queues' consumer hands each batch to `receiving::consume`, which answers each message (chunk 7.7c2) |
| Flags, privacy | Evaluation; export and erasure flows | same |
| Development | Request timeline, local inbox and disk, published-event log | none |

**Capability drivers.** `Database` (begin, query, execute, commit,
rollback, and one statement on its own) and `Passkeys` are traits of
their own, and `Cache` will be; every outside capability's driver,
`files`', `email`'s, `publishing`'s and a package's alike, is one trait,
`capability::Capability`, named by its connection, so the capability
seam logs, records, guards and fakes them all one way (chunk 7.0c,
below). Rate limits and the wall clock (chunk 4.10), and deferring work,
its budget, handing it on and shutting down (chunks 6.1a and 6.7), are
`Host` methods. A schedule's tick is no trait: every host calls
`schedules::tick`, natively from a timer and on Workers from the cron
trigger, and the `trigger:` adapter that lets another source call it
comes in 8.2; publishing events outward comes with Phase 7. Drivers
classify their own errors into the runtime's fixed set (conflict with the
constraint name, serialisation failure, unavailable, fault), so executors
and targets agree; the experiments found exactly this classification
drifting between adapters
([shared conformance](experiments/shared-rust-conformance-results.md)).
Package-backed capabilities use one generic driver, the package driver
(chunk 7.2c, below), over the `Fetch` the host gives, as the PostgreSQL
driver runs over its `Transport`; packages never generate driver code. Drivers
list their targets for `jadpo check` (area 11), and test fakes implement
the same traits.

**The capability seam** (chunk 7.0c, from the Phase 6 architecture
review; area 16, "Chosen in chunk 7.0c"). Every call that leaves the
process goes through one exchange, `jadpo_runtime::outside::exchange`
(since chunk 7.9d, "One outside-call path" below), which
`effects::call(ctx, call)`, what both executors call for a store's file,
an email or a capability's delivery, reaches as every other call kind
does: a `capability::Call` (its connection, operation,
input, and `Repeat`: `Safe`, `Keyed` or `Unkeyed`) is refused with
`FaultKind::CallInTransaction` while a transaction that has written is
open (`Ctx::transaction_open`), having first closed one that has only
read (chunk 7.4, "Snapshots before calls" below); in a replayed run it
is answered from the recording
(`capability::Recording`, the random bytes and each call's answer, which
`Ctx::record_run`, `recorded_run` and `replay_run` keep and give back),
a call the recording doesn't have next refused; otherwise it is sent
through the driver `Host::capability(connection)` gives, `misconfigured`
when there is none. The invocation's `Capabilities` log each call as it
goes out and when it is answered (`capability::Logged`: connection,
operation, `Repeat`, `Source`, when, how long on the monotonic clock,
`Ended`), never its input or answer; `Ctx::take_calls` takes it, and
`serve_request` leaves a request's on `Response::calls`, which `jadpo
dev` lists under the request's line (full values in development are the
host's call log, chunk 7.2d, below). A
driver's `Answer` decides what the call gives: its value, its
operation's own failure for the site to map (`effects::Unanswered::
Failed`), or the runtime's `unavailable`, `misconfigured` or, for an
unkeyed call that may have acted, `outcome_unknown` (`boundary::
outcome_unknown`). A subscriber's plan names its `Target`: an action,
or a capability's operation handed the event whole
(`Target::Capability`), or an email (`Target::Email`, chunk 7.3),
rendered whole by `email::message` (its templates' callables run by the
executor) and sent unkeyed, and `runner::attempt_delivery` runs each
within the same budget.
`Logged::may_have_acted` (an unkeyed call answered or still out) turns
what would be a retry into `After::Park`: the delivery record becomes
`outcome_unknown`, never claimed again. An unkeyed capability send marks
its record `sending` first (`outbox::MARK_SENDING`), so one whose worker
was lost is parked by the sweep (`outbox::PARK_LOST`) rather than sent
again; a subscriber's action lost mid-call waits for chunk 7.5's step
record. Fakes are host wrappers: `capability::Faked` gives a
`capability::Fake` (scripted answers per operation, a `Hang`, and what
it was sent) in some connections' place, everything else the wrapped
host's, through `Host` for `&H`. The runtime's Rust tests share one host,
`jadpo_test::host::Harness` (one store for every name, a clock the test
moves, randomness from a counter, timers that pass when told, a budget,
and the work it is given kept), which replaced twenty hand-written hosts
in nineteen of `crates/jadpo-cli/tests`' files; the live tests of
`capability_deliveries.rs` prove the seam with a test capability, the
`mail` connection's fake.

**One outside-call path** (chunk 7.9d, from the Phase 7 architecture
review; area 12, "One outside-call path, chosen in chunk 7.9d"). Every
call kind (a provider's operation, `ai.generate`, `ai.embed`,
`ai.stream`, a store's file, an email, a capability's delivery) is
decided and sent by `jadpo_runtime::outside`, and each kind's module
only builds its request and decodes its answer. `outside::comes_from`
says, before anything is sent, where the answer comes from
(`outside::Comes`): the action's record of its steps (`steps::step`
with the call's `steps::Proposed`; `Comes::Recorded`, logged as the step
it is), an `ai` call's answer kept by an earlier run for area 6's rerun
(`Rerun::Answer`, by `Ctx::next_step` and the request's digest;
`Comes::Reused`), an `ai.stream`'s open stream (`Rerun::Stream`;
`Comes::Opened`), or a send at its `outside::Place` (its step of the
record, its place for a rerun); `outside::given_back` logs a call answered
without going out, from any of them or a replayed run's tape, and
`outside::keep` keeps an `ai` call's answer where it is given back
from. `outside::send` is the one retrying send: each send through
`outside::exchange`, and again while that is safe (one that never left or
that the provider said did nothing; one that may have acted only when it
isn't `Unkeyed`) and its `outside::Retry` allows: a provider's `retry:`
(`Retry::of`), an `ai` call's `jadpo_std::ai::SENDS` within its
`timeout:`, each send given what is left of it as its timeout and none
made once none is (`Retry::timed`), or once (`Retry::ONCE`, a store's
file, an email, a capability's delivery). Its waits are
`provider::retry_wait`'s, never past what is left of the window nor of
the invocation's time, `Ctx::deadline_at` (the route's deadline, or
background work's budget less what it keeps back, as
`Ctx::set_deadline` set it): a wait that wouldn't end within both ends
the sending and the last answer stands. An `ai` call's send again first
takes what it sends from its limits (`Sending::resend`, chunk 7.9c), and
each of its sends goes to the development call log as it is answered
(`Sending::log`). `outside::exchange` is one send, its answer asked for
whole or as it comes (`outside::Form`, `Capability::call` or
`Capability::stream`): the snapshot closed and an open transaction
refused, a replayed run's tape read, the driver found, then one of the
connection's `max_concurrent:` slots taken (`Host::slots`), the step
written (`steps::sending`), the driver asked, the call logged, the slot
given back once answered (a stream's once its answer starts to come), and
`jadpo test`'s crash after a step's send. The cap is the provider plan's
`ConnectionPlan::max_concurrent`, for a provider operation's call and, where
a package fills `ai`, for the `ai` calls through its connection
(`ai::max_concurrent_of`).

**Snapshots before calls** (chunk 7.4; area 6, "Outside calls hold no
transaction"). The seam closes what the running action has only read
before a call goes out (`Ctx::close_snapshot`), so no transaction is
open while any call is out. The boundary keeps whether its transaction
has run anything but reads: reads (`read::read`, a gate's roles, a
batch's status, the token lists) take it through
`Ctx::read_transaction`, everything else through `Ctx::transaction`,
which marks it as written. A plan's `Select` that reads an entity's `id`
carries `RowVersions` (its table, the `id` column and
`jadpo_sql::statement::row_versions`), and each read in a transaction
that has only read keeps the ids it loaded (`Ctx::loaded`); closing it
reads their `xmin` by table, one statement each, keeps each row's first
version on the boundary, and rolls back. The next store operation begins
a new transaction at the same level in the same store, counted by a
generation `boundary::Undo` compares, so a caught action whose call
closed the snapshot its savepoint was in has its own transaction rolled
back whole instead. Every update and delete plan carries a `RowCheck`
(its table, where its `id` comes from, and `row_version`'s statement,
`FOR NO KEY UPDATE`): before the write, a row loaded before a call is
read and locked in the new transaction, once per transaction
(`Ctx::check_row`), and another version, or none, is
`StoreError::ConcurrentChange`, which reruns the action within
`RERUN_ATTEMPTS`; the rerun takes the same steps, so a keyed call
repeats under its key. A reference's refusal planned as `Raises::Gone`
is a concurrent change too once rows were loaded before a call. The SQL
goldens show both statements.

**Calls before writes, checked** (chunk 7.4b; area 6, "Several outside
calls"). `jadpo_check::types::outside_calls` works out, once per project,
the outside call (`Callee::Provider`, `Callee::Files`) and the write
(`Expr::Write`, an entity's other operations, a session or key operation
that isn't `reads_only`, a batch operation other than `status`) each
action and fixture makes, itself or through the actions it calls
(`file_outside` per file), never through a run's `each:` and
`finished:`. `call_order_problems` walks each body in the order it runs
(`Walk`: each way through an `if` or a `match` on its own, a `return`,
`reject` or `propagate` ending its way, a loop's body twice) and reports
each write some way runs before a call as `data.write_before_call`, at
the write and naming the last such call, not declinable. The seam's
`call_in_transaction` stays as the guard.

**Several changing calls, checked** (chunk 7.5; area 12, "Several
changing calls in one action"). `jadpo_check::types::action_steps` works
out, once per project, what each action's and fixture's outside calls
come to (`Steps`): the most changing calls one way through it makes (a
provider operation with `changes: true`, a store's `put`), counted as
`call_order_problems` walks a body, an action called adding its own
longest way; how many it can make on every way; and the first call, the
first with no key and the first `large_output: true` one, each named as
written where it is made. `step_problems` reads it per file:
`effect.call_in_loop` for an outside call in a `for` body, made there or
by an action called there; `step.too_many` past 8 on one way;
`step.unkeyed` for a call with no key in an action that can make another
changing call; `step.large_output`; `step.context` where an action making
several on one way is run by a schedule, a route whose API doesn't
require `Idempotency-Key`, or a run's `each:` or `finished:`; and
`step.store_required` for one a subscriber, or a route whose API requires
the key (`resumable_runs`), runs with no store for its record. The
record itself is roadmap 7.5b's.

**The record of several changing calls** (chunk 7.5b; area 12, "Several
changing calls in one action"). `jadpo_check::types::record_plans` plans
a `jadpo_std::steps::RecordPlan` (its store and a digest of its step
shape) for each action `resumable_runs` names whose longest way makes
several changing calls, on its `CallablePlan::record`, with the plan's
`steps_key` (the `SigningKey` setting `application { steps: {
encryption_key: } }` names, read by `steps_settings`) and `build_id`;
the schema puts `jadpo_std::system::STEP_TABLES` in that store.
`ActionBody::record` gives the runtime a `steps::Spec`, and an outermost
boundary opens the record (`steps::open`) only when its runner set
`Ctx::set_resumable` with its generation: the runner's delivery attempt,
or a route's claim under a required `Idempotency-Key`. Opening takes the
record over for that generation, loads its steps (answers opened with a
ring `KeyRing::derived` from the steps key) and fixes the invocation's
clock at the record's first start; each rerun of the body rewinds it
(`steps::rewind`). A provider call and a store's `put` ask
`steps::step` with their operation and each top-level field of the
request: untracked, a recorded answer to replay (logged as
`Source::Replayed`), or a place to send at, whose row
`outside::exchange` writes (`steps::sending`) before the driver is
asked; the answer is kept sealed after (`steps::answered`), the
provider's `reused:` code and a request that differs are
`FaultKind::Diverged`, and a step in doubt goes again under its key only
within the provider's `window:` less its margin. A body that ends before
replaying every recorded step diverges (`steps::finished`), and
`boundary::seal` deletes the record in the commit transaction. Every
record write is one statement that takes the record over only from an
equal or older generation (`steps::SEND`, `ANSWER`, `DIVERGE`,
`DELETE`). The runner holds a diverged delivery as `outbox::DIVERGED`.

**A batch's items that make calls** (chunk 7.4b; area 12, "How a run is
kept"). The plan marks a `BatchCall` whose action makes outside calls
(`calls`, from `outside_calls`), and the runner runs each of its items
as a batch of its own (`batches::calling`): where the run is read in a
statement of its own (`PEEK`); a query's next item read in a snapshot as
a page of one, giving its own cursor (`read::read_as` with
`PageCursor::Last`), or a file's next row read first (one row a page);
the item's action run in a boundary opened without a transaction
(`runner::Batch::open_for_calls`), its calls keyed from the run and the
item's place (`Ctx::set_call_key("batch", …)`: the run's id and the
item's cursor, or where its row begins); then, in the transaction its
writes are in or one begun now (`Batch::writing`), the run's row locked
and held to the position read, the item's problem and the run's next
position, committed with its writes. A query's run is then at the item's
cursor; once a page of one finds none, the run is `read_all`. A
`finished:` whose action makes outside calls runs the same way
(`calling::finished`), keyed `finished`, the run's end committed with its
writes, for a query's, a file's and an export's run alike.

**Provider calls** (chunk 7.2c; area 11, "Chosen in chunk 7.2c"). A
provider operation's call is `Callee::Provider` (its connection and
`resource.operation`), which the interpreter runs through
`jadpo_runtime::provider::call` with the plan's `ProviderPlan`
(`jadpo_std::provider`: each connection's address, credential setting,
timeout, `idempotency:` and `error_code:`, and each public operation's
method, path, input and output numbered in the JSON table, `output_at:`,
`failed_when:`, `Repeat` and failures), the credentials `Program` holds
apart as it holds the keys (`provider::Credentials`), and the values of
the settings its address reads. The runtime writes the input with the
JSON table's codec, a provider package's `wire_name:` spellings
included (`FieldPlan::wire`, a variant's codec's `wire`), fills the
path, the query or the body, adds the credential and, for a keyed call,
the derived key (`provider::key`, from the invocation's `CallKey`, which
the HTTP component and the runner set, and its step, counted across the
invocation and taken again by a rerun), and sends a `Call` carrying the
`provider::Request` through `outside::send`. The package driver
(`jadpo_drivers::package::JsonHttp`, feature `package`) sends it through
its host's `Fetch` and classifies the answer (`provider::classify`):
`Answer::Wire`, the answer as sent, which a recording keeps, or the
seam's failures. The call's site decodes a wire answer from
`output_at:` with the output's codec, a provider type's shape open to
members it doesn't declare (`ShapePlan::open`), or raises the package
failure the provider's code names, which the interpreter maps by code
at the call's `attempt` (`RuleArm::named`, `Task::RaiseMapped`) or ends
in `FaultKind::Provider`. Natively the host's `Fetch` is
`jadpo_host_native::fetch::NativeFetch`: HTTPS through OpenSSL, or plain
HTTP to this machine, each exchange on tokio's blocking pool within the
call's timeout.

**Fakes and the call log** (chunk 7.2d; area 11, "Chosen in chunk 7.2d";
area 13, "Fakes for integrations"). A package's fake is a capability,
`jadpo_runtime::fakes::ProviderFake`: each call of an operation a test
scripted takes its next scripted `Answer`, and any other call is
answered `Answer::Generated`, which the call's site turns into the
operation's generated default (`fakes::default_answer`: `fakes::generated`
walks the output's codec, trying values that keep its rules and reading
each back, then `fakes::enveloped` puts it where `output_at:` reads it)
and decodes as any wire answer; `Capability::source_of` tells the log a
scripted answer from a generated one (`Source::Scripted`,
`Source::Generated`, `Source::Sandbox`). A test's `fakes:` is checked by
`types::body::fakes` (each answer a value of the output type, one of the
operation's failures' codes or a `FakeCondition`) and lowered to a
statement per answer, `Callee::Fake`, before the test's first statement;
the interpreter runs it through `fakes::script`, which writes a value as
the provider sends it (`fakes::as_sent`) and hands it to the fake its host
gives for the connection (`Capability::script`). `jadpo test`'s runner
wraps each test's host in `capability::Faked` with one fake in every
connection `Executor::provider_connections` names, and after the run
reports a script the calls ran past or left answers in
(`fakes::Usage`; `test.fake_exhausted`, `test.fake_unused`). `jadpo dev`
gives its `NativeHost` one driver for every provider connection
(`NativeHost::with_providers`), `jadpo_cli::dev_calls::DevProviders`: the
package driver for the operations the plan's `ConnectionPlan::sandbox`
(the connection's `development: { sandbox: }`) or `--sandbox` lets
through, the fake otherwise, answering what `jadpo calls script` left
in `build/dev/script/`. A host may keep a development call log
(`Host::call_log`, `call_log::CallLog`): a provider call's site hands it
each call as it ends, with the request as made but its headers and the
answer as sent or generated (`call_log::Seen`), and the request's id
(`Ctx::request_id`, which the HTTP component sets); only `jadpo dev`'s
host has one, writing `build/dev/calls.jsonl` for `jadpo calls`.

**A package's recordings** (chunk 7.2d2; area 11, "Provenance and
confirmation"): `recordings/<resource>.<operation>.json` in a package is
a package record (`jadpo_db::Record`), and the plan carries each
operation's text on `OperationPlan::recording` and every recording of
the app's own integrations as a `RecordedPlan` (its operation planned
whether or not a connection calls it). `jadpo_runtime::recorded` keeps an
answer as a recording (`recorded::kept`: the path to `output_at:` and
the members the output type declares, each plain text or number a
placeholder naming its built-in type, `"{Text}"`, where a generated value
reads back; what the package's own encodings and decoders read kept as
sent, digits included) and fills one back (`recorded::filled`, from
`fakes::generated`). `recorded::check` reads a recording whole through
the package's functions at the call's site, a `RoundTrip` coder checking
each field's `encode:` gives back what its `decode:` read; it is
`Executor::recording`, which `jadpo test` runs for each `RecordedPlan` as
a test of kind `recording` (`test.recording_undecodable`,
`test.round_trip_failed`), and the publishing bar counts as its
`decoders` row. `jadpo dev`'s fake answers an operation that has one
`Answer::Recorded` (`Source::Recording`), which the call's site fills
before decoding. `jadpo package record` calls the sandbox through
`Program::call_provider` on a `NativeHost` with the package driver, its
keyed call given a step outside an action (`Ctx::allow_standalone_calls`),
and keeps the answer the call log hands it.

**Asking a model** (chunk 5.5v3; area 21, "Chosen in chunk 5.5v3"). The
checker reads `application { ai: }` and each `Driver.openai_compatible`
connection (`jadpo_check::ai`), and checks `ai.generate` in a body
(`types::body::generate`) as an outside call whose five conditions are
construction-style `RuleKey`s an `attempt` maps (`refused`, `malformed`,
`timed_out`, `over_budget`, `too_long`), lowered to `Callee::Generate`
with its options; the plan decodes its site to a
`jadpo_std::ai::GenerateSite` (the prompt's entry and fields, the answer
type's JSON-table number, and the type's schema from
`jadpo_check::ai::schema`, OpenAPI's `Wire` with `$defs`) and carries the
connection and models as `Plan::ai`. `jadpo_runtime::ai::generate`, which
both executors call, renders the prompt as `prompts.render` does, builds
OpenAI's Chat Completions request with the schema cut for a strict mode
(`ai::strict`) or added to the instructions (`ai::with_schema`), sends
each attempt through `outside::send` as a `Call` whose `attempt`
counts its repairs, reads the completion, decodes the content through the
answer's codec (`Codecs::read_member` for a wrapped one), and gives
`Done::Broke` for a condition, which the interpreter raises as a
construction's broken rule. A call takes `Ctx::next_step`, and its answer
is kept with a digest of its request (`Ctx::keep_reused`) for area 6's
rerun, or, after an action's first changing call, it is a step of the
record (`steps::step` with `changes: false`). The native host gives
`JsonHttp` for the model connection (`NativeHost::with_models`), which
`jadpo dev` sets; `ai::AiFake` is in its place in every test, scripted by
`Callee::AiFake` and read by `Callee::AiRequests` through
`Capability::requests`.

**Streaming a model's answer** (chunk 5.5v6; area 7, "Streaming a model's
answer"; area 21, "Chosen in chunk 5.5v6"). `ai.stream` is checked in
`types::body::stream` (four conditions, `AiStream` an opaque built-in),
lowered to `Callee::Stream` and decoded to a `jadpo_std::ai::StreamSite`;
a route whose output is an `AiStream` (`routes::streams`) is planned with
`RoutePlan::stream` and no output codec. `jadpo_runtime::stream::stream`
sends the request through the one exchange, asked for as it comes
(`outside::Form::Streamed`: `Capability::stream`, which gives
`Streamed::Whole` or `Streamed::Coming`, a body read a `Chunk` at a time),
reads OpenAI's `chat.completion.chunk` events until the first piece, and
keeps the open answer on the invocation (`Ctx::keep_stream`), the
`AiStream` value naming its number; a rerun is given it back by step and
digest. Once the action commits, the HTTP component takes it into
`Response::stream`, an `EventStream` of `ServerEvent`s held to the call's
`timeout:` and the route's deadline on the host's monotonic clock, which
`Serving` attaches its host to. The native host sends it as a chunked body
fed by a task on the worker's thread, which stops when hyper drops the body
for a client that went away; `NativeFetch::open` reads the provider's
answer on a thread of its own and shuts its socket when the body is
dropped. `JsonHttp::stream` hands over a success's body as it comes;
`Fetch::open` defaults to the whole body. A route test reads every event
(`http::route_test`), giving `pieces` and `code`. The `ai` fake answers a
stream from its script in the driver's own form (`stream::faked`).

**Embeddings and recordings** (chunk 5.5v4; area 21, "Chosen in chunk
5.5v4"). The checker reads `embedding_dimensions:` into
`ai::AiSettings` and checks `ai.embed` and `ai.embed_all`
(`types::body::embed`) as outside calls with three construction-style
conditions, lowered to `Callee::Embed` with the model, timeout and the
vector type's dimensions; the plan decodes the site to a
`jadpo_std::embed::EmbedSite` (the vector type's JSON-table number).
`jadpo_runtime::embed::embed` sends OpenAI's embeddings request through
`outside::send`, as `generate` sends its attempts, reads each vector through
the type's codec, and keeps the answer for a rerun and as a step as
`generate` does; on `Answer::Generated`, the `ai` fake's default, it makes
each vector from the text's SHA-256 and `Env::seed` (`embed::seeded`).
Recordings of a prompt's answer are records (`jadpo_db::is_prompt_recording`)
found by `jadpo_check::prompt_recordings` and planned as
`Plan::prompt_recordings`, each with the JSON-table numbers of the types
its prompt's `ai.generate` sites decode into; `recorded(...)` lowers to
`AiFakeAnswer::Recorded`, which the interpreter scripts with the answer
`prompt_recordings::answer` fills (area 11's `recorded::filled_value`),
and `jadpo test` runs `Executor::prompt_recording` for each as a
`recording` test, listing stale ones as `Run::alerts`.

**Webhooks as events** (chunk 7.7; area 11, "Chosen in chunk 7.7"). The
checker reads a provider's `webhooks:` into `connections::WebhookReach`
(checked by `packages`), a connection's `webhooks: { secret: }`, and each
event of a package with webhooks (`webhooks::package_events`: its
`wire_name:` and `occurred_at: Instant`), gathering every connection
given a secret into a `webhooks::WebhookModel`: its route under the
`webhooks` API's prefix, its scheme, the places of the event's id, type
and time, its store and its events by key. The plan holds each as a
`jadpo_std::provider::WebhookPlan` on `ProviderPlan::webhooks`, its events
numbered in the JSON table as every event is, and `jadpo_std::system::
RECEIVED_EVENTS` joins that store's schema. The executor gives them with
the secrets `provider::Credentials` holds (`Executor::webhooks`), and the
HTTP component answers a request at a webhook's path before routing
(`jadpo_runtime::webhooks::answer`), after its address's rate limit:
`webhooks::check` reads the scheme's signature (pure, over the raw body,
every candidate compared in constant time through
`jadpo_value::crypto::signed_by` and `same_secret`), the body is read
with the event's codec on the provider's wire (a `Coder` that runs the
built-in encodings and no package function), `occurred_at` set first,
and one read-committed transaction records the provider's id (`RECEIVE`,
`ON CONFLICT DO NOTHING`) and writes the outbox row and deliveries with
`outbox::write`, the deliveries deferred as an action's are. The tick
deletes ids received 30 days before (`OLD_RECEIVED`, the plan's
`SchedulesPlan::received`). Live tests in `crates/jadpo-cli/tests/webhooks.rs`.

**Webhook replies and handshakes** (chunk 7.7b; area 11, "Chosen in chunk
7.7b"). `SignatureScheme` gains `twilio`, `slack` and `sns`, and
`WebhookPlan` a `WebhookHandshake`; each `WebhookEvent` whose package
event names a `reply:` carries a `WebhookReply`, the answering action's
key (from `webhooks::WebhookModel::replies`, checked against the action's
signature) and the reply type's number in the JSON table. The checker
reads `reply:` as a setting of an `Event`, never a field
(`types::shape::REPLY`). In the runtime, `webhooks::check` stays pure for
every scheme but SNS's, whose `webhooks::sns::check` fetches the signing
certificate through the host's `Outbound` (`Host::outbound`; natively
`jadpo_host_native::fetch::Outward`: HTTPS only, public addresses only,
no redirect) and keeps it per thread for a day, verified with
`jadpo_value::certificate::rsa_signed`. `webhooks::handshake` answers
Graph's validation before the signature, Slack's challenge and SNS's
confirmation after it, and `webhooks::replies::answer` claims the id in
`jadpo_received_events` (its `reply` column), runs the action as the
system and keeps its reply. A test reaches a server on its own machine in
AWS's place through `Outward::reaching`, built only with
`jadpo-host-native`'s `test-outbound` feature, given to the harness with
`Harness::outbound`.

**Files and a file's run** (chunk 6.4b; area 16, "Chosen in chunk
6.4b"; area 12, "How a file's run is kept"). The `files` capability's
first part is on the seam: `jadpo_runtime::files` sends a store's `put`
(a keyed call, its key `jadpo_std::files::key_for` made from the
invocation's randomness) and a ranged `read` through
`effects::call`, and gives the `File` (`files::file_value`, a record the
plan's JSON table writes as `jadpo_std::files::FILE_FIELDS`, kept in a
`jsonb` column as a `BatchRun` is) or `FileContents` (`read_whole`, at
most `WHOLE_READ_LIMIT`); a put's `storage_full` comes back as the call's
condition (`files::Done::Broke`), which the interpreter raises at its
site as a construction's broken rule. The checker reads a `File` field's
settings into `FieldModel::file`, and a store's call is `Callee::Files`.
Drivers: `jadpo_host_native::files::DiskFiles`, which `jadpo dev` gives
every connection under `.jadpo/files/`, and `files::MemoryFiles`, the
fake `jadpo test`'s host and the Rust tests give. A file helper's read
is `BatchOp::Read(RowFormat)`, planned as a `BatchPlan` whose `from` is
`BatchFrom::File` (the row type's JSON-table number and each field's
column) and whose store is `jadpo_check::types::file_run_stores`'s; the
runner's page for it (`batches::file`) reads the run's place in a
statement of its own (`PLACE`), reads the page's rows from the store
through the seam, a window at a time, decodes them (`jadpo_runtime::rows`
for CSV, TSV and JSON Lines; `Codecs::read_cells` and `read_value`), and
only then opens the page's transaction, holds the run to the same place
under its lock, and commits the items' writes, the problems and the next
place (byte, row and line) together. `Host::rows_per_batch` says how many
rows a batch holds: 100, two on `jadpo test`'s host.

**An export's run** (chunk 6.4b2; area 12, "How an export's run is
kept"; area 16, "Chosen in chunk 6.4b2"). A file helper's write is
`BatchOp::Write(RowFormat)`, its store lowered to the connection's name
as a text literal, planned as a `BatchPlan` over its query whose `each`
is `BatchEach::Row` (the inline function's body in the starting action's
body, its parameter's local and each local of the action it reads, by
`Body::outer_reads`, with the place of the value the run saved for it)
and whose `to` is a `FileWrite` (format, store, the row type's
JSON-table number and columns, and the places of `name:` and
`format:`). The runner's page for it (`batches::export`) reads the page
in a transaction that commits having written nothing, works out each
row through `Executor::row` (the interpreter's `Machine::row`, a frame
of the starting action holding only those locals), encodes the page
(`Codecs::write_cells` for CSV and TSV, `Codecs::write` for JSON), puts
it as a piece through `files::put_piece`, then commits the problems and
the next place (cursor, pieces, bytes, rows) in a second transaction
held to the place it read from; once every page is written,
`files::join` joins the pieces and `finished:` gets the `ExportSummary`.

**JSON lists, gzip and estimates** (chunk 6.4b3; area 12, "How a file's
run is kept" and "How a run is kept"; area 16, "Chosen in chunk 6.4b3";
area 19, "How a JSON file's list is read"). `json.read` given `each:` is
`BatchOp::Read(RowFormat::Json)`; its `items_at:` and any read's
`max_unpacked_size:` are written out, lower to text literals, and the
plan puts them in `FileRows` (`items_at`, `max_unpacked`), `items_at:`
in the run's shape too. `jadpo_runtime::rows::json::Walker` walks a
document pure, a window at a time, to the list (`Walked::Opened`), then
item by item and on to the document's end, holding only its frames and
the item in hand; `Opened` keeps the walk that opened the list and each
page starts a copy at its position (`Walker::at_item`), each item read by
`Codecs::read_value` as a JSON Lines line is. A file's text comes through
`batches::file::Source`: its stored bytes, or `Unpacking`, which feeds
`rows::gzip::Unpacker` (RFC 1952 members over miniz_oxide's inflate,
each header and trailer checked) the packed windows and serves the text
up to the limit, again from the start when a position is behind what it
holds. `jadpo_batch_runs.estimated_total` is written by `ADVANCE` (a
`NULL` keeps the last): a query's run's from `read::count`, its first
page's statement under `count(*)` with its limit `NULL`, behind a
savepoint; a file's from its items over how far into its text they end.
`batches.status` gives it, and the items done once finished.
`type.batch_order_updatable` is the checker's, at `from:`'s query
(`Body::batch_order`), declinable.

**What differs on Workers.**

- No threads, no timers between requests, no disk: delivery runs in
  `waitUntil`, and the minute cron calls `schedules::tick`, which runs
  the schedules due and sweeps each store's outbox,
  each within the budget the host gives it (30 seconds after a response,
  15 minutes for the cron; chunk 6.7's `Host::budget_ms`, set in 8.2),
  and what one leaves goes on in a fresh invocation (`Host::hand_on`).
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
migrations, so it starts no PostgreSQL. The live PostgreSQL steps,
twenty-odd processes of `jadpo-cli`'s integration binary, run their
tests on two threads each (`RUST_TEST_THREADS`, from the core count and
the binaries run at once; chunk 5.13), so together they keep about two
per core; every other binary keeps libtest's default, since `compiler
tests` is the longest step and mostly waits on the processes its tests
start: capping it to two threads as well made the full gate 126 to 168 s
against 95 to 96 s. The gate finds what cargo built, and the wasm32
examples, where cargo says they are, so `CARGO_TARGET_DIR` holds.
`--group lint|docs|tests|wasm32` runs one CI job's share, and
`--require-examples` makes the coverage test fail on a feature no
example uses (above).

**Gate timings** (chunk 5.13, 2026-10-19; `CARGO_INCREMENTAL=0
CARGO_PROFILE_DEV_DEBUG=0`). The 14 s measured on 2026-10-15 no longer
holds: the gate has grown to 45 steps. On the ten-core Mac, shared with
two other agents' gates (one-minute load average 25 to 120 at the start
of a run, up to 184 during one), the full gate with nothing to rebuild
reports 86 to 107 s before this chunk (median 100 s over eight runs, one
outlier at 152 s); on the code it merged with, 95 to 111 s without the
cap and 89 and 96 s with it, within the noise of a shared machine. Its
wall-clock time is 12 to 15 s more, about 10 s of which is stopping the
run's PostgreSQL server and removing its folder, after the summary is
printed. From an empty `target/` it took 219 s before (61 s of building,
load 25 to 91) and 316 s after (150 s of building, load up to 175): cold
runs are mostly building, and the building follows the machine's load.
The steps' running threads peak at about 45 at the 90th percentile with
or without the cap (median 13 with it, 15 without), from `jadpo test`'s
workers and clippy as much as from libtest, so the review's load
averages of 248 to 281 were several gates at once. The critical path is
a few long serial tests (`manifest determinism`'s two tests take 21 s
alone and 80 s or more inside the gate) and `compiler tests`, which no
thread setting tried shortened. In CI (four-core runners), the last 30
successful runs of 19 October took a median of 353 s from start to end
(265 to 486 s), the `tests` job 334 s (256 to 464 s), of which the gate
was 280 s (203 to 432 s), and `docs` 162 s; this chunk's branch took 264
s, the `tests` job 256 s and its gate 219 s, one run, so a sign rather
than a measure.

**CI.** GitHub Actions on Linux runs the gate's four groups as parallel
jobs, each with the pinned toolchain and its own cargo cache: `lint`
(formatting, clippy, the name ban, the editor grammar), `docs` (the pages
and the teaching examples), `tests` (every test, with the live PostgreSQL
steps running at once on one PostgreSQL container named by
`JADPO_TEST_DATABASE_URL`) and `wasm32`; `docs` and `tests` each build and
start their own container from `tools/postgres/Dockerfile`, PostgreSQL 16
with pgvector and PostGIS at pinned versions (chunk 5.5v1; area 21,
"Without pgvector"), while the toolchain installs. A local run's
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
