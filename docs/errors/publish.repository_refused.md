# `publish.repository_refused`

The package's repository isn't one the registry can fetch: a public git repository reached over HTTPS.

## Why Jadpo refuses it

The registry never accepts an upload, and no publish token exists: it
fetches each release itself, from a public git repository over HTTPS, so
anyone can check the commit a release came from. `jadpo package publish`
sends the address of `origin` and a tag and nothing else. An `ssh` address of
a repository is sent as the same repository's `https://` address; anything
else can't be turned into one.

## What it looks like

The package's `origin` is plain HTTP:

```sh
jadpo package publish
```

```text
error: The registry can't publish from `http://git.acme.example/postcodes.git`
  reason: Its `origin` isn't an HTTPS address. A release is made from a public git repository reached over HTTPS, since the registry never accepts an upload.
  next: Point `origin` at the repository's `https://` address: `git remote set-url origin https://<host>/<path>` (human)
  rule: publish.repository_refused
  help: https://jadpo.dev/docs/errors/publish.repository_refused
```

The same rule says when the repository has no `origin`, and when the
registry refuses the address it was sent.

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: it decides where your package is public.

## How to fix it

**If `origin` isn't its HTTPS address**, point it there:

```sh
git remote set-url origin https://github.com/acme/postcodes
```

The registry can then fetch the tag itself.

**If the repository is private**, it can't go to the public registry. Install
it from git instead, with your own access:

```sh
jadpo package add github.com/acme/postcodes@v2.1.0
```

An app locks it at that tag's commit, and nothing goes through the registry.

## Related

- [Releasing](../language/11-integrations.md#releasing)
- [Private packages come from git](../language/11-integrations.md#private-packages-come-from-git)
