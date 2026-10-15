# 04 · Lists and loops

Collections of values, walking them with `for`, and the standard
library's `collection` operations. The project is a recipe book.

Collections hold many values of one type: a `List` keeps order and
repeats, a `Set` holds each value once in no order, and a `Map` holds
values by key. A list is written in square brackets; `for` walks it; the
standard library's `collection` operations count, search, change and
filter it. So far `for` and those operations take only lists.

## What it teaches

1. **Collections as types.** `List` fields holding objects, in `types/`. See `domains/baking/types/recipe.jadpo` and [area 2, Collections](../../docs/language/02-values-and-types.md#collections).
2. **`for` and `var mut`.** Totals built up in a loop, `return` from inside one, and `collection.count`. See `actions/weigh_recipe.jadpo` and [area 3, Loops](../../docs/language/03-logic.md#loops).
3. **List literals.** Square brackets, items taking the list's element type. See `actions/weigh_recipe.test.jadpo`.
4. **Inline functions.** `collection.map` and `collection.filter` with `item -> expression`. See `actions/scale_recipe.jadpo` and [area 3, Passing functions](../../docs/language/03-logic.md#passing-functions).
5. **Searching and sets.** `collection.contains`, an `else if` chain, and a `Set` returned and compared without order, with what sets can't do yet. See `actions/check_allergens.jadpo` and [area 2, Collections](../../docs/language/02-values-and-types.md#collections).
6. **Named collections.** `ShoppingList(every_name)` builds a named set type from a list whose items already are its item type, keeping each value once. See `actions/shopping_list.jadpo` and [area 2, Named collection construction](../../docs/language/02-values-and-types.md#named-collection-construction).

The other types in `domains/baking/types/` are short; read them as `recipe.jadpo` mentions them. Each action's tests sit beside it, in `<file>.test.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/04-lists-and-loops
target/release/jadpo test examples/04-lists-and-loops
target/release/jadpo test examples/04-lists-and-loops/domains/baking/actions/weigh_recipe.test.jadpo:6
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
  <!-- try {"file": "domains/baking/actions/weigh_recipe.jadpo", "replace": "var mut total", "with": "var total", "check": ["type.assign_immutable"]} -->
- In `weigh_recipe.test.jadpo`, change `== 860` to `== 861`. `jadpo test` fails with `test.assertion_failed`, showing each side's value and the command that reruns just that test.
  <!-- try {"file": "domains/baking/actions/weigh_recipe.test.jadpo", "replace": "== 860", "with": "== 861", "test": ["test.assertion_failed"]} -->
- In `shopping_list.jadpo`, give `ShoppingList` a rule: change `Set<IngredientName> {}` to `Set<IngredientName> { min_length: 1 }`, on three lines. `jadpo check` reports `failure.attempt_required`: a recipe with no ingredients would break it, so building one can now fail; [06-failures](../06-failures/README.md) shows how a construction that can fail is handled.
  <!-- try {"file": "domains/baking/actions/shopping_list.jadpo", "replace": "Set<IngredientName> {}", "with": "Set<IngredientName> {\n    min_length: 1\n}", "check": ["failure.attempt_required"]} -->
- Then read [05-logic](../05-logic/README.md).
