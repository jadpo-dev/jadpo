# `type.foreign_value`

A value is written as another language writes it, such as `null` for `none` or Python's `True` for `true`.

## Why Jadpo refuses it

Jadpo writes a missing value one way, `none`, and the two truth values as
`true` and `false`. `null`, `nil`, `undefined`, `None`, `NULL`, `True` and
`False` come from other languages and name nothing here, so Jadpo says
which of its own values you meant.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-foreign-value expect=type.foreign_value
type ProductName = Text {
    min_length: 1
}

function no_subtitle() -> ProductName? {
    return null
}
```

```text output
error: Jadpo writes `none`, not `null`
 --> main.jadpo:6:12
  |
6 |     return null
  |            ^^^^ write `none`
  reason: `null` is a missing value in JavaScript, Java, C# and Kotlin; Jadpo writes it `none`.
  next: Use `none` instead of `null` (agent)
  rule: type.foreign_value
  help: https://jadpo.dev/docs/errors/type.foreign_value

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant Jadpo's own value**, write the one the error gives:

```jadpo fragment
function no_subtitle() -> ProductName? {
    return none
}
```

```jadpo check project=e-habit-foreign-value-fix hidden
type ProductName = Text {
    min_length: 1
}

function no_subtitle() -> ProductName? {
    return none
}
```

`no_subtitle` gives a `ProductName?` that holds no name.

## Related

- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Habits from other languages](../language/02-values-and-types.md#habits-from-other-languages)
