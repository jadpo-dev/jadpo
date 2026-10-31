# `toolchain.fetch_failed`

The Rust toolchain `jadpo build` compiles with couldn't be fetched, usually because the download server or the mirror `JADPO_RUST_DIST_SERVER` names can't be reached.

## Why Jadpo refuses it

`jadpo build` compiles your app with the one Rust toolchain this Jadpo
is pinned to, so every build of the same code is the same. The first
build on a machine fetches that toolchain from Rust's download server,
or from the mirror `JADPO_RUST_DIST_SERVER` names, checks each archive
against the digest this Jadpo pins, and keeps it in your cache folder.
When the fetch fails, the build stops before it compiles anything, and
nothing is kept that wasn't checked.

Jadpo won't build with another toolchain instead: a build that used
whatever Rust is on the machine could differ from the one you tested.

## What it looks like

Only a network that fails raises this error, so this output is copied
from a real run rather than checked by the docs. It is the first build
on a Mac, with `JADPO_RUST_DIST_SERVER` naming a mirror whose name
can't be found:

```sh
JADPO_RUST_DIST_SERVER=https://rust-mirror.example jadpo build
```

```text
Fetching Rust 1.99.0 for aarch64-apple-darwin
error: Rust 1.99.0 couldn't be fetched
  reason: `jadpo build` compiles with the Rust toolchain this Jadpo is pinned to, fetched once from `https://rust-mirror.example` into `~/.cache/jadpo/toolchains`: `rust-mirror.example` couldn't be found.
  next: Run `jadpo build` again once `https://rust-mirror.example` can be reached, or name a mirror of it in `JADPO_RUST_DIST_SERVER` (human)
  rule: toolchain.fetch_failed
  help: https://jadpo.dev/docs/errors/toolchain.fetch_failed
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides where this machine fetches Rust from.

Without `JADPO_RUST_DIST_SERVER`, the server is
`https://static.rust-lang.org`, and a machine with no network gets this
error naming that server, with the reason its fetch failed.

## How to fix it

The question is which server this machine can reach.

**If Rust's download server can be reached**, leave
`JADPO_RUST_DIST_SERVER` unset, and build again once the network is
back:

```sh
unset JADPO_RUST_DIST_SERVER
jadpo build
```

The toolchain is fetched once, checked and kept, and later builds don't
fetch it again.

**If the machine has to use a mirror**, name one it can reach:

```sh
JADPO_RUST_DIST_SERVER=https://rust-mirror.internal.example.com jadpo build
```

The mirror's manifest is checked against the `.sha256` beside it, so a
mirror can't hand the build a different toolchain.

## Related

- [What it compiles with](../language/14-tooling.md#what-it-compiles-with)
- [A native deployable](../language/14-tooling.md#a-native-deployable)
- [`jadpo build`](../language/14-tooling.md#jadpo-build)
