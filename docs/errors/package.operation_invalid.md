# `package.operation_invalid`

A provider package's operation holds something it can't, or lacks something it needs.

## Why Jadpo refuses it

Each operation a provider package declares in `resources/` is read and
checked, since calling it is compiled from what it says: its keys,
`method:` and `path:`, its `input:` one of the package's object types, its
`failures:` the package's own, no two read from a refusal the same way, and
`output_at:` and `failed_when:` places in the answer. An operation that
holds something it can't, or lacks something it needs, couldn't be called
correctly.

## What it looks like

A placeholder in `path:` is filled from the input's field of that spelling
on the wire, and no field is sent as `parcelId`:

```jadpo check project=e72c-operation file=app.jadpo hidden
application {}
```

```jadpo check project=e72c-operation file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c-operation file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=e72c-operation file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e72c-operation file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

`shared/providers/acme_couriers/v1/resources/parcels.jadpo`:

```jadpo check project=e72c-operation file=shared/providers/acme_couriers/v1/resources/parcels.jadpo expect=package.operation_invalid
public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{parcelId}"
        input: ParcelLookup
        output: Parcel
        changes: false
        source: PackageSource.written
    }
}
```

```text output
error: `parcels.get`'s path names `{parcelId}`, which its input doesn't give
 --> shared/providers/acme_couriers/v1/resources/parcels.jadpo:4:15
  |
4 |         path: "/v1/parcels/{parcelId}"
  |               ^^^^^^^^^^^^^^^^^^^^^^^^ a placeholder with no field
  reason: A placeholder in `path:` is filled from the input's field of that spelling on the wire, and its input, `AcmeCouriers.ParcelLookup`, has no field sent as `parcelId`.
  next: Add a field `parcelId` to the input, or give the field the provider's spelling with `wire_name: "parcelId"` (agent)
  rule: package.operation_invalid
  help: https://jadpo.dev/docs/errors/package.operation_invalid

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An operation's failures are told apart by what the provider answers, its
status and its text ([A provider's failures](../language/11-integrations.md#a-providers-failures)),
so two read the same way can't both be listed:

```jadpo check project=e72f-failures-alike file=app.jadpo hidden
application {}
```

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/package.jadpo hidden
/// postcodes.io's UK postcode lookups.
package PostcodesIo.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/provider.jadpo hidden
/// postcodes.io's public API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.postcodes.io"
    }
    error_code: "error"
}
```

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/types/outcode_lookup.jadpo hidden
/// Which outward code to look up.
public type OutcodeLookup = Object {
    outcode: Text
}
```

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/types/outcode.jadpo hidden
/// An outward code postcodes.io holds.
public type Outcode = Object {
    outcode: Text
}
```

`shared/providers/postcodes_io/v1/failures/nothing_found.jadpo`:

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/failures/nothing_found.jadpo
/// postcodes.io found nothing at that address.
public type NothingFound = Failure {
    kind: FailureKind.not_found
    wire_status: 404
}
```

`shared/providers/postcodes_io/v1/failures/outcode_not_found.jadpo`:

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/failures/outcode_not_found.jadpo
/// postcodes.io holds no such outward code.
public type OutcodeNotFound = Failure {
    kind: FailureKind.not_found
    wire_status: 404
}
```

`shared/providers/postcodes_io/v1/resources/outcodes.jadpo`:

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/resources/outcodes.jadpo expect=package.operation_invalid
public resource outcodes {
    get: {
        method: HttpMethod.get
        path: "/outcodes/{outcode}"
        input: OutcodeLookup
        output: Outcode
        changes: false
        failures: [NothingFound, OutcodeNotFound]
        source: PackageSource.written
    }
}
```

```text output
error: `OutcodeNotFound` and `NothingFound` are read from the same refusal
 --> shared/providers/postcodes_io/v1/resources/outcodes.jadpo:8:34
  |
8 |         failures: [NothingFound, OutcodeNotFound]
  |                                  ^^^^^^^^^^^^^^^ read the same way
  reason: `outcodes.get`'s failures are told apart by what the provider answers: both are any `404`. Two read the same way leave the runtime no way to say which one came.
  next: Give `OutcodeNotFound` or `NothingFound` its own `wire_status:` or `wire_name:`, or list only one (agent)
  rule: package.operation_invalid
  help: https://jadpo.dev/docs/errors/package.operation_invalid

check failed: 1 error, 8 files
```

Everything else this rule checks has a message of its own: a key an
operation doesn't take, a missing `method:` or `path:`, an input that isn't
an object type, a failure that isn't the package's, a `decode:` without
`wire_output:`.

## How to fix it

**If the provider spells the field differently**, give the field its
spelling with `wire_name:`:

```jadpo check project=e72c-operation-fix file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text {
        wire_name: "parcelId"
    }
}
```

```jadpo check project=e72c-operation-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72c-operation-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c-operation-fix file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=e72c-operation-fix file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

```jadpo check project=e72c-operation-fix file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{parcelId}"
        input: ParcelLookup
        output: Parcel
        changes: false
        source: PackageSource.written
    }
}
```

The placeholder is now filled from `parcel_id`, sent as `parcelId`.

**If the placeholder is misspelled**, write the field's name:

```jadpo check project=e72c-operation-fix-2 file=shared/providers/acme_couriers/v1/resources/parcels.jadpo
public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{parcel_id}"
        input: ParcelLookup
        output: Parcel
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=e72c-operation-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e72c-operation-fix-2 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c-operation-fix-2 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=e72c-operation-fix-2 file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e72c-operation-fix-2 file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

The path takes its value from the input's `parcel_id`.

**If two failures are read the same way**, tell them apart by the
provider's own text for each, at `error_code:`:

```jadpo check project=e72f-failures-alike-fix file=shared/providers/postcodes_io/v1/failures/nothing_found.jadpo
/// postcodes.io found nothing at that address.
public type NothingFound = Failure {
    kind: FailureKind.not_found
    wire_status: 404
    wire_name: "Resource not found"
}
```

```jadpo check project=e72f-failures-alike-fix file=shared/providers/postcodes_io/v1/failures/outcode_not_found.jadpo
/// postcodes.io holds no such outward code.
public type OutcodeNotFound = Failure {
    kind: FailureKind.not_found
    wire_status: 404
    wire_name: "Outcode not found"
}
```

```jadpo check project=e72f-failures-alike-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72f-failures-alike-fix file=shared/providers/postcodes_io/v1/package.jadpo hidden
/// postcodes.io's UK postcode lookups.
package PostcodesIo.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72f-failures-alike-fix file=shared/providers/postcodes_io/v1/provider.jadpo hidden
/// postcodes.io's public API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.postcodes.io"
    }
    error_code: "error"
}
```

```jadpo check project=e72f-failures-alike-fix file=shared/providers/postcodes_io/v1/types/outcode_lookup.jadpo hidden
/// Which outward code to look up.
public type OutcodeLookup = Object {
    outcode: Text
}
```

```jadpo check project=e72f-failures-alike-fix file=shared/providers/postcodes_io/v1/types/outcode.jadpo hidden
/// An outward code postcodes.io holds.
public type Outcode = Object {
    outcode: Text
}
```

```jadpo check project=e72f-failures-alike-fix file=shared/providers/postcodes_io/v1/resources/outcodes.jadpo hidden
public resource outcodes {
    get: {
        method: HttpMethod.get
        path: "/outcodes/{outcode}"
        input: OutcodeLookup
        output: Outcode
        changes: false
        failures: [NothingFound, OutcodeNotFound]
        source: PackageSource.written
    }
}
```

The runtime can now say which of the two a `404` is.

**If only one of them can really happen**, list that one:

```jadpo check project=e72f-failures-alike-fix-2 file=shared/providers/postcodes_io/v1/resources/outcodes.jadpo
public resource outcodes {
    get: {
        method: HttpMethod.get
        path: "/outcodes/{outcode}"
        input: OutcodeLookup
        output: Outcode
        changes: false
        failures: [OutcodeNotFound]
        source: PackageSource.written
    }
}
```

```jadpo check project=e72f-failures-alike-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e72f-failures-alike-fix-2 file=shared/providers/postcodes_io/v1/package.jadpo hidden
/// postcodes.io's UK postcode lookups.
package PostcodesIo.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72f-failures-alike-fix-2 file=shared/providers/postcodes_io/v1/provider.jadpo hidden
/// postcodes.io's public API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.postcodes.io"
    }
    error_code: "error"
}
```

```jadpo check project=e72f-failures-alike-fix-2 file=shared/providers/postcodes_io/v1/types/outcode_lookup.jadpo hidden
/// Which outward code to look up.
public type OutcodeLookup = Object {
    outcode: Text
}
```

```jadpo check project=e72f-failures-alike-fix-2 file=shared/providers/postcodes_io/v1/types/outcode.jadpo hidden
/// An outward code postcodes.io holds.
public type Outcode = Object {
    outcode: Text
}
```

```jadpo check project=e72f-failures-alike-fix-2 file=shared/providers/postcodes_io/v1/failures/outcode_not_found.jadpo hidden
/// postcodes.io holds no such outward code.
public type OutcodeNotFound = Failure {
    kind: FailureKind.not_found
    wire_status: 404
}
```

Any `404` from `outcodes.get` is `OutcodeNotFound`.

## Related

- [Written by hand, when there is no specification](../language/11-integrations.md#written-by-hand-when-there-is-no-specification)
- [Spellings: `wire_name:`](../language/11-integrations.md#spellings-wire_name)
- [A provider's failures](../language/11-integrations.md#a-providers-failures)
- [Calling a provider](../language/11-integrations.md#calling-a-provider)
