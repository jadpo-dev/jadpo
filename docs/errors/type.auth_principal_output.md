# `type.auth_principal_output`

An `Output` has a field that holds a caller, a `Principal.user` or a `Principal.service`.

## Why Jadpo refuses it

A caller is who signed in, as Jadpo holds it when the app runs, and it is
never sent to a client. An output sends the fields a client needs, such
as the caller's `id` or a field of their record. This holds however
deeply a field holds the caller: in a list, a map, an option, or another
object's field.

## What it looks like

main.jadpo:

```jadpo check project=e-auth-principal-output expect=type.auth_principal_output
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

type Whoami = Output {
    caller: Principal.user
}
```

```text output
error: `Whoami.caller` holds a caller
 --> main.jadpo:19:13
   |
19 |     caller: Principal.user
   |             ^^^^^^^^^^^^^^ a caller
  reason: A caller value is who signed in, as the runtime holds it, and is never sent to a client; an output sends the fields a client needs.
  next: Type `caller` with the entity's id, such as `User.id`, or send the fields a client needs (agent)
  rule: type.auth_principal_output
  help: https://jadpo.dev/docs/errors/type.auth_principal_output

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is what the client needs to know about the caller.

**If it needs to know which record they are**, type the field with the
entity's `id`:

```jadpo fragment
type Whoami = Output {
    caller: Member.id
}
```

```jadpo check project=e-auth-principal-output-fix-1 hidden
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

type Whoami = Output {
    caller: Member.id
}
```

The code that builds a `Whoami` fills `caller` with `caller.id`, and the
client gets the member's `id`.

**If it needs something about them**, send that field of their record:

```jadpo fragment
type Whoami = Output {
    handle: Handle
}
```

```jadpo check project=e-auth-principal-output-fix-2 hidden
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

type Whoami = Output {
    handle: Handle
}
```

The client gets the member's handle, read from their record, and
nothing else about them.

## Related

- [Who is calling: `current_principal`](../language/08-identity.md#who-is-calling-current_principal)
- [What code reads from the caller](../language/08-identity.md#what-code-reads-from-the-caller)
