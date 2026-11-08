# `semantic.principal_variant_invalid`

A principal's `user:` or `service:` isn't a `subject:` and an optional `active:` condition.

## Why Jadpo refuses it

A principal's `user:` or `service:` tells Jadpo how to find a caller's
record. It has to name the unique field that identifies one, and may say
when a record counts as active. Without a `subject:`, Jadpo would have no
way to look anyone up.

## What it looks like

main.jadpo:

```jadpo check project=e-principal-variant-invalid expect=semantic.principal_variant_invalid
principal {
    user: {
        active: Member.suspended == false
    }
}

entity Member {
    id: Uuid
    suspended: Bool
}
```

```text output
error: `user:` has no `subject:`
 --> main.jadpo:2:5
  |
2 |     user: {
  |     ^^^^ no `subject:`
  reason: A principal's variant names the unique field that identifies a caller, `subject: <Entity>.<field>`, and may say when a record counts as active, `active: <condition>`.
  next: Add `subject: <Entity>.<field>`, naming a unique field (agent)
  rule: semantic.principal_variant_invalid
  help: https://jadpo.dev/docs/errors/semantic.principal_variant_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If `subject:` is missing**, name the entity's unique field that
identifies a caller:

```jadpo fragment
principal {
    user: {
        subject: Member.handle
        active: Member.suspended == false
    }
}
```

```jadpo check project=e-principal-variant-invalid-fix hidden
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

A caller is now found by `handle`, and a suspended member stops counting
as signed in.

## Related

- [`subject:`](../language/08-identity.md#subject)
- [`active:`](../language/08-identity.md#active)
