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
| `jadpo-std` | Shared leaf: built-in types, exact numbers, value rules and their validators, the standard types' data entries and storage codec, the standard library's enums Rust interprets, the typed operation table; the plans both executors run, as plain data (type and failure plans, constants, read, write and purge plans, the route, settings, JSON, translation and prompt tables), and the runtime's plan as one value with its canonical byte encoding | yes |
| `jadpo-diag` | Rule catalogue, diagnostic model, edits | |
| `jadpo-syntax` | Lexer, parser, lossless syntax tree, typed AST, `jadpo.ungram` | |
| `jadpo-fmt` | Formatter | |
| `jadpo-sql` | Every PostgreSQL text the compiler writes: conditions and their one lowering (an exact SQL form, or an index search the runtime rechecks), statements, aggregates, identity lookups and table definitions, rendered from what the graph, the checker and the plan decide | |
| `jadpo-graph` | Node and edge model, identity keys, canonical JSON, diff; the physical schema, its snapshots and migration diffs | |
| `jadpo-db` | Incremental database: source inputs, project layout, the lock and the identity registry; the only compiler crate that does I/O (the before side from git comes later) | |
| `jadpo-check` | Resolution, types, failures, effects, policy, data and layout rules, as queries that build the graph | |
| `jadpo-plan` | What both executors need besides control flow: numbered callables and their frame tables, type and failure plans, constants, each read and write site's statements with policy compiled in (written by `jadpo-sql`), codecs per type, and the route, settings, JSON, translation and prompt tables; schedule and subscriber tables come with area 12 | |
| `jadpo-interp` | The interpreter | |
| `jadpo-codegen` | Rust source from graph and plan, one crate per project (§2.5, "Compiled code"); drives cargo | |
| `jadpo-value` | Runtime values, operators, JSON codecs, validation, standard-library implementations | yes |
| `jadpo-runtime` | Everything in §2.6, behind a `Host` trait | yes |
| `jadpo-drivers` | Capability drivers over a host-supplied transport, a cargo feature each: PostgreSQL, the package driver (JSON over HTTP, chunk 7.2c) and SMTP (chunks 7.3 and 8.1b) now; Redis and S3 with their areas | yes |
| `jadpo-host-native` | tokio and hyper: the HTTP listener, timers, the background work a response leaves, passkey ceremonies, the disk driver `jadpo dev` keeps files with (chunk 6.4b), HTTP for the package driver (chunk 7.2c); the inbox with email | |
| `jadpo-host-workers` | wasm-bindgen and Jadpo's own bindings to Workers: `connect()` sockets, `fetch()`, timers, `waitUntil`, bindings (§2.6, "The Workers host"); with `testing`, a test Worker's handler (§2.6, "Route tests on workerd") | yes |
| `jadpo-term` | The CLI design language: rich, plain and JSON output | |
| `jadpo-test` | Test runner, PostgreSQL harness, field-by-field value diffs; the differential comparison with compiled code, natively and inside workerd | |
| `jadpo-lsp` | Language server | |
| `jadpo-cldr` | The CLDR data a deployable carries: ICU4X's compiled data baked again for the app's supported locales (chunk 8.2c) | |
| `jadpo-cli` | The `jadpo` binary: commands, help, completion | |

Built so far: every crate above; `jadpo-codegen` since chunk 8.1f and
`jadpo-host-workers` since chunk 8.2a.

**`jadpo-std`** decides once what both sides need: what a valid value is
(`Rules` and the validators the checker runs on literals and the runtime on
values), what each standard-library operation takes and gives
(`OPERATIONS`), and each standard type's data entry (`standard`, area 19:
its text form, JSON shape, storage and validator), with the storage codec
that binds a value to its column (`standard::storage::bind`). The plans the
runtime runs are its plain data, so compiled code needs nothing from a
compiler crate. Since chunk 8.1a what the runtime reads of a program is
one owned value, `RuntimePlan` (§2.5, "The plan compiled code holds"),
written in `canonical`, the canonical byte encoding: a format of its own
on serde's data model, so a plan type takes part by deriving `Serialize`
and `Deserialize`; numbers in LEB128, as short as they can be (signed
ones zigzagged), text and lists after their lengths, a struct's fields in
order with no names, an enum's variant by its place, a map's entries in
the order of their keys' bytes whatever order the map holds them in, no
floats, and nesting held to 256 levels, so a value has exactly one
encoding and a reader refuses any other. What a plan points at in the
standard library's tables rather than holds (a standard type's name, a
standard enum's codes, an array's inner column type) is written by name
and read back as the table's own (`standard::named`).

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
platform's; a deployable's CLDR data is its supported locales' only
(chunk 8.2c), which write the same text. Operations that read the generator or the clock run only in
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

**`jadpo-term`, `jadpo-test`, `jadpo-lsp`, `jadpo-cldr` and `jadpo-cli`**
are tooling: the CLI design language's screens, the test runner (§2.5,
§2.7), the language server, the locale data `jadpo build` exports for a
deployable (§2.6, "The locale data a deployable carries"), and the binary
that nothing depends on.

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
`native_only` (the native host); `jadpo-drivers` builds with every driver
feature on. `jadpo-host-workers` builds natively too, where its bindings
compile but are never called, and runs under workerd in the gate's
`wasm32` group (§2.6, "The Workers host").
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
subscribers, and the settings), which the interpreter builds from its plan
and compiled code will hold as statics, and the keys as the program
started with them, which the context reaches (`Ctx::started`, chunk 7.9e). A refusal comes
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
readable when the executors differ. One module per domain; each route's
codecs generated from the type plan ("A typed per-route entry" below);
functions as `fn`; actions, fixtures, tests and seeds as
`async fn(&mut Ctx, &Program, …)`;
checked arithmetic that returns a fault, never panics. *Rejected:* string templates, whose malformed output
only rustc catches.

**Keeping rustc time down.** The runtime crates are compiled once per
Jadpo version, target and locale set and cached, so a build compiles
only the thin application crate. Since chunk 8.4g (decided 2026-10-22,
reversible) `jadpo build` compiles every project's deployables into
cargo targets shared by them all, in `jadpo/builds/<version>-<runtime>/`
of the user's cache folder (`$XDG_CACHE_HOME`, else `~/.cache`, where
the package cache is; the project's `build/rust/cache/` with neither),
`<runtime>` a digest of the runtime source's folder, so two copies of
Jadpo of one version never share one. Its `locales/<digest>/` holds the
CLDR data a locale set carries, named by a digest of the exported
files, written under another name and renamed into place, so no build
reads it half written and identical sets share it. Each build verifies the
expected exported files, not only the directory's name: a restored/pruned
cache may retain an empty digest directory. Verification and repair hold a
per-digest filesystem lock (`locales/.<digest>.lock`); incomplete or corrupt
entries are written under a unique staging name and published whole, while
complete entries are untouched. Export and cargo run outside that lock.
This recovers the empty directories left by the pinned CI cache action's
[recursive target cleanup](https://github.com/Swatinem/rust-cache/blob/6323deb102c322ba6fcbdcafc7e3dddab59af2b6/src/cleanup.ts#L9-L33),
without clearing valid build artifacts or changing cache keys. Its
`targets/<host>-<locales>[-<overrides>]/` is a cargo target for each
host (`native`, `wasm`), locale data and set of cargo's
`CARGO_PROFILE_RELEASE_*` overrides (the gate builds its test
deployables without link-time optimisation): ICU4X's data crates read
the data through `ICU4X_DATA_DIR`, whose change cargo rebuilds them and
everything above them for, so two sets in one target would undo each
other's build, as two profiles would. A workspace's own paths reach
rustc relative to it, and the cache, the runtime's source and cargo's
home are remapped alike for every project, so the shared units are the
same for each and a binary holds no machine's path. A build holds the
target's lock (`jadpo-build.lock`) from `cargo update` until it has
copied what it built into its workspace: cargo's own lock keeps two
builds from writing at once, not a second from replacing the first's
binary before it is copied out. Cargo names a path package's unit by
its name and its path within its workspace, so two projects'
deployables of one name build the same units; the target records the
workspace that last built each deployable name
(`jadpo-built-by/<name>`), and a build from another marks its own
files changed first, since cargo, going by the files' times, would
otherwise take the other project's program as up to date. Nothing
prunes the cache yet; deleting it costs a rebuild. The tests build into
one cache, `target/jadpo-compiled/cache/`, so the four Workers
scenarios compile the runtime once, and wasm-bindgen's library and the
gzip that sizes a module are optimised in test builds (the root
`Cargo.toml`). Measured from nothing on the ten-core Mac (chunk 8.4g):
the `wasm32` job's Workers step took 505 s (1,407 CPU-seconds, four
920 MB targets) before and 107 s (294, one 981 MB target) after;
binding and gzipping a kiosk 2.8 s and 1.7 s before, 0.8 s and 0.3 s
after. The `compiled` job's two native builds carry different locales,
so it gains nothing and builds two small greeters more. *Rejected:* one target for every locale set or profile,
for the rebuilds above; a version for each project's packages, which
would keep their units apart without marking files, but would make what
a build gives depend on the folder it is built in, against reproducible
builds (§2.8); and a target in each project, which made every app's
first build a cold release build. Generated code has no generics, because monomorphised
copies drive the LLVM IR rustc must optimise
([compile-time guide](https://nnethercote.github.io/perf-book/compile-times.html)).
Tests run interpreted; compiled builds happen only in the differential
run (dev profile) and for deployment (release). The differential run
compiles every corpus project as a module of one crate: one rustc
invocation for the corpus.

**The plan compiled code holds** (chunk 8.1a, from the Phase 4 and
Phase 6 reviews). Everything the runtime reads of a program besides its
callables' bodies is one owned value, `jadpo_std::RuntimePlan`: the
validator, shape and failure plans, the JSON table, the route table with
its codecs, the settings, subscribers, batches, schedules, purges, the
connections table (chunk 8.1d, §2.6), providers' operations, `ai:`,
prompts, each callable's number by its identity key but a test's
(`RuntimePlan::callables`, chunk 8.1e), and, with the `testing` feature,
its dev and test section (§2.6, "The `testing` feature"). `jadpo-plan`
builds it as a section of its plan (`Plan::runtime`, the type table's
numbered plans moved into it as the plan is finished), and an executor
hands the runtime a view of it, `jadpo_runtime::Plan`, through
`Executor::plan`. Each thread loads it once (`jadpo_runtime::Loaded`):
its validators compiled once for the process and shared behind an `Arc`
(`Loaded::compile`, so a project's patterns are compiled once), its
shapes made per thread, since values never cross threads (§2.6). The
interpreter's `Program` is such a thread, and reads its own validators
and shapes from the same `Loaded`. Compiled code holds the plan as a
constant in its canonical bytes (`RuntimePlan::encode`: a header naming
the Jadpo that wrote it, then the plan in `jadpo_std::canonical`, §2.1),
decodes it once as it starts (`RuntimePlan::decode`) and loads it on each
worker the same way; the site plans its generated code names (each read's, write's
and call's) joined the same plan with the code generator (chunk 8.1f,
"Compiled code" below). The header is `jadpo runtime plan <version>`,
the version of the Jadpo whose compiler wrote it (decided in chunk 8.4h,
2026-10-22): a plan is only ever read by the program it was compiled
into, built from that same Jadpo's runtime crates, so nothing else
guards compatibility, and a chunk that changes the plan's shape bumps
no number by hand.
The same project always gives the same bytes, and the bytes always read
back to an equal plan that writes them again: a test plans every page's
project and every teaching example, and plans each example twice from a
fresh database, so a plan holding anything in hash order would show
(`crates/jadpo-plan/tests/runtime_plan.rs`). *Rejected:* generating the
plan as Rust literals, which multiplies the code rustc compiles for every
application against "Keeping rustc time down", and rebuilding it at start
from the project, which needs the compiler a deployable doesn't carry;
decoding a few kilobytes once per process costs less than either.

**Compiled code** (chunk 8.1f, decided before it was built).
`jadpo-codegen` writes a checked project as one Rust crate from its plan
(`jadpo_plan::Plan`: each callable's typed body and decoded sites, and
the runtime plan), its Rust built with `quote!`, parsed into a
`syn::File` and printed by prettyplease (`jadpo_codegen::generate`), so a
file that doesn't parse is the generator's error and never rustc's. The
crate is its `Cargo.toml`, a library depending on `jadpo-std`,
`jadpo-value` and `jadpo-runtime` only, by a path its caller gives, with a
`testing` feature turning on theirs; `plan.bin` and `plan-testing.bin`,
the runtime plan's bytes from `encode_deployable` and `encode`, held as
`const PLAN: &[u8] = include_bytes!(…)`, the second under `testing`;
`src/lib.rs`, with the frame table (each callable's label, file and line,
and each site's line, start, end and anchor: plain data in a `const`
array, "Frames" below), the callables by number (a `match` giving each
one's function, tests, fixtures and seeds under `testing`) and
`program()`; a module per domain (`domains/<domain>/`), per other top
folder (`shared/`, `tests/`) and `app` for the root's files; and, since
chunk 8.1p, `entries`, each route's typed entry ("A typed per-route
entry" below). Each
callable is a function named by its number and label,
`c3_action_make_thing`: a function, a library function and a constant's
body a plain `fn(&mut Ctx, &Program, Value…) -> Ran`, and an action,
fixture, test or seed an `async fn` of the same shape, every value the
interpreter's `jadpo_value::Value` (why not a struct per type: "A typed
per-route entry" below). A program decodes its plan once per process, on
the first thread that asks, and loads it on each thread as the
interpreter does
(`Loaded::sharing`: validators compiled once, shapes and constants per
thread).

Generated code holds control flow and numbers; everything else goes
through the entry points the interpreter uses (§2.6, "The start-up state
and the runtime's plan"): `Ctx` for frames, faults, failures and an
assertion's parts, `store` for reads and writes, `action` for the
boundary, `effects::run`, `http::route_test`, `call_as` and
`test_steps::after_step`. What every compiled program shares is
`jadpo_runtime::compiled`: `Program`, the compiled `Executor`; the action
body it gives the boundary, its isolation, gate, outbox and record read
from the plan; `invoke`, a call by number, through the boundary for an
action or fixture, its future boxed so a body that calls itself through
an action has a size; `Stop`, what ends a body early (a failure, a fault,
an assertion, a broken rule for an `attempt`, a refusal for a `conflict:`
arm); and a function per kind of site (`binary`, `truth`, `field`,
`given` and `build`, `kept`, `query`, `get`, `write`, `request`,
`setting`, `now`, `failure` and `raised`, `assert_true`,
`assert_compare` and `assertion_end`, `enter` and `left`, `step`; since
chunk 8.1g `construct`, `mapped`, `broke` and `rule_arm` for an
`attempt`, `is_variant` and `items` for a `match` arm and a `for`,
`standard` and `standard_each`, `emit`, `sent_emails` and
`assert_fails`; since chunk 8.1h `caller`, `session`, `batch`, `advance`
and `schedule`; since chunk 8.1i `translate`, `render_prompt`, `escape`,
`files`, `provider`, `provider_maps` and `mapped_code`, `ai`, `package`,
`fake`, `ai_fake` and `ai_requests`), each turning what the runtime or `jadpo-value` gives
into the stop the interpreter's walker raises at the same site, with the
same words. The operators are one implementation both executors call,
`jadpo_runtime::operators::operate`, and so is the standard library:
every standard operation goes through `jadpo_runtime::effects::standard`
(chunk 8.1g), which runs one with effects as `effects::run` does and any
other through `jadpo_value::run` with the plan's locales, an inline
function a closure it calls per item, its values kept from an assertion's
report; the test that holds the interpreter to `effects::run` holds it to
this entry point too (`jadpo_value::run` may not appear in its sources).
So are area 8's operations and area 12's batches (chunk 8.1h):
`jadpo_runtime::http::session` runs `sessions.*`, a token's or a key's
operation, magic links' and a test's passkeys', `http::caller_from` makes
a caller from its record (`Principal.user(user)`), and `batches::call`
starts a run (`batches.each` and a file helper's read or write), reads it
or cancels it, each giving a store's refusal as the failure it raised;
the interpreter's copies are gone, and
`the_interpreter_runs_identity_and_batches_only_through_the_runtime`
keeps them gone. So, since chunk 8.1i, are a body's sites of areas 10,
11, 16 and 21: `jadpo_runtime::calls` has one entry point each, giving a
`Called` (a value, a condition for the `attempt`, a failure or a fault)
that each executor turns into its stop: `translate` (a
`translations.<message>` call, and an email template's wordings),
`render_prompt`, `escape` (a value an email's HTML template writes out),
`files` (a store's `put` and `read`), `provider` (a provider package's
operation, its package's functions run by the executor), `ai`
(`ai.generate`, `ai.stream`, `ai.embed` and `ai.embed_all`, by the site the
plan decodes, `calls::Asked`), and with the `testing` feature `fake` (a
test's `fakes:`), `ai_fake` (its `fakes: { ai: }`, a recording's answer
among them) and `ai_requests`. An `attempt` over a provider's operation or
a package's action maps a failure of the operation's, its package's or
`timed_out` by its code (`calls::provider_maps`, and `calls::mapped`,
which faults on one no arm maps); compiled code writes that arm into the
`attempt`'s `match`, the package action's failures listed from its body,
and calls a package's action through the connection named
(`compiled::package`, `Ctx::enter_package`). The interpreter's copies are
gone, and
`the_interpreter_runs_translations_providers_files_and_models_only_through_the_runtime`
keeps them gone. `clock.advance` is `test_steps::advance_clock`, a step
after which what came due is delivered, and a test's `<schedule>.run()`
its action invoked as the system between `schedules::test_run` and
`test_run_ended`, as the interpreter has it. A file helper's write's row
(chunk 6.4b2), the inline function the runner works out per item, apart
from its action's body, is a function of its own in its action's module
(`row<callable>_<site>`), every local of the action declared and those
the run saved set by number (`batches::row_locals`, which the interpreter
uses too), reached by its callable and site through the crate's `row`
table (`Code::row`), which the compiled `Executor::row` runs in a frame of
its action.
An `attempt`'s arm is found by `jadpo_std::arm_for`, which the checker's
`RuleArm` uses too. None of it is generic, so a generated crate
monomorphises nothing of the runtime's.

What compiled code reads of the plan besides the frame table joins the
runtime plan: `RuntimePlan::sites` (`jadpo_std::compiled::SitesPlan`:
every constant, each query's and entity `get`'s read, each write site's
plan, and each outside call's site, a provider operation's,
`ai.generate`'s, `ai.stream`'s or `ai.embed`'s, numbered in callable and
site order, `Plan::call_number`), `RuntimePlan::actions` (what the
boundary needs of each action and fixture), the build id and the steps
key. The interpreter reads the same plans from the
compiler's `Plan`, built in the same pass. The application's locales
joined the runtime plan when compiled code first called the standard
library (`RuntimePlan::locales`, chunk 8.1g), and the
compiler's `Plan::locales` reads them there; the translations joined it
with their first compiled call (`RuntimePlan::translations`, chunk 8.1i),
and the runtime's view of the plan carries them, the
locales and the outside calls' sites (`Plan::translations`, `locales` and
`calls`, whose first `ai.generate` of a prompt `Plan::generate_site` finds
for a test's scripted answer to it).

A body's sites are numbered as its typed body numbers them (§2.3), and
generated code passes those numbers, as literals, wherever the
interpreter passes its own: `Ctx::enter(callee, site)`, `fault`, `fail`,
`record` and `assertion_failed`, and the store's and the HTTP component's
site. So a fault, a failure or a failed assertion names the same frames,
and a record the same sites, whichever executor ran it. Arithmetic is
`operate` over `jadpo_value::ops`' checked operators: an overflow comes
back as a value and is raised as `int_overflow` or `decimal_overflow` at
the operator's site, never a panic.

A body is straight Rust: a local is a `Value` variable, an expression
that can stop is `?` on a `Ran`, an `if` a Rust `if` whose condition
`truth` reads, and `and` and `or` stop as soon as their answer is known.
An `assert` is a labelled block between `assertion_begin` and
`assertion_end`: a stop in its condition breaks out of the block, so the
recording always ends, and each value its condition works out, but a
literal, is recorded as the interpreter records it. Chunk 8.1g compiled
the rest of areas 1 to 4 the same way: an `attempt`'s construction and an
outcome `match`'s subject other than a call are labelled blocks too,
giving their `Ran` to a Rust `match` that raises the arm's failure (at
the construction's site, its fields worked out only then) or runs the
arm, and passes any other stop on; a call that is a subject has its
arguments' stops leave as anywhere else and only its own outcome caught,
an action through the boundary with `caught` set. The failure an arm
handles is a variable while the arm runs, which `propagate` passes on
and a `reject` inside it keeps as its cause. A value `match` is an `if`
chain over its patterns, a `for` a Rust loop over the items `items`
gives, a construction `construct`, checked only where the checker
couldn't prove its rules, and `assert … fails` `assert_fails` on what its
subject gave. An inline function is a closure `standard_each` calls per
item, setting its parameters, which are the callable's locals, and
working its body out with `?` going out of the closure, never to a label
outside it; a site that waits there is the interpreter's defect, which
the checker rules out. A `return` there (the checker allows one in a
`match` arm's block) or inside a labelled block leaves as a stop,
`Stop::Returned`, ending what it passes through as any stop does, and
the callable's end (`left`) gives its value, as the interpreter's
`return` unwinds to its frame's end. A call to a function
enters its frame, runs it and leaves the frame whatever it gave (`left`
turns a broken rule no `attempt` caught into the interpreter's defect at
the callee's entry); a call that pauses goes through `invoke`. In a test,
fixture or seed each call runs as the caller it passes, and after each
step of a test what it emitted is delivered (`step`), as in the
interpreter. A function's recursion runs on the thread's stack, to the
runtime's depth limit. Work that never waits looks at the request's
deadline every 4,096 calls, the interpreter's interval in tasks: an
`async` body gives the thread back there when a request is waiting on
one, and a function, which can't, stops with `deadline_exceeded` once it
has passed.

What isn't compiled is refused by name: `generate` gives every
`Refusal` (the form, `` `for` ``, `` `list.count` ``, `` `Type(value)` ``,
and where it is written) rather than a crate that faults where a test
happens to reach it, so the generator run over the corpus showed what
8.1g to 8.1i had left (`crates/jadpo-codegen/tests/corpus.rs`, which
also holds every crate it writes to parsing as Rust, and since chunk
8.1i that it refuses none; what it still refuses, a site that waits in a
function, is a body the checker refuses). Built in 8.1f:
`var`, assignment, `return`, `if`, an expression's statement and
`assert`; literals, locals, field reads, calls of functions, actions and
fixtures, queries and `get`, object and variant builds, lists, every
operator, `not` and `-`, `create`, `update` and `delete` with their
`conflict:` arms, a route test's `request`, constants, `config.<name>` and
`clock.now`. Of the corpus's 205 projects that check without
migrations, 65 compiled. Built in 8.1g: `for`, `assert … fails`, `emit`;
value and outcome `match`, `attempt` over a construction, `reject`,
`propagate`, `Type(value)`, every standard operation, inline functions
and `email.sent`. 183 of the 205 compile. Built in 8.1h: a caller made
from its record (`Principal.user`, `Principal.service`), `sessions.*`,
tokens and keys, magic links' and a test's passkeys' operations,
`batches.each`, `batches.status`, `batches.cancel`, a file helper's read
and write with an export's rows, `clock.advance` and a test's
`<schedule>.run()`. 190 of the 206 compile. Built in 8.1i:
`translations.<message>`, provider operations with their failures mapped
by code, a package's action, a test's `fakes:` and `fakes: { ai: }`,
`ai.<prompt>.requests`, and in projects that check only with their
migrations a store's `put` and `read`, `prompts.render`, `ai.generate`,
`ai.stream`, `ai.embed` and `ai.embed_all`, and an email template's
`{{ value }}`. All 206 compile, and every teaching example.

The differential harness is `jadpo_test::compiled`:
`differential_all(projects, folder)` compiles each project into a
workspace kept at `folder`, all of them modules of one crate
(`compiled/`, named `jadpo-compiled`; chunk 8.1j): `jadpo_codegen::module`
writes a project as `src/p<n>/`, by its place, its root `mod.rs` with its
plan's bytes beside it, and `jadpo_codegen::holding` the crate's manifest
and `src/lib.rs`, declaring each module and listing each `program` in
`PROGRAMS`; generated code names its callables from its project's root
(`super::<module>::<function>`), so a project reads the same as a crate
of its own or as a module. Beside it is `harness/`, one binary depending
on that crate and on `jadpo-test`, whose `main` hands `PROGRAMS` to
`compiled::harness`, with the repository's `Cargo.lock` and
`[profile.*]` tables copied in. It builds the harness with `cargo build
--offline` into a target folder of its own there, once, so the whole
corpus is one rustc invocation in the dev profile, and runs it on each
project in turn, a few at a time (`differential` does it for one). The
harness
(`compiled::harness`, given the program's place and the project's
folder) runs every test with records, seed 1, on the interpreter and
then on that compiled program (`jadpo_test::run_with` runs tests on any
executor), in one process and on one PostgreSQL server, and compares the
two runs (`jadpo_test::differential`), printing how many it compared and
how many passed, or the first line that differs with both sides; a
project with no tests compares none, its program still made. Since chunk
8.1h each batch run a test started is run to its end once the test has
ended, at the test's clock with counted random bytes
(`jadpo_test::Options::batches`, `test_run::AtClock`), its pages,
its `each:` and `finished:` calls and an export's rows kept in the test's
record, so compiled code's runs are compared too; `jadpo test` leaves a
run as it started until area 13's question 18 is built. Its files
are written only when they change, a module of a project no longer
given is removed, and the folder is kept
(`target/jadpo-compiled/corpus/` in the gate), so the runtime crates and
`jadpo-test` are compiled once, and a later run with nothing changed
compiles nothing and one with any change compiles the corpus's crate
again and links the harness once (chunk 8.1g; 8.1f built a harness per
project). The gate's compiled step
(`crates/jadpo-cli/tests/compiled.rs`) runs every project of the corpus
that checks, the pages' and the teaching examples' (since chunk 8.1h each
example copied whole, with its migrations, prompts, templates, recordings
and lock, so all 28 reach it), and since chunk 8.1i requires every one
to compile and give the same records both ways, and every teaching
example's tests to pass: 258 projects and 525 tests, all 28 teaching
examples with their 334 tests passing both ways, a project of its own
whose `return` inside an inline function leaves its function, and area
19's export with a test that starts it, run to its end. A project whose
migrations need an extension the server lacks or has too old a version
of (`jadpo_test::compiled::Problem::Unserved`) is skipped on a
developer's machine, saying so, and fails under CI, whose image has them
all (`jadpo_test::skip_for_version`): example 22's pgvector 0.8 on a
server with 0.7. The corpus's crate is 177,642 lines of Rust in 258
modules; rustc takes 10 s over it on the ten-core Mac (9.3 s of it the
front end, which is one thread), and a cold build of the harness 42 s,
the corpus's crate starting as soon as the runtime crates' metadata is
ready and ending before `jadpo-check`, the longest of the compiler
crates `jadpo-test` needs (one-minute load average 36 to 39). The whole
step takes 69 s with that cold build and 44 s with nothing to build, of
which running the 258 harnesses is about 22 s, the same with four
processes at once as with ten (chunk 8.1j). The gate runs it as a group
of its own, `compiled`, which CI runs as a job of its own (§2.7, "The
differential run").
*Rejected:* a crate per project (chunks 8.1f to 8.1i), which starts
rustc 258 times, each loading the runtime crates' metadata again, and
the harness's `main` in the corpus's crate, which would wait for all of
`jadpo-test` before rustc starts on the corpus, rather than building it
beside the compiler crates.

The first route is area 7's `todos` project: `POST /todos` creates a
record through an action and `GET /todos/{todo_id}` reads it through a
query, with three route tests on the page (created, read back by its id,
not found). They pass compiled and interpreted with the same records
(`crates/jadpo-cli/tests/compiled.rs`); a value the compiled action
writes, changed by hand, shows as the first row that differs.
*Rejected:* a runtime fault for a form not compiled (`not_built_yet`), so
a partly compiled project runs, which hides what is left until a test
reaches it; generated code doing what the runtime's site functions do at
each site, which multiplies what rustc compiles and gives two copies to
keep in step; the generated crate as a member of the repository's
workspace, which needs the root manifest changed for each project, or
built into the workspace's own target folder, where it waits on the
gate's clippy and wasm32 builds for cargo's lock; and loading compiled
code into the test's own process, which a separately compiled crate
can't share values with.

**Native accept and one rate-limit count** (chunk 8.1l, from the Phase 4
review). A native server is an acceptor and one worker thread per core,
each worker a current-thread tokio runtime with its own handler, program,
stores and connections (§2.6, "Single-threaded by design"). The acceptor
is a thread of its own whose current-thread runtime awaits the listener,
so a connection is taken as it arrives rather than at the next poll, and
it hands each one, with its peer's address and its place under
`http::MAX_CONNECTIONS`, to the next worker in turn over that worker's
channel. A worker whose thread has ended is passed over, and with none
left the connection is closed. A connection, and every request on it,
stays on the worker it was handed to. Until now it went to the worker its
peer's address hashed to, which put every connection from one proxy or
balancer on one worker, and counted an address on one thread only because
of that. *Rejected:* each worker accepting from the listener itself
(`SO_REUSEPORT`), since Linux shares connections out by a hash of their
addresses and macOS not at all, so neither spreads one balancer's
connections; and a worker chosen by the fewest open connections, which
reads every worker's count on each accept to correct an imbalance the
deadline checks already bound (§2.6).

The workers share the rate-limit table and nothing else: one
`jadpo_runtime::RateLimits` per server, which `http::WorkerThread::host`
gives each worker's `NativeHost`, so a caller or an address has one
allowance a window whichever workers its requests reach. It is plain data
(keys, window starts, counts), behind one lock per shard, held only to
find and bump one counter and never across an await; values, futures,
stores and connections still never cross threads, and it is the one lock
on the request path. It is bounded as before, now for the server rather
than each worker: `RATE_LIMIT_MEMORY` (16 MB, area 16) in all, split into
16 shards with an equal share, a key's shard chosen by a hash with keys
random per process, so no client can aim its keys at one shard to push
another's counter out sooner than any other. When a shard is full, the
counter whose window started longest ago goes first, which is one whose
window has ended whenever there is one, so a client still being counted
isn't given a fresh allowance while a stale counter is kept: a key whose
window starts again joins the back of its shard's order, and the order is
compacted once it holds twice the shard's counters, so it is bounded too.
Workers read the clock apart, so a time up to a window before a counter's
start counts in that window rather than starting another; one further back
means the clock went back, and the window starts again. *Rejected:* a
thread of its own that every worker sends each request's key to, which
adds a round trip between threads to every request, and a fixed table of
atomic counts indexed by a hash, which lets two clients share a count.

Under overload, past `MAX_CONNECTIONS` (1,024) open across the workers a
connection is closed as it is accepted, as before; an accept that fails
for want of descriptors or memory waits 100 ms, the connection left in the
system's queue, and one the client abandoned is skipped at once; each
worker's channel holds no more than the cap. On `Server::stop` the
acceptor takes what the system has already accepted for it, hands it out
and closes the listener, so nothing the system accepted is dropped unread.
Each worker then shuts its connections down as hyper's graceful shutdown
does: one kept open with no request in it closes at once, and one
mid-request finishes its answer and closes. One that has sent nothing yet
is given a second for its first request (`http::FIRST_REQUEST`), which is
answered before it closes, since hyper would close it at once and cut off
a request on its way; one opened ahead of need closes after that second.
It waits for them at
most `DRAIN_GRACE` (10 seconds), then drains its handler's background
work for at most `DRAIN_GRACE` again (chunk 6.7); what is still open after
that closes as the thread ends.

**The route trie** (chunk 8.1k, from the Phase 4 review). The HTTP
component finds a request's path in a trie, `jadpo_runtime::http::Routes`,
built once as a plan is loaded (`Loaded`, beside its validators and
shapes) from the route table, health's two paths and the program's
webhooks, and handed to the component on `App::routes`. A node's
children are its fixed segments in byte order, searched by halving, and
one placeholder. A path is walked fixed child first and every match
visited, so the first route found for the method is the one with fixed
text earliest, as the table's search chose it, and each route the path
matches adds its method to `Allow`; 404, 405 and `OPTIONS` are as before.
Health's paths and each webhook's are entries of the trie
(`Found::Health`, `Found::Webhook`) on paths of fixed segments only, which
the walk reaches first, so they still answer whatever the method and
ahead of a placeholder route there, and a webhook is still counted
against its address before anything in it is read; the component no
longer matches webhooks in front of the table. A request's routing
allocates nothing: segments are slices of the path, compared
percent-decoded byte by byte and checked as UTF-8 a few bytes at a time,
a placeholder decoded only when its route reads it; each route's,
webhook's and health path's label (`POST /todos`) is made once and a
response shares it (`Response::route` an `Arc<str>`); and a page cursor's
failure is recognised by its key rather than by making one. A dedicated
test binary counts allocations with a counting global allocator
(`crates/jadpo-runtime/tests/route_allocations/main.rs`, through `stats_alloc`,
since the workspace forbids `unsafe`), and a unit test checks the trie
against the table's search on every method and a spread of paths.
*Rejected:* a `Run::Webhook` in the route table itself, which would put
the runtime's own receiving into the plan's routes, the API listing and
`jadpo dev`'s count and into code generation's per-route entry points,
when a webhook's plan already lives with its provider and its secrets
with what the program started with; and a map from whole paths, which
can't hold placeholders.

**A typed per-route entry** (chunk 8.1p, decided before it was built,
from the Phase 4 review). Compiled code reads each route's request and
writes its answer with code generated from the route's types' plans,
rather than walking the codec table as the interpreter does
(`jadpo_value::json::Codecs::read` and `write`, "the codec walk"). A
project's crate holds `entries.rs`: a reader, `r<codec>`, for each codec
a route reads (a placeholder's, its input's, and each codec they reach
through fields, items, payloads and `T?`) and a writer, `w<codec>`, for
each codec it writes (its output's, the same way), each written from the
codec, its shape and its validator's number; and each route's entry,
`jadpo_runtime::http::Entry`: a reader per placeholder, the input's
reader and the output's writer. The crate gives the entries by route
number (`Code::routes`), compiled code's `Executor::app` hands them to
the HTTP component (`App::entries`), and the interpreter's app, the
trait's default, has none, so the interpreter is unchanged and stays the
reference. `Serving::handle` answers with the executor's `app()`, so a
native or Workers deployable reads and writes through the entries as its
compiled route tests do (chunk 8.4c; before it, a deployable served
with its plan's app and the codec walk).

Everything around the codecs stays the runtime's, in one function both
executors go through, `http::read_request`: a placeholder's text
percent-decoded, a query string or a body the route doesn't read refused
as an unknown field, a body refused when it isn't sent as JSON or is
empty, the header fields put in place of any member of their names, and
what was found answered `invalid_input`. So a request is refused for the
same reasons, in the same order, with the same body, whichever executor
reads it; the arguments made from what was read, idempotency, the
boundary, the envelope and the status are as they were.

*Reading.* A body is read in place: `jadpo_value::json::Doc` parses it
once into a flat list of nodes over its bytes, a string or a number a span
of the text and a string decoded only when it holds an escape. It accepts
exactly what `json::parse` accepts (RFC 8259 as `serde_json` reads it, no
member named twice, looked for by a set past 16 members so no body is read
in quadratic time, 128 levels deep, a number only when it is finite),
which a seeded comparison of the two over generated documents holds it to.
A generated reader walks it in the codec walk's order and calls the codec
walk's own rules for everything but the order: whether a field is missing,
`null` or read (`json::presence`), a scalar read by `json::scalar`, the
one reader of scalars both call, and checked by the program's validator
by number (`Codecs::check`), a list's or a set's items
(`json::collected`), the members an object doesn't declare, in name
order, and the value built by the type's shape (`Shape::build_slots`,
which `Shape::build` calls too). Members are found by their names as
literals, a plain variant by its wire name and a payload variant by its
`tag`. A problem's place is a chain of borrowed steps on the stack
(`json::reading::At`), written out only when a problem is reported, so a
request with none makes no path at all. What a reader builds is the
interpreter's `Value`, so an action is given the same arguments.

*Writing.* A writer writes the `Value` the action or query gave: an
object's members in declared order, their names as literals, an `Input`'s
field left out and a standard problem's `none` member left out as the
codec walk leaves them; a variant by its wire name, a payload variant with
its `tag`; a list's items; a scalar by `json::write_scalar`, the codec
walk's. Anything but the record of its own type with one value per field,
the variant or the list its type expects is handed to the codec walk, so
a writer never has to agree with it about a value the program didn't
give.

*Where the boundary to `Value` sits.* Bodies stay over `Value` (8.1f), and
so do the store, the boundary, effects and the standard library, so a
reader's result is the `Value` an action is called with and a writer
reads the `Value` it returns. A codec the generator doesn't walk itself is
read and written by the codec walk at that place, from a `serde_json`
value made of that part of the document alone, with its path and the
document's numbers: a map, a standard type sent as its parts (`Money`,
`ZonedTime`), a geometry, a feature, a vector's float, a page, a standard
enum read ignoring case (`Currency`, `Locale`), a `Jwk`, and a placeholder
of a type that is neither a scalar nor a plain enum.

*No Rust struct per type yet.* A struct read from a request would be
turned into a `Value` at once, since everything that takes it is over
`Value`: a conversion added, and nothing reading the struct. A struct
pays once bodies are typed (locals and parameters of a type's struct, a
field read a Rust field read, a `Value` made only at the runtime's entry
points), which is a second representation of every value to keep in step
through every site function; that is listed among 9.2's optimisations, to
be built if profiling shows a body's own work matters beside the
request's edge and the store.

*Kept in step.* The differential run sends every compiled route test's
request through the entries. Beside it, the harness reads a seeded set of
requests for every route of every project with the entries and with the
codec walk (`Executor::app`, called as any executor's, which a route
the program answers without its entry fails, and `Program::walked_app`),
and writes a set of
values both ways (`jadpo_test::entries`): each value the input's type
holds, written by the codec walk, then each place in the first two
removed, given again, or set to other kinds and edge values; members the
type doesn't declare; bodies that aren't JSON or an object; other content
types; query strings with keys given twice; header fields left in the
body; and each placeholder given its type's text and other text. The
first difference fails the compiled step with the request and both
results: 5,832 requests and values across the corpus on this machine
(example 22 left out for its pgvector, which CI has).

*The win*, measured on example 23's `POST /repairs` (a `RepairRequest` of
an enum and a text with rules, answered with a `RepairTicket` of a UUID,
the enum, an optional UUID and an optional instant), release build, per
request: reading it took 26 allocations (2,148 bytes) and 1.35 µs by the
codec walk, and 8 (716 bytes) and 0.57 µs by its entry; writing the answer
took 1.05 µs by the codec walk and 0.89 µs by its entry, with the same 17
allocations, each scalar's canonical text and the body's growth, which
both share (medians of three runs of 200,000, on the ten-core Mac under
other agents' load, one-minute load average 40 to 110). The corpus's
crate grew by 8,528 lines of entries, 4.6% of its 186,686.

*Rejected:* a Rust struct per type now (above); one walk over a document
read in place for both executors, which would speed `jadpo dev` too but
change the reference the differential compares against in the same step,
and still looks each field's codec, shape and member name up as it reads
(`Shape::json_name` and `standard_type` are a hash lookup each);
generating each scalar's reading and rules as well, a second copy of
every type's rules; and a reader per route rather than per codec, which
writes a type's walk again for each route that reads it.

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
provider calls) and the final database diff; any difference fails the
gate and prints both sides. The record holds no path numbers, and no
random well-typed bodies are compared as Cranelift compares its own
against its interpreter; neither is on the roadmap yet (checked in chunk
8.4h, 2026-10-22).

Built in chunk 8.1c: the record each run is compared by,
`jadpo_runtime::record::Record`, written by the runtime whichever
executor runs the test, in the order things happened. A test's stores are
wrapped (`record::Recorded`) so every statement is kept with its SQL
text, its parameters' types (never their values) and the rows it read or
changed, or its refusal's classification, with each transaction's end;
an outbox row written keeps each event's type and payload; the capability
seam keeps each outside call's connection, operation and answer through
`Host::record`, which only a test's `Faked` host answers, so a deployable
pays one `None` per outside call and nothing per statement; and the
runner adds the outcome (`record::outcome`: a value, a failure's code,
fields and causes, a fault's classification and site, a failed
assertion's site, comparison and parts). With `Options::record`, `jadpo
test` keeps each test's `TestRecord` in canonical form: those lines, how
the test ended, then every row its database holds when it ends (each
table but the migrations' and extensions', in name order, rows in text
order, times in UTC, a `bytea` by its length), each identity written
`id#1`, `id#2`… in the order it first appears.
`jadpo_test::differential` compares two runs test by test and names the
first line that differs, with the line before it and both sides. Only the
test's seed and clock decide a record, so the corpus run twice gives the
same records (`crates/jadpo-test/tests/records.rs`); the test
authenticator's passkeys are drawn from its device number for it
(`TestAuthenticator::seeded`). Since chunk 8.1f
`jadpo_test::compiled` runs a project's tests both ways and compares
their records this way (above, "Compiled code").

Since chunk 8.2e a test's run is one function whichever executor runs it
and on whichever host, `jadpo_runtime::test_run::once` (the `testing`
feature): the test's seed from its key and its clock from its `clock:`,
every provider connection answered by the package fake and the `ai:`
connection by the `ai` fake, each outside call kept in the record, its
settings, and for a test with a database the delivery between its steps
and its crash points; then its outcome kept, and the batch runs it
started run to their end at its clock (`test_run::AtClock`). What a
test's host gives besides its stores (`test_run::Caught`: every store of
files in memory, the email connection's mail caught), the rows its
database holds when it ends (`test_run::rows`), and its seed and delivery
order moved there with it. The runner keeps choosing the tests, making
their databases, crashing a run at each step it reached
(`run::crash_runs`) and judging it (`outcome::Ending` and `fake_rule`),
so a run made elsewhere is judged as one made here: `jadpo_test::workerd`
runs a project's route tests that way inside workerd (§2.6, "Route tests
on workerd").

Since chunk 8.4a the record holds what the review of Phase 8 found it
left out. A failure's outcome names the site it was raised at, and each
cause its own; a fault's names its site and its stack, innermost first, a
run of one site written once with how many (`main.jadpo:8  function down
×999`). Each read's bound values (a `SELECT`'s; a write's show in the
rows the test leaves) and each outside call's input are kept as a keyed
digest, `#` and sixteen hex digits of an HMAC-SHA-256 under the
record's key, each identity in the value numbered in the order the record
first met one, so a value is compared without being kept: the key is the
process's own (`jadpo_test::differential::digest_key`), drawn at random
and never shown, and handed to a test Worker with each run it asks for,
so a guessable password in a difference's printout says nothing. A
recording's test keeps the value the package's decoders read it as, so
compiled decoders are compared on what they decode. Compiled code catches
a package action's failure as the interpreter's `Task::Catch` does, in a
savepoint (`jadpo-codegen`'s outcome subject), and counts each loop turn
and inline-function call toward the deadline's next look as the
interpreter counts tasks (`compiled::pace`), an action's loop giving the
thread back. Both executors count the inline functions running at once
in `effects::standard`, past 400 the `recursion_limit` fault (area 3,
"Recursion"). Proved by corpus projects in the differential run (a
package's action caught after a read; a recursion to each limit and one
past), records perturbed one change at a time (`tests/records.rs`), and
the probes of `crates/jadpo-cli/tests/support/probes.rs` on both targets'
own stacks.

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
Since chunk 8.1n its connections use TLS as their connection says
("PostgreSQL over TLS", below).
`jadpo-host-native` gives `Runtime` (one current-thread tokio runtime
with a `LocalSet`), `NativeTransport` over tokio's TCP and Unix sockets,
and `postgres(url)`.

**TLS through the transport** (chunk 8.1b). A host secures a driver's
connection and the driver keeps its protocol: `Transport::upgrade` turns
a stream the transport opened into TLS, as its client, checking the
server's certificate and that it was issued for the name given, at the
point the protocol says (SMTP's `STARTTLS`, PostgreSQL's answer to its
SSL request in 8.1n), or before anything is written for a protocol that
speaks TLS from the start. A check that fails is `InvalidData`, a name
that can't be checked `InvalidInput`, a host without TLS `Unsupported`
(the trait's default); anything else is the connection's. The transport
also gives the driver its timer (`sleep`, which `jadpo_drivers::within`
bounds a wait with), the clock (`now_ms`) and the local address of a
stream it opened (`local_address`, reading `Stream`'s `Any`). The SMTP
conversation (`EHLO`, `STARTTLS`, `AUTH PLAIN`, the message, area 16's
rules) is `jadpo_drivers::smtp::relay` over it, every line read a byte
at a time so nothing sent after `STARTTLS`'s reply is read as the
conversation; a TLS check that fails is `misconfigured`, a handshake the
connection broke off `unavailable`. Natively it is rustls
(`jadpo_host_native::tls`), the one TLS stack on the request path, the
package driver's HTTPS and the runtime's own outward fetches included;
OpenSSL stays only for `webauthn-rs` and the test authenticator, and
`jadpo`'s own client for HQ. Its cryptography is `ring`, built with only
a C compiler; aws-lc-rs would add the post-quantum key exchange for a
far larger C build, and is one provider value away. Servers are checked
against Mozilla's roots compiled in (`webpki-roots`), not the platform's
verifier: a deployable trusts the same servers on every machine,
container images without a CA bundle included, and a test can't pass on
what a developer's keychain trusts; new roots come with a rebuild.
`tls::trusting` builds a configuration trusting one authority, what a
test's server on this machine is checked with. Since chunk 8.1n the
upgrade takes whom it trusts (`Trust`): the host's roots, or only the
authorities a database connection names (`Trust::Authorities`, natively
`tls::trusting_all`).

**PostgreSQL over TLS** (chunk 8.1n; area 5, "PostgreSQL over TLS"). How
a store's connections are secured is its connection's in `app.jadpo`,
carried in the plan as `ConnectionDriver::Postgres { url, tls }`
(`jadpo_std::connections::DatabaseTls`: `Roots`, `Authority` with its
`Certificate` setting's number, or `Off` for `tls: false`), which the
factory builds into the driver's `jadpo_drivers::postgres::Security`
(`Postgres::secured`; `Postgres::new` is `Security::Roots`). For each
endpoint the driver decides: a Unix socket, and with `Roots` a host that
is this machine (`jadpo_std::database_tls::is_this_machine`), plain;
otherwise PostgreSQL's SSL request (eight bytes, then one byte read, so
nothing past the answer is read as anything but TLS) and, on `S`, the
transport's upgrade for the host's name, within ten seconds; then
tokio-postgres's startup over the secured stream with its own TLS off
(`SslMode::Disable`), so the one TLS stack stays the host's and the driver
needs no `TlsConnect`. A server elsewhere answering anything but `S`, and
an upgrade that fails its check (`InvalidData`, `InvalidInput`,
`Unsupported`), are `StoreError::Fault` naming the server, since trying
again changes nothing; a connection that broke off is `Unavailable`. An
address with `sslmode` or another `ssl…` setting
(`jadpo_std::database_tls::tls_parameter`) is refused by the driver, by
`PostgresUrl`'s check (`configuration.value_invalid` with
`config::TLS_RULE`, as the app starts and in `jadpo config check`) and by
`jadpo migrate`. The CLI's commands that open a database (`jadpo
migrate`, `schedules`, `runs`, `deliveries`, `ai usage`, `jadpo dev`)
open it as its connection says (`jadpo_cli`'s `database::Address`),
reading the authority's setting as they read the address. SCRAM's channel
binding is left off, as tokio-postgres never sees the TLS; the
certificate check is what binds the session to the server. Proved by the
driver against a scripted server (the request sent or not, the name and
trust upgraded with, each refusal) and live against servers the harness
starts on `127.0.0.1` (`Server::start_on_tcp`), one with `ssl = on` and a
certificate an authority the test makes signed
(`crates/jadpo-test/tests/postgres_tls.rs`): a plain server elsewhere
refused and reached with `tls: false`, this machine reached without TLS,
the private authority trusted (`pg_stat_ssl` says so) and Mozilla's roots,
another authority and another name refused.

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
less `after:` (strict `<`, including literal and configured retention
boundaries since chunk 9.1d2a), oldest first, leaving out with `NOT EXISTS` any a
`restricted` reference holds, directly or through a cascade, until a batch
comes back short (since chunk 3.10 through every chain of cascades, and,
when one leads back to an entity already on it, such as a folder inside a
folder, through a recursive query that follows the cascades as deep as
the data goes); it returns each batch's count (`Purged`). Since chunk 3.12
a run stops after `batches` batches (`PURGE_BATCHES`, 20, for the
schedule) and says whether it `finished`, so a Workers cron run can't
overrun however much is due, and the next run carries on; the budget is a
batch count. Area 12's
hourly purge schedule calls it since chunk 6.2c, with the plan's purge
and its period as the program started with it (`Plan::purge`,
from `Started` since chunk 7.9e); a live test calls it
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
a placeholder, found in the route trie since chunk 8.1k, §2.5; 404 `route_not_found`, 405 `method_not_allowed` with
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
hands each connection to a worker, and one current-thread tokio runtime
per worker, each with its own handler and stores; hyper reads and writes
HTTP/1.1. Until chunk 8.1l the worker was chosen by the client's address
and kept its own rate-limit counters, so an address was counted on one
thread; since then the acceptor awaits the listener, hands connections
out in turn, and the workers share one table (§2.5, "Native accept and
one rate-limit count"). `NativeHost` gives each store by name,
the operating system's randomness and tokio's timer. Since chunk 6.9
what a host does with a request and a tick is the runtime's
(`jadpo_runtime::serve::Serving`, over any executor, whose plan gives
the route table and codecs, `Plan::app`; an `Application` trait gave them
until chunk 8.1a): it answers with `serve_request`,
hands what the response left to `runner::after_response`, runs the tick,
and tells an `Observer` of each request and tick. The native server's
handler that serves an application is `jadpo_host_native::http::AppHandler`,
`Serving` on a `NativeHost` with that host's deferred work, tracked ticks
and drain; Workers' fetch event calls the same `Serving`, one a request
on that invocation's host sharing the isolate's `http::Worker`
(`Serving::sharing`; chunk 8.2d), and its scheduled event ticks
through it (chunk 8.2f, "The cron on Workers"). `jadpo dev` runs the interpreter on it, swapping in each
save that checks cleanly, and only observes it to print its lines. Since chunk
6.7 a stopped server drains (`Server::stop`): it accepts nothing more,
its timer starts no more ticks, and each worker waits, at most
`DRAIN_GRACE` (10 seconds), for its handler's background work
(`Handler::drain`, `NativeHost::drain`, which counts what it runs with
`NativeHost::track`) to stop and give back what it holds, its budget
ended through `Host::stopping`; since chunk 8.1l it first waits as long
for its connections to give the answers they owe (§2.5). `stop_signals`
turns SIGTERM and SIGINT into that stop, a second ending the process at
once; `jadpo dev` stops on them.

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

Built in chunk 9.1d1: provisioned external JWT subjects (area 8).
`Identity::external_token` reads the issuer's `Url` setting and audience's
`Text` setting into `ExternalTokenPlan`, together with the subject's JSON
codec, SQL parameter type and current-user lookup. Deployable pruning
keeps these settings only where a served API accepts the method. The
runtime's `AuthSettings` owns one resolver cache per application startup;
`http::external_token` obtains discovery from the configured issuer path,
requires its exact issuer identity, and obtains keys only from that
trusted document's HTTPS `jwks_uri`. It uses area 19's duplicate-refusing
JSON parser, strict public `Jwk` import and shared JWT verification
(EdDSA, RS256, ES256). Explicit encryption entries are skipped; invalid
signing entries refuse the provider configuration. Only the verified
subject reaches the user's typed field decoder and parameterized
current-user query; every request reads that authority. It never imports
provider profile fields or permission claims.

Richard's 2026-10-22 network decision requires public HTTPS endpoints,
including cross-authority keys named by exact trusted metadata, with no
redirects. Native `Outward` filters DNS answers and connects only to
public addresses with normal TLS verification. Workers' Rust outbound
code rejects private literal addresses and local names; its platform
network filters DNS answers ([workerd Network configuration](https://github.com/cloudflare/workerd/blob/main/src/workerd/server/workerd.capnp)).
Generated Wrangler configuration enables
[`global_fetch_strictly_public`](https://developers.cloudflare.com/workers/configuration/compatibility-flags/#global-fetch-strictly-public)
where the method is reached, avoiding same-zone origin routing.
The test harness maps exactly two public fixture authorities to a local
HTTPS server through transports trusted by that test's generated CA;
its explicitly private/local test network is separate from production
public-network enforcement. No production TLS or address guard is weakened.

Compiler-owned bounds are 64 KiB discovery, 256 KiB JWKS, 64 keys,
3 seconds per fetch, a 5-minute key cache and one refresh per 30 seconds.
A generation-owned refresh lease reserves cooldown before awaiting and
survives cancellation without clearing a newer owner. Typed retained
failures preserve configuration (500) versus provider availability (503)
classification during cooldown; shared strict JWT structural preflight rejects
unsupported algorithms and malformed/duplicate JSON before provider I/O; known-invalid claims remain 401 without
refreshing valid cached keys. Outer HTTP deadlines drop native fetches
and abort Workers fetches through an `AbortController` drop guard.
Generated API/OpenAPI metadata, redacted failure wording and the network
inventory expose the method's runtime-owned egress. The external
first-sign-in hook and multi-column/structured subject fields remain
compile-time `toolchain.not_built_yet` gates;
without it an unknown subject is 403 `sign_in_refused`. Scope declarations
remain unbuilt; area 9 says an external method narrows only when it
declares the scopes it reads. This slice builds neither provider login
nor the golden application or its comparison baseline.

Built in chunk 5.1b: signed sessions (area 8). The identity reads
`revocation_delay:`, each method's `signing_key:` (a `SigningKey`
setting) and a browser session's `origin:` (a `Url` setting) into the
`IdentityPlan` by setting name, and each lookup returns when its session
or key ends. `http::signed` is the runtime's host-agnostic signing:
`Settings::read` derives each method's key (BLAKE3 `derive_key`) from the
values the application started with, current and `_PREVIOUS`, held by the
program's `Started` (chunk 7.9e; `App::auth` until then), which the HTTP
component reaches through the request's context, never in a value code
reads; a signed part is base64url JSON `Claims` and a keyed BLAKE3
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
whose OpenSSL doesn't build for wasm32, so the Workers host checks them
with jadpo-value's pure-Rust ES256 and RSA instead (chunk 8.2l, decided
2026-10-21); `jadpo_host_native::test_authenticator` (feature
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
Workers binding needs): natively `NativeHost` counts in bounded memory,
`RateLimits`, one table for all of a server's workers since chunk 8.1l,
and a test's host counts nothing. On Workers
there is no native default: area 16 keeps counts in memory only where
memory lasts, and its question 6 requires a Redis connection or
Cloudflare's rate-limiting binding there, with the isolate's memory
counting only while that can't be reached, which is the one reading both
give: since chunk 8.2m2 the plan's route table carries the store
`rate_limits:` names (`RateLimitStore`, with a binding for each limit
on Cloudflare's), the Workers host counts in it, and `jadpo check`
refuses a Workers deployable that serves an API without one. Round trips: the sign-in lookup, an idempotency key's claim (one
`INSERT ... ON CONFLICT` that also gives back what the key is kept with)
and its release, the response a failed action keeps, and `SELECT 1` each
run as one statement on their own (`Database::query_once`, autocommit),
and the PostgreSQL driver sends a transaction's `BEGIN` with its first
statement, both written before either answer is read; an idle connection
found closed at that first round trip ran nothing, so it is sent again on
another, as `begin` did, but an autocommitted statement only when none of
its bytes were written, since once it left the server may have committed
it (chunk 8.4d). Each attempt at a server, from the socket to its being
ready, is bounded by the address's `connect_timeout`, or 10 seconds, and
the native transport gives up on a socket that hasn't opened in 10
seconds. A `get` or a query for one record that policy
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
(`ClientAddress::read(plan, values, Target)`), which the program's
`Started` holds and the context reaches (chunk 7.9e).
`serve_request` sets the chosen address on the context (`Ctx::client`) and on
the response (`Response::client`) before anything else runs, and
`by_address` counts that. A host only gives the connection's address
(`Request::address`; natively the peer's), so the Workers host uses the same
code with `Target::Workers`, where the default is `cloudflare`
(`CF-Connecting-IP`) and `read` refuses every other source (`NotAccepted`)
for the deployable to report as a start-up problem, which a Workers
deployable does (`client_address_refused`; chunk 8.2d). With
`hops: N` the client is the N-th address from the right of `X-Forwarded-For`
(every line joined) or of `Forwarded`'s `for` parameters; a header that is
short, invalid or missing gives the connection's address, never a guess, and
addresses are kept in one spelling. Natively the workers share one
rate-limit table since chunk 8.1l, so a client is counted once whichever
workers its connections reach; until then a connection went to the
worker its peer address hashed to, and a client behind several proxies
was counted on several.

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
(`Started::new`, which `Program::with_config` calls; chunk 7.9e), and a
secret's text is held there and nowhere
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
  `waitUntil` (chunk 8.2a). Each piece gets its own context under the
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
give less, and Workers' are 30 seconds after a response and for work
handed on, 15 minutes for the cron, since chunk 8.2a). `budget::Budget` measures it on the
host's monotonic clock and keeps a tenth back, at most 5 seconds, for
giving claims back: `runner::drive` takes a piece only while it isn't
`spent`, and what it doesn't reach stays in the store (a delivery
`pending`, a batch run on its row), handed on by `after_response` to a
fresh invocation (`Host::hand_on`, given a `runner::HandOn`: natively a
task of its own running it, on Workers a request to the Worker itself
naming each piece by its record, chunk 8.2h) when the invocation ran
something, else left for the tick. One helper runs an
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

**Bounded delivery sweeps** (chunk 9.1b1). A sweep still reads at most 100
oldest due deliveries, waking breakers and parking lost/unreadable work first.
It partitions that selection round-robin into four static lanes and drains
them with the same local `race::join_all` schedules use. Every lane calls the
ordinary serial `runner::drive`: each piece gets its own context and current
host time; descendants and a batch run's own batches remain sequential. Four
is a reversible implementation bound, shared by both targets, below Workers'
six connection-opening slots; every actual connection/provider call retains
its host guard. Lanes share the invocation's host and budget, and stop
admission when spent/shutting down while draining admitted work through the
existing claim, body-only timeout, held commit, retry/release and handled-record
paths. Unstarted work remains durable for the tick. Static lanes do not steal
work, so one slow lane may outlive the others; completion order is unspecified.
After-response work and route settlement retain their existing serial driver.
Real PostgreSQL regressions rendezvous four subscriber bodies, enforce 100
selected deliveries including sequential descendants, race duplicate sweeps,
and advance shutdown/monotonic expiry after four claims, proving settlement
without assuming a fast machine. The focused 10,000-delivery method and
comparator boundaries are in `tools/bench/README.md`.

**Schedules and the tick** (chunk 6.2). The checker reads each schedule
(`jadpo_check::schedules`: `every:` and `run:`, an action of its own
domain taking nothing) and where their state is kept
(`schedules_settings`: `schedules: { store: }` or the only database, else
`schedule.store_required`); the plan holds the schedule table
(`jadpo_std::schedule::SchedulesPlan`, `Plan::schedules`, which the
runtime reads from `Executor::plan`) with the stores holding the
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
(`schedules::why`), which the CLI says in words. On Workers the minute's
cron trigger calls the same function (chunk 8.2f, "The cron on
Workers").

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
row and keep a cancel. The runner reads the batches from the executor's
plan (`Plan::batches`), so compiled code runs the same runner. In an
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
handler, program and connection pools, behind one acceptor that awaits
the listener and hands connections to the workers in turn (since chunk
8.1l; §2.5, "Native accept and one rate-limit count"). A connection and
every request on it stay on one worker, so the runtime's values never
cross threads. The one thing the workers share is the rate-limit table,
plain counters behind a lock per shard held only to bump one count, so a
caller or an address is counted once whichever workers it reaches. Work
that never waits gives its thread back as it checks the deadline, so one
request can't hold the others a worker serves. One code path serves both
targets, with `Rc` instead of `Arc` and no other lock on the request
path. *Rejected:*
multi-threaded tokio with `Send` bounds switched off on wasm32, which
doubles the async signatures and lets bugs hide on one target.

| Component | Does | On Workers |
|---|---|---|
| HTTP | Router from the route table, input decoding, body limit, deadline, health, idempotency keys, failure envelope, `request_id` | fetch event |
| Identity | Sessions, CSRF, API tokens, service keys, OIDC sign-in, revocation delay | same |
| Policy | Principal, tenancy and memberships; predicates arrive compiled in the plan's SQL and the runtime binds the caller | same |
| Transactions | Read committed or repeatable read by what the action does, a snapshot closed before each outside call and row versions checked after it (chunk 7.4), a batch's item that calls a batch of its own (chunk 7.4b), rerun within the budget | same |
| Idempotency | A key claimed in one statement before the action; its response stored in the action's transaction, or in one statement for a failure | same |
| Outbox | Delivery, retries, dead letters and (7.2b) a circuit breaker, run by the runner after the response (chunk 6.1a), natively as a task on the worker's thread | `waitUntil` after the response, what one invocation leaves handed on to a fresh one (chunk 8.2h), swept by the minute tick |
| Scheduler | Due times in the schedule's state row from its first tick, leases, run records; native timer, and tick adapters (8.2g) | Cloudflare cron |
| Rate limits, caches | Rate limits through `Host::rate_limit`: Redis when one is named, otherwise one bounded table in memory for all of a server's workers (chunk 8.1l); cache reads with timeout, invalidation after commit | Redis or Cloudflare's rate-limiting binding required (area 16, question 6), the binding counted in since chunk 8.2m2; the isolate's memory only while it can't be reached |
| Telemetry | Spans per declaration, metrics, path numbers, threshold sampling, OTLP | export in `waitUntil` |
| Email | Templates, translations, the `email` capability | SMTP on 587 or a package |
| AI | `ai.generate`: the prompt rendered, the schema, decoding and repair; the built-in `Driver.openai_compatible` and the `ai` fake (chunk 5.5v3); `ai.embed` and `ai.embed_all`, the fake's embeddings and prompts' recordings (chunk 5.5v4); `ai.stream` read piece by piece and sent as server-sent events (chunk 5.5v6) | same, through Workers' `fetch` |
| Receiving | Queues fetched on the tick through a package, each message kept once as its event (chunk 7.7c) | Cloudflare Queues' consumer, the Worker's `queue`, hands each batch to `receiving::consume`, which answers each message (chunks 7.7c2 and 8.2k) |
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

**The start-up state and the runtime's plan** (chunk 7.9e, from the
Phase 7 architecture review). What a program started with is one runtime
value, `jadpo_runtime::Started`, made on each thread by
`Started::new(plan, values, target)` from the plan, the checked settings
(`config::load`'s `ConfigValues`, or none) and the target it runs on:
each setting's value decoded (never a secret's), the key settings' keys
(`effects::Keys`), each connection's credential and each webhook's secret
(`provider::Credentials`), what signing in reads
(`http::signed::AuthSettings`), how a client's address is chosen on that
target (`http::address::ClientAddress`; the interpreter runs on
`Target::Native`, and the Workers host makes its own with
`Program::with_config_on(values, Target::Workers)`, chunk 8.2d) and how
long each purge waits. The context reaches it (`Ctx::started`): the
runtime gives it to every context it makes for a program
(`Ctx::of(executor, host, env)`: a request's, a delivery's, a tick's, a
batch's), and since chunk 8.1e every context an executor runs in is made from
it that way, a test's harness's and the record tools' too, so nothing
gives a context its start-up state later (`Ctx::start_from` is gone). So
a setting is read one way wherever it is read: `Ctx::setting`, a test's
own value (its `config:`, `Ctx::set_settings`) or else the one the
program started with, for `config.<name>`, a call's address and model,
an email's sender and a mail package's mapping, a queue's mapping and a
magic link's origin. Until this chunk a mail package's call and a
queue's read the program's value even under a test's `config:`, which
area 13 says the code a test calls reads too. The plan the runtime reads
is one borrowed view (`jadpo_runtime::Plan` since chunk 8.1a, borrowed
from the one owned `RuntimePlan`, §2.5: the JSON table with the
program's validators and shapes, the failures, settings, routes,
subscribers and event numbers, batches, schedules, purges, file stores,
providers, `ai:`, prompts and prompt recordings), from which the runtime
builds what each of its parts runs with (`Plan::events`, `batches`,
`providers`, `webhooks`, `receiving`, `polls`, `ai`, `purge` and, since
chunk 8.1a, the HTTP component's `app`, given the `Started` their
credentials and periods come from).
The `Executor` is left with its places, its plan, what it started with
and running a callable (`places`, `plan`, `started`, `run`, `function`
and `row`; chunk 8.1e), where it had twenty methods, fourteen of them
bundling plan parts with the JSON codecs or the credentials. The runtime
finds a callable by its key through the plan (`Plan::callable`: a
webhook reply's action, a package's own `decode:` or `encode:`, a test),
reads a test's `config:` and `clock:` from the plan's dev and test
section (`Plan::test_settings`, `Plan::test_clock`), and checks a
recording of the app's own integrations itself (`recorded::run`), the
package's functions reached through the executor: `Executor::function`
runs one by its number, and `provider::Functions`, what a call's site
runs them through, is implemented for every executor by finding the
function's key in its plan. Each
call kind's site is one entry point taking the context, the site and the
input: `provider::call(ctx, provider::Calling { plan, site }, input)`
(its site naming its operation by number since chunk 8.1d2, "Provider
call sites by number" below), and `ai::generate`, `stream::stream` and
`embed::embed(ctx, ai::Calling { plan, site }, input)`, each finding the
plan's connection or `ai:`, the settings it reads (its `settings_read`,
their values through `Ctx::setting`) and the credentials before it builds
its request, so an executor only maps what comes back to its own stop.
The runtime's own calls (an email through a mail package, a queue's
receive and acknowledgement, a poll) go through the same entry point.
Chunk 8.1a made that view's plan one owned value with a canonical byte
encoding, which compiled code holds as a constant (§2.5); chunk 8.1d
builds each connection's driver from it and `Started` ("One connections
table and the driver factory" below).

**The `testing` feature** (chunk 8.1e, from the Phase 7 architecture
review). What only `jadpo test` and `jadpo dev` run is behind a cargo
feature, `testing`, on `jadpo-std`, `jadpo-runtime` and
`jadpo-host-native`: the fakes (`fakes`, `capability::Faked`, the `ai`
fake, a fake's `Answer::Generated` and `Answer::Recorded`), the recordings
(`recorded`, `prompt_recordings`), the crash points (`crashes`), the
outcome-and-effect record (`record`, `Host::record`), the delivery
between a test's steps (`test_steps`), and the plan's dev and test
section, `jadpo_std::testing::TestingPlan` (each test's identity key,
what its `config:` and `clock:` give it, each provider operation's
recording by its number, the recordings of the app's own integrations
and of prompts' answers). The section is `RuntimePlan::testing`, written
after the rest of the plan's bytes and only when it holds something
(`RuntimePlan::encode`); `RuntimePlan::encode_deployable` leaves it out,
and a build without the feature refuses bytes that hold one
(`Unreadable::Testing`). The tools (`jadpo-plan`, `jadpo-interp`,
`jadpo-test`, `jadpo-cli`) turn it on, each runtime crate's own tests
through a dev-dependency on itself, and no shared or runtime crate turns
it on by default or for another (`no_runtime_crate_turns_on_testing`).
Since chunk 8.2e `jadpo-host-workers` has one too, its test Worker's
handler (§2.6, "Route tests on workerd"), which only `jadpo-test` turns
on.
The gate builds the native host's deployable example
(`crates/jadpo-host-native/examples/deployable.rs`: a plan read from its
bytes, its drivers built by the factory, served through `Serving` on
`NativeHost`) on its own, so without the feature, and reads its symbols
for any of the above (`tools/deployable.py`); its wasm32 build of the
runtime crates is without it too. Since chunk 8.1o the binary `jadpo
build` writes is read the same way, in the live test that runs it. Measured on that example, a release
build on aarch64 macOS: 30,513,808 bytes without the feature and
30,629,520 with it (26,698,936 and 26,865,496 stripped), about 0.4%, as
most of a deployable is the standard library's locale and time-zone
data; what the feature buys is that a deployable reaches no fake, crash
point or test section at all. A Workers module (chunk 8.2b, "A Workers
deployable" below) is built without it the same way, and measured on
teaching example 23 at 10,246,720 bytes without it and 11,702,013 with
it, about 14%, its code a quarter larger.

**One connections table and the driver factory** (chunk 8.1d, from the
Phase 7 architecture review; area 11, "Connecting in `app.jadpo`" and
"Targets"). The plan lists every connection `connections:` declares
once, in name order, `RuntimePlan::connections`
(`jadpo_std::connections::Connection`: its name, its driver with the
settings that driver is built from, by number, and the targets it runs
on), where five places listed them before: the provider plan's
connections, the email plan's SMTP servers, `ai:`'s model connection, the
stores of files and the Cloudflare queues. A connection's number is its
place there: an `OperationPlan` names its connection by it, and
`provider::Credentials` keeps each credential by it. `ConnectionDriver`
is `Postgres { url }`, `Redis { url }`, `Disk { path }`, `Smtp` (its
server, port and sign-in settings), `OpenAiCompatible { url }`,
`CloudflareQueues` (its queue and dead-letter queue) or `Package` (how
each call through it is made, the old `ConnectionPlan`, without its
name). Each driver's targets are recorded now
(`ConnectionDriver::targets`: the disk and a service Jadpo runs, `runs:`,
natively only; Cloudflare's queues on Workers only; every network driver
and an HTTP package everywhere); `jadpo check` refuses a declared
deployable's mismatch since chunk 8.1o2 ("Declared deployables" below). The parts of the plan that use a
connection keep only what is their own and name it: `ai:` how it fills
`ai` (an `AiPlan`, whose built-in driver's `url:` its requests are
built from, so it is kept there too), `receiving:` how a queue's messages
become events (the Wrangler consumer is written from its
`QueuePlan`), `email:` which connection sends. `jadpo_runtime::Target`
(native or Workers) moved into `jadpo_std::connections` with it.
`Started` holds, beside the credentials, the text each connection's
driver is built from (`Started::driver_setting`: a database's address,
an SMTP server's name and sign-in, secrets included, never printed).
`jadpo_drivers::factory` builds every connection's driver from the table
and `Started` (`Drivers::build`) over what the host supplies
(`factory::Supplies`: the target, the `Transport`, HTTP as a
`package::Fetch`, a `factory::Disk`): PostgreSQL over the transport, one
package driver over HTTP shared by every package and
`Driver.openai_compatible` connection, the SMTP driver for its server
over the transport, the disk's store, and nothing for a Cloudflare
queue, whose batches come to the Workers consumer. A connection it can't
build (a target its driver doesn't run on, a part the host doesn't
supply, a setting with no value, a driver this build lacks) is kept as
`factory::Unbuilt` with why, for the deployable to report as it starts
(since chunk 8.1o, in its `runtime.started` event); its driver is then
none, which the runtime answers `misconfigured`. Natively `jadpo_host_native::supplies()` gives tokio's
sockets with rustls and `NativeFetch` (no disk yet: a deployable's disk
at `path:` comes with the files capability), and `NativeHost::
with_drivers` takes what it builds, where `with_providers`, `with_models`
and `with_mail` gave a closure-chosen driver each; `with_store` and
`with_files` remain for a store or a disk given in the factory's place
(`jadpo dev`'s own databases and `.jadpo/files/`, a test's). `jadpo dev`
builds each worker's drivers from the program it starts with and puts
its fakes in front with `capability::Faked::choosing`, which picks a
fake as each connection is asked for from the program running now (the
inbox for `email:`'s connection, `DevProviders` for a package's), since
saving the project changes them; `AppHandler` serves any
`http::Served` host, the native host or it faked. The model's built-in
driver is the factory's, so a model connection added while `jadpo dev`
runs is reached from its next start. `jadpo package record`, `jadpo ai
record` and `jadpo package conform` build their one connection's driver
the same way (`Drivers::keeping`). `jadpo test` builds none: a test
reaches no real driver, so its host gives each connection's stand-in by
kind from the table (the in-memory files, the mail fake, a package's and
`ai`'s fakes through `Faked`), as before. *Rejected:* a closure per
driver kind on the host, which is what grew to four; building drivers
inside the runtime, which would tie it to the transports the hosts own.

**A native deployable** (chunk 8.1o; area 14, "A native deployable").
`jadpo build` checks the project as `jadpo check` does and, with no
errors, builds the plan (`jadpo_plan::Plan::with_build` giving the
runtime plan area 18's build id in place of the graph's digest) and
writes a cargo workspace into `build/rust/`
(`jadpo_codegen::deployable::workspace`): the program's crate as
`generate` writes it (`jadpo-program`), a binary package
(`jadpo-deployable`) whose binary is named after the deployable and
depends on it and on `jadpo-host-native` without the `testing` feature,
the toolchain file and the runtime crates' `Cargo.lock` this Jadpo
pins. Its `main` is one call, `jadpo_host_native::deployable::run`,
given the program, the build stamp and each store's migrations (their
names and digests) as constants. `jadpo_codegen::deployable::build` adds
the two packages to the lock (`cargo update --workspace`) and builds in
release, `--locked` and offline, with `--remap-path-prefix` for the
workspace, the runtime crates and cargo's home, and `SOURCE_DATE_EPOCH`
fixed; the binary is copied to `build/target/<deployable>/` beside the
APIs' documents and failure files. The runtime's half is
`jadpo_host_native::deployable`: `config::load` over the process's
environment (`Mode::Environment`, with the program's compiled
validators, `compiled::Program::validators`), every problem logged as
`runtime.startup_failed` before anything listens; the factory's drivers
built once on the main thread to compare each store with its migrations
(`migrate::standing`), a pending, unknown, changed or out-of-order
migration stopping the start and an unreachable database waited for;
then `http::serve` with a worker per core (each with 64 MiB of stack,
`jadpo_runtime::STACK_BYTES`, since chunk 8.4a), each making the program on
its thread with the settings read (`Program::with_config`) and its own
drivers, served through `AppHandler`, so the tick runs on the first
worker's timer as `jadpo dev`'s does; `stop_signals` turns SIGTERM into
`Server::stop`'s drain. Its events are JSON lines on standard output,
each carrying the stamp; a fault behind a 500 is logged through an
`Observer` with its `.jadpo` frames (`Fault::stack`). Proved live
(`crates/jadpo-cli/tests/deployable.rs`): teaching example 23 built,
its binary holding none of the `testing` symbols, refusing to start
without `DATABASE_URL` and with its last migration not applied, then
booking a repair, answering its readiness check, answering a request
whose body arrived after SIGTERM, and ending with code 0. The build is
kept under `target/jadpo-compiled/deployable/`, 49 s cold in release
on the ten-core Mac and seconds warm; the gate runs it in the `compiled`
group. *Rejected:* the runtime's own entry point reading `PORT` and the
stores generated into the program's crate, which would put a
deployment's concerns in every compiled program the differential run
builds; and checking migrations once the port is open, which serves
requests against a schema nobody confirmed.

**Declared deployables** (chunk 8.1o2; area 14, "Declared deployables").
`jadpo_check::deployables` reads `application { deployables: { } }`
(each deployable's `serves:`, `runs:` and `target:`, or `app` alone,
serving every API and running everything, with no target, when none is
declared) and follows what each reaches (`reaches`): from the `runs`
edges of the graph's routes in the APIs it serves, the webhooks' replies,
and, as it runs them, the schedules' and subscribers' actions and the
callables that start batch runs, through each typed body's calls, to the
connections they use (a store of files, a provider's operation, a
package's action, `ai:`'s model) and those its polls, webhooks, queues
and email go through. A deployable that declares a target is refused
each one `ConnectionDriver::targets` and the SMTP port rule it can't run
(`deployable.target_unsupported`). The plan a deployable runs held
only the routes of the APIs it serves, the webhooks only where it
serves `webhooks`, and `RuntimePlan::runs` says whether
it runs `Trigger.schedules` and `Trigger.subscribers`; since chunk 8.1o3
it holds only what it reaches (below). The runtime holds to it
where work joins a runner (`runner::split` drops a delivery or a batch
run the program doesn't run, which waits, committed, for a deployable
that runs it) and on the tick (the schedules, polls and batch runs, or
the outbox's sweep, its breakers' alerts and the queues); every tick
still deletes what has expired. `jadpo build` builds each deployable's
workspace in `build/rust/<deployable>/`, its program the package
`jadpo-program-<deployable>` (cargo names a path package's unit by its
name and its path in its workspace, so several workspaces' `program`
would share one), all into the target §2.5's "Keeping rustc time down"
shares with every project for the host and locale set (chunk 8.4g), so
the runtime crates compile once; it places each binary as a
new file, since macOS kills a binary copied over one it has run. The
build id digested a declared deployable's declaration with the graph's
digest (`jadpo_cli::stamp`) until chunk 8.1o3, and `jadpo check` writes
each deployable's target and build id in the manifest. Proved live
(`crates/jadpo-cli/tests/deployable.rs`): teaching example 23 declared as
a kiosk serving `main` and running the subscribers and an office serving
a `staff` API and running a schedule each second, both built by `jadpo
build`; the kiosk books a repair and answers 404 for the staff's route,
and runs no schedule while it ticks; the office lists the repair's notes,
answers 404 for a booking, and runs its schedule. *Rejected:* filtering
the routes in the trie with the whole table kept, which leaves a route
nobody serves in the binary.

**A deployable compiled from what it reaches** (chunk 8.1o3; area 14,
"Declared deployables"; area 18, "A build id is a digest").
`Plan::for_deployable` plans the project from what the deployable
reaches (`jadpo_plan::Keep`): the callables
`jadpo_check::deployables::reached` follows, which since this chunk
adds those the runtime runs by key or number rather than because a body
calls them (the action `on_first_sign_in:` names wherever an API is
served; where subscribers run, the action `magic_link: { send: }` names
and every email template; and every provider package's `decode:` and
`encode:`, kept whole); the queries they reach and the entities whose
`get` they call; the routes of the APIs it serves, those APIs alone and
numbered among themselves, the runtime's own routes only in the API that
holds them; the schedules, purges and polls only where it runs
schedules; the queues and the emails' templates, sender and mail package
only where it runs subscribers, every subscriber still listed, without
its action (`Target::Elsewhere`), since an action that
emits writes a delivery for each subscriber that hears it; the webhooks
where it serves `webhooks`; the connections it reaches and every
database and Redis connection, which every tick sweeps; `ai:` where it
reaches the model; the prompts its callables ask for; the event types
they emit and those of the subscribers it runs; the steps' key where it
keeps an action with several changing calls; and the settings what it
keeps reads. Each table is built from what is kept, so its numbers
depend on nothing else. The settings are found by planning twice: once
with every setting, noting each one a kept part asks for, with those the
runtime reads by name (signing in's keys and origin, a `client_address:`
setting; `RuntimePlan::setting_names`), then with only those. What a
kept part names that isn't kept, a callable, a query, a setting, an
operation's connection or a prompt, or by key a package's function or a
webhook's reply action, is noted and the plan refused
(`jadpo_plan::Unkept`), which `jadpo build` reports as
`toolchain.build_failed`: a gap between what the checker follows and
what the plan holds is Jadpo's own mistake, never a program that faults
where it reaches the gap. Kept whole: the app's translations, since any
enum value formatted may read its wording, its identity, its locales and
its stores; since chunk 8.4c its identity without a method's signing key
where no API it serves takes the method, and without the browser
session's origin where none takes browser sessions, so a deployable that
serves no such API reads neither setting (the magic-link email still
asks for the origin where subscribers run). A Worker links SMTP's driver when its plan holds an SMTP
connection, which it does only where it reaches one. The build id
(`jadpo_cli::stamp::program`) digests the program as compiled: the Rust
its callables become, as the text of its tokens
(`jadpo_codegen::unprinted`, the files `generate` writes but unprinted,
since printing is a function of the tokens), its plan's deployable
bytes, its name and each store's migrations, with the compiler, the
target and `jadpo.lock` as before; `jadpo check` works it out for each
deployable, none for a project with errors, which builds nothing, and
`jadpo build` from the plan it compiles. So a change it doesn't reach
leaves it alone, and one that moves what it reaches to other lines
doesn't, since its frames name them; the application's name, its
folder's unless `app.jadpo` gives one, is part of the program (its
OpenAPI title), so a build is explained from a folder of the same name.
Proved by `crates/jadpo-plan/tests/deployable.rs` (a café's kiosk and
office each holding only its own callables, routes, schedules, settings
and connections, the office listing the kiosk's subscriber without its
action; and every project of the corpus, each deployable naming nothing
it doesn't hold and an app that declares none running everything the
whole plan runs), `crates/jadpo-cli/tests/manifest.rs` (an office's
action, route and setting and a test leaving a kiosk's id alone, its own
action changing it) and live (`deployable.rs`: the kiosk's binary holds
no sweeping, the office's no booking). Measured on teaching example 23,
release, aarch64 macOS, against the whole program each held before: the
café's `app` 21,103,600 bytes to 21,085,888 (17.19 to 17.17 MB
stripped: it reaches all but its tests and fixture, which the `testing`
feature already left out); declared as the kiosk and office above, the
kiosk 21,123,920 to 21,086,144 bytes and the office 21,085,776 to
20,507,088 (17.21 to 17.17 MB and 17.17 to 16.68 MB stripped); and the
kiosk on Workers 12,808,367 to 12,800,144 bytes (3.98 MB gzipped either
way). A program is a small part of a deployable beside the runtime and
its locale data. *Rejected:* keeping the whole plan
and leaving out only the code of the callables a deployable doesn't
reach, whose sites, settings and tables would still be in every
deployable and in its id; digesting the graph's nodes it reaches, which
hold nothing of what `app.jadpo` says of connections and settings; and
digesting the printed Rust, which costs every `jadpo check` the printing
(25 to 110 ms a deployable on the largest teaching examples, debug,
against 5 to 18 ms for the tokens).

**A Workers deployable** (chunk 8.2b; area 14, "A Workers deployable").
`jadpo build` builds a deployable with `target: Target.wasm` the way it
builds a native one, into the shared target for wasm, with the same lock,
toolchain, remapped paths and `SOURCE_DATE_EPOCH`: its workspace
(`jadpo_codegen::deployable::workspace` with `Host::Workers`) holds the
program's crate and a `cdylib`, `jadpo-worker-<deployable>`, which
exports the Worker's `fetch` with `#[wasm_bindgen(wasm_bindgen =
jadpo_host_workers::wasm_bindgen)]`, so it depends on the Workers host
alone, and hands it the same `jadpo_runtime::deployable::Deployable` the
native host's `main` is given (the program, the stamp and each store's
migrations, moved into the runtime from `jadpo-host-native` for both).
The Workers host's features are the drivers the deployable reaches
(`Drivers`): `postgres` when the project keeps a database, since every
deployable's tick reaches each store, and `smtp`, a feature added
beside it, when 8.1o2's `reaches` finds an SMTP connection; the package
driver always. It is built
`--target wasm32-unknown-unknown --lib` in release, linked with 8 MiB of
its own stack by a build script (`jadpo_std::compiled::WASM_STACK_BYTES`,
since chunk 8.4a), and
`jadpo_codegen::workers::bind` runs wasm-bindgen's library
(`wasm-bindgen-cli-support` at the lock's release, as the host's tests
do) in its `web` form, refusing a module that carries JavaScript of its
own, which no Worker file would upload. `build/target/<deployable>/`,
cleared first, gets the module, its JavaScript, Jadpo's shim
(`worker.mjs`, which instantiates the module once per isolate with
`initSync` and hands `fetch` its request, `env` and `ctx`) and
`wrangler.json` (`workers::wrangler`: the Worker's name with dashes (its
app's name before it since chunk 8.4f), the
shim as `main`, the compatibility date 2026-10-01, `no_bundle` with a rule
per kind, and each setting without a default as a required secret).
The screen gives the module's size raw and gzipped (miniz_oxide's deflate
at level 6 and gzip's 18 bytes). Until chunk 8.2d,
`jadpo_host_workers::deployable::fetch` made the program once per
isolate and answered every request 503 (since then, "Serving on
Workers", below). Proved
(`crates/jadpo-cli/tests/workers/kiosk.rs`, the gate's `wasm32` group):
teaching example 23 declared as a kiosk on Workers, built (42 s cold on
the ten-core Mac, seconds warm), its files and Wrangler file as above,
its stamp's target `wasm` in the manifest, its module linking
PostgreSQL's driver and naming none of the `testing` symbols, then run by
the pinned workerd from the files as the Wrangler file names them, the
plan read inside the module and the Worker answering. Measured on that
module, with 8.2c's locale data for its one supported locale: 9,494,470
bytes bound (3.0 MB gzipped), of which 4.5 MB is code, 3.9 MB data and
1.1 MB the name section (19,665,209 bytes, 6.5 MB gzipped, with every
locale's data, before 8.2c); as cargo built it, 10,246,720 bytes without
the `testing` feature and 11,702,013 with it (3.1 and 3.6 MB gzipped),
the code 4.9 MB against 6.0 MB. Until 8.2d the module doesn't reach the HTTP component,
the runner or the drivers, so these are the program and the runtime it
calls, not yet a serving Worker. *Rejected:* exports defined in the
Workers host, which can't name the program without the generated crate
registering it through an export of its own anyway; a direct dependency
on `wasm-bindgen`, at a version the generator would have to know; `wasm-opt`, which `binaryen`
would have to be downloaded for (chunk 9.2 tunes the module); and
`vars` for settings, which take a value the build doesn't have.

**Serving on Workers** (chunk 8.2d; area 14, "A Workers deployable").
`jadpo_host_workers::deployable::fetch` is 8.1o's native handler for an
isolate, returning a promise of the `Response`
(`wasm_bindgen_futures::future_to_promise`), so the generated export and
the shim are unchanged. The parts both hosts share moved into
`jadpo_runtime::deployable`: the log (`Log`, one sorted JSON object a
line with the stamp, given a sink: standard output natively, the
console's `log` on Workers), the start-up problems (`setting_problem`,
`unaddressed`, `client_address_refused`, `pending`, `diverged`), each
store compared with the build's migrations (`compare`: `Current`,
`Refused` with its problem, or `Unreachable`, which the native host waits
for) and `Faults`, the observer logging `operation.failed` (since chunk
8.4c `Logged`, and the start itself shared). An isolate
starts at its first request (a thread-local `OnceCell`): the program
made, `config::load` over the bindings (`HostVariables` of that
request's `WorkersHost`), `ClientAddress::read` with `Target::Workers`,
whose refusal is a problem, and `Program::with_config_on(values,
Target::Workers)`; a failure is logged once and the isolate answers each
request `http::unavailable` (503 with a request id, as the runtime
answers). Each request then has its drivers built through the factory
over `supplies()` on a `WorkersHost` of its own, when it first reaches a
connection (chunk 8.4e), and, until the isolate has
found every store current, compares them first through that host's
databases, answering 503 with `runtime.startup_failed` or
`runtime.waiting` while one isn't, so a migration applied after a
deploy is picked up at the next request; the first request to find them
current logs `runtime.started`. The request is read from Workers'
`Request` (its path and query as the URL gives them, every header, the
body read through its stream up to `BODY_LIMIT` or marked too large, and
no connection address, so the client is `CF-Connecting-IP`) and served
by `Serving` on that host, sharing the isolate's `http::Worker`, its
deferred work run through `waitUntil`; the `Response` is built with each
header appended, no body for a status that has none, and a streamed
answer sent as a `ReadableStream` since chunk 8.2h (below). Proved under workerd
(`crates/jadpo-cli/tests/workers/kiosk.rs`, the gate's `wasm32` group, now
given the run's PostgreSQL, which CI's `wasm32` job starts as the others
do; a local run's listens on a Unix socket, which `connect()` can't
open, so the test starts one of its own on `127.0.0.1`): teaching
example 23's kiosk without `DATABASE_URL` answering 503 and logging the
variable once; against a database missing its last migration answering
503 and naming it, then, the migration applied, the same isolate booking
a repair whose subscribers find it a bench and print its ticket while the
request waits (`settle:`), the ticket read back, a thank-you's note
pinned up after its 202 in `waitUntil`, its readiness check reaching the
database, and 120 requests a minute for each `CF-Connecting-IP`, the
121st 429 while another address is answered; every line one JSON object
with the stamp, the database's address in none; checked locally against
a server asking for a SCRAM password too. The module with it, on that
kiosk: 12,814,132 bytes bound, 4.0 MB gzipped (6.9 MB code, 4.3 MB data,
1.6 MB names), against 9.5 and 3.0 MB before, now that the module
reaches the HTTP component (the identity routes among it), the runner,
the drivers and the Workers host; past the 3 MB Cloudflare documents for
a compressed Worker on its Free plan and inside the Paid plan's 10 MB,
which chunk 8.2n measures against the real plan and chunk 9.2 tunes. *Rejected:* reading
the settings at each request, which a Worker's fixed bindings don't
need; keeping a failed migration check for the isolate's life, which
would leave a deploy made before `jadpo migrate apply` refusing until
its isolates were recycled; and one `Serving` an isolate over a host
standing for whichever request is current, which an isolate serving
requests at once would mix up.

**Before the real deploy** (chunk 8.4f; area 14, "What `jadpo build`
writes", "The Wrangler file" and Decisions, "A deployable's release
profile and its Worker's name"). Each deployable's workspace
`Cargo.toml` gives its release profile (`deployable::release_profile`):
`lto = "fat"` and `codegen-units = 1` for both hosts, `opt-level = 3`
natively and `opt-level = "s"` for Workers; cargo's own
`CARGO_PROFILE_RELEASE_*` variables still override it. `workers::bind`
takes `Names`: a deployable's module is bound with wasm-bindgen's
`remove_name_section` and `remove_producers_section`
(`Names::Removed`), a test Worker's keeps its names (`Names::Kept`, so
a trap names the Rust it was in), and a native binary keeps its symbols.
The Wrangler file is one ordered value, `workers::Wrangler` written by
`workers::wrangler` (a small `Json` of its own, since `serde_json`'s map
sorts its keys without its `preserve_order` feature, which the whole
workspace would share): every part is a field, so no binding can be
dropped as `rate_limited` and `queued` could drop theirs by splicing
text that didn't end as they expected. Its `name` is `worker_name(app,
deployable)`, the project's name (its folder's) and the deployable's,
each lower-cased with each run of anything but a letter or digit one
dash, at most 63 with the deployable's kept whole, as the pinned
Wrangler checks a Worker's name; `observability: { enabled: true }` is
written because Wrangler 4.149.0's upload sends `observability` only
when the file has it (its config's default is `undefined`), and Workers
Logs keeps nothing for a Worker without it. Checked: the pinned
Wrangler's `deploy --dry-run`, network refused, accepts example 23's
kiosk file given a Hyperdrive id without a warning, naming its bindings
and reporting an upload of 7,908.95 KiB, 2,945.88 KiB gzipped.

Measured on example 23 as tools/bench builds it (native `app`; Workers
`kiosk`), on the ten-core Mac while other builds ran, before and after:

| | before | after |
|---|---:|---:|
| native binary | 21,256,704 bytes, 7,759,639 gzipped | 14,832,368 bytes, 6,150,912 gzipped |
| Workers module | 13,503,038 bytes, 4,141,531 gzipped | 8,062,968 bytes, 2,945,138 gzipped |
| native rebuild, the program touched (load ~6) | 3.5 s | 62.9 s |
| Workers rebuild, the program touched (load ~5 to 13) | 8.0 s | 33.4 s |
| native build from nothing | 100 s (load 28) | 111 s (load 9) to 561 s (load 17 to 50) |
| Workers build from nothing | 102 s (load 21) | 85 s (load 6) to 277 s (load 48) |
| `POST /repairs` x16, native | 2,923 req/s, 0.744 CPU ms | 2,798 req/s, 0.727 CPU ms |
| `POST /repairs` x16, workerd and `pool` | 431 req/s, 2.256 CPU ms | 487 req/s, 2.077 CPU ms |
| `GET /health/ready` x16, native | 114,544 req/s, 0.027 CPU ms | 94,373 req/s, 0.028 CPU ms |
| `GET /health/ready` x16, workerd | 16,078 req/s, 0.063 CPU ms | 15,878 req/s, 0.064 CPU ms |

Gzipped at zlib's level 6, as the build screen gives it; the request
rows are `tools/bench/run.py --targets native,workers --connections 16
--rounds 3`, medians, load average 13 to 33, so a difference under
about 10% in requests a second is noise and the CPU milliseconds a
request are the steadier measure. The Workers module is 40% smaller, its
whole upload 2,945.88 KiB gzipped as Wrangler counts it, at the 3 MB
Cloudflare documents for a Free plan's Worker compressed (whether that
is 3,000,000 bytes or 3 MiB is 8.2n's to find on the real plan), and a
booking costs it 8% less CPU; natively the binary is 30% smaller and a
booking about 2% cheaper, within the noise. Thin link-time optimisation
in 16 units, tried through cargo's variables, gave a 22,404,544-byte
binary (larger than before) and a 9,991,297-byte module, 3,315,202
gzipped, so fat in one unit is what reaches the size. The cost is the
rebuild: a minute natively and half a minute for Workers where it took
seconds, which `jadpo dev` never pays, since it interprets. The gate
builds its test deployables with cargo's codegen units and no
link-time optimisation (`TEST_RELEASE_PROFILE` in tools/verify.py, set
for the `compiled` and `wasm32` steps that run `jadpo build`), so its
CI jobs keep their time; the profile itself is checked as written
(jadpo-codegen's tests) and the module's names gone (the kiosk's test).
Phase 9 starts from these numbers. *Rejected:* stripping a native
binary's symbols, which have no upload limit to meet and which a
backtrace and Phase 9's profiler read; `strip` in cargo's profile for
Workers, which would take the names from cargo's module too, which the
tests read for what a module links; and thin link-time optimisation,
above.

**Route tests on workerd** (chunk 8.2e, the vertical slice's proof).
`jadpo_test::workerd::differential_all` runs a project's route tests on
its compiled program inside workerd and compares each test's record with
the interpreter's, as `jadpo_test::compiled` compares compiled code's run
natively (§2.5, "Differential test"). It writes each project as a module
of one crate with the `testing` feature, as the native harness does,
beside a test Worker's crate, `jadpo-test-worker`, a `cdylib` whose
`fetch` is `jadpo_host_workers::testing::fetch` over the crate's
`PROGRAMS` (the Workers host's `testing` feature, which turns the
runtime's on; only `jadpo-test` turns it on, and no deployable), builds
it for `wasm32-unknown-unknown` in the dev profile without debug
information (`jadpo_codegen::cargo::build_module`), offline, in a
workspace kept under `target/jadpo-compiled/workerd/`, binds it with
`jadpo_codegen::workers::bind`, and serves it under the pinned workerd
with a shim of its own, its outbound network `local` alone. Every test
that runs a Worker finds workerd through one installer,
`jadpo_test::workerd_install` (the Workers host's tests include it by
its path): the pinned release, installed by `npm ci` under a lock across
processes (`tools/workers/.install.lock`) only when it isn't there, and
tried again when npm leaves no binary, as it does without a word when it
couldn't fetch the platform's package, an optional dependency. Each request
is one run of one test, `Asked` posted as JSON: the program, the test's
key, the run's order seed for a test with a database, a copy of each
store's database by its address over TCP, a crash to make, and whether
to run the batches it started and keep a record. The Worker makes the
program once per isolate with the settings a test run reads (`test:`
values, defaults, throwaway secrets from Web Crypto) on
`Target::Workers`, and runs the test through `test_run::once` on a host
of its own: each copy reached over `connect()` by the PostgreSQL driver,
its statements kept in the record; `test_run::Caught`; Workers' passkey
ceremonies; no randomness, timer or clock, as a test's host has none.
It answers `Answered`: the record's lines, the rows its stores held when
it used one (`test_run::rows`, over the same connections), how the run
ended, whether a delivery between its steps failed for good, the fakes'
use, the steps it reached and what its keyed calls did. The runner
judges that as it judges a run of its own (`outcome::Ending`,
`fake_rule`), crashes it again at each step it reached
(`run::crash_runs`), each run on a fresh copy, and builds the record
(`TestRecord::of_lines`), which `differential::compare` holds to the
interpreter's. The copies are made on the run's server when it is named
by a URL, as CI's is, and otherwise on one the process starts on
`127.0.0.1` (`Server::tcp_url`; `TestDatabases::on`, its templates kept
per server), since a local run's listens on a Unix socket, which
`connect()` can't open. A test's software authenticator
(`test_passkeys`) is the native host's, over OpenSSL, so a test Worker
has none: a route test that makes passkeys differs there until one is
written in pure Rust. Proved (`crates/jadpo-cli/tests/compiled.rs`, a
step of the `wasm32` group, whose job builds for wasm32 already; the
`compiled` job's time ran out with it): area 7's `todos` (3 route tests) and teaching example
23's repair café (4, through `settle:`'s subscribers, a request's work
after its answer delivered before the next line, and its fixtures), every
test passing with the interpreter's records, statements, events and rows
among them. The module is 44.6 MB bound (42.5 MB as cargo built it,
every locale's data and the dev profile); on the ten-core Mac, under a
load average of 150 to 330, the test took 226 s from nothing, nearly all
of it building, and 24 s warm, of which binding is 15 s and the seven
tests 2 s; its target folder is 918 MB. *Rejected:* the release profile,
which would build every runtime crate optimised for tests that run in
seconds unoptimised; one target folder with
the native harness, whose builds would then wait on each other's lock;
and a test Worker's run of its own beside `jadpo test`'s, which would be
a second copy of what a test runs under.

**The cron on Workers** (chunk 8.2f; area 12, "The tick"; area 14, "A
Workers deployable"). `jadpo check` refuses, for a Workers deployable
that runs `Trigger.schedules`, each schedule whose `every:` is under a
minute (`deployable.target_unsupported` at its `every:`, naming it,
beside 8.1o2's refusals in `jadpo_check::deployables::target_problems`),
since the minute's cron can't run it. `jadpo build` writes the cron,
`triggers: { crons: ["* * * * *"] }`, into a Workers deployable's
Wrangler file when its tick has a store to keep or sweep
(`SchedulesPlan::ticks`), as every native deployable ticks; the
generated crate exports `scheduled` beside `fetch` and the shim hands
it the controller, `env` and `ctx`.
`jadpo_host_workers::deployable::scheduled` starts the isolate as
`fetch` does and, once its stores are current (out of date, logging
why as a request does and ticking nothing), runs `Serving::tick` on a
`WorkersHost` of its own made `for_cron`, whose tick budget is what is
left of the cron's 15 minutes (`cron_left`). While the program ticks
more often than once a minute (`Serving::tick_every_ms`: every 10
seconds while it receives a queue, as the native timer does) it ticks
again at each such mark of the cron's minute, from its
`scheduledTime`, one tick after the last ends (`next_tick`), so an
invocation holds no more connections than one tick does and a mark a
long tick passed isn't made up; it ends with the minute. Proved under
workerd (`crates/jadpo-cli/tests/workers/depot.rs`, the `wasm32` group): a
depot built for Workers that serves nothing, reads a local SQS through
the SQS package and takes stock every minute, its Wrangler file naming
the cron and `jadpo check` refusing the schedule at `every: 30s`; its
`scheduled` run as the cron trigger runs it, through a service
binding's `scheduled()` (workerd's `service_binding_extra_handlers`,
`--experimental`, in the test alone): out of date, logging the
migration and leaving the queue alone; then, current, a cron 40
seconds into its minute keeping the queue's message as its event and
delivering it, then ticking again at the 50-second mark and receiving
a message sent between the two, the schedule's first run due a minute
after the first tick; and, the schedule due, a cron at the end of its
minute running it once. *Rejected:* a tick at each mark started
whether or not the last had ended, as the native timer starts one,
which on Workers would hold the connections of several ticks in one
invocation; and a schedule refused in `jadpo build` rather than by
`jadpo check`, where an agent sees it as it writes the schedule.

**The `trigger:` adapter** (chunk 8.2g; area 12, question 16 and "The
tick"; area 14, "A native deployable"). `jadpo_check::schedules::
schedules_settings` reads `schedules: { trigger: }`, a `ScheduleTrigger`
(`jadpo_std::ScheduleTrigger`: `native`, the default, or `external`),
anything else `schedule.settings_invalid` with the nearest, and the plan
carries it (`SchedulesPlan::trigger`). It changes only a
native deployable: `jadpo_host_native::deployable::run` reads its one
argument (`Command::read`: none or `serve`, `tick`, anything else
`command_unknown` and code 2), and serving with `external` makes each
worker's `AppHandler::untimed`, whose `Handler::tick` is `None`, so the
first worker's timer ends at its first wake and the server never ticks.
`tick` starts as serving does (`configured`: the plan, `config::load`,
the factory's drivers, each store compared with `Then::NextTick`) but
waits for no database, an unreachable one `store_unreachable`; then one
`Serving::tick` on a `NativeHost`, spawned as the host's tracked work so
the first SIGTERM drains it (`NativeHost::drain`) as a server drains its
background work, logged as `runtime.ticked` (`ticked_fields`: each run
by its schedule's name, the deliveries, what was received and polled,
the rows deleted, the alerts as their own events, each store it couldn't
work in and the unbuilt connections), code 1 when a store couldn't be
worked in, or when the tick didn't finish, having panicked or outlasted
the drain (chunk 8.4b). `jadpo dev` keeps its timer and a Worker its cron whichever
is written. `jadpo build` gives a native deployable whose ticks another
scheduler starts a `tick` line, the command and how often to run it
(`tick_every_ms`), and `--json` its `tick`. Proved live
(`crates/jadpo-cli/tests/deployable.rs`): example 23's office, once
`trigger: ScheduleTrigger.external`, serving for 2.5 s without running
its every-second schedule, `office tick` refusing to start without
`DATABASE_URL`, then running the schedule once, logging it by name and
ending with code 0, and `office ticks` ending with code 2. *Rejected:*
an HTTP endpoint for the tick and its secret (Richard, 2026-10-21); a
`tick` that waits for its database, which stacks a scheduler's runs up
behind an outage that the next minute's run would see past; and
`ScheduleTrigger.native` refused for a Worker, which would bar an app
with a server and a Worker from writing the default down.

**One start for both hosts** (chunk 8.4c; area 14, "A native
deployable" and "A Workers deployable"). What the native host's
`configured`, `check_stores` and `tick` and the Workers host's `started`
and `stores_current` each did their own way is `jadpo_runtime::
deployable`'s: `start` (the plan read, `config::load` over the host's
variables, `ClientAddress::read` for the target, `Program::with_config_on`;
the values given back for a native server's workers), `compare_store`
(a store with no driver `store_unbuilt`, naming the factory's reason
through `NotBuilt`, which `jadpo_drivers::factory::Unbuilt` implements,
since the runtime can't name the drivers' crate), `compare_stores`
(`Stores`: the problems, and the stores out of reach, which a tick,
`Then::NextTick`, makes `store_unreachable` problems), `waiting`
(`runtime.waiting`'s words by what follows), `unbuilt` (the list
`runtime.started` and `runtime.ticked` give) and `Logged`, the observer
both hosts serve through in place of `Faults`: `operation.failed` as
before, and each tick's result as `runtime.ticked` (`ticked_fields`,
moved from the native host), given its duration by `Serving::tick`
(`Observer::ticked` takes the milliseconds the host's monotonic clock
counted) and the connections the serving host's factory didn't build.
So a native server's timer and a Worker's cron log each tick as `tick`
does, a store a tick can't work in among it. Proved by the runtime's own
tests (a tick on a host whose store can't be reached logged with its
`store_failed` problem; each store compared alike), live by example 23's
office logging its every-second ticks with a table it sweeps renamed for
a moment, and under workerd by the depot's cron logging each of its
ticks and the same failure. *Rejected:* a duration kept on `Ticked`,
which every tick's caller would then fill; and logging only a tick that
did something or failed, which a quiet log can't tell from a server that
stopped ticking.

**PostgreSQL through Hyperdrive** (chunk 8.2i; area 5, "PostgreSQL
through Hyperdrive"). A database connection's address on Workers is its
`url:` setting's binding, which `jadpo build` writes into the Wrangler
file as a Hyperdrive binding (`jadpo_codegen::workers::Hyperdrive`, its
`id` from `JADPO_HYPERDRIVE_<variable>` in the build's environment, or
none, which the screen alerts and `wrangler deploy` refuses) in place of
a required secret. `WorkersHost::variable` reads a Hyperdrive binding (an
object with a `connectionString` and a `connect` function) as an address
of its parts, its user, password and database percent-encoded, at the
host and port the binding names (`<hex>.hyperdrive.local:5432`), never
the binding's own `connectionString`, which carries `sslmode=disable`
and the password unencoded; reading the host registers the Workers
runtime's route to Hyperdrive for the isolate, and the host keeps it as
the platform's pooler (`jadpo_host_workers::hyperdrive`). The drivers'
`Transport` gained `is_pooler`, false natively and true on Workers for a
host and port a Hyperdrive binding gave, and PostgreSQL's driver reaches
a pooler without its SSL request whatever its connection's security,
since the binding's socket offers no TLS and its address says
`sslmode=disable` (workerd's `api/hyperdrive.c++`): the connection stays
inside Cloudflare, and TLS to the database, with its certificate's
check, is Hyperdrive's, configured on it (`--sslmode`, `--ca-certificate-id`)
and checked on the real account by chunk 8.2n. A text binding of the same
name stays an address the Worker reaches directly, over TLS through
`startTls()` after the SSL request; Workers checks the certificate as the
first bytes go out and reports only a broken socket then, so the driver
wraps an upgraded stream (`Checked`), keeping an `InvalidData` failure of
its first read or write, and a startup that fails with one is the fault
naming the server and whom it was checked against (`failed_check`), as
an upgrade refused natively is, never unavailable. Hyperdrive's caching
must be off (`--caching-disabled`), since it doesn't invalidate reads on
a write and Jadpo's reads are current; nothing a Worker sees tells it, so
it is stated where the binding is written. The driver already keeps no
session state: statements unnamed (`query_typed`), each transaction's
level in its own `BEGIN`, held by a live test that finds a reused
connection at the server's level with no prepared statement after a
repeatable read transaction; a column of an extension's type (pgvector's
`vector`, PostGIS's `geography`) is the one exception, tokio-postgres
describing it once per connection through a named statement, which
Hyperdrive documents keeping for the client that prepared it and chunk
8.2n checks. Proved under workerd (`crates/jadpo-host-workers/tests/workerd.rs`):
a Hyperdrive binding read as its address, its host the pooler and
`connect()` to it reaching the binding's service in a later request; a
store reaching a scripted PostgreSQL elsewhere over TLS through
`startTls()`, and one signed by an authority workerd doesn't trust
refused as the fault (without `Checked`, unavailable); and
(`crates/jadpo-cli/tests/workers/kiosk.rs`) example 23's kiosk built with its
Wrangler file's Hyperdrive binding, with and without an id, then served
through workerd's Hyperdrive binding to the run's PostgreSQL: the
migrations compared, a repair booked, its thank-you pinned up after the
answer, the readiness check reaching the database, the log holding no
address. *Rejected:* TLS from the Worker to Hyperdrive through
`startTls()`, which the binding's socket doesn't offer and its address
turns off; a key of its own on the connection (`hyperdrive:`), which
would put a target's detail in `app.jadpo` both targets read; telling a
pooler by its name's `.hyperdrive.local` suffix, which natively is an
mDNS name like any other; and the binding's own `connect()`, which the
transport would have to hold the binding for when the host's name
reaches it already.

**After the response on Workers** (chunk 8.2h; area 12, "Running
background work"; area 7, "Streaming a model's answer"). Work an
invocation's 30 seconds didn't reach goes on in a fresh invocation of the
Worker. `Host::hand_on` is given a `runner::HandOn` (the executor, the
host and the pieces left), which natively runs on the host that handed it
on (`HandOn::run`) and on Workers is sent as its message
(`HandOn::message`): JSON naming each delivery by its id and each batch
run by its run's, with its store, never an event's value.
`WorkersHost::hand_on` calls `handOn(message)` on `ctx.exports.JadpoHandOn`,
the loopback binding Workers gives the Worker's own `JadpoHandOn`
entrypoint, a `WorkerEntrypoint` class the shim exports beside the
default export, so no request from outside reaches it; since chunk 8.4b
no binding in the Wrangler file names the Worker, so a Worker deployed
with `wrangler deploy --name` or `--env` hands work to itself, not to the
one the file names (until then a service binding, `JADPO_HAND_ON`, did).
The calling invocation keeps itself alive with `waitUntil` until the call
returns. `jadpo_host_workers::deployable::hand_on`, which the generated
crate exports as `hand_on`, starts the isolate as `fetch` does, builds
its drivers on a host of its own (`for_hand_on`, whose background work
has what is left of the invocation's 30 seconds counted from its start,
`hand_on_left`, chunk 8.4b) and, in that invocation's `waitUntil`,
once its stores are current, calls `runner::carry_on`: each delivery is
read back from its record in one statement a store (`outbox::HANDED_ON`,
`DUE`'s columns for the ids given, still `pending` or `running`), so one
run or given up meanwhile is left out, each batch run taken from its row,
and the pieces run as an invocation of kind `HandedOn`, handing on in turn
what they don't reach. A call that fails, as past the platform's limit on
invocations one request chains, leaves the work in the store for the
minute's cron. A streamed answer is the `Response`'s body as a
`ReadableStream` over an underlying source of the host's own
(`streamed::body`): each `pull` reads the next event through
`EventStream::next_written`, on the host the request was served on and
held to its times as natively, and enqueues its bytes, closing after the
last; the source's callbacks are let go once it ends. A client that goes
away ends the request, and Workers ends its `fetch()` to the model with
it: workerd finds the client gone when it next sends it a piece, rather
than calling the source's `cancel`, which, when a platform does call it,
stops the read in hand and drops the stream, aborting that `fetch()`
itself; when Cloudflare's edge finds a client gone is for chunk 8.2n.
Proved under workerd (`crates/jadpo-cli/tests/workers/newsroom.rs`) by a news desk
of its own whose model is a server the test starts: a draft's first piece
reaching the client before the model writes the rest, a client that goes
away closing the model's connection once the model writes again,
eighteen stories filed in one request each summarised
by a subscriber in two seconds, and a batch run over eighteen clippings
the same, both past one invocation's budget and finished by the
invocation each handed on to, then the minute's cron (chunk 8.2f)
telling the at most one summary the budget's end cut short; natively, `tests/background_budget.rs` carries
work on from its message alone, a delivery run meanwhile left out.
*Rejected:* a queue (area 12, Decisions, "Work handed on, on Workers");
a request to the Worker's own URL, which reaches its public `fetch`; and a
`TransformStream` written by a task of its own, which would read the
model's answer ahead of the client rather than as it is taken.

**Six connections an invocation** (chunk 8.2j; area 14, "A Workers
deployable"; area 11, `max_concurrent:`). Cloudflare lets a Worker's
invocation have six connections waiting for their answer's head at once,
a `fetch()` until its status and headers come and a `connect()` until its
socket opens, and queues a seventh until one of them gets there; since
2026-04-09 a connection whose head has come no longer counts, and the
platform no longer cancels a connection it finds idle (Cloudflare's
limits page and its changelog, "Relaxed simultaneous connection
limiting"). workerd enforces no such limit: its server's limit enforcer
enforces none (`src/workerd/server/server.c++` at the pinned release), so
the runtime counts them itself. Each invocation's `WorkersHost` holds its
six places (`jadpo_host_workers::Opening`), and everything the invocation
opens takes one: the drivers' `WorkersTransport` and the package driver's
`WorkersFetch`, built over `WorkersHost::supplies`, and the runtime's own
`WorkersOutbound`, the work it leaves in `waitUntil` included, since that
runs on the same host. A `fetch()` gives its place back once its head has
come or it failed, a `connect()` once its socket opened or failed; a
seventh waits, in the order asked, woken by the place given back, which is
safe since an invocation's work all runs in its own context. Asking for
a place first hands any free place to the longest waiter
(`Placing::poll` calling `hand()`, chunk 8.4b): otherwise a place given
back while a waiter handed one hadn't yet taken it would sit free while
the newcomer waited behind it. A request
waits within its own timeout, and one whose timeout passes before it went
out is `NotSent`, safe to send again, rather than an abort that Workers'
`fetch()` can't tell from a request that left. Not counted, since
Cloudflare's list leaves them out: the service binding work is handed on
through (whose callee shares the top-level request's six, a call that
overlaps its caller only as the caller ends) and the rate-limiting
binding; nor `startTls()`, an upgrade of a socket already open.
`max_concurrent:`'s slots are the isolate's (area 11, decided
2026-10-21), shared by its invocations, and a slot one gives back is
handed to a call of another without the first waking it: wasm-bindgen's
executor polls every woken task from one queue in a microtask of the
waker's (`js-sys` 0.3.106, `futures/queue.rs`), so a task woken from
another invocation runs in that one's context, and its I/O with it; and
a call waiting on nothing of its own is ended by workerd as hung, as the
process's woken set did to the second of two invocations sharing one
slot when tried under workerd in the chunk. So the Workers host's set is
a looking one
(`Slots::looking`, `jadpo_runtime::slots`): a slot given back is handed
to the call that has waited longest, which finds it when it next looks,
every 10 ms on its own invocation's timer (`slots::wait`, which
`outside::exchange` calls); and since Cloudflare cancels a request whose
client went away before its response, and work in `waitUntil` past its
30 seconds, without dropping what it held (its context and errors
pages), a call holding a slot says so every second (`slots::keeping`),
and a holder or a waiter not heard from for five seconds is forgotten,
its slot handed on. Natively
the set is the process's, its waiters woken, as before. Proved under
workerd (`crates/jadpo-host-workers/tests/connections.rs`, the `wasm32`
group): eight requests sent at once without counting all reaching the
server together, and counted never more than six waiting for their heads,
one whose timeout passed while it waited `NotSent` and never seen by the
server; an echo's `connect()` waiting behind six requests until a head
came; four requests from a response and four from the work it left in
`waitUntil`, never more than six waiting at once; a call of one
invocation taking up the slot another gave back, its own request sent
and answered; and a slot taken and never given back, as an invocation
cancelled mid-call leaves it, taken back once its holder had been quiet
for five seconds (workerd itself lets a request whose client left run on
until it answers, so the probe leaves its slot as a cancelled one
would). They add about 2 s to the step (6.7 s against 4.5 s warm on the
ten-core Mac), the five-second wait running beside its other tests. *Rejected:* counting the connections an invocation holds
open, Cloudflare's rule before 2026-04-09, which would hold a seventh
behind a store's idle connections and need them closed to make room;
leaving the seventh to the platform's queue, where its timer runs unseen
and workerd, which queues nothing, never shows it; a promise of the
waiting call's settled from the giving invocation, which the decision
rules out; and a slot taken back after its call's own timeout, which
would cut short a call legitimately slow before its request went out.

**Cloudflare Queues' consumer** (chunk 8.2k; area 11, "Chosen in chunk
7.7c2"; area 14, "A Workers deployable"). The generated crate exports
`queue` beside `fetch` and `scheduled`, and the shim hands it the batch,
`env` and `ctx`. `jadpo_host_workers::deployable::queue` (its own module,
`deployable/queue.rs`) starts the isolate as `fetch` does and, once its
stores are current, hands `receiving::consume` the batch's `queue` and
each message's body as text (a string as sent, an `ArrayBuffer` or view
as UTF-8, any other value through `JSON.stringify`, `undefined` as
nothing, which parks as unreadable) with its `attempts`; it answers each
message on its own, `ack()` or `retry({ delaySeconds })` as `consume`
says, and only then gives the kept events' deliveries to
`runner::after_response`, so they run in `waitUntil` and what its 30
seconds don't reach is handed on (chunk 8.2h). An isolate that can't
start or whose stores aren't current retries every message after the
delay its attempts give (`receiving::retry_delay`) and keeps nothing; a
batch with a message retried logs `queue.retried` (the queue, how many,
`not_started`, `not_received` or `store_unavailable`), never a body.
Since chunk 8.4b the delay doubles up to an hour over 30 retries
(`receiving::RETRY_DELAY_MAX_SECONDS`, `CONSUMER_RETRIES`), about 22
hours, so a message outlasts a deploy's migration; each queue's
dead-letter queue is a consumer too (`DEAD_LETTER_RETRIES`, 100), and
`consume` given a batch from it parks each message as `dead_lettered`
(`receiving::dead_lettered`), its id and event when they can be read;
and `answer_messages` reports a message whose `ack()` or `retry()` was
missing or threw, so the handler's promise rejects once every message is
answered and Cloudflare hands again each one not acknowledged, rather
than acknowledging it as a handler that ends well does.
`jadpo build` adds `queues: { consumers: [ ] }` to the Wrangler file from
the deployable's plan (`jadpo_codegen::workers::queued` until chunk 8.4f, now one part of
`workers::wrangler`, through `jadpo_std::receiving::wrangler_queues`), which keeps the queues only
where subscribers run. The file is checked against Wrangler itself:
Wrangler 4.149.0 is pinned in `tools/workers/` beside workerd, whose
release it is held to by an npm override, and installed with it by the
shared installer (`jadpo_test::workerd_install::wrangler`), about 70 MB
more in `tools/workers/`; the test runs
its `deploy --dry-run` with no account, its banner's update check,
telemetry and error reports off, a home of its own, and
`tools/workers/offline.mjs` preloaded through `--import`, which refuses
and reports every DNS lookup and outbound connection, so a run that
reached for the network fails (Wrangler's update check reaches npm's
registry when it warns of an unexpected field, which the preload caught
while this was built). Wrangler validates a consumer's setting names and
types but no queue name, and it warned of the `.wasm` rule hiding its own
default, which the file now gives `fallthrough: false`. Proved under
workerd (`crates/jadpo-cli/tests/workers/queues.rs`, the `wasm32` group)
through a service binding's `queue()` on the test's helper Worker: a
warehouse whose Wrangler file Wrangler accepts without a warning; out of
date, a message retried after 20 seconds on its second attempt; current,
a batch of a text body, a JSON body, a repeat, an unlisted type and one
that isn't JSON each acknowledged, two events kept and delivered after
the answers, two messages parked; and a batch from a queue it doesn't
receive retried whole. *Rejected:* answering the batch whole
(`ackAll`/`retryAll`), which would retry kept messages with a failed one;
throwing from the handler while the stores aren't current, which
Cloudflare counts as a failed attempt with no delay of Jadpo's; and
checking the Wrangler file against Wrangler's JSON schema alone, which
says nothing of what `wrangler deploy` warns of.

**The locale data a deployable carries** (chunk 8.2c; area 10,
"Locales"). `jadpo test`, `jadpo dev` and the compiled step run with
ICU4X's compiled data for every locale `Locale` lists; a deployable
carries its supported locales' only, native and Workers alike.
`jadpo build` hands `jadpo_cldr::export` the supported locales' tags,
which bakes again, from the data compiled into this Jadpo
(`jadpo_cldr::Compiled`, every marker of the ICU4X data crates in the
lock, each implemented by its own crate's macro), each marker's entries
for the locales a supported locale's lookups walk through (the locale,
then each its fallback gives for that marker's configuration, down to
`und`, which is always kept), collation among them (chunk 8.4i; an app
with no `locales { }` can't sort by a locale, chunk 8.4j), and every
marker with one entry, through
icu_provider_export's `ExportDriver` and baked writer. The files go into
a folder of the build cache named by their digest (§2.5, "Keeping rustc
time down"; chunk 8.4g), written whole and renamed into place, one
folder for every project whose locales give the same files, so their
shared target compiles the runtime crates once; `deployable::build` runs
cargo with `ICU4X_DATA_DIR` naming it (`Release::cldr`), so each ICU4X
data crate compiles them in place of its own: every `compiled_data` constructor is the same call,
and each lookup in a supported locale finds the entry the full data
gives (`jadpo-cldr`'s tests walk every marker's fallback for six locales
to hold it). A `Locale` the app doesn't support reaching formatting,
a message or `list.sort_by` is written in, or sorts by, the supported
one a header naming it would get (`jadpo_value::locale::written_in`) on
every target, so `jadpo test` and the deployable write the same text
and the same order. Proved live (`deployable.rs`): a
shop supporting `en_gb` and `fr_fr`, built, holds no German or Spanish
weekday and answers what `jadpo dev` answers; its native binary went
from 31.3 MB to 21.0 MB (27.4 to 17.1 MB stripped; release, aarch64
macOS). Exporting takes about two
seconds in a debug `jadpo`; the sizes are in area 10's decisions.
*Rejected:* ICU4X's blob provider read at run time, which would put a
second constructor beside every compiled-data one and deserialise as the
app runs; data filtered by marker as well as by locale, whose savings
the linker already makes.

**Provider call sites by number** (chunk 8.1d2, split from 8.1d). A
provider call's site plan (`jadpo_std::provider::CallSite`) names the
operation it calls by its number in the plan's provider operations
(`CallSite::called`, `Called::Operation`), so `provider::call` finds it
there (`Providers::numbered`) rather than scanning for the connection's
and the operation's names. A package action's own operation is
`Called::Own`, its place among its package's operations: the runtime
adds it to the first operation of the connection the action was called
through (`ProviderPlan::first`, by the connection's number;
`ProviderPlan::called`), since one action serves every connection to its
package. `jadpo-plan` numbers the operations before it decodes any site
(`providers::Numbering`, in the order the provider plan lists them, a
connection's together in its package's order), and the runtime's own
calls carry their operations' numbers in their plans the same way
(`PackageEmail::called`, `ReceivePlan::called`,
`AcknowledgePlan::called`, `PollPlan::called`), each sent through a site
that reads the whole answer (`provider::whole_site`). The names stay
where they are read: in what is logged and faked, a fault's words, and
the record tools, which name a call from outside the plan
(`ProviderPlan::number_of`). Since chunk 8.1e `ai:`'s calls through a
package find theirs by number too: a package's `generate:`, `stream:` and
`embed:` mappings carry their operation's number (`PackageAi::called`,
`PackageStream::called`, `PackageEmbed::called`), its name kept for
what is logged, recorded and faked.

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

**Single changing calls and safe key windows** (chunk 9.1d2a; areas 11,
"Keys and windows", and 12, "A step in doubt"). A resumable action reaching
a keyed changing provider call, with a store already available for its writes
or events, also has a `RecordPlan`, with `replay: false`. It uses the existing
step tables only for operation, first dispatch time, key generation and runner
generation: no request fingerprint or answer is kept, and no steps encryption
setting is required. Code keeps a fresh `clock.now`; provider keys retain their
existing derivation. A retry at or beyond the provider window less the smaller
of 10% and one hour parks as `outcome_unknown` before another send. Every
physical send checks this horizon on the host clock and fences the runner's
generation, including retries of a previously written step. Operator retry,
abandon, visibility and 30-day cleanup use the existing record paths; a
successful action deletes the metadata in its commit. New metadata-only
shapes carry a `dispatch-v1:` prefix in `RecordPlan.shape` and in the stored
shape; legacy untagged shapes remain encrypted replay.
Opening an incompatible mode persists a `diverged` problem for operators
before decoding answers or sending anything, including records with no answer
and an empty request. This mode check does not compare whole shape digests.
The existing operator build comparison reads the same tagged plan shape, so
unchanged metadata records remain compatible across builds. Multi-call
encrypted replay is unchanged. A storeless route still has no durable claim
or historical window across independent requests; this change adds no configuration
requirement to those accepted routes.

**Connection URL structure** (chunk 9.1d2a; area 10, "Database and other
connection URLs").
`jadpo_std::connection_url` checks PostgreSQL and Redis URI structure without
network I/O, through the built-in validator shared by literals, startup and
`config check`. PostgreSQL multi-host and explicit socket forms need their own
URI handling rather than a web URL parser. Legacy hostless database paths
retain their type acceptance; this does not add implicit socket or user
defaults to the host driver. Existing TLS-parameter checks
still run after structure checks, and configuration reports all invalid
settings without their values.

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
`failed_when:`, `Repeat` and failures), the credentials the program
started with, held apart as the keys are (`provider::Credentials`, in
`Started`), and the values of the settings its address reads, as its
context reads them (chunk 7.9e). The runtime writes the input with the
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
`jadpo_host_native::fetch::NativeFetch`: HTTPS through rustls (since
chunk 8.1b), or plain HTTP to this machine, over a connection kept for
its host ("A pooled HTTP client" below).

**A pooled HTTP client** (chunk 8.1m, from the Phase 7 review). Natively
every request that leaves the process, a provider's call
(`fetch::NativeFetch`) and the runtime's own fetch (`fetch::Outward`),
goes through one client, `fetch::Client`, of which each worker has its
own (`supplies()` is called per worker): hyper's HTTP/1.1 client
connection (`hyper::client::conn::http1`) over tokio's sockets, its task
spawned on the worker's runtime, and rustls's connector made once from
Mozilla's roots. An answer read to its end gives its connection back to
a pool held in an `Rc` on the worker, keyed by the runtime driving it,
the scheme, the host and the port; the next call to that host takes the
newest one kept, at most `fetch::MOST_IDLE` (8) waiting per host, none
older than `fetch::IDLE` (30 seconds), looked at as connections are
taken and kept. A kept connection the server has closed since is let go
before anything is written on it (hyper's `ready` fails), and a request
one hands back unwritten (`try_send_request`) goes out on a fresh one,
so keeping connections never turns a call that never left into one that
may have acted; one whose answer says `Connection: close` is never kept.
A server may still close or reset a kept connection as a request reaches
it, after the request is written and before any answer (chunk 8.1m2):
no client can see that coming. The request goes out once more, on a
fresh connection, only when `provider::Request::repeatable` says it may
be sent again once it has left, which `Call::with_request` sets from the
call's `Repeat` (`Safe` or `Keyed`; `Outward`'s `GET`s always); a change
with no key is `NoAnswer`, as any call that may have acted.
Requests go out with hyper's title-cased header names, `Host` first and
no `Connection: close`. A call's timeout bounds the whole exchange: a
connection not made within it, or within five seconds, is `NotSent`, an
answer not whole within it `TimedOut`. Dropping a call's future, or a
streamed answer's body before its end, drops its connection, whose
drop aborts the connection's task and so closes its socket at once,
where the blocking exchange it replaces went on reading until its own
timeout. A streamed answer is hyper's body read frame by frame, each
waited for at most the call's timeout. A name is looked up with tokio's
`lookup_host`, the system's resolver on tokio's blocking pool, only when
a connection is opened, so the platform's own configuration
(`/etc/hosts`, a VPN's resolvers) counts and a kept connection asks for
none. `Outward` is the same client with a guard: HTTPS only, connected
only to the public addresses the name gave (`fetch::public`), its pool
apart from `NativeFetch`'s, no redirect followed (hyper's client follows
none), at most the bytes asked for. A test reaches a server on this
machine through `NativeFetch::trusting`, given a configuration that
trusts the test's authority, and through `Outward::reaching`.
*Rejected:* hyper-util's pooled
`legacy::Client`, whose pool sits behind a lock shared across threads,
the one kind §2.6 keeps off the request path, and whose connector must
be `Send`; reqwest, which wraps that client and brings its own
configuration of TLS and redirects to turn off; and a resolver of our
own or a resolver crate, which would read `resolv.conf` and skip the
platform's configuration, for a lookup a kept connection rarely makes.

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
connection the plan names (`Plan::provider_connections`), and after the run
reports a script the calls ran past or left answers in
(`fakes::Usage`; `test.fake_exhausted`, `test.fake_unused`). `jadpo dev`
puts one fake in front of every provider connection (chosen through
`Faked::choosing` since chunk 8.1d), `jadpo_cli::dev_calls::DevProviders`: the
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
a package record (`jadpo_db::Record`), and the plan's dev and test
section (since chunk 8.1e) carries each operation's text by its number
(`TestingPlan::operation_recordings`) and every recording of the app's
own integrations as a `RecordedPlan` (its operation planned whether or
not a connection calls it). `jadpo_runtime::recorded` keeps an
answer as a recording (`recorded::kept`: the path to `output_at:` and
the members the output type declares, each plain text or number a
placeholder naming its built-in type, `"{Text}"`, where a generated value
reads back; what the package's own encodings and decoders read kept as
sent, digits included) and fills one back (`recorded::filled`, from
`fakes::generated`). `recorded::check` reads a recording whole through
the package's functions at the call's site, a `RoundTrip` coder checking
each field's `encode:` gives back what its `decode:` read; it is
`recorded::run` (`Executor::recording` until chunk 8.1e), which `jadpo
test` runs for each `RecordedPlan` as
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
`JsonHttp` for the model connection, built by the driver factory since
chunk 8.1d; `ai::AiFake` is in its place in every test, scripted by
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
for a client that went away, and the Workers host as a `ReadableStream`
read as the client takes it (chunk 8.2h); `NativeFetch::open` reads the provider's
answer as it comes on the worker's thread and closes its connection when
the body is dropped (chunk 8.1m). `JsonHttp::stream` hands over a success's body as it comes;
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
`Plan::prompt_recordings`, in the dev and test section since chunk 8.1e,
each with the JSON-table numbers of the types
its prompt's `ai.generate` sites decode into; `recorded(...)` lowers to
`AiFakeAnswer::Recorded`, which the interpreter scripts with the answer
`prompt_recordings::answer` fills (area 11's `recorded::filled_value`),
and `jadpo test` runs `prompt_recordings::check_file` for each as a
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
RECEIVED_EVENTS` joins that store's schema. The runtime reads them from
the plan with the secrets the program started with (`Plan::webhooks`,
`provider::Credentials` in `Started`), and the
HTTP component answers a request at a webhook's path, an entry of the
route trie since chunk 8.1k (§2.5, "The route trie"), with
`jadpo_runtime::webhooks::answer`, after its address's rate limit:
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
no redirect, through the pooled client since chunk 8.1m) and keeps it per thread for a day, verified with
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

**The Workers host** (chunk 8.2a). `jadpo-host-workers` is the
runtime's host on Cloudflare Workers, over wasm-bindgen 0.2.129, js-sys
and web-sys 0.3.106 and wasm-bindgen-futures 0.4.79 (the releases
tokio-postgres's `js` feature had already put in the lock), with bindings
of Jadpo's own to what Workers gives a Worker beyond the Web platform
(`connect()` from `cloudflare:sockets`, `ctx.waitUntil`, and the globals
`fetch`, `setTimeout` and `crypto`), each bound with `catch`, so a
JavaScript exception comes back as an error rather than unwinding through
the module. It builds natively too, where the bindings compile but are
never called, so the workspace builds, lints and tests as one.

- **`WorkersHost`** is one invocation's `Host`, made from its `env` and
  `ctx`: the drivers the factory built over `supplies()`
  (`Target::Workers`, `WorkersTransport`, `WorkersFetch`, no disk), for
  each invocation, since a socket belongs to the request that opened it,
  built when it first reaches a connection, so one that reaches none (a
  readiness check answered from its last second's) builds none, and the
  connections that can't be built found once an isolate, at its start
  (chunk 8.4e); randomness from `crypto.getRandomValues`, 65,536 bytes a
  call; the wall clock, `Date.now()`, and a monotonic clock made from it
  that never goes back past an earlier reading (Workers moves both on
  only while an invocation waits, never while it computes); timers on
  `setTimeout`, cleared when a wait is dropped, its promise then settled
  by hand, so nothing is left waiting on it (chunk 8.4e); settings from
  the bindings, a text
  binding being a variable or a secret; `defer` through `ctx.waitUntil`;
  rate limits in Cloudflare's rate-limiting binding when the plan's store
  (`RouteTable::rate_limits`, `rate_limits: { store: }`) is one, each
  limit in the binding of its own the store names (`limit({ key })`, a
  refusal waiting the window's length), and in the isolate's memory, one
  table an isolate, while it can't be asked, the fallback area 16 allows
  (chunk 8.2m2), a binding that doesn't answer within 500 ms not waited
  for and each binding's fallback logged once an isolate as
  `rate_limits.fallback` (chunk 8.4b); and Workers' budgets
  (`budget_for`): 30 seconds after a response
  and for work handed on, 15 minutes for the cron. Work handed on starts
  a fresh invocation of the Worker through the loopback `ctx.exports`
  gives its `JadpoHandOn` (chunks 8.2h and 8.4b, "After the response on
  Workers"); a database needs the crate's `postgres`
  feature, so a deployable that keeps none links no driver for one; the
  passkey ceremonies are `WorkersPasskeys`, below.
- **`WorkersTransport`** opens every connection with `secureTransport:
  "starttls"`, since a driver says it will upgrade only once its
  conversation has begun, waits for it to open, and upgrades with
  `startTls()`, passing `expectedServerHostname` when the name to check
  isn't the one connected to. `startTls()` takes no authority of a
  connection's own, so `Trust::Authorities` is `InvalidInput`. Workers
  opens the upgraded socket at once and checks the certificate as the
  first bytes go out, telling JavaScript only that the socket broke, so a
  failure before anything has gone through over TLS is `InvalidData`, as
  a check that fails is natively; a driver that judges a failed check
  only by the upgrade's own answer (PostgreSQL's, chunk 8.1n) learns it
  at its first write instead, which PostgreSQL's driver keeps since chunk
  8.2i ("PostgreSQL through Hyperdrive", below). A stream reads and
  writes through one reader and one writer for its socket's life, each
  taken at its first read or write and released only to upgrade it,
  between them (chunk 8.4e; before, one of each for every read and write
  was about half of workerd's time on a booking, §2.11), and closes its
  socket when dropped; a Unix socket is `Unsupported`. **A connection
  ends within the invocation that opened it** (chunk 8.4e): workerd never
  settles a promise of an invocation that has ended, and whatever waits
  on one is kept for the isolate's life. So the transport counts the
  tasks it starts for its connections (`Tasks`), and an invocation's
  `WorkersHost`, once dropped (after its response, with the work that
  left, or with the answer it streams), keeps the invocation through
  `waitUntil` until they have ended: dropping its drivers is what starts
  each connection's goodbye. Shutting a stream down closes its writable
  side after the driver's goodbye (PostgreSQL's `Terminate`), then, with
  a read still waiting, the socket, waiting for that read to settle. An
  invocation therefore lasts a round trip past its host (chosen in chunk
  8.4e, 2026-10-22: closing without the goodbye would end it sooner but
  leave the server or Hyperdrive a connection broken off; holding the
  invocation from a connection's start instead kept it alive while a
  streamed answer's host lived, so a client going away no longer ended
  the model's call). Before, each booking's connection was left waiting,
  some 20 KB of the module's memory never given back (§2.11).
- **`WorkersFetch`** sends through `fetch()` with `redirect: "manual"`,
  so a `3xx` is an answer, races the whole exchange (or a streamed
  answer's head, then each piece) against a timer, and when the timer
  wins aborts the request through its `AbortController`'s signal, so its
  connection is let go while the invocation carries on; it reads at most
  `MOST_ANSWER`. Workers' `fetch()` rejects with the same `TypeError`
  whether or not the request left, so every rejection is `NoAnswer`, a
  call that may have acted: a keyed or safe call is sent again, a change
  with no key never twice. Natively a refused connection is `NotSent`.
- **`WorkersOutbound`**, the runtime's own fetch, sends a `GET` over HTTPS
  only, never to an address written as one that isn't public
  (`jadpo_runtime::public_address`, the native guard's check, moved to
  the runtime so both ask one question) nor to `localhost`, follows no
  redirect and reads at most the bytes asked for. A name isn't looked up
  first, since a Worker has no resolver; whether Cloudflare's network
  reaches a private address a name resolves to is for the real deploy
  (chunk 8.2n) to check.
- **`WorkersPasskeys`** (chunk 8.2l) is `Host::passkeys` on Workers:
  `webauthn-rs` 0.5's passkey ceremonies written again in pure Rust,
  since it verifies with OpenSSL. The options are its own member for
  member, the challenge 32 bytes from `getRandomValues`; an answer is
  read as it reads one (client data's type, challenge, origin and, when
  registering, `crossOrigin`; the authenticator data's relying party,
  presence, verification, backup flags and counter; byte strings in the
  four base64 forms it reads, CBOR through the same `serde_cbor_2`) and
  its signature checked with `jadpo_value::public_key`'s ES256, its DER
  read strictly, and RS256 on RustCrypto's `p256` and `rsa`. A key is
  kept as the same COSE bytes the native host writes, so a passkey's
  record reads the same on either. Attestation must be `none` or packed
  self-attestation (area 8, chosen in chunk 8.2l). Its state is JSON of
  its own, the challenge and the passkeys excluded.

Every Workers test runs on this machine under the pinned workerd, with
no Cloudflare account and no network past the machine; only the real
deploy (chunk 8.2n) needs an account. Proved under workerd, Cloudflare's
own Workers runtime (`crates/jadpo-host-workers/tests/workerd.rs`): a probe Worker
(`examples/probe.rs`) built for wasm32 by the test, bound by wasm-bindgen's
library (`wasm-bindgen-cli-support` at the lock's release, so nothing is
downloaded and no command-line tool has to match the crate), with a module
of Jadpo's own that instantiates it once per isolate, and run by workerd
1.20261007.1, pinned in `tools/workers/package-lock.json` and installed
there by `npm ci --ignore-scripts` the first time a test needs it (never
globally; `JADPO_WORKERD` names another), against servers the test starts
on this machine: an echo over TCP, the same behind TLS signed by an
authority workerd's network is told to trust and by one it isn't, and
HTTP. Its sixteen tests cover randomness, the clocks and the timer,
bindings, budgets, TCP, TLS checked for the name and refused for another
name and another authority, a Unix socket and a connection's own
authority refused, a request's method, headers and body sent and its
answer read, no redirect followed, a call past its timeout ended then and
its connection let go within 1.5 seconds while the probe goes on for two
more (without the abort it is held until the invocation ends, which the
test was checked against), a streamed answer read piece by piece and
stopped at a piece's timeout the same way, the outbound guard, work run
after the response through `waitUntil`, rate limits kept across an
isolate's requests, a connection's task run (since chunk 8.4e one still
running when its host is dropped ending after the answer, which it never
did once the invocation had ended), the drivers built only once a
connection is reached, and the factory's supplies;
since chunk 8.2l a seventeenth registers a passkey made by the test
authenticator and signs in with it twice, keeping what the native host
keeps of the same device's passkey, an answer from another site and a
counter gone back refused. Natively, `tests/passkeys.rs` holds
`WorkersPasskeys` to `webauthn-rs`: each ceremony run on both, with the
test authenticator and with answers made by hand to break one check at a
time (about sixty cases: origins, types, challenges, relying parties,
flags, keys off the curve or of other kinds, attestation, counters, user
handles, a high S), each kept or refused the same on both.
The gate runs them as a step of its `wasm32` group, in cargo's lane since
the test builds the probe; `compiler tests` leaves them out. *Rejected:*
workers-rs, whose `Socket::start_tls` unwraps (a panic aborts the
instance) and takes no options, whose event macros need its build tool's
shim, and whose `worker-build` downloads esbuild, binaryen and
wasm-bindgen, unpinned, as it builds; Node with Workers' APIs stood in
for, which tests the stand-in; and wasmtime, which runs no JavaScript.

**What differs on Workers.**

- No threads, no timers between requests, no disk: delivery runs in
  `waitUntil`, and the minute cron calls `schedules::tick`, which runs
  the schedules due and sweeps each store's outbox,
  each within the budget the host gives it (30 seconds after a response,
  15 minutes for the cron; chunk 6.7's `Host::budget_ms`, set in 8.2a),
  and what one leaves goes on in a fresh invocation (`Host::hand_on`), a
  request to the Worker itself (chunk 8.2h), through `ctx.exports` to
  its own `JadpoHandOn` since chunk 8.4b.
- PostgreSQL goes through Hyperdrive over `connect()` sockets with
  tokio-postgres, as in workers-rs's
  [example](https://raw.githubusercontent.com/cloudflare/workers-rs/main/examples/tokio-postgres/src/lib.rs),
  one connection per request, its address the binding's and reached
  without TLS (since chunk 8.2i, "PostgreSQL through Hyperdrive", above).
  Hyperdrive
  [pools by transaction and caches reads](https://developers.cloudflare.com/hyperdrive/concepts/how-hyperdrive-works/)
  without invalidating them on writes, so Jadpo requires its caching off
  (reads are current, area 6), uses unnamed statements (`query_typed`),
  keeps no session state outside a transaction and takes leases with rows,
  never advisory locks. Rows reach the module as wire bytes rather than
  JavaScript values, which should remove the conversion that held the
  experiments to about 79% of Bun
  ([optimisation results](experiments/wasm-optimization-results.md)): an
  inference until the performance spike (chunk 8.3) measures it.
- Six connections waiting for their answer's head at once per
  invocation
  ([limits](https://developers.cloudflare.com/workers/platform/limits/)),
  counted by the runtime across database, providers, SMTP and its own
  fetches, the work left in `waitUntil` included (chunk 8.2j, "Six
  connections an invocation", above).
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
`golden-todo/` includes its application and authored tests) is copied to a temporary
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
that, once the process that started it ends. Since chunk 8.1n
`Server::start_on_tcp` starts one listening on `127.0.0.1` too, at a free
port, offering TLS when given a certificate and key as PEM. The driver's live tests
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
itself before it runs (`self_test`). A test whose subject isn't a
timeout gives each call seconds, not milliseconds: under a loaded gate a
call given 300 ms timed out, was sent again and met a fake's unscripted
answer (chunk 8.1m2), so `provider_wire`'s connections wait 5 s, and
`quick`, whose timeout is the subject, 1 s against answers held 1.5 s.

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
| 8 | The differential run, the `compiled` group (built: chunks 8.1f to 8.1j); the Workers host's probe under workerd, in the `wasm32` group (built: chunk 8.2a); a teaching example built for Workers by `jadpo build` and served by workerd against the run's PostgreSQL over `connect()`, a record created and read back, in the `wasm32` group (built: chunks 8.2b and 8.2d) |
| 9 | Every golden contract case on native (`compiled`) and compiled workerd (`wasm32`), plus verifier negative controls (`lint`), built in chunk 9.1d2b; the strong TypeScript implementation has one Node/Bun static, integration and shared-contract pipeline in `tests` (chunk 9.1d3) |
| 10 | Independent whole-first-build review and agent comparison remain outstanding |

**Strong TypeScript golden verification** (chunk 9.1d3). The full profile's
`tests` group runs `examples/golden-todo/typescript/adapter/verify.py` once.
It holds an OS file lock across `npm ci --ignore-scripts --no-audit --no-fund`,
type-aware lint, typecheck, formatting, verifier controls, Vitest and the shared
contract. The lock covers execution as well as installation, since a second
`npm ci` removes the first run's dependencies. The runtime declarations are
`.node-version` and `.bun-version`; CI installs those exact versions. Vitest
uses two workers; the cold type-aware ESLint rejection control has a 15-second
tool-startup allowance, independent of application request and fetch deadlines.

The private runner uses `JADPO_TEST_DATABASE_URL` from the gate, creates
randomly named databases, applies the Drizzle migrations, and drops only its
own databases even when migration or testing fails. Standalone runs instead
own a disposable PostgreSQL16 cluster on `127.0.0.1`. Child shutdown and
transaction cleanup are awaited. Controls exercise real process lock
exclusion and release after failure, failed-migration cleanup with an unrelated
database retained, and continued shared-server availability.

The shared 44-case registry is loaded from this checkout by default. Every
case crosses raw HTTP through the production Node adapter and `Bun.serve`;
private controls invoke production authentication, application and jobs code,
with actual PostgreSQL and injected issuer/mail transports. SQL observation
records physical statements and transaction boundaries. Missing sources,
runtimes or controls fail visible obligations. A full successful run using the
same checkout may report `completeContract: true`; an explicit different
checkout always reports a preview. Reports identify both revisions, dirty
states, runtime versions and before/after source, schema, migration, lock and
contract fingerprints. These results describe the exercised contract, not an
independent review or comparative performance measurement.

Validation on 2026-10-23 used clean implementation commit
`c351051ff48c7e160f7a926b30ea7d77931a879f`, including dependency head
`deb6d7e03b1e56aa2e9e4e4c85d84f2d372cd428`. The full
`python3 tools/verify.py` gate passed all 90 steps in this worktree. Its
TypeScript pipeline passed 42 Vitest tests, static checks, private verifier
controls, actual host-disconnect controls and all 44 shared cases on each of
Node 24.18.1 and Bun 1.2.20, using the gate's shared Unix-socket PostgreSQL
server. The same gate passed all 44 native and all 44 workerd cases using
this checkout's freshly built compiler, plus the corpus differential run,
non-testing deployable, Workers deployable and watch checks. Local optional
vector tests recorded the existing pgvector 0.7.0 versus required 0.8.0
extension skip; the golden application uses no vectors.

The compiler SHA-256 for this run is
`191936bec0cc17149c92437e5f982c98c4230adbfb2f902a62a187a06e96f3a5`.
Both generated targets' source stamps equal the own-checkout
application/runtime/harness fingerprint
`47d5b6c4b770b3d1df048ec81137677b8037a197585f2a75132fd5784bc6561b`.
The TypeScript source/schema/migration/lock/contract/orchestration fingerprint
is `e534516450542afa273950fd4e8e0519fdc5459f6eaed3b40fb7ed87d473e5c5`.
These permanent validation facts follow the passing implementation gate;
fingerprinted inputs remain unchanged.

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
than fifty-odd; jadpo-runtime has one more, `route_allocations` (chunk
8.1k, its own folder under `tests/`), with no harness, so the counting
allocator it installs sees no other test's work; a named step runs its test files by name
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
`--group lint|docs|tests|compiled|wasm32` runs one CI job's share (a run
whose steps need fewer packages' tests than the workspace's builds only
those: `--group compiled` builds `jadpo-cli`'s, and `jadpo-test`'s when
it starts the server), and
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

**The differential run** (chunk 8.1j). The compiled step (§2.5,
"Compiled code") is the gate's `compiled` group, a step of its own using
the run's server: every project of the corpus that checks compiled as
modules of one crate, one rustc invocation in the dev profile, every
test run interpreted and compiled with the same records, every route's
typed entry reading a seeded set of requests and writing a set of values
as the codec walk does (chunk 8.1p, §2.5's "A typed per-route entry"),
and its counts shown under the step. It builds the compiler a second time, into the
harness's own target folder, and inside the `tests` job it shared a
four-core runner with sixty other steps: the step took 628 s there and
the job 13 min 20 s (chunk 8.1i's pull request). So CI runs it as a job
of its own, `compiled`, whose budget is its timeout: 12 minutes from
start to end when chunk 8.1j set it, about twice what it took with no
cargo cache at all, 5 min 31 s and 5 min 41 s in two runs (110 to 119 s building `jadpo-cli`'s
tests, 170 to 174 s for the step, all 258 projects and 525 tests
compared, the 28 teaching examples' 334 tests passing both ways), and 4
min 11 s with its cache (76 s and 127 s); 20 minutes since 2026-10-22,
when the corpus step took about 9 minutes and saving the cache about one
more. The `tests` job's gate beside
it took 420, 432 and 764 s in those three runs, against 739 to 873 s in
the five runs before them, which ran the step too. The projects the
step writes into `target/tmp/` are removed before the cache is saved,
since the cache action takes their `tests` folders for cargo's own and
fails on each. Locally, `python3 tools/verify.py` still runs every
group, this one among them, since a chunk that breaks compiled code
costs less to find before its pull request than after; there the step
took 136 and 149 s of gates of 161 and 197 s (load average up to 52), the
longest step; `--profile quick` leaves it out, as it leaves out every
live step, and `--group compiled` runs it alone, as CI does (90 s with
the harness built from nothing, 69 s of it the step). Area 7's `todos`
and teaching example 23's route tests inside workerd (chunk 8.2e, §2.6,
"Route tests on workerd") share the step's test file but are a step of
the `wasm32` group, skipped by this one: they joined it first and ran
the `compiled` job out of its 12 minutes, and the `wasm32` job builds
the runtime crates for wasm32 already. What they add there is that
build, about 250 crates for wasm32 in the dev profile from nothing and
the workspace's own crates with the cache; the seven tests take
seconds.

**CI.** GitHub Actions on Linux runs the gate's five groups as parallel
jobs, each with the pinned toolchain and its own cargo cache: `lint`
(formatting, clippy, the name ban, the editor grammar), `docs` (the pages
and the teaching examples), `tests` (every test, with the live PostgreSQL
steps running at once on one PostgreSQL container named by
`JADPO_TEST_DATABASE_URL`), `compiled` (the differential run above) and
`wasm32` (the runtime crates for wasm32, their checks under Node, the
Workers host, a Workers deployable and the route tests under workerd,
which its tests install into `tools/workers/` with `npm ci` the runner's
Node provides); `docs`, `tests`, `compiled` and `wasm32` each build and
start their own container from `tools/postgres/Dockerfile`, PostgreSQL 16
with pgvector and PostGIS at pinned versions (chunk 5.5v1; area 21,
"Without pgvector"), while the toolchain installs. A local run's
disposable server comes from the machine's own PostgreSQL tools, which
need pgvector for the vector tests and PostGIS for the geography tests
(Postgres.app ships both). The harness's workspace is kept under
`target/`, so the `compiled` job's cache keeps the registry crates built
in its target folder as well as the workspace's: the cache action keeps
every registry crate's build in each target folder it finds there. The
`supported-language` job needs all five and passes only when each did; it
is the check branch protection requires. Benchmarks (Phase 9) are a
separate command, not the gate. The jobs' times, one run each, so a sign
rather than a measure: before chunk 8.4g's shared target (8.4f's merge
on `main`) `compiled` 10 min 13 s, `wasm32` 9 min 4 s, `tests` 9 min
37 s, `docs` 3 min 37 s, `lint` 1 min 17 s; on 8.4g's pull request
`compiled` 11 min 8 s, `wasm32` 6 min 21 s, `tests` 12 min 45 s, `docs`
4 min 23 s, `lint` 1 min 10 s.

### 2.8 Determinism and the build stamp

- **Same source, same bytes** in `build/`: no hash-map order in output,
  project-relative paths, no timestamps, per-build numbering in source
  order. A gate test checks each corpus project twice and compares.
- **The build id digests inputs, not the binary:** the deployable's
  program as compiled from what it reaches (since chunk 8.1o3), the
  compiler version, the target and the lock
  (area 18). `jadpo incident` therefore needs only `check` of the stamped
  commit with the stamped compiler, not rustc.
- **Toolchain pinning.** Each Jadpo release pins one Rust toolchain and
  ships the runtime crates' `Cargo.lock`. `jadpo build` writes a
  `rust-toolchain.toml` into the generated crate, builds `--locked` with
  `--remap-path-prefix` and a fixed `SOURCE_DATE_EPOCH`
  ([reproducible Rust](https://reproducible-builds.org/docs/rust/)), and
  binds a Workers module with wasm-bindgen's own library at the release
  Jadpo's lock holds (chunk 8.2a), never `worker-build`, which downloads
  wasm-bindgen, binaryen and esbuild, unpinned, as it builds. wasm-opt is
  9.2's.
  Bit-identical binaries are a goal checked in CI, not a dependency of
  incidents.
- **The stamp** (build id, commit, modified flag, compiler version,
  deployable, target) is a constant in the binary and on every log event.
  Built in chunk 8.1o for a native deployable (§2.6, "A native
  deployable"): the build id is `b_` and the first twelve hex digits of a
  SHA-256 over the graph's digest, the compiler, the target and
  `jadpo.lock`; `jadpo build` writes the toolchain file, builds
  `--locked` and offline, remaps the paths and fixes `SOURCE_DATE_EPOCH`.
  Since chunk 8.1o2 a declared deployable's declaration joins the graph's
  digest, so two deployables of one app never share an id, and `jadpo
  check` writes each deployable's target and build id, with the commit
  and whether the tree was modified, in the manifest. Since chunk 8.1o3
  the source is the deployable's program as it is compiled from what it
  reaches, its Rust and its plan's bytes, with its name and its stores'
  migrations (§2.6, "A deployable compiled from what it reaches").
  Bit-identical binaries are not checked yet.

### 2.9 Risks and the first chunks

| Risk | Guard |
|---|---|
| Interpreter and compiled code diverge | One plan, one runtime, codecs from the same type plans, the differential run |
| The runtime grows without seams | Components behind traits; capability features per deployable |
| salsa's breaking releases and advisories | Exact pin, two crates, upgraded in its own change |
| Workers' JavaScript APIs move | Only `jadpo-host-workers` binds them, through wasm-bindgen at the lock's release, proved under a pinned workerd (chunk 8.2a) |
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

### 2.11 The performance spike (chunk 8.3, measured 2026-10-22)

Teaching example 23's booking route, `POST /repairs` with a staffed bench
(validation, the booking, both subscribers run under `settle:`, the
ticket read back: 18 statements and 4 `BEGIN`/`COMMIT` pairs), built by
`jadpo build` natively and for Workers, against the same route
hand-written in Bun 1.2.20 with postgres.js 3.4.7 (the same validation,
SQL, transactions, rate limit and answer), plus the readiness check every
app answers. The harness, its method and its limits are in
[tools/bench/README.md](../tools/bench/README.md); it is not in the gate.
One local PostgreSQL 16 with `synchronous_commit` off; workerd 2026-10-07,
pinned, its Hyperdrive binding given to a stand-in transaction pooler
(`workers`) or straight to PostgreSQL (`workers-direct`); an M1 Pro, load
generator on the same machine. Medians of 3 rounds, 10 s measured after 2
s of warm-up; each throughput's rounds within 5% of each other but Bun's
at 16 connections (1,845 to 2,054).

| Booking | Bun | Wasm on workerd | Wasm, direct | Native |
|---|---:|---:|---:|---:|
| req/s, 1 connection | 977 | 270 | 147 | 837 |
| req/s, 16 connections | 2,052 | 513 | 450 | 3,554 |
| req/s, 64 connections | 2,248 | 436 | 408 | 3,756 |
| p50 / p99 ms, 16 connections | 7.6 / 10.5 | 27.3 / 116 | 32.1 / 106 | 3.6 / 17.4 |
| server CPU ms a booking, 16 connections | 0.50 | 1.98 | 2.25 | 0.81 |
| peak resident MB, 64 connections | 133 | 793 | 600 | 36 |

| Readiness check | Bun | Wasm on workerd | Native |
|---|---:|---:|---:|
| req/s, 16 connections | 81,247 | 17,445 | 131,013 |
| server CPU µs a request | 12 | 58 | 26 |

Cold start, spawn to first answer: native 38 ms, Wasm 73 ms (its first
request 40 ms of it), Bun 129 ms. Idle: native 13 MB, Bun 47 MB, workerd
134 MB.

Measured again after chunk 8.4c (2026-10-22), when deployables first
answered through the typed per-route entries, native and Wasm only, the
same harness and medians of 3 rounds, on a busier machine (load 3 to 12
while measuring, from other builds):

| Booking, after 8.4c | Wasm on workerd | Native |
|---|---:|---:|
| req/s, 1 / 16 / 64 connections | 215 / 407 / 388 | 807 / 3,493 / 3,508 |
| p50 / p99 ms, 16 connections | 29.0 / 132 | 3.65 / 21.3 |
| server CPU ms a booking, 16 connections | 2.26 | 0.79 |
| peak resident MB, 64 connections | 522 | 32 |

The readiness check at 16 connections: native 123,880 req/s at 27 µs a
request, Wasm 16,290 at 62 µs. Native is unchanged within the spread.
Wasm's medians are 10 to 20% under 8.3's, but its quietest round (496
req/s and 2.05 CPU ms a booking at 16 connections) is within 4% of them,
so the difference reads as the busier machine rather than the change:
reading through the entries moves neither noticeably, and the socket
bridge and the memory growth (8.4e) are still where Wasm's time goes.

- **Wasm against Bun**: 75% behind on the booking route's throughput at 16
  connections (4.0×), 72% at one; p50 3.6× and p99 11× Bun's; 4× the
  CPU a booking. Both are one thread here, saturated from 16 connections.
  Past the roadmap's 25% line. Measured again once after 8.2j's and
  8.2k's changes to the Workers host reached `main`: 502 against 2,091
  req/s at 16 connections, 2.02 against 0.49 CPU ms, the same.
- **Wasm against native**: a seventh of native's throughput, which uses a
  thread per core; per request, 2.4× native's CPU at 16 connections and
  7.5× at one (2.88 against 0.39 ms).
- **Bun against native**: native 1.7× Bun's throughput with every core;
  per request at one connection native uses 0.73× Bun's CPU but answers
  0.2 ms slower at p50 (1.16 against 0.96 ms), and above one connection
  its CPU a request rises past Bun's (0.81 against 0.50 ms at 16).

Where workerd's time goes (V8's CPU profile through its inspector,
booking at 16 connections): about half of the samples are the socket
bridge in `jadpo-host-workers`' `WorkersStream`, which takes a new
`ReadableStreamDefaultReader` or `WritableStreamDefaultWriter` for every
read and write and releases its lock after it (`write` 13%, `read` 9%,
`releaseLock` 10%, `queueMicrotask` 8%, the readers' and writers'
constructors, `connect` 3%), garbage collection 11%, and the runtime's,
tokio-postgres's and the allocator's own WebAssembly a few percent each.
The readiness check, which reaches no database, has no single hotspot:
garbage collection 14%, `Response` and `Headers` made across the
boundary, and the drivers built for every request, the connection's URL
parsed again each time, though nothing uses them.

**Memory grows with every booking.** The module's linear memory, read
from its exported `memory`, went from 5 MB to 253 MB over about 14,000
bookings (some 17 KB each, never given back), and workerd's resident
memory past 1 GB within a round; the JavaScript heap stayed under 30 MB
used. The readiness check alone doesn't grow it. On Cloudflare an
isolate is held to 128 MB, so this would end the isolate after a few
thousand bookings: not measured there, but a defect whatever the speed.

What these are not: workerd on one machine isn't Cloudflare's edge (no
network to Hyperdrive or the database, a stand-in pooler, one isolate on
one thread); the direct variant opens a PostgreSQL backend per request,
as `wrangler dev` does; the module is unoptimised (no `wasm-opt`).

**Decided** (Richard, 2026-10-22): Wasm is 75% behind Bun, and the gap
is not accepted. Chunk 8.4e finds and fixes the memory growth, holds one
reader and one writer for a socket's life and builds the drivers only
when a request first uses a store, then measures again with this
harness; Phase 9 closes what is left. Together the bridge and the
collector are about 60% of the CPU, so 8.4e alone would leave Wasm near
0.8 ms a booking against Bun's 0.5, still past the 25% the roadmap set.

**After chunk 8.4e** (measured 2026-10-22). *Why memory grew*: every
booking's PostgreSQL connection was left waiting for its goodbye. Once
the request's drivers were dropped, tokio-postgres wrote `Terminate` and
closed the socket's writable side, but by then the invocation had
answered and ended, and workerd never settles a promise of an ended
invocation, so the connection's task (its buffers, some 19 KB) waited
for the isolate's life: counted under workerd, after 900 connections
871 tasks were still alive, 858 of them waiting on that close. Smaller, a timer dropped before
it fired cleared its timeout and left its promise unsettled, keeping
wasm-bindgen's callbacks, and a socket's last read was never settled.
*The fix* (§2.6, "The Workers host"): a `WorkersHost`, once dropped,
keeps the invocation through `waitUntil` until its connections' tasks
have ended; shutting a stream down closes the socket and waits for its
last read; a dropped timer settles its promise. Also built: one reader
and one writer for a socket's life, and the drivers built when a request
first reaches a connection. `workers.rs` holds the kiosk's linear memory
to within 512 KiB over 400 bookings after 100 (it failed before, 7.5 MB
to 14.9 MB); the harness reads it after each measurement and, with
`--soak`, over a long run.

Same harness, medians of 3 rounds, the module as 8.4f builds it (link
time optimisation, `opt-level = "s"`), load average 4 to 10 for most
rows and up to 28 for some of the third round's (other chunks' builds).
"Before" is the same tree with `jadpo-host-workers` as on `main`,
measured straight after, at load 5 to 8:

| Booking | Wasm before | Wasm after 8.4e | Bun | Native |
|---|---:|---:|---:|---:|
| req/s, 1 / 16 / 64 connections | 232 / 470 / 399 | 242 / 609 / 565 | 978 / 2,061 / 1,260 | 575 / 2,494 / 2,900 |
| p50 / p99 ms, 16 connections | 29.5 / 127 | 24.8 / 46.2 | 7.5 / 11.1 | 3.5 / 52.5 |
| server CPU ms a booking, 16 connections | 2.15 | 1.64 | 0.50 | 0.74 |
| linear memory MB, after 1 / 16 / 64 connections | 60 / 161 / 249 | 10.8 / 11.7 / 14.1 | | |
| peak resident MB, 64 connections | 458 | 240 | 98 | 26 |

The readiness check at 16 connections: Wasm 15,084 req/s at 66 µs a
request before, 17,543 at 57 µs after. Bun at 64 connections and native
at one varied most with the load (1,104 to 2,250 and 129 to 657 req/s).
Over a long run, 348,337 bookings in 10 minutes at 16 connections (545
to 596 req/s, 1.68 to 1.81 CPU ms a booking), the linear memory stayed at
11.7 MB from the first minute to the last; workerd's resident memory
went from 250 to 306 MB, which this run doesn't explain (the JavaScript
heap wasn't read): still to be checked, in Phase 9 or on the real edge.

So 8.4e takes a booking's CPU on Workers down 23% (2.15 to 1.64 ms) and
its throughput up 30% at 16 connections and 42% at 64, and the module
no longer grows. Against Bun, Wasm is still 70% behind at 16
connections and uses 3.3× the CPU a booking; Phase 9 closes that gap.
Where the time goes now (V8's profile, booking at 16 connections, under
the inspector): the socket bridge's `write` and `read` themselves, about
a third of the samples, `queueMicrotask` 7%, garbage collection 4%,
Jadpo's compiled code a few percent; no reader's or writer's
constructor nor `releaseLock`.


**Chunk 9.1a instrumentation** (2026-10-22). `tools/bench/todo.py` adds a
local generic todo-shaped microbenchmark scaffold on native, workerd, Bun
and Node. It records complete response/state checks, p95 including requests
that drain past the admission window, finite request deadlines, cold start,
server CPU/RSS/Wasm memory, load CPU and source/artifact identities. Its fast
fault-sensitive checks are part of the gate; timed measurements are not.
The existing repair-café harness above remains reproducible and exercises a
subscriber path absent from these short routes.

An initial 16-cell smoke (four targets by four routes, one connection,
one-second admission and 0.2-second warmup) passed its instrumentation
checks. It was performed while build load was decaying and is not a
comparative performance result. Later write probes checked UUIDv7 responses,
owner/title persistence and BEGIN/INSERT RETURNING/COMMIT statement shapes.
That checkpoint published no final comparative data. Its generic title
ordering, bare list, cascade and public sample routes differ from the actual
golden todo; none of the 44 golden acceptance cases is claimed as passed.

The request-path audit in `tools/bench/README.md` found substantive baseline
gaps beyond the business SQL: full body/header/input guards, durable session
renewal/revocation, address canonicalization/refusal headers, deadline and
rollback safety, idempotency, response identity and complete error/retry
mapping. In particular the TypeScript scaffold validates returned write data
after commit, while Jadpo validates before committing. Both do runtime output
boundary validation: Jadpo validates stored fields and serializes a known
shape, while the TypeScript route performs one Zod output parse (the Hono
OpenAPI response schema adds no automatic second parse). Unicode scalar
boundary tests now prevent JavaScript UTF-16 length from changing text rules.

Roadmap 9.1 remains the original full benchmark requirement: 9.1a supplies
instruments only; 9.1d must implement and validate the actual golden todo and
strong TypeScript baseline before local app comparison, then 9.1b measures
runtime improvements and 9.1c measures real Workers with Richard. The short
scaffold is no evidence that the full application performance gap has closed.


**The golden application and contract** (chunk 9.1d2b). The ordinary source
in `examples/golden-todo/` implements owner-scoped todos, current provisioned
user/service authentication, omission-aware patches, keyset lists, bounded
relationships, soft deletion and generated retention. Private reminder
revisions distinguish rescheduling, including A→B→A. Queue marking and the
first event commit together; a preparation subscriber snapshots immutable
mail input, and a sending subscriber rechecks current eligibility and owner
status before the typed provider call. Only an acknowledged provider
`accepted_at` is persisted as the receipt; this does not prove inbox delivery.
The route table also carries sorted declared provider-connection names;
readiness reports them as `unchecked` without a network probe, including on
a store outage (areas 7 and 11). There are no compiler branches for the application.
Both targets pass all 44 cases using the integrated checkout’s compiler and
runtime; no missing control or case is counted as a pass. The checked source
is `415a33b557ff2f1cf373739c11edf0fed77cdc53` (2026-10-23), with the
checkout’s `target/debug/jadpo` and runtime, not a prerequisite-checkout
substitution. The compiler’s SHA-256 is
`f9ed2a55945c535409821aa79965f75fec8a15e1891e0750f9f9b23691b28d2e`;
the application/runtime/harness source fingerprint is
`26d74a11832534009016eac536df4ef706a0916ac6706adbc0e5a964c95cdf16`.
The full `python3 tools/verify.py` gate passed all 89 steps at that source,
including all 44 native and all 44 workerd cases against the gate’s shared
PostgreSQL server over its Unix socket. The histories of prerequisites
`86c0b010`, `d0ff1cb7` and `735c6d52` are integrated in that order under
Richard’s permission to build on unmerged branches; their review and merge
remain pending.

`tools/golden-todo/run.py` executes the same 44 IDs on generated native code
and generated Wasm under workerd. Its private host controls supply clocks,
provisioned signed credentials, SQL fixtures and deterministic provider
responses; raw malformed inputs still cross the production request decoder.
`record::Recorded` observes physical SQL calls, with transaction starts
reported separately, and the harness classifies each statement once as
principal authority, credential authority, business, runtime or transaction.
Response assertions include persisted state, request identifiers, privacy,
order, pagination, credential expiry and background failure correlation.
Reminder checks include concurrent runners, edits while dispatch is pending,
immutable lost-ack replay and an unsafe key window; retention runs the actual
generated schedule. Preflight checks use actual generated deployables and
successful-start positive controls. The public-host fixture interception is
not evidence about production DNS or TLS filtering.

The original `acceptance.json` bytes remain unchanged; `acceptance-v0.2.json`
retains every ID and records the current-area corrections in its README.
A missing target/control or failed case fails visibly. Completion requires
all 44 on both targets, no case filter, fresh source-stamped builds and this
checkout’s compiler/runtime. Explicit prerequisite-checkout previews cannot
establish completion. Full gate ownership is split between native in the
`compiled` CI group and workerd in `wasm32`, with shared compiled-test caches
under `target/jadpo-compiled/golden-todo`. Independent review, TypeScript
comparison and comparative timing remain separate roadmap work.


**The strong TypeScript golden implementation** (chunk 9.1d3).
`examples/golden-todo/typescript/` is a separate ordinary application, using
strict TypeScript, Hono, Zod, Drizzle, CASL, JOSE and a mature UUIDv7 generator.
Versions are pinned in its manifest and npm lock; framework/library
infrastructure, generated migrations and locks are distinct from authored
application source. Node and Bun use their production HTTP bindings. No
throughput, code-size advantage or equivalence follows from the earlier short
scaffold or from these implementation facts.

Route registration builds OpenAPI from the same Zod schemas that production
handlers explicitly parse at input and output boundaries. Decoding rejects
duplicate JSON members, unknown and unused
inputs, malformed text and excess body size; NFC scalar bounds apply before
business validation. Instants have millisecond precision and canonical UTC
text; input UUIDs accept every unrefined bit pattern while new Todo IDs are
UUIDv7. CASL and central SQL predicates enforce owner/self access before
pagination and bounded child queries. Mutation outputs are validated inside
serializable transactions, with real rollback and retry controls. Central
transaction context rejects provider IO inside a transaction. Request scopes
link admission, deadlines, native client disconnects and active SQL
cancellation, including pool waiters and CPU-only commit checks.

Authentication has a closed user/service principal, exact-one credential
selection, durable session renewal and bounded five-minute signed authority;
DELETE uses current authority. External JOSE verification reads current local
provisioning and never invents a user or permission from provider claims. A
valid unprovisioned external subject is `sign_in_refused` (403); malformed or
invalid credentials remain 401. Discovery/JWKS validation bounds documents,
key counts, fetch duration, cache lifetime and refresh frequency, including
failed and cancelled refreshes. Algorithm and public-key checks precede cache
use; matching-key rollover is exercised with locally generated signing keys.
Request/job errors and structured logs use explicit safe projections.

Optional write idempotency keeps a caller-scoped decoded-input digest and
stores response bytes/status atomically with the mutation. Replays preserve
the stored result across a changed output schema/status. Reminder revisions
use signed64 PostgreSQL `bigint` and JavaScript `bigint` arithmetic, with exact
canonical decimal JSON strings in queued/prepared work and provider payloads.
Preparation is immutable; sends recheck authority/revision and fence accepted
receipts, with stable provider keys, safe-window holds, durable retry budgets
and connection breaker state. The fixture supplies mail responses, including
lost acknowledgements; nothing sends real mail or proves inbox delivery.

`npm run inventory` derives routes, schemas, OpenAPI, data columns, foreign
keys and failures from the registered APIs. It distinguishes static TypeScript
or lint rejection, runtime guards, and test-supported behavior. Effect labels
are reviewable declarations, not a proof of arbitrary handler behavior.
TypeScript supplies no general information-flow or policy proof; the rate
counter's 16MiB accounting budget is not a proof about JavaScript heap overhead.
Local fixtures do not establish production public DNS/TLS or edge deployment.
The own-checkout shared run and full repository gate passed as recorded in
§2.7. Independent review, dependency merges and comparative measurements
remain separate obligations.


**Complete local application comparison** (chunk 9.1d4, 2026-10-23).
These are provisional, exploratory measurements on a busy local host. The
complete golden application passed all 44 current cases on native, workerd,
Bun and Node (176/176) before timing, from this checkout and its compiler.
The final source `ead8d2c889c3aac9677f0f91595b303e77bdaa89` passed the full
92-step gate, including real workerd ingress/configuration controls, the
strong TypeScript's 42 tests and all 88 Node/Bun contract obligations. The
TypeScript test configuration collects authored tests exactly once even when
production compilation has emitted JavaScript copies; its deadline-sensitive
pipeline runs after parallel builders, retaining every assertion and timeout.
Functional production-host smokes passed before the final gate. The timed
runs used that clean, frozen source and fresh, hash-checked artifacts; no
prerequisite checkout's compiler or test-controller transport was substituted.

The machine was an Apple M1 Pro, ten cores (eight performance), 16 GiB RAM,
macOS 27.0.1 arm64. Tools: Rust 1.99.0 (`b940084d7`), PostgreSQL 16.3,
Bun 1.2.20, Node v24.18.1 and workerd 2026-10-07. Ordinary `jadpo build`
release deployables used native opt-level 3 and Wasm opt-level `s`, fat LTO
and one codegen unit; no wasm-opt. The compiler was a dev build and was not
timed; the load/pool tools were release builds. Preparation and the gate
used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4`.
TypeScript was emitted by its pinned compiler and used Bun.serve or Hono's
Node binding. All HTTP went directly to actual loopback production hosts.
No builder or other owned benchmark ran during comparison. The five-second
pre-campaign host CPU sample was nevertheless 51.8%; Creative Cloud, Claude
and macOS indexing remained active. No unrelated app or service was stopped.

Three complete rounds rotated target order, with 1/16/64 connections, two
seconds of warmup and ten seconds of measurement per cell. Requests admitted
in that window were drained and included in latency and guarded throughput;
throughput's denominator includes the drain. This is closed-loop offered
load, not an open-loop latency SLO. All 108 golden cells and all 72 focused
cells passed, with zero transport, unexpected-status or body-validation
errors, including warmup. No timed cell was retried, replaced or omitted.
The complete per-run data retain p50/p90/p95/p99/max, statuses, drain, CPU,
RSS, linear memory, source/artifact identities and load measurements. The
largest drain was 0.231 s for golden and 0.081 s for focused reads.

Golden fixtures used 2,048 real signed owner sessions, plus a foreign owner:
104,499 seeded todos (50 visible and one hidden per owner including foreign),
2,049 users and 2,048 sessions. This fixed pool on every target avoids using
the normal 600/minute caller allowance as a throughput cap; the 120/minute
unsigned/refused-address limit also remained active. Credentials used the
production codecs, with four-minute bounded authority within the five-minute
limit. Signing and fixture preparation were outside timing. Every cell
started a fresh server, verified every owner's expected response and refusal
controls, then checked original rows unchanged and every successful write's
complete six-field response against its committed row and exact signed owner.
Across golden measurement windows there were 4,751,906 completed requests;
1,977,178 successful write receipts including warmup and drain were audited.

The copied app's native client-address setting declares one trusted forwarded
hop; the TypeScript host accepts one strict forwarded IPv4 value only from
an actual loopback TCP peer. Workerd uses the required Cloudflare source and
a local socket fixture that overwrites CF-Connecting-IP with the actual
127.0.0.1 peer; forged/missing header controls exercise the real runtime.
Its declared default rate-limit binding is present. Workerd also declares
required schedule/subscriber runners; the local workerd configuration supplies
no cron, while native and TypeScript use the external-tick fixture.
No due reminder was seeded and no job ran inside the request timings. All 44
cases exercise the reminder/lifecycle contract; the request throughput below
does not measure an entire reminder workflow or the 10,000-job backlog.

All targets used one disposable PostgreSQL with synchronous_commit off,
max_connections 400 and shared_buffers 256MB. Each applied its own migrations,
then an explicit benchmark-only physical-schema fixture normalised `todo` to
Jadpo's four business indexes (created/id, owner/created/id and status/created/id
partial on visible rows, plus owner), preserving constraint indexes. Drizzle's
natural migration has one nonpartial owner/created/id index; these numbers
therefore describe the common physical fixture, not natural schema-design
performance. Exact DDL and resulting index definitions are retained in raw
results; the DDL SHA-256 is
`a5f0b6916c43e063d652d02d29a7d08da176c8b1e3c118bd40af1442970f34ff`.

Pool architecture was left as built. Native logged ten server workers, each
with its own driver permitting ten open/eight idle connections; retained
sound idle connections have no timed expiry. Golden TypeScript has five
general connections plus five exclusive transaction clients, 20-second idle
and two-second connect timeouts. Golden workerd serves one isolate through
a local transaction-pool stand-in capped at ten connections, retained until
its fixture stops. This is not Hyperdrive query caching, edge latency or TLS.
The TypeScript hosts have one serving event loop, with runtime/helper threads
included in process CPU. Their process-core use is not a count of serving
loops. No production pool was rewritten to improve a result.

For the tables, each cell is **median req/s / median per-run p95 ms**, over
three runs. The p95 summaries are medians of per-run percentiles, not pooled
percentiles. Timed golden work is a signed exact six-field todo read, a
validated/committed UUIDv7 todo create, or the signed 50-row owner-filtered,
ordered visible list with full six-field projections and null next cursor.

| Golden target / work | 1 connection | 16 connections | 64 connections |
|---|---:|---:|---:|
| Native read | 5,177 / 0.299 | 21,576 / 1.515 | 24,396 / 7.007 |
| Native write | 4,963 / 0.334 | 17,278 / 2.174 | 18,435 / 9.958 |
| Native signed-list | 2,168 / 0.564 | 9,923 / 3.491 | 10,930 / 15.285 |
| workerd read | 794 / 1.752 | 1,540 / 15.431 | 1,553 / 69.712 |
| workerd write | 643 / 2.008 | 1,495 / 15.381 | 1,397 / 77.446 |
| workerd signed-list | 578 / 2.191 | 923 / 25.106 | 918 / 123.557 |
| Bun read | 1,828 / 1.093 | 4,815 / 5.170 | 4,357 / 20.435 |
| Bun write | 1,139 / 1.877 | 2,078 / 12.199 | 1,915 / 51.618 |
| Bun signed-list | 355 / 3.688 | 466 / 42.080 | 459 / 160.201 |
| Node read | 2,131 / 0.717 | 4,037 / 7.275 | 3,877 / 26.426 |
| Node write | 1,278 / 1.211 | 2,039 / 13.130 | 2,312 / 39.325 |
| Node signed-list | 211 / 9.534 | 263 / 77.579 | 261 / 284.918 |

The 64-connection spreads below retain all three runs, including unfavourable
ones. CPU/request, process cores and sampled RSS are three-run medians at
64 connections. RSS and Wasm linear memory use MiB (the raw keys say `mb`);
RSS is sampled every 250 ms, not a lifetime high-water measurement. End-of-cell
linear memory is not JavaScript heap or total workerd RSS.

| Golden target / work | req/s min–max | p95 ms min–max | CPU ms/req | Process cores | Sampled RSS MiB | Wasm MiB |
|---|---:|---:|---:|---:|---:|---:|
| Native read | 23,064–24,457 | 6.529–7.131 | 0.108 | 2.53 | 18.5 | — |
| Native write | 17,852–18,831 | 8.918–10.152 | 0.126 | 2.33 | 16.9 | — |
| Native signed-list | 9,885–11,200 | 14.069–16.434 | 0.343 | 3.66 | 19.2 | — |
| workerd read | 1,454–1,634 | 59.623–78.592 | 0.630 | 0.98 | 234.5 | 13.8 |
| workerd write | 1,362–1,567 | 63.734–91.295 | 0.682 | 0.97 | 257.2 | 14.1 |
| workerd signed-list | 867–1,011 | 111.391–126.570 | 1.013 | 0.93 | 258.9 | 14.2 |
| Bun read | 4,183–4,573 | 19.184–22.197 | 0.249 | 1.09 | 170.3 | — |
| Bun write | 1,820–2,135 | 39.492–52.194 | 0.625 | 1.20 | 155.6 | — |
| Bun signed-list | 458–459 | 149.702–175.030 | 2.355 | 1.08 | 143.5 | — |
| Node read | 3,538–4,461 | 20.520–33.275 | 0.318 | 1.23 | 277.4 | — |
| Node write | 2,280–2,332 | 37.312–39.358 | 0.524 | 1.21 | 282.4 | — |
| Node signed-list | 251–262 | 264.869–336.754 | 6.665 | 1.73 | 367.8 | — |

The golden workerd stand-in pool adds median 0.180/0.185/0.322 CPU ms per
read/write/list request at 64 connections, outside the workerd process figures.
Native's higher-concurrency cells used roughly the whole host once PostgreSQL,
the load generator and unrelated activity are included. Across golden cells,
whole-host CPU was 40.1–99.9% (median 62.1%) and start/end one-minute load was
5.77–27.79. Focused cells were 39.4–99.9% (median 56.2%), load 6.29–14.78.
The two-thread load generator used at most 0.787 process cores for golden
and 0.624 for focused reads. These CPU observations do not prove client or
host capacity headroom; the machine was not isolated, and whole-host
saturation limits attribution of throughput differences.

Golden cold start is spawn through the first fully valid database-backed
readiness response, with fixture signing and the external pool startup
excluded. There are 27 starts per target. Values below are median [min–max];
large outliers remain included, and this is not a controlled OS cold-cache test.

| Golden target | Ready ms | First request ms | Idle RSS MiB |
|---|---:|---:|---:|
| Native | 21.73 [18.38–385.34] | 3.16 [2.59–4.72] | 11.9 |
| workerd | 71.10 [69.45–350.50] | 39.44 [33.13–268.68] | 97.5 |
| Bun | 113.61 [102.19–432.03] | 16.06 [14.38–31.80] | 92.0 |
| Node | 463.56 [422.75–1364.96] | 33.05 [24.48–128.79] | 257.5 |

**Distinct large-row scaffold.** The golden title bound is 200 Unicode
scalars. The separate 9.1a scaffold measures public sample reads with stored
payloads of 100 and 65,536 bytes (64 KiB), plus their JSON envelope; it is not
the golden app or signed-list work. The `--principals 2048` command option
applies to the scaffold's signed workloads; these public routes deliberately
prepare one owner plus the foreign fixture, and retain normal unsigned
address admission through the declared bounded metadata fixture. Exact
sample bodies are checked. The TypeScript scaffold's pool and the local
workerd stand-in are each capped at 20, as in 9.1a; these are separate from
the golden pool configuration above. It completed 4,872,023 measured requests.
The same median req/s / median per-run p95 ms notation applies.

| Focused target / stored payload | 1 connection | 16 connections | 64 connections |
|---|---:|---:|---:|
| Native 100 B | 6,125 / 0.256 | 25,856 / 1.150 | 30,718 / 5.512 |
| Native 64 KiB | 2,304 / 0.579 | 10,506 / 3.239 | 10,637 / 18.446 |
| workerd 100 B | 882 / 1.625 | 1,637 / 13.298 | 1,624 / 70.488 |
| workerd 64 KiB | 421 / 2.986 | 819 / 33.742 | 822 / 122.786 |
| Bun 100 B | 4,638 / 0.338 | 10,772 / 2.463 | 9,712 / 9.809 |
| Bun 64 KiB | 2,235 / 0.716 | 4,449 / 5.899 | 4,750 / 17.854 |
| Node 100 B | 4,038 / 0.393 | 9,537 / 2.598 | 9,210 / 10.021 |
| Node 64 KiB | 2,023 / 0.737 | 4,625 / 4.943 | 4,687 / 18.805 |

For the focused 64-KiB route at 64 connections, req/s ranges were native
10,636–11,444, workerd 713–823, Bun 4,708–4,880 and Node 4,666–5,107;
p95 ranges were respectively 15.973–19.105, 116.279–159.705,
17.215–18.590 and 17.159–20.518 ms. Median CPU/request was
0.308/1.181/0.245/0.208 ms; median sampled RSS was
42.4/370.7/127.9/291.4 MiB, in that same order. Workerd's final linear memory
was 18.1–18.2 MiB. The complete smaller-read spreads and all other metrics
remain in the raw result, without choosing favourable rounds.

In these observations workerd was behind Bun on golden reads and writes,
but ahead on golden signed lists. There is no single overall performance-gap
number. Native's extra serving workers, pool differences and the saturated,
busy host also prevent treating the table as an isolated language comparison.
These short closed-loop windows do not establish production capacity, an
open-loop SLO, stable p99, edge performance or long-run memory behaviour.
The earlier unexplained outer workerd RSS increase from 250 to 306 MB in
the long repair-café run remains a specific runtime/edge investigation;
short-cell linear-memory observations here do not resolve it.

**Reproduction identities.** The methods and fault controls are in
`tools/bench/README.md`; comparison remains outside CI. Reproduce with fresh
preparation, then `golden.py --skip-build --principals 2048` and
`todo.py --skip-build --routes read,large --principals 2048` (the defaults are
the three rounds/connection counts/windows above). Current-source input
fingerprint for both artifact stamps is
`20e30cba0878710a98d38cd4c64639b6a3bd5584cece6fc36bf31947c493fe67`.
The raw, ignored results are `tools/bench/work/golden/matrix-busy-host-ead8d2c8.json`
and `tools/bench/work/todo/matrix-busy-host-ead8d2c8.json`, SHA-256 respectively
`2dbfaa2e830bbbda05b479ef783360c4775691fa4a50428eb0a31d26f356ace8` and
`25aa350f613547b84947a10d6da1de6bc5c3ba7084f9d9fbf523f2002cac6205`.
The failed setup-only functional smoke is retained separately and supplies
no comparison cell. Each result includes full source and artifact maps;
selected exact SHA-256 identities are below.

| Input or artifact | SHA-256 |
|---|---|
| `Cargo.lock` | `215fd087c4bf5cd03f21311e67f388883a7aba013af53a3445fa99aca01f4950` |
| `examples/golden-todo/typescript/package-lock.json` | `73933caaffe00f1ebabe4bbc05630e91eaa596e40ea8113228849aed0857d82c` |
| `tools/bench/load/Cargo.lock` | `cbe692030d0854b6e2927dd5b71641aed5e8b5771541a197b92c8a2e109dad38` |
| `tools/bench/typescript/package-lock.json` | `cf6506f6515e0029255285d0a94ae47cab4fd7213902f6c1c09347590df66316` |
| `tools/workers/package-lock.json` | `7649655efb232dcc01dc327a9b2830b4e9fabfffd47a16cb3e59b9e93bfa9453` |
| Compiler (not timed) | `ae662ebca57b28e314ed702782b8d48ac43455691a906faa06b1d42c765bb5c1` |
| Golden native (14,196,128 bytes) | `53904c09887ce6f7b19ef6a7319b76f17259099bcb2e37e1be739b2db23e5634` |
| Golden Wasm (7,302,702 bytes) | `9c04cf6aecbf3b519812e502d3d0e97275e17ccb0df0d87eb3120917c9573254` |
| Release load generator | `6a7cc932bcf970d7e59a8f44b8b8c8da3e0ea0351508dd8e17a4ef5b86811545` |
| Release pool stand-in | `b56ab11147b2bcf68cb05adee3e7f652629d2c3340a085ec2e129ac65df4646f` |
| Golden emitted TypeScript: 20-file relative-path/digest manifest | `36712c181369da46ad502a201a6426fdb13dd7900f4378b25ddefb889f1a542b` |
| Focused native | `6fcf5636e1d178ca7ff94815c116225388013631ca833e6108b969abcdb56a82` |
| Focused Wasm | `9e25734534b67b9eac4c79a34e19463ab6ae36c4b391670664229c47c29db7c3` |

The TypeScript manifest digest hashes sorted JSON of relative emitted `src/`
paths to their SHA-256 strings, identically for Bun and Node. Full maps also
retain the production shim/glue, fixture signer and configuration inputs.


**Bounded durable delivery sweeps** (chunk 9.1b1, 2026-10-23).
The hypothesis was that awaiting each due delivery serially left database I/O
unoverlapped. The generic runtime now partitions the same oldest-at-most-100
selection into four static round-robin lanes, each using the existing serial
`runner::drive`; the existing local draining join was shared with schedules.
The bound is four per invocation, including descendants; contexts are separate,
a batch's own batches remain sequential, and connection opening retains its
host guards (six on Workers). Claims, leases, retries, safe-key windows,
payload parking, atomic handled-record writes, shutdown hand-on and no ordering
promise are unchanged. Admission stops when the common budget/shutdown is
spent; admitted futures drain safely. No work stealing is promised: an uneven
lane can remain busy after the others finish. §2.6 records the implementation;
`tools/bench/README.md` records the executable method and comparator limits.

This adds a focused pure-database background workload, not the full golden
reminder workflow or HTTP list/read/write comparison above. Each cell drains
exactly 10,000 durable deliveries with 100 production ticks. Native includes
100 external tick process starts. Bun and Node also start one process per
tick, whereas workerd stays running across ticks. Their startup and pool
lifetimes therefore differ: the per-target serial-versus-bounded comparisons
remain matched, but the cross-target times do not establish a warm-service
Workers-versus-Bun win. Chunk 9.1b1a adds that missing warm-service comparison;
these original results remain retained with this limitation. Workerd invokes
the generated production scheduled handler through a local binding and the
existing transaction-pool stand-in capped at four; it is not real edge
Hyperdrive. The identical pinned
Bun/Node specialized fixture uses postgres-js pool four, UUIDv7 receipt IDs,
payload validation, conditional/fenced claims, and atomic receipt+handled
commits. It covers the reached delivery path, not generic language/provider/
auth/shutdown behavior. Jadpo tick housekeeping is included; the comparator
only drains deliveries. The separate strong golden TypeScript was unchanged.

The matched pair below uses the final UUIDv7 harness. Each side has three
rotated rounds across all four targets. Both campaigns passed all 12 cells,
all 20 negative/race controls and start/end source/artifact freshness guards:
240,000 measured deliveries in total. Every cell had exactly 10,000 receipts,
unique ticket matches, UUIDv7 IDs, handled markers, succeeded deliveries,
attempts and events; zero missing/unexpected effects or pending work; and an
unchanged joined delivery/event identity-and-payload fingerprint. After timing,
a deliberately reset replay cohort added 100 attempts without an extra effect.
Controls cover malformed payload parking, live and expired leases, concurrent
duplicate sweeps, and a refused handled-ledger insert after the receipt: both
writes roll back and the work remains pending/unavailable for retry. That
last case follows area6 atomicity and area12 infrastructure-fault retry; it
must not be confused with an authored receipt unique-conflict refusal.

Richard closed apps before this pair. Five-second pre-campaign whole-machine
CPU samples were still 21.8% busy before serial and 12.0% before bounded;
one-minute load was 15.94 and 3.97. No owned builds/tests ran during timing,
and unrelated processes were left alone. These are exploratory observations
on an M1 Pro (10 cores, 16 GB, macOS 27.0.1), not isolated capacity or a pure
code-effect estimate. Unchanged Bun and Node controls became slower, retaining
the adverse spread below. PostgreSQL was the same disposable local 16.3 setup
on both sides (`synchronous_commit=off`, 400 connections, 256 MB shared buffers).

Elapsed seconds are median [min–max] of three runs. Throughput is 10,000 divided
by median elapsed; p95 is the median of the three per-run nearest-rank p95s
of 100 tick durations, not per-delivery latency or a pooled percentile.

| Target | Serial 10k seconds | Four-lane 10k seconds | Deliveries/s serial → bounded | Tick p95 ms serial → bounded |
|---|---:|---:|---:|---:|
| Native Jadpo | 4.7549 [4.7515–4.7996] | 3.7219 [3.6303–3.9476] | 2,103 → 2,687 | 52.326 → 41.436 |
| workerd Jadpo | 7.7943 [7.7673–8.0264] | 4.3710 [4.3206–4.6490] | 1,283 → 2,288 | 91.737 → 52.968 |
| Bun control | 7.6728 [7.6566–7.7465] | 8.4539 [7.8349–9.4821] | 1,303 → 1,183 | 81.457 → 90.673 |
| Node control | 10.2185 [10.1988–10.2632] | 10.8426 [10.2872–10.8866] | 979 → 922 | 105.908 → 115.273 |

Native elapsed fell 21.7% and workerd 43.9%; unchanged Bun rose 10.2% and Node
6.1%. These observed shifts do not remove the host drift. Native process CPU
rose about 19% despite lower elapsed; workerd process CPU fell about 23%, with
a separate pool cost. Four lanes trade extra active work/connections and CPU
for I/O overlap; they are not a claim of an inherent language or edge win.
Every round is retained below (rounds 1, 2, 3, without selecting the fastest).
CPU seconds count the 100 tick children for native/Bun/Node, persistent workerd
for Workers, and its helper separately; PostgreSQL and Python CPU are excluded.
The scopes differ and must not be presented as total system CPU.

| Target / variant | 10k seconds, rounds 1/2/3 | Process CPU seconds, rounds 1/2/3 | Tick p95 ms, rounds 1/2/3 |
|---|---|---|---|
| Native serial | 4.7996 / 4.7549 / 4.7515 | 1.589 / 1.568 / 1.569 | 52.414 / 52.326 / 52.134 |
| Native bounded | 3.6303 / 3.7219 / 3.9476 | 1.810 / 1.859 / 1.917 | 41.209 / 41.436 / 45.363 |
| workerd serial | 7.7943 / 7.7673 / 8.0264 | 5.110 / 5.290 / 5.380 | 91.737 / 91.536 / 95.134 |
| workerd bounded | 4.3206 / 4.3710 / 4.6490 | 3.850 / 4.080 / 4.350 | 52.968 / 52.901 / 54.063 |
| Bun serial | 7.6566 / 7.7465 / 7.6728 | 10.612 / 10.735 / 10.628 | 81.407 / 81.457 / 81.663 |
| Bun bounded | 7.8349 / 9.4821 / 8.4539 | 10.828 / 12.096 / 11.576 | 84.588 / 103.950 / 90.673 |
| Node serial | 10.1988 / 10.2632 / 10.2185 | 11.642 / 11.678 / 11.603 | 105.908 / 106.094 / 105.835 |
| Node bounded | 10.2872 / 10.8426 / 10.8866 | 11.697 / 12.691 / 12.467 | 106.971 / 116.927 / 115.273 |

Pool CPU seconds were serial 2.06/2.06/2.07 and bounded 2.14/2.13/2.39.
Workerd RSS at cell end (not peak) was serial 225.9/240.5/237.9 MiB and bounded
232.4/242.2/239.8 MiB. All responses, per-tick samples, host CPU counters,
loads, state audits, controls and unfavorable rounds remain in raw JSON.

**Source and verification.** Full gate `python3 tools/verify.py` passed 92/92
in 486 s at `49760cc73f4d59715151e0a3b71ad4544a06762f`. This includes production
native/workerd behavior, strong TypeScript 42 tests plus shared conformance,
and the real PostgreSQL overlap/duplicate-claim and controlled budget/shutdown
regressions. The latter advance expiry/shutdown after four claims, without a
host-speed assertion, and prove no lost or double effects and later recovery.
The preexisting optional pgvector 0.7.0 versus required 0.8.0 example skip
remains a skip; no extension was upgraded. The preceding `6315c732` full gate
passed 91/92 but caught forbidden indexing in lane distribution. The scoped
move-only VecDeque fix passed targeted checks, Clippy and the final full gate.
Closed raw gate outputs are retained under `work/deliveries/diagnostics/`:
`gate-6315c732.log` SHA-256
`dac204b50e53c011c7eabea7cd5b0e0480b3d1d5f890fe4e22564066a1525a32`, and
`gate-49760cc7.log`
`1e0cb4279a287610e1610189695bcc0d67116fb66208f490142de56244fe073f`.

For the post-app-close reference, commit
`fbf98ad151a15b4fa884b1084206b733cc083f47` restored only `runner.rs`, `race.rs`
and `schedules.rs` from serial `67c3b4e049408f050e9c25f831d3d3bfd0185b9d`,
retaining the final UUIDv7 harness. Candidate
`59d9a853ec0d683815c539d4bc95657fd1e1c6b5` restored those files exactly from the
gated source. Its complete tracked Git tree (including modes/symlinks) equals
the gated tree: `2991c67691019972fde486c8f9c0674c6437d062`. The two timed source
maps differ only in those three production files. Each source was clean and
built its own compiler in this worktree before its campaign, with
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4`. Production
native/Wasm used ordinary release `jadpo build`, without wasm-opt; compiler
build CPU was excluded. Rust was 1.99.0, Node 24.18.1, Bun 1.2.20, local workerd
2026-10-07. Exact versions and complete source/artifact SHA-256 maps are in
each raw's embedded `build` and its label's `build.json`.

Both compiler binaries were preserved as their label's `compiler-jadpo`
before the next build overwrote `target/debug/jadpo`; `compiler-snapshot.json`
records original absolute path, source, snapshot digest and build-stamp digest.
All preserved artifact hashes were rechecked after timing. This historical
snapshot check does not bypass the current-HEAD/original-compiler guard or
claim the old stamp is reusable at a different HEAD.

| Artifact | Serial SHA-256 | Bounded SHA-256 |
|---|---|---|
| Own compiler, snapshot retained | `45d373372eab44099b46660fabcd460c335dcef4d0c7123d75e7018e50244ef0` | `3794afbd8c3181e9359b4bc71260cdeff71b97c0c406be91997ce0f4fccc2486` |
| Production native | `019ab612775e11aa6d5a0139ecf95708c1e23addcda2baa44d10383439f4ea71` | `2df19e67c7a2bbd20fbbba1abbc99ccca1264cbe1de3acc3c91b2da87ca55422` |
| Production Wasm | `2f171bb19a5f0ea76c30786162b9a4f5c20f28d692c35936ee602094f4c4ce4d` | `e68c01952609bbb19352e8a84fad07d242d7e9531dda908732fd8f643ea32287` |

The identical emitted Bun/Node fixture SHA-256 was
`2e63c203e79aac2a81cca3ad6ba7bf34f96c31e782c667b94a9f0f00edbc2e63`;
the identical final `deliveries.py` source was
`b1bcc61fbf2e6ce8fdc2033712ed6b3b220ac9a2fd36f676932a0ef16d2918ae`.

**Retained attempts and learnings.** Paths below are relative to ignored
`tools/bench/work/deliveries/`; the exact source is embedded in every raw.
The initial ledger-refusal control was corrected against existing atomicity/
infrastructure-retry semantics, then the local scheduled-binding configuration
was fixed. No failing setup row became an accepted comparison cell. The first
complete `before` protocol used TypeScript UUIDv4 receipts, unlike ordinary
Jadpo UUIDv7 creation (area5), so it is superseded, not a matched baseline.
`before-v7` corrected this with the already pinned uuid13 dependency. Its busy
host preceded Richard closing apps, so both serial and candidate were rebuilt
and measured again under the final unchanged protocol; old busy results must
not be used as the denominator for the later candidate. All attempts remain:

| Raw path | Source | Outcome | SHA-256 |
|---|---|---|---|
| `before/runs-1792762333743171000.json` | `46e4530c` | Refusal-control assertion failed; no accepted timing | `597a1c449ed2ee3ac1448b7ac2b75cb7f7ea2eaa94d697eb91fdaede0411f6d9` |
| `before/runs-1792762400986069000.json` | `9334c6af` | workerd Capnp startup failed; partial smoke only | `f26e1165e04596b70c4f939091bc7d747b62d7c3faeedb5ed89202fb2824f842` |
| `before/runs-1792762479356673000.json` | `ae5c3cc5` | Scheduled binding flag absent; partial smoke only | `73efd1a0dd21b6615b77cb6ce5ad919a68224ebc192b8bb47ea39149e393448c` |
| `before/runs-1792762518123744000.json` | `91ab7ba2` | 100-delivery smoke passed, superseded v4 | `571353cf3e6cbd166378c92f705fcc36965b47f5d9c280abb2ece56564152fd6` |
| `before/runs-1792762535431910000.json` | `91ab7ba2` | 12×10k passed, superseded v4 | `4e4d759ab789a8da2e213cf0dd24e64b9dbde750e3aa6e54d8afdb34697133b1` |
| `before-v7/runs-1792763080662053000.json` | `67c3b4e0` | 100-delivery smoke passed, v7 | `9ad597ff62f8656425e9f71a76082251bd64ac1e5661a10fdc6f9cb2b655dfa8` |
| `before-v7/runs-1792763090386670000.json` | `67c3b4e0` | 12×10k passed, earlier busy host | `46f40e333f6a8751629e720213c57b8a2073c8f017a7d5cf7d1034b9ee4d69fb` |
| `after-app-close-serial/runs-1792767570261986000.json` | `fbf98ad1` | 12×10k passed, matched serial | `e23c33f8ff0489286fb84b0ecae9a2beac5e92264a8ad9fa1314baa1868f7f6e` |
| `after-app-close-bounded/runs-1792768280720347000.json` | `59d9a853` | 12×10k passed, matched bounded | `65bc3f8a303aa9804a9932ebbcce78502bbbf78599705e66e6d73998a30fc8d4` |

Historical median 10k seconds, native/workerd/Bun/Node respectively, were
6.7696/12.5620/10.9516/13.7936 for superseded v4 and
7.9817/13.8136/12.5133/15.6958 for the earlier busy v7 campaign. Their complete
per-round CPU/spreads and failures stay in those raws; neither supports a
percentage claim against the post-app-close candidate.

To reproduce, check out the exact serial or candidate revision in this same
worktree, ensure it is clean, then run `python3 tools/bench/deliveries.py build
--label <fresh-label>` and `python3 tools/bench/deliveries.py measure --label
<fresh-label>` (default three rounds, 10,000 deliveries, all four targets).
Finish and stop all builds/tests before measuring; preserve that compiler and
label artifacts before preparing the next revision. Never reuse a stale stamp
across HEAD/source/compiler changes. `-O`/`PYTHONOPTIMIZE` is rejected at entry,
with a subprocess negative check, so disabled assertions cannot authorize data.
All owned tick children, workerd, pool stand-ins and disposable PostgreSQL
servers were stopped; no shared server/database or unrelated app was changed.
Review must retain the four-lane bound, separate contexts, serial batch work,
drain/hand-on behavior and exact handled atomicity, and version any altered
workload/ID/provenance protocol. These measurements support continuing the
remaining generic runtime work; they do not establish the cause of the wider
Wasm gap or replace profiling of the separate typed-execution work in 9.2.


**Warm durable delivery comparison** (chunk 9.1b1a, 2026-10-23).
Local workerd's four-lane runtime delivers 2,239/s against warm Bun's 4,017/s on
the database-only backlog: 56% of Bun throughput, with median tick p95 55.206 ms
against 29.479 ms. Serial workerd was 1,155/s and 107.064 ms. Bounded workerd's
elapsed time fell 48.4%, but it still takes 1.79 times Bun's time. This corrects
the earlier cross-target interpretation: 9.1b1's fresh Bun/Node processes charged
startup and pool creation on every tick while workerd stayed alive. That method
establishes each target's serial/bounded change, not a warm-service Bun win.
Both the original protocol/results above and `--mode external` remain available.
These are local workerd background-delivery measurements, not real Workers,
HTTP request latency, per-delivery latency or a full application comparison.

The hypothesis here was that matching service lifetimes would reveal the actual
remaining workerd/Bun gap. No production runtime/compiler change was added.
`typescript/deliveries.ts` now serves ordinary HTTP ticks in a persistent process
on both runtimes, using the identical `node:http` compatibility host and pool
four; this is a specialized comparator, distinct from the strong golden Bun
app's `Bun.serve` host. Each invocation gets a fresh clock, 16-minute lease,
cursor and counts; the same UUIDv7, payload validation, selected-100 bound,
conditional/fenced claims, 10-attempt/24-hour window and atomic receipt/handled
write are retained. Concurrent invocations share the bounded pool. Failed ticks
drain admitted lanes before answering; owned processes are stopped/reaped on
errors or HTTP deadlines, and graceful service shutdown closes the pool.

Workerd retains its production local HTTP→scheduled binding and separate
four-connection pool stand-in. Every persistent service restarts per cell,
receives an excluded empty readiness tick, then a real verified 100-delivery
warmup cohort before reset to exactly 10,000 measured deliveries. Thus process
startup and ordinary pool/JIT initialization are outside timing on both sides.
Controls precede the first warmup for each target. Native has no equivalent
ordinary HTTP tick service here: its external `app tick` starts, even after
its excluded warmup cohort, are included and reported only as supporting context.
Jadpo includes housekeeping; the specialized TypeScript fixture only drains
deliveries. This comparison does not establish generic auth/provider/language
or shutdown equivalence. No real providers, keys, accounts or deployments ran.

Both campaigns passed all 12 cells, all 20 malformed/rollback/live-lease/
expired-lease/duplicate-sweep controls, and strict start/end source/artifact
freshness. All 240,000 measured deliveries had exact receipt ticket sets,
UUIDv7 IDs, handled/succeeded/attempt/event counts, zero pending/missing/unexpected
effects, unchanged joined identity/payload fingerprints, and a replay cohort
adding 100 attempts without effects. The real HTTP/PostgreSQL regression tests
also prove PID reuse, later-due work and later timestamps across ticks, fresh
cohorts, SQL-error recovery and pool cleanup on Bun and Node. They run in the
full gate's own isolated step; lint/quick remain free of live services.

The host remained busy: M1 Pro, ten cores, 16 GB, macOS 27.0.1. No owned
build/test ran during either campaign. Five-second ambient CPU was 10.45% busy
before serial and 16.97% before bounded; one-minute loads were 4.73 and 4.67.
Whole-host per-cell counters are retained; the adverse serial Node cell saw
64.58% busy and is included. Unchanged Bun median elapsed rose 0.12%, Node
3.01%; these controls and all-round spread limit pure code-effect claims.
PostgreSQL 16.3 settings match 9.1b1. Rust 1.99.0, Bun 1.2.20, Node 24.18.1 and
pinned workerd 2026-10-07 were unchanged. Builds used
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4`; ordinary
native/Wasm release `jadpo build`, without wasm-opt; compiler build CPU excluded.

Elapsed is median [min–max] seconds of three rotated rounds; throughput is
10,000/median elapsed [min–max round throughput]. Tick p95 is median [min–max] of each round's
nearest-rank p95 over its 100 tick durations. Native's lifetime differs.

| Target | Serial 10k seconds | Bounded 10k seconds | Deliveries/s serial → bounded | Tick p95 ms serial → bounded |
|---|---:|---:|---:|---:|
| Local workerd Jadpo, warm | 8.6557 [8.3320–9.0474] | 4.4663 [4.4212–4.6057] | 1,155 [1,105–1,200] → 2,239 [2,171–2,262] | 107.064 [94.337–109.694] → 55.206 [53.525–60.498] |
| Bun control, warm | 2.4864 [2.4125–2.5130] | 2.4893 [2.4671–2.6585] | 4,022 [3,979–4,145] → 4,017 [3,762–4,053] | 29.096 [28.555–30.689] → 29.479 [28.676–30.942] |
| Node control, warm | 2.6868 [2.6544–3.7901] | 2.7676 [2.6816–2.9654] | 3,722 [2,638–3,767] → 3,613 [3,372–3,729] | 33.390 [30.197–50.443] → 33.479 [32.743–34.285] |
| Native Jadpo, external ticks | 5.1046 [5.0771–5.1303] | 3.8295 [3.8144–3.9550] | 1,959 [1,949–1,970] → 2,611 [2,528–2,622] | 56.309 [55.700–56.850] → 45.841 [44.089–47.899] |

CPU seconds, median [min–max]:

| Target | Process CPU serial → bounded | Separate pool CPU serial → bounded |
|---|---|---|
| workerd | 5.44 [5.44–5.59] → 4.02 [3.89–4.13] | 2.22 [2.16–2.23] → 2.28 [2.24–2.36] |
| Bun | 2.36 [2.29–2.37] → 2.35 [2.34–2.48] | — |
| Node | 2.52 [2.52–3.01] → 2.59 [2.52–2.76] | — |
| Native external | 1.686 [1.674–1.692] → 1.915 [1.905–1.944] | — |

Every round follows (1/2/3), including the slower Node run. Process CPU for
warm services includes their in-process pooling; workerd's separate pool is
listed explicitly. Combined workerd+pool median CPU fell 7.67 [7.60–7.81]→6.26 [6.17–6.49] s (18.4%),
while median pool CPU rose 2.22→2.28 s. Native tick-child CPU rose 1.686→1.915 s
(13.6%) despite lower elapsed. PostgreSQL and Python CPU are excluded; these
are server-process costs, not total system costs. Workerd RSS at cell end
was 229.3/240.0/241.2 MiB serial, 234.7/245.3/243.0 MiB bounded (not peak).

| Target / variant | 10k seconds | Deliveries/s | Tick p95 ms | Process CPU seconds | Separate pool CPU seconds |
|---|---|---|---|---|---|
| workerd serial | 9.0474 / 8.6557 / 8.3320 | 1,105 / 1,155 / 1,200 | 107.064 / 109.694 / 94.337 | 5.44 / 5.59 / 5.44 | 2.23 / 2.22 / 2.16 |
| workerd bounded | 4.4212 / 4.4663 / 4.6057 | 2,262 / 2,239 / 2,171 | 53.525 / 55.206 / 60.498 | 3.89 / 4.02 / 4.13 | 2.28 / 2.24 / 2.36 |
| Bun serial | 2.4125 / 2.5130 / 2.4864 | 4,145 / 3,979 / 4,022 | 28.555 / 30.689 / 29.096 | 2.29 / 2.37 / 2.36 | — |
| Bun bounded | 2.4671 / 2.4893 / 2.6585 | 4,053 / 4,017 / 3,762 | 28.676 / 29.479 / 30.942 | 2.34 / 2.35 / 2.48 | — |
| Node serial | 3.7901 / 2.6544 / 2.6868 | 2,638 / 3,767 / 3,722 | 50.443 / 33.390 / 30.197 | 3.01 / 2.52 / 2.52 | — |
| Node bounded | 2.6816 / 2.7676 / 2.9654 | 3,729 / 3,613 / 3,372 | 32.743 / 33.479 / 34.285 | 2.52 / 2.59 / 2.76 | — |
| Native external serial | 5.0771 / 5.1046 / 5.1303 | 1,970 / 1,959 / 1,949 | 55.700 / 56.309 / 56.850 | 1.674 / 1.686 / 1.692 | — |
| Native external bounded | 3.8295 / 3.9550 / 3.8144 | 2,611 / 2,528 / 2,622 | 45.841 / 47.899 / 44.089 | 1.915 / 1.944 / 1.905 | — |

**Source, verification and reproduction.** Full gate passed 93/93 in 982 s at
`f1e1e045ccd0d0be7c0795ddf83291ae325dbaf6`. Serial
`cc4563ce1b39ab0b65e1dc65b165a0c65c14c762` restored **only**
`crates/jadpo-runtime/src/{runner,race,schedules}.rs` from
`67c3b4e049408f050e9c25f831d3d3bfd0185b9d`, retaining the final harness/tests.
Bounded `cf9c443b026f7cd9155c224e153430c13d6e8b1f` restored those files from the
gated candidate. Its **entire** tracked Git tree, including modes/symlinks,
equals gated tree `cf3fc3b115bd2a45d949fac939394219b09691a5`; `git write-tree`
and a complete `git diff` proved equality before building. The two timed source
maps differ only in those three runtime files. TypeScript's generated artifact
is identical, SHA-256
`66977add90729e3a8ea7535bd1ed8becd3f6dcfbc9bfc8df79e900ce8f325d91`.

Each label built its own compiler and retains `compiler-jadpo` beside its
`build.json`, which names the original `target/debug/jadpo` path, source,
flags and complete source/artifact hashes. Serial compiler SHA-256 is
`ae82452b7505dc916713002556b95d949e0988d840e0ca216c3e061a764bc960`;
bounded is `28c630307d1f035d5f5c7a7fc218bea891a6b35b3d4cd9a39c618635edf22ac7`.
Their build-stamp SHA-256s are respectively
`c3cb695e5cba661dbc711e0aff009b5124e666b41f142501703a71ef872a38f5` and
`fcc9903da5692ce7b027086e050bcad27a0c92827887e003c17a6bb341eb0a09`.
All paths below are under
`/Users/richardwillars/Documents/Codex/LLM-coding-chunk-9.1b1a/tools/bench/work/deliveries/`.

| Raw path | Outcome | SHA-256 |
|---|---|---|
| `warm-serial/runs-1792776205096359000.json` | 4×100 smoke and 20 controls passed | `9e486278ae1f7b74a676170e00b561a93fc0f3368c87b34ca6e8c03a664f4aec` |
| `warm-serial/runs-1792776228246358000.json` | 12×10k serial, matched warm protocol | `64c5f376fe0252cbd98151bfd0c74463d4bb75e1763660b6a63d6de7712beff3` |
| `warm-bounded/runs-1792776755285015000.json` | 12×10k bounded, matched warm protocol | `e56731cf87d59760ac4faf8549c33693461bdc2847e7af9b2f8b0712de5fa417` |

No timed campaign failed or was replaced. An initial gate at `cda60ea6` was
interrupted after review found that new test orchestration assumed URI DSNs
and ran live checks in lint. The correction reuses the shared normalizer for
keyword/socket DSNs, maps explicit socket hosts into postgres-js options,
preserves quoted endpoint/startup parameters, and places real service checks
in an isolated full-profile tests step. Direct TCP and actual gate keyword/
socket controls passed before the final gate; no failed preparation authorized
timing. `gate-interrupted-cda60ea6.log` SHA-256 is
`87a6991cf4ef31ff4ac024d88f3410ab8fb87f812b1e904f597d9372b7822a6c`;
`gate-final.log` is
`e3f93a127c61bc266b3873195e6213d3c79d22a9a5e4d632c7016aaa3e1949db`.
The existing optional pgvector 0.7.0 versus required 0.8.0 skips remain explicit;
no extension was upgraded. All owned services, helpers, tick children and
PostgreSQL servers stopped; their retained artifacts/raws remain available.

To reproduce, use each exact clean timed revision above in its worktree, run
`python3 tools/bench/deliveries.py build --label <fresh-label>`, finish every
build/test, then `python3 tools/bench/deliveries.py measure --label <fresh-label>
--mode warm` (default three rounds, 10,000 deliveries). Preserve the compiler
snapshot/stamp/raws before changing source. Method/fact edits followed both
campaigns and their end-freshness checks; these old stamps must reject the new
documentation HEAD. No historical override weakens the guards. Review must keep
process lifetime/warmup explicit alongside Workers-versus-Bun results, retain
all adverse rounds, and rerun an affected matched comparison if measured paths
change. The lesson is to match the service boundary and pool lifetime before
claiming a cross-target win; the measured remaining workerd gap still needs work.
