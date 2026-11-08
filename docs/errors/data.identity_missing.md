# `data.identity_missing`

An entity has no `id` field.

## Why Jadpo refuses it

Every entity's identity is one field, `id`, based on `Uuid`, which is what
makes each record a thing other code can refer to, as `Note.id`. Jadpo
generates the value when a record is created, so there is nothing for your
code to supply; it only needs the field to exist.

## What it looks like

main.jadpo:

```jadpo check project=e-identity-missing expect=data.identity_missing
entity Note {
    body: Text
}
```

```text output
error: `Note` has no `id` field
 --> main.jadpo:1:8
  |
1 | entity Note {
  |        ^^^^ no `id`
  reason: Every entity's identity is one field, `id`, based on `Uuid`: generated when a record is created and never supplied by callers, so other code can refer to it as `Entity.id`.
  next: Add `id: Uuid` as the entity's first field (agent)
  rule: data.identity_missing
  help: https://jadpo.dev/docs/errors/data.identity_missing

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the entity is missing its `id`**, add it as the first field:

```jadpo check project=e-identity-missing-fix
entity Note {
    id: Uuid
    body: Text
}
```

Each new note now gets a generated `id`, and other code can refer to it as
`Note.id`.

## Related

- [Entities and the `id`](../language/05-data.md#entities-and-the-id)
