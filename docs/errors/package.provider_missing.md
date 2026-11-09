# `package.provider_missing`

A package has resources but no `provider.jadpo`.

## Why Jadpo refuses it

A package with `resources/` describes an API, so it has to say how that API
is reached: its protocol, hosts and credential. Without `provider.jadpo`
none of its operations could be called. A package without resources is a
library, which needs none.

## What it looks like

The package has `resources/` and no `provider.jadpo`:

```jadpo check project=p71-missing file=app.jadpo hidden
application {}
```

```jadpo check project=p71-missing file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

`shared/providers/acme_couriers/v1/resources/bookings.jadpo`:

```jadpo check project=p71-missing file=shared/providers/acme_couriers/v1/resources/bookings.jadpo expect=package.provider_missing
public resource bookings {}
```

```text output
error: Package `AcmeCouriers.v1` has resources but no `provider.jadpo`
 --> shared/providers/acme_couriers/v1/resources/bookings.jadpo
  reason: A package with `resources/` describes an API, so it says how the API is reached in `provider.jadpo`: its protocol, hosts and credential. A package without resources is a library.
  next: Add `shared/providers/acme_couriers/v1/provider.jadpo` with `provider { protocol: Protocol.json_http ... }`, or move the resources out if this is a library (agent)
  rule: package.provider_missing
  help: https://jadpo.dev/docs/errors/package.provider_missing

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the package describes an API**, add `provider.jadpo`:

```jadpo check project=p71-missing-fix file=shared/providers/acme_couriers/v1/provider.jadpo
/// Acme Couriers' booking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
    }
}
```

```jadpo check project=p71-missing-fix file=app.jadpo hidden
application {}
```

```jadpo check project=p71-missing-fix file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-missing-fix file=shared/providers/acme_couriers/v1/resources/bookings.jadpo hidden
public resource bookings {}
```

The package now says how its API is reached.

**If the package is a library**, take the resources out:

```sh
rm -r shared/providers/acme_couriers/v1/resources
```

```jadpo check project=p71-missing-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=p71-missing-fix-2 file=shared/providers/acme_couriers/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

With no `resources/`, it is a library package, which needs no provider.

## Related

- [How the API is reached: `provider.jadpo`](../language/11-integrations.md#how-the-api-is-reached-providerjadpo)
- [Kinds of package](../language/11-integrations.md#kinds-of-package)
