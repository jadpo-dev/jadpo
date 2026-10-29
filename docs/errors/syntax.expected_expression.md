# `syntax.expected_expression`

A variable is given its value on the line that declares it:

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
  help: docs/errors/syntax.expected_expression.md

check failed: 1 error, 1 file
```
