# `type.named_argument_unknown`

```jadpo check project=e-named-unknown expect=type.named_argument_unknown
type Stock = Int {
    min: 0
}

function line_count(lines: List<Stock>) -> Stock {
    return Stock(list.count(itmes: lines))
}
```

```text output
error: `list.count` has no argument `itmes`
 --> main.jadpo:6:29
  |
6 |     return Stock(list.count(itmes: lines))
  |                             ^^^^^ not a parameter
  reason: Its one argument is `items`.
  next: Use `items:` instead of `itmes:` (agent)
  rule: type.named_argument_unknown
  help: docs/errors/type.named_argument_unknown.md

check failed: 1 error, 1 file
```
