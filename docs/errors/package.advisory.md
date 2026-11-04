# `package.advisory`

An advisory is against a declaration of an installed package that the app reaches.

## Why Jadpo refuses it

An advisory names declarations and versions, such as `Postcodes.uk.parse`,
2.0.0 to 2.1.2, rather than a whole package, so `jadpo package audit` can
say whether your app's own code gets to the declaration, directly or
through what the package calls itself. Code reached only from a test
doesn't count. One the app reaches fails the command, so CI stops on it;
one it doesn't reach is listed and doesn't. An app with errors has no whole
picture of what it reaches, so every advisory counts until they are fixed.

`audit` is the one check that goes online: `jadpo check` and `jadpo build`
never do.

## What it looks like

The app calls `Postcodes.uk.parse`, and the installed 2.0.0 has an advisory
against it:

```sh
jadpo package audit
```

```text
error: `Postcodes.uk.parse` has an advisory: ADV-7
  reason: Reads the wrong outward code. It is against 2.0.0 to 2.1.2, and the app has 2.0.0 and reaches it.
  next: Run `jadpo package update postcodes`, or give `--to` a release that fixes it (human)
  rule: package.advisory
  help: https://jadpo.dev/docs/errors/package.advisory
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: it decides whose code your app runs.

## How to fix it

**If a release fixes it**, take it. One marked urgent is taken at once, and
any other waits its three days unless you name it:

```sh
jadpo package update postcodes
jadpo package update postcodes --to 2.1.3
```

The next `jadpo package audit` no longer lists it, since the advisory is
against the versions before the fix.

**If no release fixes it yet**, copy the package into your own integrations,
in the same format, and fix it there:

```sh
cp -R packages/postcodes shared/providers/postcodes
jadpo package remove postcodes
```

It is then the app's own code, and yours to change.

## Related

- [Yanks, takedowns and advisories](../language/11-integrations.md#yanks-takedowns-and-advisories)
- [Installing and updating from the registry](../language/11-integrations.md#installing-and-updating-from-the-registry)
