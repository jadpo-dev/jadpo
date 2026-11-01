# `data.erase_retention_required`

Erasing a person keeps records with `Erasure.retained`, their own or others', but their entity has no `retention:` saying when they go.

## Why Jadpo refuses it

`Erasure.retained` keeps a record as it is when the person it refers to
is erased, clearing only the reference. It is for records the law makes
you keep, such as invoices, which tax law requires for several years.
A record kept that way still holds personal data, its own copy of the
billing name, so it needs an end date of its own. Without a
`retention:` on its entity it would be kept for ever. A person's own field
that says `erase: Erasure.retained` keeps their record the same way, and
needs the same.

Jadpo won't choose for you: how long a kept record stays is a legal
decision about your records.

## What it looks like

main.jadpo:

```jadpo check project=e-erase-retention-required expect=data.erase_retention_required
type BillingName = Text {
    max_length: 80
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }
    billing_name: BillingName {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }
}
```

```text output
error: `Invoice` has no retention, so an invoice kept after erasure would never go
 --> main.jadpo:18:16
   |
18 |         erase: Erasure.retained
   |                ^^^^^^^^^^^^^^^^ keeps the record as it is
  reason: `Erasure.retained` on `customer_id` keeps each invoice as it is when a `User` is erased, for a reason the law gives, such as tax records, until its own retention ends. `Invoice` has no `retention:`, so it would be kept for ever.
  next: Give `Invoice` a `retention: { after: config.<setting> since: <field> }` saying how long it is kept, or use `Erasure.deleted` or `Erasure.anonymised` on `customer_id` (human)
  rule: data.erase_retention_required
  help: https://jadpo.dev/docs/errors/data.erase_retention_required

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how long an erased person's invoices are kept.

## How to fix it

The question is how long the law makes you keep an invoice.

**If it is kept for a period from when it was issued**, give the entity
a `retention:` whose period is a setting:

```jadpo fragment
config {
    invoice_retention: Duration {
        default: 2192d
    }
}

entity Invoice {
    ...
    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}
```

```jadpo check project=e-erase-retention-required-kept hidden
config {
    invoice_retention: Duration {
        default: 2192d
    }
}

type BillingName = Text {
    max_length: 80
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }
    billing_name: BillingName {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}
```

Erasing a user now keeps each of their invoices with `customer_id` set
to `none`, and the purge schedule deletes it six years after it was
issued.

**If nothing makes you keep it**, delete it with its customer:

```jadpo fragment
entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    ...
}
```

```jadpo check project=e-erase-retention-required-deleted hidden
type BillingName = Text {
    max_length: 80
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}

entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    billing_name: BillingName {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }
}
```

Erasing a user now deletes their invoices, and ordinary code still can't
delete a user who has some.

## Related

- [Keeping a record without the person](../language/15-personal-data.md#keeping-a-record-without-the-person-erasureretained)
- [Retention](../language/15-personal-data.md#retention)
- [On an entity](../language/15-personal-data.md#on-an-entity)
