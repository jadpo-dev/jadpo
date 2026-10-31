# `publish.breaking_change`

A package's new version changes or removes something its last release offered, within the same major version.

## Why Jadpo refuses it

Apps install a package and rely on what its surface offers: its public
types, failures, functions and operations. A release within one major
version, such as 2.0.0 to 2.1.0, promises those apps that everything
they used is still there and still the same. A removed entry, a field
retyped, a new failure or a changed signature breaks that promise, and
an app that upgrades would stop compiling or behave differently.

So `jadpo package check` works out the version from the surface at the
last `v<version>` tag and refuses a breaking change. A breaking change
is a new major version, which is a new package beside this one, so apps
move to it when they choose. Jadpo won't choose for you between undoing
the change and starting a new major version.

## What it looks like

`Postcodes.v2` was released as 2.0.0. Since then, `WrittenPostcode` was
replaced by `EnteredPostcode`, and `uk.parse` takes the new type. In the
package's repository:

```sh
jadpo package check
```

```text
error: `Postcodes.v2` changed in ways that break the apps that use it
  reason: Since 2.0.0, `function uk.parse` changed and `type WrittenPostcode` was removed, which an app using 2.0.0 may rely on. A breaking change is a new major version: a new package beside this one.
  next: Put back what changed, or publish the change as `Postcodes.v3` in a folder of its own (human)
  rule: publish.breaking_change
  help: https://jadpo.dev/docs/errors/publish.breaking_change
```

The publishing bar follows, with its `version` row failed:
`2 breaking changes since 2.0.0`. The surface isn't written for a
version the change can't have.

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether the apps that use the package keep working when they
upgrade, or move to a new package when they choose.

## How to fix it

The question is whether apps on 2.x should get this change.

**If they shouldn't have to change anything**, put back what changed:
the `WrittenPostcode` type, and `uk.parse` taking it. A new type beside
it, such as `EnteredPostcode`, is a new entry, which only adds. Then run
the bar again:

```sh
jadpo package check
```

The version row passes with the next minor version, 2.1.0, and the
surface is written for it.

**If the change is worth breaking for**, publish it as a new package,
`Postcodes.v3`, in a folder of its own: its `package.jadpo` names
`Postcodes.v3`, and `Postcodes.v2` stays as it was released. Its first
release is 3.0.0, the major version its label names. Run the bar there:

```sh
jadpo package check
```

Apps keep using `Postcodes.v2` until they install `Postcodes.v3`, and
can hold both side by side while their callers move across.

## Related

- [It decides the version](../language/11-integrations.md#it-decides-the-version)
- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [How the surface and the publishing bar work](../language/11-integrations.md#how-the-surface-and-the-publishing-bar-work)
- [Two versions side by side](../language/11-integrations.md#two-versions-side-by-side)
- [`jadpo package check`](../language/14-tooling.md#jadpo-package-check)
