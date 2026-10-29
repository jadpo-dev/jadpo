# Golden todo design contract

**Active executable contract:** `todo-v0.2`

**Status:** implemented and locally validated; independent freeze outstanding

The golden todo is an ordinary Jadpo application whose 44-case contract runs
on native and compiled workerd. [`acceptance-v0.2.json`](acceptance-v0.2.json)
is the active executable specification. The language area pages govern its
meaning; the corrections from the historical candidate are explained below.

The original [`acceptance.json`](acceptance.json), `todo-v0.1`, remains
byte-for-byte unchanged. It records the earlier candidate contract. Its
source, written for the archived compiler, is at `archive/compiler-v0`.
The shared harness checks raw HTTP, credentials, persisted state, jobs,
preflight and outages. An unavailable obligation fails visibly. The
supporting documents describe the required review and comparison surface:

1. [`expected-audit.md`](expected-audit.md) defines the review surface the
   compiler must derive.
2. [`adversarial-changes.md`](adversarial-changes.md) defines change-pressure
   cases and the compiler's required response to each.
3. [`typescript-baseline.md`](typescript-baseline.md) fixes the equivalent
   TypeScript stack that Phase 9's benchmarks and chunk 10.2's agent
   comparison measure against.
4. [`REVIEW.md`](REVIEW.md) records the contract's review history and the
   pending independent-review checklist.

If the source and the acceptance behaviour disagree, the contradiction blocks
the freeze until it is resolved. If acceptance behaviour exceeds the policy a
person approved, policy wins and the case is invalid until a human-reviewed
contract revision reconciles them. An agent may not edit policy to make an
implementation compile.

## Product behaviour

The application supports:

- browser sessions, user API bearers, service API keys/exchange, and opt-in JWT
  bearer validation normalised to one closed user/service principal;
- authenticated-by-default routes and one explicit public health route;
- fail-closed handling of invalid, conflicting, or ambiguous credentials;
- bounded revocation for ordinary traffic with fresh-authority checks on
  sensitive operations, without cache-dependent correctness;
- owner-scoped todo CRUD with omission-aware patches;
- stable cursor pagination and stable child ordering without N+1 reads;
- explicit owner relationship integrity and deletion lifecycle;
- optional due dates, overdue reminder scheduling, and a declared mail service;
- a soft-delete retention window followed by an idempotent purge job;
- typed configuration, secret handling, readiness, and startup preflight;
- generated structural tests plus authored business tests; and
- derived route, data, effect, failure, configuration, and policy evidence.

Sharing and administrator access are not present in the initial contract. They
enter through the frozen change sequence so the experiment can measure policy
evolution rather than beginning at the final answer.

## Freeze rule

This is a **candidate** freeze. Independent review may correct ambiguity once.
After it is marked frozen, changes need a new contract version and a
rationale, and the implementation must not weaken or rewrite the cases to fit
compiler limitations. A case may be classified `unsupported`, `cannot prove`,
or `requires human decision`, but it may not silently disappear.


## Current-language successor: `todo-v0.2`

[`acceptance-v0.2.json`](acceptance-v0.2.json) preserves the 34 HTTP, four
job, two persistence, two preflight, one policy and one runtime case IDs.
Neither version is an independently reviewed freeze. The original
[`acceptance.json`](acceptance.json) remains byte-for-byte unchanged; its
SHA-256 is recorded in the successor.

These corrections follow the area pages, retaining each original safety
obligation:

| Cases | Decided clause and executable reading |
|---|---|
| Application HTTP failures and startup refusal | Area 7, “How failures reach the client”: outer `error`, required `code`, `message`, `request_id`, optional `details`; `x-request-id` repeats that same identifier. No raw SQL, provider body, stack or input values are public. |
| AUTH-012, PUBLIC-001, CONFIG-003 | Area 7, “Health”, and area 14, “A native deployable” / “A Workers deployable”: probes belong to the runtime, outside application APIs. `/health/live` returns `status: live`; readiness is a deployment probe. Seven authored application routes remain protected. Readiness has its own `ready` / `not_ready` deployment-probe schema, including store status and declared provider connections reported as `unchecked` without network probes. CONFIG-003 asserts `reminder_mail` explicitly. |
| READ-003, LIST-004 | Area 7, “How failures reach the client”: malformed placeholders and out-of-range query fields are `422 invalid_input`, before business reads. A valid bounded credential keeps authentication queries at zero too. |
| PATCH-003, PATCH-005 | Area 6, “The operations every entity gets”: an empty patch succeeds without changing values or generated timestamps. Only `patch` observes presence; `when_supplied` clears receipt/queue state and increments the private revision whenever `due_at` is supplied, including explicit `none` and the same date again. |
| USER-001 | Area 8, “The application and revocation”: expiry of a signed part renews against a still-live durable session and current authority. A disabled user receives `403 user_disabled`, and no renewed credential. Actual durable-session expiry receives `401 invalid_credentials`. Both boundaries and all earlier fresh/refresh follow-ups remain asserted. |
| JOB-001–003 | Areas 11, “Calling”, and 12, “Emitting an event” / “Delivery: at least once, safe to repeat”: runtime-derived keys belong to durable deliveries and steps. A private increasing revision distinguishes A→B→A due-date changes. Queue marking and the first event commit together. A preparation subscriber snapshots immutable mail data in a second event; the send subscriber checks current revision, eligibility and owner status, uses that immutable payload, then records the provider’s acknowledged `accepted_at`. Queuing is never counted as sending. This is a provider acceptance receipt, not proof of inbox placement. Lost acknowledgements, in-flight edits, stale work, concurrent runners, safe retries and unknown outcomes remain obligations. |
| JOB-004 | Areas 5, “Lifecycle”, and 12, “The purge schedule”: compiler-generated `Todo.purge`, 30-day configured retention, at most 500 records per batch and 20 batches per run. Repeats and boundaries are asserted; ordinary code cannot hard-delete lifecycle rows. |
| CONFIG-001–002 | Area 10, “Checking values before the app starts”, and area 14, “A Workers deployable”: native refuses configuration before listening and exits nonzero; a Worker has no process exit and refuses application execution with safe 503 while reporting declaration names privately. Configuration is the unnamed `config` singleton. |

The source has three stored entities: User, Todo, and the Service principal
required by AUTH-009/011. It has no sharing, administrator bypass, ownership
transfer action, public authentication fixture route, raw SQL or secret reads.
Fixture SQL and test credentials belong only to the black-box controller.
`User.email` is provisioning-only and readable by system delivery work; the
self-service projection exposes only `id` and bounded ordered todos.

The app’s own `ReminderMail.v1` provider package defines a fixed hosted
reminder-template contract at the non-routable fixture hostname
`mail.example.invalid`. Local tests supply its transport. There is no real
mail account, deployment or SMTP activity. The provider keeps an idempotency
key for 24 hours; unfinished work past that safe window must be held for an
operator, never resent blindly. Built-in EmailMessage is not used because
its queue emission supplies no application delivery receipt.

## Verification

`jadpo check` and `jadpo test` run the source and authored business tests.
The shared contract runner under `tools/golden-todo/` targets generated native
code and compiled Wasm under workerd, with controlled test clocks, actual
credential verification, raw HTTP inputs and SQL-level query observation.
All 44 cases pass on each target using the integrated checkout. Partial
and prerequisite-preview runs do not establish complete conformance.
The fixture intercepts only its declared public HTTPS identity/key/mail hosts
and supplies local responses; it does not prove production DNS or TLS filtering.
SQL fixture writes and signing helpers are private test controls, and HTTP
responses always execute the selected target.
Passing authored typed tests alone does not establish the complete contract.
The runner must report every ID on each target and fail if a target or
obligation is unavailable. TypeScript and comparative timing remain 9.1d3/4.

During development, the compiler and runtime source may be supplied explicitly
from a verified prerequisite checkout. Such a preview is identified by path
and revision and does not establish this branch’s full gate. The final gate
runs against this worktree’s compiler/runtime and exact incorporated prerequisite
histories. Richard authorized stacked builds before prerequisite PR review
and merge on 2026-10-23; that authorization does not establish independent review.
