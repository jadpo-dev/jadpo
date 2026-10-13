# Contributing to Jadpo

Jadpo is experimental and licensing has not been chosen yet. The
[roadmap](docs/roadmap.md) is the plan; [AGENTS.md](AGENTS.md) has the working
rules for people and agents alike.

## Setup and checks

Clone [richardwillars/jadpo](https://github.com/richardwillars/jadpo). Today
the repository holds the language specification and the plan; the new
compiler arrives with roadmap Phase 1. The gate needs Python 3, and Rust once
the compiler workspace exists:

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
passes. The previous compiler is at the tag `archive/compiler-v0`.
