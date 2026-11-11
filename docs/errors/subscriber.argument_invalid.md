# `subscriber.argument_invalid`

A subscriber's `run:` passes something other than its event or a field of it.

## Why Jadpo refuses it

A subscriber is delivered one event and runs one action with it. Inside
`run:`, `event` is that event, and all the subscriber may pass is
`event` itself or fields read through it, such as `event.reminder_id`.
Anything more, a type, a computed value or a call, would be logic
living in the subscriber, where it isn't tested or reviewed as an
action's is. So the action works out everything else itself, and the
subscriber only says which event feeds which action.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/subscriber.argument_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the action needs a value the event carries**, pass that field of
`event`. In `main.jadpo`:

```jadpo fragment
subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
}
```

```jadpo check project=e-subscriber-argument-fix-1 hidden
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

Each `ReminderDue` now marks its own reminder sent. Anything else the
action needs, such as the reminder's note, it reads from the record
itself, as it stands when the delivery runs.

## Related

- [What a subscriber holds](../language/12-jobs-and-events.md#what-a-subscriber-holds)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)
- [`subscriber.run_invalid`](../errors/subscriber.run_invalid.md)
