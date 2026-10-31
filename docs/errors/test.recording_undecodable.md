# `test.recording_undecodable`

A saved answer from an outside service doesn't fit the types your integration reads that answer as.

## Why Jadpo refuses it

A recording is a real answer from the provider, kept in the package as
`recordings/<resource>.<operation>.json`. `jadpo test` reads each one
through the operation's output type and the package's own decoders, just
as a real call's answer is read. One that doesn't decode shows that the
types don't match what the provider sends, so a real answer like it would
make the call fail.

## What it looks like

```jadpo check project=e72d2-undecodable file=app.jadpo hidden
application {}
```

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        output_at: "data"
        changes: true
        idempotent: true
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/bookings"
    }
}
```

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
}
```

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/types/status.jadpo hidden
/// Where a booking stands.
public type Status = Enum {
    booked
    on_hold
}
```

The app's own integration for Acme Couriers reads a booking's `status` as
its `Status` enum, which has the values `booked` and `on_hold`.

shared/providers/acme_couriers/v1/types/booking.jadpo:

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/types/booking.jadpo
/// A booked collection.
public type Booking = Object {
    id: Text
    status: Status
}
```

The recording of creating a booking holds a status of `lost`.

shared/providers/acme_couriers/v1/recordings/bookings.create.json:

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/recordings/bookings.create.json expect=test.recording_undecodable
{
  "data": {
    "id": "{Text}",
    "status": "lost"
  }
}
```

```text output
FAIL  bookings.create's recording decodes
 --> shared/providers/acme_couriers/v1/recordings/bookings.create.json:1

  decode: `bookings.create`'s recording doesn't decode: `variant` on `status`
  reason: Every decoder decodes every recorded answer: the operation's output type, or a decoder of its own, refuses what the provider sent here, so a real answer like it would fault the call.
  next: Correct the type or its decoder against the recording and the provider's documentation (agent)

  rule: test.recording_undecodable
  rerun: jadpo test shared/providers/acme_couriers/v1/recordings/bookings.create.json:1 --seed 1

test failed: 1 failed, 1 recording test
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is which is wrong: the type, or the recording. The
provider's documentation says which values it sends.

**If the provider does send that value**, add it to the type. In
`shared/providers/acme_couriers/v1/types/status.jadpo`:

```jadpo fragment
    lost
```

```jadpo check project=e72d2-undecodable-value file=shared/providers/acme_couriers/v1/types/status.jadpo hidden
/// Where a booking stands.
public type Status = Enum {
    booked
    on_hold
    lost
}
```

```jadpo check project=e72d2-undecodable-value file=app.jadpo hidden
application {}
```

```jadpo check project=e72d2-undecodable-value file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d2-undecodable-value file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72d2-undecodable-value file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        output_at: "data"
        changes: true
        idempotent: true
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/bookings"
    }
}
```

```jadpo check project=e72d2-undecodable-value file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
}
```

```jadpo check project=e72d2-undecodable-value file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: Text
    status: Status
}
```

```jadpo check project=e72d2-undecodable-value file=shared/providers/acme_couriers/v1/recordings/bookings.create.json hidden
{
  "data": {
    "id": "{Text}",
    "status": "lost"
  }
}
```

The recording decodes, and so does a real answer with a status of `lost`.
A `match` on `Status` in your code now has to handle `lost` too.

**If the recording is wrong**, such as one edited by hand, correct it to
what the provider sends. In
`shared/providers/acme_couriers/v1/recordings/bookings.create.json`:

```jadpo fragment
    "status": "on_hold"
```

```jadpo check project=e72d2-undecodable-recording file=shared/providers/acme_couriers/v1/recordings/bookings.create.json hidden
{
  "data": {
    "id": "{Text}",
    "status": "on_hold"
  }
}
```

```jadpo check project=e72d2-undecodable-recording file=app.jadpo hidden
application {}
```

```jadpo check project=e72d2-undecodable-recording file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d2-undecodable-recording file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72d2-undecodable-recording file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        output_at: "data"
        changes: true
        idempotent: true
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/bookings"
    }
}
```

```jadpo check project=e72d2-undecodable-recording file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
}
```

```jadpo check project=e72d2-undecodable-recording file=shared/providers/acme_couriers/v1/types/status.jadpo hidden
/// Where a booking stands.
public type Status = Enum {
    booked
    on_hold
}
```

```jadpo check project=e72d2-undecodable-recording file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: Text
    status: Status
}
```

The recording decodes again. Recording the operation again from the
provider's sandbox, with `jadpo package record`, replaces the file with a
real answer.

## Related

- [Recordings](../language/11-integrations.md#recordings)
- [Recording tests](../language/13-testing.md#recording-tests)
- [Decoding](../language/11-integrations.md#decoding)
