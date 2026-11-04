# `publish.publisher_missing`

A package's index names no publisher, and it has to for the registry.

## Why Jadpo refuses it

A release goes under the publisher its `package.jadpo` names, and only their
releasers can confirm it. That is what stops a fork being published as the
original. The registry reads the publisher from the release itself, so a
release that names none has nobody it could belong to.

A package only used by path or from git may leave the key out, so `jadpo
package check` says `left out` and passes. `jadpo package verify` and `jadpo
package publish` are what the registry reads, and they refuse it.

## What it looks like

```sh
jadpo package verify .
```

```text
error: `Postcodes.v2` names no publisher
  reason: The registry publishes it under the publisher `package.jadpo` names, and only their releasers can confirm it, so a fork can't be published as the original.
  next: Add `publisher: "@<your handle>"` to `package.jadpo`, run `jadpo package check` and tag a new release (human)
  rule: publish.publisher_missing
  help: https://jadpo.dev/docs/errors/publish.publisher_missing
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: which handle is yours is something only you know.

## How to fix it

**If the package is published**, name your handle in the index:

```jadpo fragment
/// UK postcodes: parsing, normalising and checking the outward code.
package Postcodes.v2 {
    publisher: "@acme"
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-publisher-missing-fix file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: parsing, normalising and checking the outward code.
package Postcodes.v2 {
    publisher: "@acme"
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-publisher-missing-fix file=app.jadpo hidden
application {}
```

Run `jadpo package check` to write it into the surface and tag a new release:
the release then goes under that publisher, and only their releasers can
confirm it.

**If it is only used by path or from git**, don't publish it:

```sh
jadpo package check
```

`check` leaves the key out and passes, and apps install the package from its
repository or folder as they do now.

## Related

- [Names and publishers](../language/11-integrations.md#names-and-publishers)
- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
