# `syntax.nullable_type`

A type that may be missing is written in another language's form instead of `T?`.

## Why Jadpo refuses it

`T | null` and `T | undefined` are types that may be missing in TypeScript,
and `T | None` and `Optional[T]` in Python. Jadpo writes `T?`, which holds a
`T` or `none`, and has no `null` or `undefined` to tell apart.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-nullable expect=syntax.nullable_type
type ProductName = Text {
    min_length: 1
}

type Listing = Object {
    name: ProductName
    subtitle: ProductName | null
}
```

```text output
error: Jadpo writes `ProductName?`, not `ProductName | null`
 --> main.jadpo:7:27
  |
7 |     subtitle: ProductName | null
  |                           ^^^^^^ write `ProductName?`
  reason: `T | null` and `T | undefined` are types that may be missing in TypeScript, and `T | None` and `Optional[T]` in Python; Jadpo writes `T?`, which holds a `T` or `none`.
  next: Write `ProductName?` (agent)
  rule: syntax.nullable_type
  help: https://jadpo.dev/docs/errors/syntax.nullable_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Python's `Optional[T]` is the same error, reported on the brackets:

main.jadpo:

```jadpo check project=e-habit-nullable-optional expect=syntax.nullable_type
type ProductName = Text {
    min_length: 1
}

type Listing = Object {
    name: ProductName
    subtitle: Optional[ProductName]
}
```

```text output
error: Jadpo writes `ProductName?`, not `Optional[ProductName]`
 --> main.jadpo:7:23
  |
7 |     subtitle: Optional[ProductName]
  |                       ^^^^^^^^^^^^^ write `ProductName?`
  reason: `T | null` and `T | undefined` are types that may be missing in TypeScript, and `T | None` and `Optional[T]` in Python; Jadpo writes `T?`, which holds a `T` or `none`.
  next: Write `ProductName?` (agent)
  rule: syntax.nullable_type
  help: https://jadpo.dev/docs/errors/syntax.nullable_type

check failed: 1 error, 1 file
```

## How to fix it

**If the field may be missing**, write `?` after its type:

```jadpo fragment
type Listing = Object {
    name: ProductName
    subtitle: ProductName?
}
```

```jadpo check project=e-habit-nullable-fix hidden
type ProductName = Text {
    min_length: 1
}

type Listing = Object {
    name: ProductName
    subtitle: ProductName?
}
```

`subtitle` now holds a `ProductName` or `none`, and code that reads it
handles both.

## Related

- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Habits from other languages](../language/02-values-and-types.md#habits-from-other-languages)
