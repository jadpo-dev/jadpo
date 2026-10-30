# `syntax.unexpected_token`

```jadpo check project=e-unexpected-token expect=syntax.unexpected_token
type Count = Int {
    min: 0
}

function f(x: Count -> Count {
    return x
}
```

```text output
error: This parameter list isn't closed
 --> main.jadpo:5:21
  |
5 | function f(x: Count -> Count {
  |                     ^^ expected `)` before `->`
  |           - opened here
  reason: The parameters end with `)`, before `fails`, `->` or the body.
  next: Add `)` after `Count` (agent)
  rule: syntax.unexpected_token
  help: https://jadpo.dev/docs/errors/syntax.unexpected_token

check failed: 1 error, 1 file
```
