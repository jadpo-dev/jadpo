# `policy.declaration_duplicate`

```jadpo check project=e-declaration-duplicate expect=policy.declaration_duplicate
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
        read: [owner_id]
    }

    policy: {
        update: [owner_id]
    }
}
```

```text output
error: `Note` declares `policy:` twice
 --> main.jadpo:28:5
   |
24 |     policy: {
   |     ------ first declared here
28 |     policy: {
   |     ^^^^^^ declared again
  reason: An entity's grants are one list per effect in one `policy:`, so "who can delete this?" is always one line.
  next: Move what the second `policy:` holds into the first, and remove it (agent)
  rule: policy.declaration_duplicate
  help: docs/errors/policy.declaration_duplicate.md

check failed: 1 error, 1 file
```
