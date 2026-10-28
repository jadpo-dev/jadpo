# `semantic.name_case`

```jadpo check project=e-case file=checkout.jadpo expect=semantic.name_case
type Stock = Int {
    min: 0
}

function orderTotal(subtotal: Stock) -> Stock {
    return subtotal
}
```

```text output
error: Function `orderTotal` isn't written in lower_snake_case
 --> checkout.jadpo:5:10
  |
5 | function orderTotal(subtotal: Stock) -> Stock {
  |          ^^^^^^^^^^ should be `order_total`
  reason: Types, entities and route groups are written in UpperCamelCase; functions, actions, queries, fields, variables and the rest in lower_snake_case.
  next: Rename `orderTotal` to `order_total`, here and where it's used (agent)
  rule: semantic.name_case
  help: docs/language/01-project-layout-and-modules.html#semantic-name-case

check failed: 1 error, 1 file
```
