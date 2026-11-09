# `connection.key_invalid`

A connection gives a key its package or driver doesn't take.

## Why Jadpo refuses it

A connection's keys are the settings its package or driver declares, and where
an API is comes from exactly one place: a package with fixed `hosts:` chooses
between them with `environment:`, so a `host:` beside them would be a second
answer. A key the package doesn't take would do nothing, and a reader would
think it did.

## What it looks like

```jadpo check project=e71c-key-invalid file=config.jadpo hidden
config {
    couriers_address: Url
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e71c-key-invalid file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e71c-key-invalid file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e71c-key-invalid file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e71c-key-invalid file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e71c-key-invalid file=app.jadpo expect=connection.key_invalid
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
            host: config.couriers_address
        }
    }
}
```

```text output
error: `couriers` gives `host:`, but `AcmeCouriers.v1` has fixed hosts
 --> app.jadpo:7:13
  |
7 |             host: config.couriers_address
  |             ^^^^ a second place for its address
  reason: Where an API is comes from exactly one place: `AcmeCouriers.v1`'s `hosts:` give its live and sandbox addresses, and `environment:` chooses between them.
  next: Remove `host:` (agent)
  rule: connection.key_invalid
  help: https://jadpo.dev/docs/errors/connection.key_invalid

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A key no connection takes is refused the same way:

```jadpo check project=e-connection-key-unknown file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-unknown file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-unknown file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-key-unknown file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-unknown file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e-connection-key-unknown file=app.jadpo expect=connection.key_invalid
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
            region: "eu"
        }
    }
}
```

```text output
error: `couriers` has no key `region:`
 --> app.jadpo:7:13
  |
7 |             region: "eu"
  |             ^^^^^^ not a connection key
  reason: A connection to a package takes `package:`, `domains:`, `credential:`, `environment:`, `host:`, `timeout:`, `retry:`, `max_concurrent:`, `webhooks:`, `erasure:`, `development:`, `polls:` and `outbound:`, and `AcmeCouriers.v1` declares no settings of its own.
  next: Remove `region:` (agent)
  rule: connection.key_invalid
  help: https://jadpo.dev/docs/errors/connection.key_invalid

check failed: 1 error, 6 files
```

A package with no `credential:` scheme takes no `credential:`:

```jadpo check project=e-connection-key-credential file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-credential file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-credential file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-key-credential file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-credential file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e-connection-key-credential file=app.jadpo expect=connection.key_invalid
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
error: `couriers` gives `credential:`, which `AcmeCouriers.v1` doesn't take
 --> app.jadpo:6:13
  |
6 |             credential: config.couriers_api_key
  |             ^^^^^^^^^^ not taken
  reason: `AcmeCouriers.v1`'s `provider.jadpo` names no credential scheme, so its API is called without one.
  next: Remove `credential:`, or give the package's `provider.jadpo` the `credential:` scheme its API needs (agent)
  rule: connection.key_invalid
  help: https://jadpo.dev/docs/errors/connection.key_invalid

check failed: 1 error, 6 files
```

A connection with `driver:` takes what its driver takes:

```jadpo check project=e-connection-key-driver file=app.jadpo expect=connection.key_invalid
application {
    connections: {
        main: {
            driver: Driver.postgres
            pool: 5
        }
    }
}
```

```text output
error: A connection with `driver: Driver.postgres` takes no `pool:`
 --> app.jadpo:5:13
  |
5 |             pool: 5
  |             ^^^^ not a database setting
  reason: A connection to a PostgreSQL database gives `url:`, a `PostgresUrl` setting, and may give `authority:`, a `Certificate` setting its server's certificate is checked against, or `tls: false` for a server elsewhere that offers no TLS.
  next: Remove `pool:` (agent)
  rule: connection.key_invalid
  help: https://jadpo.dev/docs/errors/connection.key_invalid

check failed: 1 error, 1 file
```

The blocks inside a connection, such as `development:`, `webhooks:`, `polls:`
and `credential:`, refuse a key they don't hold the same way, and so does
each of the other drivers.

## How to fix it

**If the key isn't one the package takes**, remove it:

```jadpo fragment
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
        }
```

```jadpo check project=e-connection-key-invalid-fix file=app.jadpo hidden
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

```jadpo check project=e-connection-key-invalid-fix file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-invalid-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-invalid-fix file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-key-invalid-fix file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-invalid-fix file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

Calls now go to the live or sandbox host `environment:` chooses, which is the
only place the address comes from.

## Related

- [Connecting in `app.jadpo`](../language/11-integrations.md#connecting-in-appjadpo)
- [Connection errors](../language/11-integrations.md#connection-errors)
