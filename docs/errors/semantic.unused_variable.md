# `semantic.unused_variable`

```jadpo check project=e-unused-variable expect=semantic.unused_variable
type Stock = Int {
    min: 0
}

function with_delivery(per_pack: Stock, delivery: Stock) -> Stock {
    var needed: Stock = per_pack
    return Stock(per_pack + delivery)
}
```

```text output
error: `needed` is never used
 --> main.jadpo:6:9
  |
6 |     var needed: Stock = per_pack
  |         ^^^^^^ never read
  reason: A value nobody reads is never bound to a name, so a reader never wonders what it was for.
  next: Use `needed`, or remove its `var` line (agent)
  rule: semantic.unused_variable
  help: docs/language/03-logic.html#semantic-unused-variable

check failed: 1 error, 1 file
```
