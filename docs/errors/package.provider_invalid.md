# `package.provider_invalid`

A provider file holds something it can't, or lacks `protocol:`.

## Why Jadpo refuses it

`provider { }` says how a package's API is reached, and everything calling
it is compiled from that. It has one fixed set of keys, `protocol:` among
them, each read and checked, so a key Jadpo doesn't read is a mistake
rather than something to ignore. What `erasure:`, `webhooks:`, `polls:` and
`imported:` name has to be there and be the right kind of thing.

## What it looks like

`host:` isn't one of the keys:

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

`shared/providers/acme_couriers/v1/provider.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`imported:`'s `reviewed:` names the package's own types the agent has
gone through, so a
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

`shared/providers/acme_couriers/v1/provider.jadpo`:

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
  reason: `imported:` gives the description the package was imported from, `file:` and `digest:`, `reviewed:`, the package's types the agent has gone through against the provider's documentation, each named once, and `renamed:`, each name in the description code knows by another. The package declares no type `Delivry` in its `types/`.
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

`shared/providers/acme_couriers/v1/provider.jadpo`:

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


A capability mapping, `provides/<capability>.jadpo`, names one of
Jadpo's capabilities a package may fill, and is data: no call or
expression anywhere in it. `cache` is filled only by Jadpo's own drivers:

```jadpo check project=k4-provide-cache file=app.jadpo hidden
application {}
```

```jadpo check project=k4-provide-cache file=shared/providers/acme_cache/v1/package.jadpo hidden
/// Acme's cache API.
package AcmeCache.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=k4-provide-cache file=shared/providers/acme_cache/v1/provider.jadpo hidden
/// Acme's cache API. The connection gives its address in `host:`.
provider {
    protocol: Protocol.json_http
}
```

`shared/providers/acme_cache/v1/provides/cache.jadpo`:

```jadpo check project=k4-provide-cache file=shared/providers/acme_cache/v1/provides/cache.jadpo expect=package.provider_invalid
/// Would fill `cache`.
provide cache {
}
```

```text output
error: `provide cache`: no package fills `cache`
 --> shared/providers/acme_cache/v1/provides/cache.jadpo:2:9
  |
2 | provide cache {
  |         ^^^^^ filled only by Jadpo's own drivers
  reason: Jadpo owns the list of capabilities. `database`, `cache` and `schedules` are filled only by its built-in drivers, configured in `app.jadpo`; a package may fill `files`, `email`, `publishing`, `receiving`, `search`, `bot_check`, `documents`, `scanning` and `ai`.
  next: Remove `shared/providers/acme_cache/v1/provides/cache.jadpo`; what the provider does beyond the capability is the package's own operations (agent)
  rule: package.provider_invalid
  help: https://jadpo.dev/docs/errors/package.provider_invalid

check failed: 1 error, 4 files
```

A call or an expression in a mapping, such as `lower(message.to)`, is
this error too: the computing belongs in a field's `decode:` or
`encode:` in the package's types.

`erasure: { call: customers.delete  by: CustomerId }` names the operation that
erases a person and the type of the key it takes. So `call:` is one of the
package's operations with `changes: true`, and `by:` the type of exactly one
field of its input, any other optional. Each of these is this error, such as
`` `erasure:` can't call `customers.delete` with only a `Words` ``
([Erasure at providers](../language/15-personal-data.md#erasure-at-providers)).

## How to fix it

**If a key is misspelled**, use the key the error names:

```jadpo check project=p71-provider-fix file=shared/providers/acme_couriers/v1/provider.jadpo
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=p71-provider-fix file=app.jadpo hidden
application {}
```

```jadpo check project=p71-provider-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

Jadpo reads the provider's host from `hosts:`.

**If `reviewed:` names something the package doesn't have**, write one of
its types:

```jadpo check project=e72e2-reviewed-fix file=shared/providers/acme_couriers/v1/provider.jadpo
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
    imported: {
        file: "acme-couriers.json"
        digest: "sha256:4f1c"
        reviewed: [Delivery]
    }
}
```

```jadpo check project=e72e2-reviewed-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72e2-reviewed-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e2-reviewed-fix file=shared/providers/acme_couriers/v1/types/delivery.jadpo hidden
/// A parcel delivered.
public type Delivery = Object {
    id: Text
}
```

The list now says which of the package's types the agent has gone through.

**If `reviewed:` is beside `imported:`**, move it in:

```jadpo check project=e72e2-reviewed-beside-fix file=shared/providers/acme_couriers/v1/provider.jadpo
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
    imported: {
        file: "acme-couriers.json"
        digest: "sha256:4f1c"
        reviewed: [Delivery]
    }
}
```

```jadpo check project=e72e2-reviewed-beside-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e72e2-reviewed-beside-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72e2-reviewed-beside-fix file=shared/providers/acme_couriers/v1/types/delivery.jadpo hidden
/// A parcel delivered.
public type Delivery = Object {
    id: Text
}
```

The list sits beside the description it was reviewed against.

## Related

- [How the API is reached: `provider.jadpo`](../language/11-integrations.md#how-the-api-is-reached-providerjadpo)
- [Mapping imported types to standard types](../language/11-integrations.md#mapping-imported-types-to-standard-types)
- [Erasure at providers](../language/15-personal-data.md#erasure-at-providers)
