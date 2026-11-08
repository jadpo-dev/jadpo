# `semantic.unused_result`

A value is computed and thrown away.

## Why Jadpo refuses it

A call stands on its own only when its result is `Unit`. A value that
nothing uses means either the call does nothing, or the code forgot to
use what it asked for, and Jadpo won't guess which.

## What it looks like

main.jadpo:

```jadpo check project=e-unused-result expect=semantic.unused_result
type Note = Text {}

function tidy(note: Note) -> Note {
    return note
}

action save(note: Note) -> Unit {
    tidy(note)
}
```

```text output
error: This `Note` is thrown away
 --> main.jadpo:8:5
  |
8 |     tidy(note)
  |     ^^^^^^^^^^ result not used
  reason: `tidy` only computes its result, so calling it without using the result does nothing.
  next: Use the value, or remove it (agent)
  rule: semantic.unused_result
  help: https://jadpo.dev/docs/errors/semantic.unused_result

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value matters**, use it:

```jadpo fragment
function tidied(note: Note) -> Note {
    return tidy(note)
}
```

```jadpo check project=e-unused-result-fix-1 hidden
type Note = Text {}

function tidy(note: Note) -> Note {
    return note
}

function tidied(note: Note) -> Note {
    return tidy(note)
}
```

The tidied note now reaches the caller.

**If the value doesn't matter**, remove the call:

```jadpo fragment
action save(note: Note) -> Unit {}
```

```jadpo check project=e-unused-result-fix-2 hidden
type Note = Text {}

action save(note: Note) -> Unit {}
```

`save` no longer computes something only to drop it.

## Related

- [Statements](../language/03-logic.md#statements)
- [Functions and actions](../language/03-logic.md#functions-and-actions)
