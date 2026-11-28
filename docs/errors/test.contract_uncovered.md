# `test.contract_uncovered`

No case of a contract run sent the provider's sandbox an operation the app calls that was written from the provider's documentation and that no recording confirms.

## Why Jadpo refuses it

An operation written from the provider's documentation,
`source: PackageSource.written`, is only as right as the reading of
those pages. Until a real answer has been checked against it, by a
recording `jadpo package record` keeps, nothing shows the provider
answers it as its types say. Every build allows it, and the contract
run, `jadpo test --kind contract`, is where it is checked: each such
operation the app calls needs a case that sends it to the sandbox.

A test that scripts the operation's answer in its `fakes:` reaches it,
but its call never leaves the machine, so it checks the code around the
call and not the operation. A run where no case sent the operation would
pass while the provider could be answering something else entirely.

## What it looks like

A courier desk tracks parcels through its own integration with Acme
Couriers, written from Acme's documentation, with no recording of
`parcels.get`. Its one test scripts the answer:

config.jadpo:

```jadpo check project=e-contract-uncovered file=config.jadpo
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-contract-uncovered file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-contract-uncovered file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-contract-uncovered file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e-contract-uncovered file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e-contract-uncovered file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

app.jadpo:

```jadpo check project=e-contract-uncovered file=app.jadpo
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

```jadpo check project=e-contract-uncovered file=domains/shipping/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

domains/shipping/actions/track_parcel.test.jadpo:

```jadpo check project=e-contract-uncovered file=domains/shipping/actions/track_parcel.test.jadpo
use shared/providers/acme_couriers

test "a delivered parcel says so" {
    fakes: {
        couriers.parcels.get: [
            AcmeCouriers.Parcel {
                status: "delivered"
            }
        ]
    }
    var lookup: AcmeCouriers.ParcelLookup = AcmeCouriers.ParcelLookup {
        parcel_id: "P1"
    }
    var parcel: AcmeCouriers.Parcel = track_parcel(lookup)
    assert parcel.status == "delivered"
}
```

The run's one case scripts `parcels.get`, so nothing goes to the
sandbox, and the run fails:

```sh run project=e-contract-uncovered expect=test.contract_uncovered
echo 'COURIERS_API_KEY=sandbox_key' > .env.local
jadpo test --kind contract
```

```text output
```

`(agent)` at the end of the `next:` line means your coding agent can
make the fix itself.

## How to fix it

**If you can call Acme's sandbox now**, record the operation, which
confirms it:

```sh
jadpo package record couriers parcels.get --input '{"parcel_id": "P1"}'
```

The answer is checked against `Parcel` and kept as
`shared/providers/acme_couriers/v1/recordings/parcels.get.json`, and the
next contract run needs no case for `parcels.get`.

**If the run should check it each time**, add a test that reaches
`parcels.get` with no `fakes:` answer for it:

```jadpo fragment
test "a parcel's status comes back" {
    var lookup: AcmeCouriers.ParcelLookup = AcmeCouriers.ParcelLookup {
        parcel_id: "P1"
    }
    var parcel: AcmeCouriers.Parcel = track_parcel(lookup)
    assert parcel.status != ""
}
```

```jadpo check project=e-contract-uncovered-fixed file=config.jadpo hidden
config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
```

```jadpo check project=e-contract-uncovered-fixed file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-contract-uncovered-fixed file=shared/providers/acme_couriers/v1/provider.jadpo hidden
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

```jadpo check project=e-contract-uncovered-fixed file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e-contract-uncovered-fixed file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e-contract-uncovered-fixed file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

```jadpo check project=e-contract-uncovered-fixed file=app.jadpo hidden
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

```jadpo check project=e-contract-uncovered-fixed file=domains/shipping/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

```jadpo check project=e-contract-uncovered-fixed file=domains/shipping/actions/track_parcel.test.jadpo hidden
use shared/providers/acme_couriers

test "a delivered parcel says so" {
    fakes: {
        couriers.parcels.get: [
            AcmeCouriers.Parcel {
                status: "delivered"
            }
        ]
    }
    var lookup: AcmeCouriers.ParcelLookup = AcmeCouriers.ParcelLookup {
        parcel_id: "P1"
    }
    var parcel: AcmeCouriers.Parcel = track_parcel(lookup)
    assert parcel.status == "delivered"
}

test "a parcel's status comes back" {
    var lookup: AcmeCouriers.ParcelLookup = AcmeCouriers.ParcelLookup {
        parcel_id: "P1"
    }
    var parcel: AcmeCouriers.Parcel = track_parcel(lookup)
    assert parcel.status != ""
}
```

In `jadpo test` its call is answered by the package's fake; in the
contract run it goes to Acme's sandbox, and the run passes once the
answer decodes as a `Parcel`.

## Related

- [Tests and fakes](../language/11-integrations.md#tests-and-fakes)
- [Provenance and confirmation](../language/11-integrations.md#provenance-and-confirmation)
- [`jadpo package record`](../language/14-tooling.md#jadpo-package-record)
- [Fakes for integrations](../language/13-testing.md#fakes-for-integrations)
