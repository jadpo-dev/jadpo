# Performance experiments

Updated 2026-10-24. This is the canonical catalogue of performance hypotheses,
experiment dependencies and retained results. The [roadmap](roadmap.md) remains
the only delivery tracker. This catalogue is Richard's explicit exception to
AGENTS.md's prohibition on plan documents and separate evidence packets: keep
one catalogue, link existing raw records and architecture findings, and do not
create a second progress log.

## Primary comparisons and current control prerequisite (2026-10-24)

Richard corrected the primary comparison method after the screens below:

| Primary pair | Deployment and connection lifetime |
|---|---|
| TypeScript/Bun vs Jadpo native Rust executable | Standalone servers, direct PostgreSQL, persistent application state and clients/pools |
| TypeScript/workerd vs Jadpo Wasm/workerd | Local Workers, persistent application state, invocation-owned connections through the same transaction-pool helper topology/caps: golden 10, focused 20 |

**Both primary deployment pairs now have locally guarded, agent-reviewed
controls:** the faithful and explicit-option TypeScript/workerd pairs and the
current native/Bun standalone pair recorded below. These are provisional
references with the stated loader/host/lifetime limits, not universal
language/engine evidence. **P61 roundtrip and P21.suffix-bitmap are rejected;
P55's safe Response constructor remains promising with adverse RSS. Next is
independent P05 design review, preserving its original 0–5% golden hypothesis
and unknown actual gain; P11 is measured/mixed and retained. Continue eligible
exploration without another baseline prerequisite.** Acceptance uses local checks and
agent review under standing unattended authorization, not a new Richard
approval. Optional three-TypeScript lifetime arms are secondary diagnostics;
the strong direct-PG Bun application remains unchanged.

Bun versus Workers is a secondary cross-deployment comparison. Differences
include connection lifetime, driver transport, crypto and HTTP bindings; it
cannot by itself identify a compiler-only or intrinsic engine gap. All prior
raw results and within-Workers gains remain valid within their recorded
references. F01 is preserved as the historical initial54 cross-deployment
planning model, not the revised primary target. Do not rewrite its forecasts
or erase earlier adverse results.

The source-reviewed control proposal is to retain the same TypeScript
business logic, authentication, schema, validation and SQL, with one persistent
application. The golden entry is [golden_host.mjs](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/tools/bench/golden_host.mjs#L1).
A stable Store facade would resolve and bind methods to a request-local Store
through AsyncLocalStorage, awaiting close in finally; never use a mutable
global current client or cross-invocation sockets. Recreating
[createApplication](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/examples/golden-todo/typescript/src/app.ts#L267)
per request would reset limits, admission, auth and readiness state. Preserve
those caches and the focused app's windows/readiness; scheduling stays external.
Trusted loopback peer identity comes from the harness binding, never an
untrusted caller header.

Cached postgres 3.4.7 exposes a workerd entry using cloudflare:sockets;
esbuild 0.28.2 and Wrangler 4.149.0 are available locally. This is a feasible
source-backed route now exercised by the guarded local port below, not
complete-contract qualification. Its artifact/metafile evidence must retain
workerd condition resolution and actual nodejs_compat
support for AsyncLocalStorage, crypto HMAC/timingSafeEqual/X509Certificate and
net/https/dns imports. Avoid Node server/process bootstrap in the Worker entry;
do not stub business or security functions. Keep driver options identical and
capture business versus client-startup SQL in untimed acceptance: type
discovery and prepared/session setup may repeat with invocation lifetime.

Native currently uses [available_parallelism](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-native/src/deployable.rs#L181)
and [persistent Drivers per worker](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-native/src/deployable.rs#L327),
with [10 open / 8 idle connections per driver](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-drivers/src/postgres/driver.rs#L20).
Bun's [golden Store](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/examples/golden-todo/typescript/src/db/store.ts#L59)
keeps five general plus five exclusive clients; its
[focused client](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/tools/bench/typescript/server.ts#L16)
has max20/prepare:false. Preserve and disclose these architectures, worker/core
counts and connection budgets; do not impose an invented single-core limit or
route the standalone pair through the helper. CPU accounting covers the full
server process/all native threads, helper CPU separately and workerd+helper
sum, with PG/load/fixture CPU distinct. Require provenance-valid native and
Workers artifacts, exact body/state/security/peer/cancellation guards, matched
concurrency/order, isolate count and owned cleanup. Warmup/admission/drain
boundary limits remain explicit.

The same hosting-model rule applies to a later serverless comparison: Bun and
native Rust can use suitable Lambda adapters, then be compared on the same
hosting model. No deployment or account work is authorized here. Establish
local warm pairs first; cold start is a separate measurement. This control
comparison has no optimization ID or gain forecast. The first local Workers
controls and current standalone native pair are measured below.

## Resume and finish the exploration

Richard's 2026-10-24 instructions apply across sessions. Explore the available
queue to exhaustion, including ideas discovered while experimenting, before
starting proper production implementation. There is **no maximum experiment
count, batch size, task quota or time quota** that permits early promotion.
Small working windows can organize attention; they neither close the queue
nor trigger promotion. The already-built 9.2a3 keeper may finish its existing
qualification and manual review separately.

1. Read this catalogue, AGENTS.md, the roadmap and the relevant language area
   pages. Check open PRs/branches and retained experiment refs before duplicating
   work. Before allocating IDs or editing the catalogue, coordinate any open
   catalogue PR: update the pending PR for the same scope or serialize updates
   instead of making conflicting main-based branches. Related results and ideas
   may be batched into one documentation-only PR. Refresh stale source evidence
   and the queue's impact ordering.
2. Have a dedicated discovery agent inspect for further performance problems,
   record credible ideas with the fields below, calculate their potential
   savings or state what measurement is missing, and maintain impact order.
   Discovery has no target count: the initial inventory contains 54 candidates,
   rather than padding it to 100. Discovery may continue while another agent
   edits, but all owned builds and unrelated work stop during measurement.
3. Check the primary comparison prerequisite above before resuming any
   optimization measurement. Freeze one baseline source **B**, its exact
   production artifacts, harness, toolchain, dependencies, feature/profile
   flags, workload, full-body/state/auth/
   quota controls and measurement protocol. Use one reusable scratch worktree
   beside the repository, never under `/private/tmp`. Coordinate with other
   sessions so no concurrent agent build or test contaminates a timed window.
4. Take the highest-priority eligible item in the queues below. **Before its
   implementation or measurement**, commit the variant's prediction in this
   catalogue: gain/range or honest unknown, exact workload, throughput metric,
   reference source/artifacts (B or prerequisite stack), confidence and basis,
   plus a prospective implementation-complexity estimate or honest unknown.
   Retain that original forecast; never edit it with hindsight. A revised
   forecast belongs to a new dated variant/version, preserving the original.
   Then make a cheap focused prototype and its correctness controls. Independent
   hypotheses start again at B; dependent hypotheses stack on exact prerequisite
   commits. Record marginal and combined predictions against the prerequisite
   stack and B respectively, with each observed gain adjacent to its own
   prediction. Alternatives are separate variants; overlap warns against
   attributing the same savings twice.
5. Commit each prototype before building. After the control prerequisite is
   accepted, run focused correctness and a short matched screen of the primary
   deployment pair using its frozen protocol; a microbenchmark is
   supporting evidence, never an application-throughput result. Do not run the
   full 176-case qualification, full gate or a production PR for every cheap
   prototype. Preserve existing guards; never bypass source/artifact checks or
   reuse a label after its producer changes. Finish/reap every service and
   temporary PostgreSQL instance the experiment started, including failures.
6. **Before resetting the scratch branch**, retain the source and harness under
   an immutable named Git ref, for example `refs/experiments/P01/variant-1`, and
   verify that ref resolves. Save raw output, failures, logs, build manifests,
   source/artifact hashes and immutable served copies under the durable local
   benchmark archive. Record B, prerequisite refs, protocol, all rounds,
   adverse observations and the result/ref/raw location in this catalogue.
   After every experiment, fill its measured-throughput-gain cell next to the
   frozen prediction, comparing the same declared metric/workload/reference.
   Retain all rounds and label a short screen's result provisional with its
   duration, round count and measurement limits. Linked results retain p95,
   CPU, RSS, size and all adverse observations as well. Add the actual
   implementation assessment described below, its tradeoff and a recommendation
   for later human carry-forward selection; retention is not promotion.
   A dangling commit or overwritten ignored output is not retained evidence.
   Only then discard/reset the working branch for the next experiment.
7. Feed every emerging idea back into this catalogue, with a savings calculation
   or an honest unknown and the measurement needed. Re-sort the applicable
   queue. Publish catalogue/workflow changes through a **documentation-only PR
   branched from current main**, and request `gh pr merge --merge --auto` for
   that PR. Required CI must pass: no `--admin`, bypass or compiler/runtime/CI
   changes in this exception. Keep experiment code under its retained refs,
   outside the auto-merged documentation PR.
8. Defer items needing accounts, keys, deployment, an unavailable prerequisite
   or an unsettled Jadpo language decision, record the exact blocker, and
   continue eligible items. A language choice still goes to Richard with a
   recommendation. A queue containing only blocked items is **blocked**, not
   empty; do not call Jadpo fully optimized or exploration exhausted. A context
   handoff records the next eligible item here and resumes in the next session.
9. Only when the available exploration queue, including emerging ideas, is
   exhausted (or Richard explicitly redirects the work), rank retained keepers,
   implement them properly, test interactions, run longer matched confirmation,
   full qualification and exact production-path checks, and the full gate once
   per delivery chunk. Codex opens those compiler/runtime/keeper PRs for Claude
   **without auto-merge and never merges them**. The documentation exception
   does not change the code delivery rule or the roadmap's acceptance criteria.

## Reading the estimates and retaining results

All queue ranges below are **hypothetical application-throughput predictions
on the named workload**, not calibrated forecasts or confidence intervals. Zero
benefit and regressions remain possible even where the sizing range starts
above zero. p95, memory, cold start and artifact-size effects are unknown until
measured independently. Confidence concerns application benefit, not just the
existence of a source-level inefficiency. `S` is localized work, `M` connected
edits and `L` an architectural experiment; these are cost categories, not time
promises. `E` uses an existing read screen, `F` needs a focused workload and
`C` measures cold start. Every candidate is initially **queued / unmeasured**;
there is no observed result, source prototype ref or raw record until one is
entered explicitly. Each new measured-gain cell starts as **Not measured**;
measured entries now carry their observed values and linked results below.
Each future variant keeps its original prediction and its own result, including
losses. Compare observed gain with that prediction so accuracy is visible;
label the reference and summary statistic, retain all rounds in the linked
result, and distinguish absolute throughput from a Bun-normalized ratio.
For dependent variants, the predicted and adjacent measured cells each carry
both **marginal over the prerequisite stack** and **combined over B**, with
unknown predictions left honest. Do not change workload, metric or reference
after implementation to make a forecast appear accurate.

For CPU-limited throughput, if a *current* profile gives removable self-cost
share s and the experiment removes fraction r of it, a conditional bound is
`gain = 1 / (1 - s*r) - 1`. With a current per-request saving d and CPU cost C,
use `1 / (1 - d/C) - 1`. Both require the named workload, compatible CPU scopes
and the CPU-limited assumption; neither predicts DB waits, p95 or edge capacity.
Old profiles identify leads, not a current removable budget. Inclusive parents
and self samples overlap: never sum them or multiply separate predicted gains.
Measure combined stacks against B instead. A full-list aggregate prediction is
allowed as a transparent, speculative planning scenario with explicit overlap,
workload and removable-cost assumptions. Do not form it by simply adding or
multiplying the item gains, or call it measured/validated before combined-stack
measurements exist. Freeze the scenario before future results, retain its
original assumptions, and expose sensitivity to unknowns.

Every completed experiment result also records an **implementation assessment:
simple, moderate or complex**, with short reasons from the retained source.
Explain production control flow, dependencies, platform assumptions, runtime
interfaces and testing/maintenance burden; identify which rough edges belong
only to prototype controls and which a proper keeper would retain. Revisit the
pre-experiment complexity/cost estimate honestly, preserving it and recording
why the actual assessment differs; frozen performance forecasts never change.
Include the performance-versus-complexity tradeoff and an advisory recommendation
for later human carry-forward selection. Do not equate line count with messiness
or invent a numeric complexity score. These assessments do not promote a keeper
or end the uncapped exploration queue. Benchmark/control adapter assessments are
labelled separately from Jadpo optimizations; unbuilt ideas stay prospective.

The tables are separate impact queues: primary reads, other warm workloads,
then cold start/module work. Both primary-pair controls are established;
P61 roundtrip and the bitmap form are rejected, while P55 is retained with
adverse RSS. P11 is measured/mixed; independent P05 design review is next,
with its frozen hypothesis and unknown actual benefit. No P11 stack is assumed.
A future control blocker must be explicit; do not keep an obsolete baseline prerequisite. Continue an
already-running prototype before resuming the sorted queue. Within each
workload queue, prioritize applicable
opportunities with measured component evidence, considering credible removable
cost, confidence and experiment cost/risk; then investigate source-backed
unisolated mechanisms. Speculative upper ranges alone do not outrank measured
component evidence. P02 leads the mixed-large investigation (historical UTF-8
self 6.7–7.1%); P03 is the first signed-specific implementation experiment
(decoder self 1.5–1.9%, small bounded change); P01 follows (timer self 1.9–2%,
high lifecycle risk). These historical shares still require current validation.
Source-only ranges elsewhere are investigation sizing assumptions. No global
workload weights or numerical expected-value score are assumed. Current
profiles and complete screens can reorder the queue; unknown/frontier work
remains required before queue exhaustion. In particular P22's uncalibrated
upper sizing value is not a ranking bound: measure current exclusive
crypto/decode/parse cost and representative cache hit/churn behavior first.

Each new or updated entry must retain: stable ID and variant; status (`queued`,
`running`, `measured`, `rejected`, `retained`, `blocked` or `superseded`); workload;
original frozen prediction or unknown plus required measurement, named metric
and reference; calculation/basis; confidence; cost/risk; inspected source/baseline
identity; dependencies, alternatives and overlaps; then adjacent measured
throughput gain against the same reference and every adverse metric,
source/harness/baseline/prerequisite refs, raw path and digest, artifact/build
identity and protocol; actual implementation assessment, prototype-versus-keeper
burden, estimate revision/basis and carry-forward tradeoff/recommendation.
Do not renumber IDs when sorting. `retained` means a
prototype worth considering later, not production acceptance. Result updates
belong alongside the relevant entry or a linked existing §2.11 finding, not in
an additional evidence packet.

## Inspected source and existing measurements

The discovery inventory inspected the active 9.2a3 branch at `26e58372`, after
production implementation `e4dc2d39` and test refinement
[`c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0`][source]. Later HEAD
`6f55028e` contains test-only corrections. Source links below are pinned to
c9d5 for production code; line numbers are inspection leads, to be checked at
B before editing. **This branch remains unmerged.** The final full gate at
`0fceca62204cdc74a813871b3cf4298ea74c1c5c`, tree
`bdbec09f4b1ba6f2732d8722e6f2c68ffad46e13`, passed **93/93 in 360 seconds**,
exited 0 and completed owned-service cleanup. The prior 6f55028e gate passed
92/93 checks in 706 seconds, with an unchanged native HTTPS reuse fixture's
five-second connect-or-ready timeout. Five bounded isolated runs and the full
15-test suite passed on both the package-only and exact preserved workspace
artifacts, without edits or budget relaxation. That earlier timeout's cause
remains unlocalized; the successful retry does not establish its cause or erase
the earlier failed attempts. [Keeper PR #377](https://github.com/richardwillars/jadpo/pull/377),
opened at source `4fda5e597a3f508e1666ef46de0e0a3b850179b3`, remains unmerged
and awaits manual review; its exact gated source remains 0fce above.
Do not infer a main completion tick or current production deployment from the
passing local gate.

The exploration baseline B is now frozen at a3 production source
`4fda5e597a3f508e1666ef46de0e0a3b850179b3`, with shared harness H84
`84e1c00bc97fb565bcb5810a01b6028b0da803a9`; the exact immutable refs,
builds and archive identities are in the P02 result below. Production behaviour
is unchanged by that harness/artifact remap. **The documentation PR starts from current main; experiments
start from B, which can be an explicitly authorized unmerged PR stack.** Main
does not yet contain this Phase 9 harness or the source PR stack (the Phase 9
source PRs through #375 were open at inventory time). Do not copy its unmerged completion ticks or try
to reproduce a3 by silently substituting main.

The existing confirmation medians provide context, not a substitute for a
fresh matched baseline: golden signed read workerd/Bun **2,296/6,466 req/s**,
large ASCII **1,301/6,170**, mixed Unicode **796/1,636**. a3's test-only changes
do not by themselves qualify a new measured build. The selected a3 source,
all rounds, p95/RSS/cold adverse observations, raw/build hashes and failed
attempts are in the branch's [§2.11 finding][a3-finding] and
[benchmark method][a3-method]. Historical function-cost shares used below are
**pre-a2/a3**, not current cost measurements.

Existing a2 [PR #375][a2-pr] selects the generic first-escape JSON writer:
large ASCII/late-escape throughput about +16%, with small signed/mixed adverse
results retained. Os→O2/O3 screens grew compressed artifacts about 23–26% and
were rejected as a keeper choice. Refer to its [§2.11 finding][a2-finding] and
[method][a2-method] rather than duplicating evidence here. These are existing
measurements, not experiments counted as new queue rows.

Raw benchmark data and immutable snapshots from retired worktrees live at
`/Users/richardwillars/Documents/Codex/jadpo-benchmark-archive/<old-basename>`;
old root paths may be compatible aliases to those archives. Read old tracked
source with `git show <retained-commit>:<path>`, not from removed checkouts.
A new experiment's archive record must name its exact location and hashes.
Retire an old source worktree only after checking its exact PR state and
retaining its refs/data; do not disturb another session's active main, a3 or
scratch worktree.

## Workerd primary control — faithful port (2026-10-24)

**Provisional existing-app control, not an optimization result or strongest
TypeScript baseline.** The missing runtime port now executes on real workerd
and passes the local seam/body/state/security/cleanup/provenance checks and
agent design review. Acceptance means those local checks and review under
Richard's standing unattended authorization; it does not add a new Richard
permission gate. The separately validated one-option control follows below;
neither run establishes the strongest global TypeScript baseline. P61 remains
paused during native-pair preparation at the time of this first screen. The
current native/Bun control is now recorded below; eligible P61/queue work resumes.

Control source is `fda8fc4b12991b9046290467a173ba56bc512afa`, tree
`3468359bf6e444fb8388f3e5befc0e1ce097cd75`, retained under
`refs/experiments/a4-baseline-84e1c00b/ts-workerd-seam-fda8fc4b`.
Only six adapter/harness/test files differ from H84; original TypeScript app
and Bun sources are unchanged. The Jadpo arm consumes frozen B Wasm/JS; the
TypeScript arm consumes its distinct bundle. Both use the same cached workerd,
pair flags and helper topology, with explicit new per-app nodejs_compat
configurations: golden also retains its original global_fetch_strictly_public
flag; focused originally had no flags.
This is a newly labelled control configuration, not a replacement for earlier
B captures. Invocation-owned postgres/CF 3.4.7 clients use the same external
transaction-pool helper caps (golden 10 / focused 20), with singleton application
state and awaited request cleanup. Each implementation retains its existing
schema/authentication: TypeScript golden HMAC and Jadpo BLAKE3 are distinct.

Untimed acceptance checked 16 concurrent exact owner reads, persistent quotas
(120 unauthorized responses then 429), spoofed peer rejection, partial-body
abort cleanup and a following exact read. This is not a blocked-SQL-cancel
claim. Both apps report one application instance, zero active clients and zero
close failures after probes. Golden's untimed census finds one business SELECT
and one lifecycle catalog query, with zero authority reads. Initial readiness
was separately body-verified; the census records first catalog+select1 and
second no SQL, without retaining status/body for those census calls. This is
a limited cache probe, not complete cache proof. Focused catalog queries are
separately recorded and excluded only from the existing business-query probe,
not from timed work.
Default fetch_types and other driver options are unchanged. Six golden client
objects are constructed for each request; startup/catalog/close costs remain
inside service/helper CPU. Helper CPU near 1.2–1.3ms per TypeScript request is
material and motivated the separately validated tuning below, not a
language-win claim.

Before implementation/measurement, the explicit Workerd-only URL
`fetch_types=false` variant's prediction was frozen **unknown**, including
possible loss. The following source-based scope was recorded before its result:
Cached postgres 3.4.7 fetchArrayTypes populates SQL-array OID parsers,
serializers and its type map; neither app's schemas/queries use PostgreSQL
arrays or custom types. Built-in scalar/JSONB handling and Drizzle timestamp
handling remain. Keep original application/npm sources, all other driver
options and the six golden client objects unchanged. Golden discovery uses a
named prepare:true path while Drizzle unsafe business queries default to
prepare:false; the helper intercepts Terminate and reuses backends without
DISCARD. Removing discovery may remove query/row/codec setup and a possible
retained-named-statement startup path. This is not a measured prepared-statement
leak claim without a direct census, and focused prepare:false means that golden
path cannot explain every cost. Validate and label this variant separately.

**Adapter/control assessment — complex; not a Jadpo optimization.** Retained
fda8fc4b adds request-owned client/AsyncLocalStorage facades, separate Worker
entries, compatibility/configuration binding and lifecycle/security/provenance
checks across two existing apps. Manual benchmark probes are prototype control
scaffolding, but faithful application state, cancellation/cleanup and platform
compatibility would remain maintenance obligations in a proper port. **Advice:**
retain as the labelled default-options reference, not code to add to a Jadpo
keeper; interpret its startup overhead with the distinct option control below.

The fixed 12 cells cover three workloads × Jadpo/TypeScript workerd × two
whole-order reversed rounds, c16, 1s warmup + 2s admission, 2048 principals,
inspector NONE and PostgreSQL duration logging off. All cells complete with
zero load/warmup errors, invalid bodies and state changes; exact body identities
are 166/65610/152992 bytes. All 66 immutable served copies were verified. Sleep
observation is empty, and all 24 host/helper PIDs plus owned PostgreSQL were
reaped. This short screen is not 176-case qualification, a full gate, capacity
confirmation or intrinsic language/engine evidence.

Medians are per implementation over both rounds. CPU columns keep service
and external helper separate; the sum is their comparable combined scope,
excluding PG and load-generator CPU. RPS ratio is a ratio of medians, not an
optimization gain, forecast accuracy result or pooling-adjusted language score.

| Workload | Jadpo / TS req/s | Jadpo / TS p95 ms | Jadpo / TS service CPU ms/request | Jadpo / TS helper CPU ms/request | Jadpo / TS combined CPU ms/request | Jadpo/TS RPS ratio |
|---|---|---|---|---|---|---|
| Signed golden | 2292.015 / 460.612 | 8.7705 / 42.9280 | 0.513311 / 2.290268 | 0.141375 / 1.267217 | 0.654686 / 3.557486 | 4.9760× |
| Large ASCII | 1264.107 / 516.920 | 18.9760 / 42.2220 | 0.810399 / 2.014007 | 0.173509 / 1.294398 | 0.983908 / 3.308405 | 2.4455× |
| Mixed Unicode | 794.183 / 270.508 | 26.3085 / 109.4095 | 1.291105 / 3.719865 | 0.209429 / 1.240310 | 1.500534 / 4.960175 | 2.9359× |

Every following cell is **req/s / p95 ms / service CPU ms/request / separate
helper CPU ms/request / sampled peak RSS MB**. Focused raw names peak_rss_mb
and golden sampled_peak_rss_mb are sampled process observations, not lifetime
maxima; neither includes the helper's RSS.

| Workload / round | Jadpo workerd | TypeScript workerd |
|---|---|---|
| Signed golden / 1 | 2309.519 / 8.709 / 0.578959 / 0.159862 / 315.7 | 431.841 / 43.225 / 2.415902 / 1.294597 / 682.9 |
| Signed golden / 2 | 2274.511 / 8.832 / 0.447663 / 0.122888 / 314.2 | 489.382 / 42.631 / 2.164634 / 1.239837 / 691.0 |
| Large ASCII / 1 | 1284.983 / 18.499 / 0.795807 / 0.170807 / 356.2 | 513.065 / 42.085 / 2.026144 / 1.288515 / 384.5 |
| Large ASCII / 2 | 1243.231 / 19.453 / 0.824990 / 0.176211 / 375.3 | 520.775 / 42.359 / 2.001871 / 1.300281 / 394.0 |
| Mixed Unicode / 1 | 788.835 / 26.833 / 1.302706 / 0.207678 / 291.9 | 266.389 / 111.183 / 3.754579 / 1.282051 / 316.1 |
| Mixed Unicode / 2 | 799.532 / 25.784 / 1.279503 / 0.211180 / 295.9 | 274.628 / 107.636 / 3.685152 / 1.198569 / 319.5 |

Cold/first fields are the existing harness observations, **not a dedicated
cold-start benchmark**. Each cell is cold ms / first ms / idle RSS MB.

| Workload / round | Jadpo workerd | TypeScript workerd |
|---|---|---|
| Signed golden / 1 | 61.614 / 26.615 / 97.5 | 54.898 / 35.378 / 59.9 |
| Signed golden / 2 | 62.166 / 26.947 / 97.4 | 57.866 / 38.035 / 60.1 |
| Large ASCII / 1 | 60.168 / 36.808 / 93.7 | 52.505 / 38.547 / 58.4 |
| Large ASCII / 2 | 58.722 / 35.871 / 93.8 | 52.738 / 39.746 / 58.5 |
| Mixed Unicode / 1 | 63.532 / 36.336 / 93.8 | 52.568 / 36.609 / 58.5 |
| Mixed Unicode / 2 | 57.746 / 34.795 / 93.9 | 56.629 / 42.636 / 58.5 |

Preserve variation and host limits: TypeScript signed RPS is 431.841/489.382
across rounds. Jadpo golden round1 has a 622.664ms clock gap (within the frozen
1s guard), with service+helper CPU 0.578959+0.159862ms/request versus
0.447663+0.122888 in round2; other gaps are 4.926–9.733ms. Host load changes
across the declared schedule (load1 roughly 3.84–14.09), and per-row host CPU,
load-generator CPU, elapsed/drain windows and complete status/body data remain
in raw. Jadpo idle RSS is higher in every cell; its cold harness observation is
higher in every corresponding cell. TypeScript golden sampled peak RSS reaches
691MB. None of those observations is discarded because throughput favors Jadpo.

Archive paths below are relative to A, the durable campaign archive already
identified for P02. They retain complete scalar rows, source/artifact maps,
metafiles and served identities; setup failures are not performance screens.

| Record | Archive-relative path | SHA-256 |
|---|---|---|
| Frozen control build | `controls/ts-workerd-seam-fda8fc4b/build.json` | `e3299af17583c87d017cba24408301f6a0b62bb7579768a27d5551b0cfae384a` |
| Guarded seam setup | `controls/ts-workerd-seam-fda8fc4b/seam-setup/results.json` | `76f2da8523ed2cd8b5ec897f6dd59b02e8922e9dde638e14f28be5a468bd2bec` |
| Complete fixed 12 paired result | `controls/ts-workerd-seam-fda8fc4b/paired12-none-pg-off-c16-r2/results.json` | `73f3d25534f84a29a7843442c070c4c6f228568b93547b17ac076cbfd94b9295` |
| Failed neutral-bundler/main-fields setup, source 340724e0 | `controls/ts-workerd-340724e0/build.json` | `f44bb312c7220be366ce53674ed2af9d45e96471952801d4b9c70dfb4f68ab18` |
| Golden bundle passed, focused flags assumption rejected, source aae76127 | `controls/ts-workerd-mainfields/build.json` | `de28103664bb7379ca7ffd2bbd44ee988305c4e60dd52e3ba644fa8a4c1eb7c3` |
| Corrected per-app-flags setup, source 74eccf8d | `controls/ts-workerd-per-app-flags/build.json` | `2e8bdb54e9907d8b6a9c9bb97d49fd00c005ab8c08c01f66f86c9dc65b7a4965` |
| Initial compatibility setup at 74eccf8d | `controls/ts-workerd-per-app-flags/compatibility-setup/results.json` | `0210bb32b93dbfb39783533652aad02bd5531ff1be9dd040e4058f24f98c307c` |

This adds no optimization ID or gain forecast. All 61 original predictions,
F01 and previous results remain unchanged. The faithful port is useful evidence;
its separately labelled platform tuning is measured below, without a
strongest-TypeScript or general performance conclusion.

## Workerd primary control — explicit no-array discovery (2026-10-24)

**Second provisional platform-option control; not a strongest global
TypeScript baseline or intrinsic language win.** The frozen prediction was
**unknown, including loss**. Source `b7fbec9bacf10c1715c463a6eaa4d0bdd577eb87`,
tree `b4d4495a90a70c6f4788ee1ecf73cc8a60fac141`, is retained at
`refs/experiments/a4-baseline-84e1c00b/ts-workerd-no-array-b7fbec9b`.
The only runtime-setting change from the faithful port is binding-derived
Workerd URL `fetch_types=false`. Cached postgres/CF options are deep-equal
except that boolean, which is omitted from server startup parameters. Original
app/npm sources, all other options, six golden client objects, business SQL,
authentication, validation, helper caps/topology, singleton state and invocation
lifetimes are unchanged. No lazy construction or cross-request I/O was added.

Remaining adapter costs are explicit: golden constructs six Sql objects and
ten Connection state objects per invocation; focused constructs up to twenty
Connection states. These are object counts, not ten/twenty opened sockets.
Object/queue/parser/Drizzle recreation and AsyncLocalStorage/facade work remain.
Optional lazy-client, pool-max or minification reference variants have unknown
benefit and are not mandatory before further Jadpo work. Application creation
is once per isolate; focused's three schema queries run/check once and readiness
caching is validated in the tuned seam. There is no accidental per-request app,
readiness or schema-startup rerun. HMAC versus BLAKE3 and existing ORM,
validation and policy implementations still differ, so this deployment pair
is not universal engine evidence.

Actual workerd seam probes verify built-in UUID/text (é/😀), timestamps/NULL,
int8/int4 and nested JSONB including backspace/form-feed. Neither schema/query
uses PostgreSQL arrays/custom types. The untimed golden census now has exactly
one business SELECT, no authority reads and zero catalog queries. Readiness
status/body are checked here; first SQL is select1, then none. Focused catalog
census is also empty. Sixteen concurrent owners, quota/trusted-peer checks,
partial-body abort followed by an exact read and cleanup all pass. Both apps
end with opened=closed, active 0, closeFailures 0 and one application instance.

Disabling fetchArrayTypes removes the discovery query, row transfer, parser/
serializer/map setup and a possible golden named-prepared startup path as one
combined option effect. Golden discovery used prepare:true while Drizzle unsafe
business SQL defaults prepare:false; the helper reuses backends without
DISCARD/DEALLOCATE. This supports a possible retained-statement mechanism,
**not a measured prepared-statement leak** without a direct census. Focused's
prepare:false also prevents assigning its costs to that golden-only path.

**Adapter/control assessment — simple increment on the complex port.**
Retained b7fbec9b adds one explicit client URL option while preserving the
adapter's existing control flow, app state and client lifetimes. The array-free
workload assumption and actual scalar/JSON/census tests are lasting semantic
requirements, not removable test clutter; no additional runtime interface is
needed. **Tradeoff / advice:** retain this as the current scoped platform-option
reference alongside the default port, without declaring a strongest universal
TypeScript baseline or promoting a Jadpo optimization.

The fixed 12-cell protocol is unchanged: three workloads × Jadpo/TypeScript
workerd × two full-order reversed rounds, c16, 1s warmup + 2s admission,
2048 principals, inspector NONE and PG duration logging off. The Jadpo arm
consumes frozen B Wasm/JS; both arms retain the new pair's nodejs_compat and
per-app original flags, cached workerd and same helper topology. All 12 cells
complete with zero load/warmup errors, invalid bodies and state changes;
all 66 immutable served copies and exact bodies pass. Sleep observation is
empty; all 24 host/helper PIDs and owned PostgreSQL/workerd are reaped.
This screen itself supplies no native primary result, full qualification or
gate claim; the separately measured native pair follows below.

Medians below are within this screen. The service/helper sum excludes PG and
load-generator CPU, and the RPS ratio is a ratio of medians, not an optimization
gain. TypeScript medians in the earlier default-port screen were
460.612/516.920/270.508 req/s, versus 793.541/902.127/359.782 here (signed/ASCII/
Unicode). Those are **separate screens, not a matched default-versus-tuned
marginal estimate**; retain both, and do not attribute a pure query-removal gain
or compare cross-campaign absolute rates as a controlled causal effect.

| Workload | Jadpo / tuned TS req/s | Jadpo / tuned TS p95 ms | Jadpo / tuned TS service CPU ms/request | Jadpo / tuned TS helper CPU ms/request | Jadpo / tuned TS combined CPU ms/request | Jadpo/tuned-TS RPS ratio |
|---|---|---|---|---|---|---|
| Signed golden | 2276.299 / 793.541 | 8.9610 / 28.8465 | 0.448529 / 1.350737 | 0.123915 / 0.131942 | 0.572444 / 1.482679 | 2.8685× |
| Large ASCII | 1251.125 / 902.127 | 20.5750 / 27.4525 | 0.812897 / 1.138229 | 0.177433 / 0.164535 | 0.990330 / 1.302764 | 1.3869× |
| Mixed Unicode | 796.610 / 359.782 | 26.7070 / 99.5560 | 1.282694 / 2.849962 | 0.208588 / 0.212274 | 1.491282 / 3.062236 | 2.2141× |

Every cell below is **req/s / p95 ms / service CPU ms/request / separate helper
CPU ms/request / sampled peak RSS MB**, with the same raw RSS naming and
sampling limits as the first control.

| Workload / round | Jadpo workerd | Tuned TypeScript workerd |
|---|---|---|
| Signed golden / 1 | 2287.473 / 8.870 / 0.445318 / 0.124427 / 316.5 | 780.891 / 29.379 / 1.365667 / 0.134014 / 624.5 |
| Signed golden / 2 | 2265.125 / 9.052 / 0.451741 / 0.123402 / 316.1 | 806.190 / 28.314 / 1.335807 / 0.129870 / 630.4 |
| Large ASCII / 1 | 1289.124 / 18.346 / 0.796905 / 0.170213 / 364.7 | 895.788 / 27.876 / 1.137493 / 0.160133 / 383.7 |
| Large ASCII / 2 | 1213.127 / 22.804 / 0.828888 / 0.184653 / 361.8 | 908.465 / 27.029 / 1.138965 / 0.168937 / 379.1 |
| Mixed Unicode / 1 | 795.193 / 26.191 / 1.285090 / 0.205864 / 309.4 | 365.934 / 96.105 / 2.807018 / 0.215924 / 309.9 |
| Mixed Unicode / 2 | 798.027 / 27.223 / 1.280298 / 0.211311 / 303.4 | 353.630 / 103.007 / 2.892907 / 0.208623 / 320.2 |

The harness cold/first observations remain separate from a dedicated cold-start
benchmark. Each cell is cold ms / first ms / idle RSS MB.

| Workload / round | Jadpo workerd | Tuned TypeScript workerd |
|---|---|---|
| Signed golden / 1 | 62.063 / 30.985 / 97.4 | 56.188 / 36.672 / 59.0 |
| Signed golden / 2 | 62.636 / 30.663 / 97.4 | 53.314 / 33.544 / 59.0 |
| Large ASCII / 1 | 62.376 / 35.541 / 93.8 | 49.157 / 35.404 / 53.5 |
| Large ASCII / 2 | 58.194 / 34.843 / 93.8 | 49.201 / 35.857 / 53.5 |
| Mixed Unicode / 1 | 58.611 / 35.171 / 93.8 | 49.029 / 35.388 / 53.5 |
| Mixed Unicode / 2 | 58.774 / 32.934 / 93.8 | 49.422 / 35.870 / 53.5 |

The large helper-CPU discrepancy in the first port is absent in this control:
median helper CPU is roughly 0.12–0.21ms/request in both arms. Other differences
remain. Jadpo ASCII round2 p95 is 22.804ms versus 18.346 in round1; TypeScript
Unicode p95 increases 96.105→103.007ms. TypeScript signed peak RSS is
624.5/630.4MB versus Jadpo 316.5/316.1; Jadpo idle RSS and harness cold observations
are higher in every paired cell. Clock gaps are 5.131–11.204ms. Load1 varies
roughly 2.98–12.00 across the schedule, with per-row host/load-generator CPU,
elapsed/drain scope and all status/body observations retained in raw. Do not
turn either two-round screen into a capacity or statistical-confidence claim.

Archive-relative identities use the same durable A as the first control:

| Record | Archive-relative path | SHA-256 |
|---|---|---|
| Explicit-option control build | `controls/ts-workerd-no-array-b7fbec9b/build.json` | `ecbc40f1569abbd9e7b10d4d4ff90b4bba90f7c038b173fa48051ecaa990a3d7` |
| Actual codecs/census/seam setup | `controls/ts-workerd-no-array-b7fbec9b/seam-setup/results.json` | `5b2892c137381927ae6809079a84376cca8bc3c5bdb8e32bf7ea3986230d3e50` |
| Complete explicit-option fixed12 result | `controls/ts-workerd-no-array-b7fbec9b/paired12-none-pg-off-c16-r2/results.json` | `c1f3dd30086c2ec7ebef29a23ad036945c39a56be8e652ccbfc34faec782bcc1` |

The current native/Bun production pair follows below, completing the local
primary controls. The earlier 288a6778 binaries remain historical; the fresh
producer/source chain below is explicit. P61 roundtrip's later result is retained
below; continue the eligible queue under standing authorization. No original forecast, F01 or prior result is
rewritten, and exploration is neither exhausted nor promoted.

## Standalone primary control — current native/Bun (2026-10-24)

**Provisional matched standalone control, not an optimization result, capacity
claim or intrinsic language/engine win.** Both primary deployment pairs now
have guarded local results. P61 roundtrip's subsequent result is retained below;
continue eligible exploration under standing authorization, with no additional baseline
prerequisite or Richard permission is introduced. Exploration remains uncapped,
with no production promotion from these control results.

The actual native producer is `2626bbc0caa232105aa75cf8d82eafcd2d05380c`,
tree `dde2fe98fa2236f9de880b14553d4b336e7fae0f`; run/reader harness is
`f17d3e0cafbca67e3b0aaf07e5744189e9742281`, tree
`7fa9d476f5ebbf797eeab2958b4a1cb8d543becd`, retained at
`refs/experiments/a4-baseline-84e1c00b/native-bun-loader-context`.
The original producer ref is `refs/experiments/a4-baseline-84e1c00b/native-bun-2626bbc0`.
Fresh native golden/focused commands succeeded in 128.8s/66.2s, shipping
native opt3/fat LTO/one codegen unit, Rust 1.99, offline four Cargo jobs, using
unchanged B production crates/lock/fixtures and the saved B compiler. No
compiler/dependency/production code changes, Wasm build, 176-case qualification
or full gate were part of this control. Historical 288a6778 binaries were not
reused. Native golden buildstamp is b_46b4b0bb629f; focused b_f824df8b759e.
Generated build provenance and hashes establish identity; startup commit:null /
modified:true does not pretend a generated project is a clean source commit.

The original build record remains complete:false after a final reader looked
for build/rust/Cargo.toml rather than actual build/rust/app/Cargo.toml, after
both native binaries had built. The derived complete record preserves that
producer hash/source/failure and revalidates the same actual native bytes,
arguments, profiles and sources with only two reviewed helper/test changes.
Reader corrections also locate focused logs/native.log and match the actual
runtime.started port/build/worker event. A 90ceb596 setup failed before app
start because copied psql could not find @loader_path/../lib/libpq.5.dylib;
its failed raw is retained separately. The final reader executes the original
baseline-pinned Postgres.app 16.3 psql path/hash, with no copied-library or PATH
fallback. Frozen Bun 1.2.20 and Node 24.18.1 copies were probed before database
work and rehashed at the end; none was silently substituted.

Natural persistent direct-PG pools are retained in both arms: native has ten
actual workers, each driver max 10 open/8 idle; Bun golden has five general
plus five exclusive clients, focused max 20. No artificial one-core cap or
external transaction helper is introduced. Post-load connection observations
are native 16 for all workloads, Bun 5 signed / 16 focused; these are observations
outside timing, not every connection's lifetime maximum. App CPU includes the
entire server process and all native threads. Helper CPU is 0 in both arms;
PostgreSQL appears only in whole-host utilization, not the app-CPU column.

All four setup hosts pass exact body/auth/privacy/state guards. The fixed 12
screen covers three workloads × native/Bun × two whole-order reversed rounds,
c16, 1s warmup + 2s admission, 2048 principals, inspector NONE and PG durations
off. All 12 rows complete with zero load/warmup errors, invalid bodies and
state changes; current binary/tool end hashes, full-body identities, source/
shared/dependency/artifact guards and Sleep checks pass. All owned server
processes and PostgreSQL are reaped. Actual argv0 is the current native binary
or frozen Bun executable, not a stale path or generic footer label. The
prototype's generic stdout footer still says workerd; authoritative raw
kind/protocol/runtime/commands correctly identify standalone native/Bun.

**Adapter/control implementation assessment — moderate; no Jadpo optimization
implementation.** Producer/build-source/run-harness identity, native log/port
validation, installed psql loader context and owned process/pool accounting
need explicit branches and negative controls. Wrong profile/log paths and the
copied-psql attempt are resolved prototype reader roughness, not changes a
proper production executable needs. The lasting burden is trustworthy
cross-target provenance and resource accounting, not a new runtime interface
or changed shipping profile. **Tradeoff / advice:** retain this primary control
and its limits for human interpretation, not code to promote as a performance
keeper. It has no per-optimization gain forecast.

Medians use this screen's two rounds. The ratio is native median RPS divided
by Bun median RPS; it is not an optimization gain or forecast-accuracy score.

| Workload | Native / Bun req/s | Native / Bun p95 ms | Aggregate native / Bun app CPU ms/request | Native/Bun RPS ratio |
|---|---|---|---|---|
| Signed golden | 39172.219 / 6259.024 | 0.7815 / 3.6825 | 0.101247 / 0.196808 | 6.2585× |
| Large ASCII | 23354.605 / 5865.204 | 1.0260 / 4.3220 | 0.174534 / 0.219791 | 3.9819× |
| Mixed Unicode | 4945.083 / 1644.350 | 4.2055 / 12.0725 | 0.376655 / 0.723739 | 3.0073× |

Each cell below is **req/s / p95 ms / aggregate app CPU ms/request / sampled
peak RSS MB**. Focused peak_rss_mb and golden sampled_peak_rss_mb have the
existing process-sampling limits; PG/load CPU and RSS are separate.

| Workload / round | Jadpo native | TypeScript/Bun |
|---|---|---|
| Signed golden / 1 | 36265.749 / 0.970 / 0.101869 / 15.8 | 6130.536 / 3.825 / 0.199609 / 204.0 |
| Signed golden / 2 | 42078.689 / 0.593 / 0.100625 / 15.8 | 6387.512 / 3.540 / 0.194008 / 203.0 |
| Large ASCII / 1 | 23620.541 / 1.008 / 0.175008 / 28.0 | 5964.795 / 3.982 / 0.216918 / 197.3 |
| Large ASCII / 2 | 23088.668 / 1.044 / 0.174060 / 28.0 | 5765.614 / 4.662 / 0.222665 / 189.5 |
| Mixed Unicode / 1 | 4941.086 / 4.212 / 0.377969 / 25.9 | 1636.887 / 12.125 / 0.733191 / 174.8 |
| Mixed Unicode / 2 | 4949.079 / 4.199 / 0.375341 / 25.0 | 1651.813 / 12.020 / 0.714286 / 185.1 |

Harness cold/first observations are not a dedicated cold-start benchmark.
Every cell is cold ms / first ms / idle RSS MB.

| Workload / round | Jadpo native | TypeScript/Bun |
|---|---|---|
| Signed golden / 1 | 23.710 / 2.069 / 11.8 | 88.461 / 13.915 / 92.5 |
| Signed golden / 2 | 18.008 / 2.020 / 11.8 | 91.194 / 13.829 / 89.6 |
| Large ASCII / 1 | 17.643 / 1.909 / 11.4 | 90.584 / 4.164 / 90.4 |
| Large ASCII / 2 | 17.299 / 1.847 / 11.3 | 84.415 / 5.762 / 89.5 |
| Mixed Unicode / 1 | 17.343 / 1.969 / 11.4 | 84.347 / 3.949 / 89.4 |
| Mixed Unicode / 2 | 17.514 / 1.913 / 11.3 | 88.259 / 6.238 / 88.8 |

**Concrete limits:** the two-thread load generator consumes 1.9884/1.9891 CPU
cores for native Unicode, so that result encounters a client ceiling. Native
signed/ASCII whole-host CPU is about 99.4–99.6%, including shared-machine and
PG pressure; it is not service CPU or proof that the native server has spare
machine capacity. Native signed varies 36265.749→42078.689 req/s, p95
0.970→0.593ms, while Bun ASCII p95 worsens 3.982→4.662ms. Clock gaps are
4.802–10.553ms and load1 endpoints vary roughly 3.85–15.42. Retain every round,
loader CPU, host/load/drain windows and unchanged distinct application-stack
semantics; do not infer an intrinsic engine comparison or confirmed capacity
from these two short reversed rounds. No further baseline measurement is
required merely to resume eligible exploration.

All paths below are relative to durable archive A used by the other controls.
The retained record chain distinguishes failed setup from actual measurements:

| Record | Archive-relative path | SHA-256 |
|---|---|---|
| Original native producer, complete:false reader failure retained | `controls/native-bun-2626bbc0/build.json` | `2b72663710afe22866d2f4ea6f20ddac7b88d3917a469700d9a4a80b13b0b781` |
| Derived current reader, complete:true, same built bytes | `controls/native-bun-loader-context/build.json` | `9d66e65b13d8e68fdf040bfda9981d2e57060825096abffaa594f96092d0b9e1` |
| Failed 90ceb596 copied-psql setup, no performance result | `controls/native-bun-reader-90ceb596/setup-smoke/results.json` | `800637dd903124c5cf3957818d254af3af48d4cd1b84619d4dbe058dab5989a9` |
| Final four-host setup | `controls/native-bun-loader-context/setup-smoke/results.json` | `1987b21a37c556b4eae52c91009dbecf0031b4e3d56be407168c0a69911fbed0` |
| Complete fixed 12 standalone pair | `controls/native-bun-loader-context/paired12-none-pg-off-c16-r2/results.json` | `01889df3191a1c3ec623c40eebda1523b78df8cb77ae8d65c57ae0cf28e0a7e1` |
| Actual golden native executable | `controls/native-bun-2626bbc0/golden/native/build/target/app/app` | `239b02f6b9b8ff141c6fee082c8b01c41f07bc75c39aa3af771b358cb622cb77` |
| Actual focused native executable | `controls/native-bun-2626bbc0/focused/native/build/target/app/app` | `e505ba4891c07db4f457d9be6bdd9497d33ec5102af63c41cf14ce7fe15ef63e` |

## Primary read queue

P02 is measured/mixed and retained as an exploratory prototype, with no
production promotion or favorable rerun. The current-B CPU diagnostic below
supported P03's independent signed-read experiment, now measured small positive
with limited causal attribution; its original 0.5–2% prediction is unchanged.
P21 first-only is now a promising measured ASCII prototype; the current
P21.suffix form is negative/rejected. Neither is a production promotion.
Richard's later comparison-method correction supersedes the earlier P61-next
priority: locally reviewed primary controls now have two provisional screens,
and the current native/Bun pair is recorded above. P61 roundtrip is now
measured/rejected; its independent host-sink alternative stays unmeasured.
P55 remains measured/promising with adverse RSS; P21.suffix-bitmap is now
rejected. P11 is measured/mixed; independent P05 design review is next from
H84/B4fda, with no P11 stack or baseline prerequisite. Preserve original forecasts/results and continue
eligible ideas without an arbitrary promotion boundary.
P13 chain reuse remains a separate hypothesis; P59/P60 need a callback/I/O census
before structural implementation. These are priorities, not a batch limit.
F01 below remains frozen for the initial 54 ideas; later discoveries expand the
queue without retroactively changing that forecast.

| Candidate / current status | Applicable workload / experiment | Predicted throughput gain (hypothetical) | Measured throughput gain | Basis, next check and pinned source | Confidence; cost; risk | Dependency / alternative / overlap IDs |
|---|---|---|---|---|---|---|
| **P02 — Safe SIMD UTF-8 validation** (measured/mixed; prototype retained) | Mixed large / ASCII reads; E | 2–6% mixed; 0–1% ASCII | P/F mixed +6.071%; ASCII −0.586%; signed control +0.103%. Combined P/B +6.550% / −0.968% / −1.554% (mixed/ASCII/signed). [All rounds, predictions and adverse metrics](#p02--simd-utf-8-first-screen-2026-10-24). | Historical mixed self 6.7–7.1%, ASCII ≈0.55%; conditional full-removal ceiling ≈7.2–7.7%, not achievable validation removal. Verify cached safe API and Wasm SIMD support; identical-feature control, no handwritten unsafe validator or download. [postgres/wire.rs:607](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L607), [postgres/wire.rs:648](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L648). | Medium; S–M; medium | Depends on F (same-feature control); F/B and combined P/B predictions unknown; overlaps P04 |
| **P03 — Signed base64url table decoder** (measured small positive; prototype retained) | Signed reads; E | 0.5–2% | Signed +2.0625% vs B (marginal=combined); ASCII/mixed controls +1.4274%/+1.3656%, no direct decoder attribution. [All18 rows and limits](#p03--signed-base64url-first-screen-2026-10-24). | Linear alphabet search, intermediate `Vec<u32>` and full canonical re-encode; historical self 1.5–1.9%; current D signed self 1.5216%, ancestor union 1.9410% of all weighted samples (diagnostic scope, not removable headroom). Preserve canonical alphabet/length/tail bits, UTF-8 and constant-time tag comparison. [http/signed.rs:319](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/signed.rs#L319), [encoding.rs:124](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/encoding.rs#L124). | Medium; S; medium parsing risk | Independent; may supply P24 helper; overlaps P07/P23/P24; masked by P22 |
| **P01 — Timer cancellation** (queued; unmeasured) | Signed/small reads; E + timer controls | 0–3% | Not measured | Promise timers resolve on early cancellation. Historical self 1.9–2%; GC/microtask saving unknown. Retain settlement and 8.4e no-leak guarantees. [timer.rs:34](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L34), [timer.rs:73](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L73), [timer.rs:85](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L85). | Medium-low; M; high lifecycle risk | Independent; overlaps P14; measure both separately then combined |
| **P11 — Skip unneeded scalar count** (measured mixed; prototype retained) | Large text; E | 0–5% | P11/B signed −1.9439%, ASCII +1.4957%, mixed +3.0801%; signed adverse both rounds, mixed tail/RSS adverse. [All18 rows and assessment](#p11--scalar-count-first-screen-2026-10-24). | chars().count runs without dimensions/min/max. Check actual rule chain and compiler output: benefit can be zero if count is otherwise needed/eliminated. [rules.rs:570](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-std/src/rules.rs#L570). | Low; S; low | Independent; overlaps P13/P43; optional P13 stack |
| **P05 — Prepare immutable driver configuration** (selected independent design; unmeasured) | Golden reads; E + many-connections F | 0–5% golden; many connections unknown | Not measured | First DB access builds all configured connections and parses URLs/security. Separate preparation-once and lazy per-driver variants; measure configuration scaling. Sockets remain invocation-local. [factory.rs:204](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/factory.rs#L204), [deployable.rs:325](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/deployable.rs#L325). | Low; M; medium | Independent |
| **P04 — SQL text ownership** (queued; unmeasured) | Large text reads; E | 0–5% | Not measured | Verified raw → String → `Rc<str>` copies, cost unisolated. Profile allocation/copy first; moving String into Rc alone does not guarantee zero-copy. [postgres/wire.rs:607](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L607), [read.rs:1101](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L1101). | Low; M–L; high representation risk | Independent; overlaps P02/P09; representation affects P52 |
| **P13 — Reuse scalar count across rule chain** (queued; unmeasured) | Deep-chain text; E + chain F | 0–5%; current-fixture relevance uncertain | Not measured | Avoid repeated inherited-rule scans. Profile chain count; preserve first-failure order and exact diagnostic count. [validate.rs:265](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/validate.rs#L265). | Low; M; medium | Independent, or explicit prerequisite P11 when stacked; overlaps P43 |
| **P19 — Cheap response capacity hints** (queued; unmeasured) | Multi-field/list responses; E + list F | 0–3% | Not measured | Response String starts empty; nested writer reserves text separately. Use cheap hints; avoid expensive full sizing traversal or unbounded retained buffer. [http/mod.rs:1590](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/mod.rs#L1590). | Low; S–M; low | Independent; overlaps baseline a2 reserve/P04/P53 |
| **P07 — Direct Claims deserialization** (queued; unmeasured) | Signed reads; E | 0–3% | Not measured | serde_json Map then field copies. Preserve historical encodings, optional fields and current duplicate/unknown-field behavior. [http/signed.rs:218](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/signed.rs#L218). | Low; S–M; medium compatibility risk | Independent; overlaps P03/P23; masked by P22 |
| **P06 — Batch request-header bridge** (queued; unmeasured) | Ordinary reads; E + header-heavy F | 0–3%; header-heavy unknown | Not measured | JS iterator → pair Array → separate string conversions/lowercase/trim. Preserve order, duplicates, case and trimming; profile bridge cost for header-heavy bound. [fetch.rs:215](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/fetch.rs#L215). | Low; M; medium | Independent; overlaps P10/P28 |
| **P21 — Vectorize first-escape search only** (first measured/retained; suffix rejected) | ASCII/late-escape reads; E | 0–3%; early-escape mixed ≈0 | First/B ASCII +11.2918%, signed +1.9197%, mixed −0.5716%; suffix/first −0.9633% / −0.9024% / −0.7076% (ASCII/signed/mixed). Bitmap/First signed +0.3103%, ASCII −1.6059%, mixed −21.5197%; bitmap rejected. [Bitmap result](#p21suffix-bitmap--measured-result-2026-10-24). [All24 original rows, separate predictions and limits](#p21--first-escape-and-dependent-suffix-screen-2026-10-24). | Current D ASCII writer self 7.8301%, but only its first-escape scan is targeted; that scan is scalar even in F. Retain the a2 scalar suffix, safe same-feature control and mixed near-zero first-scan scope; no full-writer/old-a2 credit. Conditional P21.suffix is separately predicted below. [json.rs:1624](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json.rs#L1624). | Low; M; medium | Independent; baseline a2 writer retained |
| **P12 — Direct UUID hex formatting** (queued; unmeasured) | Singleton reads; E + UUID-list F | 0–2%; UUID-list impact unknown | Not measured | Sixteen format! allocations then Value::uuid lowercasing. Measure direct hex first; trusted canonical constructor as separate variant. Profile UUID-list share before estimating it. [read.rs:1327](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L1327), [read.rs:1103](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L1103). | Low-medium; S; low | Independent |
| **P10 — Batch response-header bridge** (queued; unmeasured) | Small/header-heavy responses; E + header F | 0–2% | Not measured | Reduce header/JS boundary construction. Keep owned Uint8Array, bodyless statuses and rejected-header behavior; no unsafe Wasm memory view. [deployable.rs:417](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/deployable.rs#L417). | Low; M; medium | Independent; interacts P06/P28 |
| **P15 — Reuse immutable Logged observer** (queued; unmeasured) | Golden reads; E + many-unbuilt F | 0–1%; many unbuilt connections unknown | Not measured | Per-request Logged construction and reserialization of unchanged unbuilt-driver list. Measure list scaling; cached observer must not retain invocation host. [deployable.rs:265](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/deployable.rs#L265), [deployable.rs:158](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/deployable.rs#L158). | Low; S; low | Independent |
| **P23 — Fixed-size Claims hex decoder** (queued; unmeasured) | Signed reads; E | 0–1% | Not measured | Per-byte radix parsing. Preserve accepted case and forms. [http/signed.rs:289](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/signed.rs#L289). | Low; S; low-medium | Independent; P07 may absorb; P22 masks |
| **P14 — Lazy timer after first Pending** (queued; unmeasured) | I/O reads; E + immediate/deadline F | 0–1%; immediate-completion impact unknown | Not measured | Avoid timer creation for immediate completion; retain original deadline origin and left-first priority. Profile immediate-completion incidence. [timer.rs:85](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L85). | Low; S; medium | Independent of P01; separate then combined |
| **P17 — Remove second bound-parameter vector** (queued; unmeasured) | Small reads E; many-parameter F | 0–1%; many parameters unknown | Not measured | Param vector followed by typed-reference vector. Profile scaling; retain extension-type inference. [postgres/driver.rs:986](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L986), [postgres/driver.rs:1007](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L1007). | Low; S–M; medium | Independent; overlaps P16 allocation |
| **P28 — Index common header metadata once** (queued; unmeasured) | Ordinary reads E; many-header F | 0–1%; many headers unknown | Not measured | Repeated linear header/credential lookup. Measure scaling; indexing may lose for short lists. Keep multiplicity and first-value behavior. [http/mod.rs:98](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/mod.rs#L98), [http/identity.rs:143](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/identity.rs#L143). | Low; M; medium | Independent; overlaps P06/P10 |
| **P08 — Receive-buffer offset** (queued; unmeasured) | Large/fragmented reads; E + fragmentation F | 0–0.3% direct | Not measured | Held Vec repeatedly split_offs remainder; historical self 0.12–0.20%. Replace with position and preserve fragmentation/EOF behavior; a cheap small-bound control. [transport.rs:406](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/transport.rs#L406). | Low-medium; S; low-medium | Independent |

## Other warm-workload queue

| Candidate / current status | Applicable workload / experiment | Predicted throughput gain (hypothetical) | Measured throughput gain | Basis, next check and pinned source | Confidence; cost; risk | Dependency / alternative / overlap IDs |
|---|---|---|---|---|---|---|
| **P16 — Direct PostgreSQL parameter encoding** (queued; unmeasured) | UUID reads E; large text/JSON writes F | 0–5% writes; 0–1% current UUID reads | Not measured | Temporary Vec then BytesMut extension. Preserve every SqlValue, encoding errors and buffer rollback. [postgres/driver.rs:1020](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L1020). | Low; S–M; medium | Independent; stacks with P17 but allocation savings overlap |
| **P18 — Share row names and record type Rcs** (queued; unmeasured) | Wide lists F; singleton control E | 0–5% lists; 0–2% singleton | Not measured | Field names allocated per row; Value::record allocates type name. Existing Shape demonstrates immutable name sharing. [read.rs:810](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L810). | Low; M; low-medium | Independent; overlaps P04/P09/P35/P33 allocation |
| **P09 — Raw PostgreSQL row stream** (queued; unmeasured) | Multirow; F, singleton control E | 0–5% multirow; 0–1% singleton | Not measured | Avoid `Vec<tokio Row>` before `Vec<runtime Row>`. Preserve drain, errors, COMMIT completion and connection return. [postgres/driver.rs:689](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L689), [postgres/driver.rs:850](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L850), [postgres/driver.rs:1091](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L1091). | Low; M; high | Independent; overlaps P04; P51 natural implementation site |
| **P25 — Copy incoming body into final bounded Vec** (queued; unmeasured) | Large request bodies; F | 0–5%; GET zero | Not measured | fetch::piece creates temporary Vec then extend. Preserve byte limit, cancellation and partial errors. [deployable.rs:372](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/deployable.rs#L372), [fetch.rs:233](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/fetch.rs#L233). | Low; M; medium | Independent; may supply helper prerequisite for P26 |
| **P26 — Direct-copy full provider response body** (queued; unmeasured) | Provider-heavy routes; F | 0–5% | Not measured | Avoid intermediate body copy; preserve maximum length, timeout and abort semantics. [fetch.rs:260](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/fetch.rs#L260). | Low; M; medium | Independent, or prerequisite P25 only if sharing its helper |
| **P35 — Construct known records into final layout** (queued; unmeasured) | Construction-heavy; F | 0–5% | Not measured | Temporary slot Vec and final fields. Typed HTTP input already uses stack slots; preserve omissions/defaults and faults. [shape.rs:123](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/shape.rs#L123), [compiled.rs:753](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/compiled.rs#L753). | Low; M; high | Independent; overlaps P18/P33 |
| **P53 — Direct quoted numeric JSON output** (queued; unmeasured) | Numeric-heavy responses; F | 0–3%; text cases near zero | Not measured | Temporary to_string then write_string. Preserve integer wire type and decimal scale. [json.rs:1607](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json.rs#L1607). | Low; S; medium | Independent; overlaps P19 allocation |
| **P43 — Precompute validator traits** (queued; unmeasured) | Deep chains/construction; F | 0–3% | Not measured | Repeated secret/standard/vector/sanitizing/rounding scans. Preserve most-specific selection. [validate.rs:163](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/validate.rs#L163), [validate.rs:217](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/validate.rs#L217). | Low; S–M; medium | Independent; overlaps P11/P13 |
| **P51 — Validate result schema once** (queued; unmeasured) | Wide multirow; F | 0–3%; singleton near zero | Not measured | Repeated per-row type checking. Preserve every cell/null/extension OID validation. [postgres/driver.rs:1091](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L1091). | Low; S–M; medium | Independent; P09 natural implementation site, not prerequisite |
| **P27 — Borrow unescaped query strings** (queued; unmeasured) | Query-heavy routes; F | 0–3% | Not measured | Each percent-decoded key/value allocates plus BTreeMap entries. Preserve plus, malformed escapes, UTF-8, duplicate order and sorted unknown fields. [http/mod.rs:648](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/mod.rs#L648). | Low; M; medium | Independent |
| **P44 — Borrow cross-rule record fields** (queued; unmeasured) | Cross-rule-heavy; F | 0–3%; absent rules zero | Not measured | Avoid cloned temporary Vec; preserve named-rule order and errors. [validate.rs:339](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/validate.rs#L339). | Low; M; medium | Independent |

## Cold-start and module queue

| Candidate / current status | Applicable workload / experiment | Predicted throughput gain (hypothetical) | Measured throughput gain | Basis, next check and pinned source | Confidence; cost; risk | Dependency / alternative / overlap IDs |
|---|---|---|---|---|---|---|
| **P46 — Avoid replaced startup defaults** (queued; unmeasured) | Cold; C | Unknown; warm throughput zero expected | Not measured | Default/test configuration immediately replaced by production configuration. Measure replaced construction cost; retain interpreter/test defaults and startup failures. [compiled.rs:196](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/compiled.rs#L196), [compiled.rs:213](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/compiled.rs#L213), [deployable.rs:330](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/deployable.rs#L330). | Low-medium; M; medium | Independent; overlaps P47 |
| **P45 — Deduplicate regex compilation** (queued; unmeasured) | Cold/RSS; C | Unknown; warm throughput zero expected | Not measured | Already once PER PROCESS; identical validators may compile duplicate regex. Count duplicates and measure cold CPU/RSS; immutable sharing only. [plan.rs:55](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/plan.rs#L55). | Low; M; medium | Independent |
| **P47 — Share/emit immutable plan data** (queued; unmeasured) | Large-app cold/RSS; C | Unknown; warm benefit unestablished | Not measured | Plan/place/shape structures reconstructed despite existing process/isolate cache. Measure startup allocations versus artifact growth before redesign. [compiled.rs:143](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/compiled.rs#L143), [plan.rs:72](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/plan.rs#L72). | Low; L; high size/compatibility risk | Independent; overlaps P46 |
| **P57 — Plan-chosen subsystem Cargo features** (queued; unmeasured) | Artifact/cold/start/RSS; C + warm control | Unknown; warm throughput may be zero | Not measured | Explicit roadmap 9.2 target: HTML/regex/Markdown/geodesy/batches/webhooks/sign-in. Runtime has only testing feature and value broad unconditional dependencies: [crates/jadpo-runtime/Cargo.toml:1](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/Cargo.toml#L1), [crates/jadpo-value/Cargo.toml:1](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-value/Cargo.toml#L1). Inspect retained code; compare a manual known-plan feature control before a production planner. | Low; M control / L planner; high reachability/linking risk | P56 may help some pruning, not universal prerequisite; test alone and combined; distinct P45/P47 |

## Already present and excluded repetitions

Do not count existing work as a new saving: a2 first-escape JSON writing and a3
terminal repeatable-read handoff are in proposed B. BEGIN plus SELECT was
already pipelined before a3; do not substitute autocommit. Repeated byte-bulk
escape writing and Os→O2/O3 keeper choices were rejected; a materially new
hypothesis must say what changed. There is no outgoing JS JSON.parse/stringify
step to remove, and String.into_bytes moves its buffer. Typed HTTP object/list
codecs, borrowed input Doc, direct pure-function arguments and process/isolate
plan caching already exist. No cross-invocation socket cache or global executor
drain is proposed. Native parallelism is separate delivery work; real Workers
measurements still need account/deployment authorization.

## Unknown-impact frontiers

These are part of the available queue, not discarded ideas. The next action is
the focused measurement stated in the row, followed by a calculated range and
re-ranking. Do not fabricate a number to make the table sortable.

| Candidate / current status | Applicable workload / experiment | Predicted throughput gain (hypothetical) | Measured throughput gain | Basis, next check and pinned source | Confidence; cost; risk | Dependency / alternative / overlap IDs |
|---|---|---|---|---|---|---|
| **P61 — Conditional host long-text JSON escaping** (roundtrip rejected; host sink unmeasured) | Transfer-inclusive long-text Wasm micro first; HTTP only if worthwhile | Unknown; transfer/append allocations may lose, especially ASCII | HTTP throughput not measured. Roundtrip micro rejected: host median time +110–385% for large/Unicode cases; host sink unmeasured. [All60 cells and assessment](#roundtrip-result-and-implementation-assessment-2026-10-24). | Primitive js_sys::JSON::stringify only; current [json.rs:1620](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-value/src/json.rs#L1620) and [entries.rs:633](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-codegen/src/entries.rs#L633) need optional Workers writer capability with pure/native default. Exact canonical bytes require Rust fallback for U+0008/U+000C (or proven token-aware repair); catch failures and append only after success. Full Unicode/control/prefix differential checks and generic thresholds. See [frozen micro and independent host-sink alternative](#p61--frozen-roundtrip-micro-and-conditional-host-sink). | Low; S micro / L integration; high canonical-byte/capability risk | Conditional; competes P21/string writers; independent P55 but allocation/copy effects interact; no old a2 gain credit |
| **P59 — Reusable socket Promise completion state** (queued; unmeasured) | All PostgreSQL paths, especially fragmented reads; F | Unknown; current exclusive bridge/allocation/causal GC profile first | Not measured | Current D samples completion/scheduling contexts; constructor/completion and true-I/O census is still required to identify removable work. Fresh JsFuture per read/write at [transport.rs:445](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/transport.rs#L445) and [transport.rs:515](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/transport.rs#L515) creates Rc/state, two once callbacks and Promise.then. Prototype read-only, write-only then combined; retain owned bytes, settlement after cancel/drop, close/TLS/pending-read/error/slot/waitUntil leak guarantees. | Low; M–L; high lifetime/completion risk | Shared-state prerequisite explicit for variants; independent P01/P08/P55; overlaps P60 scheduling/GC, not all read/write/connect cost |
| **P60 — Invocation-local ready-task pumping** (queued; unmeasured) | PostgreSQL task handoffs; F; investigate first | Unknown; removable exclusive task-bridge budget only | Not measured | Current D samples completion/scheduling contexts; constructor/completion and true-I/O census is still required to identify removable work. Connection spawn [postgres/driver.rs:366](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-drivers/src/postgres/driver.rs#L366) and [transport.rs:205](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/transport.rs#L205) cross a microtask executor that defers wakes during polling. Drive only invocation-owned ready tasks; residual work stays in that invocation waitUntil. Preserve bounded fairness/deadlines/abort/drain/streaming/8.4e no-leak rules. | Low; L; high scheduling/lifecycle risk | Alternative/overlap P59; neither prerequisite; combined measurement if retained; distinct P49; no global drain/socket reuse or PG-backend-creation claim |
| **P55 — Safe synchronous Response slice constructor** (measured mixed/promising; prototype retained) | Large ASCII/mixed primarily; small signed control; F | Unknown; measure exclusive intermediate copy/allocation and causal GC | P55/B signed +3.3368%, ASCII +0.1139%, mixed +4.9831%; mixed RSS higher both rounds. [All18 rows and implementation assessment](#p55--safe-response-constructor-first-screen-2026-10-24). | Remove intermediate owned Uint8Array copy through cached safe Response slice API; current [deployable.rs:432](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/deployable.rs#L432). Verify generated JS and construct→mutate/drop/reuse/grow-memory→async-read body ownership, exact binary/Unicode/empty/large bodies, bodyless 101/204/205/304, streaming, statuses/headers/errors. No borrowed view across await. | Low; S; medium lifetime/host-semantics risk | Independent of P10/P25/P26; allocation effects interact P61 |
| **P56 — Direct known standard-operation calls** (queued; unmeasured) | Hot pure standard op; F; current singleton reads may gain zero | Unknown; current body attribution/call census first | Not measured | Known Op still enters broad enum/argument-shape dispatcher: [body.rs:1908](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-codegen/src/body.rs#L1908), [compiled.rs:884](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/src/compiled.rs#L884), [lib.rs:89](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-value/src/lib.rs#L89). Arguments already stack slices, return unboxed; preserve Fault→Refused, inline pace/deadlines/recording/stack. LTO may already fold dispatch. Explicit roadmap 9.2 target. | Low; S–M prototype; high semantic risk | Independent of P34; may enable P57 dead-code pruning; measure separately/combined |
| **P58 — Explicit BLAKE3 wasm32 SIMD backend** (queued; unmeasured) | Short keyed signed tokens; F + actual signed screen | Unknown; old identified crypto budget only about 0.2–0.24%; small lead | Not measured | Cached BLAKE3 1.8.7 wasm32_simd feature is separate from global +simd128; current [Cargo.toml:245](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/Cargo.toml#L245) uses std+pure. Keep keyed format and constant-time Hash equality. Verify boundary-length/current/previous/wrong-key vectors, Wasm SIMD presence and workerd behavior. No substitution of Bun HMAC format. | Low; S–M; medium crypto/target risk | Independent P03; P22 can mask; B/C same flags or separate F/F+C comparison; reuse P02 F only as explicit control |
| **P22 — Bounded authenticated Claims cache** (queued; unmeasured) | Repeated credentials; E + hit/churn/rotation F | Unknown impact; 0–8% uncalibrated sizing only, not a ranking bound; churn can regress | Not measured | Repeated crypto/decode/parse. First measure current exclusive crypto/decode/parse cost and representative hit/churn behavior. Key by exact token and scope to the immutable current/previous signing-key set; every request retains expiry, method, constant-time secret verifier, fresh-route behavior and CSRF. Bound entries AND bytes; no benchmark-tuned 2,048-owner capacity. [http/identity.rs:449](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/identity.rs#L449). | Low; M; high security/lifecycle risk | Independent; masks P03/P07/P23 |
| **P29 — Threshold include-key deduplication** (queued; unmeasured) | Large collections; F | Unknown; profile key count and contains cost | Not measured | keys.contains per parent is quadratic. Potentially substantial at scale; measure threshold and small-list regression; preserve required order. [read.rs:713](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L713). | Medium mechanism, unknown app benefit; S–M; medium | Independent; compatible with P31 |
| **P30 — Pipeline sibling include SELECTs** (queued; unmeasured) | Several includes; F | Unknown; measure sequential handoffs/wait share | Not measured | Use SAME repeatable-read transaction. Preserve snapshot, policy/error ordering, cancellation and drain; no automatic a3 terminal eligibility. [read.rs:712](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L712), [read.rs:737](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L737). | Low; L; high | Independent; overlaps include-work changes P29/P31 |
| **P42 — Single typed-object member dispatch** (queued; unmeasured) | Wide JSON objects; F | Unknown; measure field count and repeated lookup share | Not measured | Linear lookup for each declared field plus unknown scan. Potentially substantial wide-object work; preserve duplicate rejection, declared validation order and sorted unknown names. Small objects may lose. [json/doc.rs:771](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json/doc.rs#L771), [entries.rs:464](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-codegen/src/entries.rs#L464), [json/doc.rs:795](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json/doc.rs#L795). | Low-medium; M; high ordering risk | Independent; interacts P48 |
| **P31 — Share built include collections** (queued; unmeasured) | High fan-out/shared references; F | Unknown; profile per-parent `Vec<Value>` cloning | Not measured | Immutable Values permit collection sharing; preserve list order, cardinality and policy. Measure fan-out and retained memory. [read.rs:786](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L786). | Low-medium; M; medium | Independent; combines P29; interacts P30 |
| **P40 — Borrowed Doc for remaining JSON paths** (queued; unmeasured) | Stored JSON/provider fields; F | Unknown; profile DOM/text Value allocation | Not measured | Migrate still-allocating paths. HTTP typed input ALREADY uses Doc; retain exact codec/error semantics. [read.rs:889](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L889), [read.rs:946](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L946), [http/mod.rs:854](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/mod.rs#L854). | Low; L; high | Independent; supersedes P39/P41 where migrated |
| **P32 — Generated known-field slots** (queued; unmeasured) | Field-heavy loops; F | Unknown; current reads likely small; profile lookup share | Not measured | Generated name lookup can use proven shape slots. Keep shape guarantees, fallback, faults and sites. [body.rs:605](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-codegen/src/body.rs#L605), [compiled.rs:660](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/compiled.rs#L660). | Low; M; high lowering risk | Independent; P33 not prerequisite |
| **P33 — Last-use Value moves** (queued; unmeasured) | CPU-heavy bodies; F | Unknown; profile Rc clone/drop and last-use frequency | Not measured | Avoid unconditional clones only with control-flow proof. Preserve captures, branches and evaluation order. [body.rs:596](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-codegen/src/body.rs#L596). | Low; L; high control-flow risk | Independent; enables ownership follow-ons; overlaps P18/P35 |
| **P34 — Direct known action/body arguments** (queued; unmeasured) | Deep action workloads; F | Unknown; profile Vec/boxed-entry call cost | Not measured | ACTION/BODY calls allocate `Vec<Value>`. Pure FUNCTION calls already direct; preserve transaction/frame behavior. [body.rs:1388](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-codegen/src/body.rs#L1388), [body.rs:283](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-codegen/src/body.rs#L283), [body.rs:2049](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-codegen/src/body.rs#L2049). | Low; L; high | Independent |
| **P36 — One-pass min_by/max_by** (queued; unmeasured) | Large min/max lists; F | Unknown; measure keyed-Vec allocation and scan share | Not measured | Avoid materialized keyed Vec. Evaluate each key once/in order; preserve first tie and failures. [list.rs:159](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/list.rs#L159), [list.rs:147](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/list.rs#L147). | Medium mechanism, unknown app benefit; S; medium | Independent |
| **P38 — Bulk scalar-bounded text slices** (queued; unmeasured) | Long prefix/suffix slices; F | Unknown; profile scalar traversal and allocations | Not measured | Collects chars; suffix counts whole string then walks again. Bulk-copy boundaries/reverse scan must preserve Unicode SCALAR semantics. [text.rs:112](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/text.rs#L112). | Low-medium; S–M; medium | Independent |
| **P39 — Old DOM parser no-escape string path** (queued; unmeasured) | Generic/stored JSON ordinary strings; F | Unknown; measure old-parser string share | Not measured | Scans then serde-decodes every string. Typed Doc already has fast path. Preserve malformed/control/UTF-8 handling. [json/document.rs:169](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json/document.rs#L169). | Low; S; medium | Independent; alternative to P40 on migrated paths |
| **P48 — Decode escaped Doc strings once** (queued; unmeasured) | Escape-heavy input/member names; F | Unknown; measure repeated accessor decode incidence | Not measured | serde validates escaped span then accessor decodes again. Cache can add memory and lose for unused fields. [json/doc.rs:403](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json/doc.rs#L403), [json/doc.rs:114](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json/doc.rs#L114). | Low; M; medium | Independent; interacts P42 |
| **P24 — General base64/cursor table decoder** (queued; unmeasured) | Bytes input/pagination; F | Unknown; profile decode share and payload sizes | Not measured | General decoder avoids intermediate/re-encode but scans alphabet. Cursor acceptance differs from signed canonical tail rules; preserve both. [encoding.rs:124](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/encoding.rs#L124), [read.rs:1443](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L1443). | Low; S–M; medium | Independent, or prerequisite P03 if reusing helper; overlaps P03/P07/P23 |
| **P50 — One cancellable pool waiter** (queued; unmeasured) | Contended invocation-local/native pools; F | Unknown; singleton likely zero | Not measured | Each poll appends waker, return wakes/drains all. Measure waiter amplification; require cancellation deregistration, safe skipping and starvation controls, no naive wake-one. [postgres/driver.rs:77](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L77), [postgres/driver.rs:52](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L52). | Low; M; high starvation risk | Independent |
| **P49 — Wake-driven join** (queued; unmeasured) | Many uneven background/schedule futures; F | Unknown; normal reads zero | Not measured | Every wake polls every unfinished future. Measure fan-out/poll amplification; preserve output order and complete drain/cancellation. [race.rs:36](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/race.rs#L36). | Low; M; high | Independent |
| **P52 — Direct exact-decimal PG codec** (queued; unmeasured) | Decimal/quantity-heavy; F | Unknown; profile numeric text conversion share | Not measured | Avoid decimal text intermediates. Preserve 38-digit bounds, scales, signs, zero and special-value rejection. [postgres/wire.rs:768](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L768), [read.rs:1080](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L1080). | Low; L; high | Independent; SqlValue representation interacts P04 |
| **P54 — Reuse repeated SQL within stable session** (queued; unmeasured) | Long write loops; F | Unknown; singleton likely worse | Not measured | Only explicit transaction/session with stable backend. Measure prepare/repetition break-even. No global cache across transaction-pooled backends; preserve retry/schema/type behavior. [postgres/driver.rs:546](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/driver.rs#L546). | Low; M–L; high | Independent |
| **P41 — Old parser numeric/path copies** (queued; unmeasured) | Deep numeric JSON; F | Unknown; profile lexeme/path allocations | Not measured | Copies exact numeric text/path and formats array indices. Preserve numeric source lexeme and diagnostics. [json/document.rs:227](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json/document.rs#L227), [json/document.rs:152](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json/document.rs#L152). | Low; M; high exactness risk | Independent; superseded where P40 removes path |
| **P37 — Reserve flatten output** (queued; unmeasured) | Large nested lists; F | Unknown; measure reallocations versus extra length pass | Not measured | Repeated extend from empty Vec. Check overflow/failure behavior; small lists may lose. [list.rs:288](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/list.rs#L288). | Low; S; low-medium | Independent |
| **P20 — Direct JSON control escape hex** (queued; unmeasured) | Control-heavy strings; F | Unknown; current mixed fixture little/none expected | Not measured | Generic formatting for \u00XX. Measure control frequency and writer share; LF/TAB already dedicated cases. Exact canonical bytes required. [json.rs:1639](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json.rs#L1639). | Low; S; low | Independent |

## Source details for discoveries P55–P61

These seven source-backed additions are outside F01's frozen initial54. Their
current impacts are unknown; profiling must establish a removable budget before
claiming a large gain. The inspected production source is 4fda above.

- P55 uses cached web-sys 0.3.106 `gen_Response.rs:187–190`,
  `new_with_opt_u8_array_and_init(Option<&mut [u8]>, &ResponseInit)`.
  wasm-bindgen-cli-support 0.2.129 `wit/outgoing.rs:670–705` emits OptionView;
  `js/binding.rs:1984–1994` emits a direct Wasm array view. Baseline generated
  `b/s/focused/workers/build/target/kiosk/kiosk.js` instead has an intermediate
  `new Uint8Array(getArrayU8FromWasm0(...))`, SHA-256
  `809b0b79a023e5c2c764256826551540f02e33b1c6064417cceda58fc71ad386`.
  Rust type safety alone does not establish host copy ownership: the row's
  mutation/drop/memory-growth-before-consumption test is required.
- P56 does not remove a heap argument Vec or boxed return: neither exists on
  this standard-operation path. `effects` borrows its tables. A direct call can
  help reachability but LTO behavior must be measured. P57 measures actual
  retained code and cold/build/RSS costs, separately from warmed request speed.
- P58's cached BLAKE3 `build.rs:24/296/373`, `platform.rs:94/151/187` and
  `wasm32_simd.rs:359/376` enable SIMD short compression with the explicit
  feature, including keyed token lengths. The old three named profiles have
  BLAKE3 leaf-self sums 0.1867/0.2199/0.2274%, and nonoverlapping any-BLAKE3-
  ancestor sample unions 0.2015/0.2397/0.2409%; portable compression leaves
  0.1119/0.1398/0.1497%. Do not add self and inclusive figures or treat them as
  all current/inlined crypto cost. Raw captures remain under archive
  `LLM-coding-chunk-9.2a1/tools/bench/work/request-profile/request-current-rustup-16d24dc4/named-c16-r3/{001,006,011}-golden-read.cpuprofile`.
  Test consumed keyed digests/sign/open at 0,1,63,64,65,255,1023,1024,1025 and
  actual token lengths, existing vectors and all key cases on Wasm/workerd.
- P59's cached js-sys 0.3.106 `futures/mod.rs:196` constructs Promise callback
  state; P60's `futures/queue.rs:33/79` defers wakes during task polling.
  Keep callbacks alive through settlement even after cancellation. Reusing an
  allocation does not justify an unsafe borrowed byte array across await.
  a3 handoff savings are already in B and not available as fresh credit.
- P61's safe cached `js_sys::JSON::stringify` catches exceptions, but native
  JSON uses `\b`/`\f` where Jadpo's canonical writer deliberately uses
  `\u0008`/`\u000c`. Reject those scalars to unchanged fallback or prove
  token-aware repair; blind replacement corrupts literal backslash sequences.
  Preserve non-ASCII literals, non-NFC, astral scalars, U+2028/U+2029, other
  controls, quotes, backslashes, empty/huge/prefixed outputs and failure atomicity.
  An explicit optional Workers capability supplies the host writer; value stays
  pure and native defaults unchanged. The transfer-inclusive roundtrip below is
  now rejected; the independent host-sink alternative remains unmeasured and
  makes no new language serialization choice.

## P02 — SIMD UTF-8 first screen (2026-10-24)

**Measured/mixed, exploratory prototype retained; no production promotion or
favorable rerun.** The parent independently checked the complete fixed 24-cell
raw capture. P uses safe SIMD UTF-8 validation; F retains standard validation
with the same global simd128 flag and an unused cached dependency, so F/B is
not a flag-only causal estimate. No BLAKE3 backend change was made. The frozen
P/F prediction was mixed +2–6%, ASCII +0–1%; F/B and combined P/B forecasts
were unknown. Signed reads are a regression control without a frozen numeric
prediction. Original ranges remain in the queue; no hindsight forecast edit.

**Implementation assessment — moderate.** In retained P db5abfba versus F
97ad528b, PostgreSQL text/JSON/JSONB/text-array decoding replaces standard
validation with safe simdutf8 calls and owned conversion; no runtime interface
or hand-written unsafe validator is added. A proper keeper still introduces a
pinned dependency and needs an explicit target-feature/support policy plus
native/Wasm invalid-input/error controls. The F-only comparison arms and large codec-control example are experiment
scaffolding, not a new production API. Any enabling SIMD target feature/flag
remains an explicit lasting production-compatibility obligation; it cannot be
dropped while claiming the observed SIMD gain.
The initial offline-cache failure is setup roughness, not intrinsic algorithm
complexity. This fits the original S–M cost's moderate end. **Tradeoff / advice:**
retain for later consideration, but do not select the current form on Unicode
throughput alone: ASCII loss and adverse p95 require resolution or an explicitly
justified scope. Any proper keeper needs those platform/correctness obligations.

The two rounds reverse the entire declared order: three workloads × B/F/P/Bun,
c16, 2 s admission after 1 s warmup, 2,048 principals, shipping Os, inspector
NONE and PostgreSQL durations OFF. All errors, warmup errors, invalid bodies
and persisted deltas are zero; three complete expected body identities match.
All 144 immutable served copies and 72 current program originals were reviewed.
UTC versus load-clock gaps are 4.845–272.194 ms; no Sleep/DarkWake was observed.
All owned services, including temporary PostgreSQL, were reaped. This short
local screen is provisional, not long confirmation, p95 assurance, edge data
or full contract qualification. There was no full gate or production PR for
this prototype.

Each actual below is **100 × (median candidate req/s / median reference req/s − 1)**,
using this contemporaneous campaign's two rounds; per-round ratios are also
retained. It is not a historical-day raw-rate comparison or the median of
ratios, and does not infer SQL or socket counts.

| Comparison / workload | Frozen predicted throughput gain | Actual gain (ratio of medians) | Round 1 / round 2 actual gains |
|---|---|---|---|
| P/F marginal / mixed | 2–6% | +6.071% | +6.810% / +5.334% |
| P/F marginal / ASCII | 0–1% | -0.586% | -0.733% / -0.439% |
| P/F marginal / signed | Unknown (regression control) | +0.103% | -0.426% / +0.636% |
| P/B combined / mixed | Unknown | +6.550% | +7.014% / +6.084% |
| P/B combined / ASCII | Unknown | -0.968% | -1.723% / -0.205% |
| P/B combined / signed | Unknown | -1.554% | -2.777% / -0.303% |
| F/B control / mixed | Unknown | +0.452% | +0.192% / +0.712% |
| F/B control / ASCII | Unknown | -0.384% | -0.997% / +0.235% |
| F/B control / signed | Unknown | -1.655% | -2.362% / -0.934% |

Mixed P/F exceeds the forecast's upper edge slightly; ASCII is below its
predicted range, with both rounds negative. Mixed P/F p95 changes are
−2.830% / **+18.693%**; ASCII **+9.802% / +7.496%**; signed +0.883% / −0.459%.
Mixed service CPU/request changes −7.212% / −6.867%, while its separate helper
CPU rises; ASCII service CPU is approximately flat and signed is mixed.
Signed sampled peak RSS B 294.0/291.3→P 315.9/296.1 MB is adverse. All rows,
including regressions and the higher mixed round-2 p95, remain below and in raw.

Every cell is **req/s / p95 ms / service CPU ms per request / separate pool-helper
CPU ms per request / sampled peak RSS MB**. Service CPU excludes PostgreSQL and
the helper. RSS uses each harness's sampled-peak field, not a lifetime maximum.

| Workload / round | B | F | P | Bun control |
|---|---|---|---|---|
| mixed / 1 | 801.216 / 26.241 / 1.4613 / 0.2353 / 306.3 | 802.753 / 26.538 / 1.2855 / 0.2040 / 306.3 | 857.417 / 25.787 / 1.1928 / 0.2085 / 304.3 | 1646.303 / 11.955 / 0.7288 / 0.0000 / 172.6 |
| mixed / 2 | 798.485 / 28.157 / 1.2687 / 0.2114 / 295.7 | 804.173 / 25.839 / 1.2840 / 0.2099 / 300.7 | 847.067 / 30.669 / 1.1958 / 0.2110 / 306.3 | 1637.742 / 12.260 / 0.7271 / 0.0000 / 184.3 |
| ASCII / 1 | 1300.536 / 18.825 / 0.7902 / 0.1688 / 358.2 | 1287.567 / 18.742 / 0.7969 / 0.1663 / 359.8 | 1278.125 / 20.579 / 0.7947 / 0.1675 / 362.7 | 5951.585 / 4.141 / 0.2182 / 0.0000 / 188.7 |
| ASCII / 2 | 1288.254 / 19.478 / 0.7924 / 0.1662 / 361.6 | 1291.276 / 19.010 / 0.7906 / 0.1658 / 357.7 | 1285.612 / 20.435 / 0.7913 / 0.1668 / 366.3 | 5727.145 / 4.620 / 0.2241 / 0.0000 / 183.9 |
| signed / 1 | 2353.625 / 8.665 / 0.4300 / 0.1207 / 294.0 | 2298.040 / 8.949 / 0.4410 / 0.1217 / 314.8 | 2288.253 / 9.028 / 0.4447 / 0.1243 / 315.9 | 6256.505 / 3.588 / 0.1933 / 0.0000 / 205.0 |
| signed / 2 | 2303.960 / 8.818 / 0.4374 / 0.1213 / 291.3 | 2282.443 / 8.925 / 0.4438 / 0.1224 / 312.7 | 2296.969 / 8.884 / 0.4367 / 0.1238 / 296.1 | 6162.685 / 3.695 / 0.1987 / 0.0000 / 205.9 |

Cold-ready / first-answer milliseconds, all rounds (sparse local observations,
not a deployment cold-start guarantee). P/F mixed cold and first-answer times
rise in both rounds; ASCII cold rises in both; signed cold rises in round 2.
The first B mixed observation is retained without a favorable replacement.

| Workload / round | B cold / first | F cold / first | P cold / first | Bun cold / first |
|---|---|---|---|---|
| mixed / 1 | 181.782 / 119.917 | 59.956 / 34.994 | 62.427 / 36.624 | 174.813 / 8.541 |
| mixed / 2 | 58.441 / 35.156 | 59.424 / 36.075 | 60.266 / 36.273 | 83.215 / 4.600 |
| ASCII / 1 | 62.150 / 36.430 | 60.612 / 37.657 | 61.986 / 36.200 | 108.047 / 5.164 |
| ASCII / 2 | 57.468 / 34.469 | 57.313 / 34.464 | 57.598 / 34.431 | 96.396 / 4.429 |
| signed / 1 | 67.716 / 33.131 | 78.767 / 30.339 | 62.952 / 28.276 | 186.679 / 16.507 |
| signed / 2 | 59.771 / 24.860 | 58.198 / 25.441 | 59.790 / 25.021 | 92.345 / 11.868 |

Whole-host CPU is 31.727–57.294%; load-average endpoints are 11.910–18.819.
Per-row host CPU/load, load-generator CPU, pool-helper CPU, body bytes/hashes,
startup and process context are retained in raw. Such host/control drift and
only two short reversed rounds limit attribution; no favorable cell was rerun.

| Artifact variant | Golden Wasm / gzip6 bytes | Focused Wasm / gzip6 bytes |
|---|---|---|
| B | 7325897 / 2690117 | 7364454 / 2662029 |
| F | 7227132 / 2660139 | 7262733 / 2635201 |
| P | 7229455 / 2660983 | 7265056 / 2636349 |

P grows relative to F in both apps; the smaller F/P artifacts versus B are not
validator-only size attribution. gzip6 is a deterministic Wasm compression
proxy, not provider upload size; module gzip sums/other sizes stay in manifests.

All paths below are relative to the durable archive
`/Users/richardwillars/Documents/Codex/jadpo-benchmark-archive/performance-exploration/a4-baseline-84e1c00b/`.
Immutable refs share prefix `refs/experiments/a4-baseline-84e1c00b/`.

| Identity | Retained source/ref or raw record | SHA-256 / tree |
|---|---|---|
| B production | `production-base` → `4fda5e597a3f508e1666ef46de0e0a3b850179b3` | Production base; inherited a3 behaviour |
| H84 | `harness-baseline` → `84e1c00bc97fb565bcb5810a01b6028b0da803a9` | Tree `8fc8bdfc23e05dea15d498e36d3df120877a1286` |
| Shared H84 | `shared.json` | `860842f0d5ab4bc03001e57b89df873cec97dff161fbd761f0748cb9941a577c` |
| B build | `b/build.json` | `27601fe276921157443e72cd0d735d230f169f7b7354463682dcd4f86fe663af` |
| F source | `f-simd` → `97ad528b7bf85f3f6ba5ce4bac4dc37f7e989886` | Tree `f4335592584b5524fda077926a9efa37196ecc0a` |
| F build | `variants/f-simd/build.json` | `2695df1289aa25172f42349cbdf5c9f75cb29b2b548135266e7d4be05fc103d0` |
| P source | `p02-simd` → `db5abfba30e646de1d045f14ac23389172cb7e73` | Tree `32e146cc27889d2172850d4569c04f8b598742c5` |
| P build | `variants/p02-simd/build.json` | `2a1185ad2c0ea4a2994c7acfb4a96ada88db01bc35fbd15c056507843a99017e` |
| Fixed 24-cell result | `screens/p02-simd/screen24-none-pg-off-c16-r2/results.json` | `004a0a9e699ae9e18ab0df6a142dde509bb744962ba3fdc850281d2eec0157d4` |
| F controls | `variants/f-simd/utf8-control/results.json` | `d9442f9675cc2234ec2c6d095c2813807d35b93c8b5d414cc797ce31410ff76e` |
| P controls | `variants/p02-simd/utf8-control/results.json` | `5cca603f38daaaf6c214db07f340ecc4ad108f703acef97d40d030ca549a7527` |
| B no-load smoke | `screens/b/setup-b/results.json` | `20c1a51bb8f18b0f21564a92f9e7d5c171070cd03e1ab8e0cf7462a6ae4e606d` |
| Cohort no-load smoke | `screens/p02-simd/setup-cohort/results.json` | `8e5d69537f9917b7421e1706e49adf6d395ed2dac3e2ae80bf4bf50afe2fb804` |

Focused validation includes 341,514 codec cases on the native baseline and
SIMD Wasm F/P controls, 35 driver library tests and affected Clippy. They retain
valid/invalid UTF-8, source errors and JSONB marker behavior; no speed claim
comes from invalid-input allocation changes. Control Wasm hashes F/P are
`aaaa36e8f40e0ba00fa016b3894cecfcea6fc0e97fd2690f1e4cea8da11af37b` /
`bfee1086ec828fb816201e1421ef70c5b53bbbe9cb3d656759dd6877b0d5afcb`.
The recorded initial offline resolver setup failure occurred before any load
or build-manifest capture. Its source
`0336a0261e787eabc69d620f5793f16b99682d60` is retained by the Git ref
`refs/experiments/a4-baseline-84e1c00b/f-offline-resolution-attempt`, not an
archive folder. Exact resolver stderr remains in task tool/session 31169 output,
not a durable local archive file; that setup attempt has no build manifest,
benchmark raw or timing. Two exact cached index/archive records were
checksum-verified and copied into the active Cargo namespace, without
downloading. Generated app code/plans remain equal
across B/F/P except lock entry; no source/artifact guard was bypassed. Prototype
source, prerequisites and all immutable artifacts were retained before reset.

## P03 — signed base64url first screen (2026-10-24)

**Measured small positive, prototype retained; causal attribution is limited.**
The parent independently accepted the complete 18-cell capture. The original
signed application-throughput prediction **+0.5–2% versus B** remains frozen.
This is independent from H84/B4fda, without P02 or new features/dependencies;
marginal and combined P03/B are the same comparison. Public ASCII/Unicode
controls had approximately zero direct decoder scope, with unpredicted
regression/codegen effects checked. Their positive movement is not a decoder
gain and prevents assigning the signed change wholly to the decoder. No
production promotion, full gate, prototype PR or favorable rerun followed.

**Implementation assessment — simple, with security-sensitive correctness.**
Retained e25e40b0 changes one private decoder: a fixed sextet table and checked
byte accumulator replace alphabet search, an intermediate vector and canonical
re-encoding. It adds no dependency, feature, platform branch or runtime interface.
The explicit length/tail-bit checks and literal table need clear invariants and
the retained exhaustive two/three-symbol tail and differential
canonical-acceptance tests. Earlier Clippy/const-API
attempts are resolved prototype roughness; a proper keeper would keep the same
small pure decoder shape and useful differential tests, not those failed paths.
This matches the original S estimate. **Tradeoff / advice:** a favorable simple
candidate for later human carry-forward selection, subject to confirmation of
the small signed benefit and the public-control attribution limit; no promotion.

The fixed two rounds reverse the declared B/P03/Bun × three-workload order,
c16, 2 s admission plus 1 s warmup, 2,048 principals, inspector NONE and
PostgreSQL durations OFF. All18 errors/warmup errors/invalid bodies/state
deltas are zero, three full body identities match, all96 immutable served
snapshots and source/artifact guards pass. Clock gaps are 5.230–10.543 ms,
Sleep has no events and all owned services were reaped. This remains a
provisional short local screen, not long confirmation or a capacity/edge/p95
assurance. Do not subtract public-control movement as an invented causal
correction.

Actual gains use **100 × (median P03 req/s / median B req/s − 1)** in this
contemporaneous campaign, not historical-day raw rates or a median of round
ratios. Both round ratios and the same-run Bun controls remain visible.

| Workload | Frozen throughput prediction | Actual P03/B gain (marginal = combined) | Round1 / round2 gain | Median B / P03 / Bun req/s |
|---|---|---|---|---|
| signed | 0.5–2% | +2.0625% | +2.8993% / +1.2167% | 2336.9356 / 2385.1346 / 6307.2538 |
| ASCII control | Approximately 0 direct; other movement unknown | +1.4274% | +3.4755% / -0.5110% | 1276.6191 / 1294.8418 / 5964.2702 |
| mixed control | Approximately 0 direct; other movement unknown | +1.3656% | +1.2062% / +1.5259% | 788.4493 / 799.2166 / 1633.6090 |

The signed ratio is slightly above the prediction's upper edge; the public
controls also improve, so this is a bounded positive signal rather than a
universal or isolated causal claim. Signed p95 changes −2.1729% / −1.4273%,
service CPU/request −3.2835% / −0.6164%. All measured p95s decline, while
signed round2 sampled peak RSS **298.0→314.3 MB** and ASCII round2 service
CPU/request **0.786175→0.789474 ms** are adverse and retained. No memory-neutral
or leak claim follows from these sparse samples.

Every cell is **req/s / p95 ms / service CPU ms per request / separate pool-helper
CPU ms per request / sampled peak RSS MB**. Service CPU excludes the helper and
PostgreSQL; RSS is sampled, not a lifetime maximum.

| Workload / round | B | P03 | Bun control |
|---|---|---|---|
| signed / 1 | 2349.389 / 8.560 / 0.4332 / 0.1189 / 299.8 | 2417.505 / 8.374 / 0.4190 / 0.1218 / 299.5 | 6314.016 / 3.569 / 0.1923 / 0.0000 / 205.4 |
| signed / 2 | 2324.482 / 8.758 / 0.4356 / 0.1202 / 298.0 | 2352.765 / 8.633 / 0.4329 / 0.1210 / 314.3 | 6300.491 / 3.633 / 0.1935 / 0.0000 / 205.5 |
| ASCII / 1 | 1241.499 / 20.256 / 0.8133 / 0.1763 / 362.3 | 1284.648 / 19.376 / 0.7913 / 0.1629 / 359.6 | 5986.041 / 4.170 / 0.2162 / 0.0000 / 194.0 |
| ASCII / 2 | 1311.739 / 18.911 / 0.7862 / 0.1709 / 362.1 | 1305.036 / 18.322 / 0.7895 / 0.1678 / 354.0 | 5942.500 / 4.324 / 0.2202 / 0.0000 / 189.6 |
| mixed / 1 | 790.619 / 32.162 / 1.2806 / 0.2072 / 301.4 | 800.156 / 26.065 / 1.2709 / 0.2108 / 301.5 | 1644.593 / 12.148 / 0.7271 / 0.0000 / 173.9 |
| mixed / 2 | 786.280 / 29.911 / 1.2879 / 0.2146 / 310.0 | 798.277 / 26.559 / 1.2749 / 0.2114 / 302.3 | 1622.625 / 12.331 / 0.7334 / 0.0000 / 180.5 |

All cold-ready / first-answer milliseconds follow; they are sparse local
observations, not deployment cold-start predictions. Per-row host/load, body
bytes/hashes, load-generator and helper CPU, other latency quantiles and
process context remain in the full raw18 records, including adverse entries.

| Workload / round | B cold / first | P03 cold / first | Bun cold / first |
|---|---|---|---|
| signed / 1 | 154.812 / 90.004 | 62.736 / 30.280 | 927.187 / 18.959 |
| signed / 2 | 62.294 / 29.059 | 59.004 / 29.335 | 88.607 / 11.725 |
| ASCII / 1 | 62.449 / 36.156 | 57.789 / 34.569 | 90.285 / 3.396 |
| ASCII / 2 | 57.235 / 34.894 | 57.644 / 34.614 | 85.161 / 5.765 |
| mixed / 1 | 58.560 / 35.585 | 59.445 / 36.596 | 84.780 / 3.803 |
| mixed / 2 | 57.676 / 34.631 | 58.807 / 35.373 | 86.149 / 4.880 |

Whole-host CPU spans 32.248–53.175%, load-average endpoints 5.870–15.395.
Host drift and two short reversed rounds limit attribution. Generated
application inputs match B exactly and only `http/signed.rs` changes from H84;
there are no feature/dependency edits. The two production Wasm modules each
shrink 222 bytes: golden **7325675 / 2689999** raw/gzip6, focused
**7364232 / 2661741**. gzip6 remains a compression proxy, not a provider upload
measurement. Exact candidate module hashes are golden
`cd20b412e6682a69ef668ff54ae81d1b6364560b9aadcb18d5b22be8e0d1c1f2`,
focused `badfa8710533a4f0fc9089899fd65fcc6d878b3f0676d7c93a0c3a6bff25bff7`;
all distinct original/saved artifact maps remain guarded in the build record.

P03 source `e25e40b0f30d8a1394df0488cb2a8c203dd1256f`, tree
`e7dbb94898153a747dc6b268d00f79cc4628cf94`, is retained at
`refs/experiments/a4-baseline-84e1c00b/p03-table`. It reuses the exact frozen
shared H84/B records above. Paths below are relative to archive A used by P02:

| P03 record | Archive-relative path | SHA-256 |
|---|---|---|
| Build | `variants/p03-table/build.json` | `97a8dc1f8dfc766ed47a30ce561fea963c5c393b20891f1291e02217ce22b7f8` |
| No-load cohort smoke | `screens/p03-table/setup-cohort/results.json` | `428ff9703f0399a5f343c93947f3ff54bdda96f6780f23e0a69d2b737314a622` |
| Complete fixed18 result | `screens/p03-table/screen18-none-pg-off-c16-r2/results.json` | `a60512531facedaa3e31161ff51788ade752dbe5b2b11600b567651533f91cf2` |

Nine signed controls pass on the test-only baseline source
`a320802f146ccbae9837b92967533b99391fca72` and on the final candidate; affected
Clippy library/tests pass with warnings denied. Earlier candidate
`5eec038aa881603f27b1a1cfa49e0480116b1d60` passed the same nine controls but
Clippy denied three indexing sites. Follow-up
`4f946f5092e7684218923560ba61db45dc4d082b` failed compilation because const
slice get/get_mut remained unstable on pinned Rust 1.99. Final e25e uses a
literal 256-entry table and checked runtime lookup. All setup source commits
remain retained ancestors of the named P03 ref; exact setup stderr is in task
sessions 63042/45152, not invented benchmark raw. No earlier screen or
performance failure occurred for those preparation attempts. Source/artifacts
were retained and guarded before resetting for the next independent idea.

## P21 — first-escape and dependent suffix screen (2026-10-24)

**First-only: promising measured ASCII prototype retained. Current repeated-
helper suffix: negative/rejected. No production promotion or favorable rerun.**
First is independent H84/B4fda, without P02/P03, features or dependency edits;
Suffix explicitly depends on the exact First manifest. Both change only
`jadpo-value/src/json.rs`, with unchanged application input maps and ordinary
Os flags (no added target features).

Original First/B prediction **0–3% ASCII/late-escape throughput** remains
frozen; early-escape mixed and signed direct scope was approximately zero,
with regressions possible. Late-escape was not captured in this screen.
Suffix's additional signed/ASCII prediction was approximately zero; mixed
marginal gain was unknown including zero/regression, and combined Suffix/B
was unknown. Those forecasts are not rewritten after results. Preserve a2's
earlier approximately −5.6% mixed repeated-scalar-span result as separate
negative history; this measured suffix uses the repeated word helper.

**First-only implementation assessment — moderate.** Retained 914367c6 adds
one safe u64 first-escape helper, checked complete-word reads, explicit
little-endian lane order and a scalar tail; the existing writer/suffix API is
unchanged, without dependencies, target features or unsafe code. Its subtract-
borrow mask is non-obvious: only the first marked lane is exact, so proof
comments and lane/tail/borrow/Unicode differential tests are lasting maintenance
requirements. The earlier as_chunks Clippy repair is resolved prototype
roughness; a proper keeper can retain this bounded helper rather than a new
writer framework. This agrees with P21's original M estimate. **Tradeoff /
advice:** carry First into later human selection as a promising ASCII candidate;
retain mixed loss and unexplained signed movement and confirm interactions
before any production decision.

**Repeated-helper suffix implementation assessment — moderate, with poorer
tradeoff.** The retained a7a2baf6 delta over First adds a remaining-slice loop,
repeated helper calls, UTF-8-boundary splitting and one-scalar escape/continuation
bookkeeping. It reuses First's safe helper and adds no dependency or interface,
but a proper keeper would still maintain that extra control flow and boundary
coverage; its cost is not merely untidy benchmark scaffolding. There was no
separate pre-run complexity label for this variant; this is a source-based
backfill, not a rewritten performance prediction. **Tradeoff / advice:** do not
carry this current suffix form forward: extra writer complexity accompanies
negative marginal medians and adverse mixed CPU/tail observations. Preserve its
source/result and assess any distinct bitmap alternative separately.

The parent independently accepted all24 fixed B/First/Suffix/Bun × three-
workload cells, two reversed rounds, c16, 2 s admission plus 1 s warmup,
2,048 principals, inspector NONE and PostgreSQL durations OFF. All errors,
warmup errors, invalid bodies and persisted deltas are zero; three complete
body identities match. All144 immutable served snapshots and source/artifact
checks pass; clock gaps are 4.982–10.769 ms, Sleep has no events and all owned
services were reaped. This is a provisional short local screen, not long
confirmation, full qualification, a gate, edge capacity or a tail-latency promise.

Each actual is **100 × (median candidate req/s / median reference req/s − 1)**
within this campaign. Round ratios are retained separately; public controls
and host drift prevent blanket causal attribution. First's signed +1.92% is
unexplained by its tiny identified writer footprint. Diagnostic sample budgets
are not guaranteed request-throughput ceilings, so do not force the observed
numbers into that earlier profile model.

| Comparison / workload | Frozen predicted throughput gain | Actual gain (ratio of medians) | Round1 / round2 gains |
|---|---|---|---|
| First/B independent / signed | Approximately 0 direct; regressions possible | +1.9197% | +1.4773% / +2.3669% |
| First/B independent / ASCII | 0–3% | +11.2918% | +12.8654% / +9.8162% |
| First/B independent / mixed | Approximately 0 direct; regressions possible | -0.5716% | -0.8443% / -0.2968% |
| Suffix/First marginal / signed | Approximately 0 additional; regressions possible | -0.9024% | -2.1403% / +0.3380% |
| Suffix/First marginal / ASCII | Approximately 0 additional; regressions possible | -0.9633% | -0.0064% / -1.8855% |
| Suffix/First marginal / mixed | Unknown, zero/regression possible | -0.7076% | -1.1590% / -0.2551% |
| Suffix/B combined / signed | Unknown | +1.0000% | -0.6946% / +2.7129% |
| Suffix/B combined / ASCII | Unknown | +10.2197% | +12.8582% / +7.7456% |
| Suffix/B combined / mixed | Unknown | -1.2751% | -1.9935% / -0.5511% |

First's ASCII gain exceeds its forecast, while mixed regresses. Suffix loses
against First in all three workload medians; its positive combined ASCII
number does not turn the suffix addition into a win. Retain signed/mixed
movements without assigning them all to the scan. Same-batch medians are:

| Workload | B req/s | First req/s | Suffix req/s | Bun req/s |
|---|---|---|---|---|
| signed | 2281.7467 | 2325.5503 | 2304.5647 | 6238.1592 |
| ASCII | 1256.0369 | 1397.8656 | 1384.3998 | 5992.0286 |
| mixed | 801.5571 | 796.9755 | 791.3364 | 1595.2313 |

Adverse observations include mixed First round2 p95 **+1.049%** versus B,
mixed Suffix round1 p95 **+19.305%** versus B, First ASCII round2 sampled
peak RSS **363.0→385.2 MB**, and higher mixed CPU/request in both variants.
Every row is retained below; these are neither universal nor memory-neutral
improvements. There is no long confirmation or keeper integration at this point.

Each cell is **req/s / p95 ms / service CPU ms per request / separate helper
CPU ms per request / sampled peak RSS MB**. Service CPU excludes PostgreSQL
and the pool helper; sampled RSS is not a lifetime maximum.

| Workload / round | B | First | Suffix | Bun control |
|---|---|---|---|---|
| signed / 1 | 2293.940 / 8.906 / 0.4458 / 0.1240 / 318.6 | 2327.829 / 8.498 / 0.4333 / 0.1223 / 296.8 | 2278.006 / 8.880 / 0.4470 / 0.1227 / 314.3 | 6197.993 / 3.697 / 0.1967 / 0.0000 / 204.4 |
| signed / 2 | 2269.554 / 9.010 / 0.4465 / 0.1210 / 310.4 | 2323.272 / 8.910 / 0.4362 / 0.1203 / 315.4 | 2331.123 / 8.746 / 0.4345 / 0.1220 / 296.6 | 6278.326 / 3.663 / 0.1958 / 0.0000 / 203.7 |
| ASCII / 1 | 1215.653 / 22.438 / 0.8357 / 0.1844 / 353.4 | 1372.052 / 19.016 / 0.7407 / 0.1670 / 355.6 | 1371.964 / 18.388 / 0.7469 / 0.1668 / 362.6 | 5989.727 / 4.166 / 0.2168 / 0.0000 / 189.6 |
| ASCII / 2 | 1296.421 / 18.830 / 0.7911 / 0.1651 / 363.0 | 1423.679 / 17.221 / 0.7245 / 0.1610 / 385.2 | 1396.836 / 18.134 / 0.7382 / 0.1712 / 365.0 | 5994.330 / 4.105 / 0.2150 / 0.0000 / 195.5 |
| mixed / 1 | 804.647 / 25.817 / 1.2708 / 0.2097 / 296.8 | 797.854 / 25.815 / 1.2803 / 0.2113 / 302.4 | 788.606 / 30.801 / 1.2838 / 0.2077 / 292.1 | 1609.052 / 12.440 / 0.7333 / 0.0000 / 179.2 |
| mixed / 2 | 798.467 / 26.025 / 1.2795 / 0.2112 / 301.6 | 796.097 / 26.298 / 1.2851 / 0.2059 / 303.3 | 794.066 / 26.713 / 1.2929 / 0.2061 / 302.1 | 1581.411 / 12.898 / 0.7305 / 0.0000 / 175.2 |

All24 cold-ready / first-answer observations (milliseconds) follow, including
adverse values. They are sparse local startup data, not deployment forecasts.
Other latency quantiles, full bodies/hashes, request counts, load-generator/
helper CPU and process context remain in the complete raw24 records.

| Workload / round | B cold / first | First cold / first | Suffix cold / first | Bun cold / first |
|---|---|---|---|---|
| signed / 1 | 155.456 / 90.395 | 62.992 / 31.810 | 82.343 / 32.332 | 956.575 / 19.163 |
| signed / 2 | 59.990 / 26.521 | 60.075 / 25.299 | 82.192 / 35.052 | 92.218 / 12.182 |
| ASCII / 1 | 58.776 / 35.734 | 60.015 / 36.928 | 60.001 / 36.950 | 96.924 / 4.249 |
| ASCII / 2 | 59.420 / 36.304 | 58.904 / 35.927 | 58.110 / 32.953 | 85.765 / 4.684 |
| mixed / 1 | 59.287 / 36.313 | 59.562 / 33.202 | 59.717 / 34.003 | 88.918 / 5.772 |
| mixed / 2 | 59.019 / 35.963 | 58.458 / 35.662 | 58.742 / 35.583 | 84.798 / 4.139 |

Whole-host CPU spans **23.869–49.520%**, load-average endpoints
**2.853–16.059**. All per-row host observations remain in raw; the two
short reversed rounds and differing controls/host context limit attribution.

| Artifact variant | Golden Wasm / gzip6 bytes | Focused Wasm / gzip6 bytes |
|---|---|---|
| B | 7325897 / 2690117 | 7364454 / 2662029 |
| First | 7326106 / 2690204 | 7364663 / 2662165 |
| Suffix | 7325992 / 2690291 | 7364549 / 2662129 |

Compression is a deterministic Wasm gzip6 proxy, not provider upload size.
First source `914367c6ef2cc5f921169d1e7f7970a3087b16e5`, tree
`a9647782e0c4c7fffa6e1bbd17751a407d2afd0d`, is retained at
`refs/experiments/a4-baseline-84e1c00b/p21-first`. Suffix source
`a7a2baf6be0bb7895046c5dc05b8a7906021c875`, tree
`62f9be011909559361b2842d90fa42f5fea6b2d4`, is retained at the same prefix's
`p21-suffix` ref and pins First as an exact manifest prerequisite. Shared H84
and B remain unchanged. All paths below are relative to archive A above:

| P21 record | Archive-relative path | SHA-256 |
|---|---|---|
| First build | `variants/p21-first/build.json` | `6414b0a92a29b82376a4138a12136a89900b1f0fb2be167aff42ceb051f476d6` |
| Dependent Suffix build | `variants/p21-suffix/build.json` | `4f483dd4f4010e3501ab1e7b1dc516d56a957823948a604c525270da692ceb86` |
| Six-host no-load smoke | `screens/p21-suffix/setup-cohort/results.json` | `a0da45553ef986e715708787d23170c3fabcbec30839258c46c12c647fcb6f0c` |
| Complete fixed24 result | `screens/p21-suffix/screen24-none-pg-off-c16-r2/results.json` | `c9859343eb377af7bbdc3aa6ba0892afe714c7f5224b82ccfdfad92eae546900` |

Baseline writer controls at `13b8aec8d4e93c7d31e48c07979f111bc0920e29`
pass 13 JSON tests. Initial First `d70680c1296b4b5cd2a00a7358313cf327d105fb`
passes 14 JSON tests plus the allocation check, but Clippy requires typed
`as_chunks::<8>()` views instead of chunks_exact. That setup-only source is
retained in the final candidate ancestry, with stderr in task 79261 and no
timed raw. Corrected First 9143 and Suffix a7a2 both pass the 14 JSON tests,
allocation check and affected Clippy. The root reviewed Suffix's nine-line
writer delta. Both source/build/artifact sets were retained before resetting
H84 for the next independent experiment. No earlier failed setup is presented
as a performance capture.

### P21.suffix-bitmap — original design and forecast

This design and its unknown performance forecast were recorded before the
measured/rejected bitmap form below. It depends on retained P21 First 914367c6,
not the rejected repeated-helper Suffix. The initial independent reference
checked all 256 byte values and 3420 selected words as math/source evidence,
not candidate app/performance results. The actual implementation preserves
First's checked 8-byte reads, little-endian lane order, reserve and ordinary
ASCII first-scan path. Original marginal and combined predictions stay
**unknown, including loss**; no additional unescaped-ASCII or ordinary-signed
work-path benefit was intended, and codegen/layout effects remained unknown.
Mixed writer 18.8% self remains sampled context, not promised removable cost.
Preserve the earlier scalar-span/repeated-helper negatives separately.

The assessed algebraic mask design (not a runtime-tested prototype) is:
`L = repeat(0x7f)`, `H = repeat(0x80)`;
`zero(x) = ~(((x & L).wrapping_add(L)) | x | L) & H`;
`control(w) = ~(((w & L).wrapping_add(repeat(0x60))) | w) & H`;
combine `control(w) | zero(w ^ repeat(0x22)) | zero(w ^ repeat(0x5c))`.
For each low-seven-bit byte l, l+127 is 127..254 and l+96 is 96..223;
no lane reaches 256, so no cross-byte carry occurs. OR-ing the original byte
rejects high bytes. Enumerate set bits only from exact masks, not the earlier
borrow masks with possible later-lane false positives. Word reads may cross
scalar boundaries, but String appends must start/end at discovered ASCII
escape positions or known string boundaries, never arbitrary word ends.
Canonical escape spelling and existing-prefix/append behavior remain required.

### P21.suffix-bitmap — measured result (2026-10-24)

**Reject this bitmap form; no promotion or favorable rerun.** Original marginal
and combined forecasts remain **unknown, including loss**. Actual source is
`b53754454cdbf6b31ec9d568446060d4882b9a1a`, tree
`03fe7bf6d63c67a6e0b7b1a0c360aa5a21bdcc92`, retained at
`refs/experiments/a4-baseline-84e1c00b/p21-bitmap`. It changes only json.rs
on retained First 914367c6, explicitly reusing First's 6414 build prerequisite;
it does not depend on rejected Suffix a7a2 or stack P55/P61. B4fda/H84 and
shipping Os flags remain unchanged, with no target feature or dependency edits.

**Actual implementation assessment — moderate.** Exact lane-mask arithmetic,
bit iteration, separate scalar-escape output and copied-span bookkeeping add
lasting UTF-8-boundary/canonical-control proof and test maintenance over First.
The code remains safe/platform-neutral, without a new dependency, unsafe code
or runtime API. First's early scan/reserve/unescaped-ASCII path is preserved.
Expected-red controls and a fixed slice-lint attempt are prototype development
roughness; a proper keeper would still retain the added mask/span invariants.
This matches the prospective moderate assessment. **Tradeoff / advice:** do not
carry this current form forward: extra writer complexity accompanies a clear
mixed loss and adverse ASCII p95/RSS. This rejects the tested form, not every
possible bitmap implementation or workload. Retain the earlier scalar-span and
repeated-helper suffix rejections as distinct source/results.

The fixed 24 cells cover B/First/bitmap/Bun × three workloads × two whole-order
reversed rounds, c16, 2s admission + 1s warmup, 2048 principals, inspector NONE
and PG durations OFF. All 24 complete with zero load/warmup errors, invalid
bodies and persisted changes; full-body/source/artifact/dependency guards and
144 saved served copies pass. Sleep is empty; clocks are 5.156–10.926ms and
all owned services/PostgreSQL are reaped. Bun is secondary cross-deployment
context; no fresh TypeScript/workerd arm or capacity/176/gate claim is made.

All gains use ratios of this screen's two-round medians, with both round ratios
retained. Fresh First/B ASCII is **+6.1243%**, a different campaign from the
historical +11.2918%; retain both and do not rewrite the original result or
forecast. The combined bitmap/B ASCII gain includes First's benefit and does
not turn a losing marginal bitmap into a keeper.

| Workload | B / First / bitmap / secondary Bun median req/s | Fresh First/B gain |
|---|---|---|
| Signed golden | 2252.472 / 2296.217 / 2303.342 / 6268.270 | +1.9421% |
| Large ASCII | 1292.209 / 1371.348 / 1349.325 / 5932.937 | +6.1243% |
| Mixed Unicode | 803.660 / 802.666 / 629.935 / 1609.245 | -0.1237% |

| Workload / comparison | Frozen prediction | Actual gain | Round1 / round2 actual gains |
|---|---|---|---|
| Signed golden / marginal bitmap/First | Unknown, including loss | +0.3103% | -0.1826% / +0.7978% |
| Signed golden / combined bitmap/B | Unknown, including loss | +2.2584% | +0.1497% / +4.4117% |
| Large ASCII / marginal bitmap/First | Unknown, including loss | -1.6059% | -3.8002% / +0.5480% |
| Large ASCII / combined bitmap/B | Unknown, including loss | +4.4200% | +1.4414% / +7.3813% |
| Mixed Unicode / marginal bitmap/First | Unknown, including loss | -21.5197% | -21.8352% / -21.2018% |
| Mixed Unicode / combined bitmap/B | Unknown, including loss | -21.6168% | -21.5073% / -21.7258% |

Mixed marginal RPS falls −21.8352/−21.2018% in the two rounds, service CPU rises
+27.6947/+27.2658% and p95 rises +29.9315/+21.0921%. ASCII marginal RPS is
−3.8002/+0.5480%; p95 and sampled peak RSS are higher in **both** rounds
(First 362.5→ bitmap 370.4 and 364.4→365.6MB). Signed median +0.3103% has no
intended ordinary-signed suffix-work benefit; retain its variation without a
blanket causal claim. No further bitmap rerun is selected.

Each cell below is **req/s / p95 ms / service CPU ms/request / separate helper
CPU ms/request / sampled peak RSS MB** with the same existing RSS-sampling
limits; PG and load-generator resources remain distinct.

| Workload / round | B | First | Bitmap | Secondary Bun |
|---|---|---|---|---|
| Signed golden / 1 | 2275.973 / 8.794 / 0.447368 / 0.122807 / 318.0 | 2283.551 / 8.901 / 0.447696 / 0.122297 / 314.2 | 2279.380 / 8.979 / 0.444396 / 0.122592 / 315.1 | 6187.699 / 3.622 / 0.198659 / 0.000000 / 202.7 |
| Signed golden / 2 | 2228.970 / 9.654 / 0.449866 / 0.132050 / 295.2 | 2308.884 / 8.800 / 0.442382 / 0.123004 / 315.5 | 2327.305 / 8.804 / 0.435155 / 0.122186 / 314.8 | 6348.842 / 3.554 / 0.192822 / 0.000000 / 205.6 |
| Large ASCII / 1 | 1288.445 / 18.784 / 0.795674 / 0.166087 / 359.0 | 1358.648 / 19.840 / 0.751466 / 0.168622 / 362.5 | 1307.017 / 20.121 / 0.777439 / 0.179116 / 370.4 | 5929.858 / 4.118 / 0.218013 / 0.000000 / 193.2 |
| Large ASCII / 2 | 1295.973 / 18.994 / 0.787553 / 0.165194 / 356.7 | 1384.049 / 18.202 / 0.748201 / 0.165468 / 364.4 | 1391.634 / 18.436 / 0.740343 / 0.168097 / 365.6 | 5936.016 / 4.128 / 0.217977 / 0.000000 / 189.1 |
| Mixed Unicode / 1 | 802.196 / 25.845 / 1.273964 / 0.204082 / 304.0 | 805.561 / 27.142 / 1.275416 / 0.209489 / 295.3 | 629.665 / 35.266 / 1.628639 / 0.212431 / 300.5 | 1604.303 / 12.329 / 0.734884 / 0.000000 / 174.4 |
| Mixed Unicode / 2 | 805.124 / 25.877 / 1.270820 / 0.209747 / 297.5 | 799.770 / 26.133 / 1.278709 / 0.211049 / 303.8 | 630.204 / 31.645 / 1.627358 / 0.212264 / 302.6 | 1614.186 / 12.496 / 0.731256 / 0.000000 / 178.6 |

Harness cold/first observations are not a separate cold-start benchmark.
Every cell is cold ms / first ms / idle RSS MB.

| Workload / round | B | First | Bitmap | Secondary Bun |
|---|---|---|---|---|
| Signed golden / 1 | 151.157 / 86.000 / 97.6 | 65.136 / 30.376 / 97.4 | 63.037 / 28.371 / 97.4 | 187.202 / 17.683 / 91.7 |
| Signed golden / 2 | 63.078 / 28.266 / 97.5 | 63.589 / 31.961 / 97.3 | 63.112 / 28.383 / 97.9 | 94.023 / 12.124 / 91.3 |
| Large ASCII / 1 | 58.640 / 35.612 / 94.0 | 59.719 / 36.530 / 93.9 | 58.853 / 35.831 / 94.0 | 95.503 / 5.092 / 89.9 |
| Large ASCII / 2 | 59.709 / 33.381 / 93.8 | 58.640 / 35.533 / 93.9 | 59.295 / 35.970 / 93.9 | 85.821 / 5.285 / 90.0 |
| Mixed Unicode / 1 | 60.636 / 36.035 / 93.7 | 60.119 / 33.815 / 94.1 | 59.864 / 33.898 / 93.8 | 84.380 / 2.867 / 90.4 |
| Mixed Unicode / 2 | 59.445 / 36.272 / 93.7 | 61.169 / 34.860 / 93.8 | 58.745 / 35.747 / 93.9 | 87.327 / 4.967 / 87.9 |

All host/load-generator CPU, load drift, elapsed/drain windows and status/body
fields remain in raw; load1 endpoints span roughly 3.08–16.72. Size comparison
is retained rather than omitted for a rejected form:

| App | B raw/gzip6 Wasm bytes | First raw/gzip6 bytes | Bitmap raw/gzip6 bytes |
|---|---|---|---|
| golden | 7325897 / 2690117 | 7326106 / 2690204 | 7326523 / 2690633 |
| focused | 7364454 / 2662029 | 7364663 / 2662165 | 7365144 / 2662235 |

gzip6 is a deterministic compression proxy, not provider upload size. Inputs
and flags are unchanged; the First prerequisite remains the exact retained
artifact, not a rebuilt replacement with a historical label.

Focused controls retain expected-red source
`a9ab01defc24f29edd7d51d21be901b4c7e2e081`. Initial candidate
`20f1a44b7813e7050e6a33b188151c6972974b1f` passed 15 JSON tests/allocation but
failed Clippy slicing; ref
`refs/experiments/a4-baseline-84e1c00b/p21-bitmap-clippy-slice-attempt`
retains that source. The checked split_at correction passes all 15 JSON tests,
allocation and Clippy before measurement. Setup attempts have no benchmark
raw or timing result; their logs below are correctness/setup evidence only.

Archive-relative identities use durable A:

| Record | Archive-relative path | SHA-256 |
|---|---|---|
| Candidate build with explicit First prerequisite | `variants/p21-bitmap/build.json` | `ed2d6a374736ed54786b1b0f576c1eb457b5d9dec3ee6053605b67db1ff2e77c` |
| Six-host no-load smoke | `screens/p21-bitmap/setup-smoke/results.json` | `d6c9de6ac89fb1e7ae4a535035e69a96d57b6d811c96df0b704aa5cbc9896b3d` |
| Complete fixed 24 result | `screens/p21-bitmap/screen24-none-pg-off-c16-r2/results.json` | `e66114af6655cda336edf284999ce97bbf5bd0224d523c8b75c1f2b8a46d4eec` |
| Complete screen console | `screens/p21-bitmap-screen24-console.log` | `a38fb58fde23245ce6de720a6db10d9b754348bcd6929933e454f1450da5eadf` |
| Expected-red focused controls | `screens/p21-bitmap-focused-red.log` | `e573e9536f805857d03b95265ae812803c19fb85d41f415235b1718cbb7dc624` |
| Tests/allocation passed, initial slice lint failed | `screens/p21-bitmap-focused-green.log` | `8a0ba8b319b80f868e3c9247131bbf00c7fca610f45002aac7bacbb4db9f65b8` |
| Checked suffix tests/allocation/Clippy pass | `screens/p21-bitmap-focused-checked-suffix.log` | `cb6aa5a3c7667cca78d6cc170b20ffbabb5a324fab4fb58a7b4f44564ac4f536` |

The following independent P11 plan was recorded before its mixed result below,
from H84/B4fda without prior experiment stacks. Its focused prototype skips chars().count/check_length when dimensions,
min_length and max_length are all absent. Source shows an empty field-rule
layer plus Payload's mandatory max_length 65536 layer; required bounded counting
remains, and shipping optimization may already remove redundant empty-rule
work. Keep pattern/format/version/standard validation order and Unicode scalar
counts unchanged. The original **0–5% large-text hypothesis** stays frozen;
before measurement, actual gain was unknown and could be zero/regression.
The prospective complexity estimate was **simple**, localized with no API/dependency/platform change. The proposed
fixed 18 B/P11/Bun screen kept Bun secondary. P13 was not stacked and remains
separate for later meaningful chain reuse. These are source/design
facts, not measured execution counts; this negative bitmap result does not
exhaust the queue.

## P11 — scalar-count first screen (2026-10-24)

**Measured mixed, prototype retained; no promotion or favorable rerun.** The
original **0–5% large-text throughput hypothesis** stays frozen. Signed is a
regression control, not a workload promised that range. This is independent
H84/B4fda with no previous experiment stack or prerequisite; marginal equals
combined P11/B. Source `13eb9c1e05c2533806055669b2afaf29e7fadfec`, tree
`09bb1d5876c1edf191c62c7030ed21966c18bc78`, is retained at
`refs/experiments/a4-baseline-84e1c00b/p11-count`.

**Actual implementation assessment — simple.** One local three-Option guard
skips chars().count/check_length only when dimensions, min_length and
max_length are all absent. Mandatory Payload max_length 65536 counting and
pattern→format→version→standard validation order remain, with unchanged
Unicode-scalar diagnostics. It adds no API, dependency, platform assumption
or new runtime interface, matching the simple pre-estimate. The lasting keeper
burden is ordinary semantic/order/Unicode controls; source/disassembly probes
are experiment scaffolding. **Tradeoff / advice:** retain the simple candidate
for later human selection, while weighing both-round signed regressions and
mixed tail/RSS against modest large-text gains. Simplicity does not erase
those adverse results, and there is no automatic carry-forward decision.

The fixed 18 screen is B/P11/Bun × three workloads × two full-order reversed
rounds, c16, 2s admission + 1s warmup, 2048 principals, shipping Os, inspector
NONE and PG duration logging OFF. All 18 rows complete with zero load/warmup
errors, invalid bodies and state changes; exact full-body/source/artifact/shared
bindings, 96 saved served copies and no-Sleep guards pass. All owned services
including PostgreSQL are reaped. Bun is secondary cross-deployment context;
no fresh TypeScript/workerd arm, long confirmation, 176-case qualification or
full gate is claimed.

Actual gain is `100 × (median P11 req/s / median B req/s − 1)` within this
screen, not historical-day raw-rate comparison or a percentage of Bun.

| Workload | Original frozen forecast | Actual P11/B gain (marginal = combined) | Round1 / round2 gains | Median B / P11 / secondary Bun req/s |
|---|---|---|---|---|
| Signed control | No frozen numeric signed prediction | -1.9439% | -1.4684% / -2.4140% | 2337.752 / 2292.308 / 6242.423 |
| Large ASCII | 0–5% large text | +1.4957% | +1.7420% / +1.2526% | 1291.897 / 1311.220 / 5936.217 |
| Mixed Unicode | 0–5% large text | +3.0801% | +1.5893% / +4.6511% | 776.897 / 800.827 / 1632.132 |

Signed RPS falls in both rounds (−1.4684/−2.4140%), while service CPU rises
+1.9598/+3.4061%, p95 rises +3.4523/+2.4934% and RSS rises 300.6→315.1 /
297.0→314.4MB. Unicode improves in RPS but round2 p95 worsens +8.6756% and
RSS is higher both rounds (301.8→308.4 / 293.3→302.3MB). Those are retained
adverse observations, not a no-regression or leak claim.

Each cell below is **req/s / p95 ms / service CPU ms/request / separate helper
CPU ms/request / sampled peak RSS MB**, under the existing process-sampling
limits. PG and load-generator resources remain separate.

| Workload / round | B | P11 | Secondary Bun |
|---|---|---|---|
| Signed control / 1 | 2324.210 / 8.632 / 0.435997 / 0.120275 / 300.6 | 2290.081 / 8.930 / 0.444541 / 0.122031 / 315.1 | 6238.967 / 3.621 / 0.197822 / 0.000000 / 206.0 |
| Signed control / 2 | 2351.294 / 8.743 / 0.429057 / 0.118946 / 297.0 | 2294.535 / 8.961 / 0.443671 / 0.121792 / 314.4 | 6245.878 / 3.606 / 0.194400 / 0.000000 / 202.5 |
| Large ASCII / 1 | 1283.459 / 19.078 / 0.799689 / 0.170807 / 354.5 | 1305.816 / 18.909 / 0.782144 / 0.167875 / 360.9 | 5931.015 / 4.247 / 0.217903 / 0.000000 / 192.7 |
| Large ASCII / 2 | 1300.336 / 18.637 / 0.792800 / 0.168518 / 362.1 | 1316.624 / 18.431 / 0.783794 / 0.166604 / 356.5 | 5941.420 / 4.113 / 0.216880 / 0.000000 / 187.7 |
| Mixed Unicode / 1 | 797.242 / 26.692 / 1.271028 / 0.205607 / 301.8 | 809.913 / 25.695 / 1.264508 / 0.213806 / 308.4 | 1632.861 / 12.284 / 0.734980 / 0.000000 / 179.8 |
| Mixed Unicode / 2 | 756.553 / 27.387 / 1.350820 / 0.229508 / 293.3 | 791.740 / 29.763 / 1.284461 / 0.213033 / 302.3 | 1631.404 / 12.210 / 0.734756 / 0.000000 / 176.8 |

Harness cold/first observations remain separate from a dedicated cold-start
benchmark. Each cell is cold ms / first ms / idle RSS MB.

| Workload / round | B | P11 | Secondary Bun |
|---|---|---|---|
| Signed control / 1 | 74.642 / 42.739 / 97.4 | 62.138 / 27.674 / 97.3 | 184.885 / 17.813 / 93.8 |
| Signed control / 2 | 62.237 / 27.654 / 97.4 | 59.509 / 24.864 / 97.3 | 86.702 / 13.461 / 89.4 |
| Large ASCII / 1 | 63.079 / 37.120 / 93.9 | 59.599 / 33.821 / 93.9 | 92.506 / 5.039 / 89.5 |
| Large ASCII / 2 | 58.235 / 35.312 / 93.9 | 58.307 / 35.384 / 93.9 | 84.490 / 3.602 / 89.1 |
| Mixed Unicode / 1 | 62.544 / 36.480 / 93.8 | 59.047 / 33.044 / 94.0 | 86.181 / 3.328 / 89.5 |
| Mixed Unicode / 2 | 59.826 / 33.784 / 93.8 | 59.288 / 33.267 / 94.0 | 83.500 / 5.086 / 89.7 |

All host/load-generator CPU, elapsed/drain windows, body/status data and load
drift remain in raw; load1 endpoints span roughly 3.25–8.82. Both Wasm modules
grow 30 raw bytes: golden 7325897→7325927, gzip6 2690117→2690104; focused
7364454→7364484, gzip6 2662029→2662039. gzip6 is a compression proxy, not
provider upload size. Flags and application inputs remain unchanged.

Baseline semantic controls at `0926f698bb310f3de2e07d0df95a600055db2dfb`
**already passed**, rather than being presented as expected-red tests. After
the change, eight Rules and three Validator tests plus affected Clippy pass.
Saved shipping-input disassembly shows baseline unconditional counting versus
the candidate's guard before counting at function level. This is static
instruction evidence, **not dynamic execution counts or proof of the measured
RPS cause**. An initial named-symbol filter warned that the symbol was absent
and emitted only a retained header; corrected address-range inspection required
no rebuild. That command warning is not a benchmark or producer failure.

Archive-relative identities use durable A:

| Record | Archive-relative path | SHA-256 |
|---|---|---|
| Candidate production build | `variants/p11-count/build.json` | `662352a4738a177f053bbdca46dd5f1c180c9d9915c63002c0bf8acaa76145da` |
| Four-host no-load smoke | `screens/p11-count/setup-smoke/results.json` | `6f249e99aef6ea9102cd65c1906ca713e45556c0aba3cdceeea851bfe2eea9dd` |
| Complete fixed 18 result | `screens/p11-count/screen18-none-pg-off-c16-r2/results.json` | `70a937a4dc2bab0007fa49861b1024237c8bed3ebe6d379f1230018cc21209f1` |
| Complete screen console | `screens/p11-count-screen18-console.log` | `9b8e00e52272dbdd2d05dc4c537d46d44eb43ccaa041e9a065f7fefb4fec9933` |
| Passing baseline semantic controls | `screens/p11-baseline-controls.log` | `63293de7bd62a7abfb1990f53d086fb632b37daa5cb03f3337ae83e07d9fd19e` |
| Candidate Rules/Validator/Clippy controls | `screens/p11-focused-controls.log` | `793112b39fa1b276b85be566b52b3eb3ccb34655ccb65b10ffaf686f7a4a5e81` |
| Baseline corrected function-range disassembly | `screens/p11-count-baseline-check-text-range.disasm` | `babfae5b5b0fc313872254a4efe6cc38551ab045d582e13492cc4aee2cec17fe` |
| Candidate corrected function-range disassembly | `screens/p11-count-candidate-check-text-range.disasm` | `53d08fe7e7472691cb03fb7787435f66cd4f5b2489a70f563640223839cc77d7` |

**P43.active-checks — conditional emerging alternative; unknown impact.**
Preserve P43's original 0–3% hypothesis separately. Rules::is_empty equals exact
Rules::default equality, including all markers, so prepared active-check indexes
could skip exact-empty checks while preserving the rules vector/public iterator
cardinality, Debug and graph-plan expectations. Do not physically filter layers;
keep nonempty order, built-in/transform/fault behavior and fake/RNG behavior
unchanged. Prospective complexity is **moderate** for added indirection, memory
and tests; a cached length bit alone still leaves the empty-layer loop. This
source-only idea overlaps P11, but no saving is inferred from P11's aggregate
result. No implementation, decision or promotion follows, and P05 stays next.

Next is **P05 independent design review**, H84/B4fda without a P11 stack.
Preserve its original **0–5% golden-read hypothesis**, with actual gain unknown;
there is no measured many-driver or native-warm claim. Prospective complexity
is **moderate**: prepare immutable PostgreSQL configuration at the existing
IsolateStarted boundary while constructing fresh invocation-local pools and
Supplies. Do not retain invocation sockets/state in the immutable preparation.
Keep preparation-once and lazy-per-driver variants separate if both are tried.
This is a source/design direction, not an implemented or measured P05 result.
P13 remains available for a later independently meaningful or explicit stacked
experiment; no arbitrary count or negative-result threshold ends exploration.

## P55 — safe Response constructor first screen (2026-10-24)

**Measured mixed/promising, prototype retained; no promotion or favorable rerun.**
The frozen prediction is **unknown gain or loss on all HTTP read workloads**;
no new numeric forecast or GC-causality claim is inferred after measurement.
This is independent H84/B4fda without P61/P21/P03/P02 or TS/native-control
stacks. Marginal and combined P55/B are the same comparison. Actual source
`12e6ae80fff373b2652845f1c982c5ce2dba894c`, tree
`5cab04bbc85178d20e3af6a7e87dff0ce116b1e1`, is retained at
`refs/experiments/a4-baseline-84e1c00b/p55-response`; the diagnostic ref ends
`p55-response-12e6ae80`. Source/artifacts/results are retained; verify those
records before any reset.

**Actual production-change assessment — simple; ownership validation moderate.**
The host writer substitutes the cached safe web_sys optional mutable-slice
Response constructor for an explicit owned Uint8Array copy and drops an unused
import. It adds no dependency, unsafe code, custom binding or public runtime
interface; headers/status/bodyless handling and the separate stream path stay
intact. This matches the prior simple estimate. A proper keeper still needs
moderate platform/ownership test maintenance: synchronous host snapshotting
must remain true after Rust memory overwrite/free/reuse/growth, rather than
being assumed from safe Rust types. Diagnostic example/IR/harness scaffolding
is separate from the production change. **Tradeoff / advice:** retain for later
human selection as a simple promising mixed-text candidate, while keeping
higher mixed RSS, flat ASCII and variable signed results visible; no keeper
promotion or no-regression/leak claim.

The hardened actual-workerd preflight has 40 ownership/constructor cases
(16 ownership plus 24 status/error), 16 proved memory grows, two negative
controls and HEAD/GET/stream checks. LLVM IR preserves
constructor→physical A5/5A overwrites→black-box barrier→drop ordering; the
Response body is read later and remains exact. Actual production OptionView
glue confirms the intended intermediate-copy removal. Earlier 3e6f841a
preflight also passed but lacked the anti-optimization barrier: retain it as a
weaker passing test, **not a failure or performance screen**. Final diagnostics
and production builds use current source and checked generated artifacts.

The fixed 18 cells are B/P55/Bun × three workloads × two full-order reversed
rounds, c16, 2s admission + 1s warmup, 2048 principals, shipping Os, inspector
NONE and PostgreSQL duration logging OFF. All 18 rows complete with zero load/
warmup errors, invalid bodies and state changes. Full expected body identities,
96 saved served copies, source/artifact guards and Sleep checks pass; clocks
are 5.024–10.206ms and all owned processes/PostgreSQL are reaped. Bun in this
screen is **secondary cross-deployment context only**; the earlier TypeScript/
workerd control is a separate campaign, not a fresh matched arm here. No
176-case qualification, full gate, long confirmation or production PR follows.

Actual gain is `100 × (median P55 req/s / median B req/s − 1)`, within this
campaign, with each round retained. It is not a historical-day raw-rate
comparison or a percentage of Bun.

| Workload | Frozen prediction | Actual P55/B gain (marginal = combined) | Round1 / round2 gains | Median B / P55 / secondary Bun req/s |
|---|---|---|---|---|
| Signed golden | Unknown, including loss | +3.3368% | +0.5128% / +6.1563% | 2323.760 / 2401.300 / 6378.137 |
| Large ASCII | Unknown, including loss | +0.1139% | +0.4272% / -0.1947% | 1287.202 / 1288.669 / 5927.545 |
| Mixed Unicode | Unknown, including loss | +4.9831% | +3.1294% / +6.9298% | 766.374 / 804.564 / 1627.268 |

Signed variation is substantial: +0.5128/+6.1563%; ASCII is effectively flat
(+0.4272/−0.1947%). Mixed improves in both rounds (+3.1294/+6.9298%), with
p95 −25.2453/−12.2047% and service CPU −2.1134/−6.0008%. Its sampled peak RSS
is **higher in both rounds**, B 302.1→ P 311.8 and B 311.0→ P 316.3MB (median
306.55→314.05MB). ASCII round1 p95 worsens +1.3255%. These short observations
support retention, not a blanket no-regression, leak or causal-GC conclusion.

Each following cell is **req/s / p95 ms / service CPU ms/request / separate
helper CPU ms/request / sampled peak RSS MB**, with existing process-sampling
limits. PG and load-generator CPU remain distinct.

| Workload / round | B | P55 | Secondary Bun |
|---|---|---|---|
| Signed golden / 1 | 2321.879 / 8.831 / 0.438804 / 0.120456 / 318.3 | 2333.786 / 8.758 / 0.436270 / 0.124038 / 317.0 | 6384.980 / 3.571 / 0.191721 / 0.000000 / 207.5 |
| Signed golden / 2 | 2325.641 / 8.708 / 0.433476 / 0.120172 / 298.1 | 2468.815 / 8.249 / 0.412121 / 0.115152 / 297.0 | 6371.294 / 3.588 / 0.192926 / 0.000000 / 206.8 |
| Large ASCII / 1 | 1277.457 / 19.012 / 0.802181 / 0.167445 / 356.5 | 1282.915 / 19.264 / 0.792233 / 0.166990 / 349.1 | 5912.177 / 4.190 / 0.219687 / 0.000000 / 192.7 |
| Large ASCII / 2 | 1296.947 / 18.909 / 0.796020 / 0.168389 / 358.5 | 1294.422 / 18.223 / 0.792308 / 0.165385 / 348.5 | 5942.913 / 4.273 / 0.216898 / 0.000000 / 188.6 |
| Mixed Unicode / 1 | 785.116 / 34.339 / 1.289507 / 0.208597 / 302.1 | 809.685 / 25.670 / 1.262255 / 0.208333 / 311.8 | 1635.144 / 12.172 / 0.729927 / 0.000000 / 176.7 |
| Mixed Unicode / 2 | 747.633 / 30.054 / 1.347047 / 0.218978 / 311.0 | 799.442 / 26.386 / 1.266214 / 0.210006 / 316.3 | 1619.393 / 12.454 / 0.732082 / 0.000000 / 175.2 |

Harness cold/first fields are not a dedicated cold-start benchmark. Each cell
is cold ms / first ms / idle RSS MB.

| Workload / round | B | P55 | Secondary Bun |
|---|---|---|---|
| Signed golden / 1 | 126.604 / 78.044 / 97.7 | 63.114 / 31.794 / 97.4 | 186.202 / 18.032 / 92.6 |
| Signed golden / 2 | 61.738 / 27.350 / 97.4 | 60.042 / 25.751 / 97.6 | 92.212 / 12.102 / 92.0 |
| Large ASCII / 1 | 60.953 / 35.081 / 93.8 | 57.919 / 34.926 / 93.8 | 87.810 / 4.019 / 90.8 |
| Large ASCII / 2 | 57.647 / 34.919 / 94.0 | 57.707 / 34.985 / 93.8 | 84.515 / 4.790 / 89.0 |
| Mixed Unicode / 1 | 57.495 / 34.860 / 93.8 | 58.003 / 35.359 / 93.7 | 84.465 / 4.426 / 91.2 |
| Mixed Unicode / 2 | 57.154 / 34.503 / 93.7 | 57.565 / 32.581 / 93.8 | 89.705 / 5.485 / 89.2 |

Whole-host/load-generator CPU, load drift, elapsed/drain windows and all status/
body data stay in raw; load1 endpoints range roughly 4.04–15.25. Module sizes
shrink 56 bytes in each app: golden 7325897→7325841 raw / 2690117→2690084 gzip6;
focused 7364454→7364398 raw / 2662029→2661990 gzip6. gzip6 remains a compression
proxy, not upload size. Shipping flags and application input maps stay unchanged
apart from the host-constructor source; do not attribute the observed workload
changes to an unmeasured GC mechanism.

Archive-relative retained identities use the same durable A:

| Record | Archive-relative path | SHA-256 |
|---|---|---|
| Candidate production build | `variants/p55-response/build.json` | `2cf97b0401029361f6b0dbbf585fd85bb58aa833a379dc148eddbc53542e5f00` |
| Four-host no-load smoke | `screens/p55-response/setup-smoke/results.json` | `f01654b5f57f457f52543ad7a3bf322ee6f9e23f3e8cd29ba7bb3be7260aaeec` |
| Complete fixed 18 result | `screens/p55-response/screen18-none-pg-off-c16-r2/results.json` | `c6c3b3ea7f45c9d5b1dd44eb390bdc728c9cf9a2ac6086cfa74faab56cd2e50b` |
| Complete screen console | `screens/p55-response-screen18-console.log` | `db8b96a621ab558cd6708fca8da3186a350cd32eba0124f86ee6e38fc33f3475` |
| Hardened ownership diagnostic | `diagnostics/p55-response-12e6ae80/build.json` | `22098603becee5ab50993cfcd8625dab9c6f3b34cdc7594233088198e78c5aed` |
| Actual ownership preflight | `diagnostics/p55-response-12e6ae80/preflight.json` | `cc82707110acbd6f1e43a3c601a8b59a777d9485ad9d23c8930fe474720fa640` |
| Constructor/overwrite/barrier/drop IR | `diagnostics/p55-response-12e6ae80/response_body.ll` | `b091448721c7a643cd67b37ba3fe4645ae66f7453e06cce19fb194aab94d9ad5` |
| Earlier weaker passing diagnostic | `diagnostics/p55-response-3e6f841a/build.json` | `5c36f728650a6724a22c129fa7a845f3ecdb9356388bfa4f1e7c5acf6af72c6d` |
| Earlier weaker passing preflight | `diagnostics/p55-response-3e6f841a/preflight.json` | `0e2c086249771df23bc7c0a145d264b2c268656b8bb76e19d4507c50ea715944` |

The dependent P21.suffix-bitmap was subsequently measured and rejected above;
its unknown forecasts stay intact. P11's mixed result follows above; independent
P05 design review is next with its frozen golden hypothesis and unknown actual
benefit, without a P11 stack. Retain P55's simple code/moderate ownership burden and adverse
RSS for later human selection; exploration continues without promotion.

## P61 — frozen roundtrip micro and conditional host sink

**P61.roundtrip is measured/rejected; P61.host-sink remains a distinct,
unmeasured alternative.** Both primary-pair controls remain established.
P11 is measured/mixed and independent P05 design review is next; P55's measured
result and ownership assessment remain retained above.
Continue eligible exploration without another baseline prerequisite.

The original source-only preparation fa204074/ref ending
`p61-roundtrip-fa204074-source-paused` remains retained as its historical state.
After exact H84 reset it was restored and completed independently of P02/P03/
P21 and the TS/native controls. Actual source is
`2f772a88421b5f1c2ed68332cbbb18e11dd46f15`, tree
`74898b7e6a36643a43ac6beec6787e03a009a05d`, immutable ref
`refs/experiments/a4-baseline-84e1c00b/p61-roundtrip-builtin-import`.
The frozen forecast remains **unknown, including loss; no application RPS
projection**. No production runtime API was integrated.

The frozen protocol uses the six string_screen shapes and five rotated/
reversed rounds. Rust-owned input is prepared before each timed group. Each
writer call allocates fresh output, writes/appends, drops output and consumes
a checksum. The baseline is the pure B Rust writer; the host variant additionally
scans canonical backspace/form-feed eligibility and performs Rust→JS conversion,
primitive JSON.stringify and JS→owned Rust String conversion/append, or falls
back to Rust. Dense
ASCII contains early control8/12 and takes the unchanged Rust fallback. The
primary clock is external host monotonic elapsed time around one fixed-N HTTP
batch, including request/binding/checksum-response overhead; it is **time per
write, lower is better**, not application throughput or tail latency.

| P61 variant | Frozen prediction | Observed result / metric | Scope / dependency |
|---|---|---|---|
| P61.roundtrip | Unknown, including loss; micro metric only | Rejected: clear time regressions in five non-fallback shapes; dense fallback effectively unchanged; no HTTP RPS measured | Independent H84/B4fda; full transfer-inclusive micro |
| P61.host-sink | Unknown, including loss; no claimed application gain | Not measured | Conditional alternative; does not depend on roundtrip winning |

### Roundtrip result and implementation assessment (2026-10-24)

**Reject roundtrip integration.** Medians use all five rounds, with time change
`100 × (median host ns/write / median Rust ns/write − 1)`; positive means slower.
No gain was forecast numerically, so do not turn these costs into a revised
throughput prediction. Five non-fallback shapes clearly regress; dense fallback's
+0.225% median time is effectively unchanged within its round variation, not a
separate meaningful regression. Rust is B's writer, not the P21 first-only helper.

| Shape | Iterations/group | Frozen prediction | Rust / host median µs/write | Host time change vs Rust | Rust / host process CPU median ns/write |
|---|---|---|---|---|---|
| small-ascii | 200000 | Unknown, including loss | 0.109073 / 0.570899 | +423.410% | 150.0 / 550.0 |
| large-ascii | 1024 | Unknown, including loss | 55.448730 / 116.506226 | +110.115% | 58593.7 / 117187.5 |
| late-escape | 1024 | Unknown, including loss | 55.376424 / 116.441609 | +110.273% | 58593.7 / 117187.5 |
| ordinary-unicode | 256 | Unknown, including loss | 167.336590 / 800.043946 | +378.105% | 156250.0 / 820312.5 |
| mixed-unicode | 256 | Unknown, including loss | 235.551270 / 1142.159508 | +384.888% | 234375.0 / 1132812.5 |
| dense-ascii | 128 | Unknown, including loss | 831.113281 / 832.983078 | +0.225% | 781250.0 / 859375.0 |

All 60 unique case/writer/round cells complete with expected checksums and
elapsed/iteration arithmetic. The preflight passes 44 actual canonical-byte
checks plus 4 throw/undefined/fallback-prefix fault checks; four saved served
copies and original served files match. All source/shared B/dependency,
316-artifact/current ICU/end-flag, Sleep and cleanup guards pass. The primary
clock gaps are 0.000204–0.004630ms, and all owned services were reaped. This is
not an application HTTP screen, production gate or 176-case qualification;
app RPS/p95/cold-start changes remain unmeasured.

Each cell below is **external ns/write / process CPU ns/write / point RSS MB**.
CPU derives from coarse/quantized process accounting and is supporting evidence,
not cycle-accurate timing. RSS 56.2–58.3MB is a point observation, not sampled
peak, lifetime maximum or leak proof. Complete elapsed times/checksums,
start/end windows, per-case ranges and all five rounds stay in raw.

| Shape / round | B Rust writer | Host roundtrip |
|---|---|---|
| small-ascii / 1 | 119.8446 / 150.0 / 56.2 | 580.8563 / 600.0 / 57.2 |
| small-ascii / 2 | 108.7337 / 100.0 / 58.1 | 570.8987 / 550.0 / 58.1 |
| small-ascii / 3 | 112.7194 / 150.0 / 58.1 | 579.1179 / 550.0 / 58.1 |
| small-ascii / 4 | 108.6442 / 150.0 / 58.1 | 561.2615 / 550.0 / 58.0 |
| small-ascii / 5 | 109.0729 / 100.0 / 58.1 | 560.6679 / 550.0 / 58.1 |
| large-ascii / 1 | 55495.6455 / 58593.8 / 57.3 | 116749.0645 / 117187.5 / 57.2 |
| large-ascii / 2 | 55307.3320 / 58593.7 / 58.1 | 116738.4844 / 117187.5 / 58.1 |
| large-ascii / 3 | 55448.7304 / 58593.8 / 58.2 | 116356.3232 / 117187.5 / 58.2 |
| large-ascii / 4 | 55506.4287 / 48828.1 / 58.0 | 116431.8847 / 117187.5 / 58.0 |
| large-ascii / 5 | 55298.0147 / 48828.1 / 58.1 | 116506.2256 / 117187.5 / 58.1 |
| late-escape / 1 | 55783.9355 / 48828.1 / 57.3 | 116441.6094 / 117187.5 / 57.3 |
| late-escape / 2 | 55279.2968 / 58593.8 / 58.1 | 116211.7109 / 107421.9 / 58.1 |
| late-escape / 3 | 55376.4238 / 48828.1 / 58.2 | 116271.1582 / 117187.5 / 58.2 |
| late-escape / 4 | 55434.5294 / 58593.8 / 58.0 | 116753.1328 / 117187.5 / 58.0 |
| late-escape / 5 | 55334.4727 / 58593.7 / 58.1 | 116529.9473 / 117187.5 / 58.1 |
| ordinary-unicode / 1 | 168203.2894 / 156250.0 / 57.3 | 801151.6916 / 820312.5 / 57.3 |
| ordinary-unicode / 2 | 167094.2383 / 156250.0 / 58.0 | 800043.9457 / 781250.0 / 58.0 |
| ordinary-unicode / 3 | 167336.5900 / 156250.0 / 58.2 | 799572.5896 / 820312.5 / 58.2 |
| ordinary-unicode / 4 | 167474.2853 / 195312.5 / 57.4 | 799785.8047 / 781250.0 / 58.0 |
| ordinary-unicode / 5 | 167116.2113 / 156250.0 / 58.2 | 802585.9374 / 820312.5 / 58.2 |
| mixed-unicode / 1 | 235546.0583 / 234375.0 / 57.3 | 1140809.5706 / 1132812.5 / 57.4 |
| mixed-unicode / 2 | 235430.5002 / 234375.0 / 58.0 | 1147546.5471 / 1171875.0 / 58.0 |
| mixed-unicode / 3 | 243089.8435 / 234375.0 / 58.2 | 1140477.2113 / 1132812.5 / 58.2 |
| mixed-unicode / 4 | 235551.2697 / 234375.0 / 57.4 | 1142159.5082 / 1132812.5 / 57.4 |
| mixed-unicode / 5 | 236074.8713 / 234375.0 / 58.3 | 1143559.2419 / 1132812.5 / 58.3 |
| dense-ascii / 1 | 834366.2103 / 859375.0 / 57.4 | 832983.0780 / 859375.0 / 57.4 |
| dense-ascii / 2 | 831113.2815 / 781250.0 / 57.8 | 841806.6409 / 859375.0 / 57.8 |
| dense-ascii / 3 | 830654.6251 / 859375.0 / 58.3 | 831306.9693 / 781250.0 / 58.3 |
| dense-ascii / 4 | 829134.7658 / 781250.0 / 58.3 | 829954.1014 / 859375.0 / 58.3 |
| dense-ascii / 5 | 831324.8672 / 781250.0 / 58.3 | 833820.3124 / 859375.0 / 58.3 |

**Actual prototype implementation assessment — moderate, isolated diagnostic.**
The retained example adds one fallible host writer with a canonical control-byte
scan, atomic append/fallback, probe-owned input preparation and opaque writer/
checksum controls. Existing js_sys bindings suffice, with no new dependency or
production runtime interface. The builtin schema-import repair and probe/IR
scaffolding are prototype setup work. **Proper keeper assessment — moderate,
still prospective:** a real integration would need a target-neutral host seam,
pure-Rust default, error/fault fallback and portable exact-byte tests. None of
that production API was delivered, and the estimate is not a completed keeper
implementation. This is consistent with the prior moderate roundtrip estimate.
**Tradeoff / advice:** do not carry this roundtrip integration forward: it adds
platform/fallback maintenance while measured transfer-inclusive time loses.
Keep its complete negative result; the larger independent host sink is not
rejected and the JS-escaping frontier is not exhausted.

Only the cached offline example was compiled/bound, with Rust 1.99 and exact B
child Os/fat LTO/one-CGU flags, remaps, proxy selectors and owned cache/ICU
paths; no new dependency, native build or application integration occurred.
Current 303 ICU files (10,160,786 bytes) were hashed before/after. This proves
current input identity, **not historical ICU byte equivalence**. LLVM IR retains
opaque indirect calls and black-box barriers; work was not replaced with a
constant output. The successful warm setup build took 4.40s.

The prior e023295d setup compiled offline in 25.25s and emitted bound/unbound
Wasm plus IR, but workerd failed its physical schema import before preflight.
Its complete:false build, exact host stderr and explicitly **partial observed
build tail** remain retained under its immutable source/ref. The corrected
source changes the import to binary builtin `/workerd/workerd.capnp`; the
cached package schema is only a hashed reference, not the server's import.
No micro timing is invented for that failed setup.

Archive-relative paths use durable A. All retained raw/log/proof identities are:

| Record | Archive-relative path | SHA-256 |
|---|---|---|
| Successful example build | `diagnostics/p61-roundtrip-builtin-import/build.json` | `0b46d479d7c6fc88a8562f335a123c1df713e05278a0780877d056292c9dc564` |
| Actual 44 canonical checks / four fault checks | `diagnostics/p61-roundtrip-builtin-import/preflight.json` | `27f8697ace5c8a197d0d04f6a40fa044c19ec73bcaafe68da8e5933f19b1b07b` |
| Complete fixed 60 five-round raw | `diagnostics/p61-roundtrip-builtin-import/fixed60-five-rounds/results.json` | `171a004fb35d5b3000a7438bfac2eb39dfa8111b1c6916fc9f3084a17f3cf844` |
| Complete successful setup stdout/stderr | `diagnostics/p61-roundtrip-builtin-import-setup.log` | `b46b8abdcce4e5e89818c0d6abb75d848018db2da6db338802b2219385343e1a` |
| Complete five-round console | `diagnostics/p61-roundtrip-builtin-import/fixed60-five-rounds-console.log` | `e8336e67b5e8feed40af701ed8f12c189ac151bd7104959af5a9f8a332635541` |
| Unbound example Wasm | `diagnostics/p61-roundtrip-builtin-import/unbound.wasm` | `7c00e8324e7d47b8165af2dfaca72f31892e8d2db3d5846523bb4ab8c431debb` |
| Opaque-call/black-box IR | `diagnostics/p61-roundtrip-builtin-import/driver.ll` | `28a6bbc9002c873d784e9b6531882e7b52835645b0a73681477e9ee8178ef6d9` |
| Failed schema-import setup | `diagnostics/p61-roundtrip-e023295d/build.json` | `79d2941a75be4bc0dc936ee92bf3fee2bbe22b0cef1aaad6b50b50b2fae51fb0` |
| Failed setup exact host stderr | `diagnostics/p61-roundtrip-e023295d/preflight-host.log` | `6407c9b3d5e6f44c876cd89438e4139ba630e82e4fe8c82a278b39624d6bcbb2` |
| Failed setup partial observed build tail | `diagnostics/p61-roundtrip-e023295d/build-tail-observed.txt` | `afe3fab31397025b16a89cca775568a0e59657aa5b376fc526906330da57909f` |

The bound example Wasm SHA-256 is
`f494128e4a59a5419ce97a627ddc6f98186db7623a2ba0226fdda798be88428d`;
its exact served/original map stays in the build/raw records. Failed source is
`e023295dbb096c51bbb4418e583d435d10f0f4f8`, tree
`c5e8a2fc118535d220c386274454d8e041e7a09e`, ref
`refs/experiments/a4-baseline-84e1c00b/p61-roundtrip-e023295d`.

### Host-sink alternative remains unmeasured

**Prospective implementation assessment only — complex.** A proper host sink
would retain new request-owned body/capability interfaces across codegen,
runtime and host, plus cancellation, byte fallback and custom-codec maintenance.
It is not implemented or measured. Its original unknown performance forecast
stays frozen; roundtrip losses do not determine its outcome.

Before full sink integration, consider a separately frozen **host-retained-
result lower-bound diagnostic**: retain input transfer, canonical scan,
JSON.stringify and scoped ownership while removing JS→Rust string return and
Rust append. Gain is unknown; current aggregate costs cannot be subtracted to
predict it. If final Response encoding is omitted, label that lower bound and
exclude any application-gain claim. Even a promising probe still needs full
HTTP ownership/byte validation. This is a conditional cheap check, not an
implementation or host-sink rejection. The bitmap form is separately rejected;
independent P05 design review is next, without a P11 stack.

**P61.host-sink remains eligible despite the measured roundtrip loss.** It would retain escaped JS string tokens/chunks through Response
construction. This is a substantive source-backed alternative, not an expansion
of the current probe; cost **L**, confidence low, ownership/semantics risk high.

Current [typed writers](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-codegen/src/entries.rs#L628)
and [generic Codecs::write](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-value/src/json.rs#L1354)
write into `&mut String`; [HTTP output](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/src/http/mod.rs#L1586)
moves that owned String into the [Response byte Vec](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/src/http/mod.rs#L108).
`into_bytes()` moves the buffer; it is **not another copy**. The
[Workers adapter](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/deployable.rs#L417)
then creates an owned Uint8Array and Response. There is no pluggable host sink
in those current interfaces.

The conditional design is a target-neutral sink separating raw Rust-formatted
tokens from TEXT escaping, with a pure String default and a new request-owned
body/capability seam across codegen, generic fallback, runtime and Workers.
Keep numeric formatting, field order/omission and custom-codec traversal in
Rust; batch raw punctuation/numeric tokens across the bridge. Do not use
whole-object JS/BigInt serialization, numeric coercion, hidden re-encoding or
global/cross-invocation JS state. A safe caught Response binding must accept
the existing JS value directly: cached web-sys 0.3.106
`new_with_opt_str_and_init` accepts Rust `&str`, which would reintroduce transfer
and defeat that retained-JS design.

Materialized/pure fallback may initially handle byte-dependent paths, including
[HEAD byte clearing](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/src/http/mod.rs#L434),
[idempotency persistence](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/src/http/idempotency.rs#L268)
and [response_value parsing](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/src/http/mod.rs#L2092).
Preserve custom codecs, streams, bodyless statuses, primitive-string escaping,
canonical control-8/12 fallback and errors/non-string results. Append each
escaped token only after complete success; never silently rerun a custom-codec
traversal as whole-document fallback. Release request handles on completion or
cancellation. JS chunk overhead, flushing/flattening, Response encoding and GC
can erase savings and must stay inside the measured accounting.

Host-sink overlaps P55 by bypassing the same intermediate body-copy opportunity;
do not add their gains. P55 still applies to byte-fallback paths. There is no
claimed Value/SQL clone elimination, and the current writer's 18.8% sampled
self footprint is context, not removable headroom. The host-sink alternative
itself has no implementation, test, build or capture.

## Current-B CPU diagnostic and compiler audit (2026-10-24)

**Instrumented diagnostic, not a throughput result or a new performance gain.**
D is `64e6b82d5e6ba65527c0521c9c9a33df1becdd02`, tree
`bed2294744be52282799608d147f226e6bfe72d8`, retained at
`refs/experiments/a4-baseline-84e1c00b/current-b-cpu-64e6b82d`.
Its only changes from H84 are `tools/bench/worker_diagnostic.py` and
`tools/bench/test_worker_diagnostic.py`. Production B4fda, H84 and shared
artifacts are unchanged; named JavaScript/non-custom Wasm sections were checked
against B. D is separate diagnostic tooling, not a P02 stack or a new B.

One capture each of golden signed read, large ASCII and mixed Unicode, c16,
2 s warmup and 8 s admission, named module/inspector, 2,048 principals,
PostgreSQL duration logging OFF. All 3/3 captures pass complete-body, unchanged
state, error/warmup, source/served-artifact, PG-off and Sleep guards. Owned
services were reaped. No full gate, 176-case qualification, production PR,
Bun comparison, capacity or p95 claim follows from these captures. The pinned
Bun binary's probed help did not advertise `--cpu-prof`; no flag or profile
emission was invented and no Bun HTTP performance capture was made here.

Profile weights are `sum(timeDeltas)`, not frame hit counts. The profiler
starts before load spawn and includes warmup, measured admission, drain and
stop-command overhead. Samples are not per-request wall stages or function-call
counts. Nonidle excludes only leaves literally named `(idle)`; `(program)`,
GC and unresolved samples remain in that denominator.

| Profile | Weighted total ms | Nonidle weighted ms | Samples |
|---|---|---|---|
| G signed | 10064.296 | 9547.906 | 54222 |
| A ASCII | 10022.366 | 9583.956 | 55387 |
| U mixed | 10029.083 | 9655.846 | 57092 |

Each cell below is **percent of all weighted samples / percent of nonidle
weighted samples**. The leaf families are disjoint and exhaustive; total shares
sum to 100%, and nonidle shares excluding idle sum to 100%, up to rounding.
An empty URL alone does not establish a native subsystem or removable work.

| Disjoint leaf family | G signed | A ASCII | U mixed |
|---|---|---|---|
| Named Wasm + Wasm bridge | 22.6020 / 23.8245 | 25.7569 / 26.9351 | 40.3548 / 41.9147 |
| Named empty-URL native/API/bridge | 23.9580 / 25.2538 | 28.4137 / 29.7135 | 22.0616 / 22.9144 |
| JS or builtin, nonempty non-Wasm URL | 20.3777 / 21.4798 | 17.9289 / 18.7491 | 15.0463 / 15.6279 |
| Unnamed empty-URL | 12.5837 / 13.2642 | 9.9564 / 10.4118 | 8.1731 / 8.4890 |
| (program) | 8.2165 / 8.6608 | 6.9286 / 7.2456 | 5.7063 / 5.9269 |
| Garbage collector | 7.1312 / 7.5169 | 6.6411 / 6.9449 | 4.9363 / 5.1271 |
| Idle | 5.1309 / excluded | 4.3743 / excluded | 3.7215 / excluded |

Selected self samples are subsets of those families, not extra budgets.
`connect/read/write` below require exact function name and empty URL; the
Cloudflare socket wrapper is excluded from native/API `connect`. The JS
queueMicrotask binding excludes the native queueMicrotask leaf cost.
A dash means not isolated/applicable in this table, not proof of no work.

| Selected leaf-self group | G signed | A ASCII | U mixed |
|---|---|---|---|
| Native/API connect | 8.5811 / 9.0452 | 7.0148 / 7.3357 | 4.8016 / 4.9872 |
| Native/API read | 2.8927 / 3.0492 | 10.4253 / 10.9022 | 9.2576 / 9.6154 |
| Native/API write | 5.9169 / 6.2369 | 3.9692 / 4.1508 | 2.3204 / 2.4101 |
| JS queueMicrotask binding | 6.2485 / 6.5864 | 3.7488 / 3.9203 | 2.8612 / 2.9718 |
| JSON write_string | — (union below) | 7.8301 / 8.1882 | 18.8153 / 19.5426 |
| UTF-8 from_utf8, all named contexts | — | 0.9202 / 0.9623 | 8.4612 / 8.7882 |
| Scalar count | — | 2.8162 / 2.9451 | 2.3069 / 2.3961 |
| Signed base64url decoder | 1.5216 / 1.6039 | — | — |
| copy_into | No sampled frame | 0.1541 / approximately 0.1611 | 0.0901 / approximately 0.0936 |

Each ancestor-union group counts a sample once if **any** matching ancestor is
present. These groups overlap one another and the self/family rows; do not add
them. A union is a calling-context footprint, not removable function cost.

| Ancestor-union group | G signed | A ASCII | U mixed |
|---|---|---|---|
| Queue::schedule_task | 6.5202 / 6.8729 | 6.2572 / 6.5435 | 5.0132 / 5.2070 |
| JsFuture From constructor + completion family | 6.9195 / 7.2938 | 8.4966 / 8.8853 | 7.1360 / 7.4118 |
| JSON write_string | 0.0504 / 0.0532 | 7.8378 / 8.1963 | 18.9141 / 19.6452 |
| PostgreSQL wire::decode | 0.0592 / 0.0624 | 1.0050 / 1.0510 | 8.5958 / 8.9281 |
| copy_into | No sampled frame | 0.2097 / 0.2193 | 0.1125 / 0.1169 |
| All Uint8Array::from slice-binding ancestry | 0.5775 / 0.6088 | 0.6422 / 0.6715 | 0.6224 / 0.6464 |
| Signed decoder | 1.9410 / 2.0460 | — | — |
| Named BLAKE3 | 0.2059 / 0.2170 | — | — |

The Uint8Array union includes PostgreSQL writes and other uses, so it is not
P55's response-copy budget. Queue and JsFuture groups include shared queue work.
Queue wrapper/native/schedule leaf samples partition by observed calling
context as follows (percent of all samples only):

| Queue leaf calling context | G signed | A ASCII | U mixed |
|---|---|---|---|
| JsFuture completion stacks | 4.4699 | 5.2512 | 4.3071 |
| future_to_promise contexts | 1.3557 | 0.6351 | 0.4819 |
| Other contexts | 0.5677 | 0.4364 | 0.2537 |

Those are sampled contexts, not callback/Promise/I/O counts. P59 cannot remove
true I/O completion, and P60 cannot claim all queue, GC or native connect cost.
Invocation-local sockets still require their connection; the existing helper
already reuses backend PostgreSQL sessions. Unknown/async-lost ancestry and
native/program samples remain unassigned rather than credited to an idea.
A constructor-versus-completion and true-I/O census is required before any
structural P59/P60 implementation or numeric impact promotion.

### Constructor-only refinement and planned callback/I/O census

A narrower union on the same timeDelta-weighted D captures matches JsFuture
constructor names ending `::from`, excluding `::from::finish`. It counts each
sample once under any matching constructor. This is still an ancestor context,
not all removable work or a callback count, and overlaps allocator/closure work.

| JsFuture constructor-only union | G signed | A ASCII | U mixed |
|---|---|---|---|
| Percent total / nonidle samples | 1.5917 / 1.6778 | 2.1169 / 2.2137 | 1.8135 / 1.8836 |

Caller partitions below are percentages of all weighted samples, with rounding:

| Constructor caller | G signed | A ASCII | U mixed |
|---|---|---|---|
| poll_read | 0.6184 | 1.4402 | 1.3717 |
| poll_write | 0.2487 | 0.2261 | 0.1211 |
| poll_shutdown | 0.1057 | 0.0953 | 0.0561 |
| Timer | 0.4319 | 0.1178 | 0.0730 |
| Other | 0.1870 | 0.2375 | 0.1914 |

Read+write constructor unions are approximately **0.8671 / 1.6663 / 1.4928%**
of total G/A/U samples. This narrows P59's directly identified constructor
mechanism; completion and queue unions must not be added to it. Causal GC
relief is unassigned, and not all identified constructor work is removable.
No original prediction or F01 number changes.

The next census is **planned, not an executed result**: separately pin
instrumented diagnostic JavaScript with unchanged named B Wasm, count existing
connect/read/write/close and bytes, then registrations/completions, closure
creation/invocation and queue scheduling/execution, classifying synchronous
completion versus queue-dispatch origin. Use WeakMap tags and callback
forwarding without extra Promise/then calls or global prototype edits. Baseline
JS is `b/artifacts/13055-kiosk.js`, SHA-256
`809b0b79a023e5c2c764256826551540f02e33b1c6064417cceda58fc71ad386`;
hook sites are connect165, close154/158/161, reader305/writer351,
queue433–434, read436–438, then515–521, write541–543 and makeMutClosure722.
Each census needs its own source/artifact identity and semantic guards; it is
not a D-equivalence or capacity claim. Whole-window counts include warmup/drain
and cannot be divided by measured-only request counts.

P03 remains an independent H84/B4fda experiment: its **original 0.5–2% signed
application-throughput prediction** was frozen before implementation and is
unchanged. Marginal equals combined P03/B, with no P02 stack. ASCII/Unicode
have approximately zero direct scope, with unpredicted regression/codegen
side effects still checked. The signed decoder's 153135 self-weighted µs and
195351 ancestor-union µs over 10064296 total µs support a bounded lead, not a
headroom promise or an observed gain. P03's separate matched screen above now
supplies its actual gains; these diagnostic weights do not change its original
prediction or isolate the screen's causal effect.

At the time of this diagnostic, P21 was the next bounded ASCII first-escape
lead, not a claim to remove the writer's entire 7.83% self footprint; its
subsequent result is retained above. Mixed input escapes nearly immediately,
so its first-scan scope stays near zero. P61's frozen roundtrip micro and its
independent host-sink alternative were paused for the primary control; both
primary pairs now exist, and eligible exploration resumes.
Current scalar-count samples motivate
P11/P13 checks while retaining mandatory max-length counting. Named BLAKE3's
small sampled footprint keeps P58 a small/unknown lead. These findings change
investigation priority, not original predictions or frozen F01.

The following P21.suffix prediction was frozen before its result above;
the measured current form is now negative/rejected. The original forecast is
retained verbatim rather than being rewritten from the observation.

`P21.suffix` is a separate conditional dependent variant, with the safe word-
first-escape helper as its prerequisite. After P21 first-only, compare repeated
helper/bulk UTF-8 suffix copying against both that prerequisite and B, reporting
marginal and combined gains. Freeze its additional signed/ASCII prediction at
approximately zero, mixed gain **unknown with zero/regression possible**;
the mixed fixture's ordinary gaps are usually shorter than eight bytes. The
earlier repeated scalar-span writer's approximately −5.6% mixed result remains
negative evidence; this new repeated word-helper mechanism still needs its own
measurement. It competes with P61 and does not replace the production suffix
before measurement. P21's original first-only predictions remain unchanged.

### Compiler audit: bulk memory already present

The audited pinned rustc 1.99.0 b940084d7 / LLVM 23.1.1 target defaults include
bulk-memory, multivalue, mutable-globals, nontrapping-fptoint, reference-types
and sign-ext, without default SIMD. Frozen B child flags only remap paths;
F/P02 add global simd128. All use shipping Os/fat LTO/one codegen unit.
Actual bound focused disassembly has:

| Bound focused module | memory.copy occurrences | memory.fill occurrences | SIMD instruction occurrences |
|---|---|---|---|
| B | 6641 | 185 | 0 |
| F | 6644 | 185 | 16708 |
| P02 | 6644 | 186 | 16969 |

These are static counts, not executed counts or hotness. B is not an MVP-only
module lacking a bulk-memory switch. The B named intermediate writer starts at
0x130d79, size 0x608; its first-escape scan is 0x130e16–0x130e42 with byte load
at 0x130e1f and bulk memory.copy at 0x130e7f/0x130f07. F's scan remains scalar
at 0x12b35f–0x12b38b, byte load 0x12b368; no SIMD occurs in that writer.
Thus global SIMD alone has not vectorized this particular scan.

The audit used Xcode's `llvm-objdump` (Apple LLVM 21), binary SHA-256
`792a7ab8593499fabb6316aa915de8833d61949d6329f89a0d6416f8ed99f4c8`,
with disassemble/demangle and the `jadpo_value::json::write_string` symbol.
Recorded workerd argv/config has no V8 flags. Cached capnp exposes v8Flags,
local help exposes Perfetto and binary strings expose Liftoff/TurboFan trace
machinery, but none proves the effective tier or hot optimized-tier completion.
Ordinary CPU profile names do not prove a tier restriction. A separate tier
diagnostic is conditional on remaining ambiguity, not an automatic next step.
No new production/native/176 build or benchmark was run for this audit.

### ABI audit: externref and multivalue already active

There is no newly discovered global wasm-bindgen ABI switch to enable.
Frozen B's saved `b/artifacts/13055-kiosk.js` (SHA-256
`809b0b79a023e5c2c764256826551540f02e33b1c6064417cceda58fc71ad386`)
passes objects directly at fetch/connect/read/then/write bindings and lacks
legacy heap/getObject/addHeapObject helpers. Its bound `13053-kiosk.wasm`
identity is in the artifact table below; TABLE includes externref (0x6f),
IMPORT includes externref initialization, and target_features includes
reference-types/multivalue. Pinned [Workers binding](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-codegen/src/workers.rs#L73)
uses web(true) without disabling overrides. Cached wasm-bindgen-cli-support
0.2.129 detects those input features and runs the externref transformation;
an older off-by-default comment does not establish a missing setting here.

[Initialization](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-codegen/src/workers.rs#L108)
is already once per isolate; saved glue caches decoders, encoders and memory
views. Scalar ASCII/Unicode transfer and owned Uint8Array copying still exist,
but belong to P55/P61/P59/P60, not a new global ABI candidate. This source/
artifact inspection adds no gain forecast, build, test or performance result.

All paths below are relative to the same durable archive A used for P02.
The diagnostic build/raw/profile identities are:

| Diagnostic record | Archive-relative path | SHA-256 |
|---|---|---|
| Prepared D build | `diagnostics/current-b-cpu-64e6b82d/build.json` | `77dca11e4fead1671285a999385efee670cf7668462497cb1e8eefab3c9b2cd8` |
| Complete three-capture result | `diagnostics/current-b-cpu-64e6b82d/workers-c16-w2-d8-pg-off/results.json` | `1b6ce3f6a385b6e8d3f6935cfca05b7ef3de5e9685bf26e40b5cdf5ca92c2c02` |
| G profile | `diagnostics/current-b-cpu-64e6b82d/workers-c16-w2-d8-pg-off/001-golden-read.cpuprofile` | `1933966f33def210e127d55c1202925e2b9b775a668ebf8b380a2c0729190e3d` |
| A profile | `diagnostics/current-b-cpu-64e6b82d/workers-c16-w2-d8-pg-off/002-ascii-large.cpuprofile` | `1f7fde88b133392e6837eb2dae6a619e65c29f04077ec04d9cf22b3753498cfe` |
| U profile | `diagnostics/current-b-cpu-64e6b82d/workers-c16-w2-d8-pg-off/003-unicode-large.cpuprofile` | `55d06e221c41c224d751181aceaf5e52d178c05ed9f31fce89ba07100f3fcc18` |

Static-audit artifacts are also retained, distinct from the production/timed
module paths; the scan addresses refer to the named intermediate artifacts:

| Artifact | Archive-relative path | SHA-256 |
|---|---|---|
| B named intermediate | `b/artifacts/13045-jadpo_worker_kiosk.wasm` | `338f5914a3f4d07e6e90ffeb0ee3b696d470ee5872624e532ebdfb481e7fda86` |
| F intermediate | `variants/f-simd/artifacts/00083-jadpo_worker_kiosk.wasm` | `121e8e4857913331dbbb2338c087b312ec018aa797d3298935365c57aba97ac5` |
| B bound focused | `b/artifacts/13053-kiosk.wasm` | `5f757dff726b79bec6e11f316cd2a330ff7c4bdb742f2ceaab22ef930d1fd21c` |
| F bound focused | `variants/f-simd/artifacts/00091-kiosk.wasm` | `7955e36cc4ec08a4325bcd3c328b3abbc7851fb4ef8cfecbb7b48804e715a61f` |
| P02 bound focused | `variants/p02-simd/artifacts/00091-kiosk.wasm` | `45a69fdce98caa47bff7ef7d6ecf1477fd929854763980e43050bfa8c8cec5ce` |

The following F01 section is preserved unchanged. Under Richard's later
method correction it is a **historical cross-deployment planning model**;
its Bun-relative gap is secondary to the target-matched primary pairs above.

## F01 — full-list planning forecast (2026-10-24)

This initial **speculative engineering forecast** is recorded before subsequent
experiment measurements and retained as F01. Future forecasts get a new dated
version; do not overwrite F01 after seeing results. It accounts for all 54 IDs
exactly once, including workload-irrelevant zeros. It predicts the union of
successful applicable keepers, not a requirement to ship every prototype.
Every measured aggregate gain below remains **Not measured**.

The forecast anchor is the existing a3 confirmation's production behavior at
c9d5, with final gated source 0fce and the exact identities linked above. It is
not a fresh matched baseline campaign for a new experiment. Worker baselines
G/A/U are **2296.1126 / 1301.4498 / 795.8662 req/s**; fixed Bun controls are
**6465.9123 / 6170.3874 / 1635.8239**. G is golden signed singleton, A large
ASCII unsigned GET, U mixed Unicode unsigned GET. F01 freezes a production-
behaviour/percentage forecast anchored to these historical numbers. Evaluate
actual percentage gains against a **contemporaneous matched B with the same
production behaviour**, recording campaign/reference identities and host drift;
never compare raw req/s across days. Each experiment still freezes its exact
B/harness/artifacts before measurement. Shared-harness or artifact remaps alone
do not require hindsight changes to F01: keep its absolute-rate predictions
fixed and label actual percentages against their matched B. A production
baseline or hypothesis-scope change requires a new dated forecast before
results, preserving F01.

For each workload, `W_forecast = W_baseline / (1 - f)`, with Bun held fixed.
Here f sums **assumed net removable current Worker CPU shares across disjoint
budgets**, not per-ID throughput percentages. Each table cell is
**low / central / optimistic percentage points of current Worker CPU**. The
budgets are analyst sizing assumptions, not a new measured profile. Their
nonoverlap is an explicit modeling assumption to test; shared implementation
work or interactions can invalidate it. Allocation-induced GC relief belongs
to its causal budget once. The historical 36–38% socket/bridge/scheduling/GC
category is not an additional removable pot.

| Exclusive budget / assigned IDs | G CPU points | A CPU points | U CPU points | Assumptions and zero contributions |
|---|---|---|---|---|
| Authenticated credential verification/Claims: P03, P07, P22, P23 | 2 / 5 / 8 | 0 / 0 / 0 | 0 / 0 / 0 | Cache hits mask decoder/parse/hex work; misses share the same budget. Representative hit rate and exclusive auth cost are unknown. |
| Invocation driver/configuration, timer and socket housekeeping: P01, P05, P08, P14, P15, P50 | 1 / 4 / 7 | 1 / 3 / 5 | 0.8 / 2.5 / 4.5 | P50 is zero for these uncontended singleton requests. Timer alternatives overlap. Causal GC/microtask relief is included once here. |
| HTTP headers, metadata and bridge: P06, P10, P25, P26, P27, P28 | 0.5 / 2 / 4 | 0.5 / 1.5 / 3 | 0.3 / 1 / 2.5 | P25/P26/P27 are zero: these GET requests have no incoming body, provider-response body or query string. Only P06/P10/P28 earn a budget. |
| PostgreSQL decoding and physical row materialization: P02, P04, P09, P12, P16, P17, P18, P51, P52, P54 | 1 / 3 / 6 | 2.5 / 6 / 11 | 5 / 10 / 15 | Count UTF-8, ownership, row-name/UUID/parameter allocations once, excluding validation and generated-body work. P51 is near zero for singleton rows; P52/P54 are zero for these nonnumeric/single-SQL reads. |
| Validation: P11, P13, P43, P44 | 0 / 0.5 / 1.5 | 0 / 3 / 6 | 0 / 3.5 / 7 | P44 is zero: no cross-rules. P11/P13/P43 overlap. Payload has max_length 65536, so mandatory scalar counting stays; central savings require genuinely redundant empty-rule/chain/check work. This is a major low-confidence assumption. |
| Remaining JSON response writing: P19, P20, P21, P53 | 0 / 0.25 / 0.5 | 0.5 / 2 / 4 | 0 / 0.5 / 1.5 | P20/P53 are zero for the current control/numeric content; P21 is approximately zero for mixed early escapes. The a2 writer is already in the baseline; do not reclaim its old budget. |
| Generated body/Value operations outside the previous budgets: P32, P33, P34, P35 | 0 / 0.25 / 1 | 0 / 0.5 / 1.5 | 0 / 0.5 / 1.5 | Tiny conditional credit for direct body clone/field/record work; exclude PG row allocations, validation and JSON writing already counted above. These simple reads have no deep action/field loops. |
| Absent input/stored-JSON, bytes and pagination paths: P24, P39, P40, P41, P42, P48 | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | Zero on all three workloads; potential benefits elsewhere are unmeasured. |
| Absent includes, collection/text algorithms and background joins: P29, P30, P31, P36, P37, P38, P49 | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | Zero on all three workloads; no singleton-read credit for those other workloads. |
| Cold startup: P45, P46, P47 | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | Zero warm-throughput credit; cold/RSS benefits remain unknown. |
| **Total assumed f** | **4.5 / 15 / 28** | **4.5 / 16 / 30.5** | **6.1 / 18 / 32** | Conditional savings, not confidence limits. |

Source basis is the pinned inspection in those IDs. Historical self-sample
anchors are P03 decoder 1.5–1.9%, P01 timer 1.9–2%, P02 UTF-8 mixed 6.7–7.1%
versus ASCII about 0.55%, and P08 receive buffer 0.12–0.20%. They predate a2/a3
and do not establish current budget shares; all other allocations and their
extrapolation are explicit assumptions. The focused
[Payload rule](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/tools/bench/fixtures/todo/domains/planning/types/payload.jadpo#L1)
requires its maximum-length check. a2/a3 savings are already in the baseline.

| Workload / conditional scenario | Assumed CPU removal f | Predicted throughput gain for anchored behaviour | Measured throughput gain vs matched B | F01 predicted Worker req/s (fixed) | Predicted share of fixed Bun |
|---|---|---|---|---|---|
| G signed / Low | 4.5% | +4.71% | Not measured | 2,404 | 37.18% |
| A ASCII / Low | 4.5% | +4.71% | Not measured | 1,363 | 22.09% |
| U mixed / Low | 6.1% | +6.50% | Not measured | 848 | 51.81% |
| G signed / Central | 15% | +17.65% | Not measured | 2,701 | 41.78% |
| A ASCII / Central | 16% | +19.05% | Not measured | 1,549 | 25.11% |
| U mixed / Central | 18% | +21.95% | Not measured | 971 | 59.33% |
| G signed / Optimistic | 28% | +38.89% | Not measured | 3,189 | 49.32% |
| A ASCII / Optimistic | 30.5% | +43.88% | Not measured | 1,873 | 30.35% |
| U mixed / Optimistic | 32% | +47.06% | Not measured | 1,170 | 71.55% |

The central scenario is **+17.65% / +19.05% / +21.95%** throughput for G/A/U;
even the optimistic case reaches only **49.32% / 30.35% / 71.55% of fixed Bun**.
This model therefore does not predict parity from the initial list. These are
conditional planning scenarios, not empirical confidence intervals or assured
lower/upper outcomes. Many experiments may fail or be irrelevant; actual net
gain can be below the low case, including zero or regression. Do not ship a
regression to satisfy a count.

Amdahl here assumes these local c16 workloads stay principally Worker-CPU
limited, SQL capacity does not become the next bottleneck, Bun is unchanged
and semantics survive. It predicts no p95, RSS, startup or real-edge result.
The largest sensitivities are representative Claims-cache hit/churn and
exclusive auth cost; row ownership/copy/allocation cost; redundant versus
mandatory validation scans; removable driver/header work; and whether the
proposed generated-body operations run. If only half the central assumed
removable budget is realized, the same model gives approximately
**+8.11% / +8.70% / +9.89%**, not the central gains. If none is realized it
gives zero. Actual regressions and new bottlenecks are outside that simple
positive-savings model and must be reported, not hidden.

When a combined stack is measured, put its observed percentage gain versus
its contemporaneous matched B beside the applicable frozen prediction for the
same production behaviour. Retain campaign/reference identity, host drift,
all rounds and short-screen limits; link source/artifacts/raw results and
every other/adverse metric. Keep F01's absolute-rate predictions fixed rather
than comparing them with a different day's raw rates. Retain its original
numbers so prediction accuracy remains inspectable. Separate future
workloads can reveal large benefits in zero-budget IDs without retroactively
crediting these three reads.

[source]: https://github.com/richardwillars/jadpo/commit/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0
[a3-finding]: https://github.com/richardwillars/jadpo/blob/0fceca62204cdc74a813871b3cf4298ea74c1c5c/docs/compiler-runtime.md#standalone-repeatable-read-handoffs-chunk-92a3-2026-10-24-london
[a3-method]: https://github.com/richardwillars/jadpo/blob/0fceca62204cdc74a813871b3cf4298ea74c1c5c/tools/bench/README.md#terminal-standalone-reads-92a3
[a2-pr]: https://github.com/richardwillars/jadpo/pull/375
[a2-finding]: https://github.com/richardwillars/jadpo/blob/0fceca62204cdc74a813871b3cf4298ea74c1c5c/docs/compiler-runtime.md#json-string-writing-and-bounded-wasm-screens-chunk-92a2-2026-10-24-london
[a2-method]: https://github.com/richardwillars/jadpo/blob/0fceca62204cdc74a813871b3cf4298ea74c1c5c/tools/bench/README.md#json-string-writing-and-fast-wasm-screens-92a2
