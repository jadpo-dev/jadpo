# 08 · Testing

How tests are written and run: kinds of test, shared test data, and
assertions about failures. The project is a bakery's loyalty card: ten
stamps earn a free coffee.

A test runs the real application; only the outside world and the clock
are ever replaced, and nothing in this example needs either. There are
no mocks of your own code. Tests sit beside what they test, shared test
data comes from fixtures, and a failure assertion names the failure it
expects.

## What it teaches

1. **Tests beside their subject.** `<file>.test.jadpo` next to each action. See `domains/loyalty/actions/` and [area 13, Where tests live](../../docs/language/13-testing.md#where-tests-live).
2. **Kinds of test.** A test that calls only functions is pure; one that calls an action or a fixture is an action test. See `actions/stamp_card.test.jadpo` and [area 13, Kinds of test](../../docs/language/13-testing.md#kinds-of-test).
3. **Fixtures.** Shared test data in `fixtures/`, one fixture calling another and an action. See `domains/loyalty/fixtures/` and [area 13, Shared test data: fixtures](../../docs/language/13-testing.md#shared-test-data-fixtures).
4. **Asserting failures.** `assert call(...) fails Failure`, and `attempt` in a test. See `actions/stamp_card.test.jadpo` and [area 13, Assertions](../../docs/language/13-testing.md#assertions).
5. **Reading a failed test.** What `jadpo test` reports and how to rerun one test. See `actions/redeem_card.test.jadpo` and [area 13, What a failed test reports](../../docs/language/13-testing.md#what-a-failed-test-reports).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/07-testing
target/release/jadpo test examples/07-testing
target/release/jadpo test examples/07-testing/domains/loyalty/actions/stamp_card.test.jadpo:18
target/release/jadpo test examples/07-testing --watch
target/release/jadpo fmt --check examples/07-testing
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

- In `stamp_card.test.jadpo`, change `fails CardFull` to `fails CardNotFull`. `jadpo test` fails with `test.assertion_failed` and shows the failure that happened instead.
  <!-- try {"file": "domains/loyalty/actions/stamp_card.test.jadpo", "replace": "fails CardFull", "with": "fails CardNotFull", "test": ["test.assertion_failed"]} -->
- Run `target/release/jadpo test examples/07-testing --kind pure`, then `--seed 7`: one kind of test, then the same tests in another order.
- Then read [08-domains-and-sharing](../08-domains-and-sharing/README.md).
