# 08 · Domains and sharing

A project with two domains, `pantry`, which keeps the flour, and
`sales`, which bakes what customers order: what each keeps to itself,
what it offers, and what they share.

A project grows by adding domains, never by nesting folders. Each domain
owns everything in it, and its declarations are private unless marked
`public`. Code two domains share moves to `shared/`. Each file names
where its names come from on `use` lines, and the dot in a call names
the owner of what is called.

## What it teaches

1. **`shared/`.** Types, a failure and a library two domains use. See `shared/types/`, `shared/failures/too_heavy.jadpo` (raised by the library and by `domains/pantry/actions/restock.jadpo`) and [area 1, Sharing and promotion](../../docs/language/01-project-layout-and-modules.md#sharing-and-promotion).
2. **Libraries.** A shared library and a domain's own, called through their names. See `shared/libraries/portions.jadpo`, `domains/pantry/libraries/sacks.jadpo` and [area 3, Calling](../../docs/language/03-logic.md#calling).
3. **`public`.** A public action and a public failure, the only things another domain can reach. See `domains/pantry/actions/take_flour.jadpo`, `domains/pantry/failures/flour_short.jadpo` and [area 1, Ownership and calls replace modules](../../docs/language/01-project-layout-and-modules.md#ownership-and-calls-replace-modules).
4. **Calling across domains.** `use pantry` brings in the domain, and the dot names the owner at every call: `pantry.take_flour(...)`. Its public failure comes in by folder like any other name, `use pantry/failures { FlourShort }`, and so do `shared/`'s names. See `domains/sales/actions/bake_loaves.jadpo` and [area 1, Imports](../../docs/language/01-project-layout-and-modules.md#imports).
5. **A scenario.** A test in the root `tests/` folder sees only what domains make public. See `tests/baking_from_the_pantry.test.jadpo` and [area 13, Where tests live](../../docs/language/13-testing.md#where-tests-live).
6. **A public fixture.** Fixtures (example 07) shared across domains: `sales`'s tests and the scenario call `pantry.a_days_flour()`, which application code never can. See `domains/pantry/fixtures/a_days_flour.jadpo` and [area 13, Shared test data: fixtures](../../docs/language/13-testing.md#shared-test-data-fixtures).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/08-domains-and-sharing
target/release/jadpo test examples/08-domains-and-sharing
target/release/jadpo test examples/08-domains-and-sharing/tests/baking_from_the_pantry.test.jadpo:11
target/release/jadpo test examples/08-domains-and-sharing --watch
target/release/jadpo fmt --check examples/08-domains-and-sharing
```

- `check` ends with `checked: <n> files` and no errors. It also writes
  the project's manifest to `build/` inside the project, which git
  ignores.
- `test` runs every test in an order a seed decides and ends with
  `tested: <n> passed`, counting each kind of test (pure or action).
  `<file>:<line>` runs the one test on that line; `--seed <n>` repeats an
  order.
- `--watch` keeps running and, on every save, reruns the tests the change
  reaches. Stop it with ctrl-c.
- `fmt --check` ends with `formatted: <n> files`: every file is already
  in the one layout `jadpo fmt` writes.

## Try next

- In `take_flour.jadpo`, delete `public`. `jadpo check` reports `visibility.private_cross_domain` where `sales` calls it.
  <!-- try {"file": "domains/pantry/actions/take_flour.jadpo", "replace": "public action take_flour", "with": "action take_flour", "check": ["visibility.private_cross_domain"]} -->
- In `a_days_flour.jadpo`, delete `public`. `jadpo check` reports `visibility.private_cross_domain` where `sales`'s test and the scenario call it.
  <!-- try {"file": "domains/pantry/fixtures/a_days_flour.jadpo", "replace": "public fixture a_days_flour", "with": "fixture a_days_flour", "check": ["visibility.private_cross_domain"]} -->
- In `tests/baking_from_the_pantry.test.jadpo`, add `var one: FlourWeight = attempt sacks.weight_in(1)` and `assert one == 16kg`. `jadpo check` reports `visibility.private_cross_domain`: a scenario can't use `pantry`'s private library, though the pantry's own tests can.
  <!-- try {"file": "tests/baking_from_the_pantry.test.jadpo", "replace": "    assert pantry.take_flour(16001g, 1) fails FlourShort\n", "with": "    assert pantry.take_flour(16001g, 1) fails FlourShort\n    var one: FlourWeight = attempt sacks.weight_in(1)\n    assert one == 16kg\n", "check": ["visibility.private_cross_domain"]} -->
- In `bake_loaves.jadpo`, delete the line `use pantry`. `jadpo check` reports `import.missing` at `pantry.take_flour`: a call into another domain needs the domain imported.
  <!-- try {"file": "domains/sales/actions/bake_loaves.jadpo", "replace": "use pantry\n", "with": "", "check": ["import.missing"]} -->
- Then read [09-bakery-orders](../09-bakery-orders/README.md).
