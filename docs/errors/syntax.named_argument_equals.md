# `syntax.named_argument_equals`

A named argument is written with `=` instead of `:`.

## Why Jadpo refuses it

`name = value` names an argument in Python, Kotlin and C#. Jadpo writes
`name: value`, the form a member has inside braces, so a named value looks
the same in both places and `=` only ever means assigning to a variable.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-named-equals expect=syntax.named_argument_equals
type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}

type People = Int {
    min: 1
}

function share(total: ItemPrice, people: People) -> ItemPrice {
    return ItemPrice(decimal.divide(total, people, rounding = Rounding.half_even))
}
```

```text output
error: Jadpo names an argument with `:`, not `=`
 --> main.jadpo:11:61
   |
11 |     return ItemPrice(decimal.divide(total, people, rounding = Rounding.half_even))
   |                                                             ^ write `:`
  reason: `name = value` names an argument in Python, Kotlin and C#; Jadpo writes `name: value`.
  next: Use `:` instead of `=` (agent)
  rule: syntax.named_argument_equals
  help: https://jadpo.dev/docs/errors/syntax.named_argument_equals

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a named argument**, write `:`:

```jadpo fragment
function share(total: ItemPrice, people: People) -> ItemPrice {
    return ItemPrice(decimal.divide(total, people, rounding: Rounding.half_even))
}
```

```jadpo check project=e-habit-named-equals-fix hidden
type ItemPrice = Decimal {
    scale: 2
    min: 0.00
}

type People = Int {
    min: 1
}

function share(total: ItemPrice, people: People) -> ItemPrice {
    return ItemPrice(decimal.divide(total, people, rounding: Rounding.half_even))
}
```

`decimal.divide` now rounds the share with `Rounding.half_even`.

## Related

- [Calling](../language/03-logic.md#calling)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
