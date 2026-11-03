# `publish.failed`

The registry's verifier refused the release.

## Why Jadpo refuses it

`jadpo package publish` asks the registry to release a repository at a tag.
The registry's verifier fetches that commit and holds it to the whole
publishing bar with the Jadpo release the package declares and the newest,
and everything the registry shows is worked out from what it finds. A
release that fails never reaches a releaser to confirm.

`jadpo package publish` runs the bar here first, so this usually means
something differs between the two: another Jadpo release, or files the
verifier doesn't accept in a commit.

## What it looks like

`--dry-run` runs the verifier on the pushed tag's commit and publishes
nothing, and a failed row is named:

```sh
jadpo package publish --dry-run
```

```text
error: The registry's verifier refused `Postcodes.v2 2.1.0`
  reason: Failed: tests: 1 of 3 fails.
  next: Fix it, run `jadpo package check`, then tag a new release: a tag that moves is refused (agent)
  rule: publish.failed
  help: https://jadpo.dev/docs/errors/publish.failed
```

The same rule says when a member of the publisher cancelled a pending
release, and when the registry hasn't finished after 20 minutes.

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a row of the bar failed**, fix what it names and release a new version,
because a tag that moves is refused:

```sh
jadpo package check
git add -A && git commit -m "fix: read postcodes in upper case"
git tag v2.1.1 && git push origin v2.1.1
jadpo package publish --dry-run
```

The dry run then checks the new tag's commit as the release will be, and
publishes nothing.

## Related

- [Releasing](../language/11-integrations.md#releasing)
- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [Developing for the registry](../language/11-integrations.md#developing-for-the-registry)
