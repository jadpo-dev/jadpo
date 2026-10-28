# `type.principal_subject_invalid`

```jadpo check project=e-principal-subject-invalid expect=type.principal_subject_invalid
principal {
    user: {
        subject: Member
    }
}

entity Member {
    id: Uuid
}
```

```text output
error: `Member` isn't an entity's field
 --> main.jadpo:3:18
  |
3 |         subject: Member
  |                  ^^^^^^ not `<Entity>.<field>`
  reason: A user is looked up by a unique field of the entity it is stored as, written `<Entity>.<field>`, such as `User.email`.
  next: Name a unique field of the entity, such as `User.email` (agent)
  rule: type.principal_subject_invalid
  help: docs/language/08-identity.html#type-principal-subject-invalid

check failed: 1 error, 1 file
```
