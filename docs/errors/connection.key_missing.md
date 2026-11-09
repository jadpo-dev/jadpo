# `connection.key_missing`

A connection leaves out a key its package or driver needs.

## Why Jadpo refuses it

A connection lists the domains that may call it, so a new caller is a one-line
change a reviewer sees. It also gives what its package needs to reach the
provider: the credential the package's scheme sends, and which of its hosts to
use. Those are values only you have, so Jadpo can't fill them in.

## What it looks like

```jadpo check project=e71c-key-missing file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e71c-key-missing file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e71c-key-missing file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e71c-key-missing file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e71c-key-missing file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e71c-key-missing file=app.jadpo expect=connection.key_missing
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            environment: config.couriers_environment
        }
    }
}
```

```text output
error: `couriers` doesn't give `domains:`
 --> app.jadpo:3:9
  |
3 |         couriers: {
  |         ^^^^^^^^ no `domains:`
  reason: A connection lists the domains whose actions may call it, so a new caller is a one-line change a reviewer sees; other domains go through those domains' public actions.
  next: Add `domains: [<domain>]`, listing the domains that call `couriers` (agent)
  rule: connection.key_missing
  help: https://jadpo.dev/docs/errors/connection.key_missing

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A package with a live and a sandbox host needs `environment:`, so each
deployment says which it calls:

```jadpo check project=e-connection-key-environment file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-environment file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-environment file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-key-environment file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-environment file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e-connection-key-environment file=app.jadpo expect=connection.key_missing
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
        }
    }
}
```

```text output
error: `couriers` doesn't give `environment:`
 --> app.jadpo:3:9
  |
3 |         couriers: {
  |         ^^^^^^^^ no `environment:`
  reason: `AcmeCouriers.v1` has a live and a sandbox host, and `environment:`, a `ProviderEnvironment` setting, chooses between them, so each deployment says which it calls.
  next: Declare `<name>: ProviderEnvironment` in `config.jadpo`, and add `environment: config.<name>` (agent)
  rule: connection.key_missing
  help: https://jadpo.dev/docs/errors/connection.key_missing

check failed: 1 error, 6 files
```

A package that sends a credential needs it from a secret setting:

```jadpo check project=e-connection-key-credential-missing file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-credential-missing file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-credential-missing file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-connection-key-credential-missing file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-credential-missing file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e-connection-key-credential-missing file=app.jadpo expect=connection.key_missing
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

```text output
error: `couriers` doesn't give `credential:`
 --> app.jadpo:3:9
  |
3 |         couriers: {
  |         ^^^^^^^^ no `credential:`
  reason: `AcmeCouriers.v1` sends a `bearer` credential with every call, which the connection gives from a secret setting.
  next: Declare `<name>: Text { secret: true }` in `config.jadpo`, and add `credential: config.<name>` (agent)
  rule: connection.key_missing
  help: https://jadpo.dev/docs/errors/connection.key_missing

check failed: 1 error, 6 files
```

A driver needs what it needs too, such as `namespace:` on
`Driver.cloudflare_rate_limits`, and `host:` on `Driver.smtp`.

## How to fix it

**If `domains:` is missing**, list the domains whose actions call the
connection:

```jadpo fragment
        couriers: {
            ...
            domains: [shipping]
            ...
        }
```

```jadpo check project=e-connection-key-missing-fix-1 file=app.jadpo hidden
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

```jadpo check project=e-connection-key-missing-fix-1 file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-missing-fix-1 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-missing-fix-1 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-key-missing-fix-1 file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-missing-fix-1 file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

`shipping` can now call `couriers`, and no other domain can.

**If `environment:` is missing**, name a `ProviderEnvironment` setting:

```jadpo fragment
        couriers: {
            ...
            environment: config.couriers_environment
        }
```

```jadpo check project=e-connection-key-missing-fix-env file=app.jadpo hidden
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

```jadpo check project=e-connection-key-missing-fix-env file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-missing-fix-env file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-missing-fix-env file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-connection-key-missing-fix-env file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-missing-fix-env file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

Each deployment now says, in `config.jadpo`, whether it calls the live host or
the sandbox.

**If the credential is missing**, declare a secret setting and pass it:

```jadpo fragment
config {
    couriers_api_key: Text {
        secret: true
    }
    ...
}
```

```jadpo check project=e-connection-key-missing-fix-2 file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-connection-key-missing-fix-2 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-connection-key-missing-fix-2 file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-connection-key-missing-fix-2 file=domains/shipping/types/parcel_reference.jadpo hidden
/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
```

```jadpo check project=e-connection-key-missing-fix-2 file=domains/shipping/actions/read_reference.jadpo hidden
use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
```

```jadpo check project=e-connection-key-missing-fix-2 file=app.jadpo hidden
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

Each call now sends the credential from the deployment's environment, which
code never sees.

## Related

- [Connecting in `app.jadpo`](../language/11-integrations.md#connecting-in-appjadpo)
- [Connection errors](../language/11-integrations.md#connection-errors)
