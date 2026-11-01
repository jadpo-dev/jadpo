# `recording.failed`

A recording by `jadpo package record` or `jadpo ai record` got no answer worth keeping from the sandbox or the model, or its call couldn't be made.

## Why Jadpo refuses it

`jadpo package record` calls one operation of a provider's sandbox once
and keeps the answer as that operation's recording, which tests and
`jadpo dev` answer from; `jadpo ai record` does the same for a prompt's
model. A recording is only worth keeping if it is an answer the
operation's types read. When the call can't be made, gets no answer, or
gets one that doesn't decode, nothing is recorded, so no test ever
answers from it.

## What it looks like

A project whose `couriers` connection reaches Acme Couriers' live or
sandbox API, as `COURIERS_ENVIRONMENT` chooses, with the sandbox on
this machine at port 1:

```jadpo check project=e-recording-failed file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-recording-failed file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-recording-failed file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "http://127.0.0.1:1"
    }
}
```

```jadpo check project=e-recording-failed file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e-recording-failed file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e-recording-failed file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

app.jadpo:

```jadpo check project=e-recording-failed file=app.jadpo
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

```jadpo check project=e-recording-failed file=domains/shipping/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

Nothing is listening there, so the call gets no answer:

```sh run project=e-recording-failed expect=recording.failed
echo 'COURIERS_ENVIRONMENT=sandbox' > .env.local
jadpo package record couriers parcels.get --input '{"parcel_id": "P1"}'
```

```text output
error: The sandbox didn't answer `couriers.parcels.get`
  reason: It couldn't be reached, or it said it did nothing and asked to be called later.
  next: Check the sandbox is up, then run it again (human)
  rule: recording.failed
  help: https://jadpo.dev/docs/errors/recording.failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which of Acme's services the call reaches.

An answer the operation's types don't read is refused the same way,
with the field that didn't decode and the type to correct, which your
agent can do. `jadpo ai record` says the same of a model that gave no
answer, or a fixture that failed before anything was asked.

## How to fix it

The question is whether the sandbox's address is right.

**If it is**, start the sandbox, then record the operation again:

```sh
jadpo package record couriers parcels.get --input '{"parcel_id": "P1"}'
```

An answer that decodes as a `Parcel` is kept as
`shared/providers/acme_couriers/v1/recordings/parcels.get.json`.

**If it isn't**, correct the `sandbox:` host in the package's
`provider.jadpo` to the provider's sandbox:

```jadpo fragment
provider {
    ...
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-recording-failed-host file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-recording-failed-host file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-recording-failed-host file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-recording-failed-host file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e-recording-failed-host file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e-recording-failed-host file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

```jadpo check project=e-recording-failed-host file=app.jadpo hidden
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

```jadpo check project=e-recording-failed-host file=domains/shipping/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

The next recording calls `https://sandbox.acme-couriers.example`.

## Related

- [`jadpo package record`](../language/14-tooling.md#jadpo-package-record)
- [Provenance and confirmation](../language/11-integrations.md#provenance-and-confirmation)
- [`jadpo ai show` and `jadpo ai record`](../language/14-tooling.md#jadpo-ai-show-and-jadpo-ai-record)
