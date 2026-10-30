# `syntax.fails_after_result`

```jadpo check project=e-fails-after expect=syntax.fails_after_result
type Title = Text {
    max_length: 200
}

type TitleBlank = Failure {
    kind: FailureKind.invalid_value
    message: "Enter a title."
}

function check_title(title: Title) -> Title fails TitleBlank {
    if (title == "") {
        reject TitleBlank
    }
    return title
}
```

```text output
error: `fails` goes before `->`
 --> main.jadpo:10:45
   |
10 | function check_title(title: Title) -> Title fails TitleBlank {
   |                                             ^^^^^^^^^^^^^^^^ after the result type
  reason: A signature says what can fail before what it returns: `fails A, B -> Result`.
  next: Move `fails TitleBlank` before `->` (agent)
  rule: syntax.fails_after_result
  help: https://jadpo.dev/docs/errors/syntax.fails_after_result

check failed: 1 error, 1 file
```
