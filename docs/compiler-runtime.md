# Jadpo compiler and runtime architecture

**Status:** exploratory architecture  
**Goal:** preserve implementation conclusions without prematurely committing to
a production compiler design

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

### 1.1 Graph-centred compiler (open, 2026-10-13)

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

**Proposed:** option 2, moved over one declaration kind at a time (routes
and actions first, because spans and path IDs need them), starting before
roadmap item 3's rework reaches the generator, so that rework is done once.

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

## 2. Toolchain language

Rust was the leading suggestion for the core toolchain because compiler tooling,
safety, performance, and a possible native-runtime future all fit it well. Zig
was raised as another possible systems language but not explored in detail.

This is provisional. The first experiment need not prove that a production Rust
compiler can be built; it must prove the programming model.

## 3. Execution strategy

### 3.1 First practical target

A pragmatic initial route is:

- Rust (or another suitable implementation language) for parsing and semantic
  checks;
- generated TypeScript as an implementation artifact;
- Bun or a similar runtime;
- Postgres as the database;
- an intentionally boring generated runtime.

This avoids rebuilding an HTTP server, database driver, TLS, observability,
queues, and deployment ecosystem before the hypothesis is validated.

The first prototype could be interpreted rather than compiled. Native code,
performance, and LLVM are not proof of the design.

### 3.2 Possible long-term target

A native executable with Postgres as its main infrastructure assumption would
give the language a cleaner identity and deployment story:

```text
backend build
  -> myapp
  -> migrations
  -> OpenAPI
  -> docs
  -> audit
```

This may be more elegant and commercially distinctive, but it is a later
technical problem. The initial reason to switch is correctness, constrained
evolution, and agent safety—not performance.

### 3.3 Scheduled Wasm target experiment

On 2026-09-29 the project owner scheduled
[WASM-EXP1](archive/implementation-history.md#wasm-exp1--bounded-wasm-runtime-experiment)
after authentication. The owner subsequently clarified the order: finish the
agreed authentication scope, complete the comprehensive validation phase, then
begin this experiment. It will compare one generated
Jadpo slice on Bun and Wasm, using a local Wasm host and Cloudflare Workers.
The experiment evaluates a compiler-owned runtime with explicit host
capabilities, semantic parity, source-level errors, performance and packaging.
It also compares `Jadpo -> generated Rust -> Wasm` with direct Wasm generation
from the checked semantic model, using small equivalent probes before choosing
the route for the full slice. Compiler complexity, runtime functionality,
toolchain burden and actual Cloudflare compatibility determine the
recommendation. Direct code generation may still use a Rust-built runtime;
runtime implementation language and application lowering are separate choices.

TypeScript/Bun remains the working target until a recorded decision changes
it. Wasm does not itself supply domain validation, durable storage, distributed
coordination or portable threading. Host adapters must preserve the accepted
language guarantees or reject unsupported requirements. A replicated storage
platform and a full backend migration are outside this bounded experiment.

On 2026-09-30 [WASM-EXP1 completed](experiments/wasm-experiment-results.md). Both compiler
routes passed the bounded probe locally and on Cloudflare; the provisional
Rust full slice reused identical core bytes with explicit SQLite adapters.
The route comparison remains inconclusive. Request overhead failed the
preregistered I/O latency gate, and ABI-limit fault mapping remains incomplete,
so the decision is to defer adoption for further backend development and retain
Bun. This does not schedule a production migration or an unbudgeted extension.

The subsequent [optimisation follow-up](experiments/wasm-optimization-results.md) identified
fresh instance creation as the dominant prototype cost. Exclusive instance reuse
and host caching reached roughly 79% of Bun throughput with microsecond-scale
p95 differences in the local fixture. Correctness passed locally and on Cloudflare;
the original latency gate still fails at concurrency one. This strengthens the
case for further bounded investigation while retaining Bun as the working target.

A further [boundary and HTTP experiment](experiments/wasm-http-experiment-results.md) retained
that ABI while caching compiler-owned constants and compiling host descriptor
checks. Small HTTP reads approached Bun; 16 KiB returned rows remained materially
slower. Larger-value transport is the next performance question, not a selected
replacement ABI. This changed artifact has local evidence only; earlier Cloudflare
passes apply to their recorded artifacts.

The [ownership/speed-build follow-up](experiments/wasm-value-path-results.md) then transferred
completed owned values and selected a speed-oriented release build. It won small
concurrent reads against generated Bun in the local fixture, with lower p95 and
CPU, but larger values and unresolved durable-write stalls prevent a general
performance/adoption conclusion. The JSON ABI remains unchanged.

The [row-transport follow-up](experiments/wasm-row-transport-results.md) then tested an internal
schema-bound binary format while keeping external HTTP JSON. Egress-only transport
improved concurrent large-row throughput about 7%, p95 about 5% and CPU/request
about 7% against previous Wasm in all five local pairs. Ingress did not help.
Practical Bun parity failed, single-write stalls remained adverse, and no new
Cloudflare qualification was performed. The candidate remains experimental; a
fully specialised internal value representation is still unimplemented.

The [immutable-row follow-up](experiments/wasm-typed-values-results.md) retains validated rows
as owned, schema-ordered scalars with shared immutable ownership and a locally
derived size bound. Generic JSON decoding remains. Concurrent large reads improve
about 3% in throughput/CPU and 2% in p95 against row transport, but Bun parity still
fails. Separate SQLite diagnostics identify native commit/statement pauses; changing
both targets to WAL/FULL greatly improves local writes. The Wasm adapter then beats
the generated Bun baseline on the tested writes, with different SQL plans and host
machinery. This is experimental local evidence, not a production backend or storage
configuration change.

The [compact-query/row-receipt follow-up](experiments/wasm-read-path-results.md) avoids sending
full checked plans and copying unchanged validated rows back from the guest.
The selected mode improves concurrent large-read throughput about 11% over the
previous candidate, but still trails Bun by about 19% in the local HTTP fixture.
Direct typed ingress is a measured alternative. The same module now passes a
[local workerd/SQLite Durable Object comparison](experiments/wasm-workerd-read-results.md)
without Bun, including fresh ownership changes. Concurrent large reads still trail
generated JavaScript on that host by about 10% throughput and 7% p95 in paired
medians. This short local comparison cannot establish production capacity, CPU,
cold starts or full backend coverage. No default target change follows from it.

The [framework-informed workerd boundary pass](experiments/wasm-workerd-boundary-results.md)
keeps the guest identical and tests direct host-to-guest JSON writes and synchronous
borrowed output views. Forty-eight tests and 608,612 HTTP requests pass. Concurrent
large-ASCII paired medians improve about 3% throughput/1% p95 over the previous
driver, but only three of five pairs win; escaped data still trails generated JS.
Retained instance memory increases from six to seven pages. Revised binary and
adaptive encoders are measured alternatives with ASCII regressions. This remains
an isolated experiment, with no default runtime or compiler change.

The [native-runtime-informed value pass](experiments/wasm-native-values-results.md) retains
that same guest and tests conservative JSON size proofs plus bounded selection of
typed ingress. Fifty-three tests and 924,283 HTTP requests pass. Concurrent escaped
HTTP improves about 17% throughput/15% p95 versus the previous WASM driver in all
five pairs, with approximate local JS parity for that fixture. ASCII, Unicode-only
and late-escape families still trail JS; small and Unicode HTTP regress against
previous WASM. Retention stays at seven pages. Direct typed writes and full-string
selection remain unsuccessful alternatives. This does not change the default target.

The [SIMD/bulk-memory guest pass](experiments/wasm-guest-scans-results.md) now improves isolated
large-text execution 14–52% over sampled WASM, using a negotiated guest exact-size
check and typed ingress without content sampling. Fifty-two host tests, nine Rust
tests and 949,050 HTTP requests pass. Retained guest memory drops to six pages.
Concurrent large HTTP still trails JS by 5–14%, with mixed tails; parity and a
default switch remain unproven. Shared ownership of input strings was separately
tested and remains unselected because gains are only 1–2% with an ASCII loss.

## 4. Build outputs

P8 and P9 establish one compiler-owned disposable output root:

```text
build/
  app.meta.json
  inventory/
  audit/
  validators/
  compatibility/
  openapi/
  target/
    app.ts                 generated Bun entry point
```

`build/` is excluded from source discovery and ignored by the canonical
scaffold. The compiler must be able to reproduce it from authored source, so no
file beneath it is an authoritative input or normal review surface. Source maps
and deployment packaging remain future additions; they must stay within the
same boundary rather than creating a second generated tree.

## 5. Static checks

The compiler should eventually include:

- parsing and name/type resolution;
- constraint and exhaustiveness checks;
- effect tracking;
- optional-value narrowing;
- query cardinality and projection typing;
- policy/authorisation proof or conformance analysis;
- secret/data-flow checks;
- declared-egress checks;
- lifecycle and migration impact analysis;
- transaction and idempotency analysis;
- resource-bound checks;
- output exposure checks;
- generation consistency and internal validation.

Compiler errors should describe the domain decision or violated invariant, not
dump the details of generated framework code.

## 6. Runtime validation

Compile-time typing is insufficient because backend data crosses untrusted
runtime boundaries. Validators are generated from the same declarations and run
on:

- HTTP requests;
- HTTP responses;
- database reads;
- database writes;
- environment/config values;
- queue and event payloads;
- external API responses;
- cached/deserialised data.

A Postgres row that does not satisfy the expected entity type is a hard runtime
contract failure. It must not become a malformed trusted object.

Domain constraints should also become database `CHECK` constraints where
possible. One rule should not be hand-copied into code, Zod, SQL, OpenAPI, docs,
and tests.

### 6.1 Callable execution and outcomes

Authored functions stay synchronous and pure. Authored actions have no
`async`, `await`, promise, or result-wrapper surface; the compiler derives
internal suspension to a fixed point through the action call graph and emits
target-level `async`/`await` only where required. Every ordinary call completes
before the next authored statement runs.

An exhaustive outcome match lowers to a compiler-owned call boundary. The
successful value continues through the `success` arm. A caught declared
`DomainFailure` dispatches on its semantic failure name to a compatible recovery
value, a newly constructed mapped failure, or an exact rethrow for `propagate`.
Unknown domain failures and non-domain exceptions are rethrown, so defects and
impossible states cannot be mistaken for recoverable authored outcomes. The
same lowering is wrapped in an internal asynchronous expression when the
derived callee graph may suspend.

## 7. Database runtime

Postgres is the opinionated initial default. Ordinary source uses declarative,
typed named queries and entity-owned mutations under the accepted
[entity/query contract](entity-query-model.md). Entity-centred and top-level
cross-entity query declarations are read-only runtime operations. Only an
entity's actions directly mutate that entity; top-level workflow actions compose
those operations. The runtime owns:

- parameterisation and safe SQL generation;
- connection handling;
- transaction execution;
- serialization/deserialization;
- constraint error translation;
- migration execution;
- observability and timeouts.

An entity action invoked directly receives an independently failure-atomic
database boundary. Reaching multiple entity mutation scopes does not silently
widen that transaction: the enclosing action must explicitly declare atomic
intent or a durable-workflow disposition. For an explicit atomic boundary,
nested entity actions and named queries reuse one compatible transaction
context and never commit independently; their success is provisional until the
outer commit. Propagated failure rolls back the boundary; a nested failure
recovered by the caller first rolls that action back to a compiler-owned
savepoint. Policy, lifecycle, and invariant reads guarding a write use the same
transaction and concurrency plan. The compiler rejects false atomicity across
databases or external services unless every adapter proves a common
prepare/commit and durable-recovery protocol. It emits the derived transaction
domain, isolation, locking/conditional-write, savepoint, deadlock-ordering, and
retry plan for audit.

Every mutable fact has one authority. Declared caches, graph views, search
indexes, and denormalised read models are compiler-managed derived
representations rather than additional write targets for application source.
The implemented local foundation records each projection obligation in the
authority transaction with a unique change ID and monotonic per-entity
revision. SQLite runtime evidence covers commit and nested-savepoint rollback.
Physical workers are not yet generated: at-least-once adapter delivery,
idempotent apply, lag/watermarks, retry, replay, rebuild, and reconciliation
remain required future contracts and are reported as pending in the audit.

Named queries declare authoritative, read-your-writes, bounded-staleness, or
eventual freshness. The runtime may use a stronger source but not a weaker one;
read-your-writes propagates the commit revision and waits, falls back to an
equivalent authoritative plan, or returns a typed availability problem.
Policy, ownership, lifecycle, and invariant checks default to authority; an
eventual query declaration cannot weaken them. A derived plan must prove its
revocation/freshness contract or revalidate candidate identities at authority.
Work across independently authoritative stores or services uses a persisted
durable workflow with idempotent steps, retry, compensation, reconciliation,
and outcome uncertainty. It is not reported as a transaction. Prepared
distributed atomicity is a separate future adapter capability, never an
automatic fallback.

Complex safe query constructs should be added based on real applications.
Raw SQL, if eventually necessary, is a deliberate audited escape hatch.

## 8. External service runtime

Service operations are generated from reviewed contracts rather than hand-made
client libraries. The runtime owns:

- authentication and secret injection;
- request/response validation;
- timeouts and retry policy;
- idempotency support;
- provider-error classification;
- observability and redaction;
- permitted destination/egress enforcement.

Provider failures are translated at this boundary into typed domain failures or
operational faults.

## 9. Source mapping and semantic metadata

Generated TypeScript must never be the normal developer-facing source. Ordinary
source maps are necessary but insufficient because generated code often has no
one-to-one source line.

Every important semantic node should receive a stable identifier, for example:

```text
route:todos:get-one
query:todos:get-one:load
policy:todos:get-one:owner
output:todos:get-one
```

Generated target sections retain both source ranges and semantic node IDs:

```text
generated lines 145-163
  source: todos.<source>:5
  node: route:todos:get-one:owner-check
```

`app.meta` can map runtime operations, target locations, source locations,
entities, fields, policies, and effects. Maps may be uploaded to a monitoring
service during deployment rather than publicly shipped.

## 10. Diagnostic categories

### 10.1 Source-language compile errors

These are application errors expressed entirely in language concepts:

```text
ERROR AUTH003

GET /todos/:id returns Todo but does not prove
that the current user may read the selected record.

todos.<source>:14
```

### 10.2 Generated-target compile errors

Valid source producing invalid TypeScript is a compiler bug, not an application
error. The developer should see an internal compiler diagnostic with the source
node and a report reference. They must not be asked to edit generated code.

### 10.3 Production runtime errors

Runtime failures are caught, enriched, and mapped to source concepts:

```text
ERROR

app: TodoApp
route: GET /todos/:id
operation: Todo lookup
entity: Todo
source: todos.<source>:14
request: req_28a91

cause:
  database connection timed out

duration:
  5000ms
```

The low-level generated stack remains available to compiler/runtime maintainers
but is not the primary diagnostic.

Production handling follows the [failure model](failure-model.md):

- declared domain failures use their standard kind to select the boundary
  response and record a semantic propagation trace without a native stack by
  default;
- operational faults retain a normalised cause chain and low-level stack
  internally while returning a generic safe response;
- defects and unclassified target exceptions are containment failures, return a
  generic internal response, and raise an internal invariant diagnostic;

Generated Postgres startup uses a bounded two-second connection attempt. If the
database cannot be reached, the process emits one secret-safe
`RUNTIME_STARTUP_FAILED` operational event and exits before opening its HTTP
listener.
- only fields declared in a failure's public schema reach the client;
- internal diagnostic fields remain subject to type-aware redaction and
  retention policy.

## 11. Errors without a literal source line

Default route authentication may expand to a large generated component despite
having no explicit source line. When it fails, the runtime should report the
route and inherited security rule rather than pretend an arbitrary generated
line is meaningful:

```text
Authentication runtime failure

GET /account

Declared at:
  account.<source>:7
  route GET /account

Inherited rule:
  authentication required by default

Internal component:
  session.verify

Cause:
  session store unavailable
```

Semantic mapping can make debugging better than ordinary TypeScript because the
runtime knows what the application was trying to do.

## 12. Agent-facing diagnostics

Diagnostics should minimise the context an agent must ingest while actively
guiding it toward a valid repair. Prefer:

```text
Required Todo lookup failed in GET /todos/:id
todos.<source>:17

Expected one Todo.
Database returned zero.

Related declarations:
- Todo entity: entities.<source>:8
- route: todos.<source>:11
```

over a target stack such as:

```text
TypeError at generated/runtime/db/query.ts:874
```

The implemented direction is one catalogue-backed semantic diagnostic with a
human-first summary, reason, recommended next step, bounded alternatives,
readable lower-case dotted rule identifier, precise location, typed context,
bounded impact, decision owner, help identifier, and source revision. A proposed
edit is marked preferred only when the compiler can justify it, preview its
behavioural and public-contract effect, and validate it against that revision.

The LLM packet is intentionally richer than production telemetry. Safe runtime
events carry correlation, semantic operation, and source-revision IDs; a trusted
local tool joins those IDs with the compiler manifest to recover source
location, call graph, affected routes, declared problem sets, and repair choices.
Raw customer values, credentials, provider errors, SQL, request bodies, and
headers are neither required nor permitted in that join.

The LSP renders the same object progressively: summary in the Problems panel,
cause and recommendation on hover, preferred verified edit plus alternatives in
Quick Fix, and impact/documentation in an expandable detail view. Editor plugins
do not construct their own advice.

`CompilerDiagnostic`, `PublicFailureResponse`, `OperationalLogEvent`, and
`AgentIncidentPacket` remain distinct types. Logging and diagnostic APIs accept
only compiler-approved safe values; secret values cannot be rendered, and an
internal failure field is not automatically loggable.

## 13. Formatting

Formatting is non-semantic. A canonical formatter gives stable diffs and a
single representation.

The discussion considered formatting on every save, then identified a tool-use
risk: rewriting a file while an agent is making iterative position-based edits
can invalidate its view. The preferred direction is checkpoint formatting:

- explicit format command;
- compilation boundary;
- pre-commit or pre-merge;
- CI verifies canonical form.

The agent drafts, the tool normalises, then the next iteration starts from the
canonical result.

## 14. Operational integrations

Structured semantic logs should integrate with systems such as Sentry or
Datadog while retaining the low-level trace underneath. The runtime should
provide consistent request IDs, operation/entity context, source node, duration,
redaction, and failure classification.

Observability must not become a route for secrets or private fields to escape.
Stable failure codes and kinds may be bounded metric dimensions; arbitrary
messages, IDs, and provider payloads must not become labels.

## 15. Implementation sequence

The active sequence, evidence, and exit gates live in the
[compiler implementation roadmap](roadmap.md). In summary:

1. freeze a Jadpo seed, core grammar, and machine-readable fixtures;
2. build the parser, semantic graph, type checker, and failure checker;
3. emit semantic artifacts before executable target code;
4. generate the smallest TypeScript/Bun runtime slice;
5. require the complete todo source before persistence/auth expansion;
6. use the order/payment application and TypeScript baseline to decide whether
   to continue;
7. defer native compilation and production deployment until the language model
   proves itself.

## 16. Failure condition

If users or agents routinely inspect generated `.ts` files to understand or fix
application behaviour, the source mapping and abstraction have failed.


### First-party authentication runtime checkpoint

The [AUTH-P4/P7 example](../examples/first-party-authentication/README.md) uses
compiler-owned Web Crypto HMAC-SHA-256 and a generated session authority with no
package dependency. Supported protected routes authenticate before decoding
inputs, preserve the operation clock, and supply the principal to existing
policy enforcement. Cookie writes require both configured-origin and
session-bound CSRF checks, including writes reachable from GET routes.

The narrow supported identity mapping and remaining service/JWT, PostgreSQL and
assurance gates are recorded in the [authentication plan](authentication-plan.md).
Authentication completion and the subsequent comprehensive validation phase
precede the scheduled Wasm probes and provide working adapter behavior for a
later target comparison.
