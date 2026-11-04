# `install.not_in_registry`

The registry has no package of the name you gave, or no such release of it.

## Why Jadpo refuses it

`jadpo package add @acme/postcodes` installs that exact name or stops. A
name decides whose code your app runs, and a name one letter away from a
popular one is how a stranger's package gets installed, so Jadpo never
picks one that looks like it. A name with no publisher means Jadpo's own
package, `@jadpo/<name>`, and never someone else's of that name.

## What it looks like

An app asks for a package with a letter missing:

```sh
jadpo package add @acme/postcode
```

```text
error: The registry has no package `@acme/postcode`
  reason: `jadpo package add` installs that exact name or stops: it never picks one that looks like it, since the name decides whose code your app runs.
  next: Check the name, or look for it with `jadpo package search` (human)
  rule: install.not_in_registry
  help: https://jadpo.dev/docs/errors/install.not_in_registry
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: it decides whose code your app runs.

With a release the package doesn't have, the error says `` `@acme/postcodes`
has no release 9.0.0 `` and its reason lists the releases it has.

## How to fix it

**If the name was mistyped**, look for the package and give its whole name:

```sh
jadpo package search "uk postcodes"
jadpo package add @acme/postcodes
```

The package is installed under that name, and the lock records it, so every
machine then installs the same one.

**If the release was mistyped**, give one the error lists:

```sh
jadpo package add @acme/postcodes --to 2.1.0
```

That release is installed at once, whatever its age.

## Related

- [Installing and updating from the registry](../language/11-integrations.md#installing-and-updating-from-the-registry)
- [Names and publishers](../language/11-integrations.md#names-and-publishers)
- [`jadpo package add`](../language/14-tooling.md#jadpo-package-add)
