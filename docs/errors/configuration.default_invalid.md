# `configuration.default_invalid`

A setting's `default:` or `test:` value, or a test's `config:` value, breaks the rules of the setting's type.

## Why Jadpo refuses it

A value written in source is checked while compiling, against the same
rules the app applies to an environment variable when it starts. A default
that breaks them would stop the app starting on any deployment that relies
on it, so it is caught here. Jadpo can't tell whether the value or the type
is the mistake, so it leaves that to you.

## What it looks like

```jadpo check project=e-default-invalid expect=configuration.default_invalid
type PrimaryDatabase = Url {}

config {
    primary_database: PrimaryDatabase {
        default: "postgres://localhost:5432/todo"
    }
}
```

```text output
error: `"postgres://localhost:5432/todo"` isn't a valid `PrimaryDatabase`
 --> main.jadpo:5:18
  |
5 |         default: "postgres://localhost:5432/todo"
  |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not a valid value
  reason: The value isn't an `http://` or `https://` URL with a host, such as `https://example.com`; a default is checked while compiling, with the rules of the setting's type.
  next: Write a valid `PrimaryDatabase` value (agent)
  rule: configuration.default_invalid
  help: https://jadpo.dev/docs/errors/configuration.default_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A value that breaks a rule its own type declares is refused the same way:

```jadpo check project=e-default-invalid-rule expect=configuration.default_invalid
type ListLimit = Int {
    min: 1
    max: 100
}

config {
    page_size: ListLimit {
        default: 500
    }
}
```

```text output
error: `500` isn't a valid `ListLimit`
 --> main.jadpo:8:18
  |
8 |         default: 500
  |                  ^^^ breaks `max: 100`
  reason: `ListLimit` declares `max: 100`, and the value is above 100. A default is checked against every rule while compiling.
  next: Use a value that `ListLimit` allows (agent)
  rule: configuration.default_invalid
  help: https://jadpo.dev/docs/errors/configuration.default_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If the value is wrong**, write one the type allows:

```jadpo check project=e-default-invalid-value
type ListLimit = Int {
    min: 1
    max: 100
}

config {
    page_size: ListLimit {
        default: 100
    }
}
```

A deployment that doesn't set `PAGE_SIZE` now starts with 100.

**If the setting is the address of a PostgreSQL database**, declare it as
one, with no default, because an address like that carries a password and
is always secret:

```jadpo check project=e-default-invalid-type
config {
    primary_database: PostgresUrl
}
```

The value now comes from each deployment's environment, and the app won't
start without a `postgres://` address.

## Related

- [Defaults and test values](../language/10-configuration-and-time.md#defaults-and-test-values)
- [What a setting can hold](../language/10-configuration-and-time.md#what-a-setting-can-hold)
- [Database and other connection URLs](../language/10-configuration-and-time.md#database-and-other-connection-urls)
