# `step.unkeyed`

A changing call with no idempotency key is the only changing call in its
action, so a receipt sent after a payment belongs in a subscriber of the
payment's event:

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
