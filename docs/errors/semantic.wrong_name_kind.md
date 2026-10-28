# `semantic.wrong_name_kind`

```jadpo check project=e-wrong-kind expect=semantic.wrong_name_kind
type Count = Int {
    min: 0
}

function g(x: Count) -> Count {
    return x
}

type Pair = Object {
    first: g
}
```

```text output
error: `g` is a function, not a type
 --> main.jadpo:10:12
   |
10 |     first: g
   |            ^ expected a type
  reason: Here Jadpo expects a type, such as `Count` or `List<Count>`.
  next: Name a type here (agent)
  rule: semantic.wrong_name_kind
  help: docs/errors/semantic.wrong_name_kind.md

check failed: 1 error, 1 file
```
