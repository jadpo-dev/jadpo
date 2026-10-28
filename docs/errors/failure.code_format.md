# `failure.code_format`

```jadpo check project=e-code-format expect=failure.code_format
type Missing = Failure {
    kind: FailureKind.not_found
    code: "Not Found!"
}
```

```text output
error: The code `Not Found!` isn't lower_snake_case
 --> main.jadpo:3:11
  |
3 |     code: "Not Found!"
  |           ^^^^^^^^^^^^ not lower_snake_case
  reason: A failure's code is what clients branch on and translate by, so it is lower_snake_case, like a failure's name turned into one.
  next: Write `"not_found"` (agent)
  rule: failure.code_format
  help: docs/language/04-failures.html#failure-code-format

check failed: 1 error, 1 file
```
