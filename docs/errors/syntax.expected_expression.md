# `syntax.expected_expression`

A value is missing where one is needed.

## Why Jadpo refuses it

Jadpo gives a name its value where the name is written, on the same line, so
nothing can be read before it has one and a reader never has to look further
down for it. The same goes for anything else that has to hold a value: a
member after its colon, a `rule` after its colon, a `constant` after its `=`
and a `match` arm after its `->`.

## What it looks like

main.jadpo:

```jadpo check project=e-expression expect=syntax.expected_expression
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    var result: Count =
    return result
}
```

```text output
error: `result` has no value
 --> main.jadpo:6:23
  |
6 |     var result: Count =
  |                       ^ expected a value after `=`
  reason: A variable is given its value on the line that declares it: `var name: Type = value`.
  next: Write `result`'s value after `=` (agent)
  rule: syntax.expected_expression
  help: https://jadpo.dev/docs/errors/syntax.expected_expression

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A member with nothing after its colon is this error too, and so is a `rule`
with no condition, a `constant` with no value and a `match` arm with no
result, each naming what is missing:

main.jadpo:

```jadpo check project=e-expression-2 expect=syntax.expected_expression
type Count = Int {
    min:
}
```

```text output
error: `min:` has no value
 --> main.jadpo:2:8
  |
2 |     min:
  |        ^ expected a value after `:`
  reason: Every member inside braces is `name: value`, with the value on the same line.
  next: Write the value after `min:` (agent)
  rule: syntax.expected_expression
  help: https://jadpo.dev/docs/errors/syntax.expected_expression

check failed: 1 error, 1 file
```

## How to fix it

**If a variable has no value**, write it after `=`:

```jadpo fragment
function doubled(count: Count) -> Count {
    var result: Count = Count(count * 2)
    return result
}
```

```jadpo check project=e-expression-fix-1 hidden
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    var result: Count = Count(count * 2)
    return result
}
```

`doubled` now returns twice its count.

**If a member has no value**, write it after the colon:

```jadpo check project=e-expression-fix-2
type Count = Int {
    min: 0
}
```

`Count` now has a minimum of `0`.

## Related

- [Variables](../language/03-logic.md#variables)
- [Statements](../language/03-logic.md#statements)
- [Match](../language/03-logic.md#match)
