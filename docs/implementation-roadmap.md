# Jadpo implementation roadmap

**Updated:** 2026-10-06 · **Scope:** all recorded work, with open tasks below and completed/deferred context indexed

Completed capabilities, dated evidence, and the previous phase descriptions are
in [Implementation history](implementation-history.md). This roadmap owns work
and sequencing; the [decision register](decision-register.md), specifications,
and [language issues](language-issues.md) own semantics. Existing phase and issue
IDs are retained below for traceability.

**Looking for something previously discussed?** Start with the
[topic and legacy-work index](#discussion-and-legacy-work-index), including
**Revset**, the web review UI, **Jev**, money/time, advanced language questions,
native Rust/Wasm, documentation benchmarks and the developer console/MCP.
The [conversation reconciliation](conversation-coverage.md#4-roadmap-reconciliation--2026-10-01)
records the sources checked and their limits. Detailed linked requirements are
part of a task's acceptance scope; a short task row does not replace them.
History contains a preserved source snapshot **including unfinished requirements**;
being stored there never means a requirement was completed or withdrawn.

The next product milestone is the complete golden todo on the existing Bun
target. The latest recorded [supported checkpoint](work-plans/golden-delivery-planning.md#rm-301307108--services-durable-jobs-and-reminders)
passes all 65 checks, with recorded golden and release gates still open. Closed
native scheduler assembly has independently reviewed SQLite/PostgreSQL boundary
evidence; full worker/public activation gates remain open.
The 44 integrated golden acceptance cases remain unexecuted. Focused UserWithTodos implementation evidence is in
[RM-107 history](implementation-history.md#2026-10-02--rm-107-self-scoped-user-todo-route).

## How to read and maintain this plan

The [planning assessment and 2026-10-03 sweep](work-plans/roadmap-assessment.md) now
covers the open tasks recorded at that sweep with a proportionate next-stage plan. Later intake has explicit planning states in the same assessment. This includes
decision/probe plans and conditional tasks; it does not approve unresolved
contracts or make tasks execution-ready. The original 2026-10-01 assessment
covered 89 open tasks and remains as historical evidence.

- Execute work using the [adaptive roadmap workflow](roadmap-workflow.md), which
  selects planning depth, routes work by model suitability and records evidence.
- `RM-xxx` identifies a task permanently; `E01`–`E11` identify epics. Dependencies
  name task IDs or external gate IDs from the table below. `—` means no open
  prerequisite; already implemented foundations are not repeated as dependencies.
- **Ready** means work can start; **queued** means its listed prerequisites must
  finish; **decision** means prepare/resolve a contract with its named owner;
  **external** needs evidence outside local implementation; **conditional** is
  unscheduled until its trigger is met. These statuses describe remaining work.
- `Next stage / model routing` records the next necessary stage, preferred
  model/effort, and any model deferral with reason/resume condition. Linked task
  plans can hold acceptable alternatives. Assess on selection; do not infer fit
  from generic historical recommendations. This column does not change task
  dependencies, planning status, completion or authorisation.
- Estimates are **agent session hours**, including investigation, implementation,
  verification, agent review, documentation and likely rework. These are full open-task
  forecasts, not measured remaining time after partial runs. S/M bands use observed analogues;
  L is a broader integration allowance; N is a weak new-subsystem extrapolation;
  U is unmeasured external work. See the [timing evidence and band definitions](task-timing/README.md).
  These are provisional ranges, not measured task durations or delivery promises.
  All current open-task forecasts were reviewed against the 2026-10-01 timing snapshot;
  14 previously unestimated tasks now have provisional scope-based ranges.
  Existing bands are retained: only two completed tasks have local timing records,
  below the five-comparable-completions calibration threshold. See the
  [review and retained forecasts](task-timing/README.md#2026-10-01-forecast-review).
  A numeric forecast does not change a task's planning or activation state.
  Human effort, external waiting and unattended soak time are separate. Conditional
  estimates apply only if activated; revise decision-dependent estimates when the
  contract is settled. The previous engineer-day estimates are withdrawn.
- Start a [task timer](task-timing/README.md#record-future-work) before work, record
  verification and pauses/blockers, and finish with evidence. Preserve estimates,
  partial attempts and rework; use recorded results to recalibrate comparable tasks.
- Each task's completion condition is in its description. Finish it with linked
  evidence, move its row into the history, and retain its ID there so dependants
  still resolve. Add dated progress to history or the validation ledger rather
  than growing this document into a log. Never renumber IDs or equate related
  unit tests with integrated acceptance.
- Preserve the recognisable name, motivation, constraints, alternatives and
  acceptance evidence of each idea. A move/merge needs a destination in the
  [coverage index](#discussion-and-legacy-work-index); an archive alone is not
  a disposition for open work. Never silently shrink scope to fit a short row.
- If implementation needs an undecided semantic choice, park that task and name
  the issue; continue independent work. This reorganisation does not approve new
  language semantics, a deployment, a target switch, or public launch.

## Epics and order

| Epic | Outcome | Legacy scope | Scheduling |
|---|---|---|---|
| [E01](#e01--complete-the-golden-todo) | Golden todo runs against its unchanged behavioural contract | P11, AUTH, POLICY | Main product priority |
| [E02](#e02--close-language-and-tooling-gaps) | Required language boundaries and tooling are specified and verified | P4/NAME-002, P10.6, DX1/DX2 | Conformance and E01 prerequisites |
| [E03](#e03--add-reviewed-services-and-durable-jobs) | Reminder job and one reviewed external service work safely | SERVICE-001, ASYNC-001, TEST-001 | Decisions first, then E01 integration |
| [E04](#e04--qualify-transactions-and-deployment-readiness) | Retry, dependency health and selected deployment behaviour are evidenced | P10.7, TX-001, CONFIG-001, CONSISTENCY-001 | Core evidence now; adapters conditional |
| [E05](#e05--complete-engineering-validation) | Cross-feature and hostile testing covers the completed implementation | VAL-001 | Incremental alongside E01–E04 |
| [E06](#e06--make-policy-approval-enforceable-and-reviewable) | Independent approval binds to understandable behavioural changes | P10R.4, POLICY-P5 | Local preparation now; protected system required |
| [E07](#e07--complete-independent-assurance-and-user-evidence) | Frozen contracts and external studies support or reject the claims | P10R, DX2 | Sessions after technical implementation |
| [E08](#e08--test-the-thesis-with-orderpayment-and-typescript) | Fair comparison and explicit continuation decision | P12, WORKFLOW-001, LAYOUT-001 | Reference work may proceed; final trials gated |
| [E09](#e09--qualify-alternative-runtime-candidates) | Bounded backend/trust/performance evidence with a recorded disposition | WASM-EXP1 follow-ups | Separate experiment track; Bun remains default |
| [E10](#e10--research-the-public-documentation-site) | Evidence-backed documentation structure and tested prototypes | Website workstream | Conditional on E08 and owner authorisation |
| [E11](#e11--developer-console-and-mcp-runtime-inspection) | Developers and their LLMs inspect performance, errors, scheduled jobs and changes using shared runtime evidence | DX2, RM-211, RM-308, RM-602 extensions | Proposed follow-up; explicit selection activates planning, outside E01 |
| [E12](#e12--explore-application-contracts-and-frontend-integration) | Assess deep client contracts and optional web rendering through a concrete React example | Frontend/client-contract exploration | Deferred to the end of the roadmap; needs planning and later explicit selection |

## Delivery order — golden Todo first

This is the recommended execution queue for the next bounded batches. It does
not change task dependencies, activate conditional work, or close a task without
its acceptance evidence. Independent lanes may overlap only with separate
ownership/workspaces; otherwise keep one writer per checkout.

| Queue / owner | Next observable checkpoint | Unlocks / gate |
|---|---|---|
| 1A · compiler/runtime writer | **RM-306** — advance from the reviewed native scheduler assembly to bounded recovery/continuation checkpoints, retaining commit-before-provider boundaries on both database adapters. | The integrated native assembly passes 65/65 supported checks with independent correction review; full RM-306 acceptance remains open. Public activation requires the unresolved execution/lease profile and runtime gates. |
| 1B · independent harness writer | **RM-109 preparation** — use the reviewed 44-ID result protocol to prepare real application adapters in a separate worktree. | Preparation can overlap RM-306; full acceptance execution and task closure still require RM-108, RM-402, RM-403 and the other listed prerequisites. |
| 2 · compiler/runtime writer | **RM-307** — connect scheduler and worker recovery, then execute the 18 frozen duplicate/crash/retry/exhaustion traces with controlled clocks and separate worker processes. | Depends on RM-306 and the completed RM-304; enables RM-108 and RM-505. Contract-only traces do not satisfy runtime acceptance. |
| 3 · compiler/runtime writer | **RM-402 / RM-403** — finish transaction hard-bound/replay qualification and readiness hard-bound/platform evidence before final golden acceptance. | Scoped evidence exists; remaining acceptance and independent review stay open. These changes share runtime files and are integrated sequentially. |
| 4 · application integration owner | **RM-108**, then **RM-109 execution** — compose the checked reminder worker and run the prepared SQLite/PostgreSQL harness, retaining every original case result. | Reminder activation needs the owner-selected execution/lease profile and RM-307; RM-109 closure also needs RM-402/RM-403. |
| 5 · integration owner | **RM-504, RM-505 and RM-603**, then **RM-110** — finish residual database/hostile/attestation evidence and run the final exact-source technical golden gate. | RM-603 requires RM-601's canonical artifact; technical completion leaves external assurance separate. |

**Satisfied prerequisites:** [RM-303 provider adapter](implementation-history.md#2026-10-03--rm-303-checked-provider-adapter),
[RM-304 service fakes](implementation-history.md#2026-10-04--rm-304-checked-authored-service-fakes)
and [RM-305 durable contract](implementation-history.md#2026-10-04--rm-305-durable-delivery-semantic-contract)
resolve through history. RM-106 is also complete. They remain dependency facts,
not scheduled remaining work.

The remaining critical path is **RM-306 → RM-307 → RM-108 → RM-109 → RM-110**,
with RM-402/RM-403/RM-504/RM-505/RM-601/RM-603 supplying required gates.
The [current delivery window](work-plans/roadmap-assessment.md#current-parallel-delivery-window--2026-10-06)
defines isolated ownership; the [owning golden plan](work-plans/golden-delivery-planning.md#rm-301307108--services-durable-jobs-and-reminders)
records the implementation checkpoints and pending decision. One writer owns
shared compiler/runtime files; the independent harness writer owns only test and
result-protocol preparation. The integration owner merges changes and updates
shared roadmap, timing and evidence records after combined validation.

RM-201, RM-210, RM-503 and RM-601 remain valid independent work, but should not
displace this queue while the golden Todo milestone is the selected priority.
Conditional work, E10 and E11 remain parked until their stated triggers or
explicit selection.

There is no meaningful single calendar total while external gates and conditional
scope remain unresolved; do not sum all epics into a release commitment.

## External gates

These are prerequisites, not implementation tasks. Effort to prepare or exercise
each gate is estimated in its dependent tasks; waiting time is unknown.

| ID | Required input / evidence | Who supplies it | Next stage / model routing |
|---|---|---|---|
| X-REVIEW | Independent contract reviewer and at least five matching first users; independent trial adjudication | Project owner / external participants |
| X-CI | Selected protected CI/review system, authenticated reviewer authority and protected configuration | Project owner / repository administrator |
| X-PLATFORM | Selected hosting platform and a disposable deployment test environment | Project owner / platform operator |
| X-HOST | Permitted controlled HTTP/SQLite measurement environment with independent load-generator headroom; hosted access for hosted claims | Experiment owner / host operator |
| X-SITE | Public-site authorisation after a continue, redesign or framework-pivot decision; a stop result does not unlock this gate | Project owner |

## E01 — Complete the golden todo

**Done when:** all 44 frozen cases execute against the complete application,
with source, runtime, audit and evidence agreeing. This completes technical
integration; E07 still gates Milestone C and assurance claims.
Sources: [migration checkpoint](../examples/golden-todo-migration/README.md),
[golden contract](../examples/golden-todo/README.md),
[obligation map](../tests/validation/golden-obligations.json),
[authentication plan](authentication-plan.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-108 | Compose the reminder job and reviewed mail service with golden queries/actions; implement deterministic clock, provider failure and duplicate-delivery cases. | 1–4h (L) | RM-105, RM-106, RM-107, RM-304, RM-307 | In progress | Owner accepted narrow reminder-service authority, 3 scheduled invocations / 1 hour and exact self-disable field repair; provider keeps 3 attempts / 30 seconds. Two approved update-only grants restore self-disable; fresh full gate passes 61/61 and scoped correction is independently approved. [Plan](work-plans/golden-delivery-planning.md#rm-301307108--services-durable-jobs-and-reminders). Checked worker/clock/crash/duplicate integration and the pending golden-only 60s execution / 40s renewable lease decision remain. Confirmed Sol/high pin. |
| RM-109 | Complete route/OpenAPI/policy/query audit agreement and SQLite/PostgreSQL golden harness coverage; run every case and retain failures by original case ID. | 1–4h (L) | RM-103, RM-104, RM-105, RM-106, RM-107, RM-108, RM-402, RM-403 | In progress | Independent lane: source-bound case-result protocol and 44-ID inventory are prepared/reviewed; implement real adapter observations next, and run full acceptance only after all listed prerequisites, including RM-403 CONFIG-003 readiness evidence. [Current window](work-plans/roadmap-assessment.md#current-parallel-delivery-window--2026-10-06); actual bounded delivery pin gpt-6.1-sol/high, reassess a future batch. |
| RM-110 | Close the technical golden gate using a fresh verifier report, full behavioural results, named proof/validation evidence and adversarial cases; preserve frozen versus repaired-toolchain results. | 0.5–2h (M) | RM-109, RM-504, RM-505, RM-603 | Queued | Run final exact-source require-golden gate and audit every case, proof and review. Preferred gpt-6-sol high. |

## E02 — Close language and tooling gaps

**Done when:** required conformance gaps have executable cases and public
contracts agree across compiler, runtime and tooling. Conditional extensions
remain out of scope unless application evidence requires them.
Sources: [language issues](language-issues.md), [formatter rules](formatter-rules.md),
[developer tooling](developer-tooling.md), [failure model](failure-model.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-201 | Close NAME-002 with a grammar-derived identifier/reserved-word conformance matrix and table-driven lexer/parser/semantic unit tests plus positive/negative compile fixtures. Inventory every identifier-bearing production and test type and value names in every declaration and reference position, including bindings, parameters, fields, modules/imports, callables, operations, variants, and configuration/auth/job declarations. Specify and test exact case sensitivity; lexical characters (`[A-Za-z_][A-Za-z0-9_]*`), ASCII/Unicode boundaries and token boundaries; leading-digit rejection; standalone/leading underscores; and the semantic `UpperCamelCase` type versus `lower_snake_case` runtime rules, including accepted digits and rejected underscore forms. For every keyword, test variable-name and type-name positions, references to any permitted contextual name, and stable CLI/LSP diagnostic spans. Current regressions distinguish `UserName` from `Username` and check type/runtime naming shapes. A parser probe found `return` is accepted as a binding name but cannot be read as an expression; classify it and similar combinations under the owner-selected reserved-word policy. The owner selected reserved control/declaration words with explicitly safe contextual names; derive the exact matrix and review the authoritative contract before implementation. | 1–4h (L) | —; owner policy selected; public-language review remains | In progress | inventory and policy conformance implementation Preferred gpt-6-sol high. |
| RM-202 | Resolve FMT-005's source-line preservation versus line-break-insensitive canonical-output requirement; record the owner decision and update the acceptance contract consistently. | 0.25–0.75h (S) | — | Decision | Audit the recorded owner decision and consistent acceptance wording, then hand off RM-203. Preferred gpt-6-luna xhigh. |
| RM-203 | Finish TOOL-005 to the chosen contract: implement AST-guided line reconstruction if selected and exact-output cases; retain token, string, comment, parseability and idempotence checks. | 1–4h (L) | RM-202 | Queued | implementation after RM-202 reconciliation Preferred gpt-6-sol high. |
| RM-207 | Finalise operational mappings, especially unknown write outcomes and retry safety; implement missing read/write/fallback cases with route/OpenAPI/audit parity and contained defects. | 1–4h (L) | — | In progress | [Shared boundary matrix](../tests/assurance/operational-boundary-v0.1.json) reviewed; checked read unavailability and committed-write uncertainty pass both database adapters with route/OpenAPI/audit parity. The reference HTTP adapter also tests accepted-provider/lost-ack mapping to one safe `OutcomeUnknown` response without blind retry. Deadline-aware timeout, durable-job persistence/restart/reconciliation and independent runtime review remain. Preferred gpt-6-astra high for full task. |
| RM-208 | Decide Set/Map wire semantics and complete boundary round-trip/rejection cases; align generated schemas and runtime representation. | 1–4h (L) | — | Decision | prepare wire-contract options and executable boundary table Preferred gpt-6-sol high. |
| RM-209 | Record the compatibility/deprecation policy for legacy type/persist and current entity syntax; align public examples and migration diagnostics while preserving frozen pressure sources. | 0.5–2h (M) | RM-101 | Decision | prepare compatibility options and migration inventory Preferred gpt-6-sol high. |
| RM-210 | Exercise all four snippet adapters in actual renderer hosts, including Highlight.js/Monaco, and verify visual output and exact plain-text fallback. Registration tests alone do not close TOOL-003. | 0.5–2h (M) | — | Ready | actual host qualification Preferred gpt-6-luna xhigh. |
| RM-211 | Complete any remaining runtime-event enrichment/exporter coverage with browser/log/trace/provider secret canaries and revision-matched agent packets; retain CLI/JSON/LSP repair parity. | 0.5–2h (M) | RM-501 | Queued | inventory remaining sinks then focused canary checks Preferred gpt-6-sol high. |
| RM-212 | If application evidence requires it, decide compound persistence, composite-reference spelling and relationship extensions (DATA-001/DATA-004) separately. Preserve explicit join entities, per-hop bounds/order, required inverse-one totality, policy-safe traversal and fixed query budgets; include failure-context binding only under RM-215's contract. Add fixtures and separately estimate approved implementation. | 0.25–0.75h (S) per decision | RM-101 | Conditional | wait for named application pressure, then scope decision Preferred gpt-6-sol high. |

### Recovered scope requiring planning

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-214 | Resolve SYN-001's durable documentation, intent/rule/decision annotations and provenance: distinguish human-owned intent from derived docs/audit, preserve rationale and rejected alternatives, and specify links to compiler facts. Finish with an approved contract or explicit deferral plus fixtures; no natural-language annotation may become a proof by itself. | 0.5–2h (M) | — | Decision | prepare minimal documentation/provenance options Preferred gpt-6-sol high. |
| RM-215 | Reconcile TYPE-001 and FAIL-001–004 against implemented validated construction, exact `fails`, `attempt` propagation and exhaustive outcome `match`. Record only the remaining decisions: optional failure-context binding/cause disclosure, compatible public-code aliases, catalogue/422 pressure, localisation ownership and explicit panic. Preserve the implemented model; each approved extension needs fixtures and a separately scoped implementation. | 1–4h (L) | — | Decision | reconcile stale spec statements and prepare residual decisions Preferred gpt-6-sol high. |
| RM-216 | Pressure-test the remaining ordinary-logic/type questions: `for` iteration and collection operations, nested/property/type matching, lambdas, recursion/`while`, generics/user collections, transparent aliases, raw primitive signatures and reviewed representation escapes. Activate when an application demonstrates a gap or the owner selects a language review; document support, rejection or deferral with positive/negative examples and bounded-resource implications. Existing enum/match/value semantics remain the baseline. | 1–4h (L) | — | Conditional | wait for selected pressure, then bounded design probe Preferred gpt-6-astra high. |
| RM-217 | Decide HTTP **disconnect cancellation**, file/upload and streaming failure contracts, including what can be reported after headers/data are sent and how in-flight effects/transactions finish. Separate ordinary cancellation from structured parallelism and durable work. Produce adapter/fixture requirements before implementation; activate on application need or explicit owner selection. | 1–4h (L) | — | Conditional | wait for activation then adapter/contract decision package Preferred gpt-6-astra high. |
| RM-218 | Specify and audit the previously stated **safe resource defaults**: rate limits, request/body/collection bounds, timeouts, execution/termination budgets and cancellation. Inventory existing enforcement and gaps, define deterministic failure/disclosure behaviour and cross-target adapter limits, and split missing implementations into measured tasks. | 1–4h (L) | — | Decision | bounded enforcement and adapter-limit inventory Preferred gpt-6-sol high. |
| RM-219 | Resolve TOOL-001's general non-schema semantic identity and verify **source maps / app.meta / revision-matched debugging** across rebuilds, renames and bugfix branches. Separate persistent logical operation identity from exact deployment/build and call-site identity; preserve original incident evidence while mapping to changed, removed, split or ambiguous current nodes without claiming behavioural equivalence. Show generated runtime failures mapping to the correct Jadpo operation/source or explicitly identified compiler-owned component; retain private target traces and prove stale metadata is rejected. Include unrelated-node insertion, branch divergence and graph-changing fixes. Schema registry identity is already implemented. | 1–4h (L) | — | Decision | source/revision identity inventory and decision probe; compare registry-backed identity with content/name hashes and graph ordinals, then test historical versus current-source mappings. Preferred gpt-6-sol high. |
| RM-220 | Retain future time, configuration and test extensions: recurrence/holidays/business calendars/natural-language time/custom friendly profiles; non-env config sources/live reload/secret-manager adapters; and authored property tests. Activate only on demonstrated application need or explicit selection; assess each independently, preserve deterministic clocks and existing CONFIG/TIME/TEST contracts, and record a decision plus separately scoped implementation. | Unestimated (needs planning) | — | Conditional | wait for named extension and activation Preferred gpt-6-sol high. |
| RM-221 | Measure and improve compiler-generated Bun HTTP path lookup so routing remains efficient as route tables grow. Compare the current per-route segment split and linear scan with plausible indexed, trie/radix and precompiled-regex approaches on representative static/parameter routes and method mixes; choose the fastest correct design for the selected workload, or retain the baseline if gains are not meaningful. Preserve existing precedence, parameter decoding and malformed-path behaviour, authentication order and deterministic generated artifacts. Proposed completion: repeatable dispatch and generated-HTTP benchmarks report latency, throughput, allocation/memory and startup/build trade-offs, with semantic parity tests for the selected algorithm. | Unestimated (needs planning) | — | Decision | Define route corpus, primary speed metric, measurement controls and acceptable trade-offs before benchmarking; relate to RM-104/RM-204 without treating them as prerequisites. Recommend gpt-6-sol medium for experiment design; reassess implementation routing after selection. |
| RM-222 | Review syntax **language-wide before the new component/event/graph design**: inventory stacked prefixes and whitespace-separated operation forms across failures, calls, queries, mutations, messaging and routes; distinguish propagation from submission, completion and retry. Retain the selected UpperCamelCase/lower_snake_case conventions and declaration family; review existing `attempt` with typed `emit_event(...)`, clearer query forms and the unnamed singleton principal successor. Preserve exhaustive handling and exact failure contracts. Produce reviewed grammar, positive/negative examples and a separately scoped migration/tooling plan. | 1–4h (L) | — | Decision | [Assessed decision plan](work-plans/roadmap-assessment.md#rm-222--language-wide-syntax-decision-and-migration-brief); owner selected naming, singleton principal and one `query(Todo)` operation with cardinality in its block; [syntax inventory](syntax.md#22-successor-syntax-review--2026-10-04) and grammar candidate are saved, with compatibility/public-language review pending. Finish before RM-309/RM-1108 adoption. Planning batch actual gpt-6-astra medium; recheck a future batch. RM-201/RM-215 contracts remain intact. |
| RM-223 | Implement RM-222's approved **canonical syntax migration**, including affected parser/AST/failure checking, formatter, LSP/highlighters, diagnostics/docs and supported conversion fixtures. Equivalent programs preserve exact failure, policy, query and transaction behaviour. Keep frozen historical sources/results intact and prevent two competing recommended spellings. | Unestimated (needs planning) | RM-222, RM-110 | Conditional | Successor after existing technical golden gate and syntax freeze; [conditional technical plan](work-plans/roadmap-assessment.md#assessed-successor-implementation-plans--2026-10-04). Revalidate production inventory and estimate against reviewed grammar. This chat may implement after the other session finishes and the gates pass; its active scope is unchanged. |


## E03 — Add reviewed services and durable jobs

**Done when:** the golden reminder uses a reviewed outbound service and a
durable job with bounded retries, explicit failure outcomes and deterministic
crash/replay evidence. Do not select provider or queue semantics implicitly.
Sources: [decision packages D3–D4](decision-sprint.md#d3--service-001-and-the-service-side-of-async-001),
[time/testing contract](time-testing-plan.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-306 | Implement compiler-owned job/event declarations, transactional outbox and durable delivery state; prove atomic enqueue and restart-safe identity/version tracking. | 2–8h (N) | RM-305 | In progress | Reviewed paging, CI-I01 completion and ACT-STORAGE-TIME-01 storage foundation pass 65/65. Next: wire the saved scheduler draft/bridge/clock helper and verify actual generated assembly; the unwired draft has no execution evidence. [Owning checkpoint](work-plans/golden-delivery-planning.md#rm-301307108--services-durable-jobs-and-reminders). Resumed by the owner in this bounded parallel-delivery batch, actual gpt-6.1-sol/high. Profile decision, public activation and 18 generated traces remain open. |
| RM-307 | Implement scheduling and worker execution with bounded selection, retries, cancellation and replay; pass duplicate/crash/exhaustion tests with controlled clocks. | 2–8h (N) | RM-306, RM-304 | Queued | After RM-306 assembly, prove scheduler/worker recovery with all 18 frozen traces, separate worker processes and controlled clocks; reviewed storage and contract-only traces are insufficient. [Current window](work-plans/roadmap-assessment.md#current-parallel-delivery-window--2026-10-06); retain the bounded delivery pin gpt-6.1-sol/high, reassess a future batch. |
| RM-308 | Add inspect/retry/dead-letter operator surfaces and evidence exports; verify safe diagnostics and bounded recovery from the recorded terminal states. | 1–4h (L) | RM-307 | Queued | After durable states settle, define operator capability/state-transition matrix and bounded safe export before implementation. Preferred gpt-6-sol high. |
| RM-309 | Design an enforced **component messaging and subscription contract**, using RM-222's reviewed syntax: forbid direct cross-component behaviour calls while allowing local actions across files; define one typed event API and approved read contracts, a generated catalogue and component subgraphs, mandatory transition-owned event production, multiple typed subscriptions, per-subscriber durable delivery and optional execution configuration with safe defaults. Enforce effect ownership transitively, non-callable subscriber entry points and pure shared helpers; include import/wrapper/relocation bypass tests, including subscriber-only effects within one component. Keep concepts and authored wiring minimal. Resolve payload sufficiency/privacy, async policy authority, local atomicity, ordering, backpressure, joins, completion, cancellation, compensation, uncertainty, versions and replay. Assess stateful subscribers as the single authored coordination model rather than assuming a second workflow DSL. Completion: complete todo/representative order examples and fault/negative cases, reviewed adopt/adapt/defer decision, and separately scoped implementation/migration. | 2–8h (N) | RM-222 | Decision | [Candidate event contract](event-model.md) and [assessed plan/worked traces](work-plans/roadmap-assessment.md#rm-309--one-enforced-interaction-model). Reconcile DATA/TX/ASYNC/SERVICE/WORKFLOW in their owners before freeze; [independent candidate review and correction recheck](../tests/validation/event-design-independent-review.json) completed; final language/policy/effect qualification remains pending. Preserve frozen ASYNC-001 and active RM-306/RM-307. Planning actual gpt-6-astra medium; forecast provisional, excludes implementation/review waiting. |
| RM-310 | Implement the reviewed **component/effect and automatic-publication compiler slice**: checked logical owners, typed catalogue/multi-event handlers, non-callable subscribers, transitive effect admission and mandatory transition facts on every supported mutation path. CompleteTodo with two receivers is the first slice; prove allowed same-owner cross-file helpers and reject direct/import/wrapper/relocation/same-component reaction bypasses. | Unestimated (needs planning) | RM-309, RM-223, RM-219 | Conditional | After contract/identity freeze and existing golden gate, revalidate the [saved compiler slice plan](work-plans/roadmap-assessment.md#assessed-successor-implementation-plans--2026-10-04) against current IR/lowering. No second action-call boundary pattern; estimate after exact ownership/payload rules settle. |
| RM-311 | Extend the existing durable runtime for **independent subscriber delivery**: commit-time enrollment/version identity, per-subscriber obligations, same-domain processed marker/mutations/output facts, safe default execution and scoped ordering. Prove crash/duplicate/fence/exhaustion/capacity/upgrade recovery on SQLite/PostgreSQL; completed peers do not replay because another subscriber fails. Preserve uncertainty and reviewed external-effect rules. | Unestimated (needs planning) | RM-310, RM-307 | Conditional | [Conditional runtime plan](work-plans/roadmap-assessment.md#assessed-successor-implementation-plans--2026-10-04); start with two local receivers then one reviewed external effect. Reuse ASYNC-001 engine and policy; revalidate storage/worker changes after accepted compiler contract. No broker prerequisite or new retry engine. |
| RM-312 | Qualify a separately versioned **golden Todo successor using enforced subscribers**: automatic transition facts, independent reactions, typed defaults, current policy, operator diagnostics and full supported behavioural gate. Preserve original frozen baseline and compare exact-source results; prove missing wiring/bypass cannot compile and rejected/no-op/rollback paths do not publish success facts. | Unestimated (needs planning) | RM-311, RM-308 | Conditional | [Conditional integration plan](work-plans/roadmap-assessment.md#assessed-successor-implementation-plans--2026-10-04); revalidate after compiler/runtime evidence. Original 29-task delivery remains unchanged. Order joins/compensation continue under RM-802–RM-804 rather than a duplicate reference app. |

## E04 — Qualify transactions and deployment readiness

**Done when:** required local transactions and dependency readiness have real
failure/recovery evidence. Physical derived stores and platform-specific hooks
activate only when a selected application/platform requires them.
Sources: [entity/transaction model](entity-query-model.md),
[configuration plan](configuration-plan.md), [D6 adapter decision](decision-sprint.md#d6--physical-consistency-001-adapters-and-readiness).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-402 | Implement the accepted retry plan and broaden SQLite/PostgreSQL multi-process contention, nested rollback and failure-injection evidence; retain isolated clocks and contiguous change revisions. | 1–4h (L) | RM-401, RM-207 boundary-contract milestone | In progress | Reviewed scoped deadline/backoff corrections, native SQLite LOCKED, deterministic jitter and contiguous revision checks pass; approved disposable verification cleared historical host denials. See [current checkpoint and limits](transaction-retry-plan.md#rm-402-reviewed-adapter-checkpoint--2026-10-04). Whole-request hard bounds and broader replay eligibility remain unproved; no whole-task closure. Confirmed batch pin gpt-6-sol/high. |
| RM-403 | Add required/advisory live dependency checks with bounded deadlines, safe status IDs, recovery and liveness independence; keep startup validation before listener binding. | 1–4h (L) | RM-303 | In progress | Scoped recovery/startup-lock, PostgreSQL auth-session, real connection-loss/32-way recovery and asynchronous response-deadline evidence independently approved. Six deterministic groups, ten independent emitted-module probes and live readiness14/14 pass. [CONFIG-P5 evidence and remaining hard-bound/platform gates](configuration-plan.md#config-p5--liveness-and-readiness) own current status. Mail advisory/unprobed; owner-selected6.1-sol/high. |
| RM-404 | Implement the selected platform's promotion/readiness hooks and test failed revision non-promotion or controller rollback while the prior revision stays healthy. | 2–8h (N) | RM-403, X-PLATFORM | External | When deployment is selected, map actual hooks and rollback limits, disposable fixture and cleanup to prior-revision acceptance. Preferred gpt-6-sol high. |
| RM-405 | If an application requires a physical derived store, approve CONSISTENCY-001 adapter/freshness/recovery semantics and name that store; otherwise record fail-closed deferral. | 0.5–2h (M) | RM-801 | Conditional | Use RM-801 concrete query pressure to compare authority-only vs one named derived adapter, produce capability/freshness/recovery matrix or explicit fail-closed deferral. Preferred gpt-6-sol high. |
| RM-406 | Implement the selected derived-store adapter: ordered idempotent outbox delivery, watermarks, lag and explicit freshness/fallback behaviour. | 2–8h (N) | RM-405, RM-306 | Conditional | After RM-405, produce one concrete adapter protocol with checkpoints, idempotency identity, watermark/fallback and conformance fixtures. Preferred gpt-6-sol high. |
| RM-407 | Prove derived-store crash recovery, duplicate delivery, reconciliation and generation-based rebuild/catch-up/switch; reject unsupported cross-domain atomic guarantees. | 1–4h (L) | RM-406 | Conditional | After adapter selection, enumerate crash points and reconciliation invariants; specify old/new generation retention and safe switch. Preferred gpt-6-sol high. |

### Recovered scope requiring planning

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-408 | Extend **schema/data migrations** beyond the bounded DATA-003 review/SQL core when an application requires it: transforms/backfills, removals, constraints on added fields, lifecycle changes, nominal refinements and enum evolution. Define exact-change-set approval, existing-data validation, forward/rollback/irreversible behaviour and crash evidence for each selected adapter; retain fail-closed unsupported cases. | Unestimated (needs planning) | RM-801 | Conditional | At RM-801 pressure select one unsupported change, classify identity/data/rollback risks, design reviewed two-adapter fixtures and estimate implementation separately. Preferred gpt-6-sol high. |
| RM-409 | Resolve DATA-005's portable **foreign-key/check/exclusion constraint identities** and precise failure mappings. Inventory supported SQLite/PostgreSQL evidence and define compiler-owned identities without parsing ambiguous driver text; add an approved contract and positive/negative adapter cases before extending mappings. | 1–4h (L) | — | Decision | Inventory exact driver fields using paired constraint collisions; propose compiler-owned identities and precise/unknown outcomes, reviewed fixtures before mappings. Preferred gpt-6-sol high. |
| RM-410 | Evaluate **advanced queries and workload-aware index advice** when application/workload evidence requires it: aggregation/complex joins, richer cross-entity planning, patch predicates, adapter EXPLAIN/statistics, selectivity/write cost and compound/partial indexes. Keep static warnings and explicit index acceptance as implemented foundations; never infer production index value from syntax alone. Produce evidence-backed decisions and separately scoped implementations. | Unestimated (needs planning) | — | Conditional | When pressure exists, freeze bounded workload/query shapes, compare EXPLAIN/read/write costs, decide adopt/defer and scope missing semantics separately. Preferred gpt-6-sol high. |

## E05 — Complete engineering validation

**Done when:** the supported implementation has independent, cross-feature and
mutation evidence, and every remaining limitation is explicit. A green local
run does not replace E07's external assurance gates.
Source: [validation entry point and work packages](../tests/validation/README.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-503 | Expand targeted implementation-mutation campaigns from the gap inventory; record killed/surviving mutations and repair missing assertions rather than changing contracts. Five selected mutants across semantic naming, keyword token boundaries, and generated Unicode length are killed; broaden the review across additional obligation areas. See [mutation findings](../tests/validation/rm503-mutation-findings.md). | 1–4h (L) | RM-501 | In progress | prepare bounded next campaign with verify-loop checks Preferred gpt-6-sol high. |
| RM-504 | Close remaining PostgreSQL-specific auth/policy/query/time evidence, including exact JWT provenance and timezone-rule provenance where claimed; record SQLite-only instrumentation separately. | 1–4h (L) | RM-402 | Queued | residual PostgreSQL matrix then run missing cases Preferred gpt-6-luna xhigh. |
| RM-505 | Add cross-feature hostile cases for lifecycle, auth revocation, policy scoping, services/jobs, transactions and secret boundaries; tie each result to a named obligation. | 1–4h (L) | RM-106, RM-304, RM-307, RM-402, RM-501 | Queued | case design then prerequisite-bound integration tests Preferred gpt-6-sol high. |
| RM-506 | Register new suites/examples and verify artifact, diagnostic, compatibility and formatter contracts in the unified local gate; archive a fresh report with all open gates named. | 0.5–2h (M) | RM-110, RM-201, RM-203, RM-208, RM-209, RM-210, RM-211, RM-502, RM-503, RM-504, RM-505, RM-308, RM-403, RM-508 | Queued | integrate completed suites and final required report Preferred gpt-6-luna xhigh. |
| RM-507 | Execute the same required gate in hosted CI and establish protected required checks; retain hosted reports and verify bypass/failure handling. Local workflow files alone are not evidence. | 0.5–2h (M) | RM-506, X-CI | External | prepare provider-specific hosted qualification after local gate Preferred gpt-6-sol high. |

### Recovered scope requiring planning

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-508 | Reconcile the request for **every compiler diagnostic and language edge case** with actual executed assertions: inventory current public codes and grammar/boundary obligations, map each to real trigger or explicit operational-emitter tests and all human/JSON/LSP/IDE projections, identify unexercised branches, and add missing targeted cases. Catalogue membership, source references and mutation samples alone are not exhaustive coverage; preserve contextual names, root-cause grouping, accurate repair labels, stale-edit rejection and secret canaries. | 2–8h (N) | RM-501 | Ready | build executed-assertion coverage matrix and prioritize gaps Preferred gpt-6-sol high. |

## E06 — Make policy approval enforceable and reviewable

**Done when:** an implementation agent cannot self-approve policy weakening,
and the review surface exposes the behaviour and uncertainty behind each
human decision. E07 measures whether people understand it.
Sources: [approval protocol](approval-protocol.md),
[semantic graph review requirements](implementation-history.md#backlog--focused-semantic-change-graphs-in-the-review-ui),
[comprehension study](comprehension-study.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-601 | Produce versioned before/after behavioural artifacts with request/intent, actors, data/effects, proof/evidence classification and exact subject digests; provide a usable non-graphical export. | 1–4h (L) | — | In progress | Scoped job, static role/member, principal, actor-route, field/output and corrected generated-byte provenance reviews approved. Field48/48+4 internal; byte pins7 public+5 helper and fresh supported61/61. [Canonical audit, checkpoint and limits](work-plans/golden-delivery-planning.md#rm-601603--review-artifacts-and-local-attestation-validation) retain full field-flow, worker/emission, feasible counterexample/provenance and whole artifact-review gates. Owner selected6.1-sol/high. |
| RM-602 | Implement the **Revset-inspired focused semantic change graph / web review UI** described [below](#revset-and-the-web-review-ui): before/after actor → route → action/query → field/output paths, newly reachable unchanged declarations, successive agent revisions, scenarios, source/evidence drill-down and narrow approval choices. Include changed behaviour with unchanged graph structure and an equivalent non-graphical export; unchanged review content never implies valid approval. | 2–8h (N) | RM-601 | Queued | artifact-bound prototype plan after RM-601 schema Preferred gpt-6-sol high. |
| RM-603 | Implement and locally test attestation validation bound to policy/graph digest, reviewer authority, versions and expiry; reject missing, stale, replayed, forged and scope-mismatched approvals. | 1–4h (L) | RM-601 | Queued | verifier-boundary implementation after canonical subject Preferred gpt-6-sol high. |
| RM-604 | Wire validation to the protected CI/review authority; prove source or ordinary repository edits cannot manufacture release approval and test the full policy-weakening attack fixtures. | 1–4h (L) | RM-603, X-CI | External | select authorised protected CI provider and qualify attack matrix Preferred gpt-6-astra high. |
| RM-605 | Prepare counterbalanced PR-diff, concise behavioural and relationship-aware review tasks with transitive-risk and unchanged-graph cases; freeze scoring and capture comprehension, time and false confidence. | 0.5–2h (M) | RM-602 | Queued | assemble/freeze matched review-task bundles Preferred gpt-6-sol high. |

## E07 — Complete independent assurance and user evidence

**Done when:** P10R's seven packages are independently reviewed/frozen, five
first-user sessions and formal DX2/comprehension evidence are recorded, and the
Milestone C assurance decision is explicit. The owner's 2026-09-25 deferral still
applies: technical/reference implementation may continue as exploratory work;
external sessions follow technical completion and precede final P12 trials.
Sources: [P10R package requirements](implementation-history.md#p10r--assurance-and-validation-reset),
[review guide](first-user-review-guide.md), [comparison protocol](comparison-protocol.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-701 | Reconcile all seven P10R packages, claim-to-proof/threat mappings and current documentation authority; preserve original and revised digests plus implementation exposure. | 0.5–2h (M) | RM-506, RM-605 | Queued | Build seven-package manifest; map each claim to current rule/threat/runtime evidence and authority; flag unresolved claims and implementation exposure, then validate links/digests. Preferred gpt-6-sol high. |
| RM-702 | Obtain independent contract review, resolve findings and record reviewed versions of golden, proof, threat, approval, user-hypothesis, comparison and documentation packages. | Unmeasured (U) | RM-701, RM-805, RM-806, RM-807, X-REVIEW | External | After RM-701 prepare reviewed-version manifest, conflict questions and reviewer independence criteria; arrange external review only with named reviewer access. Preferred gpt-6-sol high. |
| RM-703 | Conduct at least five matching first-user sessions after technical completion; retain raw observations, adoption objections and synthesis without retrofitting the candidate hypothesis. | Unmeasured (U) | RM-702, X-REVIEW | External | Freeze revised artifact digest after review; schedule qualifying participants, retain pseudonymous raw notes and separate statements from interpretation; synthesize hypothesis disposition. Preferred gpt-6-sol medium. |
| RM-704 | Run formal fresh-agent and first-user repair/comprehension trials with frozen tasks, including all three review surfaces; report accuracy, repair cycles, risk detection and false confidence. | Unmeasured (U) | RM-702, RM-605, X-REVIEW | External | Bind reviewed surfaces and diagnostic repair tasks to clean checkpoints; freeze expected outcomes, participant assignments and scoring, then execute fresh-agent/human trials independently. Preferred gpt-6-sol high. |
| RM-705 | Freeze the reviewed comparison protocol and tooling/context baseline; preserve any threshold revisions, adjudication rubrics and prior implementation exposure before final trials. | 0.5–2h (M) | RM-702, RM-703, RM-704 | Queued | Resolve one permitted threshold review; digest prompts, artifacts, dependencies, context, rubrics and records; record prior exposure and contamination controls. Preferred gpt-6-sol high. |
| RM-706 | Record the P10R/Milestone C disposition against every exit condition, including protected attestation, hosted checks and applicable deployment evidence; unresolved critical claims remain blocked. | 0.5–2h (M) | RM-705, RM-604, RM-507 | Queued | After prerequisites map every original exit to evidence or explicit scope disposition, including protected attestation and hosted deployment; retain unmet critical claims as blocked. Preferred gpt-6-sol high. |

RM-404 is additionally required by RM-706 if a platform deployment is included in
the assurance claim. Omitting it must narrow the claim explicitly; it cannot be
reported as passed. Conditional derived-store work is treated the same way.

## E08 — Test the thesis with order/payment and TypeScript

**Done when:** equivalent applications complete the frozen trials, raw evidence
and independent adjudication are retained, and every preregistered threshold
has a pass/fail/mixed result with a continue/redesign/framework-pivot/stop decision.
Sources: [P12 requirements](implementation-history.md#p12--orderpayment-application-and-typescript-baseline),
[comparison protocol](comparison-protocol.md), [D5 workflow decision](decision-sprint.md#d5--orderpayment-pressure-and-workflow-001).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-801 | Write the order/payment pressure dossier and shared acceptance cases: **money/currency/Decimal precision, rounding and overflow**, inventory, declines, webhooks, duplicate/late effects, refunds, migrations, role collisions and pricing. Evaluate payload enums across typing, generated clients, wire/storage compatibility and migrations; record decisions and separately scope any missing language implementation before reference-app completion. | 1–4h (L) | — | Ready | Prepare pressure dossier and shared acceptance matrix with exact money examples, payment state transitions, crash/unknown-outcome traces and payload enum persistence/client evolution cases; route gaps to explicit decisions. Preferred gpt-6-astra high. |
| RM-802 | Approve WORKFLOW-001 from those cases: durable steps, idempotency, deadlines, compensation, cancellation, unknown outcomes, reconciliation and operator intervention. | 1–4h (L) | RM-801, RM-301, RM-305 | Decision | Use accepted pressure cases to specify state machine, before/after-effect checkpoints, versioning, idempotency, deadlines, fallible compensation and bounded operator actions with executable traces. Preferred gpt-6-astra high. |
| RM-803 | Implement persisted workflow state/versioning and checkpoint-before-effect/checkpoint-after-outcome execution; prove restart and duplicate-step recovery. | 2–8h (N) | RM-802, RM-303, RM-306 | Queued | After contract acceptance split one local persisted step plus one idempotent fake external effect; map compiler IR, persistence, worker and restart test ownership before broadening. Preferred gpt-6-sol high. |
| RM-804 | Implement compensation, cancellation, reconciliation and bounded operator recovery; pass crash, late-success, refund-failure and intervention traces. | 2–8h (N) | RM-803 | Queued | Build transition/fault matrix from RM-802 and RM-803; separately scope late success, refund failure, retries, reconciliation and manual resolution with restart evidence. Preferred gpt-6-astra high. |
| RM-805 | Build the Jadpo order/payment reference application against the shared suite; log compiler changes and application adaptation separately and retain original failures. | 2–8h (N) | RM-804, RM-110 | Queued | After dossier/workflow/golden completion map each shared case to source, operation and evidence; plan first vertical order/payment path before whole application. Preferred gpt-6-sol high. |
| RM-806 | Build a credible, behaviourally equivalent TypeScript baseline with the specified framework/schema/persistence/policy/test/agent facilities; pass the same suite. | 2–8h (N) | RM-801 | Queued | Once shared cases exist select/pin strongest equivalent facilities and state/effect architecture; produce parity matrix and implement against same suite with separate adaptation costs. Preferred gpt-6-sol high. |
| RM-807 | Pressure-test project layout on todo, order/payment and multi-domain/application examples; decide broader enforcement/escape rules and whether finite scaffold packs or module extensions are justified. | 1–4h (L) | RM-805 | Decision | Run named layouts through existing pressure matrix; record discovery/refactor/import/exception evidence; choose convention/warning/error/escape per gap and scope accepted implementation separately. Preferred gpt-6-sol high. |
| RM-808 | Run at least three clean counterbalanced trials per stack with identical requirements/context and pinned models/tools; retain per-change time, tokens, defects, interventions, diagnostics and escape-hatch records. | Unmeasured (U) | RM-705, RM-706, RM-805, RM-806, RM-807 | Queued | Verify frozen digests; allocate clean counterbalanced attempts; collect all time/token/failure/friction records and retain original failed-toolchain attempts. Preferred gpt-6-sol high. |
| RM-809 | Independently adjudicate every trial and friction incident; score all frozen thresholds and record the continuation decision. Critical safety failure blocks the assurance claim regardless of productivity results. | Unmeasured (U) | RM-808, X-REVIEW | External | Apply frozen rubric to every result and friction incident; report raw denominators/uncertainty and strong countercase; record continue/redesign/framework-pivot/stop decision. Preferred gpt-6-sol high. |

Reference work in E08 may advance before external review under the existing
owner deferral. RM-702 waits for the planned technical/reference implementations
and layout decision; first-user sessions follow, then the final comparative
trials. New enum, module or scaffold implementations require separately
estimated tasks after the relevant decision; this plan does not assume they
are needed. If approved, add them to the technical-completion dependencies.

## E09 — Qualify alternative runtime candidates

**Done when:** each activated bounded campaign has reproducible conformance,
performance and trust evidence with a disposition. Previous experiments remain
completed even where they failed a parity gate. Bun remains the default;
experimental native/Workers recommendations do not authorise a migration.
Sources: [backend coverage plan](wasm-large-row-plan.md),
[host capabilities](wasm-host-capabilities.md), [trust boundary](wasm-host-trust-decision.md),
[monitor results](capability-monitor-results.md),
[campaign stop conditions](../experiments/capability-monitor/performance-ledger.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-901 | Complete COV-1's source/backend/host capability inventory and rank unsupported vertical slices; distinguish generated lowering from handwritten fixture evidence. | 0.5–2h (M) | — | Ready | Inventory checked language capabilities with Bun/generated-Rust/direct-Wasm/host columns, positive/negative evidence and smallest next slice; rank gaps without running or reopening a campaign. Preferred gpt-6-sol high. |
| RM-902 | Prepare the next controlled host campaign, retaining conformance, independent authority, fresh guests, paired baselines, generator headroom and fixed budgets; explicitly reopen the reached monitor stop condition before new optimisation work. | 0.5–2h (M) | RM-901 | Decision | Use inventory to propose bounded native/workerd attribution with paired baselines, conformance guards, workload matrix, hardware/load generator and fixed budget; explicitly request reopening before execution. Preferred gpt-6-astra high. |
| RM-903 | Run the scoped native/workerd monitor campaign and attribute remaining guest/host/control-message costs and Unicode tails; report latency, CPU, RSS, startup and SQL counts against the frozen 2× monitor gate. | 1–4h (L) | RM-902, X-HOST | External | After RM-902 freeze exact native/workerd baselines, trace guest/host/control costs and Unicode corpus; collect latency/CPU/RSS/startup/SQL with guards and inconclusive criteria. Preferred gpt-6-sol high. |
| RM-904 | Qualify WAL/FULL writes, sustained checkpoints and crash recovery on a controlled host with matched storage plans; separate storage effects from target effects. | 1–4h (L) | RM-902, X-HOST | External | Define bounded sustained-load and kill/restart matrix with identical storage pragmas, checkpoint schedule and acknowledged-state oracle; separate storage and target deltas. Preferred gpt-6-sol high. |
| RM-905 | If authorised for hosted qualification, run actual Workers cold-start and required capability probes, including transaction/freshness/isolation limits; clean up disposable resources and retain evidence. | 1–4h (L) | RM-902, X-HOST, X-PLATFORM | Conditional | After activation select required capability subset and approved host resources; freeze manifest/probes and cleanup plan, then report documented vs host-verified limits separately. Preferred gpt-6-sol high. |
| RM-906 | Use COV-2 to qualify one prioritised **compiler-generated** vertical slice and record adopt/extend/defer/reject plus shared-Rust versus direct-Wasm disposition. Include promotion of fixture-local monitor effect sequences, completion bindings and failure mappings into normal versioned compiler artifacts where required; prove regenerated metadata and host enforcement agree. Handwritten fixture evidence is not backend coverage. Scope larger lowering separately. | 1–4h (L) | RM-901, RM-903, RM-904 | Queued | Select top inventory gap; plan compiler-owned versioned lowering plus positive/negative regenerated conformance, classify shared-Rust vs direct-Wasm and adopt/extend/defer/reject. Preferred gpt-6-astra high. |

Hosted claims additionally require RM-905. Instance pooling, remaining-payload
rewrites and general backend expansion are conditional on campaign evidence and
a new bounded scope; they are not automatic follow-ons to a microbenchmark win.

## E10 — Research the public documentation site

**Done when:** the original benchmark/rubric outputs are reviewed and representative
users can complete discovery, first-success, reference, troubleshooting,
version-selection and trust-verification tasks. Production website work is
scoped afterwards; it is not authorised by this backlog.
Source: [preserved website brief and benchmark set](implementation-history.md#8-public-facing-website-and-documentation-benchmark-workstream).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-1001 | Recheck the seven primary documentation benchmarks—**Rust, Stripe, Django, ArchWiki, PostgreSQL, Godot and Twilio**—plus the [complementary shortlist, evidence standard and 12-part rubric](implementation-history.md#benchmark-set). Complete a real task per site and retain dated teardown evidence, wrong turns, time to success and limitations. | 2–8h (N) | RM-809, X-SITE | Conditional | After public-site authorisation choose comparable realistic tasks per named benchmark and execute dated teardowns using retained rubric; keep popularity separate from usability evidence. Preferred gpt-6-sol medium. |
| RM-1002 | Produce the cross-case matrix and adopt/adapt/defer/reject decisions, including audience fit and maintenance cost. | 0.5–2h (M) | RM-1001 | Conditional | Build cross-case capability matrix with audience need, evidence strength, team/budget assumptions and maintenance cost; record each pattern disposition. Preferred gpt-6-sol medium. |
| RM-1003 | Design the information architecture, content provenance, search/versioning, accessibility and agent-access contracts; prototype the prescribed page types. | 2–8h (N) | RM-1002 | Conditional | Translate accepted patterns into versioned content model and ownership; plan low-fidelity prototypes, search/accessibility/agent retrieval checks and traceability to canonical sources. Preferred gpt-6-sol high. |
| RM-1004 | Test agreed journeys with representative users; revise and approve the content/architecture brief, then create a separately estimated production-site backlog. | Unmeasured (U) | RM-1003, X-REVIEW | Conditional | Freeze website task instrument and acceptance criteria before representative-user sessions; capture failures and approve revised brief before production backlog estimates. Preferred gpt-6-sol high. |

### Recovered scope requiring planning

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-1005 | When public-site work is authorised, verify **jadpo.dev** ownership, DNS, hosting and deployment configuration and record a separately approved launch plan. Jadpo naming/CLI/file-extension migration is already reported complete; domain registration and public deployment are not established by that rename. `jadpo.com` remains an optional future acquisition, not a prerequisite. | 0.5–2h (M) | RM-1004, X-SITE, X-PLATFORM | Conditional | Only after activation inventory registrar/DNS/hosting evidence, access and rollback; propose separately approved launch checklist and scope. Preferred gpt-6-sol medium. |

## E11 — Developer console and MCP runtime inspection

**Done when:** a developer can use a UI to find slow routes/actions, investigate
errors, inspect scheduled jobs and their execution history, and review changes;
their connected LLM can query the same bounded runtime evidence over MCP and
support a measured bottleneck repair. Reuse existing compiler semantics and
review/operator contracts rather than creating a second interpretation.

Captured from the owner on 2026-10-01. **Activation:** explicitly select this
epic or a named task for planning/delivery; prerequisites still apply. Intake
does not expand the selected 29-task delivery scope or the E01 milestone.
Sources: [intake and open questions](work-plans/developer-console-mcp.md),
[diagnostics and incident packets](developer-tooling.md#33-audience-and-disclosure-boundaries),
RM-308 operator surfaces and RM-602 change review.

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-1101 | Add historical route/action performance evidence and shared inspection queries: stable operation/source-revision identity, latency/error/volume trends and drill-down to safe incident evidence. Show sample/retention limits, detect a seeded slowdown across comparable runs and measure collection overhead. Reuse existing telemetry and remain database/provider agnostic. | 2–8h (N) | RM-211 | Conditional | On activation specify versioned inspection/event schema, stable identity across revisions, retention/sampling, overhead budget and deterministic seeded slowdown workload; assess RM-219 identity dependency. Preferred gpt-6-sol high. |
| RM-1102 | Build a developer console for performance trends/slow operations and error investigation, with navigation to matching source and RM-602 behavioural change review. Demonstrate finding a seeded bottleneck, diagnosing a failed operation and reviewing its proposed change without inventing semantics in the UI. | 2–8h (N) | RM-1101, RM-602 | Conditional | After shared evidence contract define read-only incident/slowdown/change journeys and local/remote access; prototype against fixed seeded evidence before implementation. Preferred gpt-6-sol high. |
| RM-1103 | Add scheduled-task views: definitions/schedules, upcoming runs and execution history with status, timings, attempts, errors and permitted output, including explicit missing/truncated/expired results. Reuse RM-308 operator contracts and verify failed/retried/cancelled runs remain correctly correlated after restart. | 1–4h (L) | RM-1102, RM-308 | Conditional | Map RM-308 run/attempt/cancel/retry states into shared schema; define safe output/retention and restart correlation fixtures before UI integration. Preferred gpt-6-sol high. |
| RM-1104 | Provide a first-party MCP server for bounded inspection of performance history, slow routes/actions, diagnostic context, scheduled tasks and run history/results using the same evidence contracts as the console. Verify a connected developer LLM can investigate representative incidents, with scoped access, output disclosure controls and no implicit mutation authority. | 2–8h (N) | RM-1101, RM-308 | Conditional | Select bounded read tools and transport/auth access model; define disclosure/pagination limits and revision validation; plan connected-LLM incident tests against deterministic seed. Preferred gpt-6-astra high. |
| RM-1105 | Demonstrate an LLM-assisted bottleneck repair workflow: identify a regression through MCP, retrieve revision-matched context, propose a reviewable fix, then compare before/after performance and correctness under equivalent workloads. Surface evidence in the console and keep implementation/deployment subject to the normal authorisation and review boundaries. | 1–4h (L) | RM-1104, RM-1102 | Conditional | After console/MCP exist prepare fixed regression/checkpoints, correctness guards, before/after workload and acceptance thresholds; use verified repair evidence without implicit edit/deploy authority. Preferred gpt-6-sol high. |

### Recovered scope requiring planning

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-1106 | Explore the recovered **Jev / optional advisory intelligence** idea: diagnostic root-cause prioritisation, intent drift/change risk, migration/test guidance, and runtime health/anomaly/capacity or threshold-breach prediction. Compare deterministic baselines with a replaceable provider using bounded evidence, uncertainty and false-positive measurements. Compilation, policy and mandatory tests must remain deterministic/offline; no advisory output authorises a change. | Unestimated (needs planning) | RM-1101 | Conditional | On explicit selection choose one advisory question; assemble bounded labelled evidence and deterministic comparator; define uncertainty, privacy, false-positive and cost evaluation before provider use. Preferred gpt-6-sol high. |
| RM-1107 | Assess the discussed **paid monitoring/debugging service** over compiler-derived semantic telemetry, with optional provider proxying and later hosted builds/deployment as separate alternatives. Record customer need, privacy/retention, costs, provider terms/portability and a proceed/defer/reject decision; preserve a usable local core. No commercial proposition, provider, pricing or launch is approved. | Unestimated (needs planning) | — | Conditional | On explicit selection choose customer/research question, evidence sources and cost/privacy portability matrix; compare monitoring, proxy and later build/deploy separately; record proceed/defer/reject. Preferred gpt-6-sol high. |
| RM-1108 | Add **hierarchical application graphs and live execution overlays** for users and LLMs: connected component/operation subgraphs, node/edge documentation and rationale links, generated flowcharts, request/event/subscriber paths and joins, safe logs/errors, queue versus execution timings, traffic/failure statistics and filtering. UI and structured inspection share one versioned graph/evidence contract. Production incidents retain exact build/source-map evidence and map through stable logical identities into bugfix branches with changed/missing/ambiguous locations explicit. Completion: trace a branched/retried dev request, diagnose a seeded slowdown and production error after graph-changing fixes, and measure collection overhead/disclosure. | Unestimated (needs planning) | RM-222, RM-309, RM-219, RM-1101, RM-311 | Conditional | [Assessed graph contract/probe plan](work-plans/developer-console-mcp.md#hierarchical-application-graph-plan--2026-10-04); planning selected, implementation still conditional. Extend RM-1102/RM-1104 consumers. Start artifact/seeded-evidence slices before real delivery overlays once their own prerequisites settle; full acceptance needs RM-311. Freeze evidence/overhead limits and re-estimate after first probe. |


## E12 — Explore application contracts and frontend integration

**Done when:** a bounded React exploration makes the proposed developer experience,
rendering/deployment trade-offs and assurance limits concrete, with an
adopt/adapt/defer/reject decision and separately scoped follow-up work.

Captured from the owner on **2026-10-03**, explicitly placed **at the end of the
roadmap**. **Activation:** later explicit selection after preceding roadmap work;
intake does not start exploration, approve new semantics or expand current delivery.
Treat the web frontend as an independent client, like a mobile app. Explore a
compiler-derived, versioned public application contract plus client consumption
declarations, generated TypeScript bindings/runtime and an optional SSR/hosting
plugin. Keep presentation in ordinary React/JavaScript, allow separate projects
and colocated or independent deployments, and retain conventional HTTP/API use.
Swift/iOS and Kotlin/Android bindings and a native Jadpo frontend language remain
alternatives to assess, not committed implementations. Avoid ecosystem lock-in
by testing replaceability rather than assuming generated JavaScript provides it.
Related foundations: [generated artifacts](generated-artifacts.md),
[gateway/runtime boundaries](runtime-target-v0.1.md),
[authority and freshness](entity-query-model.md), and [trust model](threat-model.md).

| ID | Remaining task and completion condition | Effort | Depends on | State | Next stage / model routing |
|---|---|---|---|---|---|
| RM-1201 | Explore deep application contracts and optional frontend integration through a **React todo application**. Show representative authored component, query/form/action code, generated types/library and client consumption declarations. Assess public types/constraints/omission, permitted outputs, domain versus operational/unknown outcomes, authentication, freshness/revisions, mutation refresh, concurrency/retry/deduplication and supported-client compatibility. **Proposed acceptance:** a bounded React proof comparing browser rendering, optional SSR, initial-data reuse and colocated versus remote calls; retain enforcement/disclosure checks and stale-read, disconnect, duplicate-submit and older-client evidence. Compare setup/deployment effort, meaningful-content and interaction latency, request counts and maintenance/exit costs; record adopt/adapt/defer/reject and scoped follow-ups. | Unestimated (needs planning) | RM-109; further prerequisites TBD at planning | Conditional | Needs planning. On later selection bound the React proof, supported contract subset, renderer/transport and measurements; assess related RM-219/RM-402/RM-405–RM-407/RM-801 without importing their entire scope. Preserve enforcement for every transport; separate renderer dependencies and trust from the checked backend. Assess model/effort when selected. |

## Scope boundaries

Package registries, macros/general metaprogramming, a broad standard library,
general caller mutation, unrestricted module extensions, broad deployment
support and production native/Wasm promotion remain outside the committed
prototype. Existing LSP, formatter, bounded modules and experiments are already
implemented foundations or explicitly listed work, not future prohibitions.
New scope needs an explicit task and its owning semantic decision.

## Revset and the web review UI

**RM-601, RM-602, RM-605 and RM-704 remain open.** The user requested a custom
web review experience because large AI-written diffs conceal context, nuance,
relationships, indirect effects and plausible-looking bugs. The intended surface
starts with meaningful behavioural decisions, then progressively reveals focused
paths, evidence and source. [Original rationale and full brief](implementation-history.md#p10r4--human-approval-protocol)
and [approval protocol](approval-protocol.md#5-review-surface) remain required reading.

**Revset** is inspiration, not a dependency. Prototype one permission or
information-disclosure change, such as support staff gaining the ability to
export customer email addresses. Show who can now do what, to which data, why,
and the human decision required. Preserve all of these acceptance requirements:

- Changed relationships and unchanged declarations that become newly reachable;
  a focused actor → route → query/action → field → output/external-effect path,
  expandable into surrounding context.
- Current/proposed behaviour, successive agent revisions and alternatives,
  distinguishing unchanged review content from approval validity for the exact
  subject digest.
- Source, proof obligations, tests, runtime boundaries and explicitly classified
  uncertainty linked to nodes and relationships. Versioned compiler artifacts
  supply meaning; the UI cannot invent another semantic model.
- Actor/scenario simulation, contract/body changes and counterexamples, including
  changed behaviour with unchanged graph structure. A graph is not a correctness
  proof. Support focused human-owned decisions rather than blanket approval.
- An equivalent non-graphical export for CI, accessibility and archive.
- Comparison with ordinary PR diffs and concise behavioural review, measuring
  affected-actor/data/transitive-effect comprehension, risk detection, review
  time, clarification, approval quality and false confidence. Include the
  unchanged-graph case; retain the comprehension study's frozen scoring.

This prototype alone does not close P10R or validate the approval protocol.

## Discussion and legacy work index

This is a coverage index, not a second task list. **An item can be preserved
without being approved for implementation.** Open work resolves to stable RM IDs;
completed foundations resolve to evidence; provisional, rejected and superseded
ideas retain their reasons in the linked source. The
[2026-10-01 reconciliation and source inventory](conversation-coverage.md#4-roadmap-reconciliation--2026-10-01)
explain what was checked. New discussions must update this index or its owning
issue/specification and relevant task; keyword names should remain searchable.

| Discussed topic / old workstream | Current disposition and task | Context and acceptance source |
|---|---|---|
| Original LLM backend language / Project OS / living specification; minimum ambiguity per token | Continuing design constraints across every epic; compiler, human and agent authority remain separate | [Original extraction, all 22 themes](conversation-coverage.md#1-chronological-extraction), [vision](vision.md), [agent workflow](agent-workflow.md) |
| P0–P7 / Milestone A: grammar, nominal/field types, validated construction, closed failures/effects | Implemented foundations; remaining conformance RM-201, RM-208, RM-215, RM-216, RM-508 | [History evidence](implementation-history.md#implemented-capabilities), [type system](type-system.md), [failure model](failure-model.md) |
| P8–P9 / Milestone B: deterministic artifacts, dependency-free Bun, schema/route/audit/docs generation | Implemented foundations; parity RM-109/RM-506, metadata RM-219 | [Generated artifacts](generated-artifacts.md), [Bun target](runtime-target-v0.1.md); optional JWT is the specifically approved pinned-package exception in AUTH-001 |
| P10: typed CRUD, relationships/joins, query counts, constraints and transactions | Core implemented; advanced traversal RM-212, migrations RM-408, identities RM-409, query/index work RM-410, concurrency RM-401/RM-402 | [Persistence](persistence-v0.1.md), [original relationship brief](implementation-history.md#p10-relationship-and-joined-loading-slice) |
| P10.5: patches/omission, modules/imports, immutable values/local rebinding, migration identity and accepted index advice | Bounded core implemented; extensions RM-216/RM-408/RM-410/RM-807 | [Original scope and limits](implementation-history.md#p105--pre-p11-language-completion), [issue log](language-issues.md) |
| P10.6: entity versus type/persist; entity dossiers, non-persistent entities, receivers, policy, queries and file roles | Accepted entity model; compatibility RM-209, lifecycle RM-205/RM-206, bindings RM-204, relationship/compound spelling RM-212 | [Entity/query model](entity-query-model.md), [superseded prototype rationale](implementation-history.md#implemented-type-and-persistence-consistency-prototype--superseded) |
| P10.7: exact `fails`, `attempt`, exhaustive outcome handling; no authored async/await; IDE outcome views | Core implemented; remaining mappings RM-207/RM-215, transactions RM-401/RM-402, cancellation RM-217 | [Full original callable/outcome/IDE contract](implementation-history.md#p107--callable-execution-outcome-matching-and-ide-outcomes) |
| Authentication: secure by default, browser/API/service identity, signed/opaque/JWT, independent credential and principal freshness | RM-102–RM-104, RM-109, RM-504/RM-505; preserve accepted one-principal-lookup budget with credential checks counted separately | [AUTH-001](authentication-plan.md), [golden auth clarification](decision-register.md#golden-fresh-authentication-query-accounting--2026-09-30) |
| Policy: roles scoped to companies, memberships, automatic query scoping, field/input/database/output enforcement and admin-only fields | Core evidence retained; golden integration E01, adversarial parity RM-504/RM-505, protected approval E06 | [POLICY-001](policy-plan.md), [policy examples/tests](../tests/runtime/README.md) |
| Golden todo / P11: ownership, CRUD, due dates, disablement/soft delete, reminders, readiness and audit agreement | RM-102–RM-110 and dependencies; all 44 frozen obligations must execute | [Obligation map](../tests/validation/golden-obligations.json), [candidate](../examples/golden-todo/README.md), [migration](../examples/golden-todo-migration/README.md) |
| Services / reviewed OpenAPI or documentation imports / typed provider failures / safe secrets | RM-301–RM-304; owner selected a local HTTP reference mail provider, not a compulsory hosted provider | [SERVICE decision package](decision-sprint.md#d3--service-001-and-the-service-side-of-async-001), [owner answers](work-plans/golden-delivery-planning.md#checkpoint) |
| Jobs/events / durable retries / no thundering herd / scheduled history and results | RM-305–RM-308, RM-108, RM-1103/RM-1104; safe bounded backoff/jitter defaults, database/provider independence | [Time/testing](time-testing-plan.md), [console intake](work-plans/developer-console-mcp.md) |
| Multi-database transactions / graph and cache representations / one authoritative fact | Same-domain atomic core implemented; RM-405–RM-407 projections and RM-802–RM-804 compensating workflows are separate contracts | [Authority, freshness, outbox, recovery and concurrency](entity-query-model.md); no cross-store atomicity claim from local transactions |
| Config/env/secrets / prompted terminal entry / startup, liveness, readiness, rollback | Prompted `config set`, `.env.local`, typed config and local dev core implemented; RM-403/RM-404 adapter/deployment evidence; further source/reload extensions RM-220 | [CONFIG-001](configuration-plan.md), [original complete exit](implementation-history.md#7-typed-configuration-and-deployment-readiness-workstream) |
| Time/timezones / Temporal / Zone enum / friendly dates / compiler-owned timestamps / test clocks | Core implemented; PostgreSQL/provenance RM-504; service/job fixtures E03; future calendars/recurrence/custom formats RM-220 | [Approved TIME/TEST contract](time-testing-plan.md) |
| Money/currency / Decimal / precision, rounding, overflow and payment states | Explicit pressure cases RM-801; contract and any missing implementation must precede RM-805 completion | [P12 dossier requirements](implementation-history.md#p12--orderpayment-application-and-typescript-baseline), [decision register](decision-register.md#4-open-language-questions) |
| Enum power / tagged sums / exhaustive matching / dot variants | Core implemented; storage/evolution/generated-client pressure RM-801/RM-408; broader matching RM-216 | [TYPE-006](language-issues.md); enum methods/traits and automatic display labels remain deliberately excluded, including localisation rationale |
| Syntax consistency / colons / readable errors / failure signature order / naming and reserved words | Implemented contracts retained; full naming matrix RM-201, formatting RM-202/RM-203, remaining syntax questions RM-214–RM-216; RM-222 language-wide review precedes RM-309 component messaging and RM-1108 graph work | [Syntax](syntax.md), [naming contract](naming-and-qualification.md), [formatter matrix](formatter-rules.md) |
| Enforced component messaging / typed multi-event subscribers / hierarchical live graphs / production incidents across bugfix branches | RM-222 syntax review first, then RM-309 successor contract and conditional RM-1108 graph inspection; reuse RM-219 identity and existing E11 console/MCP work | [New intake and constraints](work-plans/roadmap-assessment.md#syntax-component-messaging-and-graph-intake--2026-10-04), [existing failure channels](failure-model.md#8-disclosure-and-observability-channels) |
| DX0.5: hand-authoring, watch, live Bun restart, default healthcheck, last-known-good revision, beautiful terminal and clean JSON | Implemented foundation; retain regression checks under RM-506, live dependency health RM-403 | [Developer loop and exit evidence](developer-tooling.md#dx05--checked-local-development-loop) |
| DX1/DX2: VS Code/LSP, human-first guided errors, rich agent packets, repair alternatives, third-party logs and source links | RM-210/RM-211/RM-219/RM-508; external repair/comprehension trials RM-704 | [Developer tooling](developer-tooling.md), [diagnostic evidence limits](../tests/diagnostics/README.md) |
| Shiki / Prism / Highlight.js / Monaco / Markdown fences and plain-text fallback | RM-210 actual-host evidence remains open; registration alone is insufficient | [Renderer contract](developer-tooling.md#4-llm-and-chat-presentation) |
| Exhaustive testing / every language construct and possible error / independent hostile cases / verifier loops | RM-503–RM-508; RM-501/RM-502 are bounded completed inventory/fuzz slices, not universal coverage | [Validation work packages](../tests/validation/README.md), [gap inventory](../tests/validation/roadmap-gap-inventory.md) |
| Revset / advanced custom web review / semantic impact graphs / successive agent revisions | RM-601/RM-602/RM-605, then RM-704 comprehension evidence | [Full active brief](#revset-and-the-web-review-ui), [original Revset entry](implementation-history.md#backlog--focused-semantic-change-graphs-in-the-review-ui) |
| P10R.1–P10R.7: golden, policy proof kernel, threats/TCB, approval, first-user hypothesis, comparison and documentation authority | RM-701–RM-706 with E06/E08 dependencies; five first-user sessions and independent review deferred until technical completion, never waived | [All seven original packages](implementation-history.md#p10r--assurance-and-validation-reset), [research brief](research-brief.md) |
| Milestone C and SQLite D1 / hosted claims | RM-706 must account for the original D1/hosted gate. RM-905/RM-404 supply selected-platform evidence; absent D1 evidence must remain unmet or receive an explicit claim/scope disposition | [Original Milestone C](implementation-history.md#milestone-c--basic-backend-prototype); local SQLite is not D1 qualification |
| P12 / Milestone D: Jadpo versus credible TypeScript, repeated fair trials and incident review | RM-801–RM-809; preserve time, tokens, defects, interventions, workarounds, tool/context parity and continue/redesign/framework-pivot/stop thresholds | [Full comparative brief](implementation-history.md#p12--orderpayment-application-and-typescript-baseline), [comparison protocol](comparison-protocol.md) |
| Opinionated project layout / deterministic skeleton / optional wizard and capability packs | Base scaffold implemented; RM-807 pressure-tests small/large/multi-domain/multi-app layouts and bounded escape needs before enforcement/extensions | [Project structure](project-structure.md); wizard stays an interface over reproducible flags |
| WASM-EXP1 / native Rust / Workers / direct Wasm versus shared generated Rust | Experiments retained as research; RM-901–RM-906 qualify missing coverage and platform evidence; Bun remains default | [Coverage plan](wasm-large-row-plan.md), [host limits](wasm-host-capabilities.md), [trust decision](wasm-host-trust-decision.md), [monitor results](capability-monitor-results.md) |
| Large-row latency / Unicode tails / no node_modules / embedded database/queue/cache / threading / simple build commands | Completed bounded candidates and rejected/deferred alternatives stay in experiment evidence; RM-901 capability inventory, RM-902–RM-906 qualification; no automatic embedded distributed platform or target migration | [Experiment ledger](../experiments/capability-monitor/performance-ledger.md), [compiler target rationale](compiler-runtime.md), [CLI](../jadpo/README.md) |
| Public documentation benchmark / Rust-style tutorial / practical runnable examples | RM-1001–RM-1004 retain every named primary/complementary benchmark, evidence rubric, page prototype and usability journey | [Complete original website brief](implementation-history.md#8-public-facing-website-and-documentation-benchmark-workstream); private learning experiments are distinct from public launch |
| Jadpo name / `.jadpo` / `jadpo.dev` / optional `.com` | Repository rename reported complete in its source chat; current name is adopted. Domain/launch evidence RM-1005; no purchase assumed | [Naming decision](decision-register.md#jadpo-name-and-canonical-domain--2026-09-25), [audit provenance](conversation-coverage.md#4-roadmap-reconciliation--2026-10-01) |
| Developer console / performance history / error investigation / jobs / MCP-assisted bottleneck repair | RM-1101–RM-1105; original requested outcomes preserved, conditional activation unchanged | [Full intake and planning questions](work-plans/developer-console-mcp.md) |
| Jev / diagnostic guidance / runtime health prediction / paid monitoring and provider proxy | Recovered exploratory ideas RM-1106/RM-1107; not accepted compiler/runtime dependencies or a validated business model | [Preserved discussion and constraints](product-strategy.md#9-optional-advisory-intelligence-and-hosted-observability) |
| SSR versus separate client apps / deep application contracts / React plugin / generated TypeScript, Swift and Kotlin integration / optional Jadpo frontend | RM-1201, deferred E12 exploration at the roadmap's end; frontend remains optional and backend/API use remains supported | [Owner intake, proposed React proof and unresolved choices](#e12--explore-application-contracts-and-frontend-integration) |
| Roadmap workflow / adaptive planning and implementation loops / model routing / timing / daily progress / documentation hygiene | Operational tooling and recorded RM-213 outcome retained; this reconciliation does not start delivery or expand another chat's selected scope | [Workflow](roadmap-workflow.md), [cheat sheet](workflow-cheat-sheet.md), [timing](task-timing/README.md), [reporting](progress/README.md), [maintenance](README.md#document-ownership-and-maintenance) |

### Language issue coverage

Every active issue has a destination below, including deferred questions. The
issue log owns semantic status; this index does not declare unresolved features
implemented. Related issue IDs share a task only where its described scope covers
them. Conditional implementation still requires its named trigger and planning.

| Issue | Remaining task or recorded disposition |
|---|---|
| NAME-001 | Implemented current naming/qualification contract; RM-201 verifies boundaries and every new surface inherits it. |
| NAME-002 | RM-201 full identifier and reserved-word matrix plus owner-selected policy. |
| MOD-001 | Bounded core implemented; RM-807 layout/module pressure, RM-216 selected language extensions; aliases/re-exports/packages not silently accepted. |
| LAYOUT-001 | Entity roles implemented; RM-807 wider layout and companion-file decisions. |
| SCAFFOLD-001 | Deterministic static base implemented; RM-807 conditional capability packs/wizard. |
| SYN-001 | RM-214 durable comment/documentation/intent/rule/decision contract. |
| TYPE-001 | Validated construction core implemented; RM-215 reconciles remaining failure-contract questions. |
| TYPE-002 | Invariant collection core implemented despite stale first-parser wording; RM-208 Set/Map boundary contract, RM-216 richer operations. |
| TYPE-003 | RM-216 conditional transparent-alias decision; nominal aliases remain current. |
| TYPE-004 | RM-216 reviewed low-level representation escape only with application evidence. |
| TYPE-005 | Immutable-value/local-slot core implemented; RM-216 conditional caller-mutation/foreign-buffer/zero-copy pressure; hidden optimisations must remain unobservable. |
| TYPE-006 | Closed/payload enums implemented; RM-801 persistence/client compatibility pressure, RM-408 selected migration extensions. |
| FAIL-001 | Outcome matching/recovery core implemented; RM-215 reconciles old deferred wording and remaining context/cause details. |
| FAIL-002 | RM-215 compatible public-code alias contract and review syntax. |
| FAIL-003 | RM-215 catalogue pressure; RM-207 operational mappings; frozen golden 422 expectations retained. |
| FAIL-004 | RM-215 localisation ownership decision; no automatic enum display labels. |
| ROUTE-001 | RM-204 query/header grammar; selected success/principal cases are implemented; RM-109 parity. |
| ROUTE-002 | RM-217 streaming/files and post-header failure contract. |
| EFFECT-001 | Pure functions/named reads implemented; RM-302 services and RM-306 events supply remaining effects. |
| QUERY-001 | Named query core implemented; RM-410 richer planning, RM-405–RM-407 derived stores. |
| DATA-001 | Bounded relationship core implemented; RM-212 policy-safe deeper/composite traversal decisions. |
| DATA-002 | Fixed patch and omitted-versus-none core implemented; RM-410 richer patch conditions if justified. |
| DATA-003 | RM-408 broader migrations; existing exact-bound review and fail-closed limits retained. |
| DATA-004 | Explicit relationship alias core implemented; RM-212 composite-reference viability. |
| DATA-005 | RM-409 portable constraint identities/mappings. |
| DATA-006 | Static index warnings/acceptance implemented; RM-410 workload-aware EXPLAIN/statistics and compound/partial indexes. |
| DATA-007 | Entity/query/mutation core implemented; RM-205/RM-206 lifecycle, RM-209 compatibility. |
| TX-001 | Local core implemented; RM-401/RM-402 retries/contention/recovery, RM-504 adapter evidence. |
| CONSISTENCY-001 | Declaration/change-record core implemented; RM-405–RM-407 physical delivery, freshness, replay/rebuild and intervention. |
| WORKFLOW-001 | Fail-closed boundary implemented; RM-802–RM-804 durable execution, compensation and recovery. |
| AUTH-001 | Implemented adapter subsets; RM-102–RM-104 full selected integration and RM-504/RM-505 evidence. |
| POLICY-001 | Core implemented; E01 integration, RM-504/RM-505 hostile evidence, E06 protected approval. |
| SERVICE-001 | RM-301–RM-304 contract, import/checker, provider and fakes. |
| ASYNC-001 | RM-305–RM-308 durable jobs/events and operator recovery. |
| CONFIG-001 | Typed/env/prompted-secret/local-startup core implemented; RM-403/RM-404 live readiness/deployment; RM-220 further source/reload contracts. |
| TIME-001 | Temporal/clock core implemented; RM-504 adapter/provenance evidence, RM-220 future time extensions. |
| TEST-001 | Callable typed-fixture core implemented; RM-304/RM-307/RM-109 service/job/route consumers, RM-220 conditional property syntax. |
| TOOL-001 | Persistent schema IDs implemented; RM-219 non-schema identity and revision mapping. |
| TOOL-002 | CLI/LSP/editor/agent bundle implemented; RM-704 external trials, RM-508 exact diagnostic evidence. |
| TOOL-003 | RM-210 actual snippet renderer hosts and plain-text fallback. |
| TOOL-004 | Checked dev loop completed; retain RM-506 regression gate and RM-403 dependency-health distinction. |
| TOOL-005 | RM-202/RM-203 formatter decision and grammar-derived exact-output conformance. |

Open questions without issue IDs are also retained: money/currency/decimal in
RM-801; runtime budgets in RM-218; source identity/maps in RM-219; comments and
intent in RM-214; ordinary logic/type/escape decisions in RM-216; files/streaming
and structured-concurrency pressure in RM-217; further config/time/test surfaces
in RM-220. Output absence/omission and primitive/literal questions stay under
RM-208/RM-216 and the current type/wire specification. No rejected feature is
silently revived by inclusion in this index.
