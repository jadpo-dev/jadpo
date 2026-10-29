# `test.fake_exhausted`

A test called a faked provider operation more times than its `fakes:`
scripts answers. The call past the script was
answered with the generated default, so whatever the test did next
followed from a call it didn't expect; the report names the operation,
at the line of its first scripted answer:

```jadpo check project=e-fake-exhausted file=app.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-exhausted file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-exhausted file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-exhausted file=domains/shipping/actions/book.jadpo hidden
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

```jadpo check project=e-fake-exhausted file=domains/shipping/actions/book.test.jadpo expect=test.fake_exhausted
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_taken]
    }
    assert book("N1 9GU") fails SlotGone
    assert book("SW1A 1AA") fails SlotGone
}
```

```text output
FAIL  a taken slot is the app's own failure
 --> domains/shipping/actions/book.test.jadpo:6

  fake: `couriers.bookings.create` was called twice, and its `fakes:` scripts 1 answer
  reason: A scripted operation answers each call with the next scripted answer, so a call past the last means the code calls it more often than the test says; that call was answered with the generated default.
  next: Script an answer for every call of `couriers.bookings.create`, or find why the code calls it again (agent)

  rule: test.fake_exhausted
  rerun: jadpo test domains/shipping/actions/book.test.jadpo:6 --seed 1

test failed: 1 failed, 1 action test
```
