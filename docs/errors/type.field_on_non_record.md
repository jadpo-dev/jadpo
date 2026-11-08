# `type.field_on_non_record`

Code reads a field, as in `name.value`, from a value whose type has no fields, such as one built on `Text`.

## Why Jadpo refuses it

Only objects, inputs, outputs, events, failures and entities have fields.
A type built on `Text`, a number or a time holds one value, and the value
is the thing itself: there is no `.value` or `.text` to unwrap. Reading a
field from one usually comes from a language where a wrapper type keeps
its value in a field.

## What it looks like

main.jadpo:

```jadpo check project=e-field-scalar expect=type.field_on_non_record
type ProductName = Text {
    max_length: 120
}

function raw(name: ProductName) -> ProductName {
    return name.value
}
```

```text output
error: `ProductName` has no fields
 --> main.jadpo:6:17
  |
6 |     return name.value
  |                 ^^^^^ no such field
  reason: `name` is a `ProductName`, built on `Text`; only objects, inputs, outputs, events, failures and entities have fields.
  next: Use `name` itself (agent)
  rule: type.field_on_non_record
  help: https://jadpo.dev/docs/errors/type.field_on_non_record

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `File` and a `FileContents` have no fields a program reads either, even
the ones a `File` keeps, so `sheet.key` is the same error, in code and in
an email's template.

## How to fix it

**If you meant the value itself**, use it as it is:

```jadpo fragment
function raw(name: ProductName) -> ProductName {
    return name
}
```

```jadpo check project=e-field-scalar-fix hidden
type ProductName = Text {
    max_length: 120
}

function raw(name: ProductName) -> ProductName {
    return name
}
```

`raw` gives the name it was passed, which is already a `ProductName`.

## Related

- [Named types](../language/02-values-and-types.md#named-types)
- [Objects](../language/02-values-and-types.md#objects)
- [What a `File` holds](../language/16-built-in-services.md#what-a-file-holds)
