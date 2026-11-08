# `semantic.principal_variant_unknown`

A principal declares a kind of caller other than `user:` or `service:`.

## Why Jadpo refuses it

A caller is a person or another system. A role or a tenant is a field of
the user's entity, which policy reads, never a kind of caller: two kinds
of person signing in the same way should go through the same checks.

## What it looks like

main.jadpo:

```jadpo check project=e-principal-variant-unknown expect=semantic.principal_variant_unknown
principal {
    user: {
        subject: Member.handle
    }
    admin: {
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

```text output
error: `admin` isn't a kind of caller
 --> main.jadpo:5:5
  |
5 |     admin: {
  |     ^^^^^ not `user` or `service`
  reason: A principal declares `user:`, a person, and `service:`, another system; a role or a tenant is a field of the user's entity, which policy reads (area 9).
  next: Remove `admin` (agent)
  rule: semantic.principal_variant_unknown
  help: https://jadpo.dev/docs/errors/semantic.principal_variant_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it's a role**, make it a field of the entity, and remove the variant:

```jadpo fragment
entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    is_admin: Bool
}
```

```jadpo check project=e-principal-variant-unknown-fix hidden
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
    is_admin: Bool
}
```

An admin is now a `Member` whose record says so, and policy can read it.

## Related

- [The principal](../language/08-identity.md#the-principal)
- [What code reads from the caller](../language/08-identity.md#what-code-reads-from-the-caller)
