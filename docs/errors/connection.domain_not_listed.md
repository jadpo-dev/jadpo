# `connection.domain_not_listed`

An action calls an outside service through a connection whose `domains:` list doesn't include the action's domain.

## Why Jadpo refuses it

Each connection in `app.jadpo` lists the domains whose actions may call
it, such as `domains: [shipping]` for a courier's API. A new caller then
shows in review as a one-line change to that list. Other domains reach
the service through the listed domain's public actions, so the code that
talks to it stays in one place.

Jadpo won't add the domain for you. The list decides which parts of your
app may use an outside service, and so what a change to one domain can
reach.

## What it looks like

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

app.jadpo:

```jadpo check project=e72c-domain file=app.jadpo
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

The `support` domain calls the connection directly.

domains/support/actions/track_parcel.jadpo:

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
  help: https://jadpo.dev/docs/errors/connection.domain_not_listed

check failed: 1 error, 9 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which domains may call the courier's API.

## How to fix it

The question is whether `support` should talk to the courier itself.

**If it should**, add `support` to the connection's `domains:` in
`app.jadpo`:

```jadpo fragment
            domains: [shipping, support]
```

```jadpo check project=e72c-domain-listed file=app.jadpo hidden
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping, support]
            environment: config.couriers_environment
        }
    }
}
```

```jadpo check project=e72c-domain-listed file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e72c-domain-listed file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c-domain-listed file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72c-domain-listed file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e72c-domain-listed file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e72c-domain-listed file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

```jadpo check project=e72c-domain-listed file=domains/shipping/actions/read_lookup.jadpo hidden
use shared/providers/acme_couriers

/// Reads which parcel a customer asked about.
action read_lookup(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.ParcelLookup {
    return lookup
}
```

```jadpo check project=e72c-domain-listed file=domains/support/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

```jadpo check project=e72c-domain-listed file=network.jadpo_record hidden
// Generated by Jadpo from the hosts the app can reach; update it with `jadpo records update`, never by hand.
record network 1

connection couriers
    for AcmeCouriers.v1
    host https://api.acme-couriers.example
    host https://sandbox.acme-couriers.example
```

`support`'s actions may now call `couriers` directly, and the change shows
in review as a new caller.

**If it should go through `shipping`**, give `shipping` a public action
that makes the call. `domains/shipping/actions/locate_parcel.jadpo`:

```jadpo check project=e72c-domain-public file=domains/shipping/actions/locate_parcel.jadpo
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is, for other domains to ask.
public action locate_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

Then `support` calls that action, with `use shipping` in place of
`use app { couriers }`. In `domains/support/actions/track_parcel.jadpo`:

```jadpo fragment
use shipping
    return shipping.locate_parcel(lookup)
```

```jadpo check project=e72c-domain-public file=domains/support/actions/track_parcel.jadpo hidden
use shipping
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return shipping.locate_parcel(lookup)
}
```

```jadpo check project=e72c-domain-public file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e72c-domain-public file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e72c-domain-public file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e72c-domain-public file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e72c-domain-public file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e72c-domain-public file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

```jadpo check project=e72c-domain-public file=app.jadpo hidden
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

```jadpo check project=e72c-domain-public file=domains/shipping/actions/read_lookup.jadpo hidden
use shared/providers/acme_couriers

/// Reads which parcel a customer asked about.
action read_lookup(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.ParcelLookup {
    return lookup
}
```

```jadpo check project=e72c-domain-public file=network.jadpo_record hidden
// Generated by Jadpo from the hosts the app can reach; update it with `jadpo records update`, never by hand.
record network 1

connection couriers
    for AcmeCouriers.v1
    host https://api.acme-couriers.example
    host https://sandbox.acme-couriers.example
```

Only `shipping` calls the courier, and `support` asks `shipping` where a
parcel is.

## Related

- [Connecting in `app.jadpo`](../language/11-integrations.md#connecting-in-appjadpo)
- [Calling a provider](../language/11-integrations.md#calling-a-provider)
- [Changing another domain's data](../language/01-project-layout-and-modules.md#changing-another-domains-data)
