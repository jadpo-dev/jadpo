# `connection.value_invalid`

A credential is read from a secret setting, which code never sees:

```jadpo check project=e71c-value-invalid file=config.jadpo hidden
config {
    couriers_api_key: Text
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e71c-value-invalid file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e71c-value-invalid file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e71c-value-invalid file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e71c-value-invalid file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e71c-value-invalid file=app.jadpo expect=connection.value_invalid
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
}
```

```text output
error: `couriers`'s credential must be a secret setting
 --> app.jadpo:6:25
  |
6 |             credential: config.couriers_api_key
  |                         ^^^^^^^^^^^^^^^^^^^^^^^ `couriers_api_key` isn't secret
  reason: A credential is read from the deployment's environment and never seen by code, so it is a `secret: true` setting, `config.<name>` (area 10, "Secrets").
  next: Add `secret: true` to `couriers_api_key` in `config.jadpo` (agent)
  rule: connection.value_invalid
  help: docs/language/11-integrations.html#connection-value-invalid

check failed: 1 error, 6 files
```

`development: { sandbox: }` names the package's own operations and
resources:

```jadpo check project=e72d-sandbox-unknown file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/types/postcode.jadpo hidden
/// Where a collection is.
public type Postcode = Text {
    max_length: 8
}
```

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/types/booking_id.jadpo hidden
/// The courier's reference for a booking.
public type BookingId = Text {
    max_length: 40
}
```

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
}
```

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: BookingId
}
```

```jadpo check project=e72d-sandbox-unknown file=shared/providers/acme_couriers/v1/failures/slot_taken.jadpo hidden
/// The slot went before the booking arrived.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "That slot is taken."
}
```

```jadpo check project=e72d-sandbox-unknown file=domains/shipping/failures/slot_gone.jadpo hidden
/// The slot the customer chose has gone.
type SlotGone = Failure {
    kind: FailureKind.conflict
    message: "That slot has gone; choose another."
}
```

```jadpo check project=e72d-sandbox-unknown file=domains/shipping/actions/book.jadpo hidden
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

```jadpo check project=e72d-sandbox-unknown file=app.jadpo expect=connection.value_invalid
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
            development: {
                sandbox: [bookings.creat]
            }
        }
    }
}
```

```text output
error: `couriers`'s `sandbox:` lists `bookings.creat`, which `AcmeCouriers.v1` doesn't have
 --> app.jadpo:8:27
  |
8 |                 sandbox: [bookings.creat]
  |                           ^^^^^^^^^^^^^^ no such operation or resource
  reason: `sandbox:` lets the package's public operations and resources through to its sandbox in `jadpo dev`; `AcmeCouriers.v1` has `bookings.create` and `bookings`.
  next: Use `bookings.create` instead of `bookings.creat` (agent)
  rule: connection.value_invalid
  help: docs/language/11-integrations.html#connection-value-invalid

check failed: 1 error, 12 files
```
