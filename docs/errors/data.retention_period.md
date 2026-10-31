# `data.retention_period`

A retention's `after:` isn't a `Duration` setting in `config.jadpo`.

## Why Jadpo refuses it

How long a person's data is kept is decided per environment, and the
personal-data report says how long each kind is kept, so every retention
period is a setting in `config.jadpo`. Here a visitor's sign-up address
is kept for `30d` written beside the field, where no environment could
change it and no report could read it as a setting.

## What it looks like

main.jadpo:

```jadpo check project=e-retention-period expect=data.retention_period
entity Visitor {
    id: Uuid
    signup_ip: IpAddress? {
        retention: {
            after: 30d
            since: created_at
        }
    }
    created_at: Instant {
        generated: Generated.on_create
    }
}
```

```text output
error: `after: 30d` is written in place
 --> main.jadpo:5:20
  |
5 |             after: 30d
  |                    ^^^ a duration, not a setting
  reason: A retention period is a `Duration` setting in `config.jadpo`, so each environment sets its own and the personal-data report can say how long each kind of data is kept.
  next: Add `signup_ip_retention: Duration { default: 30d }` to `config.jadpo`, and write `after: config.signup_ip_retention` (agent)
  rule: data.retention_period
  help: https://jadpo.dev/docs/errors/data.retention_period

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A setting that isn't a `Duration`, such as a `Text`, is refused the same
way, since it says no length of time:

```jadpo check project=e-retention-period-text expect=data.retention_period
config {
    signup_ip_retention: Text {
        default: "a month"
    }
}

entity Visitor {
    id: Uuid
    signup_ip: IpAddress? {
        retention: {
            after: config.signup_ip_retention
            since: created_at
        }
    }
    created_at: Instant {
        generated: Generated.on_create
    }
}
```

```text output
error: `after: config.signup_ip_retention` isn't a duration
 --> main.jadpo:11:20
   |
11 |             after: config.signup_ip_retention
   |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^ not a `Duration` setting
  reason: `after:` is how long a value or record is kept, a `Duration` setting; `signup_ip_retention` is `Text`.
  next: Make `signup_ip_retention` a `Duration` in `config.jadpo`, such as `signup_ip_retention: Duration { default: 30d }` (agent)
  rule: data.retention_period
  help: https://jadpo.dev/docs/errors/data.retention_period

check failed: 1 error, 1 file
```

## How to fix it

**If 30 days is the period wherever the app runs**, add a `Duration`
setting with it as the default, and name it in `after:`:

```jadpo fragment
config {
    signup_ip_retention: Duration {
        default: 30d
    }
}

entity Visitor {
    ...
    signup_ip: IpAddress? {
        retention: {
            after: config.signup_ip_retention
            since: created_at
        }
    }
    ...
}
```

```jadpo check project=e-retention-period-fix hidden
config {
    signup_ip_retention: Duration {
        default: 30d
    }
}

entity Visitor {
    id: Uuid
    signup_ip: IpAddress? {
        retention: {
            after: config.signup_ip_retention
            since: created_at
        }
    }
    created_at: Instant {
        generated: Generated.on_create
    }
}
```

Each environment can now set `SIGNUP_IP_RETENTION` to its own period, and
the address is cleared at the first hourly run once the visitor signed up
longer ago than that.

## Related

- [Retention](../language/15-personal-data.md#retention)
- [Where the periods come from](../language/15-personal-data.md#where-the-periods-come-from)
- [Retention periods](../language/10-configuration-and-time.md#retention-periods)
- [`data.retention_since`](../errors/data.retention_since.md)
