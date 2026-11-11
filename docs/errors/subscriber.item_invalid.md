# `subscriber.item_invalid`

A subscriber holds an item it doesn't take, gives one twice or leaves out `on:` or `run:`.

## Why Jadpo refuses it

A subscriber holds three things: the one event it listens for, `on:`,
the action it runs, `run:`, and, if its retries differ from the
default, `retry: { attempts: within: }`. Everything else about a
delivery is the same for every subscriber, so Jadpo refuses an item it
would ignore, such as `timeout:`, rather than let it look like it
changes something. A subscriber without `on:` or `run:` has nothing to
react to or nothing to do.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/subscriber.item_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the item was meant to limit how long a delivery takes**, remove it.
In `main.jadpo`:

```jadpo fragment
subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
}
```

```jadpo check project=e-subscriber-item-fix-1 hidden
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

A delivery stopped by the run's time limit counts as an attempt and is
tried again while its budget lasts, as every subscriber's is.

**If it was meant to change how long the subscriber keeps trying**, say
so with `retry:`. In `main.jadpo`:

```jadpo fragment
subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
    retry: {
        attempts: 5
        within: 1h
    }
}
```

```jadpo check project=e-subscriber-item-fix-2 hidden
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
        attempts: 5
        within: 1h
    }
}
```

A `ReminderDue` whose delivery keeps failing is tried at most 5 times,
and fails once a further try would fall more than an hour after it was
emitted, instead of the default 10 tries within 24 hours.

## Related

- [What a subscriber holds](../language/12-jobs-and-events.md#what-a-subscriber-holds)
- [Changing the budget: `retry:`](../language/12-jobs-and-events.md#changing-the-budget-retry)
- [A run that takes too long](../language/12-jobs-and-events.md#a-run-that-takes-too-long)
