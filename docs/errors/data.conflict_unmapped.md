# `data.conflict_unmapped`

A write could be refused by a unique rule or a reference, and its `conflict:` block doesn't say which failure the caller gets then.

## Why Jadpo refuses it

Jadpo knows which rules each write can break. A create can break every
unique rule of its entity, and a delete can be blocked by a `restricted`
reference to the record. When one does, the caller needs a failure it
can handle, such as "that email is already registered", rather than the
database's own error.

There is no catch-all, because each rule means something different to
the caller. So each rule the write can break needs its own arm in
`conflict:`, naming a failure of kind `conflict`.

## What it looks like

main.jadpo:

```jadpo check project=e-conflict-unmapped expect=data.conflict_unmapped
entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
}

action register(email: Customer.email) -> Customer {
    return Customer.create {
        set: {
            email: email
        }
    }
}
```

```text output
error: `Customer.create` can break `Customer.email`, and nothing maps it
 --> main.jadpo:9:12
  |
9 |     return Customer.create {
  |            ^^^^^^^^^^^^^^^ can be refused
  reason: A write is refused when it would give two records the same values a unique rule covers; it needs an arm in `conflict:` naming the `conflict` failure the caller gets, since there is no catch-all.
  next: Add `conflict: { Customer.email: SomeFailure }` to the block (agent)
  rule: data.conflict_unmapped
  help: https://jadpo.dev/docs/errors/data.conflict_unmapped

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the write can clash with an existing record**, declare a failure of
kind `conflict`, map the rule to it in `conflict:`, and pass it on with
`attempt`, saying in `fails` that the action can fail with it:

```jadpo fragment
type EmailTaken = Failure {
    kind: FailureKind.conflict
}

action register(email: Customer.email) fails EmailTaken -> Customer {
    return attempt Customer.create {
        set: {
            email: email
        }
        conflict: {
            Customer.email: EmailTaken
        }
    }
}
```

```jadpo check project=e-conflict-unmapped-fix-1 hidden
entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
}

type EmailTaken = Failure {
    kind: FailureKind.conflict
}

action register(email: Customer.email) fails EmailTaken -> Customer {
    return attempt Customer.create {
        set: {
            email: email
        }
        conflict: {
            Customer.email: EmailTaken
        }
    }
}
```

Registering an email that another customer already has now fails with
`EmailTaken`, and a client calling it through a route gets a 409, the
status of its kind, `conflict`.

## Related

- [When a write is refused](../language/06-reads-and-writes.md#when-a-write-is-refused)
- [A write that breaks a unique rule: `conflict:`](../language/04-failures.md#a-write-that-breaks-a-unique-rule-conflict)
- [Kinds](../language/04-failures.md#kinds)
- [`data.conflict_cannot_break`](../errors/data.conflict_cannot_break.md)
