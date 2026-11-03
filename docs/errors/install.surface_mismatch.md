# `install.surface_mismatch`

The package you are installing has code that offers something different from its committed `surface.jadpo_record`.

## Why Jadpo refuses it

A package's surface, `surface.jadpo_record`, lists everything it offers
an app: its public types, failures, functions and operations. It is
what reviewers read when the package is installed or upgraded, as a
diff in the pull request. `jadpo package check` writes it from the code.

A surface that doesn't match the code would let the code offer, or
change, something no reviewer saw. So `jadpo package add` and `update`
compile the package, compare the surface its code gives with the
committed one byte for byte, and install nothing when they differ.

Jadpo won't rewrite the package's surface for you, because the surface
belongs to the package's own repository and its release.

## What it looks like

A `Postcodes` package gained a public type, `InwardCode`, after its
surface was written. An app installs it from its folder:

```sh
jadpo package add ../postcodes
```

```text
error: `Postcodes.v2`'s surface.jadpo_record doesn't match its code
  reason: Its code has `type InwardCode`, which its committed surface doesn't. A package is installed only when its surface, what reviewers read, is what its code offers.
  next: Run `jadpo package check` in the package's repository, commit its surface.jadpo_record, and release it again (human)
  rule: install.surface_mismatch
  help: https://jadpo.dev/docs/errors/install.surface_mismatch
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what the package offers the apps that install it.

## How to fix it

**If the new code belongs in the package**, write its surface again in
the package's own repository:

```sh
jadpo package check
```

```text
Facts
  kind: library, pure
  storable: InwardCode, UkPostcode, WrittenPostcode
  failures: uk.parse: InvalidPostcode

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        1 example, every failure shown
  tests         passed                        no tests
  version       passed                        2.0.0, the first release
  changelog     passed                        2.0.0 says why
  licence       passed                        mit, from Jadpo 1.0

Wrote surface.jadpo_record

package check passed: Postcodes.v2 2.0.0, 7 passed
```

Commit the surface with the code, and release it again if the app
installs it from git. Then the install goes through:

```sh
jadpo package add ../postcodes
```

```text
Added Postcodes.v2 2.0.0
    folder   packages/postcodes/v2
    source   path ../postcodes
    surface  matches its code

package added: Postcodes.v2 2.0.0, 1 function, 3 types, 1 failure

next: jadpo package show postcodes  shows what it offers
```

When a package from git is the one refused, the surface has to match at
the tag you install, so the fix is a new release with the surface
committed.

## Related

- [The surface](../language/11-integrations.md#the-surface)
- [Installing safely](../language/11-integrations.md#installing-safely)
- [How packages are installed and linked](../language/11-integrations.md#how-packages-are-installed-and-linked)
- [`jadpo package add`](../language/14-tooling.md#jadpo-package-add)
- [`jadpo package check`](../language/14-tooling.md#jadpo-package-check)
