# `type.not_record`

Braces build a value of a type that has no fields, such as one built on `Text`, or of one only the runtime builds, or `include` or `pick` copies from a type with no fields.

## Why Jadpo refuses it

Braces hold a value's fields, so they build only types that have them:
objects, inputs, outputs, events, failures and enum variants with fields.
A type built on `Text`, a number or a time holds one value, given in
brackets. Some types with fields are built only by the runtime, such as a
`File` that a store kept, so code never makes one up.

## What it looks like

main.jadpo:

```jadpo check project=e-not-record expect=type.not_record
type ProductName = Text {
    max_length: 120
}

function blank() -> ProductName {
    return ProductName {}
}
```

```text output
error: `ProductName` isn't built with `{ ... }`
 --> main.jadpo:6:12
  |
6 |     return ProductName {}
  |            ^^^^^^^^^^^^^^ has no fields
  reason: Braces build objects and variants with fields; `ProductName`, built on `Text`, takes one value in brackets.
  next: Write `ProductName(...)` with one value (agent)
  rule: type.not_record
  help: https://jadpo.dev/docs/errors/type.not_record

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Copying fields from a type that has none is the same error:

main.jadpo:

```jadpo check project=e-not-record-copy expect=type.not_record
type ProductName = Text {
    max_length: 120
}

type ProductView = Output {
    include: [ProductName]
}
```

```text output
error: `ProductName` has no fields to copy
 --> main.jadpo:6:15
  |
6 |     include: [ProductName]
  |               ^^^^^^^^^^^ no fields
  reason: `include` and `pick` copy fields from objects, inputs, outputs, events and entities.
  next: Remove `ProductName` from the list (agent)
  rule: type.not_record
  help: https://jadpo.dev/docs/errors/type.not_record

check failed: 1 error, 1 file
```

Its fix is to remove `ProductName` from the list, and to write a field
out, as `name: ProductName`, if the output needs one.

## How to fix it

**If the type holds one value**, build it with that value in brackets:

```jadpo fragment
function blank() -> ProductName {
    return ProductName("")
}
```

```jadpo check project=e-not-record-fix hidden
type ProductName = Text {
    max_length: 120
}

function blank() -> ProductName {
    return ProductName("")
}
```

`blank` gives a `ProductName` holding empty text.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Objects](../language/02-values-and-types.md#objects)
- [Building a type from other types](../language/02-values-and-types.md#building-a-type-from-other-types)
