# `data.person_removal_across_stores`

Something other than erasure would remove a person another store refers to, and nothing would apply what that reference's `erase:` says.

## Why Jadpo refuses it

A reference to a person from another store isn't a foreign key. No database
can apply it, so only erasing the person does, after their own commit. If the
person's record went any other way, by a hard `delete`, a `retention:` or a
`purge:`, their records in the other store would stay, holding an `id` no
record has, and no later erasure could find them, because there is nobody left
to erase.

Jadpo won't guess whether those records should go too or be kept, so it asks
you to choose how the person goes.

## What it looks like

app.jadpo:

```jadpo check project=e-person-removal file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
    }
}
```

domains/accounts/entities/user.jadpo:

```jadpo check project=e-person-removal file=domains/accounts/entities/user.jadpo
entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    policy: {
        delete: [Access.public]
    }

    person: true
}
```

```jadpo check project=e-person-removal file=domains/accounts/entities/old_note.jadpo migrate=initial
use accounts/entities { User }

entity OldNote {
    id: Uuid
    owner_id: User.id {
        erase: Erasure.deleted
    }

    store: archive
}
```

domains/accounts/actions/remove.jadpo:

```jadpo check project=e-person-removal file=domains/accounts/actions/remove.jadpo expect=data.person_removal_across_stores
use accounts/entities { User, UserNotFound }

/// Removes someone.
action remove(id: User.id) fails UserNotFound -> Unit {
    attempt User.delete(id)
}
```

```text output
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because it decides how a person leaves your app.

## How to fix it

**If removing them means forgetting them,** call `User.erase`, which applies
the reference. It needs [an erasure
log](data.erasure_log_required.md).

**If removing them should keep them findable,** give `User` a lifecycle whose
`delete` transition hides them, with no `purge:`. Erasure still finds them.

**If they must go on a schedule,** keep the other entity in the person's store,
where the reference is a foreign key and says `on_delete:`, and a `retention:`
or a `purge:` takes it into account.
