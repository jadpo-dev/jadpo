# `failure.code_format`

A failure's `code:` is empty or isn't lower_snake_case.

## Why Jadpo refuses it

A failure's code is what clients branch on and translate by, so every code in
an API has the same shape: lower_snake_case, like the failure's name turned
into one. A code in any other form is one a client has to special-case.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/failure.code_format

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An empty code is refused the same way, and `next:` asks you to remove it:

main.jadpo:

```jadpo check project=e-code-format-empty expect=failure.code_format
type Missing = Failure {
    kind: FailureKind.not_found
    code: ""
}
```

```text output
error: This failure's code is empty
 --> main.jadpo:3:11
  |
3 |     code: ""
  |           ^^ not lower_snake_case
  reason: A failure's code is what clients branch on and translate by, so it is lower_snake_case, like a failure's name turned into one.
  next: Remove `code:` to use `missing` (agent)
  rule: failure.code_format
  help: https://jadpo.dev/docs/errors/failure.code_format

check failed: 1 error, 1 file
```

## How to fix it

**If you want a code of your own**, write it in lower_snake_case:

```jadpo check project=e-code-format-fix-1
type Missing = Failure {
    kind: FailureKind.not_found
    code: "not_found"
}
```

Clients now get `not_found`.

**If the failure's name will do**, remove `code:`:

```jadpo check project=e-code-format-fix-2
type Missing = Failure {
    kind: FailureKind.not_found
}
```

Clients now get `missing`, the failure's name in lower_snake_case.

## Related

- [The code clients see](../language/04-failures.md#the-code-clients-see)
- [Declaring a failure](../language/04-failures.md#declaring-a-failure)
