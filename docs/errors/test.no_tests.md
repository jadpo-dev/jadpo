# `test.no_tests`

A `jadpo test` command found no test to run: the project has none, or none matches the path, line, name, kind or `--failed` it gives.

## Why Jadpo refuses it

A run of no tests would pass, and "tested: 0 passed" reads like a green
run. A misspelt `--name`, a file that holds no test, or `--failed`
before any run would then look like success while nothing was checked.
So a `jadpo test` that selects nothing fails, and says what the
selection left out.

## What it looks like

A project with one test file:

```jadpo check project=e-no-tests file=app.jadpo hidden
application {}
```

```jadpo check project=e-no-tests file=domains/planning/types/due_at.jadpo hidden
type DueAt = Instant {}
```

```jadpo check project=e-no-tests file=domains/planning/types/overdue.jadpo hidden
type Overdue = Bool {}
```

```jadpo check project=e-no-tests file=domains/planning/libraries/due_dates.jadpo hidden
use planning/types { DueAt, Overdue }

library due_dates {
    /// Whether a todo due at `due` is overdue at `now`.
    function is_overdue(due: DueAt, now: DueAt) -> Overdue {
        return Overdue(due < now)
    }
}
```

```jadpo check project=e-no-tests file=domains/planning/actions/remind_if_overdue.jadpo hidden
use planning/types { DueAt, Overdue }
use planning/libraries { due_dates }

action remind_if_overdue(due: DueAt, now: DueAt) -> Overdue {
    return due_dates.is_overdue(due, now)
}
```

domains/planning/libraries/due_dates.test.jadpo:

```jadpo check project=e-no-tests file=domains/planning/libraries/due_dates.test.jadpo
use planning/types { DueAt }

test "a todo due yesterday is overdue" {
    var due: DueAt = "2026-01-14T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}
```

No test's name has the text `--name` gives:

```sh run project=e-no-tests expect=test.no_tests
jadpo test --name tomorrow
```

```text output
error: No test's name contains `tomorrow`
  reason: The one test's name doesn't contain this text.
  next: Check the spelling, or leave out `--name` to run every test (agent)
  rule: test.no_tests
  help: https://jadpo.dev/docs/errors/test.no_tests
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

`--failed` before any run has no failures to rerun:

```sh run project=e-no-tests expect=test.no_tests
jadpo test --failed
```

```text output
error: There's no last run to rerun
  reason: `jadpo test` keeps the failures of each run in `build/test/last-run.json`, and there isn't one yet.
  next: Run `jadpo test` first, then `jadpo test --failed` (agent)
  rule: test.no_tests
  help: https://jadpo.dev/docs/errors/test.no_tests
```

A path, a line or a `--kind` that holds no test is refused the same
way, with where the tests are.

## How to fix it

**If you meant some of the tests**, give text their names hold:

```sh
jadpo test --name overdue
```

The test whose name holds `overdue` runs.

**If you meant every test**, leave the selection out:

```sh
jadpo test
```

Every test in the project runs, and its failures are kept for
`--failed`.

## Related

- [`jadpo test`](../language/14-tooling.md#jadpo-test)
- [Running tests](../language/14-tooling.md#running-tests)
