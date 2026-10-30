# `data.conflict_unknown`

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
