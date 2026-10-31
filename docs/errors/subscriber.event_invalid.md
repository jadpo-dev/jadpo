# `subscriber.event_invalid`

A subscriber's `on:` names something other than an event, such as an entity or a plain type.

## Why Jadpo refuses it

A subscriber runs an action each time an event is committed, and only
events are committed and delivered: an action announces one with `emit`,
and it reaches each subscriber after the action's writes commit. An
entity, or any type whose base isn't `Event`, is never delivered, so a
subscriber listening for one would never run. Jadpo refuses it rather
than leave a subscriber that silently does nothing.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/subscriber.event_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the event already exists**, name it in `on:`. In `main.jadpo`:

```jadpo fragment
subscriber send_due {
    on: ReminderDue
```

```jadpo check project=e-subscriber-event-fix-1 hidden
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

`send_due` now runs `send_reminder` once for each `ReminderDue` an action
emits, after that action commits. Inside `run:`, `event` is the
`ReminderDue`, so `event.reminder_id` is the reminder it names.

When there is no event for what happened yet, declare one as a type
whose base is `Event`, have the action where it happens `emit` it, and
name it in `on:`. A subscriber can't listen for a record changing on its
own: the action that changes it announces the change.

## Related

- [Subscribers](../language/12-jobs-and-events.md#subscribers)
- [Declaring an event](../language/12-jobs-and-events.md#declaring-an-event)
- [Emitting an event](../language/12-jobs-and-events.md#emitting-an-event)
