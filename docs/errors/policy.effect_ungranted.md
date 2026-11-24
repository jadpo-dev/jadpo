# `policy.effect_ungranted`

A route runs an action that nobody is allowed to call, because nothing in the app says who may.

## Why Jadpo refuses it

An action takes its permission from the stored records it reads and
changes: each entity's grants say who may create, read, update or delete
its records. An action that reaches no entity, such as a health check,
has no grants to take, so nobody may call it. The same happens when a
route reaches a create, read, update or delete that no grant names.
A route nobody may call answers every request with 403, so Jadpo refuses
it when you check, rather than when someone calls it.

Jadpo won't choose who may call the action for you. Root `policy { }`
lists such actions under `actions:`, each with who may run it, and that
list decides who can reach your app's data and behaviour.

## What it looks like

main.jadpo:

```jadpo check project=e-effect-ungranted expect=policy.effect_ungranted
type Health = Output {
    ok: Bool
}

action check_health() -> Health {
    return Health {
        ok: true
    }
}

routes Checkup {
    prefix: /health

    POST / {
        authentication: RouteAuthentication.none
        output: Health
        run: check_health()
    }
}
```

```text output
error: `POST /health` runs `check_health`, which nobody may call
 --> main.jadpo:14:5
   |
14 |     POST / {
   |     ^^^^^^ reaches no entity
  reason: An action takes its permission from the entities it reaches, and `check_health` reaches none, so it is refused to everyone unless root `policy { }` lists who may call it under `actions:`.
  next: List `check_health: [...]` under `actions:` in `policy.jadpo` (human)
  rule: policy.effect_ungranted
  help: https://jadpo.dev/docs/errors/policy.effect_ungranted

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who may call `check_health`.

A line under `actions:` can also let nobody in. A user field such as
`Note.owner_id`, or a role held in a record, is checked on the records
the action itself creates, changes or deletes. An action that changes
none of them, such as a summary of the caller's notes, passes nobody
through such a line, so its route could never succeed. Here,
`note_tidings` changes no note.

main.jadpo:

```jadpo check project=e-listed-line-unheld expect=policy.effect_ungranted
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
    }
}

type Tidings = Output {
    ok: Bool
}

action note_tidings() -> Tidings {
    return Tidings {
        ok: true
    }
}

routes Bulletin {
    prefix: /tidings

    POST / {
        output: Tidings
        run: note_tidings()
    }
}

policy {
    actions: {
        note_tidings: [Note.owner_id]
    }
}
```

```text output
error: `POST /tidings` runs `note_tidings`, which nobody may call
 --> main.jadpo:46:5
   |
46 |     POST / {
   |     ^^^^^^ nobody passes its line
  reason: `note_tidings`'s line under `actions:` names `Note.owner_id`, the caller a `Note` the action changes points at, and `note_tidings` changes no such record itself, so no caller passes the line as the action starts and this route could never succeed.
  next: List `Access.authenticated` or a role held across the whole application for `note_tidings` under `actions:` (human)
  rule: policy.effect_ungranted
  help: https://jadpo.dev/docs/errors/policy.effect_ungranted

check failed: 1 error, 1 file
```

A field's own `policy:` can let nobody change it, as a field only the
identity provider sets does. A route whose action may set such a field
could only be refused, whether it always sets it or only when the
request supplies it, as a patch does. Here a user may change their
display name, but nobody may change their email:

main.jadpo:

```jadpo check project=e-effect-ungranted-field expect=policy.effect_ungranted
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
    display_name: Handle
    email: Email {
        policy: {
            update: []
        }
    }

    policy: {
        read: [id]
        update: [id]
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

type AccountChanges = Input {
    pick: [User.display_name, User.email]
    optional: all
}

action edit_account(user_id: User.id, changes: AccountChanges) fails UserNotFound -> Unit {
    attempt User.update(user_id) {
        patch: changes
    }
}

routes Accounts {
    prefix: /users

    PATCH /{user_id: User.id} {
        input: AccountChanges
        status: SuccessStatus.no_content
        run: edit_account(user_id, input)
    }
}
```

```text output
error: `PATCH /users/{user_id}` may set `User.email`, which nobody may
 --> main.jadpo:47:5
   |
47 |     PATCH /{user_id: User.id} {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^ may set `email`
  reason: Policy is always on, and a field's own `policy:` narrows who may change it: `User.email`'s own `update:` names nobody, so every request that supplies it is refused. Who may set a field is the app owner's decision, not the code's, and letting more people set it is a widening a person accepts.
  next: Leave `email` out of what this route's action writes, or ask the app's owner whether a request may set it: they name who may in `email`'s own `update:` (human)
  rule: policy.effect_ungranted
  help: https://jadpo.dev/docs/errors/policy.effect_ungranted

check failed: 1 error, 1 file
```

Who may set a field is the app owner's decision, so this one stops for
them too.

## How to fix it

The question is who should be able to call the action.

**If anyone may call it, signed in or not**, list it for `Access.public`.
In a project, this goes in `policy.jadpo`, beside `app.jadpo`, with the
action's domain in front of its name (`monitoring.check_health`):

```jadpo fragment
policy {
    actions: {
        check_health: [Access.public]
    }
}
```

```jadpo check project=e-effect-ungranted-public hidden
type Health = Output {
    ok: Bool
}

action check_health() -> Health {
    return Health {
        ok: true
    }
}

routes Checkup {
    prefix: /health

    POST / {
        authentication: RouteAuthentication.none
        output: Health
        run: check_health()
    }
}

policy {
    actions: {
        check_health: [Access.public]
    }
}
```

`POST /health` now answers anyone. Its route says
`authentication: RouteAuthentication.none`, so no caller has to sign in.

**If anyone signed in may call it**, list it for `Access.authenticated`:

```jadpo fragment
policy {
    actions: {
        note_tidings: [Access.authenticated]
    }
}
```

```jadpo check project=e-listed-line-authenticated hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id]
    }
}

type Tidings = Output {
    ok: Bool
}

action note_tidings() -> Tidings {
    return Tidings {
        ok: true
    }
}

routes Bulletin {
    prefix: /tidings

    POST / {
        output: Tidings
        run: note_tidings()
    }
}

policy {
    actions: {
        note_tidings: [Access.authenticated]
    }
}
```

Every signed-in caller may now call `POST /tidings`. Its reads still
follow `Note`'s grants, so each caller sees only the notes they own.

To let in only some people, list a role they hold across the whole
application, such as `StaffRole.operator`, a membership whose scope is
the application ([A membership role](../language/09-policy.md#a-membership-role)).
A role held in one record lets nobody in here, for the reason above.

**If a route sets a field nobody may change**, leave the field out of
what its action writes. For a patch, take it out of the `Input`:

```jadpo fragment
type AccountChanges = Input {
    pick: [User.display_name]
    optional: all
}
```

```jadpo check project=e-effect-ungranted-field-fix hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
    display_name: Handle
    email: Email {
        policy: {
            update: []
        }
    }

    policy: {
        read: [id]
        update: [id]
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

type AccountChanges = Input {
    pick: [User.display_name]
    optional: all
}

action edit_account(user_id: User.id, changes: AccountChanges) fails UserNotFound -> Unit {
    attempt User.update(user_id) {
        patch: changes
    }
}

routes Accounts {
    prefix: /users

    PATCH /{user_id: User.id} {
        input: AccountChanges
        status: SuccessStatus.no_content
        run: edit_account(user_id, input)
    }
}
```

If the app's owner decides people may change it after all, they name who
in the field's own `update:`, such as `update: [id]` for the person
themselves. That widens who may change it, so once a commit holds the
field it needs a person's acceptance
([Who may set a field](../language/09-policy.md#who-may-set-a-field)).

## Related

- [Permissions on actions](../language/09-policy.md#permissions-on-actions)
- [A membership role](../language/09-policy.md#a-membership-role)
- [Lines that depend on a record](../language/09-policy.md#lines-that-depend-on-a-record)
- [Anyone signed in, or anyone](../language/09-policy.md#anyone-signed-in-or-anyone)
- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
- [Who may set a field](../language/09-policy.md#who-may-set-a-field)
