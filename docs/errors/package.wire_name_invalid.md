# `package.wire_name_invalid`

> `wire_name:` gives a
> provider package's field or enum variant its spelling on the wire, so it
> is refused anywhere else, empty, repeating the name, or sending two
> members of one type under one spelling.

Only a provider package's types say how a provider spells a field; the
app's own are sent as its contract writes them:

```jadpo check project=e72c-wire-name file=app.jadpo hidden
application {}
```

```jadpo check project=e72c-wire-name file=domains/shipping/actions/show_collection.jadpo hidden
use shipping/types { CollectAt }

/// Shows when a parcel is collected.
action show_collection(at: CollectAt) -> CollectAt {
    return at
}
```

```jadpo check project=e72c-wire-name file=domains/shipping/types/collect_at.jadpo expect=package.wire_name_invalid
/// When a parcel is collected.
type CollectAt = Object {
    at: Instant {
        wire_name: "collectAt"
    }
}
```

```text output
error: `wire_name` is for a provider package's fields
 --> domains/shipping/types/collect_at.jadpo:4:9
  |
4 |         wire_name: "collectAt"
  |         ^^^^^^^^^ outside a provider package
  reason: `wire_name:` gives a field's spelling on a provider's wire, where it differs from its Jadpo name, so only a provider package's types say it; the app's own types are sent as its contract writes them.
  next: Remove `wire_name:` (agent)
  rule: package.wire_name_invalid
  help: docs/errors/package.wire_name_invalid.md

check failed: 1 error, 3 files
```

A package that receives webhooks names each event as its provider sends
it, so the runtime can find the event a body carries:

```jadpo check project=e77-wire-name file=app.jadpo hidden
application {}
```

```jadpo check project=e77-wire-name file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e77-wire-name file=shared/providers/acme_pay/v1/provider.jadpo hidden
/// Acme Pay's API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
    }
    webhooks: {
        check: SignatureScheme.standard_webhooks
        event_id: "id"
        event_type: "type"
    }
}
```

```jadpo check project=e77-wire-name file=shared/providers/acme_pay/v1/events/payment_refunded.jadpo expect=package.wire_name_invalid
/// A payment was given back.
public type PaymentRefunded = Event {
    occurred_at: Instant
    reference: Text
}
```

```text output
error: Webhook event `PaymentRefunded` doesn't give `wire_name:`
 --> shared/providers/acme_pay/v1/events/payment_refunded.jadpo:2:13
  |
2 | public type PaymentRefunded = Event {
  |             ^^^^^^^^^^^^^^^ no `wire_name:`
  reason: `AcmePay.v1` declares `webhooks:`, so each event it declares gives the type its provider names it by, `wire_name:`, which the runtime matches a webhook's event type against.
  next: Add `wire_name:` with the provider's name for the event, such as `wire_name: "payment.succeeded"` (agent)
  rule: package.wire_name_invalid
  help: docs/errors/package.wire_name_invalid.md

check failed: 1 error, 4 files
```
