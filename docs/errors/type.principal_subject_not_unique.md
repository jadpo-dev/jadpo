# `type.principal_subject_not_unique`

The `subject:` of `principal { }` names a field without `unique: true`, so it could find more than one record.

## Why Jadpo refuses it

Jadpo finds which record a signed-in caller is by looking them up in this
field, so the field has to pick out exactly one record. If two records
could hold the same value, one person could be taken for another.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.principal_subject_not_unique

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If no two records should share the value**, mark the field `unique:
true`:

```jadpo fragment
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
}
```

```jadpo check project=e-principal-subject-not-unique-fix-1 hidden
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
    handle: Handle {
        unique: true
    }
}
```

No two members can now have the same `handle`, and a caller's `handle`
finds one record.

**If two records may share the value**, name a field that doesn't repeat,
adding one if there isn't one:

```jadpo fragment
principal {
    user: {
        subject: Member.login
    }
}

entity Member {
    id: Uuid
    handle: Handle
    login: Handle {
        unique: true
    }
}
```

```jadpo check project=e-principal-subject-not-unique-fix-2 hidden
principal {
    user: {
        subject: Member.login
    }
}

type Handle = Text {
    max_length: 40
}

entity Member {
    id: Uuid
    handle: Handle
    login: Handle {
        unique: true
    }
}
```

Jadpo now finds a signed-in caller's record by its `login`, and `handle`
can repeat.

## Related

- [`subject:`](../language/08-identity.md#subject)
- [The principal](../language/08-identity.md#the-principal)
