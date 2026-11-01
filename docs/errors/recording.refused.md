# `recording.refused`

A recording was refused before anything was sent, because its connection would call a live API or has nothing set for its sandbox, its package is installed, or there is nothing to ask its prompt with.

## Why Jadpo refuses it

`jadpo package record` calls one operation of a provider once and keeps
the answer as that operation's recording, which tests and `jadpo dev`
answer from. It calls the provider's sandbox, never its live API, where
a call can cost money or change real data, and never with a stand-in
for a credential, whose answer would be a refusal worth nothing as a
recording. `jadpo ai record` holds a prompt's model to the same rules.

So the connection has to choose the sandbox, with everything it reads
set, before the call is made. Jadpo won't choose the environment or set
a credential for you: which account and which of the provider's
services the call reaches is yours to decide.

## What it looks like

A project whose `couriers` connection reaches Acme Couriers' live or
sandbox API, as `COURIERS_ENVIRONMENT` chooses:

```jadpo check project=e-recording-refused file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-recording-refused file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-recording-refused file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-recording-refused file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e-recording-refused file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e-recording-refused file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

app.jadpo:

```jadpo check project=e-recording-refused file=app.jadpo
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

```jadpo check project=e-recording-refused file=domains/shipping/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

`.env.local` chooses the live API, so recording `parcels.get` is refused:

```sh run project=e-recording-refused expect=recording.refused
echo 'COURIERS_ENVIRONMENT=live' > .env.local
jadpo package record couriers parcels.get --input '{"parcel_id": "P1"}'
```

```text output
error: `couriers` calls its provider's live API
  reason: `jadpo package record` calls a provider's sandbox, never its live API, and `COURIERS_ENVIRONMENT` chooses `live`.
  next: Set `COURIERS_ENVIRONMENT` to `sandbox` in `.env.local`, then run it again (human)
  rule: recording.refused
  help: https://jadpo.dev/docs/errors/recording.refused
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which of Acme's APIs the call reaches.

With nothing setting `COURIERS_ENVIRONMENT`, the connection would call a
sandbox it has no setting for:

```sh run project=e-recording-refused expect=recording.refused
jadpo package record couriers parcels.get --input '{"parcel_id": "P1"}'
```

```text output
error: `couriers` has nothing set for its sandbox
  reason: `jadpo package record` calls the provider's sandbox through `couriers`, which reads `COURIERS_ENVIRONMENT`, and nothing sets it.
  next: Set `COURIERS_ENVIRONMENT` to `sandbox` in `.env.local`, then run it again (human)
  rule: recording.refused
  help: https://jadpo.dev/docs/errors/recording.refused
```

The other refusals, an installed package whose files its digest holds,
or a prompt that no call asks or that `jadpo dev` hasn't asked yet, say
what to do in their `next:` line.

## How to fix it

**If the connection should call the sandbox**, choose it in
`.env.local`, then record the operation again:

```sh
echo 'COURIERS_ENVIRONMENT=sandbox' > .env.local
jadpo package record couriers parcels.get --input '{"parcel_id": "P1"}'
```

The call goes to `https://sandbox.acme-couriers.example`, and an answer
that decodes as a `Parcel` is kept as
`shared/providers/acme_couriers/v1/recordings/parcels.get.json`.

## Related

- [`jadpo package record`](../language/14-tooling.md#jadpo-package-record)
- [Provenance and confirmation](../language/11-integrations.md#provenance-and-confirmation)
- [`jadpo ai show` and `jadpo ai record`](../language/14-tooling.md#jadpo-ai-show-and-jadpo-ai-record)
