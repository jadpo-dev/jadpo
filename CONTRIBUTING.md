# Contributing to Jadpo

Jadpo is experimental. Contributions are accepted under the licence of
the part they change: code under the [Apache License 2.0](LICENSE), as its
section 5 provides, and documentation under [CC BY 4.0](docs/LICENSE.md). The
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

The gate needs Python 3 and Rust, and PostgreSQL's tools (`initdb`,
`pg_ctl`) for its live database tests: it starts one disposable server for
the run, or uses the one `JADPO_TEST_DATABASE_URL` names. It builds once,
runs its steps concurrently and prints `ok` or `FAILED` for each;
`--profile quick` leaves out the live database tests and the wasm32 build,
skips `jadpo test` for projects with migrations (saying so), and needs no
PostgreSQL at all:

```sh
python3 tools/verify.py
python3 tools/verify.py --profile quick
```

Each crate's integration tests are one binary whose modules are the files
in its `tests/` folder; run one file with
`cargo test -p jadpo-cli --test integration queries::`.

A built feature no teaching example uses yet is only noted: the examples
are brought up to date once per phase, by the examples chunk at its end,
which runs `python3 tools/verify.py --require-examples` to make it fail.

To render the language specification as web pages into
`build/language-site/`:

```sh
python3 tools/language_docs.py
```

## Changes

Branch from `main`, keep one roadmap chunk per pull request, and merge when CI
passes.
