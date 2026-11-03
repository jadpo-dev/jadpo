# `install.yanked`

The release you asked for has been yanked, so it can't be newly installed.

## Why Jadpo refuses it

A releaser can yank a release that should no longer be used. Apps that have
it keep working, since the code is committed in them, and nothing installs
it anew. Jadpo never takes a yanked release by itself, so this comes only
when you name one, or when a package has no other.

## What it looks like

An app asks for a release its publisher withdrew:

```sh
jadpo package add @acme/postcodes --to 2.1.0
```

```text
error: `@acme/postcodes` 2.1.0 has been yanked
  reason: A releaser withdrew it: apps that have it keep working, and nothing installs it anew.
  next: Take another release: leave out `--to` for the newest (human)
  rule: install.yanked
  help: https://jadpo.dev/docs/errors/install.yanked
```

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: it decides which release your app runs.

## How to fix it

**If you named the release**, take the newest good one by leaving out `--to`:

```sh
jadpo package add @acme/postcodes
```

The newest release that isn't yanked or a pre-release is installed instead.

**If you have it installed**, see whether an advisory touches it and move on:

```sh
jadpo package audit
jadpo package update postcodes
```

The app keeps working until you do, and then runs the newer release.

## Related

- [Yanks, takedowns and advisories](../language/11-integrations.md#yanks-takedowns-and-advisories)
