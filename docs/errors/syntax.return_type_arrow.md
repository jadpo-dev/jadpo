# `syntax.return_type_arrow`

A signature's result type is written after `:` instead of `->`.

## Why Jadpo refuses it

`: Type` after the parameters is the result type in TypeScript and Kotlin.
Jadpo writes `-> Type`, and keeps `:` for what comes before a value's type
or a member's value, so a colon never has to be told apart from a result.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/syntax.return_type_arrow

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you were giving the result type**, write `->`:

```jadpo fragment
function same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-habit-colon-result-fix hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}
```

`same` now declares that it gives back a `Count`.

## Related

- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
