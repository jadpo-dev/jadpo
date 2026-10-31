# `data.retention_since`

A retention's `since:` isn't one of the record's `Instant` fields.

## Why Jadpo refuses it

A retention keeps a value or a record for its `after:` period, counted
from a moment the record holds: when it was created, issued or last
seen. `since:` names that moment, so it must be one of the record's own
`Instant` fields. Here it names `name`, which is text, so there is no
time to count from and nothing would ever expire.

## What it looks like

main.jadpo:

```jadpo check project=e-retention-since expect=data.retention_since
config {
    visitor_retention: Duration {
        default: 365d
    }
}

type VisitorName = Text {
    max_length: 80
}

entity Visitor {
    id: Uuid
    name: VisitorName
    created_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.visitor_retention
        since: name
    }
}
```

```text output
error: `name` isn't a time
 --> main.jadpo:20:16
   |
20 |         since: name
   |                ^^^^ a `VisitorName`
  reason: `since:` names the `Instant` field a retention counts from: a value or record goes once that time is longer ago than `after:`. `Visitor`'s `Instant` fields are `created_at`.
  next: Name an `Instant` field of the record (agent)
  rule: data.retention_since
  help: https://jadpo.dev/docs/errors/data.retention_since

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `since:` naming a field the record doesn't have is refused the same
way, with the field it most likely means:

```jadpo check project=e-retention-since-unknown expect=data.retention_since
config {
    visitor_retention: Duration {
        default: 365d
    }
}

entity Visitor {
    id: Uuid
    created_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.visitor_retention
        since: created
    }
}
```

```text output
error: `Visitor` has no field `created`
 --> main.jadpo:15:16
   |
15 |         since: created
   |                ^^^^^^^ not declared
  reason: `since:` names one of `Visitor`'s `Instant` fields, which are `created_at`. Did you mean `created_at`?
  next: Use `created_at` instead of `created` (agent)
  rule: data.retention_since
  help: https://jadpo.dev/docs/errors/data.retention_since

check failed: 1 error, 1 file
```

## How to fix it

**If the period counts from when the visitor was created**, name that
`Instant` field:

```jadpo fragment
entity Visitor {
    ...
    retention: {
        after: config.visitor_retention
        since: created_at
    }
}
```

```jadpo check project=e-retention-since-fix hidden
config {
    visitor_retention: Duration {
        default: 365d
    }
}

type VisitorName = Text {
    max_length: 80
}

entity Visitor {
    id: Uuid
    name: VisitorName
    created_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.visitor_retention
        since: created_at
    }
}
```

Each visitor is now deleted at the first hourly run once they were
created longer ago than `visitor_retention`.

## Related

- [Retention](../language/15-personal-data.md#retention)
- [Where the periods come from](../language/15-personal-data.md#where-the-periods-come-from)
- [`data.retention_period`](../errors/data.retention_period.md)
