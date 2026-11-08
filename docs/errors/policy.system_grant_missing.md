# `policy.system_grant_missing`

A schedule or subscriber reaches something whose list doesn't name `Access.system`, the one way an entity lets background work in.

## Why Jadpo refuses it

Schedules and subscribers run with no request, as the system caller,
which an entity lets in by naming `Access.system`. Everything the work
could reach has to name it, whichever branch runs, as everything a route
anyone may call reaches has to name `Access.public`. A list can fail that
in three ways:

- **It names `Access.public` and not `Access.system`.** `Access.public`
  tests no caller, so without this check the work would run, on a list
  that never said background work may change these records.
- **It grants nobody.** A create, read, update or delete no list grants
  anyone can only be refused, and without this check you would first see
  that once it was deployed, as a delivery that failed.
- **It names only people.** A user field, a role, `Access.authenticated`,
  a line under `actions:` without `Access.system`, or a listed query,
  whose line can't name it. The system is none of these, so reaching one
  fails the delivery too.

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

An entity that grants nobody what the subscriber does is refused the same
way. Here `SentNote` has no `create:`, so nothing could ever create one:

```jadpo check project=e-system-grant-missing-no-list expect=policy.system_grant_missing
type Note = Text {
    max_length: 200
}

entity SentNote {
    id: Uuid
    note: Note

    policy: {
        read: [Access.authenticated]
    }
}

type ReminderDue = Event {
    note: Note
}

action keep_sent(note: Note) -> SentNote {
    return SentNote.create {
        set: {
            note: note
        }
    }
}

subscriber keep_due {
    on: ReminderDue
    run: keep_sent(event.note)
}
```

```text output
error: `keep_due` runs as the system, which nothing lets create `SentNote`
 --> main.jadpo:26:12
   |
26 | subscriber keep_due {
   |            ^^^^^^^^ runs as `Access.system`
  reason: A subscriber runs as the system, which an entity grants background work by naming `Access.system`; `SentNote`'s policy grants nobody `create`, so every run would be refused with `not_permitted`.
  next: Add `create: [Access.system]` to `SentNote`'s `policy:` (human)
  rule: policy.system_grant_missing
  help: https://jadpo.dev/docs/errors/policy.system_grant_missing

check failed: 1 error, 1 file
```

A list naming only people is refused the same way, even on a branch the
subscriber never takes. Here a route and a subscriber share
`settle_reminder`, and only the route ever deletes:

```jadpo check project=e-system-grant-missing-people expect=policy.system_grant_missing
type Note = Text {
    max_length: 200
}

entity Person {
    id: Uuid
    handle: Note {
        unique: true
    }

    policy: {
        read: [id]
    }
}

principal {
    user: {
        subject: Person.handle
    }
}

authentication {
    browser_session: {
        cookie: "reminders_session"
    }
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated, Access.system]
        update: [Access.authenticated, Access.system]
        delete: [Access.authenticated]
    }
}

type ReminderDue = Event {
    reminder_id: Reminder.id
}

action settle_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    var reminder: Reminder = attempt Reminder.get(reminder_id)
    if (reminder.sent) {
        attempt Reminder.delete(reminder_id)
    } else {
        attempt Reminder.update(reminder_id) {
            set: {
                sent: true
            }
        }
    }
}

routes Reminders {
    prefix: /reminders

    POST /{reminder_id: Reminder.id}/settled {
        status: SuccessStatus.no_content
        run: settle_reminder(reminder_id)
    }
}

subscriber send_due {
    on: ReminderDue
    run: settle_reminder(event.reminder_id)
}
```

```text output
error: `send_due` runs as the system, which `Reminder`'s `delete:` doesn't let in
 --> main.jadpo:67:12
   |
67 | subscriber send_due {
   |            ^^^^^^^^ runs as `Access.system`
  reason: A subscriber runs as the system, which an entity grants background work by naming `Access.system`; `Reminder`'s `delete:` names only people, and what the code could reach counts, whichever branch runs.
  next: Add `Access.system` to `Reminder`'s `delete:`, or split `settle_reminder` so the system's work is an action of its own that doesn't delete `Reminder` (human)
  rule: policy.system_grant_missing
  help: https://jadpo.dev/docs/errors/policy.system_grant_missing

check failed: 1 error, 1 file
```

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

**If nothing grants it yet**, give the subscriber its own list:

```jadpo fragment
        create: [Access.system]
```

```jadpo check project=e-system-grant-missing-no-list-fixed hidden
type Note = Text {
    max_length: 200
}

entity SentNote {
    id: Uuid
    note: Note

    policy: {
        create: [Access.system]
        read: [Access.authenticated]
    }
}

type ReminderDue = Event {
    note: Note
}

action keep_sent(note: Note) -> SentNote {
    return SentNote.create {
        set: {
            note: note
        }
    }
}

subscriber keep_due {
    on: ReminderDue
    run: keep_sent(event.note)
}
```

`keep_due` may now create a `SentNote`, and no request may.

**If a route shares the action**, split it, so the system's work is an
action of its own that doesn't reach the people's list:

```jadpo check project=e-system-grant-missing-split
type Note = Text {
    max_length: 200
}

entity Person {
    id: Uuid
    handle: Note {
        unique: true
    }

    policy: {
        read: [id]
    }
}

principal {
    user: {
        subject: Person.handle
    }
}

authentication {
    browser_session: {
        cookie: "reminders_session"
    }
}

entity Reminder {
    id: Uuid
    note: Note
    sent: Bool

    policy: {
        create: [Access.authenticated]
        read: [Access.authenticated, Access.system]
        update: [Access.authenticated, Access.system]
        delete: [Access.authenticated]
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

action settle_reminder(reminder_id: Reminder.id) fails ReminderNotFound -> Unit {
    var reminder: Reminder = attempt Reminder.get(reminder_id)
    if (reminder.sent) {
        attempt Reminder.delete(reminder_id)
    } else {
        attempt mark_sent(reminder_id)
    }
}

routes Reminders {
    prefix: /reminders

    POST /{reminder_id: Reminder.id}/settled {
        status: SuccessStatus.no_content
        run: settle_reminder(reminder_id)
    }
}

subscriber send_due {
    on: ReminderDue
    run: mark_sent(event.reminder_id)
}
```

`send_due` now runs `mark_sent`, which only updates a reminder. The route
still runs `settle_reminder`, which deletes one or calls `mark_sent`.

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

- [Policy and background work](../language/09-policy.md#policy-and-background-work)
- [`Access.public` doesn't let background work in](../language/09-policy.md#accesspublic-doesnt-let-background-work-in)
- [A list naming only people](../language/09-policy.md#a-list-naming-only-people)
- [The system and `actions:` lines](../language/09-policy.md#the-system-and-actions-lines)
- [Actions the system runs inside a request](../language/09-policy.md#actions-the-system-runs-inside-a-request)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)
