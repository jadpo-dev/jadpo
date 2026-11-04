# `publish.publisher_mismatch`

A release names another publisher than the one asked to publish it.

## Why Jadpo refuses it

`package.jadpo` names a package's publisher so a fork can't be published as the
original. The registry reads that name from the release and holds it to what it
knows: a repository's earlier releases are one publisher's, a name is released
from one repository, and a signed-in member asks as one publisher. A release
that disagrees is refused, whoever would have confirmed it.

## What it looks like

A fork of `@acme/postcodes` that still says `publisher: "@acme"`:

```sh
jadpo package publish
```

```text
error: `Postcodes.v2 2.1.0` can't be released as `@acme`
  reason: The registry refused it: `@acme/postcodes` is released from `https://github.com/acme/postcodes`, and this is `https://github.com/mallory/postcodes`.
  next: If this is a fork, name your own handle in `package.jadpo` and tag a new release; if it is yours, ask on jadpo.dev (human)
  rule: publish.publisher_mismatch
  help: https://jadpo.dev/docs/errors/publish.publisher_mismatch
```

`jadpo package verify <folder> --publisher @other` says the same of a package
naming anyone but `@other`, and the verifier does when the registry tells it who
the release is for.

`(human)` at the end of the `next:` line tells your coding agent to stop and ask you which fix you want, because the choice is yours to make: whose package this is isn't something an agent can decide.

## How to fix it

**If this is a fork**, publish it under your own handle. It is then your own
package, `@you/postcodes`, and its name in code can stay:

```jadpo fragment
/// UK postcodes: parsing, normalising and checking the outward code.
package Postcodes.v2 {
    publisher: "@you"
    licence: Licence.mit
    jadpo: "0.3"
}
```

```jadpo check project=e-publisher-mismatch-fix file=shared/providers/postcodes/v2/package.jadpo hidden
/// UK postcodes: parsing, normalising and checking the outward code.
package Postcodes.v2 {
    publisher: "@you"
    licence: Licence.mit
    jadpo: "0.3"
}
```

```jadpo check project=e-publisher-mismatch-fix file=app.jadpo hidden
application {}
```

Run `jadpo package check` and tag a new release: nothing of `@acme`'s is
touched, and your releasers confirm it.

**If the package is yours and moved**, an owner moves it to its new repository on
its page on jadpo.dev, with a passkey, and then asks again:

```sh
jadpo package publish
```

The new repository's releases are then the package's.

**If a tag named the wrong publisher by mistake**, release a new version that
names the right one:

```sh
jadpo package check
git add -A && git commit -m "fix: name the right publisher"
git tag v2.1.2 && git push origin main v2.1.2
```

A package never changes publisher by a release: handing one on is
[succession](../language/11-integrations.md#releasing).

## Related

- [Names and publishers](../language/11-integrations.md#names-and-publishers)
- [Releasing](../language/11-integrations.md#releasing)
