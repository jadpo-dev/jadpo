# `test.contract_refused`

A contract run sent nothing to the provider's sandbox, because a connection it goes through has nothing set, or nothing valid, for its credential or address, or its environment chooses `live`.

## Why Jadpo refuses it

`jadpo test --kind contract` runs the tests that reach a provider
against its sandbox: each call a test's `fakes:` doesn't script goes
through the connection, with the credential, environment and address
`jadpo dev` reads, from the environment and then `.env.local`. It is
the run that shows the provider still answers as the package's types
say, so it has to reach the real sandbox with a real key.

A connection whose environment chooses `live` would send the tests'
calls to the provider's live API, where a call can cost money or change
real data, and a connection with nothing set would send a stand-in key,
whose refusal proves nothing. So the run checks every connection it
goes through before it sends anything. Jadpo won't choose the
environment or set a key for you: which of your accounts at the
provider the calls reach is yours to decide.

## What it looks like

A courier desk tracks parcels through its own integration with Acme
Couriers, and one test reaches it:

config.jadpo:

```jadpo check project=e-contract-refused file=config.jadpo
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-contract-refused file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-contract-refused file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
}
```

```jadpo check project=e-contract-refused file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e-contract-refused file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e-contract-refused file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

app.jadpo:

```jadpo check project=e-contract-refused file=app.jadpo
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

```jadpo check project=e-contract-refused file=domains/shipping/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

domains/shipping/actions/track_parcel.test.jadpo:

```jadpo check project=e-contract-refused file=domains/shipping/actions/track_parcel.test.jadpo
use shared/providers/acme_couriers

test "a parcel's status comes back" {
    var lookup: AcmeCouriers.ParcelLookup = AcmeCouriers.ParcelLookup {
        parcel_id: "P1"
    }
    var parcel: AcmeCouriers.Parcel = track_parcel(lookup)
    assert parcel.status != ""
}
```

Nothing on this machine sets `COURIERS_API_KEY`, so the run is refused
before anything is sent:

```sh run project=e-contract-refused expect=test.contract_refused
jadpo test --kind contract
```

```text output
error: `couriers` has nothing set for the provider's sandbox
  reason: A contract run sends its cases' calls through `couriers` to the provider's sandbox, with the settings `jadpo dev` reads, and nothing sets `COURIERS_API_KEY`, so only a stand-in would be sent.
  next: Set `COURIERS_API_KEY` in `.env.local`, or in the job's environment, for the sandbox, then run it again (human)
  rule: test.contract_refused
  help: https://jadpo.dev/docs/errors/test.contract_refused

tests not run: 1 error, 9 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which of your accounts at Acme the tests' calls are made with.

A connection whose environment chooses `live` is refused too, whatever
else is set:

```sh run project=e-contract-refused expect=test.contract_refused
printf 'COURIERS_API_KEY=sandbox_key\nCOURIERS_ENVIRONMENT=live\n' > .env.local
jadpo test --kind contract
```

```text output
error: `couriers`'s environment is live
  reason: A contract run sends its cases' calls to the provider's sandbox, never its live API, and `COURIERS_ENVIRONMENT` chooses `live`.
  next: Set `COURIERS_ENVIRONMENT` to `sandbox` for the run, then run it again (human)
  rule: test.contract_refused
  help: https://jadpo.dev/docs/errors/test.contract_refused

tests not run: 1 error, 9 files
```

## How to fix it

**If the run should reach Acme's sandbox**, set the sandbox's key, and
choose the sandbox, where you run it:

```sh
jadpo config set couriers_api_key
jadpo test --kind contract
```

`jadpo config set` asks for the key without showing what you type and
saves it in `.env.local`, which is never committed. In a scheduled job,
give `COURIERS_API_KEY` from the job's secrets and
`COURIERS_ENVIRONMENT=sandbox`. The test's call then goes to
`https://sandbox.acme-couriers.example`.

**If you meant to run the tests without reaching Acme**, leave out
`--kind contract`:

```sh
jadpo test
```

Every provider call is then answered by its package's fake, and nothing
leaves your machine.

## Related

- [Tests and fakes](../language/11-integrations.md#tests-and-fakes)
- [Provider calls in development](../language/11-integrations.md#provider-calls-in-development)
- [`jadpo test`](../language/14-tooling.md#jadpo-test)
- [`jadpo config set` and `jadpo config check`](../language/14-tooling.md#jadpo-config-set-and-jadpo-config-check)
