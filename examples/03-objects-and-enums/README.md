# 03 · Objects and enums

Values with parts: objects, values that arrive from outside and values
sent back, values that may be absent, enums and tagged unions, and types
built from other types. The project takes cake orders.

## What it teaches

1. **Objects and field rules.** `type CakeLine = Object { ... }`, with a field that tightens its type. See `domains/sales/types/cake_line.jadpo` and [area 2, Objects](../../docs/language/02-values-and-types.md#objects).
2. **Inputs, outputs and absent values.** `optional:` and `default:` on an `Input`, `T?` and `none`, and fields that only arrive from outside (`Bytes`, `Time`, `Map`). See `actions/quote_cake.jadpo` and [area 2, Absent values](../../docs/language/02-values-and-types.md#absent-values).
3. **Field types.** `CakeLine.each` is a type of its own. Same file; [area 2, Field types](../../docs/language/02-values-and-types.md#field-types).
4. **Enums and tagged unions.** Plain variants, variants with fields, and reading them with `match`. See `actions/choose_handover.jadpo` and [area 2, Enums](../../docs/language/02-values-and-types.md#enums).
5. **Values never change.** Rebuilding an object, a field used as a constructor, and `==` comparing contents. See `actions/reprice_line.jadpo` and [area 2, Values never change](../../docs/language/02-values-and-types.md#values-never-change).
6. **Types built from other types.** `include`, `pick`, `omit`, a written field replacing a copied one, and `optional: all`. See `actions/summarise_line.jadpo` and [area 2, Decided: building a type from other types](../../docs/language/02-values-and-types.md#decided-building-a-type-from-other-types).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/03-objects-and-enums
target/release/jadpo test examples/03-objects-and-enums
target/release/jadpo test examples/03-objects-and-enums/domains/sales/actions/quote_cake.test.jadpo:22
target/release/jadpo test examples/03-objects-and-enums --watch
target/release/jadpo fmt --check examples/03-objects-and-enums
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

- In `quote_cake.test.jadpo`, delete the line `icing: none` from the expected `CakeQuote`. `jadpo check` reports `type.missing_field`: a nullable field is still required.
- In `quote_cake.jadpo`, add a variant `wedding` to `CakeSize`. `jadpo check` reports `type.match_non_exhaustive` in `size_price`: every `match` must cover every variant.
- Then read [04-lists-and-loops](../04-lists-and-loops/README.md).
