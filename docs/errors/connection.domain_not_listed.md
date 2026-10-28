# `connection.domain_not_listed`

> Only the actions of the domains a connection's
> `domains:` lists call it (question 18).

A connection lists the domains that call it, so a new caller is a change
a reviewer sees:

```jadpo check project=e72c-domain file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e72c-domain file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c-domain file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72c-domain file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{parcel_id}"
        input: ParcelLookup
        output: Parcel
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=e72c-domain file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e72c-domain file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

```jadpo check project=e72c-domain file=app.jadpo hidden
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

```jadpo check project=e72c-domain file=domains/shipping/actions/read_lookup.jadpo hidden
use shared/providers/acme_couriers

/// Reads which parcel a customer asked about.
action read_lookup(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.ParcelLookup {
    return lookup
}
```

```jadpo check project=e72c-domain file=domains/support/actions/track_parcel.jadpo expect=connection.domain_not_listed
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

```text output
error: `support` calls `couriers`, whose `domains:` doesn't list it
 --> domains/support/actions/track_parcel.jadpo:6:12
  |
6 |     return couriers.parcels.get(lookup)
  |            ^^^^^^^^^^^^^^^^^^^^ not a listed domain
  reason: A connection lists the domains whose actions may call it, so a new caller is a one-line change a reviewer sees; `couriers` lists `shipping`, and other domains go through their public actions.
  next: Add `support` to `couriers`'s `domains:` in `app.jadpo`, or call one of `shipping`'s public actions instead (human)
  rule: connection.domain_not_listed
  help: docs/language/11-integrations.html#connection-domain-not-listed

check failed: 1 error, 9 files
```
