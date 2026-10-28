# `semantic.unknown_callee`

```jadpo check project=e-unknown-callee expect=semantic.unknown_callee
type Count = Int {
    min: 0
}

function f(x: Count) -> Count {
    return lookup(x)
}
```

```text output
error: `lookup` isn't a function, action or query
 --> main.jadpo:6:12
  |
6 |     return lookup(x)
  |            ^^^^^^ not declared
  reason: Nothing in this project called `lookup` can be called.
  next: Declare `lookup`, or call an existing function (agent)
  rule: semantic.unknown_callee
  help: docs/language/01-project-layout-and-modules.html#semantic-unknown-callee

check failed: 1 error, 1 file
```
