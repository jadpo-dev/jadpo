# `flag.store_invalid`

Flags' settings in `app.jadpo` hold something other than a database's `store:`.

## Why Jadpo refuses it

`flags: { store: }` names the database every flag's state is kept in,
and holds nothing else: the flags themselves are declared in
`flags.jadpo`. A name that isn't one of the app's databases would leave
the flags with nowhere to keep what they are switched to.

## What it looks like

```jadpo check project=e-flag-store-invalid file=app.jadpo expect=flag.store_invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    flags: {
        store: mian
    }
}
```

```text output
error: `store: mian` isn't a database
 --> app.jadpo:9:16
  |
9 |         store: mian
  |                ^^^^ not a database
  reason: Every flag's state is kept in a database, so a change reaches each server; this application's databases are `main`.
  next: Use `main` instead of `mian` (agent)
  rule: flag.store_invalid
  help: https://jadpo.dev/docs/errors/flag.store_invalid

check failed: 1 error, 3 files
```

```jadpo check project=e-flag-store-invalid file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-flag-store-invalid file=flags.jadpo hidden
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it names a database by a misspelling**, use its name:

```jadpo fragment
    flags: {
        store: main
    }
```

```jadpo check project=e-flag-store-invalid-fix-1 file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    flags: {
        store: main
    }
}
```

```jadpo check project=e-flag-store-invalid-fix-1 file=config.jadpo hidden
config {
    database_url: PostgresUrl
}
```

```jadpo check project=e-flag-store-invalid-fix-1 file=flags.jadpo hidden migrate=initial
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2099-01-31"
    }
}
```

Each flag's state is now kept in `main`. With one database, `flags:` can
also be left out: the only database keeps it.

## Related

- [Changing a flag](../language/16-built-in-services.md#changing-a-flag)
- [Stores](../language/05-data.md#stores)
