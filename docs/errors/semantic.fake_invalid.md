# `semantic.fake_invalid`

A test's `fakes:` scripts something no provider operation answers: an operation the app's connections don't have, a name
that is none of the operation's failures' codes or a condition, an
operation scripted twice or with no answers, or a value for one that
gives nothing. A value of the wrong type is the usual `type.mismatch`.

```jadpo check project=e-fake-invalid file=app.jadpo hidden
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
        }
    }
}
```

```jadpo check project=e-fake-invalid file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    error_code: "error.code"
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
        docs: "https://docs.acme-couriers.example/bookings"
    }
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-invalid file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-invalid file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-invalid file=domains/shipping/actions/book.jadpo hidden
use app { couriers }
use shipping/failures { SlotGone }
use shared/providers/acme_couriers

/// Books a collection with the courier.
action book(postcode: AcmeCouriers.Postcode) fails SlotGone -> AcmeCouriers.Booking {
    return attempt couriers.bookings.create(
        AcmeCouriers.CreateBooking {
            postcode: postcode
        }
    ) {
        slot_taken: SlotGone
    }
}
```

```jadpo check project=e-fake-invalid file=domains/shipping/actions/book.test.jadpo expect=semantic.fake_invalid
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_takn]
    }
    assert book("N1 9GU") fails SlotGone
}
```

```text output
error: `slot_takn` isn't an answer `couriers.bookings.create` gives
 --> domains/shipping/actions/book.test.jadpo:6:36
  |
6 |         couriers.bookings.create: [slot_takn]
  |                                    ^^^^^^^^^ not a failure or a condition
  reason: A scripted answer is a value of `AcmeCouriers.Booking`, one of `couriers.bookings.create`'s failures by its code (`slot_taken`), or a condition: `unavailable`, `lost`, `in_use` or `misconfigured`.
  next: Use `slot_taken` instead of `slot_takn` (agent)
  rule: semantic.fake_invalid
  help: https://jadpo.dev/docs/errors/semantic.fake_invalid

check failed: 1 error, 13 files
```
