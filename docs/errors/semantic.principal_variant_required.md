# `semantic.principal_variant_required`

A principal declares neither `user:` nor `service:`.

## Why Jadpo refuses it

A principal says which entity a signed-in caller is. With neither a
`user:` nor a `service:`, the block says nothing, and nobody could ever
sign in.

## What it looks like

main.jadpo:

```jadpo check project=e-principal-variant-required expect=semantic.principal_variant_required
principal {}
```

```text output
error: The principal declares no kind of caller
 --> main.jadpo:1:1
  |
1 | principal {}
  | ^^^^^^^^^ no `user:` or `service:`
  reason: A principal says which entity a signed-in caller is: `user:` for a person, `service:` for another system, or both.
  next: Add `user: { subject: <Entity>.<field> }` (agent)
  rule: semantic.principal_variant_required
  help: https://jadpo.dev/docs/errors/semantic.principal_variant_required

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the app signs people in**, declare `user:`:

```jadpo fragment
principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=e-principal-variant-required-fix hidden
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

A caller who signs in is now a `Member`.

## Related

- [The principal](../language/08-identity.md#the-principal)
