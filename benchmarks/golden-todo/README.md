# Golden todo

An ordinary Jadpo application, written in the decided language with no
compiler special case (the gate's name ban covers its names), and the
contract it is held to: [`acceptance-v0.2.json`](acceptance-v0.2.json),
`todo-v0.2`, 44 cases (34 HTTP, four job, two persistence, two preflight,
one policy and one runtime). The language area pages govern what each case
means; where a case and a page disagree, the page wins and the case is
corrected in a new contract version, never the source weakened to fit.
[`acceptance.json`](acceptance.json) is the earlier `todo-v0.1`, kept
unchanged because `todo-v0.2` records its SHA-256.

The other documents here:

1. [`expected-audit.md`](expected-audit.md): the review surface the
   compiler derives.
2. [`adversarial-changes.md`](adversarial-changes.md): change-pressure
   cases and the compiler's required response to each.
3. [`typescript-baseline.md`](typescript-baseline.md): the equivalent
   TypeScript stack, in [`typescript/`](typescript/README.md), that the
   benchmarks and chunk 10.2's agent comparison measure against.
4. [`REVIEW.md`](REVIEW.md): the contract's review log. The contract has
   not had an independent freeze; until it has, review may correct an
   ambiguity once, and afterwards a change needs a new contract version
   and its reason. A case may be classified `unsupported`, `cannot prove`
   or `requires human decision`, but never silently dropped.

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

## How the cases read the area pages

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

- `jadpo check`, `jadpo fmt --check` and `jadpo test` run the source and
  its authored business tests, as for every teaching example
  (`tools/docs/examples.py`).
- `benchmarks/contract/run.py` runs all 44 cases on three targets, built
  by this checkout's own compiler and runtime: **interpreted**, the
  interpreter running the project's plan; **native**, generated Rust; and
  **workerd**, generated Wasm under the pinned workerd. Interpreted and
  native share one controller binary, the interpreter chosen by
  `GOLDEN_EXECUTOR=interpreted`. Every case crosses raw HTTP through the
  production request decoder, with controlled clocks, real credential
  verification and SQL-level query observation against the gate's
  PostgreSQL. A target, control or case that is missing or fails, fails
  visibly; a complete contract needs all 44 on all three targets, unfiltered,
  from fresh builds.
- The gate runs interpreted and native in one step of its `compiled`
  group and workerd in `wasm32`; the verifier's negative controls run in
  `lint`.
- Startup refusal (CONFIG-001/002): the native deployable built by
  `jadpo build` exits nonzero before listening; a Worker answers a safe
  503; the interpreted program reads its settings as a deployable does
  and exits nonzero. `jadpo dev`'s stand-ins for unset provider settings
  (area 11, "Provider calls in development") are a development
  convenience, not this case.
- Both deployables build with `jadpo build` from a copy of `jadpo` outside
  the checkout, with no Rust installed
  (`crates/jadpo-cli/tests/released.rs`): natively as written, and for
  Workers with the rate-limit store and deployable area 16's "Rate limits
  on Workers" gives a Worker, the same addition `benchmarks/contract/worker.py`
  makes for workerd. The source declares no deployable: a native and a
  Workers deployable share a rate-limit store only as Redis, which until
  its driver comes leaves each Worker counting in its own memory, a
  `decline` the golden todo has no reason to make (area 14, "Its target,
  checked").
- The fixture intercepts only the declared public identity, key and mail
  hosts and answers locally; it proves nothing about production DNS or
  TLS filtering. SQL fixture writes and signing helpers are private test
  controls.
- Passing the authored tests alone does not establish the contract.
  TypeScript's run of the same cases is in
  [`typescript/`](typescript/README.md).
