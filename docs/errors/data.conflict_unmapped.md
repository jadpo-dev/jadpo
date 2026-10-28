# `data.conflict_unmapped`

Each unique rule and restricting reference a write can break needs an arm
in `conflict:`; there is no catch-all:

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
  help: docs/language/06-reads-and-writes.html#data-conflict-unmapped

check failed: 1 error, 1 file
```
