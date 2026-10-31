# `test.fake_unused`

A test passed otherwise, but its `fakes:` scripted more answers for a provider operation than the code called it for.

## Why Jadpo refuses it

A test never reaches a real provider. `fakes:` scripts the answers an
operation gives, in order, and each answer stands for a call the test
expects the code to make. An answer left over means the code made fewer
calls than the test says, so either the test is wrong about the code, or
the code no longer does what the test was written for. Either way the
test would pass without proving what it claims.

## What it looks like

domains/shipping/actions/book.test.jadpo, testing `book`, which books a
collection through the `couriers` connection:

```jadpo check project=e-fake-unused file=app.jadpo hidden
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

```jadpo check project=e-fake-unused file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-unused file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-unused file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-unused file=domains/shipping/actions/book.jadpo hidden
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

```jadpo check project=e-fake-unused file=domains/shipping/actions/book.test.jadpo expect=test.fake_unused
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_taken, unavailable]
    }
    assert book("N1 9GU") fails SlotGone
}
```

```text output
FAIL  a taken slot is the app's own failure
 --> domains/shipping/actions/book.test.jadpo:6

  fake: `couriers.bookings.create` was called once, and its `fakes:` scripts 2 answers
  reason: Each scripted answer is for a call the test expects, so one left over means the code calls the operation less often than the test says.
  next: Remove the answers no call takes, or find why the code doesn't call `couriers.bookings.create` (agent)

  rule: test.fake_unused
  rerun: jadpo test domains/shipping/actions/book.test.jadpo:6 --seed 1

test failed: 1 failed, 1 action test
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

Here `book` maps `slot_taken` to `SlotGone` at the first call, so nothing
calls `couriers.bookings.create` a second time to take `unavailable`.

**If the code is right to call it once**, script one answer. In
`domains/shipping/actions/book.test.jadpo`:

```jadpo fragment
        couriers.bookings.create: [slot_taken]
```

```jadpo check project=e-fake-unused-fix-1 file=domains/shipping/actions/book.test.jadpo hidden
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure" {
    fakes: {
        couriers.bookings.create: [slot_taken]
    }
    assert book("N1 9GU") fails SlotGone
}
```

```jadpo check project=e-fake-unused-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-fake-unused-fix-1 file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-unused-fix-1 file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-unused-fix-1 file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-unused-fix-1 file=domains/shipping/actions/book.jadpo hidden
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

The test scripts the one call `book` makes, and passes.

**If the test meant a provider that is down first**, put `unavailable`
first. A call answered `unavailable` is tried again, so the second
answer is taken too:

```jadpo fragment
test "a taken slot is the app's own failure, after a retry" {
        couriers.bookings.create: [unavailable, slot_taken]
        ...
}
```

```jadpo check project=e-fake-unused-fix-2 file=domains/shipping/actions/book.test.jadpo hidden
use app { couriers }
use shipping/failures { SlotGone }

test "a taken slot is the app's own failure, after a retry" {
    fakes: {
        couriers.bookings.create: [unavailable, slot_taken]
    }
    assert book("N1 9GU") fails SlotGone
}
```

```jadpo check project=e-fake-unused-fix-2 file=app.jadpo hidden
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

```jadpo check project=e-fake-unused-fix-2 file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e-fake-unused-fix-2 file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e-fake-unused-fix-2 file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e-fake-unused-fix-2 file=domains/shipping/actions/book.jadpo hidden
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

The test now proves that `book` retries an unavailable courier and still
fails with `SlotGone` when the slot is taken.

## Related

- [The number of answers](../language/13-testing.md#the-number-of-answers)
- [Scripting answers: `fakes:`](../language/13-testing.md#scripting-answers-fakes)
- [Fakes for integrations](../language/13-testing.md#fakes-for-integrations)
- [`test.fake_exhausted`](../errors/test.fake_exhausted.md)
