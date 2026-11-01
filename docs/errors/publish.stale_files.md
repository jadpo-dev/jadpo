# `publish.stale_files`

A commit's `surface.jadpo_record` or changelog list isn't what the package's code gives.

## Why Jadpo refuses it

A release shows what its code gives, never what its publisher says.
`jadpo package check` writes `surface.jadpo_record` and the new version's
list in `CHANGELOG.md` from the code, and they are committed so that a
reviewer reads them in the pull request.

`jadpo package verify` checks a commit as it stands, as the registry does,
and writes nothing. A file that disagrees with the code would show a reader
something other than what the release does, so it is refused. It usually
means the code changed after `jadpo package check` last ran, or the file
was edited by hand.

## What it looks like

`Postcodes.v2` gained a public type after `jadpo package check` last ran,
and the commit was verified as it stood:

```sh
jadpo package verify . --previous ../postcodes-2.0.0
```

```text
error: `surface.jadpo_record` isn't what the package's code gives
  reason: What the commit carries isn't the surface of 2.1.0, which its code gives. A release shows what its code gives, so the file a commit carries has to match; `jadpo package verify` checks a commit as it stands and writes nothing.
  next: Run `jadpo package check`, commit the surface.jadpo_record it writes, and tag the release again (agent)
  rule: publish.stale_files
  help: https://jadpo.dev/docs/errors/publish.stale_files
```

The publishing bar's `surface` row fails with "doesn't match its code", or
its `changelog` row with "2.1.0's list is out of date" for
`CHANGELOG.md`.

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself: nothing is being decided, since both files come from the
code.

## How to fix it

**If the file is out of date**, write it again and commit it, then move the
tag to that commit if it hasn't been released:

```sh
jadpo package check
git add surface.jadpo_record CHANGELOG.md
git commit -m "chore: write the surface for 2.1.0"
```

The surface and the list then match the code, and `jadpo package verify`
passes those rows. The reason you wrote beneath the list stays.

## Related

- [The verifier](../language/11-integrations.md#the-verifier)
- [How a release's facts, change record and fingerprints are worked out](../language/11-integrations.md#how-a-releases-facts-change-record-and-fingerprints-are-worked-out)
- [The surface](../language/11-integrations.md#the-surface)
- [`jadpo package verify`](../language/14-tooling.md#jadpo-package-verify)
