# `data.identity_type`

An entity's `id` isn't a required `Uuid`, or has data settings of its own.

## Why Jadpo refuses it

An entity's `id` is the one thing every link to its records is built on,
so it is always a `Uuid`, or a named type built on one, and it always
exists. It is also always unique and generated on create, so there is
nothing for `unique:`, `generated:` or `on_delete:` to add.

## What it looks like

main.jadpo:

```jadpo check project=e-identity-type expect=data.identity_type
entity Note {
    id: Text
}
```

```text output
error: `Note.id` must be a required `Uuid`
 --> main.jadpo:2:5
  |
2 |     id: Text
  |     ^^ not a `Uuid`
  reason: An entity's `id` is a `Uuid`, or a named type built on one; links between entities are references to it.
  next: Write `id: Uuid` (agent)
  rule: data.identity_type
  help: https://jadpo.dev/docs/errors/data.identity_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An `id` that can be `none` is refused the same way:

```jadpo check project=e-identity-nullable expect=data.identity_type
entity Note {
    id: Uuid?
}
```

```text output
error: `Note.id` must be a required `Uuid`
 --> main.jadpo:2:5
  |
2 |     id: Uuid?
  |     ^^ can be `none`
  reason: An identity always exists, so `id` can't be optional.
  next: Write `id: Uuid` (agent)
  rule: data.identity_type
  help: https://jadpo.dev/docs/errors/data.identity_type

check failed: 1 error, 1 file
```

So is an `id` with data settings:

```jadpo check project=e-identity-settings expect=data.identity_type
entity Note {
    id: Uuid {
        unique: true
    }
}
```

```text output
error: `Note.id` must be a required `Uuid`
 --> main.jadpo:2:5
  |
2 |     id: Uuid {
  |     ^^ has data settings
  reason: `id` is always unique and generated on create, so it takes no `unique:`, `generated:` or `on_delete:`.
  next: Write `id: Uuid` (agent)
  rule: data.identity_type
  help: https://jadpo.dev/docs/errors/data.identity_type

check failed: 1 error, 1 file
```

## How to fix it

**If the field is the record's identity**, write it as a plain `Uuid`:

```jadpo check project=e-identity-type-fix-1
entity Note {
    id: Uuid
}
```

Each new note now gets a generated `id`.

**If you also look records up by a value of your own**, keep that in a
unique field beside `id`:

```jadpo fragment
    code: Text {
        unique: true
    }
```

```jadpo check project=e-identity-type-fix-2 hidden
entity Note {
    id: Uuid
    code: Text {
        unique: true
    }
}
```

`code` now identifies a note to people, and `id` is what other records
refer to.

## Related

- [Entities and the `id`](../language/05-data.md#entities-and-the-id)
