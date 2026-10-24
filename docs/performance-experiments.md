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
entered explicitly. Every measured-gain cell therefore starts as **Not measured**.
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

The proposed exploration baseline B is this a3 production behavior, provisional
until a3 finishes and its exact source/build/harness identities and dependency
PRs are frozen. **The documentation PR starts from current main; experiments
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

| Candidate / current status | Applicable workload / experiment | Predicted throughput gain (hypothetical) | Measured throughput gain | Basis, next check and pinned source | Confidence; cost; risk | Dependency / alternative / overlap IDs |
|---|---|---|---|---|---|---|
| **P02 — Safe SIMD UTF-8 validation** (queued; unmeasured) | Mixed large / ASCII reads; E | 2–6% mixed; 0–1% ASCII | Not measured | Historical mixed self 6.7–7.1%, ASCII ≈0.55%; conditional full-removal ceiling ≈7.2–7.7%, not achievable validation removal. Verify cached safe API and Wasm SIMD support; identical-feature control, no handwritten unsafe validator or download. [postgres/wire.rs:607](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L607), [postgres/wire.rs:648](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L648). | Medium; S–M; medium | Independent; overlaps P04 |
| **P03 — Signed base64url table decoder** (queued; unmeasured) | Signed reads; E | 0.5–2% | Not measured | Linear alphabet search, intermediate `Vec<u32>` and full canonical re-encode; historical self 1.5–1.9%. Preserve canonical alphabet/length/tail bits, UTF-8 and constant-time tag comparison. [http/signed.rs:319](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/signed.rs#L319), [encoding.rs:124](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/encoding.rs#L124). | Medium; S; medium parsing risk | Independent; may supply P24 helper; overlaps P07/P23/P24; masked by P22 |
| **P01 — Timer cancellation** (queued; unmeasured) | Signed/small reads; E + timer controls | 0–3% | Not measured | Promise timers resolve on early cancellation. Historical self 1.9–2%; GC/microtask saving unknown. Retain settlement and 8.4e no-leak guarantees. [timer.rs:34](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L34), [timer.rs:73](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L73), [timer.rs:85](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/timer.rs#L85). | Medium-low; M; high lifecycle risk | Independent; overlaps P14; measure both separately then combined |
| **P11 — Skip unneeded scalar count** (queued; unmeasured) | Large text; E | 0–5% | Not measured | chars().count runs without dimensions/min/max. Check actual rule chain and compiler output: benefit can be zero if count is otherwise needed/eliminated. [rules.rs:570](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-std/src/rules.rs#L570). | Low; S; low | Independent; overlaps P13/P43; optional P13 stack |
| **P05 — Prepare immutable driver configuration** (queued; unmeasured) | Golden reads; E + many-connections F | 0–5% golden; many connections unknown | Not measured | First DB access builds all configured connections and parses URLs/security. Separate preparation-once and lazy per-driver variants; measure configuration scaling. Sockets remain invocation-local. [factory.rs:204](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/factory.rs#L204), [deployable.rs:325](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/deployable.rs#L325). | Low; M; medium | Independent |
| **P04 — SQL text ownership** (queued; unmeasured) | Large text reads; E | 0–5% | Not measured | Verified raw → String → `Rc<str>` copies, cost unisolated. Profile allocation/copy first; moving String into Rc alone does not guarantee zero-copy. [postgres/wire.rs:607](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-drivers/src/postgres/wire.rs#L607), [read.rs:1101](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/read.rs#L1101). | Low; M–L; high representation risk | Independent; overlaps P02/P09; representation affects P52 |
| **P13 — Reuse scalar count across rule chain** (queued; unmeasured) | Deep-chain text; E + chain F | 0–5%; current-fixture relevance uncertain | Not measured | Avoid repeated inherited-rule scans. Profile chain count; preserve first-failure order and exact diagnostic count. [validate.rs:265](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/validate.rs#L265). | Low; M; medium | Independent, or explicit prerequisite P11 when stacked; overlaps P43 |
| **P19 — Cheap response capacity hints** (queued; unmeasured) | Multi-field/list responses; E + list F | 0–3% | Not measured | Response String starts empty; nested writer reserves text separately. Use cheap hints; avoid expensive full sizing traversal or unbounded retained buffer. [http/mod.rs:1590](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/mod.rs#L1590). | Low; S–M; low | Independent; overlaps baseline a2 reserve/P04/P53 |
| **P07 — Direct Claims deserialization** (queued; unmeasured) | Signed reads; E | 0–3% | Not measured | serde_json Map then field copies. Preserve historical encodings, optional fields and current duplicate/unknown-field behavior. [http/signed.rs:218](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-runtime/src/http/signed.rs#L218). | Low; S–M; medium compatibility risk | Independent; overlaps P03/P23; masked by P22 |
| **P06 — Batch request-header bridge** (queued; unmeasured) | Ordinary reads; E + header-heavy F | 0–3%; header-heavy unknown | Not measured | JS iterator → pair Array → separate string conversions/lowercase/trim. Preserve order, duplicates, case and trimming; profile bridge cost for header-heavy bound. [fetch.rs:215](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-host-workers/src/fetch.rs#L215). | Low; M; medium | Independent; overlaps P10/P28 |
| **P21 — Vectorize first-escape search only** (queued; unmeasured) | ASCII/late-escape reads; E | 0–3%; early-escape mixed ≈0 | Not measured | Keep a2 scalar suffix. Safe API/identical SIMD-feature control if relevant. This does not repeat rejected repeated-byte-bulk spans. [json.rs:1624](https://github.com/richardwillars/jadpo/blob/c9d5c98bf6aef9ad9ce673a51234fd7b42201ae0/crates/jadpo-value/src/json.rs#L1624). | Low; M; medium | Independent; baseline a2 writer retained |
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
