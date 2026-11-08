# `data.repeated_field`

A unique rule or an index lists the same field twice.

## Why Jadpo refuses it

A unique rule or an index over a field already covers it, so listing it
again adds nothing, and a second name there is more likely a slip for
another field.

## What it looks like

main.jadpo:

```jadpo check project=e-repeated-field expect=data.repeated_field
entity Todo {
    id: Uuid
    due_at: Instant

    indexes: {
        by_due_at: [due_at, due_at]
    }
}
```

```text output
error: The index `by_due_at` lists `due_at` twice
 --> main.jadpo:6:29
  |
6 |         by_due_at: [due_at, due_at]
  |                             ^^^^^^ listed again
  reason: Each field counts once in an index.
  next: Remove the second `due_at` (agent)
  rule: data.repeated_field
  help: https://jadpo.dev/docs/errors/data.repeated_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you listed the field by mistake**, remove the second one:

```jadpo fragment
    indexes: {
        by_due_at: [due_at]
    }
```

```jadpo check project=e-repeated-field-fix-1 hidden
entity Todo {
    id: Uuid
    due_at: Instant

    indexes: {
        by_due_at: [due_at]
    }
}
```

The index now covers `due_at` once.

**If you meant another field**, list that one:

```jadpo fragment
    indexes: {
        by_due_at: [due_at, created_at]
    }
```

```jadpo check project=e-repeated-field-fix-2 hidden
entity Todo {
    id: Uuid
    due_at: Instant
    created_at: Instant {
        generated: Generated.on_create
    }

    indexes: {
        by_due_at: [due_at, created_at]
    }
}
```

The index now orders todos by `due_at`, then by `created_at`.

## Related

- [Indexes](../language/05-data.md#indexes)
- [Unique rules](../language/05-data.md#unique-rules)
