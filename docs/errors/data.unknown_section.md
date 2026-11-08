# `data.unknown_section`

An entity holds something that is neither a field nor a section it takes.

## Why Jadpo refuses it

An entity's file is only its data definition: its fields, and the sections
`store:`, `unique:`, `indexes:`, `lifecycle:`, `caches:`, `policy:`,
`membership:`, `person:`, `retention:`, `search:` and `audit:`. An
entity's identity is always its `id` field, so `identity:` has nothing to
say. Actions, queries and failures go in the domain's own folders, so
anything else in the file is either a slip or in the wrong place.

## What it looks like

main.jadpo:

```jadpo check project=e-unknown-section expect=data.unknown_section
entity Note {
    id: Uuid

    identity: id
}
```

```text output
error: `identity` isn't something an entity holds
 --> main.jadpo:4:5
  |
4 |     identity: id
  |     ^^^^^^^^ not a field or a section
  reason: An entity's file holds only its data definition: fields (`name: Type`), and the sections `store:`, `unique:`, `indexes:`, `lifecycle:`, `caches:`, `policy:`, `membership:`, `person:`, `retention:`, `search:` and `audit:`. Its identity is always the field `id`; actions, queries and failures go in the domain's own folders.
  next: Remove it from `Note`, or move it where it belongs (agent)
  rule: data.unknown_section
  help: https://jadpo.dev/docs/errors/data.unknown_section

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A misspelt section is refused the same way, and the error names the
section it most likely means:

```jadpo check project=e-unknown-section-misspelt expect=data.unknown_section
entity Note {
    id: Uuid
    written_at: Instant

    indexs: {
        by_written_at: [written_at]
    }
}
```

```text output
error: `indexs` isn't something an entity holds
 --> main.jadpo:5:5
  |
5 |     indexs: {
  |     ^^^^^^ not a field or a section
  reason: An entity's file holds only its data definition: fields (`name: Type`), and the sections `store:`, `unique:`, `indexes:`, `lifecycle:`, `caches:`, `policy:`, `membership:`, `person:`, `retention:`, `search:` and `audit:`. Its identity is always the field `id`; actions, queries and failures go in the domain's own folders. Did you mean `indexes`?
  next: Use `indexes` (agent)
  rule: data.unknown_section
  help: https://jadpo.dev/docs/errors/data.unknown_section

check failed: 1 error, 1 file
```

## How to fix it

**If it doesn't belong in an entity**, remove it, or move it to the file
it belongs in:

```jadpo check project=e-unknown-section-fix-1
entity Note {
    id: Uuid
}
```

`id` is now the identity of `Note`, as it always is.

**If you misspelt a section**, use its name:

```jadpo fragment
    indexes: {
        by_written_at: [written_at]
    }
```

```jadpo check project=e-unknown-section-fix-2 hidden
entity Note {
    id: Uuid
    written_at: Instant

    indexes: {
        by_written_at: [written_at]
    }
}
```

The database now keeps the index.

## Related

- [Entities and the `id`](../language/05-data.md#entities-and-the-id)
- [Fields and their settings](../language/05-data.md#fields-and-their-settings)
