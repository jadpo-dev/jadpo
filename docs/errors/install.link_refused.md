# `install.link_refused`

A package is linked to a checkout in a CI run or a production build.

## Why Jadpo refuses it

`jadpo package link` compiles the app against a package's folder on one
machine, so you can try a change before releasing it. That folder isn't in
the repository, so CI would be testing something nobody else can reproduce,
and a build would ship code no release vouches for. `jadpo check` and
`jadpo test` refuse a link when the `CI` variable is set, and `jadpo build`
always does.

## What it looks like

An app with `Postcodes` linked to a checkout beside it, checked in CI:

```jadpo check project=e-install-link-refused file=app.jadpo hidden
application {}
```

```sh run project=e-install-link-refused expect=install.link_refused
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
jadpo package link ../postcodes > /dev/null
CI=true jadpo check
```

```text output
error: `Postcodes.v1` is linked to `../postcodes`
 --> jadpo.lock:4:1
  |
4 | package Postcodes.v1 link ../postcodes
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ a live checkout
  reason: A link compiles the app against a checkout on one machine, so CI (the `CI` variable is set) and production builds refuse it, and a link never ships.
  next: Run `jadpo package unlink postcodes`, then commit `jadpo.lock` (agent)
  rule: install.link_refused
  help: https://jadpo.dev/docs/errors/install.link_refused

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you have finished trying the change**, undo the link, and commit the
lock it restores:

```sh
jadpo package unlink postcodes
git add jadpo.lock
git commit -m "chore: use the installed Postcodes"
```

The app compiles against the installed release again, and CI's check
passes. Release the package and update it when the change should ship.

## Related

- [Trying it in an app before publishing](../language/11-integrations.md#trying-it-in-an-app-before-publishing)
- [How packages are installed and linked](../language/11-integrations.md#how-packages-are-installed-and-linked)
