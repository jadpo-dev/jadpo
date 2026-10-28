# `connection.key_invalid`

Where an API is comes from exactly one place, so a package with fixed
`hosts:` takes no `host:`; a key the package doesn't take, or no
connection takes, is refused the same way:

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
  help: docs/language/11-integrations.html#connection-key-invalid

check failed: 1 error, 6 files
```
