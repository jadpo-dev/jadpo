# `type.auth_principal_construction`

Application code makes a caller from a record with `Principal.user(...)` or `Principal.service(...)`, which only tests, fixtures and seeds may do.

## Why Jadpo refuses it

A caller, a `Principal.user` or a `Principal.service`, is who Jadpo
checked when they signed in. Code that could make one from any record
could act as anyone, and policy, which reads the caller, would let it.
So a caller reaches application code only as a parameter, which a route
fills from `current_principal`. Tests, fixtures and seeds make one from a
record, to act as that person.

## What it looks like

main.jadpo:

```jadpo check project=e-auth-principal-construction expect=type.auth_principal_construction
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

action act_as(member: Member) -> Principal.user {
    return Principal.user(member)
}
```

```text output
error: An action can't make a caller
 --> main.jadpo:19:12
   |
19 |     return Principal.user(member)
   |            ^^^^^^^^^^^^^^ made from a record
  reason: A caller only comes from signing in; tests, fixtures and seeds make one from a record with `Principal.user(...)`, and application code takes it as a parameter a route passes.
  next: Take a `Principal.user` parameter, which the route passes as `current_principal.user` (agent)
  rule: type.auth_principal_construction
  help: https://jadpo.dev/docs/errors/type.auth_principal_construction

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the action needs who is calling**, take the caller as a parameter
and read what it needs from it:

```jadpo fragment
action act_as(caller: Principal.user) -> Member.id {
    return caller.id
}
```

```jadpo check project=e-auth-principal-construction-fix hidden
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

action act_as(caller: Principal.user) -> Member.id {
    return caller.id
}
```

The route that runs the action passes `current_principal.user`, the
person who signed in, and a test passes `Principal.user(member)` for the
member it acts as.

## Related

- [Who is calling: `current_principal`](../language/08-identity.md#who-is-calling-current_principal)
- [What code reads from the caller](../language/08-identity.md#what-code-reads-from-the-caller)
