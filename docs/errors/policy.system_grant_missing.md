# `policy.system_grant_missing`

A schedule or subscriber reaches a grant that names `Access.public` but not `Access.system`, so the background work would always be refused.

## Why Jadpo refuses it

Schedules and subscribers run with no request, as the system caller,
which gets exactly what `Access.system` grants. `Access.public` lets in
anyone who calls a route, signed in or not, but the system is nobody
calling. So a grant that names `Access.public` and not `Access.system`
never lets background work in, and the subscriber would be refused every
time it runs.

Jadpo won't choose the fix for you. Granting `Access.system` lets your
app's background work change these records, which is a decision about
who may change them.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/policy.system_grant_missing

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether background work may mark reminders as sent.

The magic-link `send:` action, a webhook's reply action and the
`on_first_sign_in:` action run as the system too, and are checked the
same way, where each is named. Every action background work reaches lets
the system in by its own line under `actions:`, wherever it is called
from, as it does for a request. So a line naming only `Access.public` is
refused however deep the call:

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
  help: https://jadpo.dev/docs/errors/policy.system_grant_missing

check failed: 1 error, 1 file
```

## How to fix it

The question is whether this work should run in the background. If it
shouldn't, reach `Reminder` from a route instead, where the route's
caller is let in by `Access.public`.

**If the subscriber may change the records**, add `Access.system` to the
grant it reaches:

```jadpo fragment
        update: [Access.public, Access.system]
```

```jadpo check project=e-system-grant-missing-entity hidden
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
        update: [Access.public, Access.system]
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

`send_due` may now mark a reminder as sent when a `ReminderDue` event is
delivered. Anyone calling a route may still update a reminder, as
before.

**If the subscriber may run a listed action**, add `Access.system` to
that action's line under `actions:`:

```jadpo fragment
        mark_sent: [Access.public, Access.system]
```

```jadpo check project=e-system-grant-missing-action-line hidden
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
        mark_sent: [Access.public, Access.system]
    }
}
```

`send_reminder` may now call `mark_sent` when `send_due` runs it. The
line names `Access.system` beside `Access.public`, so it lets in the
system and every caller of a route alike.

## Related

- [Background work](../language/09-policy.md#background-work)
- [`Access.public` doesn't let background work in](../language/09-policy.md#accesspublic-doesnt-let-background-work-in)
- [The system and `actions:` lines](../language/09-policy.md#the-system-and-actions-lines)
- [Actions the system runs inside a request](../language/09-policy.md#actions-the-system-runs-inside-a-request)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)
