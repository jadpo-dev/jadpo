# `data.unique_single_field`

A rule in `unique:` has only one field.

## Why Jadpo refuses it

The `unique:` section is for rules over several fields. A single field is
made unique in its own block, `unique: true`, so there is one place to
look for it and one form for writing it.

## What it looks like

main.jadpo:

```jadpo check project=e-unique-single expect=data.unique_single_field
entity Account {
    id: Uuid
    handle: Text

    unique: {
        only_handle: [handle]
    }
}
```

```text output
error: The unique rule `only_handle` has one field
 --> main.jadpo:6:9
  |
6 |         only_handle: [handle]
  |         ^^^^^^^^^^^ one field
  reason: The `unique:` section holds rules over several fields; a single field is made unique in its own block, `handle: ... { unique: true }`.
  next: Move it to `handle`'s block as `unique: true` (agent)
  rule: data.unique_single_field
  help: https://jadpo.dev/docs/errors/data.unique_single_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field is unique on its own**, say so in its block and remove the
rule:

```jadpo check project=e-unique-single-fix
entity Account {
    id: Uuid
    handle: Text {
        unique: true
    }
}
```

No two accounts can now have the same handle.

## Related

- [Unique rules](../language/05-data.md#unique-rules)
