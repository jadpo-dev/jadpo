# 07 · Domains and sharing

A project with two domains, `pantry`, which keeps the flour, and
`sales`, which bakes what customers order: what each keeps to itself,
what it offers, and what they share.

A project grows by adding domains, never by nesting folders. Each domain
owns everything in it, and its declarations are private unless marked
`public`. Code two domains share moves to `shared/`. There are no
imports: the dot in a call names the owner of what is called.

## What it teaches

1. **`shared/`.** Types, a failure and a library two domains use. See `shared/` and [area 1, Sharing and promotion](../../docs/language/01-project-layout-and-modules.md#sharing-and-promotion).
2. **Libraries.** A shared library and a domain's own, called through their names. See `shared/libraries/portions.jadpo`, `domains/pantry/libraries/sacks.jadpo` and [area 3, Calling](../../docs/language/03-logic.md#calling).
3. **`public`.** A public action and a public failure, the only things another domain can reach. See `domains/pantry/actions/take_flour.jadpo`, `domains/pantry/failures/flour_short.jadpo` and [area 1, Ownership and calls replace modules](../../docs/language/01-project-layout-and-modules.md#ownership-and-calls-replace-modules).
4. **Calling across domains.** The dot names the owner: `pantry.take_flour(...)`. See `domains/sales/actions/bake_loaves.jadpo`.
5. **A scenario.** A test in the root `tests/` folder sees only what domains make public. See `tests/flour_runs_out.test.jadpo` and [area 13, Where tests live](../../docs/language/13-testing.md#where-tests-live).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/07-domains-and-sharing
target/release/jadpo test examples/07-domains-and-sharing
target/release/jadpo test examples/07-domains-and-sharing/tests/flour_runs_out.test.jadpo:6
target/release/jadpo test examples/07-domains-and-sharing --watch
target/release/jadpo fmt --check examples/07-domains-and-sharing
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
- In `tests/flour_runs_out.test.jadpo`, add `assert sacks.grams_in(1) == 16000`. `jadpo check` reports `visibility.private_cross_domain`: a scenario can't use `pantry`'s private library, though the pantry's own tests can.
- Then read [08-testing](../08-testing/README.md).
