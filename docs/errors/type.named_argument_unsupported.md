# `type.named_argument_unsupported`

```jadpo check project=e-named-argument expect=type.named_argument_unsupported
type Number = Int {}

function add(left: Number, right: Number) -> Number {
    return Number(left + right)
}

function three() -> Number {
    return add(left: 1, right: 2)
}
```

```text output
error: `add` takes its arguments in order, without names
 --> main.jadpo:8:16
  |
8 |     return add(left: 1, right: 2)
  |                ^^^^ named
  reason: Named arguments exist only for standard-library operations; your own callables are called positionally, in the order of their parameters.
  next: Remove the argument names (agent)
  rule: type.named_argument_unsupported
  help: docs/errors/type.named_argument_unsupported.md

check failed: 1 error, 1 file
```
