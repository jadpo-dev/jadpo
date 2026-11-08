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
error: `User.delete` would delete a `User` that `OldNote.owner_id` refers to from another store
 --> domains/accounts/actions/remove.jadpo:5:13
  |
5 |     attempt User.delete(id)
  |             ^^^^^^^^^^^ deletes outside erasure
  reason: `OldNote.owner_id` isn't a foreign key: only erasing a `User` applies what its `erase:` says, after the commit, in its store. Deleted any other way, the `User` would be gone and the `OldNote` records would stay, out of reach of any erasure.
  next: Call `User.erase`, or give `User` a lifecycle whose `delete` transition hides them (human)
  rule: data.person_removal_across_stores
  help: https://jadpo.dev/docs/errors/data.person_removal_across_stores

check failed: 1 error, 4 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how a person leaves your app.

## How to fix it

The question is what removing this person should mean.

**If removing them means forgetting them,** call `User.erase`, which applies
the reference. It needs [an erasure log](data.erasure_log_required.md):

```jadpo check project=e-person-removal-erase file=domains/accounts/actions/remove.jadpo
use accounts/entities { User, UserNotFound }

/// Removes someone.
action remove(id: User.id) fails UserNotFound -> Unit {
    attempt User.erase(id)
}
```

```jadpo check project=e-person-removal-erase file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
        erasures: {
            driver: Driver.postgres
        }
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
}
```

```jadpo check project=e-person-removal-erase file=domains/accounts/entities/user.jadpo hidden
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

```jadpo check project=e-person-removal-erase file=domains/accounts/entities/old_note.jadpo hidden migrate=initial
use accounts/entities { User }

entity OldNote {
    id: Uuid
    owner_id: User.id {
        erase: Erasure.deleted
    }

    store: archive
}
```

Removing someone now erases them, and their old notes go after the commit.

**If removing them should keep them findable,** give `User` a lifecycle whose
`delete` transition hides them, with no `purge:`:

```jadpo check project=e-person-removal-hide file=domains/accounts/entities/user.jadpo
entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
        }
    }

    policy: {
        read: [Access.public]
        delete: [Access.public]
    }

    person: true
}
```

```jadpo check project=e-person-removal-hide file=app.jadpo hidden
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

```jadpo check project=e-person-removal-hide file=domains/accounts/entities/old_note.jadpo hidden migrate=initial
use accounts/entities { User }

entity OldNote {
    id: Uuid
    owner_id: User.id {
        erase: Erasure.deleted
    }

    store: archive
}
```

```jadpo check project=e-person-removal-hide file=domains/accounts/actions/remove.jadpo hidden
use accounts/entities { User, UserNotFound }

/// Removes someone.
action remove(id: User.id) fails UserNotFound -> Unit {
    attempt User.delete(id)
}
```

`User.delete` now hides them, and a later erasure still finds them and their
old notes.

**If they must go on a schedule,** keep the other entity in the person's store,
where the reference is a foreign key and says `on_delete:`, and a `retention:`
or a `purge:` takes it into account:

```jadpo check project=e-person-removal-one-store file=domains/accounts/entities/old_note.jadpo
use accounts/entities { User }

entity OldNote {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }
}
```

```jadpo check project=e-person-removal-one-store file=app.jadpo hidden
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

```jadpo check project=e-person-removal-one-store file=domains/accounts/entities/user.jadpo hidden migrate=initial
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

```jadpo check project=e-person-removal-one-store file=domains/accounts/actions/remove.jadpo hidden
use accounts/entities { User, UserNotFound }

/// Removes someone.
action remove(id: User.id) fails UserNotFound -> Unit {
    attempt User.delete(id)
}
```

An ordinary delete now takes the person's old notes with them, in one
statement.

## Related

- [Records in another store](../language/15-personal-data.md#records-in-another-store)
- [`data.reference_across_stores`](data.reference_across_stores.md)
- [Retention](../language/15-personal-data.md#retention)
