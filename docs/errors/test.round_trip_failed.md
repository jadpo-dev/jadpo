# `test.round_trip_failed`

A field's `encode:` doesn't give back what its `decode:` read from a recording.

## Why Jadpo refuses it

A field with `decode:` and `encode:` reads what a provider sends and writes
it back. If the two don't undo each other, what the app sends isn't what
the provider sent. Jadpo checks each against the recordings of real
answers the package holds, so the mistake shows up in a test rather than
in a call to the provider.

## What it looks like

The package reads the courier's `EVE` as the afternoon, and writes the
afternoon back as `PM`:

```jadpo check project=e72d2-round-trip file=app.jadpo hidden
application {}
```

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
}
```

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/types/daypart_code.jadpo hidden
/// A part of the day as the courier writes it.
public type DaypartCode = Text {
    max_length: 3
}
```

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/types/daypart.jadpo hidden
/// When in the day a collection comes.
public type Daypart = Enum {
    morning
    afternoon
}
```

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/failures/daypart_unknown.jadpo hidden
/// A part of the day the courier names that this package doesn't know.
type DaypartUnknown = Failure {
    kind: FailureKind.invalid_value
    message: "The courier named a part of the day this package doesn't know."
}
```

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: Text
    daypart: Daypart {
        decode: dayparts.read_daypart
        encode: dayparts.write_daypart
    }
}
```

`shared/providers/acme_couriers/v1/libraries/dayparts.jadpo`:

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/libraries/dayparts.jadpo
/// The courier's part of the days, which it writes as "AM" and "PM".
library dayparts {
    /// The part of the day the courier's code names.
    function read_daypart(code: DaypartCode) fails DaypartUnknown -> Daypart {
        if (code == "AM") {
            return Daypart.morning
        }
        if (code == "PM" or code == "EVE") {
            return Daypart.afternoon
        }
        reject DaypartUnknown
    }

    /// The courier's code for a part of the day.
    function write_daypart(daypart: Daypart) -> DaypartCode {
        if (daypart == Daypart.morning) {
            return "AM"
        }
        return "PM"
    }
}
```

`shared/providers/acme_couriers/v1/recordings/bookings.create.json`:

```jadpo check project=e72d2-round-trip file=shared/providers/acme_couriers/v1/recordings/bookings.create.json expect=test.round_trip_failed
{
  "data": {
    "id": "{Text}",
    "daypart": "EVE"
  }
}
```

```text output
FAIL  bookings.create's recording decodes
 --> shared/providers/acme_couriers/v1/recordings/bookings.create.json:1

  decode: `dayparts.read_daypart` and `dayparts.write_daypart` don't round-trip `bookings.create`'s recording
  recorded: "EVE"
  written: "PM"
  reason: A field's `encode:` writes back what its `decode:` read, so what the app sends is what the provider sent; `dayparts.write_daypart` doesn't give back the value `dayparts.read_daypart` read from the recording.
  next: Correct `dayparts.read_daypart` or `dayparts.write_daypart` so each undoes the other (agent)

  rule: test.round_trip_failed
  rerun: jadpo test shared/providers/acme_couriers/v1/recordings/bookings.create.json:1 --seed 1

test failed: 1 failed, 1 recording test
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the provider's value means something of its own**, give it a value of
its own, so each side undoes the other:

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/types/daypart.jadpo
/// When in the day a collection comes.
public type Daypart = Enum {
    morning
    afternoon
    evening
}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/libraries/dayparts.jadpo
/// The courier's part of the days, which it writes as "AM", "PM" and "EVE".
library dayparts {
    /// The part of the day the courier's code names.
    function read_daypart(code: DaypartCode) fails DaypartUnknown -> Daypart {
        if (code == "AM") {
            return Daypart.morning
        }
        if (code == "PM") {
            return Daypart.afternoon
        }
        if (code == "EVE") {
            return Daypart.evening
        }
        reject DaypartUnknown
    }

    /// The courier's code for a part of the day.
    function write_daypart(daypart: Daypart) -> DaypartCode {
        if (daypart == Daypart.morning) {
            return "AM"
        }
        if (daypart == Daypart.afternoon) {
            return "PM"
        }
        return "EVE"
    }
}
```

```jadpo check project=e72d2-round-trip-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/types/daypart_code.jadpo hidden
/// A part of the day as the courier writes it.
public type DaypartCode = Text {
    max_length: 3
}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/failures/daypart_unknown.jadpo hidden
/// A part of the day the courier names that this package doesn't know.
type DaypartUnknown = Failure {
    kind: FailureKind.invalid_value
    message: "The courier named a part of the day this package doesn't know."
}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: Text
    daypart: Daypart {
        decode: dayparts.read_daypart
        encode: dayparts.write_daypart
    }
}
```

```jadpo check project=e72d2-round-trip-fix file=shared/providers/acme_couriers/v1/recordings/bookings.create.json hidden
{
  "data": {
    "id": "{Text}",
    "daypart": "EVE"
  }
}
```

`EVE` is read as the evening and written back as `EVE`, so the recording
round-trips.

**If the recording holds a value the provider doesn't send**, record a
real answer. `recordings/bookings.create.json`:

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/recordings/bookings.create.json
{
  "data": {
    "id": "{Text}",
    "daypart": "PM"
  }
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
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

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/types/create_booking.jadpo hidden
/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/types/daypart_code.jadpo hidden
/// A part of the day as the courier writes it.
public type DaypartCode = Text {
    max_length: 3
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/types/daypart.jadpo hidden
/// When in the day a collection comes.
public type Daypart = Enum {
    morning
    afternoon
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/failures/daypart_unknown.jadpo hidden
/// A part of the day the courier names that this package doesn't know.
type DaypartUnknown = Failure {
    kind: FailureKind.invalid_value
    message: "The courier named a part of the day this package doesn't know."
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/types/booking.jadpo hidden
/// A booked collection.
public type Booking = Object {
    id: Text
    daypart: Daypart {
        decode: dayparts.read_daypart
        encode: dayparts.write_daypart
    }
}
```

```jadpo check project=e72d2-round-trip-fix-2 file=shared/providers/acme_couriers/v1/libraries/dayparts.jadpo hidden
/// The courier's part of the days, which it writes as "AM" and "PM".
library dayparts {
    /// The part of the day the courier's code names.
    function read_daypart(code: DaypartCode) fails DaypartUnknown -> Daypart {
        if (code == "AM") {
            return Daypart.morning
        }
        if (code == "PM" or code == "EVE") {
            return Daypart.afternoon
        }
        reject DaypartUnknown
    }

    /// The courier's code for a part of the day.
    function write_daypart(daypart: Daypart) -> DaypartCode {
        if (daypart == Daypart.morning) {
            return "AM"
        }
        return "PM"
    }
}
```

`PM` is read as the afternoon and written back as `PM`.

## Related

- [A field's own functions: `decode:` and `encode:`](../language/11-integrations.md#a-fields-own-functions-decode-and-encode)
- [Recording tests](../language/13-testing.md#recording-tests)
- [`test.recording_undecodable`](../errors/test.recording_undecodable.md)
