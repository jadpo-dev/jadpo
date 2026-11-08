# `data.erasure_log_required`

An action erases a person, and the application doesn't say where the erasure log is kept, so restoring a backup would bring them back.

## Why Jadpo refuses it

Erasing someone removes them from your database, not from its backups.
Restore a backup taken before they were erased and they are back, as if they
had never asked. The erasure log is what prevents that: each erasure leaves an
entry, and the app erases again anyone logged who is still there every time it
starts.

The log has to survive the restore, so it can't live in a person's own
database, and only you know what else your app has. Jadpo won't guess, and
won't let an erasure go without one.

## What it looks like

app.jadpo:

```jadpo check project=e-erasure-log-required file=app.jadpo
application {}
```

```jadpo check project=e-erasure-log-required file=domains/accounts/entities/user.jadpo hidden migrate=initial
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

domains/accounts/actions/close_account.jadpo:

```jadpo check project=e-erasure-log-required file=domains/accounts/actions/close_account.jadpo expect=data.erasure_log_required
use accounts/entities { User, UserNotFound }

/// Erases someone.
action close_account(id: User.id) fails UserNotFound -> Unit {
    attempt User.erase(id)
}
```

```text output
error: Erasing a `User` needs an erasure log, and the application names none
 --> domains/accounts/actions/close_account.jadpo:5:13
  |
5 |     attempt User.erase(id)
  |             ^^^^^^^^^^ leaves an entry in the log
  reason: Restoring a backup would bring back everyone erased since it was taken. The log is what erases them again as the app starts, so it is kept where the restore can't take it back: a database that holds no person's records, or a store of files.
  next: Add `privacy: { erasure_log: { store: <connection> } }` to `application { }`, naming a connection for it (human)
  rule: data.erasure_log_required
  help: https://jadpo.dev/docs/errors/data.erasure_log_required

check failed: 1 error, 3 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because where the log is kept decides what you have to run and
back up.

## How to fix it

Name a connection for it in `app.jadpo`: a database that holds no person's
records, or a store of files. A database of its own needs no entities:

```jadpo check project=e-erasure-log-required-fix file=app.jadpo
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

```jadpo check project=e-erasure-log-required-fix file=domains/accounts/entities/user.jadpo hidden
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

```jadpo check project=e-erasure-log-required-fix file=domains/accounts/actions/close_account.jadpo hidden migrate=initial
use accounts/entities { User, UserNotFound }

/// Erases someone.
action close_account(id: User.id) fails UserNotFound -> Unit {
    attempt User.erase(id)
}
```

The page on [what erasure doesn't
rewrite](../language/15-personal-data.md#what-erasure-doesnt-rewrite) says what
the log holds and how it is replayed.

## Related

- [What erasure doesn't rewrite](../language/15-personal-data.md#what-erasure-doesnt-rewrite)
- [`data.erasure_log_store`](data.erasure_log_store.md)
