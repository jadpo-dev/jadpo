# `publish.too_large`

A commit's release is past the size limit.

## Why Jadpo refuses it

Every app that installs a package commits its files, so a release's size is
paid for by everyone who uses it. A release's files come to at most 16 MB
and 10,000 files. What is left out of a release, such as `.github/`, doesn't
count.

Jadpo won't choose what to remove, because what a package offers is its
publisher's to decide.

## What it looks like

A provider package keeps a recording of every response it has ever seen:

```sh
jadpo package verify .
```

```text
error: The release holds 412 files, past what one may
  reason: A release's files come to at most 16 MB and 10000 files, since every app that installs it commits them; this one's come to 21.4 MB.
  next: Split the package, or remove recordings it doesn't need (human)
  rule: publish.too_large
  help: https://jadpo.dev/docs/errors/publish.too_large
```

The publishing bar's `files` row fails with "21.4 MB over the 16.0 MB limit".

`(human)` at the end of the `next:` line tells your coding agent to stop and
ask you which fix you want, because the choice is yours to make: it decides
what the package offers.

## How to fix it

**If it holds recordings it doesn't need**, remove them. One recording of an
operation confirms its types; a second of the same shape adds nothing:

```sh
git rm recordings/payment_intents.create.2.json
```

The release comes to less, and the `files` row passes once it is within the
limit.

**If it covers a whole API that large**, split it into packages by what apps
use together, and verify each from its own folder:

```sh
jadpo package verify ../stripe-billing
```

Each is then its own release, within the limit on its own.

## Related

- [The verifier](../language/11-integrations.md#the-verifier)
- [Releasing](../language/11-integrations.md#releasing)
- [`jadpo package verify`](../language/14-tooling.md#jadpo-package-verify)
