# `connection.package_unknown`

A connection names a package the app doesn't hold, or a library package, which needs no connection.

## Why Jadpo refuses it

A connection's `package:` names a provider package by its name and version:
one installed in `packages/`, or the app's own in `shared/providers/`. An
unknown name can't be connected to anything, so Jadpo refuses it rather than
fail when the first call is made.

A library package carries types, failures and functions and calls no API, so
a connection to one would have nothing to connect.

## What it looks like

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

When the app holds no package of that name, the error says how to get one:

```jadpo check project=e-connection-package-none file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-package-none file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-package-none file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-package-none file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-package-none file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e-connection-package-none file=app.jadpo expect=connection.package_unknown
application {
    connections: {
        couriers: {
            package: OtherCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
        }
    }
}
```

```text output
error: `couriers` connects to `OtherCouriers.v1`, which the app doesn't hold
 --> app.jadpo:4:22
  |
4 |             package: OtherCouriers.v1
  |                      ^^^^^^^^^^^^^^^^ no such package
  reason: A connection's `package:` names a provider package by its name and version, installed in `packages/` or the app's own in `shared/providers/`; the app holds no package called `OtherCouriers`.
  next: Install it with `jadpo package add`, or write it as the app's own integration in `shared/providers/<name>/<version>/` (agent)
  rule: connection.package_unknown
  help: https://jadpo.dev/docs/errors/connection.package_unknown

check failed: 1 error, 6 files
```

## How to fix it

**If the version is wrong**, write the version the app holds:

```jadpo fragment
            package: AcmeCouriers.v1
```

```jadpo check project=e-connection-package-unknown-fix file=app.jadpo hidden
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

```jadpo check project=e-connection-package-unknown-fix file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-package-unknown-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-package-unknown-fix file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-package-unknown-fix file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-package-unknown-fix file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

`couriers` now connects to the package in `shared/providers/acme_couriers/v1/`.

**If the package isn't installed**, add it by its address and version, then
name it in `package:`:

```sh
jadpo package add github.com/acme/couriers@v1.0.0
```

The package then lives in `packages/`, where a connection can name it.

## Related

- [Connecting in `app.jadpo`](../language/11-integrations.md#connecting-in-appjadpo)
- [How connections to packages are read](../language/11-integrations.md#how-connections-to-packages-are-read)
