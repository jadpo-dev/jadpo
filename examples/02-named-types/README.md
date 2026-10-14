# 02 · Named types

Every built-in representation Jadpo has so far, the rules a type can add
to it, and how values move between types. The project is a bakery's
pantry.

## What it teaches

1. **A type two files share.** `domains/pantry/types/grams.jadpo` lives in `types/` because two actions use it. [Area 1, Sharing and promotion](../../docs/language/01-project-layout-and-modules.md#sharing-and-promotion).
2. **Whole numbers and `Bool`.** `min`, `max`, negative values, a meaning given to a yes-or-no answer, and a construction the compiler proves safe. See `actions/weigh_flour.jadpo` and [area 2, Constraints](../../docs/language/02-values-and-types.md#constraints).
3. **Exact decimals.** `scale`, decimal literals and `/` between whole numbers. See `actions/price_flour.jadpo` and [area 2, Built-in types](../../docs/language/02-values-and-types.md#built-in-types).
4. **Text.** Lengths, a whole-text `pattern`, refinements and widening, joining with `+`, and string escapes. See `actions/label_jar.jadpo` and [area 2, Named types](../../docs/language/02-values-and-types.md#named-types).
5. **Identities, contacts, links and time.** `Uuid` with a `version`, `Email`, `Url`, `IpAddress`, `format: TextFormat.email`, `Instant`, `CalendarDate` and `Duration` with bounds. See `actions/check_delivery.jadpo` and [area 2, Built-in types](../../docs/language/02-values-and-types.md#built-in-types).
6. **Signatures use named types.** No parameter or result is a bare `Int` or `Bool`. Every file; [area 2, Signatures use named types](../../docs/language/02-values-and-types.md#signatures-use-named-types).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/02-named-types
target/release/jadpo test examples/02-named-types
target/release/jadpo test examples/02-named-types/domains/pantry/actions/weigh_flour.test.jadpo:8
target/release/jadpo test examples/02-named-types --watch
target/release/jadpo fmt --check examples/02-named-types
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

- In `label_jar.test.jadpo`, change `"RY-042"` to `"ry-042"`. `jadpo check` reports `type.invalid_literal`: the pattern must match the whole text.
- In `weigh_flour.jadpo`, change `total_weight(each, sacks)` to `total_weight(temperature, sacks)`. `jadpo check` reports `type.mismatch`: a `Celsius` is never a `Grams`, though both are whole numbers.
- Then read [03-objects-and-enums](../03-objects-and-enums/README.md).
