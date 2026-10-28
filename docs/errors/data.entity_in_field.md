# `data.entity_in_field`

```jadpo check project=e-entity-in-field expect=data.entity_in_field
entity User {
    id: Uuid
}

entity Todo {
    id: Uuid
    keeper: User
}
```

```text output
error: `Todo.keeper` holds a whole `User`
 --> main.jadpo:7:13
  |
7 |     keeper: User
  |             ^^^^ a `User` record
  reason: An entity value is a snapshot, never stored inside another record; a field refers to a record by its identity, `User.id`, which the database keeps as a foreign key.
  next: Store `User.id` instead (agent)
  rule: data.entity_in_field
  help: docs/language/05-data.html#data-entity-in-field

check failed: 1 error, 1 file
```
