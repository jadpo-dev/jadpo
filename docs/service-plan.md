# SERVICE-001 — outbound service contract

**Status:** RM-301 semantic contract frozen, 2026-10-02, after the
[independent correction re-review](../tests/validation/rm301-independent-contract-review.json)
resolved all six findings and the retry-catalog ambiguity. The reviewed candidate
body is pinned as SHA-256 `d0162532b1e7bcab8b0ed5dbf355830f9a7d4147284a0548e681c62b8949819f`.
This status/review-footer update records freeze without changing that body.
RM-302 owns exact parser spelling/diagnostics; RM-303 and the durable-job tasks
own executable adapter/recovery conformance. A generated Bun HTTP adapter and
loopback integration suite now exist; the scoped adapter review is approved in
the [RM-303 review record](../tests/validation/rm303-independent-implementation-review.json).
Durable-job and production-provider conformance are not claimed.

## Scope and source

The first implementation target is the owner-selected local HTTP reference
mail provider for `ReminderMail.send_overdue_reminder`. It sends no real mail.
The preserved baseline remains the pressure source. Its original app digest is
recorded in the [baseline provenance](../tests/validation/golden-baseline/provenance.json).
The owner-approved RM-107 email privacy revision removed `UserWithTodos.email`
from the current golden app without changing ReminderMail. Preserve both app
revisions when comparing this service contract:

- original `examples/golden-todo/app.jadpo`, SHA-256
  `9d7617942426da304a9b5b2d819dc2d460cebd720c2b5c7d65ae21d1da67b22f`;
- current successor `examples/golden-todo/app.jadpo`, SHA-256
  `9fd6595ea7287c36bf55cff80221162fa299e98581cbe8b2fc55293c090cc10b`;
- unchanged `examples/golden-todo/policy.jadpo`, SHA-256
  `ea80e3d77d5d2f59b04ecbf5a12fe93d32b5f92e885531c04136a3296331164b`.

The preserved pressure source uses `Todo.id` and a blanket timeout mapping.
Those two declarations are comparison evidence; the accepted successor below
supersedes them.
The [successor source](../tests/assurance/service-successor-v0.1.jadpo) declares
the same `POST /v1/messages`, `ReminderMessage` and `ReminderReceipt`, with a
nominal generated `ReminderIntentId`, explicit bearer credential sink, pinned
import and phase-aware outcomes. This is a contextual semantic candidate, not
a standalone compile-pass fixture: referenced golden types/configuration come
from the pressure application; RM-302 settles exact parser spelling.

The successor keeps a 5-second per-attempt upper bound and at most three
attempts within one 30-second logical-operation budget. The remaining deadline
may shorten either bound. The provider snapshot bytes are unchanged.

## Checked declaration and import

The current `service` spelling in the pressure source is a candidate, not an
accepted parser feature. RM-302 must first check one canonical positive
declaration equivalent to that source, then reject variants with a dynamic
endpoint, unpinned contract, undeclared operation/path, secret in a URL or
output, missing idempotency identity, or a provider call from a pure function or
query. Calls are permitted only from actions, jobs and workflows with an
admitted external effect. The effect graph records the operation, request and
response types, secret slot, egress authority, idempotency identity, deadline,
and possible outcomes. An atomic database transaction cannot include the HTTP
effect; a durable intent must commit first when an action needs both.

The `ReminderMail` block in the preserved pressure source remains the
comparison base. The separate successor replaces its lifetime Todo key and
unsafe blanket timeout mapping; it distinguishes proved pre-dispatch refusal
from uncertain possible dispatch. Neither pressure source nor successor is
claimed as a compile-pass service fixture. RM-302 must give a stable diagnostic for each
negative mutation before expanding the grammar:

| Mutation | Required rejection |
|---|---|
| Replace `base_url` with a runtime value, or alter `egress` to another authority | Dynamic or undeclared egress |
| Omit the import pin, change one byte of the imported snapshot, or add a second operation | Missing, stale or broadened import |
| Place `config.mail_api_key` in a URL, body, result, log or diagnostic | Secret sink violation |
| Omit `idempotency_key`, or derive it from an attempt ID | Unstable delivery identity |
| Call the operation from a pure function or query | Disallowed external effect |
| Call it within an atomic database write without a committed durable intent | Incompatible effects |
| Map a possibly dispatched timeout to a known no-effect failure | Unsafe outcome mapping |

The exact import-pin spelling and stable diagnostic codes are RM-302 syntax
work; this catalog fixes their semantics without pretending that the candidate
source already parses.

An imported OpenAPI snapshot is untrusted data. The candidate reference is
[`service-reference-mail-v0.1.json`](../tests/assurance/service-reference-mail-v0.1.json),
version `0.1.0`, exact-byte SHA-256
`c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d`.
Import is from a repository file, uses that SHA-256 pin and explicit version, and
never fetches a changing URL during build. The compiler validates its supported
subset and generates a review diff against the checked service declaration.
It rejects missing pins, digest mismatch, unsupported schema features,
ambiguous operation IDs, and any new host, path, method, request field, response
field or failure that broadens the accepted contract. A narrower import needs
an explicit reviewed compatibility disposition. The snapshot is a review
candidate, not authority to accept arbitrary OpenAPI features: RM-302 must
implement only the checked subset and reject unsupported constructs.

## HTTP and secret boundary

Production egress is exactly `https://mail.example.invalid:443` and
`POST /v1/messages`. The compiler-owned adapter rejects user-controlled hosts,
schemes, ports, paths, proxy routing and redirects, including same-host
redirects. It uses the typed `config.mail_api_key` only in the declared
`Authorization: Bearer <config.mail_api_key>` header after connection setup; it never interpolates the secret into
a URL, body, error, log, trace, audit, artifact, or fake call record. The secret
is never returned to application code. The adapter validates the closed request
before dispatch and the closed response before returning a typed receipt.
The generated `Idempotency-Key` header must equal the immutable request body's
`idempotency_key`, and both must be the persisted nominal intent UUID. A
mismatch is rejected before dispatch. Authored code cannot set a different
header, read the bearer value or construct a transport certainty result.
Unexpected fields, malformed JSON, invalid timestamps or provider objects are
contained as operational faults. A `202` response must echo the matching
`Idempotency-Key` header and carry a valid `accepted_at`; the adapter rejects a
missing or mismatched key as uncertain after dispatch. The explicit `400`,
`401`, `409` and `429` status/code pairs are the only mapped provider failures.
No raw response or provider exception crosses the boundary.

The local reference server is a real HTTP listener with an injected test-only
transport endpoint; its loopback address is not an authored egress permission or
production configuration option. The test adapter keeps the declared host,
path, method and credential-slot identity in its checked descriptor while
routing the socket to the disposable listener. An in-process fake is a separate
RM-304 surface and cannot establish HTTP adapter conformance.

## Outcome and retry matrix

Under the owner-approved successor, one logical delivery keeps the same
persisted intent ID through every permitted attempt and later reconciliation.
A new due-date revision gets a different durable intent. The historical
pressure source keeps `Todo.id`; the separate reviewed successor supersedes
that identity without silently rewriting the human-owned policy file. A new request ID may
be used for each attempt.
The provider fixture must deduplicate by the key and reject a different payload
under the same key. Its stored receipt is the only basis for claiming a
successful delivery after a lost response.

| Observation | Semantic outcome | Automatic retry |
|---|---|---|
| Validation or credential configuration fails before dispatch | `Misconfigured` or safe validation failure; no effect | No |
| Connection cannot be established; no bytes dispatched | `Unavailable`, known no effect | Yes, within the 3-attempt/30-second bounds |
| Provider explicitly rejects the recipient before acceptance | `ReminderRecipientRejected` | No |
| Provider explicitly refuses for rate limiting and guarantees no acceptance | `ReminderTemporarilyUnavailable` | Yes, within bounds |
| Provider authentication fails | `Misconfigured`; internal diagnosis only | No |
| Request may have been dispatched, then timeout, connection loss or malformed acknowledgement | `OutcomeUnknown`; keep delivery identity for reconciliation | No blind retry |
| Valid success receipt with matching idempotency key | typed `ReminderReceipt` | No |
| Provider returns an unexpected status or body without a no-effect guarantee | contained operational fault with uncertain effect if dispatch occurred | No blind retry |

The pressure source's blanket `provider.timeout ->
ReminderTemporarilyUnavailable` is rejected by the successor negative fixture.
Only compiler-owned transport evidence proving no request dispatch admits
retryable `Unavailable`; a timeout alone proves nothing. Possible dispatch
followed by timeout, loss, bad acknowledgement or unexpected response is
`OutcomeUnknown`. Request and response key checks remain adapter-owned; the
closed application receipt still contains only validated `accepted_at`. `OutcomeUnknown` uses RM-207's selected Bun HTTP response when it reaches
an HTTP boundary; a background job persists inspectable uncertainty and does
not resend on restart or the next schedule. Other target mappings require
their own explicit contract.

Retries use a monotonic deadline, bounded exponential backoff with full jitter,
and one shared budget across nested adapter calls. Cancellation stops any
future attempt. Neither an idempotency key by itself nor an expired worker lease
proves the provider rejected a possible first attempt. The pinned version 0.1.0 offers only POST and has no status operation.
Automatic status lookup and same-key replay of an unknown attempt are unsupported
in this version. Unknown remains pending trusted host/operator reconciliation;
the isolated reference harness may verify the provider's stored exact-key,
exact-payload acceptance record. An application-created receipt, a sent byte,
timeout or lease expiry is not reconciliation evidence. Adding a production
lookup/replay operation requires a separately pinned and reviewed successor.

## Reminder identity reconciliation

**Owner decision, 2026-10-02:** a changed due-date schedule may send a new
reminder. Each schedule receives a new durable delivery intent identity, retained
across attempts and reconciliation. “Yes and yes” also authorised the independent
service review. The previous question below is retained as decision rationale,
not an outstanding owner choice. The [initial independent findings](../tests/validation/rm301-independent-contract-review.json)
are addressed by the successor source, durable state contract and 30-case
catalog. A [JOB-001 replacement](../tests/assurance/service-acceptance-successor-v0.1.json)
is separately pinned to current acceptance bytes; the other 43 cases remain
unchanged. The independent review accepts these as the semantic successor contract;
there is no executable HTTP or job evidence.

### Durable identity and schedule guard

Compiler-owned private delivery state stores a generated nominal UUID, Todo ID,
schedule revision, immutable closed request payload, attempt budget, outcome
and receipt evidence. It is not a new application entity, role or source-level
capability. Clients and ordinary authored actions cannot read, forge or mutate
it. Application jobs retain POLICY-D30 service-principal/effect checks; service
delivery receives none of RM-205's maintenance authority.

Todo creation establishes a compiler-owned schedule revision. Every successful
patch supplying `due_at` advances it and clears `reminder_sent_at` in the same
transaction, including a supplied value equal to the old one; omission or a
failed patch does neither. This preserves the pressure source's supplied-field
semantics and prevents ABA (A→B→A) receipt confusion. A transaction with a unique
`(Todo.id, schedule_revision)` constraint creates one intent or returns the
existing one. Identity and payload commit before any HTTP attempt. Conflict,
worker restart and known-no-effect retry reuse that ID and payload; they never
create a second identity for the same revision. Terminal/cancelled identities
also retain that uniqueness.

A separate transaction admits one dispatcher only after rechecking the current
revision, overdue/open/unsent/visible Todo, active owner, current authorised
recipient, external-effect policy and remaining shared retry budget. Payload
fields are fixed at creation; changed sender/title cannot mutate them under the
same key. A now-unauthorised recipient or disabled owner prevents admission.
Required-owner visibility is applied before ordering and limit, as in DATA-007.
The transaction records `possible_dispatch` durably before releasing its claim
to the HTTP adapter. No database transaction is held across HTTP.

When an eligible current-revision intent reaches key-ordered admission, older
pending or `retry_wait` intents for that same source whose schedule revision is
stale are cancelled under the ordering-key lock before the successor is
claimed. The native cancellation check requires `possible_dispatch = 0`, so
those records cannot have produced a provider effect and their claim/attempt
history remains intact. If a claim races this cancellation before its possible-
dispatch checkpoint, the shared key lock decides the order: cancellation can
make the predecessor terminal only while `possible_dispatch = 0`, and a later
checkpoint then fails. A running predecessor that has crossed the checkpoint,
and any `outcome_unknown` predecessor, remain active and keep the successor
behind FIFO until a trusted terminal outcome or resolution.

Committed admission is the concurrency cut: a due-date patch before it prevents
A dispatch; a patch after it may race A's HTTP request. A crash after admission
without a durable outcome leaves A unknown, even if no bytes were actually sent.
Lease expiry or a second worker cannot manufacture proof of no effect. A listed
transient failure with proved no acceptance may be durably recorded for a
budgeted same-ID retry; uncertain or defective attempts never are.

Valid exact-key acknowledgement is retained against A's immutable intent even
if B has superseded it. In the same completion transaction,
`Todo.reminder_sent_at` is written only when Todo still names A's revision and
is unsent. Otherwise B stays unsent and may later deliver using its own ID.
Unknown A remains inspectable under A; a B intent may be created but its dispatch
is held while an earlier intent for the same Todo is possible/unknown. Trusted
resolution releases B's eligibility; it never silently resends A. Late A outcomes
update A only and cannot authorise a new B sent mark. RM-305 fixes scheduling,
lease and operator surfaces without weakening these service invariants.

### Policy and acceptance successor

The catalog records the narrow owner-selected policy amendment: replace
`external_effect ... idempotency_key: Todo.id` with a persisted `ReminderIntentId`
bound to the same authorised Todo and schedule. Job caller, recipient
`Todo.owner.email`, bearer secret sink, and prohibition on client sent-state
writes stay unchanged. The human-owned `policy.jadpo` bytes and digest remain
comparison evidence; this agent does not claim to approve that file. The owner's
new-identity direction and the independent review disposition govern this scoped
semantic successor before lowering. No general delivery-state authority or
additional recipient permission follows from it.

JOB-001's separate successor expects that persisted UUID in body and header, a
compiler-observed sent time, and a matching-revision sent mark. JOB-002 retains
no additional call after confirmed success. The catalog's A accepted → patch B
→ eligible B → second 202 trace requires two distinct keys and two acceptances;
this describes required future executable evidence, not a completed HTTP test.

### Receipt time trust

Under TIME-D22, provider `accepted_at` is validated as `Instant` and retained
only as provider evidence. It has no security, ordering, idempotency or retry
authority. The compiler records its own wall-clock receipt observation at valid
acknowledgement (or trustworthy reconciliation) and uses that instant for the
revision-guarded `reminder_sent_at`. An implausible but syntactically valid
provider clock cannot mark a different revision or change the logical identity;
monotonic deadlines remain separate. The source receipt need not expose the
compiler's private observation field.

Historical planning review, 2026-10-01: the frozen application uses `Todo.id` as the
idempotency key and clears `reminder_sent_at` when the due date is supplied.
This candidate rejects a changed payload under the same key. The review before
RM-302/RM-108 must therefore specify what a reminder rescheduled after delivery
means, and show a trace of initial acceptance, date change, later schedule and
provider response. Preserve the original key and frozen source; a new reminder
identity or repeat-delivery policy requires an explicit reviewed successor, not
an implementation shortcut. Likewise retain the existing post-dispatch timeout
versus unknown-outcome revision requirement. These were the historical contract gaps; the separate successor above now
records the owner-selected correction. No HTTP implementation evidence existed
at that planning point; see the RM-303 implementation checkpoint below.

Concrete decision trace, 2026-10-02: Todo `t1` is due on day A and the provider
accepts a message under key `t1`. A later patch moves its due date to day B and
clears `reminder_sent_at`. If the next job sends the changed payload under `t1`,
the pinned provider returns `409 idempotency_conflict`; it must not be treated as
a successful day-B reminder. The recommended successor gives each durable
reminder intent its own generated identity, fixed across its attempts and
reconciliation, and creates a new intent when a new schedule calls for a new
reminder. That choice revises the frozen `Todo.id` key and JOB-001 expectation,
so the owner's accepted direction still requires a reviewed successor. The alternative is
one reminder per Todo lifetime; then clearing `reminder_sent_at` cannot imply a
new send, and the patch/job contract must be revised instead.

## Fixture and review gate

The candidate case catalog is
[`service-contract-v0.1.json`](../tests/assurance/service-contract-v0.1.json).
It covers success; recipient, rate-limit and auth failures; pre-dispatch
refusal; post-acceptance response loss; malformed body; redirect; forbidden
host; wrong or reused key; exhausted deadline; and a secret canary in every
output surface. The 30 cases record expected semantic outcome, stored provider state,
dispatch count and retry eligibility, including rescheduling, duplicate workers,
crash recovery, wrong keys, ABA revisions and provider-clock trust. Compile-fail
candidates include the blanket timeout mapping. The registered RM-303 suite
passes 19 tests and 90 assertions for the bounded HTTP adapter cases below,
including provider acceptance followed by a lost acknowledgement and exact-key
deduplication. Broader job scheduling, duplicate-worker, restart, rescheduling
and reconciliation cases remain assigned to RM-307/RM-108. The fake fixture
exposes only declared operations and typed outcomes; it does not substitute for
the local HTTP traces.

The completed RM-302 review verified the canonical source and imported snapshot
digests, closed types, effect and secret flow, no-effect versus unknown
classification, retry budget, and the source revision above. RM-207's provider
uncertainty mapping is approved. RM-303's real HTTP traces and its independent
implementation review are complete for the scoped adapter boundary; see the
[review record](../tests/validation/rm303-independent-implementation-review.json).
The author's self-check is not that review. Verification here is
contract/fixture consistency, not a verify-loop campaign: the implementation's
failure injection is already specified.

## RM-303 adapter implementation plan

The compiler emits one closed `ReminderMail.send_overdue_reminder` call using
the pinned host, port and path; authored values cannot choose a destination.
The Bun target opens one direct `Bun.connect` socket per attempt, with TLS and
the pinned server name in production; it does not consult proxy settings or
reuse connections. Before any HTTP bytes are written, the adapter checks both
the peer's trusted-CA status and the leaf certificate's hostname against
`mail.example.invalid`; `Socket.authorized` alone proves only the CA check
([Bun socket reference](https://bun.com/reference/bun/connect)). A connection,
chain or hostname verification failure is known no-effect. Once the verified
TLS connection's write begins, timeout, connection loss or an invalid reply is
`OutcomeUnknown`. The test hook redirects only to an IPv4/IPv6
loopback HTTP origin. The adapter uses the same immutable UUID in the JSON body and
`Idempotency-Key` header, bounds credential, request and response sizes, accepts only the pinned
statuses and closed JSON shapes, and never logs payloads, credentials, provider
bodies or raw transport errors. The checked `config.mail_api_key` remains inside
the compiler-generated adapter.

The test-only transport override is a trusted host hook gated to test mode and
loopback endpoints. It preserves the pinned Host header, method, path, key and
body while directing traffic to a real ephemeral listener; authored configuration
cannot select it. Retry uses the frozen three-attempt/30-second outer budget,
5-second per-attempt timeout and full jitter, and only retries explicit
no-dispatch failures or the pinned rate-limit response. Tests must exercise
success/deduplication, mapped provider failures, pre-dispatch refusal/timeout,
post-dispatch loss/timeout, malformed/mismatched acknowledgements, redirects,
unexpected responses, secret canaries, retry/exhaustion and the immutable
egress boundary. The current loopback suite covers these adapter cases, along
with an elapsed deadline and attempt budget shared across service calls in one
operation. Job/restart/reschedule traces remain RM-307/RM-108 work.

**RM-303 implementation checkpoint, 2026-10-03:** The generated adapter uses
one direct verified-TLS socket per attempt and explicitly checks the leaf
certificate hostname before sending any HTTP bytes. The registered real
loopback HTTP/TLS suite passes **19 tests and 90 assertions**, including trusted
and wrong-host certificates, typed response mappings, credential/request
bounds, shared operation deadline and attempt budget, exact-key deduplication,
pre-dispatch retries, cancellation, and a provider acceptance stored before a
lost acknowledgement and audit-contract parity. The full supported verifier
passes **59/59 steps** ([report](../build/validation/20261003T234639-50956/report.json));
it still marks release equivalence false and the full golden app fails
compilation. The independent [RM-303 implementation review](../tests/validation/rm303-independent-implementation-review.json)
approves this scoped adapter boundary; durable-job and production-provider work
remain open.
