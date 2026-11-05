# Receive a webhook
<!-- topic: guide-receive-a-webhook -->

A webhook is a request another service sends your app when something happens
there, such as a payment going through. In Jadpo you don't write its route. You
describe what the sender sends, give the connection the shared secret, and write
a subscriber. Jadpo generates the route, checks the signature, drops repeats, and
hands you a typed event after the transaction commits. This guide keeps each
payment that a payments provider, Acme Pay, announces.

You start from any app. It gains a `Payment` entity here, along with a query and
a `GET /payments` route that lists them, so you can see the result.
[Query records](03-query-records.md) shows how to write those.

```jadpo check project=guide-webhooks file=domains/till/types/payment_view.jadpo hidden
use till/entities { Payment }

/// A payment, as the API sends it.
type PaymentView = Output {
    pick: [Payment.reference, Payment.pence, Payment.taken_at]
}
```

```jadpo check project=guide-webhooks file=domains/till/queries/latest_payments.jadpo hidden
use till/entities { Payment }
use till/types { PaymentView }

/// The latest payments.
query latest_payments() -> List<PaymentView> {
    from: Payment
    order: [Payment.taken_at desc]
    limit: 20
}
```

```jadpo check project=guide-webhooks file=domains/till/routes/payments.jadpo hidden
use till/types { PaymentView }
use till/queries { latest_payments }

/// The till's payments.
routes Payments {
    prefix: /payments

    /// The latest payments.
    GET / {
        authentication: RouteAuthentication.none
        output: List<PaymentView>
        run: latest_payments()
    }
}
```

## Describe the sender

If you have a package for the sender, from a folder or a git tag, its webhooks
come with it, so install it with `jadpo package add` and skip to [the
connection](#connect-it). Acme Pay has none, so your app holds its own description
in `shared/providers/`, written as a package.

`shared/providers/acme_pay/v1/package.jadpo`:

```jadpo check project=guide-webhooks file=shared/providers/acme_pay/v1/package.jadpo
/// Acme Pay's payments API, written from its documentation.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

`webhooks:` says how the sender signs what it sends, and where in the body each
event's id, type and time are. The built-in schemes include
`SignatureScheme.stripe`, `.standard_webhooks`, used by Svix and others, and
`.hmac_sha256` with the `header:` that holds the signature.

`shared/providers/acme_pay/v1/provider.jadpo`:

```jadpo check project=guide-webhooks file=shared/providers/acme_pay/v1/provider.jadpo
/// Acme Pay's API, which signs its webhooks as Stripe does.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.stripe
        event_id: "id"
        event_type: "type"
        occurred_at: "created"
    }
}
```

Each kind of event is a type in the package's `events/` folder. `wire_name:` is
the name the sender gives that kind of event, or that field. `occurred_at` is
filled from the place `webhooks:` names. Any other member of the body is left out.

`shared/providers/acme_pay/v1/types/payment_id.jadpo`:

```jadpo check project=guide-webhooks file=shared/providers/acme_pay/v1/types/payment_id.jadpo
/// Acme Pay's reference for a payment.
public type PaymentId = Text {
    max_length: 40
}
```

`shared/providers/acme_pay/v1/events/payment_succeeded.jadpo`:

```jadpo check project=guide-webhooks file=shared/providers/acme_pay/v1/events/payment_succeeded.jadpo
/// A payment was taken.
public type PaymentSucceeded = Event {
    wire_name: "payment.succeeded"
    occurred_at: Instant
    payment_id: PaymentId {
        wire_name: "paymentId"
    }
    pence: Int {
        min: 0
    }
}
```

`public` lets your app's subscribers hear it.

## Connect it

A connection with `webhooks: { secret: }` receives the sender's webhooks. Its
name gives the route: this one is `POST /webhooks/payments`.

`app.jadpo`:

```jadpo check project=guide-webhooks file=app.jadpo
application {
    connections: {
        payments: {
            package: AcmePay.v1
            domains: [till]
            credential: config.acme_pay_key
            environment: config.acme_pay_environment
            webhooks: {
                secret: config.acme_pay_webhook_secret
            }
        }
    }
}
```

`config.jadpo`:

```jadpo check project=guide-webhooks file=config.jadpo
config {
    acme_pay_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
    acme_pay_key: Text {
        secret: true
    }
    acme_pay_webhook_secret: Text {
        secret: true
    }
}
```

On your machine, `jadpo config set acme_pay_webhook_secret` asks for the secret at
a prompt, and keeps it in `.env.local`, which git ignores. Use the secret the
sender gives you, or any value while you try this. `jadpo dev` fakes the calls your
app makes through the connection, so `acme_pay_key` can wait.

## Handle the event

A subscriber takes the event as it takes one of your own. It runs as the system
caller, so the entity's `create:` has to name `Access.system`.

`domains/till/entities/payment.jadpo`:

```jadpo check project=guide-webhooks file=domains/till/entities/payment.jadpo
use shared/providers/acme_pay

/// A payment Acme Pay took.
entity Payment {
    id: Uuid
    reference: AcmePay.PaymentId
    pence: Int
    taken_at: Instant

    policy: {
        create: [Access.system]
        read: [Access.public]
    }
}
```

`domains/till/actions/keep_payment.jadpo`:

```jadpo check project=guide-webhooks file=domains/till/actions/keep_payment.jadpo
use till/entities { Payment }
use shared/providers/acme_pay

/// Keeps a payment Acme Pay says it took.
action keep_payment(event: AcmePay.PaymentSucceeded) -> Payment {
    return Payment.create {
        set: {
            reference: event.payment_id
            pence: event.pence
            taken_at: event.occurred_at
        }
    }
}
```

`domains/till/subscribers/payment_succeeded.jadpo`:

```jadpo check project=guide-webhooks file=domains/till/subscribers/payment_succeeded.jadpo
use till/actions { keep_payment }
use shared/providers/acme_pay

/// Keeps each payment Acme Pay announces.
subscriber payment_succeeded {
    on: AcmePay.PaymentSucceeded
    run: keep_payment(event)
}
```

Make the migration for your new tables with `jadpo migrate new receive_payments`,
and bring the records up to date with `jadpo records update`. The migration also
makes `jadpo_received_events`, where Jadpo remembers the ids it has seen.

`jadpo routes` lists the generated route, with `signature` as what it asks for:

```sh
jadpo routes
```

```text
main  /
  GET  /openapi.json  generated openapi      none             generated
  GET  /payments      query latest_payments  none  120/1m ip  domains/till/routes/payments.jadpo:9

webhooks  /webhooks
  POST  /webhooks/payments  generated webhook  signature  generated

3 routes: 2 APIs
```

## Accept the new host

The connection can reach Acme Pay's hosts, so data your app holds can now leave
for them. Once your app has a commit to compare with, `jadpo check` asks a person
to accept that. [Get a person's sign-off](09-get-a-sign-off.md) goes through the
steps.

`acceptances/20261104T1133_payments_from_acme_pay.jadpo`:

```jadpo check project=guide-webhooks file=acceptances/20261104T1133_payments_from_acme_pay.jadpo
/// Acme Pay is the payments provider we signed with, and the app only receives from it for now.
acceptance {
    changes: {
        payments: NetworkChange.host_added
    }
}
```

## Test it

A test calls the action with the event, as a subscriber would. That tests what
your app does with an event, and Jadpo's own tests cover the signature and the
repeats.

`domains/till/actions/keep_payment.test.jadpo`:

```jadpo check project=guide-webhooks file=domains/till/actions/keep_payment.test.jadpo
use till/entities { Payment }
use shared/providers/acme_pay

test "a payment Acme Pay announces is kept" {
    clock: "2099-03-01T09:30:00Z"
    var kept: Payment = keep_payment(
        AcmePay.PaymentSucceeded {
            occurred_at: clock.now
            payment_id: "pay_42"
            pence: 1250
        }
    )
    assert kept.reference == "pay_42"
    assert kept.pence == 1250
    assert kept.taken_at == "2099-03-01T09:30:00Z"
}
```

```sh
jadpo test
```

```text
tested: 1 passed, 1 action test
```

## Send yourself one

Start `jadpo dev`. In another terminal, sign a body the way Acme Pay does and send
it. The secret is the one you set above, and `read -rs` keeps it out of your
shell's history.

```sh
read -rs SECRET
NOW=$(date +%s)
BODY=$(printf '{"id":"evt_1","type":"payment.succeeded","created":%s,"paymentId":"pay_42","pence":1250}' "$NOW")
SIG=$(printf '%s.%s' "$NOW" "$BODY" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')
curl -i -X POST http://localhost:3000/webhooks/payments \
  -H "stripe-signature: t=$NOW,v1=$SIG" \
  -H 'content-type: application/json' -d "$BODY"
```

```text
HTTP/1.1 204 No Content
x-request-id: req_effc0ee1a381c964
date: Wed, 04 Nov 2026 11:26:15 GMT
```

The event is kept by then, and your subscriber runs just after the response.

```sh
curl http://localhost:3000/payments
```

```text
[{"reference":"pay_42","pence":"1250","taken_at":"2026-11-04T11:26:15Z"}]
```

Send the same request again and it answers `204` too, but the event has the same
id, so nothing new is kept and the list doesn't change. A signature that doesn't
match is refused, saying nothing of why. `jadpo dev`'s request line says why, here
`mismatch`, and it would say `stale` for a time more than five minutes off:

```text
HTTP/1.1 401 Unauthorized
content-type: application/json
x-request-id: req_e42e9840803efbc9
content-length: 123
date: Wed, 04 Nov 2026 11:26:15 GMT

{"error":{"code":"unauthenticated","message":"The webhook's signature doesn't match.","request_id":"req_e42e9840803efbc9"}}
```

## What to watch for

- **Events can come out of order.** Senders don't promise an order, so an event's
  `occurred_at` tells you when it happened. If your subscriber changes a record,
  ignore an event older than what the record already holds.
- **A delivery can run twice.** Subscribers run at least once, as
  [Delivery: at least once, safe to repeat](../language/12-jobs-and-events.md#delivery-at-least-once-safe-to-repeat)
  explains, so write them to be safe to repeat.
- **A subscriber that fails is retried, then parked.** `jadpo deliveries` lists what
  needs an operator. Check it if an event seems to have gone missing, because the
  `204` only says it was kept.

A sender that wants an answer in the webhook's own response, such as Twilio, needs
[a reply](../language/11-integrations.md#webhooks-that-need-a-reply) instead of a
subscriber. A partner who calls your app under an agreement signs in with
a [service credential](../language/08-identity.md#service-credentials) and calls an
ordinary route.

## Read more

[Webhooks as events](../language/11-integrations.md#webhooks-as-events),
[How webhooks are received](../language/11-integrations.md#how-webhooks-are-received),
[Senders with no package](../language/11-integrations.md#senders-with-no-package),
[Events from outside](../language/12-jobs-and-events.md#events-from-outside) and
[Subscribers](../language/12-jobs-and-events.md#subscribers).
