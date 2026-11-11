# Keep a record of who changed what
<!-- topic: guide-keep-a-record-of-who-changed-what -->

An entity keeps no history of its changes unless you ask. Give it an `audit:`
section and Jadpo writes down each change in the same transaction: who made it,
when, through which action and request, and each changed field's value before
and after. You write no logging code, and nothing can change a record without
its entry. This guide keeps the history of a book-keeper's invoices and reads it
back through a route.

You start from an app with a `Clerk` entity that people sign in as, and actions
called `hire_clerk` and `annotate_invoice`. [Add sign-in](05-add-sign-in.md) and
[Store and change a record](02-store-and-change-a-record.md) show how to write
those. The first step needs neither.

```jadpo check project=guide-audit file=app.jadpo hidden
use billing/entities { Clerk }

application {}

principal {
    user: {
        subject: Clerk.handle
    }
}

authentication {
    api_token: {}
}
```

```jadpo check project=guide-audit file=domains/billing/types/handle.jadpo hidden
/// A clerk's handle.
type Handle = Text {
    max_length: 40
}
```

```jadpo check project=guide-audit file=domains/billing/types/invoice_note.jadpo hidden
/// A line of text about an invoice.
type InvoiceNote = Text {
    max_length: 200
}
```

```jadpo check project=guide-audit file=domains/billing/entities/clerk.jadpo hidden
use billing/types { Handle }

/// Someone who keeps the books.
entity Clerk {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }
}
```

```jadpo check project=guide-audit file=domains/billing/failures/handle_taken.jadpo hidden
/// Someone has that handle already.
type HandleTaken = Failure {
    kind: FailureKind.conflict
}
```

```jadpo check project=guide-audit file=domains/billing/actions/hire_clerk.jadpo hidden
use billing/entities { Clerk }
use billing/types { Handle }
use billing/failures { HandleTaken }

/// Takes on a clerk.
action hire_clerk(handle: Handle) fails HandleTaken -> Clerk {
    return attempt Clerk.create {
        set: {
            handle: handle
        }
        conflict: {
            Clerk.handle: HandleTaken
        }
    }
}
```

```jadpo check project=guide-audit file=domains/billing/actions/annotate_invoice.jadpo hidden
use billing/entities { Invoice, InvoiceNotFound }
use billing/types { InvoiceNote }

/// Notes something on an invoice.
action annotate_invoice(
    me: Principal.user,
    invoice_id: Invoice.id,
    note: InvoiceNote
) fails InvoiceNotFound -> Invoice {
    return attempt Invoice.update(invoice_id) {
        set: {
            note: note
        }
    }
}
```

## Turn it on

`audit:` goes on the entity. `retention:` says how long each entry is kept,
counted from when it was written, and takes a `Duration` setting as every period
does. Write `audit: {}` to keep each entry until its record is erased.

`audit_retention` is a setting of your own, in `config.jadpo`. `Duration` is built
into Jadpo.

`config.jadpo`:

```jadpo check project=guide-audit file=config.jadpo
config {
    /// How long each invoice change is kept: seven years.
    audit_retention: Duration {
        default: 2557d
    }
}
```

`Invoice` is your entity. Its `number` is an `InvoiceNumber`, a standard type built
into Jadpo.

`domains/billing/entities/invoice.jadpo`:

```jadpo check project=guide-audit file=domains/billing/entities/invoice.jadpo
/// A bill, whose every change is kept for seven years.
entity Invoice {
    id: Uuid
    number: InvoiceNumber

    policy: {
        create: [Access.public]
        read: [Access.public]
    }

    audit: {
        retention: {
            after: config.audit_retention
        }
    }
}
```

An ordinary action changes it. You add nothing to the action for the change to be
recorded.

`domains/billing/actions/issue_invoice.jadpo`:

```jadpo check project=guide-audit file=domains/billing/actions/issue_invoice.jadpo
use billing/entities { Invoice }

/// Issues an invoice.
action issue_invoice(number: InvoiceNumber) -> Invoice {
    return Invoice.create {
        set: {
            number: number
        }
    }
}
```

The entries live in a table Jadpo owns, `jadpo_audit`, in the same store as the
record, so make a migration:

```sh
jadpo migrate new audit_invoices
```

From now on, every write to an invoice adds an entry. Nothing in your code can
change or remove one. A field no request may touch, `FieldAccess.server_only`, is
left out.

An audited entity can't hold an `Email`, a field marked `personal:`, or a `File`
field yet. Keep them on an entity with no `audit:`, such as a `Customer` that the
invoice refers to. The last section of this page says more.

## Say who may see which field

A history is read as the caller may see it, so an invoice worth reading has people
and fields with different rules. Here each invoice belongs to the clerk who issued
it, and only they may read its `discount`.

`domains/billing/entities/invoice.jadpo`, changed. `clerk_id`, `note` and `discount`
are new, and `policy:` now names the clerk:

```jadpo check project=guide-audit file=domains/billing/entities/invoice.jadpo
use billing/entities { Clerk }
use billing/types { InvoiceNote }

/// A bill, whose every change is kept for seven years.
entity Invoice {
    id: Uuid
    clerk_id: Clerk.id {
        on_delete: OnDelete.restricted
    }
    number: InvoiceNumber
    note: InvoiceNote?
    discount: Int {
        policy: {
            read: [clerk_id]
            update: [clerk_id]
        }
    }

    policy: {
        create: [clerk_id]
        read: [Access.authenticated]
        update: [clerk_id]
    }

    audit: {
        retention: {
            after: config.audit_retention
        }
    }
}
```

`Clerk` is who signs in, and `InvoiceNote` is a short text type, both from the app you
start from.

`domains/billing/actions/issue_invoice.jadpo`, changed. It now takes the signed-in clerk
and sets the new fields:

```jadpo check project=guide-audit file=domains/billing/actions/issue_invoice.jadpo
use billing/entities { Invoice }

/// Issues an invoice.
action issue_invoice(me: Principal.user, number: InvoiceNumber) -> Invoice {
    return Invoice.create {
        set: {
            clerk_id: me.id
            number: number
            note: none
            discount: 5
        }
    }
}
```

Run `jadpo migrate new invoice_clerks` for the new columns.

## Read the history

`Invoice.history(invoice_id)` is a query Jadpo generates. It gives one page of 25
entries, newest first, and its `next` is the cursor for the page after. Each is
an `AuditEntry`:

| Field | Holds |
|---|---|
| `at` | When the change was made |
| `change` | `AuditChange.created`, `.updated` or `.deleted` |
| `by` | `AuditBy.user`, `.service`, `.application` (a schedule or a subscriber) or `.no_caller` (a test's, a seed's or a tool's call without a caller) |
| `user_id`, `service_id` | The signed-in user's or service's `id`, or `none` |
| `action` | The outermost action the change was made through, by name |
| `request` | The request's id, when a request made the change |
| `trace` | The trace id it was made in, which a subscriber keeps from the request that emitted its event |
| `fields` | Each changed field: its name, `field`, and its text `before` and `after`, `none` where it had no value |

A history is read as the caller may see it. It needs the `read` grant on the
record and leaves out the fields the caller can't read, so here another clerk
sees each change without the `discount`, which only the invoice's clerk may read.
A caller who may not read the record gets an empty page.

## Send it through a route

A route can't send an `AuditEntry` as it is, because it isn't an `Output`. You
build an `Output` of your own, which also makes you choose what your API's clients
see.

`domains/billing/types/change_line.jadpo`:

```jadpo check project=guide-audit file=domains/billing/types/change_line.jadpo
/// One field an invoice's change touched.
type ChangeLine = Output {
    field: Text
    before: Text?
    after: Text?
}
```

`domains/billing/types/invoice_change.jadpo`:

```jadpo check project=guide-audit file=domains/billing/types/invoice_change.jadpo
use billing/types { ChangeLine }

/// One change to an invoice, as the API sends it.
type InvoiceChange = Output {
    at: Instant
    action: Text?
    user_id: Uuid?
    fields: List<ChangeLine>
}
```

`domains/billing/actions/invoice_changes.jadpo`:

```jadpo check project=guide-audit file=domains/billing/actions/invoice_changes.jadpo
use billing/entities { Invoice }
use billing/types { ChangeLine, InvoiceChange }

/// The invoice's latest changes, newest first, as the caller may see them.
action invoice_changes(me: Principal.user, invoice_id: Invoice.id) -> List<InvoiceChange> {
    var page: Page<AuditEntry> = Invoice.history(invoice_id)
    return list.map(
        page.items,
        entry -> InvoiceChange {
            at: entry.at
            action: entry.action
            user_id: entry.user_id
            fields: list.map(
                entry.fields,
                line -> ChangeLine {
                    field: line.field
                    before: line.before
                    after: line.after
                }
            )
        }
    )
}
```

`domains/billing/routes/invoices.jadpo`:

```jadpo check project=guide-audit file=domains/billing/routes/invoices.jadpo
use billing/entities { Invoice }
use billing/types { InvoiceChange }
use billing/actions { invoice_changes }

/// The office's invoices.
routes Invoices {
    prefix: /invoices

    /// One invoice's latest changes, newest first.
    GET /{invoice_id: Invoice.id}/changes {
        output: List<InvoiceChange>
        run: invoice_changes(current_principal.user, invoice_id)
    }
}
```

Run `jadpo api update` to bring the API record up to date. This route gives the
latest 25 changes. For more, take a `PageCursor` from the caller and pass it as
`Invoice.history(invoice_id, after: after)`.

## Test it

These tests make changes through the actions, as a person would, and read them
back.

`domains/billing/actions/invoice_changes.test.jadpo`:

```jadpo check project=guide-audit file=domains/billing/actions/invoice_changes.test.jadpo
use billing/entities { Clerk, Invoice }
use billing/types { InvoiceChange }
use billing/actions { annotate_invoice, hire_clerk, issue_invoice }

test "each change to an invoice comes back newest first" {
    var ada: Clerk = attempt hire_clerk("ada")
    var invoice: Invoice = issue_invoice(Principal.user(ada), "INV-1")
    attempt annotate_invoice(Principal.user(ada), invoice.id, "Paid by card")
    var changes: List<InvoiceChange> = invoice_changes(Principal.user(ada), invoice.id)
    assert list.map(changes, change -> change.action) == ["annotate_invoice", "issue_invoice"]
    assert list.map(changes, change -> list.map(change.fields, line -> line.field)) == [
        ["note"],
        ["clerk_id", "number", "discount"]
    ]
    assert list.any(
        changes,
        change -> list.any(change.fields, line -> line.after == "Paid by card")
    )
}

test "another clerk sees each change without the discount" {
    var ada: Clerk = attempt hire_clerk("ada")
    var bea: Clerk = attempt hire_clerk("bea")
    var invoice: Invoice = issue_invoice(Principal.user(ada), "INV-2")
    var changes: List<InvoiceChange> = invoice_changes(Principal.user(bea), invoice.id)
    assert list.map(changes, change -> list.map(change.fields, line -> line.field)) == [
        ["clerk_id", "number"]
    ]
}
```

`domains/billing/routes/invoices.test.jadpo`:

```jadpo check project=guide-audit file=domains/billing/routes/invoices.test.jadpo
use billing/entities { Clerk, Invoice }
use billing/actions { hire_clerk, issue_invoice }

test "the route answers with the changes, for anyone signed in" {
    var ada: Clerk = attempt hire_clerk("ada")
    var invoice: Invoice = issue_invoice(Principal.user(ada), "INV-3")
    var response: Response = request {
        method: HttpMethod.get
        path: "/invoices/{invoice_id}/changes"
        params: {
            invoice_id: invoice.id
        }
        as: Principal.user(ada)
    }
    assert response.status == 200
}
```

```sh
jadpo test
```

```text
tested: 3 passed, 2 action tests, 1 route test
```

## How long entries last

The entity's built-in purge schedule deletes each entry when its retention ends.
An entry outlives its record otherwise: deleting an invoice leaves its history
until the history's own retention ends, though `history` then gives an empty page.
A person's erasure deletes the history of every record it deletes.

The audit trail records changes to data. Who read or exported a person's data is
kept in [access records](../language/15-personal-data.md#access-records) instead.

To send entries somewhere else, subscribe to the built-in `AuditEntryRecorded`
event. Each entry is then also emitted in the same transaction, and your subscriber
gets it like any other event. The database stays the record of truth.

## Read more

[Audit trail](../language/16-built-in-services.md#audit-trail),
[Subscribers](../language/12-jobs-and-events.md#subscribers),
[The permissions report](../language/09-policy.md#the-permissions-report) and
[Marked fields in logs](../language/15-personal-data.md#marked-fields-in-logs).

## Not built yet

An audited entity can't yet hold a value its history would have to keep in a way
Jadpo hasn't built. Each of these is `toolchain.not_built_yet` where it is written:

- An `Email`, a field marked `personal:`, or a standard type that is personal data
  whatever holds it, such as a `PhoneNumber`.
- A `File` field, or a field with a `retention:` of its own.
- A person's record removed other than by their erasure, such as by a `delete` in
  code or by a cascade.
- A `delete` whose cascade deletes, or whose `OnDelete.cleared` changes, an audited
  record. Make the reference `OnDelete.restricted`, as `clerk_id` is here, and
  change those records first.
- An erasure that keeps an audited record, anonymised or retained.

An `Email` on `Invoice` gives this, its `rule:` and `help:` lines left out:

```text
error: Auditing `Invoice` isn't built yet
 --> domains/billing/entities/invoice.jadpo:26:5
   |
12 |     customer_email: Email
   |     -------------- an `Email`
26 |     audit: {
   |     ^^^^^ decided, not built
  reason: `customer_email` is an `Email`, personal data (`PersonalData.contact`) whatever holds it, and its history keeps a personal value only as a keyed hash, as logs do, which isn't built yet (area 15, "Marked fields in logs"): accepted, the value would be copied into every entry.
  next: Leave `audit:` out of `Invoice` for now (agent)
```

To send each entry to a SIEM or log archive, list `AuditEntryRecorded` in
`publishing:`, through a connection to the archive's address
([Sending audit entries elsewhere](../language/16-built-in-services.md#sending-audit-entries-elsewhere)).
