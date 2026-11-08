# `type.reserved_field_name`

A field has a name kept for something else, such as `optional`, which builds a type's fields.

## Why Jadpo refuses it

`include`, `pick`, `omit` and `optional` are lines that build a type's
fields, so a field with one of those names couldn't be told from them. On
an event, `reply` is refused the same way, because a webhook event a
package declares uses `reply:` to name the reply's type.

## What it looks like

main.jadpo:

```jadpo check project=e-reserved expect=type.reserved_field_name
type Flag = Bool {}

type Settings = Object {
    optional: Flag
}
```

```text output
error: `optional` can't be a field's name
 --> main.jadpo:4:5
  |
4 |     optional: Flag
  |     ^^^^^^^^ reserved
  reason: `include`, `pick`, `omit` and `optional` are lines that build a type's fields, so no field may take their names.
  next: Rename the field (agent)
  rule: type.reserved_field_name
  help: https://jadpo.dev/docs/errors/type.reserved_field_name

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the field needs a name of its own**, rename it to say what it holds:

```jadpo fragment
type Settings = Object {
    is_optional: Flag
}
```

```jadpo check project=e-reserved-fix hidden
type Flag = Bool {}

type Settings = Object {
    is_optional: Flag
}
```

Code that builds or reads the field uses the new name.

## Related

- [Names from reserved words](../language/02-values-and-types.md#names-from-reserved-words)
- [Building a type from other types](../language/02-values-and-types.md#building-a-type-from-other-types)
