# `package.not_locked`

A package sits in `packages/`, but `jadpo.lock` has no line for it, so it was copied in rather than installed.

## Why Jadpo refuses it

`packages/` holds only packages Jadpo installed. Installing one checks
it and records in `jadpo.lock` its version, a digest of its files and
where it came from. A package copied in by hand has none of that, so
nothing says whose code it is or whether it has been changed.

Jadpo won't choose for you whether the code is someone else's package or
your app's own. That decides whether it is installed from its source or
kept and changed as part of your app.

## What it looks like

```jadpo check project=p71-unlocked file=app.jadpo hidden
application {}
```

The app has a package in `packages/postcodes/v2/`, and no `jadpo.lock`.

packages/postcodes/v2/package.jadpo:

```jadpo check project=p71-unlocked file=packages/postcodes/v2/package.jadpo expect=package.not_locked
/// UK postcodes.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```text output
error: Package `Postcodes.v2` isn't in `jadpo.lock`
 --> packages/postcodes/v2/package.jadpo
  reason: `packages/` holds only installed packages, each with its line in `jadpo.lock`: its version, digest and source. A package with no line was copied in rather than installed, so nothing says where it came from.
  next: Install it with `jadpo package add`, or, if it is the app's own code, move it to `shared/providers/postcodes/v2/` (human)
  rule: package.not_locked
  help: https://jadpo.dev/docs/errors/package.not_locked

check failed: 1 error, 2 files
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether `Postcodes` is an installed package or your app's own code.

## How to fix it

The question is where `Postcodes` comes from.

**If it is a package from elsewhere**, install it from its source, such
as the folder it was copied from or its git repository at a release tag:

```sh
jadpo package add ../postcodes
```

Jadpo checks the package, installs it in `packages/postcodes/v2/` in
place of the copy, and adds its line to `jadpo.lock`.

**If it is your app's own code**, move it to
`shared/providers/postcodes/v2/`, where the app's own integrations live.
`shared/providers/postcodes/v2/package.jadpo`:

```jadpo check project=p71-unlocked-own file=shared/providers/postcodes/v2/package.jadpo
/// UK postcodes.
package Postcodes.v2 {
    licence: Licence.mit
    jadpo: "1.0"
}
```

```jadpo check project=p71-unlocked-own file=app.jadpo hidden
application {}
```

`Postcodes.v2` is now part of your app, which may change it, and it needs
no line in the lock. Each `use packages/postcodes` becomes
`use shared/providers/postcodes`.

## Related

- [Where packages live](../language/11-integrations.md#where-packages-live)
- [Installing safely](../language/11-integrations.md#installing-safely)
- [How packages are installed and linked](../language/11-integrations.md#how-packages-are-installed-and-linked)
