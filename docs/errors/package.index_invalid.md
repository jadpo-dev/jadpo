# `package.index_invalid`

A package's index is a `///` description and two keys, `licence:` and
`jadpo:`. Packages have no dependencies, so it has no `requires:`:

```jadpo check project=p71-index file=app.jadpo hidden
application {}
```

```jadpo check project=p71-index file=shared/providers/acme_couriers/v1/package.jadpo expect=package.index_invalid
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
    requires: [Postcodes.v2]
}
```

```text output
error: A package's index can't hold `requires:`
 --> shared/providers/acme_couriers/v1/package.jadpo:5:5
  |
5 |     requires: [Postcodes.v2]
  |     ^^^^^^^^^^^^^^^^^^^^^^^^ not an index key
  reason: Packages have no dependencies: a package that needs another's code carries a copy, so installing one is one folder and one line in the lock.
  next: Remove `requires:`, and copy what the package needs into it (agent)
  rule: package.index_invalid
  help: https://jadpo.dev/docs/errors/package.index_invalid

check failed: 1 error, 2 files
```
