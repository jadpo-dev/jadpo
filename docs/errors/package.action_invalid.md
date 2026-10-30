# `package.action_invalid`

A package's action calls only its own API: its operations, in any order,
and its pure functions, never another action, so what it does is one way
through it that a retry replays in order. It never emits when it is
`public`, since a package emits only about the outside world, and it
never shares a name with one of its resources, which share its
connection's namespace. A package's code also reads none of the app's
settings (`config.<name>`, keys included) and calls none of its sign-in
operations (`sessions.*`, `magic_links.*`), which are the app's own:

```jadpo check project=p76-calls file=app.jadpo hidden
application {}
```

```jadpo check project=p76-calls file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p76-calls file=shared/providers/acme_pay/v1/provider.jadpo hidden
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
    }
    error_code: "error.code"
}
```

```jadpo check project=p76-calls file=shared/providers/acme_pay/v1/types/pay_request.jadpo hidden
/// What each call takes.
public type PayRequest = Object {
    reference: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-calls file=shared/providers/acme_pay/v1/types/pay_reply.jadpo hidden
/// What each call gives.
public type PayReply = Object {
    id: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-calls file=shared/providers/acme_pay/v1/resources/customers.jadpo hidden
resource customers {
    create: {
        method: HttpMethod.post
        path: "/v1/customers"
        input: PayRequest
        output: PayReply
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
```

```jadpo check project=p76-calls file=shared/providers/acme_pay/v1/actions/open_customer.jadpo hidden
/// Opens the customer.
public action open_customer(input: PayRequest) -> PayReply {
    return customers.create(input)
}
```

```jadpo check project=p76-calls file=shared/providers/acme_pay/v1/actions/start_subscription.jadpo expect=package.action_invalid
/// Opens the customer, through the other action.
public action start_subscription(input: PayRequest) -> PayReply {
    return open_customer(input)
}
```

```text output
error: `start_subscription` calls the package's action `open_customer`
 --> shared/providers/acme_pay/v1/actions/start_subscription.jadpo:3:12
  |
3 |     return open_customer(input)
  |            ^^^^^^^^^^^^^ another action
  reason: A package's action calls only its own API: its operations, in any order, and its pure functions. The steps it makes are one way through it, which a retry replays in order.
  next: Make `open_customer`'s calls here, or move what both share to a function (agent)
  rule: package.action_invalid
  help: https://jadpo.dev/docs/errors/package.action_invalid

check failed: 1 error, 8 files
```
