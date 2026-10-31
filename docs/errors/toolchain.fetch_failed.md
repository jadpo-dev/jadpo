# `toolchain.fetch_failed`

The Rust toolchain or the crates `jadpo build` compiles with couldn't be fetched, usually because the server they come from, or the mirror in its place, can't be reached.

## Why Jadpo refuses it

`jadpo build` compiles your app with the one Rust toolchain this Jadpo
is pinned to, so every build of the same code is the same. The first
build on a machine fetches that toolchain from Rust's download server,
or from the mirror `JADPO_RUST_DIST_SERVER` names, checks each archive
against the digest this Jadpo pins, and keeps it in your cache folder.
The runtime's crates come the same way, the first time cargo's cache
lacks them: from crates.io, or the registry cargo's own configuration
puts in its place, each checked against the lock's checksum. When either
fetch fails, the build stops before it compiles anything, and nothing is
kept that wasn't checked.

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

Crates that can't be fetched give the same rule, with cargo's own lines
under the reason. This output is copied from a real run too: the first
build on a Mac whose `~/.cargo/config.toml` puts a mirror whose name
can't be found in crates.io's place:

```sh
jadpo build
```

```text
Fetching the runtime's crates
error: `app`'s crates couldn't be fetched
  reason: `jadpo build` fetches the crates the runtime's lock names into `~/.cargo` when cargo's cache lacks them, from crates.io or the registry cargo's configuration puts in its place. cargo said:
          error: failed to get `blake3` as a dependency of package `jadpo-runtime v0.1.0 (~/.cache/jadpo/runtime/0.1.0-27184530dfd5/jadpo-runtime)`
              ... which satisfies path dependency `jadpo-runtime` (locked to 0.1.0) of package `jadpo-program-app v0.0.0 (./build/rust/app/program)`
          Caused by:
            failed to load source for dependency `blake3`
          Caused by:
            unable to update registry `crates-io`
          Caused by:
            failed to query replaced source registry `crates-io`
          Caused by:
            download of config.json failed
          Caused by:
            [6] Couldn't resolve host name (Could not resolve host: crates-mirror.example)
  next: Run `jadpo build` again once crates.io can be reached, or put a mirror of it in crates.io's place in cargo's configuration (`[source.crates-io]`) (human)
  rule: toolchain.fetch_failed
  help: https://jadpo.dev/docs/errors/toolchain.fetch_failed
```

A cargo that can't read the workspace Jadpo wrote fails before anything
is fetched, and that is
[`toolchain.build_failed`](toolchain.build_failed.md), not this.

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

**If it is the crates that can't be fetched**, build again once
crates.io can be reached, or put a mirror it can reach in crates.io's
place in cargo's configuration, `~/.cargo/config.toml` or the
`config.toml` in `CARGO_HOME`:

```toml
[source.crates-io]
replace-with = "mirror"

[source.mirror]
registry = "sparse+https://crates-mirror.internal.example.com/index/"
```

Each crate is checked against the lock's checksum, so a mirror can't
hand the build a different crate.

## Related

- [What it compiles with](../language/14-tooling.md#what-it-compiles-with)
- [A native deployable](../language/14-tooling.md#a-native-deployable)
- [`jadpo build`](../language/14-tooling.md#jadpo-build)
