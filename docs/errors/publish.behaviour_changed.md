# `publish.behaviour_changed`

An example or test of a package's last release fails against its new version, and the change isn't marked a fix.

## Why Jadpo refuses it

A release within one major version, such as 2.0.0 to 2.0.1, promises the
apps on it that what they used still does what it did. The types can't
show everything: a function can take and give the same types and answer
differently, and nothing in the surface or the upgrade's review says so.

So the bar runs the last release's examples and tests against the new
code, whatever the package's own tests now say. Each one that fails is a
change an app would get with nothing to review, so it goes in the change
record and the release is refused. An example or test that no longer
compiles has failed too, so removing a helper an old test leans on can't
hide a change.

Jadpo won't decide whether the old behaviour was right: only someone who
knows what the package is for can say.

## What it looks like

`Postcodes.v2` was released as 2.0.0, and `uk.parse` reads a postcode in
upper case. A change makes it read lower case instead, with the same types,
and changes the tests beside it to match. In the package's repository:

```sh
jadpo package check
```

```text
error: `uk.parse` behaves differently from 2.0.0
  reason: `example of uk.parse`, from 2.0.0, now fails against the new code. Run as a test of the package, it expected "SW1A 1AA" and got "sw1a 1aa". An app on 2.0.0 may rely on what it showed, and a change the types don't show reaches it with nothing to review.
  next: Put back what it did, publish the change as `Postcodes.v3`, or, if 2.0.0 was wrong, mark it a fix under `## 2.0.1` in CHANGELOG.md: Fix `uk.parse`: why it had to change (human)
  rule: publish.behaviour_changed
  help: https://jadpo.dev/docs/errors/publish.behaviour_changed
```

The change record lists it, and the publishing bar's `behaviour` row fails:

```text
Changes since 2.0.0
  ~ behaviour changed  uk.parse  example of uk.parse

Publishing bar
  behaviour     failed                        1 of 2 fails, not marked a fix
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether apps on this major version get a different answer from the
same call when they upgrade.

## How to fix it

The question is whether the last release was right.

**If the last release was right**, put back what it did. Then run the bar
again:

```sh
jadpo package check
```

The `behaviour` row passes, and the entry leaves the change record.

**If the last release was wrong**, mark the change a fix in the new
version's section of `CHANGELOG.md`, beneath the list Jadpo writes, saying
why it had to change:

```markdown
## 2.0.1

- behaviour changed `uk.parse`: example of uk.parse

Fix `uk.parse`: upper case broke the courier's lookup.
```

The `behaviour` row passes with "marked a fix", and the release shows the
reason beside the entry, so whoever upgrades can read it.

**If the change is worth having but the last release wasn't wrong**,
publish it as a new package, `Postcodes.v3`, in a folder of its own: its
`package.jadpo` names `Postcodes.v3`. Run the bar there:

```sh
jadpo package check
```

Apps keep `Postcodes.v2` until they move across, and its change record
still lists what behaves differently.

## Related

- [The changelog](../language/11-integrations.md#the-changelog)
- [How a release's facts, change record and fingerprints are worked out](../language/11-integrations.md#how-a-releases-facts-change-record-and-fingerprints-are-worked-out)
- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [`jadpo package check`](../language/14-tooling.md#jadpo-package-check)
