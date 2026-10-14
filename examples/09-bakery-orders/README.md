# 09 · A bakery's orders

A fuller project that puts the earlier examples together: pricing a
basket from the menu, with discount codes, across two domains. It uses
shared types and a shared library, an input and an output, tagged
unions, loops, the standard library, failures declared, raised, passed
on and handled, a fixture, and pure, action and scenario tests. Its
comments point out what each part is for rather than re-teaching it;
the earlier examples teach each idea on its own.

## What it teaches

1. **Shared types and a library.** A tagged union for menu items and a `money` library both domains use. See `shared/types/` and `shared/libraries/money.jadpo`.
2. **The menu.** A public action that prices each kind of item with `match`. See `domains/menu/actions/price_item.jadpo`.
3. **The basket and the receipt.** A public `Input` whose list has a length rule and whose code may be left out, and the `Output` sent back. See `domains/sales/types/`.
4. **Pricing.** A loop over the lines calling another domain, a statement `match` on an optional code, a text `match` that rejects unknown codes, and a check whose `min` arm names its failure with fields: `min: DiscountTooLarge { ... }`. See `domains/sales/actions/price_lines.jadpo`, `domains/sales/actions/price_basket.jadpo` and [area 4, Turning one failure into another](../../docs/language/04-failures.md#turning-one-failure-into-another).
5. **Handling failures where they happen.** The till recovers from an unknown code by charging full price, and passes the other failures on with `propagate`. See `domains/sales/actions/till_total.jadpo` and [area 4, Handling a failure in match](../../docs/language/04-failures.md#handling-a-failure-in-match).
6. **Tests at every level.** A fixture, a pure test of `amount_off`, action tests that assert values and failures, and a scenario across both domains. See `domains/sales/fixtures/family_basket.jadpo`, `domains/sales/actions/price_basket.test.jadpo` and `tests/basket_to_receipt.test.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/09-bakery-orders
target/release/jadpo test examples/09-bakery-orders
target/release/jadpo test examples/09-bakery-orders/domains/sales/actions/price_basket.test.jadpo:10
target/release/jadpo test examples/09-bakery-orders --watch
target/release/jadpo fmt --check examples/09-bakery-orders
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

- In `price_item.jadpo`, change the white loaf's price from 250 to 260. Six tests fail with `test.assertion_failed`: every one that counts on a white loaf's price. The scenario still passes, because it asks the menu for the price.
  <!-- try {"file": "domains/menu/actions/price_item.jadpo", "replace": "Grain.white -> 250", "with": "Grain.white -> 260", "test": ["test.assertion_failed"], "failed": 6} -->
- In `price_basket.jadpo`, delete the `_ -> reject CodeUnknown { ... }` arm. `jadpo check` reports `type.match_wildcard_required`, and `failure.stale_declaration` because `CodeUnknown` can no longer happen.
  <!-- try {"file": "domains/sales/actions/price_basket.jadpo", "replace": "        _ -> reject CodeUnknown {\n            code: code\n        }\n", "with": "", "check": ["failure.stale_declaration", "type.match_wildcard_required"]} -->
- Then read [10-library-loans](../10-library-loans/README.md), where records are stored.
