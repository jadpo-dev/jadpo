# Generated Bun target v0.1

**Status:** accepted for the P9 jadpo-seed slice  
**Target:** TypeScript executed by Bun 1.2 or later

`jadpo build <project>` first runs the complete syntax, semantic, nominal
type, failure, and effect checks. It then refreshes the seven P8 artifacts and
writes one disposable executable target:

```text
build/
  target/
    app.ts
```

The generated file has no package dependency or framework configuration. It
exports a Fetch-compatible `handleRequest(request)` function and starts
`Bun.serve` when executed as the main module.

## Closed runtime dependency contract

A generated application requires a Bun executable and nothing from a package
registry. It must never require `bun install`, `npm install`, or an equivalent
step.

Generated TypeScript may import only:

- `bun` and `bun:*` built-ins supplied by the Bun runtime; and
- compiler-owned relative TypeScript modules emitted in the same `build/`
  target set.

The generator rejects any bare third-party module with
`JADPO_TARGET_EXTERNAL_MODULE`. It also rejects generated `package.json`,
`bun.lock`, `bun.lockb`, or `node_modules` artifacts with
`JADPO_TARGET_DEPENDENCY_MANIFEST`. Authored source has no import or package
escape hatch that can bypass this check.

CI and acceptance commands use Bun's `--no-install` option. This matters because
Bun can otherwise auto-install an unresolved bare package during execution;
the disabled mode proves the target is closed over Bun's runtime and its own
generated files.

Applications use an explicit empty environment file, including the
compiler-owned `test` and `dev` launches. The pinned Bun 1.2.20 ignores the
unsupported `--no-env-file` flag and still discovers dotenv files. A production
launcher must retain both package and environment-loading boundaries:

```text
bun --no-install --env-file=/dev/null build/target/app.ts
```

The generated application reads only declared binding names from `Bun.env`,
decodes one complete typed snapshot, and exits before `Bun.serve` when any
required value is missing or invalid. Cloud-specific launch generation remains
outside this target contract; dropping the explicit empty-file argument is not
a supported production launch. Windows launches use `--env-file=NUL`.

P11 adds one narrow capability exception without changing this P9 seed
evidence: when JWT bearer validation is explicitly declared, the compiler may
add its exact pinned `jose` dependency plus a compiler-owned frozen
lock/integrity record. Applications without that declaration retain this closed
dependency-free contract. Authors cannot select, replace, or import the package,
and Bun auto-install remains disabled in both modes. See the
[AUTH-001 implementation plan](authentication-plan.md).

## Runtime boundaries

The v0.1 generator implements the already-checked core constructs needed by the
seed: semantic scalar types and constraints, closed input/output records,
actions and functions, conditionals, validated construction, returns, typed
rejection, and explicit public routes.

At the HTTP boundary it:

- generates a fresh `req_<uuid>` request ID and returns it in the body and
  `x-request-id` response header;
- parses JSON and validates the exact closed input shape before invoking the
  action;
- applies the same email, length, pattern, primitive, and record constraints as
  the compiler slice;
- validates and reconstructs the declared output, thereby serialising only its
  declared fields;
- maps declared domain failures through compiler-derived status, code, message,
  and public-field allowlists;
- never copies internal failure context into the response;
- maps malformed JSON to 400 `invalid_request`, declared body contract violations
  to 422 `invalid_input`, and declared path value violations to 422 `invalid_value`,
  following the owner-selected GF-032 frozen golden expectations; and
- contains output-contract failures, unknown generated-runtime failures, and
  other defects behind the generic 500 `internal_fault` envelope.

Input validation is handled locally inside the route branch. A validation error
after action invocation is therefore an internal contract defect, not a client
400. This distinction prevents generated or application defects from being
misclassified as caller mistakes.

Authentication is deliberately not stubbed. A route without the exact opt-out
`auth: none` produces `JADPO_TARGET_AUTH_NOT_IMPLEMENTED` until P11 provides
the required-default authentication runtime.

## Checked scheduled-job frontend (RM-306)

The frontend now checks the narrow ASYNC-001 schedule entry and exact nominal
clock binding, for example `run: scan(JobRunAt(clock.now))` where JobRunAt is an
unconstrained named scalar directly based on Instant. It preserves the ordinary
primitive-signature ban. Jobs are non-callable/non-exported; their static graph,
failure contract, snapshot-constructor proof and reachable service effects are
auditable. This is **non-executing frontend evidence**: `jadpo artifacts` can
emit `audit/jobs.json`, but target/build returns
`JADPO_TARGET_JOB_NOT_IMPLEMENTED` until a reviewed finite execution profile,
durable failure dispositions and checked worker lowering exist. It never emits
an inert job registry or invokes the action at startup. Historical pressure
syntax is not certified or rewritten. The
[owning plan](work-plans/golden-delivery-planning.md#rm-301307108--services-durable-jobs-and-reminders)
records the initial primitive-signature mismatch, correction review and current
verification. No event/subscriber grammar, service authority or runtime ASYNC
trace is established by this frontend slice.

## Private durable delivery storage foundation (RM-306)

The generated persistence module contains compiler-owned outbox and claim
primitives using the application's selected SQLite/PostgreSQL authority database.
They are **private host integration under development**, not a checked authored
job API, scheduler or operator endpoint. No additional package or hosted queue
is required. [ASYNC-001](async-plan.md) owns the frozen semantics; the
[current delivery plan](work-plans/golden-delivery-planning.md) records staged
implementation and acceptance.

Enqueue requires the live source transaction's attempt token and connection.
Its immutable source identity, payload/version and per-key sequence survive
commit/restart; rollback removes the mutation and intent together. Token expiry
is shared across views, and each SQL invocation rechecks it. The independent
[storage correction review](../tests/validation/rm306-independent-correction-review.json)
accepts retained-client containment and rejection of sparse/unsupported arrays.

The reviewed private state slice adds database-time claims, cumulative immutable limits,
first-claim lifetime, BIGINT fences, FIFO admission and a pre-effect checkpoint.
Late checkpoint returns give no admission, while an already-written marker
remains conservative evidence; expiry after possible dispatch becomes unknown
and blocks successors. Allocation uses fresh database time; late results return
no handle while preserving any charged invocation/fence. Seven generated-host
claim cases pass on SQLite (79 assertions), and the full PostgreSQL persistence
suite passes 56/56 (464 assertions). The independent
[claim correction review](../tests/validation/rm306-independent-claim-correction-review.json)
approves the stale-time correction within this private foundation, not the whole
task. The fresh supported [gate](../build/validation/20261004T091849-87931/report.json)
passes 61/61. Independent probes used real SQLite; PostgreSQL execution is main
verification evidence, not reviewer-run evidence.
These are state primitive tests, not provider dispatch or the 18-case ASYNC catalog.

Bounded lease renewal has its own independent
[review](../tests/validation/rm306-independent-renewal-review.json): exact current
claim/fence and live stored deadlines guard a fresh database-time update, capped
by the immutable execution/lifetime. Counts and possible-dispatch evidence do not
reset; late results return no handle. Three generated-host cases pass on both
adapters. Database clock rollback may shorten a lease; no monotonic wall-clock
or cached-handle dispatch guarantee is claimed.

Additional private slices are implemented: reference-mail outcome storage has
independent [approval](../tests/validation/rm306-independent-mail-outcome-review.json).
Cancellation before checkpoint atomically ends work, while after possible dispatch
it only records a durable request and does not release an unknown key. The
reference-mail-only acknowledgement accepts the closed validated `accepted_at`
shape, conditions success on a live current fence/possible-dispatch marker, and
stores the receipt with a database observation in a nested savepoint. Provider
time is evidence, never claim/retry authority. A native receipt constraint failure
rolls the terminal transition back even if the host catches it. An acknowledgement
eligible at the guarded SQL sample is retained if the statement response is late;
an already-expired/stale claim cannot newly acknowledge. Explicit uncertainty
keeps the original intent blocked, including after restart. These trusted-host
tests do not prove that a receipt originated at a provider or that authored code
cannot forge it: that authority requires the future checked worker binding.
The fresh [gate](../build/validation/20261004T094724-2900/report.json) passes 61/61;
the full PostgreSQL persistence suite passes 67/67 (608 assertions), with five
focused SQLite mail-outcome cases passing (69 assertions). Cancellation's
[original review](../tests/validation/rm306-independent-cancellation-review.json)
found a caught-state-fault orphan-request gap. The correction wraps cancellation
in a method-local savepoint; real SQLite/PG trigger failures can be caught while
the outer host commits unrelated work, without retaining an unmatched request.
Four focused SQLite cases pass (49 assertions), and the full PostgreSQL suite
passes 68/68 (616 assertions). The corrected fresh
[gate](../build/validation/20261004T095612-6271/report.json) passes 61/61;
the independent [correction review](../tests/validation/rm306-independent-cancellation-correction-review.json)
approves the exact cancellation fix, with 27 separate real SQLite probes and
main-attributed PostgreSQL evidence. All approvals remain storage-foundation-only.

The private reference-mail retry slice persists closed known-no-effect failure
classes/counts, one outcome per fencing generation, and an absolute database-time
retry eligibility instant. Allocation checks that instant again in guarded SQL;
restart, later scans and safe reclaim retain the original lifetime/invocation
limits. Permanent or exhausted safe failures retain bounded dead-letter metadata
(identity/version/fence/counts/class/observation only, no payload or raw error).
Current trustworthy no-effect proof can complete a stored cancellation request;
unknown/expired/stale outcomes never reopen resend. This is trusted-host state,
not checked adapter provenance or full-jitter/scheduler execution.

The [original retry review](../tests/validation/rm306-independent-retry-state-review.json)
found a P1 in the inherited savepoint dependency: native whole-transaction
rollback followed by caught failed cleanup left a reusable client. The
[scoped correction review](../tests/validation/rm306-independent-retry-state-correction-review.json)
accepts shared attempt poisoning/revocation, local SQL/policy-entry guards and
poisoned-callback refusal before commit. Real SQLite whole-rollback and real
PostgreSQL injected rollback-to failure regressions now reject subsequent
outbox/authority writes. Proved ordinary savepoint recovery still permits an
unrelated outer commit. Unproved recovery remains conservative unknown where
appropriate, never an automatic replay or permission to autocommit. This does
not cancel already-issued SQL; callers must still await all work.

The preceding retry-only [gate](../build/validation/20261004T112820-24028/report.json)
passed 61/61 before the poisoning correction. Current focused generated-host
retry/poison cases pass on both applicable adapters. The next full gate exposed
the independently reproduced golden self-disable field-policy conflict
([failed gate](../build/validation/20261004T113901-31108/report.json),
[policy review](../tests/validation/rm108-independent-policy-composition-review.json)).
The owner subsequently approved exactly two self-update-only lifecycle field
grants. After that source repair, the fresh supported
[gate](../build/validation/20261004T141712-52403/report.json) passes 61/61,
including the current retry/poison guards and both golden adapter suites.
Release equivalence remains false; no ASYNC or checked-worker conformance is
inferred. Do not treat the earlier passing gate as verification of the new guard bytes.
The [owning checkpoint](work-plans/golden-delivery-planning.md#checkpoint) records
current counts, authority input and pending compiler/worker stages.

Limits remain explicit: exact compiler-owned table layout is assumed; schema
attestation/migration, public typed payload/identity bindings, policy admission,
full authored completion/cancellation authority, public safe retry/dead-letter diagnostics, scheduler,
worker integration and privileged reconciliation are not established. A host
must await source operations and the transaction commit before using a returned
checkpoint result; an uncommitted callback result is not dispatch authority.
Already-invoked SQL and uncertain commits retain the adapter's existing rules.

## Executable evidence

The [Bun acceptance suite](../tests/runtime/jadpo-seed.test.ts) starts a real
TCP listener on a bounded temporary localhost port and sends Fetch requests
through the generated handler. It proves:

- a valid registration returns the exact declared output;
- malformed email, constrained invite code, malformed JSON, and unknown fields
  all fail before action execution with a safe 400 response;
- a percent-decoded `Customer.email` path binding is validated before an inline
  route action, and an invalid segment receives the same safe 400 envelope;
- typed query values are decoded exactly once from the raw URL, defaults apply
  only to absent keys, malformed transport syntax receives 400, and valid
  values that fail declared shapes or refinements receive 422; integer decoding
  rejects values outside the exact JavaScript safe-integer range before rounding;
- routes with ordinary typed headers use the generated `node:http` adapter to
  count raw case-insensitive field lines before Fetch normalization, rejecting
  repeated declared values while ignoring unrelated transport headers; request
  bodies remain streamed so protected-route authentication and CSRF checks run
  before body consumption;
- the exact reserved invite code maps automatically to the declared 422
  failure; and
- neither the internal `invite_code` field nor its `reserved` value appears in
  the public response.

`bun build --no-install` also bundles the generated file successfully as a
dependency-free Bun entry point. Generated target source remains disposable
and is not the normal review or debugging surface; authored source, derived
audits, OpenAPI, and semantic metadata retain those roles.
