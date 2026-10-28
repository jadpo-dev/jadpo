# `syntax.expected_name`

```jadpo check project=e-expected-name expect=syntax.expected_name
type Pair = Object {
    : Int
}
```

```text output
error: This member has no name
 --> main.jadpo:2:5
  |
2 |     : Int
  |     ^ expected a name before `:`
  reason: Every member inside braces is `name: value`.
  next: Add the member's name before `:` (agent)
  rule: syntax.expected_name
  help: docs/language/01-project-layout-and-modules.html#syntax-expected-name

check failed: 1 error, 1 file
```
