# `step.large_output`

An answer kept for a rerun is at most 1 MB, so an operation its package
marks `large_output: true` is never one of several changing calls:

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
  help: docs/errors/step.large_output.md

check failed: 1 error, 10 files
```
