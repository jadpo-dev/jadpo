---
title: Jobs and events
area: 12
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - event.*
  - receiving.*
  - schedule.*
  - step.*
  - subscriber.*
  - type.batch_*
---

# Jobs and events

Work that does not happen inside a request: schedules that run an action on a
timer, events an action announces, subscribers that react to them, and what
happens when that work fails.

> Decided on 2026-10-13. Code on this page applies the decisions from areas 1 to 12. Built in Phase 6, chunks 6.1a to 6.10; what is built and what isn't is under "Review", "Status".

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
// domains/planning/schedules/chore_census.jadpo
/// Queues a reminder for each overdue todo.
schedule chore_census {
    every: 15m
    run: queue_chore_census()
}
```

- `every:` takes a whole number of seconds, minutes, hours or days above
  zero (`30s`, `15m`, `1h`, `1d`; `schedule.every_invalid`). `run:` is a
  call of an action of the schedule's own domain, with no arguments,
  taking no parameters (`schedule.run_invalid`). Anything else in a
  schedule is `schedule.item_invalid`.
- Runs fall on a grid in UTC that starts at the first tick that sees the
  schedule: its first run is due one `every:` later, and each after that
  one `every:` on. The grid is kept in the schedule's state row, so a
  restart or a deploy neither moves it nor adds a run (decided
  2026-10-19). A changed `every:` takes effect from the earlier of the
  kept due time and one new interval after the tick.
- Only one run of a schedule happens at a time. Ticks missed while the
  application was down, or while a run was still going, become one run,
  not a backlog: a tick at or past the due time runs it once and moves its
  next due time to the first grid time after the tick, and a due tick
  that finds the schedule paused, or leased by a run still going, is
  skipped, not queued, recorded with its reason, its due time moving on
  the same way.
- Each schedule's state and lease live in the store
  `schedules: { store: }` names (area 5), or the only database when there
  is one; with several and none named, or none at all, each schedule is
  `schedule.store_required`. `schedules:` also holds `trigger:`, what
  ticks a native deployable ("The tick"; question 16, built in chunk
  8.2g). The state and run tables are in the migration snapshot only when a schedule
  is declared or an entity purges.
- Inside the run, `clock.now` is fixed: every read gives the time the run
  started, the tick's, so the action does not need the time passed in.
- A failed run is logged and shown to the operator, and retried at its
  next due time, not at every minute's tick (decided 2026-10-19).
  Schedules have no retry settings.
- Entities with `purge:` in their lifecycle (area 5) get a built-in hourly
  purge schedule from the compiler, `Member.purge` for `Member`; nothing is
  written for it ("The purge schedule", below).

### How a schedule runs

- **The lease.** Taken in one conditional statement on the state row (the
  due time unchanged, not paused, no live lease), so of two instances
  ticking at once one runs it and the other does nothing. It lasts what
  is left of the tick's budget and a minute ("Running background work"),
  at most 16 minutes natively; a run that held it past then was lost with
  its worker, and its record ends `failed` with `lease_expired`.
- **A run.** The action, called with nothing, as the system reaching
  every tenant ("Background work and tenants"), in a context of its own
  whose `clock.now` is the tick's time throughout, in a new trace. It
  runs within the 15-minute limit of a run and its tick's budget; a run
  so stopped is recorded failed with `deadline_exceeded`, its lease given
  back. What it emits is delivered after it. A failure is recorded with
  its code and kind. Due schedules run side by side, so a long run holds
  no other up.
- **Grants.** A schedule reaching what only `Access.public` grants is
  `policy.system_grant_missing`, as a subscriber or a batch is (area 9,
  "Background work").

### The tick

`jadpo_runtime::schedules::tick` is the one entry point every host calls:
once a minute, or as often as the shortest `every:` when that is shorter,
at least once a second. Natively it is the server's timer on its first
worker's thread, at each whole period of UTC, unless `schedules: {
trigger: ScheduleTrigger.external }` leaves it to another scheduler
(below); `jadpo dev` ticks on its timer whichever is written, and a
test's or a tool's host doesn't. On Workers it is the minute's cron
trigger, which `jadpo build` writes into the Wrangler file (built in
chunk 8.2f): each run ticks, and while the app receives a queue ticks
again at each 10-second mark of its minute, so a queue is read as often
as natively. There an `every:` under a minute is an error naming the
schedule, since a minute's cron can't run it (decided 2026-10-19;
`deployable.target_unsupported`, area 14). Each state row keeps the last
tick that reached it.

`trigger:` names what ticks a native deployable (question 16, built in
chunk 8.2g): `ScheduleTrigger.native`, the server's own timer, unless it
is written, or `ScheduleTrigger.external`, another scheduler (a
Kubernetes CronJob, an ECS scheduled task, a systemd timer) running the
deployable's `tick` command, `<deployable> tick`, which reads its
settings and compares its stores' migrations as serving does, runs one
tick and ends (area 14, "A native deployable"). The server then never
ticks. There is no HTTP endpoint and no tick secret: a scheduler that can
only call a URL needs a wrapper of its own (Richard, 2026-10-21). A
Worker always ticks on its cron, whichever is written. The scheduler runs
`tick` as often as the server's timer would tick, which `jadpo build`
says beside the binary; one running it less often runs schedules and
reads queues late, which the ticks-stopped alert says ("Watching
schedules"). Two ticks at once, or a `tick` beside a server's timer, are
harmless, since the lease decides.

```jadpo check project=external-tick file=app.jadpo
application {
    schedules: {
        trigger: ScheduleTrigger.external
    }
}
```

Then, each step starting only while the tick's budget lasts, the tick:

- runs the schedules due, and any run asked for by hand ("Running a
  schedule by hand"), and delivers what they emit;
- in every store holding the runtime's tables, delivers what is left to
  deliver (the outbox's sweep: up to 100 deliveries `pending` past their
  next attempt or `running` past their lease, oldest first, which a
  crash between the commit and the delivery, a host that dropped work,
  or a worker that died holding a lease left);
- deletes expired idempotency keys (area 7); an event with its delivery
  records once every delivery succeeded, 24 hours after it was emitted
  (one nothing subscribes to too), so a failed delivery's event stays for
  an operator; a delivery an operator dismissed or cancelled, 30 days
  after the operator's last action on it ("What an operator sees"); the
  handled records of those deliveries; browser
  sessions and API tokens that have ended and service keys that have
  expired (area 8), which a lookup already ignores, in the store each is
  kept in, beside its principal's users or services; schedule run
  records and ended batch runs 30 days old ("How a run is kept"); and
  an operator's log rows 30 days old whose delivery or run is gone;
- picks up batch runs whose lease lapsed.

Retention cleanup runs after the prompt work (due schedules, delivery sweeps,
queues, polls, alerts and expired batch leases), at most hourly per store
once a pass completes (chunk 9.1b2). Each store's `jadpo_housekeeping`
lease admits one instance; stores holding only schedule state or AI usage
are included. Its cleanup holdings come from the whole application schema,
so a deployable with no schedule or batch runners still cleans their history
without keeping their executable bodies. A failed, capped or interrupted
pass stays eligible on the next tick. Completion and release are fenced by the owner's identity; a
crashed owner's lease expires after the remaining tick budget and the
usual minute's margin. Every batch checks the budget and shutdown before
starting. The retention windows and deletion ordering below are unchanged.

A handled record goes only once its delivery can no longer repeat, which
is once the delivery's own record is gone: with its event deleted,
nothing redelivers it, whatever an operator does later. In the event's
store, the statement that deletes the event deletes the handled records
kept there of its deliveries, so a single-store application never holds
one past its event. A subscriber that writes another store keeps its
record there, and the tick deletes it by the delivery's `id` once the
event's statement has committed; a store it can't reach then keeps those
records, which only cost space, and never loses one early (decided in
chunk 6.10).

What an operator settled is kept as long as a run record, then goes
(decided in chunk 6.13). A delivery `dismissed` or `cancelled` never
runs again, so the tick deletes it 30 days after the last action logged
on it, with its handled records as above; no other state is deleted
while its event stays, so a delivery that could still run (`pending`,
`running`, `failed`, `outcome_unknown`) is never deleted, however old.
An event goes once every delivery of it left succeeded: one with a
delivery that could run, or one set aside within the 30 days, stays, so
`jadpo deliveries` shows that delivery with its event. The tick deletes
those deliveries before it deletes events, so an event whose last such
delivery goes is deleted on the same tick. Its time is the log's, not a
column of the delivery's: each action writes its log row with its
change, and a log row stays while its delivery does, so the window
counts from the operator's last action without changing the delivery's
table.

Each delete takes 1,000 rows at a time, at most ten times a tick: for
events, 1,000 events, with their delivery and handled records; for
deliveries an operator settled, 1,000 deliveries, with their handled
records. The tick
hands nothing on: what it doesn't reach, the next tick carries on.

### The purge schedule

Each entity whose lifecycle has `purge:`, and a store, gets its own
schedule, as the decision table says ("a built-in hourly schedule per
entity"): named after the entity and its operation (`Member.purge`, as
`Member.delete` is) and keyed by the entity's born name, so a rename keeps
its state and run records. Its `.jadpo` place, which `jadpo why` gives,
is the lifecycle's `purge:`. It runs every hour on any schedule's grid,
and is recorded and watched like any schedule: its state row, lease and
run records are a schedule's, in `schedules: { store: }` or the only
database; with several databases and none named, the lifecycle's
`purge:` is `schedule.store_required`, suggesting the entity's own store,
and `migration.out_of_date` waits until it is named. `jadpo schedules`
lists it, `history`, `pause`, `resume`, `run` and `jadpo why` take its
name, and `jadpo dev` prints each run.

A run is the runtime's purge entry point (area 5) at the tick's time,
outside every action and policy: at most 20 batches of 500, within the
run's limit, what it leaves carried on by the next hour's run. The period
is its `after:`, or the value the application started with for the
setting it reads. It emits nothing. A store that can't be reached is a
failed run with `unavailable`, as for an action; anything else the store
refuses is `internal_fault`, the batches before it staying deleted. Its
run records count the records it deleted (`items`): the screens show
`2 records purged` where an action's run shows its events, and
`jadpo dev`'s `--json` line carries `items` and `batches`, how many each
batch deleted. No test runs it (area 13).

## Batch work keeps its progress

An action is all-or-nothing (area 3). If one action emailed 500 owners and
failed on the 300th, its database writes would roll back but 299 emails would
already be sent. So a schedule that covers many items does not do the work
itself: it finds the items and emits one event per item. Each item is then
handled by its own action, retried on its own, and one bad item never blocks
or undoes the rest (question 11).

```jadpo fragment
// domains/planning/actions/queue_chore_census.jadpo
action queue_chore_census() -> Unit {
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
        each: product -> ProductExportRow {
            sku: product.sku
            name: product.name
            price: product.price
        },
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
  import a file twice; in an application with a tenant, the same key in
  two tenants starts two runs.
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
runs proceed side by side. The run's state lives in the store `from:`'s
query reads (decided 2026-10-19, "How a run is kept"). Each item's
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
record changed during it appears as its page saw it; a `from:` that
orders by a field that can be updated is a declinable error
([`type.batch_order_updatable`](#type-batch-order-updatable), area 1's
"Declining a judgement call"; Jadpo has no warnings), since ordering by
a field no update changes (`id`, or a creation time with `id`) is what
keeps every record in exactly once.

Batches replace chained events for paging: an event per page would stop
at the 16-deep chain limit ("Limits and runtime faults") after 16 pages.
This also settles area 11's pagination, where work over many pages ran one
page per subscriber delivery.

> Built in chunk 6.4: `batches.each` over a named query's pages, with
> `finished:` and `key:`, `batches.status` and `batches.cancel`, each page
> one transaction committing its items' writes and the run's position,
> and a run resumed after a crash or a deploy ("How a run is kept",
> below). Built in chunk 6.4b, with the `files` capability's first part
> (area 16): `csv.read`, `tsv.read` and `json_lines.read`, `column:` and
> a file row's problems by its row and column ("How a file's run is
> kept", below). Built in chunk 6.4b2: the writers, `csv.write`,
> `tsv.write`, `json_lines.write` and `json.write`'s file form, with
> `CsvFormat` and `ExportSummary` ("How an export's run is kept",
> below). Built in chunk 6.4b3: `json.read`'s file form and gzip read
> with `max_unpacked_size:` ("How a file's run is kept"), the declinable
> error for a `from:` ordered by a field that can be updated, and
> `estimated_total` ("How a run is kept"). Built in chunk 7.4b: an item
> whose `each:` action makes outside calls a batch of its own, and
> `finished:` the same ("How a run is kept").

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

/// The items, a page at a time, in an order no change to an item moves.
query items(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.id asc]
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

### How a run is kept

- **A batch is a page.** Each transaction is one page of `from:`, at the
  size its query's `page:` reads, so 60 items in pages of 3 commit 20
  times. A page type allowing more than the 1,000 items of one batch
  ("Limits and runtime faults") isn't split yet; `PageSize` stops at 100.
- **Where a run is kept.** In the store `from:`'s query reads (decided
  2026-10-19): one row per run in `jadpo_batch_runs` (which call, the
  shape it was started under, its `key:`, what it saved, the tenant it is
  limited to, how deep in a chain of events it started, the cursor of the
  next page, its counts, its state and its runner's lease), each problem
  in `jadpo_batch_problems`, and each cancel asked for in
  `jadpo_batch_cancels`; only a store some batch walks has them. Starting
  a run writes that store, and `each:`'s and `finished:`'s actions may
  write no other (`store.action_spans_stores`), so an item's writes and
  the position always commit together. A run always has its query's
  store, so it never needs `events: { store: }`.
- **Starting.** The run's row is written in the starting action's
  transaction, and the run left for after the response at once; an action
  that rolls back left no row, so nothing runs. A start needs no outbox
  row: its run's row is as durable, and the tick picks up a start a host
  dropped, as the outbox's sweep does a delivery. A start keyed with
  `key:` that finds the same call's run with the same key (by its
  contract JSON), in the same tenant, gives that run, in whatever state,
  whatever else the call was given; a start without one is always a new
  run. In an application with a tenant, `key:` is unique per call, the
  run's tenant and the key, so two tenants' runs with the same key never
  meet (decided 2026-10-19); the row keeps the tenant (all zeros when it
  reaches no tenant's records or the application has none). A run kept
  before the tenant was kept has none, and no later start finds it, since
  nothing says which tenant its key was given in: a keyed run still going
  when that migration is deployed may be started again by a retry, rather
  than one tenant being handed another's run.
- **Saved values.** `each:`'s and `finished:`'s arguments are the item
  (or the summary), a field inside it, or a value that reads nothing of
  it, worked out as the run starts and saved with it; anything that works
  something out of the item is `type.batch_each_invalid`, and is the
  action's to work out. `from:`'s query's arguments are saved as what a
  request carries, an `Input` among them; what `each:` and `finished:`
  capture, and `key:`, as an entity field holds a value, so an `Input`
  there is `type.batch_capture_not_storable`. A secret is never saved.
  The values are a JSON array of each value's own JSON (its type's
  contract JSON), `null` for none, kept with the encoding they were
  written in (`jadpo_batch_runs.encoding`) and read with the standard JSON
  reader, a number keeping the digits it was written with.
- **Which call, by a stable name.** A run names the `batches.each` call
  that started it by what the call is, in identity keys that survive a
  rename: the action starting it, the query `from:` walks and the action
  `each:` runs (`action check_stock from query products_to_check each
  action check_product`), so adding or moving another call leaves its
  name as it was; a second call of the same three in one action adds its
  place among those alone (` #2`). Its shape, what it saves and calls, is
  kept as canonical type text, naming declared types by their identity
  keys, with a digest, so a release that changes the compiler's own
  internals leaves every running batch's shape as it was.
- **Problems.** An item whose action fails is undone alone and becomes a
  problem: its row is its place in the walk, from 1, its column empty (a
  query's item has no columns; the file helpers fill it), its rule the
  failure's code. One whose action faults with a defect or an overflow is
  a problem too, its rule the fault's classification (`internal_fault`,
  `int_overflow`), and is undone alone as a failure is. A store that
  can't be reached, a deadline or another transaction's change is no
  item's: the whole page is undone; one another transaction's change
  undid runs again, up to three times, and otherwise the run waits for
  the tick.
- **Items that make outside calls** (chosen in chunk 7.4b). When
  `each:`'s action makes outside calls, itself or through an action it
  calls, each item is a batch of its own, as "How a run goes" decides, so
  no call goes out while a transaction is open (area 6). The run's place
  is read in a statement of its own, locking nothing; the item is read in
  a snapshot (a query's page of one item, or a file's next row) that its
  first call closes; its action runs as the system, as a subscriber's
  delivery does; then, in the transaction its writes are in, the run's row
  is locked and held to the position the item was read at, and the item's
  problem, if it failed, and the run's next position commit with its
  writes. A query's run is then at the item's own cursor, a file's after
  its row, so a run resumed after a crash meets the same item at the same
  place. Each call's key is derived from the run and that place (the
  item's cursor, or where its row begins in the file), so the repeated
  item's calls go out under the keys they were first sent with. A
  concurrent change runs the item again, up to three times, as it would a
  page. A `finished:` whose action makes outside calls runs the same way,
  its calls keyed from the run, and the run's end commits with its
  writes. An unkeyed call that may have acted (`outcome_unknown`) is the
  item's problem, never sent again; one lost with its runner waits for
  multi-step calls' step record, as a subscriber's does.
- **Who runs it.** Each page's read, its items and `finished:` act as the
  system ("Background work and tenants"), and read the clock as their page
  began; each page begins at repeatable read when `each:`'s or
  `finished:`'s action loads a record, read committed otherwise. A run
  keeps how deep in a chain of events the action that started it was, and
  its items run at that depth, so what they emit is one deeper than the
  event that started the run, and a loop through a batch stops at 16 as
  any loop does. Each item's action, and `finished:`'s, has the allowance
  of one action's events to itself, 1,000, however many the items before
  it in the page emitted.
- **Finishing.** `finished:` runs once after the last page, in a
  transaction of its own, given the `ImportSummary` with every problem in
  row order, and the run is `finished` in the same transaction. If its
  action fails, what it wrote is undone and the run is `failed` with the
  failure's code. A cancelled run doesn't run `finished:`.
- **Cancelling.** `batches.cancel` keeps a row of its own in the calling
  action's transaction and never touches the run's, so it never waits for
  the page in hand; the next page finds it and ends the run `cancelled`.
  Until then `batches.status` says `running`.
- **Status.** `batches.status` reads the counts and the state in the
  calling action's transaction. A `BatchRun` no store keeps is a
  fault. Both find a run by its `id` alone, so they need the
  application's runs in one store: with none, or more than one, they are
  `toolchain.not_built_yet`.
- **The estimated total** (chosen in chunk 6.4b3). A query's run counts
  its query as its first page is read, in that page's transaction, as
  the system limited to the run's tenant: the query's statement with no
  page's limit or cursor, so the count is of what the walk would meet if
  nothing changed; later pages keep the count it was given. A file's run
  works its estimate out with each page: the items done so far, scaled
  by the file's size over how far into the file they end, and never
  fewer than are done; for a gzip file the size is what it is unpacking
  to at the rate it has so far. `estimated_total` is `none` until then,
  the items done once the run has finished, and the last estimate once
  it failed or was cancelled. It is kept in
  `jadpo_batch_runs.estimated_total`. A count the store refuses leaves
  the run without one rather than stopping it, and a query whose records
  Jadpo checks again after its statement (area 20's `geo.` conditions)
  has none.
- **An order an update can change** (chosen in chunk 6.4b3). A `from:`
  whose query orders by a field that can be updated, before any `id`, is
  `type.batch_order_updatable`, reported at the `from:` and declined on
  the action that starts the run. A field can't be updated when it is
  `id`, marked `immutable: true` or generated with
  `Generated.on_create`; what comes after `id`, or after a unique field
  that can't be updated, is passed over, since it never moves a record.
  Which updates the program makes doesn't count, so adding one elsewhere
  never makes a run's order wrong after the fact; a policy with no
  `update:` doesn't either, since adding one is a one-line change.
- **Carried on by time.** A runner holds its run by a lease of what is
  left of its budget and a minute ("Running background work"), renewed
  with each page it commits, and goes page after page while its budget
  lasts, then gives its lease back and is carried on in a fresh
  invocation from the position that committed; the budget ending mid-page
  drops that page, nothing it did kept. A run that couldn't commit a page
  in a whole budget gives its lease back and waits for the tick, which
  picks up every run still running whose lease has lapsed.
- **What a deploy changes.** A run whose call a deploy changed (its query,
  its actions or what it saves), or whose saved values are in an encoding
  this release doesn't read, a later release's after a rollback, ends
  `failed` with why ("its saved values were written in encoding 2, which
  this release doesn't read") rather than reading what it saved as
  something else. A run with no encoding whose call and shape match is
  read, its values as encoding 1's; one kept before calls were named by
  identity has a name no call now gives, and ends `failed` because "it was
  started by a release that named its `batches.each` call another way".
  Listing such runs before a deploy is area 14's `jadpo runs unfinished`.
- **Kept 30 days.** In each hourly cleanup pass, in every store that keeps batch runs, a
  run that ended (finished, failed or cancelled) 30 days or more ago is
  deleted with its problems and its cancel, as are cancels as old of runs
  no store keeps (asked of a `BatchRun` a field held after its run was
  deleted). A run still going is never deleted, however old.
- **Names.** `batches` is a standard namespace, so, as every one, it names
  no variable.

### How a file's run is kept

What a file helper's run (`csv.read`, `tsv.read`, `json_lines.read`,
and from chunk 6.4b3 `json.read`'s file form) does beyond a query's run,
chosen in chunk 6.4b and, for JSON and gzip, chunk 6.4b3, each the most
conservative reading of "Batches" above and area 19's "Data formats":

- **Where it is kept.** A file has no query whose store keeps its run,
  so it is kept in the store the starting action writes, counting what
  `each:`'s and `finished:`'s actions write, which the one-store rule
  makes one; when none of them writes, in the application's only
  database. With several databases and nothing written, the call is
  `toolchain.not_built_yet`.
- **A batch is rows.** Each transaction holds a batch of the file's rows,
  a hundred (`jadpo test` reads two, so every import resumes between
  batches), and the run's position is where the next row begins: its
  byte, its row and its line, kept with the run. The file is saved with
  the run as its `File`, so a resumed run reads the same one.
- **Read before the transaction.** No outside call goes out while a
  transaction is open (area 16, chunk 7.0c), and an item's action runs
  inside its batch's, so each batch's rows are read from the store first,
  through the `files` capability, a window of at most a megabyte at a
  time until they are whole, from the position the last batch committed.
  Then the batch's transaction opens, holds the run's row to that same
  position, runs the rows' items, and commits their writes with the
  problems and the position after the last row read. A runner reads the
  file's opening once each time it picks the run up: its encoding and,
  for CSV and TSV, its header.
- **Columns.** The header names the columns, each a field of `as:`'s
  `Input` by its name or its `column:` (a CSV or TSV column's; JSON
  Lines' fields are the `Input`'s JSON). A column no field reads, one
  named twice, or a field that may not be left out with no column, stops
  the run before any row: an `Input` refuses an unknown field. A row
  with more cells than the header is a problem (`unknown_field`); an
  empty cell is left out, so `optional:` and `default:` apply, and a
  required one is `required`.
- **Rows and problems.** A row is read as a query string's value is,
  each cell its field's text, a standard type its canonical text; one
  that can't be read is a problem with its row as a spreadsheet numbers
  it (the header row 1, a blank line counted but no item) and its
  column, by the header's name, and the rule; JSON Lines' row is its
  line, and its column the field's path. A row whose action fails is a
  problem with no column and the failure's code. Never the value.
- **Quotes and lines.** RFC 4180's quoting: a quoted cell holds the
  delimiter, line breaks and `""`; LF, CRLF and a lone CR end a row. A
  quote inside an unquoted cell, or text after a cell's closing quote, is
  kept as written, as spreadsheets read it.
- **What stops it.** At the file's level, before or at the place it is,
  the rows before it done: a quote that never closes, named by the line
  it opened on; bytes that aren't text in the file's encoding (a NUL
  among them), named by their line; a gzip file read without
  `max_unpacked_size:`, and one that unpacks past it, is damaged or is
  cut short (below); and a row longer than 16 MB. The run ends `failed`
  with why. A store that can't be reached pauses it, as it does a
  query's.
- **A JSON file's list** (chunk 6.4b3). `json.read`'s file form reads
  the list `items_at:` names, or the document when it is given none, an
  item a row: its row is its place in the list, from 1, and a problem's
  column the field's path, as JSON Lines' is. The document is walked a
  window at a time, holding only where the walk is and the item in hand,
  so a document of any size is read in the memory of its largest item:
  the objects on the way to the list are read for their members' names,
  and every other value is passed over, its brackets matched and its
  strings followed. An item is the text of one value in the list, which
  the row type's codec reads as a request's body is read, so an item that
  isn't JSON (`tru`), or isn't the row type, is that item's problem and
  the rest carry on. What stops the run, at the place it is, the items
  before it done: no list where `items_at:` says (or, with none, a
  document that isn't a list), structure that isn't JSON (a missing comma,
  a bracket that doesn't match), named by its line, a document that ends
  before its JSON does, an object on the way naming a member twice, and
  an item longer than 16 MB. Once the list closes, the rest of the
  document is walked to its end, so one cut short or followed by more
  text stops the run after its last item. Each runner walks the document
  from its start to the list once, as it reads a CSV's header; a page
  starts from that walk, set to the position its run committed: the byte
  after the last item done, with its line and the next item's place.
  `items_at:` is part of the call's shape, so a deploy that changes it
  ends a running run `failed`, as one that changes what it saves does.
- **gzip** (chunk 6.4b3). A file whose first bytes are gzip's is read
  only when the helper is given `max_unpacked_size:`, and is unpacked as
  it is read (area 16, "Chosen in chunk 6.4b3"): the run's position, its
  rows' lines and its encoding are its unpacked text's. Its text is never
  read past `max_unpacked_size:`: the run stops at the row that would
  reach past it ("its file unpacks to more than 200 MB, its
  `max_unpacked_size:`"), the rows wholly inside it done, so a small
  upload can't unpack to gigabytes. A runner that picks the run up
  unpacks the file again from its start to its position, since gzip
  can't be entered part way. A file that isn't gzip is read as it is,
  whatever `max_unpacked_size:` says. `max_unpacked_size:` is the
  plan's, not a value the run saves, so a deploy may change it under a
  running run.
- **Encodings.** UTF-8, a byte-order mark removed, and UTF-16 with its
  mark, little- or big-endian, never guessed; anything else isn't text.

### How an export's run is kept

What a file helper's write (`csv.write`, `tsv.write`, `json_lines.write`
and `json.write`'s file form) does beyond a query's run, chosen in chunk
6.4b2, each the most conservative reading of "Batches" above and area
19's "Data formats", which says how each row is written:

- **Where it is kept.** As `batches.each`'s: in the store `from:`'s query
  reads, which `finished:`'s action writes too.
- **Each page's put after its page commits.** No outside call goes out
  while a transaction is open (area 16, chunk 7.0c), so a page goes in
  three steps. Its transaction locks the run's row, reads the page after
  the position that committed, as the system, and commits, having
  written nothing. Then, with no transaction open, `each:` works out each
  item's row, and the page's rows are put in the store as a piece of the
  file of their own, under a key made from the run's id and the piece's
  number. Last, a second transaction locks the run's row again, holds it
  to the position the page was read from, and commits the problems and
  the position after the page: the next page's cursor, and the pieces
  kept with their bytes and rows. A run stopped between the put and that
  commit puts the piece again when it resumes, under the same key, so the
  file never holds a row twice.
- **`each:` is pure.** Its body is one expression, the row, worked out
  from the item and what the run saved; `attempt` only starts a
  statement, so it can't fail. What stops it is a fault, an overflow
  among them, which makes the item a problem by its place in the walk,
  its rule the fault's classification and its row not written.
- **What it saves.** Each of the action's parameters and variables
  `each:` reads, once, as a value an entity field holds
  (`type.batch_capture_not_storable` otherwise), beside `name:` and
  `format:`; `finished:`'s captures and `key:` as `batches.each`'s.
- **Finishing.** Once every page is written, the pieces are joined into
  the file, with no transaction open (the store's `join`, area 16,
  "Chosen in chunk 6.4b2"); `finished:` then runs in a transaction of its
  own, given the `ExportSummary`, whose `file` holds the store, the key,
  `name:`, the size and the type. The file's key is made from the run's
  id, so a run that joins again finds it whole.
- **Cancelling.** A cancel ends the run at its next page, as a query's
  run, and once that end commits the file's pieces, and the file if they
  were joined, are dropped. A store that can't be reached then leaves
  them; deleting what nothing points at comes with the rest of the files
  capability.
- **A store with no room** pauses the run, as one that can't be reached
  does: what committed stands, and the tick carries it on.

### Batches, tenants and grants

In an application with a `tenant:`, each page's read, its items and
`finished:` act as the system limited to the tenant the run's saved values
of the tenant's `id` type name ("Background work and tenants"): a `from:`
argument, a value `each:` or `finished:` captures, or `key:`, whose own
type is the tenant's `id` (`Household.id`, or one that may be `none`), or
a field declared with it, as a reference to the tenant is
(`rent_round.society_id`). A tenant's id held inside a saved record
doesn't count. The run reads them from what it saved, so it has the same
tenant each time it resumes. Values that all name the same tenant
(`key: household_id` beside an `each:` capturing `household_id`) limit
the run to it (decided 2026-10-19); a run that saves no such value, one
that is `none`, or values naming different tenants, reaches no tenant's
records, only those every tenant shares. An application without a
`tenant:` runs batches reaching every record the system is granted.

As for a schedule or a subscriber, what `from:`'s query reads, or the
action `each:` or `finished:` calls reaches, itself or through what it
calls, where a grant list, a field's own list or the action's line under
`actions:` names `Access.public` and not `Access.system`, is
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
    order: [Item.id asc]
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
    order: [Item.id asc]
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

### What a run reaches under policy

- **What the summary carries.** An `ImportSummary` holds counts and
  problems, never a record's fields. `items` shows nothing of the items
  the run walked, as a count of a query's records doesn't.
  `items_with_problems` and `problems` count the items whose action
  failed and say which and why, so they carry what decides whether
  `each:`'s action fails: what decided each `reject` and `attempt` it
  runs, and whether an action it calls fails, read of the item and of
  what the run saved. What decided that the run starts goes with all
  three.
- **The item, checked like any call.** Each item is one of the records
  `from:` finds, the same record all through `each:`'s action, so what
  that action stores from it, given the item or a field of it, is checked
  where `each:` calls it, as a call's arguments are.
- **What a run saves has left its record**, as a value an event carries
  has ("Subscribers"): it is proved as one from any record of its entity,
  but for a record a saved tenant id was read from, which is of the
  tenant the run is limited to. One the starting action was passed is
  checked where that action is called, the same way.
- **The run's tenant proves what it reaches.** Every record a run reads is
  of the tenant it is limited to, so the item, and what decided its
  action's failures, may be stored wherever that tenant's roles may read
  them, as a record of an event's tenant may. A record every tenant
  shares (`Tenancy.shared`) isn't the tenant's.
- **Every action a run reaches, by its own line.** What a schedule, a
  subscriber or a batch reaches is checked against the line under
  `actions:` of every action it reaches, not only the one it runs, and a
  listed action's line decides its own body's writes wherever it is
  called from (area 9's Decisions, "Lines of the actions background work
  calls"). Routes are checked the same way.
- **What the run's actions show.** Nothing reaches whoever started the
  run: a failure's fields and a record found by a value are shown to
  nobody, and a problem keeps only the failure's code.
- **What `batches.status` gives** is held to what decides the failures of
  every batch the application starts, since where it is read nothing says
  which batch a `BatchRun` is: whether each one's `each:` action fails,
  and whether its `finished:` action fails, which ends it `failed`. Each
  is held as from any record of its entity, never one of a run's tenant,
  and with nothing of what the starting action was passed. It is read by
  a caller of whom nothing is known but what the reading action's line
  under `actions:` checks as it starts, since the run is found by its
  `id` alone. A status read inside a batch's own actions goes round, and
  is solved as calls that go round are. So in an application with a
  tenant, a run's status shows only what anyone at all, or a role held
  across the whole application, may read: letting a reader through the
  record that keeps the run would need the `BatchRun` to say which batch
  and tenant it is.
- **No request carries a batch type.** An `Input`, read from a body or,
  for a `GET` or a `DELETE`, from the query string, and a route's
  placeholder may not hold a `BatchRun`, a `BatchProgress`, a `RowProblem`
  or an `ImportSummary`: directly, in a list, a map or an option, in an
  object, through a type built on one, or through an entity's field
  holding one (`Sweep.run`), `type.batch_input`. A `BatchState` is a plain
  enum a request may name. A named query's parameter isn't refused: what
  is passed to it comes from a route's placeholder or input, which are,
  or from an action. Area 2 gives a batch type's fields to read, so one
  named as a type (`BatchRun.id`) is `type.unknown_field`, as is a
  standard object's field it lacks. A run is reached from `batches.each`,
  or from a field the caller may read.

An item followed into what its action stores: `post_item` puts a diary's
confession, which only its owner reads, on a board anyone signed in
reads:

```jadpo check project=e-batch-item-confession file=journal.jadpo hidden
type Line = Text {
    min_length: 1
    max_length: 200
}

type DiaryPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Diarist {
    id: Uuid
    handle: Line {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

principal {
    user: {
        subject: Diarist.handle
    }
}

entity Diary {
    id: Uuid
    owner_id: Diarist.id {
        on_delete: OnDelete.cascade
    }
    title: Line
    confession: Line {
        policy: {
            read: [owner_id, Access.system]
        }
    }

    policy: {
        create: [owner_id]
        read: [Access.authenticated, Access.system]
    }
}

query diaries(page: DiaryPage) -> Page<Diary> {
    from: Diary
    order: [Diary.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

entity Board {
    id: Uuid
    text: Line

    policy: {
        create: [Access.system]
        read: [Access.authenticated, Access.system]
    }
}

authentication {
    browser_session: {
        cookie: "journal_session"
    }
}
```

```jadpo check project=e-batch-item-confession expect=policy.output_field_unproved
action post_item(diary: Diary) -> Unit {
    var board_post: Board = Board.create {
        set: {
            text: diary.confession
        }
    }
    assert board_post.text == board_post.text
}

action post_all(page: DiaryPage) -> BatchRun {
    return batches.each(from: diaries(page), each: diary -> post_item(diary))
}
```

```text output
error: `post_all` passes `Diary.confession` to `post_item`, which stores it in `Board.text`, though not everyone who may read `text` may read it
 --> main.jadpo:11:71
   |
11 |     return batches.each(from: diaries(page), each: diary -> post_item(diary))
   |                                                                       ^^^^^ from `Diary.confession`
  reason: `post_item` stores what it is given here in `Board.text`, and everyone who may read `Board.text`, anyone signed in among them, reads what is stored there, from any request, and only the user `owner_id` points at and schedules and subscribers may read `confession`. A value stored in a field keeps its own field's rules, as one sent in an output does, so it is stored only where everyone who may read it may read the value.
  next: Pass `post_item` what everyone who may read `Board.text` may read, or give `text`'s `read:` only those who may read `confession` (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 2 files
```

How many diaries a run refused shows what refused them, a diary's
confession, to anyone signed in, whom `count_drafts`'s line lets in:

```jadpo check project=e-batch-status-confession file=journal.jadpo hidden
type Line = Text {
    min_length: 1
    max_length: 200
}

type DiaryPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Diarist {
    id: Uuid
    handle: Line {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}

principal {
    user: {
        subject: Diarist.handle
    }
}

entity Diary {
    id: Uuid
    owner_id: Diarist.id {
        on_delete: OnDelete.cascade
    }
    title: Line
    confession: Line {
        policy: {
            read: [owner_id, Access.system]
        }
    }

    policy: {
        create: [owner_id]
        read: [Access.authenticated, Access.system]
    }
}

query diaries(page: DiaryPage) -> Page<Diary> {
    from: Diary
    order: [Diary.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

authentication {
    browser_session: {
        cookie: "journal_session"
    }
}

policy {
    actions: {
        count_drafts: [Access.authenticated]
    }
}
```

```jadpo check project=e-batch-status-confession expect=policy.output_field_unproved
type Draft = Failure {
    kind: FailureKind.rejected
    message: "This diary is a draft."
}

type DraftCount = Output {
    drafts: Int
}

action check_diary(diary: Diary) fails Draft -> Unit {
    if (diary.confession == "draft") {
        reject Draft {}
    }
}

action check_all(page: DiaryPage) -> BatchRun {
    return batches.each(from: diaries(page), each: diary -> check_diary(diary))
}

action count_drafts(draft_walk: BatchRun) -> DraftCount {
    var at: BatchProgress = batches.status(draft_walk)
    return DraftCount {
        drafts: at.problems
    }
}
```

```text output
error: `DraftCount.drafts` carries `Diary.confession`, which not every caller of `count_drafts` may read
 --> main.jadpo:23:17
   |
23 |         drafts: at.problems
   |                 ^^^^^^^^^^^ decided by `Diary.confession`
  reason: `batches.status` finds a run by its `id` alone, so what it gives shows what decides the failures of every batch the application starts, `Diary.confession` among them, to anyone who may call `count_drafts`, and only the user `owner_id` points at and schedules and subscribers may read `confession`.
  next: Let only those who may read `confession` call `count_drafts`, through its line under `actions:`, or decide which items fail by what its callers may read (agent)
  rule: policy.output_field_unproved
  help: docs/language/09-policy.html#policy-output-field-unproved

check failed: 1 error, 2 files
```

A run's placeholder:

```jadpo check project=e-batch-input-placeholder file=stock.jadpo hidden
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
    order: [Item.id asc]
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

action stop_sweep(shelf_run: BatchRun) -> Unit {
    batches.cancel(shelf_run)
}

policy {
    actions: {
        stop_sweep: [Access.authenticated]
    }
}
```

```jadpo check project=e-batch-input-placeholder expect=type.batch_input
routes Sweeps {
    prefix: /sweeps

    DELETE /{shelf_run: BatchRun} {
        authentication: RouteAuthentication.none
        status: SuccessStatus.no_content
        run: stop_sweep(shelf_run)
    }
}
```

```text output
error: Placeholder `shelf_run` holds a `BatchRun`, which no request carries
 --> main.jadpo:4:25
  |
4 |     DELETE /{shelf_run: BatchRun} {
  |                         ^^^^^^^^ a `BatchRun`
  reason: Only the runtime builds a `BatchRun`: a run comes from `batches.each`, or from a field the caller may read where the application kept it, and `batches.status` and `batches.cancel` find it by its `id` alone. A request that named one could read or stop any run.
  next: Take the `id` of the record that keeps the run, and read the `BatchRun` from its field (agent)
  rule: type.batch_input
  help: docs/language/12-jobs-and-events.html#type-batch-input

check failed: 1 error, 2 files
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

### The event record

An event's shape is recorded, and a change to it shown as a diff, as an
API's is (decided 2026-10-19). Each domain that declares an event keeps
one record beside its events, `domains/<domain>/events/events.jadpo_record`,
a view in the record format (`record events 1`) whose header says it is
generated from "the events of domain `<domain>`". Every event is
recorded, not only `public` ones: what is waiting in the outbox when a
build ships is read with the new shape. There is no command of its own:
`jadpo records update` writes it, and `jadpo records` lists it.

- **What it holds.** An `event <Name>` entry for each event, `public` when
  another domain may subscribe to it, then its fields as its payload
  carries them, in the API record's wire form (`words: Text { max_length:
  200 }`), then an `object`, `enum`, `input` or `output` entry for each
  type a field names, written as the API record writes them. Events,
  fields and types sort by born name. Descriptions are left out, as the
  API record leaves them out.
- **How it is checked.** As the API record is (area 18, "Checking a
  record"): a missing record, one whose events changed
  (`record.out_of_date`, naming each entry added, changed or removed) and
  the record of a domain left with no events are put right by
  `jadpo records update`; a hand edit Jadpo could not have written is
  `record.edited`. While an event file has errors the record isn't
  compared. Classifying a change as breaking, and accepting one, wait for
  area 18's acceptances with the API record's.

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

- Only actions emit, and only events. Functions and queries cannot, nor a
  test, a fixture or a seed (`event.emit_invalid`), and routes, schedules
  and subscribers emit by calling an action.
- The event is saved in the same database transaction as the action's writes
  and delivered only after that transaction commits. If the action fails or
  rolls back, the event is discarded and no subscriber ever sees it. This is
  the transactional outbox pattern, which Laravel (`afterCommit`), Oban
  (insert inside the transaction) and Rails 8
  (`enqueue_after_transaction_commit`) also use.
- **Where it is kept.** In the outbox of the store the action's
  transaction is in, written as the action commits, with a delivery
  record for each subscriber, `pending` and due at once. An action that
  began no transaction keeps them in the store it writes, or, writing
  none, the one `events: { store: }` names, beginning its transaction
  there for them. An action that emits and writes no store, itself or
  through what it calls, with no `events: { store: }`, is
  `event.store_required`, even when it reads one, as the decision says
  "writes". An event nothing subscribes to is still written. Its row
  keeps its type's key, its payload as the contract's JSON, when it was
  emitted, the emitting action's `traceparent`, the encoding its payload
  was written in, and how deep in a chain it is. A route whose action
  writes no store but emits keeps its `Idempotency-Key`s beside its
  events (area 7).
- Past 1,000 events in one action, or an event's JSON past 64 KB, is a
  defect as it is emitted, and the action rolls back.
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
- A subscriber runs as the system (`Access.system`, area 9), not as the
  user who caused the event (question 8). If it needs to know who, the
  event carries a field such as `by: User.id`.
- A subscriber holds `on:`, an event type (another domain's public one,
  imported); `run:`, a call of an action of the subscriber's own domain
  (`subscriber.run_invalid`) whose arguments are `event` or fields read
  through it (`subscriber.argument_invalid`), typed against its
  parameters; and `retry:`. Anything else is `subscriber.item_invalid`.

**What a subscriber stores from its event.** Each action's `emit`s, and
those of the actions it calls, are part of what it passes to its callers,
so what an event carries is the union of what every `emit` of it put
there, with what a request sent left out, and along chains of events what
a subscriber's action emits from the event it was given. What the
subscriber's action stores from what it is passed is checked where the
subscriber passes it, as a call is (area 9, "What a write stores"), and
refused with `policy.output_field_unproved` at the argument. What decided
an `emit`, and any `reject` in its action, go with every field of the
event.

A value from an event has left its record: the record may change or move
between the `emit` and the delivery, so a value an event carries is
proved as one from any record of its entity. Of the field's readers, only
what holds of anyone signed in, or of a role held across the whole
application, carries over. The one relation kept is the tenant: when the
event's tenant field came from the very record a value did, that record
was the tenant's, and the subscriber is limited to that tenant, so the
value may be stored wherever that tenant's roles read it. A subscriber
that needs more passes the event's ids and reads the current state, as
"Declaring an event" advises.

## Background work and tenants

In an application with a `tenant:`, each kind of background work reaches
one stated set of tenants, and "no tenant" means one thing for each
(Richard, 2026-10-19; area 9, "Background work"):

| Work | Reaches |
|---|---|
| A schedule's run | Every tenant's records: no tenant's event starts it. A run by hand, and a test's `.run()`, too |
| A subscriber's delivery | The tenant its event's one field of the tenant's `id` type names (`household_id: Household.id`), so the tenant is written in the event where a reviewer sees it. With no such field, more than one, or `none` in it: no tenant's records, only those every tenant shares |
| A batch's run | The tenant its saved values of the tenant's `id` type name, values naming the same tenant counting as that one ("Batches, tenants and grants"). With none, `none`, or values naming different tenants: no tenant's records, only those every tenant shares |
| The magic-link `send:` action | The tenant of its delivery, as a subscriber's: `jadpo.MagicLinkEmail` names none, so no tenant's records, only those every tenant shares (area 8) |
| A webhook's reply action, and `on_first_sign_in:`'s | No tenant's records, only those every tenant shares: each runs as the system inside a request no tenant's event starts, and "no tenant" is never every tenant for them (area 9; Richard, 2026-10-21) |

A schedule names no tenant by design, so it reaches every one; a
subscriber and a batch name theirs by a value, and a value that names no
tenant never widens what they reach. The system's runs inside a request
(the sign-in email's `send:` action, a webhook's reply action and
`on_first_sign_in:`'s) reach no tenant's records (chunk 7.9b). In an application without a
`tenant:`, each reaches every record the system is granted. The value
naming a subscriber's or a batch's tenant must come from a record of that
tenant its action reached under its caller's policy, or it is
`policy.event_tenant_unproved` (area 9, "Background work"), so a caller
can never aim background work at a tenant they don't belong to.

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
  the record and does nothing. The record (`jadpo_handled_deliveries`,
  keyed by the delivery's `id`) is written after the action's outbox rows,
  in the store its transaction is in by then: the store it writes, or,
  writing none, the one `events: { store: }` names, where its events are.
  So an action that only emits keeps one, and a repeat emits nothing
  twice; one that neither writes nor emits keeps none, since running it
  again changes nothing in a database. It references nothing in another
  store.
- **Outside calls can repeat.** A provider call cannot join the
  transaction. Each delivery has a stable key (event and subscriber), from
  which the runtime derives the idempotency key each provider operation
  sends (area 11; "Several changing calls in one action", below), so a
  provider that supports keys ignores the repeat.
- **Every action repeats as a unit.** A changing call that takes no
  idempotency key, and that its package does not mark safe to repeat, is
  the only changing call in its action. Once it has gone out it is never
  sent again blindly: a retry the action would otherwise get, and area 6's
  rerun after a concurrent change, stop it as `outcome_unknown` for an
  operator instead (area 16, "Parked, never sent again blindly"; chunk
  7.9a). Changing calls that do take keys may be several, in order:
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
  "Structured values"). A delivery reads the payload by the encoding its
  outbox row records (area 19, "Changing a standard type's encoding"):
  this release's, or none, a payload kept before encodings were recorded,
  read as encoding 1's. A payload of an encoding this release doesn't
  read, a later release's after a rollback, is never guessed at: its
  delivery is parked, `failed` with `payload_unreadable` and why, as a
  payload that no longer reads as its type is, with its problem (the rule
  and the field, never the value).

### How a delivery runs

Once the action commits, each delivery is the runner's, after the
response ("Running background work"). It claims its record in one
statement (`running`, its lease what is left of the invocation's budget
and a minute), at the host's time, so a delivery another runner holds, or
one not due, is left alone. It looks for its handled record, and a
delivery already handled is marked `succeeded` without running again;
otherwise it runs the subscriber's action as the system limited to its
event's tenant ("Background work and tenants"), in a boundary of its own
and a new span of the event's trace, and records how it ended. A request
continues a valid W3C `traceparent` it brings, in a span of its own, or
starts a trace; an event keeps the context of the action that emitted it,
so what a delivery emits keeps it too. A delivery to a subscriber the
build no longer has fails as a defect, and an event 16 deep in a chain
fails with `event_chain_limit`.

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
A route's action that makes a changing call keeps its client's key, in
the store it writes or `events: { store: }`'s, even when it neither
writes nor emits; and its calls' keys come from the client's key whether
or not any store keeps it. A delivery's attempt counts its steps from 1
wherever it runs, a route waiting for it (`settle:`) included. A sign-in
email (area 16, "Built-in emails") is made with a link and a code each
attempt, so each attempt's calls go under a key of their own and keep no
record (chunk 7.9a).
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
acted. Only code's `clock.now` is fixed: the runtime's own timestamps (an
event's `emitted_at`, from which `within:` counts, a request's AWS
signature, the day an `ai` budget counts, a received event's) are the
host's time (chunk 7.9a). At each step the operation and fingerprint must match the record,
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
  `5xx` under that key. A step answered with one of its operation's own
  failures whose kind is `unavailable` or `rate_limited` did nothing: it
  stays in doubt, its failure never kept or replayed (chunk 7.9a).
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

Built in chunk 7.5, what `jadpo check` keeps: up to 8 changing calls on
one way through an action ([`step.too_many`](#step-too-many)), a changing
call with no key the only one in its action
([`step.unkeyed`](#step-unkeyed)), several only from a subscriber or a
route whose API requires `Idempotency-Key`
([`step.context`](#step-context)), no `large_output: true` operation
among them ([`step.large_output`](#step-large-output)), a store for their
record ([`step.store_required`](#step-store-required)), and no outside
call inside `for` (area 3's
[`effect.call_in_loop`](03-logic.md#effect-call-in-loop)). A *changing
call* is a provider's operation with `changes: true` or a store's `put`;
each way through an `if` or a `match` counts on its own, a `return` ends
its way, and an action called counts the changing calls of the longest
way through it. The record and replay came with roadmap 7.5b, the
operator's items with 7.5c, the tests' crash points with 7.5d, and the
warning with 7.5e.

Built in chunk 7.5e, the warning: after the first changing call on a way
through an action, each point that can end it for good is
[`step.late_failure`](#step-late-failure), one finding for the action at
the first such point with the others beside it, declinable since the
condition may be unable to change. A point is an `attempt` whose call or
write passes on a failure that isn't `unavailable` or `rate_limited` (an
operation's or a called action's, a write's `conflict:` or not-found, a
construction's), a `reject` or a `propagate`; a call whose failures a
`match` handles isn't one, though a `reject` in its arm is. Where two ways
meet, a changing call on either counts. A package's action is never told
itself: its surface lists `after_effect` for each failure that can follow
its own changing call, and the app's `attempt` of it is the point, even
as the app's first call. It holds for single calls too, as area 11's
question 26 asked. The same walk sums how long a route's outside calls
can take, area 11's
[`route.calls_past_deadline`](11-integrations.md#route-calls-past-deadline).
Choices in the decisions table, "The late-failure warning and the
deadline sum, chosen in chunk 7.5e".

Built in chunk 7.5b, the record: an action whose longest way makes
several changing calls, run by a subscriber's delivery or by a route
whose API requires `Idempotency-Key` under the key it claimed, keeps its
steps in `jadpo_step_records` and `jadpo_steps` (area 5, "System
tables") in the store it writes, or `events: { store: }`'s. Each step
(each changing call, and a provider's read after the first) is written
before it goes out, with its operation, an HMAC of each top-level field
of its request and when it was sent, and its answer after it, sealed; a
rerun (a delivery's retry, a client's retry under its key, area 6's
rerun) replays an answered step without sending it once its operation and
fields match, with `clock.now` fixed at the first run's start; a step in
doubt goes again under its key within the provider's `window:` less the
margin, and parks as `outcome_unknown` after it, with no key, or on a
`5xx` from a provider that `replays_failures: true`; the provider's
`in_use:` is retried later, its `reused:` is `diverged`; a request that
differs, a run that makes another operation at a step, and one that ends
before replaying every recorded step stop as `diverged` before sending
anything, the delivery held in that state with the step, the operation
and the fields, never a value; a record written by a newer runner (a
delivery's later attempt, a route's later claim) takes no write from an
older one; a `put` in such an action keeps its object under a key
derived from its step; and the record is deleted in the action's commit.
Answers are sealed with `application { steps: { encryption_key: } }`
([`step.encryption_key_required`](#step-encryption-key-required),
[`step.settings_invalid`](#step-settings-invalid)). Live tests in
`crates/jadpo-cli/tests/step_records.rs`. Choices in the decisions table,
"The record, chosen in chunk 7.5b".

A bakery pays an approved invoice and keeps a copy of it, two changing
calls each under its own key, then records the payment, in a subscriber:

```jadpo check project=several-steps file=app.jadpo hidden
application {
    events: {
        store: main
    }
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        pay: {
            package: AcmePay.v1
            domains: [bakery]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=several-steps file=config.jadpo hidden
config {
    main_url: PostgresUrl
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
    step_encryption_key: SigningKey
}
```

```jadpo check project=several-steps file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=several-steps file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
}
```

```jadpo check project=several-steps file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
public resource payments {
    create: {
        method: HttpMethod.post
        path: "/v1/payments"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=several-steps file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=several-steps file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=several-steps file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=several-steps file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=several-steps file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=several-steps file=domains/bakery/events/invoice_approved.jadpo hidden
use bakery/types { PriceList }
use shared/providers/acme_pay

/// The bakery approved a supplier's invoice.
type InvoiceApproved = Event {
    invoice: AcmePay.Reference
    lines: PriceList
}
```

```jadpo check project=several-steps file=domains/bakery/events/events.jadpo_record hidden
// Generated by Jadpo from the events of domain bakery; update it with `jadpo records update`, never by hand.
record events 1

event InvoiceApproved
    invoice: Text { max_length: 40 }
    lines: Text { min_length: 1, max_length: 2000 }
```

```jadpo check project=several-steps file=domains/bakery/entities/supplier_payment.jadpo hidden
use shared/providers/acme_pay

/// A supplier's invoice the bakery paid, with the copy it kept.
entity SupplierPayment {
    id: Uuid
    payment: AcmePay.Reference
    invoice_copy: File {
        store: sheets
        max_size: 1mb
        types: [FileType.csv]
    }

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

```jadpo check project=several-steps file=domains/bakery/subscribers/pay_approved_invoice.jadpo
use bakery/actions { pay_invoice }
use bakery/events { InvoiceApproved }

/// Pays each invoice once it is approved.
subscriber pay_approved_invoice {
    on: InvoiceApproved
    run: pay_invoice(event)
}
```

```jadpo check project=several-steps file=domains/bakery/actions/pay_invoice.jadpo migrate=initial
use app { pay, sheets }
use bakery/entities { SupplierPayment }
use bakery/failures { SheetNotKept }
use bakery/events { InvoiceApproved }
use shared/providers/acme_pay

/// Pays an approved invoice, keeps a copy of it, then records both: two
/// changing calls, each under its own key, then the write.
action pay_invoice(event: InvoiceApproved) fails SheetNotKept -> Unit {
    var payment: AcmePay.Payment = pay.payments.create(
        AcmePay.PaymentRequest {
            reference: event.invoice
        }
    )
    var invoice_copy: File = attempt sheets.put(encoding.to_utf8(event.lines), FileType.csv) {
        storage_full: SheetNotKept
    }
    var paid: SupplierPayment = SupplierPayment.create {
        set: {
            payment: payment.id
            invoice_copy: invoice_copy
        }
    }
    assert paid.payment == payment.id
}
```

Built in chunk 7.5f, development and the metric. In `jadpo dev`, each
step a request's action reached is a line of its trace under the
request's line, in place of its call: its place, its operation, where
its answer came from and what became of it, `replayed`, `answered`, its
failure's code, `in doubt`, `diverged` or `outcome unknown`, never a
request, an answer or a key; and a delivery whose action reached steps
gets a line of its own, as a schedule's run does, with its id, the
subscriber and how the attempt left it, its steps under it and, when it
needs an operator, `jadpo deliveries show <id>`; `--json` carries both
as `steps`, and the delivery as an event of its own. The tick carries
the metric on each store's unfinished records, how many and when the
oldest started, and raises an alert with no setup once one has been
unfinished a day (`{"event": "step.alert", "alert": "unfinished_steps",
...}`), once until none is, across instances; `jadpo deliveries` shows
it beside the items, and its `--json` the metric. Live tests in
`crates/jadpo-cli/tests/step_development.rs`. Choices in the decisions
table, "Development and the metric, chosen in chunk 7.5f".

```text
$ jadpo dev
9c1f0a2b  POST /partners/subs…               503 unavailable   41 ms
  └─ step 1 · pay.customers.create · generated · answered
  └─ step 2 · sheets.put · live · answered
  └─ step 3 · pay.subscriptions.create · generated · in doubt
41d7e09a  subscriber start_subscription      ◆ diverged
  └─ step 1 · pay.customers.create · replayed
  └─ step 2 · sheets.put · diverged
  → jadpo deliveries show 41d7e09a
```

Its test scripts one payment. `jadpo test` runs it again with a crash at
each boundary of both steps; each time, the step sent again goes under
its key and is answered with what the fake kept, so one answer is enough
(chunk 7.5d):

```jadpo check project=several-steps file=domains/bakery/actions/approve_invoice.jadpo hidden
use bakery/types { PriceList }
use bakery/events { InvoiceApproved }
use shared/providers/acme_pay

/// Approves a supplier's invoice, which is then paid.
action approve_invoice(invoice: AcmePay.Reference, lines: PriceList) -> Unit {
    emit InvoiceApproved {
        invoice: invoice
        lines: lines
    }
}
```

```jadpo check project=several-steps file=domains/bakery/actions/approve_invoice.test.jadpo
use app { pay }
use shared/providers/acme_pay

test "an approved invoice is paid once" {
    fakes: {
        pay.payments.create: [
            AcmePay.Payment {
                id: "pay-17"
            }
        ]
    }
    approve_invoice("inv-17", "rye,4.20")
}
```

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

### Running background work

The runtime drives one runner for all background work, so the
interpreter and compiled code run it alike: an action called with its
arguments as the system, in a boundary of its own, and what it sets off
run after it. Every piece of deferred work has a record of its own in the
store, a delivery or a batch run, so none is lost when an invocation
stops short.

- **After the response, never in it.** A committed action's deliveries,
  and the batch runs it started, are left for the runner, not run while
  the request waits (but under `settle:`, area 7). A deployable hands them
  to its host, which runs them once the response has been handed back:
  natively a task on the worker thread that answered, with that worker's
  program, so no thread needs a program of its own; on Workers
  `waitUntil` (since chunk 8.2a). What a host does with a request (answer it,
  hand on what it left) and with a tick is one runtime type every host
  wraps, so a native server and `jadpo dev` serve through the same
  handler, `jadpo dev` only listening to print its lines. Each piece
  gets a context of its own, under the host's clock as it starts and
  with no seed. A host with nothing to
  run it on, a test's or a tool's, drops it, and the tick picks up what a
  dropped piece left.
- **A budget for each invocation.** Every invocation that runs background
  work (after a response, the tick, or work handed on) is given a budget
  by its host, at most the 15-minute limit of a run: natively and under
  `jadpo dev` that limit; a test's host may give less, and Workers' are
  30 seconds after a response and for work handed on, 15 minutes for the
  cron (chunk 8.2a). A tenth of it, at most 5 seconds, is kept back for giving claims
  back: once no more than that is left, the runner takes no more work.
- **A run's limit within the budget.** A subscriber's or a schedule's
  action runs until the 15-minute limit or what is left of the budget
  before the time kept back, whichever is sooner. Its body is raced,
  never its commit: the transaction is held open until the body has its
  value, then committed outside the race, so work that committed is never
  given back as though it hadn't run, and a body that ends past its limit
  keeps nothing. A delivery so stopped is settled against its `retry:`
  budget ("Retries and giving up"); a schedule's run is recorded failed
  with `deadline_exceeded`. The purge schedule's run is held to the same
  limit.
- **Every lease the budget and a minute.** A delivery's claim, a
  schedule's lease and a batch run's lease last what is left of the
  claimer's budget and a minute, so a Worker that is ended loses what it
  held at most 90 seconds after it began; natively, with the whole run
  limit as its budget, a lease lasts at most 16 minutes. A claim taken
  while a route waits for its follow-on work lasts the longest deadline a
  route has, 60 seconds, and the minute.
- **Work not reached waits in the store.** A delivery an invocation didn't
  reach stays `pending`, and a batch run waits on its row. What an
  invocation after a response left goes on in a fresh invocation with a
  budget of its own, once the invocation ran something; one that could
  run nothing leaves it for the tick rather than hand it on for ever.
  Natively that is a task of its own; on Workers a request to the Worker
  itself, through the loopback binding `ctx.exports` gives its own
  `JadpoHandOn` entrypoint (a service binding naming the Worker until
  chunk 8.4b), naming each piece by its record, which the fresh
  invocation reads back and runs in its own `waitUntil`, within what is
  left of its own 30 seconds, counted from its start (decided 2026-10-21,
  chunk 8.2h; Decisions, "Work handed on, on Workers"). A
  hand-on the platform refuses, as past its limit on invocations one
  request chains, leaves the work in the store for the minute's cron. The tick finds what is due at its time, then runs
  each piece under the host's clock as that piece starts, so work that
  takes time never claims with a lease already past.
- **Stopping gives back as it was.** A native server asked to stop
  (SIGTERM, or ctrl-c under `jadpo dev`) accepts nothing more, starts no
  more ticks, and ends every budget at once: the action running stops,
  nothing it did kept; a delivery is given back `pending`, its attempt not
  counted, as a route's `within:` gives one back; a batch run's lease is
  given back; a schedule's run is recorded `deadline_exceeded`, its lease
  given back. A route waiting for its follow-on work stops waiting, as
  when `within:` passes. The server waits at most 10 seconds for this,
  then ends, and `jadpo dev` exits 0; a second signal ends the process at
  once (143 for SIGTERM, 130 for SIGINT). If the signals can't be
  watched, `jadpo dev` runs as before, a signal ending it at once:
  `cli.signal_handler_failed`, in area 14's list, stays unbuilt.
- A backfill worked out in Jadpo (area 5) runs its rows on the same
  runner, a batch at a time, each value kept and checked by its field's
  rules as any value of the field's type is.

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
  (area 11; built in chunk 7.7b, the event never kept in the outbox and
  its reply kept with its id for a repeat).
  Built in chunk 7.7 (area 11, "Chosen in chunk 7.7"): the event's id
  recorded in `jadpo_received_events` and its outbox row and deliveries
  written in one transaction in `events: { store: }` or the only store
  of records, `within:` counted from arrival, the ids deleted by the tick
  after 30 days.
- **A package's own events.** A package's `public` events, its webhooks
  and the enriched events its own subscribers emit
  (`Stripe.SubscriptionRenewed`), are subscribed to with `on:` like the
  app's own. An event without `public` reaches only the package's own
  subscribers, and subscribing to one is a compile error. A package emits
  only about the outside world, never as a side effect of the app calling
  it: the app's action emits its own event there, where a reviewer sees it.
  Built in chunk 7.6b (area 11, "Chosen in chunk 7.6b"): a package's
  subscriber hears its webhooks' events once per connection receiving
  them, its action calling the API through that connection, and what the
  action emits is kept in the outbox where the webhooks are and delivered
  to the app's subscribers of its public events.
- **Polls.** `polls:` in a package's `provider.jadpo` names an operation, a
  default interval and the event emitted when the answer changes, turning
  an API without webhooks into events. The runtime runs a poll only while
  something subscribes to its event, keeps what it last saw (runtime state
  in the database, like webhook de-duplication) and emits only changes;
  the connection can change the interval (`polls: { latest_rates: 6h }`).
  Polls are for feeds that do not depend on the app's data (rates, a price
  list); checking each of the app's own records is the app's schedule
  calling the package, which a package can ship as a recipe.
  Built in chunk 7.6c (area 11, "Chosen in chunk 7.6c"): each poll the
  app subscribes to, called by the tick through its connection once due,
  what it last saw kept in `jadpo_polls` beside the outbox, and its event
  and deliveries kept with the new digest in one transaction.
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
  dead-letter queue and parked for an operator: on Cloudflare Queues the
  Worker consumes the dead-letter queue and parks each message it hands
  back as `dead_lettered` (chunk 8.4b; area 11, "Chosen in chunk
  7.7c2").
- An HTTP API is a package and anything needing its own protocol or a
  platform binding is a built-in driver: Amazon SQS and Google Pub/Sub's
  pull API are packages; SNS and Pub/Sub's push are webhooks through a
  package with a handshake (area 11); Cloudflare Queues is a built-in
  driver on Workers, its consumer generated into the Wrangler
  configuration, and a package (its HTTP pull API) from a native server;
  Kafka is a built-in driver in a later release.
  Built in chunk 7.7c (area 11, "Chosen in chunk 7.7c"): each message is
  acknowledged once the transaction keeping its event and its id commits,
  its subscribers then delivered from the outbox; one that can't be kept
  is parked whole in `jadpo_parked_messages`, which `jadpo deliveries`
  lists, retries and dismisses; queues are received on the tick, at least
  every 10 seconds. The Cloudflare Queues driver is built in chunk 7.7c2
  (area 11, "Chosen in chunk 7.7c2"), and the Worker's consumer in chunk
  8.2k (area 14, "A Workers deployable"): a
  connection with `driver: Driver.cloudflare_queues`, `queue:` and
  `dead_letter_queue:`, its consumer handed each batch and answering for
  each message, acknowledged once kept, as a repeat or parked, retried
  when the store can't take it now. Since chunk 8.4b a message the Worker
  can't take yet is retried for about 22 hours, its delay doubling up to
  an hour, so it outlasts a deploy's migration, and the dead-letter
  queue's consumer parks what reaches it.

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
  not retry together: each wait is drawn evenly from zero to a ceiling
  that starts at 30 seconds and doubles with each attempt up to 6 hours,
  and is never under a second. A `Retry-After` from a provider is
  honoured, once provider calls come (Phase 7). By default a subscriber
  gets 10 attempts within 24 hours, then fails: a delivery past its
  attempts, or whose next attempt would fall after `within:` from when
  the event was emitted, fails.
- A subscriber can change its budget; schedules cannot, since their next
  due time is their retry. `retry:` takes `attempts:` from 1 to 100 and
  `within:` up to 30 days (`subscriber.retry_invalid`).
- A fault reaching a store, and a run out of time, are retried like
  `unavailable`. A delivery's record keeps the failure's code and kind,
  never its fields.
- **The run limit.** A delivery whose action the run's limit stops
  ("Running background work") is an attempt that ran out of time
  (`deadline_exceeded`), settled at once against its budget: tried again
  after a wait while the budget lasts and failed after, nothing it did
  kept. So a subscriber that never ends fails after its attempts rather
  than taking every invocation's budget.
- **Given up at the claim.** A delivery claimed again over its `retry:`
  attempts, or later than `within:` after its event was emitted, fails
  without running. Claiming one a worker held past its lease marks its
  last failure `lease_expired`, as a schedule's run lost with its worker
  ends. The claim that gives up isn't counted as an attempt, and the
  record keeps the last attempt's failure, or `deadline_exceeded` for one
  never tried in time. A handled record found first still makes it
  `succeeded`, and a store that can't be reached while looking for one is
  retried within the same budget.

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

### When a provider keeps failing

Jitter keeps retries from arriving together, but 5,000 deliveries calling
a provider that is down would still each be tried ten times (question 18).
So the outbox keeps a circuit breaker per connection: after a run of
`unavailable` failures from one provider, the deliveries that call it are
held rather than attempted; after a pause one of them, the probe, is
tried, and its success releases the rest. Held deliveries keep their
retry budget, are shown in `jadpo deliveries` and are alerted on.
Chosen in chunk 7.2b, where the pages left it open:

- **The run.** Five delivery attempts in a row whose call through the
  connection found its provider unavailable open its breaker. An attempt
  counts once per connection it called through, by how the last such
  call ended: unavailable when the call surely did nothing or no answer
  came back (a `5xx`, `408`, `429`, a timeout, a call that never left);
  answered, ending the run, when the provider answered at all, a refusal
  or a rejected credential included. A delivery attempt counts once
  however many times the connection's own `retry:` sent its call (area
  11, question 8): by how the last of them ended, so one a retry got
  through ends the run, and five attempts of three refused sends each
  open the breaker, not fifteen sends. Only calls to the connection's own
  driver count, never a fake's or a recording's answer, and only calls a
  delivery makes: a route's calls are its caller's to retry.
- **Which deliveries.** A subscriber's, when its action calls out through
  the connection, itself or through an action it calls (an action a
  batch's run calls runs after the commit, not in the delivery), or when
  it hands its event to a capability the connection fills. Each is held at
  its claim, before it runs: `held`, not due, the attempt its claim
  counted given back, its connection and the time it was held from kept
  on its record.
- **The pause and the probe.** The first pause is a minute. Once it has
  passed, the first delivery claimed for the connection is the probe, the
  rest still held; the tick's sweep wakes the delivery held longest to be
  one, so a probe goes even when nothing new arrives. A probe the provider
  answers closes the breaker and releases every delivery it held, due at
  once; one that finds it unavailable opens it again for twice the last
  pause, at most ten minutes. A probe whose run calls the provider no
  more gives the probe back, and one whose worker is lost loses it when
  its claim's lease ends. A delivery already running when the breaker
  opened finishes, and its answer closes the breaker too.
- **The budget.** Being held spends no attempt, and the time held doesn't
  count towards `within:`: a delivery's `within:` counts from its event,
  or an operator's retry, moved on by the time it was held. An operator's
  retry gives a fresh budget, the time held included.
- **Where it is kept.** In `jadpo_circuit_breakers` (area 5, "System
  tables"), in every store that keeps deliveries, each store's breakers
  holding its own deliveries; every change is one conditional statement,
  so it holds across processes and on Workers. The sweep also releases
  any delivery whose breaker has closed, which a release racing a hold can
  leave.
- **The alert.** The tick raises one alert per outage, `connection_unavailable`,
  recorded on the breaker's row as a schedule's alerts are on its state,
  so of several instances one raises it; it is raised again only after
  the breaker has closed and opened again. Its log event is `{"event":
  "delivery.alert", "alert": "connection_unavailable", "connection": …,
  "since": …, "held": …}`, and `jadpo dev` prints it with the schedules'.

## What an operator sees

Every delivery and schedule run has a state:

| State | Meaning | The operator can |
|---|---|---|
| `pending` | Waiting to run, or waiting to retry | Cancel it |
| `running` | A worker has it | Wait; a worker that dies loses it after its time limit |
| `held` | Held while a provider it calls is unavailable ("When a provider keeps failing") | Wait; a probe that gets through releases it |
| `succeeded` | Done | Nothing |
| `failed` | Gave up, or a failure that is not retried (the dead-letter list) | Retry it once the cause is fixed, or dismiss it |
| `outcome_unknown` | An outside call may have happened | Check with the provider, then mark it done or not done; "not done" retries it |
| `diverged` | A rerun of an action with several changing calls no longer matches its record | Deploy a fix and retry, or abandon it |

Each failed or unknown item keeps the event, the subscriber, every attempt
with its failure code and internal fields (area 4's logging rules), and the
request that emitted it. A `failed` or `outcome_unknown` item writes a log
event and counts towards an alert. The screens and commands are area 14's.

**`jadpo deliveries`** (area 14, question 13; chunk 6.12) lists the
items that need an operator, `failed` and `outcome_unknown`, newest
first, 20 a page, and `--state` those `pending`, `running`, `held`,
`dismissed` or `cancelled`; a connection whose circuit breaker holds
deliveries is an alert line on the list and on the held ones, with when
it became unavailable, how many it holds and when one is tried again
(chunk 7.2b); an operator acts on one by its id, the last 8 hex digits
of its UUID (random in a version 7 UUID, whose first are its time), or
more. Chosen in chunk 6.12, where the pages left it open:

- **Two more states.** `dismissed` and `cancelled` are a delivery's
  record set aside by an operator; neither runs again. Each is kept 30
  days after the operator's last action on it, and its event with it;
  then the tick deletes it ("The tick"; chunk 6.13).
- **What each action does to a delivery.** `retry` (from `failed`) makes
  it `pending` and due now with a fresh budget: its attempts counted from
  none, and its `within:` from the retry (the record's `retried_at`), not
  from its event, so one retried after its window still runs; the tick's
  sweep then delivers it. `dismiss` (from `failed`) and `cancel` (from
  `pending`) set it aside; `resolve --done` (from `outcome_unknown`)
  marks it `succeeded`, and `--not-done` retries it as `retry` does. A
  delivery whose outcome is unknown is never retried blindly: `retry`
  is refused and points at `resolve`. Each is one statement that
  changes the record only from the state it was read in, so of two
  operators one acts and the other is told it changed.
- **Schedule runs.** A schedule's item is its latest run (a skipped tick
  aside) when that failed, or its outcome is unknown, until a later run
  ends it or an operator settles it, so a schedule failing every 15
  minutes is one item, not a backlog; its earlier runs stay in `jadpo
  schedules history`. `retry`, and `resolve --not-done`, ask the app's
  tick to run it by hand and wait for its record, as `jadpo schedules
  run` does against a deployment ("Running a schedule by hand"); a run
  asked for that way is `pending` until a tick takes it, and `cancel`
  withdraws it. `dismiss` and `resolve --done` leave the run record as
  it is and settle it in the log.
- **Who did it.** Every action writes a row of `jadpo_operator_actions`
  (area 5, "System tables") in the store the item is in, in the same
  statement as its change: when, which action, the item and its state
  then, the name the operator's machine gives (`USER`, else `LOGNAME`
  or `USERNAME`), and the database role PostgreSQL authenticated,
  which the statement reads itself (`session_user`), so it can't be
  claimed. The CLI shows both under the result (`logged  sam · role
  app_owner`). A row is kept 30 days, as a run record is, and then
  until its delivery or run is gone, so an item still on record keeps
  the last thing done to it and a dismissed run stays settled; the tick
  then deletes it (chunk 6.13). Jadpo has no
  operator accounts of its own, as it has no reviewer accounts
  (area 14, question 12): the role is the authenticated identity.
- **Where.** The commands read every store that holds an entity, where
  deliveries are, and `schedules: { store: }`, each found as `jadpo
  schedules` finds its database: `--url` (with `--store` when there are
  several), else the connection's setting, else a running `jadpo dev`'s.
  They run none of the app's code, and read none of its settings beyond
  those addresses.

**Unfinished multi-step runs are operator items too** (decided
2026-10-16). Whenever an action with several changing calls ends without
committing (failed, diverged, parked, or a route whose key lapsed), the
operator sees an item listing what took effect at each provider, step by
step with its answer, and can retry it (with `--not-done`, under a new key
generation) or abandon it. Nothing about it is quietly dropped; an item is
kept 30 days, then only its step list.

Built in chunk 7.5c. A delivery that `diverged` is listed by default
beside those that failed or are in doubt, and `--state` takes
`diverged`, `unfinished` and `abandoned`; a route's record whose client
didn't retry within a day of its last run is an item of its own,
`unfinished`, with the action as its target. `jadpo deliveries show <id>`
lists an item and, when its action keeps a record, each step: its
operation, `answered`, `failed`, `no answer` or `answer dropped`, when it
was sent and its key's generation once replaced, never a request, an
answer or a key; then the one thing to do, a decision for a step in
doubt. `retry` takes a delivery that failed or diverged, its record's
generation counted again from none as its attempts are; `retry --not-done`
(and `resolve --not-done`) sends the last step again under its next key
generation when it has no answer or failed, its answer cleared; `abandon`
ends a delivery that failed, diverged or is in doubt, or a route's
record, keeping its step list. A delivery with a record is abandoned, not
dismissed or resolved as done, since its action's writes never committed.
Each change is one statement with its log row when the record is in the
delivery's store. The tick drops a record's answers 30 days after its
last run, and deletes an abandoned record, and its delivery, 30 days
after it was abandoned; a route's client retrying under an abandoned
record's key is answered `500` and nothing is sent. The provider's
`new_key_on:` answer sends a step again at once, once, under its next key
generation, which the record keeps first. `jadpo runs unfinished --build
<build id>` (area 14) lists the records and batch runs a new build
changes. Live tests in `crates/jadpo-cli/tests/step_operators.rs`.
Choices in the decisions table, "Operators, chosen in chunk 7.5c".

**Batch runs and queue messages** show here as well: each batch run with
its state, progress against the file's size, problems and an estimated
finish, which `batches.status` gives the app; and each queue message
parked because its type is not listed or because it kept failing.

### Watching schedules

Schedules fail quietly in most frameworks: a job that stops running produces
no error at all. Jadpo records and checks every part of a schedule's life
(Richard, 2026-10-13):

- **Every run is recorded** in a system table (`jadpo_schedule_runs`): due
  time, start, end, outcome (`running`, `succeeded`, `failed`, or
  `skipped` with `paused` or `lease_held`) with its failure's code and
  kind, never a value, the records a purge deleted, the events the run
  kept once it committed, and its `traceparent`, so a run opens as one
  trace (question 17). Records are kept 30 days.
- **Every tick is recorded too,** including skipped ones and why (lease
  held, paused); each state row keeps the last tick that reached it.
- **`jadpo schedules`** lists each schedule with its last run and next due
  time (or `‖ paused`), the alerts that hold and a footer. The lease
  holder is the run holding the lease, shown as the last run `● running`
  with its start (instances have no names yet), and recent failures are
  the failed runs since the last success, beside a failed last run
  (`· 3 in a row`). It succeeds whatever it lists, a failed last run or an
  alert included, as other listing commands do, and `--json` carries the
  alerts for a health check.
- **`pause` and `resume`** set the state row's `paused`, making the row as
  the first tick would when none has yet; a run under way carries on.
  Pausing a paused schedule, or resuming one that isn't, changes nothing
  and says so.
- **`history <name>`** gives every record, runs and skipped ticks, newest
  first, 20 a page (`--page <n>`), grouped by UTC day; then the metrics
  over the records kept: runs, failed runs and skipped ticks, the median
  and longest duration, and the median and greatest delay (lateness).
- **The commands read** the database the schedules' state is in: the one
  `--url` names, else the address its connection's setting gives (area
  10), else the one a running `jadpo dev` started for the session, which
  it notes in `build/dev/stores.json` and removes when it stops. There is
  no `--env` flag: an environment is its configuration. Times are UTC, as
  schedules count them.
- **`jadpo why <schedule>`** answers "why didn't it run?" in area 17's
  shape, the first that holds: paused (and the due times skipped since);
  no tick since a time; a lease held past its end; a run still holding
  it; a tick skipped since the last run; the last run failed (its code,
  kind, failures before it, next due time and trace); it succeeded; not
  due yet; or no tick has seen it. Its `.jadpo` place is the schedule's
  declaration, its doc comment's line when it has one. Other alerts
  follow it. The answer is the runtime's, as data (the answer, the alerts
  that hold, a standing request), which the CLI says in words and the
  console and the MCP server read too.
- **Metrics** (area 16's telemetry): each run's lateness (start minus due)
  and duration, measured on the monotonic clock from the run's own time,
  carried by the tick's result and `jadpo dev`'s `--json` lines, and per
  schedule by `history`; skipped ticks and failures per schedule. Items
  per run wait for a schedule run that counts them, as a purge does.
- **Alerts with no setup,** each a condition worked out from the rows at
  a time: ticks stopped, when the last tick any schedule received is
  older than twice the host's tick period; a lease past its end with its
  run never ended; a run that has taken, or took, 80% of the 15-minute
  limit; three or more failed runs since the last success; and, unless
  paused, no success for over twice `every:`, counted from the last
  success's start (kept on the state row as `last_succeeded_at`, so a
  monthly schedule isn't lost with its 30-day records) or else from its
  first due time. Each is a log event (`{"event": "schedule.alert",
  "alert": …}`, area 14's question 13); sending it on waits for the
  hosts' log sink and telemetry (area 16).
- **The tick raises the alerts.** Once a tick's schedules have run, it
  works out the alerts that hold for each schedule, as any reader does,
  and records them on the schedule's state row (`alerts`: each alert's
  id and when it was raised) in one statement that changes the record
  only while it is what the tick read. Of several instances ticking, or
  several Workers cron invocations, the one whose statement changed it
  raises what newly holds, and the others raise nothing. An alert is
  raised once until it stops holding, its details (a growing failure
  count) changing nothing; one that stops and starts again is raised
  again. Ticks stopping is never raised by a tick, since a tick arriving
  is the proof they haven't. `jadpo dev` prints the alerts its tick
  raised, so a restarted session doesn't print one already raised.
  Readers still work the alerts out when they read: `jadpo schedules`
  and `jadpo why` show those that hold at their own time, since a
  stopped application raises nothing and its operator must still see
  its alerts.
- The console (area 17) shows each schedule's timeline of runs and ticks.

### Running a schedule by hand

`jadpo schedules run <name>` runs a schedule now, paused or not, under
its lease: refused with `schedules.lease_held`, and nothing done, while a
run holds it. Its record's reason is `by_hand`, and the grid isn't moved.
Where it runs depends on the database (Richard, 2026-10-19):

- **Against `jadpo dev`**, the database a running `jadpo dev` noted for
  that store in `build/dev/stores.json`, it runs the schedule straight
  away in the command's own process, with the app's settings from the
  environment over `.env.local`, as the system reaching every tenant at
  the command's time, its due time when it started, and shows the
  outcome.
- **Against a deployment**, any other database, including one
  `jadpo dev --url` was given, it reads nothing of the application's
  settings and builds none of its code, so a deployment's settings and
  secrets never leave its servers. It writes a new run's id to the state
  row's `run_requested` column (made as the first tick would when none
  has), and waits for the record with that id. A request while another
  stands joins it, so several requests before a tick make one run. The
  tick reads `run_requested` with each state row: a schedule's due run
  goes first, and a request is run on the same tick only when that left
  the lease free (the due tick skipped as paused, or none due) and no
  live lease holds it, so a tick runs a schedule once at most. It takes
  the lease in one conditional statement that also clears the request,
  so of two instances one runs it; its record's id is the one requested,
  its due time the tick's, and what it emits is delivered after it.
- **The wait.** The command reads the record every 200 ms, timing the
  wait on its own clock so the two machines' clocks never have to agree.
  A tick has to take the request within twice the period the application
  ticks at (the ticks-stopped alert's limit: 2 minutes for a minute's
  tick), or longer while another run holds the lease, up to the longest
  lease, 16 minutes, and that again, since it can't know the deployment's
  budget; otherwise it withdraws the request, unless a tick took it
  meanwhile, and fails with `schedules.no_tick`: "No tick picked up
  `<name>`'s run in 2 min", its next step to check the app is running
  and run it again. Withdrawing keeps a request from running whenever the
  application next starts, perhaps much later. Once taken, it waits for
  the run to end; a run still going past the lease's length was lost with
  its worker (`schedules.run_failed`). A command interrupted while it
  waits leaves the request, which the next tick runs.
- **What it shows.** The run's outcome from its record, on the screens
  above: ``✓ The app's tick ran `<name>` in 12 ms, emitting 1 event``, or
  the failed run's block; a run that fails exits 1 with
  `schedules.run_failed`. `--json` carries `"requested": true`, and while
  it waits the live line says so. `history` lists a requested run once a
  tick has started it, as a run by hand (due `–`, `· by hand`), and
  `jadpo dev` prints it with `· by hand`. While a request waits,
  `jadpo why` adds `request  a run by hand, waiting for the app's next
  tick`, and `jadpo schedules --json` gives each schedule's
  `runRequested`.

## Limits and runtime faults

| Limit | Proposed value | When it is exceeded |
|---|---|---|
| Events emitted by one action | 1,000, and as many for each item's action of a batch | Defect: the action fails and rolls back |
| Size of one event | 64 KB | Defect: the action fails and rolls back |
| Events causing events, from one request | 16 deep | The delivery fails with `event_chain_limit`, stopping loops; paging uses batches, not chains |
| One run of a subscriber or schedule | The time limit of the route or integration it calls, at most 15 minutes and what is left of its invocation's budget ("Running background work") | Treated as a timeout: retried if nothing outside was called, otherwise `outcome_unknown` |
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

Built in chunk 6.1b: the errors of events and subscribers. Built in chunk 6.2: the errors of schedules. Built in chunk 7.7c: the errors of queues received.

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

### `receiving.invalid`

A queue in `receiving:` is read through a connection to a package that
provides `receiving` or to Cloudflare Queues (`driver:
Driver.cloudflare_queues`), says where each message's id is with `id_at:`, and
lists the events its messages may be; with more than one event, `type_at:`
says where each message's type is. A database's connection reads no queue:

```jadpo check project=e-receiving-invalid file=app.jadpo expect=receiving.invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
    }
    receiving: {
        main: {
            id_at: "message_id"
            events: []
        }
    }
}
```

```text output
error: `main` isn't a connection a queue is read through
 --> app.jadpo:8:9
  |
8 |         main: {
  |         ^^^^ not a queue's connection
  reason: A queue in `receiving:` is a connection to a package that provides `receiving`, or one with `driver: Driver.cloudflare_queues`; it gives `id_at:`, where each message's id is in its body, `events:`, the events its messages may be, and `type_at:`, where each message's type is, unless it lists one event.
  next: Add `main` to `connections:` with the `package:` its queue is read through, or `driver: Driver.cloudflare_queues`, or name one that is (agent)
  rule: receiving.invalid
  help: docs/language/12-jobs-and-events.html#receiving-invalid

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

`trigger:` is a `ScheduleTrigger`:

```jadpo check project=e-schedules-trigger file=app.jadpo expect=schedule.settings_invalid
application {
    schedules: {
        trigger: ScheduleTrigger.cron
    }
}
```

```text output
error: `ScheduleTrigger.cron` isn't a schedule trigger
 --> app.jadpo:3:18
  |
3 |         trigger: ScheduleTrigger.cron
  |                  ^^^^^^^^^^^^^^^^^^^^ not a `ScheduleTrigger`
  reason: A native deployable ticks on the server's own timer, `ScheduleTrigger.native`, or when another scheduler runs its `tick` command, `ScheduleTrigger.external`; a Worker always ticks on Cloudflare's cron.
  next: Use `ScheduleTrigger.external` instead (agent)
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

An entity's built-in purge schedule keeps its state in the same store, so
with several databases a lifecycle that purges needs `schedules: { store: }`
too:

```jadpo check project=e-purge-schedule-store file=app.jadpo hidden
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

```jadpo check project=e-purge-schedule-store file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-purge-schedule-store file=domains/planning/entities/todo.jadpo expect=schedule.store_required
/// Something to do.
entity Todo {
    id: Uuid
    deleted_at: Instant?

    store: main

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

```text output
error: `Todo`'s purge has no database named to keep its state in
 --> domains/planning/entities/todo.jadpo:18:9
   |
18 |         purge: {
   |         ^^^^^ a schedule with no store for its state
  reason: Purge runs as a built-in schedule every hour, and each schedule keeps its next due time and the lease that lets one instance run it at a time in a database; this application has `archive` and `main`, and `app.jadpo` names none with `schedules: { store: }`.
  next: Add `schedules: { store: main }` to `application { }` in `app.jadpo` (human)
  rule: schedule.store_required
  help: docs/language/12-jobs-and-events.html#schedule-store-required

check failed: 1 error, 3 files
```

### `step.context`

Several changing calls are made only where a retry can carry on from the
last answer: a subscriber, or a route whose API requires
`Idempotency-Key`. A route of an API that doesn't require it gets a new key
each time its client tries again:

```jadpo check project=e-step-context file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-context file=config.jadpo hidden
config {
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-context file=policy.jadpo hidden
use bakery

policy {
    actions: {
        bakery.file_prices: [Access.public]
    }
}
```

```jadpo check project=e-step-context file=domains/bakery/types/sheet_text.jadpo hidden
/// A price sheet as a supplier sends it.
type SheetText = Text {
    min_length: 1
    max_length: 100000
}
```

```jadpo check project=e-step-context file=domains/bakery/types/price_sheet_upload.jadpo hidden
use bakery/types { SheetText }

/// A price sheet a supplier uploads.
type PriceSheetUpload = Input {
    sheet_text: SheetText
}
```

```jadpo check project=e-step-context file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-context file=api/main.jadpo_record hidden
// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

GET /openapi.json
    generated openapi
    authentication none
    status 200

POST /price-sheets
    authentication none
    input PriceSheetUpload (body)
    status 204
    fails sheet_not_kept

input PriceSheetUpload
    sheet_text: Text { min_length: 1, max_length: 100000 }

failure sheet_not_kept
    status 503
    message "There was no room to keep the sheet."
```

```jadpo check project=e-step-context file=domains/bakery/actions/file_prices.jadpo
use app { sheets }
use bakery/types { PriceSheetUpload }
use bakery/failures { SheetNotKept }

/// Keeps a supplier's price sheet, then a copy for the accountant.
action file_prices(sent_sheet: PriceSheetUpload) fails SheetNotKept -> Unit {
    var sheet: File = attempt sheets.put(encoding.to_utf8(sent_sheet.sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    var invoice_copy: File = attempt sheets.put(
        encoding.to_utf8(sent_sheet.sheet_text),
        FileType.csv
    ) {
        storage_full: SheetNotKept
    }
    assert sheet != invoice_copy
}
```

```jadpo check project=e-step-context file=domains/bakery/routes/price_sheets.jadpo expect=step.context
use bakery/types { PriceSheetUpload }
use bakery/actions { file_prices }

/// Where suppliers send their price sheets.
routes PriceSheets {
    prefix: /price-sheets

    /// Keeps a supplier's price sheet.
    POST / {
        authentication: RouteAuthentication.none
        input: PriceSheetUpload
        status: SuccessStatus.no_content
        run: file_prices(input)
    }
}
```

```text output
error: `POST /price-sheets` runs `file_prices`, which makes 2 changing calls, but API `main` doesn't require `Idempotency-Key`
 --> domains/bakery/routes/price_sheets.jadpo:9:5
  |
9 |     POST / {
  |     ^^^^^^ a retry can't resume them
  reason: Several changing calls are each recorded under their action's key, so a retry carries on from the last answer instead of sending them again; a route keeps its key across a retry only when its API requires the client's `Idempotency-Key`.
  next: Require the key with `idempotency: Idempotency.required` on API `main`, or answer `202` and make the calls in a subscriber (human)
  rule: step.context
  help: docs/language/12-jobs-and-events.html#step-context

check failed: 1 error, 8 files
```

A schedule's retry is its next run, under a key of its own, and a run
keeps each item to one changing call, so a schedule and a `batches.each`
whose action makes several are refused the same way, at the schedule and
at the run's call.

### `step.encryption_key_required`

The answers of an action with several changing calls are kept, encrypted,
until it commits, so `app.jadpo` names the `SigningKey` setting they are
encrypted with (decided 2026-10-20). The error is reported once, at the
first such action:

```jadpo check project=e-step-encryption-key-required file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
    events: {
        store: main
    }
}
```

```jadpo check project=e-step-encryption-key-required file=config.jadpo hidden
config {
    main_url: PostgresUrl
}
```

```jadpo check project=e-step-encryption-key-required file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-encryption-key-required file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-encryption-key-required file=domains/bakery/events/prices_changed.jadpo hidden
use bakery/types { PriceList }

/// A supplier sent new prices.
type PricesChanged = Event {
    price_list: PriceList
}
```

```jadpo check project=e-step-encryption-key-required file=domains/bakery/events/events.jadpo_record hidden
// Generated by Jadpo from the events of domain bakery; update it with `jadpo records update`, never by hand.
record events 1

event PricesChanged
    price_list: Text { min_length: 1, max_length: 2000 }
```

```jadpo check project=e-step-encryption-key-required file=domains/bakery/subscribers/file_new_prices.jadpo hidden
use bakery/actions { file_prices }
use bakery/events { PricesChanged }

/// Files each supplier's new prices.
subscriber file_new_prices {
    on: PricesChanged
    run: file_prices(event)
}
```

```jadpo check project=e-step-encryption-key-required file=domains/bakery/actions/file_prices.jadpo expect=step.encryption_key_required
use app { sheets }
use bakery/failures { SheetNotKept }
use bakery/events { PricesChanged }

/// Keeps a supplier's new prices as a sheet, then a copy for the
/// accountant.
action file_prices(event: PricesChanged) fails SheetNotKept -> Unit {
    var sheet: File = attempt sheets.put(encoding.to_utf8(event.price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var invoice_copy: File = attempt sheets.put(encoding.to_utf8(event.price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    assert sheet != invoice_copy
}
```

```text output
error: `file_prices` makes 2 changing calls, but `app.jadpo` names no key to encrypt their answers with
 --> domains/bakery/actions/file_prices.jadpo:8:31
  |
8 |     var sheet: File = attempt sheets.put(encoding.to_utf8(event.price_list), FileType.csv) {
  |                               ^^^^^^^^^^ its answers are kept
  reason: Each answer of an action with several changing calls is kept, encrypted, until the action commits, so a retry can replay it; `application { steps: { encryption_key: } }` names the `SigningKey` setting it is encrypted with, and `file_prices` is the first action that needs it.
  next: Add `steps: { encryption_key: config.step_encryption_key }` to `application { }` in `app.jadpo`, and declare `step_encryption_key: SigningKey` in `config.jadpo` (human)
  rule: step.encryption_key_required
  help: docs/language/12-jobs-and-events.html#step-encryption-key-required

check failed: 1 error, 7 files
```

### `step.large_output`

An answer kept for a rerun is at most 1 MB, so an operation its package
marks `large_output: true` is never one of several changing calls:

```jadpo check project=e-step-large-output file=app.jadpo hidden
application {
    connections: {
        pay: {
            package: AcmePay.v1
            domains: [bakery]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-large-output file=config.jadpo hidden
config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
public resource payments {
    create: {
        method: HttpMethod.post
        path: "/v1/payments"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/resources/bank_statements.jadpo hidden
public resource bank_statements {
    export: {
        method: HttpMethod.post
        path: "/v1/bank_statements"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        large_output: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=e-step-large-output file=domains/bakery/actions/pay_supplier.jadpo expect=step.large_output
use app { pay }
use shared/providers/acme_pay

/// Pays a supplier's invoice, then exports the month's statement.
action pay_supplier(invoice: AcmePay.Reference) -> AcmePay.Payment {
    var payment: AcmePay.Payment = pay.payments.create(
        AcmePay.PaymentRequest {
            reference: invoice
        }
    )
    var bank_statement: AcmePay.Payment = pay.bank_statements.export(
        AcmePay.PaymentRequest {
            reference: payment.id
        }
    )
    assert bank_statement.id == payment.id
    return payment
}
```

```text output
error: `pay.bank_statements.export` gives a large answer, and `pay_supplier` makes several changing calls
 --> domains/bakery/actions/pay_supplier.jadpo:11:43
   |
11 |     var bank_statement: AcmePay.Payment = pay.bank_statements.export(
   |                                           ^^^^^^^^^^^^^^^^^^^^^^^^^^ `large_output: true`
  reason: In an action with several changing calls, each answer is kept, up to 1 MB, for a retry to replay, and its package marks this operation's answer as large.
  next: Move `pay.bank_statements.export` to an action of its own, run by a subscriber of an event `pay_supplier` emits (agent)
  rule: step.large_output
  help: docs/language/12-jobs-and-events.html#step-large-output

check failed: 1 error, 10 files
```

### `step.late_failure`

After an action's first changing call has taken effect, a failure that
ends it for good rolls back its writes but not what the call did, and a
retry fails the same way. Such a point (an `attempt` whose call or write
can fail with a failure that isn't retried, `reject`, `propagate`) is an
error until it is moved before the changing call, handled with `match`,
or declined. Here the shop could be checked before the sheet is kept:

```jadpo check project=e-step-late-failure file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/types/shop_state.jadpo hidden
/// Whether a shop is trading this week.
type ShopState = Enum {
    open
    closed
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/failures/shop_closed.jadpo hidden
/// The shop isn't trading this week.
type ShopClosed = Failure {
    kind: FailureKind.rejected
    message: "The shop is closed this week."
}
```

```jadpo check project=e-step-late-failure file=domains/bakery/actions/keep_price_sheet.jadpo expect=step.late_failure
use app { sheets }
use bakery/types { PriceList, ShopState }
use bakery/failures { SheetNotKept, ShopClosed }

/// Keeps the week's price sheet for a shop that is trading.
action keep_price_sheet(
    price_list: PriceList,
    state: ShopState
) fails SheetNotKept, ShopClosed -> File {
    var sheet: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    if (state == ShopState.closed) {
        reject ShopClosed
    }
    return sheet
}
```

```text output
error: `keep_price_sheet` can fail for good at `reject ShopClosed`, after `sheets.put` took effect
 --> domains/bakery/actions/keep_price_sheet.jadpo:14:9
   |
10 |     var sheet: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
   |                               ---------- takes effect first
14 |         reject ShopClosed
   |         ^^^^^^^^^^^^^^^^^ can fail with `ShopClosed`
  reason: Once a changing call has taken effect, a failure that ends `keep_price_sheet` for good rolls back its writes but not what `sheets.put` did, and a retry fails the same way; a failure of kind `unavailable` or `rate_limited` is retried instead, and one handled with `match` doesn't end it.
  next: Check the condition before `sheets.put`, record what happened and finish in a subscriber of an event `keep_price_sheet` emits, or undo `sheets.put` as another step in a `match` arm, or write `decline step.late_failure: "<why>"` above it if this is deliberate (agent)
  rule: step.late_failure
  help: docs/language/12-jobs-and-events.html#step-late-failure

check failed: 1 error, 6 files
```

### `step.settings_invalid`

`steps:` holds one setting, `encryption_key:`, which reads a `SigningKey`
setting:

```jadpo check project=e-step-settings-invalid file=config.jadpo hidden
config {
    step_encryption_key: Text {
        secret: true
    }
}
```

```jadpo check project=e-step-settings-invalid file=app.jadpo expect=step.settings_invalid
application {
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```text output
error: `encryption_key:` reads `step_encryption_key`, which isn't a `SigningKey`
 --> app.jadpo:3:25
  |
3 |         encryption_key: config.step_encryption_key
  |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^ `step_encryption_key` is `Text`
  reason: A record's answers are encrypted with a `SigningKey` setting: secret, at least 32 characters, and replaced by moving the old value to the variable with `_PREVIOUS` after its name (area 10), as grants' `encryption_key:` is.
  next: Declare `step_encryption_key: SigningKey` in `config.jadpo` (agent)
  rule: step.settings_invalid
  help: docs/language/12-jobs-and-events.html#step-settings-invalid

check failed: 1 error, 2 files
```

### `step.store_required`

Each changing call is recorded beside the outbox of the store its action
writes, so an action that writes none needs `events: { store: }`:

```jadpo check project=e-step-store-required file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-store-required file=config.jadpo hidden
config {
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-store-required file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-store-required file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-store-required file=domains/bakery/events/prices_changed.jadpo hidden
use bakery/types { PriceList }

/// A supplier sent new prices.
type PricesChanged = Event {
    price_list: PriceList
}
```

```jadpo check project=e-step-store-required file=domains/bakery/events/events.jadpo_record hidden
// Generated by Jadpo from the events of domain bakery; update it with `jadpo records update`, never by hand.
record events 1

event PricesChanged
    price_list: Text { min_length: 1, max_length: 2000 }
```

```jadpo check project=e-step-store-required file=domains/bakery/subscribers/file_new_prices.jadpo hidden
use bakery/actions { file_prices }
use bakery/events { PricesChanged }

/// Files each supplier's new prices.
subscriber file_new_prices {
    on: PricesChanged
    run: file_prices(event)
}
```

```jadpo check project=e-step-store-required file=domains/bakery/actions/file_prices.jadpo expect=step.store_required
use app { sheets }
use bakery/failures { SheetNotKept }
use bakery/events { PricesChanged }

/// Keeps a supplier's new prices as a sheet, then a copy for the
/// accountant.
action file_prices(event: PricesChanged) fails SheetNotKept -> Unit {
    var sheet: File = attempt sheets.put(encoding.to_utf8(event.price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var invoice_copy: File = attempt sheets.put(encoding.to_utf8(event.price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    assert sheet != invoice_copy
}
```

```text output
error: `file_prices` makes 2 changing calls but writes no store to keep their record in
 --> domains/bakery/actions/file_prices.jadpo:8:31
  |
8 |     var sheet: File = attempt sheets.put(encoding.to_utf8(event.price_list), FileType.csv) {
  |                               ^^^^^^^^^^ no store for the record
  reason: Each changing call is recorded before it goes out, beside the outbox of the store its action writes, so a retry can replay it; `file_prices` writes no store, itself or through what it calls, and `app.jadpo` names none with `events: { store: }`.
  next: Add `events: { store: main }` to `application { }` in `app.jadpo` (human)
  rule: step.store_required
  help: docs/language/12-jobs-and-events.html#step-store-required

check failed: 1 error, 7 files
```

### `step.too_many`

An action makes at most 8 changing calls on one way through it, counting
those of the actions it calls:

```jadpo check project=e-step-too-many file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-too-many file=config.jadpo hidden
config {
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-too-many file=domains/bakery/types/price_list.jadpo hidden
/// A supplier's prices, one line for each loaf.
type PriceList = Text {
    min_length: 1
    max_length: 2000
}
```

```jadpo check project=e-step-too-many file=domains/bakery/failures/sheet_not_kept.jadpo hidden
/// The sheets store had no room for a sheet.
type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
```

```jadpo check project=e-step-too-many file=domains/bakery/actions/share_prices.jadpo expect=step.too_many
use app { sheets }
use bakery/types { PriceList }
use bakery/failures { SheetNotKept }

/// Keeps the week's price sheet for each of the bakery's nine shops.
action share_prices(price_list: PriceList) fails SheetNotKept -> List<File> {
    var bakehouse: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var market: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var station: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var harbour: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var castle: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var bridge: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var mill: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var abbey: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    var quayside: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
        storage_full: SheetNotKept
    }
    return [bakehouse, market, station, harbour, castle, bridge, mill, abbey, quayside]
}
```

```text output
error: `share_prices` makes 9 changing calls in a row, more than 8
 --> domains/bakery/actions/share_prices.jadpo:31:34
   |
31 |     var quayside: File = attempt sheets.put(encoding.to_utf8(price_list), FileType.csv) {
   |                                  ^^^^^^^^^^ the 9th changing call
  reason: Each changing call is a step its action records before sending and replays on a retry, and an action makes at most 8 on any one way through it, counting those of the actions it calls.
  next: Finish part of the work in a subscriber of an event `share_prices` emits (agent)
  rule: step.too_many
  help: docs/language/12-jobs-and-events.html#step-too-many

check failed: 1 error, 5 files
```

### `step.unkeyed`

A changing call with no idempotency key is the only changing call in its
action, so a receipt sent after a payment belongs in a subscriber of the
payment's event:

```jadpo check project=e-step-unkeyed file=app.jadpo hidden
application {
    connections: {
        pay: {
            package: AcmePay.v1
            domains: [bakery]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-unkeyed file=config.jadpo hidden
config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
public resource payments {
    create: {
        method: HttpMethod.post
        path: "/v1/payments"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/resources/receipts.jadpo hidden
public resource receipts {
    send: {
        method: HttpMethod.post
        path: "/v1/receipts"
        input: PaymentRequest
        output: Payment
        changes: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=e-step-unkeyed file=domains/bakery/actions/pay_supplier.jadpo expect=step.unkeyed
use app { pay }
use shared/providers/acme_pay

/// Pays a supplier's invoice, then sends them a receipt.
action pay_supplier(invoice: AcmePay.Reference) -> AcmePay.Payment {
    var payment: AcmePay.Payment = pay.payments.create(
        AcmePay.PaymentRequest {
            reference: invoice
        }
    )
    var receipt: AcmePay.Payment = pay.receipts.send(
        AcmePay.PaymentRequest {
            reference: payment.id
        }
    )
    assert receipt.id == payment.id
    return payment
}
```

```text output
error: `pay.receipts.send` takes no idempotency key, but `pay_supplier` makes another changing call, `pay.payments.create`
 --> domains/bakery/actions/pay_supplier.jadpo:11:36
   |
11 |     var receipt: AcmePay.Payment = pay.receipts.send(
   |                                    ^^^^^^^^^^^^^^^^^ no key
  reason: A changing call with no key may act twice if it is sent again, and once its answer is lost only an operator can say whether it went through, so it stays the only changing call in its action.
  next: Move `pay.receipts.send` to a subscriber of an event `pay_supplier` emits (agent)
  rule: step.unkeyed
  help: docs/language/12-jobs-and-events.html#step-unkeyed

check failed: 1 error, 10 files
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
    order: [Item.id asc]
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
    order: [Item.id asc]
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
the run. (A file helper's write's `each:` is one inline function giving
the item's row instead, chunk 6.4b2, refused when it is anything else.)
One that works something out of the item is refused:

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
    order: [Item.id asc]
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
    order: [Item.id asc]
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

### `type.batch_input`

Only the runtime builds a batch type, and `batches.status` and
`batches.cancel` find a run by its `id` alone, so no request carries one:
an `Input`, read from a body or a query string, and a route's placeholder
may not hold a `BatchRun`, a `BatchProgress`, a `RowProblem` or an
`ImportSummary`, however deep (chunk 6.6b). A run comes from
`batches.each`, or from a field the caller may read where the
application kept it:

```jadpo check project=e-batch-input file=stock.jadpo hidden
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
    order: [Item.id asc]
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

```jadpo check project=e-batch-input expect=type.batch_input
type StopSweep = Input {
    shelf_run: BatchRun
}

action stop_sweep(request: StopSweep) -> Unit {
    batches.cancel(request.shelf_run)
}
```

```text output
error: `StopSweep.shelf_run` holds a `BatchRun`, which no request carries
 --> main.jadpo:2:16
  |
2 |     shelf_run: BatchRun
  |                ^^^^^^^^ a `BatchRun`
  reason: Only the runtime builds a `BatchRun`: a run comes from `batches.each`, or from a field the caller may read where the application kept it, and `batches.status` and `batches.cancel` find it by its `id` alone. A request that named one could read or stop any run.
  next: Take the `id` of the record that keeps the run, and read the `BatchRun` from its field (agent)
  rule: type.batch_input
  help: docs/language/12-jobs-and-events.html#type-batch-input

check failed: 1 error, 2 files
```

### `type.batch_order_updatable`

A run walks live records a page at a time, each page after the record
the one before ended on, so a `from:` whose query orders by a field an
update can change can miss a record whose field moved behind that place
mid-run, or meet it twice. An order no update changes keeps every record
in exactly once: `id`, a field marked `immutable: true`, or a time
generated as the record is created, then `id` (chunk 6.4b3; "How a run
is kept"):

```jadpo check project=e-batch-order-updatable file=stock.jadpo hidden
type ShelfCode = Text {
    min_length: 1
    max_length: 40
}

type StockCount = Int {
    min: 0
    max: 100000
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
    stock: StockCount
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
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

```jadpo check project=e-batch-order-updatable expect=type.batch_order_updatable
/// The items, a page at a time, in code order.
query items_by_code(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

/// Starts marking every item seen.
action sweep_shelves(page: ItemPage) -> BatchRun {
    return batches.each(from: items_by_code(page), each: item -> mark_seen(item))
}
```

```text output
error: `batches.each` walks `items_by_code` in an order an update can change
 --> main.jadpo:13:31
   |
13 |     return batches.each(from: items_by_code(page), each: item -> mark_seen(item))
   |                               ^^^^^^^^^^^^^^^^^^^ ordered by `Item.code`, which an update can change
  reason: A run walks live records a page at a time, each page after the record the one before ended on, so a record whose `Item.code` changes mid-run can move behind that place and be missed, or ahead of it and be met twice. An order no update changes meets every record once: `id`, or a time generated as the record is created, then `id`.
  next: Order `items_by_code` by `Item.id`, or by a field marked `immutable: true` or generated with `Generated.on_create` and then `Item.id`, or write `decline type.batch_order_updatable: "<why>"` above it if this is deliberate (agent)
  rule: type.batch_order_updatable
  help: docs/language/12-jobs-and-events.html#type-batch-order-updatable

check failed: 1 error, 2 files
```

It is a judgement call, so it can be declined on the action, with the
reason the order is safe. Here the walk goes from the most stock to the
least while sales only ever lower an item's stock: a sale moves an item
further along the walk, so it can be met twice but never missed, and
marking an item seen twice does no harm:

```jadpo check project=batch-order-declined file=stock.jadpo hidden
type StockCount = Int {
    min: 0
    max: 100000
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
    stock: StockCount
    seen: Bool

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
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

```jadpo check project=batch-order-declined
/// The items, a page at a time, the most stock first.
query items_by_stock(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.stock desc]
    page: {
        size: page.page_size
        after: page.after
    }
}

/// Starts marking every item seen, the best stocked first.
decline type.batch_order_updatable: "Sales only lower stock, which moves an item further along the walk, never past it; marking one seen twice does no harm"
action sweep_by_stock(page: ItemPage) -> BatchRun {
    return batches.each(from: items_by_stock(page), each: item -> mark_seen(item))
}
```

### `type.batch_row_invalid`

A file helper reads each row as the `Input` its `as:` names, and writes
each as the `Output` its `each:` gives (chunk 6.4b2). A CSV or TSV row
holds one value a cell, so a row type holding a list, a map, an object
or a standard type with no one text form is refused, pointing at JSON
Lines, whose rows hold any JSON value; so is a row type that isn't an
`Input` (or, written, an `Output`), or two fields reading or writing one
column. From chunk 6.4b3 the same rule refuses `json.read`'s `items_at:`
when it isn't member names written out between dots (`"data.items"`),
and a reader's `max_unpacked_size:` when it isn't a size (`200mb`):

```jadpo check project=e-batch-row-invalid file=app.jadpo hidden
application {
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/bakery/sheets"
        }
    }
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/types/order_code.jadpo hidden
/// A line's code on an order.
type OrderCode = Text {
    min_length: 1
    max_length: 20
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/types/code_row.jadpo hidden
use bakery/types { OrderCode }

/// One code a sheet lists.
type CodeRow = Input {
    reference: OrderCode
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/entities/order_sheet.jadpo hidden
/// A sheet of order_sheet, kept as it was sent.
entity OrderSheet {
    id: Uuid
    sheet: File {
        store: sheets
        max_size: 5mb
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/take_code.jadpo hidden
use bakery/types { CodeRow }

/// Takes one code a sheet lists.
action take_code(listed_code: CodeRow) -> Unit {
    assert listed_code.reference == listed_code.reference
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/read_codes.jadpo hidden migrate=initial
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { CodeRow }
use bakery/actions { take_code }

/// Starts taking the codes a kept sheet lists.
action read_codes(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var code_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return csv.read(code_sheet.sheet, as: CodeRow, each: listed_code -> take_code(listed_code))
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/types/order_row.jadpo
use bakery/types { OrderCode }

/// One order a sheet lists, with its lines.
type OrderRow = Input {
    reference: OrderCode
    lines: List<OrderCode>
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/take_order.jadpo hidden
use bakery/types { OrderRow }

/// Takes one order a sheet lists.
action take_order(order_entry: OrderRow) -> Unit {
    assert order_entry.reference == order_entry.reference
}
```

```jadpo check project=e-batch-row-invalid file=domains/bakery/actions/read_orders.jadpo expect=type.batch_row_invalid
use bakery/entities { OrderSheet, OrderSheetNotFound }
use bakery/types { OrderRow }
use bakery/actions { take_order }

/// Starts taking the orders a kept sheet lists.
action read_orders(sheet_id: OrderSheet.id) fails OrderSheetNotFound -> BatchRun {
    var order_sheet: OrderSheet = attempt OrderSheet.get(sheet_id)
    return csv.read(order_sheet.sheet, as: OrderRow, each: order_entry -> take_order(order_entry))
}
```

```text output
error: `csv.read` can't read its rows as `OrderRow`
 --> domains/bakery/actions/read_orders.jadpo:8:44
  |
8 |     return csv.read(order_sheet.sheet, as: OrderRow, each: order_entry -> take_order(order_entry))
  |                                            ^^^^^^^^ `OrderRow.lines` holds a list
  reason: A CSV row holds one value a cell, and `OrderRow.lines` holds a list; a JSON Lines row holds lists and objects, each line one JSON value.
  next: Read the file with `json_lines.read`, or give `lines` a type one cell holds (agent)
  rule: type.batch_row_invalid
  help: docs/language/12-jobs-and-events.html#type-batch-row-invalid

check failed: 1 error, 9 files
```

## Review

### Status

Built in Phase 6 (chunks 6.1a to 6.10): the runner and the system caller
(area 9); `emit`, the outbox, subscribers, delivery after the response,
handled records, retries, trace context and `events: { store: }`, with the
errors that name `event.` and `subscriber.`; each domain's event record
and what a subscriber stores from its event; schedules, their state and
leases, the native tick and its sweeps, every run and skipped tick
recorded, with the errors that name `schedule.`; watching schedules,
`jadpo schedules` with `run` (against `jadpo dev` or a deployment),
`pause`, `resume` and `history`, `jadpo why <schedule>`, each run and
alert in `jadpo dev`'s output, the alerts with no setup and the metrics;
each entity's purge schedule and a test's `schedule.run()`;
`batches.each`, `batches.status`, `batches.cancel` and `key:`, with
tenants, grants, what a run stores under policy and no request carrying
a batch type; background work within its host's budget and a native
server draining as it stops; records a later release still reads; and
alerts raised by the tick, once across instances, with `jadpo why`'s
answer decided by the runtime; and the tick deleting handled records once
their delivery's record is gone, and area 8's ended sessions, tokens and
expired keys. Built in chunk 6.4b: the file helpers' reads, `csv.read`,
`tsv.read` and `json_lines.read`, with `column:`, a file row's problems
by its row and column, the faults that stop a file's run, and
`type.batch_row_invalid` ("How a file's run is kept"). Built in chunk
6.4b2: the writers, `csv.write`, `tsv.write`, `json_lines.write` and
`json.write`'s file form, with `CsvFormat` and `ExportSummary`, each
page's rows put once its transaction has closed ("How an export's run is
kept"). Built in chunk 6.13: the tick deleting what an operator settled,
and the operator's log, after 30 days. Built in chunk 6.4b3: `json.read`'s
file form, gzip read with `max_unpacked_size:`, `estimated_total`, and
`type.batch_order_updatable` for a `from:` ordered by a field that can be
updated, declinable. Built in chunk 7.2b: the outbox's circuit breaker
per connection ("When a provider keeps failing"), held deliveries in
`jadpo deliveries`, and its alert. Built in chunk 7.5: what `jadpo check`
keeps of several changing calls in one action, with the errors that name
`step.` ("Several changing calls in one action"). Built in chunk 7.5c:
unfinished records as operator items, `new_key_on:` and `jadpo runs
unfinished` ("What an operator sees"). Built in chunk 7.5d: fakes keeping
keys with `lost` and `in_use`, and `jadpo test`'s crash at each step
boundary (area 13). Built in chunk 7.5e: the late-failure warning,
`step.late_failure`, and the sum of a route's outside calls against its
deadline ("Several changing calls in one action").
Built in chunk 7.5f: each step's state under its
request's or delivery's line in `jadpo dev`, and the metric and alert
on unfinished records ("Several changing calls in one action").
Built in chunk 7.9a, from the Phase 7 soundness review: a route's action
that calls out keeping its client's key, a delivery's attempt keying its
calls the same wherever it runs, a sign-in email keyed per attempt, a
failure that did nothing left in doubt, the runtime's own timestamps on
the host's clock, and an unkeyed change never sent again on a rerun
("Several changing calls in one action"; live tests in
`crates/jadpo-cli/tests/step_replays.rs`; decisions table, "Keys and
replays, chosen in chunk 7.9a").
Built in chunk 8.2h: on Workers, what an invocation's 30 seconds didn't
reach handed on to a fresh invocation of the Worker, a request to itself
naming each piece by its record ("Running background work"; decisions
table, "Work handed on, on Workers"); proved under workerd with a
subscriber's deliveries and a batch run each longer than one
invocation's budget (`crates/jadpo-cli/tests/workers/newsroom.rs`).

Not yet:

- A test's `fakes:` failing a subscriber's provider call: roadmap 7.2,
  with its generated fakes. Delivering events in tests after each step,
  in an order the printed seed shuffles, and retrying or giving up a
  call a fake fails, is built (area 13, "Caches and events"; chunk 6.11).
- `Retry-After`, polls, `receiving`, publishing outward, webhooks and a
  package's own events come with Phase 7; their errors are built with
  them. The warning and the deadline sum come with roadmap 7.5e; each
  step's state in `jadpo watch` with `jadpo watch`, and in `jadpo trace`
  with the request timelines (area 17), and the record's entry in the
  personal-data report with the report (area 15), all after the first
  build.
- On Cloudflare Workers, the `trigger:` adapter: roadmap 8.2g. Delivery
  after commit in `waitUntil` and the budgets are built (chunks 8.2a and
  8.2d), the cron tick (chunk 8.2f), and work an invocation didn't reach
  handed on to a fresh invocation of the Worker (chunk 8.2h).
- Sending the metrics and the alerts' log events to a monitoring tool
  waits for telemetry (area 16), and the console's timeline for area 17.

### Consistency with the rest of the language

- **Setting in the header.** `job chore_census every 15m {` breaks the
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
    `from:` orders by a field that can be updated. Built in chunk 6.4b3
    as a declinable error, `type.batch_order_updatable`, since every
    finding is an error (area 1, question 11).
14. **What carries on after a route answers `status: SuccessStatus.accepted`?**
    (carried from area 7) Area 7 has a route answer 202 and continue the work
    in the background. Is that always an event and a subscriber, and how
    does the client learn the outcome? *Decided 2026-10-13:* yes, as recommended.
15. **How do tests run schedules and subscribers?** (carried from area 13)
    Area 13 calls `chore_census.run()`, relies on subscriber delivery
    order and needs retries of a failing subscriber. Does this page define
    those, and in what order are subscribers delivered in tests?
    *Decided 2026-10-13:* `schedule.run()` runs a schedule under the test
    clock; events are delivered after each commit; a fake can fail a
    subscriber to test retries; delivery order is shuffled with a printed
    seed, so a test that depends on order fails visibly. Built in chunk
    6.11 (area 13, "Caches and events"), a fake's failure on 7.0c's
    fakes; a test scripts one with `fakes:` from roadmap 7.2.
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
    *Richard (2026-10-21):* `ScheduleTrigger.external` is the native
    deployable's `tick` command: `<deployable> tick` reads the settings,
    checks migrations as `serve` does, runs one tick against the
    schedules' store and exits, so a Kubernetes CronJob, ECS task or
    systemd timer runs it each minute. No HTTP endpoint and no tick
    secret; a scheduler that can only call a URL needs its own wrapper.
    On Workers the cron trigger calls the Worker's scheduled handler.
    Built in chunk 8.2g ("The tick"; "The `trigger:` adapter" in
    Decisions).
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
    Built in chunk 7.2b, its thresholds chosen there ("When a provider
    keeps failing").
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

| Topic | Decision | Date |
|---|---|---|
| Multi-step calls' encryption key | Recorded answers are encrypted with `application { steps: { encryption_key: config.step_encryption_key } }`, a `SigningKey` setting rotated as grants' `encryption_key:` is; required only when some action makes several changing calls, a compile error naming the first such action otherwise (decided for Richard, following grants) | 2026-10-20 |
| Keys and replays, chosen in chunk 7.9a | From the Phase 7 soundness review, each the conservative reading. A route's action keeps its client's `Idempotency-Key` in the store it writes, else beside its events, else, when it makes a changing call (a provider's operation with `changes: true`, a store's `put`), in the store `events: { store: }` names, where its record of several steps is kept too, so its steps are resumed under the key and one call's response is kept for a repeat; an action that only reads keeps none, and its OpenAPI says where the key is kept. With no store to keep it, the route's calls' keys are still derived from the client's key in its caller's scope, never the request's id. A delivery's attempt counts its keyed calls from none under its own key and with none of the `ai` answers kept for a rerun, wherever it runs (a route waiting with `settle:`, a test), giving back whoever ran it theirs after. A sign-in email (`MagicLinkEmail`, and the action `send:` names) is keyed per attempt, from fresh randomness, since each attempt makes a link and a code that end the last's: a provider replaying the last attempt's key would leave the person a dead code; its key doesn't survive a retry, so it keeps no record of its steps. A step's own failure whose kind is `unavailable` or `rate_limited` is never kept, and one kept by an earlier runtime never replayed: the step stays in doubt and goes again under its key. Code's `clock.now` is the record's fixed time; an event's `emitted_at`, a delivery's handled record, a request's AWS signature, the day an `ai` budget counts and a received event's stamp are the host's. An unkeyed change that went out (answered, or no answer) stops a rerun after a concurrent change, in the boundary or at a held commit, as `outcome_unknown`, which a route keeps for its key and a delivery parks; areas 6, 12 and 16 say the same. In `jadpo test` how often each scripted operation and prompt was called is judged on the run without a crash only, since a crash sends its step again and the `ai` fake, or a call with no key, answers it afresh; a request a crash sends again forgets the `ai` answers kept for a rerun, as a client's retry reaches a runner that kept none, so a model asked before the steps is asked again there as in production (decided for Richard) | 2026-10-21 |
| One outside-call path, chosen in chunk 7.9d | From the Phase 7 architecture review, each the conservative reading. Every outside call (a provider's operation, `ai.generate`, `ai.embed`, `ai.stream`, a store's file, an email, a capability's delivery) goes out through one exchange and one retrying send, and where its answer is given back from (the action's record, an `ai` call's earlier run, a replayed run's recording) is decided in one place. `max_concurrent:` caps every call through its connection: a provider operation's, as since chunk 7.6c, and where a package fills `ai`, its `ai.generate`, `ai.embed` and `ai.stream` calls too; the built-in `Driver.openai_compatible` has no cap. A call takes a slot only once it is cleared to go (no transaction open, no recording answering it, a driver found), before its step is written, and gives it back once answered, a stream once its answer starts to come, so a later call of the same action through the connection never waits on a stream the route hasn't sent yet. A delivery whose action calls `ai` through a connection with every slot out waits for one within its budget, as a route's call does, rather than being given back to the outbox: giving back reads the connections the circuit breaker does, a provider's operations'. No wait to send a call again outlasts the invocation's time, a route's remaining deadline or what background work's budget leaves before the time it keeps back: a wait that wouldn't end within it, a `Retry-After` past it included, ends the sending at once and the last answer stands, so a `429` or a provider that couldn't be reached is `unavailable`, and no answer in time `timed_out` where the call maps it, as when `within:` or an `ai` call's `timeout:` ends it. Each send's own timeout isn't shortened: the deadline and the budget stop a call still out, as before. A store's file, an email and a capability's delivery are still sent once, having no `retry:` (the outbox retries a delivery). An embedding given back from the action's record is logged as the step it is, as the other calls are (decided for Richard) | 2026-10-21 |
| Development and the metric, chosen in chunk 7.5f | Each the conservative reading. Until area 17's request timelines (`jadpo trace`), a request's timeline in `jadpo dev` is its line and the trace lines under it, so each step a request's action reached is a trace line there, in place of the call it made; a delivery, which `jadpo dev` didn't print, gets a line of its own only when its subscriber's action reached steps under a record, since that is when one needs following, with `jadpo deliveries show <id>` under it when the attempt failed, parked or diverged. A step's state is what this run made of it, from the record: `replayed` (answered on an earlier run), `answered`, its failure's code, `in doubt` (sent, no answer kept), `diverged` or `outcome unknown` (in doubt and not to be sent again), with where its answer came from; never its request, answer or key, nor the key's generation. The metric counts the records not abandoned, a run under way included, and gives when the oldest's first run started; the tick carries it for each store that keeps records, until telemetry (area 16) sends it. The alert holds once a record has been unfinished a day from its first run's start, a subscriber's default `within:` and a route's key's lapse, so no retry with the default budget is still to finish it; each such record is marked in `jadpo_step_records.alerted_at`, and the tick raises the alert only when none still unfinished was marked before, so it is raised once until no record is unfinished past a day, by one instance's tick. `jadpo deliveries` shows it in its default list and with `--state diverged` or `unfinished` (decided for Richard) | 2026-10-20 |
| The late-failure warning and the deadline sum, chosen in chunk 7.5e | Each the conservative reading. A failure ends a run for good unless its kind is `unavailable` or `rate_limited`, the two retried ("Retries and giving up"); one whose kind isn't known counts as ending it. A point after the first changing call is an `attempt` passing a failure on (the failures its arms map to, or else the call's or write's own), `reject` and `propagate`; a failure handled by an outcome `match` isn't one. Where two ways meet, a changing call on either may have been made. One finding for each action, at its first point, the others shown beside it, so one `decline` answers it, as declines answer one finding each. A package's action is never told, since the app decides what can be left behind: its surface lists each failure that can follow its own changing call as `after_effect`, and an app's `attempt` of it is a point even before the app's own first changing call; an app's action called before any is told itself instead. The deadline sum is one analysis over every outside call a route reaches, a provider's operation and an `ai` call alike, so chunk 5.5v6's streaming routes rely on it: a provider call through a connection costs its `timeout:` (10s by default; 3s since 2026-10-20, area 11's "Default timeouts") times its `retry: { attempts: }` (3), a package's action its own calls through the connection the app called it through, and `ai.generate` its `timeout:` times its `attempts:`; the waits between attempts and `within:` aren't counted, and a store's `put` and `read` have no `timeout:` and cost nothing. The ways are summed and the longest compared with the route's `deadline:` (10s by default): more is `route.calls_past_deadline` at the route, a plain mistake, not declinable (area 1, question 11), whatever its API; equal fits. Schedules and subscribers have no deadline to compare (decided for Richard) | 2026-10-20 |
| Tests, chosen in chunk 7.5d | Each the conservative reading. A package's fake keeps a keyed call's first answer under its key, as its provider does: the same key and request are given it again without taking a scripted answer, and a different request the package's `reused:` answer (with none, a refusal without a code, a fault). A failure is kept as Stripe keeps one, whatever the package's `replays_failures:`, unless its kind is `unavailable` or `rate_limited`, so a step sent again after a crash is answered as it was the first time. `lost` acted: what it did, the generated default, answers the next call under its key. `in_use` is another runner's request under the key, the one that acts, so it keeps nothing and answers as `unavailable`; `unavailable` and `misconfigured` keep nothing either. `jadpo dev`'s fake keeps keys the same way, its last 10,000. Crash points count every step a test's run reaches under a record, a step sent again counted again, across the whole run; only a run that passed is crashed, once for each boundary of each step, each on its own copy of the database with a fresh fake, and the first crash that fails is the one reported, with a `crash` line first. A crash isn't counted as an attempt: the delivery is given back, due at once, and a route's claim is given up and its request sent again under its key. A crashed run whose fake acted more or less often than the run without one is a defect in Jadpo, reported as `test.fault`. A delivery that stops as `diverged` between a test's steps ends the test as `test.subscriber_failed`, since it never finishes; one parked as `outcome_unknown` still doesn't. Each delivery attempt in a test counts its keyed calls from none under the delivery's own key, as a runner's fresh context does, and gives the test's back after. An `ai.generate` call kept as a step (chunk 5.5v3) gets crash points like any other step; the `ai` fake keeps no keys, area 21 asking none of it, so a step sent again after a crash once it was answered takes the prompt's next scripted answer, as a model asked again answers afresh. Crash points have no switch to turn them off (decided for Richard) | 2026-10-20 |
| Operators, chosen in chunk 7.5c | Each the conservative reading. A route's record becomes an item once a day has passed since its last run took it (area 11's 24 hours), whatever its own `window:`; before that it is its client's to retry, and an operator can only abandon it, since only its client holds the request. "Not done" acts on the record's last step when it has no answer or failed, clearing its answer and moving its key to the next generation, with its window counted from the operator's action; with nothing in doubt it is a plain retry, and on a delivery with no record it is `retry` (failed) or `resolve --not-done` (in doubt). A retry resets the record's generation to none with the delivery's attempts. A delivery with a record is refused `dismiss` and `resolve --done`, pointing at `abandon`; `abandon` on one without is refused. A record whose answers were dropped can't be retried, only abandoned. Answers are dropped 30 days after the record's last run; an abandoned record and its delivery go 30 days after the abandon. A route's client retrying under an abandoned record's key stops as a fault, answered `500`, sending nothing. `new_key_on:` sends again once per call; a second such answer is the operation's failure. `show` never shows a key: the CLI reads no setting it would be derived from. `jadpo runs unfinished` requires `--build`, refuses a build the checkout doesn't make (naming the one it does), lists a record whose build differs and whose action's step shape the build changes or no longer keeps, and a running batch run whose call the build changes or no longer has; it succeeds whatever it lists, as listing commands do (decided for Richard) | 2026-10-20 |
| The record, chosen in chunk 7.5b | Each the conservative reading, where the pages left it open. A record is kept only where the action's key survives a retry: a subscriber's delivery, and a route whose API requires `Idempotency-Key` under the key it claimed; a test calling the action directly keeps none. It is one row keyed by a digest of the key's scope and the key, with a row per step, in the store the action writes or `events: { store: }`'s. A request's fingerprint is an HMAC-SHA-256 of each top-level field as the provider is sent it, by its name there, under a key derived from the steps key (checked under the current and the previous one), so a low-entropy field can't be guessed from it; an answer is sealed with XChaCha20-Poly1305 under another key derived from it (`crypto.encrypt`'s construction; grants' encryption isn't built yet), bound to the record and the step. The generation every write checks is a delivery's attempt or a route's claim time; a write takes the record over from an equal or older one only. A rerun is checked step by step (its operation and fields) and at the end (every recorded step replayed), before anything more is sent; the record keeps the build it started under (until area 18's build id, a digest of every graph node's key and digest) and a digest of its step shape for roadmap 7.5c's `jadpo runs unfinished`, and a rerun doesn't compare the digest, since steps added after the last recorded one aren't a change. A step answered "did nothing" (`unavailable`, `misconfigured`) stays in doubt, so the window rule still holds it. A `put` keeps its object under a key derived from the record and its step and goes again whatever the window. `diverged` is a runtime fault that holds a delivery in the state `diverged` with the step, its operation and the fields; a route answers it `500` and keeps no response, so a client's retry after a fix finishes it. `new_key_on:` comes with 7.5c's new key generation (decided for Richard) | 2026-10-20 |
| Triggers | Carried from area 1: routes, schedules (replacing "jobs") and subscribers each call actions; durable workflows are subscribers | 2026-10-12 |
| Keyword and folders | Carried from area 1: `schedule` in `schedules/`; events in `events/`; subscribers in `subscribers/` | 2026-10-12 |
| Cross-domain changes | Carried from area 1: public actions inside the caller's transaction, or events for reactions that need not be all-or-nothing | 2026-10-12 |
| Actions | Carried from area 3: every action is all-or-nothing and has no settings; `attempt` only on calls that can fail | 2026-10-12 |
| Faults | Carried from area 4: faults are never listed in `fails` or matched; integrations map provider problems to declared failures | 2026-10-12 |
| Schedule form | `schedule name { every: 15m  run: action() }`; runs never overlap, a busy tick is skipped; the next tick retries (read as the next due time: "Schedule and batch defaults", 2026-10-19); calendar times later | 2026-10-13 |
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
| Schedule ticks on every host | Schedule adapters, many and chosen in `app.jadpo`: an adapter supplies the tick and may supply the lease (default: main database, superseded 2026-10-19 by "Outbox placement, settled": `schedules: { store: }`, or the only database); a schedule or event with no store for its state is a compile error; system tables (`jadpo_` prefix) are in migration snapshots (Richard) | 2026-10-13 |
| Tracing through events | Always on: events carry the emitting action's trace context; subscriber runs continue it; `jadpo trace <request id>` | 2026-10-13 |
| Schedule monitoring | Every run and tick recorded; `jadpo schedules` (list, run, pause, resume, history); `jadpo why <schedule>`; metrics; alerts with no setup (no success in twice the interval, ticks stopped, near the time limit, three failures, stuck lease); console timeline (Richard) | 2026-10-13 |
| Circuit breaker | Per connection: after repeated `unavailable` failures, deliveries calling the provider are held, one probe after a pause, success releases the rest; held deliveries keep their retry budget; shown and alerted | 2026-10-13 |
| Event transport | Internal only: the main store's outbox (each written store's, read as "Outbox placement, settled" says, 2026-10-19) delivered by the runtime (on Workers after commit via `waitUntil`, swept by the minute tick); external queues only receive published copies of chosen public events (Richard) | 2026-10-13 |
| Outbox placement | No default store: the outbox, delivery records and idempotency keys go in each store an action writes; an action writing no store needs `events: { store: <connection> }`, or it is a compile error (Richard, from the architecture plan) | 2026-10-13 |
| Batches | The one way to work over many items: `csv.read`/`write`, `tsv`, `json_lines`, `json.read(file, items_at:, as:)`/`write` and `batches.each(from:, each:, finished:, key:)`; inline `->` for `each:` and `finished:`; started after commit, returning a run with `batches.status` and `batches.cancel`; `key:` for repeated starts (unique per call, tenant and key: "A batch's `key:` with tenants", 2026-10-19); batches committing writes and position together; file order kept; problems without values; only storable captures; replaces chained events for paging (Richard) | 2026-10-16 |
| Rows and formats | Rows read are `Input`s and written `Output`s with `column:` and `wire:`; `CsvFormat.spreadsheet` or `.standard` required on write; UTF-8, UTF-16 with a mark and declared legacy encodings read, UTF-8 written; gzip only with `max_unpacked_size:`; no loop over rows and no whole-file parse (Richard) | 2026-10-16 |
| Several changing calls | Up to 8 keyed changing calls per action, only from subscribers and routes requiring `Idempotency-Key`; no outside call inside `for` in any action, a loop of calls being `batches.each`; each step recorded before sending (operation, request fingerprint, time) and its answer after, encrypted, a fault over 1 MB (`large_output: true` operations refused); `store.put` a keyed step; reruns replay and check the whole path, stopping as `diverged` for an operator; `clock.now` fixed per record; keys hashed from app, environment, connection, caller scope, action key, step and generation; steps in doubt resent only within the provider's window; one runner per record; the "ends for good after an effect" warning; unfinished records become operator items with retry and abandon, kept 30 days; records deleted in the commit, listed in the personal-data report; `jadpo runs unfinished --build <build id>` lists, before a deploy, unfinished runs whose path the new build changes (Richard, from the multi-step calls review) | 2026-10-16 |
| Several changing calls, checked (chosen in chunk 7.5) | Each the conservative reading. A changing call is a provider's operation with `changes: true` or a store's `put`; reads are never counted. Each way through an `if` or a `match` counts on its own, a `return`, `reject` or `propagate` ending its way, an action called counting the changing calls of its longest way, and a loop's body once, since a call in it is refused. A changing call with no key may share its action with no other changing call on any way through it, another way included, since a rerun after a fresh read can take another way (`step.unkeyed`). `step.context` is reported where the action is run: a schedule, a route whose API doesn't require `Idempotency-Key`, and a run's `each:` or `finished:`, whose items stay one changing call each; tests and fixtures may call such an action. `large_output: true` is refused on any operation, one that reads included, in an action whose longest way makes several changing calls. The record's store is checked for an action a subscriber, or a route whose API requires the key, runs: the record belongs to the action that is run, and a called action's calls join it. A recorded answer's cap is 1 MB, as "Settled for the build" says; area 11's 256 KB is corrected to it (decided for Richard) | 2026-10-20 |
| Events from outside | Webhooks, a package's `public` events and polls, and queue messages through `receiving`, each written to the outbox and delivered like the app's own; unlisted queue message types and repeatedly failing messages parked, never dropped (Richard) | 2026-10-16 |
| Settled for the build | An item of `batches.each` whose action makes outside calls is a batch of its own, keyed from the run and its position; `store.put` a keyed step; a recorded answer over 1 MB is a fault naming the step, and `large_output: true` operations are refused in multi-step actions; the batch types' fields are area 2's (decided for Richard) | 2026-10-16 |
| Outbox placement, settled (Phase 5 review) | Each store an action writes keeps its outbox and the delivery records beside it, as the earlier "Outbox placement" row says; "Where events travel" and "Event transport" are read that way. A subscriber's handled record goes in the store it writes, with no reference across stores. Schedule leases go in `schedules: { store: }` (area 5), defaulting to the only store when there is one (Richard, as recommended) | 2026-10-19 |
| Event records (Phase 5 review) | An event's shape is recorded and diffed in the generated record exactly as an API's is (chunk 7.0's format); accepting a breaking change waits for area 18's acceptances together with the API record's (Richard, as recommended) | 2026-10-19 |
| A schedule and tenants (after chunk 6.1a) | A schedule's run, which no tenant's event starts, reaches every tenant's records, as the system caller does with no tenant; a subscriber stays limited to its event's tenant (Richard, as recommended) | 2026-10-19 |
| An event's tenant (after chunk 6.1b) | An event belongs to the tenant its one field holding a tenant's id names (e.g. `household_id: Household.id`), so the tenant is written in the event where a reviewer sees it; an event with no such field, or more than one, reaches no tenant's records (Richard, as recommended) | 2026-10-19 |
| A batch's tenant (after chunk 6.4) | A batch run belongs to the tenant its one saved value holding a tenant's id names, as an event does; none or several reach no tenant's records (Richard, as recommended; several naming the same tenant count as that one: "Schedule and batch defaults") | 2026-10-19 |
| `jadpo schedules run` (after chunk 6.2b) | Against `jadpo dev` it runs the schedule straight away and shows the outcome; against a deployed application it marks the schedule due, so the application's own server runs it on its next tick under the lease, and the command waits for that run's record and shows it, so a deployment's settings and secrets never leave its servers (Richard, as recommended) | 2026-10-19 |
| Work handed on, on Workers: the loopback and the budget (chunk 8.4b) | The fresh invocation is reached through the loopback binding `ctx.exports` gives the Worker's own `JadpoHandOn`, not a service binding naming it, since Wrangler keeps a binding's `service` when `wrangler deploy --name` or `--env` deploys the file under another name, which would hand a staging Worker's work to production's; workerd pinned in `tools/workers/` gives `ctx.exports` at the compatibility date without a flag, and chunk 8.2n checks it on the real edge. The fresh invocation's 30 seconds are counted from its start, so starting the isolate and reading the work back are spent from them (decided by the building session, reversible) | 2026-10-22 |
| Work handed on, on Workers (chosen in chunk 8.2h) | `Host::hand_on` on Workers is a request to the Worker itself, through a service binding (`JADPO_HAND_ON`, which `jadpo build` writes into the Wrangler file of a Worker that runs background work) to a named entrypoint of its own shim (`JadpoHandOn`), never its default export, so nothing from outside reaches it. It is sent JSON naming each piece by its record and store, never an event's value; the fresh invocation reads each delivery back from its record, leaving out one run or given up since, and each batch run from its row, and carries them on in its own `waitUntil` with Workers' 30 seconds, handing on in turn; the invocation that handed it on is kept until that call returns. A hand-on that fails, as past the platform's limit on invocations one request chains, leaves the work in the store for the minute's cron. Rejected: a queue, which needs a queue made in the account and a consumer for every Worker, delays each hand-on by its batching, and would tie background work to chunk 8.2k's consumer; and a request to the Worker's own URL, which would reach its public `fetch` and need a secret to tell itself from a caller. Whether Cloudflare gives each handed-on invocation its own 30 seconds once the one that called it has ended is checked on the real deploy (chunk 8.2n) (decided for Richard, the conservative reading) | 2026-10-21 |
| An `every:` under a minute on Workers (Phase 6 review) | An error naming the schedule when a Workers deployable reaches it, since a minute's cron can't run it and Jadpo has no warnings (Richard, as recommended) | 2026-10-19 |
| The tick on Workers (chunk 8.2f) | Chosen in the chunk, each reversible: a Workers deployable reaches a schedule when its `runs:` lists `Trigger.schedules`, and one more often than once a minute is `deployable.target_unsupported` (area 14), at its `every:`, rather than a rule of its own, since it is one more thing a target can't run; the Wrangler file names the cron `* * * * *` for every Workers deployable whose tick has a store to keep or sweep, as every native deployable ticks; each run of the cron ticks once its stores are current (out of date, it logs why as a request does and ticks nothing), then, while the app ticks more often than once a minute (a received queue, every 10 seconds, as natively), again at each such mark of the cron's minute, one tick after the last ends, never two at once, so a Worker holds no more connections than one tick does, and a mark a long tick passed is not made up; the invocation ends with its minute, and each of its ticks has what is left of the cron's 15 minutes | 2026-10-21 |
| The `trigger:` adapter (chunk 8.2g) | Chosen in the chunk, each reversible, around Richard's decision (question 16): `trigger:` takes `ScheduleTrigger.native`, the default, or `ScheduleTrigger.external`, and changes only a native deployable, a Worker ticking on its cron whichever is written, so an app with a server and a Worker can write either; `<deployable> tick` (and `serve`, the same as no command; anything else ends with code 2, `command_unknown`) doesn't wait for a database it can't reach, ending with code 1 (`store_unreachable`), since the scheduler's next run is the next try and the ticks missed become one run; it logs `runtime.ticked` (each run by its schedule's name, deliveries, what it received and polled, rows deleted, alerts, each store it couldn't work in) and ends with code 1 only when a store couldn't be worked in, a failed run being recorded, alerted and retried at its next due time as a server's is; the first SIGTERM drains it as a server drains its background work; `tick` runs whatever is written, the lease deciding; nothing refuses an `every:` under a minute with `external`, since a scheduler can run `tick` each second: `jadpo build` says how often to run it, and the ticks-stopped alert says when it runs less often | 2026-10-21 |
| Schedule and batch defaults (Phase 6 review) | What the code does is decided: a schedule's first run is due one `every:` after the first tick that sees it; a failed run is retried at its next due time; schedule state defaults to the only database; batch runs are kept in the store the `from:` query reads; several saved values naming the same tenant count as that tenant; a route whose action starts a batch answers `202` (Richard, as recommended) | 2026-10-19 |
| A batch's `key:` with tenants (Phase 6 review) | In an application with a tenant, `key:` is unique per call, run tenant and key, so two tenants' runs with the same key never meet (Richard, as recommended) | 2026-10-19 |
| Queue messages the store refuses (Phase 7 review, chunk 7.9c) | A message whose event the store refuses as a statement, as it would every time it was sent (a value it can't take, such as a text holding `\u0000`, which `jsonb` can't hold), is parked `unreadable` and acknowledged, so the queue doesn't send it again until its retention deletes it. A store that can't be reached, a conflict, a concurrent change or a fault the driver can't classify still leaves it on the queue, to be kept when it is sent again (decided for Richard, the conservative reading) | 2026-10-21 |
| A delivery's answer closes its breaker (Phase 7 review, chunk 7.9c) | A delivery attempt whose call through a connection its provider answered closes the connection's breaker and releases what it holds, whatever the breaker's state when the delivery was claimed, as "Retries and giving up" says of a delivery already running when the breaker opened. It is one conditional statement after each attempt that called the provider, changing nothing while the breaker is closed with no failures in a run (decided for Richard, as the page reads) | 2026-10-21 |
| The system's runs inside a request and tenants (Phase 7 review, chunk 7.9b) | "Background work and tenants" gains two rows: the magic-link `send:` action reaches its delivery's tenant, as a subscriber does, which for `jadpo.MagicLinkEmail` is none; a webhook's reply action and `on_first_sign_in:`'s reach no tenant's records, never every tenant's, since only a schedule's "no tenant" is every tenant (Richard, 2026-10-21) | 2026-10-21 |
