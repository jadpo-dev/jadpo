# `policy.subject_duplicate`

A grant list names the same caller twice.

## Why Jadpo refuses it

A grant list means "any of these", so naming someone twice lets in nobody new.
It usually means a line was meant to name someone else, and Jadpo can't tell
who, so it asks you to look.

## What it looks like

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
  help: https://jadpo.dev/docs/errors/policy.subject_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name was repeated by mistake**, remove it:

```jadpo fragment
    policy: {
        read: [owner_id, Access.authenticated]
    }
```

```jadpo check project=e-subject-duplicate-fix hidden
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
        read: [owner_id, Access.authenticated]
    }
}
```

Each caller is now named once. If the second name was meant to be someone
else, write that person instead.

## Related

- [Grants](../language/09-policy.md#grants)
- [A user field on the record](../language/09-policy.md#a-user-field-on-the-record)
