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
  reason: `provider { }` has one fixed set of keys: `protocol:`, `hosts:`, `runs:`, `credential:`, `oauth:`, `idempotency:`, `error_code:`, `webhooks:`, `polls:`, `erasure:`, `max_concurrent:`, `settings:`, `imported:`, `reviewed:`, `description:`, `aws:`, `credential_header:` and `headers:`. Did you mean `hosts:`?
  next: Use `hosts:` instead of `host:` (agent)
  rule: package.provider_invalid
  help: https://jadpo.dev/docs/errors/package.provider_invalid

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
  help: https://jadpo.dev/docs/errors/package.provider_invalid

check failed: 1 error, 4 files
```

A package written by hand has no `imported:`, so it lists its reviewed types in
`provider { }` itself. An imported package keeps its list in `imported:`,
beside the description it was reviewed against, so a `reviewed:` beside
`imported:` is a mistake:

```jadpo check project=e72e2-reviewed-beside file=app.jadpo hidden
application {}
```

```jadpo check project=e72e2-reviewed-beside file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e2-reviewed-beside file=shared/providers/acme_couriers/v1/types/delivery.jadpo hidden
/// A parcel delivered.
public type Delivery = Object {
    id: Text
}
```

```jadpo check project=e72e2-reviewed-beside file=shared/providers/acme_couriers/v1/provider.jadpo expect=package.provider_invalid
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
    imported: {
        file: "acme-couriers.json"
        digest: "sha256:4f1c"
    }
    reviewed: [Delivery]
}
```

```text output
error: `reviewed:` is beside `imported:`
 --> shared/providers/acme_couriers/v1/provider.jadpo:11:5
   |
11 |     reviewed: [Delivery]
   |     ^^^^^^^^^^^^^^^^^^^^ outside `imported:`
  reason: An imported package lists its reviewed types in `imported:`, beside the description they were reviewed against; `reviewed:` in `provider { }` itself is a hand-written package's.
  next: Move the list into `imported:` as its `reviewed:` (agent)
  rule: package.provider_invalid
  help: https://jadpo.dev/docs/errors/package.provider_invalid

check failed: 1 error, 4 files
```

`erasure: { call: customers.delete  by: CustomerId }` names the operation that
erases a person and the type of the key it takes. So `call:` is one of the
package's operations with `changes: true`, and `by:` the type of exactly one
field of its input, any other optional. Each of these is this error, such as
`` `erasure:` can't call `customers.delete` with only a `Words` ``
([Erasure at providers](../language/15-personal-data.md#erasure-at-providers)).
