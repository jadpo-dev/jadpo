# `subscriber.retry_invalid`

`attempts:` is from 1 to 100:

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
  help: docs/language/12-jobs-and-events.html#subscriber-retry-invalid

check failed: 1 error, 1 file
```
