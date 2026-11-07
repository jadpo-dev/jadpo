# `package.operation_invalid`

> A provider package's operations in `resources/` are each read and
> checked: its keys, `method:` and `path:` (each placeholder
> one of its input's fields, by its spelling on the wire), its `input:`
> one of the package's object types, its `failures:` the package's own,
> no two read from a refusal the same way, and `output_at:` and
> `failed_when:` places in the answer.

A placeholder in `path:` is filled from the input's field of that
spelling on the wire:

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

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/failures/not_found.jadpo
/// postcodes.io found nothing at that address.
public type NotFound = Failure {
    kind: FailureKind.not_found
    wire_status: 404
}
```

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/failures/outcode_not_found.jadpo
/// postcodes.io holds no such outward code.
public type OutcodeNotFound = Failure {
    kind: FailureKind.not_found
    wire_status: 404
}
```

```jadpo check project=e72f-failures-alike file=shared/providers/postcodes_io/v1/resources/outcodes.jadpo expect=package.operation_invalid
public resource outcodes {
    get: {
        method: HttpMethod.get
        path: "/outcodes/{outcode}"
        input: OutcodeLookup
        output: Outcode
        changes: false
        failures: [NotFound, OutcodeNotFound]
        source: PackageSource.written
    }
}
```

```text output
error: `OutcodeNotFound` and `NotFound` are read from the same refusal
 --> shared/providers/postcodes_io/v1/resources/outcodes.jadpo:8:30
  |
8 |         failures: [NotFound, OutcodeNotFound]
  |                              ^^^^^^^^^^^^^^^ read the same way
  reason: `outcodes.get`'s failures are told apart by what the provider answers: both are any `404`. Two read the same way leave the runtime no way to say which one came.
  next: Give `OutcodeNotFound` or `NotFound` its own `wire_status:` or `wire_name:`, or list only one (agent)
  rule: package.operation_invalid
  help: https://jadpo.dev/docs/errors/package.operation_invalid

check failed: 1 error, 8 files
```
