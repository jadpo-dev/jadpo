# `semantic.unknown_name`

```jadpo check project=e-unknown-name expect=semantic.unknown_name
type Pair = Object {
    first: Missing
}
```

```text output
error: `Missing` isn't declared
 --> main.jadpo:2:12
  |
2 |     first: Missing
  |            ^^^^^^^ not declared
  reason: Nothing in this project or the standard library is called `Missing`.
  next: Declare `Missing`, or use an existing name (agent)
  rule: semantic.unknown_name
  help: docs/language/01-project-layout-and-modules.html#semantic-unknown-name

check failed: 1 error, 1 file
```
