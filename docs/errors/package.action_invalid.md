# `package.action_invalid`

A package's action does something package code may not.

## Why Jadpo refuses it

A package's action calls only its own API: its operations, in any order, and
its pure functions. So what it does is one way through it, which a retry
replays in order. It never calls another action, and it never emits when it
is `public`, since a package emits only about the outside world. A
package's code also acts on its own API and never on the app: it reads none
of the app's settings, keys included, and calls none of its sign-in
operations. And an action never shares a name with one of the package's
resources, which share its connection's namespace.

## What it looks like

One public action calls another:

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

`shared/providers/acme_pay/v1/actions/start_subscription.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A package that reads one of the app's settings is this error too:

```jadpo check project=p76-config file=app.jadpo hidden
application {}
```

```jadpo check project=p76-config file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p76-config file=shared/providers/acme_pay/v1/provider.jadpo hidden
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

```jadpo check project=p76-config file=shared/providers/acme_pay/v1/types/pay_request.jadpo hidden
/// What each call takes.
public type PayRequest = Object {
    reference: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-config file=shared/providers/acme_pay/v1/types/pay_reply.jadpo hidden
/// What each call gives.
public type PayReply = Object {
    id: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-config file=shared/providers/acme_pay/v1/resources/customers.jadpo hidden
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

```jadpo check project=p76-config file=config.jadpo hidden
config {
    use_customers: Bool {
        default: true
    }
}
```

`shared/providers/acme_pay/v1/actions/open_customer.jadpo`:

```jadpo check project=p76-config file=shared/providers/acme_pay/v1/actions/open_customer.jadpo expect=package.action_invalid
/// Opens the customer, if the app allows it.
public action open_customer(input: PayRequest) -> PayReply? {
    if (config.use_customers) {
        return customers.create(input)
    }
    return none
}
```

```text output
error: `AcmePay.v1` reads the app's setting `config.use_customers`
 --> shared/providers/acme_pay/v1/actions/open_customer.jadpo:3:9
  |
3 |     if (config.use_customers) {
  |         ^^^^^^^^^^^^^^^^^^^^ the app's setting
  reason: A package acts on its own API, never on the app: its code reads none of the app's settings, its keys included, so it can't send them anywhere or sign with them. What a provider needs reaches it through the connection's `credential:`, which the runtime sends.
  next: Take what `config.use_customers` gives as the action's input from the app's own action, or name it in the connection's `credential:` (agent)
  rule: package.action_invalid
  help: https://jadpo.dev/docs/errors/package.action_invalid

check failed: 1 error, 8 files
```

Emitting from a `public` action, emitting an event its provider's webhooks
bring, calling `sessions.*` or `magic_links.*`, and an action named like a
resource are each this error, with a message of their own.

## How to fix it

**If an action calls another**, make that action's calls in this one, or
move what both share to a function:

```jadpo check project=p76-calls-fix file=shared/providers/acme_pay/v1/actions/start_subscription.jadpo
/// Opens the customer, as its first step.
public action start_subscription(input: PayRequest) -> PayReply {
    return customers.create(input)
}
```

```jadpo check project=p76-calls-fix file=app.jadpo hidden
application {}
```

```jadpo check project=p76-calls-fix file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p76-calls-fix file=shared/providers/acme_pay/v1/provider.jadpo hidden
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

```jadpo check project=p76-calls-fix file=shared/providers/acme_pay/v1/types/pay_request.jadpo hidden
/// What each call takes.
public type PayRequest = Object {
    reference: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-calls-fix file=shared/providers/acme_pay/v1/types/pay_reply.jadpo hidden
/// What each call gives.
public type PayReply = Object {
    id: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-calls-fix file=shared/providers/acme_pay/v1/resources/customers.jadpo hidden
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

```jadpo check project=p76-calls-fix file=shared/providers/acme_pay/v1/actions/open_customer.jadpo hidden
/// Opens the customer.
public action open_customer(input: PayRequest) -> PayReply {
    return customers.create(input)
}
```

`start_subscription` is now one way through the provider's API, which a
retry replays in order.

**If it reads the app's setting**, have the app's own action read it and
decide whether to call the package:

```jadpo check project=p76-config-fix file=shared/providers/acme_pay/v1/actions/open_customer.jadpo
/// Opens the customer.
public action open_customer(input: PayRequest) -> PayReply {
    return customers.create(input)
}
```

```jadpo check project=p76-config-fix file=app.jadpo hidden
application {}
```

```jadpo check project=p76-config-fix file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=p76-config-fix file=shared/providers/acme_pay/v1/provider.jadpo hidden
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

```jadpo check project=p76-config-fix file=shared/providers/acme_pay/v1/types/pay_request.jadpo hidden
/// What each call takes.
public type PayRequest = Object {
    reference: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-config-fix file=shared/providers/acme_pay/v1/types/pay_reply.jadpo hidden
/// What each call gives.
public type PayReply = Object {
    id: Text {
        max_length: 40
    }
}
```

```jadpo check project=p76-config-fix file=shared/providers/acme_pay/v1/resources/customers.jadpo hidden
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

The package no longer reads the setting: the app reads it, where a reviewer
sees it.

## Related

- [What package code may do](../language/11-integrations.md#what-package-code-may-do)
- [Calling a provider](../language/11-integrations.md#calling-a-provider)
