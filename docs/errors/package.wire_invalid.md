# `package.wire_invalid`

A provider package's field or failure says how it is sent in a way it can't.

## Why Jadpo refuses it

A provider package's field says how it is sent with `wire:`, one of the
built-in encodings, or with its package's own `decode:` and `encode:`
functions, given together, or `wire_parts:` for a value sent in parts. The
runtime reads and writes the provider's answer from those, so each has to
be possible: an encoding has to hold the field's type, a function has to
take and give the right types, and a failure's `wire_status:` has to be a
`4xx` the driver reads as a failure.

## What it looks like

`Wire.unix_seconds` holds an `Instant`, and this field is `Text`:

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

`shared/providers/acme_couriers/v1/types/parcel.jadpo`:

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
  help: https://jadpo.dev/docs/errors/package.wire_invalid

check failed: 1 error, 4 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A failure's `wire_status:` is the status its provider refuses with. A
`401` or `403` is read as the credential refused, and a `408` or `429` as
a call that did nothing, before any failure is looked for:

```jadpo check project=e72f-wire-status file=app.jadpo hidden
application {}
```

```jadpo check project=e72f-wire-status file=shared/providers/postcodes_io/v1/package.jadpo hidden
/// postcodes.io's UK postcode lookups.
package PostcodesIo.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72f-wire-status file=shared/providers/postcodes_io/v1/provider.jadpo hidden
/// postcodes.io's public API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.postcodes.io"
    }
    error_code: "error"
}
```

`shared/providers/postcodes_io/v1/failures/too_many_requests.jadpo`:

```jadpo check project=e72f-wire-status file=shared/providers/postcodes_io/v1/failures/too_many_requests.jadpo expect=package.wire_invalid
/// postcodes.io is asked too often.
public type TooManyRequests = Failure {
    kind: FailureKind.rate_limited
    wire_status: 429
}
```

```text output
error: A provider's `429` is never one of its failures
 --> shared/providers/postcodes_io/v1/failures/too_many_requests.jadpo:4:18
  |
4 |     wire_status: 429
  |                  ^^^ not a refusal Jadpo reads as a failure
  reason: A `408` or `429` says the call did nothing and may be sent again, read as `unavailable`, so no failure can be it.
  next: Write the `4xx` status the provider refuses with, or remove `wire_status:` (agent)
  rule: package.wire_invalid
  help: https://jadpo.dev/docs/errors/package.wire_invalid

check failed: 1 error, 4 files
```

Either way of saying it used outside a provider package, both on one field,
`Wire.minor_units` without the field's one currency, and a `wire_parts:`
with a part left out, unknown or given twice are this error too, each with
its own message.

## How to fix it

**If the field is the type the encoding holds**, make it that type:

```jadpo check project=e72c2-wire-fix file=shared/providers/acme_couriers/v1/types/parcel.jadpo
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
    collected_on: Instant {
        wire: Wire.unix_seconds
    }
}
```

```jadpo check project=e72c2-wire-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72c2-wire-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c2-wire-fix file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

The package reads the number as an `Instant`.

**If the provider sends it as plain text**, remove `wire:`:

```jadpo check project=e72c2-wire-fix-2 file=shared/providers/acme_couriers/v1/types/parcel.jadpo
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
    collected_on: Text
}
```

```jadpo check project=e72c2-wire-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e72c2-wire-fix-2 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c2-wire-fix-2 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

The field is sent as the contract writes it.

**If the provider refuses with another status**, write that one:

```jadpo check project=e72f-wire-status-fix file=shared/providers/postcodes_io/v1/failures/too_many_requests.jadpo
/// postcodes.io is asked too often.
public type TooManyRequests = Failure {
    kind: FailureKind.rate_limited
    wire_status: 409
}
```

```jadpo check project=e72f-wire-status-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72f-wire-status-fix file=shared/providers/postcodes_io/v1/package.jadpo hidden
/// postcodes.io's UK postcode lookups.
package PostcodesIo.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72f-wire-status-fix file=shared/providers/postcodes_io/v1/provider.jadpo hidden
/// postcodes.io's public API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.postcodes.io"
    }
    error_code: "error"
}
```

A `409` is read as `TooManyRequests`. If the provider really answers `429`,
delete the failure: Jadpo reads that status as a call that did nothing, and
sends it again.

## Related

- [The built-in encodings: `wire:`](../language/11-integrations.md#the-built-in-encodings-wire)
- [A field's own functions: `decode:` and `encode:`](../language/11-integrations.md#a-fields-own-functions-decode-and-encode)
- [Parts sent separately: `wire_parts:`](../language/11-integrations.md#parts-sent-separately-wire_parts)
- [A provider's failures](../language/11-integrations.md#a-providers-failures)
