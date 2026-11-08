# `data.conflict_unknown`

A write's `conflict:` block names something that isn't a unique rule, a reference or a transition.

## Why Jadpo refuses it

A `conflict:` block says which failure the caller gets for each rule that
can refuse the write, and each line is keyed by that rule, written
`Entity.name`. A key that matches no rule would map a refusal that can
never happen, while the refusal it was meant for has no failure at all.

## What it looks like

main.jadpo:

```jadpo check project=e-conflict-unknown expect=data.conflict_unknown
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
            Customer.mail: EmailTaken
        }
    }
}
```

```text output
error: `Customer.mail` names no unique rule or reference
 --> main.jadpo:18:13
   |
18 |             Customer.mail: EmailTaken
   |             ^^^^^^^^^^^^^ not a rule
  reason: A `conflict:` arm is keyed by the rule the store refuses, written `Entity.name`: a field with `unique: true`, a rule named in `unique:`, a `restricted` reference that refuses a delete, or a lifecycle transition whose `when:` can refuse it. `Customer.create` can break `Customer.email`. Did you mean `Customer.email`?
  next: Use `Customer.email` instead of `Customer.mail` (agent)
  rule: data.conflict_unknown
  help: https://jadpo.dev/docs/errors/data.conflict_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you misspelt the rule**, use the name the error suggests:

```jadpo fragment
        conflict: {
            Customer.email: EmailTaken
        }
```

```jadpo check project=e-conflict-unknown-fix hidden
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

A second customer with the same address now gets `EmailTaken`.

## Related

- [When a write is refused](../language/06-reads-and-writes.md#when-a-write-is-refused)
- [Unique rules](../language/05-data.md#unique-rules)
