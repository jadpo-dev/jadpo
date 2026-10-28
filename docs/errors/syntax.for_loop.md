# `syntax.for_loop`

A loop is `for item in items {`, without brackets. A counting loop has no Jadpo form, so its error has no edit.

```jadpo check project=e-habit-for expect=syntax.for_loop
type Stock = Int {
    min: 0
}

type OrderLine = Object {
    needed: Stock
}

function stock_total(lines: List<OrderLine>) -> Stock {
    var mut total: Stock = 0
    for (const line of lines) {
        total = Stock(total + line.needed)
    }
    return total
}
```

```text output
error: Jadpo writes `for line in lines`, not `for (const line of lines)`
 --> main.jadpo:11:9
   |
11 |     for (const line of lines) {
   |         ^^^^^^^^^^^^^^^^^^^^^ write `for line in lines`
  reason: `for (const item of items)` walks a list in JavaScript and TypeScript, `for (item in items)` in Kotlin, `for (Type item : items)` in Java and `foreach (var item in items)` in C#; Jadpo writes `for item in items {`, without brackets.
  next: Write `for line in lines` (agent)
  rule: syntax.for_loop
  help: docs/language/03-logic.html#syntax-for-loop

check failed: 1 error, 1 file
```
