# `package.provider_missing`

A package with `resources/` describes an API, so it says how the API is
reached:

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

```jadpo check project=p71-missing file=shared/providers/acme_couriers/v1/resources/bookings.jadpo expect=package.provider_missing
public resource bookings {}
```

```text output
error: Package `AcmeCouriers.v1` has resources but no `provider.jadpo`
 --> shared/providers/acme_couriers/v1/resources/bookings.jadpo
  reason: A package with `resources/` describes an API, so it says how the API is reached in `provider.jadpo`: its protocol, hosts and credential. A package without resources is a library.
  next: Add `shared/providers/acme_couriers/v1/provider.jadpo` with `provider { protocol: Protocol.json_http ... }`, or move the resources out if this is a library (agent)
  rule: package.provider_missing
  help: docs/errors/package.provider_missing.md

check failed: 1 error, 3 files
```
