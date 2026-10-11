# WASM-EXP1 results — 30 September 2026

**Later optimisation follow-up:** [instance reuse and host caching](wasm-optimization-results.md)
substantially reduced the prototype overhead: approximately30,000–31,000 local
operations/s versus38,000–39,000 for Bun, with0.041–0.043ms p95. The original
results below are retained unchanged as historical evidence; the later report
qualifies the earlier traffic concern and recommends continued investigation.

**Disposition: defer adoption for further backend development; retain TypeScript/Bun. Compilation-route
comparison: inconclusive.** This is a bounded compiler/runtime experiment,
not a new production backend. The frozen experiment, repeated measurements, review and cloud cleanup are
complete. The evidence supports a portability proof and a defer recommendation.

## What the experiment established

The same checked Jadpo probe compiled through both **Jadpo → Rust → Wasm** and
**Jadpo → generated Wasm**, and both passed all 15 frozen probe cases locally
and on actual Cloudflare Workers. Direct application generation still used a
Rust-built generic JSON/validation helper; it did not interpret application
control flow in JavaScript. Rust was selected provisionally for the full slice
because rustc supplied async frames and ordinary function composition. This
was a maintenance choice, not evidence that Rust is the long-term winner.

The full Rust slice ran the identical **107,129-byte core module** locally with
Bun SQLite and on Cloudflare with SQLite-backed Durable Objects. Both passed
A01–A16, including owner-scoped parameterized SQL, missing/null distinctions,
result validation, authored domain recovery, multi-write rollback, 100 pairs
of interleaved requests, and committed-state observation from a new connection
or RPC. Separate A17 rejection tests and A18 source-mutation/reconstruction
checks passed. The full core SHA-256 is
`bf23ef07059ab0a469409cb980d5f8aade0dd3160ee09157e062f1e8d5cfb6ad`.

Cloudflare execution used version
`f87e203e-c6d1-4a89-8698-555687eb0a7e` of the disposable full-slice Worker,
compatibility date `2026-09-30`, and no explicit compatibility flags. Response
headers identified LHR. Captured provider fault logs contained only the five
allowed safe fields, with ordinary inner failures mapped to source operations
26 and 28. The secret sentinel was absent from the checked response body,
headers and captured logs. Account plan was not verified or changed.

These results meet the bounded portability-benefit criterion. Adoption gates
are conjunctive: the diagnostics gate remains qualified, and the local I/O performance
gate fails decisively for this implementation. Even an adopt-for-further-development result
would leave the production target unchanged.

These results establish a useful **bounded portability mechanism**. They do
not show that Wasm itself supplies better domain errors, validation, storage,
or transactions: those guarantees came from the checked model, generated
runtime and host adapter. The local authority and cloud Durable Object are
different explicit storage capabilities. Nothing here implements a replicated
database, queues, cache coherence or horizontal transactional coordination.

## Frozen protocol and acceptance

The [preregistration](wasm-experiment-plan.md) fixed the six-hour cumulative
active-effort cap, fixture, acceptance cases, route ordering, ABI and decision
thresholds before implementation. Rust ran first after a recorded coin flip.
Both routes had the same independent 45-minute allowance. The full slice used
existing accepted syntax; nested handled mutation/savepoints were explicitly
excluded and rejected before either probe began.

Authentication baseline: `4dc5604`. Its subsequent 45-step validation gate
passed 383 Rust tests, 180 compile fixture pairs, four editor tests, 21
verifier/dependency tests, 205 local runtime cases and 91 PostgreSQL cases.
One SQLite-only query-counter case was explicitly skipped on PostgreSQL.
This satisfied the agreed experiment start scope, not all product release
exits. The golden application's 60 diagnostics and 44 unexecuted integrated
cases, AUTH-P7/P8, P10R/P12 and external trials remain open.

| Evidence | Bun baseline | Rust probe | Direct probe | Full Rust slice |
| --- | --- | --- | --- | --- |
| P01–P15 local | 14 pass; P14 not applicable | 15 pass | 15 pass | 15 pass |
| P01–P15 actual Cloudflare | Not a Cloudflare Bun deployment | 15 pass | 15 pass | 15 pass |
| A01–A16 real storage | 16 pass locally | Outside probe | Outside probe | 16 pass locally and on Cloudflare |
| A17 unsupported capabilities | Backend-specific gate, not run | Bounded negatives | Four generator negatives | Five source/host rejection cases pass |
| A18 mutation/rebuild | Pass | Pass | Pass | Pass; original and mutant reproducible |

A17 is a compiler/host publication gate executed locally; there is no claim
that Cloudflare runs the compiler. Runtime reports deliberately retain two
`not_run` entries for A17/A18 and link their separate evidence. A14 remote-log
and header checks also have external evidence, rather than treating the
returned event array as proof that the log sink was safe. The Bun A15 wrapper
supplies a top-level operation identity; it does not prove automatic inner
fault-stack mapping.

The fixture SHA-256 is
`447ce2f023b53cecd705999ec280ae9b9d61f7ae0e85184b77c62c9f7aa143f1`, checked
revision `src_59ae092e2dc659cb`. A shared structural projection is produced after the ordinary
semantic/type/failure/entity/policy gates.
Both experimental Wasm generators consume that projection. The Bun baseline uses
the existing target generator on the same successfully checked project.
Application code is emitted from checked statements and expressions. Unsupported
constructs fail before artifact publication. No generated application output
was repaired manually.

## Route comparison

| Dimension | Rust-mediated probe | Direct application generation |
| --- | --- | --- |
| Wasm bytes | 97,678 | 83,497 |
| Application lowering | Generated Rust async functions | Generated WAT basic blocks and explicit continuations/registers |
| Generic runtime | Rust JSON/value/validation and future driver | Rust JSON/value/validation helper; no application interpreter |
| Suspension ownership | rustc future frames | Generator-maintained state and register allocation |
| Build path | Rust target/linker | Rust helper, pinned WABT conversion/link pass |
| Clean backend median, five runs | 3.544 s | 3.606 s |
| Source-edit pipeline median, five runs | 1.007 s | 0.348 s |
| Fresh probe process median, twenty runs | 25.179 ms | 21.528 ms |
| Required Wasm imports | None | None |
| Long-term route decision | Provisional full-slice choice | Still viable; smaller bounded probe |

The route timings are descriptive, with retained frontend/download caches and
other work occurring on the workstation. Clean and incremental timing scopes
differ; each scope is disclosed in the route reports. They are not sufficient
to establish a general performance winner. Direct generation leaves more
control-flow/linking machinery in Jadpo; Rust adds a compiler dependency and
needs diagnostic translation. Both require the same semantic checks, value
ABI, host contracts and source maps. A complete backend would need considerably
more than either probe's small generator.

## Performance and packaging

The end-to-end run uses the same checked readonly probe, fixed 256-byte input,
verified `alpha` result, and local SQLite policy read for its I/O mode. CPU mode
uses the same read effect with a copied in-memory row. It includes validation,
JSON copies and fresh Wasm instantiation per request. Each target runs alone,
with alternating target order, five repetitions at concurrency 1 and 8,
10-second warmup and 30-second timed measurement. Concurrency is outstanding
requests on one event loop, not parallel CPU execution; SQLite is synchronous.

| Workload / concurrency | Target | Throughput/s median (range) | p50 ms median | p95 ms median (range) | p99 ms median |
| --- | --- | ---: | ---: | ---: | ---: |
| cpu / 1 | bun | 222846 (220262–227781) | 0.003625 | 0.006250 (0.006000–0.006541) | 0.020209 |
| cpu / 1 | wasm | 3260 (3196–3341) | 0.095875 | 0.794416 (0.780916–0.806959) | 0.866959 |
| cpu / 8 | bun | 223716 (215957–232628) | 0.003666 | 0.006667 (0.006292–0.006958) | 0.020125 |
| cpu / 8 | wasm | 965 (954–972) | 8.245417 | 9.432750 (9.371542–9.624958) | 10.495833 |
| io / 1 | bun | 38482 (37234–38830) | 0.022667 | 0.031500 (0.030791–0.034625) | 0.085458 |
| io / 1 | wasm | 2118 (2071–2230) | 0.151166 | 1.050125 (1.033083–1.079750) | 1.146833 |
| io / 8 | bun | 39144 (39027–39405) | 0.022334 | 0.031333 (0.031125–0.031666) | 0.105125 |
| io / 8 | wasm | 2103 (2098–2105) | 0.155209 | 1.132500 (1.131417–1.139000) | 3.607416 |

All forty timed runs completed with **79,855,562 requests and zero measured
errors**. Ratios of the per-target median I/O p95 values are **33.34×** at
concurrency one and **36.14×** at concurrency eight, failing the maximum 20%
regression gate. No CPU repetition met the speed/throughput benefit criterion.
This is evidence against adopting this prototype, not a claim that all Wasm
programs are slow. The ABI intentionally pays for a fresh instance and copied
JSON on every request; the small fixture does little computation to amortize
those costs.

[All per-run values and qualifications](../../experiments/wasm-exp1/measurement/throughput-results.json)
are retained; the summarizer rejects incomplete runs or timed semantic errors.

Measurements used an Apple M1 Pro (ten logical CPUs), 16 GiB RAM and macOS
14.1 arm64; detailed [machine observations](../../experiments/wasm-exp1/measurement/machine.json)
include filesystem and power settings.

Measurement qualifications: workstation load, thermals, power mode and OS
caches were uncontrolled. Other short experiment checks and builds overlapped
early runs; this is a descriptive local experiment, not a calibrated benchmark.
Every timed result is checked and errors fail the run. A review found that
warmup failures are not counted by the frozen runner; the timed checks remain
active. Its fresh baseline database has one matching fixture table, but the
runner selects the first matching shape rather than asserting uniqueness.
Whole-process RSS includes both target support and stored latency samples, so
it cannot support a per-target memory advantage claim. True Cloudflare cold
start and isolated per-request memory are unavailable.

Cloudflare reported 3 ms startup for the full bundle and 315.27 KiB uploaded
(118.94 KiB gzip). That bundle includes the source mutant, test oracle and trap
fixture, so these are not comparable production deployment-size figures.
Pinned build tools include Rust 1.78.0, Bun 1.2.20, Node 24.18.1,
Wrangler 4.144.0 and WABT 1.0.39. Build-tool dependencies and deployed runtime
files are accounted separately. Wasm does not remove the compiler toolchain
or supply host APIs; this fixture's existing Bun runtime is also package-free.

Matched full-slice builds include source checking, checked projection, target
emission/build and one verified real-SQLite request. Five clean and five
incremental samples per target all passed; clean removes the route's generated
outputs/Cargo target, retaining the installed frontend, registry and OS caches.
Twenty additional fresh processes per target completed a verified first request.

| Process-wall measurement | Bun median (range) | Rust/Wasm median (range) |
| --- | ---: | ---: |
| Clean build and semantic check | 0.203 s (0.198–0.308) | 3.705 s (3.683–4.415) |
| Source-edit rebuild and semantic check | 0.194 s (0.191–0.375) | 1.050 s (1.041–1.530) |
| Fresh process through first request | 87.303 ms (78.154–251.830) | 46.019 ms (41.570–191.086) |

The clean median is about **18.3×** Bun, also exceeding the protocol's two-times
build-cost qualification. Bun combines projection and backend emission in one
compiler call; Rust generates and compiles after projection. Wrapper wall time
includes setup and cleanup; separate stage times are retained. The private
measurement crate uses a different library file path, which changes embedded
panic-location strings and the Wasm hash. All five clean generated Rust sources
match the frozen source, and same-layout binaries repeat exactly; these private
builds do not claim the deployed artifact hash. First-request measurements use
the frozen full module and original Bun app. Those Wasm samples finish with six
linear-memory pages (384 KiB); observed process RSS medians are 45,441,024 bytes
for Wasm and 58,941,440 for Bun. These include the host and are not peak or
per-request retained memory. See [matched measurements and exact scopes](../../experiments/wasm-exp1/rust-full/matched-measurements.md).

Supplemental measurements passed all compiled-core and helper-frame checks at
256, 4,096 and 65,536 encoded bytes. Median per-call times across twenty short
repetitions were:

| Complete input and host-response bytes | Full core sync | Full core async | Empty helper sync | Empty helper async | Host codec control |
| --- | ---: | ---: | ---: | ---: | ---: |
| 256 | 0.079 ms | 0.091 ms | 0.048 ms | 0.054 ms | 0.00074 ms |
| 4,096 | 0.556 ms | 0.094 ms | 0.453 ms | 0.061 ms | 0.00658 ms |
| 65,536 | 0.736 ms | 0.198 ms | 0.408 ms | 0.122 ms | 0.03834 ms |

The full core executes checked application semantics. The separate handwritten
helper exercises only the same driver's empty capability crossing; its largest
pending frame has the stated size and its input/response are slightly smaller.
The non-monotonic synchronous results and sync/async differences show allocation,
GC and scheduling sensitivity. Subtracting the codec control is not an isolated
native-call cost. These short microbenchmarks do not establish causal speedups.

Twenty alternating fresh processes per target reached module/schema readiness
in a median **69.84 ms for Bun** (68.79–84.53) and **23.51 ms for Wasm**
(22.51–25.90). Observed child RSS medians were 59,154,432 and 44,326,912 bytes,
respectively. This is warm-filesystem local readiness, including different
module-loading/storage paths, with no HTTP listener or application request;
it is not peak memory, per-request memory, or a Cloudflare cold start. Faster
readiness does not waive the failed steady-state I/O threshold.

The unbundled component inventory is **179,574 bytes for Bun** and **168,913
bytes for Wasm plus the local adapters/projection**; individual-file gzip sums
are 22,237 and 60,960 bytes. The cloud-adapter set is 169,218 raw / 61,152 summed
gzip bytes. These are inventoried components rather than equivalent deployable
HTTP bundles, and do not establish the 30% deployment-size benefit. The shared
Wrangler/WABT development installation has 36 packages and 355,853,390 logical
file bytes (220,872 KiB allocated); it is not shipped with either application.
The installed Wasm standard-library component adds 76,266,042 logical bytes;
[host executable sizes](../../experiments/wasm-exp1/measurement/dependencies.json)
are recorded separately and do not pretend to measure the entire Rust toolchain.
See the [boundary/startup procedure](../../experiments/wasm-exp1/measurement/boundary-README.md)
and [pinned tooling inventory](../../experiments/wasm-exp1/tooling/setup-evidence.json).

## Review findings and remaining work

Independent adversarial review found and fixed a nullable entry-parameter bug;
eight retained source-to-Wasm regressions pass, with frozen module bytes
unchanged. It also found a weak exact-output assertion; the harness now rejects
an extra-field mutant. JSON object ordering was corrected as nonsemantic while
array ordering and expected field values stayed fixed. Four additional atomic
and principal counterexamples preserved complete database snapshots.

One diagnostic gap is deliberately retained: an oversized host-result frame
rolls back safely and produces a generic internal fault, but the shared driver
loses the inner operation ID. Its event is truthfully marked as an entrypoint
fallback. Thus ordinary A15 fault cases pass, but universal precise inner-fault
mapping is not established. Arbitrary traps have the same source-location
limitation. This blocks an unqualified diagnostics/adoption claim.

A future backend would still need broad language lowering, polished Jadpo
translation of Rust/build failures, general cancellation/resource budgets,
production authentication/HTTP adapters, configuration and secret lifecycle,
migrations, more storage capabilities and unsupported-case coverage. Async
here means the outer request/RPC and the probe's delayed capability; the whole
transaction runs synchronously inside one authority. External awaited I/O
inside a transaction, cross-store atomicity, nested handled savepoints and
parallel authored execution remain unsupported.

AWS and Fastly were reviewed on paper only. Lambda would need an engine and
Runtime API/bootstrap adapter; Fastly's WASI host path would need its own
capability adapter and evidence. Fastify is a Node framework rather than a
hosting platform. None was deployed. See the
[host review](../../experiments/wasm-exp1/evidence/host-paper-review.json) and
[official-source links in the protocol](wasm-experiment-plan.md#6-host-facts-and-proposed-adapter-boundary).

## Reproduction and evidence

Start with [experiment README](../../experiments/wasm-exp1/README.md), then the
[projection](../../experiments/wasm-exp1/compiler/README.md),
[Rust probe](../../experiments/wasm-exp1/rust/README.md),
[direct probe](../../experiments/wasm-exp1/direct/README.md),
[full slice](../../experiments/wasm-exp1/rust-full/README.md), and
[host runner](../../experiments/wasm-exp1/full/README.md). All Bun commands use
`--no-install --env-file=/dev/null`; this installed Bun version does not reliably
isolate ambient configuration using `--no-env-file`.

The [frozen manifest](../../experiments/wasm-exp1/evidence/freeze.json),
[route decision](../../experiments/wasm-exp1/evidence/route-decision.json),
[full compiler evidence](../../experiments/wasm-exp1/rust-full/report.json),
[actual cloud evidence](../../experiments/wasm-exp1/evidence/cloud-full.json),
[independent review](../../experiments/wasm-exp1/direct/review/report.json), and
[raw evidence manifest](../../experiments/wasm-exp1/evidence/raw-manifest.json)
retain hashes and exact scopes. Raw acceptance bodies and captured safe logs
are retained as deterministic gzip files. Generated modules and larger build
logs remain in ignored experiment outputs and can be rebuilt from source.

Cloud cleanup is verified in [cleanup.json](../../experiments/wasm-exp1/evidence/cleanup.json):
a deletion migration removed the synthetic `SliceAuthority` namespace and
data, then both disposable Workers were deleted. API readback showed neither
experiment Worker and no Durable Object namespaces; the six pre-existing
Workers remain. No paid-plan change or production migration occurred.

WASM-EXP1 is complete. The target recommendation is **defer adoption for
further backend development**; the route is **inconclusive**. All planned
measurement groups and the bounded local/cloud cases ran; universal inner-fault
mapping, comparable minimal HTTP packaging, true cloud cold start and isolated
per-request memory remain explicitly unproved. They are limitations of the
completed experiment, not silently passed gates.
If revisited, first isolate instance lifecycle and codec costs in a separately
budgeted question before expanding language coverage. No extension starts as
part of this experiment.

The [effort ledger](../../experiments/wasm-exp1/effort.json) records approximately
**212.39 cumulative active minutes (3.54 hours)**, including a conservative
30-minute preflight charge, within the six-hour total and every stage cap.
Participant estimates are rounded and waiting intervals overlap; benchmark
waiting is recorded separately and is not focused engineering time.
