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
3. **Checks that can fail.** `attempt CakeCount(n) { max: OrderTooLarge }`: an arm for each rule that can break, and a check the compiler proves can't fail. See `actions/double_order.jadpo` and [area 4, Passing a failure on](../../docs/language/04-failures.md#passing-a-failure-on-attempt).
4. **One failure for many rules.** `invalid:` alone covers every rule that can break, for a count keyed in at the till, and only a construction's rules; beside an arm of its own for one rule, it covers the rest. See `actions/key_in_order.jadpo`, then `actions/check_icing.jadpo`, and [area 2, Construction failures by rule](../../docs/language/02-values-and-types.md#construction-failures-by-rule).
5. **A condition as proof.** An `if` tells the compiler the range inside it, so `CakeCount(count * 2)` takes no `attempt`. See `actions/double_if_room.jadpo` and [area 2, Conditions as proof](../../docs/language/02-values-and-types.md#conditions-as-proof).
6. **A collection's rules.** `attempt Tray(waiting) { min_length: NothingToBake ... }`: a named list type built from a list of its items, whose lengths break as a number's range does. See `actions/fill_tray.jadpo` and [area 2, Named collection construction](../../docs/language/02-values-and-types.md#named-collection-construction).
7. **Business rules beside type rules.** An empty shelf is raised with `reject` as a `conflict`, not as a broken rule, and that check leaves only `max` able to break. See `actions/order_shelf.jadpo`.
8. **`Unit` actions.** A failure rejected by name alone and an early `return`. See `actions/check_kitchen.jadpo`.
9. **Passing failures on.** `attempt` starting a `var`, a `return` or a line of its own. See `actions/place_order.jadpo` and [area 3, Attempt](../../docs/language/03-logic.md#attempt).
10. **Handling failures.** An outcome `match` that recovers, maps with `reject` and passes on with `propagate`, and hides someone else's order. See `actions/change_order.jadpo` and [area 4, Handling a failure in match](../../docs/language/04-failures.md#handling-a-failure-in-match).

The types in `domains/sales/types/` are short; read them as the actions mention them. Each action's tests sit beside it, in `<file>.test.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/06-failures
target/release/jadpo test examples/06-failures
target/release/jadpo test examples/06-failures/domains/sales/actions/change_order.test.jadpo:23
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
  <!-- try {"file": "domains/sales/actions/reserve_cakes.jadpo", "replace": "asked: CakeCount) fails CakeSoldOut -> CakeCount {\n    // A call", "with": "asked: CakeCount) -> CakeCount {\n    // A call", "check": ["failure.undeclared_propagation"]} -->
- In `double_order.jadpo`, replace the `attempt` and its block with `CakeCount(count * 2)`. `jadpo check` reports `failure.attempt_required`: the compiler can't prove `count * 2` stays within `max: 24`; it can reach 48.
  <!-- try {"file": "domains/sales/actions/double_order.jadpo", "replace": "attempt CakeCount(count * 2) {\n        max: OrderTooLarge\n    }", "with": "CakeCount(count * 2)", "check": ["failure.attempt_required"]} -->
- In `double_order.jadpo`, add `min: OrderTooLarge` to the block. `jadpo check` reports `failure.rule_cannot_break`: `count * 2` is never below 1, so that arm could never run.
  <!-- try {"file": "domains/sales/actions/double_order.jadpo", "replace": "        max: OrderTooLarge\n", "with": "        max: OrderTooLarge\n        min: OrderTooLarge\n", "check": ["failure.rule_cannot_break"]} -->
- In `key_in_order.jadpo`, change `invalid: CountOutOfRange` to `max: CountOutOfRange`. `jadpo check` reports `failure.rule_unmapped`: a keyed count can be below 1 too, and now nothing maps `min`.
  <!-- try {"file": "domains/sales/actions/key_in_order.jadpo", "replace": "        invalid: CountOutOfRange", "with": "        max: CountOutOfRange", "check": ["failure.rule_unmapped"]} -->
- In `double_if_room.jadpo`, change `count <= 12` to `count <= 13`. `jadpo check` reports `failure.attempt_required`: inside the branch `count * 2` can now reach 26, past `max: 24`, so the condition no longer proves `CakeCount(count * 2)`.
  <!-- try {"file": "domains/sales/actions/double_if_room.jadpo", "replace": "if (count <= 12)", "with": "if (count <= 13)", "check": ["failure.attempt_required"]} -->
- In `order_shelf.jadpo`, delete the `if (left == 0) { ... }` check. `jadpo check` reports `failure.rule_unmapped`: without the check `left` can be 0, which breaks `min: 1`, and no arm maps it.
  <!-- try {"file": "domains/sales/actions/order_shelf.jadpo", "replace": "    if (left == 0) {\n        reject ShelfEmpty\n    }\n", "with": "", "check": ["failure.rule_unmapped"]} -->
- In `fill_tray.jadpo`, change `waiting: List<CakeCount>` to `waiting: List<Stock>`, and add `Stock` to `use sales/types { ... }`. `jadpo check` reports `type.constructor_input`: building a `Tray` never converts its items, so each number becomes a `CakeCount` where it is made, with its own arms, as `order_shelf.jadpo` does.
  <!-- try {"file": "domains/sales/actions/fill_tray.jadpo", "replace": "waiting: List<CakeCount>", "with": "waiting: List<Stock>", "also": [{"replace": "use sales/types { CakeCount }", "with": "use sales/types { CakeCount, Stock }"}], "check": ["type.constructor_input"]} -->
- Then read [07-testing](../07-testing/README.md).
