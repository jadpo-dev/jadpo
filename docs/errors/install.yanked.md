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

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you, because which release your app runs is your choice to make.

## How to fix it

**Take the newest good release**, by leaving out `--to`:

```sh
jadpo package add @acme/postcodes
```

**If you have it installed**, look for what replaced it:

```sh
jadpo package audit
jadpo package update postcodes
```

## Related

- [Yanks, takedowns and advisories](../language/11-integrations.md#yanks-takedowns-and-advisories)
