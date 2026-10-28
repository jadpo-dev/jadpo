# `syntax.return_type_arrow`

The result type follows `->`.

```jadpo check project=e-habit-colon-result expect=syntax.return_type_arrow
type Count = Int {
    min: 0
}

function same(count: Count): Count {
    return count
}
```

```text output
error: Jadpo writes the result type after `->`, not `:`
 --> main.jadpo:5:28
  |
5 | function same(count: Count): Count {
  |                            ^ write `->`
  reason: `: Type` after the parameters is the result type in TypeScript and Kotlin; Jadpo writes `-> Type`, as in `function total(order: Order) -> Amount {`.
  next: Use `->` instead of `:` (agent)
  rule: syntax.return_type_arrow
  help: docs/language/03-logic.html#syntax-return-type-arrow

check failed: 1 error, 1 file
```
