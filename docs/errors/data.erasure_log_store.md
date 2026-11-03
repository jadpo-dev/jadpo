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
```

```text output
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because it decides what you run and back up.

## How to fix it

Add a database of its own and name that. It needs no entities:

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
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
```

Or name a store of files, on a server with a disk of its own:

```jadpo fragment
application {
    connections: {
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

A Worker has no disk, so it keeps the log in a database.
