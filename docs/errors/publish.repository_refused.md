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

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because where your package is public is your choice to make.

## How to fix it

**Point `origin` at its HTTPS address**:

```sh
git remote set-url origin https://github.com/acme/postcodes
```

**If the repository is private**, it can't go to the public registry.
Private packages come from git: `jadpo package add
github.com/acme/postcodes@v2.1.0`.

## Related

- [Releasing](../language/11-integrations.md#releasing)
- [Private packages come from git](../language/11-integrations.md#private-packages-come-from-git)
