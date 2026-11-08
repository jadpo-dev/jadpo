# `install.in_use`

A package `jadpo package remove` was asked to remove is still brought in by the app's code.

## Why Jadpo refuses it

`jadpo package remove` takes a package's folder out of `packages/` and
its line out of `jadpo.lock`. A file of the app's with
`use packages/<name>` would then name a package that isn't there, and
the app would stop compiling. So the command removes nothing while any
file still brings the package in, and names each one.

## What it looks like

An app that installed `Postcodes` from a folder beside it, and uses its
`starter` library in an action:

```jadpo check project=e-install-in-use file=app.jadpo hidden
application {}
```

```sh run project=e-install-in-use expect=install.in_use
(cd .. && jadpo package new Postcodes --library > /dev/null)
cat > ../postcodes/package.jadpo <<'DOC'
/// UK postcodes, read and tidied.
package Postcodes.v1 {
    licence: Licence.mit
    jadpo: "X.Y"
}
DOC
(cd ../postcodes && jadpo package check > /dev/null)
jadpo package add ../postcodes > /dev/null
mkdir -p domains/intake/actions
cat > domains/intake/actions/tidy_name.jadpo <<'DOC'
use packages/postcodes

/// Tidies the name a customer typed.
action tidy_name(entered: Postcodes.EnteredText) fails Postcodes.NothingEntered -> Postcodes.TidyText {
    return attempt Postcodes.starter.tidied(entered)
}
DOC
jadpo check > /dev/null
jadpo package remove postcodes
```

```text output
error: The app still uses `Postcodes.v1`
  reason: `domains/intake/actions/tidy_name.jadpo` brings it in with `use packages/postcodes`, so removing it would leave that file unable to compile.
  next: Take `use packages/postcodes` and what it brings in out of the app's code, then run `jadpo package remove postcodes` again (agent)
  rule: install.in_use
  help: https://jadpo.dev/docs/errors/install.in_use
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

With two major versions installed side by side, such as `Postcodes.v1`
and `Postcodes.v2`, the `use` line brings in both. Removing one is then
refused only while a file writes a name through it, such as
`Postcodes.v1.starter.tidied`, and the next step is to move those names
to the other version.

## How to fix it

**If the app no longer needs the package**, take out the code that uses
it, `use` line and all, then remove it again:

```sh
rm domains/intake/actions/tidy_name.jadpo
jadpo package remove postcodes
```

The package's folder and its line in `jadpo.lock` go, and `jadpo check`
passes without it.

**If the app still uses it**, keep it installed. To move to a newer
release instead, update it:

```sh
jadpo package update postcodes
```

The package stays in `packages/`, at the newest release of its major
version, and the command lists what changed for review.

## Related

- [`jadpo package update` and `remove`](../language/14-tooling.md#jadpo-package-update-and-remove)
- [How packages are installed and linked](../language/11-integrations.md#how-packages-are-installed-and-linked)
- [Two versions side by side](../language/11-integrations.md#two-versions-side-by-side)
