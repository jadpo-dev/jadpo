# `connection.package_unknown`

A connection names a provider package the app holds, by its name and
version; a library package needs no connection at all:

```jadpo check project=e71c-package-unknown file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e71c-package-unknown file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e71c-package-unknown file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e71c-package-unknown file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e71c-package-unknown file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e71c-package-unknown file=app.jadpo expect=connection.package_unknown
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v2
            domains: [shipping]
            environment: config.couriers_environment
        }
    }
}
```

```text output
error: `couriers` connects to `AcmeCouriers.v2`, which the app doesn't hold
 --> app.jadpo:4:22
  |
4 |             package: AcmeCouriers.v2
  |                      ^^^^^^^^^^^^^^^ no such package
  reason: A connection's `package:` names a provider package by its name and version, installed in `packages/` or the app's own in `shared/providers/`; the app holds `AcmeCouriers.v1`.
  next: Write `package: AcmeCouriers.v1` (agent)
  rule: connection.package_unknown
  help: https://jadpo.dev/docs/errors/connection.package_unknown

check failed: 1 error, 6 files
```
