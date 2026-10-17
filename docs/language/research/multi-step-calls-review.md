# Multi-step provider calls: a stress test

Status: research, 2026-10-16, for item 7b of
[packages and extension points](packages-and-extension-proposals.md).
Decides nothing. Vendor facts come from each vendor's own documentation,
read on 2026-10-16 (sources at the end); anything not confirmed there is
marked "(to check)".

## Summary

**Does it hold?** Yes, with changes. Recording each answer and replaying it
on a rerun is what Temporal, Azure Durable Functions, Restate and DBOS do,
and Temporal's suggested idempotency key (run id plus activity id) has the
same shape as the proposal's. Jadpo differs from all of them in two ways,
and most of the fixes below follow from them:

- **One build runs at a time,** so Jadpo cannot keep an unfinished action
  on the code it started with, as Restate, Step Functions and Temporal's
  pinned mode do.
- **Database reads are not recorded,** so a rerun can compute a different
  request from fresh data.

The answer to both is the same: check every replayed step against what was
recorded, and stop before anything is sent when they differ.

**Must fix.** Each is small; the numbered cases below give the detail.

1. **Record each step before sending it** (operation, a fingerprint of the
   request, the time sent), then its answer. Without the first write the
   runtime cannot tell a step in doubt from one never sent, cannot respect
   a provider's key window, and cannot catch a changed request (cases 2, 3
   and 7).
2. **Check the whole path on a rerun:** each replayed step's operation
   *and* request fingerprint, and at the end that every recorded step was
   replayed. A mismatch stops the action as `diverged` before anything is
   sent. This one check covers deploys, loops, branches, changed reads and
   changed configuration (cases 1, 3, 4 and 15). The proposal checks only
   the operation.
3. **`clock.now` stays fixed for the life of a record,** across every
   rerun. Today a retry and area 6's rerun both take a fresh time, so any
   request carrying the time would diverge on every rerun (cases 5 and
   10).
4. **Derive the provider's key by hashing,** not by appending a step
   number to the client's key. Area 7 scopes a client's `Idempotency-Key`
   to the caller, but Stripe scopes keys to the account, so two users who
   send the same key would be given one Stripe answer. This affects today's
   single call too. Hashing also fits Square's 45-character limit (case
   17).
5. **The package states how its keys behave:** how long the provider keeps
   them, their maximum length, the codes for "in use" and "different
   request", and whether failed answers are replayed. The runtime never
   resends an unanswered step after the provider's window; it parks it.
   PayPal Orders keeps keys for 6 hours, a quarter of a subscriber's
   default retry budget (cases 2 and 7).
6. **Several changing calls only where the action's key survives a
   retry:** subscribers, and routes whose API requires `Idempotency-Key`. A
   route without one gets a new request id when the client retries, and a
   schedule's retry is the next tick with a new key, so neither can resume;
   both would repeat steps 1 and 2 under new keys (case 16).
7. **No outside call inside `for`** in an action with several changing
   calls. Steps are then numbered along a path whose length the compiler
   knows, and batch work stays one event per item (cases 11 and 15).
8. **One runner per record:** a delivery's lease carries a generation that
   every record write checks, and a provider's "key in use" answer means
   wait and retry, never a failure (case 6).
9. **An unfinished record never disappears quietly.** One that ends without
   committing (failed, diverged, abandoned by a client, parked) becomes an
   operator item listing what took effect at each provider, with retry and
   abandon (cases 7, 8 and 9).
10. **Records live in the store the action writes** and are deleted in its
    commit transaction; answers are encrypted, capped in size and listed in
    the personal-data report (case 12).

**What should change the decision.** Nothing overturns it, but three things
narrow it, and they are worth knowing before it moves onto the area pages:

- Must-fix 6 means a route makes several changing calls only if its API
  requires `Idempotency-Key`. Browser forms rarely send one, so the usual
  shape becomes a route that answers 202 and a subscriber that does the
  steps.
- GitHub, Twilio Messages and Postmark take no key at all, so their
  changing calls stay one per action. Stripe's v1 API stores a 500 under
  its key for 24 hours, so a 500 at a later step parks the action for an
  operator rather than retrying.
- The warning for "can fail for good after an earlier step" fires on the
  scenario page's own Stripe example: a declined card after the customer is
  created and the card attached. It is the common case, not an edge, so
  the warning's text and fixes matter more than the proposal assumed.

## Words used here

- **Step**: one outside call made by an action after its first changing
  call, or the first changing call itself; numbered from 1 along the path
  the action runs, package actions' calls included.
- **Record**: the rows holding an action's steps, keyed by the action's
  key.
- **Replay**: on a rerun, a step already answered gives its recorded answer
  and sends nothing.
- **In doubt**: a step recorded as sent with no answer recorded.

## The cases

### 1. A deploy between the failure and the rerun

**What goes wrong.** Steps 1 and 2 were recorded by build A, and build B
reruns the action. B may have reordered the calls, added one before step 2,
removed step 3, upgraded the package, or changed the request at a recorded
step (a new field, a different price in configuration). Checking only the
operation at each step misses three of these: two calls to the same
operation swapped, a changed request, and a removed last step. The last is
the worst: B commits, and the provider keeps an effect the code no longer
knows about.

**How others handle it.** Restate and Step Functions keep a run on the
deployment or version it started on, and an old deployment is removed only
once drained. Temporal offers the same as pinned worker versioning, or
`patched()` markers in the code. Durable Functions calls adding, removing
or reordering calls a breaking change and gives each instance a version for
life. DBOS recovers only workflows whose version (a hash of the source)
matches. Inngest matches steps by name, lets runs pick up new code, and
only logs a warning on a reorder. When replay does not match, Temporal and
Restate keep the run open and retrying until fixed code is deployed rather
than failing it.

**Rule.** Jadpo cannot pin, so it detects and holds:

- Replay checks each step's operation and request fingerprint, and at the
  end checks that every recorded step was replayed (must-fix 2). The
  fingerprint is taken over the typed request value, not the encoded
  bytes, so a package upgrade that encodes the same values differently is
  not a change.
- A mismatch stops the action as `diverged` before anything is sent, naming
  the step, its operation and the top-level request fields that differ. As
  in Temporal and Restate it is held, not failed: the operator retries
  after deploying code that matches, or abandons it.
- Answers are stored as the provider sent them and decoded by the current
  package on replay, so a field new code reads is there; a stored body the
  new package cannot decode is `diverged` too.
- Each record keeps a digest of the action's step shape (the operations on
  each path, package steps included). `jadpo deploy` counts unfinished
  records whose action's shape the new build changes and asks before
  deploying. Changes that need no warning: code after the last recorded
  step, new steps after it, writes, and failure mappings.

```text
$ jadpo deploy production
  3 unfinished runs of billing.subscribe were recorded with steps this build changes:
    step 3 was stripe.subscriptions.create, is now stripe.subscription_schedules.create
  They will stop as diverged when they next run. Deploy anyway? [y/N]
```

### 2. Provider key windows

**What goes wrong.** Windows vary widely:

| Provider | Window |
|---|---|
| PayPal Orders v2 | 6 hours, up to 72 on request; other PayPal APIs differ (a refund example says 45 days) |
| Stripe v1, Shopify | 24 hours |
| Checkout.com | 72 hours |
| Adyen | 7 to 14 days |
| Stripe v2, Braintree | 30 days |
| GoCardless | at least 30 days |
| Square | 24 hours per a non-Square source (to check) |
| GitHub, Twilio Messages, Postmark | no key |

Jadpo's side: a subscriber retries 10 times within 24 hours by default and
can ask for days; the circuit breaker holds deliveries without spending
their budget; `outcome_unknown` waits for an operator indefinitely; a
route's key lasts 24 hours.

An answered step is replayed from the record and never sent again, so the
window does not matter for it. That is the record's main advantage over
leaning on the provider's own keys. The danger is the step in doubt: sent
again after the provider has forgotten its key, it runs twice.

**How others handle it.** None of the durable-execution systems knows a
provider's window; Temporal, Inngest and Cloudflare leave the key to the
author of the step. Restate's own idempotency retention defaults to one
day (to check).

**Rule.** A keyed package's `idempotency:` block must give `window:`. The
runtime resends a step in doubt only while its send time plus the window,
less a margin (the smaller of a tenth of the window and one hour), is still
ahead; after that the action parks as `outcome_unknown` naming the step.
The same holds for today's single call. A package may instead mark an
operation safe to repeat as it is (a `PUT` that replaces, a `DELETE`),
which needs neither key nor window; that is the only way a GitHub call can
be a step.

### 3. A key reused with different parameters

**What goes wrong.** A rerun computes a different request for a step (the
causes are cases 4 and 5). For an answered step nothing is sent, but the
code carries on with the answer to a request it would no longer make. For
the step in doubt, the resend carries the old key and a new body.
Providers disagree about what happens then:

- Rejected: Stripe v1 (`idempotency_error`), Shopify
  (`IDEMPOTENCY_KEY_PARAMETER_MISMATCH`), Square (`IDEMPOTENCY_KEY_REUSED`),
  Braintree (915232); the IETF draft says 422.
- Not stated: Adyen, PayPal, Checkout.com, GoCardless (to check). A provider
  that answers from the key alone hands back the answer to a different
  request, silently.

Stripe also scopes a key to the account across endpoints, so one key on
two different operations is the same error; the step number in the key is
what prevents that.

Area 11's reference package maps `idempotency_conflict` to `misconfigured`,
which lumps "a different request" together with "still running" (case 6).

**How others handle it.** Temporal checks the operation at each position on
replay, not its input; DBOS raises an error when the same step records a
different result.

**Rule.** The fingerprint check (must-fix 2) runs before sending, so a
changed request never reaches the provider. The package names the
provider's mismatch code; if it arrives anyway (an upgrade changed the
encoding of a step in doubt), the action is `diverged`, never
`misconfigured`.

### 4. Reads between steps

**What goes wrong.** Area 6 gives the reads between two outside calls a
snapshot of their own, and a rerun reads again. A changed database read can
send the rerun down another branch (the operation check catches it), build
a different request (only the fingerprint catches it), or skip a recorded
step (only the end check catches it). A provider read, such as
`invoices.retrieve` between two changes, can also answer differently.

**How others handle it.** All of them make reads steps. Temporal and
Durable Functions put all input and output in activities; Inngest and DBOS
list database access as step work; Cloudflare Workflows allows conditions
only on the triggering event and earlier steps' outputs.

**Rule.**

- **Provider reads** after the first changing step are recorded and
  replayed like changing steps (no key sent). They are outside calls
  already, cheap to record, and they make the path depend only on the
  record.
- **Database reads** are not recorded. They can be large, they are the
  app's own data (area 15), and area 6's rerun exists precisely to read
  fresh. The fingerprint guards them: a fresh read that changes nothing
  sent is harmless, and one that changes a request stops the action as
  `diverged`. This is weaker than Temporal, which forces determinism, but
  honest: the provider already acted on the old request, and nothing
  automatic can reconcile that.
- **Reads before the first changing step** are not recorded: no record
  exists yet, and a rerun starts afresh with nothing to protect.

### 5. `clock.now`, generated ids and other changing values

**What goes wrong.** Area 10 fixes `clock.now` for one operation, but "a
later request, schedule run or retry gets a new value", and area 6's rerun
runs "with fresh reads and a fresh `clock.now`". A step whose request holds
the time (`metadata: { requested_at: clock.now }`, a billing anchor, a
collection date worked out from today) would diverge on every rerun.

The others are already safe:

- New records' ids come from the invocation's seed when they are created,
  and creates come after the last call, so a new id cannot reach a request.
- Configuration and translations change only with a deploy (case 1).
- The language has no random numbers.
- Iteration order of maps and sets is assumed stable (to check against
  area 2).

**How others handle it.** Temporal stores time and random values in the
history; Durable Functions has `CurrentUtcDateTime` and a deterministic
`NewGuid()`; Restate has `ctx.date.now()` and `ctx.rand` seeded by the
invocation; Inngest, Cloudflare and DBOS ask for the value to be wrapped in
a step.

**Rule.** Once a record exists, `clock.now` is the time stored in it (the
first run's start) for every rerun, area 6's included. Before the first
changing step a rerun takes a fresh time, as today. This amends areas 6 and
10. One consequence to accept: a delivery that resumes 20 hours later
writes timestamps from when it began, which is also when the provider
acted.

### 6. Two runs at once

**What goes wrong.** Two runners can hold one record: a delivery whose
lease lapsed on a slow worker is taken by another server, or a client
retries with the same `Idempotency-Key` while the first request runs. Both
replay and send step 3 under the same key. The provider answers the second
with its "in use" response: Stripe 409, Adyen 409 or 422 (code 704),
Shopify `IDEMPOTENCY_CONCURRENT_REQUEST`, Braintree 915233, Checkout.com 409
(wait 30 seconds); PayPal "might fail the second". Treated as a failure,
that ends the action for good, or as `misconfigured` in area 11's reference
mapping. Both runners then write the record, and the slower can overwrite.

A request and a redelivered event that start the same business operation
are not this case: they are two triggers with two keys, so two operations
(see the last rule).

**How others handle it.** All single-flight by id: one Temporal execution
per workflow id, one Restate handler with write access per object key, one
DBOS execution per workflow id, Inngest's singleton mode.

**Rule.**

- **Routes** are already single-flight: area 7 claims the key before the
  action runs, answers a repeat 409 while it runs, and lets the claim lapse
  after the deadline plus five seconds.
- **Deliveries**: the lease carries a generation, and every record write is
  conditional on it, so a runner whose lease lapsed fails its next record
  write and stops before sending another step. A step it had already sent
  shares its key with the new runner's, so the provider deduplicates.
- **"In use"** is named in the package and means wait: the runtime retries
  within the call's budget, then ends the run as retry-later. Never a
  failure, never `misconfigured`.
- **Two triggers for one operation** (a route and a webhook both starting
  a subscription) get two keys. Checking state first is the app's job; the
  mechanism cannot see that they are the same.

### 7. A timeout at step 2 of 4, and parking

**What goes wrong.** Today an unmapped timeout is `outcome_unknown` and
parks, and the operator "checks with the provider, then marks it done or
not done". Halfway through a sequence "done" does not fit: steps 3 and 4
and the writes need step 2's answer, which the operator does not have.

For a keyed step inside its window a timeout is not really unknown: sending
again with the same key returns the answer. Two providers break that:
Stripe v1 stores a 500 under its key and returns it again for 24 hours,
and Braintree's 915234 says the first attempt failed and the key cannot be
used again.

**How others handle it.** Temporal and Cloudflare retry steps by policy and
expect them to be idempotent; none resolves an unknown outcome without
that. Temporal lets an operator reset a workflow to an earlier point.

**Rule.**

- A keyed step in doubt inside its window is not `outcome_unknown`: after
  the call's own retries the run ends retry-later, and the next attempt
  sends it again under the same key.
- It parks as `outcome_unknown` only when the step has no key, when its
  window would pass, or when the provider replays failures and answered
  5xx.
- The package names a "first attempt failed, use a new key" answer
  (Braintree's), and only then does the runtime send again under the next
  key generation by itself.
- An operator has two actions on an unfinished record: retry (with
  `--not-done` to send the step in doubt under a new key, once they have
  checked it did not happen) and abandon (end the action as failed, keeping
  the list of effects for tidying up by hand).

```text
$ jadpo deliveries show d_7Q2
billing.subscribe, from subscriber start_subscription
Outcome unknown at step 3 of 4

  1  stripe.customers.create         answered     10:02:11  cus_P4x…
  2  stripe.payment_methods.attach   answered     10:02:12  pm_1N…
  3  stripe.subscriptions.create     no answer    10:02:12  Stripe answered 500 and repeats it for this key until 10:02 tomorrow
  4  stripe.invoices.finalize        not reached

Check in Stripe whether customer cus_P4x… has a subscription.
  It doesn't:  jadpo deliveries retry d_7Q2 --not-done
  It does:     jadpo deliveries abandon d_7Q2, then tidy up in Stripe
```

### 8. Undo steps that fail, and undoing after a deploy

**What goes wrong.** An undo is another changing call. If it fails for
good (already deleted, past the refund period), the action ends with
effects left behind. If it is unavailable the run retries: replay reaches
the failed step, whose failure must be recorded so the rerun takes the
same branch, and the undo is sent again under its own key. After a deploy
the undo may be different code; the steps before it are still checked, and
an undo not yet recorded simply runs as the new code says.

**How others handle it.** Only Cloudflare Workflows has compensation built
in: a `rollback` per step, run in reverse when the workflow fails; a failed
rollback ends the instance with `rollback.outcome = failed` (whether the
remaining rollbacks still run: to check). Restate treats sagas as a code
pattern whose compensations are retried. Nobody defines what happens when
a compensation fails for good.

**Rule.** No built-in compensation, so there is one way: code. A step's
failure answer is recorded like a success. An undo is a step like any
other: it needs a key, gets the same warning when it can fail for good, and
a failure mapped as "already undone" (Stripe's `resource_missing` on a
delete) counts as handled. A run that ends with an undo failed becomes an
operator item (must-fix 9) listing what took effect and what the undo
tried. The warning lists undo last of its three fixes, because it is the
weakest.

### 9. The final write fails for good after every call succeeded

**What goes wrong.** Area 11's question 26: a unique rule, a restricted
reference or a `reject` after the calls. The transaction rolls back, the
provider keeps four effects, and a rerun fails the same way.

Records make this recoverable. They hold every answer, so once the cause
is fixed (the data corrected, or code deployed) a retry replays all four
steps without contacting the provider, even after its window, and tries the
write again.

**How others handle it.** In Temporal and the rest the database write is
just another step, so this is the ordinary saga case.

**Rule.** One warning covers it and the step case: after the first changing
step, every point that can end the action for good (a later step's
failure, a write that can be refused, `reject`, a called action's failure)
is warned unless handled with `match`. The warning names three fixes:
check before the first step; record the answers and finish in a subscriber
of an event the write emits; undo as another step. The run ends `failed`
with its record kept (must-fix 9), so an operator can retry after a fix.
This answers question 26 for single-call actions too.

```text
warning: `Subscription.create` can be refused after Stripe has acted
  --> domains/billing/actions/subscribe.jadpo:9:12
   |
 2 |     var started = attempt stripe.start_subscription(...)   // 4 steps at Stripe
 9 |     return attempt Subscription.create(...)                // unique rule on account_id
   |
   = If it is refused, the four Stripe steps stay done and nothing here records them.
   = Check `account_id` has no subscription before calling Stripe, or
     emit an event with Stripe's answer and create the record in its subscriber, or
     undo the Stripe steps in a `match` on the failure.
```

### 10. Area 6's rerun after a concurrent change

**What goes wrong.** Today a changed row reruns the whole action with fresh
reads and a fresh time. With a record, replay is free (no provider
traffic), but a fresh time makes every rerun diverge, and a fresh read that
feeds a request does too. The window for a concurrent change now spans
every call, seconds rather than milliseconds, so these reruns happen more.

**Rule.** Replay with the record's time (must-fix 3) and the fingerprint
check. Only what was sent is fingerprinted, so another request changing an
unrelated field of the same row reruns cleanly. The budget stays three
attempts, then retry-later. A rerun that diverges is held as `diverged`.
The area 6 page should advise loading the records an action writes after
its last call where it can, which narrows the window to the commit; the
compiler cannot insist on it.

### 11. Workers limits on a long sequence

**What goes wrong.** Per step: one fetch (up to three with retries) and two
record writes; a four-step action can make 12 fetches and 8 writes before
it commits. Limits that bite:

| Limit | Value |
|---|---|
| Subrequests per invocation | 50 on the free plan; Cloudflare's Workflows page gives 10,000 by default on paid; the figure for plain Workers (to check); whether D1 or Hyperdrive queries count (to check) |
| CPU time | 10 ms free; 30 s by default on paid, up to 5 minutes |
| Work after the response (`waitUntil`) | about 30 seconds (to check) |
| Route deadline | 10 s by default, at most 60 s (area 7) |
| One run of a subscriber | its call's or route's time limit, at most 15 minutes (area 12) |

A run cut off by any of these is, to the runtime, a crash. Area 12's limit
row, "retried if nothing outside was called, otherwise `outcome_unknown`",
would park every multi-step run cut off, though the record makes it safe to
resume.

**Rule.**

- At most 8 changing steps per action, package steps included, counted on
  the longest path (possible because must-fix 7 keeps calls out of loops);
  confirmed by measurement like area 12's other limits.
- Area 11's deadline warning sums the timeout times the attempts of every
  step on the longest path, against the route's deadline.
- Area 12's limit row changes: a run cut off with a record resumes; only
  its step in doubt follows case 7.

On Workers, where a cut-off after the response is ordinary, this is the
mechanism's strongest argument: the once-a-minute sweep picks the run up
where it stopped.

### 12. Growth, cleanup and personal data

**What goes wrong.** One row per step accumulates if records outlive their
actions; answers can be large; and they hold the provider's copy of a
person (email, address, card's last four) and sometimes secrets (a
PaymentIntent's `client_secret`).

**How others handle it.** Size: Step Functions 256 KiB per payload,
Cloudflare Workflows 1 MiB per step result, Temporal 2 MB per payload and
50 MB per history, Inngest 4 MiB per step. Retention: Temporal Cloud 1 to
90 days, Cloudflare 3 or 30 days, Restate one day (to check). Personal
data: Temporal's payload codec encrypts inputs and results before the
server sees them; Step Functions uses a customer's KMS key; Inngest has an
encryption middleware.

**Rule.**

- **Where**: the store the action writes, beside its outbox (area 12's
  placement decision); an action writing no store needs
  `events: { store: }`, or it is a compile error.
- **When removed**: in the action's commit transaction, together with the
  delivery's handled marker or the route's stored response, so commit and
  removal cannot disagree.
- **Kept when unfinished**: failed, parked, diverged or abandoned records
  stay until the operator acts, at most 30 days, after which the answers
  are dropped and the operator item keeps the step list. A route's record
  whose client never came back becomes an operator item when the route's
  key lapses (24 hours).
- **Size**: at most 256 KB per answer; a larger one is a fault naming the
  step.
- **Personal data**: requests are kept only as fingerprints, never values.
  Answers are encrypted with a runtime key. `User.erase` cannot reach them,
  as nothing links them to a person, so the personal-data report lists
  "provider answers for unfinished actions, at most 30 days", as it does
  for logs.
- **Watching**: a metric and a no-setup alert for the number of unfinished
  records and the age of the oldest.

### 13. Testing, and what `jadpo dev` shows

**What goes wrong.** Area 13's fakes answer scripted calls in order and
fail a test that leaves answers unused. A replayed step consumes no
answer, so the fake must know about records. Nothing exercises a crash
between the provider's answer and the record write, which is exactly where
these bugs live.

**How others handle it.** Temporal has a replayer, run in CI against real
histories, and a time-skipping test environment. Cloudflare's test API has
`mockStepResult`, `mockStepError(times)` and `forceStepTimeout`. Inngest
mocks steps.

**Rule.**

- **Fakes keep keys** as the provider does: the same key and request return
  the first answer; a different request returns the package's mismatch
  answer.
- **Two more scripted answers**: `lost` (the provider acted and the answer
  never arrived) and `in_use`.
- **Crash points are tested automatically.** `jadpo test` reruns each test
  that reaches a multi-step action with a crash at every step boundary
  (before sending, after sending but before recording, after recording) and
  checks that the end state and the fake's effects match the run without a
  crash, reporting the boundary that broke. This is Jadpo's replayer:
  nobody has to remember to write it.
- **`jadpo dev` and `jadpo watch`** show each step as sent (with its key),
  answered, replayed from the record, or in doubt; the dev console lists
  unfinished records with retry and abandon, as in production.

```text
billing.subscribe   delivery d_7Q2, run 2 (resumed)
  1  stripe.customers.create         replayed from record
  2  stripe.payment_methods.attach   replayed from record
  3  stripe.subscriptions.create     sent jd1_4q7…   200   412 ms
  4  stripe.invoices.finalize        sent jd1_9xa…   200   188 ms
  committed Subscription 0192f…, record removed
```

### 14. Package actions inside actions

**What goes wrong.** The app sees `stripe.start_subscription` as one call,
but its four steps and its failures are inside. Without help the app's
warning cannot see that `CardDeclined` comes after two effects. A package
action called twice in one action makes eight steps. Failures are mapped
twice: the package maps `card_declined` to its own `CardDeclined`, and the
app maps that to `PaymentDeclined`.

**Rule.**

- **One sequence, one record**: a package action's steps are numbered in
  the calling action's sequence, so a second call's steps are 5 to 8 with
  their own keys (two customers, as the code says).
- **Failures** are recorded as the provider's answer and mapped again on
  replay, through both levels, by the current code.
- **The surface** lists, for each public action, its steps on each path
  and which of its failures can come after an effect ("`CardDeclined`
  after `customers.create` and `payment_methods.attach`"), and the app's
  warning at the call is computed from it.
- **Limits** count package steps, so a package action over the limit
  cannot be published.

### 15. Calls inside `for` and inside `match`

**What goes wrong.** In a loop, positions shift when a list read again has
a different length or order, and step 3 becomes a different item's call to
the same operation; with only an operation check the record's answer for
item B is handed to item C. In a `match`, step 3 in one branch and step 3
in the other share a position, and so a key.

**How others handle it.** Inngest adds a counter to a repeated step name
and asks authors to "keep the input list and its order stable"; Cloudflare
uses the name plus its occurrence count; Temporal uses position.

**Rule.** No outside call inside `for` in an action with several changing
calls (must-fix 7); area 12 already makes batch work one event per item.
Branches are fine: a step is recorded before it is sent, so a position's
key is only ever used by the operation recorded there, and a rerun that
takes the other branch stops as `diverged` before sending. Output names
each step by its operation as well as its number.

### 16. Schedules, and routes without a key

**What goes wrong.** A schedule's retry is the next tick, a new run with a
new key. A route without `Idempotency-Key` uses its request id, and a
client's retry is a new request with a new id. Either way a sequence that
stopped at step 3 starts again from step 1 under new keys: a second
customer, a second card attached, and the first record stranded. Today's
single call has the same flaw but repeats one call.

**Rule.** An action with several changing calls reachable from a schedule,
or from a route whose API does not require `Idempotency-Key`, is a compile
error naming the two fixes: require the key, or answer 202 and do the
steps in a subscriber.

### 17. Key scope across callers and apps

**What goes wrong.** Area 7 keeps a client's key under its caller
(`user <id>`, or `anonymous`), and area 11 sends "the client's
`Idempotency-Key`" to the provider. Stripe keeps keys per account across
endpoints. Two users who both send `Idempotency-Key: 1` to a route calling
`customers.create` would have the second given the first's customer as a
replay, or an `idempotency_error`. Lengths differ too: Square 45
characters on CreatePayment, Adyen 64, GoCardless 128, Stripe 255, while a
client's key may already be 255.

**Rule.** The key sent is `jd1_` and 32 base32 characters of a SHA-256 over
the application, environment, connection, caller scope, action key, step
number and key generation: 36 characters, fitting every limit found. A
package whose provider allows fewer gives `max_length:` and the hash is
cut to fit. Today's single call should take the same key.

## The revised proposal

1. **Several changing calls.** An action may make up to 8 changing calls
   in order, counted on its longest path with package actions' calls
   included. Each must take an idempotency key or be marked by its package
   as safe to repeat as it is. A changing call with neither stays the only
   changing call in its action. Compiler-checked.
2. **Where.** Only in actions run from subscribers, or from routes whose
   API requires `Idempotency-Key`; from schedules and other routes it is a
   compile error naming those two fixes.
3. **No calls in loops.** No outside call inside `for` in such an action.
4. **Keys.** Each step's key is derived from the application, environment,
   connection, caller scope, action key (the delivery key, or the client's
   key), step number and key generation, hashed to 36 characters or the
   package's `max_length:`.
5. **The package says how keys behave**: `window:`, `max_length:`, the
   provider's codes for "in use" and "different request", whether failed
   answers are replayed, and any "first attempt failed, use a new key"
   answer.
6. **The record.** Before a step is sent the runtime records its position,
   operation, request fingerprint (over the typed value) and send time;
   when the answer comes, success or failure, it records the answer as
   sent, encrypted, at most 256 KB. Provider reads after the first changing
   step are recorded the same way. Database reads are not.
7. **Replay.** A rerun replays answered steps in order without sending
   them, using the time stored in the record as `clock.now`. At each step
   the operation and fingerprint must match, and at the end every recorded
   step must have been replayed; otherwise the action stops as `diverged`
   before anything is sent, naming the step and the fields that differ.
   Held for an operator, not failed.
8. **A step in doubt** ends the run retry-later, and the next attempt
   sends it again under the same key while its window lasts; it parks as
   `outcome_unknown` when it has no key, when its window would pass, or
   when the provider replays a 5xx.
9. **One runner.** Route keys are claimed as today; delivery leases carry a
   generation every record write checks. A provider's "in use" answer is
   retried, never a failure.
10. **Transactions.** Database writes still come after the last call, with
    no transaction open during calls (area 6). Record writes are the
    runtime's own, each in a short transaction.
11. **The warning.** After the first changing step, every point that can
    end the action for good (a step's failure, a refusable write, `reject`,
    a called action's failure) is warned unless handled with `match`,
    naming three fixes: check first; record and finish in a subscriber;
    undo as another step. Package surfaces list which failures follow an
    effect, so the warning reaches through package actions. This also
    answers area 11's question 26.
12. **Ending.** The record is removed in the action's commit transaction. A
    record that ends any other way (failed, diverged, parked, or a route's
    key lapsing) becomes an operator item listing each step and its
    answer, with retry (`--not-done` for a new key) and abandon; it is kept
    at most 30 days, then only the step list remains.
13. **Where records live.** In the store the action writes, beside the
    outbox; with no such store, `events: { store: }` is required.
14. **Deploys.** Each record keeps a digest of its action's step shape;
    `jadpo deploy` lists unfinished records whose shape the build changes
    and asks before going ahead.
15. **Tests and tools.** Fakes honour keys and gain `lost` and `in_use`;
    `jadpo test` reruns multi-step tests with a crash at each step
    boundary; `jadpo dev`, `jadpo watch` and `jadpo deliveries show` list
    each step's state.
16. **Package actions.** Their steps are steps of the calling action, under
    one record; a second call continues the numbering.
17. **Amended elsewhere.** Area 6 (time fixed on reruns once a record
    exists), area 10 (the same), area 11 (key derivation, the package's key
    block, question 26), area 12 (one changing call per action replaced;
    the run-limit row; the operator's actions), area 13 (fakes and crash
    points), area 15 (the report entry).

## Open questions for Richard

1. **Should a route without a required `Idempotency-Key` be refused, or
   only warned?** It decides whether a web form can start a multi-step
   action directly. *Recommendation:* refused, naming both fixes. A warning
   would let duplicated Stripe customers through on every client retry,
   and the subscriber shape is already how area 12 does work that may
   outlast a request.
2. **Is 8 steps the right limit?** *Recommendation:* yes for now; the
   Stripe example needs 4, the free Workers plan's 50 subrequests allow
   about 10 with retries, and it is confirmed by measurement like area
   12's other limits.
3. **Should a call with no key be allowed as the last step, now that a
   record stops it repeating?** It would let "charge, then send the
   receipt through Postmark" be one action. *Recommendation:* no. A receipt
   belongs in a subscriber of the action's event, and an unkeyed step in
   doubt always needs an operator.
4. **Should timestamps written after a resumed action be from when it
   started?** That follows from fixing `clock.now` per record.
   *Recommendation:* yes; it is also when the provider acted, and the
   alternative makes every rerun whose request holds the time diverge.
5. **Should a diverged action be held or failed?** *Recommendation:* held,
   as Temporal and Restate do, so deploying a fix and retrying finishes it;
   failing it would leave its provider effects with nothing to resume
   them.
6. **Should crash-point test runs be on by default?** They multiply a
   multi-step test's runs by up to three per step. *Recommendation:* yes;
   the interpreter makes them cheap, and the crash between answer and
   record is the bug nobody writes a test for.
7. **Should `jadpo deploy` stop for confirmation when unfinished records'
   steps change, or only report it?** *Recommendation:* stop and ask
   interactively, list them and carry on with `--yes` in CI; the cost of
   missing it is held work for an operator, not lost data.
8. **Should question 26 close with the warning in rule 11?**
   *Recommendation:* yes; it is the same warning, and kept records make the
   single-call case recoverable too.

## Sources

Durable execution:

- Temporal: <https://docs.temporal.io/worker-versioning>,
  <https://docs.temporal.io/develop/typescript/versioning>,
  <https://docs.temporal.io/workflow-definition>,
  <https://docs.temporal.io/activity-definition>,
  <https://docs.temporal.io/workflow-execution/limits>,
  <https://docs.temporal.io/develop/typescript/testing-suite>,
  <https://docs.temporal.io/production-deployment/data-encryption>
- AWS Step Functions:
  <https://docs.aws.amazon.com/step-functions/latest/dg/concepts-state-machine-version.html>,
  <https://docs.aws.amazon.com/step-functions/latest/dg/service-quotas.html>,
  <https://docs.aws.amazon.com/step-functions/latest/dg/encryption-at-rest.html>
- Azure Durable Functions:
  <https://learn.microsoft.com/en-us/azure/azure-functions/durable/durable-functions-versioning>,
  <https://learn.microsoft.com/en-us/azure/azure-functions/durable/durable-functions-code-constraints>
- Inngest: <https://www.inngest.com/docs/learn/versioning>,
  <https://www.inngest.com/docs/guides/working-with-loops>,
  <https://www.inngest.com/docs/usage-limits/inngest>,
  <https://www.inngest.com/docs/guides/handling-idempotency>
- Restate: <https://docs.restate.dev/operate/versioning>,
  <https://docs.restate.dev/guides/sagas>,
  <https://docs.restate.dev/develop/ts/journaling-results>
- Cloudflare Workflows:
  <https://developers.cloudflare.com/workflows/reference/limits/>,
  <https://developers.cloudflare.com/workflows/build/rules-of-workflows/>,
  <https://developers.cloudflare.com/workers/testing/vitest-integration/test-apis/>
- DBOS: <https://docs.dbos.dev/python/tutorials/workflow-tutorial>,
  <https://docs.dbos.dev/python/tutorials/upgrading-workflows>

Provider keys:

- Stripe v1: <https://docs.stripe.com/api/idempotent_requests>,
  <https://docs.stripe.com/error-low-level>; Stripe v2:
  <https://docs.stripe.com/api-v2-overview#idempotency>
- Adyen: <https://docs.adyen.com/development-resources/api-idempotency>
- PayPal: <https://developer.paypal.com/api/rest/reference/idempotency/>
- Square:
  <https://developer.squareup.com/docs/build-basics/common-api-patterns/idempotency>
- Shopify:
  <https://shopify.dev/docs/apps/build/apis/graphql-admin/implementing-idempotency>
- GitHub:
  <https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api>
- Checkout.com: <https://www.checkout.com/docs/developer-resources/api/idempotency>
- GoCardless: <https://docs.gocardless.com/docs/api-reference/limits>
- Braintree:
  <https://developer.paypal.com/braintree/graphql/integration_guides/idempotency/>
- Postmark: <https://postmarkapp.com/support/article/what-is-an-idempotency-key>
- IETF draft:
  <https://www.ietf.org/archive/id/draft-ietf-httpapi-idempotency-key-header-07.html>
