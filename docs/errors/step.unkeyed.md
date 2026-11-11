# `step.unkeyed`

An action makes a changing call that takes no idempotency key, and another changing call beside it.

## Why Jadpo refuses it

An action that makes several changing calls records each one, so that a
retry replays the calls already answered and sends again only the one
in doubt: sent, with no answer recorded. A call with an idempotency key
can be sent again safely, since the provider answers a repeat under the
same key with the first answer. A call with no key may act twice if it
is sent again, and once its answer is lost only an operator can say
whether it went through. So a call like that stays the only changing
call in its action, where being in doubt holds up nothing else.

## What it looks like

domains/bakery/actions/pay_supplier.jadpo:

```jadpo check project=e-step-unkeyed file=app.jadpo hidden
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

```jadpo check project=e-step-unkeyed file=config.jadpo hidden
config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/provider.jadpo hidden
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

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
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

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/resources/receipts.jadpo hidden
public resource receipts {
    send: {
        method: HttpMethod.post
        path: "/v1/receipts"
        input: PaymentRequest
        output: Payment
        changes: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=e-step-unkeyed file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=e-step-unkeyed file=domains/bakery/actions/pay_supplier.jadpo expect=step.unkeyed
use app { pay }
use shared/providers/acme_pay

/// Pays a supplier's invoice, then sends them a receipt.
action pay_supplier(invoice: AcmePay.Reference) -> AcmePay.Payment {
    var payment: AcmePay.Payment = pay.payments.create(
        AcmePay.PaymentRequest {
            reference: invoice
        }
    )
    var receipt: AcmePay.Payment = pay.receipts.send(
        AcmePay.PaymentRequest {
            reference: payment.id
        }
    )
    assert receipt.id == payment.id
    return payment
}
```

```text output
error: `pay.receipts.send` takes no idempotency key, but `pay_supplier` makes another changing call, `pay.payments.create`
 --> domains/bakery/actions/pay_supplier.jadpo:11:36
   |
11 |     var receipt: AcmePay.Payment = pay.receipts.send(
   |                                    ^^^^^^^^^^^^^^^^^ no key
  reason: A changing call with no key may act twice if it is sent again, and once its answer is lost only an operator can say whether it went through, so it stays the only changing call in its action.
  next: Move `pay.receipts.send` to a subscriber of an event `pay_supplier` emits (agent)
  rule: step.unkeyed
  help: https://jadpo.dev/docs/errors/step.unkeyed

check failed: 1 error, 10 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the provider takes an idempotency key for the
call.

**If Acme Pay takes a key when sending a receipt**, say so on the
operation in its package. In
`shared/providers/acme_pay/v1/resources/receipts.jadpo`:

```jadpo fragment
    send: {
        ...
        changes: true
        idempotent: true
        ...
    }
```

```jadpo check project=e-step-unkeyed-fix-1 file=shared/providers/acme_pay/v1/resources/receipts.jadpo hidden
public resource receipts {
    send: {
        method: HttpMethod.post
        path: "/v1/receipts"
        input: PaymentRequest
        output: Payment
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-unkeyed-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-step-unkeyed-fix-1 file=config.jadpo hidden
config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-unkeyed-fix-1 file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-step-unkeyed-fix-1 file=shared/providers/acme_pay/v1/provider.jadpo hidden
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

```jadpo check project=e-step-unkeyed-fix-1 file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
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

```jadpo check project=e-step-unkeyed-fix-1 file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=e-step-unkeyed-fix-1 file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=e-step-unkeyed-fix-1 file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

```jadpo check project=e-step-unkeyed-fix-1 file=domains/bakery/actions/pay_supplier.jadpo hidden
use app { pay }
use shared/providers/acme_pay

/// Pays a supplier's invoice, then sends them a receipt.
action pay_supplier(invoice: AcmePay.Reference) -> AcmePay.Payment {
    var payment: AcmePay.Payment = pay.payments.create(
        AcmePay.PaymentRequest {
            reference: invoice
        }
    )
    var receipt: AcmePay.Payment = pay.receipts.send(
        AcmePay.PaymentRequest {
            reference: payment.id
        }
    )
    assert receipt.id == payment.id
    return payment
}
```

Each receipt is now sent with a key made from its step, as the payment
is, so a retry that sends it again gets the first answer back rather
than a second receipt. Say it only if the provider does take one: the
key goes in the header the package's `idempotency:` names.

**If it takes none**, send the receipt from a subscriber of an event
`pay_supplier` emits, so it is the only changing call in its own
action. In `domains/bakery/actions/pay_supplier.jadpo`:

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

```jadpo check project=e-step-unkeyed-fix-2 file=domains/bakery/actions/pay_supplier.jadpo hidden
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

```jadpo check project=e-step-unkeyed-fix-2 file=domains/bakery/events/supplier_paid.jadpo
use shared/providers/acme_pay

/// A supplier's invoice was paid.
type SupplierPaid = Event {
    payment_id: AcmePay.Reference
}
```

`domains/bakery/subscribers/send_supplier_receipt.jadpo`:

```jadpo check project=e-step-unkeyed-fix-2 file=domains/bakery/subscribers/send_supplier_receipt.jadpo
use bakery/actions { send_receipt }
use bakery/events { SupplierPaid }

/// Sends the receipt once a supplier is paid.
subscriber send_supplier_receipt {
    on: SupplierPaid
    run: send_receipt(event)
}
```

`domains/bakery/actions/send_receipt.jadpo`:

```jadpo check project=e-step-unkeyed-fix-2 file=domains/bakery/actions/send_receipt.jadpo
use app { pay }
use bakery/events { SupplierPaid }
use shared/providers/acme_pay

/// Sends a supplier the receipt for a payment.
action send_receipt(paid: SupplierPaid) -> AcmePay.Payment {
    return pay.receipts.send(
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

```jadpo check project=e-step-unkeyed-fix-2 file=app.jadpo hidden
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

```jadpo check project=e-step-unkeyed-fix-2 file=config.jadpo hidden
config {
    main_url: PostgresUrl
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
```

```jadpo check project=e-step-unkeyed-fix-2 file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-step-unkeyed-fix-2 file=shared/providers/acme_pay/v1/provider.jadpo hidden
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

```jadpo check project=e-step-unkeyed-fix-2 file=shared/providers/acme_pay/v1/resources/payments.jadpo hidden
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

```jadpo check project=e-step-unkeyed-fix-2 file=shared/providers/acme_pay/v1/resources/receipts.jadpo hidden
public resource receipts {
    send: {
        method: HttpMethod.post
        path: "/v1/receipts"
        input: PaymentRequest
        output: Payment
        changes: true
        source: PackageSource.written
    }
}
```

```jadpo check project=e-step-unkeyed-fix-2 file=shared/providers/acme_pay/v1/types/payment_request.jadpo hidden
/// What a payment, a receipt or a statement is for.
public type PaymentRequest = Object {
    reference: Reference
}
```

```jadpo check project=e-step-unkeyed-fix-2 file=shared/providers/acme_pay/v1/types/payment.jadpo hidden
/// What Acme Pay gives back.
public type Payment = Object {
    id: Reference
}
```

```jadpo check project=e-step-unkeyed-fix-2 file=shared/providers/acme_pay/v1/types/reference.jadpo hidden
/// Acme Pay's reference for a record.
public type Reference = Text {
    max_length: 40
}
```

The payment and the receipt are now separate runs. The receipt goes
only once the payment's run has committed, and if its answer is lost,
only that delivery waits for an operator; the payment isn't held up.

## Related

- [What is allowed](../language/12-jobs-and-events.md#what-is-allowed)
- [Providers that take no key](../language/12-jobs-and-events.md#providers-that-take-no-key)
- [A step in doubt](../language/12-jobs-and-events.md#a-step-in-doubt)
- [Subscribers](../language/12-jobs-and-events.md#subscribers)
