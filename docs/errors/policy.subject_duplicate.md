# `policy.subject_duplicate`

```jadpo check project=e-subject-duplicate expect=policy.subject_duplicate
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

entity Note {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        read: [owner_id, Access.authenticated, owner_id]
    }
}
```

```text output
error: `owner_id` is listed twice
 --> main.jadpo:25:48
   |
25 |         read: [owner_id, Access.authenticated, owner_id]
   |                                                ^^^^^^^^ listed again
  reason: A grant list means "any of these", so naming someone twice adds nothing and hides which line was meant.
  next: Remove the second `owner_id` (agent)
  rule: policy.subject_duplicate
  help: docs/language/09-policy.html#policy-subject-duplicate

check failed: 1 error, 1 file
```
