# `syntax.fails_after_result`

A signature's `fails` list comes after its result type instead of before `->`.

## Why Jadpo refuses it

A signature says what can fail before what it returns, as in `fails A, B ->
Result`. The result is then always the last thing on the line, and the
failures are always in the same place.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the function can fail with `TitleBlank`**, move `fails TitleBlank`
before `->`:

```jadpo fragment
function check_title(title: Title) fails TitleBlank -> Title {
    if (title == "") {
        reject TitleBlank
    }
    return title
}
```

```jadpo check project=e-fails-after-fix hidden
type Title = Text {
    max_length: 200
}

type TitleBlank = Failure {
    kind: FailureKind.invalid_value
    message: "Enter a title."
}

function check_title(title: Title) fails TitleBlank -> Title {
    if (title == "") {
        reject TitleBlank
    }
    return title
}
```

The signature now reads in Jadpo's order: what can fail, then what it
returns.

## Related

- [Saying what can fail: `fails`](../language/04-failures.md#saying-what-can-fail-fails)
- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
