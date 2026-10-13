# WASM-EXP1 preregistration and preflight

**Status:** fixture/tool setup, 2026-09-30. No compilation-route probe,
Wasm backend implementation, benchmark, or cloud deployment has started under
this plan. Bun remains the working target. A negative or budget-exhausted result
is an acceptable experiment outcome, not a reason to weaken the checks.

## 1. Authority and start gate

This implements the protocol in [WASM-EXP1](../archive/implementation-history.md#wasm-exp1--bounded-wasm-runtime-experiment)
and [compiler/runtime architecture](../compiler-runtime.md#33-scheduled-wasm-target-experiment).
The [semantic model](../archive/specs-v0/semantic-model.md), [types](../archive/specs-v0/type-system.md),
[failures](../archive/specs-v0/failure-model.md), [entity/query contract](../archive/specs-v0/entity-query-model.md),
[policy](../archive/specs-v0/policy-plan.md), and [time/testing contract](../archive/specs-v0/time-testing-plan.md)
remain authoritative. The Bun runtime is a measured baseline, not permission to
copy any defect discovered in it.

Before either route probe, the coordinating task must record completion of the
agreed authentication scope and the subsequent comprehensive validation phase,
with exact source revision and evidence references. The first-party checkpoint
alone does not satisfy this gate. Unfinished external release-assurance exits
must remain explicit; this plan does not reclassify them as passed. If the
coordinator cannot establish the agreed start gate, stop at this preregistration.

The user's through-Wasm instruction authorizes disposable experiment resources
within that scope. Do not ask again merely because deployment is involved.
Cloud account access, deployment capability and product availability still need
verification; read access alone proves none of them. No production service,
custom domain, paid-plan change, AWS deployment or Fastly deployment belongs here.

## 2. Fixed effort and stopping rules

The cap is **six focused engineer-hours, cumulatively across all participants**.
Record start/end timestamps, active minutes and blocked/waiting time separately.
Current planning counts against the preflight allowance. Download/build wait is
reported as elapsed time even when it does not consume active engineering time.

| Stage | Maximum active minutes | Required output |
| --- | ---: | --- |
| This preregistration and read-only preflight | 30 | Plan, host facts, tool/access gaps |
| Gate, source/toolchain/fixture freeze and baseline checkpoint | 15 | Digest manifest and baseline evidence |
| Rust-mediated probe | 45 | Equivalent probe results, or precise stopping failure |
| Direct-generation probe | 45 | Equivalent probe results, or precise stopping failure |
| Chosen route, complete slice and host glue | 135 | Local and Cloudflare correctness evidence |
| Repeated measurements | 60 | Raw measurements and explicit missing metrics |
| Results and disposition | 30 | Target and route recommendations, cleanup record |
| **Total** | **360** | |

Probe allowances are equal and cannot be borrowed by the other route. Determine
their execution order by a recorded coin flip after the common fixture and ABI
are frozen. Common setup is charged once and listed separately; no prebuilt
route-specific helper is hidden as free setup. Stop each probe at 45 minutes,
even if a fix appears close. An incomplete probe is **inconclusive**, not proof
that the route is intrinsically inferior. If neither probe passes the shared
correctness checks, stop implementation and report defer/reject as justified.

At the full-slice limit, stop feature work and use the reserved measurement/report
time on what exists. Missing cloud access, unavailable dependencies, a semantic
gap, an unsupported host feature or a budget overrun may end the experiment
early. They must not produce a silent local-only success claim. Any extension
requires a new bounded question and recorded budget before more work starts.

## 3. Frozen inputs and reproducibility

The coordinating task owns the post-authentication source checkpoint. The
preflight checkout observed `fa7e7d5a105f21ef0a963f9fa57697ef89512067`; this is
**not** the experiment baseline. Existing unrelated working changes were not
altered. Before a probe, record:

- Baseline commit, complete relevant uncommitted diff, source-tree hash, accepted
  contract hashes, compiler binary hash, compiler-owned runtime/glue hashes,
  fixture and case hashes, and the protocol revision.
- Exact Rust/Cargo/LLVM, Bun, Node, Wasm encoder/parser/validator, optional
  optimizer, Wrangler/workerd versions and executable hashes; all dependency
  versions, lockfiles and download integrity information. Record compile flags,
  target triple, optimization level, panic behavior and allowed Wasm features.
- OS/architecture, CPU, power mode, memory, storage, background workloads,
  host configuration, Worker compatibility date/flags, account plan, limits,
  location evidence, resource names and deployed artifact/version identifiers.
- Exact clean-build, incremental-build, run, reset, test, measure and cleanup
  commands. Keep source, generated artifacts, logs and raw JSON/CSV measurements
  separately under one experiment output root. Never repair generated output.

Freeze one small accepted-syntax application and its tests before target work.
Use entity-owned policy-scoped read/write, an explicit atomic action, and a
handled domain outcome. Prefer existing accepted examples as source patterns,
but do not copy legacy spellings without running the normal checked build.
Fixture principals enter only at the trusted test boundary; no public header
or body becomes a principal selector. Such fixtures prove policy execution,
not authentication adapters or protected production routes.

The same application must produce the existing Bun baseline and both probes.
No handwritten Rust/WAT equivalent counts as compiler generation. A handwritten
host harness or shared compiler-owned runtime is permitted and accounted for.
Change a source literal/constraint and rebuild during each probe: the observed
behavior must change correspondingly, proving the artifact is source-derived.

## 4. What is actually available to lower

Read-only inspection found that `AnalyzedProject` in
[`core/src/lib.rs`](../../jadpo/crates/core/src/lib.rs) contains parsed syntax,
`SemanticGraph`, `TypeCheckResult`, `FailureCheckResult`, `EntityModel`, and
`PolicyModel`. `analyze_sources` can return that structure with diagnostics;
successfully obtaining it is not itself proof that checks passed. The current
[`target.rs`](../../jadpo/crates/core/src/target.rs) consumes this complete model.
The graph exposes identities, refinement/call edges and source ranges; it is
not already a complete executable control-flow IR.

Both experimental routes must enter after the normal diagnostic gates and use
one checked, bounded lowering projection of the complete model. Resolve
operations, types, field presence, policy predicates, outcomes, transaction
plans and suspension from those checked facts. Retain semantic operation IDs,
source ranges and checked-source revision. Do not reparse source with regexes,
infer policy from names, or lower the graph's name inventory alone. Reject every
unsupported reachable construct before publishing an artifact. Separate a
code-generation gap from a missing host capability in diagnostics.

| Route | Probe implementation | Particular evidence to collect |
| --- | --- | --- |
| Jadpo → generated Rust → Wasm | Generate Rust from the checked projection; compile for `wasm32-unknown-unknown` with a small compiler-owned runtime and explicit imports | Rust source volume, rustc dependency/build cost, optimization/library reuse, ownership and panic/trap handling, diagnostic translation, and async frame implementation |
| Jadpo → Wasm directly | Emit binary with a pinned encoder, or generated WAT compiled by a pinned tool, from the same projection; runtime helpers may still be Rust-built | Encoder/linking cost, validation, runtime-helper burden, memory ownership, control-flow and suspension state generation, source mapping, optimizer/tool dependencies |

Both probes implement exactly: one constrained nominal text input, an optional
nullable field, one successful value, one declared failure recovered by an
exhaustive outcome match, and one named read whose host result arrives later.
Test invalid boundary input, delayed success, delayed failure, wrong-type host
result, and two interleaved request contexts. Run each probe locally and submit
the same feature/import shape to the Cloudflare host. Do not choose a route
solely from code size or because it was implemented first.

Compare active effort, changed compiler/runtime lines, build dependencies,
repeat build time, emitted features/imports, binary/glue size, ownership,
suspension, diagnostic quality, library reuse and remaining work. Prefer a route
only if it passes all probe cases and either the other demonstrably fails a
required capability or its measured benefit outweighs recorded maintenance
cost. If both pass without a clear difference, choose one provisionally for the
slice while recording the route conclusion as inconclusive.

## 5. Runtime boundary and value representation

Start with single-threaded core Wasm and an explicit versioned import/export ABI;
neither route may assume a full OS or portable WASI. Use bounded linear-memory
buffers with documented allocation/free ownership and checked pointer/length
arithmetic. Preserve UTF-8 text semantics, exact UUID validation, nominal
validation descriptors, and distinct omitted/null/present tags. Every borrowed
buffer's lifetime ends before suspension unless copied into its request frame.
Cap request/result buffers and memory growth identically for the two probes.

Actions retain authored sequential semantics. A narrow explicit continuation
protocol (completed/domain-failed/host-pending, plus opaque request and operation
handles) is a comparable starting implementation for both routes. Generated
glue awaits the host Promise, validates the response, and resumes only its
matching frame. No JS Promise is treated as a synchronous Wasm return value.
JSPI, Asyncify, wasm-bindgen futures or other alternatives may be recorded, but
cannot become an unbudgeted third route or an assumed cloud feature.

Implement only needed capabilities: parameterized storage, request transport,
request-stable clock, entropy for correlation where needed, declared typed
configuration, and secret-safe operation events. Database results re-enter the
same validation rules as Bun. Domain failures are tagged semantic outcomes;
traps and unexpected host errors remain generic public faults with internal
semantic ID/source mapping. No SQL, credentials, body values or raw host errors
may appear in public errors or safe logs.

Exercise the asynchronous boundary with the existing named-query/storage
effect, including a delayed host response. External HTTP may be part of the
host adapter, but this experiment must not invent authored SERVICE/ASYNC syntax
or count a handcrafted external-service call as checked language support.

## 6. Host facts and proposed adapter boundary

Official documentation was consulted on 2026-09-30. Recheck and freeze these
facts at execution; account-specific behavior must be recorded rather than
inferred from documentation.

**Cloudflare Wasm.** Workers supports precompiled Wasm modules and SIMD, has no
Worker threading, and describes WASI as experimental with a partial syscall
set. This supports a conservative explicit ABI; it does not prove every emitted
proposal, component-model import or JSPI mechanism. List and validate each
artifact's actual imports/features, then instantiate it on the deployed host.
[Workers Wasm](https://developers.cloudflare.com/workers/runtime-apis/webassembly/)

Wrangler bundles `.wasm` modules; generated JavaScript imports the module and
instantiates it with an explicit import object. Keep bindings and HTTP handling
in generated glue. Compilation inside a request and filesystem/socket imports
are not part of the proposed ABI.
[Wasm in JavaScript](https://developers.cloudflare.com/workers/runtime-apis/webassembly/javascript/)

**Cloudflare limits.** The documentation currently states 128 MB per isolate
including JavaScript and Wasm allocations, one-second top-level startup, and
64 MiB uncompressed Worker size with no separate compressed-size limit. It lists
10 ms request CPU on Free and up to five minutes on Paid; configured/default
limits and the actual plan still matter. Record all uploaded modules and glue,
not just `.wasm`. Do not substitute remembered older compressed-size limits.
[Workers limits](https://developers.cloudflare.com/workers/platform/limits/)

**Cloudflare storage.** Proposed full-slice authority is one SQLite-backed
Durable Object transaction domain, accessed through generated glue. Its
`transactionSync` rolls back when the callback throws and requires a synchronous
callback with synchronous storage operations. Therefore Wasm policy checks,
read-result validation and mutations for one atomic scope must complete inside
that callback. Arbitrary awaited HTTP or cross-object operations inside the
scope must be rejected. Nested/savepoint behavior requires direct evidence; it
must not be inferred from a method name.
[SQLite Durable Object storage](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)

The async probe can suspend while awaiting a read or one complete transaction
request; the authoritative transaction itself remains synchronous on the
Durable Object. Validate returned data before the caller continues. Local host
glue can use Bun SQLite with the same capability contract; this is a local Wasm
host, not a claim of standalone WASI portability. Also instantiate the artifact
in workerd when available to separate Bun's engine from Workers compatibility.

D1 provides prepared statements and transactional batches that roll back on a
statement failure. That does not by itself prove callback transactions with
intermediate application validation or nested handled-failure savepoints.
Treat D1 as an alternative only if the exact frozen transaction plan is proved;
do not replace the required plan with a weaker batch to get a pass.
[D1 database API](https://developers.cloudflare.com/d1/worker-api/d1-database/)

The host capability manifest must identify transaction domain, isolation,
authoritative/read-your-writes behavior, rollback/savepoints, connection and
payload bounds, scheduling, timeout/cancellation, and error normalization.
Unsupported requirements fail closed. A local SQLite file and a single Durable
Object do not establish replicated-database or distributed-transaction support.

**AWS, paper only.** A Lambda custom runtime can bundle a runtime/engine and
implement the Runtime API with an executable `bootstrap`. Inference: the core
module is a plausible reusable artifact, but the engine, Linux architecture,
invocation loop, IAM/configuration, storage and async imports need an AWS host
adapter; `.wasm` alone is not a deployable Lambda runtime. Record the selected
OS-only runtime and limits if a later experiment is authorized. No AWS adapter
or deployment is included here.
[Lambda custom runtimes](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-custom.html)

**Fastly, paper only.** Compute's documented Rust path uses WASI-compatible
crates and `wasm32-wasip1`; a successful build does not prove every crate's
runtime functionality. Inference: reuse of checked lowering/runtime logic is
plausible, but Cloudflare's JS imports are not Fastly host imports. A separate
HTTP/storage adapter, supported feature set, resource-limit review and rollback
proof remain required. No Fastly adapter is built here.
[Fastly Rust guide](https://www.fastly.com/documentation/guides/compute/developer-guides/rust/)

The historical name **Fastify** refers to the Node.js web framework, not the
Fastly hosting platform. Fastify could wrap a Node-hosted Wasm module but is not
an additional cloud host or a reason to change this experiment's baseline.
[Fastify](https://fastify.dev/)

## 7. Acceptance matrix, frozen before implementation

Run identical applicable assertions against Bun, local Wasm and Cloudflare
Wasm. Record pass/fail/unsupported/not-run for each target; unsupported is not a
pass. Normalize only generated request IDs, host timing and explicitly declared
nonsemantic ordering. Store request, expected safe response, database snapshot
hash and safe semantic event expectations for each case.

| ID | Required observation |
| --- | --- |
| A01 | Valid request and response round-trip exact allowed fields |
| A02 | Malformed JSON, invalid UUID, short/long text and unknown fields reject before mutation |
| A03 | Omitted patch field preserves state; explicit null clears it |
| A04 | Empty/invalid patch preserves state with the baseline failure contract |
| A05 | Missing/duplicate rows produce declared outcomes and preserve state |
| A06 | Owner can read/update; another principal cannot read/update that row |
| A07 | Policy predicates are present in the executed parameterized storage operation; guessed identifiers cannot bypass them |
| A08 | Stored invalid row and malformed host result are contained as internal faults, not client validation errors |
| A09 | Handled declared failure takes exactly its recovery arm; unexpected host failure cannot enter that arm |
| A10 | Multi-write failure rolls back earlier writes; compare complete authority snapshots before and after failure |
| A11 | Policy/invariant read and guarded write share the declared transaction context; authority read sees committed state |
| A12 | Delayed host success/failure resumes sequentially, once, with the correct request and operation IDs |
| A13 | Interleaved principals/requests do not exchange context, buffers, configuration, results or failures |
| A14 | Secret sentinel and internal failure context never occur in public bodies, headers or safe logs |
| A15 | Fault event resolves to the original Jadpo operation, source range and checked revision |
| A16 | Reopened local process/new request to cloud authority sees committed state; rejected mutation never becomes visible |
| A17 | Unsupported syntax, host imports, atomic cross-store plans and unavailable freshness fail before publication/execution |
| A18 | Source mutation changes generated behavior; clean rebuild reproduces behavior and artifact manifest |

For A13, run 100 interleaved request pairs with two fixture principals and
deterministic delay schedules. Add a trap followed by a valid request to catch
uncleared frames. For A10, compare complete stored-state snapshots before and
after each failed scope. Nested handled mutation/savepoint execution is outside
this frozen slice; the bounded backend must reject it, not silently flatten it.
This scope was fixed before fixture freeze or either route implementation; the
roadmap requires rollback, not implementation of every transaction construct.

## 8. Measurements and decision thresholds

Correctness is mandatory: **all A01–A18 applicable cases must pass on all three
targets, with zero unauthorized access, disclosure, partial commits or context
leaks**. A missing required host case blocks adoption. Report any baseline defect
and repair/checkpoint it separately; do not adjust expected behavior to favor
either target.

Use the same machine and fixture for local target comparisons. Record five
clean builds with caches removed, five incremental builds with the same small
source edit, and twenty fresh-process starts. Warm each runtime, then alternate
Bun/Wasm order across five runs per workload at concurrency 1 and 8: a fixed
10-second warmup and 30-second measurement per run, capped to fit the remaining
budget. Freeze payload sizes and workload counts before seeing target results.

The CPU workload must be expressible in accepted Jadpo and run the same checked
value/control-flow work with a verified result, rather than timing a Rust-only
algorithm. The I/O workload uses the same local SQLite fixture and equivalent
policy/transaction operations. Measure empty host-call and serialization paths
separately at fixed 256-byte, 4-KiB and 64-KiB payloads. Include all host glue,
validation and copies in end-to-end numbers.

Report build wall time, artifact bytes (raw/compressed), deployment bytes,
runtime/toolchain dependency bytes, startup, process RSS and Wasm pages where
observable, successful throughput, error rate and p50/p95/p99 latency. Use
per-run medians/ranges; bootstrap across runs if time permits. Do not claim
statistical significance from request count alone. Cloud measurements include
network/provider/storage effects and are reported separately, not as a Bun vs
Wasm causal speedup. True cloud cold start and per-request memory may be
unobservable; label them unavailable rather than inventing zero values.

Recommend **adopt for further backend development** only after correctness,
reproducibility and diagnostic gates pass, no more than 20% regression in local
I/O p95 latency, and at least one preregistered benefit:

1. At least 20% lower CPU-workload median service time in at least four of five
   comparable local runs, without throughput or error-rate regression; or
2. At least 30% smaller complete application deployment artifact at comparable
   packaging boundaries, with toolchain/runtime costs separately disclosed; or
3. The identical core Wasm artifact (same hash) runs locally and on Cloudflare
   with generated host glue, no hand edits or app-specific adapter branches,
   and all required cases pass. This is a bounded portability benefit, not proof
   that AWS/Fastly or arbitrary apps work.

Report build time even when the qualifying benefit is portability; a clean
build over twice baseline or a dependency/install burden incompatible with the
closed-runtime model prevents an unqualified adoption recommendation and needs
an explicit extend/defer rationale. Runtime installation and build-time compiler
dependencies are different quantities and must not be conflated.

Use **extend** only for a named promising uncertainty with a new proposed budget;
**defer** for incomplete prerequisites/evidence or disproportionate present
cost; **reject** for demonstrated guarantee incompatibility or measured lack of
benefit within the declared scope. Separately disposition the compilation route
as Rust-mediated/direct/inconclusive. None of these replaces the full Bun target.

## 9. Local preflight inventory and remaining access needs

Read-only commands on 2026-09-30 observed:

| Tool | Observed |
| --- | --- |
| Rust / Cargo | 1.78.0; rustc commit `9b00956e56009bab2aa15d7bff10916599e3d6d6`, LLVM 18.1.2 |
| Rust toolchain/targets | `stable-aarch64-apple-darwin`; installed target only `aarch64-apple-darwin` |
| Bun | 1.2.20 |
| Node | v24.18.1 |
| clang | Apple clang 15.0.0, arm64-apple-darwin23.1.0 |
| AWS CLI | 2.36.7; no account operation performed by this preflight |
| Not found on PATH | Wrangler, workerd, Miniflare, wasm-tools, wasm-opt, wasm-pack, wasm-bindgen, Wasmtime, Wasmer, Fastly CLI |

No target or package installation occurred. PATH absence is not an exhaustive
disk search. Required next tooling is a pinned Rust Wasm target, a pinned
encoder/validator path for direct generation, and local Workers packaging/host
tools. A Bun-hosted module can provide local Wasm execution without introducing
Wasmtime as an extra requirement. Add wasm-bindgen/optimizer only if a measured
route needs them; record their cost and supported Rust version.

The coordinator reported a successful Cloudflare account-list read through the
connector. It owns subsequent read-only account checks. Remaining readiness is
the actual plan/limits, ability to upload one isolated Worker, availability of
SQLite Durable Objects/bindings, logs/metrics, a private fixture injection path,
and cleanup of exact experiment resources. No credential values belong in the
report. The experiment must not enable a new paid plan to overcome a limitation.

## 10. Required final evidence

Deliver the frozen manifest, route-comparison table, generated artifacts and
source-to-operation map, feature/import and capability inventories, exact
reproduction commands, complete acceptance matrix, raw measurements, effort
ledger, and resource cleanup evidence. The short conclusion must state both
target and route dispositions, failed/unrun cases, remaining backend work and
the limits of paper-only AWS/Fastly conclusions. Until that evidence exists,
this document is a preregistration, not an experimental result.

## Execution start checkpoint

The auth baseline is `4dc5604` (2026-09-30). Subsequent full validation report
`build/validation/20260930T024217-25646/report.json` has SHA-256
`7325a2ce5851b6e63361dfc2185b3b0c145fab7ba8550a3b3ed0d8acfd318ad9`.
All 45 supported steps passed: 383 Rust tests, 180 compile fixture pairs,
205 local runtime cases and 91 PostgreSQL cases, with one explicit SQLite-only
query-counter skip. The golden app's 60 diagnostics, 44 unexecuted integrated
cases and external release gates remain open. This is the agreed bounded
authentication/validation start gate, not full product readiness.

Rust's `wasm32-unknown-unknown` target is now installed for Rust 1.78.0.
Common fixture/tool setup resumed at 2026-09-30 01:48:59 UTC. Charge the earlier
preregistration/preflight its full 30-minute allowance conservatively: its
individual active intervals were not instrumented. New work records intervals
in the experiment effort ledger. Host preparation and the common checked-model
projection consume the full-slice allowance; they are not free extra budgets.
