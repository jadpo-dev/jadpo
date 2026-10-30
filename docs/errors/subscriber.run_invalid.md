# `subscriber.run_invalid`

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
  help: https://jadpo.dev/docs/errors/subscriber.run_invalid

check failed: 1 error, 1 file
```
