# `type.foreign_type`

A type is named as another language names it, such as `string` or `int`, and the project declares no type of that name.

## Why Jadpo refuses it

Jadpo names its built-in types once each: `Text`, `Int`, `Bool`, `Unit`,
`List`, `Map`, and a value that may be missing as `T?`. Writing `string`,
`int`, `bool`, `void`, `Vec`, `HashMap` or `Option<T>` out of habit names
nothing, so Jadpo says which of its own types you meant. A name your
project declares is your own type and never this error.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-foreign-type expect=type.foreign_type
type Listing = Object {
    name: string
}
```

```text output
error: Jadpo writes `Text`, not `string`
 --> main.jadpo:2:11
  |
2 |     name: string
  |           ^^^^^^ write `Text`
  reason: `string` is text in TypeScript, C# and Go; Jadpo calls it `Text`.
  next: Use `Text` instead of `string` (agent)
  rule: type.foreign_type
  help: https://jadpo.dev/docs/errors/type.foreign_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant Jadpo's own type**, write the name the error gives:

```jadpo check project=e-habit-foreign-type-fix
type Listing = Object {
    name: Text
}
```

`name` holds any text. A named type built on `Text` can add rules, such as
a `max_length`.

## Related

- [Built-in types](../language/02-values-and-types.md#built-in-types)
- [Values that may be missing](../language/02-values-and-types.md#values-that-may-be-missing)
- [Habits from other languages](../language/02-values-and-types.md#habits-from-other-languages)
