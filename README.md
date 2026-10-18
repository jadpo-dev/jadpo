# Jadpo

**Jadpo — one language for the whole web.**

Jadpo is a programming language for software written mainly by LLMs and
reviewed by people. A model proposes the code; a deterministic compiler
decides whether it is valid and permitted; a person reviews what it does
(who can now do what to which data) rather than reading framework code.

## What it looks like

```jadpo
// domains/sales/actions/till_total.jadpo
use sales/actions { price_basket, price_lines }
use sales/failures { BasketEmpty, CodeUnknown, DiscountTooLarge }
use sales/types { Basket }
use shared/types { Pence }

/// What the till charges for `basket`. A mistyped discount code shouldn't
/// stop a sale, so the till charges full price instead.
action till_total(basket: Basket) fails BasketEmpty, DiscountTooLarge -> Pence {
    return match price_basket(basket) {
        success(receipt) -> receipt.total
        CodeUnknown -> price_lines(basket)
        BasketEmpty -> propagate
        DiscountTooLarge -> propagate
    }
}
```

- Every name not declared in the file says where it lives: `Basket` is in
  `domains/sales/types/basket.jadpo`.
- Every way a call can fail is in its signature, and the compiler checks
  that each one is handled or passed on.
- A project has the same shape at every size: business areas under
  `domains/`, each with the same fixed folders.

## How it works

The compiler, written in Rust, turns a project into one graph of what it
declares and how the parts connect. Everything reads that graph: the
checker, the formatter, the language server and the interpreter that runs
tests during development. A code generator will compile the same graph
into deployables for native servers and for WebAssembly on Cloudflare
Workers, and every test will then run both ways.

## Where it stands

- **The language is specified:** twenty-one areas, all decided, in
  [docs/language](docs/language/README.md).
- **Built so far:** project layout, values and types, logic, failures, the
  interpreter, the test runner and watch mode, and stored data: entities,
  migrations and queries on PostgreSQL.
- **Next:** writes, the `use` lines shown above, then HTTP, identity and policy, configuration, events
  and schedules, packages, and the compiled native and WebAssembly
  deployables. The order is the [roadmap](docs/roadmap.md).

## Try it

Build `jadpo` from the repository root (the Rust toolchain is pinned in
`rust-toolchain.toml`), then check and test an example:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/01-first-project
target/release/jadpo test examples/01-first-project
```

The [examples](examples/README.md) teach the language in order, from a
first project to stored records, each commented and with its own tests.

## Find your way

- [Documentation index](docs/README.md)
- [Language specification](docs/language/README.md)
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md)

The project's public home is [jadpo.dev](https://jadpo.dev/); source and
pull requests are at
[richardwillars/jadpo](https://github.com/richardwillars/jadpo).

## Licence

Jadpo's code is licensed under the [Apache License 2.0](LICENSE): use it,
change it and build commercial software with it freely. The documentation is
[CC BY 4.0](docs/LICENSE.md). Modified versions must use a different name;
[TRADEMARKS.md](TRADEMARKS.md) says how the name may be used. The design
skills in `.agents/skills/` are third-party, under their own licences.
