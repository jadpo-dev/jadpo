# `semantic.standard_namespace_reserved`

```jadpo check project=e-std-namespace expect=semantic.standard_namespace_reserved
type Count = Int {
    min: 0
}

function temporal(x: Count) -> Count {
    return x
}
```

```text output
error: `temporal` is a standard-library namespace
 --> main.jadpo:5:10
  |
5 | function temporal(x: Count) -> Count {
  |          ^^^^^^^^ reserved
  reason: Standard-library namespaces can't name a declaration, domain, library or variable, so `temporal.` always means the standard library.
  next: Choose another name than `temporal` (agent)
  rule: semantic.standard_namespace_reserved
  help: https://jadpo.dev/docs/errors/semantic.standard_namespace_reserved

check failed: 1 error, 1 file
```
