---
title: Jobs and events
area: 12
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - event.*
  - schedule.*
  - subscriber.*
  - type.batch_*
---

# Jobs and events

Work that does not happen inside a request: schedules that run an action on a
timer, events an action announces, subscribers that react to them, and what
happens when that work fails.

> Decided on 2026-10-13. Code on this page applies the decisions from areas 1 to 12. Built in chunk 6.1a: the runner that runs background work, deferred after a response, inline or per item, as the system caller `Access.system` grants (area 9). Built in chunk 6.1b: events kept in the outbox with the action's writes, subscribers delivered to after the response as the system limited to the event's tenant, handled records, retries, trace context and the outbox sweep. Built in chunk 6.1c: each domain's event record, and what a subscriber stores from its event checked against what every `emit` of it put there. Built in chunk 6.2: schedules, their leases and state, the minute tick and the runtime's sweeps on it, and every run recorded. The commands that watch schedules come in chunk 6.2b. Built in chunk 6.4: `batches.each`, `batches.status`, `batches.cancel` and `key:`, a run walking a named query a page at a time and resuming after a crash or a deploy; the file helpers come in chunk 6.4b. Built in chunk 6.4c: `batches.each` in an application with a tenant, a run limited to the tenant its saved tenant id names, and a batch reaching what only `Access.public` grants refused (`policy.system_grant_missing`).

Three things start work, and each one calls an ordinary action (decided in
area 1):

| Trigger | Starts when | Lives in |
|---|---|---|
| Route | A request arrives | `routes/` (area 7) |
| Schedule | A timer fires | `schedules/` |
| Subscriber | An event it listens for is committed | `subscribers/` |

All the logic lives in actions, so a reviewer reads one kind of code
whatever started it. Schedules and subscribers add only *when* to run and
*what happens if it fails*.

Two more things feed this machinery without being new triggers. Work over
more items than one action can hold runs as a batch, started by an action
and calling an action per item ("Batches", below). And the outside world
produces events for subscribers: webhooks, a package's own events and
polls, and messages read from a queue ("Events from outside", below).

## Schedules

A schedule runs one action at a fixed interval. It has a name, an `every:`
and a `run:` that calls an action in the same domain with no arguments
(question 1).

```jadpo fragment
// domains/planning/schedules/overdue_reminders.jadpo
/// Queues a reminder for each overdue todo.
schedule overdue_reminders {
    every: 15m
    run: queue_overdue_reminders()
}
```

- `every:` takes seconds, minutes, hours or days (`30s`, `15m`, `1h`, `1d`),
  counted in UTC from when the application starts.
- Only one run of a schedule happens at a time. Ticks missed while the
  application was down, or while a run was still going, become one run, not a
  backlog. Each schedule's state and lease live in the store
  `schedules: { store: }` names (area 5), or the only store when there is one.
- Inside the run, `clock.now` is fixed: every read gives the time the run
  started, so the action does not need the time passed in.
- A failed run is logged and shown to the operator; the next tick is the
  retry. Schedules have no retry settings.
- Entities with `purge:` in their lifecycle (area 5) get a built-in hourly
  purge schedule from the compiler; nothing is written for it.

## Batch work keeps its progress

An action is all-or-nothing (area 3). If one action emailed 500 owners and
failed on the 300th, its database writes would roll back but 299 emails would
already be sent. So a schedule that covers many items does not do the work
itself: it finds the items and emits one event per item. Each item is then
handled by its own action, retried on its own, and one bad item never blocks
or undoes the rest (question 11).

```jadpo fragment
// domains/planning/actions/queue_overdue_reminders.jadpo
action queue_overdue_reminders() -> Unit {
    // a query in queries/ returning at most 500 todos not yet queued
    var cutoff: ReminderCutoff = ReminderCutoff(clock.now)
    var todos: List<Todo> = overdue_todos(cutoff)   // a list query declares no failures
    for todo in todos {
        attempt Todo.update(todo) {
            set: { reminder_queued_at: clock.now }
        }
        emit ReminderDue {
            todo_id: todo.id
        }
    }
}

// domains/planning/subscribers/send_reminder.jadpo
subscriber send_reminder {
    on: ReminderDue
    run: send_reminder_email(event.todo_id)
}
```

A query returning a list has no declared failures, so it takes no
`attempt`; a database outage is a fault, and the next tick retries. The update
can fail (the todo changed since it was read), so it takes `attempt`: the
failure ends the run, nothing commits, and the next tick tries again. The
query's limit bounds each run; anything left over is picked up by the
next tick. Marking each todo in the same transaction stops the next tick from
queuing it twice.

This shape suits a bounded set found by a query. Work over a set with no
bound (every row of an uploaded file, a large export, every page of a
provider's list) is a batch.

## Batches: work over many items

Decided 2026-10-16 (Richard). One way reads and writes row data of any
size, and walks any large set of items: explicit helpers per format and
direction, all running on one shared machinery, batches. Every helper
handles small and large inputs alike.

```jadpo fragment
// domains/catalogue/actions/import_products.jadpo
action import_products(import: ProductImport) -> BatchRun {
    return csv.read(
        import.sheet,
        as: ProductRow,
        each: row -> save_product(row, account: import.account_id),
        finished: summary -> import_finished(summary, import: import.id),
        key: import.id
    )
}

// domains/catalogue/actions/export_products.jadpo
action export_products(request: ProductExport) -> BatchRun {
    return csv.write(
        exports,
        name: "products.csv",
        format: CsvFormat.spreadsheet,
        from: products_for_export(request.account_id),
        each: product -> ProductExportRow { sku: product.sku, name: product.name, price: product.price },
        finished: done -> send_export_ready(done.file, to: request.requested_by),
        key: request.id
    )
}
```

| Helper | Reads or writes |
|---|---|
| `csv`, `tsv`, `json_lines` and `json` read and write helpers | Files; each operation's signature, result and failures are in [area 19, "Data formats"](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines) (moved 2026-10-17) |
| `batches.each(from:, each:, finished:, key:)` | No file: a large query, or every page of a provider's list |

- **Reading** takes a stored `File`, the row type (`as:`), `each:` calling
  one action per row, and `finished:`. **Writing** takes the store to write
  to, the file's `name:`, `from:` giving the items, `each:` returning the
  row to write for each, and `finished:`, which receives the stored file.
  `batches.each` takes `from:` and calls an action per item. `from:` is a
  call to a named query returning a `Page` (area 6), walked in keyset
  pages, or to a provider's list operation, walked by `page.next` (area
  11).
- **`each:` and `finished:` are the inline `->` form** that area 3 allows
  for standard operations: without a destination it calls an action in the
  same domain per item, as a subscriber's `run:` does; when writing it is
  a pure expression giving the row. Values they capture (`import.id`) are
  saved with the run, so only storable values may be captured.
- **A helper starts after the calling action commits**, like `emit`, and is
  called only in actions. It returns a `BatchRun` at once and runs in the
  background; if the action rolls back, the run never starts. `key:`
  makes a repeated start return the same run, so a client's retry does not
  import a file twice.
- **`batches.status(run)`** gives a `BatchProgress`: items done, the
  estimated total (from the file's size, or the query's count), problems so
  far and the state (`running`, `finished`, `failed`, `cancelled`);
  `batches.cancel(run)` stops the run after the batch in hand. `finished:`
  receives an `ImportSummary` (reads and `batches.each`) or an
  `ExportSummary` (writes), each with the counts of items done and items
  with problems and the problems themselves, and for an export the written
  `File`. Each `RowProblem` holds the row, the column and the rule. The
  types are area 2's.

**How a run goes.** Items are grouped into batches, and each batch is one
transaction that commits its writes and the run's position together, so a
crash or a deploy resumes at the next item and nothing is written twice.
A file's order is kept, and a run's batches go one after another; separate
runs proceed side by side. The run's state lives beside the outbox, in the
store the action writes, or the one `events: { store: }` names. Each item's
action runs inside its batch's transaction, under area 6's rule for a
handled failure, so an item whose action fails is undone alone and becomes
a problem while the rest carry on. `batches.each` is also the one way to
make an outside call per item, since no action calls inside `for` (area
3): when `each:`'s action makes outside calls, each item is a batch of its
own, run as a subscriber's delivery is, its calls first under keys derived
from the run and the item's position, then its writes and the run's
position committed together, so a repeated item replays rather than
resends. `finished:` runs once after the last batch, in its own
transaction.

**Problems never stop a run.** A row that cannot be decoded, or an item
whose action fails, becomes a `RowProblem`: the row as a spreadsheet
numbers it (the header is row 1, the first data row row 2), the column,
the field and the rule, never the value, so personal data stays out of
problem lists and logs. Only a file-level fault stops a run, before or at
the place it occurs: a missing required column, an unterminated quote
(named by the line where it opened), an encoding the format does not
declare, or text that is not text. "All rows or none" is a recipe, not a
feature: a first run that only validates, then a second that writes.

**Rows are typed both ways.** A row read is an `Input` and a row written
an `Output` (area 7's rules, problems and JSON encoding): a column maps to
a field by its name or by `column: "Email address"`, and cells in other
conventions use the decided `wire:` encodings (`Wire.day_month_year`) or a
pure `decode:` function. A CSV or TSV row holds single values only; a list
or an object in one is a compile error naming JSON Lines. Writing CSV
names its reader: `CsvFormat.spreadsheet` writes a byte-order mark, CRLF
line endings and escapes formula-like text, and `CsvFormat.standard` CRLF
only; a write without `format:` is a compile error. Reading takes UTF-8
(a byte-order mark removed), UTF-16 with a mark, and legacy encodings the
format declares, never guessing; writing is UTF-8. gzip is read only with
`max_unpacked_size:`, and no archives are opened. The format namespaces
themselves are area 19's ("Data formats", decided 2026-10-17).

**No other way.** There is no loop over a file's rows and no parse of a
whole file into a list. `store.read(file)` returns `FileContents` for
passing bytes on (area 16), which the helpers do not accept; a provider's
CSV report is put in a store and read like an upload; `json.read` given
text (named `json.decode` until area 19, 2026-10-17) is only for single
JSON documents in memory. An export walks live data, so a
record changed during it appears as its page saw it; `jadpo check` warns
when `from:` orders by a field that can be updated, since ordering by an
immutable field (`id`, or a creation time with `id`) is what keeps every
record in exactly once.

Batches replace chained events for paging: an event per page would stop
at the 16-deep chain limit ("Limits and runtime faults") after 16 pages.
This also settles area 11's pagination, where work over many pages ran one
page per subscriber delivery.

> Built in chunk 6.4: `batches.each` over a named query's pages, with
> `finished:` and `key:`, `batches.status` and `batches.cancel`, each page
> one transaction committing its items' writes and the run's position,
> and a run resumed after a crash or a deploy; what the page left open is
> "Chosen in chunk 6.4", under "Review". The `csv`, `tsv`, `json_lines`
> and `json` file helpers, `store.read`'s whole-read cap and the warning
> for a `from:` ordered by a field that can be updated come in chunk
> 6.4b, with a minimal `files` capability (area 16).

A sweep that marks every item seen, a page at a time, once per sweep,
keeping how many it went through:

```jadpo check project=batches-sweep
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}

entity Stocktake {
    id: Uuid
    shelf_run: BatchRun?
    seen: Int?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}

/// The items, a page at a time, in code order.
query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

/// Marks one item seen.
action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}

/// Keeps how many items the sweep went through.
action swept(summary: ImportSummary, stocktake_id: Stocktake.id) fails StocktakeNotFound -> Unit {
    attempt Stocktake.update(stocktake_id) {
        set: {
            seen: summary.items
        }
    }
}

/// Starts the sweep over every item, once per sweep.
action start_sweep(stocktake_id: Stocktake.id, page: ItemPage) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> mark_seen(item),
        finished: summary -> swept(summary, stocktake_id),
        key: stocktake_id
    )
}

/// How far the sweep has got.
action sweep_progress(shelf_run: BatchRun) -> BatchProgress {
    return batches.status(shelf_run)
}

/// Stops the sweep after the page in hand.
action stop_sweep(shelf_run: BatchRun) -> Unit {
    batches.cancel(shelf_run)
}
```

## Declaring an event

An event records that something happened. It is a type with the `Event` base,
named in the past tense, one per file in the domain's `events/` folder, and
marked `public` when another domain subscribes to it (question 3).

```jadpo fragment
// domains/planning/events/todo_completed.jadpo
/// A todo was marked complete.
public type TodoCompleted = Event {
    todo_id: Todo.id
    completed_at: Instant
}
```

- This is the same form as failures (area 4): `type X = Event { ... }`, as
  `type X = Failure { ... }`.
- Fields follow the ordinary type rules. Carry identifiers and the facts of
  the moment, not whole records: a subscriber that needs more reads the
  current state.
- Each event is its own type. There are no event families with variants
  (`TodoEvent.completed`), so subscribing to an event and matching on it never
  depend on a list that grows.
- An event nothing subscribes to is reported like any unused declaration
  (area 1).

## Emitting an event

`emit` is a statement, written like `reject` (question 4).

```jadpo fragment
// domains/planning/actions/complete_todo.jadpo
action complete_todo(todo_id: Todo.id) fails TodoNotFound -> Todo {
    var todo: Todo = attempt mark_complete(todo_id)
    emit TodoCompleted {
        todo_id: todo.id
        completed_at: clock.now
    }
    return todo
}
```

- Only actions emit. Functions and queries cannot, and routes, schedules and
  subscribers emit by calling an action.
- The event is saved in the same database transaction as the action's writes
  and delivered only after that transaction commits. If the action fails or
  rolls back, the event is discarded and no subscriber ever sees it. This is
  the transactional outbox pattern, which Laravel (`afterCommit`), Oban
  (insert inside the transaction) and Rails 8
  (`enqueue_after_transaction_commit`) also use.
- `emit` needs no `attempt`: it cannot fail with a domain failure, and a
  storage problem is a fault that fails the whole action (areas 3 and 4).
- `emit` returns nothing. The caller cannot wait for or read what
  subscribers do.

## Subscribers

A subscriber names one event and the action it runs. Inside `run:`, `event`
is the delivered event (question 5).

```jadpo fragment
// domains/planning/subscribers/update_statistics.jadpo
subscriber update_statistics {
    on: TodoCompleted
    run: count_completion(event)
}
```

- One event per subscriber. To react to two events, write two subscribers.
- Every subscriber of an event gets its own delivery. One failing does not
  affect the others, and nothing that happens in a subscriber reaches the
  action that emitted the event: that action has already committed and
  returned.
- A subscriber's action may emit further events.
- A subscriber runs as the application, not as the user who caused the event
  (question 8). If it needs to know who, the event carries a field such as
  `by: User.id`.

## Reacting in another domain

A domain changes another domain in one of two ways (decided in area 1):

| | Public action | Event |
|---|---|---|
| When | Inside the caller's transaction | After the caller commits |
| If it fails | The caller fails and everything rolls back | Only the subscriber's delivery fails; the emitter is unaffected |
| Who knows whom | The caller names the action | The emitter does not know who listens |
| Use for | Rules that must hold together (reserve stock with the order) | Reactions that may happen later (send the confirmation email) |

The subscriber lives in the reacting domain and calls that domain's own
action; the event must be `public` in the emitting domain.

```jadpo fragment
// domains/notifications/subscribers/email_on_completion.jadpo
subscriber email_on_completion {
    on: TodoCompleted
    run: send_completion_email(event)
}

// domains/notifications/actions/send_completion_email.jadpo
action send_completion_email(event: TodoCompleted) fails EmailRejected -> Unit {
    ...
}
```

## Long-running coordination

A process that waits for several things over time, such as an order waiting
for payment and stock, needs no workflow language (decided in area 1). Its
progress is an ordinary entity, each subscriber records one step on it, and a
schedule handles deadlines.

```jadpo fragment
// domains/ordering/subscribers/on_payment_succeeded.jadpo
subscriber on_payment_succeeded {
    on: PaymentSucceeded
    run: record_payment(event.order_id)
}

// domains/ordering/actions/record_payment.jadpo
action record_payment(order_id: Order.id) fails OrderNotFound -> Unit {
    // set paid_at on the order; if stock is already reserved,
    // set status to ready and emit OrderReady
}

// domains/ordering/schedules/cancel_unpaid_orders.jadpo
schedule cancel_unpaid_orders {
    every: 5m
    run: cancel_expired_orders()
}
```

The state is in the database, so it survives restarts and deploys, can be
queried and shown in an admin screen, and two steps arriving together cannot
both complete the order, because every write checks for concurrent change
(area 3). Temporal and Inngest offer a different model, code that pauses
between steps; the [proposals](research/12-jobs-and-events-proposals.md)
compare them.

## Delivery: at least once, safe to repeat

- **At least once.** An event committed is delivered to each subscriber at
  least once. A crash can deliver it again.
- **Database work happens once.** The subscriber's writes commit together
  with a record that this delivery was handled, so a repeated delivery finds
  the record and does nothing.
- **Outside calls can repeat.** A provider call cannot join the
  transaction. Each delivery has a stable key (event and subscriber), from
  which the runtime derives the idempotency key each provider operation
  sends (area 11; "Several changing calls in one action", below), so a
  provider that supports keys ignores the repeat.
- **Every action repeats as a unit.** A changing call that takes no
  idempotency key, and that its package does not mark safe to repeat, is
  the only changing call in its action, so a rerun or a redelivery repeats
  that one call. Changing calls that do take keys may be several, in order:
  each is recorded as it goes, so a rerun replays what already happened
  instead of sending it again (question 11, revised 2026-10-16). Writes are
  not limited this way: every write to the action's store, across any
  number of domains, commits together after the last call (area 6).
- **No order is promised** between events (question 6). A subscriber that
  cares about order reads the current state instead of trusting the event's
  copy, which is also what makes repeats safe.
- **A payload is the contract's JSON.** An event saved in the outbox, a
  job's arguments and the values a batch captures are written as the JSON
  of their types, by the route codec a request's body goes through, so a
  standard type keeps its currency, unit and every place (area 19, "Every
  boundary through the contract"). The version rule: a payload is read
  with its type as it is when it is delivered, an event type's shape is
  recorded as an API record's is, and a change to it that would break an
  API record (a field removed, renamed or narrowed) needs area 18's
  acceptance; a payload saved before such a change that no longer reads is
  parked with its problem, never dropped, as a message that keeps failing
  is ("Events from outside"). Decided by the lead session, 2026-10-18,
  under Richard's instruction of 2026-10-17 that every boundary goes
  through the contract; built with the outbox (roadmap 6.1). Chunk 5.9b
  numbered every event type in the plan's JSON table, by its identity
  key, for the outbox to write and read its payload through (area 5,
  "Structured values").

## Several changing calls in one action

Decided 2026-10-16 (Richard), replacing "one changing call per action"
(question 11), after the [multi-step calls review](research/multi-step-calls-review.md),
adopted with its ten fixes and its recommended answers. Starting a
subscription at a payment provider takes several calls (create the
customer, attach the card, create the subscription); each takes effect at
the provider, so an action that makes them must never send one twice and
never lose track of one that happened.

```jadpo fragment
// domains/billing/subscribers/start_subscription.jadpo
subscriber start_subscription {
    on: PlanChosen
    run: subscribe(event)
}

// domains/billing/actions/subscribe.jadpo
action subscribe(event: PlanChosen) fails CardDeclined -> Unit {
    var customer: Stripe.Customer = attempt stripe.customers.create(...)       // step 1
    attempt stripe.payment_methods.attach(...)                                 // step 2
    var subscription: Stripe.Subscription = attempt stripe.subscriptions.create(...)   // step 3
    attempt Account.update(...)                                                // the writes, after the last call
}
```

**What is allowed.**

- **Up to 8 changing calls** in order, counted on the action's longest
  path, a package action's calls included (a package action's steps are
  steps of the calling action, under one record, and a second package
  action continues the numbering). Each must take an idempotency key or be
  marked by its package as safe to repeat as it is; a changing call with
  neither stays the only changing call in its action, even as the last
  step, since an unkeyed step in doubt always needs an operator. All
  compiler-checked.
- **Only where the action's key survives a retry**: in actions run by
  subscribers, and by routes whose API requires `Idempotency-Key` (area 7).
  A route without a required key gets a new request id when the client
  retries, and a schedule's retry is the next tick, so neither could
  resume; starting several changing calls from one is a compile error that
  names the fixes: require the key, or answer `202` and do the steps in a
  subscriber (the usual shape for a browser form), or for a schedule emit
  an event per item.
- **No outside call inside `for`**, in any action (area 3), so steps are
  numbered along a path whose length the compiler knows; a loop of calls
  is `batches.each`.
- **`store.put` is a step** like a provider call: in an action with
  several changing calls its key is derived as any step's is, so a rerun
  writes the same object rather than a second one (area 16).
- **Large answers stay out.** A package may mark an operation
  `large_output: true`; the compiler refuses such an operation in an action
  with several changing calls.
- **Writes come after the last call**, with no transaction open during
  the calls (area 6); the record's own writes are the runtime's, each in a
  short transaction of its own.

**Keys.** Each step's key is derived from the application, the
environment, the connection, the caller's scope, the action's key (the
delivery key, or the client's `Idempotency-Key`), the step's number and a
key generation, hashed to 36 characters or the package's `max_length:`.
Hashing fixes a collision that affects single calls too: area 7 scopes a
client's key to the caller, but a provider such as Stripe scopes keys to
the account, so two users sending the same key would otherwise be given
one answer. Each package states how its provider treats keys (area 11):
how long they are kept (`window:`), their maximum length, the codes for
"in use" and "different request", whether failed answers are replayed,
and any "use a new key" answer.

**The record.** Before a step is sent, the runtime records its position,
its operation, a fingerprint of its request (over the typed value) and
the time sent; when the answer comes, success or failure, it records the
answer, encrypted. An answer over 1 MB is a fault naming the step. A
provider read after the first
changing step is recorded the same way; database reads are not. The
record lives in the store the action writes, beside the outbox (with no
such store, `events: { store: }` is required), and is deleted in the
action's commit transaction. Records are listed in the personal-data
report (area 15).

**A rerun replays.** When the action runs again (a retry, a redelivery,
area 6's rerun after a concurrent change), answered steps are replayed in
order without being sent, and `clock.now` is the time stored in the
record, fixed for the record's whole life, so timestamps written after a
resumed action are from when it started, which is also when the provider
acted. At each step the operation and fingerprint must match the record,
and at the end every recorded step must have been replayed. Any
difference (a deploy changed the code, a fresh database read changed a
request, configuration changed) stops the action as `diverged` before
anything is sent, naming the step and the fields that differ, and holds
it for an operator rather than failing it, so deploying a fix and retrying
finishes it.

- **A step in doubt** (sent, no answer recorded) ends the run to be retried
  later, and the next attempt sends it again under the same key while the
  provider's window lasts. It parks as `outcome_unknown` instead when it
  has no key, when its window would pass, or when the provider replays a
  `5xx` under that key.
- **One runner per record.** A route's key is claimed as today (area 7); a
  delivery's lease carries a generation that every record write checks. A
  provider's "key in use" answer means wait and retry, never a failure.
- **Ending for good after a step took effect is warned.** After the first
  changing step, every point that can end the action for good (a step's
  failure, a write that can be refused, `reject`, a called action's
  failure) is a warning unless handled with `match`, naming three fixes:
  check first, before the first step; record what happened and finish in a
  subscriber; or undo as another step. Package surfaces list which failures
  follow an effect, so the warning reaches through package actions. This
  answers area 11's question 26 for single calls too.
- **An unfinished record never disappears quietly.** One that ends without
  committing (failed, diverged, parked, or a route whose key lapsed)
  becomes an operator item listing each step and its answer, with retry
  (`--not-done` for a new key generation) and abandon ("What an operator
  sees"). It is kept at most 30 days, then only the step list remains.
- **Deploys.** Each record keeps a digest of its action's step shape.
  Before a deploy, an operator runs `jadpo runs unfinished --build <build
  id>` against production: it lists the unfinished multi-step runs and
  batch runs whose recorded path the new build changes, so they can be let
  finish first. A run resumed under a changed build still stops as
  `diverged`, as above.
- **Tests and tools.** Fakes honour keys and can answer `lost` and
  `in_use`, and `jadpo test` reruns every multi-step test with a crash at
  each step boundary, on by default (area 13); `jadpo dev`, `jadpo watch`
  and `jadpo deliveries show` list each step's state (areas 14 and 17).

Calls to providers that take no key (GitHub, Twilio's messages, Postmark)
stay one per action. A Stripe `500` at a later step is stored under its
key for a day, so it parks the action for an operator rather than being
retried.

The errors (several changing calls from a schedule or from a route
without a required key, more than 8 steps, an unkeyed call beside another
changing call) and the warning are built with multi-step calls, which
follow chunk 7.4 and are not yet a roadmap chunk.

## Where events travel

Events stay inside the Jadpo runtime (Richard, 2026-10-13). They are stored
in the outbox of the store the action writes, in the same transaction as
the action, and the runtime delivers them: on a long-running server from
its own workers, on
Cloudflare Workers right after the commit (`waitUntil`) with the
once-a-minute schedule tick sweeping anything left. One place holds every
log and trace, a request is never slowed by delivery, and no site needs a
queue service. A route that wants the finished result instead of `202`
waits for the work its action set off with `settle:` (area 7, 2026-10-15).

External queues are never the delivery path. A connection can *publish*
chosen public events outward, so other systems can react to them: an outbound
bridge fed from the outbox after commit, at least once, with the trace
context attached (Cloudflare Queues, SQS, Pub/Sub, a webhook). Reading a
queue the other way, through `receiving`, turns each message into one of
the app's events in its own outbox ("Events from outside", below), so
delivery to subscribers stays the runtime's.

## Events from outside

Decided 2026-10-16 (Richard). Besides events the app's actions emit, four
sources outside the app produce events. Each is written to the app's
outbox and delivered to subscribers like any event, with the same retries.

- **Webhooks.** A connection with a webhook secret generates
  `POST /webhooks/<connection>` (area 11): the runtime checks the
  signature with a five-minute tolerance, drops repeats by the provider's
  event id, and writes the event to the outbox; subscribers run after
  commit. Every webhook event carries the provider's `occurred_at`;
  delivery may be out of order, which the app handles by ignoring an event
  older than what it stored (a recipe shows it). A webhook that needs a
  reply runs the connection's named action inside the request instead
  (area 11).
- **A package's own events.** A package's `public` events, its webhooks
  and the enriched events its own subscribers emit
  (`Stripe.SubscriptionRenewed`), are subscribed to with `on:` like the
  app's own. An event without `public` reaches only the package's own
  subscribers, and subscribing to one is a compile error. A package emits
  only about the outside world, never as a side effect of the app calling
  it: the app's action emits its own event there, where a reviewer sees it.
- **Polls.** `polls:` in a package's `provider.jadpo` names an operation, a
  default interval and the event emitted when the answer changes, turning
  an API without webhooks into events. The runtime runs a poll only while
  something subscribes to its event, keeps what it last saw (runtime state
  in the database, like webhook de-duplication) and emits only changes;
  the connection can change the interval (`polls: { latest_rates: 6h }`).
  Polls are for feeds that do not depend on the app's data (rates, a price
  list); checking each of the app's own records is the app's schedule
  calling the package, which a package can ship as a recipe.
- **Queues, through `receiving`**, the inbound twin of `publishing`.

```jadpo fragment
// app.jadpo
application {
    connections: {
        warehouse_queue: {
            package: AmazonSqs.v2012_11_05
            credential: { ... }
            queue_url: config.warehouse_queue_url
        }
    }
    receiving: {
        warehouse_queue: {
            type_at: "type"
            id_at: "message_id"
            events: [Warehouse.OrderShipped, Warehouse.StockLow]
        }
    }
}
```

- A queue names the field giving each message's type, the field giving
  its id, and the events it may carry; a queue of one kind lists one event
  and no `type_at:`. The events come from another Jadpo app's generated
  package or are declared by the app (a domain's `events/` or its own
  integration in `shared/providers/`, each with its `wire_name:`); the
  queue's package knows only how to fetch.
- Each message becomes its event for the app's subscribers and is
  acknowledged only after the subscriber's transaction commits; repeats are
  dropped by the message id, stored in the database.
- A message whose type is not listed is parked for an operator, never
  dropped. A message that keeps failing is left to the queue's own
  dead-letter queue and parked for an operator.
- An HTTP API is a package and anything needing its own protocol or a
  platform binding is a built-in driver: Amazon SQS and Google Pub/Sub's
  pull API are packages; SNS and Pub/Sub's push are webhooks through a
  package with a handshake (area 11); Cloudflare Queues is a built-in
  driver on Workers, its consumer generated into the Wrangler
  configuration, and a package (its HTTP pull API) from a native server;
  Kafka is a built-in driver in a later release.

## Retries and giving up

What happens after a run depends on how it ended, using the failure kinds
from area 4 (question 7):

| How the action ended | What happens |
|---|---|
| Succeeded | Done |
| A failure of kind `unavailable` or `rate_limited` | Retried later |
| Any other declared failure (`not_found`, `conflict`, `rejected`, ...) | Failed; not retried, because the same input fails the same way |
| A fault before anything committed (database unavailable, timed out) | Retried later |
| `outcome_unknown`: an outside call may have happened | Parked; never retried automatically |
| A defect (`internal_fault`, overflow) | Failed; not retried until someone fixes the code |

- Retries wait longer each time, with full random jitter, so many failures do
  not retry together; a `Retry-After` from a provider is honoured. By default a subscriber gets 10 attempts within 24 hours,
  then fails.
- A subscriber can change its budget; schedules cannot, since the next tick
  is their retry.

```jadpo fragment
subscriber record_payment_webhook {
    on: PaymentReceived
    run: record_payment(event.payment_id)
    retry: {
        attempts: 20
        within: 3d
    }
}
```

## What an operator sees

Every delivery and schedule run has a state:

| State | Meaning | The operator can |
|---|---|---|
| `pending` | Waiting to run, or waiting to retry | Cancel it |
| `running` | A worker has it | Wait; a worker that dies loses it after its time limit |
| `succeeded` | Done | Nothing |
| `failed` | Gave up, or a failure that is not retried (the dead-letter list) | Retry it once the cause is fixed, or dismiss it |
| `outcome_unknown` | An outside call may have happened | Check with the provider, then mark it done or not done; "not done" retries it |
| `diverged` | A rerun of an action with several changing calls no longer matches its record | Deploy a fix and retry, or abandon it |

Each failed or unknown item keeps the event, the subscriber, every attempt
with its failure code and internal fields (area 4's logging rules), and the
request that emitted it. A `failed` or `outcome_unknown` item writes a log
event and counts towards an alert. The screens and commands are area 14's.

**Unfinished multi-step runs are operator items too** (decided
2026-10-16). Whenever an action with several changing calls ends without
committing (failed, diverged, parked, or a route whose key lapsed), the
operator sees an item listing what took effect at each provider, step by
step with its answer, and can retry it (with `--not-done`, under a new key
generation) or abandon it. Nothing about it is quietly dropped; an item is
kept 30 days, then only its step list.

**Batch runs and queue messages** show here as well: each batch run with
its state, progress against the file's size, problems and an estimated
finish, which `batches.status` gives the app; and each queue message
parked because its type is not listed or because it kept failing.

### Watching schedules

Schedules fail quietly in most frameworks: a job that stops running produces
no error at all. Jadpo records and checks every part of a schedule's life
(Richard, 2026-10-13):

- **Every run is recorded** in a system table: due time, start, end,
  duration, lateness (start minus due), outcome with its failure, items
  processed, events emitted, and the trace id, so a run opens as one trace
  (question 17).
- **Every tick is recorded too,** including skipped ones and why (lease held,
  paused), and the last tick received from an external adapter.
- **`jadpo schedules`** lists each schedule with its last run, next due time,
  lease holder, recent failures and the adapter's last tick; `run <name>`
  starts one now (still respecting the lease), `pause` and `resume` stop and
  start it, and `history <name>` shows past runs.
- **`jadpo why <schedule>`** answers "why didn't it run?": paused, lease held
  by a run started at a given time, adapter not ticking since a given time,
  or the run failed.
- **Metrics** (area 16's telemetry): run duration, lateness, skipped ticks,
  failures and items per run, per schedule.
- **Alerts with no setup:** no successful run within twice the interval;
  ticks stopped arriving from an external adapter; a run past 80% of the
  15-minute limit; three failures in a row; a lease held past its expiry.
- The console (area 17) shows each schedule's timeline of runs and ticks.

## Limits and runtime faults

| Limit | Proposed value | When it is exceeded |
|---|---|---|
| Events emitted by one action | 1,000 | Defect: the action fails and rolls back |
| Size of one event | 64 KB | Defect: the action fails and rolls back |
| Events causing events, from one request | 16 deep | The delivery fails with `event_chain_limit`, stopping loops; paging uses batches, not chains |
| One run of a subscriber or schedule | The time limit of the route or integration it calls, at most 15 minutes | Treated as a timeout: retried if nothing outside was called, otherwise `outcome_unknown` |
| Changing calls in one action | 8, on its longest path, package actions' calls included | Compile error |
| One batch of a batch run | 1,000 items or 4 MiB of file, whichever comes first | The batch ends and the next begins |

Numbers are recommendations to confirm with measured runs (question 12).

## The delivery descriptor

There is none. An earlier design put a 45-line closed sub-language inside a
job, `delivery: reminder_v1 { selection: ... hooks: ... service: ... authority: ... completion: ... }`,
written for exactly one feature: overdue-todo reminders. It restated a query
(`order_by: Todo.due_at asc, Todo.id asc`, `limit: 500`) and accepted only
literal values such as `limit: 500` and `"reminder.v1"`. The batch pattern
above does the same job with an ordinary query, an ordinary action and an
ordinary subscriber, so the descriptor is deleted, with its 14 syntax rules
and 6 type rules (question 2).

## Errors

Built in chunk 6.1b: the errors of events and subscribers. Built in chunk 6.2: the errors of schedules.

### `event.emit_invalid`

Only an action emits, and only an event; a function emits by calling an action:

```jadpo check project=e-emit-invalid expect=event.emit_invalid
type Note = Text {
    max_length: 200
}

type NoteSeen = Event {
    note: Note
}

function seen(note: Note) -> NoteSeen {
    emit NoteSeen {
        note: note
    }
    return NoteSeen {
        note: note
    }
}
```

```text output
error: A function can't emit `NoteSeen`
 --> main.jadpo:10:5
   |
10 |     emit NoteSeen {
   |     ^^^^^^^^^^^^^^^ only actions emit
  reason: An event is kept in the same transaction as an action's writes and handed to its subscribers once it commits, so only actions emit; a function emits by calling an action.
  next: Move the `emit` into the action this code calls, or make this an action (agent)
  rule: event.emit_invalid
  help: docs/language/12-jobs-and-events.html#event-emit-invalid

check failed: 1 error, 1 file
```

### `event.settings_invalid`

`events: { store: }` names a database:

```jadpo check project=e-events-settings file=app.jadpo expect=event.settings_invalid
application {
    events: {
        store: archive
    }
}
```

```text output
error: `store: archive` isn't a database
 --> app.jadpo:3:16
  |
3 |         store: archive
  |                ^^^^^^^ not a database
  reason: An event is kept in a database's outbox until its subscribers have it; this application's databases are `main`.
  next: Name a database connection (agent)
  rule: event.settings_invalid
  help: docs/language/12-jobs-and-events.html#event-settings-invalid

check failed: 1 error, 1 file
```

### `event.store_required`

An action that emits and writes no store has nowhere to keep its events until `events: { store: }` names one:

```jadpo check project=e-store-required expect=event.store_required
type Note = Text {
    max_length: 200
}

type NoteSeen = Event {
    note: Note
}

action glance(note: Note) -> Unit {
    emit NoteSeen {
        note: note
    }
}
```

```text output
error: `glance` emits `NoteSeen` but writes no store to keep it in
 --> main.jadpo:10:5
   |
10 |     emit NoteSeen {
   |     ^^^^^^^^^^^^^^^ no outbox
  reason: An event is kept in the outbox of the store its action writes, in the same transaction, until its subscribers have it; `glance` writes no store, itself or through what it calls, and `app.jadpo` names none with `events: { store: }`.
  next: Add `events: { store: main }` to `application { }` in `app.jadpo` (human)
  rule: event.store_required
  help: docs/language/12-jobs-and-events.html#event-store-required

check failed: 1 error, 1 file
```

### `schedule.every_invalid`

`every:` is a whole number of seconds, minutes, hours or days:

```jadpo check project=e-schedule-every expect=schedule.every_invalid
action tidy() -> Unit {}

/// Tidies up four times a second.
schedule tidy_up {
    every: 250ms
    run: tidy()
}
```

```text output
error: `every: 250ms` can't be how often a schedule runs
 --> main.jadpo:5:12
  |
5 |     every: 250ms
  |            ^^^^^ not seconds, minutes, hours or days
  reason: `every:` is a whole number of seconds, minutes, hours or days above zero (`30s`, `15m`, `1h`, `1d`), counted in UTC from when the application starts.
  next: Write a duration such as `every: 15m` (agent)
  rule: schedule.every_invalid
  help: docs/language/12-jobs-and-events.html#schedule-every-invalid

check failed: 1 error, 1 file
```

### `schedule.item_invalid`

A schedule holds `every:` and `run:`, once each; how it is retried is the same for every schedule:

```jadpo check project=e-schedule-item expect=schedule.item_invalid
action tidy() -> Unit {}

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
    retry: next_schedule
}
```

```text output
error: `retry` isn't something a schedule holds
 --> main.jadpo:7:5
  |
7 |     retry: next_schedule
  |     ^^^^^ not a schedule item
  reason: A schedule says how often it runs with `every:` and the action it runs with `run:`; nothing else, since one run at a time, merged missed ticks and the next tick as the retry are how every schedule runs.
  next: Write `every: <duration>` and `run: <action>()`, once each (agent)
  rule: schedule.item_invalid
  help: docs/language/12-jobs-and-events.html#schedule-item-invalid

check failed: 1 error, 1 file
```

### `schedule.run_invalid`

`run:` calls an action of the schedule's own domain with nothing; the time is `clock.now`, fixed for the run:

```jadpo check project=e-schedule-run expect=schedule.run_invalid
type RemindAt = Instant {}

action remind(at: RemindAt) -> Unit {}

/// Sends the reminders due.
schedule reminders {
    every: 15m
    run: remind(RemindAt(clock.now))
}
```

```text output
error: A schedule passes nothing to `remind`
 --> main.jadpo:8:16
  |
8 |     run: remind(RemindAt(clock.now))
  |                ^^^^^^^^^^^^^^^^^^^^^ arguments given
  reason: `run:` calls one action of the schedule's own domain with no arguments; the action reads `clock.now`, fixed for the run, and queries for anything else, so all the logic lives in the action.
  next: Write `run: remind()`, and let `remind` read `clock.now` and its queries for what it needs (agent)
  rule: schedule.run_invalid
  help: docs/language/12-jobs-and-events.html#schedule-run-invalid

check failed: 1 error, 1 file
```

### `schedule.settings_invalid`

`schedules: { store: }` names a database:

```jadpo check project=e-schedules-settings file=app.jadpo expect=schedule.settings_invalid
application {
    schedules: {
        store: archive
    }
}
```

```text output
error: `store: archive` isn't a database
 --> app.jadpo:3:16
  |
3 |         store: archive
  |                ^^^^^^^ not a database
  reason: A schedule's next due time and its lease are kept in a database; this application's databases are `main`.
  next: Name a database connection (agent)
  rule: schedule.settings_invalid
  help: docs/language/12-jobs-and-events.html#schedule-settings-invalid

check failed: 1 error, 1 file
```

### `schedule.store_required`

With several databases, `schedules: { store: }` says which keeps every schedule's state and lease; with one, it is that one:

```jadpo check project=e-schedule-store file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
}
```

```jadpo check project=e-schedule-store file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-schedule-store file=domains/planning/actions/tidy.jadpo hidden
action tidy() -> Unit {}
```

```jadpo check project=e-schedule-store file=domains/planning/schedules/tidy_up.jadpo expect=schedule.store_required
use planning/actions { tidy }

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
```

```text output
error: `tidy_up` has no database named to keep its state in
 --> domains/planning/schedules/tidy_up.jadpo:4:10
  |
4 | schedule tidy_up {
  |          ^^^^^^^ no store for its state
  reason: Each schedule keeps its next due time and the lease that lets one instance run it at a time in a database; this application has `archive` and `main`, and `app.jadpo` names none with `schedules: { store: }`.
  next: Add `schedules: { store: archive }` to `application { }` in `app.jadpo` (human)
  rule: schedule.store_required
  help: docs/language/12-jobs-and-events.html#schedule-store-required

check failed: 1 error, 4 files
```

### `subscriber.argument_invalid`

A subscriber passes its event or the event's fields:

```jadpo check project=e-subscriber-argument expect=subscriber.argument_invalid
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.system]
        update: [Access.system]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action send_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt Reminder.update(reminder_id) {
        set: {
            sent: true
        }
    }
}

subscriber send_due {
    on: ReminderDue
    run: send_reminder(Reminder.id)
}
```

```text output
error: A subscriber passes its event, not `Reminder`
 --> main.jadpo:31:24
   |
31 |     run: send_reminder(Reminder.id)
   |                        ^^^^^^^^ `Reminder`
  reason: A subscriber's `run:` passes `event` or fields read through it, as written; the action works out anything else itself.
  next: Pass `event` or a field of it, and move the rest into the action (agent)
  rule: subscriber.argument_invalid
  help: docs/language/12-jobs-and-events.html#subscriber-argument-invalid

check failed: 1 error, 1 file
```

### `subscriber.event_invalid`

`on:` names an event, not an entity:

```jadpo check project=e-subscriber-event expect=subscriber.event_invalid
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.system]
        update: [Access.system]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action send_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt Reminder.update(reminder_id) {
        set: {
            sent: true
        }
    }
}

subscriber send_due {
    on: Reminder
    run: send_reminder(event.reminder_id)
}
```

```text output
error: `on: Reminder` isn't an event
 --> main.jadpo:30:9
   |
30 |     on: Reminder
   |         ^^^^^^^^ not an event
  reason: A subscriber listens for one event, a type with the `Event` base that an action emits.
  next: Name an event, declared as `type <Name> = Event { ... }` in a domain's `events/` folder (agent)
  rule: subscriber.event_invalid
  help: docs/language/12-jobs-and-events.html#subscriber-event-invalid

check failed: 1 error, 1 file
```

### `subscriber.item_invalid`

A subscriber holds `on:`, `run:` and `retry:` only:

```jadpo check project=e-subscriber-item expect=subscriber.item_invalid
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.system]
        update: [Access.system]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action send_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt Reminder.update(reminder_id) {
        set: {
            sent: true
        }
    }
}

subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
    timeout: 5m
}
```

```text output
error: `timeout` isn't something a subscriber holds
 --> main.jadpo:32:5
   |
32 |     timeout: 5m
   |     ^^^^^^^ not a subscriber item
  reason: A subscriber names the event it listens for with `on:` and the action it runs with `run:`, and may change how it is retried with `retry: { attempts: within: }`; nothing else.
  next: Write `on: <Event>` and `run: <action>(event)`, once each (agent)
  rule: subscriber.item_invalid
  help: docs/language/12-jobs-and-events.html#subscriber-item-invalid

check failed: 1 error, 1 file
```

### `subscriber.retry_invalid`

`attempts:` is from 1 to 100:

```jadpo check project=e-subscriber-retry expect=subscriber.retry_invalid
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.system]
        update: [Access.system]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action send_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt Reminder.update(reminder_id) {
        set: {
            sent: true
        }
    }
}

subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
    retry: {
        attempts: 500
    }
}
```

```text output
error: `attempts: 500` can't be how often it is tried
 --> main.jadpo:33:19
   |
33 |         attempts: 500
   |                   ^^^ not 1 to 100
  reason: `retry:` holds `attempts:`, how many times a delivery is tried, from 1 to 100 (10 by default), and `within:`, a duration up to 30 days after the event was emitted (24 hours by default).
  next: Write `retry: { attempts: 20  within: 3d }`, or leave `retry:` out for the default (agent)
  rule: subscriber.retry_invalid
  help: docs/language/12-jobs-and-events.html#subscriber-retry-invalid

check failed: 1 error, 1 file
```

### `subscriber.run_invalid`

`run:` calls the action:

```jadpo check project=e-subscriber-run expect=subscriber.run_invalid
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.system]
        update: [Access.system]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action send_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt Reminder.update(reminder_id) {
        set: {
            sent: true
        }
    }
}

subscriber send_due {
    on: ReminderDue
    run: send_reminder
}
```

```text output
error: `run: send_reminder` isn't a call
 --> main.jadpo:31:10
   |
31 |     run: send_reminder
   |          ^^^^^^^^^^^^^ not a call
  reason: `run:` calls one action of the subscriber's own domain, passing the event or its fields; all the logic lives in the action.
  next: Write `run: <action>(event)` (agent)
  rule: subscriber.run_invalid
  help: docs/language/12-jobs-and-events.html#subscriber-run-invalid

check failed: 1 error, 1 file
```

### `type.batch_capture_not_storable`

What a run saves (`from:`'s query's arguments, what `each:` and
`finished:` capture, and `key:`) is kept as its type's JSON, so the run can
resume after a crash or a deploy; a value an entity field couldn't hold,
such as a request's `Input`, is refused, naming it:

```jadpo check project=e-batch-capture-not-storable file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
```

```jadpo check project=e-batch-capture-not-storable expect=type.batch_capture_not_storable
action mark_on(item: Item, page: ItemPage) -> Unit {
    assert item.seen == item.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_on(item, page))
}
```

```text output
error: `batches.each` can't save `page`
 --> main.jadpo:6:72
  |
6 |     return batches.each(from: items(page), each: item -> mark_on(item, page))
  |                                                                        ^^^^ `ItemPage` has no stored form
  reason: What `each:` captures is worked out when the run starts and saved with it, as its type's JSON, so the run can resume after a crash or a deploy; `page` is `ItemPage`, an `Input`, the shape a request arrives in, where a record keeps an `Object`.
  next: Pass something an entity field could hold instead, such as the `id` that finds `page` (agent)
  rule: type.batch_capture_not_storable
  help: docs/language/12-jobs-and-events.html#type-batch-capture-not-storable

check failed: 1 error, 2 files
```

### `type.batch_context`

Only an action starts a run, reads its progress or cancels it, keeping
what it does in the action's transaction:

```jadpo check project=e-batch-context file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
```

```jadpo check project=e-batch-context expect=type.batch_context
action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}

/// How far a sweep has got, for a report.
function shelf_report(shelf_run: BatchRun) -> BatchProgress {
    return batches.status(shelf_run)
}
```

```text output
error: A function can't call `batches.status`
 --> main.jadpo:7:12
  |
7 |     return batches.status(shelf_run)
  |            ^^^^^^^^^^^^^^ outside an action
  reason: `batches.status` reads a run the runtime keeps in the store an action writes, so only an action calls it.
  next: Call `batches.status` in an action (agent)
  rule: type.batch_context
  help: docs/language/12-jobs-and-events.html#type-batch-context

check failed: 1 error, 2 files
```

### `type.batch_each_invalid`

`each:` and `finished:` are one call of an action in the same domain,
whose arguments are the item, a field inside it, or a value saved with
the run; one that works something out of the item is refused:

```jadpo check project=e-batch-each-invalid file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
```

```jadpo check project=e-batch-each-invalid expect=type.batch_each_invalid
action mark_code(code: ShelfCode) -> Unit {
    assert code == code
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_code(ShelfCode(item.code)))
}
```

```text output
error: `batches.each`'s `each:` isn't one call of an action
 --> main.jadpo:6:68
  |
6 |     return batches.each(from: items(page), each: item -> mark_code(ShelfCode(item.code)))
  |                                                                    ^^^^^^^^^^^^^^^^^^^^ the item and more
  reason: An argument reading `item` is `item` itself or a field inside it, which the run reads from each item; one that reads nothing of it is worked out now and saved with the run. Work the rest out in the action it calls.
  next: Write `each: item -> <action>(...)`, calling an action in this domain (agent)
  rule: type.batch_each_invalid
  help: docs/language/12-jobs-and-events.html#type-batch-each-invalid

check failed: 1 error, 2 files
```

### `type.batch_from_invalid`

A run walks `from:` a page at a time, so it is a call of a named query
giving a `Page`; a list's query, or a page already read, is refused:

```jadpo check project=e-batch-from-invalid file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}
```

```jadpo check project=e-batch-from-invalid expect=type.batch_from_invalid
query first_items() -> List<Item> {
    from: Item
    order: [Item.code asc]
    limit: 10
}

action sweep_shelf() -> BatchRun {
    return batches.each(from: first_items(), each: item -> mark_seen(item))
}
```

```text output
error: `batches.each`'s `from:` isn't a call of a named query giving a `Page`
 --> main.jadpo:8:31
  |
8 |     return batches.each(from: first_items(), each: item -> mark_seen(item))
  |                               ^^^^^^^^^^^^^ gives `List<Item>`
  reason: A run walks `from:` a page at a time, after the action that starts it commits: it calls a named query that gives a `Page<T>`, and each item is a `T`; the query's arguments are worked out now and saved with the run.
  next: Write `batches.each(from: <query>(...), ...)`, with a query whose result is `Page<T>` (agent)
  rule: type.batch_from_invalid
  help: docs/language/12-jobs-and-events.html#type-batch-from-invalid

check failed: 1 error, 2 files
```

## Review

### Status

Built in chunk 6.1a: the runtime's runner, which drives an executor for
work no request waits on, as the system caller (area 9): deferred until
after a response, inline, or once per item in batches; "Chosen in chunk
6.1a" below. Built in chunk 6.1b: `emit` and the outbox, subscribers,
delivery after the response, handled records, retries, trace context,
`events: { store: }` and the outbox sweep, with the errors below that
name `event.` and `subscriber.`; "Chosen in chunk 6.1b" below. Built in
chunk 6.2: schedules, their state and leases, the native minute tick,
every run and skipped tick recorded, and the tick's sweeps, with the
errors that name `schedule.`; "Chosen in chunk 6.2" below. Built in
chunk 6.1c: each domain's event record and what a subscriber stores
from its event; "Chosen in chunk 6.1c" below. The commands that watch
schedules, the built-in purge schedule and
schedule tests in chunk 6.2b;
on Cloudflare Workers, the cron tick and outbox delivery after commit come in
chunk 8.2. Webhooks and a package's own events come with packages (Phase
7). Batches were built in chunk 6.4 ("Chosen in chunk 6.4"
below), with tenants and their grants in chunk 6.4c ("Chosen in chunk
6.4c" below), but for the file helpers, which come in chunk 6.4b, and on
Workers each run's continuation in a fresh invocation, which chunk 8.2
checks. Several changing calls in one action, polls and `receiving`
were decided on 2026-10-16 and come with Phase 7; their errors are built
with them.

### Chosen in chunk 6.1a

What the page leaves to the runtime about running background work,
each the most conservative reading:

- **One runner.** The runtime drives an executor for all of it, so the
  interpreter and compiled code run it the same way: a job is a call of
  an action with its arguments as the system, in a boundary of its own,
  and what a job sets off runs after it.
- **After the response, never in it.** A committed action's events are
  left for the runner, not delivered while the request waits. A
  deployable hands what a response left to its host, which runs it once
  the response has been handed back: Workers' `waitUntil` (chunk 8.2),
  natively a task on the worker thread that answered, with that worker's
  program, so no thread needs a program of its own. Each run has a
  context of its own under the host's clock and no seed, stops at the
  15-minute limit of one run, and gives the thread back as it checks
  that limit. A host with nothing to run it on, a test's or a tool's,
  drops it; the outbox (chunk 6.1b) is what keeps an event, and the
  minute tick sweeps what a dropped run left. Until subscribers run,
  nothing receives an event, and a run's outcome is kept nowhere;
  delivery records keep it from chunk 6.1b.
- **Inline.** A route that waits for its follow-on work (`settle:`, chunk
  6.3) runs it in its own invocation, then answers as the caller it was.
- **Per item.** Each item's action joins its batch's transaction and is
  undone alone when it fails, as a call whose failure its caller handles
  is (area 6); the run writes its position in the same transaction, so
  the items' writes and the position commit together, and the batch's
  events wait for its commit. A concurrent change found by an item or at
  the commit keeps nothing, and the run runs the batch again. A backfill
  worked out in Jadpo (area 5) runs its rows this way, each value kept
  and checked by its field's rules as any value of the field's type is.

### Chosen in chunk 6.1b

What the page leaves to the build about events, each the most
conservative reading:

- **Where an event is kept.** In the outbox of the store the action's
  transaction is in, written as the action commits, with a delivery
  record for each subscriber, `pending` and due at once, so nothing is
  written for an action that rolls back. An action that began no
  transaction keeps them in the store it writes, or, writing none, the
  one `events: { store: }` names, beginning its transaction there for
  them. An action that emits and writes no store, itself or through what
  it calls, with no `events: { store: }`, is `event.store_required`, even
  when it reads one, as the decision says "writes". An event nothing
  subscribes to is still written. Its row keeps its type's key, its
  payload as the contract's JSON, when it was emitted, the emitting
  action's `traceparent`, the encoding and how deep in a chain it is.
- **Emitting.** Only an action emits (`event.emit_invalid` in a function,
  test, fixture or seed), and only an event. Past 1,000 events in one
  action, or an event's JSON past 64 KB, is a defect as it is emitted, and
  the action rolls back.
- **Delivery.** Once the action commits, each delivery is the runner's,
  after the response: it claims its record in one statement (`running`,
  its lease the 15-minute limit of a run and a minute), so a delivery
  another runner holds, or one not due, is left alone; runs the
  subscriber's action as the system, in a boundary of its own; and
  records how it ended. No order is promised. A delivery to a subscriber
  the build no longer has fails as a defect, and an event 16 deep in a
  chain fails with `event_chain_limit`.
- **Handled records.** `jadpo_handled_deliveries`, keyed by the
  delivery's `id`, in the store the subscriber's action writes, inserted
  in its transaction when it opened one there, so it commits with the
  writes and references nothing in another store. Before the action runs
  the runner looks for it, and a delivery already handled is marked
  succeeded without running again. An action that writes no store keeps
  none, since running it again changes nothing in a database.
- **A subscriber's tenant.** The tenant its event came from is the
  event's one field holding the tenant's id (`household_id:
  Household.id`). An event of an application with a tenant that holds no
  such field, or more than one, limits its subscribers to no tenant: they
  reach only records every tenant shares. *Flagged:* the pages say the
  subscriber is limited to its event's tenant without saying how an event
  knows it; reading it from the event's own field is what a reviewer
  sees, as `by: User.id` carries who. Recommended as built; the
  alternative, the tenant of the emitting request's records, depends on
  data the event doesn't carry.
- **Retries.** As "Retries and giving up" tables them: a failure of kind
  `unavailable` or `rate_limited`, a fault reaching a store or a run out
  of time is retried; any other failure or defect fails at once. Each
  wait is drawn evenly from zero to a ceiling that starts at 30 seconds
  and doubles with each attempt up to 6 hours (full jitter), and is never
  under a second; a delivery past its attempts, or whose next attempt
  would fall after `within:` from when the event was emitted, fails.
  `retry:` takes `attempts:` from 1 to 100 and `within:` up to 30 days
  (`subscriber.retry_invalid`). A record keeps the failure's code and
  kind, never its fields. `Retry-After` comes with provider calls
  (Phase 7).
- **The sweep.** `runner::sweep` delivers up to 100 deliveries of one
  store that are `pending` past their next attempt or `running` past
  their lease, oldest first: what a crash between the commit and the
  delivery, a host that dropped deferred work, or a worker that died
  holding a lease left. A payload that no longer reads as its type is
  parked as `failed` with its problem (the rule and the field, never the
  value). The minute tick that calls it, and deleting delivered events,
  are chunk 6.2's.
- **Trace context.** A request continues a valid W3C `traceparent` it
  brings, in a span of its own, or starts a trace; an event keeps the
  context of the action that emitted it, and each delivery runs in a new
  span of that trace, so what it emits keeps it too.
- **Idempotency keys.** A route whose action writes no store but emits
  keeps its `Idempotency-Key`s beside its events; one that does neither
  still keeps none (area 7, chunk 4.2).
- **What a subscriber holds.** `on:` an event type (another domain's
  public one, imported), `run:` a call of an action of the subscriber's
  own domain (`subscriber.run_invalid`) whose arguments are `event` or
  fields read through it (`subscriber.argument_invalid`), typed against
  its parameters, and `retry:`; anything else is
  `subscriber.item_invalid`.
- **Not yet.** Delivering in tests after each commit and failing a
  subscriber with a fake (area 13), `jadpo deliveries` and the operator's
  retry and dismiss (area 14), the circuit breaker, publishing outward,
  and events from outside (Phase 7) are not built; event records and
  what a subscriber stores from an event are chunk 6.1c's.

### Chosen in chunk 6.1c

What "Event records" (decided 2026-10-19) and area 9's "Events and
stores" leave to the build, each the most conservative reading:

- **Where an event record is kept.** One per domain that declares an
  event, beside its events: `domains/<domain>/events/events.jadpo_record`,
  a view in the record format (`record events 1`) whose header says it is
  generated from "the events of domain `<domain>`". Every event is
  recorded, not only `public` ones: what is waiting in the outbox when a
  build ships is read with the new shape. There is no command of its own;
  `jadpo records update` writes it, and `jadpo records` lists it under
  that command.
- **What it holds.** An `event <Name>` entry for each event, `public`
  when another domain may subscribe to it, then its fields as its payload
  carries them, in the API record's wire form (`words: Text { max_length:
  200 }`), then an `object`, `enum`, `input` or `output` entry for each
  type a field names, written as the API record writes them. Events,
  fields and types sort by born name. Descriptions are left out, as the
  API record leaves them out.
- **How it is checked.** As the API record is (area 18, "Checking a
  record"): a missing record, one whose events changed (`record.out_of_date`,
  naming each entry added, changed or removed) and the record of a domain
  left with no events are put right by `jadpo records update`; a hand
  edit Jadpo could not have written is `record.edited`. While an event
  file has errors the record isn't compared. Classifying a change as
  breaking, and accepting one, wait for area 18's acceptances with the
  API record's.
- **What a subscriber stores from its event.** Each action's `emit`s, and
  those of the actions it calls, are part of what it passes to its
  callers, so what an event carries is the union of what every `emit` of
  it put there, with what a request sent left out, and along chains of
  events what a subscriber's action emits from the event it was given.
  What the subscriber's action stores from what it is passed is checked
  where the subscriber passes it, as a call is (area 9, "What a write
  stores"), and refused with `policy.output_field_unproved` at the
  argument. What decided an `emit`, and any `reject` in its action, go
  with every field of the event.
- **A value from an event has left its record.** The record may change or
  move between the `emit` and the delivery, so a value an event carries is
  proved as one from any record of its entity: of the field's readers,
  only what holds of anyone signed in, or of a role held across the whole
  application, carries over. The one relation kept is the tenant: when the
  event's tenant field came from the very record a value did, that record
  was the tenant's, and the subscriber is limited to that tenant, so the
  value may be stored wherever that tenant's roles read it. A subscriber
  that needs more passes the event's ids and reads the current state, as
  "Declaring an event" advises.

### Chosen in chunk 6.2

What the page leaves to the build about schedules, each the most
conservative reading:

- **When a schedule runs.** Its state row (`jadpo_schedules`) is made by
  the first tick that sees it, its first run due one `every:` later, and
  each after that one `every:` on, on the same grid in UTC. The row
  outlives a restart or a deploy, so neither moves the grid nor adds a
  run. *Flagged:* "counted in UTC from when the application starts" could
  mean each start counts afresh; that would add a run, or delay one, on
  every deploy. Recommended as built.
- **Missed ticks.** A tick at or past the due time runs it once and moves
  its next due time to the first grid time after the tick, so ticks missed
  while the application was down become one run. A due tick that finds
  the schedule paused, or leased by a run still going, is skipped, not
  queued: recorded with its reason, and its due time moves on the same
  way. A changed `every:` takes effect from the earlier of the kept due
  time and one new interval after the tick.
- **The lease.** Taken in one conditional statement on the state row (the
  due time unchanged, not paused, no live lease), so of two instances
  ticking at once one runs it and the other does nothing. It lasts the
  15-minute limit of a run and a minute; a run that held it past then was
  lost with its worker, and its record ends `failed` with `lease_expired`.
- **A run.** The action, called with nothing, as the system with no
  tenant (Richard, 2026-10-19), in a context of its own whose `clock.now`
  is the tick's time throughout, in a new trace, stopped at 15 minutes.
  Its events are delivered after it on the runner. A failure is recorded
  with its code and kind and not retried. *Flagged:* "the next tick is the
  retry" is read as the next due time; the other reading reruns a failing
  schedule at every minute's tick, which a schedule over a due field
  doesn't need. Recommended as built. Due schedules run side by side, so a
  long run holds no other up.
- **Run records.** `jadpo_schedule_runs`: due, started and ended, outcome
  (`running`, `succeeded`, `failed` or `skipped` with `paused` or
  `lease_held`), the failure's code and kind, never a value, the events
  the run kept once it committed and its `traceparent`. Each state row
  keeps the last tick that reached it. Records are kept 30 days. Items
  processed come with batches (chunk 6.4); a log event and the alerts
  with telemetry and chunk 6.2b.
- **Where the state is.** `schedules: { store: }`, or the only database
  (Richard, 2026-10-19); with several and none named, or none at all,
  each schedule is `schedule.store_required`. `schedules:` holds only
  `store:` for now: a `trigger:` adapter (question 16) comes with the
  Workers cron of chunk 8.2. The two tables are in the migration snapshot
  only when a schedule is declared.
- **`run:` and `every:`.** `run:` is a call of an action of the
  schedule's own domain, with no arguments, taking no parameters
  (`schedule.run_invalid`); `every:` a whole number of seconds, minutes,
  hours or days above zero (`schedule.every_invalid`); anything else in a
  schedule is `schedule.item_invalid`. A schedule reaching what only
  `Access.public` grants is `policy.system_grant_missing`, as a
  subscriber is (area 9, decided 2026-10-19).
- **The tick.** `jadpo_runtime::schedules::tick`, which a host calls once
  a minute, or as often as the shortest `every:` when that is shorter, at
  least once a second: natively the server's timer on its first worker's
  thread at each whole period of UTC (`jadpo dev` ticks; a test's or a
  tool's host doesn't). On Workers the cron trigger is chunk 8.2's and
  calls the same function. *Flagged:* a minute's cron can't run an
  `every:` under a minute more than once a minute; recommended for 8.2: a
  warning on Workers for such a schedule.
- **The runtime's sweeps.** On each tick, in every store holding the
  runtime's tables: `runner::sweep` for deliveries left behind; expired
  idempotency keys deleted (area 7); and an event deleted with its
  delivery records once every delivery succeeded, 24 hours after it was
  emitted (one nothing subscribes to too), so a failed delivery's event
  stays for an operator. Each delete takes 1,000 rows at a time, at most
  ten times a tick. Handled records stay: deleting them waits until a
  delivery can no longer repeat is worked out. Batch runs resume through
  `schedules::HOOKS`, which runs chunk 6.4's `batches::resume`.

### Chosen in chunk 6.4

What the page leaves open about batches, each the most conservative
reading; the file helpers wait for chunk 6.4b:

- **A batch is a page.** Each transaction is one page of `from:`, at the
  size its query's `page:` reads, so 60 items in pages of 3 commit 20
  times. A page type allowing more than the 1,000 items of one batch
  ("Limits and runtime faults") isn't split yet; `PageSize` stops at 100.
- **Where a run is kept.** In the store `from:`'s query reads: one row
  per run in `jadpo_batch_runs` (which call, the shape it was started
  under, its `key:`, what it saved as its types' JSON, the cursor of the
  next page, its counts, its state and its runner's lease), each problem
  in `jadpo_batch_problems`, and each cancel asked for in
  `jadpo_batch_cancels`; only a store some batch walks has them. Starting
  a run writes that store, and `each:`'s and `finished:`'s actions may
  write no other (`store.action_spans_stores`), so an item's writes and
  the position always commit together. A run always has its query's
  store, so it never needs `events: { store: }`.
- **Starting.** The run's row is written in the starting action's
  transaction, and the run is left for after the response at once
  (`Deferred::Batch`); an action that rolls back left no row, so the
  runner finds nothing to run. A start keyed with `key:` that finds the
  same call's run with the same key (by its contract JSON) gives that run,
  in whatever state, whatever else the call was given; a start without
  one is always a new run. A start needs no outbox row: its run's row is
  as durable, and the resume sweep below picks up a start a host
  dropped, as the outbox sweep does a delivery (chunk 6.1b).
- **Saved values.** `each:`'s and `finished:`'s arguments are the item
  (or the summary), a field inside it, or a value that reads nothing of
  it, worked out as the run starts and saved with it; anything that
  works something out of the item is `type.batch_each_invalid`, and is
  the action's to work out. `from:`'s query's arguments are saved as
  what a request carries, an `Input` among them; what `each:` and
  `finished:` capture, and `key:`, as an entity field holds a value, so
  an `Input` there is `type.batch_capture_not_storable`. A secret is
  never saved.
- **Problems.** An item whose action fails is undone alone and becomes a
  problem: its row is its place in the walk, from 1, its column empty
  (a query's item has no columns; the file helpers fill it), its rule the
  failure's code. One whose action faults with a defect or an overflow
  is a problem too, its rule the fault's classification
  (`internal_fault`, `int_overflow`), and is undone alone as a failure
  is. A store that can't be reached, a deadline or another transaction's
  change is no item's: the whole batch is undone; one another
  transaction's change undid runs again, up to three times, and
  otherwise the run waits for the sweep.
- **Finishing.** `finished:` runs once after the last page, in a
  transaction of its own, given the `ImportSummary` with every problem
  in row order, and the run is `finished` in the same transaction. If
  its action fails, what it wrote is undone and the run is `failed`
  with the failure's code. A cancelled run doesn't run `finished:`.
- **Cancelling.** `batches.cancel` keeps a row of its own in the calling
  action's transaction and never touches the run's, so it never waits
  for the batch in hand; the next batch finds it and ends the run
  `cancelled`. Until then `batches.status` says `running`.
- **Status.** `batches.status` reads the counts and the state in the
  calling action's transaction; `estimated_total` is `none` until a
  query's count is built. A `BatchRun` no store keeps is a fault. Both
  find a run by its `id` alone, so they need the application's runs in
  one store: with none, or more than one, they are
  `toolchain.not_built_yet`.
- **Resuming.** A runner holds its run by a lease of two minutes,
  renewed with each page's commit. `batches::resume` picks up every run
  still running whose lease has lapsed, from the position that
  committed: the runtime's minute tick calls it (chunk 6.2). A run whose
  call a deploy changed (its query, its actions or what it saves) ends
  `failed` rather than reading what it saved as something else; listing
  such runs before a deploy is area 14's `jadpo runs unfinished`. A
  runner hands its run on after 500 pages, so one invocation stays within
  the 15 minutes one run has.
- **Who runs it.** Each page's read, its items and `finished:` act as the
  system with no tenant, and read the clock as their batch began; each
  batch begins at repeatable read when `each:`'s or `finished:`'s action
  loads a record, read committed otherwise. *Flagged:* which tenant a run
  an action starts is limited to isn't decided, so `batches.each` in an
  application with a `tenant:` is `toolchain.not_built_yet`; recommended:
  as chunk 6.1b limits a subscriber to its event's one field holding the
  tenant's id, a run is limited to the tenant its one saved value holding
  a tenant's id names (a `from:` argument, a capture or `key:`), and to
  no tenant when none or several do.
  *Flagged too:* a batch whose query or actions reach what only
  `Access.public` grants isn't refused yet; recommended: the same compile
  error as a schedule's or a subscriber's (area 9, 2026-10-19), with
  6.1b's check.
- **Names.** `batches` is a standard namespace, so, as every one, it
  names no variable: area 19's and example 4's `batches` variables were
  renamed. An inline function is now also a named argument's value
  (`each: row -> ...`), as area 3 decided.

### Chosen in chunk 6.4c

What the decisions of 2026-10-19 on a batch's tenant and its grants
leave to the build, each the most conservative reading:

- **A run's tenant.** In an application with a `tenant:`, each page's
  read, its items and `finished:` act as the system limited to the
  tenant the run's saved values holding a tenant's id name: a `from:`
  argument, a value `each:` or `finished:` captures, or `key:`, whose
  own type is the tenant's `id` (`Household.id`, or one that may be
  `none`), as chunk 6.1b reads an event's own field. A tenant's id held
  inside a saved record doesn't count. The run reads them from what it
  saved, so it has the same tenant each time it resumes. A run that
  saves no such value, or one that is `none`, reaches no tenant's
  records, only those every tenant shares. An application without a
  `tenant:` runs batches as before, reaching every record the system
  is granted.
- **Several saved tenant ids.** Read as several tenants: values that all
  name the same tenant (`key: household_id` beside an `each:` capturing
  `household_id`) limit the run to it, and values naming different
  tenants reach no tenant's records. *Flagged:* the decision says "none
  or several"; counting the values instead, so any two reach no tenant,
  would make the usual `key:` beside a capture of the same id reach
  nothing at all. Recommended as built.
- **Batches and `Access.public`.** As for a schedule or a subscriber:
  what `from:`'s query reads, or the action `each:` or `finished:` calls
  reaches, itself or through what it calls, where a grant list, a
  field's own list or the action's line under `actions:` names
  `Access.public` and not `Access.system`, is
  `policy.system_grant_missing`, at that query's or action's call. Each
  call is checked, so one batch may be refused at more than one.

A batch's query that only `Access.public` may read:

```jadpo check project=e-batch-system-grant-query expect=policy.system_grant_missing
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.public]
        update: [Access.system]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

```text output
error: `items` runs as the system, which only `Access.public` lets read `Item`
 --> main.jadpo:46:31
   |
46 |     return batches.each(from: items(page), each: item -> mark_seen(item))
   |                               ^^^^^^^^^^^ runs as `Access.system`
  reason: A batch runs as the system, which an entity grants background work by naming `Access.system`; `Item`'s `read:` names `Access.public` and not `Access.system`, and anyone at all is never how background work is let in.
  next: Add `Access.system` to `Item`'s `read:`, or reach `Item` from a route instead (human)
  rule: policy.system_grant_missing
  help: docs/language/09-policy.html#policy-system-grant-missing

check failed: 1 error, 1 file
```

A batch's action changing what only `Access.public` may change:

```jadpo check project=e-batch-system-grant-action expect=policy.system_grant_missing
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type ItemPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Item {
    id: Uuid
    code: ShelfCode
    seen: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.public, Access.system]
        update: [Access.public]
    }
}

query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action mark_seen(item: Item) -> Unit {
    var seen: Item = Item.update(item) {
        set: {
            seen: true
        }
    }
    assert seen.seen
}

action sweep_shelf(page: ItemPage) -> BatchRun {
    return batches.each(from: items(page), each: item -> mark_seen(item))
}
```

```text output
error: `mark_seen` runs as the system, which only `Access.public` lets update `Item`
 --> main.jadpo:46:58
   |
46 |     return batches.each(from: items(page), each: item -> mark_seen(item))
   |                                                          ^^^^^^^^^^^^^^^ runs as `Access.system`
  reason: A batch runs as the system, which an entity grants background work by naming `Access.system`; `Item`'s `update:` names `Access.public` and not `Access.system`, and anyone at all is never how background work is let in.
  next: Add `Access.system` to `Item`'s `update:`, or reach `Item` from a route instead (human)
  rule: policy.system_grant_missing
  help: docs/language/09-policy.html#policy-system-grant-missing

check failed: 1 error, 1 file
```

### Consistency with the rest of the language

- **Setting in the header.** `job overdue_reminders every 15m {` breaks the
  colon rule (area 3); the schedule form uses `every: 15m`.
- **One-value settings.** `concurrency: singleton` and `retry: next_schedule`
  are required and accept one value each; they become defaults.
- **The time parameter is ritual.** `run: scan(JobRunAt(clock.now))` passes
  the time in, although `clock.now` is already fixed for the whole run.
- **The delivery descriptor restates a query** in a second, closed syntax
  that mixes literal-only values with compiler words, against "one spelling
  per concept".
- **`emit_event(...)`** is a lowercase intrinsic with no owner, beside
  `reject F { }`, and wraps in `attempt` something that cannot fail, which
  area 3 makes an error.
- **`event` as a keyword** contradicts area 4's reasoning for
  `type X = Failure`; the syntax review had kept it, before area 4 decided.
- **Event families** (`TodoEvent.completed`) are enums by another name, with
  per-variant policy that duplicates area 9.
- **`component`** in the candidate design maps modules to owners; area 1
  made domains the owners.
- **The candidate design is very large.** Components, trigger restrictions,
  producer policy, worker-and-origin authority, transition publication,
  lanes and enrollment generations; the
  [proposals](research/12-jobs-and-events-proposals.md) measure it against
  other frameworks.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Scheduling the purge worker (from area 5) | 12 | Answered here: a built-in hourly schedule per entity with `purge:` |
| Retry by kind, dead letters and alerting; `outcome_unknown` never retried blindly (from area 4) | 12 | Answered here: question 7 |
| `type X = Event { ... }` (from area 4) | 12 | Question 3 |
| Calling an action per item inside `for`, and the write syntax the examples leave as comments | 6 | Input to area 6 |
| Who a subscriber acts as; reading event fields under policy | 9 | Depends on question 8 |
| Fixed `clock.now` per run; calendar schedules with a time zone, if ever added | 10 | Input to area 10 |
| The delivery key as an integration's idempotency key; one changing call per action; provider timeouts becoming `outcome_unknown` | 11 | Input to area 11 |
| Running due schedules and delivering pending events in tests | 13 | Input to area 13 |
| The operator's view and commands: list, retry, dismiss and resolve deliveries; alerts | 14 | Input to area 14 |
| `toolchain.target_job_not_implemented` and `toolchain.target_durable_workflow_not_implemented`, raised by `jadpo build` | 14 | Input to area 14 |
| `type.job_delivery_selector_invalid`, `type.job_delivery_service_invalid`, `type.job_delivery_sealed_use`: not reachable by `jadpo check`; removed with the descriptor if question 2 is accepted | 14 | Depends on question 2 |
| `BatchRun`, `BatchProgress`, `RowProblem`, `ImportSummary`, `ExportSummary` | 2 | Question 19 |
| The inline `->` form in the batch helpers' `each:` and `finished:`; no outside call inside `for` | 3 | Questions 19 and 20 |
| Writes after the last call; `clock.now` fixed for a multi-step record's life, on reruns too | 6, 10 | Question 20 |
| Pagination: work over many pages is `batches.each`, not one page per delivery; key derivation and each package's key block; question 26 answered by the warning; `polls:` | 11 | Questions 19, 20 and 22 |
| Fakes answering `lost` and `in_use`; crash points at each step; batches in tests | 13 | Questions 19 and 20 |
| `diverged` and unfinished multi-step runs, batch runs and parked queue messages in `jadpo deliveries`; `jadpo runs unfinished --build <build id>` listing runs a new build changes | 14 | Questions 19 to 21 |
| Multi-step records and batch runs' captured values in the personal-data report | 15 | Questions 19 and 20 |
| Each step's state in `jadpo dev`, `jadpo watch` and `jadpo trace` | 17 | Question 20 |

### Open questions

1. **Is a schedule `schedule name { every: 15m  run: action() }`?** The
   action takes no arguments and reads `clock.now`, fixed for the run;
   `concurrency` and `retry` become defaults (one run at a time, missed ticks
   merged, next tick retries); `every:` gains days. *Recommendation:* yes.
   Runs never overlap: a tick that arrives while a run is going is skipped,
   not queued, and there is no option to allow overlap (Richard asked,
   2026-10-13); work too large for one run emits one event per item.
   Rails' Solid Queue (`schedule: every day at 9am`) and Oban's cron plugin
   also offer calendar times; add them only when an application needs them. *Decided 2026-10-13:* yes, as recommended.
2. **Delete the delivery descriptor?** It is 45 lines of closed syntax for
   one feature, duplicates a query and can never pass checking.
   *Recommendation:* yes, with its 14 syntax and 6 type rules; the batch
   pattern does the same with ordinary code. *Decided 2026-10-13:* yes, as recommended.
3. **Is an event `type TodoCompleted = Event { ... }`, one type per event?**
   Carried from area 4. *Recommendation:* yes, one per file in `events/`, no
   families and no per-event policy. It matches failures, and Laravel and
   Rails also define one class per event or job. See the
   [proposals](research/12-jobs-and-events-proposals.md). *Decided 2026-10-13:* yes, as recommended.
4. **Is emitting `emit TodoCompleted { ... }`, a statement with no
   `attempt`, allowed only in actions and delivered after commit?**
   *Recommendation:* yes. It mirrors `reject`, and areas 3 and 4 already rule
   out `attempt` on something that cannot fail. *Decided 2026-10-13:* yes, as recommended.
5. **Is a subscriber `subscriber name { on: Event  run: action(event) }`,
   one event each?** *Recommendation:* yes. It has the same shape as routes
   and schedules; several events in one subscriber would read as "all of
   them" to some readers and "any of them" to others. *Decided 2026-10-13:* yes, as recommended.
6. **Is delivery order promised?** ASYNC-001 promised order per key for jobs,
   which needs per-key queues and lets one stuck item block the rest.
   *Recommendation:* no order promised. Sidekiq, Laravel and EventBridge
   promise none either; subscribers read current state, which retries need
   anyway. This changes ASYNC-001. *Decided 2026-10-13:* yes, as recommended.
7. **Retries by failure kind, with a default budget?** *Recommendation:* the
   table above: `unavailable`, `rate_limited` and pre-commit faults retry;
   other failures and defects fail at once; `outcome_unknown` is parked.
   Default 10 attempts within 24 hours, changeable per subscriber with
   `retry: { attempts:  within: }`. Sidekiq defaults to 25 retries over
   about 20 days and Oban to 20 attempts; a day suits an outage, and a
   payment webhook can ask for more. *Decided 2026-10-13:* yes, as recommended.
8. **Who does a subscriber act as?** The candidate design checks both a
   worker's and the original user's permissions on every delivery.
   *Recommendation:* the application itself, limited to its own domain; the
   event carries the user's id when the reaction needs it. Rails, Laravel and
   Sidekiq jobs have no user either. Area 9 confirms.
   *Decided 2026-10-13:* replaced. Schedules and subscribers act as
   `Access.system`, a fourth kind of "who" (area 9) that an entity must grant
   explicitly (`update: [owner_id, Access.system]`); a subscriber is limited
   to the tenant its event came from; the permissions report shows what
   background work can touch.
9. **Should lifecycle transitions emit events automatically?** The candidate
   design publishes facts from transitions so no write path can skip them.
   *Recommendation:* no for now: emit in the action that makes the change,
   one way to do it. Revisit if reviews find a missed emit. *Decided 2026-10-13:* yes, as recommended.
10. **Can an event be delivered later (`in 24h`)?** Sidekiq, Laravel and
    Inngest can delay jobs. *Recommendation:* not now. A schedule querying a
    due field covers it, rereads current state, and keeps one way to wait. *Decided 2026-10-13:* yes, as recommended.
11. **May an action call more than one integration operation that changes
    something?** *Recommendation:* no, a compile error. After a lost reply
    the runtime must know which single call to ask about; batch work emits one
    event per item instead. Inngest makes each outside call its own step for
    the same reason. *Decided 2026-10-13:* yes, as recommended. *Reason
    corrected 2026-10-15 (Richard):* the runtime stops at the first call
    whose outcome is unknown, so even several calls would leave only one in
    doubt. The rule's purpose is that an action repeats as a unit, and that
    no second call can fail for good after the first took effect (see
    "Delivery: at least once, safe to repeat").
    *Revised 2026-10-16 (Richard):* replaced by "Several changing calls in
    one action" (question 20). Calls without idempotency keys stay one per
    action; keyed calls may be up to 8 in order, each recorded and replayed
    on a rerun.
12. **Are the limits right?** 1,000 events per action, 64 KB per event,
    chains 16 deep, 15 minutes per run. *Recommendation:* adopt them as
    defaults and confirm with measured runs, as ASYNC-001 requires. *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* paging no longer chains events, so the
    16-deep limit stops only loops; batches (question 19) hold up to 1,000
    items or 4 MiB of file each, and an action makes at most 8 changing
    calls (question 20), both confirmed by measurement.
13. **Do large exports need a consistent snapshot read?** (carried from area
    6) A schedule paging through a query for an export or report sees
    records change between pages. Should it read one consistent snapshot,
    and how is that declared? *Decided 2026-10-13:* yes, as recommended.
    *Revised 2026-10-16 (Richard):* an export is a batch (question 19)
    walking keyset pages of live data, and `jadpo check` warns when its
    `from:` orders by a field that can be updated.
14. **What carries on after a route answers `status: SuccessStatus.accepted`?**
    (carried from area 7) Area 7 has a route answer 202 and continue the work
    in the background. Is that always an event and a subscriber, and how
    does the client learn the outcome? *Decided 2026-10-13:* yes, as recommended.
15. **How do tests run schedules and subscribers?** (carried from area 13)
    Area 13 calls `overdue_reminders.run()`, relies on subscriber delivery
    order and needs retries of a failing subscriber. Does this page define
    those, and in what order are subscribers delivered in tests?
    *Decided 2026-10-13:* `schedule.run()` runs a schedule under the test
    clock; events are delivered after each commit; a fake can fail a
    subscriber to test retries; delivery order is shuffled with a printed
    seed, so a test that depends on order fails visibly.
16. **Where does a schedule's tick come from on each host?** Raised by
    Richard (2026-10-13) with the move to native Rust and WebAssembly
    targets. Cloudflare's cron triggers can fire more than once and do not
    wait for a running invocation; Durable Objects guarantee one alarm at a
    time but exist only on Cloudflare. *Recommendation:* split the *tick*
    from the *state*. The state always lives in the application's main
    store: a generated table holding each schedule's next due time and a
    lease (`running_until`). On every tick the runtime takes due schedules
    whose lease is free in one conditional update, runs them, and releases
    the lease; a tick that finds the lease taken skips, and a crashed run's
    lease expires after the 15-minute limit. The tick is pluggable,
    `schedules: { trigger: ScheduleTrigger.native }` (default: an in-process
    timer) or `ScheduleTrigger.external` (the host calls a generated tick
    once a minute: a Cloudflare cron trigger, a Kubernetes CronJob, AWS
    EventBridge Scheduler). One external entry per application, whatever the
    number of schedules, so `every: 7m` works where cron syntax cannot and
    Cloudflare's per-account cron limit is never reached. Duplicate ticks
    are harmless because the lease decides. Durable Objects stay a possible
    later optimisation, not the design.
    *Richard (2026-10-13):* design it as adapters, built for many, chosen in
    configuration. *Decided 2026-10-13 (Richard's requirements):*
    - a schedule adapter supplies the tick and may supply the lease; the
      default lease lives in the main database; adapters are many and named
      in `app.jadpo` (`ScheduleTrigger.native` by default; external ticks
      such as Cloudflare cron, Kubernetes CronJob or AWS EventBridge; a
      Durable Objects adapter that also holds the lease later), and an
      adapter may generate its host's configuration;
    - declaring a schedule (or an event, which needs the outbox) with no
      store the chosen adapter can keep state in is a compile error that
      says which store to add;
    - the generated system tables (schedule state, the event outbox,
      delivery records, idempotency keys, sessions) are part of the
      migration snapshots like any entity, prefixed `jadpo_`, so
      `jadpo migrate new` creates and changes them and `jadpo check` reports
      drift in them too.

    *Superseded 2026-10-19* by "Outbox placement, settled" (Decisions):
    a schedule's lease goes in `schedules: { store: }`, not always the
    main store, and each store an action writes keeps its own outbox.
17. **Can a request's whole effect be traced through events?** Raised by
    Richard (2026-10-13). *Recommendation:* yes, always on. An event is
    stored with the trace context of the action that emitted it (W3C
    `traceparent`, in the same transaction), and each subscriber run is a
    child of it, so route → action → events → subscribers → further events
    → provider calls form one trace. Schedules start a trace per run;
    provider webhooks start one linked to the provider's event id; calls
    between Jadpo applications pass the context on. Each event also records
    what caused it (request id or parent event), which the 16-deep chain
    limit already needs. `jadpo trace <request id>` and the console (area 17)
    show the whole tree; area 16's OpenTelemetry export carries it. *Decided 2026-10-13:* yes, as recommended.
18. **Should deliveries pause when a provider keeps failing?** Jitter stops
    retries arriving together, but 5,000 deliveries all calling a provider
    that is down still retry 10 times each. *Recommendation:* a circuit
    breaker per connection: after a run of `unavailable` failures from one
    provider, deliveries that call it are held rather than attempted, one
    probe delivery is tried after a pause, and success releases the rest.
    Held deliveries do not spend their retry budget. Shown in
    `jadpo deliveries` and alerted. Resilience4j and Polly offer the same
    pattern; built in, nobody has to remember it. *Decided 2026-10-13:* yes, as recommended.
19. **How is work over many items done: a file's rows, a large query,
    every page of a provider's list?** *Decided 2026-10-16 (Richard):* by
    batches, the one way, through explicit helpers per format and
    direction (`csv.read` and `csv.write`, `tsv`, `json_lines`, `json.read`
    and `json.write`) and `batches.each` for work with no file. Each starts
    after the calling action commits, returns a run (`batches.status`,
    `batches.cancel`, `key:` for a repeated start), groups items into
    batches each committing its writes and position together, keeps file
    order, and turns bad rows and failed items into problems without
    values while the rest carry on. Rows read are `Input`s and rows written
    `Output`s; writing CSV names its reader; encodings are declared, never
    guessed; gzip only with `max_unpacked_size:`. No loop over a file's
    rows and no whole-file parse into a list. It replaces chained events
    for paging and settles area 11's pagination.
20. **May an action make several changing calls?** *Decided 2026-10-16
    (Richard):* yes, as "Several changing calls in one action" says, with
    the review's ten fixes and its answers: a route without a required
    `Idempotency-Key` starting one is refused, not warned; 8 steps is the
    limit for now; an unkeyed call is never allowed as a last step;
    timestamps after a resumed action are from when it started; a diverged
    action is held, not failed; crash-point test runs are on by default;
    before a deploy, `jadpo runs unfinished --build <build id>` lists the
    unfinished runs whose path the new build changes (Jadpo has no deploy
    command), and a run resumed under a changed build stops as `diverged`;
    and area 11's question 26 closes with
    the warning.
21. **Can the app read a queue directly?** *Decided 2026-10-16 (Richard):*
    yes, through the `receiving` capability: a queue names its type and id
    fields and the events it may carry; each message becomes its event,
    acknowledged after the subscriber's transaction commits, repeats
    dropped by message id; unlisted types and messages that keep failing
    are parked for an operator.
22. **Do packages produce events?** *Decided 2026-10-16 (Richard):* only
    about the outside world: webhooks, events their own subscribers emit
    from them, and polls, never as a side effect of the app calling them.
    Only `public` package events can be subscribed to; a poll runs only
    while something subscribes, emits only changes, and is for feeds that
    do not depend on the app's data.

### Decisions

Nothing is decided on this page yet.

| Topic | Decision | Date |
|---|---|---|
| Triggers | Carried from area 1: routes, schedules (replacing "jobs") and subscribers each call actions; durable workflows are subscribers | 2026-10-12 |
| Keyword and folders | Carried from area 1: `schedule` in `schedules/`; events in `events/`; subscribers in `subscribers/` | 2026-10-12 |
| Cross-domain changes | Carried from area 1: public actions inside the caller's transaction, or events for reactions that need not be all-or-nothing | 2026-10-12 |
| Actions | Carried from area 3: every action is all-or-nothing and has no settings; `attempt` only on calls that can fail | 2026-10-12 |
| Faults | Carried from area 4: faults are never listed in `fails` or matched; integrations map provider problems to declared failures | 2026-10-12 |
| Schedule form | `schedule name { every: 15m  run: action() }`; runs never overlap, a busy tick is skipped; the next tick retries; calendar times later | 2026-10-13 |
| Delivery descriptor | Deleted with its 14 syntax and 6 type rules | 2026-10-13 |
| Event declaration | `type X = Event { ... }`, one per file in `events/` | 2026-10-13 |
| Emitting | `emit X { ... }` in actions only, delivered after commit, no `attempt` | 2026-10-13 |
| Subscriber form | `subscriber name { on: Event  run: action(event) }`, one event each | 2026-10-13 |
| Ordering | No order promised; reverses ASYNC-001 (2026-10-04) | 2026-10-13 |
| Retries | By failure kind; default 10 attempts within 24 hours; `retry: { attempts: within: }`; exponential backoff with full jitter; `Retry-After` honoured | 2026-10-13 |
| Subscriber identity | `Access.system`, granted explicitly by entities; limited to the event's tenant | 2026-10-13 |
| Automatic events from transitions | No; the action that makes the change emits | 2026-10-13 |
| Delayed delivery | No; a schedule over a due field instead | 2026-10-13 |
| One changing integration call per action | Yes, a compile error otherwise (Superseded 2026-10-16: several keyed changing calls are allowed; see "Several changing calls"; unkeyed calls stay one per action) | 2026-10-13 |
| Why one changing call | Corrected (Richard): every action repeats as a unit, a rerun repeating one keyed call, and no second call can fail for good after the first took effect; not "knowing which call to ask about", since the runtime stops at the first unknown outcome anyway | 2026-10-15 |
| Limits | 1,000 events per action, 64 KB per event, chains 16 deep, 15 minutes per run, confirmed by measurement | 2026-10-13 |
| Snapshot reads for exports | None; keyset pages; area 15's export reads in one transaction | 2026-10-13 |
| Work after an accepted response | An event and subscriber; the client reads a record with a status field | 2026-10-13 |
| Schedules and subscribers in tests | `schedule.run()`, delivery after commit, fakes fail subscribers, shuffled order with a printed seed | 2026-10-13 |
| Schedule ticks on every host | Schedule adapters, many and chosen in `app.jadpo`: an adapter supplies the tick and may supply the lease (default: main database); a schedule or event with no store for its state is a compile error; system tables (`jadpo_` prefix) are in migration snapshots (Richard) | 2026-10-13 |
| Tracing through events | Always on: events carry the emitting action's trace context; subscriber runs continue it; `jadpo trace <request id>` | 2026-10-13 |
| Schedule monitoring | Every run and tick recorded; `jadpo schedules` (list, run, pause, resume, history); `jadpo why <schedule>`; metrics; alerts with no setup (no success in twice the interval, ticks stopped, near the time limit, three failures, stuck lease); console timeline (Richard) | 2026-10-13 |
| Circuit breaker | Per connection: after repeated `unavailable` failures, deliveries calling the provider are held, one probe after a pause, success releases the rest; held deliveries keep their retry budget; shown and alerted | 2026-10-13 |
| Event transport | Internal only: the main store's outbox delivered by the runtime (on Workers after commit via `waitUntil`, swept by the minute tick); external queues only receive published copies of chosen public events (Richard) | 2026-10-13 |
| Outbox placement | No default store: the outbox, delivery records and idempotency keys go in each store an action writes; an action writing no store needs `events: { store: <connection> }`, or it is a compile error (Richard, from the architecture plan) | 2026-10-13 |
| Batches | The one way to work over many items: `csv.read`/`write`, `tsv`, `json_lines`, `json.read(file, items_at:, as:)`/`write` and `batches.each(from:, each:, finished:, key:)`; inline `->` for `each:` and `finished:`; started after commit, returning a run with `batches.status` and `batches.cancel`; `key:` for repeated starts; batches committing writes and position together; file order kept; problems without values; only storable captures; replaces chained events for paging (Richard) | 2026-10-16 |
| Rows and formats | Rows read are `Input`s and written `Output`s with `column:` and `wire:`; `CsvFormat.spreadsheet` or `.standard` required on write; UTF-8, UTF-16 with a mark and declared legacy encodings read, UTF-8 written; gzip only with `max_unpacked_size:`; no loop over rows and no whole-file parse (Richard) | 2026-10-16 |
| Several changing calls | Up to 8 keyed changing calls per action, only from subscribers and routes requiring `Idempotency-Key`; no outside call inside `for` in any action, a loop of calls being `batches.each`; each step recorded before sending (operation, request fingerprint, time) and its answer after, encrypted, a fault over 1 MB (`large_output: true` operations refused); `store.put` a keyed step; reruns replay and check the whole path, stopping as `diverged` for an operator; `clock.now` fixed per record; keys hashed from app, environment, connection, caller scope, action key, step and generation; steps in doubt resent only within the provider's window; one runner per record; the "ends for good after an effect" warning; unfinished records become operator items with retry and abandon, kept 30 days; records deleted in the commit, listed in the personal-data report; `jadpo runs unfinished --build <build id>` lists, before a deploy, unfinished runs whose path the new build changes (Richard, from the multi-step calls review) | 2026-10-16 |
| Events from outside | Webhooks, a package's `public` events and polls, and queue messages through `receiving`, each written to the outbox and delivered like the app's own; unlisted queue message types and repeatedly failing messages parked, never dropped (Richard) | 2026-10-16 |
| Settled for the build | An item of `batches.each` whose action makes outside calls is a batch of its own, keyed from the run and its position; `store.put` a keyed step; a recorded answer over 1 MB is a fault naming the step, and `large_output: true` operations are refused in multi-step actions; the batch types' fields are area 2's (decided for Richard) | 2026-10-16 |
| Outbox placement, settled (Phase 5 review) | Each store an action writes keeps its outbox and the delivery records beside it, as the earlier "Outbox placement" row says; "Where events travel" and "Event transport" are read that way. A subscriber's handled record goes in the store it writes, with no reference across stores. Schedule leases go in `schedules: { store: }` (area 5), defaulting to the only store when there is one (Richard, as recommended) | 2026-10-19 |
| Event records (Phase 5 review) | An event's shape is recorded and diffed in the generated record exactly as an API's is (chunk 7.0's format); accepting a breaking change waits for area 18's acceptances together with the API record's (Richard, as recommended) | 2026-10-19 |
| A schedule and tenants (after chunk 6.1a) | A schedule's run, which no tenant's event starts, reaches every tenant's records, as the system caller does with no tenant; a subscriber stays limited to its event's tenant (Richard, as recommended) | 2026-10-19 |
| An event's tenant (after chunk 6.1b) | An event belongs to the tenant its one field holding a tenant's id names (e.g. `household_id: Household.id`), so the tenant is written in the event where a reviewer sees it; an event with no such field, or more than one, reaches no tenant's records (Richard, as recommended) | 2026-10-19 |
| A batch's tenant (after chunk 6.4) | A batch run belongs to the tenant its one saved value holding a tenant's id names, as an event does; none or several reach no tenant's records (Richard, as recommended) | 2026-10-19 |
