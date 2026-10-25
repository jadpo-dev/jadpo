# 01 · A first project

The smallest real Jadpo project: one domain, two actions, the type they
share and their tests. Read the files in the order below; the comments
explain each part.

## What it teaches

1. **The project root.** `app.jadpo` marks where a project starts, and every project has the same shape. See `app.jadpo` and [area 1, Decided layout](../../docs/language/01-project-layout-and-modules.md#decided-layout).
2. **Domains and files.** Everything besides `app.jadpo` lives in a domain: a business area, named by its folder under `domains/`, one action per file in `actions/`, named after it. See `domains/baking/actions/weigh_loaves.jadpo` and [area 1, Files and names](../../docs/language/01-project-layout-and-modules.md#files-and-names).
3. **Imports.** A file opens with `use` lines saying where each name it uses but doesn't declare lives: `use baking/types { Grams }` is `domains/baking/types/grams.jadpo`. Built-in types need none. Same file; [area 1, Imports](../../docs/language/01-project-layout-and-modules.md#imports).
4. **Named types.** `Grams` and `LoafCount` are whole numbers with a meaning and rules. `LoafCount` lives beside its one user; `Grams`, which two files use, has a file of its own in `types/`. See `domains/baking/types/grams.jadpo` and [area 2, Named types](../../docs/language/02-values-and-types.md#named-types).
5. **Functions and actions.** A pure helper, called by its bare name, and the action that uses it. See `domains/baking/actions/weigh_loaves.jadpo` and [area 3, Functions and actions](../../docs/language/03-logic.md#functions-and-actions).
6. **Constructors and `if`.** `Grams(...)` checks a computed value; a condition is always in brackets. Same file; [area 2, Creating values](../../docs/language/02-values-and-types.md#creating-values) and [area 3, Conditions](../../docs/language/03-logic.md#conditions).
7. **Calling your own domain.** An action in another file of the domain is imported from its folder and called by its bare name. See `domains/baking/actions/usual_order.jadpo`.
8. **Comments.** `///` describes a declaration; `//` is a note for the reader. Every file.
9. **Tests.** A test sits beside its file and checks with `assert`; it sees what its subject declares without importing it. See `domains/baking/actions/weigh_loaves.test.jadpo` and [area 13, Writing a test](../../docs/language/13-testing.md#writing-a-test).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/01-first-project
target/release/jadpo test examples/01-first-project
target/release/jadpo test examples/01-first-project/domains/baking/actions/weigh_loaves.test.jadpo:21
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
  in the one layout `jadpo fmt` writes, with the `use` lines it needs.

## Try next

- In `weigh_loaves.test.jadpo`, change `weigh_loaves(250, 1)` to `weigh_loaves(250, 0)`. `jadpo check` refuses it with `type.invalid_literal`: `LoafCount` starts at 1, and literals are checked while compiling.
  <!-- try {"file": "domains/baking/actions/weigh_loaves.test.jadpo", "replace": "weigh_loaves(250, 1)", "with": "weigh_loaves(250, 0)", "check": ["type.invalid_literal"]} -->
- In `weigh_loaves.jadpo`, change `return Grams(each * loaves)` to `return each * loaves`. `jadpo check` reports `type.mismatch`: arithmetic gives a plain `Int`, and only `Grams(...)` makes it `Grams` again.
  <!-- try {"file": "domains/baking/actions/weigh_loaves.jadpo", "replace": "return Grams(each * loaves)", "with": "return each * loaves", "check": ["type.mismatch"]} -->
- In `usual_order.jadpo`, delete the line `use baking/types { Grams }`. `jadpo check` reports `import.missing` where `Grams` is used, with the edit that adds the line back; `jadpo fmt` adds it.
  <!-- try {"file": "domains/baking/actions/usual_order.jadpo", "replace": "use baking/types { Grams }\n", "with": "", "check": ["import.missing"]} -->
- In `usual_order.jadpo`, change `use baking/actions { weigh_loaves }` to `use baking/actions { weigh_loaves, usual_order }`. `jadpo check` reports `import.unneeded`: a file sees what it declares without importing it.
  <!-- try {"file": "domains/baking/actions/usual_order.jadpo", "replace": "use baking/actions { weigh_loaves }", "with": "use baking/actions { weigh_loaves, usual_order }", "check": ["import.unneeded"]} -->
- In `weigh_loaves.jadpo`, change `var total: Grams` to `let total: Grams`, as JavaScript or Rust would have it. `jadpo check` reports `syntax.variable_keyword`, whose edit writes `var` back; the rest of the project is still checked, since `let` reads as `var`.
  <!-- try {"file": "domains/baking/actions/weigh_loaves.jadpo", "replace": "var total: Grams", "with": "let total: Grams", "check": ["syntax.variable_keyword"]} -->
- Then read [02-named-types](../02-named-types/README.md).
