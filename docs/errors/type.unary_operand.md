# `type.unary_operand`

```jadpo check project=e-unary expect=type.unary_operand
type Phrase = Text {}

type Flag = Bool {}

function negate(phrase: Phrase) -> Flag {
    return Flag(not phrase)
}
```

```text output
error: `not` needs a `Bool`, and `phrase` is a `Phrase`
 --> main.jadpo:6:21
  |
6 |     return Flag(not phrase)
  |                     ^^^^^^ this is `Phrase`
  reason: `not` works on `Bool` values and types built on `Bool`; Jadpo has no truthiness.
  next: Compare `phrase` to get a `Bool` (agent)
  rule: type.unary_operand
  help: docs/language/03-logic.html#type-unary-operand

check failed: 1 error, 1 file
```
