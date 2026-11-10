# `package.index_invalid`

A package's index is missing, or holds something an index can't.

## Why Jadpo refuses it

A package's index says what the package is: a `///` description,
`licence:` and `jadpo:`, and `publisher:` when it is published. Installing
a package and reading it both start here, so an index that is missing or
that holds anything else stops the package being read. Packages have no
dependencies, so `requires:` is one of the things an index can't hold.

## What it looks like

An index with a `requires:` line:

```jadpo check project=p71-index file=app.jadpo hidden
application {}
```

`shared/providers/acme_couriers/v1/package.jadpo`:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An index without its `///` description is this error too:

```jadpo check project=p71-index-description file=app.jadpo hidden
application {}
```

`shared/providers/acme_couriers/v1/package.jadpo`:

```jadpo check project=p71-index-description file=shared/providers/acme_couriers/v1/package.jadpo expect=package.index_invalid
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: Package `AcmeCouriers.v1` has no description
 --> shared/providers/acme_couriers/v1/package.jadpo:1:9
  |
1 | package AcmeCouriers.v1 {
  |         ^^^^^^^^^^^^^^^ no `///` description
  reason: A package's index starts with a `///` description of what the package is for: it heads `jadpo package show` and the registry's page.
  next: Add a `///` line above `package` saying what the package does (agent)
  rule: package.index_invalid
  help: https://jadpo.dev/docs/errors/package.index_invalid

check failed: 1 error, 2 files
```

A package with no `package.jadpo`, one that declares no package or a
second one, and one missing `licence:` or `jadpo:` are each this error,
with a message of their own.

## How to fix it

**If the index holds something an index can't**, remove it, and copy what
the package needs into it:

```jadpo check project=p71-index-fix file=shared/providers/acme_couriers/v1/package.jadpo
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-index-fix file=app.jadpo hidden
application {}
```

Installing the package is one folder and one line in the lock, whatever it
uses.

**If the index has no description**, write one on a `///` line above
`package`:

```jadpo check project=p71-index-description-fix file=shared/providers/acme_couriers/v1/package.jadpo
/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-index-description-fix file=app.jadpo hidden
application {}
```

The package now says what it is to anyone reading it or installing it.

## Related

- [The index: `package.jadpo`](../language/11-integrations.md#the-index-packagejadpo)
- [No presets, no dependencies](../language/11-integrations.md#no-presets-no-dependencies)
