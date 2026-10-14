# 04 · Lists and loops

Collections of values, walking them with `for`, and the standard
library's `collection` operations. The project is a recipe book.

## What it teaches

1. **Collections as types.** `List` fields holding objects, in `types/`. See `domains/baking/types/recipe.jadpo` and [area 2, Collections](../../docs/language/02-values-and-types.md#collections).
2. **`for` and `var mut`.** Totals built up in a loop, `return` from inside one, and `collection.count`. See `actions/weigh_recipe.jadpo` and [area 3, Loops](../../docs/language/03-logic.md#loops).
3. **List literals.** Square brackets, items taking the list's element type. See `actions/weigh_recipe.test.jadpo`.
4. **Inline functions.** `collection.map` and `collection.filter` with `item -> expression`. See `actions/scale_recipe.jadpo` and [area 3, Passing functions](../../docs/language/03-logic.md#passing-functions).
5. **Searching and sets.** `collection.contains`, an `else if` chain, and a `Set` returned and compared without order. See `actions/check_allergens.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/04-lists-and-loops
target/release/jadpo test examples/04-lists-and-loops
target/release/jadpo test examples/04-lists-and-loops/domains/baking/actions/weigh_recipe.test.jadpo:4
target/release/jadpo test examples/04-lists-and-loops --watch
target/release/jadpo fmt --check examples/04-lists-and-loops
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

- In `weigh_recipe.jadpo`, change `var mut total` to `var total`. `jadpo check` reports `type.assign_immutable`: only a `var mut` can be assigned.
- In `weigh_recipe.test.jadpo`, change `== 860` to `== 861`. `jadpo test` fails with `test.assertion_failed`, showing each side's value and the command that reruns just that test.
- Then read [05-logic](../05-logic/README.md).
