# `data.erase_own_field`

A person's own record says something about erasure it can't: `erase:` on a field with anything but `Erasure.retained` or with no `personal:`, a reference that deletes a record that must stay, or a field named as the column erasure keeps.

## Why Jadpo refuses it

On a field of the person's own record, `erase:` says one thing: this field
stays when they are erased, because the law makes you keep it. Every other
marked field is cleared whatever you write, and the whole record is deleted
when nothing stays. So `Erasure.deleted` and `Erasure.anonymised` there
say nothing erasure doesn't already do. A field that isn't marked
`personal:` stays anyway, and `Erasure.retained` on it would keep the
person's record for no reason.

An entity that keeps a record this way can't have a reference that deletes
the record when another entity's records go, since that would take what
must stay. It also notes when each person was erased in a column called
`jadpo_erased_at`, so no field may be stored under that name.

## What it looks like

main.jadpo:

```jadpo check project=e-erase-own-field expect=data.erase_own_field
config {
    policy_retention: Duration {
        default: 1826d
    }
}

type PolicyNumber = Text {
    max_length: 20
}

entity Customer {
    id: Uuid
    email: Email? {
        personal: PersonalData.contact
    }
    policy_number: PolicyNumber {
        personal: PersonalData.identifying
        erase: Erasure.deleted
    }
    policy_ended_at: Instant?

    person: true

    retention: {
        after: config.policy_retention
        since: policy_ended_at
    }
}
```

```text output
error: `Customer.policy_number` is the person's own field, so only `Erasure.retained` means anything on it
 --> main.jadpo:18:9
   |
18 |         erase: Erasure.deleted
   |         ^^^^^ `Erasure.deleted`
  reason: On a field of the person's own record, `erase:` says one thing: this field stays when they are erased. Every other marked field is cleared, and the record is deleted when nothing stays, so `Erasure.deleted` says nothing a `Customer` doesn't already do.
  next: Remove `erase:` from `policy_number`, or say `erase: Erasure.retained` if the law makes you keep it (human)
  rule: data.erase_own_field
  help: https://jadpo.dev/docs/errors/data.erase_own_field

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether a person's data outlives their request to be forgotten.

## How to fix it

The question is whether the law makes you keep the field.

**If it does**, say so:

```jadpo fragment
policy_number: PolicyNumber {
    personal: PersonalData.identifying
    erase: Erasure.retained
}
```

```jadpo check project=e-erase-own-field-kept hidden
config {
    policy_retention: Duration {
        default: 1826d
    }
}

type PolicyNumber = Text {
    max_length: 20
}

entity Customer {
    id: Uuid
    email: Email? {
        personal: PersonalData.contact
    }
    policy_number: PolicyNumber {
        personal: PersonalData.identifying
        erase: Erasure.retained
    }
    policy_ended_at: Instant?

    person: true

    retention: {
        after: config.policy_retention
        since: policy_ended_at
    }
}
```

Erasing a customer now clears their email and keeps their policy number,
hidden, until their retention ends.

**If it doesn't**, remove `erase:`:

```jadpo fragment
policy_number: PolicyNumber {
    personal: PersonalData.identifying
}
```

```jadpo check project=e-erase-own-field-removed hidden
type PolicyNumber = Text {
    max_length: 20
}

entity Customer {
    id: Uuid
    email: Email? {
        personal: PersonalData.contact
    }
    policy_number: PolicyNumber {
        personal: PersonalData.identifying
    }

    person: true
}
```

Erasing a customer now deletes their whole record.

## Related

- [The person's own record](../language/15-personal-data.md#the-persons-own-record)
- [Erasure](../language/15-personal-data.md#erasure)
- [Retention](../language/15-personal-data.md#retention)
