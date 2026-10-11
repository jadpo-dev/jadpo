# Jadpo implementation history

**Archived:** 2026-10-01  
**Purpose:** completed implementation evidence and the preserved roadmap before reorganisation.

Use the [active roadmap](implementation-roadmap.md) for open tasks, estimates,
dependencies, and current sequencing. The snapshot below preserves existing
working-tree changes, dated evidence, detailed requirements, and legacy phase
IDs. Its statuses and “next action” text are historical, including open work;
they are not a second active backlog. Current semantics remain owned by the
[decision register](decision-register.md) and linked specifications.

The snapshot also preserves **unfinished requirements**, including the Revset
review UI and full documentation-site research brief. Their active destinations
are in the [roadmap coverage index](implementation-roadmap.md#discussion-and-legacy-work-index).
Historical status is not a completion decision; linked requirement text remains
part of current task scope unless an explicit superseding decision says otherwise.

## Implemented capabilities

These are bounded implementation checkpoints, not a claim that the full product
or independent assurance gates are complete.

| Legacy phase | Implemented scope | Evidence |
|---|---|---|
| P0–P7 / Milestone A | Language foundations, compiler CLI, parsing, semantic graph, nominal types, failures and effects | [Compiler](../jadpo/README.md), [fixtures](../tests/compile/README.md) |
| P8–P9 / Milestone B | Deterministic artifacts/scaffolding and generated TypeScript/Bun HTTP runtime | [Artifacts](generated-artifacts.md), [runtime](runtime-target-v0.1.md) |
| P10, P10.5 | SQLite/PostgreSQL CRUD, bounded relationships, updates, constraints, migration identities, modules and local rebinding | [Persistence](persistence-v0.1.md), [migration](migration-identity-v0.1.md) |
| P10.6 | Entity/query/policy foundations, explicit route success modes, generated UUID identity and nullable input defaults | [Entity model](entity-query-model.md), [migration source](../examples/golden-todo-migration/README.md) |
| P10.7 | Call/outcome semantics, IDE outcomes, local atomicity, savepoints and bounded concurrency evidence | [Runtime suites](../tests/runtime/README.md), [validation ledger](../tests/validation/unattended-progress.md) |
| DX0.5, DX1, internal DX2 | Development loop, LSP/editor support, diagnostic catalogue, agent context and formatter coverage; FMT-005 and external trials remain open | [Tooling](developer-tooling.md), [formatter matrix](formatter-rules.md) |
| CONFIG / TIME / TEST / POLICY | Typed configuration, Temporal/operation clocks, isolated callable fixtures and scoped policy enforcement | [Configuration](configuration-plan.md), [time/testing](time-testing-plan.md), [policy](policy-plan.md) |
| AUTH checkpoints | Browser/API signed and opaque credentials, service keys, pinned JWT paths and protected-route checks | [First-party example](../examples/first-party-authentication/README.md), [auth extensions](auth-runtime-extensions.md) |
| VAL-001 foundation | Unified local verifier and prepared CI workflow; latest recorded snapshot has 47 supported steps and seven PostgreSQL modes passing | [Validation entry point](../tests/validation/README.md), [ledger](../tests/validation/unattended-progress.md) |
| WASM-EXP1 and bounded follow-ups | Compilation-route, value/row transport and local host experiments; no default-target promotion | [Experiment results](wasm-experiment-results.md), [guest scans](wasm-guest-scans-results.md) |
| Native/shared Rust and capability monitors | Bounded conformance, independent authority and single-pass monitor prototypes, with explicit trust/performance limits | [Shared Rust](shared-rust-conformance-results.md), [capability host](capability-host-results.md), [monitor](capability-monitor-results.md) |

## 2026-10-04 — RM-304 checked authored service fakes

**Completed task:** RM-304

Checked authored service fixtures now exercise callable success, declared failure,
safe retry, timeout and uncertain outcomes without provider effects. Requests and
receipts retain their declared types; each test has isolated queues and clocks,
while nested calls share one attempt/deadline budget. Fresh top-level operations
reset their elapsed baseline without rewinding the fixture sequence. Missing or
empty checked authored fake mappings fail before transport.

The [independent implementation review](../tests/validation/rm304-independent-implementation-review.json)
found two defects; the [correction review](../tests/validation/rm304-independent-correction-review.json)
accepts both fixes for checked authored fixtures. The registered fake suite passes
**7 tests / 40 assertions**, with provider connections/fetches blocked and counted
at zero. The fresh [supported gate](../build/validation/20261004T084327-70967/report.json)
passes **61/61 steps**, including compile/formatter contracts and both database
adapters. This does not establish durable jobs or complete golden behaviour.

Review limitations remain explicit: malformed host-supplied maps with own nullish
entries and the loose direct-host receipt seam are outside checked authored
fixture support. No universal malformed-host-map guarantee is claimed. Real
HTTP/TLS evidence belongs to RM-303, not these fakes. See the owning
[TIME/TEST implementation record](time-testing-plan.md#45-capability-fakes-and-traces).

Original forecast **0.5–2h** retained. This continuation is recorded in
[`RM-304-8b24e5555b0f`](task-timing/runs/RM-304-8b24e5555b0f.jsonl), with review runs
[`RM-304-2d31c37421d1`](task-timing/runs/RM-304-2d31c37421d1.jsonl) and
[`RM-304-a806f10af6b7`](task-timing/runs/RM-304-a806f10af6b7.jsonl). Earlier attempts
remain in the recorder; partial sessions are not separate task completions.

## 2026-10-04 — RM-305 durable delivery semantic contract

**Completed task:** RM-305

Froze [ASYNC-001](async-plan.md) after the owner's five recorded decisions,
the [independent transaction/effect review](../tests/validation/rm305-independent-contract-review.json)
and [metadata-only freeze confirmation](../tests/validation/rm305-independent-freeze-review.json).
The reviewed semantics cover database-backed atomic enqueue, immutable identity
and versions, per-key FIFO, fenced leases, bounded/coalesced scheduling,
cumulative safe-retry budgets, cancellation, uncertain effects and privileged
replay. SQLite and PostgreSQL share one authored semantic contract; no hosted
queue is required. Review pins identify exact candidate and frozen bytes.

The [18 acceptance traces](../tests/assurance/async-contract-v0.1-candidate.json)
remain **contract-only, not executable evidence**. RM-306 owns compiler spelling,
outbox/state and restart proof; RM-307 owns workers/scheduling and crash traces;
RM-108 owns concrete golden limits and reminder integration. This closes a
semantic decision task, not the durable runtime or golden application.

Original forecast **0.5–2h** retained. This continuation is recorded in
[`RM-305-bf538ba4248f`](task-timing/runs/RM-305-bf538ba4248f.jsonl), with independent
review runs [`RM-305-11c3a87195dc`](task-timing/runs/RM-305-11c3a87195dc.jsonl)
and [`RM-305-a691be5ba07d`](task-timing/runs/RM-305-a691be5ba07d.jsonl).
Earlier preparation and its measured/missing intervals remain in the recorder;
these slices are not claimed as the whole-task elapsed time.

## 2026-10-03 — RM-303 checked provider adapter

**Completed task:** RM-303

Generated a Bun HTTP adapter for the frozen local reference mail provider. The
adapter pins its destination, method and path; verifies TLS authorization and
the pinned certificate hostname before dispatch; bounds credentials, headers
and bodies; and returns only a checked receipt. One logical operation shares a
30-second monotonic deadline and a maximum of three attempts, with each
five-second attempt shortened to the remaining time. Retries are limited to
known pre-dispatch refusal and explicit provider no-acceptance; a response lost
after provider acceptance stays unknown and is not replayed. A stateful test
provider records acceptance before dropping the acknowledgement and proves
same-key/same-payload deduplication and changed-payload conflict.

The focused service-adapter suite passes **19 tests and 90 assertions**. The
[independent implementation review](../tests/validation/rm303-independent-implementation-review.json)
approves this adapter scope, with the reviewer's loopback-listener sandbox limit
recorded. The [supported verifier report](../build/validation/20261003T234639-50956/report.json)
passes **59/59 checks**, but is not release-equivalent: the golden application
still fails compilation with 50 diagnostics and its integrated behavioural
suite did not run. The service audit consequently keeps broader runtime
conformance `not_established`; durable jobs, reconciliation and production
provider compatibility remain open.

The original **2–8h** forecast is retained. Timing is recorded in runs
[`RM-303-5eabc02fc2a9`](task-timing/runs/RM-303-5eabc02fc2a9.jsonl) and
[`RM-303-1af8a1c775d2`](task-timing/runs/RM-303-1af8a1c775d2.jsonl); the first
contains implementation, verification and independent-review correction work,
and the second records completion of the missing durable evidence trail. Their
combined active time is **105.20 minutes**, including **30.02 minutes of
verification**.

## 2026-10-03 — RM-302 checked service contracts

**Completed task:** RM-302

The compiler now parses the pinned mail OpenAPI snapshot, enforces its closed
request and receipt shapes against source declarations, and records field-level
source/import parity in the service audit. It requires the request's
`idempotency_key` to use the nominal `ReminderIntentId` type, requires the
configured bearer slot to be secret and non-null text, and resolves the pinned
import only within the owning project root. The [independent correction review](../tests/validation/rm302-independent-correction-review.json)
confirms the original schema, secret-marker and import-boundary findings are
resolved, including a mutation that replaces `ReminderIntentId` with `Uuid`.

The final [full supported report](../build/validation/20261003T213248-5311/report.json)
passes all 58 checks, including the Rust workspace, compile fixtures, runtime
suites and disposable PostgreSQL suites. The full golden application and release
gates remain open; no live provider integration is claimed. Recorded effort is
130.89 active minutes across the two implementation runs and the initial review
run; each timed run records `gpt-6-luna/xhigh`. The follow-up review's time and
actual model/effort are unknown. The original 2–8h forecast is retained. See runs
[`RM-302-6b717ae9cae8`](task-timing/runs/RM-302-6b717ae9cae8.jsonl),
[`RM-302-0a05e99f86ab`](task-timing/runs/RM-302-0a05e99f86ab.jsonl), and
[`RM-302-5f0a10056c41`](task-timing/runs/RM-302-5f0a10056c41.jsonl).

## 2026-10-03 — RM-106 golden lifecycle routes

**Completed task:** RM-106

Implemented fresh-authenticated self-disable and owner-scoped Todo soft-delete
in the migrated golden application. SQLite and PostgreSQL route evidence covers
concealment, retained state, fresh credential failure, timestamp behavior and
concurrent patch/delete. The [independent auth/lifecycle review](../tests/validation/rm106-independent-route-review.json)
approves this scoped integration with no blocking findings. Its two test-strength
notes remain visible in that report; the broader 44-case golden compile and
release gates remain open.

The [supported verifier report](../build/validation/20261003T203150-84421/report.json)
passes 58/58 checks. Recorded effort totals 238.45 active minutes across the
implementation/verification run (`RM-106-b3d3a86b6de8`, gpt-6-luna/xhigh) and
review run (`RM-106-cad27e70d915`, model/effort unknown under the owner's scoped
continuation instruction). Both runs retain their partial outcome records; the
original 1–4h forecast is unchanged.

## 2026-10-02 — RM-301 outbound service semantic contract

**Completed task:** RM-301

Froze [SERVICE-001](service-plan.md) after
[independent correction review](../tests/validation/rm301-independent-contract-review.json)
resolved all six initial findings and one retry-catalog ambiguity. The local
POST-only HTTP reference provider has an exact-byte version/hash pin, explicit
bearer credential sink, closed payload/receipt and matching body/header UUID.
Each schedule revision gets one durable intent with immutable payload; guarded
admission/completion prevent duplicate workers or stale A receipts from marking
B sent. Possible dispatch surviving timeout, loss or crash stays unknown, without
blind replay or invented status lookup. Compiler receipt observation, rather
than provider clock time, owns the matching-revision sent mark.

The [candidate declaration](../tests/assurance/service-successor-v0.1.jadpo),
[30 contract cases](../tests/assurance/service-contract-v0.1.json), and
[JOB-001 successor](../tests/assurance/service-acceptance-successor-v0.1.json)
are exact-byte pinned by the review. The pressure app, human-owned policy and
44-case current acceptance remain preserved comparison evidence; the separate
scoped policy identity amendment and JOB-001 overlay record the owner's new
reminder direction. The other 43 IDs are unchanged. The reviewed plan candidate
SHA-256 is `d0162532b1e7bcab8b0ed5dbf355830f9a7d4147284a0548e681c62b8949819f`; closure records status and review disposition.
RM-302 owns exact syntax/import/effect checking, RM-303 the real HTTP traces, and
RM-305/RM-108 durable integration. No executable service, golden-case completion,
external P10R approval or deployment is claimed. P10R/link/pin/diff checks pass.

Original forecast: 0.5–2h. All earlier planning/review attempts remain recorded.
Latest parent corrections/closure: RM-301-c0b69a48d6e2 and RM-301-1f3f6ad95b7b
(`gpt-6.1-sol/high`). Interrupted RM-301-50be866d3066 retains its 5.34-minute
unknown interval, excluded from precise effort comparisons. Independent review
retains its own Sol High pin and evidence; its exact model is recorded there
and in the reviewer timer.

## 2026-10-02 — RM-205 entity lifecycle contract

**Completed task:** RM-205

Froze [DATA-007's entity-owned lifecycle contract](lifecycle-plan.md) after
[independent correction review](../tests/validation/rm205-independent-contract-review.json)
resolved R1–R3. User disablement retains children and conceals ordinary hidden
reads; Todo delete remains a logical owner delete through one guarded soft
transition. Joined visibility applies before reminder ordering/limit.
Retention purge has only a checked clause-bound compiler maintenance authority,
expired/deleted-row guard, deterministic 500-row bound, referential constraints
and redacted audit. Authored jobs retain ordinary POLICY-D30 authority.

The [separate POLICY-D30-M1 addendum](policy-plan.md#19-retention-maintenance-addendum--policy-d30-m1)
records the owner's selected maintenance plane without changing the historically
approved policy section-2 digest. The final review pins exact candidate bytes
(`663d9bf8a60cc2d5e200c771d18c1205971b420a743a0b78fe5babb102da3215` for
DATA-007), the unchanged baseline and the separate addendum row. Freeze updates
status and references only. [27 assurance cases](../tests/assurance/lifecycle-v0.1.json)
remain non-executable contract fixtures; RM-206 owns lowering/conformance and
RM-106/RM-108 own application integration. This closes a decision contract, not
lifecycle runtime delivery. P10R/link/pin/diff checks pass.

Original forecast: 0.5–2h. Preserve prior planning/correction runs and independent
reviews; the latest parent correction/closure runs are RM-205-090cf4e6fc8e,
RM-205-8dff420ad28a and RM-205-fca941d693bd (`gpt-6.1-sol/high`). Independent
review runs RM-205-899b7f58d1ed and RM-205-b3043e25edec use `gpt-6-sol/high`.

## 2026-10-02 — RM-401 bounded retry contract

**Completed task:** RM-401

Froze the [transaction retry contract](transaction-retry-plan.md): safe retries
are enabled by default only for proved no-commit transient failures, a repeatable
effect graph and a shared bounded budget. Each attempt gets a fresh clock and
authority snapshot; delivery identity stays stable. Possible commits with lost
acknowledgements remain uncertain and cannot be replayed blindly.

Evidence: [SQLite/PostgreSQL phase probe](../tests/validation/rm401-phase-probe-results.json),
[14 contract cases](../tests/assurance/transaction-retry-v0.1.json), and
[independent review plus correction re-review](../tests/validation/rm401-rm207-independent-contract-review.json).
The same review accepts RM-207's shared boundary-contract milestone after
clarifying that job retries obey the proof gate and cleanup failure alone does
not imply an uncertain write. RM-207 stays open for integrated runtime evidence.
RM-402 owns production retries and generated-adapter conformance; neither is
claimed here. JSON/reference checks pass; this closure changes contracts only.

Timing retains earlier partial work (`RM-401-60951e3d329c`,
`RM-401-fb913791a10d`), independent review (`RM-401-d5ffa319028f`,
`RM-401-44873da3b893`) and closure (`RM-401-a7b93742b38f`). Original forecast:
0.5–2h. Review used Sol High; closure retained this batch's Astra High pin.

## 2026-10-02 — RM-107 self-scoped user todo route

**Completed task:** RM-107

Added authenticated `GET /users/{user_id}/todos` to the migrated golden Todo
source. The named query applies the existing self policy to one required User
lookup, then runs one Todo keyset-page query using the authenticated owner's
relationship, `deleted_at IS NULL`, ascending `(created_at, id)` order, and a
100-item response bound. Both filters enter SQL before `LIMIT`; the page
validates and projects only `TodoView`, and `UserWithTodos` contains only
`user_id` and `todos`. The owner kept `User.email` provisioning-only, so neither
stored email nor provider claims enter the output. This reuses checked page
lowering and leaves broad include semantics unchanged.

The [SQLite golden route suite](../tests/runtime/golden-todo-routes.test.ts)
passes 8 tests / 78 assertions. It covers zero, one and more than 100 children,
older deleted rows, a foreign row, the exact output fields, stable order,
concealment of foreign/missing parents, and a statement trace with exactly two
application data reads apart from authentication resolution. The [real
HTTP/JWT suite](../tests/runtime/golden-auth-http-jwt.test.ts) passes 4 tests /
30 assertions, including a conflicting `email` claim. The [PostgreSQL route
mode](../tests/runtime/golden-todo-postgres.test.ts) passes on PostgreSQL 16.3
with 1 test / 29 assertions; its runtime result matches a bounded ordered SQL
query, excludes old deleted/foreign rows, and validates the same projection.
Its generated page plan uses one indexed keyset query and the checked target
contains one parent lookup followed by that page query. Authentication query
accounting is separate from the route's two data reads.

The full supported [validation report](../build/validation/20261002T165828-85176/report.json)
passes 55 checks, including nine PostgreSQL modes. The golden source compilation
gate still reports 53 diagnostics, so the 44 frozen acceptance cases remain
`not_executed`; the independent policy review also remains pending. AUTH-007 and
REL-001 have focused evidence, but their map status is retained until the
source-bound golden harness runs. The first full-gate attempt
([report](../build/validation/20261002T165719-84507/report.json)) exposed that a
target unit test still expected five generated routes; after updating its count
to six, the focused test and final full report passed.

The original forecast was 0.5–2 agent-hours. Timer run
`RM-107-8e6fcc38df94` records 35.23 active minutes, including 7.23 minutes of
verification (a subset of active time), no blocked or unknown interval, and
gpt-6-luna/xhigh. This is one completed task sample, not a recalibration of the
forecast band.

## 2026-10-02 — RM-105 filtered keyset list

**Completed task:** RM-105

Added the checked `query page` expression and generated `GET /todos` route with
typed optional status/due-before filters, a URL-encoded JSON cursor, stable
descending `(created_at, id)` order, owner/deleted-row scope before limit, a
page-size default of 25 bounded to 1–100, and one `page_size + 1` data query.
First-page and continuation SQL are static prepared shapes; PostgreSQL optional
parameters carry explicit types. The compiler creates the partial descending
index `todo_page_owner_id_created_at_id_where_deleted_at_is_null_idx` over
owner/order keys for `deleted_at IS NULL` rows. Partial predicates participate
in both index de-duplication and naming, so distinct visibility filters on the
same key columns cannot collide. Pagination avoids OFFSET and sorting the
continuation result.

The SQLite route suite passes 6 tests / 52 assertions, including equal-time
cursor progression, stable repeats, foreign/deleted-row exclusion, composed
filters, defaults and invalid size, and an `EXPLAIN QUERY PLAN` seek through the
generated index without a temporary B-tree. The PostgreSQL 16.3 disposable-cluster
suite passes 1 test / 16 assertions over 8,000 older rows plus equal-time,
deleted and foreign rows. It traverses the same cursor pages and uses
`EXPLAIN ANALYZE` at a 4,000-row-deep cursor to verify the partial index, no Sort,
and no more than 26 rows emitted for a page size of 25. Both suites check the
generated one-data-query/no-offset/page-plus-one manifest. Authentication lookup
accounting remains separate from that data-query budget.

The full supported [validation report](../build/validation/20261002T162110-68433/report.json)
passes after registering the PostgreSQL suite and adding conformance tests for
the new page diagnostics. The report keeps the full golden application gate open;
the frozen LIST-001/002 obligations remain unexecuted.

This is bounded route/compiler evidence. LIST-001/002 remain `not_executed` in
the frozen obligation map until RM-109 runs the complete source-bound harness.
The implementation run is `RM-105-a3b3bbd36f04`; it retains the original 1–4h
forecast and selected gpt-6-luna/xhigh settings.

Evidence: [SQLite route suite](../tests/runtime/golden-todo-routes.test.ts),
[PostgreSQL route suite](../tests/runtime/golden-todo-postgres.test.ts),
[runtime test instructions](../tests/runtime/README.md), and the
[RM-105 plan](work-plans/golden-delivery-planning.md#rm-105--l-short-plan-conditional-on-rm-204).

## 2026-10-02 — RM-204 typed route query and header bindings

**Completed task:** RM-204

Added `query: NamedObject` and explicitly mapped `headers: { local: Type from
"Wire-Name" optional }` route inputs with typed `query.*` and `headers.*`
bindings. Static checking requires a closed query Object, limits header values
to supported scalar types, rejects binding collisions and invalid or duplicate
wire names, and prevents application declarations from consuming credential,
CSRF, trusted-identity, or compiler-owned headers. Formatter output, generated
inventory/validator artifacts and OpenAPI parameters derive from the same checked
route model.

The generated HTTP boundary validates the raw query before one form-decoding
pass, rejects unknown and repeated keys, accepts JSON-encoded structured fields,
applies source defaults only when absent, and separates malformed syntax (400)
from typed-value failures (422). Integer query values outside JavaScript's exact
safe range are rejected before conversion can round them. Bun Fetch does not
retain repeated ordinary-header lines, so applications with declared headers use
the built-in `node:http` adapter to count `IncomingMessage.rawHeaders` before Fetch
normalization. Request bodies stay streamed through this adapter, preserving
authentication and CSRF ordering without pre-authentication buffering.

The query/header and protected-route real-listener suites pass 13 tests / 152
assertions. They cover defaults, percent/UTF-8 and structured JSON decoding,
safe-integer bounds, OpenAPI agreement, duplicate/missing declared headers,
bearer and browser authentication, Origin/CSRF, duplicate raw credentials,
POST body conversion and response delivery. The [independent runtime/security
review](../tests/validation/rm204-route-input-review.json) approves the corrected
boundary. The fresh 54-step [supported gate](../build/validation/20261002T083246-98017/report.json)
passes; the complete frozen golden and release gates remain RM-109/RM-110.

Original estimate: 1–4 agent-hours. Implementation/checkpoint runs
`RM-204-8c4ec42af98a`, `RM-204-b41c0538f662` and `RM-204-ec61df337c62`, plus
independent review runs `RM-204-db4af1cb9434`, `RM-204-5fc0d8b8380d` and
`RM-204-d7f2a1a862a6`, retain the measured work and review findings.

## 2026-10-02 — RM-104 generated service credential exchange

**Completed task:** RM-104

Added the checked strategy-level `exchange` declaration and compiler-owned
`POST /auth/exchange` route while retaining trusted-host one-time key provisioning.
There is no authored mint or administration endpoint. The generated route uses
the existing exact credential inventory, passes the selected service key only to
the host exchange operation, reuses its checked private credential record, and
performs one active service authority lookup before signing and auditing. Its
response contains only the new bounded bearer, token type and exact UTC expiry.

The [generated HTTP suite](../tests/runtime/golden-service-credentials.test.ts)
passes 18 tests / 166 assertions on SQLite and 18 / 160 on disposable PostgreSQL.
It covers exact key-expiry capping, active and disabled services, expired/revoked/
wrong-audience keys, user and competing credentials, duplicate raw Authorization
lines through a real listener, audit failure, secret-free artifacts, one SQLite
private/declared/principal lookup each, and a fresh generated process. The clean
[supported gate](../build/validation/20261002T075300-73475/report.json) passes;
the complete frozen 44-case golden application gate remains RM-109/RM-110.

The [independent runtime/security review](../tests/validation/rm104-generated-runtime-review.json)
approves the scoped implementation. Its two preliminary findings were withdrawn
after direct compiler evidence confirmed that the project-wide credential-slot
invariant rejects a second `authorization_header` bearer strategy before target
generation, making both scenarios unreachable in checked source. The accepted
[exchange contract](../tests/validation/rm104-service-exchange-contract.json)
records the wire, failure, audit and secrecy boundaries.

Original estimate: 1–4 agent-hours. Runtime implementation run
`RM-104-01e20c5f8563`, independent review run `RM-104-f3d478716ecf`, and closure
run `RM-104-7fb41ab9cca0` retain the measured work; earlier partial planning and
contract-review runs remain in the timing ledger.

## 2026-10-01 — RM-213 documentation audit and maintenance controls

**Completed task:** RM-213

Audited documentation authority, duplication and task-context retrieval. Removed
repeated generic procedure from the local workflow (1,692 → 719 words), added
named reading paths for protected todo generation, service planning and local
attestation, and replaced the index's duplicated phase snapshot with current
source links. Added concise ownership/write/update/archive rules linked from
AGENTS.md; no separate cleanup skill is justified without evidence these fail.
Preserved frozen contracts, history, raw evidence and existing section anchors.

[Audit evidence and limits](work-plans/documentation-hygiene.md#audit-outcome--2026-10-01)
records the three before/after navigation checks. Local links/anchors and
`git diff --check` pass; eight authority/frozen files retain their hashes and the
preserved roadmap snapshot remains unchanged. This is documentation work, not a
compiler/runtime or external-usability claim. Original estimate was unknown;
runs `RM-213-eca999414c8d` (planning) and `RM-213-5dd4a47ebfda` (audit/cleanup)
retain actual timing and unknown model/effort.

## 2026-10-01 — RM-101 golden obligation mapping

**Completed task:** RM-101

Mapped all 44 frozen acceptance IDs to existing migrated declarations, missing
compiler/runtime work, owning task IDs and required observations in the
[obligation map](../tests/validation/golden-obligations.json). Every case retains
its unexecuted/clarification state and requires RM-109 integration. Preserved
57 original candidate diagnostics byte-for-byte and 23 informational contract
digests in [baseline provenance](../tests/validation/golden-baseline/provenance.json).
Acceptance and human-owned policy bytes are unchanged.

Verifier inventory, case-set/digest/path/task-reference checks and diff checks
pass. The existing local compiler confirms 11 files/47 declarations in the
migration; this is source-only evidence from the available binary, not a rebuilt
compiler or runtime acceptance. An initial unsupported `--json` invocation was
corrected to `--diagnostic-format=json`. Mapping exposed pending GF-032 boundary
semantics and RM-403 readiness outside the selected scope; neither is silently
resolved. Original estimate 0.5–2h; run `RM-101-675218da2803` retains timing.

## 2026-10-01 — RM-103 browser origin and CSRF closure

**Completed task:** RM-103

The owner-authorised [successor revision](../examples/golden-todo/REVIEW.md#2026-10-01-browser-origin-configuration-revision) adds required `BROWSER_ORIGIN` to the migrated source and binds it to the signed browser validator. Its before/after successor digests are retained there. The original app, 44-case acceptance file and human-owned policy still match the [RM-101 baseline hashes](../tests/validation/golden-baseline/provenance.json); later RM-102 authentication changes explain why the current successor authentication source no longer has the origin-only after digest.

The generated [protected-browser suite](../tests/runtime/golden-protected-route.test.ts) proves a valid origin plus session-bound CSRF token can create a Todo, while missing/wrong origin, missing/wrong token and another session's token reject without a write. Bearer and cookie credentials remain distinct. The common [first-party suite](../tests/runtime/first-party-authentication.test.ts) covers invalid origin configuration, exact origin matching and compiler-detected mutative GET using the same generated authentication boundary. The generated entry point calls `loadConfiguration` and initialises authentication before `Bun.serve`, so invalid required origin configuration fails before listener binding; the suite's subprocess startup checks exercise this ordering for invalid configuration. This conclusion combines generated-source order with targeted initialization and generic process tests; no separate process test uses the exact missing-origin value.

The [prior independent authentication review](../tests/validation/rm102-authentication-review.json) explicitly scoped bounded RM-102/103 protected browser integration and found no blocking issue in its reviewed change; the [RM-102 closure review](../tests/validation/rm102-declared-credential-review.json) records combined user/browser review and real HTTP integration. Neither is external P10R approval. The previously recorded [52-step supported gate](../build/validation/20261001T231148-76808/report.json) remains the latest product gate; this closure is an evidence/documentation audit and did not rerun it. All 44 frozen cases remain unexecuted and RM-109 retains the complete integrated golden gate.

Original estimate: 0.5–2 agent-hours. This closure audit is timed as `RM-103-43e6136d59ac`; prior partial RM-103 timing remains in the recorder. Scope after closure is RM-104 authored service exchange and RM-107 synchronous routes, not a new origin or authentication protocol.

## 2026-10-01 — RM-102 migrated authentication target

**Completed task:** RM-102

Completed target generation and execution for the migration's selected signed
browser/API, external JWT and service-key/bounded-token adapters, configuration
and typed principal mappings. Declared credential binding separates the verifier
and lifecycle authority from the nonsecret service subject and stable UUID.
Issuance atomically writes credential and private metadata; expiry, revocation,
service disablement, rotation, identity substitution and metadata corruption
have fail-closed SQLite/PostgreSQL regressions. Both trusted revocation APIs
update the declared record; failed insertion of either row rolls back issuance.

Real localhost HTTP tests execute signed-user and cryptographically verified
JWT Todo creation, check stored ownership and typed `primary`, and reject absent,
malformed and bad-signature credentials without writes. Earlier protected-browser
checks cover origin/CSRF and initialization failure. The [final full gate](../build/validation/20261001T231148-76808/report.json)
passes **52 supported steps, including eight PostgreSQL modes**. The new service
suite passes 13 tests / 103 assertions per database; real HTTP/JWT adds 3 / 27.
[Independent auth and public-language review](../tests/validation/rm102-declared-credential-review.json)
approves this task's scoped acceptance after the recorded findings were fixed.
The [runtime contract](auth-runtime-extensions.md) owns syntax and supported limits.

RM-104 still owns authored issuance/exchange endpoints; RM-109/RM-110 own the
44-case integrated golden harness/gate, with no case claimed executed here.
Unknown-commit/restart and concurrent-disable injection are not established by
this slice; transaction recheck/locking is source-reviewed and two-write rollback
is executed. This is not external assurance or release qualification.

Original forecast **1–4h**, preserved in the timing records. Completion run
`RM-102-a64117c8d2e1` and contributing schema/test/review runs
`RM-102-c995459d4fa3`, `RM-102-7eee400fc4c9`, `RM-102-4855722e650e`
and `RM-102-8c26cfd02f84` retain actual Astra High settings. Prior partial
attempts, including the earlier strength batch, remain in the task timing ledger;
do not treat the final run alone as whole-task effort.

## Preserved roadmap snapshot

The original text follows unchanged. Its original progress log begins at
[Progress log](#11-progress-log). New completion entries belong above this
snapshot and should name the stable task ID from the active roadmap.

---

# Jadpo implementation roadmap

**Status:** active progress tracker
**Last updated:** 2026-10-01
**Current phase:** comprehensive validation is in progress; the unified local verification gate and CI workflow are implemented, with full golden behaviour and independent test expansion still open. The unattended DATA-007/TX-001/CONSISTENCY-001 and CONFIG-001 foundations are implemented and verified. AUTH-P0–P3 cover the closed user/service principal, route reachability, exact credential selection, typed authority resolution, and the strategy-independent runtime selector. The 2026-09-29 AUTH-P4/P7 first-party checkpoint adds real signed/opaque browser/API credentials, protected-route execution, configuration sinks, authority/session checks, CSRF, and SQLite/PostgreSQL/HTTP evidence including restart and initialization recovery; the scoped browser/API milestone is complete, while service/JWT and broader authentication exits remain open. NAME-P0–P2 are enforced. TIME/TEST supplies the Temporal runtime, stable operation clock/deadlines, lifecycle timestamp ownership, isolated typed fixtures, and callable evidence. POLICY-P0–P4 plus the executable P5 core supply scoped user/service roles, automatic persistence predicates, validation composition, concealment, invoke policy, and audit evidence. The remaining exits are genuinely gated: service/JWT authentication and broader principal mappings, services and jobs, protected CI approval attestation, broader PostgreSQL coverage, platform hooks, P10R/DX2 human evidence, physical cross-store adapters, and durable workflows require their recorded decisions, providers, or external systems.

This document is the implementation control plane. It records what must be
built, what evidence completes each phase, what is deliberately deferred, and
what should happen next.

Progress is evidence-based. A phase is not complete because code exists or
because it feels nearly finished; every exit gate must be satisfied and linked.

### Unattended implementation triage

Before starting any roadmap area, assess whether its next slice is executable
from accepted decisions and existing contracts. Park the slice instead of
implementing it when it would:

- choose between materially different language, runtime, product, policy, or
  security semantics that have not already been accepted;
- freeze public syntax, generated contracts, migration behaviour, or an
  authority boundary while a relevant question remains open;
- require human judgement, external evidence, or approval that the compiler
  and repository cannot supply; or
- make a speculative foundation that would bias a later decision even if the
  surface were hidden temporarily.

A parked slice must name the unresolved question and the affected exit-gate
evidence in this roadmap or the decision register. Do not implement one option
as an implicit default. Continue with the next independent, fixture-backed
slice whose semantics are already decided. Mark the whole phase `blocked` only
when no such independent work remains; otherwise keep the phase `in progress`
and distinguish its implemented and parked parts explicitly.

### 2026-09-25 roadmap correction

The original charter required complete golden applications to force the
important semantic decisions before substantial compiler construction. The
project instead built the seed compiler and began persistence before completing
the canonical todo application. That work remains useful exploratory evidence,
but it is not evidence that the product thesis is true and must not be allowed
to determine the comparison protocol after the fact.

P10R is therefore a mandatory interruption, not an additional implementation
feature. Until its exit gate is met:

- completed P10 capabilities may receive defect fixes, reproducibility work,
  and stabilisation evidence; new work must remain assigned to an explicit
  later exploratory phase rather than silently expanding completed P10;
- technical implementation may continue through P10.5, P11, and P12 only as
  explicitly labelled exploratory work; it cannot satisfy a release-equivalent
  assurance gate or product-validation claim;
- the complete todo design, policy/proof model, threat model, human-approval
  protocol, first-user hypothesis, and falsification thresholds must remain
  versioned so later external evidence cannot be folded silently into the
  original candidate; and
- completed compiler work is classified as exploratory scaffolding rather than
  validation of the language or product.

### 2026-09-25 project-owner deferral

The project owner explicitly chose to defer the outside contract review and five
first-user sessions and continue implementation. On 2026-09-25 the owner
clarified that the five sessions should occur only after the planned technical
implementation is complete because implementation has so far been inexpensive.
Technical work may therefore continue through P10.5, P11, and the P12 reference
implementations. This does not satisfy, waive, or retroactively pass the P10R
exit gate. All such work remains exploratory evidence: P11 release-equivalent
assurance claims, Milestone C completion, comparative validation, and product-
validation conclusions remain blocked until the deferred evidence is completed.

The candidate contracts, thresholds, fixtures, and protocols must retain dated
digests before P11/P12 implementation proceeds. If later external review
changes them, the original and revised versions and the implementation exposure
to each must be recorded. The five sessions occur after feature-complete
implementation and before final P12 comparative trials or a
continue/redesign/pivot/stop decision.

The correction preserves the intended technical scope. Its purpose is to make
the existing scope testable without allowing implementation momentum to decide
the result.

## 1. Milestones

### Milestone A — semantic compiler

The first meaningful compiler can run:

```text
jadpo check <project>
jadpo inspect <project>
```

It parses the core language, resolves declarations, builds a semantic graph,
checks nominal typing and failure propagation, and emits human-readable plus
machine-readable diagnostics. `inspect` emits a semantic manifest.

Milestone A comprises phases P0–P7.

### Milestone B — runnable compiler

The first end-to-end compiler can run:

```text
jadpo build <project>
```

It generates a TypeScript/Bun application and runtime metadata for the
Jadpo seed application. The result serves one real route with boundary
validation and safe failure handling.

Milestone B comprises phases P8–P9.

### Milestone C — basic backend prototype

The compiler supports the canonical todo application with Postgres, SQLite (local), SQLite (D1),
authentication required by default, ownership policy, CRUD, safe failures, and
generated artifacts.

Milestone C comprises P10, the corrective P10R gate, P10.5, P10.6, P10.7, the
DX0.5 local development loop, and P11. P10R must complete before Milestone C can be
claimed. By explicit owner direction, P10.5, DX0.5, and P11 implementation may
proceed before the deferred external evidence arrives, but it remains
exploratory and cannot produce release-equivalent assurance. DX0.5 must pass
before P11 application-authoring work begins so the golden todo is developed
through the same compiler feedback loop intended for users and agents.

### Milestone D — falsification

The difficult order/payment application and a strong TypeScript baseline test
whether the approach deserves further investment.

The candidate comparison protocol and minimum editor, language-server,
formatter, renderer, and agent-context baseline described by the developer
tooling workstream receive dated digests before implementation continues. P12
reference implementation may proceed under the review deferral; final trials
execute the externally reviewed frozen protocol without redefining it in
response to results.

Milestone D is phase P12.

## 2. Status vocabulary

| Status        | Meaning                                                      |
| ------------- | ------------------------------------------------------------ |
| `not started` | No implementation evidence exists beyond planning.           |
| `in progress` | At least one deliverable exists, but the exit gate is unmet. |
| `deferred`    | An explicit owner decision postponed required evidence; the exit gate remains unmet. |
| `blocked`     | Work cannot continue without a named decision or dependency. |
| `complete`    | Every exit-gate item is satisfied and linked below.          |

Percentages are intentionally avoided. They imply precision without proving
which semantic risks remain.

## 3. Progress summary

| Phase | Deliverable                               | Status      | Evidence or next action                                                                                                                                                                 |
| ----- | ----------------------------------------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P0    | Design foundation                         | complete    | Charter, semantic model, type system, failure model, acceptance cases                                                                                                                   |
| P1    | Jadpo seed and core grammar            | complete    | [Core grammar](grammar-v0.1.md), [seed application](../examples/jadpo-seed/app.jadpo), [expected semantics](../examples/jadpo-seed/expected.md), [issue log](language-issues.md) |
| P2    | Executable compiler fixtures              | complete    | [One hundred and eighty-seven source/expectation pairs](../tests/compile/README.md) cover syntax, names, types, failures, disclosure, effects, typed CRUD, patches, relationships, modules, local mutation, enums, matching, operators, authored tests, callable/outcome semantics, entity/query/transaction, configuration, authentication, Temporal/testing, naming, and policy |
| P3    | Rust workspace and CLI                    | complete    | [Rust workspace](../jadpo/README.md), deterministic scaffold manifest, discovery tests, stable build-stage diagnostics                                                               |
| P4    | Lexer, parser, and syntax tree            | complete    | Span-preserving lexer and AST, recovering parser, exact seed outline, fixture coverage, real `jadpo check` syntax pass                                                               |
| P5    | Declaration index and semantic graph      | complete    | Deterministic IDs, resolved declaration references, field/refinement nodes, callable edges, semantic JSON manifest                                                                       |
| P6    | Nominal type and constraint checker       | complete    | Expression typing, nominal compatibility, validated constructors, nested structured-field selection, nullable/optional records, invariant collections, stable fixture diagnostics      |
| P7    | Failure and effect checker                | complete    | Closed failure propagation, typed rejection context, derived route status, disclosure contracts, effect checks; Milestone A                                                            |
| P8    | Derived artifacts and layout boundaries   | complete    | Eleven byte-stable artifact files, including entity and transaction audits, explicit discovery/output boundary, generated diagnostic catalogue/reference, three layout candidates, and deterministic static scaffold |
| P9    | TypeScript/Bun target and runtime         | complete    | Dependency-free generated Bun target and six real HTTP acceptance cases; Milestone B                                                                                                   |
| P10   | Postgres and persistence constructs       | complete    | Typed CRUD, inferred transactions, foreign keys, bounded repeated relationship loads, atomic multi-field updates, named compound uniqueness, and precise unique-conflict mappings pass SQLite and live PostgreSQL exit suites            |
| P10R  | Assurance and validation reset            | deferred    | Candidate packages exist; outside review and five first-user sessions are deferred until feature-complete implementation, not passed                                                    |
| P10.5 | Pre-P11 language completion               | complete    | Exploratory under P10R deferral; patches, bounded relationships, migration review, index acceptance, bounded modules/imports, and immutable-value/scoped-local-rebinding semantics are implemented; advanced extensions remain deferred |
| DX0.5 | Checked local development loop             | complete    | JSON plus source-rendered diagnostics, coalesced atomic watch, compiler-owned HTTP health, structured runtime faults, last-known-good startup rollback, portable structured shutdown, and edit/recovery protocol tests |
| P10.6 | Problems, routes, and entity/type/persistence boundary | unattended DATA-007 and policy cores complete; selected route success semantics implemented | First-class entities, optional persistence, `.Ref` types, receiver kinds, entity operations, named queries with freshness, mutation ownership, recognised project roles, authority/derived-representation declarations, entity audit output, and POLICY-001 syntax/enforcement are implemented. `success: created` and `success: no_content` now enforce their response-body contracts and generate 201/204 responses plus matching inventory/OpenAPI evidence. Query/header bindings, lifecycle syntax, compound-persistence spelling, and physical multi-store adapters remain parked behind their owning contracts. |
| P10.7 | Callable execution and outcome matching       | in progress; unattended local TX core complete | The call/outcome/IDE work remains implemented. Explicit atomic-intent diagnostics, independently failure-atomic entity actions, same-domain enforcement, guarded value receivers, nested transaction joining/savepoints, audited isolation/locking/retry plans, and authority-plus-change-record atomicity have compiler and runtime evidence. SQLite and disposable PostgreSQL pass handled nested-savepoint rollback plus 24 concurrent same-entity updates with isolated operation timestamps and contiguous change revisions. PostgreSQL also passes this case with writers in two separate processes. A separate SQLite multi-process probe returned `SQLITE_BUSY` at transaction start; default retry/failure handling is pending the TX-001 contract choice. Broader retry evidence and physical derived-store delivery remain outstanding; durable-workflow target generation fails closed. |
| DX1   | Compiler-backed language service            | complete; formatter conformance follow-up open | Standard `jadpo lsp`, live unsaved diagnostics, symbols, cross-file definitions/references, hover, contextual completion, signature help, semantic tokens, rename, formatting, explicit generated-artifact commands, and a persistent VS Code client pass protocol tests. TOOL-005 maps all 122 grammar productions to formatting rules and exact-output fixtures. Unit tests compare all 58 compile-pass snapshots after whitespace perturbations; a focused exact-output case covers alternatives absent from those fixtures: `restrict`, both inverse `many` spellings, strategy-free `projection`, compatibility `type Name = Enum { ... }`, all freshness values, descending ordered pagination, `else`, negative numbers, escaped strings, `pattern`/`format` constraints, nullable `default none`, integer/decimal/boolean config defaults, and optional `previous_secret`/`origin` settings. The 58 pass and 129 fail fixtures receive integration checks for token/comment/string preservation, parseability, diagnostic stability, and idempotence. The matrix-total test checks every named grammar production has an exact-output snapshot link. Only removed nonblank-line-break exactness remains gated: the roadmap requests line-break-insensitive canonical output while FMT-005 currently preserves nonblank source breaks. |
| DX2   | Guided diagnostics and agent context        | internal implementation complete; external trials pending | The corrected 480-code inventory includes configuration, policy and test-runner families, with authored copy and audience projection tests. One hundred and eighty-seven compile pairs plus focused Rust scenarios provide bounded trigger evidence. Syntax-aware catalogue references exclude comments and unrelated helper code; this index does not prove assertion execution or exhaustive trigger coverage. Fresh-agent and first-user repair-cycle trials remain separate evidence. |
| VAL-001 | Comprehensive validation | supported local checks pass; release evidence open | The latest 2026-10-01 [`python3 tools/verify.py` report](../build/validation/20261001T180732-46495/report.json) records all 47 steps passed across compiler/build fixtures, editor, authored/runtime suites, and seven PostgreSQL modes, including P10.7 savepoint and same-/cross-process concurrency scenarios. It verifies 187 compile-fixture pairs, formatter snapshots and whitespace cases, generated-artifact navigation tests, and the Shiki/Prism/Highlight.js/Monaco adapter registration checks. The parser property test checks at least 3,000 deterministic token mutations sampled across accepted and rejected compile fixtures; it also found and fixed an inverted diagnostic range in malformed nested calls. The result is not release-equivalent: the frozen golden candidate remains at 57 diagnostics with behavioural evidence unrun. Earlier evidence includes independent review, ten targeted implementation mutations across the programme, and a frozen internal cold-start pilot that passed 12/12 obligations; none replaces formal external studies. The [ledger](../tests/validation/unattended-progress.md) records fixes and limits. Golden compilation/behaviour, Set/Map wire semantics, compatibility/deprecation, sustained fuzzing, broader campaigns and external comprehension/approval evidence remain open. |
| WASM-EXP1 | Bounded Wasm runtime experiment | complete, including bounded performance follow-ups; Bun retained | Both compiler routes passed the initial local/Cloudflare probe; later Rust candidates have their own local evidence. Latest immutable-row candidate adds about 3% throughput / 2% p95 / 3% CPU improvement versus row transport, but fails large-read Bun parity. SQLite diagnostics and WAL/FULL controls greatly improve writes on both targets; Wasm leads the generated Bun baseline on the tested WAL writes, using different SQL/host machinery. Controlled storage/recovery, broader coverage and hosted qualification remain open. See [latest report](wasm-typed-values-results.md). |
| P11   | Authentication, policy, and golden todo   | browser/API and service/JWT runtime checkpoints verified; golden source migration in progress | The 2026-09-30 checkpoint reconciles the five recorded golden authentication/query-accounting discrepancies without changing policy; all 44 golden cases remain unexecuted. The accepted `created`/`no_content` route modes now remove three of the golden source's 60 baseline diagnostics; 57 remain. The separate `examples/golden-todo-migration` package checks eleven source files and 47 declarations: four persistent entity dossiers, create/patch/disable and output contracts, direct Todo owner and identity-bound User self-role bindings, `Todo.by_id` -> `Todo.get` -> authenticated `GET /todos/{todo_id}`, and protected `POST /todos`/`PATCH /todos/{todo_id}` routes. The PATCH action preserves empty-patch rejection, future due-date validation, due-date-triggered reminder reset, and an authoritative `by_id` preflight before the entity-scoped update; integrated execution and lifecycle race behavior remain unproved. The API bearer also declares a service API-key validator, explicit `ServiceCredential.service_id` owner mapping, and active `ServiceCredential.verifier` resolution. Protected routes narrow `current_principal.user`/`.service`, with runtime 403 enforcement for variant mismatch; the service-auth route suite also verifies a service-key success and user-principal 403 on a service-only route. Creation uses generated UUID identity, future-only due dates, initial status, conflict mapping, and projects identity/lifecycle timestamps. Focused compiler checks reject caller-supplied owner values and public routes that consume principal-dependent operations. The query build audit proves the owner-read obligation, while full target generation still fails closed on the selected auth adapter/configuration/principal combination. The package also has typed config, bounded principal, signed/JWT user-authentication declarations, active-user resolution, and static public liveness. `CreateTodo.due_at` preserves GF-013A with a checked nullable `default none`; its route input validator fills omitted values before application code runs. DATA/query/transaction and CONFIG foundations are executable. Authentication has closed typed principals, reachability enforcement, exact selector behavior, and verified signed/opaque cookie and bearer, service-key, and pinned-JWT runtime paths. Policy has scoped direct/membership roles, automatic query/mutation predicates, field/validation composition, route/invoke handling, service-principal enforcement, audit, and multi-company SQLite evidence. Temporal/testing provides the runtime, clocks, deadlines, typed fixtures, and callable reports. Authored service credential issuance/exchange endpoints, User disablement and soft-delete lifecycle, broader protected target generation/runtime and auth adapter coverage, remaining named-query integration, full source integration, integrated behavior, SERVICE/ASYNC job and mail contracts, protected approval, P10R evidence, and broader PostgreSQL coverage remain open. The latest full local run passed 47 steps and seven PostgreSQL modes; service authentication passed 15 tests/213 expectations, and the 187-pair fixture corpus passed. |
| P12   | Order/payment application and TS baseline | not started | Use the order/payment application to pressure-test WORKFLOW-001 multi-authority state, idempotency, compensation, reconciliation, and outcome uncertainty. External sessions and protocol freeze precede final comparative trials and the continuation decision. |

The bounded module core now parses explicit logical module headers and selective
imports, enforces private-by-default visibility, and rejects dependency cycles.
Advanced namespaces stay deferred until a golden-application split demonstrates
their need.

## 4. Phase details

### P0 — Design foundation

**Objective:** establish enough semantic authority to avoid inventing the
language inside compiler code.

**Deliverables:**

- [language charter](charter.md);
- [semantic model](semantic-model.md);
- [type system](type-system.md);
- [failure model](failure-model.md);
- [type-system acceptance cases](../examples/type-system-cases.md);
- [failure-model acceptance cases](../examples/failure-model-cases.md);
- [decision register](decision-register.md).

**Exit gate:** type compatibility, validated construction, failure
classification, disclosure, and authority order are documented without relying
on the source conversation.

**Status:** complete.

### P1 — Jadpo seed and core grammar

**Objective:** turn the broad language proposal into one small compiler contract
that can be implemented without guessing.

**Deliverables:**

- a canonical [Jadpo seed application](../examples/jadpo-seed/app.jadpo);
- its [expected semantic facts](../examples/jadpo-seed/expected.md);
- a frozen-for-implementation [core grammar](grammar-v0.1.md);
- a living [language issue log](language-issues.md);
- a list of constructs explicitly excluded from the core.

**Exit gate:** every token in the seed application is covered by the grammar;
every seed declaration has an expected semantic identity; unresolved broader
features are logged rather than represented with pseudocode.

**Status:** complete.

### P2 — Executable compiler fixtures

**Objective:** convert prose acceptance examples into files that can drive
implementation test-first.

**Initial fixture set:**

1. valid `Email` literal construction;
2. invalid constant email;
3. wrong identifier domain;
4. field-to-parent widening;
5. sibling field rejection;
6. raw primitive callable parameter rejection;
7. undeclared failure propagation;
8. automatic `NotFound` HTTP mapping;
9. internal failure context excluded from public output;
10. arbitrary exception rejection.

**Fixture contract:** each case contains source plus an expectation describing:

- compile success or diagnostic code;
- primary source span;
- expected/received semantic types where relevant;
- selected refinement or propagation path;
- generated boundary metadata where relevant.

**Exit gate:** the initial ten fixtures are machine-readable and can be consumed
by the future test harness without manually interpreting Markdown.

**Status:** complete.

### P3 — Rust workspace and CLI

**Objective:** create the smallest durable compiler shell.

**Planned layout:**

```text
jadpo/
    crates/
        syntax/
        semantic/
        diagnostics/
        core/
        cli/
runtime/
    typescript/
stdlib/
tests/
```

**Commands:**

```text
jadpo check <project>
jadpo inspect <project>
jadpo build <project>   // placeholder until P9
```

**Exit gate:** the workspace builds; the CLI discovers source files; fixture
tests can invoke it; unsupported commands fail with stable diagnostics.

**Evidence:**

- dependency-free [Rust workspace](../jadpo/Cargo.toml);
- compiler CLI and source discovery;
- ten fixtures and the Jadpo seed discovered by workspace tests;
- deterministic `inspect` scaffold manifest;
- stable unsupported-command and deferred-build diagnostics;
- all workspace unit and documentation tests passing.

**Status:** complete.

### P4 — Lexer, parser, and syntax tree

**Objective:** parse the core grammar while preserving useful source spans.

**Required work:**

- lexer and trivia handling;
- error-recovering parser;
- syntax tree and typed AST accessors;
- multiple diagnostics in one run;
- parser snapshots and malformed-source fixtures;
- deterministic formatting of diagnostics.

**Exit gate:** the seed parses without diagnostics, every initial fixture
reaches the intended syntax node, and malformed source recovers far enough to
report more than the first error.

**Evidence:**

- dependency-free lexer with tokens for the complete core vocabulary;
- retained whitespace and line-comment trivia;
- UTF-8-safe byte spans that slice original source;
- route-path, string, integer, decimal, keyword, and punctuation tokens;
- recoverable invalid-escape, unterminated-string, and unexpected-character
  diagnostics;
- Jadpo seed and all ten initial fixtures lex without diagnostics;
- recovering recursive-descent parser and span-preserving typed AST;
- exact declaration-outline snapshot for the Jadpo seed;
- all ten initial fixtures reach their intended syntax, including the stable
  `SYN_UNSUPPORTED_THROW` node and diagnostic;
- malformed source reports multiple syntax errors while preserving both later
  declarations;
- `jadpo check` now runs discovery, lexing, and parsing and reports aggregate
  syntax failure without discarding specific diagnostics;
- fourteen Rust unit tests plus all documentation tests passing.

**Status:** complete.

### P4 conformance follow-up — Identifier and reserved-word rules (NAME-002)

The grammar already defines identifiers as ASCII letters or `_` followed by
ASCII letters, digits, or `_`; a digit cannot start an identifier. The accepted
naming contract also sets `UpperCamelCase` for type-like names and
`lower_snake_case` for runtime names. NAME-002 makes the remaining boundaries
explicit and independently testable without silently changing those rules.

**Required work:**

- inventory every keyword and classify it as hard-reserved or contextual;
- record which declaration/reference positions may use contextual words and
  which positions reject hard-reserved words, including `input`, `output`, and
  `value`;
- specify identifier lookup case sensitivity and how case-only mismatches are
  diagnosed alongside the existing naming-style checks;
- confirm the ASCII character policy, first/continuation character rules,
  digit continuations, standalone/leading/repeated/trailing underscores,
  leading digits, non-ASCII characters, and token boundaries;
- add table-driven valid/invalid source cases for variables, parameters,
  fields, types, variants, modules, imports, declarations, and references; and
- verify stable source spans and diagnostics through compiler and language
  service surfaces.

**Acceptance:** every reserved/contextual word and identifier-boundary rule has
an explicit specification entry plus positive and negative executable cases.
Longer names containing a keyword, contextual name positions, case-only
reference differences, leading digits versus valid digit continuations, each
underscore boundary, Unicode, and invalid punctuation are covered. Any change
beyond the current grammar or accepted naming contract requires an explicit
reviewed decision first.

**Status:** queued; NAME-001's existing naming and ownership rules remain in
force until this conformance follow-up is completed.

### P5 — Declaration index and semantic graph

**Objective:** establish semantic identity before checking behaviour.

**Required nodes and edges:**

- named types and record declarations;
- field types and `refines` edges;
- functions, actions, failures, and routes;
- callable dependencies;
- failure-kind relationships;
- source spans and deterministic semantic IDs.

The initial project uses one application-wide namespace. Modules and imports
remain deferred.

**Exit gate:** `jadpo inspect` emits a deterministic manifest for the seed,
including every expected node and refinement edge in its expectation document.

**Evidence:**

- application-wide declaration index plus explicit prelude nodes;
- deterministic semantic IDs derived from sorted semantic names;
- addressable nodes for ordinary fields and scoped public/internal failure fields;
- resolution of type, field-type, callable signature, failure-kind, `fails`, and
  route references;
- stable diagnostics for duplicates, unknown names, wrong name kinds, unknown
  callees, and non-callable invocations;
- refinement edges for named types and every nominal field type;
- callable dependency edge from the seed route to its action;
- `jadpo inspect` emits a deterministic semantic JSON manifest containing
  nodes, source spans, refinements, and calls;
- the seed contract test proves all fourteen expected authored nodes and all
  eight expected refinement edges;
- every syntax-valid initial fixture indexes without a name-resolution error;
- eighteen Rust unit tests plus all documentation tests passing.

**Status:** complete.

### P6 — Nominal type and constraint checker

**Objective:** implement the distinctive type rules before broad language
features.

**Required checks:**

- nominal named types;
- field refinements;
- exact compatibility and widening;
- no implicit narrowing or sibling substitution;
- no primitive substitution;
- complete record construction;
- `T?`, `none`, and input omission;
- validated `Type(value)` construction;
- compile-time literal constraints;
- primitive prohibition in application callable signatures;
- initial invariant collection behaviour when collections enter the core.

**Exit gate:** the initial type fixtures pass with stable diagnostic codes and
spans; the compiler never delegates semantic compatibility to TypeScript.

**Evidence:**

- local expression environments and nominal field selection;
- exact compatibility plus widening only along declared refinement paths;
- nested selection through a structured field resolves via its declared named
  record and preserves that record's nested field identities;
- rejection of implicit narrowing, sibling substitution, and semantic-to-
  primitive unwrapping;
- primitive application-signature diagnostics covering the whole annotation;
- validated `Type(value)` construction with compile-time `format`, length,
  numeric, and core-pattern checks for literals;
- complete record construction, unknown-field checks, optional omission,
  nullable field identity, and `none` handling;
- invariant generic arguments for the initial collection model;
- typed public/internal failure context values;
- stable codes, exact primary spans, and inferred-type assertions across the
  type-focused fixtures;
- the seed type-checks without diagnostics.

**Status:** complete.

### P7 — Failure and effect checker

**Objective:** make expected failure flow closed and mechanically visible.

**Required checks:**

- standard failure kinds;
- domain failure declarations and stable codes;
- `reject` payload validation;
- `fails` sets and transitive propagation;
- route-reachable failure derivation;
- public versus internal disclosure schemas;
- rejection of arbitrary `throw`, strings, and numeric status mapping;
- semantic propagation paths;
- initial effect distinction between functions and actions.

`attempt` remains deferred until direct rejection and propagation are solid.

**Exit gate:** core failure fixtures pass; the seed's route failure response is
derived rather than handwritten; Milestone A is complete.

**Evidence:**

- complete initial standard-kind catalogue and deterministic HTTP defaults;
- domain failure contracts with stable code, safe message, and separate public
  and internal field inventories;
- duplicate public-code rejection;
- required, unknown, duplicate, and typed rejection-context validation;
- direct and transitive `fails` propagation checks;
- route-reachable failures and status codes derived from the invoked action;
- functions cannot reject or call actions;
- arbitrary `throw` remains a syntax-level error;
- the checked manifest includes inferred expression types, failure contracts,
  disclosure fields, and route failure mappings;
- all failure/effect fixtures and the seed pass their expected contracts.

**Status:** complete. Milestone A is complete.

### P8 — Derived artifacts

**Objective:** prove the semantic model is useful before producing executable
target code.

**Outputs:**

- `app.meta.json` semantic manifest;
- route and callable inventory;
- failure/disclosure audit;
- OpenAPI subset;
- validator plan;
- compatibility report for public failure codes;
- project-structure proposal and scaffold specimen;
- deterministic static-base scaffolding contract shared by interactive and
  non-interactive use.

**Exit gate:** generated artifacts accurately describe the seed and are stable
across repeated builds. Authored-source discovery and compiler-owned output
locations are explicit enough that P9 cannot accidentally establish them.
The same compiler version can reproduce the selected base scaffold without an
LLM making directory or naming decisions.

**Evidence:** [`jadpo artifacts`](../jadpo/README.md) emits the seven-file
[generated artifact contract](generated-artifacts.md); unit tests prove repeated
emission and relative/absolute invocation are byte-identical. Generated source
is excluded from discovery. The [structure workstream](project-structure.md)
records three candidate trees and selects the elastic single-application
prototype. `jadpo new` creates a valid five-file base, produces identical
bytes for identical names, and refuses to overwrite non-empty destinations.

**Status:** complete.

### P9 — TypeScript/Bun target and minimal runtime

**Objective:** complete one request from wire input to safe response.

**Runtime slice:**

- HTTP routing;
- request/correlation IDs;
- input decoding and generated validation;
- action invocation;
- output validation and exact serialisation;
- automatic failure mapping;
- generic fault containment;
- semantic source metadata.

The generated TypeScript is a disposable artifact, not normal developer-facing
source.

**Exit gate:** the seed route accepts valid input, rejects invalid input safely,
maps its domain failure automatically, never discloses internal context, and
passes an end-to-end HTTP test. Milestone B is complete.

**Evidence:** `jadpo build` emits the [generated Bun target](runtime-target-v0.1.md)
beneath `build/target/`. The [runtime acceptance
suite](../tests/runtime/jadpo-seed.test.ts) uses a real localhost listener to
prove success, malformed JSON, malformed semantic values, constrained values,
closed input shapes, derived 422 mapping, and non-disclosure. Bun bundles the
target with auto-install disabled. The compiler rejects bare package imports,
dependency manifests, lockfiles, and `node_modules`; only Bun built-ins and
compiler-owned relative modules are permitted. Authenticated routes fail
generation until their runtime exists.

**Status:** complete. Milestone B is complete.

### P10 — Postgres and persistence constructs

**Objective:** add the smallest honest persistence model.

**Order:**

1. entity schema metadata;
2. generated tables for a fresh database;
3. `create`;
4. required/optional/many query cardinality;
5. validated row decoding;
6. simple `update` and `delete`;
7. constraint-error normalisation;
8. entity identity, uniqueness, and indexes;
9. relationship declarations and generated foreign keys;
10. explicit relationship loading through joins or bounded batched queries;
11. inferred action transactions across multiple operations;
12. migration planning only after fresh-schema behaviour works.

#### P10 relationship and joined-loading slice

The golden pressure case is “load a user and all of that user's todo tasks.”
This is a product capability, not merely a database optimisation. The language
must describe the relationship and requested result shape so the compiler can
derive storage, queries, validation, documentation, and policy checks together.

The joined-load pressure case selected an explicit logical-name clause:
`owner_id: User.id references User.id as owner on_delete cascade`. This keeps
the storage field, nominal target, traversal name, and lifecycle action visible
in one declaration. Inferring `owner` from `_id` was rejected as convention
magic and a separate relationship declaration was rejected for duplicating the
target and ownership facts. Omitting `as` keeps the field name as the logical
name for existing concise cases. Composite-reference syntax remains a later
migration/design pressure test rather than a hidden assumption here.

**Required relationship semantics:**

- explicit one-to-one, many-to-one, and one-to-many declarations with named
  ownership direction; many-to-many starts with an explicit join entity rather
  than an invisible table;
- nominal reference typing, so a `Todo.owner_id` relationship to `User.id`
  cannot be populated with an unrelated UUID-shaped field;
- generated foreign keys and indexes for both SQLite and PostgreSQL, with
  explicit `restrict`, `cascade`, or nullable-reference behaviour on deletion;
- required versus optional parent references and the corresponding orphan
  rules;
- relationship cycles and maximum eager-load depth diagnosed rather than
  recursively expanded without bound.

**Required loading semantics:**

- an explicit load/include selection at the query site; ordinary entity access
  never performs hidden lazy queries or creates an N+1 path;
- typed nested results such as a complete `User` plus `List<Todo>`, with zero
  children represented by an empty list rather than a missing parent;
- deterministic child ordering and explicit pagination before a to-many load
  can be accepted;
- parent pagination applied to parents, not flattened join rows;
- compiler choice between a safe join and a bounded batched query when joining
  multiple to-many relationships would cause Cartesian multiplication;
- validation and nominal decoding of every parent and child row before the
  nested result becomes trusted application data;
- derived query-plan metadata showing joins, batch boundaries, predicates,
  ordering, cardinality, and selected fields.

**Security boundary:** relationship traversal must not bypass ownership or
policy. P10 establishes the typed query and storage plan; P11 must prove that
parent and child policy filters compose and that an inaccessible child cannot
leak through an otherwise accessible parent.

**Executable exit evidence:** fixtures and SQLite/PostgreSQL runtime tests cover
zero, one, and many todos for one user; multiple users without cross-user
mixing; missing/invalid parents; foreign-key rejection; deterministic ordering;
parent pagination; rollback-safe lifecycle behaviour; and equivalent nested
JSON on both adapters. A query-count assertion proves the generated path is
bounded and does not regress to N+1 execution.

**Exit gate:** typed CRUD works against Postgres and raw driver errors never
cross the adapter boundary.

**Current evidence:** the [persistence v0.1 slice](persistence-v0.1.md) adds
typed create/read/update/delete expressions, rejects persistence operations from
functions, emits matching Postgres/SQLite fresh schemas and parameterised
statements, validates database rows before trust, normalises raw driver errors,
and infers one transaction for every transitively mutative action. That records
the executable prototype's P10 evidence; TX-001 supersedes silent widening for
the accepted entity/workflow model without erasing the historical test result.
Nested mutative calls reuse the scoped adapter, read-only actions avoid write
transactions, and required-mutation cardinality checks remain inside the same
boundary. Fixed-shape multi-field updates are nominally checked and lower to
one parameterised statement. Named compound uniqueness is checked and emitted
for both adapters; identity, single-field unique, and compound-unique failures
normalise to compiler-owned identities before selecting typed conflict
bindings. It also checks named inverse collections and
lowers repeated required-parent includes to one parent query plus independently
bounded children, while many-parent includes use independent parent-page joins
merged by a deterministic key. Exact query counts and SQL are exposed in
persistence metadata. The SQLite and PostgreSQL
runtime tests prove successful CRUD, zero/multiple-row handling, typed
not-found/conflict mapping, rollback, driver-fault behaviour, foreign-key
lifecycle, and zero/one/many nested child results without cross-parent mixing.
Relationship paths beyond the bounded depth-two slice and required inverse-one
semantics, precise foreign-key/check conflict mapping, and advanced
isolation/external-effect boundaries remain deferred beyond the bounded P10.5
core. A clean Postgres 16 run passes all 22 live adapter tests,
including inferred rollback and precise single/compound constraint mappings;
raw driver errors remain contained behind `PersistenceFault`.

**Status:** complete.

### P10.5 — Pre-P11 language completion

**Objective:** settle the language surfaces that the complete golden todo must
exercise before authentication and policy implementation begins: owning-parent
and nested relationship traversal, one-to-one ergonomics, relationship declaration
syntax, migration identity, omission-aware patches, authored imports/modules,
and value/reference/mutation semantics. The original evidence order required a
P10R freeze before this pass; under the explicit owner deferral, this work is
exploratory until the later external evidence and freeze are complete.

**Status:** complete as exploratory implementation under the explicit P10R
review deferral. Direct optional-field patch inputs preserve omission separately
from explicit `none`, reject empty patches, and lower to one finite supplied-flag
SQL shape on SQLite and PostgreSQL. Fixed derived writes may run unconditionally
or only when a named patch field was supplied, closing GF-013B without dynamic
SQL. Required-child queries can also traverse one owning reference with explicit
`required` or `optional` cardinality through a bounded two-query plan; nullable
references preserve `none`. Unique-backed optional inverse declarations also
load through a bounded parent-plus-child plan and preserve `none` when no child
exists. This status does not authorise P11 assurance claims.
The first nested slice composes a non-nullable owning reference with an optional
inverse and fixes both maximum depth and query count in generated metadata.
The subsequent shape review rejects required inverse-one claims until totality
is enforceable (or absence is an explicit domain path), defers nested to-many
until every collection hop is bounded, and keeps many-to-many as an explicit
join entity. The provisional migration-identity contract is documented in
[migration identity v0.1](migration-identity-v0.1.md).
The bounded module core and value boundary are also implemented: imports are
selective and visibility-checked, ordinary parameters and returned data are
immutable values, and only type-compatible lexical `var mut` locals can be rebound.
No field/caller mutation or authored reference notation is accepted. More
powerful namespace or `inout`-style features require new application evidence.

The parallel DX0.5 developer-tooling slice is an exit prerequisite for P11
application authoring. It does not define language semantics, but it must expose
the authoritative compiler loop used to build the golden todo: structured JSON
diagnostics, watched checking/building, atomic generated-output promotion, and
a live Bun process that advances only to a successful build. Its detailed gate
is defined in the [developer tooling workstream](developer-tooling.md).

### P10R — Assurance and validation reset

**Objective:** restore a fair evidence sequence before implementation choices
become irreversible, and turn the central assurance claim into a bounded model
that can fail.

This phase does not add runtime capability. It freezes the contracts against
which P11 and P12 will be judged.

#### P10R.1 — Complete golden todo design contract

Write the complete canonical todo application as design evidence, not as
pseudocode tailored to current compiler support. The frozen package must include:

- canonical source for all behaviour required by the validation plan;
- black-box acceptance tests and adversarial change requests;
- intended policy, lifecycle decisions, configuration, service contract, job,
  authored business tests, and expected derived audit;
- expected compiler decisions, including explicit `cannot prove`,
  human-decision, and unsupported-capability outcomes;
- a ledger of every construct the current language cannot yet express; and
- a strong TypeScript baseline specification naming its framework, schema,
  persistence, policy, static-analysis, test, and agent-context facilities.

The design may expose necessary language changes, but those changes enter the
issue log and acceptance fixtures before implementation. The existing compiler
must not be used to silently reduce the application to what it already supports.

#### P10R.2 — Policy and proof kernel

Create a small normative policy/proof specification before implementing policy.
It must define:

- the supported principal, qualified scoped-role, membership/direct-binding,
  entity-effect, field, route, public-access, and external-effect facts while
  keeping lifecycle/business predicates separate;
- the exact proof obligations created by every query, mutation, projection,
  route, relationship traversal, service call, and policy weakening;
- the facts that may be introduced by authentication, validated input, guarded
  queries, trusted configuration, and prior proof steps;
- composition rules across calls, nested queries, relationships, and actions;
- the difference between `proved`, `rejected`, `requires human decision`,
  `unsupported`, and `cannot prove`;
- the conservative behaviour for ambiguity: absence of a proof never becomes
  permission;
- soundness claims made by the project and claims explicitly not made; and
- minimal positive, negative, and indeterminate fixtures for every rule.

The phrase “compiler proof” may only be used for a property tied to a named rule
and fixture. Other properties must be described as validation, generation,
testing, convention, or residual risk as appropriate.

#### P10R.3 — Threat model and trusted computing base

Create one authoritative threat model covering accidental and adversarial agent
changes, ambiguous human requests, hostile boundary data, cross-tenant access,
secret disclosure, undeclared egress, dependency/provider failure, generated
target defects, CI bypass, and misuse of escape hatches.

For every advertised assurance property, record:

- the protected asset and relevant attacker or failure source;
- the compiler/runtime boundary that prevents or detects it;
- the components trusted for the claim, including compiler, generated runtime,
  database constraints, policy store, approval mechanism, CI, deployment
  configuration, runtime built-ins, and external contracts;
- whether enforcement is static, runtime, generated-test, operational, or
  human-review based;
- known bypasses, escape hatches, and residual risk; and
- executable or review evidence required before the claim may appear in product
  documentation.

The threat model must distinguish a deterministic decision from a correct or
sound decision. It must not describe the compiler/runtime itself as outside the
trusted computing base.

#### P10R.4 — Human approval protocol

Specify an approval mechanism that an implementation agent cannot satisfy by
editing ordinary repository files. At minimum, a releasable policy weakening
must produce an approval record bound to:

- a canonical digest of the exact policy and affected semantic graph;
- the behavioural/policy diff presented to the reviewer;
- authenticated reviewer identity and authority;
- the decision, rationale, and expiry or supersession behaviour; and
- the compiler and policy-schema version used to interpret it.

CI must reject missing, stale, self-issued, replayed, or scope-mismatched
approval. Local development may inspect an unapproved change, but it may not
produce a release-equivalent success. Acceptance cases must include an agent
editing source and attempting to manufacture or reuse approval.

The first implementation may use a protected CI/review-system attestation rather
than cryptography invented by this project. The security property is separation
of authority, not a bespoke signature format.

This work must not assume that conventional pull-request review is an effective
human assurance mechanism. The project begins from the observation that code
review is increasingly skipped or superficial because generated changes are
large, reviewers lack the originating context, behaviour is distributed across
files and framework layers, plausible-looking code is difficult to distrust,
and defects may hide in indirect effects, omitted cases, generated behaviour,
configuration, or interactions between otherwise reasonable changes.

P10R must investigate and record why reviewers fail to understand or challenge
agent-generated changes, including at least:

- missing product intent, constraints, and rejected alternatives;
- diff volume and plausible-looking implementation detail overwhelming the
  meaningful decision;
- behaviour and invariants distributed across routes, actions, policy,
  persistence, jobs, services, configuration, and generated artifacts;
- indirect reads, writes, external calls, retries, lifecycle changes, and other
  side effects that are not obvious from the edited lines;
- reviewers being unable to distinguish compiler-proved facts, tested claims,
  agent assertions, assumptions, and unresolved uncertainty;
- inadequate time, unclear ownership, approval fatigue, social pressure to
  unblock a change, and habituation to mostly-correct generated output; and
- interfaces that ask a human to approve an entire change when only a small
  number of decisions genuinely require human judgement.

The approval surface should therefore be a compiler-derived behavioural and
decision review, not a shortened code diff. For each requested human decision,
it must present, in domain language:

- the original request and the decision requiring approval;
- the current behaviour, proposed behaviour, and reason for the change;
- affected actors, routes, entities, fields, policies, lifecycles, services,
  secrets, jobs, and deployment/configuration surfaces;
- direct and transitive reads, writes, emissions, external effects, transaction
  boundaries, retry/idempotency consequences, and failure/disclosure changes;
- the relevant relationship path through the semantic graph rather than a list
  of disconnected files;
- which claims are statically proved, runtime validated, test-supported,
  operationally enforced, assumed, unsupported, or still uncertain;
- counterexamples and adversarial cases considered, including what newly becomes
  possible; and
- the narrowest available choices, their consequences, and the exact policy
  delta each choice authorises.

Source and generated-code views remain available for investigation, but approval
must not depend on a reviewer reconstructing the behaviour from them. The review
UI must support drilling from the behavioural summary to the responsible source,
proof obligation, test, or runtime boundary without losing the decision context.

The intended product includes a custom web-based UI, so the review experience is
not constrained by pull-request conventions or a static linear diff. Treat that
as a core product opportunity rather than a presentation detail. The UI should
be able to test richer review models such as:

- an intent-first change narrative that progressively reveals implementation
  evidence;
- interactive before/after views of routes, policies, data access, lifecycle,
  failures, configuration, and external effects;
- explorable semantic-graph paths showing why a declaration or decision affects
  apparently distant behaviour;
- filters for newly public surfaces, weakened protections, destructive changes,
  secret access, external egress, transaction changes, and escape hatches;
- actor- and scenario-based simulation answering questions such as “what can
  this user do now that they could not do before?”;
- direct comparison of accepted, rejected, and alternative policy choices with
  their downstream consequences;
- focused approval of individual human-owned decisions instead of one blanket
  approval for an entire implementation; and
- stable links from every claim to its proof rule, boundary validation, test,
  runtime evidence, source declaration, and residual uncertainty.

The web UI must consume versioned compiler/semantic artifacts rather than infer
meaning independently or become a second source of language semantics. Its
interactive presentation may evolve quickly, but every displayed assurance
claim must remain reproducible in machine-readable evidence and in a usable
non-graphical export for CI, accessibility, and archival review.

The research must compare at least a normal pull-request diff, a concise
behavioural diff, and the proposed relationship/effect-aware approval view.
Measure comprehension accuracy, important risks noticed, false confidence,
review time, requests for clarification, and approval/rejection quality. Include
changes whose individual lines look reasonable but whose transitive behaviour is
unsafe. If reviewers cannot reliably understand the decision from the proposed
surface, the approval protocol has failed even when its identity and attestation
mechanics are secure.

##### Backlog — Focused semantic change graphs in the review UI

**Status:** backlog; added 2026-09-30. Part of the P10R.4 review UI work.
Inspired by [Revset's graph-based review](https://revset.dev/); implement within
Jadpo using compiler artifacts, with no Revset dependency.

Prototype a behavioural decision view with an explorable before/after graph
for each meaningful change. Start with one permission or data-disclosure change,
such as allowing support staff to export customer email addresses. Lead with
who can now do what, to which data, and the decision required; expand into the
relevant actor → route → query/action → field → output or external-effect path.

Scope:

- Show changed relationships as well as declarations, including unchanged
  declarations newly reachable through a changed route or permission.
- Present a focused impact path per decision, expandable into surrounding
  context, rather than requiring navigation of a whole-application graph.
- Compare current/proposed behaviour and successive agent revisions; distinguish
  unchanged review content from approval validity under the exact subject digest.
- Link nodes and relationships to source, proof obligations, tests, runtime
  boundaries, and explicitly classified uncertainty.
- Include scenarios and contract/body changes whose behaviour differs despite
  unchanged graph structure; a graph alone cannot establish correctness.
- Consume versioned compiler-produced before/after artifacts and retain an
  equivalent non-graphical export. Identify unsupported analysis explicitly.

Acceptance evidence: compare the focused graph view with a concise behavioural
review using the existing [comprehension study](comprehension-study.md). Measure
whether reviewers correctly identify affected actors, data scope, transitive
effects, and the reason access changes, alongside review time and false
confidence. Include a case with changed behaviour but unchanged graph structure.
This prototype does not itself complete P10R or validate the approval protocol.

#### P10R.5 — First-user and adoption hypothesis

Document one initial research user without narrowing the language's intended
technical scope. The current candidate is:

> An experienced product engineering team using coding agents for substantial
> greenfield or newly isolated SaaS-backend work, where a human remains
> accountable for security and policy but cannot economically review all
> generated implementation code.

Define the accountable human, daily user, adoption trigger, existing
alternative, acceptable switching cost, required interoperability, trust
objections, and the smallest credible adoption path. The first path should be a
new or isolated service with generated HTTP/database boundaries, not a mandatory
rewrite of an existing system.

Interview or structured-review evidence from at least five people matching the
candidate profile must test whether the stated pain, review surface, audit, and
approval flow are valuable. These interviews are product evidence, not proof of
compiler safety. If another user profile proves stronger, record the change and
its reasons before P12 rather than silently broadening the audience.

#### P10R.6 — Preregistered comparison protocol and thresholds

Give the candidate protocol a dated digest before P11 implementation begins.
External review and the final freeze occur after feature-complete implementation
and before comparative trials. Any review-driven revision must preserve and
report the original digest and the implementation exposure to it. The protocol
must include a
severity rubric, adjudication rules, raw record format, randomised or
counterbalanced run order, clean checkpoints, and at least three independent
runs per stack. Use the same model/settings for the primary comparison; any
cross-model results are reported separately. An evaluator who did not implement
the relevant change classifies defects and comprehension answers from frozen
rubrics while blinded to the stack where the artifact permits it.

The initial continuation thresholds are:

1. **Critical safety:** no preregistered critical authentication, tenant,
   authorization, secret-disclosure, or destructive-lifecycle case may be
   silently accepted by the language. It must be rejected or require the named
   human decision before release-equivalent success.
2. **Safety advantage:** across preregistered high/critical adversarial cases,
   the language must prevent or escalate at least 80%, and must outperform the
   TypeScript baseline by at least 25 percentage points. Detection only by a
   generated test does not count as semantic prevention.
3. **Valid-work friction:** no more than 20% of ordinary non-adversarial changes
   may require a language/compiler change, unsupported-capability disposition,
   or escape hatch. Repeated compiler attempts count as friction even when the
   final program succeeds.
4. **Escape hatches:** no more than 10% of ordinary changes may require an escape
   hatch, and none may bypass a critical assurance property without the frozen
   approval protocol.
5. **Agent efficiency:** median time-to-correct-change and uncached token use may
   each be at most 25% worse than TypeScript. A safety win does not erase larger
   productivity costs; it requires an explicit continuation decision.
6. **Diagnostic quality:** at least 80% of first diagnostics must identify the
   correct violated obligation or missing decision, and the median valid repair
   must take no more than two compile/edit cycles.
7. **Comprehension:** independent reviewers and fresh agents must answer at least
   80% of frozen behaviour, policy, effect, and lifecycle questions correctly;
   the language/audit result must not trail TypeScript by more than 10 percentage
   points in any critical category.

These thresholds may be challenged once during P10R. After protocol freeze they
cannot be changed in response to results. Results between thresholds are
reported as mixed or inconclusive, not rounded into a win. Failure of thresholds
2–7 triggers an explicit continue/redesign/framework-pivot/stop decision;
failure of threshold 1 blocks the current assurance claim regardless of other
scores.

#### P10R.7 — Documentation authority and reading surface

Restructure documentation without deleting design history:

- a concise research brief owns the problem, first user, hypotheses,
  counter-hypothesis, and continuation rules;
- normative specifications contain only current semantics and use explicit
  `accepted`, `provisional`, or `open` markers at the relevant rule;
- the decision register owns decision status and links to one normative home
  rather than repeating full specifications;
- the roadmap owns sequence and evidence, never semantic authority;
- implementation evidence is separated from product-validation evidence; and
- conversation extraction and obsolete syntax move to an archive reading path,
  outside the normal onboarding sequence.

Add a short documentation consistency check to each phase exit: current status,
authority, named evidence, open questions, and links must agree across the
research brief, decision register, normative specification, and roadmap.

**P10R exit gate:** all seven packages are reviewed and frozen; the canonical
todo source and acceptance suite exist independently of compiler support; every
assurance claim maps to a named proof/validation rule and threat-model entry;
the approval protocol has adversarial acceptance cases; the first-user review is
recorded; comparison thresholds and adjudication are preregistered; and the
documentation index exposes one unambiguous current reading path.

**Status:** in progress.

**Candidate evidence (not yet independently reviewed or frozen):**

- [golden todo contract](../examples/golden-todo/README.md), including proposed
  source, protected policy, machine-readable black-box cases, a twenty-change
  adversarial sequence, expected audit, TypeScript baseline, and language-
  friction ledger;
- [policy and proof kernel](policy-proof-v0.1.md) with named rules, conservative
  outcomes, trusted fact sources, obligations, non-claims, and a minimal fixture
  matrix, now represented by 39 machine-readable positive/negative/indeterminate
  fixtures;
- [runtime validation rules](validation-rules-v0.1.md) and a machine-readable
  evidence map covering all 39 todo cases and all twenty threat entries;
- [threat model](threat-model.md) naming the trusted computing base, enforcement
  layers, evidence, bypasses, and residual risk;
- [approval protocol](approval-protocol.md) binding independent attestations to
  canonical policy/semantic digests and defining adversarial replay and
  comprehension cases, with twelve machine-readable attack fixtures;
- [research brief](research-brief.md) freezing the candidate first user and
  TypeScript counter-hypothesis, plus a
  [structured first-user review instrument](first-user-review-guide.md) and
  machine-readable record template; and
- [comparison protocol](comparison-protocol.md) defining clean checkpoints,
  counterbalancing, raw records, severity, adjudication, comprehension, and the
  continuation thresholds, plus a twelve-question, 43-point
  [comprehension instrument](comprehension-study.md) with a machine-readable
  scoring key.

The remaining gate is review evidence, not more unreviewed surface area: resolve
ambiguities found by independent contract review, freeze digests and fixtures,
conduct the five structured first-user reviews, and record the freeze decision.

### P10.6 — Recoverable problems, routes, and entity/type/persistence boundary revision

**Status:** failure-flow, route, prelude, artifact, target, and the bounded
DATA-007 entity/query foundation are implemented. The existing `type`/`persist`
prototype remains supported compatibility evidence. Entity dossiers, optional
persistence, `.Ref` types, receiver kinds, operations, named queries, mutation
ownership, project roles, authority/representation declarations, freshness,
and entity audits pass fixtures. Routes now support typed `created` and
`no_content` outcomes with response/result checks, generated 201/204 behavior,
and matching route-inventory/OpenAPI contracts. Query/header binding grammar,
operational mapping, compound-persistence spelling, lifecycle syntax, and
physical multi-store adapters remain separately parked.

**Why this phase exists:** first-user review of the implemented P7 and P10
surfaces found two related abstraction leaks before P11. The failure model
closes expected domain rejection but leaves recoverable operational conditions
outside the callable contract. The current compiler then tested one authored
`type` model plus a separate `persist` declaration. That implementation exposed
a broader question: whether a non-persistent entity should still be an
identity-bearing domain and capability boundary. DATA-007 answered yes and
fixed explicit operation, query, mutation-ownership, layout, and transaction
rules. P10.6 must now implement that accepted boundary before authentication
and policy make the prototype harder to replace.

#### Failure declaration and disclosure decisions

- Retain the term `failure`; it distinguishes a typed negative outcome from an
  uncatchable compiler/runtime defect.
- Replace the visually understated `failure Name: Kind` relationship with an
  explicit `kind: Kind` member in the failure body.
- Define each application failure once and reuse it across callables. Group
  declarations by domain rather than redefining failures per action.
- Retain an explicit stable public `code` and an optional static safe
  `message`. A source declaration may be renamed without silently changing the
  public code.
- Retain separate declaration-time `public` and `internal` context schemas.
  Public fields are part of the boundary contract; internal fields never enter
  a client response merely because a failure is produced.
- Flatten context values at production sites. `reject`, persistence mappings,
  and future adapters supply one object; the canonical declaration permanently
  determines which fields are public or internal.
- Reject overlapping public/internal field names so flattened construction is
  unambiguous.
- At every production site require all non-optional context fields, reject
  unknown and duplicate fields, resolve every value in lexical scope, and
  check its exact nominal type. Adding a required context field is therefore a
  checked change across every production site.

The intended declaration shape is:

```text
failure CustomerNotFound {
    kind: NotFound
    code: "customer_not_found"
    message: "Customer not found."

    public {
        customer_id: Customer.id
    }

    internal {
        lookup_id: Customer.id
    }
}
```

An intended production site is flat:

```text
missing: CustomerNotFound {
    customer_id: input.id
    lookup_id: input.id
}
```

#### Exhaustive recoverable-problem decisions

- Broaden `fails` from domain failures to the complete set of recoverable
  problems that a callable may propagate. This includes application-defined
  failures and normalized operational problems.
- For every callable expression, compute a success type and a closed set of
  recoverable problems. Every member of that set must be handled locally,
  mapped to another declared failure, or propagated in the enclosing
  callable's `fails` set.
- Apply this rule to functions, actions, jobs, handlers, and generated boundary
  operations. Purity remains separate: a function may handle a failure from a
  fallible pure call but still may not perform persistence or other effects.
- Use exactly two explicit acknowledgement forms. `attempt` unwraps the success
  value and visibly propagates the complete recoverable problem set. An
  exhaustive outcome `match` handles, maps, recovers from, or explicitly
  propagates each success and failure case; it replaces the earlier candidate
  `attempt` handler block.
- Require `attempt` or outcome `match` at every fallible expression. Require the
  authored `fails` clause to equal the exact reachable unhandled set: missing
  and stale extra entries both fail compilation, and compiler inference is
  diagnostic help rather than an implicit source edit.
- Make handling exhaustive after accounting for declared propagation. No
  failure or operational problem may disappear through a wildcard, implicit
  catch, or unchecked target exception.
- Reuse the language's existing exhaustive-match analysis and do not expose
  authored `Result<T, E>` plumbing through ordinary source. Exact success and
  failure arms supply ordinary compatible expressions, `reject`, or explicit
  `propagate`; no separate `recover` keyword is required.

The intended propagation shape is:

```text
action load_customer(id: Customer.id) -> Customer
    fails CustomerNotFound, Unavailable
{
    return attempt query required Customer {
        where: id == id
        missing: CustomerNotFound {
            customer_id: id
        }
    }
}
```

#### Operational problems and defects

- Expose a deliberately small, source-agnostic built-in operational vocabulary.
  The initial candidates are `Unavailable`, `TimedOut`, `RateLimited`, and
  `OutcomeUnknown`.
- Do not expose adapter, vendor, database, or provider names in those problem
  types. An action handles `Unavailable`, not `PrimaryStore.Unavailable` or a
  raw driver exception. The attempted operation provides source context to the
  reader; compiler/runtime metadata retains exact provenance for diagnostics,
  audit, and telemetry.
- Keep `OutcomeUnknown` distinct from `Unavailable`. In particular, a failed
  write may have taken effect even when acknowledgement was lost; fallback or
  retry is unsafe without idempotency or reconciliation evidence.
- Allow application logic to catch normalized operational problems and select
  a fallback, secondary capability, domain-specific translation, or early
  response. Any recoverable problem raised by that fallback is checked in the
  same way.
- Allow a generic operational problem to propagate directly, using a safe
  built-in boundary code, or be translated into a named application failure
  with its own `kind`, stable `code`, message, and disclosure contract.
- Keep true defects and impossible states outside the catchable problem set.
  Generated-code invariant failures, corrupt runtime state, and compiler/runtime
  defects terminate the operation and are contained and reported at the
  runtime boundary.

#### Boundary mapping decisions

- Continue to derive HTTP status from semantic `kind`; application and route
  code may not choose numeric statuses ad hoc.
- Continue to derive a named application's stable response error identifier
  from its declared `code`.
- Give built-in operational problems safe generic boundary codes and messages.
  Finalize the HTTP status and retry semantics for `OutcomeUnknown` before
  implementation.
- Derive every route's domain and operational error surface from the callable
  graph. Generated OpenAPI, audit, tests, and runtime adapters must agree with
  that graph.

#### Entity, persistence, and policy decisions

**Accepted as DATA-007:** the authoritative semantic contract is the
[entity, query, and transaction model](entity-query-model.md). The bounded
punctuation is now executable and fixture-backed. The implemented `type` plus
top-level `persist` prototype below remains comparison and migration evidence.

- Define `entity` as identity-bearing domain data rather than data that is
  necessarily stored in a database. Keep ordinary object types for structured
  data without identity.
- Permit ordinary construction and use of an entity without a persistence
  capability.
- Make persistence an optional explicit part of an entity contract. Only an
  entity with persistence may participate in generated insert/query/update/
  delete operations, indexes, uniqueness, references, and migrations.
- Separate ordinary entity construction from persistence. Revisit the current
  database `create` spelling; `insert` is a candidate because it does not imply
  that constructing an entity value requires storage.
- Place entity-specific access, field, and lifecycle policy under the entity's
  semantic definition so reviewers do not have to reconcile a distant policy
  catalogue with the data it governs.
- Retain an application/capability-level policy home for authentication
  strategy, public routes, cross-entity rules, external effects, secrets, jobs,
  and deployment behaviour.
- Preserve the human-approval boundary for policy changes even if policy is
  semantically colocated with an entity. Approval binds to the semantic policy
  change/digest rather than granting authority through ordinary file edits.
- Give every entity one authoritative file under `entities/`, with no partial
  declarations, extension methods, inheritance, overrides, or initial receiver
  overloads. Paths organise source but do not define schema identity.
- Make entity functions/actions checked qualified operations with dot-call
  syntax. Receivers explicitly require either the compiler-owned entity
  reference or a complete immutable entity value. Value-to-reference projection
  is permitted; reference-to-value loading is never implicit. A mutating value
  receiver must reload/lock current authority state or use a checked revision
  or conditional-write guard.
- Make reads named compiler-understood query declarations. Entity-centred reads
  live with the entity; genuinely cross-entity projections live under
  `queries/`. Routes, functions, workflows, jobs, policy, and configuration do
  not contain raw query expressions. Queries over derived state declare
  authoritative, read-your-writes, bounded-staleness, or eventual freshness.
- Allow only entity-owned actions to directly mutate that entity. Multi-entity
  application actions under `workflows/` compose entity actions rather than
  bypassing their invariants or policy.
- Make transaction intent explicit. Entity actions are independently
  failure-atomic, but reaching multiple mutation scopes requires an authored
  atomic or durable-workflow disposition. Within an explicit
  atomic boundary, nested entity actions and queries share one same-domain
  transaction and locally handled nested failures roll back to compiler-owned
  savepoints. Nested success remains provisional until outer commit, and policy
  or invariant reads guarding writes share the same transaction plan.
- Give every mutable fact one authority. Treat caches, graph views, search
  indexes, and denormalised records as declared derived representations. Commit
  an authority change and durable change record together, then generate
  ordered, idempotent, replayable, watermark-visible, reconcilable delivery.
- Distinguish local atomic, prepared atomic, durable projection, and durable
  workflow. Multi-authority changes use persisted idempotent steps, retries,
  authored compensation, reconciliation, and explicit outcome uncertainty.
  Cross-domain `atomic` is accepted only for adapters proving one compatible
  prepare/commit and recovery protocol; it never degrades silently.

The implemented bounded semantic shape is:

```text
entity Customer {
    id: Uuid
    email: Email

    identity: id

    persistence {
        store: primary
        role: authority
        unique: email
    }

    cache hot {
        store: redis
        from: primary
        strategy: invalidate
        delivery: durable
    }

    projection relationships {
        store: graph
        from: primary
        delivery: durable
    }

    policy {
        // Entity-specific access, field, and lifecycle rules.
    }

    query by_email(email: Email) -> Customer? {
        // Declarative read.
    }

    action change_email(self: ref, email: Email) -> Customer {
        // Explicit Customer mutation.
    }
}
```

An entity without `persistence` remains constructible domain data but has no
generated storage operations.

#### Implemented type and persistence consistency prototype — superseded

The following revision is executable and remains useful comparison,
compatibility, and migration evidence, but DATA-007 has superseded it as the
accepted source boundary. Do not extend or canonicalise it as a substitute for
fixture-first implementation of the accepted entity/query model.

- Use `type` for every authored scalar, object, and enum shape. Remove
  declaration-role duplication across `value`, `input`, `output`, `entity`, and
  `enum`.
- Spell structured types as `type Name = Object { ... }` and closed alternatives
  as `type Name = Enum { ... }`.
- Make boundary `input:` and `output:` members reference any type. Boundary use,
  not the declaration keyword, derives decoding, validation, and serialization.
- Support nested closed `Object` fields and `List<Object { ... }>` recursively.
- Move storage to a separate `persist Name { ... }` declaration. The absence of
  `persist` is the complete and canonical way to keep a type out of the
  database.
- Use colon-delimited constraints and persistence settings. Eliminate postfix
  field storage modifiers and call-shaped compound persistence metadata.
- Put callable failures before the success arrow:
  `action name(parameters) fails A, B -> Result`.
- Add `Email`, `Url`, and `IpAddress` as compiler-owned validated semantic
  types, and complete the prelude with the accepted representation, time, and
  container types recorded in the decision register.
- Remove automatic source document links. Generated-artifact navigation belongs
  in CodeLens or an explicit command so only actual diagnostics produce source
  underlines.

Canonical examples are:

```text
type SupportContact = Object {
    email: Email
    alternatives: List<Object {
        label: Text
        email: Email
    }>
}

type Customer = Object {
    id: Uuid
    email: Email
}

persist Customer {
    identity: id
    unique: email
}

action register_customer(input: RegisterCustomer)
    fails InviteCodeRejected
    -> RegistrationAccepted
{
    // ...
}
```

#### Route, action, and locality decisions

- Preserve `route`, `action`, and `function` as distinct semantic declarations:
  a route is an HTTP boundary, an action is a reusable runtime-managed operation
  that may perform effects, and a function is reusable pure, non-suspending
  computation.
- Do not require every route to delegate to a one-to-one named action. The
  canonical starting point for small endpoint-specific behaviour is a local
  inline action inside the route.
- Extract a named action when behaviour is reused by another route, action,
  job, handler, or test boundary; when it represents a stable domain command;
  or when it needs its own policy, transaction, idempotency, or observability
  boundary. Do not extract merely to reproduce controller/service layering.
- A route contains exactly one behaviour form: either an inline `action` body
  or `run:` invoking a named action. Supplying both or neither is invalid.
- `run:` remains because it explicitly maps route-local typed transport values
  into a genuinely reusable named action. It is not required for the common
  inline case and cannot contain arbitrary route logic.
- An inline action has the same effect, transaction, policy, return, and
  recoverable-problem rules as a named action. Its closed problem set is
  declared on the action header as `action: fails A, B { ... }`.
- Inline action structure is brace-delimited. Indentation and line breaks are
  never semantic; no YAML-style continuation or tab-sensitive syntax is
  permitted.
- Retain required authentication as the route default. Replace the current
  `auth: public explicitly` spelling with the exact opt-out `auth: none`.
  Omission never disables authentication, and the override must remain
  conspicuous in source, audit, tests, and generated documentation.
- Keep authentication and authorisation separate. `auth: none` changes the
  inbound identity requirement; it does not bypass entity or application
  policy. A no-auth route whose behaviour requires an authenticated actor is a
  compile/proof error.
- Configure authentication strategies outside ordinary route business logic.
  Strategy narrowing may be added later, but route syntax must not expose
  provider SDKs, raw tokens, cookies, or provider-specific identity data.
- Keep body, path, query, and ordinary-header decoding in the route boundary.
  These become validated typed bindings visible to the inline action or passed
  explicitly through `run:`. Named actions and functions remain transport-
  independent.
- Declare path placeholders in the route template as `{name}` and type them in
  a brace-delimited `path: { name: Type }` group. Expose validated values through
  the `path.name` namespace so they cannot collide with query, header, or body
  fields.
- Require an exact one-to-one correspondence between template placeholders and
  path bindings: every placeholder is declared once, every declaration appears
  in the template, and duplicate placeholders are invalid.
- Reserve authentication credentials and trusted identity headers for the
  generated authentication adapter; ordinary header binding cannot expose
  them to application logic.
- Preserve the field-style `name: value` route body and braces, including
  block-valued `path:` and `action:` items. Final query/header/
  body binding spelling remains a focused grammar task; the accepted semantics
  do not authorise decorators, significant indentation, parameter annotations,
  or a wholesale route syntax redesign.

The accepted local-first shape is:

```text
route GET /customers/{customer_id}/orders/{order_id} {
    path: {
        customer_id: Customer.id
        order_id: Order.id
    }

    action: fails CustomerNotFound, OrderNotFound, Unavailable {
        // Endpoint-local effectful behaviour.
    }

    output: OrderView
}
```

An explicit authentication exception is:

```text
route GET /health {
    auth: none
    output: Health

    action: {
        return Health { ok: true }
    }
}
```

The extraction form remains:

```text
route POST /orders {
    input: CreateOrder
    output: OrderView
    run: place_order(input)
}
```

#### Required implementation evidence

- focused parser, semantic, failure-flow, and target fixtures for explicit
  `kind`, flat context construction, and exact context checking;
- direct, transitive, and propagated operational-problem fixtures across
  functions and actions; local handling and mapping belong to P10.7;
- exhaustive `attempt` propagation fixtures; local recovery, mapping, and
  fallback operations move to the P10.7 outcome-match evidence;
- runtime cases for unavailable reads, definitely-not-executed writes, unknown
  write outcomes, and their safe generic boundary mappings; retry and fallback
  decisions belong to P10.7 outcome matching;
- route/OpenAPI/audit agreement for named failures and generic operational
  problems;
- route fixtures proving authenticated default, exact `auth: none` opt-out,
  inline-action problem checking, mutually exclusive inline/`run:` behaviour,
  exact multi-placeholder path binding and namespacing, transport binding,
  named-action extraction, and whitespace-insensitive brace structure;
- fixtures distinguishing complete identity-free values, non-persistent
  identity-bearing entities, compiler-owned entity references, and persistent
  entity capabilities;
- one-authoritative-file project-role diagnostics for `entities/`, `values/`,
  `queries/`, `workflows/`, and `routes/`, without making paths schema identity;
- entity function/action fixtures for reference and value receivers, checked
  dot/qualified calls, value-to-reference projection, rejected implicit loads,
  rejected overload/extension forms, and explicit return-and-rebind;
- named entity and top-level cross-entity query fixtures proving read-only
  effects, cardinality, bounds, policy, audit/index inventory, and rejection of
  raw query expressions in routes, functions, workflows, jobs, policy, and
  configuration;
- mutation-ownership fixtures proving only an entity's actions directly create,
  update, or delete it, while workflows compose entity actions;
- persisted-entity parity with the existing SQLite/PostgreSQL guarantees; and
- an explicit P11 handoff retaining human approval of entity-policy weakening,
  the accepted AUTH-001 and CONFIG-001 architectures, and the now-approved
  POLICY-001 contract for later implementation.

**Exit gate:** every recoverable problem in the executable P10.6 subset is
normalized and either propagated through the exact callable contract or mapped
at a generated boundary; P10.7 owns local handling and application mapping. No
raw adapter exception enters authored source; defects remain contained;
ordinary value types and non-persistent entities work without storage;
persisted entities retain the existing database guarantees; the accepted
DATA-007 entity/query/mutation/layout boundary is executable and auditable; and
P11 receives the accepted policy/capability ownership boundary.

### P10.7 — Callable execution, outcome matching, and IDE outcomes

**Status:** in progress. The fixture-backed call matrix, fixed-point internal
suspension slice, exhaustive outcome recovery/mapping/propagation, explicit
authored `async`/`await` rejection, callable plus inline-action outcome
presentation, handled/mapped/propagated call-site state, failure-arm navigation,
and persistence-free HTTP runtime sequencing are implemented. The unattended
TX-001 slice is also implemented: explicit atomic intent, independent
entity-action failure atomicity, nested joining with compiler-owned savepoints,
same-domain enforcement, value-receiver guards, audited isolation/locking/retry
plans, and authority-change-record atomicity have compiler and runtime
evidence. The entity-dossier scenario proves handled nested-failure savepoint
rollback and 24 same-process concurrent writes with isolated operation
timestamps and contiguous revisions against SQLite and live PostgreSQL.
PostgreSQL also passes 24 same-entity updates from two separate Bun processes.
A SQLite multi-process probe returned `SQLITE_BUSY` at transaction start; the
retry/failure default remains undecided. Broader retry evidence and
adapter-dependent cross-store execution remain outstanding.

**Why this phase exists:** exposing target-level `async` and `await` would create
a second, contagious effect system beside the existing function/action boundary
and make an adapter implementation change alter authored application source.
At the same time, `attempt` propagation alone does not supply the accepted local
handling and recovery surface. This phase completes the callable model before
P11 adds services, jobs, authentication, and more operational failures.

#### Accepted callable and execution model

- A `function` is pure, non-suspending computation from its arguments. It may
  call functions and may produce declared failures, but it cannot access
  persistence, services, events, secrets, ambient time or randomness, or invoke
  an action.
- An `action` is a runtime-managed application operation that may perform
  effects and may suspend internally. It may call functions or actions. An
  action need not perform an effect merely to qualify as a route or operation
  boundary.
- Authored Jadpo has no `async`, `await`, promise, or detached-call type. Every
  ordinary action call completes before the next statement executes. The
  compiler derives target-level suspension through the action call graph and
  emits any required TypeScript/Bun `async` and `await` internally.
- Ordinary calls are sequential. Parallelism, cancellation, partial completion,
  and work that outlives its caller require a later explicit structured-
  concurrency, event, or durable-job decision. Fire-and-forget calls are not an
  escape hatch.
- Routes remain action boundaries. One-off behaviour is an inline `action:`;
  `run:` invokes a named action only. A route does not invoke a function
  directly, though either action form may call pure functions.

The canonical static route remains concise:

```text
route GET /health {
    auth: none
    output: Health

    action: {
        return Health {
            status: HealthStatus.ok
        }
    }
}
```

The canonical extracted operation remains:

```text
action health()
    fails Unavailable
    -> Health
{
    var database = attempt check_database()
    return build_health(database)
}

route GET /health {
    auth: none
    output: Health
    run: health()
}
```

#### Accepted failure inference and acknowledgement model

- The compiler calculates each callable's complete escaping failure set as a
  fixed point over direct `reject` sites, fallible constructors and persistence
  operations, attempted callees, explicitly propagated outcome arms, newly
  rejected mappings, and compiler-known recoverable operational problems.
- That inferred set must exactly equal the authored `fails` list. Missing,
  duplicate, and stale entries are compile errors. A diagnostic shows the
  concrete callee or operation that introduced each failure and offers an exact
  guided declaration edit, but never silently changes the callable or its public
  contract.
- A fallible expression has exactly two legal acknowledgement forms. `attempt`
  returns its successful value and propagates every failure. `match` handles
  the complete success/failure outcome locally. A bare or ignored fallible call
  is invalid, and no ordinary source value has an exposed `Result` type.
- Outcome matches are exhaustive and have no failure wildcard. They contain one
  `success(value)` arm and one exact `failure FailureName` arm for every declared
  application failure or built-in operational problem. When the callee's
  failure surface changes, the match becomes non-exhaustive until the author
  makes a new decision.
- A failure arm may return a compatible replacement success value, reject a new
  declared failure, or use `propagate` to preserve the matched failure. Mapping
  retains the original occurrence as an internal semantic cause. Defects and
  impossible runtime states never enter outcome matching.

Canonical propagation is:

```text
action customer_summary(id: Customer.id)
    fails CustomerNotFound, Unavailable
    -> CustomerSummary
{
    var customer = attempt load_customer(id)
    return build_customer_summary(customer)
}
```

Canonical local handling is:

```text
var customer = match load_customer(id) {
    success(customer) => customer
    failure CustomerNotFound => anonymous_customer(id)
    failure Unavailable => propagate
}
```

Mapping a failure remains explicit:

```text
var customer = match load_customer(id) {
    success(customer) => customer
    failure CustomerNotFound => reject InviteCodeRejected {
        invite_code: input.invite_code
    }
    failure Unavailable => propagate
}
```

The optional context-binding spelling
`failure FailureName(problem) => ...` remains a focused follow-up decision. Its
absence does not block handling, mapping, recovery, propagation, or the initial
implementation of exhaustive outcome matching.

#### IDE and inspection contract

Hovering a function or action declaration, reference, or call must show the
whole source-level outcome contract rather than only the successful signature.
The compact view contains:

- callable kind and parameters;
- successful result type;
- every declared application failure, with its standard kind and summary;
- every built-in operational problem;
- for a call site, whether each failure is propagated, handled, or mapped when
  that fact is statically known; and
- for an action, that it may suspend internally and always completes before the
  caller continues.

For example:

```text
action load_customer(id: Customer.id)

Success
  Customer

Failures
  CustomerNotFound · NotFound
  Unavailable       · operational

Execution
  May suspend internally; completes before the caller continues.
```

The LSP obtains this view from the same semantic callable/failure graph used by
the compiler, route inventory, audit, OpenAPI, and agent JSON. It must not show
generated `Promise`, exception, or `Result` wrappers as Jadpo types. An
infallible callable explicitly shows `Failures: none`; an inline action exposes
the route output type as its success outcome and its own exact `fails` surface.
Signature help and completion use the same contract, and navigation from a
failure outcome resolves to its application declaration or standard catalogue
entry.

#### Required implementation evidence

1. Add positive and negative compile fixtures for the complete call matrix:
   function-to-function, action-to-function, action-to-action, and rejected
   function-to-action calls, including direct and transitive cases.
2. Add fixtures for infallible and fallible functions and actions; exact direct
   and transitive `fails`; bare-call rejection; `attempt` propagation; exhaustive
   outcome recovery, mapping, and propagation; and missing, duplicate, stale,
   wildcard, and newly introduced outcome arms.
3. Add route fixtures proving that inline and named actions remain the only
   behaviour forms and that pure helpers remain callable from both.
4. Add internal suspension analysis to the semantic graph, propagate it to a
   fixed point through action calls, and generate target-level asynchronous code
   without changing authored signatures.
5. Add SQLite and live-PostgreSQL runtime cases for independently
   failure-atomic entity actions, the diagnostic for multiple mutations without
   an explicit consistency disposition, explicitly atomic nested suspending
   actions, transaction-context preservation, handled-failure savepoints,
   mapped and propagated failures across suspension, same-domain rejection,
   audited isolation/locking/retry plans, deterministic sequencing, semantic
   traces, and absence of unhandled target promises or rejections.
6. Add central human/agent/IDE diagnostics for bare fallible calls, incomplete
   outcome matches, function-to-action calls, invalid authored `async`/`await`,
   and affected-caller guidance when promoting a function to an action.
7. Add LSP and VS Code golden tests for declaration, reference, and call hover;
   action/function signatures; success plus complete failure outcomes; handled,
   mapped, and propagated call-site state; inline actions; navigation; and
   Unicode ranges.
8. Synchronise the formatter, semantic tokens, examples, grammar, syntax,
   semantic model, compiler/runtime documentation, generated references, and
   extension package only after the corresponding behaviour is executable.

**Exit gate:** authored source contains no asynchronous target mechanism;
functions remain pure and cannot invoke actions; every action call completes or
produces a declared failure before its caller continues; every fallible
expression is acknowledged by `attempt` or exhaustive outcome `match`; inferred
and authored `fails` sets agree exactly; generated asynchronous execution
preserves declared atomic boundaries and semantic traces without silently
widening transactions; TX-001 concurrency and retry plans are auditable; and
every IDE hover presents the complete successful and failure outcome contract
from the compiler graph.

### WASM-EXP1 — Bounded Wasm runtime experiment

Latest checkpoint: [immutable-row and SQLite write results](wasm-typed-values-results.md).
The experiment and its requested performance follow-ups are complete; Bun remains
the working target. Earlier measurements below describe their dated checkpoints.

**Follow-up plan, two bounded passes complete (2026-09-30):** [large-row investigation and backend
coverage plan](wasm-large-row-plan.md). LR-1 attributes transport and validation
costs; LR-2 compares bounded buffer/typed-format candidates; LR-3 qualifies
correctness and practical parity; LR-4 measures current startup and host behaviour.
COV-1/COV-2 separately inventory and qualify backend coverage. The first pass
measured row transport, local HTTP, startup and validators: egress-only improved
large reads, but Bun parity failed and single-write stalls remain adverse.
The second pass adds immutable typed rows and local size proofs, plus SQLite
statement/commit attribution and a WAL/FULL control that improves both targets.
Direct typed decoding, general lowering, the full inventory, sustained storage/
recovery and hosted qualification remain open; no default-target switch is scheduled.
The [HOST-1/HOST-2 review](wasm-host-capabilities.md) adds a current documented
Cloudflare Workers/Bun capability matrix and required adapter probes; temporary
files are request-scoped memory, while threads/processes and persistent ordinary
files require a different host or explicit service. New probes remain unexecuted.

**Status:** complete on 2026-09-30. **Defer adoption for further backend
development; retain Bun.** Both routes passed the 15-case local/Cloudflare
probe; the provisional Rust full slice passed A01–A16 on real local/cloud
storage, with separate A17 rejection and A18 mutation/rebuild evidence.
The identical core hash established bounded portability, but the I/O p95 gate
failed (33.34× / 36.14× Bun), and oversized host frames lose the inner fault
location while still rolling back safely. Route disposition: **inconclusive**.
Disposable Workers and their synthetic authority were removed and verified.
See [results and raw evidence](wasm-experiment-results.md) and the
[frozen protocol](wasm-experiment-plan.md).

**Optimisation follow-up:** the owner subsequently requested latency/throughput
investigation. Instance reuse, prepared statements and immutable-plan caching
raised local throughput to roughly79% of Bun with all bounded cases still
passing locally and on Cloudflare. See [follow-up results](wasm-optimization-results.md).
The earlier fresh-instance results remain historical evidence; production Bun
is unchanged and direct-versus-Rust remains inconclusive.

**Placement:** after completion of the agreed authentication scope and the
subsequent comprehensive validation phase, as clarified by the owner on
2026-09-29. The first-party checkpoint alone does not trigger this experiment.
Begin with the small compilation-route probes and reassess before expanding
the experiment. Decision work on other contracts can continue. This is
exploratory work under the P10R deferral;
scheduling the experiment does not select Wasm as the production target.

**Question:** can a Wasm application with a compiler-owned Jadpo runtime and
explicit host capabilities preserve the language's guarantees while improving
portability, deployment packaging, or execution enough to justify another
backend? Performance and simpler builds are hypotheses to measure.

**Compilation-route question:** compare `Jadpo -> generated Rust -> Wasm`
with `Jadpo -> Wasm` generation from the checked semantic model. Neither route
is selected in advance. The comparison concerns application code generation;
direct Wasm generation may still link a compiler-owned runtime implemented in
Rust or another language, and does not require writing an optimiser from scratch.

**Bounded scope:**

- Keep the TypeScript/Bun target as the working baseline. Compile one small
  representative Jadpo application from the existing checked semantic model
  into Wasm; a handwritten equivalent alone is not compiler evidence.
- Start with equivalent small lowering probes for both compilation routes,
  including typed values, a domain outcome and an asynchronous host boundary.
  Use their evidence to choose the route for the complete slice within the
  same effort budget. Record an inconclusive comparison explicitly rather than
  treating the first route implemented as the winner.
- Exercise request and database-result validation, a policy-scoped database
  read/write, a handled domain failure, an unexpected host failure, and an
  asynchronous host call. Include rollback and request-context isolation.
- Use fixture principals at the test boundary to exercise policy without
  inventing real authentication adapters or claiming protected-route support.
- Run the slice in a local Wasm host and Cloudflare Workers with generated
  host glue. Keep I/O behind explicit capabilities for storage, HTTP, clocks,
  randomness and configuration; implement only those needed by the slice.
- Record actual host guarantees and reject unsupported transaction/freshness
  requirements. Cloudflare storage need not match the local adapter, but any
  semantic difference or unsupported operation must be visible.
- Assess AWS and Fastly compatibility on paper only. Additional host adapters,
  a production backend replacement, new language syntax, parallel execution,
  durable workflows, and a replicated database/queue platform are outside this
  experiment. Embedded local storage is not distributed-storage evidence.

**Protocol and deliverables:**

1. Before implementation or measurement, checkpoint the Bun baseline, source,
   acceptance cases, compiler/runtime versions, host configuration, measurement
   procedure, and evaluation thresholds. Record a bounded effort budget and
   stopping point; do not tune success criteria after seeing results.
2. Produce reproducible build/run instructions, the generated Wasm artifact,
   compiler-owned runtime/glue, and a host-capability inventory. Document
   memory/value representation, suspension, error translation, and mapping
   runtime failures back to Jadpo source and semantic operations.
   Include a route comparison covering compiler implementation and maintenance
   complexity, build-toolchain/dependency burden, optimisation/library reuse,
   memory ownership, async lowering, source diagnostics, and required runtime
   functionality. Probe Cloudflare compatibility for both routes: emitted Wasm
   features, imports, generated glue, host bindings and deployment limits.
   Distinguish code-generation limitations from host limitations; neither route
   may assume that changing the compiler pipeline supplies a missing host API.
3. Run identical applicable acceptance cases against Bun and Wasm. Preserve
   policy, validation, failure disclosure, transaction and secret-redaction
   guarantees; report unsupported cases rather than weakening the test.
4. Measure build time, artifact/dependency footprint, startup, memory where
   observable, throughput and latency distributions for a small CPU workload
   and the I/O slice. Record host-call/serialization overhead and implementation
   effort. Use repeated comparable runs; separate target effects from provider,
   database and network effects, and mark unavailable metrics explicitly.
5. Write a short results report with raw evidence, limitations, remaining
   backend work, and an adopt/extend/defer/reject recommendation. Separately
   recommend Rust-mediated or direct Wasm generation, or record the evidence
   still needed to choose. Any extension
   needs a new bounded question; a successful slice does not imply readiness
   to replace the complete Bun target.

**Exit gate:** local and Cloudflare evidence is recorded, semantic parity and
limitations are explicit, and both the target recommendation and compilation-
route disposition are reviewed and recorded. Correctness and preserved guarantees are mandatory; adoption also
needs a demonstrated benefit against the preregistered criteria. A negative
result is a valid completed experiment. If the effort budget is exhausted or
Cloudflare access is unavailable, retain the partial report and named blocker;
do not label the missing host evidence complete or keep expanding the spike.

**Follow-through:** an accepted target change must update the decision register,
runtime architecture, adapter plans and roadmap before a broader migration.
Otherwise resume P11 on Bun with the findings retained. This experiment does
not satisfy P10R, DX2 external trials, or P12 comparative validation.

### P11 — Authentication, policy, and golden todo

**Objective:** implement the frozen P10R assurance contracts and demonstrate the
safe-default backend thesis on the canonical todo application without changing
the test to match the compiler.

**Required behaviour:**

- authentication required unless public is explicit;
- explicit public access and weaker requirements remain visible security
  exceptions rather than protections the LLM must remember to enable;
- multiple authentication strategies behind one generated adapter boundary,
  with deterministic selection, validated claim mapping, no implicit privilege
  merging, and fail-closed errors;
- a provider-independent typed principal containing identity, allowlisted user
  information, and authentication strength, with roles resolved separately
  from authoritative POLICY-001 bindings;
- current principal and derived ownership/membership scope visible to policy without
  exposing provider SDKs, tokens, or session mechanics;
- a typed configuration contract, secret-safe environment binding, and
  fail-closed startup/readiness behaviour;
- todo CRUD and partial updates;
- `T?` versus omitted patch fields;
- concealed lookup where policy requires it;
- generated route/failure/security audit;
- reminder job and one reviewed external service after synchronous CRUD works.

P11 must preserve a frozen-toolchain result. If the todo exposes a language or
compiler defect, record the original outcome, change the issue/specification and
focused fixtures, then rerun from a named checkpoint. Report the frozen and
improved results separately.

**Exit gate:** the canonical todo application runs; every policy decision has a
named proof result; the threat-model evidence is satisfied; an agent cannot
self-approve a weakening; and the adversarial ownership, tenancy, secret, public
route, and lifecycle changes meet the preregistered P10R thresholds. Milestone C
is complete only after an independent review confirms the frozen evidence.

### P12 — Order/payment application and TypeScript baseline

**Objective:** try to falsify the language after the prototype works.

**Required pressure:** money/currency, inventory, provider contracts, payment
declines, idempotency, webhooks, retries, transactions, refunds, migrations,
role collisions, non-trivial pricing logic, and whether data-carrying enum
variants materially prevent invalid payment/provider states. The enum trial must
cover exhaustive matching, variant payload typing, boundary tagging and
decoding, compatibility, persistent representation, database constraints, and
migration behaviour; attractive surface syntax alone is not sufficient reason
to implement the feature.

Build two behaviourally equivalent applications: one authored in this language
and one authored with a strong, opinionated TypeScript stack. Give them the same
predeclared acceptance tests, requirements, adversarial prompts, and ordered
change sequence. The TypeScript implementation must be a credible competitor,
not a deliberately weak control.

P12 uses the protocol, thresholds, severity rubric, baseline definition, and
reporting template frozen in P10R. Verify their digests before the first run;
do not reinterpret or extend them after seeing comparative results. Preserve
three costs separately: building or repairing the language and compiler,
authoring the application against the frozen toolchain, and adapting the
application after a language/toolchain change. None may disappear into
unmeasured setup.

Run at least three clean trials per stack with counterbalanced order. The
order/payment requirements, prompts, acceptance tests, starting repositories,
and allowed agent context are identical. A solution learned in an earlier run
is either made available symmetrically or recorded as contamination. Report
per-run results and distributions; do not present only pooled totals.

**Required experiment record:**

- pin compiler, runtime, dependency, model, tool, prompt, and starting-repository
  versions so each result is reproducible;
- record wall-clock and active agent time, input/output tokens, number of turns,
  context supplied, files and declarations touched, compiler/test iterations,
  human interventions, and final authored/generated source size for every
  change;
- record defects, security or policy mistakes, regressions, incomplete work,
  documentation drift, and which mechanism first detected each one;
- record every language compiler error or warning encountered during ordinary
  implementation, including repeated diagnostics, false positives, false
  negatives, unclear messages, and cases where the agent became stuck;
- record every desired expression that the language could not represent
  directly, every workaround, and every escape hatch, including the additional
  code, tokens, time, risk, and loss of clarity it caused;
- record equivalent TypeScript friction rather than attributing every problem
  on that side simply to “TypeScript”; and
- retain raw run logs and per-change results, not only a retrospective summary.

Every compiler-friction or workaround incident must receive an explicit review.
Classify it as a compiler defect, diagnostic/documentation defect, missing
standard capability, missing language feature, intentional safety boundary,
agent misunderstanding, tooling/ecosystem gap, or experiment/setup problem.
Then choose and justify one disposition: fix the implementation, improve the
diagnostic or documentation, change the language, add a constrained standard
capability, accept the friction as a worthwhile guardrail, provide a reviewed
escape hatch, or reject the attempted use case. Language changes must enter the
issue log and gain focused fixtures before the application is retried; they
must not be silently patched in merely to make the new language win. Preserve
the failed run, then rerun the affected change from a named clean checkpoint and
report both the original and improved results. Distinguish the initially frozen
comparison from any later tuned-toolchain comparison.

The final critical review applies every P10R continuation threshold and must
separately answer:

- which implementation was easier to write and change, and why;
- which reached correct behaviour faster;
- which consumed fewer tokens and required less context;
- which produced fewer and less severe defects;
- which diagnostics shortened or lengthened the agent loop;
- which required fewer workarounds, escape hatches, and human decisions;
- which repository was easier for a human and a fresh agent to understand;
- which advantages came from the language rather than its framework,
  scaffolding, generated code, or novelty; and
- whether the evidence supports continuing the language, changing its design,
  pivoting the work into a TypeScript framework, or stopping.

Both quantitative measurements and a candid qualitative account are required.
An inconvenient result, including TypeScript winning overall, is a successful
experiment if it is well evidenced.

**Exit gate:** all clean trials are complete; both applications are judged
against the same acceptance suite; the raw per-change experiment ledger,
independent adjudication, comprehension results, and incident reviews are
complete; all proposed language changes are linked to issues and fixtures; each
preregistered threshold has a pass/fail/mixed result; and the required
continue/redesign/framework-pivot/stop decision is recorded. A failed critical
safety threshold cannot be overridden by productivity or source-size results.

## 5. Project structure workstream

An opinionated project structure is now an explicit roadmap concern. The goal
is to reduce navigation cost and unnecessary naming/layout decisions for humans
and agents while remaining viable for small, large, multi-domain,
integration-heavy, and multi-application repositories.

The work is tracked in the [project structure workstream](project-structure.md)
and `LAYOUT-001` in the [language issue log](language-issues.md). P8 must produce
concrete candidate trees and select a prototype before P9 fixes runtime and
generated-output paths. Enforcement remains open until the todo and
order/payment applications provide evidence.

The intended progression is convention, scaffold, warning, and potentially
compiler enforcement with a narrow reviewed escape—not immediate rejection of
every non-canonical repository.

Project creation follows the same principle. The initial direction is one
static, deterministic base produced by `jadpo new`. A human-facing wizard,
if added, is only an interface over explicit flags; an LLM uses the same
non-interactive command. Optional capability packs may add canonical material
later, but they cannot invent or rearrange the project architecture.

## 6. Developer tooling and LLM presentation workstream

Editor and presentation support are tracked in the
[developer tooling workstream](developer-tooling.md), with open decisions in
`TOOL-002`, `TOOL-003`, `TOOL-004`, and `TOOL-005` in the
[language issue log](language-issues.md).

The compiler is the one semantic authority. A standard LSP should reuse its
parser, semantic graph, inferred types, documentation identifiers, and
structured diagnostics rather than implementing editor-only language rules.
The delivery order is:

1. a stable machine language/file identifier, TextMate grammar, file
   association, comments/brackets/folding, and fenced-snippet fixtures;
2. a checked local-development loop with versioned JSON diagnostics,
   source-file watching, atomic rebuilds, last-known-good serving, and
   automatic Bun restart/reload after successful builds;
3. compiler-parity diagnostics, symbols, definition/reference navigation,
   hover documentation, completion, and semantic tokens over LSP;
4. formatter, rename, conservative code actions, and links to generated
   contracts/audits;
5. a central diagnostic catalogue, human-first summaries, guided and verified
   repairs, decision ownership, bounded semantic context, clean IDE Quick Fix
   alternatives, and root-cause grouping;
6. audience-specific browser, telemetry, and agent incident schemas with
   secret-safe OpenTelemetry/structured-log adapters and local semantic
   enrichment through stable operation/source-revision IDs;
7. Shiki, Prism, Highlight.js, and Monaco adapters for documentation and chat
   clients that allow custom grammars; and
8. a compact skill/plugin or MCP interface that lets agents retrieve versioned
   grammar, diagnostics, symbols, and documentation from the same language
   service.

### Formatter conformance follow-up — TOOL-005

The formatter matrix and conformance suite are implemented. The matrix maps
all 122 named grammar productions to canonical rules and links representative
exact-output snapshots. A unit test extracts those production names and checks
that every matrix row has a valid snapshot link. The unit suite checks exact
output for all 58 compile-pass fixtures after whitespace perturbations. A
focused exact-output case covers syntax alternatives absent from positive
fixtures: `restrict`, both inverse `many` spellings, strategy-free `projection`,
compatibility `type Name = Enum { ... }`, all freshness values, descending ordered pagination, `else`, negative numbers,
escaped strings, `pattern`/`format` constraints, nullable `default none`, integer,
decimal, and boolean config defaults, and optional `previous_secret`/`origin`
validator settings. Integration
checks exercise the complete compile corpus under space, tab, indentation,
blank-line, line-ending, and removed-line-break perturbations. Positive cases
check token preservation, parseability, and idempotence; negative cases check
token preservation and stable syntax diagnostics.

One formatter contract remains unresolved before AST-guided line reconstruction:
FMT-005 says nonblank source line breaks are preserved, while the roadmap's
acceptance calls for identical canonical output after those breaks are removed.
The current removed-line-break corpus does not compare its output to the
unperturbed canonical output. Resolve this rule before implementing AST-guided
line reconstruction; keep TOOL-005 open until the rule and exact-output cases
agree.

The exact-output cases snapshot every positive compile fixture and the
previously unrepresented grammar alternatives, with extra spaces and tabs,
incorrect indentation, excess blank lines, and CRLF variants. Keep removed
nonblank-line-break expectations behind the pending FMT-005 decision.

Fenced Markdown code can carry the stable language identifier, but highlighting
is ultimately controlled by the renderer. Unsupported hosts must receive exact
readable plain text; the project must not fake support by labelling source as a
different language. Inline backtick spans are not expected to receive rich
syntax colouring.

DX0 highlighting can proceed during P10–P11 without displacing semantic work.
DX0.5 is scheduled during P10.5 and must pass before P11 application authoring.
The minimum LSP and agent-context baseline must be recorded and frozen before
P12 so tooling quality is measured honestly and does not change midway through
the TypeScript comparison.

### DX2 guided-diagnostic and safe-enrichment contract

Implement diagnostics as compiler-owned semantic repair protocols rather than
scattered strings. One versioned catalogue defines each lower-case dotted rule
identifier, human summary and reason, typed context, recommended next step,
bounded alternatives, decision owner, impact, help identifier, fixtures, and
legacy aliases. Compiler call sites provide typed facts only.

The version-2 agent packet uses the clear keys `schemaVersion`, `diagnosticId`,
`sourceRevision`, `summary`, `reason`, `recommendedNextStep`, `alternatives`,
`ruleId`, `severity`, `location`, `context`, `impact`, and `helpId`. Repair kinds
are `automatic_fix`, `guided_choice`, and `human_decision`. Automatic fixes are
revision-bound compiler edits with a preview of behavioural and public-contract
impact; ambiguous semantic choices remain alternatives, and protected choices
become explicit human questions.

The LSP renders the same object without exposing raw JSON: the smallest precise
squiggle, plain summary in Problems, summary/reason/recommendation on hover,
recommended verified edit plus clean alternatives in Quick Fix, multi-file diff
preview, and an expandable impact/help view. Stale fixes fail closed. Parser and
type cascades group beneath a root cause, while terminal stage summaries move to
report status rather than appearing as extra problems.

Keep `CompilerDiagnostic`, `PublicFailureResponse`, `OperationalLogEvent`, and
`AgentIncidentPacket` as distinct audience types. Browser failures contain only
declared public data. Operational events contain safe correlation and semantic
IDs and map to structured JSON, OpenTelemetry, and constrained provider
adapters. Diagnostic/logging APIs reject arbitrary rendered values; secrets are
non-renderable and internal context is not automatically loggable. A trusted
local tool enriches a runtime event with the matching compiler graph and source
revision to give an LLM rich source, context, impact, occurrence, and repair
information without exporting customer data or secrets.

**Required evidence:**

- a generated catalogue manifest and documentation page covering every public
  diagnostic, with no remaining ad hoc user-facing message construction;
- fixtures for summary/reason rendering, each repair kind, decision ownership,
  related locations, bounded impact, root-cause grouping, and legacy aliases;
- CLI/JSON/LSP parity tests plus IDE protocol cases for Problems, hover, ordered
  Quick Fix alternatives, stale-edit rejection, and multi-file preview;
- counterfactual repair tests proving the recommended edit resolves the named
  diagnostic and accurately reports any new diagnostic or public-contract
  change;
- browser, log, trace, exporter-buffer, and provider-adapter tests planted with
  credential, header, body, connection-string, customer-data, and raw-exception
  canaries; and
- fresh-agent and first-user trials showing that common mechanical errors are
  corrected in one cycle and the overall median valid repair remains within the
  two-cycle roadmap threshold.

**Exit gate:** every public diagnostic is central, readable, actionable,
fixture-backed, and rendered consistently across CLI, JSON, documentation, and
IDE; safe preferred repairs are verifiably correct; semantic alternatives and
human decisions are explicit; no audience boundary can serialize another
audience's payload; and the secret-canary and repair-cycle evidence passes before
the P12 tooling freeze.

## 7. Typed configuration and deployment-readiness workstream

Configuration is part of the application contract, not an untyped collection
of process strings. Design it against the P11 golden todo application and
freeze its minimum tool/runtime behaviour before the P12 comparison. The
approved `CONFIG-001` plan fixes the v0.1 source and tool/runtime shape.

**Authored contract and discoverability:**

- declare each value once with its semantic type, requiredness, optional
  default, constraints, documentation, and whether it is secret;
- keep each explicit environment binding beside its typed field in one
  structured source declaration, with fields required unless they have a
  checked non-secret literal default;
- give tooling and deployment systems a generated machine-readable manifest
  containing names, types, constraints, descriptions, requiredness, binding
  names, and safe validation checks—but never secret values;
- generate human documentation, local-development templates, IDE hover and
  completion data, and compact agent context from that same manifest;
- use only `.env.local` for local development, explicit test-harness values for
  tests, and the same declared binding names in deployment environments; do not
  define implicit overlays or silently substitute local defaults in production;
  and
- track value provenance so diagnostics can identify the missing or invalid
  binding without printing its contents.

**Validation boundaries:**

- ordinary source `check` validates declarations, references, types,
  constraints, conflicting defaults, and environment coverage without needing
  access to production secrets;
- `jadpo config set <field>` obtains a local value through a secret-safe prompt,
  and `jadpo config check` reports only safe presence/validity status;
- `jadpo dev`, generated deployment integration, and runtime startup validate
  actual values automatically before binding a public listener or reporting
  readiness;
- parsing and local constraints are distinct from bounded live checks: for
  example, a database URL can be structurally valid yet fail a timed connection
  or minimum-capability probe; and
- all command output, logs, generated artifacts, health responses, and failure
  reports redact secret values and avoid echoing credentials embedded in URLs.

**Health and deployment contract:**

- liveness reports whether the process/event loop is functioning and must not
  fail merely because an external dependency is temporarily unavailable;
- readiness remains false until effective configuration is valid and declared
  dependency checks have passed, and can become false when a required
  dependency is no longer usable;
- live checks are explicit, bounded by timeouts, side-effect-free where
  possible, and classified as required or advisory rather than inferred from a
  value's type;
- health endpoints expose stable status/check identifiers and safe reasons,
  not configuration or secret values; and
- automatic deployment validation plus readiness provide the hosting platform
  with a deterministic promotion gate. The deployment controller owns rollback to
  the prior healthy revision; the application supplies the evidence and never
  claims it performed a rollback itself.

Every v0.1 value is startup-bound. A valid `.env.local` change causes a
controlled `dev` restart; production rotation uses a process restart or rolling
deployment. Live reload is deferred until an application demonstrates the need.
Do not add ambient reads of `process.env` to authored application logic; the
compiler-owned generated boundary is the only place raw bindings become typed
trusted configuration.

**Executable exit evidence:** fixtures and runtime/deployment-harness tests
cover missing required values, malformed values, duplicate bindings,
secret redaction, startup before listener binding, readiness success/failure
and timeout, liveness independence, live dependency recovery, and a failed new
revision that is not promoted (or is rolled back) while the prior revision
remains healthy.

## 8. Public-facing website and documentation benchmark workstream

**Status:** conditional and not started. Activate only after P12 records a
`continue`, `redesign`, or framework-pivot decision and the project owner
authorises a public-facing website. This workstream is not evidence that the
product thesis has already succeeded.

**Objective:** before designing or implementing the public website, reassess
high-quality developer documentation projects individually, identify the
characteristics that make each effective, and deliberately select the patterns
that fit this project's audiences and trust model. Do not copy a site's visual
style wholesale or infer quality from popularity alone.

### Benchmark set

The primary case studies are:

- [Rust documentation](https://doc.rust-lang.org/) for its progression from
  books and learning material to generated crate/API reference, local/offline
  documentation, examples, source links, and compiler-integrated help;
- [Stripe documentation](https://docs.stripe.com/) for API onboarding,
  language-switchable examples, sandbox/test-mode integration, personalised
  context, lifecycle guidance, and treatment of errors and failure states;
- [Django documentation](https://docs.djangoproject.com/) for its separation
  of tutorials, task-oriented how-to guides, conceptual explanations, and
  precise reference material, plus explicit version annotations;
- [ArchWiki](https://wiki.archlinux.org/) for community-maintained operational
  knowledge, troubleshooting alongside configuration, alternative approaches,
  and documentation useful beyond the project's immediate users;
- [PostgreSQL documentation](https://www.postgresql.org/docs/current/) for
  complete, authoritative, versioned manuals organised for learners, users,
  administrators, client developers, extension authors, and contributors;
- [Godot documentation](https://docs.godotengine.org/en/stable/) for combining
  beginner projects, application guides, generated API/class reference,
  editor-accessible help, translations, engine internals, and community notes;
  and
- [Twilio documentation](https://www.twilio.com/docs) for outcome-oriented
  quickstarts, consistent cross-product concepts, multi-language runnable
  examples, webhook guidance, and short time to a first successful result.

Specialist and complementary references should also receive bounded reviews:

- [MDN Web Docs](https://developer.mozilla.org/) for standards-based web
  reference, examples, cross-browser compatibility, and mixed learning and
  reference paths;
- [GitHub Docs](https://docs.github.com/) and project repositories/issues for
  large-scale product navigation, versioning, source-adjacent knowledge, and
  the boundary between official documentation and operational discussion;
- [DevDocs](https://devdocs.io/) for fast federated search, consistent
  presentation, keyboard navigation, and offline reference access;
- [Can I Use](https://caniuse.com/) for focused compatibility evidence and a
  compact answer to a single recurring developer question;
- [OWASP](https://owasp.org/) for consensus-based security guidance, explicit
  scope limits, risk communication, and the distinction between awareness
  material and verifiable standards;
- [freeCodeCamp](https://www.freecodecamp.org/learn/) for structured,
  practice-led beginner learning rather than authoritative reference; and
- the current [DevPortal Awards](https://devportalawards.org/) winners and
  juror reports, initially North Developer, Shell Developer Portal, and
  Redpanda Docs, as a watchlist of contemporary onboarding and portal patterns
  rather than assumed permanent exemplars.

Laravel, Vue, Tailwind CSS, Kubernetes, and FastAPI may be added as comparison
cases if later evidence supports their recurring reputation. Their inclusion
must not be justified only by an isolated testimonial or listicle.

### Current evidence baseline

Preserve the following findings as the reason for the initial shortlist, then
recheck them when the workstream begins:

- the [2025 Stack Overflow Developer Survey](https://survey.stackoverflow.co/2025/developers)
  received about 49,000 responses and reported technical documentation as the
  most-used learning resource, at nearly 68%; its recruitment was weighted
  toward engaged Stack Overflow users, so it establishes broad importance but
  not an impartial documentation ranking;
- a peer-reviewed [Rust adoption study](https://www.cs.umd.edu/~mwh/papers/rust-adoption.pdf)
  combined 16 professional interviews with 178 completed community surveys and
  reported 91% positive sentiment for official documentation; the authors note
  likely self-selection toward people who already view Rust positively;
- the open [APIbenchmarks index](https://www.apibenchmarks.com/) currently
  scores Stripe at 97.5/A+ for payment APIs and Twilio at 94.1/A+ for messaging
  APIs using a reproducible method, but documentation/developer experience is
  only 30% of the composite score and the benchmark itself must be reviewed;
- Django's own
  [writing guide](https://docs.djangoproject.com/en/stable/internals/contributing/writing-documentation/)
  explicitly separates tutorials, how-to guides, topic explanations, and
  reference; the same structure is now formalised by
  [Diataxis](https://diataxis.fr/) and used beyond Django, which is evidence of
  design influence rather than direct satisfaction measurement;
- ArchWiki is maintained by an official team and thousands of contributors,
  publishes current [activity statistics](https://wiki.archlinux.org/title/ArchWiki:Statistics),
  and is repeatedly used outside Arch; this is a strong operational and
  reputation signal, not a controlled comparative study;
- PostgreSQL states that its
  [official manual](https://www.postgresql.org/docs/current/preface.html) is
  written by developers and volunteers in parallel with the software and
  divides its material by user stage and role;
- Godot documents its maintenance and translation process across approximately
  1.1 million source words, versioned manuals, generated class reference, and
  user-note feedback in its
  [documentation-quality report](https://godotengine.org/article/ensuring-quality-godot-documentation/);
- MDN reports more than 15 million monthly users and collaboration with browser
  vendors, standards organisations, and Open Web Docs on its
  [about page](https://developer.mozilla.org/en-US/about); these are scale and
  governance signals reported by MDN itself; and
- the expert-juried [2025 DevPortal Awards results](https://pronovix.com/articles/best-developer-portals)
  selected North Developer as Best Overall, Best Onboarding, and Best Solution
  Portal. This is professional-jury evidence rather than mass-user evidence.

### Evidence standard

Each case study must distinguish:

- direct observation of the current website and representative user journeys;
- developer survey or task-completion evidence, including sample, date, and
  self-selection limits;
- reproducible benchmark results and what proportion of the score actually
  measures documentation;
- independent jury or professional-review evidence;
- scale, contribution, maintenance, and versioning signals; and
- community reputation or anecdote, clearly labelled as weaker evidence.

Retain dated links and captures for material decisions because these websites
change. Prefer primary research and published methodologies. Popularity,
traffic, GitHub stars, visual polish, and a project's overall developer
satisfaction are not substitutes for documentation usability.

### Common assessment rubric

Assess every site against the same questions so the review does not become a
collection of unrelated impressions:

1. Who are the intended audiences, what prior knowledge is assumed, and which
   user jobs receive an explicit path?
2. How are tutorials, how-to guides, explanation, reference, examples,
   troubleshooting, release notes, and migration guidance separated and
   connected?
3. How quickly can a new user reach a verified first success, and what setup,
   authentication, configuration, or conceptual friction occurs first?
4. How effective are navigation, search, page hierarchy, cross-linking,
   progressive disclosure, and orientation within a large documentation set?
5. Are examples complete, runnable, copy-safe, version-correct, multi-language
   where appropriate, and paired with expected output and failure cases?
6. How are versions, deprecations, compatibility, insecure releases,
   migrations, and behavioural changes made visible?
7. How closely are authored documentation, generated API reference, source,
   schemas, diagnostics, tests, and actual product behaviour kept in sync?
8. How well do error messages, diagnostics, troubleshooting paths, and support
   escalation lead a user from failure to recovery?
9. What feedback, contribution, editorial review, localisation, accessibility,
   mobile, performance, print, and offline mechanisms exist?
10. What machine-readable contracts, stable anchors, plain-text/Markdown
    representations, structured examples, and bounded LLM/agent retrieval paths
    exist without making AI output the authority?
11. Which trust signals expose provenance, security boundaries, known limits,
    ownership, freshness, and the distinction between normative and community
    content?
12. What maintenance team, workflow, tooling, telemetry, and ongoing cost are
    required to preserve the characteristic being considered?

Each review must include at least one realistic task completed from a clean
starting point. Record time to first useful result, wrong turns, search terms,
pages visited, unresolved questions, and whether the documentation or another
resource supplied the decisive answer.

### Required outputs

- one dated teardown per benchmark, using the common rubric and preserving
  evidence strength and limitations;
- a cross-case characteristic matrix that separates broadly reusable patterns
  from patterns dependent on the benchmark's domain, team size, or budget;
- a decision record for every characteristic adopted, adapted, deferred, or
  rejected, including maintenance cost and the user need it serves;
- a proposed public-site audience map, information architecture, content model,
  search/versioning strategy, contribution model, accessibility baseline, and
  machine-readable/agent access contract;
- low-fidelity prototypes for the home/get-started path, tutorial, how-to,
  concept page, reference page, diagnostic page, security/assurance evidence,
  migration guide, and release/version page; and
- task-based tests with representative first users before committing to the
  production visual system or website implementation.

The website must present generated language contracts, policy/security
evidence, examples, and human-authored explanation as distinct content types
with visible provenance. The compiler and versioned source artifacts remain
semantic authorities; the public site must not create a second hand-maintained
definition of language behaviour.

**Exit gate:** every primary case study has a completed evidence-backed
teardown; complementary reviews cover every capability selected for the site;
the characteristic matrix and decisions are reviewed; representative users can
complete the agreed discovery, first-success, reference, troubleshooting,
version-selection, and trust-verification tasks; and the approved information
architecture and content contracts are traceable to user needs and maintained
sources. Only then should production website design and implementation begin.

## 9. Scope guard

Until the current phase or an explicit workstream schedules them, do not
prioritise:

- package management or a registry;
- a full LSP or editor marketplace release before the P10 semantics are stable;
- native generated binaries;
- optimisation;
- macros or general metaprogramming;
- a broad standard library;
- multiple deployment targets;
- formatter polish beyond deterministic test formatting;
- modules beyond the initial global application namespace;
- services, jobs, events, and database syntax not needed by the current phase.

Work outside this guard requires an explicit roadmap change because it delays
the semantic proof.

The thin DX0 highlighting baseline and DX0.5 checked local-development loop are
exempt from this guard because they have an explicit workstream and cannot
define semantics. DX0.5 must call the existing compiler pipeline rather than
introducing watch-only language behaviour.

## 10. Progress update procedure

Every implementation change should update this document when it changes phase
evidence or status:

1. link the new artifact or test evidence;
2. check it against the phase exit gate;
3. update the summary status;
4. identify one concrete next action;
5. append a dated entry to the log below;
6. record any newly discovered language ambiguity in the issue log.

A phase may move backwards if a golden application invalidates its assumptions.

## 11. Progress log

This log preserves implementation and decision history; entries are not current
authority and same-day workstreams are not guaranteed to appear in commit-time
order. When an entry is superseded, the progress summary, phase contract, and
decision register above determine current work.

### 2026-09-25 — P0 complete

- Consolidated the source conversation into the documentation set.
- Accepted nominal semantic and field types.
- Accepted validated `Type(value)` construction.
- Accepted the standard-kind failure architecture and safe disclosure model.
- Added normative type-system and failure-model acceptance cases.

### 2026-09-25 — P1 complete; P2 started

- Added the core grammar used by the first compiler slice.
- Added the canonical Jadpo seed application and expected semantic facts.
- Added the living language issue log.
- Deferred broader syntax explicitly rather than encoding it as pseudocode.
- Began P2 with the initial ten-fixture conversion target.

### 2026-09-25 — P2 and P3 complete; P4 started

- Converted the first ten type/failure cases into paired `.jadpo` and
  `.expect.json` fixtures.
- Validated JSON, source/expectation pairing, and unique expected source spans.
- Added the Rust workspace with syntax, semantic, diagnostic, compiler, and CLI
  crates without external dependencies.
- Added recursive `.jadpo` discovery and deterministic scaffold inspection.
- Added stable diagnostics for front-end and build phases not yet implemented.
- Verified the workspace with four passing unit tests and CLI smoke checks.

### 2026-09-25 — P4 lexer complete

- Added core token kinds, retained trivia, and UTF-8-safe byte ranges.
- Added lexical recovery and stable diagnostics for invalid escapes,
  unterminated strings, and unexpected characters.
- Verified clean lexing for the Jadpo seed and all ten compiler fixtures.
- Increased the passing Rust test count from four to nine.
- Tightened the core expression grammar to one qualified-name/postfix path
  before parser implementation.

### 2026-09-25 — P4 complete; P5 started

- Added the span-preserving typed AST and recovering recursive-descent parser.
- Added an exact declaration-outline snapshot for the Jadpo seed.
- Parsed every initial compiler fixture through its intended syntax node.
- Preserved arbitrary `throw` as an unsupported node with the stable
  `SYN_UNSUPPORTED_THROW` diagnostic.
- Demonstrated recovery across multiple errors in one malformed source file.
- Replaced the CLI's front-end placeholder with a real syntax-checking path.
- Increased the passing Rust test count from nine to fourteen.
- Opened P5 with parsed-project boundaries ready for declaration lowering.

### 2026-09-25 — P5 complete; P6 started

- Added a deterministic application-wide declaration index and explicit
  prelude catalogue.
- Made record and failure fields first-class semantic nodes using their
  `Type.field` or scoped failure identity.
- Resolved declaration-level type, failure, route, and callable references.
- Emitted nominal refinement and callable-dependency edges.
- Added spanned diagnostics for duplicate, unknown, wrong-kind, and
  non-callable names.
- Upgraded `jadpo inspect` from a file list to the semantic graph manifest.
- Upgraded `jadpo check` to gate syntax and declaration semantics.
- Proved the seed's fourteen authored nodes and eight refinement edges and
  indexed every syntax-valid initial fixture.
- Increased the passing Rust test count from fourteen to eighteen.

### 2026-09-25 — P6 complete; P7 started

- Added expression environments, typed field selection, and nominal
  compatibility over refinement paths.
- Enforced sibling separation, no implicit narrowing, and no semantic-to-
  primitive unwrapping.
- Added validated constructors and compile-time literal constraint checks.
- Added complete record construction, `optional`, nullable values, `none`, and
  invariant collection arguments.
- Enforced semantic application signatures instead of raw primitives.
- Expanded the executable fixture suite from ten to sixteen cases.

### 2026-09-25 — P7 and Milestone A complete; P8 started

- Added the standard failure-kind catalogue and HTTP mappings.
- Added failure contracts with explicit public/internal disclosure schemas.
- Enforced direct and transitive failure propagation and typed rejection
  payloads.
- Added required-context and duplicate-code diagnostics.
- Enforced the initial function/action effect boundary.
- Derived route failure responses from action contracts.
- Expanded `jadpo inspect` with inferred expression types, failure
  contracts, disclosure fields, and route mappings.
- Expanded the fixture suite to twenty-one paired cases.
- Verified twenty-four Rust unit tests plus all documentation tests.
- Completed the first semantic compiler milestone and opened P8.

### 2026-09-25 — Project structure workstream added

- Added project-layout standardisation as an explicit P8–P12 workstream.
- Recorded the goal of reducing navigation and low-value structural decisions
  for both developers and LLMs.
- Kept enforcement open pending concrete candidate trees and pressure tests.
- Required the P8 proposal to distinguish authored, generated, test, local,
  secret, and deployment material.
- Added staged enforcement criteria and explicit small, large, multi-domain,
  integration-heavy, and multi-application cases.

### 2026-09-25 — Deterministic scaffolding added to the workstream

- Selected a static canonical base as the initial scaffolding hypothesis.
- Required human prompts and LLM creation to map to the same finite command
  options and produce identical output.
- Deferred dynamic application profiles until repeated project shapes justify
  them.
- Defined future capabilities as additive packs that cannot rearrange the base
  architecture.
- Required scaffold version and creation recipe metadata for reproducibility
  and safe upgrades.

### 2026-09-25 — Nested structured-field implementation gap closed

- Made selection through a structured field traverse its declared named record
  rather than looking for a synthetic path-dependent record shape.
- Preserved the nested field's declared nominal identity: selecting
  `customer.billing_address.postal_code` now infers `Address.postal_code`.
- Added a dedicated diagnostic for selecting through a nullable structured
  field before handling `none`.
- Added fixture pair 22 with inferred-type and refinement assertions, plus
  fixture pair 23 for nullable-selection rejection and its exact primary span.
- Kept the verified suite at twenty-four Rust unit tests plus all documentation
  tests.

### 2026-09-25 — Comparative falsification protocol strengthened

- Required behaviourally equivalent language and TypeScript applications to
  follow the same acceptance suite and ordered change sequence.
- Added per-change capture of tokens, context, time, attempts, diagnostics,
  defects, interventions, source changes, workarounds, and escape hatches.
- Required every compiler-friction incident to be classified and given an
  explicit disposition, with language changes entering the issue and fixture
  process before retry.
- Made an honest qualitative review and the possibility of a TypeScript win,
  framework pivot, redesign, or stop decision part of the P12 exit gate.

### 2026-09-25 — P8 complete; P9 next

- Added `jadpo artifacts` with seven versioned JSON outputs beneath the
  compiler-owned `build/` boundary.
- Derived route/callable inventories, failure disclosure audit, validator plan,
  public-code compatibility snapshot, and the seed's OpenAPI 3.1 subset.
- Normalised metadata source paths so relative and absolute invocations are
  byte-identical, and excluded `build/` from source discovery.
- Added `jadpo new` with a deterministic compiling health-route scaffold,
  reproducible recipe metadata, and overwrite protection.
- Documented three concrete layout candidates and selected the elastic
  single-application tree for P9–P11 pressure testing.
- Increased the verified suite from twenty-four to thirty-one Rust unit tests,
  plus all documentation tests.

### 2026-09-25 — P9 and Milestone B complete; P10 next

- Upgraded `jadpo build` from a placeholder to checked TypeScript/Bun target
  generation while retaining the P8 artifact set.
- Generated dependency-free semantic validators, closed record boundaries,
  action execution, exact output validation/serialisation, request IDs, route
  dispatch, domain-failure mapping, and generic fault containment.
- Kept input validation local to the request boundary so output contract defects
  become contained 500 faults rather than misleading client 400 responses.
- Rejected authenticated target generation explicitly until the P11 runtime
  exists; no route is silently weakened to public access.
- Added six real HTTP acceptance cases for success, malformed email, constrained
  invite codes, malformed JSON, unknown fields, automatic 422 mapping, and
  internal-context non-disclosure.
- Verified thirty-three Rust unit tests, all documentation tests, Clippy with
  warnings denied, Bun target bundling, and six Bun HTTP tests.
- Completed the first runnable compiler milestone.

### 2026-09-25 — P10 typed create slice running

- Added `create Entity { ... }` to the lexer, parser, syntax tree, nominal type
  checker, effect checker, semantic traversal, and Bun target generator.
- Required entity-field identities at persistence construction and inferred the
  created entity as the expression result.
- Added `EFFECT_FUNCTION_PERSISTENCE`; persistent writes are action-only rather
  than silently permitted in pure functions.
- Generated deterministic entity metadata, matching Postgres and SQLite fresh
  schemas, `$n`/`?n` parameterised inserts, and a Bun SQLite adapter.
- Validated every `INSERT ... RETURNING` row as its entity before returning it
  to application code.
- Added the persistence seed, fixture pairs 24–25, and two real HTTP/SQLite
  acceptance cases proving invalid requests write nothing and valid rows round
  trip through storage.
- Added a dual generated adapter that selects PostgreSQL from `DATABASE_URL`
  and otherwise uses SQLite, without changing the source-language operation.
- Ran the PostgreSQL path against a fresh Postgres 16 cluster and added two
  equivalent HTTP/database acceptance cases.
- Verified thirty-four Rust unit tests and ten Bun HTTP tests for the create
  slice before beginning typed reads.
- Added `query optional Entity { where: field == value }` with nominal predicate
  typing, `Entity?` inference, action-only effect enforcement, and an explicit
  rejection for nullable predicate fields until null matching is specified.
- Generated parameterised `SELECT ... LIMIT 2` operations for SQLite and
  PostgreSQL, validated a single selected row, returned `none` for no row, and
  contained duplicate-row cardinality violations as operational faults.
- Added fixture pairs 26–28 and six cross-adapter HTTP/database cases covering
  no row, one row, and duplicate rows. The verified runtime suite is now sixteen
  Bun tests.

### 2026-09-25 — Developer tooling workstream added

- Made IDE support part of the product hypothesis rather than post-launch
  polish, with a compiler-backed LSP as the single semantic implementation.
- Staged lexical highlighting, navigation/hover/completion, formatting and code
  actions, renderer adapters, and agent-facing retrieval separately.
- Required a stable machine language identifier across file associations,
  Markdown fences, LSP, and grammar packages without forcing an early product
  name.
- Separated model output from client rendering: agents can emit correctly
  tagged fenced code, while highlighting depends on the host installing or
  recognizing the grammar.
- Added a P12 tooling freeze gate so the language-versus-TypeScript comparison
  records IDE, formatter, renderer, and agent-context support on both sides.

### 2026-09-25 — Checked local development loop scheduled

- Added DX0.5 during P10.5 as a required gate before P11 application authoring,
  so the golden todo is built through the intended human/agent feedback loop.
- Scheduled versioned JSON diagnostics, `jadpo watch`, and `jadpo dev`
  over the same authoritative compiler pipeline used by one-shot commands.
- Required atomic generated-output promotion, last-known-good serving during
  invalid edits, visible stale-revision state, readiness-gated Bun activation,
  automatic restart/reload after successful builds, and clean shutdown.
- Required protocol tests for valid/invalid/recovered edits, coalesced events,
  source creation/deletion, target and startup failures, continuity, ignored
  compiler-owned output, and child-process cleanup.
- Bound the later VS Code client and DX1 LSP to the same diagnostics and
  lifecycle state rather than terminal-text scraping or editor-only semantics.

### 2026-09-25 — Generated runtime dependency closure enforced

- Promoted “dependency-free Bun target” from a description to an accepted
  compiler contract: generated imports are limited to `bun`, `bun:*`, and
  compiler-owned relative TypeScript modules.
- Added `JADPO_TARGET_EXTERNAL_MODULE` and
  `JADPO_TARGET_DEPENDENCY_MANIFEST` generation guards so a future target change
  cannot silently introduce a registry dependency, package manifest, lockfile,
  or `node_modules` tree.
- Kept the deterministic scaffold free of package metadata and added a focused
  regression test for both rejected dependency forms.
- Verified thirty-five Rust unit tests, strict Clippy, and generated Bun
  bundling/HTTP execution with auto-install disabled. No `bun install` step is
  part of the project lifecycle.

### 2026-09-25 — Route item separators made consistent

- Standardised route metadata on the field-style `name: value` form: `auth:`,
  `input:`, `output:`, and `run:`.
- Updated canonical applications, compile fixtures, scaffold output, grammar,
  syntax examples, and failure-model documentation.
- Made the parser reject the former space-only spelling with a focused
  diagnostic regression test.
- Verified thirty-six Rust unit tests, formatting, and strict Clippy.

### 2026-09-25 — Required lookup and persistence fault boundary

- Added `query required Entity { ... missing: FailureName }` with a non-nullable
  result type and checked optional public/internal failure context.
- Required the missing failure to be declared by the enclosing action and to
  derive from `NotFound`; absence now becomes a typed rejection, never an
  untyped exception.
- Added three compile fixture pairs covering the valid form, a wrong failure
  kind, and undeclared propagation.
- Normalised SQLite and PostgreSQL driver exceptions into a compiler-owned
  `PersistenceFault` that retains the raw exception only as its cause.
- Added required-query and real driver-failure runtime coverage to both storage
  adapters while retaining dependency-free Bun output.
- Verified thirty-seven Rust tests, strict Clippy, dependency-free no-install
  bundles, and twenty-two generated Bun HTTP/database tests across SQLite and
  PostgreSQL.

### 2026-09-25 — Transactional typed update and delete

- Added `update required` and `delete required` expressions returning the
  validated affected entity rather than a row count.
- Required explicit `missing:` (`NotFound`) and `conflict:` (`Conflict`)
  bindings with ordinary declaration, propagation, context, and type checks.
- Limited the first update slice to one nominally typed field and added five
  fixture pairs covering valid mutations, update shape, and failure kinds.
- Generated parameterised SQLite/PostgreSQL update and delete statements plus
  deterministic mutation metadata.
- Preflighted up to two rows and executed mutations inside adapter-native
  transactions so multiple matches roll back without partial writes.
- Classified native SQLite and PostgreSQL constraint codes at the adapter
  boundary and converted update violations into the declared domain conflict.
- Verified thirty-eight Rust tests, strict Clippy, dependency-free no-install
  bundles, and thirty-six Bun HTTP/database tests across SQLite and PostgreSQL.

### 2026-09-25 — Relationship and joined-loading workstream added

- Promoted entity relationships from a generic deferred concern to an explicit
  P10/P11 deliverable, using `User` with ordered `Todo` children as the golden
  pressure case.
- Required nominal references, generated foreign keys/indexes, explicit delete
  lifecycle, typed nested result shapes, and bounded eager loading without
  hidden lazy queries.
- Required the compiler to choose and expose a join or bounded batched plan,
  preserve parent pagination, validate every row, and avoid Cartesian
  multiplication for multiple to-many loads.
- Added cross-adapter evidence for zero/one/many children, relationship
  integrity, deterministic ordering, lifecycle behaviour, nested JSON parity,
  and a query-count guard against N+1 regressions.
- Made policy composition across parent and child relationships a P11 security
  gate rather than assuming a database join is authorization-safe.

### 2026-09-25 — Entity identity, uniqueness, and indexes implemented

- Accepted postfix `identity`, `unique`, and `index` modifiers on entity fields
  while keeping storage properties out of nominal value compatibility.
- Added semantic rejection for persistence modifiers outside entities,
  nullable identity fields, and multiple identity fields on one entity.
- Generated stable named primary-key and unique constraints plus idempotent
  lookup indexes for both SQLite and PostgreSQL, including runtime schema
  initialisation and persistence-manifest metadata.
- Added an `Account` pressure entity and SQLite evidence that identity and
  unique duplicates fail while the declared non-unique index exists.
- Added an optional postfix `conflict:` binding to `create`, checked it against
  the action failure set and `Conflict` kind, and proved an HTTP 409 mapping
  without exposing the underlying driver constraint.
- Preserved the dependency-free target contract: the generated TypeScript uses
  only Bun's built-in database clients and requires no installation step.

### 2026-09-25 — Owning entity relationships and foreign keys implemented

- Accepted `references Target.field on_delete restrict|cascade|set_null` on an
  owning entity field, with an explicit lifecycle required in source.
- Required the owner to carry the exact nominal target-field type and the
  target to be a non-nullable identity or unique key; nullable-only `set_null`
  is checked statically.
- Generated stable named foreign keys and automatic owner-field indexes for
  SQLite/PostgreSQL, enabled SQLite foreign-key enforcement, and ordered fresh
  table creation so referenced entities exist before dependants.
- Diagnosed cross-entity dependency cycles that the fresh-schema generator
  cannot safely order rather than emitting broken SQL.
- Added the `User`–`Todo` pressure case and executable SQLite evidence for
  orphan rejection, owner indexing, and cascade deletion. Equivalent live
  PostgreSQL assertions are checked in when `DATABASE_URL` is available.

### 2026-09-25 — Ordered many-result queries implemented

- Accepted `query many` with a bound equality predicate and mandatory explicit
  `order_by: ... asc|desc` syntax.
- Required an identity/unique ordering key so collection order is deterministic
  without relying on adapter row order or an unstated tie-breaker.
- Generated parameterised SQLite/PostgreSQL many-result statements, exposed
  them in persistence metadata, and validated every returned entity.
- Added `List<T>` TypeScript generation and collection validation without any
  runtime dependency.
- Proved the `User`–`Todo` route returns todos in stable identity order on
  SQLite; the PostgreSQL suite contains the equivalent live assertion.

### 2026-09-25 — Relationship declaration syntax revisit queued

- Recorded the current owning-reference spelling as provisional rather than a
  stable language commitment.
- Scheduled a syntax comparison after inverse relationships and the joined-load
  pressure case reveal the full requirements.
- Kept ownership, cardinality, nominal target identity, lifecycle, inverse
  naming, composite-key viability, and IDE/LLM readability as explicit review
  criteria.

### 2026-09-25 — Explicit inverse loading implemented

- Accepted `inverse todos: many Todo via Todo.owner_id` as the first named
  parent-side relationship declaration, checked against the stored owning
  reference rather than treating it as a persisted field.
- Accepted one explicit required-parent include with a named output shape and
  mandatory identity/unique child ordering.
- Selected a bounded two-query batch for the first pressure case and exposed
  its strategy, cardinalities, query count, ordering, and SQL in persistence
  metadata.
- Validated the parent and every child before constructing nested output;
  zero children become an empty list and a missing parent maps to the declared
  typed failure.
- Added SQLite runtime coverage for zero/one/many children, cross-user
  isolation, missing parents, and the two-query plan. The PostgreSQL suite
  carries the equivalent nested result checks for a configured live database.

### 2026-09-25 — Explicit relationship pagination implemented

- Required every included to-many relationship to declare a positive literal
  `limit` and non-negative literal `offset` after its deterministic ordering.
- Rejected dynamic, zero, and negative bounds in the first slice so query cost
  remains statically visible and reviewable.
- Lowered both bounds to parameters in SQLite and PostgreSQL rather than
  interpolating them into generated SQL.
- Added runtime evidence that adjacent pages return the expected ordered child
  rows and metadata evidence for `LIMIT $2 OFFSET $3` query planning.

### 2026-09-25 — Parent-first paginated relationship joins implemented

- Accepted one inverse include on `query many` only when parent and child
  ordering and pagination are both explicit and statically bounded.
- Generated a parent-page CTE before child expansion, then ranked children per
  parent and applied each child window before a single left join.
- Regrouped flat adapter rows into `List<OutputShape>`, preserving parents with
  zero children and validating every nested value at the application boundary.
- Exposed `strategy: parent_page_join`, `query_count: 1`, the
  parent-before-join guarantee, adapter SQL, and all parameter positions in the
  persistence manifest.
- Added SQLite evidence that a parent with more children than its child limit
  does not consume another parent's page slot, plus stable adjacent parent-page
  behavior. Equivalent PostgreSQL coverage is checked in for a configured
  database.

### 2026-09-25 — Bounded multiple relationship includes implemented

- Changed query syntax and the AST from one optional include to repeated
  includes that must name one exact output shape.
- Rejected duplicate relationships, mixed result shapes, missing relationship
  fields, and independently invalid ordering or pagination.
- Lowered required-parent multiple includes to one parent query plus one
  bounded child query per relationship.
- Lowered paginated many-parent multiple includes to independent parent-page
  joins merged by the unique parent ordering key, avoiding Cartesian row
  multiplication and N+1 execution.
- Added `User`–`Todo`–`Note` SQLite/PostgreSQL pressure cases and plan metadata
  with exact query counts and `cartesian_product_avoided: true`.

### 2026-09-25 — Atomic fixed-shape multi-field updates implemented

- Allowed `update required` to name one or more distinct fields in `set:` and
  retained nominal type checking for every replacement.
- Rejected duplicate fields explicitly while keeping omission-aware dynamic
  patch selection outside this fixed-shape slice.
- Lowered every authored field set to one parameterised
  `UPDATE ... SET ... RETURNING` statement and recorded the exact SQLite and
  PostgreSQL SQL in persistence metadata.
- Added an `Account` pressure route proving two fields update together and that
  a uniqueness conflict leaves both previously stored values unchanged.
- Verified 51 Rust tests, strict Clippy, deterministic regeneration, and 29
  generated Bun HTTP/database tests against SQLite. The equivalent PostgreSQL
  assertions are checked in pending a supplied live `DATABASE_URL`.

### 2026-09-25 — Named compound constraints and precise conflicts implemented

- Added `constraint name: unique(field_a, field_b)` to entities, rejecting
  unknown, repeated, nullable, undersized, or duplicate field sets.
- Emitted stable named compound uniqueness for SQLite and PostgreSQL and
  recorded authored names plus ordered fields in persistence metadata.
- Allowed mutations to map several compiler-owned constraint identities before
  one optional fallback; duplicate and unknown mappings fail compilation.
- Normalised native identity, single-field unique, and compound-unique failures
  to the same safe logical names across both adapters without exposing raw
  database identifiers to application logic.
- Proved SQLite distinguishes `Account.handle` from
  `Account.tenant_owner`, returns their separate typed conflicts, and rolls
  back every other field in the failed update.
- Verified 53 Rust tests, strict Clippy, deterministic regeneration, and 29
  generated Bun HTTP/database tests. Equivalent PostgreSQL assertions are
  checked in pending a supplied live `DATABASE_URL`.

### 2026-09-25 — P10 persistence core complete

- Ran all 22 generated PostgreSQL acceptance cases against an isolated fresh
  Postgres 16 cluster, covering typed CRUD, relationships, bounded repeated
  includes, exact cardinality, inferred action rollback, multi-field atomicity,
  constraint-specific conflicts, fallback conflicts, and raw driver failures.
- Reproduced a Bun 1.2.20 PostgreSQL transaction defect where prepared/pipelined
  constraint failures rolled back on the server but left `SQL.begin()`
  unresolved. Generated PostgreSQL clients now use `prepare: false`; parameter
  binding remains intact while transactions complete and release reliably.
- Made the live test setup repeatable by removing its test-only partial unique
  index before each suite run.
- Verified 53 Rust tests, strict Clippy, 29 generated Bun/SQLite HTTP/database
  tests, and 22 live PostgreSQL HTTP/database tests.

### 2026-09-25 — Query clauses made structurally consistent

- Changed query, update, and delete syntax to consistently label clause values:
  `where:`, `order_by:`, `limit:`, `offset:`, `include:`, and `into:` now match
  the existing `set:`, `missing:`, and `conflict:` convention.
- Made the colon mandatory rather than retaining two accepted spellings, with a
  targeted missing-colon parser diagnostic for every clause label.
- Updated the grammar, design examples, persistence seed, compile fixtures, and
  generated persistence target.
- Verified 55 Rust tests, strict Clippy, and 29 generated Bun/SQLite
  HTTP/database tests.

### 2026-09-25 — Typed configuration and deployment validation queued

- Added a P11/P12 workstream for one typed configuration contract shared by
  source checking, generated documentation, IDE/LLM context, CI, runtime, and
  deployment systems.
- Initially separated static compiler checks from an environment-bound
  preflight so normal source analysis would not require production secrets.
  The later approved CONFIG-001 design keeps that separation but makes real
  value validation automatic in development, deployment, and startup.
- Initially queued environment overlays and a restart-versus-reload decision.
  The approved design instead has one `.env.local`, no implicit overlays, and
  startup-bound values only.
- Split liveness from readiness and required bounded dependency capability
  checks without exposing configured values.
- Defined rollback as a deployment-controller action driven by deterministic
  validation/readiness evidence, with an executable no-promote/rollback test.

### 2026-09-25 — Assurance and validation reset inserted

- Recorded that compiler and persistence construction overtook the charter's
  requirement to let complete golden programs force the semantic model first.
- Reclassified completed compiler work as exploratory implementation evidence,
  not validation of the language or product thesis.
- Capability-froze P10 and inserted mandatory P10R gates for the complete todo
  design, policy/proof kernel, threat model, independent human approval,
  first-user hypothesis, preregistered comparison thresholds, and documentation
  authority.
- Required repeated counterbalanced TypeScript/language trials, independent
  adjudication, explicit critical-safety and valid-work-friction thresholds,
  and a recorded continue/redesign/framework-pivot/stop decision.
- Defined human approval as a research problem in behavioural comprehension,
  not an assumption that conventional pull-request code review remains viable;
  P10R must compare code diffs with relationship- and effect-aware decision
  views and measure false confidence as well as review accuracy.
- Recorded the custom web UI as the primary opportunity for intent-first,
  interactive semantic-graph, scenario, effect, and per-decision review, while
  keeping all claims reproducible from compiler-owned artifacts.
- Preserved the planned technical scope; the correction changes evidence order
  and decision quality rather than reducing the intended backend domain.
- Added the candidate `todo-v0.1` source/policy contract, 39 black-box cases,
  adversarial sequence, expected audit, TypeScript baseline, and explicit
  language-friction ledger without claiming current compiler support.
- Added candidate proof, threat, independent-approval, research-user, and
  comparison protocols plus deterministic package/link/digest verification.
- Ran an author-side contradiction pass that corrected policy authority,
  overdue-reminder semantics, due-date rescheduling, relationship lifecycle
  coverage, application-owned identity data, deployment-plane readiness, and
  explicit success status behaviour. Independent review remains pending.
- Added omission-aware `patch:` syntax, semantic and failure checks, generated
  supplied-flag SQL plans, manifest evidence, four compiler fixtures, and an
  end-to-end SQLite test covering empty, omitted, explicit-`none`, and supplied
  fields. Also preserved nullability through qualified field references.
- Added atomic patch-derived `set:` writes with the narrow
  `value when patch.field supplied` rule, rejected patch/derived overlap, added
  positive and negative compiler fixtures, and proved at runtime that omission
  preserves the derived column while the triggering field clears it.
- Added explicit owning-parent includes for required-child queries with
  `required`/`optional` cardinality, exact nested output checking, a bounded
  two-query generated plan, persistence-manifest evidence, compiler fixtures,
  and SQLite coverage for required plus null/populated optional references.
- Added unique-backed `inverse ...: optional ... via ...` declarations,
  rejected non-unique owning fields, generated bounded optional-inverse plans,
  and proved absent, present, and duplicate-child behavior on SQLite.
- Added a depth-two `include: owner.profile optional` path with exact nested
  output checking, a fixed three-query plan, maximum-depth metadata, a compiler
  fixture, and SQLite evidence for absent and present leaves.
- Reviewed owning-reference surface syntax and implemented the explicit
  `references User.id as owner on_delete ...` form. Generated schemas retain
  `owner_id`, while semantic metadata, direct/nested includes, result fields,
  and bounded plan names use `owner`; fixture 50 and SQLite runtime coverage
  exercise the separation without suffix inference.
- Closed the remaining relationship-shape review without inventing false
  guarantees: required inverse totality remains rejected, nested collections
  require future per-hop bounds, many-to-many uses an explicit join entity, and
  recursive paths stay depth-bounded. Drafted the checked-in, rename-stable
  schema identity registry contract as the next implementation boundary.
- Implemented the schema identity registry boundary: deterministic
  non-overwriting initialization, canonical parsing, validation during normal
  compiler commands, addition-only registration, and explicit entity/field
  renames that retain immutable IDs, physical names, and prior paths. Manual
  drift and additions mixed with removals fail closed.
- Added immutable schema identity snapshots and deterministic identity-only
  change sets for additions, removals, logical/physical renames, and
  incompatible kind/owner changes. Output is explicitly marked
  `migration_plan: false`; no SQL or lifecycle approval is inferred.
- Extended immutable snapshots with nominal field types, nullability,
  persistence flags, intrinsic constraints, relationship target/name/delete
  lifecycle, and constraint/index members. Diffs now distinguish required from
  nullable additions and conservatively route every field-shape change to an
  existing-data decision.
- Added the fail-closed decision-requirement boundary. Existing-data and
  lifecycle blockers now expose typed unresolved requirements, admissible
  strategies, and mandatory evidence slots; reference delete-action changes
  are classified separately. No requirement is treated as a decision or
  approval, and migration planning remains false.
- Added non-overwriting authored decision templates bound to the complete
  canonical change set. Validation rejects stale bindings; missing, duplicate,
  or unexpected decisions; inadmissible strategies; and missing, unexpected,
  duplicate, or empty evidence. Passing validation proves completeness only and
  continues to emit `migration_plan: false`.
- Added immutable ordered PostgreSQL/SQLite migration review plans gated on
  successful decision validation. Plans distinguish adapter operations,
  classify rollback as automatic, conditional, or unavailable, and index every
  irreversible step. Evidence remains uncompiled, so plans are explicitly
  non-executable and contain no SQL.
- Added deterministic static query-backed index advice. Normal checking warns
  when recommendations exist; the evidence view records the callable and
  predicate/ordering role. Explicit acceptance updates authored source,
  rechecks it, registers the new index identity, and restores source if the
  operation fails. Existing identity, unique, explicit, and reference-backed
  indexes are suppressed; workload statistics and compound/partial advice
  remain future adapter work.
- Added the first reviewed SQL subset without creating an execution path.
  Primitive `literal(...)` backfills are checked against field types;
  identifiers/text are escaped; nullable additions emit forward/rollback SQL
  on both adapters; and required additions with backfill emit the PostgreSQL
  add/backfill/not-null sequence. Unsupported rebuild, transform, removal, and
  lifecycle paths fail closed.
- Added a bounded SQLite required-field rebuild for one primitive addition on
  an unrenamed table. The review contains transactional shadow-table creation,
  typed copy/backfill, table swap, rollback reconstruction, and foreign-key
  integrity checks. Existing scalar references retain their target and delete
  action; their generated lookup indexes, explicit indexes, and named compound
  uniqueness that excludes the new field are recreated. Multi-change rebuilds,
  constraints on the new field, and unsupported table shapes still fail
  closed. The forward and rollback sequences also pass a real SQLite smoke
  execution with preserved data and schema metadata.
- Extended the bounded SQLite review generator to coalesce compatible field
  additions by table. One shadow-table copy can now add multiple required
  primitive fields with independently checked literal backfills together with
  nullable fields populated as `NULL`; rollback removes the complete addition
  set in one reconstruction. Multiple affected tables remain deterministic
  units whose rollback order reverses the forward order. Added-field
  constraints, transforms, removals, and lifecycle changes still fail closed.
- Classified required-to-nullable field changes as safe nullability widenings
  rather than generic existing-data transformations. PostgreSQL review SQL now
  drops `NOT NULL` with a restoring rollback; SQLite rebuilds the current
  nullable table and reconstructs the prior required field on rollback. Both
  paths remain non-executable review artifacts, and narrowing or any concurrent
  incompatible SQLite change still fails closed.
- Added evidence-checked nullable-to-required narrowing. The authored decision
  must select `validate_existing` with the compiler-owned `not_null` predicate;
  arbitrary non-empty predicate claims and staged SQL requests fail closed.
  PostgreSQL emits a visible null-count review followed by enforcing `SET NOT
  NULL`; SQLite copies into a required shadow-table column, so remaining nulls
  prevent the rebuild. Both adapters emit a nullable rollback.
- Added the bounded authored module/import core. Explicit logical module
  headers, selective imports, and private-by-default/public declarations now
  produce manifest metadata and enforce cross-file visibility. Mixed
  ambient/modular projects, missing/private/duplicate/conflicting imports,
  duplicate modules, self-imports, and cycles fail with stable diagnostics. A
  two-file module seed passes real `check` and `inspect`; aliases, re-exports,
  relative imports, multi-file modules, external packages, and namespace-local
  duplicate names remain explicitly unsupported.
- Closed the P10.5 value/reference boundary with executable assignment syntax.
  `var mut` locals may be rebound only with values compatible with their
  established nominal type;
  parameters and ordinary `var` bindings reject assignment with
  `TYPE_ASSIGN_IMMUTABLE`. Assignment is local-slot rebinding, never field or
  caller-visible mutation. Fixtures 51–52 and generated TypeScript coverage
  preserve this contract, while `ref`/alias/address/`inout` syntax remains
  absent pending real application evidence.
- Began DX0.5 with the versioned machine diagnostic protocol. `jadpo check
  <project> --diagnostic-format=json` emits one deterministic version-1 report
  on success or failure, preserving diagnostic ordering, stable codes, severity,
  source byte ranges, notes, summary counts, and process exit status. Human
  output remains the default and both modes call the same checking pipeline.
- Added `jadpo watch` over the authoritative checked build. Stable content
  fingerprints cover authored sources and the schema identity registry while
  excluding generated/staging output; a 75 ms quiet window coalesces rapid
  saves. Human and versioned JSON events expose checking, success, failure,
  monotonic revisions, and stale retained output. Artifact generation now
  completes in a sibling staging tree and promotes only a complete revision,
  with the previous `build/` restored on promotion failure. Unit coverage proves
  generated-output exclusion and last-complete-build preservation; the real
  long-running JSON watcher also passes its initial-build smoke test.
- Added the first `jadpo dev` runtime loop. It validates `PORT`, starts Bun
  with dependency installation disabled, bounds readiness to a three-second
  localhost probe, preserves a ready server through invalid source revisions,
  restarts only after successful builds, and reports unexpected exit plus
  start/readiness/failure events. JSON runtime logs stay off the protocol stream.
  Bun 1.2.20 passed a real readiness event and HTTP 200 smoke request; Ctrl-C
  left no listener. Startup-failure restoration and portable structured
  shutdown remain explicit DX0.5 work.
- Closed the initial diagnostic/readiness presentation gap. Every generated
  target now supplies `GET /health` unless an authored public health route
  replaces it, and `jadpo dev` requires HTTP 200 from that endpoint rather
  than treating an open socket as ready. Human compiler diagnostics render the
  stable machine byte range as line/column, a bounded source excerpt, caret, and
  repair notes. Generated request/startup faults emit versioned compact JSON;
  raw target stacks are opt-in through `JADPO_DEBUG_TARGET_STACKS=1`.
- Completed the pre-P11 language-learning slice with plain closed enums
  and typed `match`. Enum variants are qualified nominal values, boundary
  validators reject unknown wire spellings, generated TypeScript uses string
  unions and `switch`, and OpenAPI plus validator plans publish the finite
  alternatives. Matches are exhaustive for enums and `Bool`; nullable `none`
  is explicit; open Text/numeric/value spaces require `_`; duplicate, unknown,
  unreachable, and incomplete patterns have stable diagnostics. Match subjects
  include child field selections and callable results. Data-carrying variants
  now use nominal payload fields, a validated closed `tag` representation,
  discriminated OpenAPI unions, checked construction, arm-local bindings, and
  exhaustive narrowing. Nullable matches support `some(value)` binding. The
  expression parser has fixed unary, arithmetic, ordering, equality, `and`, and
  `or` precedence with nominal operand checks. Top-level `test` plus Boolean
  `assert` compile to a structured `jadpo test` runner without target stack
  traces. `jadpo fmt` and read-only `--check` share deterministic formatting
  with the VS Code client. The standard `jadpo lsp` service now supplies live
  unsaved diagnostics, document/workspace symbols, cross-file definitions and
  references, inferred hover types, contextual completion, signature help,
  semantic tokens, compiler-indexed rename, and formatting without an editor-
  specific parser. Fixtures 53–58 cover the successful and principal language
  failure paths; protocol tests cover Unicode positions and cross-file imports.

### 2026-09-26 — Guided diagnostics and safe observability accepted

- Accepted one versioned compiler-owned diagnostic catalogue in place of
  scattered message strings and editor-specific advice.
- Selected readable lower-case dotted rule identifiers, human-first summaries,
  concise reasons, one recommended next step, bounded alternatives, explicit
  decision ownership, precise locations, bounded context/impact, help IDs, and
  revision-bound repair previews as the version-2 contract.
- Required IDEs to render the same semantic object through precise squiggles,
  plain Problems summaries, explanatory hover, a recommended verified Quick Fix
  followed by clean alternatives, multi-file diff preview, and expandable
  impact/help rather than raw JSON.
- Separated compiler diagnostics, public failure responses, operational events,
  and agent incident packets into non-interchangeable audience schemas.
- Required safe typed diagnostic/log values, non-renderable secrets, no
  assumption that internal context is loggable, and planted-secret tests across
  browser, log, trace, buffer, and third-party adapter outputs.
- Chose redacted structured/OpenTelemetry-compatible production events carrying
  stable semantic operation and source-revision IDs, with trusted local
  compiler-graph enrichment producing the rich runtime packet used by an LLM.
- Added DX2 and its catalogue, IDE, repair, disclosure, compatibility, and
  repair-cycle exit evidence before the P12 tooling freeze.

### 2026-09-26 — Callable execution and outcome matching accepted

- Kept authored target scheduling out of the language: functions are pure and
  non-suspending, actions are runtime-managed operations that may suspend, and
  ordinary action calls always complete before their caller continues without
  authored `async`, `await`, promises, or detached calls.
- Kept routes as action boundaries. One-off behaviour remains an inline action,
  named `run:` behaviour remains action-only, and pure functions remain reusable
  from either action form.
- Split fallible-expression acknowledgement into two canonical forms:
  `attempt` propagates every failure, while exhaustive outcome `match` handles,
  maps, recovers from, or explicitly propagates each success and failure case.
- Required the compiler-derived escaping failure set to equal the authored
  `fails` list exactly. Bare fallible calls, missing or stale declarations,
  failure wildcards, and non-exhaustive outcome matches are invalid.
- Required IDE hover over a function or action declaration, reference, or call
  to show its successful result and complete failure surface, plus statically
  known call-site handling and action completion semantics, without exposing
  generated `Promise` or `Result` wrappers.
- Added P10.7 as the fixture-first implementation phase before P11. Structured
  parallelism, cancellation, and durable background-work syntax remain separate
  decisions rather than implicit extensions of ordinary calls.

### 2026-09-26 — Problem, route, and entity boundary revision accepted

- Accepted an explicit `kind` member for reusable application failure
  declarations, while retaining stable public codes, safe messages, and
  declaration-owned public/internal context schemas.
- Selected flat failure construction with exact completeness, name, scope, and
  nominal-type checks at every production site.
- Expanded the planned callable contract from domain-only rejection to an
  exhaustive set of recoverable application and operational problems: each
  problem must be handled, mapped, or propagated in `fails`.
- Selected `attempt` for explicit propagation. P10.7 subsequently assigned
  local handling, mapping, recovery, and selective propagation to exhaustive
  outcome `match`, avoiding a second handler grammar inside `attempt`.
- Selected source-agnostic operational categories rather than storage/provider-
  qualified names, with provenance retained in compiler/runtime metadata.
  `Unavailable`, `TimedOut`, `RateLimited`, and `OutcomeUnknown` are the initial
  candidates, and unknown write outcomes remain distinct from definite
  unavailability.
- Kept defects and impossible states outside catchable problem flow.
- Kept `route`, `action`, and `function` as distinct concepts while selecting a
  local-first route form: one-off behaviour is an inline brace-delimited action;
  reusable or independently meaningful behaviour is a named action invoked by
  `run:`. A route has exactly one of those forms.
- Made inline `fails` explicit in the action header, retained field-style route
  members, and rejected indentation-sensitive nesting and decorator syntax.
- Retained authentication-required-by-default and selected `auth: none` as the
  sole initial opt-out. Authentication remains separate from policy and from
  provider strategy configuration.
- Selected `{name}` path placeholders with a typed `path: { name: Type }` group,
  `path.name` access, and exact one-to-one compiler checks across multiple path
  parameters. Query, header, and body field spelling remains open.
- Initially accepted `entity` as identity-bearing domain data with optional
  persistence and entity-local policy. The unified authored type revision later
  the same day superseded this data-model portion with `type` plus separate
  `persist`; it did not settle P11 policy ownership or approval syntax.
- Added P10.6 before P11 to implement and pressure-test the revised contracts
  without rewriting the already-recorded P7/P10 evidence.

### 2026-09-25 — Public website documentation benchmark queued

- Added a conditional public-facing website and documentation benchmark
  workstream for activation after the P12 continuation decision.
- Fixed the initial case-study set across Rust, Stripe, Django, ArchWiki,
  PostgreSQL, Godot, and Twilio, with complementary specialist and current
  developer-portal references.
- Defined a shared evidence standard, twelve-part assessment rubric, required
  task observations, cross-case decision artifacts, prototype coverage, and an
  exit gate that must pass before production website design begins.

### 2026-09-26 — Unified authored type model implemented

- Made `type Name = Object { ... }` and `type Name = Enum { ... }` executable,
  including recursively closed inline objects and `List<Object { ... }>`.
- Added separate `persist Name { ... }` parsing and normalization so existing
  checked persistence, migration, SQL, and runtime behavior is retained without
  making object declarations storage-bearing by default.
- Made ordinary object types valid wherever an input, output, include
  projection, or omission-aware patch shape is required.
- Added the accepted prelude representation/time/container names and validated
  `Email`, `Url`, and `IpAddress` constructors, target validators, and OpenAPI
  formats. The former canonical authored `Email` declaration is recognized as
  a migration-compatible spelling during project analysis.
- Accepted callable `fails` before the success arrow and colon-delimited scalar
  constraints, while keeping the former positions readable during fixture
  migration.
- Removed LSP document links and the VS Code document-link provider. Generated
  OpenAPI and validation plans now use explicit commands, leaving source
  underlines exclusively for diagnostics.
- Fixed the formatter to preserve indentation through multiline parameter,
  collection, and grouped-expression delimiters, including the canonical
  pre-arrow `fails` layout.
- Bound generated PostgreSQL startup connection attempts and corrected the
  secret-canary startup suite to exercise a persistence-enabled application.
- Updated the seed, persistence example, module example, scaffold, grammar,
  snippets, syntax highlighting, completion, navigation, validation plan, and
  compile fixtures to the new model.
- Left the exact colon-delimited spelling for named compound uniqueness as the
  sole syntax decision still requiring confirmation; no new canonical spelling
  is published in the grammar.

### 2026-09-26 — Roadmap alignment and DX0.5 completion

- Completed last-known-good runtime rollback: a candidate build preserves the
  ready generated revision until readiness, and failed startup restores and
  proves the prior runtime ready again.
- Added portable signal-driven `shutdown` lifecycle output after watcher and
  child cleanup, plus protocol coverage for coalesced edits, invalid-edit
  continuity, recovery, source creation/deletion, missing Bun, failed candidate
  startup, rollback, and listener cleanup.
- Reconciled the roadmap with the unified `type` plus `persist` decision by
  removing entity-local policy from P10.6's exit gate and assigning policy
  ownership/protected-source/approval syntax to P11.
- Reconciled the syntax and semantic model with the accepted P10.7 boundary:
  functions are pure and non-suspending, `attempt` propagates, and exhaustive
  outcome `match` owns local recovery and mapping.
- Added unattended implementation triage: assess each slice before work, park
  unresolved product/design/security decisions with a named question and exit-
  gate impact, and continue only with independent fixture-backed work whose
  semantics are already accepted.

### 2026-09-26 — P10.7 call and suspension slice implemented

- Added fixture coverage for function-to-function, action-to-function, and
  action-to-action calls, complementing the existing rejected
  function-to-action case.
- Added fixed-point suspension metadata to the checked callable outcome graph
  and generated callable inventory.
- Kept pure functions and effect-free action chains synchronous in generated
  TypeScript while propagating required `async`/`await` and one persistence
  capability through suspending action calls only.
- Added compiler-backed declaration, reference, and call hover plus signature
  documentation for callable parameters, success type, complete declared
  failures, and derived completion/suspension semantics without exposing target
  `Promise` or `Result` wrappers.
- Added statically known `attempt` propagation state to fallible call-site hover.

### 2026-09-26 — Entity boundary reopened and dependent work parked

- Reopened the implemented unified `type` plus separate `persist` boundary as
  DATA-007 rather than treating the prototype as final language authority.
- Recorded the competing model: a first-class identity-bearing `entity` that
  need not persist and may provide one explicit capability surface for policy,
  lifecycle, behaviour/display, caching, graph participation, and multiple
  database adapters.
- Parked all extensions to entity/type/persist syntax and semantics, identity,
  relationships, migrations, storage adapters, entity policy, and dependent
  golden-model work until owner discussion. Existing implementation and tests
  remain comparison evidence; they are not being rolled back implicitly.

### 2026-09-26 — P10.7 exhaustive outcome slice implemented

- Added expression-level exhaustive outcome matches with one `success(value)`
  arm and one exact arm for every declared failure.
- Added local compatible-value recovery, explicit `reject` mapping, and
  `propagate`, with exact escaping-failure inference and generated TypeScript
  failure dispatch for synchronous and internally suspending calls.
- Added focused diagnostics and fixtures for invalid subjects, infallible
  subjects, missing and duplicate success arms, missing, duplicate, unknown,
  and wildcard failure arms, and success arms without a value.
- Rejected authored `async` and `await` explicitly while recovering through the
  ordinary action and call grammar.
- Added inline-action hover using the same route success/failure and derived
  completion contract as named callables.
- Added call-site hover for every locally handled, explicitly mapped, or
  propagated failure, plus definition navigation from exact failure arms.
- Added a persistence-free generated HTTP example and four Bun acceptance cases
  proving successful continuation, compatible recovery, explicit mapping, and
  exact propagation at runtime.

### 2026-09-26 — P11 unattended assessment parked on owner decisions

- Reassessed authentication-only work after the independent P10.7 slices
  completed; no implementation was started because the remaining boundary is a
  security and product decision rather than a mechanical compiler extension.
- Parked authentication provider selection, claim validation, ambiguity and
  privilege-conflict handling, actor/tenant/capability mapping, and route
  strength overrides on AUTH-001.
- At that point, parked typed secret/config sources, local-value handling,
  readiness probes, and lifecycle behaviour on CONFIG-001; those decisions are
  now approved in the 2026-09-27 CONFIG-001 entry below.
- Kept policy/proof syntax and approval ownership on POLICY-001, and entity
  ownership/lifecycle/capability plus golden-todo modelling on DATA-007.
- Confirmed that jobs/events, time injection, service import, and richer test
  harness work also have unresolved scheduled decisions and are not safe
  fallback slices.

### 2026-09-26 — DATA-007 entity/query boundary accepted

- Accepted one first-class `entity` concept for stable domain identity whether
  or not the entity is persisted; persistence is an optional explicit
  capability rather than a separate species of entity.
- Accepted one authoritative entity dossier per file under `entities/`, with
  explicit identity, capabilities, policy/lifecycle, named queries, functions,
  and actions; paths organise source but do not define semantic/schema identity.
- Accepted checked entity dot calls as qualified calls with explicit reference
  or complete-value receivers, no implicit load/save/mutation, no arbitrary-key
  receivers, and no initial extensions, inheritance, overloads, or dynamic
  dispatch.
- Accepted named read-only queries: entity-centred reads live with the entity;
  genuinely cross-entity projections live under `queries/`; raw query
  expressions do not appear in routes, functions, workflows, jobs, policy, or
  configuration.
- Accepted mutation ownership: only an entity's actions directly mutate that
  entity. Multi-entity application actions under `workflows/` compose entity
  actions without bypassing invariants or policy.
- Accepted explicit atomic intent with compiler-managed mechanics. Entity
  actions are independently failure-atomic; multiple mutation scopes require an
  authored atomic or durable-workflow disposition rather
  than silently widening a transaction from the call graph. Inside an explicit
  same-domain atomic boundary, nested entity actions and queries join it and a
  handled nested failure rolls back to a compiler-owned savepoint. Unproven
  atomicity across databases or external services is rejected.
- Opened TX-001 fixture work for the canonical intent spelling and the initial
  PostgreSQL/SQLite isolation, locking or conditional-write, deterministic
  lock-ordering, safe-retry, and savepoint matrix. These must be visible in the
  audit before multi-entity atomicity is claimed.
- Recorded the full decision in
  [entity, query, and transaction model](entity-query-model.md). Compiler
  implementation, exact punctuation, fixtures, migration, and runtime evidence
  remain incomplete and are the next technical slice.

### 2026-09-26 — Cross-store consistency boundary accepted

- Accepted exactly one authority for every mutable fact. Redis caches, graph
  views, search indexes, analytics stores, and denormalised tables derived from
  that authority are compiler-managed representations, not additional
  application write targets.
- Accepted authority-plus-durable-change-record commit as the default
  cross-store mechanism. Generated delivery is at-least-once, idempotent,
  revisioned, ordered per entity, retryable, replayable, rebuildable,
  watermark-visible, and reconcilable.
- Accepted authoritative, read-your-writes, bounded-staleness, and eventual
  query freshness. A stronger plan may satisfy a declaration; a weaker plan may
  not. Within one operation graph the compiler propagates the commit revision.
- Separated local atomic, prepared atomic, durable projection, and durable
  workflow contracts. Cross-domain atomicity is allowed only when all adapters
  prove one prepare/commit and durable-recovery protocol; it never silently
  degrades to a saga or best-effort sequence.
- Accepted persisted durable workflows for multiple real authorities, with
  idempotent steps, retry, timeout, authored compensation, reconciliation,
  operator-visible terminal states, and explicit outcome uncertainty.
- Tightened the entity/transaction boundary: mutating value receivers require
  a stale-write guard; nested success is provisional until outer commit; and
  policy, lifecycle, and invariant reads guarding writes share the transaction
  and concurrency plan.
- Opened CONSISTENCY-001 and WORKFLOW-001 fixture work. Exact declaration
  syntax, adapter capability proofs, runtime state storage, operational
  controls, and generated evidence remain implementation work.

### 2026-09-27 — Unattended entity/query/transaction foundation implemented

- Added executable entity dossiers with stable identity, optional authority
  persistence, cache/projection declarations, compiler-owned entity references,
  reference/value receivers, mutation guards, and nested entity operations.
- Added first-class named `query` callables with required freshness contracts,
  read-only effect enforcement, qualified/dot calls, and rejection of raw query
  expressions outside named queries.
- Added mutation-ownership and persistence-capability checks, recognised source
  role validation, explicit `atomic`/`durable_workflow` dispositions,
  same-domain enforcement, and guarded value-receiver writes.
- Added compiler-managed nested savepoints, entity/transaction audit artifacts,
  per-entity revisioned authority change records, and an SQLite runtime case
  proving that handled nested failure rolls back both the write and change
  record while the outer transaction can still commit.
- Added thirteen compile fixture pairs, bringing the corpus to 132, and kept target
  generation fail-closed for unimplemented physical stores and durable
  workflows.
- Parked physical derived-store delivery, replay/rebuild, watermarks,
  reconciliation, revision-token propagation, and the persisted workflow
  runtime because their adapter, state, and operational-control contracts still
  require owner decisions. AUTH-001, CONFIG-001, POLICY-001, P10R review, and
  DX2 external trials remain parked for the same reason or because they require
  human evidence.

### 2026-09-27 — AUTH-001 decision and implementation plan prepared

- Consolidated the golden todo, `AUTH-EXACTLY-ONE`, route-default, threat,
  acceptance, and audit candidates into one authentication plan.
- Made twelve owner choices explicit, including the initial strategy set,
  dependency/trusted-computing-base boundary, multi-credential selection,
  provider subject namespace, authoritative user resolution, actor shape,
  route requirements, session and OIDC validation, failures, and configuration
  ownership.
- Sequenced implementation as AUTH-P0 through AUTH-P7 with fixture-first
  compiler modeling, typed actor resolution, a strategy-independent selector,
  signed-session and OIDC adapters, artifacts/tooling, and the golden exit run.
- Defined adversarial cases, diagnostics, `audit/authentication.json`,
  dependencies on CONFIG-001/POLICY-001/TIME-001, and fail-closed stop
  conditions. No security semantics were silently selected by implementation.

### 2026-09-27 — AUTH-001 architecture approved and expanded

- Approved authentication as a common principal boundary for browsers,
  user-operated API clients, and service-to-service clients.
- Separated credential transport (cookie or bearer) from validation and
  revocation semantics (immediate authority lookup or bounded short-lived
  credential), with per-route fresh-authority checks and no cache dependency.
- Made services first-class principals with owned, expiring, rotatable,
  revocable credentials, direct opaque-key authentication, and confidential
  credential exchange for short-lived bearer access.
- Kept permissions in POLICY-001 rather than credentials, selected Bun-native
  primitives for Jadpo-issued envelopes, and retained OIDC behind a pinned
  compiler-owned standards adapter rather than an application dependency.
- Replaced the earlier AUTH-P0–P7 user-only sequence with AUTH-P0–P8 and an
  expanded browser/API/service adversarial matrix.

### 2026-09-27 — Optional JWT dependency boundary approved

- Kept browser sessions, opaque bearer credentials, API keys, and Jadpo-issued
  envelopes on Bun-native primitives with no package dependency.
- Made JWT bearer validation an explicit source capability. Only that choice
  adds the compiler-selected, exactly pinned `jose` package; its selected
  release must have zero transitive dependencies.
- Kept library, version, algorithms, validation policy, JWKS behavior, and
  resource limits compiler-owned rather than developer choices.
- Required non-JWT output to contain no external import, package metadata,
  dormant JOSE code, or install step, while JWT output records exact integrity,
  provenance, license, advisories, and dependency closure.
- Refined the product claim to: zero package dependencies by default; enabling
  JWT authentication adds one pinned dependency with zero transitive
  dependencies.

### 2026-09-27 — CONFIG-001 architecture approved

- Put each explicit environment binding beside its typed field in a structured
  declaration, eliminating separate per-environment binding files and
  space-separated lifecycle modifiers.
- Selected one local `.env.local`, no implicit environment overlays, and
  startup-bound values only. Local changes restart `dev`; production changes
  use an ordinary restart or rolling deployment.
- Added the agent-to-human `jadpo config set <field>` flow: the user enters a
  secret through a hidden terminal prompt rather than chat or a command-line
  argument. `jadpo config check` reports only safe local presence/validity.
- Kept `jadpo check` as the single authoritative full semantic check. `build`,
  `test`, `watch`, and `dev` reuse it; `dev`, deployment integration, and
  startup validate real values automatically, so no manual preflight ritual is
  required.
- Retained package-free Bun loading, explicit empty environment-file launches, secret flow,
  required/advisory readiness, outage recovery, and fail-closed traffic.
- Sequenced CONFIG-P0–P6 with adversarial evidence and stop conditions;
  CONFIG-P0 is unblocked.

### 2026-09-27 — CONFIG-001 compiler/runtime core implemented

- Added the structured declaration, semantic graph nodes, typed
  `config.<field>` access, checked defaults/bindings, secret-flow rejection,
  and value-free configuration audit.
- Added package-free generated Bun decoding and validation before listener
  startup, with automatic environment-file discovery disabled.
- Added hidden prompted local entry, value-safe local checking, atomic
  `.env.local` updates, declared-only process forwarding, and valid-change/
  last-known-good `dev` restart behavior.
- Added compiler, CLI, generated-runtime, LSP, editor, fixture, example, and
  secret-canary evidence. Real adapter sinks, dependency readiness/recovery,
  platform deployment hooks, and the golden integration remain pending their
  owning AUTH-001, SERVICE-001, TIME-001, or platform contracts.

### 2026-09-27 — AUTH-P0 decision freeze completed

- Pinned the approved authentication contract digest and propagated its
  user/service principal, exactly-one credential, revocation, fresh-authority,
  cache-independence, and dependency boundaries through the decision, proof,
  threat, golden-audit, and TypeScript-baseline surfaces.
- Expanded the golden acceptance contract with browser, opaque/JWT user API,
  service API-key/exchange, bounded ordinary request, fresh-authority,
  disabled-service, and public-liveness cases and linked them into the assurance
  evidence map.
- Migrated the golden configuration to the implemented structured syntax and
  recorded the fixture-first authentication source shape for AUTH-P1.
- Added explicit executable evidence that a non-JWT target contains no package
  manifest, lockfile, JOSE code, install instruction, or external dependency
  footprint.

### 2026-09-27 — AUTH-P1a application and principal model implemented

- Added top-level `application` and `principal` declarations with span-preserving
  AST nodes and deterministic semantic-manifest nodes.
- Made the application authentication default resolve exactly one declared
  principal and accept either immediate revocation or bounded revocation with a
  positive maximum delay.
- Made the principal a closed contract containing exactly one user and one
  service variant, with stable variant and field identities.
- Added authored diagnostics and three compile fixture pairs covering the valid
  graph plus duplicate applications/principals, missing or duplicate variants,
  and missing, forbidden, or zero revocation delays. The corpus now contains
  138 pairs.
- Kept credential strategies, reserved transport slots, claim mappings,
  authoritative resolution, and unauthenticated-principal reachability in the
  remaining AUTH-P1 work. Protected target generation remains fail-closed.

### 2026-09-27 — AUTH-P1b credential strategy topology implemented

- Added span-preserving named authentication strategies with one reserved
  cookie or authorization-header slot and one or more named validators.
- Added explicit `signed`, `opaque`, `api_key`, and `jwt` validation-mode
  metadata and closed user/service principal-variant links without exposing raw
  credentials or provider objects to authored code.
- Added stable strategy, credential-slot, and validation semantic nodes plus
  authored diagnostics for duplicate strategies, slots, and validators; empty
  validator sets; unknown modes or variants; and bearer credentials in path or
  query locations.
- Added positive and adversarial compiler fixtures, bringing the corpus to 141
  source/expectation pairs, while keeping protected target generation
  fail-closed until AUTH-P3 and concrete adapters are implemented.
- Left claim mappings, authoritative resolution declarations, reserved
  route-input enforcement, and unauthenticated-principal reachability in the
  remaining AUTH-P1 work.

### 2026-09-27 — AUTH-P1c mapping/resolution graph and initial AUTH-P2 checks implemented

- Added canonical claim mappings and explicit user/service authority-resolution
  declarations with source spans and stable semantic nodes.
- Added fail-closed validation for duplicate or invalid mapping targets,
  duplicate resolutions, malformed authority fields, and empty mappings.
- Added initial typed resolution checks for unique lookup authority, Boolean
  active-state predicates, nominal mapping compatibility, complete closed
  principal construction, and rejection of credential claims that attempt to
  populate authoritative identity fields.
- Added the 141st compiler fixture pair for adversarial resolution typing.
  Exact provider claim schemas, complete lifecycle/cardinality typing, reserved
  route-input enforcement, and unauthenticated-principal reachability remain.

### 2026-09-27 — TIME-001 and TEST-001 contract approved

- Approved and digest-pinned the complete clock, time-type, manipulation,
  formatting, persistence, and deterministic-integration-test contract in the
  [TIME-001/TEST-001 plan](time-testing-plan.md).
- Replaced ambiguous `DateTime` with UTC-millisecond `Instant`, restricted
  date-only authority to `CalendarDate`, defined resolved zone-aware `Time`,
  generated `Zone`/`Locale` enums, and froze the consistent `temporal` API.
- Assigned lifecycle timestamp ownership to generated application persistence,
  prohibited competing database defaults/triggers, and fixed database decoding
  directly into validated Jadpo values.
- Froze absolute and conversational friendly formatting, typed isolated
  fixtures, real-database evidence, clock/entropy/capability fakes, and distinct
  direct-call, route, job, generated, authored, and external evidence classes.
- Authorised TIME/TEST-P0 through P5 for fixture-first unattended
  implementation after the decision sprint; no compiler implementation was
  performed by this approval step.

### 2026-09-27 — language-wide naming and qualification contract approved

- Promoted callable spelling from a Temporal-specific choice into one
  digest-pinned language rule based on semantic ownership.
- Kept authored free callables unqualified in current/selective-import scope,
  entity operations entity- or receiver-qualified, and compiler standard
  libraries under mandatory lowercase namespaces such as `temporal.*` and
  `collection.*`.
- Froze casing, module names, imports, constructors, variants, capability/data
  access, parameter-order conventions, reserved-name handling, diagnostics,
  and the prohibition on alternate aliases or method/free-function duplicates.
- Queued NAME-P0–P2 to run with the first standard-library implementation; no
  compiler behavior was changed by the documentation freeze.

### 2026-09-27 — POLICY-001 contract approved

- Approved and digest-pinned the complete scoped-role, membership, entity and
  field policy, automatic query/mutation enforcement, validation, concealment,
  audit, and approval contract in the [POLICY-001 plan](policy-plan.md).
- Selected qualified resource-oriented roles such as `CompanyRole.owner`, one
  membership declaration or direct relationship binding as role authority, and
  one role-first entity matrix over compiler-derived
  `create`/`read`/`update`/`delete` effects.
- Removed routine action-level access declarations, manual `require policy`
  calls, repeated tenant predicates, dynamic field stripping, and lifecycle
  predicates from the policy model.
- Required closed input validation, supplied-field checks, write ownership,
  policy-scoped persistence, database result validation, authorised
  projections, and exact output validation to compose as separate fail-closed
  gates.
- Queued POLICY-P0–P6 for fixture-first unattended implementation after the
  decision sprint; no compiler behavior was changed by this approval step.

### 2026-09-27 — approved unattended compiler/runtime cores implemented

- Completed AUTH-P1 route/boundary reachability, exact principal and resolution
  typing, and the AUTH-P3 strategy-independent credential selector. Ten Bun
  cases cover zero/one/multiple credentials, kind confusion, malformed or
  widened values, authority substitution, adapter exceptions, and bounded or
  fresh resolution behavior.
- Completed NAME-P0–P2 enforcement for the current language surface, including
  casing, reserved standard ownership, canonical qualification, import/module
  conflicts, semantic ownership, and compiler-backed tooling behavior.
- Implemented the Temporal runtime and typed callable-fixture core: strict
  canonical values, closed zones/locales/policies, DST-aware resolution and
  bounds, calendar/elapsed arithmetic, human formatting, operation clocks,
  monotonic deadlines, generated timestamps and authority-change evidence
  from the same operation instant without database defaults, database decoding, isolated
  SQLite, fixed/advanceable clocks, typed configuration, and secret fixture
  containment.
- Implemented POLICY-P0–P4 and the executable P5 core: direct and membership
  roles, effect derivation, operation exceptions, field narrowing, automatic
  SQL scoping for reads/mutations/create, concealment, validation composition,
  route and non-entity invoke policy, user/service principals, and audit facts
  including derived restricted-field reads.
- Expanded the compile corpus to 179 pairs and passed the complete Rust
  workspace plus 70 local generated/runtime cases. Live PostgreSQL was not rerun
  because no `DATABASE_URL` was supplied; protected policy approval and
  service/job/golden exits remain behind their declared external or design
  gates.

### 2026-09-29 — Bounded Wasm runtime experiment scheduled

- Scheduled WASM-EXP1 before further concrete P11 adapter expansion, following
  the owner's request to evaluate Wasm as an alternative executable target.
- Bounded the work to one generated slice, a Bun comparison, local execution,
  and Cloudflare host evidence; broader hosting and distributed infrastructure
  remain outside the experiment.
- Required a frozen comparison protocol, explicit host capability limits, and
  a recorded target recommendation without presuming migration or speed gains.

### 2026-09-29 — First-party authentication and protected routes checkpoint

- Implemented configured signed and opaque user credentials over cookie/bearer
  transports, generated user/session authority checks, stable operation-time
  expiry, bounded refresh, revocation, overlapping key rotation, and
  origin/session-bound CSRF checks (including compiler-detected GET writes).
- Protected routes authenticate before input decoding, then pass the exact
  principal into existing invoke and persistence policy. Unsupported JWT,
  service validators, richer principal mappings and incomplete configuration
  still fail target generation.
- Added [the executable example](../examples/first-party-authentication/README.md),
  typed secret configuration sinks, authentication audit/OpenAPI security
  output, and 20 local runtime acceptance cases. A new negative compiler
  fixture brings the corpus to 180. The Rust workspace and local SQLite/HTTP
  evidence pass; live PostgreSQL was not run because no database URL was supplied.
- Kept trusted host credential issuance separate from authored code and HTTP
  routes. Human login, account provisioning, service/JWT support, full AUTH-P8,
  extended fuzzing, deployment readiness and independent security review remain
  outside this checkpoint. AUTH-P4/P7 are not claimed complete in their entirety.
- Followed owner direction to complete this path before beginning Wasm probes.

### 2026-09-29 — Browser/API authentication completion

- Closed the scoped first-party milestone with 24 passing runtime cases in each
  SQLite/PostgreSQL mode, covering all four credential/transport combinations.
  Startup-failure subprocess cases use SQLite in both runs.
- Fixed concurrent initialization ordering and session-schema startup failure
  handling; added separate-process persistence/revocation and identity-reuse
  regression evidence.
- Fixed the legacy public-auth diagnostic and verified both human-owned repairs.
- Retained [raw verification results](../tests/assurance/auth-first-party-2026-09-29/README.md).
  Service/JWT, broader principal mappings, full AUTH-P7/P8 and external review
  are not included in this completion claim.
- The next phase is comprehensive validation; Wasm remains queued behind it.

### 2026-09-29 — Unified validation foundation

- Added [one verification command](../tests/validation/README.md) with a checked
  example/suite inventory, per-run logs, tool versions, explicit quick-profile
  skips and a separate failing full-golden gate. Prepared GitHub Actions to run
  the same command; no remote workflow or protected CI requirement is established.
- The full local run passes 198 Rust tests, 180 CLI fixture pairs, four editor
  tests, all ten supported example builds, authored tests, 92 SQLite/local
  runtime cases, 22 PostgreSQL persistence cases and 24 PostgreSQL-mode auth cases.
- The first unsandboxed run exposed a previously skipped dev rollback test whose
  fake Bun launcher inspected the wrong argument. It now requires its socket and
  Python prerequisites explicitly, validates the launch flags, bounds event waits,
  and shuts down its child runtime on failed assertions.
- The golden app still emits 60 diagnostics. All 44 behavioural obligations are
  explicitly unexecuted, including four auth-contract conflicts and one query
  budget clarification. No candidate expectation or policy was weakened.
- Prepared coherent independent test-authoring work packages by semantic area.
  Test-authoring agents and the separate fresh-agent usability trial have not run.

## 12. Immediate next action

Three bounded passes of the owner's [Wasm large-row and backend coverage
follow-up](wasm-large-row-plan.md) are complete; see [read-path results](wasm-read-path-results.md).
Compact checked queries and validated row receipts improve concurrent large-read
throughput about 11% over previous WASM, but Bun parity still fails. Direct typed
ingress is implemented and retained as a measured alternative; it did not win
candidate selection. A separate [local workerd comparison](wasm-workerd-read-results.md)
runs without Bun and passes 585,339 measured requests. Its concurrent large-read
paired medians still trail generated JS by about 10% throughput and 7% p95; the
host change does not remove the gap. The subsequent [framework-informed boundary
pass](wasm-workerd-boundary-results.md) profiles 24 workerd cases and tests direct
JSON writes/borrowed result views without rebuilding the guest. It passes 48 tests
and 608,612 HTTP requests. Concurrent large-ASCII paired medians improve about 3%
throughput/1% p95 over the previous driver, with only three of five winning pairs;
escaped text still trails generated JS by about 23% throughput/32% p95. Binary and
adaptive alternatives retain ASCII regressions. Local generator/RPC headroom
prevents a capacity conclusion. The [native-runtime-informed follow-up](wasm-native-values-results.md)
now checks Bun, Node and CPython's native value integrations and tests conservative
size proofs, direct typed writes and bounded content selection. It passes 53 tests,
1,060,000 isolated measured calls and 924,283 HTTP requests. The sampled candidate
reduces escaped-text isolated time about 45%; concurrent escaped HTTP improves
16.6% throughput/14.8% p95 over previous WASM, winning all five pairs. That fixture
roughly matches generated JS in local concurrent HTTP, but isolated execution and
other workload families still trail JS. Small/Unicode HTTP regressions and all
losing alternatives remain recorded. No default promotion follows. Next work should
attribute the remaining host/guest string costs, broaden text/transform workloads,
and qualify a longer controlled-host comparison with independent generator headroom.
The next [guest string/bulk-memory pass](wasm-guest-scans-results.md) now completes
949,050 HTTP requests after 52 host tests, nine Rust tests and clean source-mutation
rebuilds. A negotiated guest-only exact logical size check, SIMD validation and
bulk-memory operations improve isolated large-text times by 14–52% over the sampled
candidate. Eligible large rows no longer need a content heuristic. Single-request
large HTTP throughput is near JS, but concurrent large throughput still trails
by 5–14%, and Unicode/p99 regressions remain. Guest retention falls from seven to
six pages. A further [shared-input-frame experiment](../experiments/wasm-exp1/workerd-shared-rows/README.md)
passes 52 host/10 Rust tests and 240,000 isolated calls, but offers only 1–2% gains
with a slight ASCII loss and is unselected. Keep the simpler SIMD/bulk-memory
candidate. Next work should profile its remaining host/control-message costs and
investigate Unicode tails with a longer, independently driven controlled-host run;
current no-work headroom still prevents a capacity or general-parity conclusion.
No default target change is approved by these measurements.
Native SQLite statement/commit pauses dominate the earlier write stalls. WAL/FULL
substantially improves both targets and puts the WASM adapter ahead on tested
writes, with different SQL/host plans. Controlled-host WAL/FULL qualification with
sustained checkpoints/recovery and backend inventory COV-1 remain open. Actual
Cloudflare deployment/cold starts remain a separate evidence track.

The remaining product work is P11/golden on Bun. Explicit route success modes
now remove three unsupported-syntax diagnostics from the golden candidate, but
the source still fails checking with 57 diagnostics and all 44 integrated cases
remain unexecuted. Migrate the accepted entity/query/policy source forms without
changing human-owned permissions, resolve or explicitly park remaining contract
questions, then execute the complete integrated behavioural gate. Retain
VAL-001 cross-feature tests, independent review, and mutation checks as the
foundation.

### 2026-10-01 — Golden todo entity dossier migration started

Created the separate compiler-checked successor package at
`examples/golden-todo-migration` while retaining the original frozen candidate
source and human-owned policy. Four one-entity dossiers now express User,
Service, ServiceCredential, and Todo with explicit identity, primary authority
persistence, unique fields, ownership references, inverse collections, and
delete actions. Shared enums and the TodoTitle/CredentialVerifier refinements
are in the values role. `jadpo check examples/golden-todo-migration` passes for
all five source files with zero diagnostics.

The entity-field grammar does not accept inline server defaults. This slice
does not drop the candidate defaults: the migrated create actions must assign
active/open status explicitly. The package has no migrated actions, policy,
routes, auth, configuration, jobs, or acceptance runner yet, so it supplies no
executed golden behavior evidence. The original candidate still has 57
diagnostics and all 44 integration cases remain unexecuted.

### 2026-10-01 — Golden todo value contracts migrated

Extended the successor package's `values/` role with the supported patch and
disable inputs, `TodoCursor`, `TodoView`, `TodoPage`, `UserWithTodos`, public
health output, and reminder message/receipt values. The shared values file
contains the bounded `PageSize`, authentication/health enums, and the existing
TodoTitle/CredentialVerifier refinements. The full successor source now checks
as six files, 22 declarations, zero diagnostics.

`CreateTodo`'s omission default and the complete `ListTodos` URL/query/cursor
contract remain in the preserved source until their missing language support
is available; they have not been weakened into required inputs or simpler
reads. No named query, policy, operation, route, or acceptance behavior has
moved yet.

### 2026-10-01 — Golden todo configuration, authentication, and liveness

Migrated the typed configuration bindings, secret markers, and 30-day retention
default; the bounded five-minute application revocation declaration and closed
user/service principal; signed browser and API user credentials, configured JWT
user validation, and active-user authority resolution; plus the static public
`GET /health/live` route. The route returns a constant typed health value and
reads no config or persistence. The complete migration package now checks ten
source files and 32 declarations with zero diagnostics.

This is source-check evidence only. Service API-key authority mapping and
exchange, user/service integrated runtime paths, protected routes and operations,
deployment-plane readiness, and every golden acceptance case remain unexecuted.
Browser cookie runtime also requires a configured origin setting that the frozen
candidate does not yet declare; select that under the accepted auth contract
before claiming generated browser authentication.
The five-minute bound comes from the application revocation policy; validator
per-token expiry and JWT algorithms are compiler-managed rather than additional
source settings. No per-validator TTL setting is accepted by the current
grammar, so the original `expires: 5m` spelling is not copied.

The scoped browser/API and service/JWT runtime checkpoints are verified; broader
principal mappings, production qualification and full AUTH-P7/P8 remain separate
exits. WASM-EXP1 and the bounded optimisation passes are complete; the wider
large-row/backend-coverage plan remains partly executed.
Instance reuse and host caching substantially reduced overhead; Bun remains the
working target and route selection remains inconclusive. Further investigation
now has the separately scoped large-row/backend plan above; no target migration is scheduled.

Protected approval requires external CI/review attestation; broader PostgreSQL
behavior and deployment hooks still need their own evidence. P10R still needs
outside review and five first-user sessions, and DX2 needs fresh-agent/user
repair-cycle evidence. These gates remain open; passing local authentication
and compiler tests does not create a release-equivalent assurance claim.

### 2026-09-30 — Service/JWT runtime and validation checkpoint

The unattended through-Wasm continuation added service credential ownership,
verifier-only storage, rotation/revocation and bounded exchange, plus opt-in
pinned JWT verification with one authoritative principal lookup per request.
Review fixed explicit service identity mapping, originating service strength,
cookie-JWT rejection, and the existing golden `NotPermitted` inactive category.
The dependency installer verifies the package's complete bytes and inventory.

The full local gate passed 45 steps: 383 Rust tests, 180 compile fixture pairs,
four editor tests, 21 verifier/dependency tests, 205 local runtime cases and 91
PostgreSQL cases. One SQLite-only query-count case is explicitly skipped in
PostgreSQL. Golden remains 60 diagnostics and 44 unexecuted integrated cases;
P10R/P12 and production qualification remain external/separate exits.

This follows the scoped browser/API milestone and comprehensive validation;
the added service/JWT runtime evidence strengthens the Wasm start checkpoint.
It does not claim all AUTH-P7/P8 or P11 exits. The experiment remains bounded,
uses fixture principals and does not select a production backend.

### 2026-09-30 — WASM-EXP1 completed; Bun retained

Both checked-source compilation routes passed fifteen equivalent probes locally
and on Cloudflare. A provisional Rust full slice preserved the frozen policy,
validation, failure recovery, rollback and isolation cases using identical core
bytes with Bun SQLite and a Cloudflare SQLite Durable Object. Unsupported
capabilities were rejected; mutation and clean reconstruction checks passed.

Forty timed runs completed with 79,855,562 requests and zero measured errors.
Local I/O p95 was 33.34×/36.14× Bun at concurrency one/eight, failing the adoption
threshold. Independent review fixed nullable parameter lowering and a weak
closed-output assertion; oversized host-result fault mapping remains a named
limitation. The recommendation is defer further backend adoption, with route
comparison inconclusive. All experiment cloud resources were cleaned up.

The [results](wasm-experiment-results.md) retain the frozen protocol, raw
acceptance/timing evidence, reproducible scripts, source hashes and effort.
This closes the requested experiment, not the remaining golden, AUTH-P7/P8,
P10R/P12 or external validation exits.

### 2026-09-30 — Wasm optimisation follow-up completed

Owner-requested attribution found instance creation, not SQLite, was the major
initial bottleneck. Explicit reset/exclusive reuse, prepared statements and
checked-plan caching passed retained correctness checks locally and on Cloudflare.
Eighty paired short runs produced5,496,882 correct operations: optimised Wasm
30–31k/s versus Bun38–39k/s, with0.041–0.043ms p95. The concurrency 1 original
20% latency gate still fails, but the earlier traffic concern is substantially
reduced. [Results](wasm-optimization-results.md) recommend continued bounded
profiling and realistic workload measurement; no production target change or
automatic new experiment follows. Cloud resources were cleaned up.

### 2026-09-30 — Wasm JSON boundary and HTTP follow-up completed

Profiled the remaining boundary costs, cached compiler-owned constant descriptions,
compiled exact host policy-description checks and removed unused text-length work.
The unchanged JSON ABI and validation/policy guarantees remain. Local correctness,
source mutation/rebuild and rejection gates passed. The repeated HTTP comparison
completed 140 measurements and 3,457,655 requests without errors. Small reads are
close to Bun; larger returned rows remain substantially slower. Durable-write
results are mixed and the load-generator ceiling limits capacity conclusions.
Two aborted Bun-client runs are retained separately; the final Node-client run
completed without retries. No new Cloudflare qualification or backend migration
is claimed. See [report](wasm-http-experiment-results.md). Future work should target
larger-value transport and controlled load evidence, with separate scope.

### 2026-09-30 — Wasm ownership/speed-build follow-up completed

Transferred completed owned values instead of redundant copies and compared
size-oriented and speed-oriented release builds separately. The selected combined
candidate won concurrent small-read HTTP throughput (~9%), p95 (~11%) and server
CPU (~8%) against generated Bun in all five local pairs. The full comparison ran
140 measurements / 3,693,399 requests without errors. Local correctness, rejection,
mutation/rebuild and runtime gates passed. Larger-row reads remain slower; concurrent
write throughput/tails regressed and a separate diagnostic found long server-side
application/storage pauses on both targets. Exact causes remain unresolved. This
is a specific small-read win, not a backend adoption decision or an EC2 result.
See [report](wasm-value-path-results.md). Next questions are larger-value transport
and a controlled host/storage comparison; the default target remains Bun.

### 2026-10-01 — Explicit route success outcomes implemented

P10.6 now accepts `success: created` and `success: no_content`. Created routes
require a typed response that matches the action result and return HTTP 201;
no-content routes require `Unit`, reject a declared output body, and return an
empty HTTP 204. OpenAPI carries the corresponding success response, and route
inventory schema version 2 records the mode and status. Three compile fixture
pairs cover the accepted modes and invalid body combinations; the artifact
runtime suite checks both HTTP responses and generated metadata.

The full 45-step supported validation run passed: 392 Rust tests, 183 compile
pairs, four editor tests, 207 local runtime cases, and 91 PostgreSQL cases, with
one SQLite-only JWT query-counter assertion explicitly skipped in PostgreSQL.
The golden todo now reports 57 diagnostics rather than 60, but its full source
still does not compile and all 44 integrated behavioral cases remain unexecuted.
No candidate expectation or human-owned policy was changed. The latest
[validation ledger](../tests/validation/unattended-progress.md#2026-10-01--explicit-route-success-outcomes)
records the run and remaining evidence limits.

### 2026-10-01 — Direct Todo owner policy migrated

The golden migration now binds `Todo.owner_id` to the accepted
`TodoRole.owner` role and grants that role the entity's create/read/update/delete
effects. `jadpo check examples/golden-todo-migration --diagnostic-format=json`
passes with zero diagnostics across ten files and 33 declarations. The frozen
human-owned policy file is unchanged. This source does not yet expose protected
operations, and User self-access plus soft-delete visibility remain without
accepted representations, so it is not runtime, approval, or golden-case
evidence.

### 2026-10-01 — Identity-bound User self-read migrated

The frozen `policy.jadpo` defines User self-access with the exact predicate
`principal.user.user_id == User.id`, so the migration uses a direct
`UserRole.self` binding on the User dossier's identity field. The compiler now
recognises an immutable dossier identity as a direct same-entity principal
reference while rejecting role bindings on unrelated scalar fields. The role
grants only `read`; empty field policies keep the authentication subject, email,
status, and lifecycle timestamps outside self-readable projections. The
migration checks ten files and 34 declarations with zero diagnostics, and
focused tests cover role scope, non-identity rejection, and projection
narrowing. User disablement and soft-delete lifecycle remain open; no protected
operation or acceptance case has executed.

### 2026-10-01 — Formatter conformance follow-up scheduled

Added the open DX1 work item `TOOL-005` for a canonical formatting-rule matrix
and table-driven unit coverage mapped to every production and syntactic
alternative in the accepted grammar. The cases will perturb legal whitespace
(including spaces, tabs, indentation, blank lines, and line breaks), assert
exact canonical output and idempotence, and preserve parsed structure, comments,
and string literals. This fills a formatter-unit-coverage gap beyond the
existing LSP protocol tests; exhaustive conformance remains open.

### 2026-10-01 — Formatter conformance suite started

Added token-aware horizontal spacing rules that preserve string-literal bytes
and line-comment content, plus exact-output unit cases for the current surface
families. A grammar-corpus integration check covers all 56 current compile-pass
files and verifies token preservation, parseability, idempotence, and output
stability under mixed tabs/spaces, indentation changes, and LF/CRLF changes.
The focused Rust tests pass. The formatter matrix records remaining work for
canonical route-item ordering and explicit coverage links for every grammar
production and alternative; TOOL-005 remains open.

### 2026-10-01 — Formatter whitespace coverage expanded

Expanded the compile-pass corpus checks to insert excess blank lines and remove
non-comment line breaks while preserving tokens, parseability, and formatter
idempotence. A split route header exposed that formatting a path token on its
own line could insert spaces inside it; the formatter now preserves compact
context-sensitive path text and has a regression case. The formatter now emits
route items in their accepted canonical order and retains leading comments with
their item. The full `jadpo-core` suite passes. The formatter matrix maps all
121 grammar productions. Exact canonical output after removing nonblank line
breaks remains an implementation gap: TOOL-005 preserves those source breaks
today, while the roadmap already requires byte-identical canonical output.

### 2026-10-01 — Full supported-check verification completed

After classifying the exploratory `examples/golden-todo-migration` package,
`python3 tools/verify.py` completed all 45 recorded checks, including the Rust
workspace, compiler/editor checks, authored and runtime suites, and all six
PostgreSQL modes. The verifier report is
[`20261001T100354-43738/report.json`](../build/validation/20261001T100354-43738/report.json)
and the [validation ledger](../tests/validation/unattended-progress.md#2026-10-01--full-supported-check-verification)
records the initial inventory correction and execution limits. The result is
`supported_checks_passed_with_open_gates`, not release-equivalent evidence: the
frozen golden candidate remains at 57 diagnostics with integrated behavioural
cases unexecuted. At this checkpoint User self-policy migration had not been
made, and formatter output for removed nonblank line breaks remained open.
Later entries record the exact self-scope migration and the formatter's
remaining implementation gap under the already-defined byte-identical output
contract.

### 2026-10-01 — Nullable input omission default migrated

Added `default none` for nullable `input` fields. The parser permits it only on
input records; type checking rejects a non-nullable field; the generated input
validator fills an omitted field with `null` before the action receives the
normalized record. OpenAPI leaves a defaulted field out of the request
`required` list. `examples/golden-todo-migration/values/contracts.jadpo` now
contains the frozen `CreateTodo.due_at` shape, and the source package checks ten
files and 35 declarations without diagnostics. Parser, compiler-target and
OpenAPI tests cover acceptance, non-null rejection, generated normalization,
and request optionality. The targeted Rust suites pass. This does not run the
golden CREATE-004 behavior; integrated acceptance remains open.

### 2026-10-01 — Todo by-id action and authenticated route source migrated

Added `Todo.by_id` as an authoritative required identity lookup, mapping
soft-deleted rows to the same concealed `TodoNotFound` failure before building
the established `TodoView`. Entity action `Todo.get` propagates the query
result, and authenticated `GET /todos/{todo_id}` exposes the derived 404
contract. The compiler check passes ten source files and 39 declarations; the
semantic call graph records the route-to-action-to-query path. The earlier
query-only build audit proves an automatic `TodoRole.owner` read obligation.

Build generation refuses the protected route with
`toolchain.target_auth_not_implemented` because the selected authentication
adapters/configuration/principal mapping are not supported together by the
current target generator. This is source evidence only; no route runtime or
golden behavior is claimed. The refreshed [supported-check report](../build/validation/20261001T132458-81588/report.json)
passes all 46 local verification steps and seven PostgreSQL modes but remains
`supported_checks_passed_with_open_gates`; the frozen candidate still has 57
diagnostics and all 44 integrated behavior cases remain unexecuted.

### 2026-10-01 — Formatter grammar alternatives covered

Linked the grammar coverage matrix to exact formatter snapshots and added a
focused source/output pair for alternatives absent from the compile-pass
corpus: `restrict` references, both `many` inverse spellings, `projection`
without a strategy, and `read_your_writes`, `bounded_staleness`, and `eventual`
freshness. A table-driven unit test checks parsing, exact output, idempotence,
extra spaces/tabs, incorrect indentation, CRLF, and excess blank lines for
that case. The remaining TOOL-005 gate is the owner decision on canonical
output after removing nonblank source line breaks (FMT-005).

### 2026-10-01 — Identifier and reserved-word conformance queued

Added NAME-002 after auditing the existing rules. The grammar already limits
identifiers to `[A-Za-z_][A-Za-z0-9_]*`, and NAME-001 already establishes
type-like versus runtime naming styles. Existing parser and compile tests cover
some complete-keyword, contextual-name, and casing cases, but do not provide a
complete hard-reserved/contextual inventory or identifier-boundary matrix for
all name positions. The new P4 follow-up requires an explicit case-sensitivity
rule, ASCII/Unicode and start/continuation boundaries, and table-driven
positive/negative cases across declarations and references. No accepted
language behavior or digest-pinned contract was changed.

### 2026-10-01 — RM-501 cross-area validation gap inventory

**Completed task:** RM-501

Completed RM-501's review of accepted and provisional contracts against the
checked-in validation evidence. The [roadmap gap inventory](../tests/validation/roadmap-gap-inventory.md)
selects remaining obligations across syntax/modules, types/values,
failures/callables, persistence/transactions, auth/policy, config/time,
tooling/artifacts, cross-feature integration, and sustained fuzz/mutation. It
links each gap to an existing decision, external prerequisite, or remaining
roadmap task so test campaigns can proceed only against specified behaviour.
This is a static evidence review, not a fresh verifier run or a claim that the
listed behaviours have been implemented. RM-502 through RM-505 remain active
work or dependency-gated.

### 2026-10-01 — RM-502 seeded semantic fuzz campaign

**Completed task:** RM-502

Added a reproducible 2,048-case mutational campaign over the sorted compile
pass/fail fixture corpus. Each mutated source runs through parse, semantic
graph, type, and failure analysis twice; the test checks deterministic results
and UTF-8-aligned token/diagnostic spans. A child-process deadline bounds
runaway parser/checker behaviour. Failures report the initial seed, case and
corpus index, then run a bounded delta reducer; its shrink property has a
focused test. The [campaign guide](../tests/validation/fuzz-campaign.md)
requires each fixed reproducer to enter the compile fixture corpus so the
normal fixture gate and future fuzz runs retain it. The semantic package passes
36 unit tests and both fuzz integration tests. No failing generated case was
found, so no new reproducer was added. Coverage-guided/deep-resource fuzzing
and compiler implementation mutation evidence remain open under RM-503 and
the validation gates.

### 2026-10-01 — Roadmap estimates recalibrated from recorded sessions

Withdrew the unsupported engineer-day estimates. The [timing evidence](task-timing/README.md)
records 12 completed project-chat turns with actual UTC start/end metadata,
including 7–10 minute focused fixes, 37–69 minute implementation slices and
roughly two-hour mixed batches. These are session observations, not complete
epic durations. The replacement roadmap uses explicit hour bands and marks
new-subsystem extrapolations and unmeasured external work separately; historical
forecasts are retained in [withdrawn estimates](task-timing/withdrawn-estimates.json).

Added `tools/task-time.py` and repository workflow instructions to record
future estimates, phase transitions, pauses, blockers, completion evidence and
partial/rework sessions. Six synthetic checks verify timing arithmetic, unknown
interrupted intervals, open sessions, actor switching, immutable completions,
input validation and reversed-clock rejection. No historical active/blocked
times were invented, and no complete application timing is claimed.

### 2026-10-03 — RM-206 lifecycle lowering

**Completed task:** RM-206

Implemented compiler/runtime/audit conformance for the frozen DATA-007 entity
lifecycle contract. Generated persistence enforces lifecycle-owned initial
values, guarded transitions and visibility; its maintenance worker runs a
transactional, ordered purge capped at 500 rows with a final eligibility
predicate and redacted audit outcomes. Retention binds from validated
application initialization and direct purge fails closed before binding.

Evidence: all 212 compile-fixture pairs pass; `cargo test -p jadpo-core` passes
85 unit tests plus integration tests; the lifecycle runtime suite passes 13/13
on both SQLite and PostgreSQL. An independent correction review resolved the
host create, cutoff and configuration-binding findings, and a second read-only
spot check found no residual issue within the frozen trusted-host boundary.
See the [lifecycle plan](lifecycle-plan.md#rm-206-completion-2026-10-03),
[correction review](../tests/validation/rm206-independent-correction-review.json),
[runtime tests](../tests/runtime/entity-lifecycle.test.ts) and [compile corpus
run](task-timing/runs/RM-206-d840e261433b.jsonl). The golden HTTP application and
release gates remain open; RM-106 owns the dependent golden route integration,
and RM-108 owns reminder delivery.

Eleven reliably measured RM-206 runs total 367.15 active minutes, including review
and rework; the original 1–4h estimate remains historical and is not a forecast
of remaining work. The separate final correction-review run is retained at
[`RM-206-f76cdc3c1c2e`](task-timing/runs/RM-206-f76cdc3c1c2e.jsonl), but its
recorded 460.62 active minutes include an unobserved interval and are explicitly
unreliable, so they are excluded from that sum.
