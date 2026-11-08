# `syntax.unexpected_token`

A bracket, a comma or another part of the syntax is missing, or something stands where it doesn't belong.

## Why Jadpo refuses it

Jadpo reads source against the forms the language has, and here what came
next doesn't fit the form it was reading. It reports what it expected rather
than guessing what you meant, so the fix depends on where the code stopped
fitting; the message names the place.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A block that never ends is this error too, reported on its opening `{`:

main.jadpo:

```jadpo check project=e-unexpected-token-brace expect=syntax.unexpected_token
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
```

```text output
error: This `{` isn't closed
 --> main.jadpo:5:38
  |
5 | function same(count: Count) -> Count {
  |                                      ^ no matching `}`
  reason: Every `{` is closed by a `}`; found the end of the file before this one was.
  next: Add `}` where the block ends (agent)
  rule: syntax.unexpected_token
  help: https://jadpo.dev/docs/errors/syntax.unexpected_token

check failed: 1 error, 1 file
```

And so are two parameters with no comma between them:

main.jadpo:

```jadpo check project=e-unexpected-token-comma expect=syntax.unexpected_token
type Count = Int {
    min: 0
}

function same(left: Count right: Count) -> Count {
    return left
}
```

```text output
error: Expected `,` or `)` after this parameter
 --> main.jadpo:5:27
  |
5 | function same(left: Count right: Count) -> Count {
  |                           ^^^^^ expected `,` or `)`
  reason: Parameters are separated by commas; found `right`.
  next: Add `,` between the parameters (agent)
  rule: syntax.unexpected_token
  help: https://jadpo.dev/docs/errors/syntax.unexpected_token

check failed: 1 error, 1 file
```

## How to fix it

**If a list of parameters isn't closed**, add `)` after the last one:

```jadpo fragment
function f(x: Count) -> Count {
    return x
}
```

```jadpo check project=e-unexpected-token-fix-1 hidden
type Count = Int {
    min: 0
}

function f(x: Count) -> Count {
    return x
}
```

The signature now ends before `->`, and the function checks.

**If a block isn't closed**, add `}` where it ends:

```jadpo fragment
function same(count: Count) -> Count {
    return count
}
```

```jadpo check project=e-unexpected-token-fix-2 hidden
type Count = Int {
    min: 0
}

function same(count: Count) -> Count {
    return count
}
```

The body now ends where the function does.

**If a comma is missing**, add it between the parameters:

```jadpo fragment
function same(left: Count, right: Count) -> Count {
    return left
}
```

```jadpo check project=e-unexpected-token-fix-3 hidden
type Count = Int {
    min: 0
}

function same(left: Count, right: Count) -> Count {
    return left
}
```

`same` now takes two parameters.

## Related

- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
- [Statements](../language/03-logic.md#statements)
