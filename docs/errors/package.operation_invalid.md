# `package.operation_invalid`

> A provider package's operations in `resources/` are each read and
> checked: its keys, `method:` and `path:` (each placeholder
> one of its input's fields, by its spelling on the wire), its `input:`
> one of the package's object types, its `failures:` the package's own,
> and `output_at:` and `failed_when:` places in the answer.

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
