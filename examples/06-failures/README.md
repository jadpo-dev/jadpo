# 06 · Failures

How code says that something went wrong, and how callers pass a failure
on or handle it. The project takes cake orders.

## What it teaches

1. **Declaring failures.** One per file in `failures/`: `kind`, `message`, `code`, and `client` and `internal` fields, across all seven kinds an application declares. See `domains/sales/failures/` and [area 4, Declaring a failure](../../docs/language/04-failures.md#declaring-a-failure).
2. **`fails` and `reject`.** A function that fails, and a failure raised with its fields. See `actions/reserve_cakes.jadpo` and [area 4, Raising a failure](../../docs/language/04-failures.md#raising-a-failure-reject).
3. **Checks that can fail.** `attempt Type(value) invalid: Failure`, and one the compiler proves can't fail. See `actions/double_order.jadpo` and [area 4, Passing a failure on](../../docs/language/04-failures.md#passing-a-failure-on-attempt).
4. **`Unit` actions.** A failure rejected by name alone and an early `return`. See `actions/check_kitchen.jadpo`.
5. **Passing failures on.** `attempt` starting a `var`, a `return` or a line of its own. See `actions/place_order.jadpo` and [area 3, Attempt](../../docs/language/03-logic.md#attempt).
6. **Handling failures.** An outcome `match` that recovers, maps with `reject` and passes on with `propagate`, and hides someone else's order. See `actions/change_order.jadpo` and [area 4, Handling a failure in match](../../docs/language/04-failures.md#handling-a-failure-in-match).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/06-failures
target/release/jadpo test examples/06-failures
target/release/jadpo test examples/06-failures/domains/sales/actions/change_order.test.jadpo:21
target/release/jadpo test examples/06-failures --watch
target/release/jadpo fmt --check examples/06-failures
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

- In `reserve_cakes.jadpo`, delete `fails CakeSoldOut` from `reserve_cakes`. `jadpo check` reports `failure.undeclared_propagation`: every failure that can leave a callable is listed.
- In `double_order.jadpo`, replace `attempt CakeCount(count * 2) invalid: OrderTooLarge` with `CakeCount(count * 2)`. `jadpo check` reports `failure.attempt_required`: the compiler can't prove 48 fits in `CakeCount`.
- Then read [07-domains-and-sharing](../07-domains-and-sharing/README.md).
