# `subscriber.argument_invalid`

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
  help: docs/errors/subscriber.argument_invalid.md

check failed: 1 error, 1 file
```
