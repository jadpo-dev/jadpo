# `policy.effect_duplicate`

One list per effect, so "who may read a note?" is one line:

```jadpo check project=e-effect-duplicate expect=policy.effect_duplicate
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
        read: [Access.authenticated]
    }
}
```

```text output
error: `read:` is given twice
 --> main.jadpo:26:9
   |
25 |         read: [owner_id]
   |         ---- first given here
26 |         read: [Access.authenticated]
   |         ^^^^ given again
  reason: `Note`'s policy gives each list once, so "who may read?" is always one line.
  next: Merge the two `read:` lists into one (agent)
  rule: policy.effect_duplicate
  help: docs/language/09-policy.html#policy-effect-duplicate

check failed: 1 error, 1 file
```
