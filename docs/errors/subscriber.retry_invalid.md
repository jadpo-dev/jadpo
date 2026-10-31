# `subscriber.retry_invalid`

A subscriber's `retry:` holds something other than 1 to 100 tries and up to 30 days to keep trying.

## Why Jadpo refuses it

`retry:` sets how many times a delivery is tried, `attempts:` from 1 to
100, and how long after the event was emitted it may keep trying,
`within:` up to 30 days. A value outside those limits can't be kept as
written, and Jadpo won't cut it down to the nearest limit for you: the
budget decides how long a failing delivery keeps trying before it fails
and someone has to look at it.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/subscriber.retry_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the subscriber must keep trying for longer than the default**, give
it a budget within the limits. In `main.jadpo`:

```jadpo fragment
    retry: {
        attempts: 20
        within: 3d
    }
```

```jadpo check project=e-subscriber-retry-fix-1 hidden
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
        attempts: 20
        within: 3d
    }
}
```

Each delivery of `ReminderDue` to `send_due` is now tried up to 20 times
within 3 days of the event being emitted, then fails.

**If the default is enough**, delete `retry:`. The subscriber in
`main.jadpo` becomes:

```jadpo fragment
subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
}
```

```jadpo check project=e-subscriber-retry-fix-2 hidden
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
}
```

Each delivery gets 10 attempts within 24 hours, then fails.

## Related

- [Changing the budget: `retry:`](../language/12-jobs-and-events.md#changing-the-budget-retry)
- [How many tries: the default](../language/12-jobs-and-events.md#how-many-tries-the-default)
- [Retries and giving up](../language/12-jobs-and-events.md#retries-and-giving-up)
