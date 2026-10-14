# Contributing to Jadpo

Jadpo is experimental and licensing has not been chosen yet. The
[roadmap](docs/roadmap.md) is the plan; [AGENTS.md](AGENTS.md) has the working
rules for people and agents alike.

## Setup and checks

Clone [richardwillars/jadpo](https://github.com/richardwillars/jadpo). The
repository holds the language specification, the plan, the compiler (a Rust
workspace under `crates/`) and the teaching examples. `rustup` installs the
pinned toolchain from `rust-toolchain.toml`. To build `jadpo`:

```sh
cargo build --release -p jadpo-cli
```

The gate needs Python 3 and Rust, and PostgreSQL for its live database
tests (`--profile quick` leaves them out):

```sh
python3 tools/verify.py
```

To render the language specification as web pages into
`build/language-site/`:

```sh
python3 tools/language_docs.py
```

## Changes

Branch from `main`, keep one roadmap chunk per pull request, and merge when CI
passes.
