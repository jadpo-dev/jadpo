# Run background work
<!-- topic: guide-run-background-work -->

Some work happens on no request: a reminder sent each day, or a
follow-on step after a change. Jadpo has three pieces for it. A
schedule runs an action on a timer. A batch works through many records
a page at a time. An event says something happened, and a subscriber
runs an action for each one, retried until it succeeds. This guide
reminds borrowers two days before a book is due back, using all three,
and tests the whole run.

You start from an app with a `loans` domain, holding a `Loan` entity
with a `due_at` and a `reminded_at`, a `Reminder` entity that records
each reminder, and the type a batch uses to page through loans.
[Store and change a record](02-store-and-change-a-record.md) shows how
to write entities.

```jadpo check project=guide-background-work file=app.jadpo hidden
application {
    jadpo: "0.1"
}
```

```jadpo check project=guide-background-work file=domains/loans/types/due_time.jadpo hidden
/// When a loan is due back.
type DueTime = Instant {}
```

```jadpo check project=guide-background-work file=domains/loans/types/loan_page.jadpo hidden
/// Where a batch has got to in a list of loans.
type LoanPage = Input {
    page_size: PageSize {
        default: 50
    }
    after: PageCursor {
        optional: true
    }
}
```

`domains/loans/entities/loan.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/entities/loan.jadpo
use loans/types { DueTime }

/// A book lent until a time.
entity Loan {
    id: Uuid
    title: Text {
        min_length: 1
        max_length: 200
    }
    borrower: Email
    due_at: DueTime
    reminded_at: Instant?

    policy: {
        create: [Access.system]
        read: [Access.system]
        update: [Access.system]
    }
}
```

`domains/loans/entities/reminder.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/entities/reminder.jadpo hidden
use loans/entities { Loan }

/// A reminder that a loan is due back soon.
entity Reminder {
    id: Uuid
    loan_id: Loan.id {
        on_delete: OnDelete.cascade
    }
    sent_at: Instant {
        generated: Generated.on_create
    }

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
```

Background work runs as the system, with no person signed in, so each
entity it reads or writes lets `Access.system` in. A real app lists
its people beside it.

## Find the loans a page at a time

The query a batch walks is an ordinary query giving a `Page`, in an
order no change to a loan moves, so a run meets each loan once:

`domains/loans/queries/loans_due.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/queries/loans_due.jadpo
use loans/entities { Loan }
use loans/types { DueTime, LoanPage }

/// Loans due by a time whose borrower hasn't been reminded.
query loans_due(by: DueTime, page: LoanPage) -> Page<Loan> {
    from: Loan
    where: Loan.due_at <= by
    where: Loan.reminded_at == none
    order: [Loan.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
```

## Say what happened: an event

An event is a type built on `Event`, in a domain's `events/` folder:

`domains/loans/events/loan_due_soon.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/events/loan_due_soon.jadpo
use loans/entities { Loan }

/// A loan is due back within two days.
type LoanDueSoon = Event {
    loan_id: Loan.id
}
```

The action the batch runs for each loan marks it, so no later run finds
it again, and emits the event:

`domains/loans/actions/flag_due_loan.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/actions/flag_due_loan.jadpo
use loans/entities { Loan }
use loans/events { LoanDueSoon }

/// Marks a loan as reminded, and says it is due soon.
action flag_due_loan(loan: Loan) -> Unit {
    var flagged: Loan = Loan.update(loan) {
        set: {
            reminded_at: clock.now
        }
    }
    emit LoanDueSoon {
        loan_id: flagged.id
    }
}
```

`emit` writes the event in the same transaction as the update. It is
delivered only once that commits, so a reminder is never sent for a
change that was rolled back.

## Start a batch

`batches.each` runs an action for each record a query gives, a page at a
time, each page in its own transaction. It carries on from the page it
had reached after a restart or a deploy, so however many loans fall due
at once, no run holds them all:

`domains/loans/actions/find_due_loans.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/actions/find_due_loans.jadpo
use loans/types { DueTime, LoanPage }
use loans/actions { flag_due_loan }
use loans/queries { loans_due }

/// Starts flagging every loan due within two days.
action find_due_loans() -> BatchRun {
    return batches.each(
        from: loans_due(DueTime(clock.now + 2d), LoanPage {}),
        each: loan -> flag_due_loan(loan)
    )
}
```

The batch starts once this action commits, and `BatchRun` names it, for
`batches.status` to ask about later.

## Run it on a timer: a schedule

`domains/loans/schedules/due_reminders.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/schedules/due_reminders.jadpo
use loans/actions { find_due_loans }

/// Looks for loans due soon every hour.
schedule due_reminders {
    every: 1h
    run: find_due_loans()
}
```

Runs fall on a fixed grid in UTC that a restart doesn't move. Only one
runs at a time, ticks missed while the app was down become one run, and
a run that fails is tried again at the next due time. The action is
passed nothing, and `clock.now` stays the same for the whole run.

## React to the event: a subscriber

`domains/loans/actions/send_reminder.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/actions/send_reminder.jadpo
use loans/entities { Loan, LoanNotFound, Reminder }

/// Records the reminder for a loan due soon.
action send_reminder(loan_id: Loan.id) fails LoanNotFound -> Reminder {
    var loan: Loan = attempt Loan.get(loan_id)
    return Reminder.create {
        set: {
            loan_id: loan.id
        }
    }
}
```

`domains/loans/subscribers/remind_borrower.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/subscribers/remind_borrower.jadpo
use loans/actions { send_reminder }
use loans/events { LoanDueSoon }

/// Reminds the borrower of each loan due soon.
subscriber remind_borrower {
    on: LoanDueSoon
    run: send_reminder(event.loan_id)
}
```

`event` is the event being delivered. A delivery that fails is retried
with a growing wait between attempts, so `send_reminder` may run more
than once for one event: write it so that running it twice does no
harm. Each event is also delivered at least once, never lost. A real
app sends an email here; an email is an event of its own, sent the same
way after the action commits.

Run `jadpo migrate new initial` and `jadpo records update`. The second
writes `domains/loans/events/events.jadpo_record`, the committed record of the
domain's events, so a change to one shows in review:

```text
// Generated by Jadpo from the events of domain loans; update it with `jadpo records update`, never by hand.
record events 1

event LoanDueSoon
    loan_id: Uuid
```

## Test the whole run

A schedule test runs the schedule with `due_reminders.run()`, as the
timer would. The batch it starts runs to its end, and every event it
emits is delivered, before the test's next line, so the test sees
everything the run did. `clock:` fixes the time, and `clock.advance`
moves it on:

`domains/loans/fixtures/loan_due.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/fixtures/loan_due.jadpo
use loans/entities { Loan }

/// A loan of the book given, due at the time given.
fixture loan_due(title: Loan.title, due_at: Loan.due_at) -> Loan {
    return Loan.create {
        set: {
            title: title
            borrower: "cara@example.com"
            due_at: due_at
            reminded_at: none
        }
    }
}
```

`domains/loans/queries/reminders_for.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/queries/reminders_for.jadpo
use loans/entities { Loan, Reminder }

/// The reminders sent for a loan.
query reminders_for(loan_id: Loan.id) -> List<Reminder> {
    from: Reminder
    where: Reminder.loan_id == loan_id
    order: [Reminder.sent_at asc]
    limit: 10
}
```

`domains/loans/schedules/due_reminders.test.jadpo`:

```jadpo check project=guide-background-work file=domains/loans/schedules/due_reminders.test.jadpo
use loans/entities { Loan, Reminder }
use loans/queries { reminders_for }
use loans/fixtures { loan_due }

test "a borrower is reminded once, two days before the book is due" {
    clock: "2099-11-02T10:00:00Z"
    var loan: Loan = loan_due("Dune", "2099-11-05T10:00:00Z")
    due_reminders.run()
    var none_yet: List<Reminder> = reminders_for(loan.id)
    assert list.count(none_yet) == 0
    clock.advance(1d)
    due_reminders.run()
    var once: List<Reminder> = reminders_for(loan.id)
    assert list.count(once) == 1
    clock.advance(1h)
    due_reminders.run()
    var still_once: List<Reminder> = reminders_for(loan.id)
    assert list.count(still_once) == 1
}

test "a loan due later isn't reminded yet" {
    clock: "2099-11-02T10:00:00Z"
    var later: Loan = loan_due("Emma", "2099-11-20T10:00:00Z")
    due_reminders.run()
    var reminders: List<Reminder> = reminders_for(later.id)
    assert list.count(reminders) == 0
}
```

```sh
jadpo test
```

```text
tested: 2 passed, 2 schedule tests
```

`jadpo test --kind schedule` runs only these.

## Watch it run

`jadpo dev` ticks once a minute. In a second terminal in the project,
`jadpo schedules` reads its database:

```sh
jadpo schedules
```

```text
  schedule       last run (UTC)  next
  due_reminders  - none yet      -

schedules: 1 schedule, no tick yet
```

`jadpo schedules run due_reminders` runs it now, `pause` and `resume`
stop and start it, `history` lists its runs, and `jadpo why
due_reminders` says why it didn't run when you expected.

A deployed app runs its schedules and delivers its events itself: a
server program on its own timer, and a Worker on Cloudflare's
once-a-minute trigger. An app that declares several deployables says
which runs them with `runs: [Trigger.schedules, Trigger.subscribers]`;
see [Deploy to Cloudflare Workers](11-deploy-to-workers.md).

## Read more

[Schedules](../language/12-jobs-and-events.md#schedules),
[Batch work keeps its progress](../language/12-jobs-and-events.md#batch-work-keeps-its-progress),
[Batches: work over many items](../language/12-jobs-and-events.md#batches-work-over-many-items),
[Declaring an event](../language/12-jobs-and-events.md#declaring-an-event),
[Emitting an event](../language/12-jobs-and-events.md#emitting-an-event),
[Subscribers](../language/12-jobs-and-events.md#subscribers),
[Delivery: at least once, safe to repeat](../language/12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat),
[Retries and giving up](../language/12-jobs-and-events.md#retries-and-giving-up),
[What an operator sees](../language/12-jobs-and-events.md#what-an-operator-sees) and
[Background work](../language/09-policy.md#background-work).
