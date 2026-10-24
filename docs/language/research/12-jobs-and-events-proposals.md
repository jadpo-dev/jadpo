# Jobs and events: keeping durable work simple

**Status:** proposals for [area 12](../12-jobs-and-events.md), 2026-10-12.
Decides nothing.

## What is complicated today

Two designs exist, neither built:

- **ASYNC-001** (async plan), frozen 2026-10-04: seven
  delivery states, leases with fencing generations, per-key FIFO ordering,
  cumulative attempt and lifetime budgets, coalesced schedules and bounded
  continuations. Sound, and mostly invisible to authors.
- **The event model** (event model), a candidate:
  `event` families with variants, `emit_event(...)`, subscribers with several
  `on` handlers, `component` ownership maps, `triggers:` restrictions on
  entities and services, per-variant `invoke`/`read` policy, delivery that
  checks both a worker's and the original user's permissions, events published
  automatically by lifecycle transitions, enrollment generations, ordering
  lanes, two allowed handler "shapes", and a coordination protocol. It runs to
  830 lines and 25 acceptance cases before a line is built.

On top sits the job **delivery descriptor**: 45 lines of closed syntax for one
feature (overdue reminders) that can never pass checking.

The authoring surface an agent must learn is the problem. Most of the
guarantees can stay; most of the concepts can go.

## What others do

| Framework | Unit of work | Commits with data? | Retries by default | After giving up |
|---|---|---|---|---|
| Rails Active Job + Solid Queue | Job class; recurring tasks in `config/recurring.yml` (`schedule: every day at 9am`) | Optional since Rails 8 (`enqueue_after_transaction_commit`) | Only what `retry_on` declares | Kept as failed executions; retry or discard from Mission Control |
| Laravel queues and events | Job or queued listener class | Yes with `after_commit` | `$tries` per job | `failed_jobs` table, `queue:retry` |
| Sidekiq | Worker class | No (Redis) | 25 retries over about 20 days, exponential | Dead set, kept 6 months |
| Oban (Elixir) | Worker module | Yes: insert inside an `Ecto.Multi` | 20 attempts | `discarded` state |
| Temporal | Workflow code plus activities | Separate service | Activities retry until a policy stops them | Workflow history; operator tooling |
| Inngest | Function triggered by an event, made of `step.run` calls | Separate service | Each step retried on its own; finished steps never rerun | Failed runs listed, replayable |
| AWS EventBridge | Rule routing an event to targets | n/a | 185 attempts within 24 hours | Dropped unless a dead-letter queue is set |
| Transactional outbox | Message row in the same transaction | Yes, by definition | Relay retries | Consumers must be idempotent, since the relay can repeat |

What they agree on:

1. **Delivery is at least once, and handlers must tolerate repeats.** Nobody
   offers exactly-once delivery to the outside world; the outbox write-up
   says so directly.
2. **Work is enqueued with the data or after commit,** never before. Laravel,
   Oban and Rails 8 all converged on this.
3. **One unit of work per job.** Sidekiq's and Inngest's advice is the same:
   small handlers, each retried alone.
4. **Failed work is kept and shown,** with retry and discard by hand.
5. **Ordering is rarely promised.** EventBridge, Sidekiq and Laravel promise
   none; Temporal gets order by running one workflow's steps in sequence.
6. **Authorisation is not re-checked per job.** A job runs as the
   application; it carries the ids it needs.

## Option A: queued action calls, no events

An action queues another action to run after commit, as Laravel and Rails do.

```jadpo fragment
action complete_todo(todo_id: Todo.id) fails TodoNotFound -> Todo {
    var todo: Todo = attempt mark_complete(todo_id)
    later notifications.send_completion_email(todo.id)
    return todo
}
```

- One new statement; no event types, no subscribers.
- The caller must know every reaction, so `planning` names `notifications`.
  Area 1 decided reactions in other domains use events so the emitter does
  not depend on who listens.
- Adding a reaction edits the emitting action; that is the coupling events
  exist to remove.

## Option B: events and subscribers over an outbox (recommended)

The page's design: one type per event, `emit` after commit, one subscriber per
event calling one action, retries by failure kind, coordination state in
ordinary entities.

```jadpo fragment
// domains/planning/events/todo_completed.jadpo
public type TodoCompleted = Event {
    todo_id: Todo.id
    completed_at: Instant
}

// domains/planning/actions/complete_todo.jadpo
emit TodoCompleted {
    todo_id: todo.id
    completed_at: clock.now
}

// domains/notifications/subscribers/email_on_completion.jadpo
subscriber email_on_completion {
    on: TodoCompleted
    run: send_completion_email(event)
}
```

What stays from ASYNC-001, generated rather than written: the outbox row in
the action's transaction, leases with fencing, the state list, backoff with
jitter, finite budgets, `outcome_unknown` never retried blindly, kept failure
records.

What goes from the event model, with the replacement:

| Today's candidate | Replaced by |
|---|---|
| `event TodoEvent { completed { } reopened { } }` | One `type X = Event` per fact |
| `emit_event(...)` with `attempt` | `emit X { }` |
| Several `on` handlers in a subscriber | One subscriber per event |
| `component`, `triggers:` | Domains own their code; only a domain's actions write its entities |
| Per-variant `invoke`/`read` policy; worker-and-origin authority | Subscribers run as the application within their domain; area 9 owns policy |
| Automatic publication from lifecycle transitions | `emit` in the action that makes the change |
| Ordering lanes and enrollment generations | No ordering promise; subscribers read current state |
| Two allowed handler shapes, inferred | One changing integration call per action, a compile error otherwise |
| Coordination protocol | An entity holding the process state, subscribers per step, a schedule for deadlines |

Costs: an author must split batch work into one event per item, and must not
trust event fields to be current. Both are the standard advice for Sidekiq
and Oban, and both are visible in review.

## Option C: durable workflow functions

A workflow is code that pauses between steps, as in Temporal and Inngest:

```jadpo fragment
workflow fulfil_order(order_id: Order.id) {
    var payment: Payment = step charge_card(order_id)
    wait for StockReserved { order_id: order_id } within 2d
    step dispatch(order_id)
}
```

- Long processes read top to bottom, which is clearer than state spread
  over subscribers.
- The runtime must replay the code from a recorded history, so the code must
  be deterministic (Temporal's rule), and changing a workflow while instances
  are paused needs versioning. It is a second programming model beside
  actions, and area 1 decided durable workflows are subscribers.
- Its state is hidden in a history, not queryable in the database, so an
  admin screen or a report cannot ask "which orders are waiting for stock?".

## Recommendation

**Option B.** It keeps one way to write logic (actions), adds two small
declarations (`type X = Event`, `subscriber`) and one statement (`emit`), and
keeps ASYNC-001's safety as generated behaviour. Option A loses the
decoupling area 1 chose; option C adds a second programming model for a need
the golden applications do not have yet. If an order-and-payment application
later shows entity-based coordination is too hard to read, option C can be
added for that case alone.

## Sources

- Solid Queue README, recurring tasks, concurrency and failed jobs: <https://github.com/rails/solid_queue>
- Laravel queues, `after_commit`, `ShouldBeUnique`, `WithoutOverlapping`: <https://laravel.com/docs/12.x/queues>
- Sidekiq error handling, retries and the dead set: <https://github.com/sidekiq/sidekiq/wiki/Error-Handling>
- Oban, inserting jobs inside an `Ecto.Multi`: <https://oban.hexdocs.pm/Oban.html>
- Temporal workflows, determinism and replay: <https://docs.temporal.io/workflows>
- Inngest multi-step functions: <https://www.inngest.com/docs/guides/multi-step-functions>
- EventBridge retry policy: <https://docs.aws.amazon.com/eventbridge/latest/userguide/eb-rule-retry-policy.html>
- Transactional outbox: <https://microservices.io/patterns/data/transactional-outbox.html>
