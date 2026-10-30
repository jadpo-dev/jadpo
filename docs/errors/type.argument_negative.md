# `type.argument_negative`

A count or a position is never negative: `list.at`'s
`position`, `slice`'s `from` and `count`, and the `n` of `keep_first`,
`keep_last`, `drop_first` and `drop_last`. A literal isn't below `0` and a
value's type has a `min` of at least `0`, so an off-by-one never reads
from the other end, as a negative position does in JavaScript.

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
