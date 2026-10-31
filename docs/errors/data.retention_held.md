# `data.retention_held`

An entity's `retention:` would delete records that a `restricted` reference keeps from being deleted.

## Why Jadpo refuses it

A `retention:` on an entity deletes each record when its time is up,
with everything that cascades from it. A reference marked
`OnDelete.restricted` says the opposite: the record it points at can't
be deleted while it does. Here an invoice is kept for six years, but a
payment refers to it with a restricted reference, so every invoice that
was paid would stay past its retention, and the app would never know.

Jadpo won't choose for you: what happens to a payment when its invoice
goes is a decision about your records.

## What it looks like

main.jadpo:

```jadpo check project=e-retention-held expect=data.retention_held
config {
    invoice_retention: Duration {
        default: 2192d
    }
}

entity Invoice {
    id: Uuid
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}

entity Payment {
    id: Uuid
    invoice_id: Invoice.id {
        on_delete: OnDelete.restricted
    }
}
```

```text output
error: `Invoice`'s retention can't delete an invoice a `Payment` holds
 --> main.jadpo:13:5
   |
13 |     retention: {
   |     ^^^^^^^^^ deletes each invoice when its time is up
21 |     invoice_id: Invoice.id {
   |     ---------- `Payment.invoice_id` is `OnDelete.restricted`
  reason: Retention deletes each record when its time is up, with what cascades from it. `Payment.invoice_id` is `OnDelete.restricted`, which refuses to delete the record it refers to, so an invoice it holds would be kept past its retention.
  next: Say what deleting the record does to `Payment`: `OnDelete.cascade` on `invoice_id` deletes it too, and `OnDelete.cleared` keeps it without the reference (human)
  rule: data.retention_held
  help: https://jadpo.dev/docs/errors/data.retention_held

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether a payment is kept once its invoice is deleted.

A reference that holds the record through a cascade is refused the same
way: if deleting an invoice deleted its lines, a refund holding a line
with a restricted reference would keep the invoice too.

## How to fix it

The question is what should happen to a payment when its invoice's time
is up.

**If the payment goes with its invoice**, make the reference cascade:

```jadpo fragment
entity Payment {
    id: Uuid
    invoice_id: Invoice.id {
        on_delete: OnDelete.cascade
    }
}
```

```jadpo check project=e-retention-held-cascade hidden
config {
    invoice_retention: Duration {
        default: 2192d
    }
}

entity Invoice {
    id: Uuid
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}

entity Payment {
    id: Uuid
    invoice_id: Invoice.id {
        on_delete: OnDelete.cascade
    }
}
```

An invoice past its retention is now deleted with its payments. Ordinary
code deleting an invoice deletes its payments too.

**If the payment is kept without its invoice**, make the reference one
that can be `none` and is cleared:

```jadpo fragment
entity Payment {
    id: Uuid
    invoice_id: Invoice.id? {
        on_delete: OnDelete.cleared
    }
}
```

```jadpo check project=e-retention-held-cleared hidden
config {
    invoice_retention: Duration {
        default: 2192d
    }
}

entity Invoice {
    id: Uuid
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}

entity Payment {
    id: Uuid
    invoice_id: Invoice.id? {
        on_delete: OnDelete.cleared
    }
}
```

An invoice past its retention is now deleted, and each of its payments
stays with `invoice_id` set to `none`.

## Related

- [Retention](../language/15-personal-data.md#retention)
- [On an entity](../language/15-personal-data.md#on-an-entity)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
