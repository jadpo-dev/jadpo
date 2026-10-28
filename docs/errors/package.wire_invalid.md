# `package.wire_invalid`

> A provider package's field says how it is sent
> with `wire:`, one of the built-in encodings, or with its package's own
> `decode:` and `encode:` functions, given together. Either is refused
> outside a provider package, both on one field, an encoding that doesn't
> hold the field's type or `Wire.minor_units` without the field's one
> currency, and a function that isn't the package's or doesn't take and
> give the right types. `wire_parts:` is refused on a type
> not made of parts, beside another way of sending the field, with a part
> left out, unknown or given twice, or a spelling that isn't text.

An encoding holds the types its provider sends that way:

```jadpo check project=e72c2-wire file=app.jadpo hidden
application {}
```

```jadpo check project=e72c2-wire file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c2-wire file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=e72c2-wire file=shared/providers/acme_couriers/v1/types/parcel.jadpo expect=package.wire_invalid
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
    collected_on: Text {
        wire: Wire.unix_seconds
    }
}
```

```text output
error: `Wire.unix_seconds` sends an `Instant`, and `collected_on` is a `Text`
 --> shared/providers/acme_couriers/v1/types/parcel.jadpo:5:15
  |
5 |         wire: Wire.unix_seconds
  |               ^^^^^^^^^^^^^^^^^ a `Text` field
  reason: `Wire.unix_seconds` is how a provider sends an `Instant`; a field of another type is sent as the contract writes it, in another encoding, or by the package's own `decode:` and `encode:` functions.
  next: Name the encoding the provider sends a `Text` in, or remove `wire:` (agent)
  rule: package.wire_invalid
  help: docs/language/11-integrations.html#package-wire-invalid

check failed: 1 error, 4 files
```
