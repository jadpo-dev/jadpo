# `syntax.variable_type_required`

Every variable states its type, and a literal takes it:

```jadpo check project=e-variable-type expect=syntax.variable_type_required
type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}

function free() -> ItemPrice {
    var price = ItemPrice(0.00)
    return price
}
```

```text output
error: `price` needs a type
 --> main.jadpo:7:9
  |
7 |     var price = ItemPrice(0.00)
  |         ^^^^^ expected `: Type` after the name
  reason: Every variable states its type, so a reader sees it where the value is introduced.
  next: Write `var price: ItemPrice = 0.00` (agent)
  rule: syntax.variable_type_required
  help: docs/language/02-values-and-types.html#syntax-variable-type-required

check failed: 1 error, 1 file
```
