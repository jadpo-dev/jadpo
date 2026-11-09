# `data.erasure_log_store`

The erasure log is kept somewhere restoring a person's database would take it back with them, or somewhere it can't be kept.

## Why Jadpo refuses it

The log is what erases someone again after a backup brings them back, so it
has to outlast that backup. Kept in the database the person's records are in,
a restore would bring back their records and lose every entry written since
the backup, so nobody erased since would be erased again. A store that holds
records erasure reaches has the same problem, and a connection that is neither
a database nor a store of files can't keep it at all.

Jadpo won't choose for you, since a new database or a store of files is
something you run and back up.

## What it looks like

```jadpo check project=e-erasure-log-store file=app.jadpo hidden
application {}
```

```jadpo check project=e-erasure-log-store file=domains/accounts/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
```

app.jadpo:

```jadpo check project=e-erasure-log-store file=app.jadpo expect=data.erasure_log_store
application {
    privacy: {
        erasure_log: {
            store: main
        }
    }
}
```

```text output
error: `main` holds a person's records, so a restore would take the log back with them
 --> app.jadpo:4:20
  |
4 |             store: main
  |                    ^^^^ `User` is kept here
  reason: The log is what erases someone again after a backup brings them back. Restoring `main` would bring back their `User` records and lose every entry written since the backup, so nobody erased since would be erased again.
  next: Add a database that holds no person's records and name it, or name a store of files (human)
  rule: data.erasure_log_store
  help: https://jadpo.dev/docs/errors/data.erasure_log_store

check failed: 1 error, 2 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what you run and back up.

## How to fix it

The question is what else you can run beside the database your people are in.

**If you can run a second database,** add one and name that. It needs no
entities of its own:

```jadpo check project=e-erasure-log-store-fix file=app.jadpo
application {
    connections: {
        main: {
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

```jadpo check project=e-erasure-log-store-fix file=domains/accounts/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
    decline data.personal_unread: "Read by code this example leaves out"
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
```

A restore of `main` now leaves the log as it was.

**If the app runs on one server with a disk of its own,** name a store of files:

```jadpo check project=e-erasure-log-store-files file=app.jadpo
application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        erasures: {
            driver: Driver.disk
            path: "/var/lib/app/erasures"
        }
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
}
```

```jadpo check project=e-erasure-log-store-files file=domains/accounts/entities/user.jadpo hidden migrate=initial
entity User {
    id: Uuid
    decline data.personal_unread: "Read by code this example leaves out"
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
```

Each person erased is now a file in that folder. A Worker has no disk, so it
keeps the log in a database.

## Related

- [What erasure doesn't rewrite](../language/15-personal-data.md#what-erasure-doesnt-rewrite)
- [`data.erasure_log_required`](data.erasure_log_required.md)
