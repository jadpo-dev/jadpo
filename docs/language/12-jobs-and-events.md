---
title: Jobs and events
area: 12
status: decided
checked-with: this checkout's jadpo on every gate run
---

# Jobs and events
<!-- topic: jobs-and-events -->

This page covers the work your app does outside a request: schedules that
run an action on a timer, events an action announces, subscribers that
react to them, and what happens when that work fails.

Three things start work, and each one calls an ordinary
[action](03-logic.md#functions-and-actions), the kind of function that
reads and writes stored records:

| Trigger | Starts when | Lives in |
|---|---|---|
| Route | A request arrives | `routes/` ([HTTP](07-http.md)) |
| Schedule | A timer fires | `schedules/` |
| Subscriber | An event it listens for is committed | `subscribers/` |

All your logic lives in actions, so a reviewer reads one kind of code
whatever started it. A schedule or a subscriber only adds *when* to run
and *what happens if it fails*.

Two more things feed this work without being triggers of their own:

- Work over more items than one action can hold runs as a **batch**. An
  action starts it, and it calls an action for each item
  ([Batches](#batches-work-over-many-items)).
- The outside world produces events for your subscribers: webhooks, a
  package's own events and polls, and messages read from a queue
  ([Events from outside](#events-from-outside)).

Much of this page talks about the **tick**. The tick is the runtime's
regular wake-up, once a minute in most apps: it runs the schedules that
are due and picks up background work left over
([The tick](#the-tick)).

## Schedules
<!-- topic: schedules -->

Use a schedule for work that should happen on a timer rather than when
someone asks, such as sending reminders every 15 minutes or tidying up
once a day. A schedule runs one action at a fixed interval. It has a
name, an `every:` saying how often, and a `run:` that calls an action in
the same domain with no arguments.

`domains/planning/schedules/chore_census.jadpo`:

```jadpo fragment
// domains/planning/schedules/chore_census.jadpo
/// Queues a reminder for each overdue todo.
schedule chore_census {
    every: 15m
    run: queue_chore_census()
}
```

Here `queue_chore_census` runs every 15 minutes. You don't start it,
and a restart or a deploy doesn't upset its timing.

### `every:` and `run:`

`every:` takes a whole number of seconds, minutes, hours or days above
zero: `30s`, `15m`, `1h` or `1d`. Anything else is
`schedule.every_invalid`.

`run:` calls an action of the schedule's own domain. The call passes no
arguments, and the action takes no parameters. Anything else is
`schedule.run_invalid`. A schedule holds nothing but `every:` and `run:`;
anything more is `schedule.item_invalid`.

### When runs happen

A schedule's runs fall on a grid of times, counted in UTC from the first
tick that sees the schedule. Its first run is due one `every:` after that
tick, and each run after it one `every:` later. The grid is kept in the
schedule's state row, a row the runtime keeps for each schedule, so a
restart or a deploy neither moves it nor adds a run. When you change
`every:`, the new interval takes effect from whichever comes first: the
due time already kept, or one new interval after the tick.

Only one run of a schedule happens at a time. Ticks missed while the app
was down, or while a run was still going, become one run, not a backlog.
A tick at or past the due time runs the schedule once, and moves its next
due time to the first grid time after the tick. A due tick that finds the
schedule paused, or still held by a run that hasn't finished, skips it
rather than queueing it. The skip is recorded with its reason, and the
due time moves on in the same way.

### The time inside a run

Inside a run, `clock.now` is fixed: every read gives the time the run
started, which is the tick's time. The action doesn't need the time
passed in.

### When a run fails

A failed run is logged and shown to the operator, and tried again at its
next due time, not at every tick. A schedule has no retry settings,
because its next due time is its retry.

### Where a schedule's state is kept

Each schedule's state and its lease (the claim that stops two copies of
your app running it at once) live in a database. By default that is the
app's only database. With several databases, name one in
`application { schedules: { store: main } }`
([Stores](05-data.md#stores)); with several and none named, or none at
all, each schedule is `schedule.store_required`. `schedules:` also holds
`trigger:`, which says what ticks a native deployable
([The tick](#the-tick)). The tables for schedule state and runs are in
the migration snapshot only when a schedule is declared or an entity
purges old records.

### Schedules Jadpo writes for you

An entity whose [lifecycle](05-data.md#lifecycle) has `purge:` gets an
hourly purge schedule from the compiler, such as `Member.purge` for
`Member`. You write nothing for it
([The purge schedule](#the-purge-schedule)).

### How a schedule runs

- **The lease.** A run first takes the schedule's lease, in one
  conditional statement on the state row: it succeeds only while the due
  time is unchanged, the schedule isn't paused and no live lease holds
  it. So of two instances ticking at once, one runs the schedule and the
  other does nothing. The lease lasts what is left of the tick's time
  budget plus a minute
  ([Running background work](#running-background-work)), at most 16
  minutes on a native server. A run that held the lease past then was
  lost with its worker, and its record ends `failed` with
  `lease_expired`.
- **A run.** The action is called with nothing, as the system, reaching
  every tenant's records
  ([Background work and tenants](#background-work-and-tenants)). It runs
  in a context of its own whose `clock.now` is the tick's time
  throughout, and in a new trace. It must finish within the 15-minute
  limit of a run and within its tick's budget. A run stopped by either is
  recorded failed with `deadline_exceeded`, and its lease is given back.
  What the run emits is delivered after it. A failure is recorded with
  its code and kind. Due schedules run side by side, so one long run
  holds no other up.
- **Grants.** A schedule runs as the system, so the records it reaches
  must name `Access.system` in their policy. A schedule reaching what
  only `Access.public` grants is `policy.system_grant_missing`, as a
  subscriber or a batch is ([Background work](09-policy.md#background-work)).

### The tick

The tick is how background work gets started when no request is
running. It happens once a minute, or as often as the shortest `every:`
when that is shorter, but at most once a second. Every host calls the
same entry point for it, `jadpo_runtime::schedules::tick`.

What does the ticking depends on where the app runs. A **deployable** is
one program you build and run: a native server, or a Cloudflare Worker
([A native deployable](14-tooling.md#a-native-deployable),
[A Workers deployable](14-tooling.md#a-workers-deployable)).

- **A native server** ticks on its own timer, on its first worker's
  thread, at each whole period of UTC. You don't write anything for that.
- **A Worker** ticks on a cron trigger that runs every minute, which
  `jadpo build` writes into the Wrangler file. While the app receives a
  queue, each cron run ticks again at each 10-second mark of its minute,
  so a queue is read as often as on a native server. A minute's cron
  can't run anything more often than once a minute, so on a Worker an
  `every:` under a minute is `deployable.target_unsupported`, naming the
  schedule.
- **`jadpo dev`** ticks on its own timer, whatever `trigger:` says.
- **A test's or a tool's host** doesn't tick.

Each state row keeps the last tick that reached it.

#### Ticking from another scheduler: `ScheduleTrigger.external`

Use it when your platform already has a scheduler you want to run the
work, such as a Kubernetes CronJob, an ECS scheduled task or a systemd
timer.

```jadpo check project=external-tick file=app.jadpo
application {
    schedules: {
        trigger: ScheduleTrigger.external
    }
}
```

The server then never ticks. Instead, the scheduler runs the
deployable's `tick` command, `<deployable> tick`. The command reads the
app's settings and checks its stores' migrations as serving does, runs
one tick, and ends ([A native deployable](14-tooling.md#a-native-deployable)).

There is no HTTP endpoint for the tick and no tick secret. A scheduler
that can only call a URL needs a wrapper of its own.

Run `tick` as often as the server's own timer would tick, which
`jadpo build` prints beside the binary. A scheduler that runs it less
often runs schedules and reads queues late, and the ticks-stopped alert
says so ([Watching schedules](#watching-schedules)). Two ticks at once,
or a `tick` beside a server's own timer, do no harm, because the lease
decides which one runs each schedule.

`trigger:` changes only a native deployable. Without it, the value is
`ScheduleTrigger.native`, the server's own timer. A Worker always ticks
on its cron, whichever is written.

#### What each tick does

Each tick does the following, in order. Each step starts only while the
tick's time budget lasts.

1. It runs the schedules that are due, and any run asked for by hand
   ([Running a schedule by hand](#running-a-schedule-by-hand)), and
   delivers what they emit.
2. In every store holding the runtime's tables, it delivers what is left
   to deliver. This is the outbox's sweep: up to 100 deliveries, oldest
   first, that are `pending` past their next attempt or `running` past
   their lease. A crash between a commit and its delivery, a host that
   dropped work, or a worker that died holding a lease leaves these
   behind.
3. It picks up batch runs whose lease lapsed.
4. It deletes what is no longer needed (below).

#### What the tick deletes

The tick deletes:

- expired idempotency keys ([Idempotent writes](07-http.md#idempotent-writes));
- an event with its delivery records once every delivery succeeded, 24
  hours after it was emitted, and an event nothing subscribes to after
  the same time. An event with a failed delivery stays for an operator;
- a delivery an operator dismissed or cancelled, 30 days after the
  operator's last action on it
  ([What an operator sees](#what-an-operator-sees));
- the handled records of those deliveries (below);
- browser sessions and API tokens that have ended, and service keys that
  have expired ([Identity](08-identity.md)), which a lookup already
  ignores. Each is deleted in the store it is kept in, beside its
  principal's users or services;
- schedule run records and ended batch runs 30 days old
  ([How a run is kept](#how-a-run-is-kept));
- an operator's log rows 30 days old whose delivery or run is gone.

This cleanup runs after the work that can't wait (due schedules,
delivery sweeps, queues, polls, alerts and expired batch leases), at most
once an hour for each store once a pass completes. Each store's
`jadpo_housekeeping` lease lets one instance in at a time. Stores holding
only schedule state or AI usage are included. What to clean is worked out
from the whole application's schema, so a deployable that runs no
schedules or batches still cleans their history. A pass that failed, hit
its cap or was interrupted runs again on the next tick. Finishing and
giving back the lease are checked against the owner's identity, so only
the owner can do either. A crashed owner's lease ends after the rest of
its tick's budget and the usual minute. Before each batch of deletes, the
tick checks its budget and whether the app is stopping.

A **handled record** is the note a subscriber leaves to say it has dealt
with a delivery
([Delivery: at least once, safe to repeat](#delivery-at-least-once-safe-to-repeat)).
It goes only once its delivery can no longer repeat, which is once the
delivery's own record is gone: with its event deleted, nothing delivers
it again, whatever an operator does later. In the event's store, the
statement that deletes the event also deletes the handled records kept
there for its deliveries, so an app with a single store never keeps one
after its event. A subscriber that writes another store keeps its record
there, and the tick deletes it by the delivery's `id` once the event's
statement has committed. If the tick can't reach that store then, the
records stay. They only cost space, and none is ever lost early.

What an operator dealt with is kept as long as a run record, then goes.
A delivery `dismissed` or `cancelled` never runs again, so the tick
deletes it 30 days after the last action logged on it, with its handled
records. Nothing else is deleted while its event stays. A delivery that
could still run (`pending`, `running`, `failed` or `outcome_unknown`) is
never deleted, however old.

An event goes once every delivery of it that is left has succeeded. An
event with a delivery that could still run, or one set aside within the
last 30 days, stays, so `jadpo deliveries` shows that delivery with its
event. The tick deletes those deliveries before it deletes events, so an
event whose last such delivery goes is deleted on the same tick. The 30
days count from the operator's log, not from a column of the delivery:
each operator action writes its log row together with its change, and a
log row stays while its delivery does. So the window counts from the
operator's last action without changing the delivery's table.

Each delete takes 1,000 rows at a time, at most ten times a tick: 1,000
events with their delivery and handled records, or 1,000 deliveries an
operator dealt with, with their handled records. The tick hands nothing
on. What it doesn't reach, the next tick carries on with.

### The purge schedule

An entity whose [lifecycle](05-data.md#lifecycle) has `purge:`, and
which is kept in a store, gets a schedule of its own that deletes its
old records every hour. You don't write it. It is named after the entity
and its operation, `Member.purge`, as `Member.delete` is. It is keyed by
the entity's born name, the name it was first declared with, so renaming
the entity keeps its state and run records. Its `.jadpo` place, which
`jadpo why` gives, is the lifecycle's `purge:`.

It runs every hour on the same kind of grid as any schedule, and is
recorded and watched like any schedule. Its state row, lease and run
records are a schedule's, kept in the store `schedules: { store: }`
names, or in the only database. With several databases and none named,
the lifecycle's `purge:` is `schedule.store_required`, which suggests the
entity's own store, and `migration.out_of_date` waits until one is
named. `jadpo schedules` lists it; `history`, `pause`, `resume`, `run`
and `jadpo why` take its name; and `jadpo dev` prints each run.

Each run calls the runtime's purge at the tick's time, outside every
action and every policy. It deletes at most 20 batches of 500 records
within the run's time limit, and the next hour's run carries on with
what it leaves. How old a record must be is the lifecycle's `after:`, or
the value the app started with for the setting `after:` reads. The run
emits nothing.

A store that can't be reached makes a failed run with `unavailable`, as
for an action. Anything else the store refuses is `internal_fault`, and
the batches deleted before it stay deleted. The run records count the
records deleted (`items`). The screens show `2 records purged` where an
action's run shows its events, and `jadpo dev`'s `--json` line carries
`items` and `batches`, how many each batch deleted. No test runs the
purge schedule ([Testing](13-testing.md)).

## Batch work keeps its progress
<!-- topic: batch-work-keeps-its-progress -->

An action is all-or-nothing: its writes all commit, or none do
([All-or-nothing actions](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)).
If one action emailed 500 owners and failed on the 300th, its database
writes would roll back, but 299 emails would already be sent. So a
schedule that covers many items doesn't do the work itself. It finds the
items and emits one event per item. Each item is then handled by its own
action and retried on its own, and one bad item never blocks or undoes
the rest.

In this example, `overdue_todos` is a [named query](06-reads-and-writes.md#named-queries)
returning at most 500 todos not yet queued, and `send_reminder` is the
subscriber that handles each event:

```jadpo fragment
// domains/planning/actions/queue_chore_census.jadpo
action queue_chore_census() -> Unit {
    var cutoff: ReminderCutoff = ReminderCutoff(clock.now)
    var todos: List<Todo> = overdue_todos(cutoff)
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

A query returning a list declares no failures, so its call takes no
`attempt`. A database outage is a [fault](04-failures.md#faults), and
the next run tries again. The update can fail, because the todo may have
changed since it was read, so it takes `attempt`: the failure ends the
run, nothing commits, and the next run tries again. The query's limit
bounds each run, and the next run picks up anything left over. Marking
each todo in the same transaction stops the next run from queueing it
twice.

This shape suits a bounded set found by a query. Work over a set with no
bound, such as every row of an uploaded file, a large export or every
page of a provider's list, is a [batch](#batches-work-over-many-items).

## Batches: work over many items
<!-- topic: batches-work-over-many-items -->

Use a batch when one action can't hold the work: importing every row of
an uploaded spreadsheet, writing a large export, or walking every record
a query finds or every page of a provider's list. A batch runs in the
background, a slice at a time. It keeps its place as it goes, so a crash
or a deploy carries on where it stopped and nothing is done twice.

You start a batch with a helper: one for each file format and direction,
and `batches.each` for work with no file. Every helper handles small and
large inputs alike, so there is one way to do this whatever the size.

`domains/catalogue/actions/import_products.jadpo` and
`domains/catalogue/actions/export_products.jadpo`:

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
| `csv`, `tsv`, `json_lines` and `json` read and write helpers | Files. Each operation's signature, result and failures are in [Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines) |
| `batches.each(from:, each:, finished:, key:)` | No file: a large query, or every page of a provider's list |

### Reading a file

A read helper takes the stored `File`, the row type (`as:`), an `each:`
that calls one action per row, and a `finished:` that runs once at the
end.

### Writing a file

A write helper takes the store to write to, the file's `name:`, a
`from:` giving the items, an `each:` that returns the row to write for
each item, and a `finished:`, which receives the stored file.

### Walking records or a provider's list: `batches.each`

`batches.each` takes a `from:` and calls an action for each item.
`from:` is a call to a [named query](06-reads-and-writes.md#named-queries)
returning a `Page`, which the run walks a page at a time
([Order and pages](06-reads-and-writes.md#order-and-pages)), or a call to
a provider's list operation, which it walks by `page.next`
([Integrations](11-integrations.md)).

### `each:` and `finished:`

`each:` and `finished:` are written in the inline `->` form that
standard operations may take ([Passing functions](03-logic.md#passing-functions)).
`item -> save_product(item)` calls an action in the same domain for each
item, as a subscriber's `run:` does. When writing a file, `each:` is
instead a pure expression giving the row.

Values they capture from the action around them, such as `import.id`,
are saved with the run so it can carry on later. So only values that can
be stored may be captured.

### Starting a run, and `key:`

A helper may be called only in an action. Like `emit`, it starts only
after the calling action commits. It returns a `BatchRun` at once, and
the run goes on in the background. If the action rolls back, the run
never starts.

`key:` makes a repeated start return the same run, so a client that
retries doesn't import a file twice. In an app with a tenant, the same
key in two tenants starts two runs.

### Progress, cancelling and the summary

`batches.status(run)` gives a `BatchProgress`: the items done, the
estimated total (from the file's size, or the query's count), the
problems so far, and the state, `running`, `finished`, `failed` or
`cancelled`.

`batches.cancel(run)` stops the run after the batch in hand.

`finished:` receives an `ImportSummary` for a read or a `batches.each`,
or an `ExportSummary` for a write. Each holds the count of items done,
the count of items with problems and the problems themselves, and for an
export, the written `File`. Each `RowProblem` holds the row, the column
and the rule. These types are described in
[Values and types](02-values-and-types.md#other-built-in-types).

### How a run goes

The run groups its items into batches. Each batch is one transaction
that commits its writes and the run's position together, so a crash or
a deploy resumes at the next item and nothing is written twice. A file's
order is kept, and one run's batches go one after another. Separate runs
go side by side. The run's state lives in the store `from:`'s query
reads ([How a run is kept](#how-a-run-is-kept)).

Each item's action runs inside its batch's transaction, under the rule
for a handled failure in
[All-or-nothing actions](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes).
So an item whose action fails is undone on its own and becomes a
problem, while the rest carry on.

`batches.each` is also the one way to make an outside call for each
item, since an action can't make outside calls inside `for`
([Loops](03-logic.md#loops)). When `each:`'s action makes outside calls,
each item is a batch of its own, run as a subscriber's delivery is. Its
calls go first, under keys made from the run and the item's position.
Then its writes and the run's position commit together. So an item run
again replays its calls rather than sending them again.

`finished:` runs once after the last batch, in a transaction of its own.

### Problems never stop a run

A row that can't be decoded, or an item whose action fails, becomes a
`RowProblem`. It holds the row as a spreadsheet numbers it (the header
is row 1, the first data row is row 2), the column, the field and the
rule, but never the value, so personal data stays out of problem lists
and logs.

Only a problem with the whole file stops a run, at the place it occurs
or before: a missing required column, a quote that never closes (named
by the line where it opened), an encoding the format doesn't declare, or
bytes that aren't text.

"All rows or none" isn't a setting. To get it, run the batch twice: once
to check the rows without writing, then again to write.

### Rows are typed both ways

A row read is an `Input`, and a row written is an `Output`. They follow
the same rules, problems and JSON encoding as a route's input and output
([One input](07-http.md#one-input)). A column maps to a field by the
field's name, or by `column: "Email address"`. Cells written another
way, such as dates with the day first, use the `wire:` encodings
(`Wire.day_month_year`) or a pure `decode:` function.

A CSV or TSV row holds single values only. A list or an object in one is
a compile error that suggests JSON Lines.

Writing CSV names the program that will read it. `CsvFormat.spreadsheet`
writes a byte-order mark and CRLF line endings, and escapes text that
looks like a formula. `CsvFormat.standard` writes CRLF line endings only.
A write without `format:` is a compile error.

Reading takes UTF-8 (a byte-order mark is removed), UTF-16 with a mark,
and older encodings the format declares. It never guesses. Writing is
UTF-8. A gzip file is read only when you give `max_unpacked_size:`, and
no archives are opened. The format helpers themselves are described in
[Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines).

### There is no other way

There is no loop over a file's rows, and no way to parse a whole file
into a list. `store.read(file)` returns `FileContents`, which is for
passing bytes on ([Files](16-built-in-services.md#files)), and the
helpers don't accept it. A provider's CSV report is put in a store and
read like an upload. `json.read` given text is only for single JSON
documents in memory.

### Choose an order no update changes

An export walks live data, so a record changed while it runs appears as
its page saw it. To keep every record in exactly once, order `from:`'s
query by a field no update changes, such as `id`, or a creation time
followed by `id`. A `from:` ordered by a field that can be updated is
[`type.batch_order_updatable`](../errors/type.batch_order_updatable.md).
It is an error you may decline when you know the order is safe
([Declining a judgement call](01-project-layout-and-modules.md#declining-a-judgement-call)).

### Batches instead of chained events

Don't page through a large set with an event per page: a chain of events
stops at 16 deep ([Limits and runtime faults](#limits-and-runtime-faults)),
so it would stop after 16 pages. Use a batch. Work over many pages of a
provider's list is a batch too.

### A sweep, in full

This sweep marks every item seen, a page at a time, once per sweep, and
keeps how many it went through:

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

## How a run is kept
<!-- topic: how-a-run-is-kept -->

This topic is the detail of how a `batches.each` run is stored, resumed
and checked. Most apps never need it: [Batches](#batches-work-over-many-items)
says what a batch does. The rules for a file's run and an export's run
build on this one ([How a file's run is kept](#how-a-files-run-is-kept),
[How an export's run is kept](#how-an-exports-run-is-kept)).

### A batch is a page

Each transaction is one page of `from:`, at the size its query's `page:`
reads. So 60 items in pages of 3 commit 20 times.

### Where a run is kept

A run is kept in the store `from:`'s query reads, in three
[system tables](05-data.md#system-tables):

- `jadpo_batch_runs` has one row per run: which call started it, the
  shape it was started under, its `key:`, what it saved, the tenant it is
  limited to, how deep in a chain of events it started, the cursor of the
  next page, its counts, its state and its runner's lease;
- `jadpo_batch_problems` holds each problem;
- `jadpo_batch_cancels` holds each cancel asked for.

Only a store some batch walks has these tables. Starting a run writes
that store, and `each:`'s and `finished:`'s actions may write no other
(`store.action_spans_stores`, [One store per action](06-reads-and-writes.md#one-store-per-action)),
so an item's writes and the run's position always commit together. A run
always has its query's store, so it never needs `events: { store: }`.

### Starting

The run's row is written in the starting action's transaction, and the
run is left to start after the response. An action that rolls back
leaves no row, so nothing runs. A start needs no outbox row: the run's
row is as durable, and the tick picks up a start a host dropped, as the
outbox's sweep picks up a delivery.

A start with a `key:` that finds a run of the same call with the same
key, compared by its contract JSON, in the same tenant, gives back that
run, whatever its state and whatever else the call was given. A start
without a `key:` is always a new run.

In an app with a tenant, `key:` is unique for each call, run's tenant and
key, so two tenants' runs with the same key never meet. The row keeps the
tenant: all zeros when the run reaches no tenant's records, or the app
has no tenant. A later start never finds a row with no tenant kept,
since nothing says which tenant its key was given in. A retry then
starts a new run, rather than one tenant being handed another's.

### Saved values

`each:`'s and `finished:`'s arguments may be the item (or the summary), a
field inside it, or a value that reads nothing of it. That last kind is
worked out as the run starts and saved with it. An argument that works
something out of the item is `type.batch_each_invalid`: that work belongs
in the action.

`from:`'s query arguments are saved as a request carries them, so an
`Input` may be among them. What `each:` and `finished:` capture, and
`key:`, are saved as an entity field holds a value, so an `Input` there
is `type.batch_capture_not_storable`. A secret is never saved.

The saved values are a JSON array of each value's own JSON (its type's
contract JSON), with `null` for none. They are kept with the encoding
they were written in (`jadpo_batch_runs.encoding`) and read with the
standard JSON reader, a number keeping the digits it was written with.

### Which call started a run

A run names the `batches.each` call that started it by what the call is,
in identity keys that survive a rename
([Stable identity](18-records-and-manifest.md#stable-identity)): the
action starting it, the query `from:` walks, and the action `each:` runs,
as in `action check_stock from query products_to_check each action
check_product`. Adding or moving another call leaves the name as it was.
A second call of the same three in one action adds its place among them
(` #2`).

The run's shape, what it saves and calls, is kept as canonical type
text, naming declared types by their identity keys, with a digest. So a
release that changes the compiler's own internals leaves every running
batch's shape as it was.

### Problems

An item whose action fails is undone on its own and becomes a problem.
Its row is its place in the walk, counting from 1. Its column is empty,
since a query's item has no columns (the file helpers fill it). Its rule
is the failure's code.

An item whose action faults with a defect or an overflow is a problem
too, undone on its own as a failure is. Its rule is the fault's
classification, `internal_fault` or `int_overflow`.

Some faults belong to no single item: a store that can't be reached, a
deadline, or a change made by another transaction. These undo the whole
page. A page undone by another transaction's change runs again, up to
three times; otherwise the run waits for the tick.

### Items that make outside calls

When `each:`'s action makes outside calls, itself or through an action it
calls, each item is a batch of its own, so no call goes out while a
transaction is open ([All-or-nothing actions](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)).
Each item goes like this:

1. The run's place is read in a statement of its own, locking nothing.
2. The item is read in a snapshot: a query's page of one item, or a
   file's next row. Its first call closes the snapshot.
3. Its action runs as the system, as a subscriber's delivery does.
4. In the transaction its writes are in, the run's row is locked and
   held to the position the item was read at. The item's problem, if it
   failed, and the run's next position commit with its writes.

A query's run is then at the item's own cursor, and a file's run just
after its row, so a run resumed after a crash meets the same item at the
same place. Each call's key is made from the run and that place (the
item's cursor, or where its row begins in the file), so the item's calls
go out again under the keys they were first sent with. A change by
another transaction runs the item again, up to three times, as it would
a page.

A `finished:` whose action makes outside calls runs the same way: its
calls are keyed from the run, and the run's end commits with its writes.

An unkeyed call that may have acted (`outcome_unknown`) is the item's
problem, and is never sent again. One lost with its runner waits for the
step record that several changing calls keep, as a subscriber's does
([Several changing calls in one action](#several-changing-calls-in-one-action)).

### Who runs it

Each page's read, its items and `finished:` act as the system
([Background work and tenants](#background-work-and-tenants)), and read
the clock as their page began. Each page begins at repeatable read when
`each:`'s or `finished:`'s action loads a record, and at read committed
otherwise.

A run keeps how deep in a chain of events the action that started it
was, and its items run at that depth. So what they emit is one deeper
than the event that started the run, and a loop through a batch stops at
16 deep, as any loop of events does. Each item's action, and
`finished:`'s, may emit as many events as one action may, 1,000, however
many the items before it in the page emitted.

### Finishing

`finished:` runs once after the last page, in a transaction of its own.
It is given the `ImportSummary` with every problem in row order, and the
run becomes `finished` in the same transaction. If its action fails,
what it wrote is undone and the run is `failed` with the failure's code.
A cancelled run doesn't run `finished:`.

### Cancelling

`batches.cancel` writes a row of its own in the calling action's
transaction and never touches the run's row, so it never waits for the
page in hand. The next page finds it and ends the run `cancelled`. Until
then `batches.status` says `running`.

### Status

`batches.status` reads the counts and the state in the calling action's
transaction. A `BatchRun` that no store keeps is a fault. Both
`batches.status` and `batches.cancel` find a run by its `id` alone, so
they need the app's runs in one store.

### The estimated total

A query's run counts its query as its first page is read, in that page's
transaction, as the system limited to the run's tenant. It runs the
query's statement with no page limit or cursor, so the count is what the
walk would meet if nothing changed. Later pages keep that count.

A file's run works its estimate out with each page: the items done so
far, scaled by the file's size over how far into the file they end, and
never fewer than are done. For a gzip file, the size is what it is
unpacking to, at the rate so far.

`estimated_total` is `none` until then. Once the run has finished it is
the items done, and once it failed or was cancelled, the last estimate.
It is kept in `jadpo_batch_runs.estimated_total`. A count the store
refuses leaves the run without one rather than stopping it. A query whose
records Jadpo checks again after its statement, such as one with `geo.`
conditions ([Geospatial queries](20-geospatial.md#queries)), has none.

### An order an update can change

A `from:` whose query orders by a field that can be updated, before any
`id`, is `type.batch_order_updatable`. It is reported at the `from:` and
declined on the action that starts the run. A field can't be updated
when it is `id`, is marked `immutable: true`, or is generated with
`Generated.on_create`. What comes after `id`, or after a unique field
that can't be updated, is passed over, since it never moves a record.

The check doesn't look at which updates the program makes, so adding one
elsewhere never makes a run's order wrong after the fact. A policy with
no `update:` doesn't count either, since adding one is a one-line change.

### Carried on over time

A runner holds its run by a lease lasting what is left of its budget
plus a minute ([Running background work](#running-background-work)). The
lease is renewed with each page it commits. The runner goes page after
page while its budget lasts, then gives its lease back, and a fresh
invocation carries on from the position that committed. If the budget
ends in the middle of a page, that page is dropped and nothing it did is
kept. A run that couldn't commit a page in a whole budget gives its lease
back and waits for the tick, which picks up every running run whose lease
has lapsed.

### What a deploy changes

A deploy may change a run's call: its query, its actions or what it
saves. Or the run's saved values may be in an encoding this release
doesn't read, such as a later release's after a rollback. Either way,
the run ends `failed` and says why ("its saved values were written in
encoding 2, which this release doesn't read"), rather than reading what
it saved as something else.

A run with no encoding recorded, whose call and shape match, is read
with its values as encoding 1's. A run kept before calls were named by
identity has a name no call now gives, and ends `failed` because "it was
started by a release that named its `batches.each` call another way".
To list such runs before a deploy, run `jadpo runs unfinished`
([The `jadpo` command](14-tooling.md#the-jadpo-command)).

### Kept 30 days

In each hourly cleanup pass, in every store that keeps batch runs, a run
that ended (finished, failed or cancelled) 30 days ago or more is
deleted, with its problems and its cancel. So are cancels as old for
runs no store keeps, which happens when a cancel is asked of a
`BatchRun` a field held after its run was deleted. A run still going is
never deleted, however old.

### Names

`batches` is a standard namespace, so, as with every standard namespace,
no variable may be named `batches`.

### Not built yet

A page type allowing more than the 1,000 items of one batch
([Limits and runtime faults](#limits-and-runtime-faults)) isn't split
into smaller batches yet; `PageSize` stops at 100. `batches.status` and
`batches.cancel` in an app whose runs are kept in no store, or in more
than one, are `toolchain.not_built_yet`.

## How a file's run is kept
<!-- topic: file-batch-runs (was how-a-file-s-run-is-kept) -->

This topic is what a file helper's read run does beyond a query's run
([How a run is kept](#how-a-run-is-kept)). It covers `csv.read`,
`tsv.read`, `json_lines.read` and `json.read`'s file form. Where
[Batches](#batches-work-over-many-items) and
[Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)
leave a choice open, the run takes the most cautious one.

### Where it is kept

A file has no query whose store could keep its run, so the run is kept
in the store the starting action writes, counting what `each:`'s and
`finished:`'s actions write. The rule of one store per action makes that
a single store. When none of them writes, the run is kept in the app's
only database.

### A batch is rows

Each transaction holds a batch of a hundred of the file's rows. Under
`jadpo test` it holds two, so every import in a test resumes between
batches. The run's position is where the next row begins: its byte, its
row and its line, kept with the run. The file is saved with the run as
its `File`, so a resumed run reads the same one.

### Read before the transaction

No outside call goes out while a transaction is open
([The seam every outside call goes through](16-built-in-services.md#the-seam-every-outside-call-goes-through)),
and an item's action runs
inside its batch's transaction. So each batch's rows are read from the
store first, through the `files` capability, in windows of at most a
megabyte until the rows are whole, starting from the position the last
batch committed. Then the batch's transaction opens. It holds the run's
row to that same position, runs the rows' items, and commits their
writes with the problems and the position after the last row read.

Each time a runner picks the run up, it reads the start of the file
once, for its encoding and, for CSV and TSV, its header.

### Columns

The header names the columns. Each is a field of `as:`'s `Input`, by the
field's name or its `column:` for a CSV or TSV column; for JSON Lines,
the fields are the `Input`'s JSON.

These stop the run before any row is read, because an `Input` refuses an
unknown field: a column no field reads, a column named twice, or a field
that may not be left out with no column for it.

A row with more cells than the header is a problem (`unknown_field`). An
empty cell is left out, so `optional:` and `default:` apply, and an empty
required cell is a `required` problem.

### Rows and problems

A row is read as a value in a query string is: each cell is its field's
text, and a standard type's cell is its canonical text. A row that can't
be read is a problem, with its row as a spreadsheet numbers it (the
header is row 1, and a blank line is counted but makes no item), its
column by the header's name, and the rule. In JSON Lines, a problem's
row is its line and its column the field's path. A row whose action
fails is a problem with no column and the failure's code. A problem
never holds the value.

### Quotes and line endings

Quoting follows RFC 4180: a quoted cell may hold the delimiter, line
breaks and `""`. LF, CRLF and a lone CR each end a row. A quote inside
an unquoted cell, or text after a cell's closing quote, is kept as
written, as spreadsheets read it.

### What stops the run

These stop the run at the place they are, with the rows before them
done:

- a quote that never closes, named by the line it opened on;
- bytes that aren't text in the file's encoding, a NUL among them, named
  by their line;
- a gzip file read without `max_unpacked_size:`, or one that unpacks past
  it, is damaged or is cut short ([gzip](#gzip));
- a row longer than 16 MB.

The run ends `failed` and says why. A store that can't be reached pauses
the run, as it does a query's.

### A JSON file's list

`json.read`'s file form reads the list `items_at:` names, or the whole
document when it names none. Each item is a row. Its row number is its
place in the list, counting from 1, and a problem's column is the
field's path, as in JSON Lines.

The document is walked a window at a time, holding only where the walk
is and the item in hand, so a document of any size is read in the memory
of its largest item. Objects on the way to the list are read only for
their members' names. Every other value is passed over, its brackets
matched and its strings followed.

An item is the text of one value in the list, which the row type reads
as a request's body is read. So an item that isn't JSON (`tru`), or isn't
of the row type, is that item's problem, and the rest carry on.

These stop the run at the place they are, with the items before them
done:

- no list where `items_at:` says, or, with no `items_at:`, a document
  that isn't a list;
- structure that isn't JSON, such as a missing comma or a bracket that
  doesn't match, named by its line;
- a document that ends before its JSON does;
- an object on the way to the list naming a member twice;
- an item longer than 16 MB.

Once the list closes, the rest of the document is walked to its end, so
a document cut short, or followed by more text, stops the run after its
last item.

Each runner walks the document from its start to the list once, as it
reads a CSV's header. Each page starts from that walk, set to the
position its run committed: the byte after the last item done, with its
line and the next item's place. `items_at:` is part of the call's shape,
so a deploy that changes it ends a running run `failed`, as one that
changes what the run saves does.

### gzip

A file whose first bytes are gzip's is read only when the helper is
given `max_unpacked_size:`, and is unpacked as it is read
([Files](16-built-in-services.md#files)). The run's
position, its rows' lines and its encoding are those of the unpacked
text.

Its text is never read past `max_unpacked_size:`. The run stops at the
row that would reach past it ("its file unpacks to more than 200 MB, its
`max_unpacked_size:`"), with the rows wholly inside it done, so a small
upload can't unpack to gigabytes. gzip can't be entered part way, so a
runner that picks the run up unpacks the file again from its start to
its position.

A file that isn't gzip is read as it is, whatever `max_unpacked_size:`
says. `max_unpacked_size:` belongs to the build, not to what the run
saves, so a deploy may change it under a running run.

### Encodings

A file is read as UTF-8, with any byte-order mark removed, or as UTF-16
with its mark, little- or big-endian. The encoding is never guessed:
anything else isn't text.

### Not built yet

A file's run in an app with several databases, when neither the
starting action nor `each:`'s or `finished:`'s actions write any of
them, is `toolchain.not_built_yet`.

## How an export's run is kept
<!-- topic: export-batch-runs (was how-an-export-s-run-is-kept) -->

This topic is what a file helper's write run does beyond a query's run
([How a run is kept](#how-a-run-is-kept)). It covers `csv.write`,
`tsv.write`, `json_lines.write` and `json.write`'s file form. Where
[Batches](#batches-work-over-many-items) and
[Data formats](19-standard-library.md#data-formats-json-csv-tsv-and-json-lines)
leave a choice open, the run takes the most cautious one. Data formats
also says how each row is written.

### Where it is kept

As for `batches.each`: in the store `from:`'s query reads, which
`finished:`'s action writes too.

### Each page is stored after its page commits

No outside call goes out while a transaction is open
([The seam every outside call goes through](16-built-in-services.md#the-seam-every-outside-call-goes-through)),
so each page goes in three steps:

1. A transaction locks the run's row and reads, as the system, the page
   after the position that committed. It commits, having written
   nothing.
2. With no transaction open, `each:` works out each item's row, and the
   page's rows are put in the store as a piece of the file of their own,
   under a key made from the run's id and the piece's number.
3. A second transaction locks the run's row again, holds it to the
   position the page was read from, and commits the problems and the
   position after the page: the next page's cursor, and the pieces kept
   so far with their bytes and rows.

A run stopped between the put and that commit puts the piece again when
it resumes, under the same key, so the file never holds a row twice.

### `each:` is pure

`each:`'s body is one expression, the row, worked out from the item and
what the run saved. `attempt` can only start a statement, so the body
can't fail. What can stop it is a fault, an overflow among them. That
makes the item a problem, by its place in the walk, with the fault's
classification as its rule, and its row isn't written.

### What it saves

The run saves each of the action's parameters and variables that
`each:` reads, once, as an entity field holds a value
(`type.batch_capture_not_storable` otherwise), beside `name:` and
`format:`. `finished:`'s captures and `key:` are saved as for
`batches.each`.

### Finishing

Once every page is written, the pieces are joined into the file, with no
transaction open, by the store's `join`
([Files](16-built-in-services.md#files)).
`finished:` then runs in a transaction of its own, given the
`ExportSummary`, whose `file` holds the store, the key, `name:`, the
size and the type. The file's key is made from the run's id, so a run
that joins again finds the file whole.

### Cancelling

A cancel ends the run at its next page, as for a query's run. Once that
end commits, the file's pieces, and the file if they were joined, are
dropped. If the store can't be reached then, they stay. Deleting files
nothing points at comes with the rest of the `files` capability.

### A store with no room

A store with no room pauses the run, as one that can't be reached does.
What committed stands, and the tick carries the run on.

## Batches, tenants and grants
<!-- topic: batches-tenants-and-grants -->

In an app shared by several customers, each one a **tenant**
([Tenancy](09-policy.md#tenancy)), a batch reaches only one tenant's
records: the one its saved values name. Without a `tenant:`, a batch
reaches every record the system is granted.

### Which tenant a run reaches

Each page's read, its items and `finished:` act as the system, limited
to the tenant that the run's saved values of the tenant's `id` type name
([Background work and tenants](#background-work-and-tenants)). These
saved values count:

- a `from:` argument, a value `each:` or `finished:` captures, or `key:`,
  whose own type is the tenant's `id`, such as `Household.id`, or that
  type with `none` allowed;
- a field declared with that type, as a reference to the tenant is, such
  as `rent_round.society_id`.

A tenant's id held inside a saved record doesn't count. The run reads
the values from what it saved, so it has the same tenant each time it
resumes.

Values that all name the same tenant limit the run to that tenant, as
with `key: household_id` beside an `each:` capturing `household_id`. A
run that saves no such value, one whose value is `none`, or one with
values naming different tenants, reaches no tenant's records, only those
every tenant shares.

### What a batch needs from policy

A batch runs as the system, so what it reaches must let `Access.system`
in, as for a schedule or a subscriber
([Background work](09-policy.md#background-work)). Suppose what `from:`'s
query reads, or what the action `each:` or `finished:` calls reaches
(itself or through what it calls), has a grant list, a field's own list
or a line under `actions:` that names `Access.public` and not
`Access.system`. Then that is `policy.system_grant_missing`, at that
query's or action's call. Each call is checked, so one batch may be
refused at more than one.

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
  help: docs/errors/policy.system_grant_missing.md

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
  help: docs/errors/policy.system_grant_missing.md

check failed: 1 error, 1 file
```

## What a run reaches under policy
<!-- topic: what-a-run-reaches-under-policy -->

A batch's actions run as the system, but what they do still answers to
the policy of the people who started the run or read its status. This
topic says what Jadpo checks so that a batch never leaks a record to
someone who may not read it.

### What the summary carries

An `ImportSummary` holds counts and problems, never a record's fields.
`items` shows nothing of the items the run walked, as a count of a
query's records doesn't. `items_with_problems` and `problems` count the
items whose action failed, and say which and why. So they carry what
decides whether `each:`'s action fails: what decided each `reject` and
`attempt` it runs, and whether an action it calls fails, read from the
item and from what the run saved. What decided that the run starts goes
with all three.

### The item is checked like any call's argument

Each item is one of the records `from:` finds, and it is the same record
all through `each:`'s action. So what that action stores from it, given
the item or a field of it, is checked where `each:` calls it, as a
call's arguments are.

### A saved value has left its record

What a run saves has left its record, as a value an event carries has
([Subscribers](#subscribers)): the record may change while the run goes
on. So it is checked as a value that could have come from any record of
its entity. The one exception is a record a saved tenant id was read
from, which is known to be of the tenant the run is limited to. A value
the starting action was passed is checked where that action is called,
in the same way.

### The run's tenant proves what it reaches

Every record a run reads is of the tenant it is limited to. So the item,
and what decided its action's failures, may be stored wherever that
tenant's roles may read them, as a record of an event's tenant may. A
record every tenant shares (`Tenancy.shared`) isn't the tenant's.

### Every action a run reaches is checked

What a schedule, a subscriber or a batch reaches is checked against the
line under `actions:` of every action it reaches, not only the one it
runs ([Actions](09-policy.md#actions)). A listed action's line decides
its own body's writes wherever it is called from. Routes are checked the
same way.

### What the run's actions show

Nothing reaches whoever started the run. A failure's fields, and a
record found by a value, are shown to nobody, and a problem keeps only
the failure's code.

### What `batches.status` shows

Where `batches.status` is read, nothing says which batch a `BatchRun` is,
since a run is found by its `id` alone. So what it gives is held to what
decides the failures of every batch the app starts: whether each one's
`each:` action fails, and whether its `finished:` action fails, which
ends the run `failed`. Each is checked as though it came from any record
of its entity, never one of a run's tenant, and with nothing of what the
starting action was passed.

The reader is checked only by what the reading action's line under
`actions:` lets in as it starts. A status read inside a batch's own
actions goes round in a circle, and is worked out as calls that go round
are.

So in an app with a tenant, a run's status shows only what anyone at
all, or a role held across the whole app, may read. Letting a reader in
through the record that keeps the run would need the `BatchRun` to say
which batch and tenant it is.

### No request carries a batch type

A request can't name a run. An `Input`, read from a body or, for a `GET`
or a `DELETE`, from the query string, and a route's placeholder may not
hold a `BatchRun`, a `BatchProgress`, a `RowProblem` or an
`ImportSummary`. That holds whether the type is there directly, in a
list, a map or an option, in an object, through a type built on one, or
through an entity's field holding one (`Sweep.run`); each is
`type.batch_input`. A `BatchState` is a plain enum a request may name.

A named query's parameter isn't refused, because what is passed to it
comes from a route's placeholder or input, which are checked, or from an
action. A batch type's fields can be read
([Other built-in types](02-values-and-types.md#other-built-in-types)),
but one named as a type, such as `BatchRun.id`, is `type.unknown_field`,
as is a field a standard object doesn't have. A run is reached from
`batches.each`, or from a field the caller may read.

Here `each:`'s action stores part of the item where more people can read
it: `post_item` puts a diary's confession, which only its owner may
read, on a board anyone signed in may read. Jadpo follows the item into
the action and refuses the call:

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
  help: docs/errors/policy.output_field_unproved.md

check failed: 1 error, 2 files
```

Here a run's status leaks what decided its failures. `check_diary` fails
on a diary whose confession says "draft", so the number of problems
tells the reader something about confessions. `count_drafts` shows that
number to anyone signed in, which its line under `actions:` lets in:

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
  help: docs/errors/policy.output_field_unproved.md

check failed: 1 error, 2 files
```

A route can't take a run in its path:

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
  help: docs/errors/type.batch_input.md

check failed: 1 error, 2 files
```

## Declaring an event
<!-- topic: declaring-an-event -->

An event records that something happened, so that other code can react
to it later without the action that caused it waiting. You declare it as
a type whose base is `Event`, named in the past tense, one per file in
the domain's `events/` folder.

`domains/planning/events/todo_completed.jadpo`:

```jadpo fragment
// domains/planning/events/todo_completed.jadpo
/// A todo was marked complete.
public type TodoCompleted = Event {
    todo_id: Todo.id
    completed_at: Instant
}
```

`TodoCompleted` says a todo was completed, which todo and when. Mark an
event `public` when another domain subscribes to it.

### What an event holds

An event is written as a failure is ([Declaring a failure](04-failures.md#declaring-a-failure)):
`type X = Event { ... }`, as `type X = Failure { ... }`. Its fields
follow the ordinary type rules.

Carry ids and the facts of the moment, not whole records. A subscriber
that needs more reads the current state.

### One type per event

Each event is its own type. There are no event families with variants,
such as `TodoEvent.completed`, so subscribing to an event and matching on
it never depend on a list that grows.

### An event nobody uses

An event nothing subscribes to is reported like any declaration nothing
uses.

### The event record

An event's shape is recorded, and a change to it is shown as a diff, as
an API's shape is ([The public API record](07-http.md#the-public-api-record)).
This matters because events waiting in the outbox when a new build ships
are read with the new shape. So every event is recorded, not only
`public` ones.

Each domain that declares an event keeps one record beside its events,
`domains/<domain>/events/events.jadpo_record` (written by
`jadpo records update`; never edit it by hand). It is a view in the
record format (`record events 1`), whose header says it is generated
from "the events of domain `<domain>`". There is no command of its own:
`jadpo records update` writes it, and `jadpo records` lists it.

The record holds an `event <Name>` entry for each event, marked `public`
when another domain may subscribe to it. Under each entry are its fields
as its payload carries them, in the API record's wire form
(`words: Text { max_length: 200 }`). Then comes an `object`, `enum`,
`input` or `output` entry for each type a field names, written as the API
record writes them. Events, fields and types are sorted by born name.
Descriptions are left out, as in the API record.

The record is checked as the API record is
([What is committed, and why](18-records-and-manifest.md#what-is-committed-and-why)).
A missing record, one whose events changed (`record.out_of_date`, naming
each entry added, changed or removed), and the record of a domain left
with no events, are all put right by `jadpo records update`. A hand edit
Jadpo couldn't have written is `record.edited`. While an event file has
errors, the record isn't compared.

Classifying a change as breaking, and accepting one, aren't built yet.
They come with the API record's [acceptances](18-records-and-manifest.md#acceptances).

## Emitting an event
<!-- topic: emitting-an-event -->

An action announces an event with `emit`, a statement written like
`reject` ([Raising a failure](04-failures.md#raising-a-failure-reject)).
The event reaches its subscribers only once the action's writes have
committed.

`domains/planning/actions/complete_todo.jadpo`:

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

### Saved with the action, delivered after it

The event is saved in the same database transaction as the action's
writes, and delivered only after that transaction commits. If the action
fails or rolls back, the event is thrown away and no subscriber ever
sees it. This is known as a transactional outbox: the **outbox** is the
table where events wait to be delivered.

### Who can emit

Only actions emit, and they emit only events. A function, a query, a
test, a fixture or a seed can't (`event.emit_invalid`). Routes,
schedules and subscribers emit by calling an action.

### No `attempt`, and nothing back

`emit` needs no `attempt`. It can't fail with a domain failure, and a
storage problem is a fault that fails the whole action
([Faults](04-failures.md#faults)).

`emit` returns nothing. The caller can't wait for, or read, what
subscribers do. A route that needs the finished result waits with
`settle:` ([Waiting for follow-on work](07-http.md#waiting-for-follow-on-work)).

### Where the event is kept

The event goes in the outbox of the store the action's transaction is
in, written as the action commits, with a delivery record for each
subscriber, `pending` and due at once. An action that began no
transaction keeps them in the store it writes or, if it writes none, in
the one `events: { store: }` names, beginning its transaction there for
them.

An action that emits but writes no store, itself or through what it
calls, needs `application { events: { store: main } }`. Without it, it is
`event.store_required`, even when it reads a store.

An event nothing subscribes to is still written. Its row keeps its
type's key, its payload as the contract's JSON, when it was emitted, the
emitting action's `traceparent`, the encoding its payload was written
in, and how deep in a chain of events it is. A route whose action emits
but writes no store keeps its `Idempotency-Key`s beside its events
([Idempotent writes](07-http.md#idempotent-writes)).

### Limits

More than 1,000 events in one action, or an event whose JSON is over
64 KB, is a defect as it is emitted, and the action rolls back.

## Subscribers
<!-- topic: subscribers -->

A subscriber reacts to an event: each time the event is committed, it
runs an action. It names one event in `on:` and the action to run in
`run:`. Inside `run:`, `event` is the event delivered.

`domains/planning/subscribers/update_statistics.jadpo`:

```jadpo fragment
// domains/planning/subscribers/update_statistics.jadpo
subscriber update_statistics {
    on: TodoCompleted
    run: count_completion(event)
}
```

### What a subscriber holds

A subscriber holds three things, and nothing else
(`subscriber.item_invalid`):

- `on:`, the event type. Another domain's event must be `public` and
  imported.
- `run:`, a call of an action in the subscriber's own domain
  (`subscriber.run_invalid`). Its arguments are `event` or fields read
  through it, such as `event.todo_id` (`subscriber.argument_invalid`),
  and they are type-checked against the action's parameters.
- `retry:`, optionally ([Retries and giving up](#retries-and-giving-up)).

### One event each

A subscriber listens for one event. To react to two events, write two
subscribers.

### Each subscriber on its own

Every subscriber of an event gets its own delivery. One failing doesn't
affect the others. Nothing that happens in a subscriber reaches the
action that emitted the event, which has already committed and returned.
A subscriber's action may emit further events.

### Runs as the system

A subscriber runs as the system (`Access.system`,
[Background work](09-policy.md#background-work)), not as the user who
caused the event. If it needs to know who, put a field such as
`by: User.id` in the event.

### What a subscriber stores from its event

Jadpo checks that a subscriber doesn't store a value from its event
where people who may not read the value could see it.

What an event carries is everything any `emit` of it put there, apart
from what a request sent. Along a chain of events, it also includes what
a subscriber's action emits from the event it was given. That is
because each action's `emit`s, and those of the actions it calls, count
as part of what it passes to its callers.

What the subscriber's action stores from what it is passed is checked
where the subscriber passes it, as a call is
([Policy](09-policy.md)), and refused with
`policy.output_field_unproved` at the argument. What decided an `emit`,
and any `reject` in its action, go with every field of the event.

### A value from an event has left its record

The record a value came from may change or move between the `emit` and
the delivery. So a value an event carries is checked as though it came
from any record of its entity. Of the field's readers, only "anyone
signed in" and roles held across the whole app carry over.

The one link kept is the tenant. When the event's tenant field came from
the same record as a value, that record was the tenant's, and the
subscriber is limited to that tenant. So the value may be stored
wherever that tenant's roles may read it.

A subscriber that needs more passes the event's ids and reads the
current state, as [Declaring an event](#declaring-an-event) advises.

## Background work and tenants
<!-- topic: background-work-and-tenants -->

In an app with a `tenant:` ([Tenancy](09-policy.md#tenancy)), each kind
of background work reaches a stated set of tenants, and "no tenant"
means the same thing for each: only the records every tenant shares
([Background work](09-policy.md#background-work)):

| Work | Reaches |
|---|---|
| A schedule's run | Every tenant's records: no tenant's event starts it. A run by hand, and a test's `.run()`, too |
| A subscriber's delivery | The tenant its event's one field of the tenant's `id` type names (`household_id: Household.id`), so the tenant is written in the event where a reviewer sees it. With no such field, more than one, or `none` in it: no tenant's records, only those every tenant shares |
| A batch's run | The tenant its saved values of the tenant's `id` type name, values naming the same tenant counting as that one ([Batches, tenants and grants](#batches-tenants-and-grants)). With none, `none`, or values naming different tenants: no tenant's records, only those every tenant shares |
| The magic-link `send:` action | The tenant of its delivery, as a subscriber's: `jadpo.MagicLinkEmail` names none, so no tenant's records, only those every tenant shares ([Magic links](08-identity.md#magic-links)) |
| A webhook's reply action, and `on_first_sign_in:`'s | No tenant's records, only those every tenant shares: each runs as the system inside a request no tenant's event starts, and "no tenant" is never every tenant for them |

A schedule names no tenant, so it reaches every one. A subscriber and a
batch name theirs by a value, and a value that names no tenant never
widens what they reach. The system's runs inside a request (the sign-in
email's `send:` action, a webhook's reply action and
`on_first_sign_in:`'s) reach no tenant's records. In an app without a
`tenant:`, each reaches every record the system is granted.

The value naming a subscriber's or a batch's tenant must come from a
record of that tenant, which its action reached under its caller's
policy. Otherwise it is `policy.event_tenant_unproved`
([Background work](09-policy.md#background-work)). So a caller can never
aim background work at a tenant they don't belong to.

## Reacting in another domain
<!-- topic: reacting-in-another-domain -->

One domain changes another in one of two ways
([Ownership and calls replace modules](01-project-layout-and-modules.md#ownership-and-calls-replace-modules)):
it calls one of the other domain's `public` actions, or it emits an event
the other domain subscribes to.

| | Public action | Event |
|---|---|---|
| When | Inside the caller's transaction | After the caller commits |
| If it fails | The caller fails and everything rolls back | Only the subscriber's delivery fails; the emitter is unaffected |
| Who knows whom | The caller names the action | The emitter does not know who listens |
| Use for | Rules that must hold together (reserve stock with the order) | Reactions that may happen later (send the confirmation email) |

The subscriber lives in the domain that reacts, and calls that domain's
own action. The event must be `public` in the domain that emits it. Here
the `notifications` domain emails someone when a todo in `planning` is
completed:

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
<!-- topic: long-running-coordination -->

A process that waits for several things over time, such as an order
waiting for both payment and stock, needs no workflow language. Keep its
progress in an ordinary entity, let each subscriber record one step on
it, and let a schedule handle deadlines.

Here `record_payment` sets `paid_at` on the order. If stock is already
reserved, it also sets the order's status to ready and emits
`OrderReady`. `cancel_expired_orders` cancels orders left unpaid too
long:

```jadpo fragment
// domains/ordering/subscribers/on_payment_succeeded.jadpo
subscriber on_payment_succeeded {
    on: PaymentSucceeded
    run: record_payment(event.order_id)
}

// domains/ordering/actions/record_payment.jadpo
action record_payment(order_id: Order.id) fails OrderNotFound -> Unit {
    ...
}

// domains/ordering/schedules/cancel_unpaid_orders.jadpo
schedule cancel_unpaid_orders {
    every: 5m
    run: cancel_expired_orders()
}
```

The state is in the database, so it survives restarts and deploys, and
you can query it and show it in an admin screen. Two steps arriving
together can't both complete the order, because every write checks
whether the record changed since it was read
([All-or-nothing actions](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)).

## Delivery: at least once, safe to repeat
<!-- topic: delivery-at-least-once-safe-to-repeat -->

Every committed event reaches each of its subscribers at least once.
A crash can deliver it again, so Jadpo makes a repeat harmless: database
work happens once, and outside calls carry keys that let the provider
ignore a repeat.

### At least once

An event committed is delivered to each subscriber at least once. A
crash can deliver it again.

### Database work happens once

The subscriber's writes commit together with a **handled record**, a note
that this delivery was dealt with. A repeated delivery finds the note and
does nothing.

The handled record (`jadpo_handled_deliveries`, keyed by the delivery's
`id`) is written after the action's outbox rows, in the store its
transaction is in by then: the store it writes or, if it writes none,
the one `events: { store: }` names, where its events are. So an action
that only emits keeps one, and a repeat emits nothing twice. An action
that neither writes nor emits keeps none, since running it again changes
nothing in a database. The handled record references nothing in another
store.

### Outside calls can repeat

A call to a provider can't join the database transaction. So each
delivery has a stable key, made from the event and the subscriber, and
the runtime derives from it the idempotency key each provider operation
sends ([Calling](11-integrations.md#calling),
[Several changing calls in one action](#several-changing-calls-in-one-action)).
A provider that supports idempotency keys ignores the repeat.

### Every action repeats as a unit

A changing call is a call that changes something at the provider, such
as taking a payment. A changing call that takes no idempotency key, and
that its package doesn't mark as safe to repeat, must be the only
changing call in its action. Once it has gone out, it is never sent
again blindly. A retry the action would otherwise get, or a rerun after
a concurrent change
([All-or-nothing actions](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)),
stops it as `outcome_unknown` for an operator to check instead.

Changing calls that do take keys may be several, in order. Each is
recorded as it goes, so a rerun replays what already happened rather
than sending it again. Writes aren't limited this way: every write to
the action's store, across any number of domains, commits together after
the last call.

### No order between events

No order is promised between events. A subscriber that cares about
order reads the current state rather than trusting the event's copy,
which is also what makes repeats safe.

### A payload is the contract's JSON

An event saved in the outbox, a job's arguments and the values a batch
captures are all written as the JSON of their types, by the same codec a
request's body goes through. So a standard type keeps its currency, its
unit and every decimal place
([Every boundary through the contract](19-standard-library.md#every-boundary-through-the-contract)).
Every event type is numbered in the plan's JSON table by its identity
key, and the outbox writes and reads its payload through that table
([Structured values](05-data.md#structured-values)).

A payload is read with its type as the type is when the payload is
delivered. An event type's shape is recorded as an API record's is
([The event record](#the-event-record)), and a change that would break an
API record, such as a field removed, renamed or narrowed, needs an
[acceptance](18-records-and-manifest.md#acceptances). A payload saved
before such a change that no longer reads is parked with its problem,
never dropped, as a message that keeps failing is
([Events from outside](#events-from-outside)).

A delivery reads its payload by the encoding its outbox row records
([Changing a standard type's encoding](19-standard-library.md#changing-a-standard-types-encoding)):
this release's encoding, or none for a payload kept before encodings
were recorded, which is read as encoding 1's. A payload in an encoding
this release doesn't read, such as a later release's after a rollback,
is never guessed at. Its delivery is parked, `failed` with
`payload_unreadable` and why, as a payload that no longer reads as its
type is, with its problem: the rule and the field, never the value.

### How a delivery runs

Once the action commits, each delivery is left to the runner, which
runs it after the response
([Running background work](#running-background-work)). The runner:

1. claims the delivery's record in one statement, at the host's time,
   marking it `running` with a lease of what is left of the invocation's
   budget plus a minute. A delivery another runner holds, or one not yet
   due, is left alone;
2. looks for its handled record. A delivery already handled is marked
   `succeeded` without running again;
3. otherwise runs the subscriber's action as the system, limited to its
   event's tenant ([Background work and tenants](#background-work-and-tenants)),
   in a boundary of its own, the unit that commits or rolls back as one,
   and in a new span of the event's trace;
4. records how it ended.

Tracing follows the work through events. A request continues a valid
W3C `traceparent` it brings, in a span of its own, or starts a trace. An
event keeps the trace context of the action that emitted it, so what a
delivery emits keeps it too.

A delivery to a subscriber the build no longer has fails as a defect. An
event 16 deep in a chain of events fails with `event_chain_limit`.

## Several changing calls in one action
<!-- topic: several-changing-calls-in-one-action -->

Some jobs take several calls that each change something at a provider.
Starting a subscription at a payment provider, for example, means
creating the customer, attaching the card and creating the
subscription. Each takes effect at the provider as soon as it is made.
So an action that makes them must never send one twice, and must never
lose track of one that happened. Jadpo makes this safe by recording each
call, a **step**, as it goes.

`domains/billing/subscribers/start_subscription.jadpo` and
`domains/billing/actions/subscribe.jadpo`:

```jadpo fragment
// domains/billing/subscribers/start_subscription.jadpo
subscriber start_subscription {
    on: PlanChosen
    run: subscribe(event)
}

// domains/billing/actions/subscribe.jadpo
action subscribe(event: PlanChosen) fails CardDeclined -> Unit {
    var customer: Stripe.Customer = attempt stripe.customers.create(...)
    attempt stripe.payment_methods.attach(...)
    var subscription: Stripe.Subscription = attempt stripe.subscriptions.create(...)
    attempt Account.update(...)
}
```

The three Stripe calls are steps 1, 2 and 3. The `Account.update` comes
after the last call, because the action's writes always do.

A **changing call** is a provider operation with `changes: true`, or a
store's `put`. Reads aren't counted.

### What is allowed

- **Up to 8 changing calls**, in order, counted on the action's longest
  path through its code. A package action's calls count too: its steps
  are steps of the calling action, under one record, and a second
  package action carries on the numbering.
- **Each must take an idempotency key, or be marked by its package as
  safe to repeat as it is.** A changing call with neither must be the
  only changing call in its action, even as the last step, since a step
  in doubt with no key always needs an operator.
- **Only where the action's key survives a retry.** That is in actions
  run by subscribers, and by routes whose API requires an
  `Idempotency-Key` ([Idempotent writes](07-http.md#idempotent-writes)).
  A route without a required key gets a new request id when the client
  retries, and a schedule's retry is its next run, so neither could
  resume where it stopped. Making several changing calls from either is
  a compile error, which names the fixes: require the key; or answer
  `202` and do the steps in a subscriber, the usual shape for a browser
  form; or, for a schedule, emit an event for each item.
- **No outside call inside `for`**, in any action
  ([Loops](03-logic.md#loops)), so steps are numbered along a path whose
  length the compiler knows. For a call per item, use `batches.each`.
- **`store.put` is a step** like a provider call. In an action with
  several changing calls, its key is made as any step's is, so a rerun
  writes the same object rather than a second one
  ([Files](16-built-in-services.md#files)).
- **Large answers stay out.** A package may mark an operation
  `large_output: true`, and the compiler refuses such an operation in an
  action with several changing calls.
- **Writes come after the last call**, with no transaction open during
  the calls ([All-or-nothing actions](06-reads-and-writes.md#all-or-nothing-actions-and-concurrent-changes)).
  The record's own writes are the runtime's, each in a short transaction
  of its own.

The compiler checks all of these.

### Keys

Each step's idempotency key is made from the app, the environment, the
connection, the caller's scope, the action's key (the delivery's key, or
the client's `Idempotency-Key`), the step's number and a key generation,
hashed to 36 characters or the package's `max_length:`.

A route's action that makes a changing call keeps its client's key, in
the store it writes or in `events: { store: }`'s, even when it neither
writes nor emits. Its calls' keys come from the client's key whether or
not any store keeps it.

A delivery's attempt counts its steps from 1 wherever it runs, a route
waiting for it with `settle:` included. A sign-in email
([Built-in emails](16-built-in-services.md#built-in-emails)) is made
with a new link and code on each attempt, so each attempt's calls go
under a key of their own, and it keeps no record.

Hashing also fixes a problem single calls have. Jadpo scopes a client's
key to the caller, but a provider such as Stripe scopes keys to the
account, so two users sending the same key would otherwise be given one
answer.

Each package states how its provider treats keys
([Calling](11-integrations.md#calling)): how long they are kept
(`window:`), their maximum length, the codes for "in use" and "different
request", whether failed answers are replayed, and any "use a new key"
answer.

### The record

Before a step is sent, the runtime records its position, its operation,
a fingerprint of its request (over the typed value) and the time it was
sent. When the answer comes, success or failure, it records the answer,
encrypted. An answer over 1 MB is a fault naming the step. A read from a
provider after the first changing step is recorded the same way;
database reads aren't.

The record lives in the store the action writes, beside the outbox. With
no such store, `events: { store: }` is required. The record is deleted in
the action's commit transaction. Records are listed in the personal-data
report ([Personal data](15-personal-data.md)).

### A rerun replays

When the action runs again, after a retry, a redelivery or a rerun after
a concurrent change, the answered steps are replayed in order without
being sent. `clock.now` is the time stored in the record, fixed for the
record's whole life. So timestamps written after a resumed action are
from when it started, which is also when the provider acted.

Only your code's `clock.now` is fixed. The runtime's own timestamps use
the host's time: an event's `emitted_at`, from which `within:` counts, a
request's AWS signature, the day an `ai` budget counts, and a received
event's time.

At each step, the operation and the fingerprint must match the record,
and at the end every recorded step must have been replayed. Any
difference stops the action as `diverged` before anything is sent,
naming the step and the fields that differ. A deploy that changed the
code, a fresh database read that changed a request, or changed
configuration can each cause one. A `diverged` action is held for an
operator rather than failed, so deploying a fix and retrying finishes
it.

### A step in doubt

A step in doubt was sent but has no answer recorded. It ends the run, to
be retried later, and the next attempt sends it again under the same key
while the provider's window lasts. It parks as `outcome_unknown` instead
when it has no key, when its window would pass, or when the provider
replays a `5xx` under that key.

A step answered with one of its operation's own failures whose kind is
`unavailable` or `rate_limited` did nothing. It stays in doubt, and its
failure is never kept or replayed.

### One runner per record

A route's key is claimed before its action runs. A delivery's lease
carries a generation number that every write to the record checks. A
provider's "key in use" answer means wait and retry, never a failure.

### Ending for good after a step took effect

After the first changing step, every point that can end the action for
good is a warning unless handled with `match`: a step's failure, a write
that can be refused, a `reject`, or a called action's failure. The
warning names three fixes:

- check first, before the first step;
- record what happened and finish in a subscriber;
- or undo it as another step.

Package surfaces list which failures follow an effect, so the warning
reaches through package actions. It holds for single calls too.

### An unfinished record never disappears quietly

A record that ends without committing (failed, diverged, parked, or a
route whose key lapsed) becomes an operator item. It lists each step
and its answer, and offers retry (`--not-done` for a new key generation)
and abandon ([What an operator sees](#what-an-operator-sees)). It is kept
at most 30 days; after that only the step list remains.

### Deploys

Each record keeps a digest of its action's step shape. Before a deploy,
an operator runs `jadpo runs unfinished --build <build id>` against
production. It lists the unfinished multi-step runs and batch runs whose
recorded path the new build changes, so they can be let finish first. A
run resumed under a changed build still stops as `diverged`.

### Tests and tools

Fakes honour keys and can answer `lost` and `in_use`. `jadpo test` runs
every multi-step test again with a crash at each step boundary, on by
default ([Fakes for integrations](13-testing.md#fakes-for-integrations)).
`jadpo dev`, `jadpo watch` and `jadpo deliveries show` list each step's
state ([Debugging and review](17-debugging-and-review.md)).

### Providers that take no key

Calls to providers that take no key, such as GitHub, Twilio's messages
and Postmark, stay one per action. A Stripe `500` at a later step is
stored under its key for a day, so it parks the action for an operator
rather than being retried.

### What `jadpo check` checks

- up to 8 changing calls on one way through an action
  ([`step.too_many`](../errors/step.too_many.md));
- a changing call with no key is the only one in its action
  ([`step.unkeyed`](../errors/step.unkeyed.md));
- several only from a subscriber, or a route whose API requires
  `Idempotency-Key` ([`step.context`](../errors/step.context.md));
- no `large_output: true` operation among them
  ([`step.large_output`](../errors/step.large_output.md));
- a store for their record
  ([`step.store_required`](../errors/step.store_required.md));
- no outside call inside `for`
  ([`effect.call_in_loop`](../errors/effect.call_in_loop.md)).

Each way through an `if` or a `match` counts on its own, a `return` ends
its way, and an action called counts the changing calls of the longest
way through it.

### The late-failure warning

After the first changing call on a way through an action, each point
that can end it for good is
[`step.late_failure`](../errors/step.late_failure.md). There is one
finding for the action, at the first such point, with the others beside
it. You may decline it, since the condition may be one that can't
change.

A point is:

- an `attempt` whose call or write passes on a failure that isn't
  `unavailable` or `rate_limited`: an operation's or a called action's
  failure, a write's `conflict:` or not-found, or a construction's;
- a `reject`;
- a `propagate`.

A call whose failures a `match` handles isn't a point, though a `reject`
in one of its arms is. Where two ways meet, a changing call on either
counts. A package's action is never warned itself. Instead its surface
lists `after_effect` for each failure that can follow its own changing
call, and the app's `attempt` of it is the point, even as the app's
first call. The rule holds for single calls too.

The same walk adds up how long a route's outside calls can take:
[`route.calls_past_deadline`](../errors/route.calls_past_deadline.md)
([Time limits](07-http.md#time-limits)).

### How the record is kept

An action whose longest way makes several changing calls keeps its steps
when it is run by a subscriber's delivery, or by a route whose API
requires `Idempotency-Key`, under the key it claimed. The steps go in
`jadpo_step_records` and `jadpo_steps`
([System tables](05-data.md#system-tables)), in the store it writes or
in `events: { store: }`'s.

- Each step (each changing call, and a provider read after the first) is
  written before it goes out, with its operation, an HMAC of each
  top-level field of its request, and when it was sent. Its answer is
  written after, sealed.
- A rerun (a delivery's retry, a client's retry under its key, or a
  rerun after a concurrent change) replays an answered step without
  sending it once its operation and fields match, with `clock.now` fixed
  at the first run's start.
- A step in doubt goes again under its key within the provider's
  `window:`, less a margin. It parks as `outcome_unknown` after the
  window, with no key, or on a `5xx` from a provider that
  `replays_failures: true`.
- The provider's `in_use:` answer is retried later; its `reused:` answer
  is `diverged`.
- A request that differs, a run that makes another operation at a step,
  and a run that ends before replaying every recorded step all stop as
  `diverged` before sending anything. The delivery is held in that state
  with the step, the operation and the fields, never a value.
- A record written by a newer runner (a delivery's later attempt, or a
  route's later claim) takes no write from an older one.
- A `put` in such an action keeps its object under a key made from its
  step.
- The record is deleted in the action's commit.

Answers are sealed with `application { steps: { encryption_key: } }`
([`step.encryption_key_required`](../errors/step.encryption_key_required.md),
[`step.settings_invalid`](../errors/step.settings_invalid.md)).

### An example

A bakery pays an approved invoice and keeps a copy of it, two changing
calls each under its own key, and then records the payment, all in a
subscriber. Its settings, the Acme Pay package and the types it uses are
hidden here; the subscriber and the action follow them.

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

#### Following steps in `jadpo dev`

In `jadpo dev`, each step a request's action reached is a line of the
request's trace, under the request's line, in place of its call. The
line gives the step's place, its operation, where its answer came from,
and what became of it: `replayed`, `answered`, its failure's code,
`in doubt`, `diverged` or `outcome unknown`. It never shows a request,
an answer or a key.

A delivery whose action reached steps gets a line of its own, as a
schedule's run does. It shows the delivery's id, the subscriber and how
the attempt left it, with its steps under it and, when it needs an
operator, `jadpo deliveries show <id>`. `--json` carries both as
`steps`, and the delivery as an event of its own.

For each store, the tick counts the unfinished records and notes when
the oldest started. Once a record has been unfinished for a day, it
raises an alert with no setup
(`{"event": "step.alert", "alert": "unfinished_steps", ...}`), once
across all instances, until none is unfinished. `jadpo deliveries` shows
the alert beside the items, and its `--json` the count.

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

#### Testing it

The bakery's test scripts one payment. `jadpo test` runs the test again
with a crash at each boundary of both steps. Each time, the step sent
again goes under its key and is answered with what the fake kept, so
one scripted answer is enough:

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
<!-- topic: where-events-travel -->

Events stay inside the Jadpo runtime. Each is stored in the outbox of
the store the action writes, in the same transaction as the action, and
the runtime delivers it:

- on a long-running server, from its own workers;
- on Cloudflare Workers, right after the commit (`waitUntil`), with the
  once-a-minute tick sweeping up anything left.

So one place holds every log and trace, a request is never slowed by
delivery, and you don't need a queue service. A route that wants the
finished result rather than a `202` waits for the work its action set
off with `settle:`
([Waiting for follow-on work](07-http.md#waiting-for-follow-on-work)).

An outside queue is never how events reach your subscribers. A
connection can *publish* chosen public events outward, so other systems
can react to them. That is a bridge fed from the outbox after the
commit, at least once, with the trace context attached, to Cloudflare
Queues, SQS, Pub/Sub or a webhook. Reading a queue the other way,
through `receiving`, turns each message into one of the app's own events
in its own outbox ([Events from outside](#events-from-outside)), so the
runtime still delivers it to subscribers.

### Running background work

The runtime drives one runner for all background work, so the
interpreter and compiled code run it the same way. The runner calls an
action with its arguments, as the system, in a boundary of its own, the
unit that commits or rolls back as one, and runs what that action sets
off after it. Every piece of deferred work has a record of its own in
the store, a delivery or a batch run, so none is lost when an invocation
stops short.

- **After the response, never in it.** A committed action's deliveries,
  and the batch runs it started, are left for the runner, not run while
  the request waits, except under `settle:`. A deployable hands them to
  its host, which runs them once the response has been sent: on a native
  server, as a task on the worker thread that answered, with that
  worker's program, so no thread needs a program of its own; on Workers,
  with `waitUntil`. What a host does with a request (answer it, and hand
  on what it left) and with a tick is one runtime type every host wraps.
  So a native server and `jadpo dev` serve through the same handler, and
  `jadpo dev` only listens in to print its lines. Each piece gets a
  context of its own, under the host's clock as it starts and with no
  seed. A host with nothing to run it on, a test's or a tool's, drops
  it, and the tick picks up what a dropped piece left.
- **A budget for each invocation.** An invocation is one run of the
  host's code: after a response, a tick, or work handed on. Every
  invocation that runs background work is given a time budget by its
  host, at most the 15-minute limit of a run. On a native server and
  under `jadpo dev` the budget is that limit. A test's host may give
  less. On Workers it is 30 seconds after a response and for work handed
  on, and 15 minutes for the cron. A tenth of the budget, at most 5
  seconds, is kept back for giving claims back: once no more than that
  is left, the runner takes no more work.
- **A run's limit within the budget.** A subscriber's or a schedule's
  action runs until the 15-minute limit, or until what is left of the
  budget before the time kept back, whichever is sooner. Only the body
  is timed, never its commit: the transaction is held open until the body
  has its value, then committed outside the timing. So work that
  committed is never given back as though it hadn't run, and a body that
  ends past its limit keeps nothing. A delivery stopped this way counts
  against its `retry:` budget
  ([Retries and giving up](#retries-and-giving-up)). A schedule's run is
  recorded failed with `deadline_exceeded`. The purge schedule's run is
  held to the same limit.
- **Every lease the budget plus a minute.** A delivery's claim, a
  schedule's lease and a batch run's lease last what is left of the
  claimer's budget plus a minute. So a Worker that is ended loses what it
  held at most 90 seconds after it began. On a native server, with the
  whole run limit as its budget, a lease lasts at most 16 minutes. A
  claim taken while a route waits for its follow-on work lasts the
  longest deadline a route has, 60 seconds, plus the minute.
- **Work not reached waits in the store.** A delivery an invocation
  didn't reach stays `pending`, and a batch run waits on its row. What
  an invocation after a response left goes on in a fresh invocation with
  a budget of its own, once the invocation ran something. An invocation
  that could run nothing leaves the work for the tick, rather than handing
  it on for ever. On a native server the fresh invocation is a task of
  its own. On Workers it is a request to the Worker itself, through the
  loopback binding `ctx.exports` gives its own `JadpoHandOn` entrypoint,
  naming each piece by its record. The fresh invocation reads the pieces
  back and runs them in its own `waitUntil`, within what is left of its
  own 30 seconds, counted from its start
  ([design notes](notes/12-jobs-and-events.md), "Work handed on, on
  Workers"). A hand-on the platform refuses, for example past its limit
  on how many invocations one request may chain, leaves the work in the
  store for the minute's cron. The tick finds what is due at its time,
  then runs each piece under the host's clock as that piece starts, so
  work that takes time never claims with a lease already past.
- **Stopping gives work back as it was.** A native server asked to stop
  (SIGTERM, or ctrl-c under `jadpo dev`) accepts nothing more, starts no
  more ticks, and ends every budget at once. The action running stops,
  and nothing it did is kept. A delivery is given back `pending`, its
  attempt not counted, as a route's `within:` gives one back. A batch
  run's lease is given back. A schedule's run is recorded
  `deadline_exceeded`, and its lease given back. A route waiting for its
  follow-on work stops waiting, as when `within:` passes. The server
  waits at most 10 seconds for this, then ends, and `jadpo dev` exits 0.
  A second signal ends the process at once, with 143 for SIGTERM or 130
  for SIGINT. If the signals can't be watched, `jadpo dev` runs without
  them, and a signal ends it at once.
- **Backfills.** A backfill worked out in Jadpo
  ([Computed backfills](05-data.md#computed-backfills)) runs its rows on
  the same runner, a batch at a time. Each value is kept and checked by
  its field's rules, as any value of the field's type is.

Not built yet: publishing events outward, and the error for signals that
can't be watched, `cli.signal_handler_failed`.

## Events from outside
<!-- topic: events-from-outside -->

Besides the events your actions emit, four sources outside the app
produce events: webhooks, a package's own events, polls and queues. Each
event is written to the app's outbox and delivered to subscribers like
any other, with the same retries.

### Webhooks

A connection with a webhook secret generates a route,
`POST /webhooks/<connection>`
([Webhooks as events](11-integrations.md#webhooks-as-events)). For each
request, the runtime checks the signature, allowing five minutes of
clock difference, drops repeats by the provider's event id, and writes
the event to the outbox. Subscribers run after the commit.

Every webhook event carries the provider's `occurred_at`. Webhooks may
arrive out of order, which the app handles by ignoring an event older
than what it stored; a recipe shows how.

A webhook that needs a reply runs the connection's named action inside
the request instead
([Webhooks that need a reply](11-integrations.md#webhooks-that-need-a-reply)).
That event is never kept in the outbox, and its reply is kept with its
id for a repeat.

An event's id is recorded in `jadpo_received_events`, and its outbox row
and deliveries are written in the same transaction, in the store
`events: { store: }` names or the only store of records. `within:` is
counted from when it arrived. The tick deletes the ids after 30 days.

### A package's own events

A package's `public` events are subscribed to with `on:`, like the
app's own. These are its webhooks' events, and the richer events its own
subscribers emit from them, such as `Stripe.SubscriptionRenewed`. An
event without `public` reaches only the package's own subscribers, and
subscribing to one is a compile error.

A package emits only about the outside world, never as a side effect of
the app calling it. When calling a package should announce something,
the app's action emits its own event there, where a reviewer sees it.

A package's subscriber hears its webhooks' events once for each
connection that receives them, and its action calls the API through
that connection. What the action emits is kept in the outbox where the
webhooks are, and delivered to the app's subscribers of the package's
public events ([Recipes](11-integrations.md#recipes)).

### Polls

Use a poll to turn an API without webhooks into events. `polls:` in a
package's `provider.jadpo` names an operation, a default interval, and
the event emitted when the answer changes. The runtime runs a poll only
while something subscribes to its event, keeps what it last saw (in the
database, like webhook de-duplication) and emits only changes. The
connection can change the interval: `polls: { latest_rates: 6h }`
([Polls](11-integrations.md#polls)).

Polls are for feeds that don't depend on the app's data, such as rates
or a price list. To check each of the app's own records, write a
schedule that calls the package; a package can ship one as a recipe.

The tick calls each poll the app subscribes to through its connection
once it is due. What the poll last saw is kept in `jadpo_polls` beside
the outbox, and its event and deliveries are kept with the new digest in
one transaction.

### Queues, through `receiving`

`receiving` reads messages from a queue and turns each into one of the
app's events. It is the inbound twin of `publishing`.

`app.jadpo`:

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

Here the `warehouse_queue` connection reads an Amazon SQS queue. Each
message's `type` field says which event it is, and its `message_id`
field is its id.

- **What a queue names.** The field giving each message's type
  (`type_at:`), the field giving its id (`id_at:`), and the events it may
  carry. A queue of one kind lists one event and no `type_at:`. The
  events come from another Jadpo app's generated package, or are declared
  by the app, in a domain's `events/` or in its own integration in
  `shared/providers/`, each with its `wire_name:`. The queue's package
  only knows how to fetch messages.
- **Acknowledged after the commit.** Each message becomes its event for
  the app's subscribers. It is acknowledged once the transaction keeping
  its event and its id commits, and its subscribers are then delivered
  from the outbox. Repeats are dropped by the message id, stored in the
  database.
- **Never dropped.** A message whose type isn't listed is parked for an
  operator. A message that keeps failing is left to the queue's own
  dead-letter queue, and parked for an operator. On Cloudflare Queues,
  the Worker reads the dead-letter queue and parks each message it hands
  back as `dead_lettered`. A message that can't be kept is parked whole
  in `jadpo_parked_messages`, which `jadpo deliveries` lists, retries and
  dismisses.
- **How often.** Queues are read on the tick, at least every 10 seconds
  ([Queues](11-integrations.md#queues)).

Which kind of connection reads which queue:

- An HTTP API is a package. Amazon SQS and Google Pub/Sub's pull API are
  packages.
- SNS and Pub/Sub's push are webhooks, through a package with a
  handshake.
- Anything that needs its own protocol or a platform binding is a
  built-in driver. Cloudflare Queues is a built-in driver on Workers, its
  consumer written into the Wrangler configuration, and a package (its
  HTTP pull API) from a native server. Kafka is a built-in driver in a
  later release.

The Cloudflare Queues driver
([A Workers deployable](14-tooling.md#a-workers-deployable)) is a
connection with `driver: Driver.cloudflare_queues`, `queue:` and
`dead_letter_queue:`. Its consumer is handed each batch of messages and
answers for each one: acknowledged once kept, as a repeat or parked, or
retried when the store can't take it yet. A message the Worker can't
take yet is retried for about 22 hours, its delay doubling up to an
hour, so it outlasts a deploy's migration. The dead-letter queue's
consumer parks what reaches it.

## Retries and giving up
<!-- topic: retries-and-giving-up -->

When a subscriber's action fails, Jadpo decides whether to try again
from the way it failed. You don't write anything for that. A failure
that may pass, such as a provider being down, is retried later; one that
would happen again with the same input isn't. The failure kinds are
those in [Kinds](04-failures.md#kinds):

| How the action ended | What happens |
|---|---|
| Succeeded | Done |
| A failure of kind `unavailable` or `rate_limited` | Retried later |
| Any other declared failure (`not_found`, `conflict`, `rejected`, ...) | Failed; not retried, because the same input fails the same way |
| A fault before anything committed (database unavailable, timed out) | Retried later |
| `outcome_unknown`: an outside call may have happened | Parked; never retried automatically |
| A defect (`internal_fault`, overflow) | Failed; not retried until someone fixes the code |

A fault reaching a store, and a run out of time, are retried as
`unavailable` is. A delivery's record keeps the failure's code and kind,
never its fields.

### How long between tries

Each retry waits longer than the last, with random jitter, so many
failures don't all retry at once. Each wait is drawn evenly from zero to
a ceiling that starts at 30 seconds and doubles with each attempt, up to
6 hours, and is never under a second. A `Retry-After` from a provider is
honoured.

### How many tries: the default

By default a subscriber gets 10 attempts within 24 hours, then fails. A
delivery fails when it is past its attempts, or when its next attempt
would fall more than `within:` after the event was emitted.

### Changing the budget: `retry:`

Use it for a subscriber that must keep trying for longer, such as one
recording payments from a provider that may be down for a while.

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

This subscriber gets 20 attempts within 3 days. `attempts:` takes 1 to
100, and `within:` takes up to 30 days; anything else is
`subscriber.retry_invalid`. A schedule has no `retry:`, since its next
due time is its retry.

### A run that takes too long

A delivery whose action is stopped by the run's time limit
([Running background work](#running-background-work)) is an attempt
that ran out of time (`deadline_exceeded`). It counts against its budget
at once: it is tried again after a wait while the budget lasts, and
failed after that, with nothing it did kept. So a subscriber that never
ends fails after its attempts, rather than using up every invocation's
budget.

### Given up at the claim

A delivery claimed again after its `retry:` attempts are used, or later
than `within:` after its event was emitted, fails without running.
Claiming one a worker held past its lease marks its last failure
`lease_expired`, as a schedule's run lost with its worker ends. The
claim that gives up isn't counted as an attempt. The record keeps the
last attempt's failure, or `deadline_exceeded` for a delivery never
tried in time. A handled record found first still makes it `succeeded`,
and a store that can't be reached while looking for one is retried
within the same budget.

### When a provider keeps failing

Jitter keeps retries from arriving together, but 5,000 deliveries
calling a provider that is down would still each be tried ten times. So
the outbox keeps a **circuit breaker** for each connection. After a run
of `unavailable` failures from one provider, the deliveries that call it
are held rather than tried. After a pause, one of them, the probe, is
tried, and if it succeeds the rest are released. Held deliveries keep
their retry budget, are shown in `jadpo deliveries`, and raise an alert.
You don't write anything for this.

- **What opens the breaker.** Five delivery attempts in a row whose call
  through the connection found its provider unavailable open its
  breaker. An attempt counts once for each connection it called
  through, by how the last such call ended. It counts as unavailable
  when the call surely did nothing or no answer came back: a `5xx`, a
  `408`, a `429`, a timeout, or a call that never left. It counts as
  answered, ending the run of failures, when the provider answered at
  all, a refusal or a rejected credential included. A delivery attempt
  counts once however many times the connection's own `retry:` sent its
  call ([Calling](11-integrations.md#calling)), by how the last of them
  ended. So one a retry got through ends the run, and five attempts of
  three refused sends each open the breaker, not fifteen sends. Only
  calls to the connection's own driver count, never a fake's or a
  recording's answer, and only calls a delivery makes: a route's calls
  are its caller's to retry.
- **Which deliveries are held.** A subscriber's delivery, when its action
  calls out through the connection, itself or through an action it
  calls, or when it hands its event to a capability the connection
  fills. An action a batch's run calls runs after the commit, not in the
  delivery, so it doesn't count. Each delivery is held at its claim,
  before it runs: it becomes `held` and not due, the attempt its claim
  counted is given back, and its connection and the time it was held
  from are kept on its record.
- **The pause and the probe.** The first pause is a minute. Once it has
  passed, the first delivery claimed for the connection is the probe,
  and the rest stay held. The tick's sweep wakes the delivery held
  longest to be the probe, so a probe goes even when nothing new
  arrives. A probe the provider answers closes the breaker and releases
  every delivery it held, due at once. A probe that finds the provider
  unavailable opens the breaker again for twice the last pause, at most
  ten minutes. A probe whose run no longer calls the provider gives the
  probe back, and one whose worker is lost loses it when its claim's
  lease ends. A delivery already running when the breaker opened
  finishes, and its answer closes the breaker too.
- **The budget.** Being held spends no attempt, and the time held doesn't
  count towards `within:`. A delivery's `within:` counts from its event,
  or from an operator's retry, moved on by the time it was held. An
  operator's retry gives a fresh budget, the time held included.
- **Where it is kept.** In `jadpo_circuit_breakers`
  ([System tables](05-data.md#system-tables)), in every store that keeps
  deliveries, each store's breakers holding its own deliveries. Every
  change is one conditional statement, so the breaker holds across
  processes and on Workers. The sweep also releases any delivery whose
  breaker has closed, which a release racing a hold can leave.
- **The alert.** The tick raises one alert for each outage,
  `connection_unavailable`, recorded on the breaker's row as a
  schedule's alerts are on its state, so of several instances only one
  raises it. It is raised again only after the breaker has closed and
  opened again. Its log event is
  `{"event": "delivery.alert", "alert": "connection_unavailable", "connection": …, "since": …, "held": …}`,
  and `jadpo dev` prints it with the schedules' alerts.

### Not built yet

The jitter and `Retry-After` are built for a call's own retries within
one attempt ([Calling](11-integrations.md#calling)). Between attempts of
a delivery, the next attempt waits only its backoff.

## What an operator sees
<!-- topic: what-an-operator-sees -->

Background work that goes wrong waits for a person rather than
disappearing. Every delivery and schedule run has a state, and the
operator commands show the ones that need someone:

| State | Meaning | The operator can |
|---|---|---|
| `pending` | Waiting to run, or waiting to retry | Cancel it |
| `running` | A worker has it | Wait; a worker that dies loses it after its time limit |
| `held` | Held while a provider it calls is unavailable ([When a provider keeps failing](#when-a-provider-keeps-failing)) | Wait; a probe that gets through releases it |
| `succeeded` | Done | Nothing |
| `failed` | Gave up, or a failure that is not retried (the dead-letter list) | Retry it once the cause is fixed, or dismiss it |
| `outcome_unknown` | An outside call may have happened | Check with the provider, then mark it done or not done; "not done" retries it |
| `diverged` | A rerun of an action with several changing calls no longer matches its record | Deploy a fix and retry, or abandon it |

Each failed or unknown item keeps the event, the subscriber, every
attempt with its failure code and internal fields (logged by the rules
in [Logging](04-failures.md#logging)), and the request that emitted it.
A `failed` or `outcome_unknown` item writes a log event and counts
towards an alert. The commands are described in
[Tooling](14-tooling.md#the-jadpo-command).

### `jadpo deliveries`

`jadpo deliveries` lists the items that need an operator, `failed` and
`outcome_unknown`, newest first, 20 to a page. `--state` lists those
`pending`, `running`, `held`, `dismissed` or `cancelled` instead. A
connection whose circuit breaker holds deliveries is an alert line on
the list and on the held ones, with when it became unavailable, how many
it holds and when one is tried again.

You act on an item by its id: the last 8 hex digits of its UUID, or more.
The last digits are the random part of a version 7 UUID, whose first
digits are its time.

- **Two more states.** `dismissed` and `cancelled` mean an operator set
  the delivery aside. Neither runs again. Each is kept 30 days after the
  operator's last action on it, with its event, and then the tick
  deletes it ([The tick](#the-tick)).
- **What each command does to a delivery.**
  - `retry` (from `failed`) makes it `pending` and due now, with a fresh
    budget: its attempts counted from none, and its `within:` from the
    retry (the record's `retried_at`), not from its event. So one retried
    after its window still runs. The tick's sweep then delivers it.
  - `dismiss` (from `failed`) and `cancel` (from `pending`) set it
    aside.
  - `resolve --done` (from `outcome_unknown`) marks it `succeeded`, and
    `resolve --not-done` retries it as `retry` does.

  A delivery whose outcome is unknown is never retried blindly: `retry`
  is refused and points at `resolve`. Each command is one statement that
  changes the record only from the state it was read in, so of two
  operators acting at once, one acts and the other is told it changed.
- **Schedule runs.** A schedule's item is its latest run, a skipped tick
  aside, when that run failed or its outcome is unknown. It stays until
  a later run ends it or an operator deals with it, so a schedule failing
  every 15 minutes is one item, not a backlog. Its earlier runs stay in
  `jadpo schedules history`. `retry`, and `resolve --not-done`, ask the
  app's tick to run the schedule by hand and wait for its record, as
  `jadpo schedules run` does against a deployment
  ([Running a schedule by hand](#running-a-schedule-by-hand)). A run
  asked for that way is `pending` until a tick takes it, and `cancel`
  withdraws it. `dismiss` and `resolve --done` leave the run record as it
  is, and settle the item in the log.
- **Who did it.** Every command writes a row of `jadpo_operator_actions`
  ([System tables](05-data.md#system-tables)) in the store the item is
  in, in the same statement as its change. The row holds when, which
  command, the item and its state then, the name the operator's machine
  gives (`USER`, else `LOGNAME` or `USERNAME`), and the database role
  PostgreSQL authenticated, which the statement reads itself
  (`session_user`), so it can't be faked. The CLI shows both under the
  result: `logged  sam · role app_owner`. A row is kept 30 days, as a
  run record is, and after that until its delivery or run is gone. So an
  item still on record keeps the last thing done to it, and a dismissed
  run stays dismissed; then the tick deletes the row. Jadpo has no
  operator accounts of its own, as it has no reviewer accounts
  ([Review and sign-off](14-tooling.md#review-and-sign-off)): the
  database role is the identity.
- **Which databases.** The commands read every store that holds an
  entity, which is where deliveries are, and `schedules: { store: }`.
  Each is found as `jadpo schedules` finds its database: `--url` (with
  `--store` when there are several), else the connection's setting,
  else a running `jadpo dev`'s database. The commands run none of the
  app's code, and read none of its settings beyond those addresses.

### Unfinished multi-step runs

An action with several changing calls that ends without committing
(failed, diverged, parked, or a route whose key lapsed) is an operator
item too. The operator sees what took effect at each provider, step by
step with its answer, and can retry it (with `--not-done`, under a new
key generation) or abandon it. Nothing about it is quietly dropped. An
item is kept 30 days, then only its step list.

- **Listing them.** A delivery that `diverged` is listed by default
  beside those that failed or are in doubt, and `--state` also takes
  `diverged`, `unfinished` and `abandoned`. A route's record whose
  client didn't retry within a day of its last run is an item of its
  own, `unfinished`, with the action as its target.
- **`jadpo deliveries show <id>`** lists an item and, when its action
  keeps a record, each step: its operation; `answered`, `failed`,
  `no answer` or `answer dropped`; when it was sent; and its key's
  generation once replaced. It never shows a request, an answer or a
  key. Then it says the one thing to do, a decision for a step in doubt.
- **`retry`** takes a delivery that failed or diverged, its record's
  generation counted again from none, as its attempts are.
- **`retry --not-done`** (and `resolve --not-done`) sends the last step
  again under its next key generation when it has no answer or failed,
  its answer cleared.
- **`abandon`** ends a delivery that failed, diverged or is in doubt, or
  a route's record, keeping its step list. A delivery with a record is
  abandoned, not dismissed or resolved as done, since its action's
  writes never committed.

Each change is one statement with its log row, when the record is in
the delivery's store. The tick drops a record's answers 30 days after
its last run, and deletes an abandoned record, and its delivery, 30
days after it was abandoned. A route's client retrying under an
abandoned record's key is answered `500`, and nothing is sent. The
provider's `new_key_on:` answer sends a step again at once, once, under
its next key generation, which the record keeps first.
`jadpo runs unfinished --build <build id>` lists the records and batch
runs a new build changes.

### Batch runs and queue messages

Batch runs and queue messages show here as well. Each batch run shows
its state, its progress against the file's size, its problems and an
estimated finish, which `batches.status` gives the app. Each queue
message shows when it was parked because its type isn't listed or
because it kept failing.

### Watching schedules

A schedule that stops running usually produces no error at all. Jadpo
records and checks every part of a schedule's life, so you find out.

- **Every run is recorded** in a system table, `jadpo_schedule_runs`:
  its due time, start and end; its outcome (`running`, `succeeded`,
  `failed`, or `skipped` with `paused` or `lease_held`) with its
  failure's code and kind, never a value; the records a purge deleted;
  the events the run kept once it committed; and its `traceparent`, so a
  run opens as one trace. Records are kept 30 days.
- **Every tick is recorded too,** skipped ones included, with why (lease
  held, or paused). Each state row keeps the last tick that reached it.
- **`jadpo schedules`** lists each schedule with its last run and next
  due time (or `‖ paused`), the alerts that hold, and a footer. A run
  holding the lease shows as the last run, `● running`, with its start.
  Recent failures are the failed runs since the last success, shown
  beside a failed last run (`· 3 in a row`). The command succeeds
  whatever it lists, a failed last run or an alert included, as other
  listing commands do, and `--json` carries the alerts for a health
  check.
- **`pause` and `resume`** set the state row's `paused`, making the row
  as the first tick would if none has yet. A run under way carries on.
  Pausing a paused schedule, or resuming one that isn't paused, changes
  nothing and says so.
- **`history <name>`** gives every record, runs and skipped ticks,
  newest first, 20 to a page (`--page <n>`), grouped by UTC day. Then it
  gives figures over the records kept: runs, failed runs and skipped
  ticks, the median and longest duration, and the median and greatest
  delay (how late each run started).
- **Which database the commands read.** The one the schedules' state is
  in: the one `--url` names, else the address its connection's setting
  gives ([Database and other connection URLs](10-configuration-and-time.md#database-and-other-connection-urls)),
  else the one a running `jadpo dev` started for the session. `jadpo dev`
  notes that one in `build/dev/stores.json`, and removes it when it
  stops. There is no `--env` flag: an environment is its configuration.
  Times are UTC, as schedules count them.
- **`jadpo why <schedule>`** answers "why didn't it run?", in the shape
  of every `jadpo why` answer ([Debugging and review](17-debugging-and-review.md)).
  It gives the first of these that holds: paused, with the due times
  skipped since; no tick since a time; a lease held past its end; a run
  still holding it; a tick skipped since the last run; the last run
  failed, with its code, kind, failures before it, next due time and
  trace; it succeeded; not due yet; or no tick has seen it. Its `.jadpo`
  place is the schedule's declaration, or its doc comment's line when it
  has one. Other alerts follow. The answer is the runtime's, as data (the
  answer, the alerts that hold, a standing request), which the CLI says
  in words, and the console and the MCP server read too.
- **Metrics.** Each run's lateness (its start minus its due time) and
  duration, measured on the monotonic clock from the run's own time, are
  carried by the tick's result and `jadpo dev`'s `--json` lines, and by
  `history` for each schedule. So are skipped ticks and failures for each
  schedule ([Telemetry](16-built-in-services.md)). Items per run wait
  for a schedule run that counts them, as a purge does.
- **Alerts with no setup.** Each alert is a condition worked out from the
  rows at a given time:
  - ticks stopped, when the last tick any schedule received is older
    than twice the host's tick period;
  - a lease past its end whose run never ended;
  - a run that has taken, or took, 80% of the 15-minute limit;
  - three or more failed runs since the last success;
  - and, unless paused, no success for over twice `every:`, counted from
    the last success's start (kept on the state row as
    `last_succeeded_at`, so a monthly schedule isn't lost with its
    30-day records) or else from its first due time.

  Each is a log event, `{"event": "schedule.alert", "alert": …}`.
- **The tick raises the alerts.** Once a tick's schedules have run, it
  works out the alerts that hold for each schedule, as any reader does.
  It records them on the schedule's state row (`alerts`: each alert's id
  and when it was raised), in one statement that changes the row only
  while it is what the tick read. Of several instances ticking, or
  several Workers cron invocations, the one whose statement changed it
  raises what newly holds, and the others raise nothing. An alert is
  raised once until it stops holding; changes to its details, such as a
  growing failure count, change nothing. One that stops and starts again
  is raised again. A tick never raises "ticks stopped", since a tick
  arriving proves they haven't. `jadpo dev` prints the alerts its tick
  raised, so a restarted session doesn't print one already raised.
  Readers still work the alerts out when they read: `jadpo schedules`
  and `jadpo why` show those that hold at their own time, since a
  stopped app raises nothing, and its operator must still see its
  alerts.

### Running a schedule by hand

`jadpo schedules run <name>` runs a schedule now, paused or not, under
its lease. While a run holds the lease, it is refused with
`schedules.lease_held` and nothing is done. Its record's reason is
`by_hand`, and the grid isn't moved. Where it runs depends on the
database:

- **Against `jadpo dev`**, meaning the database a running `jadpo dev`
  noted for that store in `build/dev/stores.json`, it runs the schedule
  straight away in the command's own process. It uses the app's settings
  from the environment over `.env.local`, runs as the system reaching
  every tenant at the command's time, with its due time when it started,
  and shows the outcome.
- **Against a deployment**, meaning any other database, including one
  `jadpo dev --url` was given, it reads none of the app's settings and
  builds none of its code, so a deployment's settings and secrets never
  leave its servers. It writes a new run's id to the state row's
  `run_requested` column (making the row as the first tick would if it
  doesn't exist yet), and waits for the record with that id. A request
  made while another stands joins it, so several requests before a tick
  make one run. The tick reads `run_requested` with each state row. A
  schedule's due run goes first, and a request runs on the same tick
  only when that left the lease free (the due tick was skipped as
  paused, or none was due) and no live lease holds it. So a tick runs a
  schedule once at most. It takes the lease in one conditional statement
  that also clears the request, so of two instances only one runs it.
  Its record's id is the one requested, its due time is the tick's, and
  what it emits is delivered after it.
- **The wait.** The command reads the record every 200 ms, timing the
  wait on its own clock, so the two machines' clocks never have to
  agree. A tick has to take the request within twice the period the app
  ticks at, which is the ticks-stopped alert's limit: 2 minutes for a
  minute's tick. While another run holds the lease it waits longer, up
  to the longest lease, 16 minutes, and that again, since it can't know
  the deployment's budget. Otherwise it withdraws the request, unless a
  tick took it meanwhile, and fails with `schedules.no_tick`: "No tick
  picked up `<name>`'s run in 2 min", its next step to check the app is
  running and run it again. Withdrawing keeps a request from running
  whenever the app next starts, perhaps much later. Once a tick takes
  the request, the command waits for the run to end. A run still going
  past the lease's length was lost with its worker
  (`schedules.run_failed`). A command interrupted while it waits leaves
  the request, which the next tick runs.
- **What it shows.** The run's outcome from its record, on the screens
  above: ``✓ The app's tick ran `<name>` in 12 ms, emitting 1 event``, or
  the failed run's block. A run that fails exits 1 with
  `schedules.run_failed`. `--json` carries `"requested": true`, and
  while the command waits, the live line says so. `history` lists a
  requested run once a tick has started it, as a run by hand (due `–`,
  `· by hand`), and `jadpo dev` prints it with `· by hand`. While a
  request waits, `jadpo why` adds `request  a run by hand, waiting for
  the app's next tick`, and `jadpo schedules --json` gives each
  schedule's `runRequested`.

### Not built yet

Three things aren't built yet: names for instances, so the lease holder
shows as a run rather than an instance; sending the metrics and alerts
to a monitoring tool; and the console's timeline of each schedule's runs
and ticks ([Debugging and review](17-debugging-and-review.md)).

## Limits and runtime faults
<!-- topic: limits-and-runtime-faults -->

| Limit | Value | When it is exceeded |
|---|---|---|
| Events emitted by one action | 1,000, and as many for each item's action of a batch | Defect: the action fails and rolls back |
| Size of one event | 64 KB | Defect: the action fails and rolls back |
| Events causing events, from one request | 16 deep | The delivery fails with `event_chain_limit`, stopping loops; paging uses batches, not chains |
| One run of a subscriber or schedule | The time limit of the route or integration it calls, at most 15 minutes and what is left of its invocation's budget ([Running background work](#running-background-work)) | Treated as a timeout: retried if nothing outside was called, otherwise `outcome_unknown` |
| Changing calls in one action | 8, on its longest path, package actions' calls included | Compile error |
| One batch of a batch run | 1,000 items or 4 MiB of file, whichever comes first | The batch ends and the next begins |

These limits may change once they have been measured on real runs.

## What is built
<!-- topic: jobs-and-events-built -->

| Feature | Status |
|---|---|
| Everything else on this page | Built. |
| Publishing chosen public events outward | Not built yet. |
| Each step's state in `jadpo watch`, and in `jadpo trace` with the request timelines ([Debugging and review](17-debugging-and-review.md)), and a record's entry in the [personal-data report](15-personal-data.md) | Not built yet. |
| Sending the metrics and the alerts' log events to a monitoring tool | Not built yet: waits for [telemetry](16-built-in-services.md). |
| The console's timeline | Not built yet ([Debugging and review](17-debugging-and-review.md)). |
| Jitter and `Retry-After` between a delivery's attempts | Not built yet: the next attempt waits only its backoff ([Retries and giving up](#retries-and-giving-up)). |
| Classifying a change to an event record as breaking, and accepting it | Not built yet: comes with [acceptances](18-records-and-manifest.md#acceptances). |
| Splitting a page type allowing more than 1,000 items into batches; `batches.status` and `batches.cancel` with runs in no store or several; a file's run with several databases and none written | Not built yet ([How a run is kept](#how-a-run-is-kept), [How a file's run is kept](#how-a-files-run-is-kept)). |
| `cli.signal_handler_failed` | Not built yet ([Running background work](#running-background-work)). |

[The roadmap](../roadmap.md) says when.

## Errors

- [`event.emit_invalid`](../errors/event.emit_invalid.md)
- [`event.settings_invalid`](../errors/event.settings_invalid.md)
- [`event.store_required`](../errors/event.store_required.md)
- [`receiving.invalid`](../errors/receiving.invalid.md)
- [`schedule.every_invalid`](../errors/schedule.every_invalid.md)
- [`schedule.item_invalid`](../errors/schedule.item_invalid.md)
- [`schedule.run_invalid`](../errors/schedule.run_invalid.md)
- [`schedule.settings_invalid`](../errors/schedule.settings_invalid.md)
- [`schedule.store_required`](../errors/schedule.store_required.md)
- [`step.context`](../errors/step.context.md)
- [`step.encryption_key_required`](../errors/step.encryption_key_required.md)
- [`step.large_output`](../errors/step.large_output.md)
- [`step.late_failure`](../errors/step.late_failure.md)
- [`step.settings_invalid`](../errors/step.settings_invalid.md)
- [`step.store_required`](../errors/step.store_required.md)
- [`step.too_many`](../errors/step.too_many.md)
- [`step.unkeyed`](../errors/step.unkeyed.md)
- [`subscriber.argument_invalid`](../errors/subscriber.argument_invalid.md)
- [`subscriber.event_invalid`](../errors/subscriber.event_invalid.md)
- [`subscriber.item_invalid`](../errors/subscriber.item_invalid.md)
- [`subscriber.retry_invalid`](../errors/subscriber.retry_invalid.md)
- [`subscriber.run_invalid`](../errors/subscriber.run_invalid.md)
- [`type.batch_capture_not_storable`](../errors/type.batch_capture_not_storable.md)
- [`type.batch_context`](../errors/type.batch_context.md)
- [`type.batch_each_invalid`](../errors/type.batch_each_invalid.md)
- [`type.batch_from_invalid`](../errors/type.batch_from_invalid.md)
- [`type.batch_input`](../errors/type.batch_input.md)
- [`type.batch_order_updatable`](../errors/type.batch_order_updatable.md)
- [`type.batch_row_invalid`](../errors/type.batch_row_invalid.md)
