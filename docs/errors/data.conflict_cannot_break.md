# `data.conflict_cannot_break`

A write's `conflict:` block maps a rule that the write can never break.

## Why Jadpo refuses it

Jadpo knows which rules each write can break. An update can break only
the unique rules over the fields it sets. Here `Customer.update` sets
`name` and not `email`, so the unique rule on `email` can never refuse
it, and its arm in `conflict:` never runs.

An arm that never runs misleads the reader, and the failure it names
makes callers handle something that can't happen. Usually it means the
write changed and the arm was left behind.

## What it looks like

main.jadpo:

```jadpo check project=e-conflict-cannot-break expect=data.conflict_cannot_break
type CustomerName = Text {
    max_length: 80
}

entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
    name: CustomerName
}

type EmailTaken = Failure {
    kind: FailureKind.conflict
}

action rename(customer: Customer, name: Customer.name) fails EmailTaken -> Customer {
    return attempt Customer.update(customer) {
        set: {
            name: name
        }
        conflict: {
            Customer.email: EmailTaken
        }
    }
}
```

```text output
error: `Customer.update` can't break `Customer.email`, so it takes no arm
 --> main.jadpo:23:13
   |
23 |             Customer.email: EmailTaken
   |             ^^^^^^^^^^^^^^ never runs
  reason: `Customer.update` sets none of the fields `Customer.email` covers, so it can't break it.
  next: Remove `Customer.email: ...` (agent)
  rule: data.conflict_cannot_break
  help: https://jadpo.dev/docs/errors/data.conflict_cannot_break

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the write isn't meant to change the email**, remove the arm. With
it goes the `conflict:` block, `EmailTaken`, and the `fails` and
`attempt` that only passed that failure on:

```jadpo fragment
action rename(customer: Customer, name: Customer.name) -> Customer {
    return Customer.update(customer) {
        set: {
            name: name
        }
    }
}
```

```jadpo check project=e-conflict-cannot-break-fix-1 hidden
type CustomerName = Text {
    max_length: 80
}

entity Customer {
    id: Uuid
    email: Email {
        unique: true
    }
    name: CustomerName
}

action rename(customer: Customer, name: Customer.name) -> Customer {
    return Customer.update(customer) {
        set: {
            name: name
        }
    }
}
```

`rename` now declares no failure, so its callers no longer handle an
`EmailTaken` that could never happen.

## Related

- [When a write is refused](../language/06-reads-and-writes.md#when-a-write-is-refused)
- [A write that breaks a unique rule: `conflict:`](../language/04-failures.md#a-write-that-breaks-a-unique-rule-conflict)
- [`data.conflict_unmapped`](../errors/data.conflict_unmapped.md)
- [`failure.mapping_kind`](../errors/failure.mapping_kind.md)
- [`failure.duplicate_mapping`](../errors/failure.duplicate_mapping.md)
