# `package.wire_name_invalid`

A `wire_name:` is outside a provider package, or repeats or empties a spelling.

## Why Jadpo refuses it

`wire_name:` gives a provider package's field or enum variant its spelling
on the wire, and a provider package's failure the provider's own text for
it. Nothing else has a provider's spelling, so it is refused anywhere else.
Within a provider package it is refused where it would say nothing or be
ambiguous: empty, repeating the name or code it is written on, or sending
two members of one type under one spelling.

## What it looks like

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

`domains/shipping/types/collect_at.jadpo`:

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
  help: https://jadpo.dev/docs/errors/package.wire_name_invalid

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

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

`shared/providers/acme_pay/v1/events/payment_refunded.jadpo`:

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
  help: https://jadpo.dev/docs/errors/package.wire_name_invalid

check failed: 1 error, 4 files
```

An app's own failure is known by its code alone; only a provider
package's failure says how its provider writes it:

```jadpo check project=e72f-failure-wire-name file=app.jadpo hidden
application {}
```

```jadpo check project=e72f-failure-wire-name file=domains/addresses/types/postcode.jadpo hidden
/// A postcode as someone typed it.
type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e72f-failure-wire-name file=domains/addresses/actions/check_postcode.jadpo hidden
use addresses/types { Postcode }
use addresses/failures { PostcodeUnknown }

/// Refuses every postcode, for now.
action check_postcode(postcode: Postcode) fails PostcodeUnknown -> Postcode {
    reject PostcodeUnknown
}
```

`domains/addresses/failures/postcode_unknown.jadpo`:

```jadpo check project=e72f-failure-wire-name file=domains/addresses/failures/postcode_unknown.jadpo expect=package.wire_name_invalid
/// No such postcode is in use.
type PostcodeUnknown = Failure {
    kind: FailureKind.not_found
    wire_name: "Postcode not found"
}
```

```text output
error: `wire_name` is for a provider package's failures
 --> domains/addresses/failures/postcode_unknown.jadpo:4:5
  |
4 |     wire_name: "Postcode not found"
  |     ^^^^^^^^^ outside a provider package
  reason: `wire_name:` gives a failure's code as a provider writes it, where that differs from its Jadpo code, so only a provider package's failures say it; an app's own failure is known by its code alone.
  next: Remove `wire_name:` (agent)
  rule: package.wire_name_invalid
  help: https://jadpo.dev/docs/errors/package.wire_name_invalid

check failed: 1 error, 4 files
```

## How to fix it

**If it isn't a provider package's field**, remove `wire_name:`:

```jadpo check project=e72c-wire-name-fix file=domains/shipping/types/collect_at.jadpo
/// When a parcel is collected.
type CollectAt = Object {
    at: Instant
}
```

```jadpo check project=e72c-wire-name-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72c-wire-name-fix file=domains/shipping/actions/show_collection.jadpo hidden
use shipping/types { CollectAt }

/// Shows when a parcel is collected.
action show_collection(at: CollectAt) -> CollectAt {
    return at
}
```

The app's own type is sent as its contract writes it.

**If a webhook event has no spelling**, give it the provider's name for the
event:

```jadpo check project=e77-wire-name-fix file=shared/providers/acme_pay/v1/events/payment_refunded.jadpo
/// A payment was given back.
public type PaymentRefunded = Event {
    occurred_at: Instant
    reference: Text
    wire_name: "payment.refunded"
}
```

```jadpo check project=e77-wire-name-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e77-wire-name-fix file=shared/providers/acme_pay/v1/package.jadpo hidden
/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e77-wire-name-fix file=shared/providers/acme_pay/v1/provider.jadpo hidden
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

The runtime can find this event from the type a webhook's body carries.

**If it isn't a provider package's failure**, remove `wire_name:`:

```jadpo check project=e72f-failure-wire-name-fix file=domains/addresses/failures/postcode_unknown.jadpo
/// No such postcode is in use.
type PostcodeUnknown = Failure {
    kind: FailureKind.not_found
}
```

```jadpo check project=e72f-failure-wire-name-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72f-failure-wire-name-fix file=domains/addresses/types/postcode.jadpo hidden
/// A postcode as someone typed it.
type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e72f-failure-wire-name-fix file=domains/addresses/actions/check_postcode.jadpo hidden
use addresses/types { Postcode }
use addresses/failures { PostcodeUnknown }

/// Refuses every postcode, for now.
action check_postcode(postcode: Postcode) fails PostcodeUnknown -> Postcode {
    reject PostcodeUnknown
}
```

The failure is known by its code alone.

## Related

- [Spellings: `wire_name:`](../language/11-integrations.md#spellings-wire_name)
- [Webhooks as events](../language/11-integrations.md#webhooks-as-events)
- [A provider's failures](../language/11-integrations.md#a-providers-failures)
