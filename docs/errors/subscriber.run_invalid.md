# `subscriber.run_invalid`

A subscriber's `run:` isn't a call of an action in the subscriber's own domain.

## Why Jadpo refuses it

A subscriber's `run:` is one call: an action of the subscriber's own
domain, passed the event or fields of it, so all the logic lives in the
action, where it is tested and reviewed like any other. A bare name
doesn't say what the action is given. A function isn't something a
delivery can run, since a delivery is an action's run, its writes and
events committed together. And a subscriber lives in the domain that
reacts, so another domain's action is reached through an action of the
subscriber's own domain that calls it.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/subscriber.run_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If `run:` names the action without calling it**, call it, passing the
field of the event it takes. In `main.jadpo`:

```jadpo fragment
subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
}
```

```jadpo check project=e-subscriber-run-fix-1 hidden
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

Each `ReminderDue` now runs `send_reminder` for its own reminder, as the
system, once its emitting action has committed.

## Related

- [What a subscriber holds](../language/12-jobs-and-events.md#what-a-subscriber-holds)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)
- [`subscriber.argument_invalid`](../errors/subscriber.argument_invalid.md)
