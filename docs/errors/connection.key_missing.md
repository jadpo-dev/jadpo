# `connection.key_missing`

Every connection to a package lists the domains that may call it, and
gives the credential, environment, host and settings its package needs:

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
  help: docs/errors/connection.key_missing.md

check failed: 1 error, 6 files
```
