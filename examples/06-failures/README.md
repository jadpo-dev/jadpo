# 06 · Failures

How code says that something went wrong, and how callers pass a failure
on or handle it. The project takes cake orders.

Application code says what failed; the compiler and runtime decide how
that is shown to whoever called. There is no `throw`, no exception and
no error code chosen by hand: a failure is a declared type, every
callable lists the failures that can leave it, and every call that can
fail says at the call site what happens when it does.

## What it teaches

1. **Declaring failures.** One per file in `failures/`: `kind`, `message`, `code`, and `client` and `internal` fields, across all seven kinds an application declares. See `domains/sales/failures/` and [area 4, Declaring a failure](../../docs/language/04-failures.md#declaring-a-failure).
2. **`fails` and `reject`.** A function that fails, and a failure raised with its fields. See `actions/reserve_cakes.jadpo` and [area 4, Raising a failure](../../docs/language/04-failures.md#raising-a-failure-reject).
3. **Checks that can fail.** `attempt CakeCount(n) { max: OrderTooLarge }`: an arm for each rule that can break, `invalid:` covering every rule without its own, and a check the compiler proves can't fail. See `actions/double_order.jadpo`, `actions/order_shelf.jadpo` and [area 4, Passing a failure on](../../docs/language/04-failures.md#passing-a-failure-on-attempt).
4. **A condition as proof.** An `if` that rejects an order too large to double tells the compiler the range after it, so `CakeCount(count * 2)` takes no `attempt`. See `actions/double_checked.jadpo` and [area 2, Conditions as proof](../../docs/language/02-values-and-types.md#conditions-as-proof).
5. **`Unit` actions.** A failure rejected by name alone and an early `return`. See `actions/check_kitchen.jadpo`.
6. **Passing failures on.** `attempt` starting a `var`, a `return` or a line of its own. See `actions/place_order.jadpo` and [area 3, Attempt](../../docs/language/03-logic.md#attempt).
7. **Handling failures.** An outcome `match` that recovers, maps with `reject` and passes on with `propagate`, and hides someone else's order. See `actions/change_order.jadpo` and [area 4, Handling a failure in match](../../docs/language/04-failures.md#handling-a-failure-in-match).

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
- In `double_order.jadpo`, replace the `attempt` and its block with `CakeCount(count * 2)`. `jadpo check` reports `failure.attempt_required`: the compiler can't prove 48 fits in `CakeCount`.
- In `double_order.jadpo`, add `min: OrderTooLarge` to the block. `jadpo check` reports `failure.rule_cannot_break`: `count * 2` is never below 1, so that arm could never run.
- In `double_checked.jadpo`, change `24` in the condition to `30`. `jadpo check` reports `failure.attempt_required`: below the check `count * 2` can now reach 30, past `max: 24`, so the check no longer proves `CakeCount(count * 2)`.
- In `order_shelf.jadpo`, delete the `invalid: ShelfEmpty` line. `jadpo check` reports `failure.rule_unmapped`: an empty shelf breaks `min: 1`, and nothing maps it.
- Then read [07-domains-and-sharing](../07-domains-and-sharing/README.md).
