# Contributing to Jadpo

Jadpo is experimental. The [roadmap](docs/roadmap.md) records
remaining work; [AGENTS.md](AGENTS.md) has the working rules. The full golden application remains unfinished.
Licensing has not yet been selected.

## Setup and checks

Clone [richardwillars/jadpo](https://github.com/richardwillars/jadpo) and build
the compiler from the repository root:

```sh
cargo build --locked --manifest-path jadpo/Cargo.toml -p jadpo-cli
cargo run --locked --manifest-path jadpo/Cargo.toml -p jadpo-cli -- check examples/jadpo-seed
```

The supported gate is `python3 tools/verify.py`. It requires Rust 1.78,
Bun, Node.js, Python, Ruby, Bash and PostgreSQL tools; see the
[validation guide](tests/validation/README.md) for the complete setup and suite
boundaries. Focused checks are useful during implementation. Run the required
full gate on the integrated revision before landing compiler/runtime changes.
Use `--require-golden` when claiming full golden completion; ordinary supported
checks explicitly retain the open golden gates.

## Branches and commits

`main` is the integration baseline. Make each change on a short-lived branch
from `main`, commit coherent checkpoints, and merge once the full gate passes.
Keep implementation, tests and the owning documentation in the same change.
Keep secrets and local caches out of Git; checked-in service-adapter TLS keys
are deliberately public test fixtures for `.invalid` hosts.

Use worktrees only under the repository's parent directory, never under
`/private/tmp`, and remove them once their branch is merged.
