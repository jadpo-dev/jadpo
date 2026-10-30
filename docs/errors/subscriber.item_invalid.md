# `subscriber.item_invalid`

A subscriber holds `on:`, `run:` and `retry:` only:

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
