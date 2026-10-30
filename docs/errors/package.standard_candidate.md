# `package.standard_candidate`

> This flags, in a provider package's public object, a field the
> provider sends in its own shape that looks like a standard type the
> package doesn't use ("Mapping imported types to standard types"). A whole number named like a time (`created`, `expires`,
> `paid_at`) may be an `Instant` sent as seconds; one named like an amount
> (`amount`, `amount_refunded`, `total_amount`) beside a currency, `Money`
> in minor units; a latitude beside a longitude, a `Point`. A field that
> says how it is sent (`wire:`, `wire_parts:`, `decode:`) is never one, and
> an object that is a standard type whole is `type.standard_exists`
> instead. It is declinable: a field that means something else keeps its
> type with a reason on the field.

An imported description often says only `integer`, so the importer leaves
the field as the provider types it and the error asks for the
documentation to be checked:

```jadpo check project=e72e-candidate file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=e72e-candidate file=shared/providers/acme_couriers/v1/types/delivery.jadpo expect=package.standard_candidate
/// A parcel delivered.
public type Delivery = Object {
    id: Text
    delivered_at: Int
}
```

```text output
error: `Delivery.delivered_at` may be an `Instant` sent as seconds
 --> shared/providers/acme_couriers/v1/types/delivery.jadpo:4:5
  |
4 |     delivered_at: Int
  |     ^^^^^^^^^^^^ a whole number named like a time
  reason: `delivered_at` is a whole number named like a time, which providers usually send as seconds since 1970. A package's public types use the standard types, so an app reads an `Instant` rather than a number to convert, and the package decodes it at the edge.
  next: Check the provider's documentation, then make it `delivered_at: Instant { wire: Wire.unix_seconds }`, or `Wire.unix_milliseconds` for milliseconds, or write `decline package.standard_candidate: "<why>"` above it if this is deliberate (agent)
  rule: package.standard_candidate
  help: https://jadpo.dev/docs/errors/package.standard_candidate

check failed: 1 error, 4 files
```

Swapping it reads the provider's number as the `Instant` it is; a field
that really is a number keeps its type with a reason, which `jadpo
declines list` shows:

```jadpo check project=e72e-candidate-declined file=app.jadpo hidden
application {}
```

```jadpo check project=e72e-candidate-declined file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e-candidate-declined file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=e72e-candidate-declined file=shared/providers/acme_couriers/v1/types/delivery.jadpo
/// A parcel delivered.
public type Delivery = Object {
    id: Text
    delivered_at: Instant {
        wire: Wire.unix_seconds
    }
    decline package.standard_candidate: "The courier numbers its collection slots 1 to 6; this is that number"
    slot_time: Int
}
```
