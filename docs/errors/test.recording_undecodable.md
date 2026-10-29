# `test.recording_undecodable`

A recording the operation's output type, or one of the package's own
decoders, doesn't read: here the provider sent a status the package's
`Status` doesn't have.

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

```jadpo check project=e72d2-undecodable file=shared/providers/acme_couriers/v1/types/booking.jadpo
/// A booked collection.
public type Booking = Object {
    id: Text
    status: Status
}
```

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
