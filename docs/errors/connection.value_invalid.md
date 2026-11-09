# `connection.value_invalid`

A connection's key holds a value it can't take, such as a credential that isn't a secret setting.

## Why Jadpo refuses it

A connection's values are read from each deployment, and each key has a shape
it must be: a credential comes from a secret setting that code never sees,
`sandbox:` names the package's own operations and resources, and `erasure:`
names a field of a person's record. Each of these can only be checked against
what the key means, and the wrong shape either leaks a secret or does nothing.

## What it looks like

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
  reason: A credential is read from the deployment's environment and never seen by code, so it is a `secret: true` setting, `config.<name>`.
  next: Add `secret: true` to `couriers_api_key` in `config.jadpo` (agent)
  rule: connection.value_invalid
  help: https://jadpo.dev/docs/errors/connection.value_invalid

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

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
  help: https://jadpo.dev/docs/errors/connection.value_invalid

check failed: 1 error, 12 files
```

`erasure: { key: }` names the field of the person's record holding what the
provider knows them by, so it is a field of a `person: true` entity, of the
type the package's `erasure:` `by:` names. Each of these is this error, such
as `` `pay`'s `erasure:` key `User.plan` isn't a `CustomerId` ``
([Erasure at providers](../language/15-personal-data.md#erasure-at-providers)).

A driver's keys are held to their own shapes, such as a whole number of at
least 1 for `namespace:`:

```jadpo check project=e-connection-value-namespace file=app.jadpo expect=connection.value_invalid
application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 0
        }
    }
}
```

```text output
error: `limits`'s `namespace:` must be a whole number of at least 1
 --> app.jadpo:5:24
  |
5 |             namespace: 0
  |                        ^ not a namespace id
  reason: A connection with `driver: Driver.cloudflare_rate_limits` counts rate limits in Cloudflare's rate-limiting bindings, one binding for each limit the app counts, since a binding counts one limit over one window. `namespace:` is the first of the namespace ids in your Cloudflare account the bindings are numbered from, a whole number of at least 1; they are written into the Wrangler file when the app is built.
  next: Write a namespace id no other rate limit in your Cloudflare account uses, such as `namespace: 1001` (agent)
  rule: connection.value_invalid
  help: https://jadpo.dev/docs/errors/connection.value_invalid

check failed: 1 error, 1 file
```

## How to fix it

**If a credential isn't secret**, mark its setting `secret: true`:

```jadpo fragment
config {
    couriers_api_key: Text {
        secret: true
    }
    ...
}
```

```jadpo check project=e-connection-value-invalid-fix-1 file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-value-invalid-fix-1 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-value-invalid-fix-1 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-connection-value-invalid-fix-1 file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-value-invalid-fix-1 file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e-connection-value-invalid-fix-1 file=app.jadpo hidden
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

The credential is now read from the deployment's environment and never reaches
code.

**If a namespace id is wrong**, give a whole number of at least 1:

```jadpo fragment
            namespace: 1001
```

```jadpo check project=e-connection-value-namespace-fix file=app.jadpo hidden
application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
    }
    rate_limits: {
        store: limits
    }
}
```

The binding is now numbered from that id. Use one no other rate limit in your
Cloudflare account uses.

## Related

- [Connecting in `app.jadpo`](../language/11-integrations.md#connecting-in-appjadpo)
- [Letting calls through to the sandbox](../language/11-integrations.md#letting-calls-through-to-the-sandbox)
- [Erasure at providers](../language/15-personal-data.md#erasure-at-providers)
- [Secrets](../language/10-configuration-and-time.md#secrets)
