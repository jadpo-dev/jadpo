---
title: Jobs and events
area: 12
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - syntax.job_*
  - type.job_delivery_authority_invalid
  - type.job_delivery_binding_not_implemented
  - type.job_delivery_hook_invalid
  - type.job_interval_invalid
  - type.job_run_argument_invalid
  - type.job_run_signature_invalid
---

# Jobs and events

Work that does not happen inside a request: schedules that run an action on a
timer, events an action announces, subscribers that react to them, and what
happens when that work fails.

> Decided on 2026-10-13. Code outside the collapsed boxes applies the decisions from areas 1 to 12. Today the compiler checks `job` declarations but cannot run them, and events and subscribers do not parse at all.

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
  backlog.
- Inside the run, `clock.now` is fixed: every read gives the time the run
  started, so the action does not need the time passed in.
- A failed run is logged and shown to the operator; the next tick is the
  retry. Schedules have no retry settings.
- Entities with `purge:` in their lifecycle (area 5) get a built-in hourly
  purge schedule from the compiler; nothing is written for it.

::: today

Today's form puts the interval in the header and requires two settings that
each accept only one value. The action must take exactly one parameter of a
named `Instant` type, built from `clock.now` in the call. This checks, but
`jadpo build` refuses it with `toolchain.target_job_not_implemented`, so no
job ever runs.

```jadpo project=today-job
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}
```

:::

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

::: today

No event can be declared: `event` is not a declaration keyword
(`syntax.expected_declaration`, compiled). The unbuilt candidate design
groups events into families with per-variant policy:

```jadpo fragment
event TodoEvent {
    completed {
        todo_id: Todo.id
        completed_at: Instant
    }
    reopened {
        todo_id: Todo.id
    }
}
```

:::

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

::: today

Nothing parses. The candidate design writes
`attempt emit_event(TodoEvent.completed { todo_id: input.id })`, a lowercase
intrinsic with no owner and an `attempt` on something that cannot fail.

:::

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
- **Outside calls can repeat.** An integration call cannot join the
  transaction. Each delivery has a stable key (event and subscriber), which
  integration operations send as their idempotency key (area 11), so a
  provider that supports keys ignores the repeat.
- **One outside change per action.** An action calls at most one integration
  operation that changes something (question 11). After a call whose result
  is lost, the runtime cannot know what happened; with one call per action it
  knows exactly which call to ask about.
- **No order is promised** between events (question 6). A subscriber that
  cares about order reads the current state instead of trusting the event's
  copy, which is also what makes repeats safe.

## Where events travel

Events stay inside the Jadpo runtime (Richard, 2026-10-13). They are stored
in the main store's outbox in the same transaction as the action, and the
runtime delivers them: on a long-running server from its own workers, on
Cloudflare Workers right after the commit (`waitUntil`) with the
once-a-minute schedule tick sweeping anything left. One place holds every
log and trace, a request is never slowed by delivery, and no site needs a
queue service.

External queues are never the delivery path. A connection can *publish*
chosen public events outward, so other systems can react to them: an outbound
bridge fed from the outbox after commit, at least once, with the trace
context attached (Cloudflare Queues, SQS, Pub/Sub, a webhook).

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

Each failed or unknown item keeps the event, the subscriber, every attempt
with its failure code and internal fields (area 4's logging rules), and the
request that emitted it. A `failed` or `outcome_unknown` item writes a log
event and counts towards an alert. The screens and commands are area 14's.

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
| Events causing events, from one request | 16 deep | The delivery fails with `event_chain_limit`, stopping loops |
| One run of a subscriber or schedule | The time limit of the route or integration it calls, at most 15 minutes | Treated as a timeout: retried if nothing outside was called, otherwise `outcome_unknown` |

Numbers are recommendations to confirm with measured runs (question 12).

## The delivery descriptor

Today's grammar has a 45-line closed sub-language inside a job,
`delivery: reminder_v1 { selection: ... hooks: ... service: ... authority: ... completion: ... }`,
written for exactly one feature: overdue-todo reminders. It restates a query
(`order_by: Todo.due_at asc, Todo.id asc`, `limit: 500`), accepts only
literal values such as `limit: 500` and `"reminder.v1"`, and can never pass
checking: every descriptor fails with
`type.job_delivery_binding_not_implemented`. The batch pattern above does the
same job with an ordinary query, an ordinary action and an ordinary
subscriber. Recommendation: delete it, with its 14 syntax rules and 6 type
rules (question 2).

::: today

The full descriptor and its errors are under
[`type.job_delivery_binding_not_implemented`](#type-job-delivery-binding-not-implemented).
An excerpt:

```jadpo fragment
delivery: reminder_v1 {
    selection: {
        name: reminder_candidates
        entity: Todo
        before: operation_time
        order_by: Todo.due_at asc, Todo.id asc
        limit: 500
        ...
    }
    authority: {
        validator: api_bearer.service_key
        permit: reminder_only
        ...
    }
}
```

:::

## Errors

> The examples in this section use today's syntax, because they show the compiler's real output; they are rewritten or removed as the rework lands.

Every error this area can produce, with an example that triggers it and the
compiler's actual output.

### `syntax.job_every_required`

```jadpo project=e-every expect=syntax.job_every_required
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders after 15m {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}
```

### `syntax.job_duration_required`

```jadpo project=e-duration expect=syntax.job_duration_required
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15 {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}
```

### `type.job_interval_invalid`

`1d` is rejected today, as is zero:

```jadpo project=e-interval expect=type.job_interval_invalid
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 1d {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}
```

### `syntax.job_clause_required`

```jadpo project=e-clause-required expect=syntax.job_clause_required
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
}
```

### `syntax.job_clause_value_invalid`

```jadpo project=e-clause-value expect=syntax.job_clause_value_invalid
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: parallel
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}
```

### `syntax.job_item_duplicate`

```jadpo project=e-item-duplicate expect=syntax.job_item_duplicate
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
    retry: next_schedule
}
```

### `syntax.job_item_unknown`

```jadpo project=e-item-unknown expect=syntax.job_item_unknown
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
    timeout: 5m
}
```

### `syntax.job_run_invocation_required`

```jadpo project=e-run-invocation expect=syntax.job_run_invocation_required
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: scan
    retry: next_schedule
}
```

### `syntax.job_export_invalid`

```jadpo project=e-export expect=syntax.job_export_invalid
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

public job overdue_reminders every 15m {
    concurrency: singleton
    run: scan(JobRunAt(clock.now))
    retry: next_schedule
}
```

### `type.job_run_signature_invalid`

An action with no parameters, the form question 1 proposes, is rejected
today:

```jadpo project=e-run-signature expect=type.job_run_signature_invalid
action scan() -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: scan()
    retry: next_schedule
}
```

### `type.job_run_argument_invalid`

```jadpo project=e-run-argument expect=type.job_run_argument_invalid
type JobRunAt = Instant {}

action scan(at: JobRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: scan(clock.now)
    retry: next_schedule
}
```

### `type.job_delivery_binding_not_implemented`

Every complete descriptor fails, whatever it says:

```jadpo project=e-delivery-binding expect=type.job_delivery_binding_not_implemented
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        selection: {
            name: reminder_candidates
            entity: Todo
            identity: Todo.id
            due: Todo.due_at
            before: operation_time
            open: Todo.status(TodoStatus.open)
            visible: Todo
            unsent: Todo.reminder_sent_at
            required_owner: Todo.owner
            owner_visible: User
            order_by: Todo.due_at asc, Todo.id asc
            limit: 500
            continuation: due_identity
        }
        hooks: {
            create: Todo.create_todo
            patch: Todo.patch_todo
            supplied: PatchTodo.due_at
        }
        service: {
            operation: ReminderMail.send_overdue_reminder
            intent: ReminderIntentId
            input: ReminderMessage
            output: ReminderReceipt
            payload_version: "reminder.v1"
            payload: {
                idempotency_key: generated_intent
                from: config.mail_sender
                to: Todo.owner.email
                todo_title: Todo.title
                due_at: Todo.due_at
            }
        }
        authority: {
            validator: api_bearer.service_key
            membership: ReminderServiceMembership
            role: ReminderRole.sender
            permit: reminder_only
        }
        completion: {
            name: reminder_sent
            field: Todo.reminder_sent_at
            time: compiler_receipt_observation
        }
    }
}
```

### `type.job_delivery_hook_invalid`

The same descriptor on its own, without the entities and actions it names,
also fails with "Delivery hooks or competing writes cannot be proved" on its
`create:` and `patch:` lines. The example is the one above, compiled
separately and not repeated, since question 2 removes the rule.

```jadpo project=e-delivery-hook expect=type.job_delivery_hook_invalid hidden
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        selection: {
            name: reminder_candidates
            entity: Todo
            identity: Todo.id
            due: Todo.due_at
            before: operation_time
            open: Todo.status(TodoStatus.open)
            visible: Todo
            unsent: Todo.reminder_sent_at
            required_owner: Todo.owner
            owner_visible: User
            order_by: Todo.due_at asc, Todo.id asc
            limit: 500
            continuation: due_identity
        }
        hooks: {
            create: Todo.create_todo
            patch: Todo.patch_todo
            supplied: PatchTodo.due_at
        }
        service: {
            operation: ReminderMail.send_overdue_reminder
            intent: ReminderIntentId
            input: ReminderMessage
            output: ReminderReceipt
            payload_version: "reminder.v1"
            payload: {
                idempotency_key: generated_intent
                from: config.mail_sender
                to: Todo.owner.email
                todo_title: Todo.title
                due_at: Todo.due_at
            }
        }
        authority: {
            validator: api_bearer.service_key
            membership: ReminderServiceMembership
            role: ReminderRole.sender
            permit: reminder_only
        }
        completion: {
            name: reminder_sent
            field: Todo.reminder_sent_at
            time: compiler_receipt_observation
        }
    }
}
```

### `type.job_delivery_authority_invalid`

The same descriptor also fails with "Delivery authority does not compose to
its selected service identity" on its `authority:` section (compiled
separately, not repeated).

```jadpo project=e-delivery-authority expect=type.job_delivery_authority_invalid hidden
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        selection: {
            name: reminder_candidates
            entity: Todo
            identity: Todo.id
            due: Todo.due_at
            before: operation_time
            open: Todo.status(TodoStatus.open)
            visible: Todo
            unsent: Todo.reminder_sent_at
            required_owner: Todo.owner
            owner_visible: User
            order_by: Todo.due_at asc, Todo.id asc
            limit: 500
            continuation: due_identity
        }
        hooks: {
            create: Todo.create_todo
            patch: Todo.patch_todo
            supplied: PatchTodo.due_at
        }
        service: {
            operation: ReminderMail.send_overdue_reminder
            intent: ReminderIntentId
            input: ReminderMessage
            output: ReminderReceipt
            payload_version: "reminder.v1"
            payload: {
                idempotency_key: generated_intent
                from: config.mail_sender
                to: Todo.owner.email
                todo_title: Todo.title
                due_at: Todo.due_at
            }
        }
        authority: {
            validator: api_bearer.service_key
            membership: ReminderServiceMembership
            role: ReminderRole.sender
            permit: reminder_only
        }
        completion: {
            name: reminder_sent
            field: Todo.reminder_sent_at
            time: compiler_receipt_observation
        }
    }
}
```

### `syntax.job_delivery_mode_invalid`

The examples below are cut down to the part that is wrong; each also reports
the sections it leaves out.

```jadpo project=e-delivery-mode expect=syntax.job_delivery_mode_invalid
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: invoice_v1 {
    }
}
```

### `syntax.job_delivery_key_required`

```jadpo project=e-delivery-required expect=syntax.job_delivery_key_required
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
    }
}
```

### `syntax.job_delivery_key_unknown`

```jadpo project=e-delivery-unknown expect=syntax.job_delivery_key_unknown
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        schedule: {
        }
    }
}
```

### `syntax.job_delivery_key_duplicate`

```jadpo project=e-delivery-duplicate expect=syntax.job_delivery_key_duplicate
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        completion: {
            name: reminder_sent
            field: Todo.reminder_sent_at
            time: compiler_receipt_observation
        }
        completion: {
            name: reminder_sent
            field: Todo.reminder_sent_at
            time: compiler_receipt_observation
        }
    }
}
```

### `syntax.job_delivery_term_invalid`

A few slots accept only a fixed compiler word:

```jadpo project=e-delivery-term expect=syntax.job_delivery_term_invalid
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        completion: {
            name: reminder_sent
            field: Todo.reminder_sent_at
            time: receipt.accepted_at
        }
    }
}
```

### `syntax.job_delivery_reference_invalid`

```jadpo project=e-delivery-reference expect=syntax.job_delivery_reference_invalid
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        completion: {
            name: reminder_sent
            field: generated_intent
            time: compiler_receipt_observation
        }
    }
}
```

### `syntax.job_delivery_limit_invalid`

The limit can only be 500:

```jadpo project=e-delivery-limit expect=syntax.job_delivery_limit_invalid
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        selection: {
            limit: 100
        }
    }
}
```

### `syntax.job_delivery_direction_invalid`

```jadpo project=e-delivery-direction expect=syntax.job_delivery_direction_invalid
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        selection: {
            order_by: Todo.due_at desc, Todo.id asc
        }
    }
}
```

### `syntax.job_delivery_syntax_invalid`

The ordering must name exactly two fields:

```jadpo project=e-delivery-syntax expect=syntax.job_delivery_syntax_invalid
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        selection: {
            order_by: Todo.due_at asc
        }
    }
}
```

### `syntax.job_delivery_version_invalid`

The payload version can only be `"reminder.v1"`:

```jadpo project=e-delivery-version expect=syntax.job_delivery_version_invalid
type ReminderRunAt = Instant {}

action reminder_tick(at: ReminderRunAt) -> Unit {}

job overdue_reminders every 15m {
    concurrency: singleton
    run: reminder_tick(ReminderRunAt(clock.now))
    retry: next_schedule
    delivery: reminder_v1 {
        service: {
            payload_version: "reminder.v2"
        }
    }
}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check` or `jadpo
build`, or read from source and the design documents.

| Feature | Status |
|---|---|
| `job` declarations | Check works: the interval, all three clauses, their single allowed values and the action's signature are enforced (compiled). **Gap:** `jadpo build` refuses every job with `toolchain.target_job_not_implemented`; there is no scheduler, worker, outbox or retry at runtime (compiled). |
| Scheduled action shape | The action must take exactly one parameter of a named `Instant` type and return `Unit`; a parameterless action and `scan(clock.now)` are rejected (compiled). `1d` is rejected; intervals stop at hours (compiled). |
| Events and subscribers | **Not built.** `event` and `subscriber` are not declaration keywords (`syntax.expected_declaration`, compiled); `type X = Event { }` reports `Event` as unknown (compiled). The design is a candidate in the event model. |
| Delivery descriptor | Parses with every key checked, then always fails with `type.job_delivery_binding_not_implemented` (compiled). `type.job_delivery_selector_invalid`, `type.job_delivery_service_invalid` and `type.job_delivery_sealed_use` are raised only inside the compiler's own tests: `jadpo check` on the golden migration plus the reminder fixture shows only the binding error (compiled). |
| Golden todo reminders | **Design problem (read from source):** `send_due_reminders` in `examples/golden-todo/app.jadpo` calls the mail provider for up to 500 todos inside one all-or-nothing action, so a failure part-way cannot undo the emails already sent. The batch pattern on this page replaces it. The golden migration has no job at all. |
| Durable semantics | ASYNC-001 (frozen 2026-10-04) defines states, leases, fencing, budgets and dead letters; none is implemented (read from the plan and build refusal). |

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
    the same reason. *Decided 2026-10-13:* yes, as recommended.
12. **Are the limits right?** 1,000 events per action, 64 KB per event,
    chains 16 deep, 15 minutes per run. *Recommendation:* adopt them as
    defaults and confirm with measured runs, as ASYNC-001 requires. *Decided 2026-10-13:* yes, as recommended.
13. **Do large exports need a consistent snapshot read?** (carried from area
    6) A schedule paging through a query for an export or report sees
    records change between pages. Should it read one consistent snapshot,
    and how is that declared? *Decided 2026-10-13:* yes, as recommended.
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
| One changing integration call per action | Yes, a compile error otherwise | 2026-10-13 |
| Limits | 1,000 events per action, 64 KB per event, chains 16 deep, 15 minutes per run, confirmed by measurement | 2026-10-13 |
| Snapshot reads for exports | None; keyset pages; area 15's export reads in one transaction | 2026-10-13 |
| Work after an accepted response | An event and subscriber; the client reads a record with a status field | 2026-10-13 |
| Schedules and subscribers in tests | `schedule.run()`, delivery after commit, fakes fail subscribers, shuffled order with a printed seed | 2026-10-13 |
| Schedule ticks on every host | Schedule adapters, many and chosen in `app.jadpo`: an adapter supplies the tick and may supply the lease (default: main database); a schedule or event with no store for its state is a compile error; system tables (`jadpo_` prefix) are in migration snapshots (Richard) | 2026-10-13 |
| Tracing through events | Always on: events carry the emitting action's trace context; subscriber runs continue it; `jadpo trace <request id>` | 2026-10-13 |
| Schedule monitoring | Every run and tick recorded; `jadpo schedules` (list, run, pause, resume, history); `jadpo why <schedule>`; metrics; alerts with no setup (no success in twice the interval, ticks stopped, near the time limit, three failures, stuck lease); console timeline (Richard) | 2026-10-13 |
| Circuit breaker | Per connection: after repeated `unavailable` failures, deliveries calling the provider are held, one probe after a pause, success releases the rest; held deliveries keep their retry budget; shown and alerted | 2026-10-13 |
| Event transport | Internal only: the main store's outbox delivered by the runtime (on Workers after commit via `waitUntil`, swept by the minute tick); external queues only receive published copies of chosen public events (Richard) | 2026-10-13 |
