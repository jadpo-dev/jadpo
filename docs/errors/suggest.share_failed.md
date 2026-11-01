# `suggest.share_failed`

Jadpo HQ couldn't be reached, or refused what `jadpo suggest share` sent, so the entries wait to be shared again.

## Why Jadpo refuses it

`jadpo suggest share` sends the hashes of what a suggest session
learned, the suggestions taken, ignored and found, to Jadpo HQ at the
address `JADPO_SUGGEST_HQ` gives. A connection that fails or times out,
and HQ's own trouble, are tried again up to three times over three and a
half seconds. When the last try fails, or HQ refuses what was sent, the
command stops, sends nothing more, and keeps every entry in
`.jadpo/suggest_pending` for the next run, so nothing is lost.

Jadpo won't send the entries somewhere else, or drop them: where your
project's hashes go, and whether they go at all, is yours to decide.

## What it looks like

A shop whose `pence_of` multiplies a price by hand, a suggestion that
was ignored as basis points, not money:

```jadpo check project=e-share-failed file=app.jadpo hidden
application {}
```

```jadpo check project=e-share-failed file=domains/shop/types/whole.jadpo hidden
/// A whole number.
type Whole = Int {}
```

domains/shop/actions/pence_of.jadpo:

```jadpo check project=e-share-failed file=domains/shop/actions/pence_of.jadpo
use shop/types { Whole }

/// Pence from a price in pence.
action pence_of(total_price: Whole) -> Whole {
    return Whole(total_price * 100)
}
```

`JADPO_SUGGEST_HQ` names port 1 of this machine, where nothing listens,
so the entry can't be shared:

```sh run project=e-share-failed expect=suggest.share_failed
jadpo suggest > /dev/null
jadpo suggest ignore S2 "Basis points, not money" > /dev/null
JADPO_SUGGEST_HQ=http://127.0.0.1:1 jadpo suggest share
```

```text output
error: Jadpo HQ couldn't be reached
  reason: No connection to `127.0.0.1:1`: nothing is listening there. Every entry waits in `.jadpo/suggest_pending`.
  next: Run the command again later; nothing was lost (human)
  rule: suggest.share_failed
  help: https://jadpo.dev/docs/errors/suggest.share_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where your project's entries are sent.

An address Jadpo doesn't send to, such as plain `http://` to another
machine, is refused before anything is sent:

```sh run project=e-share-failed expect=suggest.share_failed
jadpo suggest > /dev/null
jadpo suggest ignore S2 "Basis points, not money" > /dev/null
JADPO_SUGGEST_HQ=http://hq.example.test jadpo suggest share
```

```text output
error: `JADPO_SUGGEST_HQ` can't be used
  reason: `http://hq.example.test` is plain `http://`, which Jadpo sends to only on this machine; HQ's address starts with `https://`. Nothing was sent, and the entries wait.
  next: Set `JADPO_SUGGEST_HQ` to HQ's `https://` address (human)
  rule: suggest.share_failed
  help: https://jadpo.dev/docs/errors/suggest.share_failed
```

## How to fix it

The question is whether the address names the HQ you meant.

**If it does**, run the command again later, once HQ answers:

```sh
jadpo suggest share
```

The entries waiting in `.jadpo/suggest_pending` are sent then, and a
retry is safe: HQ counts each entry once.

**If it doesn't**, give HQ's `https://` address, or unset
`JADPO_SUGGEST_HQ`, so `share` sends nothing until HQ is running:

```sh
unset JADPO_SUGGEST_HQ
jadpo suggest share
```

With no address, `share` says that sharing isn't available yet, keeps
the entries and succeeds.

## Related

- [Sharing with Jadpo HQ](../language/14-tooling.md#sharing-with-jadpo-hq)
- [`jadpo suggest`](../language/14-tooling.md#jadpo-suggest)
- [Ignoring a suggestion](../language/14-tooling.md#ignoring-a-suggestion)
