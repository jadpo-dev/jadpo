# `semantic.unused_result`

A call stands on its own only when its result is `Unit`, and a function called on its own does nothing:

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
  help: docs/errors/semantic.unused_result.md

check failed: 1 error, 1 file
```
