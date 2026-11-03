# `publish.file_not_allowed`

A commit holds a file a release can't: anything but Jadpo source and the package's text files.

## Why Jadpo refuses it

Nothing in a package runs when it is installed or built, and everything an
app receives from one can be read and reviewed in its pull request. A
script, a binary or a data file would be something an app commits without
anyone being able to tell what it does, so a release holds only `.jadpo`
source, its generated records, `GUIDE.md`, `CHANGELOG.md`, the recordings
under `recordings/` and, in a `sample/` app, its `jadpo.lock`. Every one of
them has to be text, and none of them a symbolic link or a submodule.

What a repository needs for itself isn't refused. Hidden files and folders
such as `.gitignore` and `.github/`, and a `README`, `LICENSE`, `LICENCE`
or `COPYING` at the root, are left out of the release: nobody who installs
the package receives them, and they aren't in its digest.

## What it looks like

A package's repository keeps a script beside its code:

```sh
jadpo package verify .
```

```text
error: `scripts/fetch.sh` can't be in a release
  reason: A release holds only Jadpo source and the package's text files, and it isn't Jadpo source or one of a package's text files.
  next: Remove `scripts/fetch.sh` from the commit, or keep it in a hidden folder such as `.github/`, which isn't part of a release (agent)
  rule: publish.file_not_allowed
  help: https://jadpo.dev/docs/errors/publish.file_not_allowed
```

The publishing bar's `files` row fails with "1 file not allowed". At the
registry, nothing of the commit is written anywhere or run.

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself: moving the file changes nothing an app receives.

## How to fix it

**If the repository needs the file**, move it to a hidden folder:

```sh
git mv scripts/fetch.sh .github/fetch.sh
```

It stays in the repository and is left out of the release. The `files` row
then passes, with how many files the release holds and how large it is.

**If it was meant for apps to use**, write it in Jadpo, or describe it in
`GUIDE.md`.

## Related

- [The verifier](../language/11-integrations.md#the-verifier)
- [Releasing](../language/11-integrations.md#releasing)
- [`jadpo package verify`](../language/14-tooling.md#jadpo-package-verify)
