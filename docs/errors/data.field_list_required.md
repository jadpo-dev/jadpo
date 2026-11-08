# `data.field_list_required`

A `unique:` or `indexes:` section isn't a block of named lists, each of at least one of the entity's fields.

## Why Jadpo refuses it

`unique:` and `indexes:` are blocks of named rules, each a list of the
entity's own fields, in brackets, by name. The database's constraint or
index is built from exactly that list, so Jadpo reads the fields from it
and accepts nothing else in its place.

## What it looks like

main.jadpo:

```jadpo check project=e-field-list expect=data.field_list_required
entity Project {
    id: Uuid
    keeper: Text
    name: Text

    unique: {
        name_per_keeper: keeper
    }
}
```

```text output
error: The unique rule `name_per_keeper` isn't a list of fields
 --> main.jadpo:7:26
  |
7 |         name_per_keeper: keeper
  |                          ^^^^^^ not a list
  reason: Each unique rule is a list of the entity's fields in brackets: `name_per_keeper: [field, other]`.
  next: Write `name_per_keeper: [field, ...]` (agent)
  rule: data.field_list_required
  help: https://jadpo.dev/docs/errors/data.field_list_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The section itself has to be a block of named rules:

```jadpo check project=e-field-list-section expect=data.field_list_required
entity Project {
    id: Uuid
    keeper: Text
    name: Text

    unique: name
}
```

```text output
error: `unique:` holds named lists of fields
 --> main.jadpo:6:13
  |
6 |     unique: name
  |             ^^^^ not a block
  reason: `unique:` is a block of named unique rules, each a list of the entity's fields: `unique: { name: [field, other] }`.
  next: Write `unique: { ... }` (agent)
  rule: data.field_list_required
  help: https://jadpo.dev/docs/errors/data.field_list_required

check failed: 1 error, 1 file
```

Each entry in the list is one of the entity's fields, by its name alone:

```jadpo check project=e-field-list-entry expect=data.field_list_required
entity Project {
    id: Uuid
    keeper: Text
    name: Text

    unique: {
        name_per_keeper: [keeper, Project.name]
    }
}
```

```text output
error: The unique rule `name_per_keeper` lists something that isn't a field
 --> main.jadpo:7:35
  |
7 |         name_per_keeper: [keeper, Project.name]
  |                                   ^^^^^^^^^^^^ not a field's name
  reason: A unique rule lists the entity's own fields by name.
  next: Write the field's name (agent)
  rule: data.field_list_required
  help: https://jadpo.dev/docs/errors/data.field_list_required

check failed: 1 error, 1 file
```

A rule has at least one field:

```jadpo check project=e-field-list-empty expect=data.field_list_required
entity Project {
    id: Uuid
    keeper: Text

    indexes: {
        by_nothing: []
    }
}
```

```text output
error: The index `by_nothing` lists no fields
 --> main.jadpo:6:9
  |
6 |         by_nothing: []
  |         ^^^^^^^^^^ empty
  reason: An index names at least one of the entity's fields.
  next: List its fields, or remove `by_nothing` (agent)
  rule: data.field_list_required
  help: https://jadpo.dev/docs/errors/data.field_list_required

check failed: 1 error, 1 file
```

## How to fix it

**If the rule covers several fields**, list them by name, in brackets,
under the rule's name:

```jadpo fragment
    unique: {
        name_per_keeper: [keeper, name]
    }
```

```jadpo check project=e-field-list-fix-1 hidden
entity Project {
    id: Uuid
    keeper: Text
    name: Text

    unique: {
        name_per_keeper: [keeper, name]
    }
}
```

One keeper can now have only one project of a given name.

**If one field is unique on its own**, say so in the field's block:

```jadpo fragment
    name: Text {
        unique: true
    }
```

```jadpo check project=e-field-list-fix-2 hidden
entity Project {
    id: Uuid
    keeper: Text
    name: Text {
        unique: true
    }
}
```

No two projects can now have the same name.

**If the rule has no fields to give**, remove it:

```jadpo check project=e-field-list-fix-3
entity Project {
    id: Uuid
    keeper: Text
}
```

The entity no longer declares an index that lists nothing.

## Related

- [Unique rules](../language/05-data.md#unique-rules)
- [Indexes](../language/05-data.md#indexes)
