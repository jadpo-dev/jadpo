# `install.fetch_failed`

Installing a package couldn't read its git repository, or found no release there at the tag you gave.

## Why Jadpo refuses it

A package from git is installed at a release: a `v<version>` tag in its
repository, whose commit the lock records so every machine installs the
same code. Jadpo reads the repository with git, using your own access
and never prompting. When git can't read it, or the tag isn't there,
there is nothing to verify and install, so Jadpo installs nothing and
leaves `jadpo.lock` as it was.

Jadpo won't pick another address or another release for you, because
that choice decides whose code your app runs.

## What it looks like

An app installs `Postcodes` from a repository at an address that doesn't
exist:

```sh
jadpo package add /srv/postcodes@v2.1.0
```

```text
error: `/srv/postcodes` couldn't be read
  reason: Git said: repository '/srv/postcodes' does not exist.
  next: Check the repository's address and your git access to it, then run the command again (human)
  rule: install.fetch_failed
  help: https://jadpo.dev/docs/errors/install.fetch_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which repository, and which release of it, your app installs.

When the repository is read but has no tag of that name, the error says
`` `<repository>` has no tag `v2.1.0` ``, its reason lists the release
tags the repository has, and the next step is to give one of them, or to
tag the release in the package's repository after `jadpo package check`
passes.

## How to fix it

**If the address or your access was wrong**, run the command again with
the repository's real address, once git can read it with your access:

```sh
jadpo package add github.com/acme/postcodes@v2.1.0
```

A repository written `host/owner/name` is fetched over HTTPS. A URL is
used as written, and a folder on this machine is read from its path.

**If the release isn't tagged**, give one of the tags the error lists,
or tag the release in the package's own repository once
`jadpo package check` passes there, and add it again:

```sh
jadpo package add github.com/acme/postcodes@v2.0.0
```

The package is installed at that tag's commit, and the lock records the
repository, the tag and the commit.

## Related

- [Installing safely](../language/11-integrations.md#installing-safely)
- [How packages are installed and linked](../language/11-integrations.md#how-packages-are-installed-and-linked)
- [Where packages come from](../language/11-integrations.md#where-packages-come-from)
- [`jadpo package add`](../language/14-tooling.md#jadpo-package-add)
