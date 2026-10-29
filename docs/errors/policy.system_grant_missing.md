# `policy.system_grant_missing`

A subscriber runs as the system, and the entity it changes grants that
only to anyone at all:

```jadpo check project=e-system-grant-missing expect=policy.system_grant_missing
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.public]
        update: [Access.public]
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

```text output
error: `send_due` runs as the system, which only `Access.public` lets update `Reminder`
 --> main.jadpo:29:12
   |
29 | subscriber send_due {
   |            ^^^^^^^^ runs as `Access.system`
  reason: A subscriber runs as the system, which an entity grants background work by naming `Access.system`; `Reminder`'s `update:` names `Access.public` and not `Access.system`, and anyone at all is never how background work is let in.
  next: Add `Access.system` to `Reminder`'s `update:`, or reach `Reminder` from a route instead (human)
  rule: policy.system_grant_missing
  help: docs/errors/policy.system_grant_missing.md

check failed: 1 error, 1 file
```

The magic-link `send:` action, a webhook's reply action and
`on_first_sign_in:`'s run as the system too, and are checked the same way,
where each is named. Every action background work reaches
lets the system in by its own line under `actions:`, wherever it is
called from, as it does a request, so a line naming only `Access.public`
is refused however deep:

```jadpo check project=e-system-grant-missing-line expect=policy.system_grant_missing
type Note = Text {
    max_length: 200
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated, Access.system]
        update: [Access.system]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action mark_sent(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt Reminder.update(reminder_id) {
        set: {
            sent: true
        }
    }
}

action send_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    attempt mark_sent(reminder_id)
}

subscriber send_due {
    on: ReminderDue
    run: send_reminder(event.reminder_id)
}

policy {
    actions: {
        mark_sent: [Access.public]
    }
}
```

```text output
error: `send_due` runs as the system, which only `Access.public` lets call `mark_sent`
 --> main.jadpo:33:12
   |
33 | subscriber send_due {
   |            ^^^^^^^^ runs as `Access.system`
  reason: A subscriber runs as the system, which an entity grants background work by naming `Access.system`; `mark_sent`'s line under `actions:` names `Access.public` and not `Access.system`, and anyone at all is never how background work is let in.
  next: Add `Access.system` to `mark_sent`'s line under `actions:` (human)
  rule: policy.system_grant_missing
  help: docs/errors/policy.system_grant_missing.md

check failed: 1 error, 1 file
```
