# Jadpo

Jadpo is an experimental programming language for web backends that a
coding agent writes and a person reviews. Data, HTTP routes, sign-in,
permissions, outside services and background work are declared in the
language, so the compiler can check them together and answer the review
questions: what can this app do, who can do it, and what can it reach?

## What the compiler catches

An action lists the failures it can cause, and anything that calls it must
handle each one or pass it on. This `match` forgets one:

```jadpo check project=readme-error file=main.jadpo expect=failure.outcome_missing_arm
type LookupKey = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

type LookupRefused = Failure {
    kind: FailureKind.rejected
}

function lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    if (key == "refused") {
        reject LookupRefused
    }
    return "found"
}

function describe(key: LookupKey) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "missing"
    }
}
```

```text output
error: This `match` doesn't handle `LookupRefused`
 --> main.jadpo:24:12
   |
24 |     return match lookup(key) {
   |            ^^^^^^^^^^^^^^^^^ failures not handled
  reason: `lookup(...)` can fail with `LookupMissing` and `LookupRefused`, and a `match` on its outcome has an arm for every one: a value, `reject` or `propagate`.
  next: Add an arm for `LookupRefused` (agent)
  rule: failure.outcome_missing_arm
  help: docs/errors/failure.outcome_missing_arm.md

check failed: 1 error, 1 file
```

This is the compiler's real output, checked by the gate. It names the
fix, and who should make it: the agent.

## Learn more

- [Learn Jadpo](https://jadpo.dev/learn): build a small app a page at a time.
- [Documentation](https://jadpo.dev/docs): the primer, the language reference and a page for every compiler error.
- [Install](https://jadpo.dev/install)
- [For coding agents](https://jadpo.dev/agents): a prompt to paste in and the order to read the docs in. An agent can start from [llms.txt](https://jadpo.dev/llms.txt).

## Status

Experimental. All 21 areas of the language are specified. The compiler
runs programs in an interpreter and builds native and WebAssembly
deployables, but not everything specified is built yet. Production
readiness and performance against established stacks are not established.
The [roadmap](docs/roadmap.md) is the list of what remains, and
[jadpo.dev/status](https://jadpo.dev/status) shows what is built today.

## Build from source

You need Git and Rust through `rustup`; the repository pins the toolchain.

```sh
git clone https://github.com/jadpo-dev/jadpo.git && cd jadpo
cargo build --release --locked -p jadpo-cli
target/release/jadpo test examples/01-first-project
```

The [28 teaching projects](examples/README.md) go from a first function to
databases, authentication, integrations and AI.

## Contribute, report, licence

- [CONTRIBUTING.md](CONTRIBUTING.md): setup, the verification gate and working rules.
- [SECURITY.md](SECURITY.md): how to report a vulnerability.
- [Issues](https://github.com/jadpo-dev/jadpo/issues) for bugs and questions.

Code is [Apache 2.0](LICENSE); documentation is [CC BY 4.0](docs/LICENSE.md).
See [TRADEMARKS.md](TRADEMARKS.md) for use of the Jadpo name.
