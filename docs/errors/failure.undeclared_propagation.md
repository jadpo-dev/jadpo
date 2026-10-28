# `failure.undeclared_propagation`

```jadpo check project=e-undeclared expect=failure.undeclared_propagation
type Note = Text {}

type RegistrationClosed = Failure {
    kind: FailureKind.conflict
}

action register(note: Note) -> Note {
    reject RegistrationClosed
}
```

```text output
error: `RegistrationClosed` can leave `register`, but its `fails` list doesn't say so
 --> main.jadpo:8:5
  |
8 |     reject RegistrationClosed
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^ rejected here
  reason: `fails` lists exactly the failures that can leave an action, so every caller knows what it must handle.
  next: Add `RegistrationClosed` to `register`'s `fails` list (agent)
  rule: failure.undeclared_propagation
  help: docs/language/04-failures.html#failure-undeclared-propagation

check failed: 1 error, 1 file
```
