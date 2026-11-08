# `semantic.principal_variant_duplicate`

A principal declares `user:` or `service:` twice.

## Why Jadpo refuses it

A principal says once which entity a user is, and once which a service
is, so a caller is looked up one way. Jadpo can't choose between two
answers to that.

## What it looks like

main.jadpo:

```jadpo check project=e-principal-variant-duplicate expect=semantic.principal_variant_duplicate
principal {
    user: {
        subject: Member.handle
    }
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

```text output
error: `user:` is declared twice
 --> main.jadpo:5:5
  |
2 |     user: {
  |     ---- first declared here
5 |     user: {
  |     ^^^^ declared again
  reason: A principal says once which entity a user is, so a caller is looked up one way.
  next: Remove one `user:` (agent)
  rule: semantic.principal_variant_duplicate
  help: https://jadpo.dev/docs/errors/semantic.principal_variant_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If both say the same thing**, keep one:

```jadpo fragment
principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=e-principal-variant-duplicate-fix hidden
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

A signed-in user is now found one way, by `handle`.

## Related

- [The principal](../language/08-identity.md#the-principal)
