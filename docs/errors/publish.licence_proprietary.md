# `publish.licence_proprietary`

A package's licence is `Licence.proprietary`, which the public registry refuses.

## Why Jadpo refuses it

`Licence.proprietary` says only the package's owner may use it, and the
registry carries packages others may use. A package that says one thing and is
published as the other leaves everyone who installs it unsure whether they may.

A private package may be proprietary, so `jadpo package check` passes it, and
its row says the registry refuses it. `jadpo package verify` and `jadpo package
publish` are what the registry reads, and they refuse it. `jadpo package new`
starts every package proprietary, so this is where you choose.

## What it looks like

```sh
jadpo package verify .
```

```text
error: `Postcodes.v2` is proprietary, and the public registry carries only open licences
  reason: `package.jadpo` says `Licence.proprietary`, so only its owner may use it, and the registry carries packages others may use. A private package stays in git, where `jadpo package check` is all it needs.
  next: Choose an open licence in `package.jadpo`, such as `Licence.mit`, run `jadpo package check` and tag a new release, or keep it private and install it from git (human)
  rule: publish.licence_proprietary
  help: https://jadpo.dev/docs/errors/publish.licence_proprietary
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: a licence gives other people rights to your code, and only you can give them.

## How to fix it

**If the package is published**, choose one of the open licences in the index:

```jadpo fragment
/// UK postcodes: parsing, normalising and checking the outward code.
package Postcodes.v2 {
    publisher: "@acme"
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-licence-proprietary-fix file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: parsing, normalising and checking the outward code.
package Postcodes.v2 {
    publisher: "@acme"
    licence: Licence.mit
    jadpo: "X.Y"
}
```

```jadpo check project=e-licence-proprietary-fix file=app.jadpo hidden
application {}
```

Run `jadpo package check` to write it into the surface and tag the release. A
release already out under `Licence.proprietary` has a new licence on its
surface, which is a new major version.

**If it stays private**, don't publish it:

```sh
jadpo package check
```

`check` passes it, and apps install the package from its git repository.

## Related

- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [The index: `package.jadpo`](../language/11-integrations.md#the-index-packagejadpo)
- [`jadpo package new`](../language/14-tooling.md#jadpo-package-new)
