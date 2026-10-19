# 23 · A repair café

A repair café sets out benches on a Saturday morning, one for each kind
of thing its volunteers mend. A visitor books a repair at the kiosk by
the door; the café then finds the right bench and prints a ticket to
take to it. A visitor can leave the fixer a thank-you note, which is
pinned to the board. It teaches events: announcing that something
happened, the subscribers that react to it, what happens when they
fail, and a route that waits for them to finish.

Booking a repair writes the repair and nothing else. The work that
follows, finding a bench and printing the ticket, happens in
subscribers, after the booking has committed, each retried on its own:
a bench with nobody at it yet is worth trying again later, a microwave
never is. The kiosk still wants the ticket in its answer, so the route
waits for that follow-on work for up to three seconds, and says 202 when
it isn't finished.

## What it teaches

1. **Declaring an event.** A type with the `Event` base, named in the past tense, one per file in `events/`, carrying ids and the facts of the moment. See `domains/cafe/events/repair_booked.jadpo` and [area 12, Declaring an event](../../docs/language/12-jobs-and-events.md#declaring-an-event).
2. **Emitting.** `emit` keeps the event in the outbox in the same transaction as the repair, so a booking that fails announces nothing, and the event is delivered only once it commits. See `domains/cafe/actions/book_repair.jadpo` and [area 12, Emitting an event](../../docs/language/12-jobs-and-events.md#emitting-an-event).
3. **Subscribers.** `on:` names the event, `run:` calls an action of the domain with the event's fields. A subscriber runs as the system, so the entities it changes let `Access.system` in. See `domains/cafe/subscribers/find_a_bench.jadpo`, `domains/cafe/entities/repair.jadpo` and [area 12, Subscribers](../../docs/language/12-jobs-and-events.md#subscribers).
4. **Retries.** An `unavailable` failure is tried again later, any other kind fails the delivery for good, and `retry:` sets how many times and for how long. See `domains/cafe/failures/no_fixer_free.jadpo`, `domains/cafe/actions/send_to_bench.jadpo` and [area 12, Retries and giving up](../../docs/language/12-jobs-and-events.md#retries-and-giving-up).
5. **Events that cause events.** `send_to_bench` emits `BenchAssigned`, and another subscriber prints the ticket. See `domains/cafe/subscribers/print_on_assignment.jadpo`.
6. **Waiting for follow-on work.** `settle:` delivers what the booking emitted, and what that sets off, while the request waits, then answers with `answer:`'s query: 201 once it has all settled, 202 if a delivery is waiting to retry or `within:` passes. See `domains/cafe/routes/repairs.jadpo` and [area 7, Waiting for follow-on work](../../docs/language/07-http.md#waiting-for-follow-on-work).
7. **Answering at once.** `status: SuccessStatus.accepted` answers 202 and leaves the pinning to happen after the response. See `domains/cafe/routes/repairs.jadpo`.
8. **An action that only emits.** `leave_thanks` writes nothing, so `events: { store: }` names the outbox its event is kept in, and root `policy.jadpo` lists who may run it, since it reaches no entity. See `app.jadpo`, `domains/cafe/actions/leave_thanks.jadpo`, `policy.jadpo` and [area 12, `event.store_required`](../../docs/language/12-jobs-and-events.md#eventstore_required).
9. **The event record.** `jadpo records update` writes `domains/cafe/events/events.jadpo_record`, what each event carries, so a change to an event shows as a diff of one file, and an event still waiting in the outbox is read with the shape it was written in. See [area 12, Chosen in chunk 6.1c](../../docs/language/12-jobs-and-events.md#chosen-in-chunk-61c).
10. **Route tests see the follow-on work.** A route test sends its request through the real boundary, `settle:` included, so it sees the ticket the kiosk would. See `domains/cafe/routes/repairs.test.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/23-repair-cafe
target/release/jadpo test examples/23-repair-cafe
target/release/jadpo fmt --check examples/23-repair-cafe
cd examples/23-repair-cafe && ../../target/release/jadpo records
```

- `check` ends with `checked: <n> files` and no errors.
- `test` needs PostgreSQL, and ends with `tested: 4 passed, 4 route tests`.
- `records` lists the API record and the event record, each up to date.

## Try next

- In `find_a_bench.jadpo`, ask for `attempts: 200`. `jadpo check` reports `subscriber.retry_invalid`: a delivery that keeps failing that often is a fault to fix, not to retry.
  <!-- try {"file": "domains/cafe/subscribers/find_a_bench.jadpo", "replace": "attempts: 6", "with": "attempts: 200", "check": ["subscriber.retry_invalid"]} -->
- In `no_fixer_free.jadpo`, make the failure `FailureKind.rejected`. The toaster's route test fails with `test.assertion_failed`: a delivery that fails for good has settled, so the kiosk is told 201, not 202.
  <!-- try {"file": "domains/cafe/failures/no_fixer_free.jadpo", "replace": "kind: FailureKind.unavailable", "with": "kind: FailureKind.rejected", "test": ["test.assertion_failed"], "failed": 1} -->
- In `app.jadpo`, delete `events:`. `jadpo check` reports `event.store_required`: `leave_thanks` writes no store, so its event has no outbox.
  <!-- try {"file": "app.jadpo", "replace": "application {\n    events: {\n        store: main\n    }\n}", "with": "application {}", "check": ["event.store_required"]} -->
- In `repair.jadpo`, let `Access.public` change a repair instead of `Access.system`. `jadpo check` reports `policy.system_grant_missing`: a subscriber is nobody's request, so a grant to anyone at all never lets it in.
  <!-- try {"file": "domains/cafe/entities/repair.jadpo", "replace": "update: [Access.system]", "with": "update: [Access.public]", "check": ["policy.system_grant_missing"]} -->
- In `repair_booked.jadpo`, add `fault: FaultNote` to the event. `jadpo check` reports `type.missing_field` at the `emit`, and `record.out_of_date`: the event record no longer says what the event carries.
  <!-- try {"file": "domains/cafe/events/repair_booked.jadpo", "replace": "    item: ItemKind\n}", "with": "    item: ItemKind\n    fault: FaultNote\n}", "also": [{"replace": "use cafe/types { ItemKind }", "with": "use cafe/types { FaultNote, ItemKind }"}], "check": ["record.out_of_date", "type.missing_field"]} -->
- Then read [24-seed-library](../24-seed-library/README.md), where work over many items runs as a batch.
