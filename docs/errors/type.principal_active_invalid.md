# `type.principal_active_invalid`

The `active:` of `principal { }` is something other than a condition on the entity's own fields, such as a field on its own.

## Why Jadpo refuses it

`active:` says when a record may still sign in, and a caller whose record
doesn't meet it is refused. It is a condition on the entity's own fields,
written as a query's `where:` is. A field on its own says nothing to
compare it with, so Jadpo can't tell which value counts as active.

## What it looks like

main.jadpo:

```jadpo check project=e-principal-active-invalid expect=type.principal_active_invalid
principal {
    user: {
        subject: Member.handle
        active: Member.suspended
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
    suspended: Bool
}
```

```text output
error: `Member.suspended` isn't a condition `active:` can use
 --> main.jadpo:4:17
  |
4 |         active: Member.suspended
  |                 ^^^^^^^^^^^^^^^^ not a comparison
  reason: `active:` is a condition on `Member`'s own fields, such as `Member.<field> == <value>`; a field alone says nothing to compare it with.
  next: Compare a field of `Member` with a value (agent)
  rule: type.principal_active_invalid
  help: https://jadpo.dev/docs/errors/type.principal_active_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a field's value says whether the record is active**, compare the
field with that value:

```jadpo fragment
principal {
    user: {
        subject: Member.handle
        active: Member.suspended == false
    }
}
```

```jadpo check project=e-principal-active-invalid-fix hidden
principal {
    user: {
        subject: Member.handle
        active: Member.suspended == false
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
    suspended: Bool
}
```

A member whose `suspended` is `true` is now refused with `user_disabled`.

## Related

- [`active:`](../language/08-identity.md#active)
- [The principal](../language/08-identity.md#the-principal)
