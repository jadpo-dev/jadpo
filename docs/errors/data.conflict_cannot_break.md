# `data.conflict_cannot_break`

An update can break only the unique rules over the fields it sets:

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
  help: docs/language/06-reads-and-writes.html#data-conflict-cannot-break

check failed: 1 error, 1 file
```

A conflict arm naming a failure of another kind is area 4's
`failure.mapping_kind`, an arm given twice `failure.duplicate_mapping`, and
`invalid:` in a write's block `failure.invalid_outside_construction`.
