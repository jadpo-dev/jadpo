# `type.argument_negative`

A list or text operation is given a count or a position that might be negative.

## Why Jadpo refuses it

A count or a position is never negative in Jadpo: `list.at`'s
`position`, `slice`'s `from` and `count`, and the `n` of `keep_first`,
`keep_last`, `drop_first` and `drop_last`. In JavaScript a negative
position counts back from the end, so an off-by-one quietly reads the
wrong item. Jadpo has no such meaning, so it proves the number's sign
while compiling.

A literal passes when it isn't below `0`. A value passes when its type
has a `min` of at least `0`. Anything else might be negative, so it is
refused.

## What it looks like

main.jadpo:

```jadpo check project=e-argument-negative expect=type.argument_negative
type BatchName = Text {
    min_length: 1
}

type TraysOut = Int {}

/// The batches still to bake once `done` of them are out of the oven.
function still_to_bake(bakings: List<BatchName>, done: TraysOut) -> List<BatchName> {
    return list.drop_first(bakings, done)
}
```

```text output
error: `done` may be negative
 --> main.jadpo:9:37
  |
9 |     return list.drop_first(bakings, done)
  |                                     ^^^^ `TraysOut` can be below `0`
  reason: `list.drop_first`'s `n` counts from the start, so it is never negative, which is proved while compiling: a literal isn't below `0` and a value's type has a `min` of at least `0`; `TraysOut` allows values below `0`.
  next: Pass a value whose type has `min: 0`, building it where a negative one is handled; count from the end with `keep_last` or `drop_last` (agent)
  rule: type.argument_negative
  help: https://jadpo.dev/docs/errors/type.argument_negative

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the count is never negative**, say so on its type with `min: 0`:

```jadpo fragment
type TraysOut = Int {
    min: 0
}
```

```jadpo check project=e-argument-negative-fix-1 hidden
type BatchName = Text {
    min_length: 1
}

type TraysOut = Int {
    min: 0
}

/// The batches still to bake once `done` of them are out of the oven.
function still_to_bake(bakings: List<BatchName>, done: TraysOut) -> List<BatchName> {
    return list.drop_first(bakings, done)
}
```

`done` can no longer be below `0`, so `drop_first` always drops from the
start. A `TraysOut` built from a computed number is checked against
`min: 0` where it is built, so a negative number is handled there.

A result such as `list.count`'s is a plain `Int`, which proves nothing
about its sign. Build it into a type with `min: 0` before you pass it.
To count from the end of a list, use `keep_last` or `drop_last` with a
count that isn't negative.

## Related

- [Lists](../language/19-standard-library.md#lists)
- [Constraints](../language/02-values-and-types.md#constraints)
- [Creating values](../language/02-values-and-types.md#creating-values)
