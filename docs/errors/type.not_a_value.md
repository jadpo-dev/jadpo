# `type.not_a_value`

```jadpo check project=e-not-a-value expect=type.not_a_value
type Count = Int {
    min: 0
}

type Placed = Object {
    count: Count
}

function first_placed() -> Placed {
    var placed: Placed = Placed
    return placed
}
```

```text output
error: `Placed` is a type, not a value
 --> main.jadpo:10:26
   |
10 |     var placed: Placed = Placed
   |                          ^^^^^^ a type
  reason: A type says what a value looks like and holds none itself; a value is built with the type's name and every field in a block.
  next: Build a value with `Placed { ... }`, giving each field (agent)
  rule: type.not_a_value
  help: docs/errors/type.not_a_value.md

check failed: 1 error, 1 file
```
