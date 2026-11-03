# `publish.not_at_tag`

What `jadpo package publish` would ask for isn't a pushed `v<version>` tag whose commit is the one checked out.

## Why Jadpo refuses it

A release is made from a tag in a git repository, and the registry fetches
that tag itself. `jadpo package publish` holds the package to the publishing
bar first, so that the registry holds no surprises, and that means nothing
unless the bar runs on what the registry will fetch. So the checkout has to
be the tag's commit with nothing changed, at the root of the repository, and
`origin` has to have the same tag at the same commit.

The tag is `v` and the version the committed `surface.jadpo_record` says.

## What it looks like

The release was tagged and not pushed:

```sh
jadpo package publish
```

```text
error: `v2.1.0` isn't on `origin`
  reason: The registry fetches the tag from the repository, so it has to be pushed.
  next: Run `git push origin v2.1.0` (agent)
  rule: publish.not_at_tag
  help: https://jadpo.dev/docs/errors/publish.not_at_tag
```

Other messages name the rest: no such tag, a checkout at another commit or
with changes since the tag, a package below the repository's root, no
`surface.jadpo_record` yet, and a tag that is another commit on `origin`.

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If it isn't tagged or pushed**, do that:

```sh
jadpo package check
git add -A && git commit -m "feat: read the outward code"
git tag v2.1.0
git push origin main v2.1.0
```

The registry can then fetch the tag, and the commit it fetches is the one the
bar runs on here.

**If the checkout has moved on**, go back to the tag:

```sh
git checkout v2.1.0
```

Or release what you have as a new version, which is then the tag's commit.

**If the tag moved**, release a new version, since the registry refuses a tag
that points at another commit than it did:

```sh
git tag v2.1.1 && git push origin v2.1.1
```

The new tag is a new release, and the old one stays as it was.

## Related

- [Releasing](../language/11-integrations.md#releasing)
- [Developing for the registry](../language/11-integrations.md#developing-for-the-registry)
