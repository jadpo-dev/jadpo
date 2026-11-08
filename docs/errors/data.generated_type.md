# `data.generated_type`

A field with `generated:` isn't a required `Instant`.

## Why Jadpo refuses it

A generated field is stamped by the store with the operation's clock, on
create or on every change. What the clock gives is a time, so the field
has to be an `Instant` and a required one: every record has that time, and
`none` would never be written.

## What it looks like

main.jadpo:

```jadpo check project=e-generated-type expect=data.generated_type
entity Note {
    id: Uuid
    created_on: CalendarDate {
        generated: Generated.on_create
    }
}
```

```text output
error: `Note.created_on` can't be generated
 --> main.jadpo:4:9
  |
4 |         generated: Generated.on_create
  |         ^^^^^^^^^ not an `Instant`
  reason: A generated field is a required `Instant` set from the operation's clock, on create or on every change; code never assigns it.
  next: Type `created_on` as `Instant`, or remove `generated:` (agent)
  rule: data.generated_type
  help: https://jadpo.dev/docs/errors/data.generated_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A generated field that can be `none` is refused the same way:

```jadpo check project=e-generated-nullable expect=data.generated_type
entity Note {
    id: Uuid
    created_at: Instant? {
        generated: Generated.on_create
    }
}
```

```text output
error: `Note.created_at` can't be generated
 --> main.jadpo:4:9
  |
4 |         generated: Generated.on_create
  |         ^^^^^^^^^ can be `none`
  reason: A generated field is a required `Instant` set from the operation's clock, on create or on every change; code never assigns it.
  next: Make `created_at` required, or remove `generated:` (agent)
  rule: data.generated_type
  help: https://jadpo.dev/docs/errors/data.generated_type

check failed: 1 error, 1 file
```

## How to fix it

**If the store should stamp the time**, make the field a required
`Instant`:

```jadpo fragment
    created_at: Instant {
        generated: Generated.on_create
    }
```

```jadpo check project=e-generated-type-fix-1 hidden
entity Note {
    id: Uuid
    created_at: Instant {
        generated: Generated.on_create
    }
}
```

Each new note now gets the time it was created, and code never sets it.

**If your code decides the value**, remove `generated:`:

```jadpo fragment
    created_on: CalendarDate
```

```jadpo check project=e-generated-type-fix-2 hidden
entity Note {
    id: Uuid
    created_on: CalendarDate
}
```

`created_on` is now set by the write that creates a note, like any other
field.

## Related

- [Fields and their settings](../language/05-data.md#fields-and-their-settings)
