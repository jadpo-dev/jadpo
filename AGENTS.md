# Jadpo

Jadpo is an experimental language for software written mainly by LLMs and
reviewed by humans. A Rust compiler (`jadpo/`) checks source and generates a
TypeScript/Bun HTTP app with SQLite/PostgreSQL persistence.

## Where things are

- Outstanding work, in order: [docs/roadmap.md](docs/roadmap.md). It is the
  only tracker.
- Language meaning: the specifications indexed in [docs/README.md](docs/README.md)
  and the [decision register](docs/decision-register.md).
- Golden todo: contract in `examples/golden-todo/`, current app in
  `examples/golden-todo-migration/`.
- Old plans, history and process documents: [docs/archive/](docs/archive/README.md).
  Read them only when a task needs the background.

## Build and check

```sh
cargo build --locked --manifest-path jadpo/Cargo.toml -p jadpo-cli
python3 tools/verify.py --profile quick   # skips live PostgreSQL
python3 tools/verify.py                   # full gate
python3 tools/verify.py --require-golden  # also requires all 88 golden results
```

Prerequisites (Rust, Bun, Node, Python, Ruby, PostgreSQL tools) are in the
[validation guide](tests/validation/README.md). Use focused tests while
iterating and the full gate before merging compiler or runtime changes.

## Working rules

- Branch from `main`, commit and push often, merge when the full gate passes.
  Never create worktrees under `/private/tmp`.
- The verifier and tests are the evidence. Do not create evidence packets,
  review records, timing logs, progress reports or per-task plan documents.
- Update `docs/roadmap.md` only when an item finishes or the order changes.
- If a change needs an undecided language choice, stop and ask Richard with a
  recommendation; do not special-case the compiler to make one example pass.
