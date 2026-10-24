# Performance experiments

Updated 2026-10-24. This is the canonical catalogue of performance hypotheses,
experiment dependencies and retained results. The [roadmap](roadmap.md) remains
the only delivery tracker. This catalogue is Richard's explicit exception to
AGENTS.md's prohibition on plan documents and separate evidence packets: keep
one catalogue, link existing raw records and architecture findings, and do not
create a second progress log.

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
3. Freeze one baseline source **B**, its exact production artifacts, harness,
   toolchain, dependencies, feature/profile flags, workload, full-body/state/auth/
   quota controls and measurement protocol. Use one reusable scratch worktree
   beside the repository, never under `/private/tmp`. Coordinate with other
   sessions so no concurrent agent build or test contaminates a timed window.
4. Take the highest-priority eligible item in the queues below. **Before its
   implementation or measurement**, commit the variant's prediction in this
   catalogue: gain/range or honest unknown, exact workload, throughput metric,
   reference source/artifacts (B or prerequisite stack), confidence and basis.
   Retain that original forecast; never edit it with hindsight. A revised
   forecast belongs to a new dated variant/version, preserving the original.
   Then make a cheap focused prototype and its correctness controls. Independent
   hypotheses start again at B; dependent hypotheses stack on exact prerequisite
   commits. Record marginal and combined predictions against the prerequisite
   stack and B respectively, with each observed gain adjacent to its own
   prediction. Alternatives are separate variants; overlap warns against
   attributing the same savings twice.
5. Commit each prototype before building. Run focused correctness and a short,
   matched workerd/Bun screen using the frozen protocol; a microbenchmark is
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
   CPU, RSS, size and all adverse observations as well.
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

The tables are separate impact queues: primary reads, other warm workloads,
then cold start/module work. Continue an already-running prototype before
resuming the sorted queue. Within each workload queue, prioritize applicable
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
identity and protocol. Do not renumber IDs when sorting. `retained` means a
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

## Primary read queue

P02 is measured/mixed and retained as an exploratory prototype, with no
production promotion or favorable rerun. The current-B CPU diagnostic below
supported P03's independent signed-read experiment, now measured small positive
with limited causal attribution; its original 0.5–2% prediction is unchanged.
P21's independent safe u64 SWAR first-escape preparation is next, with the
original 0–3% ASCII/late-escape prediction and near-zero direct mixed/signed
scope. Optional P21.suffix keeps separate marginal/combined comparisons.
A transfer-inclusive P61 mixed-text micro precedes any host integration.
P11/P13 count work warrants focused checks; P59/P60 need a callback/I/O census
before structural implementation. These are priorities, not a batch limit.
F01 below remains frozen for the initial 54 ideas; later discoveries expand the
queue without retroactively changing that forecast.

| Candidate / current status | Applicable workload / experiment | Predicted throughput gain (hypothetical) | Measured throughput gain | Basis, next check and pinned source | Confidence; cost; risk | Dependency / alternative / overlap IDs |
|---|---|---|---|---|---|---|
| **P02 — Safe SIMD UTF-8 validation** (measured/mixed; prototype retained) | Mixed large / ASCII reads; E | 2–6% mixed; 0–1% ASCII | P/F mixed +6.071%; ASCII −0.586%; signed control +0.103%. Combined P/B +6.550% / −0.968% / −1.554% (mixed/ASCII/signed). [All rounds, predictions and adverse metrics](#p02--simd-utf-8-first-screen-2026-10-24). | Historical mixed self 6.7–7.1%, ASCII ≈0.55%; conditional full-removal ceiling ≈7.2–7.7%, not achievable validation removal. Verify cached safe API and Wasm SIMD support; identical-feature control, no handwritten unsafe validator or download. [postgres/wire.rs:607](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L607), [postgres/wire.rs:648](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L648). | Medium; S–M; medium | Depends on F (same-feature control); F/B and combined P/B predictions unknown; overlaps P04 |
| **P03 — Signed base64url table decoder** (measured small positive; prototype retained) | Signed reads; E | 0.5–2% | Signed +2.0625% vs B (marginal=combined); ASCII/mixed controls +1.4274%/+1.3656%, no direct decoder attribution. [All18 rows and limits](#p03--signed-base64url-first-screen-2026-10-24). | Linear alphabet search, intermediate `Vec<u32>` and full canonical re-encode; historical self 1.5–1.9%; current D signed self 1.5216%, ancestor union 1.9410% of all weighted samples (diagnostic scope, not removable headroom). Preserve canonical alphabet/length/tail bits, UTF-8 and constant-time tag comparison. [http/signed.rs:319](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/signed.rs#L319), [encoding.rs:124](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/encoding.rs#L124). | Medium; S; medium parsing risk | Independent; may supply P24 helper; overlaps P07/P23/P24; masked by P22 |
| **P01 — Timer cancellation** (queued; unmeasured) | Signed/small reads; E + timer controls | 0–3% | Not measured | Promise timers resolve on early cancellation. Historical self 1.9–2%; GC/microtask saving unknown. Retain settlement and 8.4e no-leak guarantees. [timer.rs:34](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L34), [timer.rs:73](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L73), [timer.rs:85](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L85). | Medium-low; M; high lifecycle risk | Independent; overlaps P14; measure both separately then combined |
| **P11 — Skip unneeded scalar count** (queued; unmeasured) | Large text; E | 0–5% | Not measured | chars().count runs without dimensions/min/max. Check actual rule chain and compiler output: benefit can be zero if count is otherwise needed/eliminated. [rules.rs:570](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-std/src/rules.rs#L570). | Low; S; low | Independent; overlaps P13/P43; optional P13 stack |
| **P05 — Prepare immutable driver configuration** (queued; unmeasured) | Golden reads; E + many-connections F | 0–5% golden; many connections unknown | Not measured | First DB access builds all configured connections and parses URLs/security. Separate preparation-once and lazy per-driver variants; measure configuration scaling. Sockets remain invocation-local. [factory.rs:204](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/factory.rs#L204), [deployable.rs:325](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/deployable.rs#L325). | Low; M; medium | Independent |
| **P04 — SQL text ownership** (queued; unmeasured) | Large text reads; E | 0–5% | Not measured | Verified raw → String → `Rc<str>` copies, cost unisolated. Profile allocation/copy first; moving String into Rc alone does not guarantee zero-copy. [postgres/wire.rs:607](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L607), [read.rs:1101](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L1101). | Low; M–L; high representation risk | Independent; overlaps P02/P09; representation affects P52 |
| **P13 — Reuse scalar count across rule chain** (queued; unmeasured) | Deep-chain text; E + chain F | 0–5%; current-fixture relevance uncertain | Not measured | Avoid repeated inherited-rule scans. Profile chain count; preserve first-failure order and exact diagnostic count. [validate.rs:265](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/validate.rs#L265). | Low; M; medium | Independent, or explicit prerequisite P11 when stacked; overlaps P43 |
| **P19 — Cheap response capacity hints** (queued; unmeasured) | Multi-field/list responses; E + list F | 0–3% | Not measured | Response String starts empty; nested writer reserves text separately. Use cheap hints; avoid expensive full sizing traversal or unbounded retained buffer. [http/mod.rs:1590](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/mod.rs#L1590). | Low; S–M; low | Independent; overlaps baseline a2 reserve/P04/P53 |
| **P07 — Direct Claims deserialization** (queued; unmeasured) | Signed reads; E | 0–3% | Not measured | serde_json Map then field copies. Preserve historical encodings, optional fields and current duplicate/unknown-field behavior. [http/signed.rs:218](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/signed.rs#L218). | Low; S–M; medium compatibility risk | Independent; overlaps P03/P23; masked by P22 |
| **P06 — Batch request-header bridge** (queued; unmeasured) | Ordinary reads; E + header-heavy F | 0–3%; header-heavy unknown | Not measured | JS iterator → pair Array → separate string conversions/lowercase/trim. Preserve order, duplicates, case and trimming; profile bridge cost for header-heavy bound. [fetch.rs:215](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/fetch.rs#L215). | Low; M; medium | Independent; overlaps P10/P28 |
| **P21 — Vectorize first-escape search only** (queued; unmeasured) | ASCII/late-escape reads; E | 0–3%; early-escape mixed ≈0 | Not measured | Current D ASCII writer self 7.8301%, but only its first-escape scan is targeted; that scan is scalar even in F. Retain the a2 scalar suffix, safe same-feature control and mixed near-zero first-scan scope; no full-writer/old-a2 credit. Conditional P21.suffix is separately predicted below. [json.rs:1624](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json.rs#L1624). | Low; M; medium | Independent; baseline a2 writer retained |
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
| **P59 — Reusable socket Promise completion state** (queued; unmeasured) | All PostgreSQL paths, especially fragmented reads; F | Unknown; current exclusive bridge/allocation/causal GC profile first | Not measured | Current D samples completion/scheduling contexts; constructor/completion and true-I/O census is still required to identify removable work. Fresh JsFuture per read/write at [transport.rs:445](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/transport.rs#L445) and [transport.rs:515](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/transport.rs#L515) creates Rc/state, two once callbacks and Promise.then. Prototype read-only, write-only then combined; retain owned bytes, settlement after cancel/drop, close/TLS/pending-read/error/slot/waitUntil leak guarantees. | Low; M–L; high lifetime/completion risk | Shared-state prerequisite explicit for variants; independent P01/P08/P55; overlaps P60 scheduling/GC, not all read/write/connect cost |
| **P60 — Invocation-local ready-task pumping** (queued; unmeasured) | PostgreSQL task handoffs; F; investigate first | Unknown; removable exclusive task-bridge budget only | Not measured | Current D samples completion/scheduling contexts; constructor/completion and true-I/O census is still required to identify removable work. Connection spawn [postgres/driver.rs:366](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-drivers/src/postgres/driver.rs#L366) and [transport.rs:205](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/transport.rs#L205) cross a microtask executor that defers wakes during polling. Drive only invocation-owned ready tasks; residual work stays in that invocation waitUntil. Preserve bounded fairness/deadlines/abort/drain/streaming/8.4e no-leak rules. | Low; L; high scheduling/lifecycle risk | Alternative/overlap P59; neither prerequisite; combined measurement if retained; distinct P49; no global drain/socket reuse or PG-backend-creation claim |
| **P55 — Safe synchronous Response slice constructor** (queued; unmeasured) | Large ASCII/mixed primarily; small signed control; F | Unknown; measure exclusive intermediate copy/allocation and causal GC | Not measured | Remove intermediate owned Uint8Array copy through cached safe Response slice API; current [deployable.rs:432](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-host-workers/src/deployable.rs#L432). Verify generated JS and construct→mutate/drop/reuse/grow-memory→async-read body ownership, exact binary/Unicode/empty/large bodies, bodyless 101/204/205/304, streaming, statuses/headers/errors. No borrowed view across await. | Low; S; medium lifetime/host-semantics risk | Independent of P10/P25/P26; allocation effects interact P61 |
| **P56 — Direct known standard-operation calls** (queued; unmeasured) | Hot pure standard op; F; current singleton reads may gain zero | Unknown; current body attribution/call census first | Not measured | Known Op still enters broad enum/argument-shape dispatcher: [body.rs:1908](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-codegen/src/body.rs#L1908), [compiled.rs:884](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-runtime/src/compiled.rs#L884), [lib.rs:89](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-value/src/lib.rs#L89). Arguments already stack slices, return unboxed; preserve Fault→Refused, inline pace/deadlines/recording/stack. LTO may already fold dispatch. Explicit roadmap 9.2 target. | Low; S–M prototype; high semantic risk | Independent of P34; may enable P57 dead-code pruning; measure separately/combined |
| **P58 — Explicit BLAKE3 wasm32 SIMD backend** (queued; unmeasured) | Short keyed signed tokens; F + actual signed screen | Unknown; old identified crypto budget only about 0.2–0.24%; small lead | Not measured | Cached BLAKE3 1.8.7 wasm32_simd feature is separate from global +simd128; current [Cargo.toml:245](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/Cargo.toml#L245) uses std+pure. Keep keyed format and constant-time Hash equality. Verify boundary-length/current/previous/wrong-key vectors, Wasm SIMD presence and workerd behavior. No substitution of Bun HMAC format. | Low; S–M; medium crypto/target risk | Independent P03; P22 can mask; B/C same flags or separate F/F+C comparison; reuse P02 F only as explicit control |
| **P61 — Conditional host long-text JSON escaping** (queued; unmeasured) | Transfer-inclusive long-text Wasm micro first; HTTP only if worthwhile | Unknown; transfer/append allocations may lose, especially ASCII | Not measured | Primitive js_sys::JSON::stringify only; current [json.rs:1620](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-value/src/json.rs#L1620) and [entries.rs:633](https://github.com/richardwillars/jadpo/blob/4fda5e597a3f508e1666ef46de0e0a3b850179b3/crates/jadpo-codegen/src/entries.rs#L633) need optional Workers writer capability with pure/native default. Exact canonical bytes require Rust fallback for U+0008/U+000C (or proven token-aware repair); catch failures and append only after success. Full Unicode/control/prefix differential checks and generic thresholds. | Low; S micro / L integration; high canonical-byte/capability risk | Conditional; competes P21/string writers; independent P55 but allocation/copy effects interact; no old a2 gain credit |
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
  pure and native defaults unchanged. This remains conditional on a successful
  transfer-inclusive micro, not a new language serialization choice.

## P02 — SIMD UTF-8 first screen (2026-10-24)

**Measured/mixed, exploratory prototype retained; no production promotion or
favorable rerun.** The parent independently checked the complete fixed 24-cell
raw capture. P uses safe SIMD UTF-8 validation; F retains standard validation
with the same global simd128 flag and an unused cached dependency, so F/B is
not a flag-only causal estimate. No BLAKE3 backend change was made. The frozen
P/F prediction was mixed +2–6%, ASCII +0–1%; F/B and combined P/B forecasts
were unknown. Signed reads are a regression control without a frozen numeric
prediction. Original ranges remain in the queue; no hindsight forecast edit.

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

P21 is the next bounded ASCII first-escape lead, not a claim to remove the
writer's entire 7.83% self footprint. Mixed input escapes nearly immediately,
so its first-scan scope stays near zero; P61 needs a transfer-inclusive text
micro before capability integration. Current scalar-count samples motivate
P11/P13 checks while retaining mandatory max-length counting. Named BLAKE3's
small sampled footprint keeps P58 a small/unknown lead. These findings change
investigation priority, not original predictions or frozen F01.

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
