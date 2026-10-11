# Wasm large-row investigation and backend coverage plan

Status: three bounded passes and a local workerd comparison complete, 30 September 2026. This is a separately scoped follow-up to the
[value-path experiment](wasm-value-path-results.md), requested by the owner.
The [row transport results](wasm-row-transport-results.md) cover attribution,
binary-row variants, local correctness/HTTP comparisons, current local startup and
validator diagnostics. Large-row parity failed; the primary small-probe regression
gate passed. The protocol below is retained as registered. General typed lowering,
the full coverage inventory and controlled/hosted qualification remain open. Bun
remains the working target; this pass does not select a backend.

The subsequent [immutable-row and write-attribution pass](wasm-typed-values-results.md)
adds modest large-read gains (about 3% throughput / 2% p95 / 3% CPU in concurrent
HTTP), while still failing Bun parity. SQLite timing localises the observed write
stalls mainly to native transaction completion and sometimes `UPDATE`. A separate
WAL/FULL comparison substantially improves both targets and puts the Wasm adapter
ahead on this fixture's write workloads. This qualifies neither sustained storage
behaviour nor a target switch; controlled-host/recovery and full coverage remain open.

The third [read-path pass](wasm-read-path-results.md) implements direct typed ingress,
compact checked queries and validated row receipts. The selected combination
improves concurrent large-read throughput about 11% over previous WASM, but remains
about 19% behind Bun. A [local workerd comparison](wasm-workerd-read-results.md)
then removes Bun from the application runtime and passes 585,339 requests. Its
concurrent large reads still trail generated JS by about 10% throughput and 7% p95
in paired medians. Removing Bun does not make the row-boundary issue irrelevant.
Next read work should attribute the workerd costs before another optimisation pass.
Neither local campaign establishes maximum server capacity or hosted cold starts.

## Question and starting evidence

Can the compiler remove the large-row transport penalty while preserving Jadpo's
validation, policy, failure and resource contracts?

The current candidate is SHA-256
`84cddcfff2fdfebd5eb5ef5013e2e1f8f9ee470cc3e51e6453551421aa85e90a`.
The 16 KiB row measured 16,829 isolated operations/s against Bun's 31,317;
concurrent HTTP measured 10,561 requests/s against 16,143, p95 2.6208 ms against
1.8143 ms, and server CPU/request 94.5 microseconds against 59.3. These are dated
local observations, not fixed acceptance targets for another machine.

Both targets already validate inputs and returned database rows. The generated
Bun `Item.read` calls `validate_Item`; its experiment wrapper also validates
entrypoint arguments. Wasm additionally checks its host protocol, and the current
storage adapter and generated guest both validate returned rows. Catalogue these
checks before claiming identical work or removing apparent duplication: they
protect different boundaries.

Today a returned row follows approximately this path:

`SQLite row → host validation → JSON stringify/UTF-8/copy → guest JSON parse →
guest validation/application → JSON serialization → copy/UTF-8/JSON parse in host
→ HTTP JSON serialization`.

Generic `serde_json::Value` trees, repeated encodings and copies are the leading
hypothesis, not a measured attribution of the entire gap. The older stage profiler
returned a small title from its probe; it must be extended to profile the actual
full-row return. Do not substitute its timings for that workload.

Wasm does not automatically accelerate validators. Measure already-loaded values
separately from values crossing the boundary. Unconstrained text type checks,
UUID checks, Unicode length constraints and record validation have different
costs. Adding authentication, policy and I/O to both targets may dilute a CPU win;
adding more computation inside the guest may amortise transport. Overall request
speed is an empirical outcome, not an assumed consequence of adding features.

## LR-1 — Establish comparable work and attribute the cost

1. Freeze the current module, generated Bun baseline, source/projection, toolchain
   and storage settings. Rerun paired baselines before changing the implementation.
2. Record where each target enforces input types, stored types, refinements,
   policy, result shape, resource bounds and faults. Any semantic mismatch becomes
   a correctness issue before a speed comparison. Diagnostic check-disabled probes
   may attribute cost but cannot qualify as candidates.
3. Extend profiling to distinguish SQL fetch, host row checks, host encoding,
   memory transfer/allocation, guest decode, guest validation, application work,
   guest encode, host decode, and HTTP response encoding. Use diagnostic guest
   counters or bounded component probes where external timers cannot separate
   phases. Record bytes transported, crossing counts, allocations where measurable,
   guest memory high-water and pool retention; identify unavailable metrics.
4. Separate three cases: fetch a large row and return a small field; fetch and
   return the full row; fetch and inspect/transform the large field. This separates
   inbound transport from outbound transport and actual application work.
5. Compare generated validators on valid and invalid values, with failure near
   the beginning and end of the record/text. Include type-only, UUID, Unicode
   min/max length and record checks. Report validation-only and complete-boundary
   results separately. Unsupported validators are coverage gaps, not substitutes.

Deliverable: a reproducible cost breakdown and ranked candidates. Instrumented
means are diagnostic; they must not be added to predict uninstrumented p95.

## LR-2 — Compare bounded implementation candidates

Implement candidates separately, choosing the first from LR-1's measured ranking.
Limit the first pass to the following three approaches rather than an open-ended
runtime rewrite:

| Candidate | Hypothesis | Required constraint |
| --- | --- | --- |
| A: fewer temporary buffers with the current JSON interface | Allocation and copies account for enough time to matter | Preserve ownership across suspension, memory growth and instance reuse; no unsafe borrowed views |
| B: generated typed JSON decoding/encoding | Generic value trees and field lookup are expensive | Generate from the checked schema, preserve unknown-field/null/refinement behaviour, avoid converting straight back into generic trees |
| C: generated typed row transport | Repeated JSON escaping/parsing dominates | Versioned internal frame with schema identity, explicit tags/nullability, bounded lengths and UTF-8 fields; validate on receipt |

Candidate C is the leading architectural experiment if the profile confirms the
transport hypothesis. Start with storage-row ingress, measure it, then add typed
row egress only if the result justifies it. Keep control envelopes unchanged
initially. Retain external HTTP JSON and the reference JSON implementation for
differential checks. This is an internal experimental interface, not new syntax
or a committed public ABI.

Use explicit ownership initially; do not make zero-copy a prerequisite. The
generated decoder may combine parsing and validation in one pass while preserving
all checks. Do not bypass guest validation, return unchecked raw host rows,
introduce result caches, or change query projections merely to improve the score.
Do not forward raw JSON directly to HTTP without proving output policy, response
shape, ownership and escaping equivalence. Such a shortcut is outside this pass.

Each candidate must work on renamed entities/fields and a source constraint
mutation. A fixture-specific fast path is not a compiler optimisation. If a
candidate shows no repeatable gain after one focused tuning pass, retain the
negative evidence and move on. If none closes the gap, report that result rather
than silently expanding into moving SQLite or the entire host into Wasm.

## LR-3 — Correctness and performance qualification

Use 256 B, 4 KiB, 16 KiB and 48 KiB text payloads, including ASCII, multibyte
Unicode, quotes, backslashes and control characters. Record actual encoded frame
sizes: all successful cases must fit the existing 65,536-byte frame limit,
including the envelope and escaping. Test just-below, exact-limit and oversized
frames separately; oversized cases must fail safely, not be silently truncated.
Include null/missing/extra fields, malformed UTF-8, lengths/offsets/tags, wrong
schema/version, forged policy, stale handles, traps, reset and interleaved users.
Do not add an unsupported multi-row query solely to populate this benchmark.

Preserve the existing probe/runtime, rollback, rejection, mutation/rebuild and
instance-reuse gates. Differentially compare values, authored failures, safe fault
events and database snapshots against Bun/reference behaviour. Keep the known
inner-fault attribution limitation explicit; neither hide it nor regress it.

Choose a candidate using isolated development measurements, freeze its hash, and
then run the HTTP qualification independently. Use sequential rotated runs, fresh
Bun baselines, concurrency 1 and 16, identical SQL settings, no retries and full
response verification. Qualification uses at least five paired repetitions with
2 seconds warmup and 10 seconds measurement per selected cell; retain raw results
and ranges, including losses. Avoid concurrent builds or other benchmarks.

Proposed experimental parity gate, fixed before candidate measurement:

- On the primary 16 KiB workload at concurrency 1 and 16, paired median
  candidate/Bun ratios: throughput at least 0.90; p95 and CPU/request at most
  1.10. Require at least four of five pairs inside these bounds for each metric.
- Small-read throughput, p95 and CPU must remain within 5% of the frozen current
  Wasm candidate by paired medians. Report whether its Bun advantage survives.
- Report p99/max, every payload class and memory retention; no errors, unbounded
  growth or silently excluded regressions. Other-size regressions above 10%
  require an explicit explanation and block a blanket large-row parity claim.
- If results are noisy or the no-work control has less than 2x throughput
  headroom, label HTTP capacity inconclusive. Isolated and CPU results remain
  useful, but rerun capacity with a separate load generator on a controlled host.

These tolerances define practical parity for this experiment, not statistical
significance, production capacity or an adoption gate. Repeat write smoke/tail
measurements to detect regressions; the existing write stalls remain a separate
unresolved adoption issue.

## LR-4 — Startup, hosting and decision

For the selected artifact, measure module compilation/instantiation, process to
ready, first successful HTTP response and warm response separately. Use at least
20 fresh processes per target, rotate order, and record filesystem/module cache
conditions, bundle size and memory. This replaces extrapolation from the earlier
46 ms Wasm / 87 ms Bun first-operation observation.

Qualify the exact new artifact on Cloudflare using the existing bounded hosted
correctness suite. Distinguish deployment-reported startup from measured requests
to fresh instances; if instance coldness cannot be established, say so. Controlled
server testing uses matching hardware/storage/runtime configuration and a separate
load generator; infrastructure provisioning and deployment are later execution
steps, not actions performed by this planning change.

Deliver a candidate decision, reproduced hashes, raw evidence and remaining gaps.
Retain Bun as the reference and working target until the independent coverage,
write behaviour and operational gates below are satisfied.

## COV-1 — Build the backend coverage inventory

Include the [HOST-1/HOST-2 capability review and probe plan](wasm-host-capabilities.md).
Separate language lowering from host functionality: a Worker can support the Wasm
instructions while lacking a requested filesystem, threading or process capability.
The initial Cloudflare/Bun matrix is documentation-backed; new hosted probes remain
unexecuted. Capability failures must be visible before deployment.

Before extending the backend broadly, inventory implemented language/runtime
contracts from the checked IR, Bun generator, compiler/runtime tests and golden
cases. For each capability record: Bun support; Rust/Wasm lowering; direct-Wasm
lowering; host dependency; positive/negative evidence; and the smallest next slice.
Use statuses `verified`, `implemented but unverified`, `unsupported`, and
`language contract unresolved`. Compilation alone never means verified.

Cover scalars/refinements/records/enums/collections; expressions/control flow and
calls; failures/diagnostics; queries/mutations/relations/migrations; transactions
and savepoints; authentication/principal freshness; row/field/invoke policy;
configuration/secrets/time; HTTP and external capabilities; suspension/cancellation
and resource budgets; and build/debug/deployment tooling. Distinguish missing Wasm
support from semantics that remain unresolved for every target.

## COV-2 — Qualify representative vertical slices

Order implementation from the inventory: (1) core values, control flow and failures;
(2) storage, policy and transactions; (3) real authenticated HTTP requests,
configuration/time and lifecycle; (4) the complete golden application once its
source/contracts are executable. Reuse the same semantic fixtures across targets,
including invalid data, cross-principal access, failures and rollback.

Production-auth comparisons must include equivalent fresh principal and credential
checks. Preserve the owner's query budget: one principal lookup, credential/session
checks counted separately. The current fixed-principal performance fixture does
not count as authentication coverage.

Services/jobs or other undecided contracts remain separately parked. Record
unexecuted golden cases openly. Rust-mediated versus direct generation stays a
separate route decision: gains on the Rust candidate do not establish direct-route
coverage. Default-target migration requires a further review of coverage, writes,
startup, host portability, build cost and diagnostics, not just the LR parity gate.

## LR-5 — Request-local row receipts and compact checked queries

Owner-requested follow-up, 30 September 2026: focus on large reads and explore
additional designs. The implementation is isolated in `experiments/wasm-exp1/read-path`;
the previous immutable-row artifact and host driver remain the frozen comparison.
The LR-3 acceptance thresholds and HTTP protocol above remain unchanged.

This pass compares independently selectable features:

- Directly decode binary storage rows into owned typed fields, using generated
  validators from the same effective types and independently counting the legacy
  JSON envelope budget inside the guest.
- If the guest returns the exact immutable row it validated, return a 48-byte
  receipt identifying the schema and the current request's read operation. The
  host resolves it only against its private scalar snapshot for that invocation,
  then returns a fresh ordinary object. Field extraction and materialisation use
  normal value transport. No row or authorisation decision is cached across calls.
- Replace repeated static read descriptors with a compact checked-plan identifier
  plus the runtime predicate. Resolve it against generated, digest-bound metadata,
  then pass the full unchanged descriptor to the existing authority/policy adapter.
  Recompute/enforce the original JSON pending-envelope limit despite the shorter
  transport. Preserve every fresh database read and principal-scope check.

The initial typed-ingress and receipt variants lost; retain their source, module
and raw measurements. A focused host-side diagnosis found slow per-string escape
scans and slower JSON serialization of frozen snapshot objects. Compare the revised
versions with ordinary private scalar snapshots and native JSON size measurement;
select using isolated rotated measurements before independently qualifying HTTP.
Neither a smaller frame nor a favourable component timer counts as a request win.


LR-5 is now measured: 45 local tests/12,785 assertions, nine native tests,
source-renaming/constraint mutation, adversarial cases and byte-identical rebuild
checks pass. The completed independent HTTP campaign has 80 cells and 12,441,614
verified requests. The earlier sleep-interrupted campaign is retained separately.
Candidate selection and all losses remain in the [results](wasm-read-path-results.md).

## LR-6 — Attribute the intended workerd host

The owner clarified that Bun is not the intended WASM runtime. The first local
workerd/SQLite Durable Object comparison is complete with the same selected
artifact and an adapted compiler-generated JS baseline. It is a shorter
exploratory protocol, not a substitute for LR-3 or a cloud deployment. It retains
all five paired runs, typed-ingress alternatives and a no-SQL RPC control.

Next, profile the separate workerd stages without using component means as HTTP
predictions: SQL result materialisation; authority checks; descriptor handling;
JSON/binary host-to-guest transfer; generated guest validation; receipts or full
return encoding; and Worker/Object RPC. Use matched payload bytes and include
full-row and field-extraction cases. Keep the frozen candidate and failure cases.
Then select a change before independent longer HTTP qualification on a controlled
host, with adequate generator headroom. Measure hosted cold starts separately;
local warm-process timings cannot replace them. Broader COV-1/COV-2 remains open.


LR-6 now has a [framework-informed attribution/host-driver pass](wasm-workerd-boundary-results.md)
with [primary-source research](wasm-framework-boundaries.md): 24 instrumented
cases, two isolated selection rounds, 48 passing tests and 608,612 verified HTTP
requests. The selected direct writer/borrowed output view keeps the guest identical.
Concurrent large-ASCII paired medians improve about 3% throughput and 1% p95 over
previous WASM, but only three of five pairs win. The revised binary and adaptive
encoders retain plain-ASCII regressions despite helping escaped text in isolation.
The next attribution slice is those escaped-value encoder/decoder costs; the longer
controlled-host qualification, hosted cold starts and broader coverage remain open.


LR-6 also has a [native-runtime-informed value pass](wasm-native-values-results.md),
with [Bun/Node/CPython source research](wasm-native-runtime-values.md). Conservative
host size proofs and bounded content selection cut escaped isolated time about
45%. All 53 tests, 1,060,000 isolated measured calls and 924,283 HTTP requests pass.
Concurrent escaped HTTP improves 16.6% throughput/14.8% p95 versus previous WASM
in all five pairs, approximately matching JS on that local fixture. Other workloads
retain JS gaps; small and Unicode-only HTTP regress against previous WASM. The
candidate and losing direct-write/full-scan alternatives remain experimental.
Guest bytes/validation and seven-page retention are unchanged. Next attribution
should separate remaining host and guest string work, broaden the text corpus and
transformed-result cases, then qualify longer runs with independent generator
headroom. Hosted cold starts and backend coverage gates remain open.


The subsequent [guest string and bulk-memory pass](wasm-guest-scans-results.md)
rebuilds the guest with SIMD UTF-8/exact escape counting, bulk-memory instructions
and an explicit contract to check the logical JSON limit once inside the guest.
It preserves the original size-error envelope and physical frame bound. Fifty-two
host tests, nine native Rust tests, source mutation/clean rebuilds and 949,050 HTTP
requests pass. Isolated large-text time improves 14–52% over prior sampled WASM;
retention falls from seven to six pages. Single-request large throughput approaches
JS, but concurrent large cases still trail 5–14%, with retained Unicode/p99 losses.
The [shared-frame follow-up](../../experiments/wasm-exp1/workerd-shared-rows/README.md)
passes its lifetime/adversarial tests but gains only 1–2%; keep it unselected.
Next: profile the final optimized host/control path, investigate concurrent Unicode
and tail behaviour, and run longer qualification with independent generator
headroom. These local results do not close LR-3, cold starts or backend coverage.
