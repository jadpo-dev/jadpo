# `type.auth_principal_failure_context`

A failure has a field that holds a caller, a `Principal.user` or a `Principal.service`.

## Why Jadpo refuses it

A failure is logged, and its `client:` fields reach the client. A caller
is who signed in, as Jadpo holds it, and belongs to neither. A failure
that needs to say who was refused carries the caller's `id`, which is
what a log or a client can use. This holds however deeply a field holds
the caller: in a list, a map, an option, or another object's field.

## What it looks like

main.jadpo:

```jadpo check project=e-auth-principal-failure-context expect=type.auth_principal_failure_context
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

type MemoLocked = Failure {
    kind: FailureKind.conflict
    client: {
        caller: Principal.user
    }
}
```

```text output
error: `MemoLocked.caller` holds a caller
 --> main.jadpo:21:17
   |
21 |         caller: Principal.user
   |                 ^^^^^^^^^^^^^^ a caller
  reason: A failure is logged and its client fields reach the client, while a caller value holds who signed in, so a failure carries the caller's `id` instead.
  next: Type `caller` with the entity's id, such as `User.id` (agent)
  rule: type.auth_principal_failure_context
  help: https://jadpo.dev/docs/errors/type.auth_principal_failure_context

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the failure says who was refused**, type the field with the entity's
`id`:

```jadpo fragment
type MemoLocked = Failure {
    kind: FailureKind.conflict
    client: {
        caller: Member.id
    }
}
```

```jadpo check project=e-auth-principal-failure-context-fix hidden
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

type MemoLocked = Failure {
    kind: FailureKind.conflict
    client: {
        caller: Member.id
    }
}
```

Code that raises the failure fills `caller` with `caller.id`, and the
client is sent the member's `id`.

## Related

- [`client:` and `internal:` fields](../language/04-failures.md#client-and-internal-fields)
- [Who is calling: `current_principal`](../language/08-identity.md#who-is-calling-current_principal)
