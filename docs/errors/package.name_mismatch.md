# `package.name_mismatch`

A package's folder is its name in `lower_snake_case` and its version, so
its index gives the folder's name in `UpperCamelCase`:

```jadpo check project=p71-name file=app.jadpo hidden
application {}
```

```jadpo check project=p71-name file=shared/providers/acme_couriers/v1/package.jadpo expect=package.name_mismatch
/// Acme Couriers' booking API.
package AcmeCourier.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: `AcmeCourier.v1` is in the folder of `AcmeCouriers.v1`
 --> shared/providers/acme_couriers/v1/package.jadpo:2:9
  |
2 | package AcmeCourier.v1 {
  |         ^^^^^^^^^^^^^^ the folder says `AcmeCouriers.v1`
  reason: A package's folder is its name in `lower_snake_case` and its version, `acme_couriers/v1`, so its name in code is `AcmeCouriers.v1`, the folder's in `UpperCamelCase`, and the index gives the same.
  next: Write `package AcmeCouriers.v1` (agent)
  rule: package.name_mismatch
  help: docs/errors/package.name_mismatch.md

check failed: 1 error, 2 files
```
