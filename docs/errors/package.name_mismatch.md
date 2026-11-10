# `package.name_mismatch`

A package's index gives a different name from its folder.

## Why Jadpo refuses it

A package's folder is its name in `lower_snake_case` and its version, so
its name in code is the folder's in `UpperCamelCase`, and the index gives
the same. If they disagreed, the folder would say one thing and code
another, and a search for the package would miss one of them.

## What it looks like

The folder says `AcmeCouriers.v1` and the index says `AcmeCourier.v1`:

```jadpo check project=p71-name file=app.jadpo hidden
application {}
```

`shared/providers/acme_couriers/v1/package.jadpo`:

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
  help: https://jadpo.dev/docs/errors/package.name_mismatch

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the folder's name is right**, write it in the index:

```jadpo check project=p71-name-fix file=shared/providers/acme_couriers/v1/package.jadpo
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-name-fix file=app.jadpo hidden
application {}
```

The folder and the index say the same name.

**If the index's name is right**, rename the folder after it:

```sh
mv shared/providers/acme_couriers shared/providers/acme_courier
```

```jadpo check project=p71-name-folder file=shared/providers/acme_courier/v1/package.jadpo hidden
/// Acme Couriers' booking API.
package AcmeCourier.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-name-folder file=app.jadpo hidden
application {}
```

Each `use` of the package names the new folder, and code writes
`AcmeCourier`.

## Related

- [Where packages live](../language/11-integrations.md#where-packages-live)
- [The index: `package.jadpo`](../language/11-integrations.md#the-index-packagejadpo)
