# `flag.store_required`

An app declares flags but no database named to keep their state in, because it has several databases or none.

## Why Jadpo refuses it

Each flag's current value, and who gets which, is kept in a table of
your app's database, so a change made while the app runs reaches every
server. With one database that is the one. With several, any of them
could hold it, and Jadpo won't choose one for you: `application { flags:
{ store: } }` names it, and with no database at all there is nowhere to
keep it.

## What it looks like

flags.jadpo, in an app with two databases, `archive` and `main`:

```jadpo check project=e-flag-store-required file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
}
```

```jadpo check project=e-flag-store-required file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-flag-store-required file=flags.jadpo expect=flag.store_required
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

```text output
error: The flags have no database named to keep their state in
 --> flags.jadpo:1:1
  |
1 | flags {
  | ^^^^^ no store for their state
  reason: Each flag's current value, and who gets which, is kept in a database so a change reaches every server while the app runs; this application has `archive` and `main`, and `app.jadpo` names none with `flags: { store: }`.
  next: Add `flags: { store: archive }` to `application { }` in `app.jadpo` (human)
  rule: flag.store_required
  help: https://jadpo.dev/docs/errors/flag.store_required

check failed: 1 error, 3 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which database keeps the state of every flag in the app.

## How to fix it

The question is which database should keep the flags' state. It holds
every flag's, so one database answers it for the whole app.

**If one database should keep it**, name it in `flags:` in `app.jadpo`.
The next step suggests the first of them, here `archive`:

```jadpo fragment
    flags: {
        store: archive
    }
```

```jadpo check project=e-flag-store-required-fix-1 file=app.jadpo hidden
application {
    connections: {
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
    }
    flags: {
        store: archive
    }
}
```

```jadpo check project=e-flag-store-required-fix-1 file=config.jadpo hidden
config {
    archive_url: PostgresUrl
    main_url: PostgresUrl
}
```

```jadpo check project=e-flag-store-required-fix-1 file=flags.jadpo hidden migrate=initial
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

Each flag's state is now kept in `archive`, in a system table its first
migration creates, `jadpo_flags`.

## Related

- [Changing a flag](../language/16-built-in-services.md#changing-a-flag)
- [Feature flags](../language/16-built-in-services.md#feature-flags)
- [Stores](../language/05-data.md#stores)
