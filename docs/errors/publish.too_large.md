# `publish.too_large`

A commit's release is past the size limit.

## Why Jadpo refuses it

The registry is itself a Jadpo application, and it receives a release's
archive whole, in one answer from the verifier. That answer has a limit, so
a larger release would pass every check and never go live. A release's
archive comes to at most 7 MB and 10,000 files, counting a small header for
each file. What is left out of a release, such as `.github/`, doesn't count.

Jadpo won't choose what to remove, because what a package offers is its
publisher's to decide.

## What it looks like

A provider package keeps a recording of every response it has ever seen:

```sh
jadpo package verify .
```

```text
error: The release holds 412 files, past what one may
  reason: A release's archive comes to at most 7 MB and 10000 files, since the registry has to receive it whole in one answer; this one's comes to 21.40 MB.
  next: Split the package, or remove recordings it doesn't need (human)
  rule: publish.too_large
  help: https://jadpo.dev/docs/errors/publish.too_large
```

The publishing bar's `files` row fails with "21.40 MB over the 7.00 MB limit".

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
