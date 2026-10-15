# 02 · Named types

Every built-in representation Jadpo has so far, the rules a type can add
to it, and how values move between types. The project is a bakery's
pantry; each action shows one family of built-in types.

The rule behind Jadpo's types: representation is not meaning. A weight
and a temperature are both whole numbers underneath, but they mean
different things, so they are different types, and the compiler never
converts between them silently. A value becomes a type only by being
checked against that type's rules.

## What it teaches

1. **A type two files share.** `domains/pantry/types/grams.jadpo` lives in `types/` because two actions use it. [Area 1, Sharing and promotion](../../docs/language/01-project-layout-and-modules.md#sharing-and-promotion).
2. **Whole numbers and `Bool`.** `min`, `max`, negative values, a meaning given to a yes-or-no answer, and a construction the compiler proves safe. See `actions/weigh_flour.jadpo` and [area 2, Constraints](../../docs/language/02-values-and-types.md#constraints).
3. **Exact decimals.** `scale`, decimal literals, and `/` between whole numbers giving a count in another unit, which gets a type of its own. See `actions/price_flour.jadpo` and [area 2, Built-in types](../../docs/language/02-values-and-types.md#built-in-types).
4. **Text.** Lengths, a whole-text `pattern`, refinements and widening, joining with `+`, and string escapes. See `actions/label_jar.jadpo` and [area 2, Named types](../../docs/language/02-values-and-types.md#named-types).
5. **Time.** `Instant`, `CalendarDate` and `Duration` with bounds, and two moments with different meanings compared through the type both are built on. See `actions/check_delivery.jadpo` and [area 2, Built-in types](../../docs/language/02-values-and-types.md#built-in-types).
6. **Identities, contacts and links.** `Uuid` with a `version`, `Email`, `Url`, `IpAddress` and `format: TextFormat.email`, each answer about them a yes or no of its own type. See `actions/check_note.jadpo` and [area 2, Built-in types](../../docs/language/02-values-and-types.md#built-in-types).
7. **Signatures use named types.** No parameter or result is a bare `Int` or `Bool`, and each yes-or-no answer has a type saying what it means. Every file; [area 2, Signatures use named types](../../docs/language/02-values-and-types.md#signatures-use-named-types).

Each action's tests sit beside it, in `<file>.test.jadpo`.

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
  <!-- try {"file": "domains/pantry/actions/label_jar.test.jadpo", "replace": "\"RY-042\"", "with": "\"ry-042\"", "check": ["type.invalid_literal"]} -->
- In `weigh_flour.jadpo`, change `total_weight(each, sacks)` to `total_weight(temperature, sacks)`. `jadpo check` reports `type.mismatch`: a `Celsius` is never a `Grams`, though both are whole numbers.
  <!-- try {"file": "domains/pantry/actions/weigh_flour.jadpo", "replace": "total_weight(each, sacks)", "with": "total_weight(temperature, sacks)", "check": ["type.mismatch"]} -->
- In `label_jar.jadpo`, change `ShelfName`'s `max_length: 12` to `max_length: 50`. `jadpo check` reports `type.constraint_loosened`: a refinement can only tighten what it builds on, and `JarName` allows at most 40.
  <!-- try {"file": "domains/pantry/actions/label_jar.jadpo", "replace": "    max_length: 12", "with": "    max_length: 50", "check": ["type.constraint_loosened"]} -->
- In `check_delivery.jadpo`, change `type Cutoff = ShopTime {}` to `type Cutoff = Instant {}`. `jadpo check` reports `type.ordering_operand` at `arrived < cutoff`: the two no longer share a named type to compare as.
  <!-- try {"file": "domains/pantry/actions/check_delivery.jadpo", "replace": "type Cutoff = ShopTime {}", "with": "type Cutoff = Instant {}", "check": ["type.ordering_operand"]} -->
- Then read [03-objects-and-enums](../03-objects-and-enums/README.md).
