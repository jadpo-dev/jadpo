# `syntax.for_loop`

A loop is written in another language's form instead of `for item in items`.

## Why Jadpo refuses it

A Jadpo loop is `for item in items {`, without brackets, over a list, so it
always ends. `for (const item of items)` and its cousins say the same thing
with more words, and a counting loop can run for as long as its condition
holds, which is why it has no Jadpo form.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.for_loop

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A counting loop is this error too, and it has no edit to apply, because
there is no Jadpo form to rewrite it to:

main.jadpo:

```jadpo check project=e-habit-for-2 expect=syntax.for_loop
type Stock = Int {
    min: 0
}

function total_to(limit: Stock) -> Stock {
    var mut total: Stock = 0
    for (var i = 0; i < limit; i = i + 1) {
        total = Stock(total + 1)
    }
    return total
}
```

```text output
error: Jadpo has no counting `for` loop
 --> main.jadpo:7:9
  |
7 |     for (var i = 0; i < limit; i = i + 1) {
  |         ^ not a Jadpo loop
  reason: `for (i = 0; i < n; i++)` counts in C-family languages; Jadpo's only loop is `for item in items`, over a list, which always ends.
  next: Loop over the list itself with `for item in items` (agent)
  rule: syntax.for_loop
  help: https://jadpo.dev/docs/errors/syntax.for_loop

check failed: 1 error, 1 file
```

## How to fix it

**If you meant to walk a list**, write `for line in lines`:

```jadpo fragment
function stock_total(lines: List<OrderLine>) -> Stock {
    var mut total: Stock = 0
    for line in lines {
        total = Stock(total + line.needed)
    }
    return total
}
```

```jadpo check project=e-habit-for-fix-1 hidden
type Stock = Int {
    min: 0
}

type OrderLine = Object {
    needed: Stock
}

function stock_total(lines: List<OrderLine>) -> Stock {
    var mut total: Stock = 0
    for line in lines {
        total = Stock(total + line.needed)
    }
    return total
}
```

The loop now runs once for each order line.

**If you were counting through a list**, loop over the list itself:

```jadpo fragment
function stock_total(lines: List<Stock>) -> Stock {
    var mut total: Stock = 0
    for line in lines {
        total = Stock(total + line)
    }
    return total
}
```

```jadpo check project=e-habit-for-fix-2 hidden
type Stock = Int {
    min: 0
}

function stock_total(lines: List<Stock>) -> Stock {
    var mut total: Stock = 0
    for line in lines {
        total = Stock(total + line)
    }
    return total
}
```

The loop now ends after the last item of the list.

## Related

- [Loops](../language/03-logic.md#loops)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
