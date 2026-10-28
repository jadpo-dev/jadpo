# `type.principal_subject_not_unique`

```jadpo check project=e-principal-subject-not-unique expect=type.principal_subject_not_unique
principal {
    user: {
        subject: Member.handle
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle
}
```

```text output
error: `Member.handle` doesn't identify one user
 --> main.jadpo:3:18
  |
3 |         subject: Member.handle
  |                  ^^^^^^^^^^^^^ not unique
  reason: A caller is looked up by its subject, so the subject names exactly one record: `Member.handle` isn't `unique: true`.
  next: Make `handle` a required field with `unique: true`, or name one that is (agent)
  rule: type.principal_subject_not_unique
  help: docs/language/08-identity.html#type-principal-subject-not-unique

check failed: 1 error, 1 file
```
