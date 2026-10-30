# `type.mismatch`

```jadpo check project=e-mismatch expect=type.mismatch
type Stock = Int {
    min: 0
}

function add(a: Stock, b: Stock) -> Stock {
    return a + b
}
```

```text output
error: `Int` can't be used where `Stock` is required
 --> main.jadpo:6:12
  |
6 |     return a + b
  |            ^^^^^ this is `Int`
  reason: Arithmetic works on the representation, so this is a plain `Int`; only `Stock(...)` makes it a `Stock`, checking its rules.
  next: Convert it with `Stock(a + b)` (agent)
  rule: type.mismatch
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file
```
