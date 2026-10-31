# `data.retention_field_required`

A field's `retention:` is on a field that can't be `none`, so its value could never be cleared.

## Why Jadpo refuses it

A `retention:` on a field clears its value when its time is up and keeps
the record. A field whose type doesn't end in `?` must always hold a
value, so there is nothing to clear it to. Here `signup_ip` is an
`IpAddress`, which can't be `none`, so the address would be kept for
ever, however long the retention says.

## What it looks like

main.jadpo:

```jadpo check project=e-retention-field-required expect=data.retention_field_required
config {
    signup_ip_retention: Duration {
        default: 30d
    }
}

entity Visitor {
    id: Uuid
    signup_ip: IpAddress {
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
error: `Visitor.signup_ip` can't be cleared
 --> main.jadpo:10:9
   |
10 |         retention: {
   |         ^^^^^^^^^ on a field that can't be `none`
  reason: A field's `retention:` clears its value when its time is up, which only an optional field (`IpAddress?`) can be without; `retention:` on the entity deletes the whole record instead.
  next: Make it `IpAddress?` (agent)
  rule: data.retention_field_required
  help: https://jadpo.dev/docs/errors/data.retention_field_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the address should go and the visitor stay**, make the field one
that can be `none`:

```jadpo fragment
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

```jadpo check project=e-retention-field-required-fix hidden
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

`signup_ip` is now `none` once the visitor signed up longer ago than
`signup_ip_retention`, and code reading it handles a visitor without
one. To delete the whole visitor instead, the `retention:` goes on the
entity ([On an entity](../language/15-personal-data.md#on-an-entity)).

## Related

- [Retention](../language/15-personal-data.md#retention)
- [On a field](../language/15-personal-data.md#on-a-field)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
