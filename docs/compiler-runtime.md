# Jadpo compiler and runtime architecture

**Status:** the build approach is decided (§1.1–1.2, 2026-10-13); the
architecture (§2) was written in roadmap chunk 0.4 and waits for Richard's
read before Phase 1.

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

**Today (read):** the compiler builds a semantic graph
(`build_semantic_graph` in `jadpo/crates/semantic/src/lib.rs`) of
declarations, call edges, refinements and external effects, keyed by name,
with no control flow inside callables. It feeds diagnostics, the language
service and approval reachability, which is why the approval report calls
its paths "may-call, not feasible". Code generation (`core/src/target.rs`,
about 14,000 lines) reads the syntax tree directly and consults the graph in
about ten places. So the graph and the generated program are two separate
readings of the source.

**Options:**

1. **Graph as a by-product (today).** No rewrite, and golden-todo progress
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

**Proposed:** option 2, built as a new compiler core rather than by moving
the existing generator over. Not yet decided: the owner will decide once
the language review (roadmap item 2) is finished, before any build work
starts.

Why a rebuild rather than a migration (read, 2026-10-13):

- The language review's decisions for areas 1–4 alone change nearly every
  syntax form, so the parser and type checker would be largely rewritten
  in rework anyway.
- The generator reads the syntax tree, the opposite of option 2; writing a
  generator for the graph is less work than moving 14,000 lines onto it.
- The compiler is tuned to the golden app: the formatter hard-codes the
  reminder payload fields (`core/src/formatter.rs`, `todo_title`,
  `due_at`), and only the golden service is accepted, matched line by line.
  The passing golden cases overstate what works.

What the rebuild keeps: the language specifications, whose compiled
examples and error coverage (`tools/language_docs.py`) become the feature
tests; the golden contract and `tools/verify.py`; the diagnostics
catalogue; and the existing runtime (outbox and delivery, JWT
authentication, migration identity) as reference design. The old compiler
stays in the repository as reference until the new one passes the golden
todo, then is archived.

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
| `jadpo-std` | Shared leaf: built-in types, exact numbers, value rules and their validators, the standard library's enums Rust interprets, the typed operation table | yes |
| `jadpo-diag` | Rule catalogue, diagnostic model, edits | |
| `jadpo-syntax` | Lexer, parser, lossless syntax tree, typed AST, `jadpo.ungram` | |
| `jadpo-fmt` | Formatter | |
| `jadpo-graph` | Node and edge model, identity keys, canonical JSON, diff | |
| `jadpo-db` | Incremental database: inputs, project layout, packages and lock, identity registry, the before side from git | |
| `jadpo-check` | Resolution, types, failures, effects, policy, data and layout rules, as queries that build the graph | |
| `jadpo-plan` | What both executors need besides control flow: SQL per engine with policy compiled in, codecs and validators per type, route, schedule and subscriber tables, system-table placement, migration diffs | |
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
(PostgreSQL only) and `jadpo-host-native` in chunk 3.1 (§2.6).

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
crates, against these rules, and on salsa anywhere but `jadpo-db` and
`jadpo-check`. *Rejected:* v0's single `core` crate, which grew to
everything and let the generator read the syntax tree.

**The standard library** (`collection`, `temporal`, `decimal` and the rest)
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
is still checked, so a `List<T>` parameter takes only a list. Enums that Rust code interprets (`Rounding`, `TextFormat`,
`UuidVersion`, `Weekday`, and `Zone` and `Locale` from their tables) are
declared once, in `jadpo-std`. The namespaces the compiler fills from the
project or the platform (`clock`, `config`, `translations`, `sessions`,
`oauth`) list in `std/names.txt` the members built so far (`clock.now`);
any other member is `toolchain.not_built_yet`.

**wasm32.** The runtime crates and the shared leaf build for
`wasm32-unknown-unknown`, checked by the gate, which reads the list from
the layer table, less the crates `[workspace.metadata.targets]` lists as
native only (`jadpo-host-native`); `jadpo-drivers` builds with every driver
feature on. `rust-toolchain.toml` pins the target with the toolchain. Compiler crates do no I/O below `jadpo-db`, which
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

**Formatter.** Syntax tree to a document algebra to a printer, owning
indentation, blank lines and member order (areas 1 and 14). Every example
is checked to be idempotent and to parse to the same AST after formatting.
Since chunk 1.9 the formatter also reads its own output back: when it
doesn't parse to the same tokens, nodes and comments, or doesn't format to
itself, it writes nothing and reports `format.unsafe`, a formatter mistake;
a fuzz test feeds it the parser fuzz test's edits of every example.

**Item trees.** Each file is summarised into its declarations and
signatures without bodies (rust-analyzer's `ItemTree`), so editing a body
never invalidates another file's resolution.

**Resolution.** One project-wide name table, because names are unique
across the project and across kinds (area 1). A dotted prefix resolves to
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

**Types.** Nominal and bidirectional: the expected type flows into
literals (area 2). Widening is the only implicit conversion; the standard
library's signatures are the only generics. An `Error` type absorbs
anything built from it, so one mistake gives one error. Body checks
follow: definite assignment, unused variables, `attempt` placement, failure
sets, effect rules per callable kind, reads in loops, one store per action,
calls before writes.

**Diagnostics.** The catalogue is a `rules!` table in `jadpo-diag` (rule
id, owning area, severity, summary, who decides, help anchor), used through
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
| `effects` | callable | its body or a callee's effects change |
| `plan` (SQL, codecs, policy) | query, write site, type | its inputs, entity or policy change |
| `graph`, `diagnostics` | project, file | anything above; assembly only |

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
itself in 12 ms. The test asserts the counts, not the times.

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
(area 6), commits, then delivers the outbox and clears caches. Limits
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
commits. The second of area 6's budget waits for the host's clock, and
caches come with area 5.

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
arguments first, so only the call's own failures reach their arms. Effects later areas build (an entity's
operations, queries, `emit`, `clock.now`) end in the `not_built_yet`
fault where they are reached. A call walks the Rust stack a few frames per
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
edges (an action, query or fixture makes it an action test), picks them by
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
places of the test database and fakes, empty until chunks 3.1 and 7.2) in
a context it makes; tests run on a thread per core, each with its own view
of the one prepared program, and their results come back in the seed's
order. The trace, fakes, the test database and clock control come with
their phases, and route and schedule tests (`--kind route`, `--kind
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
between transactions, each transaction at repeatable read, every statement
unnamed (`query_typed`, each parameter's type taken from its value), and
its own binary codecs for the runtime's types. One function classifies
errors by SQLSTATE: unique, foreign-key and exclusion violations are a
conflict with the server's constraint name; serialisation failures and
deadlocks a concurrent change; connection exceptions (class 08), shutdowns
(57P01–57P03), too many connections, a closed connection and any I/O
failure unavailable; anything else a fault with its SQLSTATE and message.
Connections are unencrypted until TLS is built. `jadpo-host-native` gives
`Runtime` (one current-thread tokio runtime with a `LocalSet`),
`NativeTransport` over tokio's TCP and Unix sockets, and `postgres(url)`.

**Single-threaded by design.** Runtime futures need not be `Send`. A
Workers isolate is single-threaded anyway; natively, `jadpo-host-native`
runs one current-thread tokio runtime per core, each with its own
connection pools, behind one listener. One code path serves both targets,
with `Rc` instead of `Arc` and no locks on the request path. *Rejected:*
multi-threaded tokio with `Send` bounds switched off on wasm32, which
doubles the async signatures and lets bugs hide on one target.

| Component | Does | On Workers |
|---|---|---|
| HTTP | Router from the route table, input decoding, body limit, deadline, health, failure envelope, `request_id` | fetch event |
| Identity | Sessions, CSRF, API tokens, service keys, OIDC sign-in, revocation delay | same |
| Policy | Principal, tenancy and memberships; predicates arrive compiled in the plan's SQL and the runtime binds the caller | same |
| Transactions | Repeatable read, revision checks, rerun within the budget | same |
| Idempotency | Keys and stored responses in the action's transaction | same |
| Outbox | Delivery, retries, circuit breaker, dead letters; native delivery workers | `waitUntil` after commit, swept by the minute tick |
| Scheduler | Due times in the schedule's state row from its first tick, leases, run records, tick adapters; native timer | Cloudflare cron |
| Rate limits, caches | Redis or bounded memory; cache reads with timeout, invalidation after commit | Redis or a binding required |
| Telemetry | Spans per declaration, metrics, path numbers, threshold sampling, OTLP | export in `waitUntil` |
| Email | Templates, translations, the `email` capability | SMTP on 587 or a package |
| Flags, privacy | Evaluation; export and erasure flows | same |
| Development | Request timeline, local inbox and disk, published-event log | none |

**Capability drivers.** One trait per capability: `Database` (begin,
query, execute, commit, rollback), `Cache`, `Email`, `Sms`, `Files`,
`Publishing` and `ScheduleTick` (ticks and an optional lease). Drivers
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
  every shared and runtime crate to the lint (chunk 1.9).
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
both with `--seed 1`; `language_docs.py --export-corpus` writes each
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
reads its parse trees, graph and typed bodies, and fails naming any built
feature no example uses. The features come from the compiler's own tables
(the grammar's node kinds, graph node and edge kinds by status and area,
type bases, constraints, input field settings, failure members and
declarable kinds, the copying lines, the operation table and the enums its
operations take, the generated namespaces' built members, the layout's
root files and folders) and from exhaustive matches over the typed body;
an exemption names a feature that can't appear in runnable code yet, with
its reason, and fails once an example uses that feature. Every chunk adds
or extends examples for what it builds.

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
(`crates/jadpo-test/tests/postgres.rs`) share one server per test binary
and create a database each; `JADPO_SKIP_POSTGRES=1` skips them, as the
gate's `compiler tests` step does, and the `live PostgreSQL` step runs
them. CI runs them against a `postgres:16` service named by
`JADPO_TEST_DATABASE_URL`. Migrations build one template per migration digest; per-test copies
(`CREATE DATABASE … TEMPLATE`) are prepared in the background (area 13).

**Example-name ban.** `tools/example_names.py` extracts every declared name
from the language pages' code blocks and from `examples/` into the
committed `tools/example-identifiers.txt`, minus an allowlist of generic
words (`id`, `status`). The gate fails if the list is stale, or if a listed
name appears in any case style as an identifier or string in
`crates/*/src/`. An example entity whose name is an allowed word (`Order`,
`Session`) goes to `tools/example-type-names.txt` instead, and may not stand
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
| 3 | Live PostgreSQL (`--profile quick` skips it) |
| 8 | The differential run; the golden todo built for wasm32 and smoke-tested in local workerd |
| 10 | `--require-golden`: every golden contract case on both targets |

**CI.** One GitHub Actions job on Linux runs `python3 tools/verify.py` with
a PostgreSQL service, the pinned toolchain and a cargo cache. Benchmarks
(Phase 9) are a separate command, not the gate.

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
| PostgreSQL from Workers, Wasm slower than Bun | Hyperdrive with caching off, unnamed statements and repeatable read, and typed codecs, measured in the performance spike (chunk 8.3), before the Phase 9 benchmarks |
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
