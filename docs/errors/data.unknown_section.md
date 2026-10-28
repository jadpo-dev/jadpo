# `data.unknown_section`

An entity has no `identity:` item:

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
  reason: An entity's file holds only its data definition: fields (`name: Type`), and the sections `store:`, `unique:`, `indexes:`, `lifecycle:`, `caches:`, `policy:` and `membership:`. Its identity is always the field `id`; actions, queries and failures go in the domain's own folders.
  next: Remove it from `Note`, or move it where it belongs (agent)
  rule: data.unknown_section
  help: docs/language/05-data.html#data-unknown-section

check failed: 1 error, 1 file
```
