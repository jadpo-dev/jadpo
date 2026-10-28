# `subscriber.event_invalid`

`on:` names an event, not an entity:

```jadpo check project=e-subscriber-event expect=subscriber.event_invalid
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
    on: Reminder
    run: send_reminder(event.reminder_id)
}
```

```text output
error: `on: Reminder` isn't an event
 --> main.jadpo:30:9
   |
30 |     on: Reminder
   |         ^^^^^^^^ not an event
  reason: A subscriber listens for one event, a type with the `Event` base that an action emits.
  next: Name an event, declared as `type <Name> = Event { ... }` in a domain's `events/` folder (agent)
  rule: subscriber.event_invalid
  help: docs/language/12-jobs-and-events.html#subscriber-event-invalid

check failed: 1 error, 1 file
```
