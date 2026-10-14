# 01 · A first project

The smallest real Jadpo project: one domain, one action and its tests.
Read the files in the order below; the comments explain each part.

## What it teaches

1. **The project root.** `app.jadpo` marks where a project starts, and every project has the same shape. See `app.jadpo` and [area 1, Decided layout](../../docs/language/01-project-layout-and-modules.md#decided-layout).
2. **Domains and files.** Everything besides `app.jadpo` lives in a domain: a business area, named by its folder under `domains/`, one action per file in `actions/`, named after it. See `domains/baking/actions/price_loaves.jadpo` and [area 1, Files and names](../../docs/language/01-project-layout-and-modules.md#files-and-names).
3. **Named types.** `Pence` and `LoafCount` are whole numbers with a meaning and rules. Same file; [area 2, Named types](../../docs/language/02-values-and-types.md#named-types).
4. **Functions and actions.** A pure helper, called by its bare name, and the action that uses it. Same file; [area 3, Functions and actions](../../docs/language/03-logic.md#functions-and-actions).
5. **Constructors and `if`.** `Pence(...)` checks a computed value; a condition is always in brackets. Same file; [area 2, Creating values](../../docs/language/02-values-and-types.md#creating-values) and [area 3, Conditions](../../docs/language/03-logic.md#conditions).
6. **Comments.** `///` describes a declaration; `//` is a note for the reader. Every file.
7. **Tests.** A test sits beside its file and checks with `assert`. See `domains/baking/actions/price_loaves.test.jadpo` and [area 13, Writing a test](../../docs/language/13-testing.md#writing-a-test).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/01-first-project
target/release/jadpo test examples/01-first-project
target/release/jadpo test examples/01-first-project/domains/baking/actions/price_loaves.test.jadpo:15
target/release/jadpo test examples/01-first-project --watch
target/release/jadpo fmt --check examples/01-first-project
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

- In `price_loaves.test.jadpo`, change `price_loaves(250, 1)` to `price_loaves(250, 0)`. `jadpo check` refuses it with `type.invalid_literal`: `LoafCount` starts at 1, and literals are checked while compiling.
- In `price_loaves.jadpo`, change `return Pence(each * loaves)` to `return each * loaves`. `jadpo check` reports `type.mismatch`: arithmetic gives a plain `Int`, and only `Pence(...)` makes it `Pence` again.
- Then read [02-named-types](../02-named-types/README.md).
