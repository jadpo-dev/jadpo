# `data.lifecycle_purge_after`

A lifecycle's `purge:` has an `after:` that isn't a positive duration or a `Duration` setting.

## Why Jadpo refuses it

`after:` is how long a hidden record is kept before purge deletes it for
good. It is written in place, as a duration such as `30d`, or read from a
`Duration` setting. A period of nothing would delete a record the moment
it is hidden, which is no different from deleting it, and anything that
isn't a duration gives purge no period to count.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-purge-after expect=data.lifecycle_purge_after
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 0d
            since: deleted_at
        }
    }
}
```

```text output
error: `after: 0d` isn't a positive duration
 --> main.jadpo:8:20
  |
8 |             after: 0d
  |                    ^^ not a positive duration
  reason: `after:` is how long a hidden record is kept before purge deletes it, a duration such as `30d` or `12h`, longer than nothing; a hidden record would be deleted the moment it is hidden.
  next: Write a duration, such as `after: 30d` (agent)
  rule: data.lifecycle_purge_after
  help: https://jadpo.dev/docs/errors/data.lifecycle_purge_after

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A value that isn't a duration at all is refused the same way:

```jadpo check project=e-purge-after-not-duration expect=data.lifecycle_purge_after
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 30
            since: deleted_at
        }
    }
}
```

```text output
error: `after: 30` isn't a positive duration
 --> main.jadpo:8:20
  |
8 |             after: 30
  |                    ^^ not a positive duration
  reason: `after:` is how long a hidden record is kept before purge deletes it, a duration such as `30d` or `12h`, longer than nothing; it isn't a duration.
  next: Write a duration, such as `after: 30d` (agent)
  rule: data.lifecycle_purge_after
  help: https://jadpo.dev/docs/errors/data.lifecycle_purge_after

check failed: 1 error, 1 file
```

So is a setting that isn't a `Duration`:

```jadpo check project=e-purge-after-setting expect=data.lifecycle_purge_after
config {
    keep_days: Int {
        default: 30
    }
}

entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: config.keep_days
            since: deleted_at
        }
    }
}
```

```text output
error: `after: config.keep_days` isn't a duration
 --> main.jadpo:14:20
   |
14 |             after: config.keep_days
   |                    ^^^^^^^^^^^^^^^^ not a duration setting
  reason: `after:` is how long a hidden record is kept before purge deletes it; `keep_days` is `Int`, and a retention period is a `Duration` setting, such as `keep_days: Duration { default: 30d }`.
  next: Make `keep_days` a `Duration` in `config.jadpo`, or write the period, such as `after: 30d` (agent)
  rule: data.lifecycle_purge_after
  help: https://jadpo.dev/docs/errors/data.lifecycle_purge_after

check failed: 1 error, 1 file
```

## How to fix it

**If the period is fixed**, write it as a duration:

```jadpo fragment
        purge: {
            after: 30d
            since: deleted_at
        }
```

```jadpo check project=e-purge-after-fix-1 hidden
entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: 30d
            since: deleted_at
        }
    }
}
```

A hidden todo is now deleted for good 30 days after `deleted_at`.

**If the period should change with where the app runs**, read a `Duration`
setting:

```jadpo fragment
config {
    keep_for: Duration {
        default: 30d
    }
}

entity Todo {
    ...
    lifecycle: {
        ...
        purge: {
            after: config.keep_for
            since: deleted_at
        }
    }
}
```

```jadpo check project=e-purge-after-fix-2 hidden
config {
    keep_for: Duration {
        default: 30d
    }
}

entity Todo {
    id: Uuid
    deleted_at: Instant?

    lifecycle: {
        visible: deleted_at == none
        purge: {
            after: config.keep_for
            since: deleted_at
        }
    }
}
```

The period is now the value of `keep_for` the app started with.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
