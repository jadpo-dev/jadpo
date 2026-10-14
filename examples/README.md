# Jadpo by example

Small, complete Jadpo projects, commented throughout, that together use
every feature the compiler has built so far. They add to the
[language pages](../docs/language/README.md), which stay the
specification: the pages say what each feature means, and these show it
working in a real project layout, with tests that pass.

Read them in order. Each one assumes the ones before it, and each has a
README saying what it teaches, how to run it and what to try next.

1. [01-first-project/](01-first-project/README.md): the project root, a
   domain, a named type, a function, an action and a test.
2. [02-named-types/](02-named-types/README.md): every built-in
   representation, the rules a type can add, refinements, literals and
   conversions.
3. [03-objects-and-enums/](03-objects-and-enums/README.md): objects,
   inputs and outputs, field types, absent values, enums and tagged
   unions, and types built from other types.
4. [04-lists-and-loops/](04-lists-and-loops/README.md): lists and sets,
   `for`, `var mut`, and the `collection` operations with inline
   functions.
5. [05-logic/](05-logic/README.md): operators, `if` chains, every form
   of `match`, division with rounding, and recursion.
6. [06-failures/](06-failures/README.md): declaring failures, `fails`,
   `reject`, `attempt`, checks that can fail, and handling failures with
   `match`.
7. [07-domains-and-sharing/](07-domains-and-sharing/README.md): several
   domains, `public`, `shared/`, libraries, and a scenario in `tests/`.
8. [08-testing/](08-testing/README.md): test kinds, fixtures, and
   asserting failures.
9. [09-bakery-orders/](09-bakery-orders/README.md): a fuller project
   that puts all of it together.

`golden-todo/` is different: it holds the contract and notes for the
golden todo application the compiler is built against, not a teaching
example.

## How the comments work

- `///` above a declaration describes it, as it would in any real
  project: editors and generated documentation show it.
- `//` comments are the lesson: what a part is for and why it is written
  that way, said once, where a reader first meets it. Later examples
  don't repeat what earlier ones taught.
- Each project's `app.jadpo` says what the project is about, and each
  file opens with what it shows.

## Running them

Build `jadpo` once from the repository root:

```sh
cargo build --release -p jadpo-cli
```

Then point it at any example, for instance:

```sh
target/release/jadpo check examples/01-first-project
target/release/jadpo test examples/01-first-project
```

The gate (`python3 tools/verify.py`) checks, formats and tests every
example, and fails when a built feature appears in none of them.
