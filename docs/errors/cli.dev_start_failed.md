# `cli.dev_start_failed`

The app couldn't start under `jadpo dev`, usually because another program is using its port, it has no database to run on, or a connection would call a provider's live API.

## Why Jadpo refuses it

`jadpo dev` checks the project, starts what the app runs on, a database
for each store and the fakes or sandboxes for its outside calls, and
then listens on a port of this machine, 3000 unless `--port` says
otherwise. When one of those can't be had, nothing is served, and the
error says which it was.

Jadpo won't pick another port for you, since the address you open and
your scripts call would then change without you knowing, and it won't
stop the program that holds the port. It won't let a call through to a
provider's live API either, where a request costs money or changes real
data; whether the calls go to the sandbox or stay faked is yours to
decide.

## What it looks like

A project whose `couriers` connection lets `parcels.get` through to the
provider's sandbox while it runs under `jadpo dev`:

```jadpo check project=e-dev-start-failed file=config.jadpo hidden
config {
    couriers_environment: ProviderEnvironment
}
```

```jadpo check project=e-dev-start-failed file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-dev-start-failed file=shared/providers/acme_couriers/v1/provider.jadpo hidden
/// Acme Couriers' tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
}
```

```jadpo check project=e-dev-start-failed file=shared/providers/acme_couriers/v1/resources/parcels.jadpo hidden
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

```jadpo check project=e-dev-start-failed file=shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo hidden
/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: Text
}
```

```jadpo check project=e-dev-start-failed file=shared/providers/acme_couriers/v1/types/parcel.jadpo hidden
/// A parcel and where it is.
public type Parcel = Object {
    status: Text
}
```

app.jadpo:

```jadpo check project=e-dev-start-failed file=app.jadpo
application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            environment: config.couriers_environment
            development: {
                sandbox: [parcels.get]
            }
        }
    }
}
```

```jadpo check project=e-dev-start-failed file=domains/shipping/actions/track_parcel.jadpo hidden
use app { couriers }
use shared/providers/acme_couriers

/// Where a customer's parcel is.
action track_parcel(lookup: AcmeCouriers.ParcelLookup) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(lookup)
}
```

With `COURIERS_ENVIRONMENT` set to `live`, the connection would send
`parcels.get` to Acme's live API, so `jadpo dev` doesn't start:

```sh run project=e-dev-start-failed expect=cli.dev_start_failed
echo 'COURIERS_ENVIRONMENT=live' > .env.local
jadpo dev
```

```text output
jadpo dev
error: `couriers` lets calls through to its provider, and its environment is live
  reason: `jadpo dev` lets the operations `development: { sandbox: }` and `--sandbox` name through to the provider's sandbox, never its live API, and `COURIERS_ENVIRONMENT` chooses `live`.
  next: Set `COURIERS_ENVIRONMENT` to `sandbox` in `.env.local`, or run `jadpo dev --fake couriers` (human)
  rule: cli.dev_start_failed
  help: https://jadpo.dev/docs/errors/cli.dev_start_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the app's calls reach Acme's sandbox or stay on this
machine.

The port is taken when another program, often another `jadpo dev`, is
already listening on it. That needs a second program running beside the
command, so this output is copied rather than checked. It is `jadpo dev`
on a Mac while another `jadpo dev` of the same project serves on port
3000:

```sh
jadpo dev
```

```text
jadpo dev, started 23:31:56
error: Port 3000 is already in use
  reason: Another program is listening on port 3000, such as another `jadpo dev`, and only one program can listen on a port.
  next: Stop what is using port 3000, or choose another with `--port` (human)
  rule: cli.dev_start_failed
  help: https://jadpo.dev/docs/errors/cli.dev_start_failed
```

Here the choice decides what runs on your machine, and where you reach
the app.

The same error says so when there is no PostgreSQL to give a store a
database, or a store's address can't be reached.

## How to fix it

The question is what the headline says couldn't be had.

**If another program is using the port**, stop it, or run on another
port:

```sh
jadpo dev --port 3100
```

The app is then served at `http://localhost:3100`.

**If a connection's environment is live**, choose the sandbox in
`.env.local`:

```sh
echo 'COURIERS_ENVIRONMENT=sandbox' > .env.local
jadpo dev
```

`parcels.get` then goes to Acme's sandbox. To keep every call on this
machine instead, run `jadpo dev --fake couriers`, which answers each
call from the connection's fake.

**If there is no database to run on**, in a project with a store,
install PostgreSQL, so `jadpo dev` can start one of its own, give the store's address with `jadpo
config set`, or name a database that is running:

```sh
jadpo dev --url postgresql://localhost:5432/couriers_dev
```

`jadpo dev` then applies the project's migrations to that database and
runs the app on it.

## Related

- [`jadpo dev`](../language/14-tooling.md#jadpo-dev)
- [Provider calls in development](../language/11-integrations.md#provider-calls-in-development)
