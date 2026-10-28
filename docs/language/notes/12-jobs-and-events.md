---
title: Jobs and events, design notes
area: 12
---

# Jobs and events: design notes

Why [Jobs and events](../12-jobs-and-events.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

What this area sets in other areas:

| Area | Rule |
|---|---|
| 2 | `BatchRun`, `BatchProgress`, `RowProblem`, `ImportSummary` and `ExportSummary` are area 2's types |
| 3 | The batch helpers' `each:` and `finished:` take the inline `->` form; no action makes an outside call inside `for` |
| 5 | An entity's `purge:` runs as a built-in hourly schedule per entity |
| 6, 10 | Writes come after the last call; `clock.now` is fixed for a run, and for a multi-step record's life, reruns included |
| 9 | Schedules, subscribers and batches act as `Access.system`, which an entity grants explicitly, each limited to the tenants "Background work and tenants" says |
| 11 | The delivery key is an input to each provider call's idempotency key; work over many pages of a provider's list is `batches.each`; area 11's question 26 is answered by the late-failure warning |
| 13 | Tests run a schedule with `schedule.run()` and deliver events after each commit in a shuffled order; fakes answer `lost` and `in_use`; crash points at each step |
| 14 | The operator's commands: `jadpo deliveries` (with `diverged` and unfinished multi-step runs, batch runs and parked queue messages), `jadpo schedules`, and `jadpo runs unfinished --build <build id>` |
| 15 | Multi-step records and batch runs' captured values are in the personal-data report |
| 17 | Each step's state shows in `jadpo dev`, `jadpo watch` and `jadpo trace` |

## Decisions

Numbered as the questions they settled, which other pages cite.

1. **A schedule is `schedule name { every: 15m  run: action() }`.** The
   action takes no arguments and reads `clock.now`, fixed for the run. One
   run happens at a time, missed ticks merge, and the next due time is the
   retry. Runs never overlap: a tick that arrives while a run is going is
   skipped, not queued, and there is no option to allow overlap; work too
   large for one run emits one event per item. Calendar times, as Rails'
   Solid Queue and Oban's cron plugin offer, are added only when an
   application needs them.
2. **There is no delivery descriptor.** Work over a set of items is an
   ordinary query, action and subscriber ("Batch work keeps its
   progress"), not a closed syntax that restates a query.
3. **An event is `type TodoCompleted = Event { ... }`**, one type per file
   in `events/`, with no families and no per-event policy. It matches
   failures, and Laravel and Rails define one class per event or job too.
4. **`emit TodoCompleted { ... }` is a statement** with no `attempt`, only
   in actions, delivered after commit. It mirrors `reject`, and areas 3
   and 4 rule out `attempt` on something that can't fail.
5. **A subscriber is `subscriber name { on: Event  run: action(event) }`**,
   one event each, the same shape as routes and schedules: several events
   in one subscriber would read as "all of them" to some readers and "any
   of them" to others.
6. **No delivery order is promised.** Order per key needs per-key queues
   and lets one stuck item block the rest; Sidekiq, Laravel and EventBridge
   promise none either. Subscribers read current state, which retries need
   anyway.
7. **Retries go by failure kind**, as the table under "Retries and giving
   up" says: by default 10 attempts within 24 hours, changeable per
   subscriber with `retry: { attempts:  within: }`, with exponential
   backoff and full jitter. A day suits an outage, and a payment webhook
   can ask for more (Sidekiq defaults to 25 retries over about 20 days,
   Oban to 20 attempts).
8. **Schedules and subscribers act as `Access.system`**, a fourth kind of
   "who" (area 9) that an entity grants explicitly (`update: [owner_id,
   Access.system]`), never as the user who caused the event. A subscriber
   is limited to the tenant its event came from, and the permissions
   report shows what background work can touch. The event carries the
   user's id when the reaction needs it.
9. **Lifecycle transitions don't emit events by themselves**: the action
   that makes the change emits, one way to do it.
10. **There is no delayed delivery** (`in 24h`): a schedule querying a due
    field covers it, rereads current state, and keeps one way to wait.
11. **An action repeats as a unit.** A changing call that takes no
    idempotency key is the only changing call in its action; keyed calls
    may be up to 8 in order, each recorded and replayed on a rerun
    (question 20). The runtime stops at the first call whose outcome is
    unknown, so the rule isn't there to know which call to ask about: it
    is there so that a rerun repeats each call under its key, and no
    second call fails for good after the first took effect without the
    warning ("Several changing calls in one action").
12. **The limits** under "Limits and runtime faults" are defaults, to
    confirm with measured runs: 1,000 events per action, 64 KB per event,
    chains 16 deep, which stops only loops since paging uses batches, 15
    minutes per run, batches of up to 1,000 items or 4 MiB of file, and at
    most 8 changing calls per action.
13. **An export reads live data**, a batch walking keyset pages, not one
    consistent snapshot; a `from:` ordered by a field that can be updated
    is the declinable error `type.batch_order_updatable`, since every
    finding is an error (area 1, question 11). Area 15's export of a
    person's data reads in one transaction.
14. **Work after a route answers `202`** (`status: SuccessStatus.accepted`)
    carries on in an event and a subscriber, and the client reads a record
    with a status field; a route that wants the result waits with
    `settle:` (area 7).
15. **Tests run schedules and subscribers** as area 13 says:
    `schedule.run()` runs a schedule under the test clock; events are
    delivered after each commit; a fake can fail a subscriber to test
    retries; delivery order is shuffled with a printed seed, so a test
    that depends on order fails visibly.
16. **The tick is split from the state.** Each schedule's next due time and
    lease are a row in `schedules: { store: }`, or the only database, and a
    tick takes a due schedule whose lease is free in one conditional
    update, so a duplicate tick is harmless: Cloudflare's cron triggers can
    fire more than once and don't wait for a running invocation. What
    ticks is `trigger:`: `ScheduleTrigger.native`, the server's own timer,
    by default, or `ScheduleTrigger.external`, the native deployable's
    `tick` command, which reads the settings, checks migrations as `serve`
    does, runs one tick and exits, so a Kubernetes CronJob, an ECS task or a
    systemd timer runs it each minute. There is no HTTP endpoint and no
    tick secret; a scheduler that can only call a URL needs its own
    wrapper. On Workers the cron trigger calls the Worker's scheduled
    handler. One entry ticks every schedule, so `every: 7m` works where
    cron syntax can't and Cloudflare's per-account cron limit is never
    reached. A Durable Objects adapter that also holds the lease is a
    possible later optimisation. A schedule or an event with no store to
    keep its state in is a compile error saying which store to add, and the
    system tables (schedule state, the outbox, delivery records, idempotency
    keys, sessions) are in the migration snapshots like any entity,
    prefixed `jadpo_`, so `jadpo migrate new` makes and changes them and
    `jadpo check` reports drift in them.
17. **Tracing through events is always on.** An event is stored with the
    W3C `traceparent` of the action that emitted it, in the same
    transaction, and each subscriber run is a child of it, so route,
    action, events, subscribers, further events and provider calls form
    one trace. Schedules start a trace per run; provider webhooks start one
    linked to the provider's event id; calls between Jadpo applications
    pass the context on. Each event records what caused it (a request id or
    a parent event), which the 16-deep chain limit needs. `jadpo trace
    <request id>` and the console (area 17) show the tree, and area 16's
    OpenTelemetry export carries it.
18. **Deliveries pause when a provider keeps failing**, through a circuit
    breaker per connection ("When a provider keeps failing"), built in so
    nobody has to remember it, as Resilience4j and Polly offer it.
19. **Work over many items is a batch**, the one way: explicit helpers per
    format and direction, and `batches.each` for work with no file
    ("Batches: work over many items").
20. **An action may make several changing calls**, as "Several changing
    calls in one action" says. A route without a required
    `Idempotency-Key` starting them is refused, not warned; 8 steps is the
    limit; an unkeyed call is never allowed as a last step; timestamps
    after a resumed action are from when it started; a diverged action is
    held, not failed; crash-point test runs are on by default. Jadpo has
    no deploy command, so before a deploy `jadpo runs unfinished --build
    <build id>` lists the unfinished runs whose path the new build
    changes, and a run resumed under a changed build stops as `diverged`.
21. **The app reads a queue directly** through the `receiving` capability
    ("Events from outside").
22. **Packages produce events** only about the outside world: webhooks,
    events their own subscribers emit from them, and polls ("Events from
    outside").

Carried from other areas, and the choices the topic sections leave open:

| Topic | Decision |
|---|---|
| Triggers | From area 1: routes, schedules and subscribers each call actions; durable workflows are subscribers |
| Keyword and folders | From area 1: `schedule` in `schedules/`; events in `events/`; subscribers in `subscribers/` |
| Cross-domain changes | From area 1: public actions inside the caller's transaction, or events for reactions that need not be all-or-nothing |
| Actions | From area 3: every action is all-or-nothing and has no settings; `attempt` only on calls that can fail |
| Faults | From area 4: faults are never listed in `fails` or matched; provider problems are mapped to declared failures at the call |
| Outbox placement | Each store an action writes keeps its outbox, its delivery records and its idempotency keys; an action writing no store needs `events: { store: <connection> }`, or it is a compile error. A subscriber's handled record goes in the store it writes, with no reference across stores. Schedule leases go in `schedules: { store: }` (area 5), the only store when there is one |
| Schedule and batch defaults | A schedule's first run is due one `every:` after the first tick that sees it; a failed run is retried at its next due time; schedule state defaults to the only database; batch runs are kept in the store the `from:` query reads; several saved values naming the same tenant count as that tenant; a route whose action starts a batch answers `202` |
| Multi-step calls' encryption key | Recorded answers are encrypted with `application { steps: { encryption_key: config.step_encryption_key } }`, a `SigningKey` setting rotated as grants' `encryption_key:` is; required only when some action makes several changing calls, a compile error naming the first such action otherwise |
| Several changing calls, checked | A changing call is a provider's operation with `changes: true` or a store's `put`; reads are never counted. Each way through an `if` or a `match` counts on its own, a `return`, `reject` or `propagate` ending its way, an action called counting the changing calls of its longest way, and a loop's body once, since a call in it is refused. A changing call with no key may share its action with no other changing call on any way through it, another way included, since a rerun after a fresh read can take another way (`step.unkeyed`). `step.context` is reported where the action is run: a schedule, a route whose API doesn't require `Idempotency-Key`, and a run's `each:` or `finished:`, whose items stay one changing call each; tests and fixtures may call such an action. `large_output: true` is refused on any operation, one that reads included, in an action whose longest way makes several changing calls. The record's store is checked for an action a subscriber, or a route whose API requires the key, runs: the record belongs to the action that is run, and a called action's calls join it. A recorded answer's cap is 1 MB |
| The record | A record is kept only where the action's key survives a retry: a subscriber's delivery, and a route whose API requires `Idempotency-Key` under the key it claimed; a test calling the action directly keeps none. It is one row keyed by a digest of the key's scope and the key, with a row per step, in the store the action writes or `events: { store: }`'s. A request's fingerprint is an HMAC-SHA-256 of each top-level field as the provider is sent it, by its name there, under a key derived from the steps key (checked under the current and the previous one), so a low-entropy field can't be guessed from it; an answer is sealed with XChaCha20-Poly1305 under another key derived from it (`crypto.encrypt`'s construction; grants' encryption isn't built yet), bound to the record and the step. The generation every write checks is a delivery's attempt or a route's claim time; a write takes the record over from an equal or older one only. A rerun is checked step by step (its operation and fields) and at the end (every recorded step replayed), before anything more is sent; the record keeps the build it started under (until area 18's build id, a digest of every graph node's key and digest) and a digest of its step shape for `jadpo runs unfinished`, and a rerun doesn't compare the digest, since steps added after the last recorded one aren't a change. A step answered "did nothing" (`unavailable`, `misconfigured`) stays in doubt, so the window rule still holds it. A `put` keeps its object under a key derived from the record and its step and goes again whatever the window. `diverged` is a runtime fault that holds a delivery in the state `diverged` with the step, its operation and the fields; a route answers it `500` and keeps no response, so a client's retry after a fix finishes it. `new_key_on:` sends the step again under the next key generation ("Operators") |
| Operators | A route's record becomes an item once a day has passed since its last run took it (area 11's 24 hours), whatever its own `window:`; before that it is its client's to retry, and an operator can only abandon it, since only its client holds the request. "Not done" acts on the record's last step when it has no answer or failed, clearing its answer and moving its key to the next generation, with its window counted from the operator's action; with nothing in doubt it is a plain retry, and on a delivery with no record it is `retry` (failed) or `resolve --not-done` (in doubt). A retry resets the record's generation to none with the delivery's attempts. A delivery with a record is refused `dismiss` and `resolve --done`, pointing at `abandon`; `abandon` on one without is refused. A record whose answers were dropped can't be retried, only abandoned. Answers are dropped 30 days after the record's last run; an abandoned record and its delivery go 30 days after the abandon. A route's client retrying under an abandoned record's key stops as a fault, answered `500`, sending nothing. `new_key_on:` sends again once per call; a second such answer is the operation's failure. `show` never shows a key: the CLI reads no setting it would be derived from. `jadpo runs unfinished` requires `--build`, refuses a build the checkout doesn't make (naming the one it does), lists a record whose build differs and whose action's step shape the build changes or no longer keeps, and a running batch run whose call the build changes or no longer has; it succeeds whatever it lists, as listing commands do |
| Tests | A package's fake keeps a keyed call's first answer under its key, as its provider does: the same key and request are given it again without taking a scripted answer, and a different request the package's `reused:` answer (with none, a refusal without a code, a fault). A failure is kept as Stripe keeps one, whatever the package's `replays_failures:`, unless its kind is `unavailable` or `rate_limited`, so a step sent again after a crash is answered as it was the first time. `lost` acted: what it did, the generated default, answers the next call under its key. `in_use` is another runner's request under the key, the one that acts, so it keeps nothing and answers as `unavailable`; `unavailable` and `misconfigured` keep nothing either. `jadpo dev`'s fake keeps keys the same way, its last 10,000. Crash points count every step a test's run reaches under a record, a step sent again counted again, across the whole run; only a run that passed is crashed, once for each boundary of each step, each on its own copy of the database with a fresh fake, and the first crash that fails is the one reported, with a `crash` line first. A crash isn't counted as an attempt: the delivery is given back, due at once, and a route's claim is given up and its request sent again under its key. A crashed run whose fake acted more or less often than the run without one is a defect in Jadpo, reported as `test.fault`. A delivery that stops as `diverged` between a test's steps ends the test as `test.subscriber_failed`, since it never finishes; one parked as `outcome_unknown` doesn't. Each delivery attempt in a test counts its keyed calls from none under the delivery's own key, as a runner's fresh context does, and gives the test's back after. An `ai.generate` call kept as a step gets crash points like any other step; the `ai` fake keeps no keys, area 21 asking none of it, so a step sent again after a crash once it was answered takes the prompt's next scripted answer, as a model asked again answers afresh. Crash points have no switch to turn them off |
| The late-failure warning and the deadline sum | A failure ends a run for good unless its kind is `unavailable` or `rate_limited`, the two retried ("Retries and giving up"); one whose kind isn't known counts as ending it. A point after the first changing call is an `attempt` passing a failure on (the failures its arms map to, or else the call's or write's own), `reject` and `propagate`; a failure handled by an outcome `match` isn't one. Where two ways meet, a changing call on either may have been made. One finding for each action, at its first point, the others shown beside it, so one `decline` answers it, as declines answer one finding each. A package's action is never told, since the app decides what can be left behind: its surface lists each failure that can follow its own changing call as `after_effect`, and an app's `attempt` of it is a point even before the app's own first changing call; an app's action called before any is told itself instead. The deadline sum is one analysis over every outside call a route reaches, a provider's operation and an `ai` call alike, so streaming routes rely on it: a provider call through a connection costs its `timeout:` (3s by default, area 11's question 8) times its `retry: { attempts: }` (3), a package's action its own calls through the connection the app called it through, and `ai.generate` its `timeout:` times its `attempts:`; the waits between attempts and `within:` aren't counted, and a store's `put` and `read` have no `timeout:` and cost nothing. The ways are summed and the longest compared with the route's `deadline:` (10s by default): more is `route.calls_past_deadline` at the route, a plain mistake, not declinable (area 1, question 11), whatever its API; equal fits. Schedules and subscribers have no deadline to compare |
| Development and the metric | Until area 17's request timelines (`jadpo trace`), a request's timeline in `jadpo dev` is its line and the trace lines under it, so each step a request's action reached is a trace line there, in place of the call it made; a delivery, which `jadpo dev` doesn't otherwise print, gets a line of its own only when its subscriber's action reached steps under a record, since that is when one needs following, with `jadpo deliveries show <id>` under it when the attempt failed, parked or diverged. A step's state is what this run made of it, from the record: `replayed` (answered on an earlier run), `answered`, its failure's code, `in doubt` (sent, no answer kept), `diverged` or `outcome unknown` (in doubt and not to be sent again), with where its answer came from; never its request, answer or key, nor the key's generation. The metric counts the records not abandoned, a run under way included, and gives when the oldest's first run started; the tick carries it for each store that keeps records, until telemetry (area 16) sends it. The alert holds once a record has been unfinished a day from its first run's start, a subscriber's default `within:` and a route's key's lapse, so no retry with the default budget is still to finish it; each such record is marked in `jadpo_step_records.alerted_at`, and the tick raises the alert only when none still unfinished was marked before, so it is raised once until no record is unfinished past a day, by one instance's tick. `jadpo deliveries` shows it in its default list and with `--state diverged` or `unfinished` |
| Keys and replays | A route's action keeps its client's `Idempotency-Key` in the store it writes, else beside its events, else, when it makes a changing call (a provider's operation with `changes: true`, a store's `put`), in the store `events: { store: }` names, where its record of several steps is kept too, so its steps are resumed under the key and one call's response is kept for a repeat; an action that only reads keeps none, and its OpenAPI says where the key is kept. With no store to keep it, the route's calls' keys are still derived from the client's key in its caller's scope, never the request's id. A delivery's attempt counts its keyed calls from none under its own key and with none of the `ai` answers kept for a rerun, wherever it runs (a route waiting with `settle:`, a test), giving back whoever ran it theirs after. A sign-in email (`MagicLinkEmail`, and the action `send:` names) is keyed per attempt, from fresh randomness, since each attempt makes a link and a code that end the last's: a provider replaying the last attempt's key would leave the person a dead code; its key doesn't survive a retry, so it keeps no record of its steps. A step's own failure whose kind is `unavailable` or `rate_limited` is never kept, and one kept by an earlier runtime never replayed: the step stays in doubt and goes again under its key. Code's `clock.now` is the record's fixed time; an event's `emitted_at`, a delivery's handled record, a request's AWS signature, the day an `ai` budget counts and a received event's stamp are the host's. An unkeyed change that went out (answered, or no answer) stops a rerun after a concurrent change, in the boundary or at a held commit, as `outcome_unknown`, which a route keeps for its key and a delivery parks; areas 6, 12 and 16 say the same. In `jadpo test` how often each scripted operation and prompt was called is judged on the run without a crash only, since a crash sends its step again and the `ai` fake, or a call with no key, answers it afresh; a request a crash sends again forgets the `ai` answers kept for a rerun, as a client's retry reaches a runner that kept none, so a model asked before the steps is asked again there as in production |
| One outside-call path | Every outside call (a provider's operation, `ai.generate`, `ai.embed`, `ai.stream`, a store's file, an email, a capability's delivery) goes out through one exchange and one retrying send, and where its answer is given back from (the action's record, an `ai` call's earlier run, a replayed run's recording) is decided in one place. `max_concurrent:` caps every call through its connection: a provider operation's, and where a package fills `ai`, its `ai.generate`, `ai.embed` and `ai.stream` calls too; the built-in `Driver.openai_compatible` has no cap. A call takes a slot only once it is cleared to go (no transaction open, no recording answering it, a driver found), before its step is written, and gives it back once answered, a stream once its answer starts to come, so a later call of the same action through the connection never waits on a stream the route hasn't sent yet. A delivery whose action calls `ai` through a connection with every slot out waits for one within its budget, as a route's call does, rather than being given back to the outbox: giving back reads the connections the circuit breaker does, a provider's operations'. No wait to send a call again outlasts the invocation's time, a route's remaining deadline or what background work's budget leaves before the time it keeps back: a wait that wouldn't end within it, a `Retry-After` past it included, ends the sending at once and the last answer stands, so a `429` or a provider that couldn't be reached is `unavailable`, and no answer in time `timed_out` where the call maps it, as when `within:` or an `ai` call's `timeout:` ends it. Each send's own timeout isn't shortened: the deadline and the budget stop a call still out. A store's file, an email and a capability's delivery are sent once, having no `retry:` (the outbox retries a delivery). An embedding given back from the action's record is logged as the step it is, as the other calls are |
| Work handed on, on Workers | `Host::hand_on` on Workers is a request to the Worker itself, through the loopback binding `ctx.exports` gives the Worker's own `JadpoHandOn` entrypoint of its shim, never its default export, so nothing from outside reaches it. It isn't a service binding naming the Worker, since Wrangler keeps a binding's `service` when `wrangler deploy --name` or `--env` deploys the file under another name, which would hand a staging Worker's work to production's; workerd gives `ctx.exports` at the compatibility date without a flag. It is sent JSON naming each piece by its record and store, never an event's value; the fresh invocation reads each delivery back from its record, leaving out one run or given up since, and each batch run from its row, and carries them on in its own `waitUntil`, handing on in turn, within its 30 seconds counted from its start, so starting the isolate and reading the work back are spent from them; the invocation that handed it on is kept until that call returns. A hand-on that fails, as past the platform's limit on invocations one request chains, leaves the work in the store for the minute's cron. It isn't a queue, which needs a queue made in the account and a consumer for every Worker and delays each hand-on by its batching, nor a request to the Worker's own URL, which would reach its public `fetch` and need a secret to tell itself from a caller |
| The tick on Workers | A Workers deployable reaches a schedule when its `runs:` lists `Trigger.schedules`, and one more often than once a minute is `deployable.target_unsupported` (area 14), at its `every:`, rather than a rule of its own, since it is one more thing a target can't run; the Wrangler file names the cron `* * * * *` for every Workers deployable whose tick has a store to keep or sweep, as every native deployable ticks; each run of the cron ticks once its stores are current (out of date, it logs why as a request does and ticks nothing), then, while the app ticks more often than once a minute (a received queue, every 10 seconds, as natively), again at each such mark of the cron's minute, one tick after the last ends, never two at once, so a Worker holds no more connections than one tick does, and a mark a long tick passed is not made up; the invocation ends with its minute, and each of its ticks has what is left of the cron's 15 minutes |
| The `trigger:` adapter | `trigger:` (question 16) takes `ScheduleTrigger.native`, the default, or `ScheduleTrigger.external`, and changes only a native deployable, a Worker ticking on its cron whichever is written, so an app with a server and a Worker can write either; `<deployable> tick` (and `serve`, the same as no command; anything else ends with code 2, `command_unknown`) doesn't wait for a database it can't reach, ending with code 1 (`store_unreachable`), since the scheduler's next run is the next try and the ticks missed become one run; it logs `runtime.ticked` (each run by its schedule's name, deliveries, what it received and polled, rows deleted, alerts, each store it couldn't work in) and ends with code 1 only when a store couldn't be worked in, a failed run being recorded, alerted and retried at its next due time as a server's is; the first SIGTERM drains it as a server drains its background work; `tick` runs whatever is written, the lease deciding; nothing refuses an `every:` under a minute with `external`, since a scheduler can run `tick` each second: `jadpo build` says how often to run it, and the ticks-stopped alert says when it runs less often |
| Queue messages the store refuses | A message whose event the store refuses as a statement, as it would every time it was sent (a value it can't take, such as a text holding `\u0000`, which `jsonb` can't hold), is parked `unreadable` and acknowledged, so the queue doesn't send it again until its retention deletes it. A store that can't be reached, a conflict, a concurrent change or a fault the driver can't classify still leaves it on the queue, to be kept when it is sent again |
| A delivery's answer closes its breaker | A delivery attempt whose call through a connection its provider answered closes the connection's breaker and releases what it holds, whatever the breaker's state when the delivery was claimed, as "When a provider keeps failing" says of a delivery already running when the breaker opened. It is one conditional statement after each attempt that called the provider, changing nothing while the breaker is closed with no failures in a run |
