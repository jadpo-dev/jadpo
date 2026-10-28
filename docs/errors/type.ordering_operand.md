# `type.ordering_operand`

A nullable value has to be matched before it can be compared:

```jadpo check project=e-ordering expect=type.ordering_operand
type Count = Int {}

type Flag = Bool {}

function over_maximum(count: Count?, maximum: Count) -> Flag {
    return Flag(count > maximum)
}
```

```text output
error: `count` may be `none`, so it can't be ordered
 --> main.jadpo:6:17
  |
6 |     return Flag(count > maximum)
  |                 ^^^^^ may be `none`
  reason: `count` is `Count?`; `<`, `<=`, `>` and `>=` order two values, and `none` has no place in an order.
  next: Match on `count` and compare inside `some(...)` (agent)
  rule: type.ordering_operand
  help: docs/language/03-logic.html#type-ordering-operand

check failed: 1 error, 1 file
```
