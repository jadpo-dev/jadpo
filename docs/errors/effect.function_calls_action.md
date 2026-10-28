# `effect.function_calls_action`

```jadpo check project=e-fn-action expect=effect.function_calls_action
type Note = Text {}

action save(note: Note) -> Note {
    return note
}

function pretend_pure(note: Note) -> Note {
    return save(note)
}
```

```text output
error: A function can't call action `save`
 --> main.jadpo:8:12
  |
8 |     return save(note)
  |            ^^^^ an action
  reason: A function is pure: it computes its result from its arguments alone, so it calls only functions, library functions and the standard library; an action may read and write data.
  next: Make the caller an action, or call `save` from the action that uses this function (agent)
  rule: effect.function_calls_action
  help: docs/language/03-logic.html#effect-function-calls-action

check failed: 1 error, 1 file
```
