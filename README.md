# Jadpo

[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/16073/badge)](https://www.bestpractices.dev/projects/16073)
[![OpenSSF Baseline](https://www.bestpractices.dev/projects/16073/baseline)](https://www.bestpractices.dev/projects/16073)

Jadpo is an experimental programming language for web backends that a
coding agent writes and a person reviews. Data, HTTP routes, sign-in,
permissions, outside services and background work are declared in the
language, so the compiler can check them together and answer the review
questions: what can this app do, who can do it, and what can it reach?

## What the compiler catches

You ask your agent to show partners their details, and it sends back their
API key too. In Jadpo that doesn't compile:

```jadpo check project=readme-error file=main.jadpo expect=type.secret_exposed
type PartnerName = Text {
    max_length: 100
}

/// What the partners page shows of a partner.
type PartnerProfile = Output {
    name: PartnerName
    partner_key: ApiKey
}
```

```text output
error: `PartnerProfile.partner_key` holds an `ApiKey`
 --> main.jadpo:8:18
  |
8 |     partner_key: ApiKey
  |                  ^^^^^^ sent to a client
  reason: An `ApiKey` is a secret value: it is never sent, logged, stored or shown. A response is written out, logged by proxies and kept by browsers, and a client never needs a secret back.
  next: Remove `partner_key`, and send what the client needs instead, such as whether one is set (agent)
  rule: type.secret_exposed
  help: https://jadpo.dev/docs/errors/type.secret_exposed

check failed: 1 error, 1 file
```

This is the compiler's real output, checked on every change. It names the
fix and who should make it: `(agent)` means your coding agent can fix it
itself. A leaking field, a route that skips sign-in, a record fetched
without checking who may see it: each is a compile error like this one.

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
