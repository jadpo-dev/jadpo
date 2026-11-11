# `step.large_output`

An action makes several changing calls, and one of them is an operation its package marks `large_output: true`.

## Why Jadpo refuses it

An action that makes several changing calls keeps each call's answer,
encrypted, so that a retry replays it rather than sending the call
again. A kept answer is at most 1 MB, and an answer over that is a fault
that stops the action. A package marks an operation `large_output: true`
when its answer can be bigger, such as a month's bank statement, so
Jadpo refuses it among several changing calls at `jadpo check` rather
than let the action fail in production the first time an answer is too
big to keep.

## What it looks like

domains/bakery/actions/pay_supplier.jadpo:

```jadpo check project=e-step-large-output file=app.jadpo hidden
application {
    connections: {
        pay: {
            package: AcmePay.v1
            domains: [bakery]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-large-output file=config.jadpo hidden
config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
public resource payments {
    create: {
        method: HttpMethod.post
        path: "/v1/payments"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/resources/bank_statements.jadpo hidden
public resource bank_statements {
    export: {
        method: HttpMethod.post
        path: "/v1/bank_statements"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        large_output: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=e-step-large-output file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=e-step-large-output file=domains/bakery/actions/pay_supplier.jadpo expect=step.large_output
use app { pay }
use shared/providers/acme_pay

/// Pays a supplier's invoice, then exports the month's statement.
action pay_supplier(invoice: AcmePay.Reference) -> AcmePay.Payment {
    var payment: AcmePay.Payment = pay.payments.create(
        AcmePay.PaymentRequest {
            reference: invoice
        }
    )
    var bank_statement: AcmePay.Payment = pay.bank_statements.export(
        AcmePay.PaymentRequest {
            reference: payment.id
        }
    )
    assert bank_statement.id == payment.id
    return payment
}
```

```text output
error: `pay.bank_statements.export` gives a large answer, and `pay_supplier` makes several changing calls
 --> domains/bakery/actions/pay_supplier.jadpo:11:43
   |
11 |     var bank_statement: AcmePay.Payment = pay.bank_statements.export(
   |                                           ^^^^^^^^^^^^^^^^^^^^^^^^^^ `large_output: true`
  reason: In an action with several changing calls, each answer is kept, up to 1 MB, for a retry to replay, and its package marks this operation's answer as large.
  next: Move `pay.bank_statements.export` to an action of its own, run by a subscriber of an event `pay_supplier` emits (agent)
  rule: step.large_output
  help: https://jadpo.dev/docs/errors/step.large_output

check failed: 1 error, 10 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the export can follow the payment**, make it its own action, run
by a subscriber of an event `pay_supplier` emits. There it is the only
changing call, so its answer isn't kept for a replay. In
`domains/bakery/actions/pay_supplier.jadpo`:

```jadpo fragment
action pay_supplier(invoice: AcmePay.Reference) -> AcmePay.Payment {
    var payment: AcmePay.Payment = pay.payments.create(
        ...
    )
    emit SupplierPaid {
        payment_id: payment.id
    }
    return payment
}
```

```jadpo check project=e-step-large-output-fix-1 file=domains/bakery/actions/pay_supplier.jadpo hidden
use app { pay }
use bakery/events { SupplierPaid }
use shared/providers/acme_pay

/// Pays a supplier's invoice, and says so.
action pay_supplier(invoice: AcmePay.Reference) -> AcmePay.Payment {
    var payment: AcmePay.Payment = pay.payments.create(
        AcmePay.PaymentRequest {
            reference: invoice
        }
    )
    emit SupplierPaid {
        payment_id: payment.id
    }
    return payment
}
```

`domains/bakery/events/supplier_paid.jadpo`:

```jadpo check project=e-step-large-output-fix-1 file=domains/bakery/events/supplier_paid.jadpo
use shared/providers/acme_pay

/// A supplier's invoice was paid.
type SupplierPaid = Event {
    payment_id: AcmePay.Reference
}
```

`domains/bakery/subscribers/export_supplier_statement.jadpo`:

```jadpo check project=e-step-large-output-fix-1 file=domains/bakery/subscribers/export_supplier_statement.jadpo
use bakery/actions { export_statement }
use bakery/events { SupplierPaid }

/// Exports the statement once a supplier is paid.
subscriber export_supplier_statement {
    on: SupplierPaid
    run: export_statement(event)
}
```

`domains/bakery/actions/export_statement.jadpo`:

```jadpo check project=e-step-large-output-fix-1 file=domains/bakery/actions/export_statement.jadpo
use app { pay }
use bakery/events { SupplierPaid }
use shared/providers/acme_pay

/// Exports the statement for a payment.
action export_statement(paid: SupplierPaid) -> AcmePay.Payment {
    return pay.bank_statements.export(
        AcmePay.PaymentRequest {
            reference: paid.payment_id
        }
    )
}
```

An event waits in a database's outbox, so `app.jadpo` names one for
the events of an action that writes none:

```jadpo fragment
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        ...
    }
    events: {
        store: main
    }
```

```jadpo check project=e-step-large-output-fix-1 file=app.jadpo hidden
application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        pay: {
            package: AcmePay.v1
            domains: [bakery]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
    events: {
        store: main
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
```

```jadpo check project=e-step-large-output-fix-1 file=config.jadpo hidden
config {
    main_url: PostgresUrl
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-large-output-fix-1 file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-step-large-output-fix-1 file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    idempotency: {
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
    }
}
```

```jadpo check project=e-step-large-output-fix-1 file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
public resource payments {
    create: {
        method: HttpMethod.post
        path: "/v1/payments"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-large-output-fix-1 file=shared/providers/acme_pay/v1/resources/bank_statements.jadpo hidden
public resource bank_statements {
    export: {
        method: HttpMethod.post
        path: "/v1/bank_statements"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        large_output: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-large-output-fix-1 file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=e-step-large-output-fix-1 file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=e-step-large-output-fix-1 file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

`pay_supplier` now makes one changing call. The statement is exported
by its own delivery once the payment's run has committed, however large
its answer.

## Related

- [What is allowed](../language/12-jobs-and-events.md#what-is-allowed)
- [The record](../language/12-jobs-and-events.md#the-record)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)
