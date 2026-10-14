# 05 · Logic

How behaviour is written: operators and the order they apply in,
choices with `if` and `match`, division that says how it rounds, and
recursion over a shape that contains itself. The project is a bakery's
kitchen.

## What it teaches

1. **Operators.** Precedence, brackets, `and`, `or`, `not`, unary minus, `%`, and comparisons. See `domains/kitchen/actions/mix_dough.jadpo` and [area 3, Expressions and operators](../../docs/language/03-logic.md#expressions-and-operators).
2. **Division.** `integer.divide` and `decimal.divide` with a named `rounding:`, each `Rounding` mode, and divisors that can't be zero. See `actions/fill_trays.jadpo` and [area 3, Expressions and operators](../../docs/language/03-logic.md#expressions-and-operators).
3. **Choosing.** `if`, `else if` and `else`; `match` as a value and as a statement, with literal, text, `true`/`false` and `_` patterns and empty arms. See `actions/set_oven.jadpo` and [area 3, Match](../../docs/language/03-logic.md#match).
4. **Recursion.** A type that contains itself, walked by a function that calls itself. See `actions/count_tiers.jadpo` and [area 3, Recursion](../../docs/language/03-logic.md#recursion).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/05-logic
target/release/jadpo test examples/05-logic
target/release/jadpo test examples/05-logic/domains/kitchen/actions/set_oven.test.jadpo:13
target/release/jadpo test examples/05-logic --watch
target/release/jadpo fmt --check examples/05-logic
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

- In `fill_trays.jadpo`, change `TrayCapacity`'s `min: 1` to `min: 0`. `jadpo check` reports `type.divisor_zero`: a divisor's type must exclude zero.
  <!-- try {"file": "domains/kitchen/actions/fill_trays.jadpo", "replace": "type TrayCapacity = Int {\n    min: 1", "with": "type TrayCapacity = Int {\n    min: 0", "check": ["type.divisor_zero"]} -->
- In `set_oven.jadpo`, delete the `_ -> "golden"` arm in `crust_on`. `jadpo check` reports `type.match_wildcard_required`: numbers can't all be listed, so a `_` arm must catch the rest.
  <!-- try {"file": "domains/kitchen/actions/set_oven.jadpo", "replace": "        _ -> \"golden\"\n", "with": "", "check": ["type.match_wildcard_required"]} -->
- Then read [06-failures](../06-failures/README.md).
