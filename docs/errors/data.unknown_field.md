# `data.unknown_field`

A unique rule or an index names a field the entity doesn't have.

## Why Jadpo refuses it

A unique rule or an index is built from the entity's own columns, so every
name in its list has to be one of the entity's fields. A name that matches
nothing would have no column behind it.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-field expect=data.unknown_field
entity Project {
    id: Uuid
    name: Text

    unique: {
        name_per_owner: [owner_id, name]
    }
}
```

```text output
error: `Project` has no field `owner_id`
 --> main.jadpo:6:26
  |
6 |         name_per_owner: [owner_id, name]
  |                          ^^^^^^^^ no such field
  reason: The unique rule `name_per_owner` names fields of `Project`, its own.
  next: Remove `owner_id` (agent)
  rule: data.unknown_field
  help: https://jadpo.dev/docs/errors/data.unknown_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An index that names a field the entity doesn't have is refused the same
way:

```jadpo check project=e-unknown-field-index expect=data.unknown_field
entity Todo {
    id: Uuid
    due_at: Instant

    indexes: {
        by_owner: [owner_id, due_at]
    }
}
```

```text output
error: `Todo` has no field `owner_id`
 --> main.jadpo:6:20
  |
6 |         by_owner: [owner_id, due_at]
  |                    ^^^^^^^^ no such field
  reason: The index `by_owner` names fields of `Todo`, its own.
  next: Remove `owner_id` (agent)
  rule: data.unknown_field
  help: https://jadpo.dev/docs/errors/data.unknown_field

check failed: 1 error, 1 file
```

## How to fix it

**If the entity should have the field**, add it:

```jadpo fragment
    owner_id: Text
```

```jadpo check project=e-unknown-field-fix-1 hidden
entity Project {
    id: Uuid
    owner_id: Text
    name: Text

    unique: {
        name_per_owner: [owner_id, name]
    }
}
```

The rule now means one owner has one project of a given name.

**If the rule is unnecessary**, remove the name, and make what is left a
rule of its own:

```jadpo fragment
    name: Text {
        unique: true
    }
```

```jadpo check project=e-unknown-field-fix-2 hidden
entity Project {
    id: Uuid
    name: Text {
        unique: true
    }
}
```

No two projects can now have the same name.

## Related

- [Unique rules](../language/05-data.md#unique-rules)
- [Indexes](../language/05-data.md#indexes)
