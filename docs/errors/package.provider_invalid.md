# `package.provider_invalid`

`provider { }` has one fixed set of keys, `protocol:` among them:

```jadpo check project=p71-provider file=app.jadpo hidden
application {}
```

```jadpo check project=p71-provider file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-provider file=shared/providers/acme_couriers/v1/provider.jadpo expect=package.provider_invalid
provider {
    protocol: Protocol.json_http
    host: "https://api.acme-couriers.example"
}
```

```text output
error: `provider` has no key `host:`
 --> shared/providers/acme_couriers/v1/provider.jadpo:3:5
  |
3 |     host: "https://api.acme-couriers.example"
  |     ^^^^ not a provider key
  reason: `provider { }` has one fixed set of keys: `protocol:`, `hosts:`, `runs:`, `credential:`, `oauth:`, `idempotency:`, `error_code:`, `webhooks:`, `polls:`, `erasure:`, `max_concurrent:`, `settings:`, `imported:`, `description:`, `aws:`, `credential_header:` and `headers:`. Did you mean `hosts:`?
  next: Use `hosts:` instead of `host:` (agent)
  rule: package.provider_invalid
  help: docs/errors/package.provider_invalid.md

check failed: 1 error, 3 files
```

`imported:`'s `reviewed:` names the package's own types the agent has
gone through (area 11, "Mapping imported types to standard types"), so a
name that isn't one is a mistake:

```jadpo check project=e72e2-reviewed file=app.jadpo hidden
application {}
```

```jadpo check project=e72e2-reviewed file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e2-reviewed file=shared/providers/acme_couriers/v1/types/delivery.jadpo hidden
/// A parcel delivered.
public type Delivery = Object {
    id: Text
}
```

```jadpo check project=e72e2-reviewed file=shared/providers/acme_couriers/v1/provider.jadpo expect=package.provider_invalid
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
    imported: {
        file: "acme-couriers.json"
        digest: "sha256:4f1c"
        reviewed: [Delivry]
    }
}
```

```text output
error: `Delivry` in `reviewed:` isn't one of the package's types
 --> shared/providers/acme_couriers/v1/provider.jadpo:10:20
   |
10 |         reviewed: [Delivry]
   |                    ^^^^^^^ not a type of the package
  reason: `imported:` gives the description the package was imported from, `file:` and `digest:`, and `reviewed:`, the package's types the agent has gone through against the provider's documentation, each named once. The package declares no type `Delivry` in its `types/`.
  next: Use `Delivery` instead of `Delivry` (agent)
  rule: package.provider_invalid
  help: docs/errors/package.provider_invalid.md

check failed: 1 error, 4 files
```
