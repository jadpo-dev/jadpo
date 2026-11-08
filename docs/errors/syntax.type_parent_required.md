# `syntax.type_parent_required`

A type is declared without a base before its block.

## Why Jadpo refuses it

Every type is written `type Name = Base { ... }`, and the base says what it
is built from, such as `Object`, `Enum`, `Input` or `Text`. Jadpo can't tell
that from the block alone, since a value of an `Object` and an `Input` are
written with the same fields and mean different things.

## What it looks like

main.jadpo:

```jadpo check project=e-parent expect=syntax.type_parent_required
type BoxSize = {
    width: Int
}
```

```text output
error: `BoxSize` needs a base type
 --> main.jadpo:1:16
  |
1 | type BoxSize = {
  |                ^ expected a base before `{`
  reason: Every type is written `type Name = Base { ... }`; the base says what it is built from, such as `Object`, `Enum`, `Input` or `Text`.
  next: Add the base before `{`, for example `type BoxSize = Object {` (agent)
  rule: syntax.type_parent_required
  help: https://jadpo.dev/docs/errors/syntax.type_parent_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it holds fields**, add `Object` before the block:

```jadpo check project=e-parent-fix
type Width = Int {
    min: 1
}

type BoxSize = Object {
    width: Width
}
```

`BoxSize` is now an object type with one field. Use `Input` or `Output` for a
request's or a response's shape, or `Enum` for a list of cases.

## Related

- [One form for every type](../language/02-values-and-types.md#one-form-for-every-type)
- [Objects](../language/02-values-and-types.md#objects)
