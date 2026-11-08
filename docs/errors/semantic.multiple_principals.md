# `semantic.multiple_principals`

A project declares more than one `principal { }`.

## Why Jadpo refuses it

The caller is declared once: one `user:` and at most one `service:`, so
every route reads the same caller. Two blocks could name two different
entities, and then no one could say who is signed in.

## What it looks like

main.jadpo:

```jadpo check project=e-multiple-principals expect=semantic.multiple_principals
principal {
    user: {
        subject: Member.handle
    }
}

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

```text output
error: `principal { }` is declared twice
 --> main.jadpo:7:1
  |
1 | principal {
  | --------- first declared here
7 | principal {
  | ^^^^^^^^^ declared again
  reason: The caller is declared once, in `app.jadpo`: one `user:` and at most one `service:`, so every route reads the same caller.
  next: Move what this `principal` holds into the first, and remove it (agent)
  rule: semantic.multiple_principals
  help: https://jadpo.dev/docs/errors/semantic.multiple_principals

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the blocks say the same thing**, remove one:

```jadpo fragment
principal {
    user: {
        subject: Member.handle
    }
}
```

```jadpo check project=e-multiple-principals-fix hidden
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

Every route now reads its caller the one way this block says.

## Related

- [The principal](../language/08-identity.md#the-principal)
