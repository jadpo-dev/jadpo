# `test.failed`

A test failed under `jadpo test`, or among a package's own tests under `jadpo package check`.

## Why Jadpo refuses it

A test says what the code should do, so one that fails means the code
and the test disagree. The run fails, exiting 1, so CI and a coding
agent never read a failed run as a pass. Its screen reports each
failure under its own rule, such as `test.assertion_failed`, with the
values in the assertion and the command that reruns it; `test.failed`
is the run's own, in `--json`'s `diagnostics`, beside each test's report
in `data.tests`. Jadpo doesn't decide which side is wrong: that is what
the code is for.

## What it looks like

A project whose library says when a todo is overdue, with one test:

```jadpo check project=e-test-failed file=app.jadpo hidden
application {}
```

```jadpo check project=e-test-failed file=domains/planning/types/due_at.jadpo hidden
type DueAt = Instant {}
```

```jadpo check project=e-test-failed file=domains/planning/types/overdue.jadpo hidden
type Overdue = Bool {}
```

```jadpo check project=e-test-failed file=domains/planning/libraries/due_dates.jadpo hidden
use planning/types { DueAt, Overdue }

library due_dates {
    /// Whether a todo due at `due` is overdue at `now`.
    function is_overdue(due: DueAt, now: DueAt) -> Overdue {
        return Overdue(due < now)
    }
}
```

```jadpo check project=e-test-failed file=domains/planning/actions/remind_if_overdue.jadpo hidden
use planning/types { DueAt, Overdue }
use planning/libraries { due_dates }

action remind_if_overdue(due: DueAt, now: DueAt) -> Overdue {
    return due_dates.is_overdue(due, now)
}
```

domains/planning/libraries/due_dates.test.jadpo:

```jadpo check project=e-test-failed file=domains/planning/libraries/due_dates.test.jadpo
use planning/types { DueAt }

test "a todo due yesterday is overdue" {
    var due: DueAt = "2026-01-14T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}
```

A new test says a todo is overdue at its due time, where the code says
only after it. The output below was copied from a run rather than
checked, since each run's order seed is chosen at random:

```sh
cat >> domains/planning/libraries/due_dates.test.jadpo <<'EOF2'

test "a todo is overdue once its due time comes" {
    var due: DueAt = "2026-01-15T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}
EOF2
jadpo test
```

```text
FAIL  a todo is overdue once its due time comes
 --> domains/planning/libraries/due_dates.test.jadpo:12

  assert due_dates.is_overdue(due, now)
         |                    |    |
         false                |    "2026-01-15T12:00:00Z"
                              "2026-01-15T12:00:00Z"

  rule: test.assertion_failed
  rerun: jadpo test domains/planning/libraries/due_dates.test.jadpo:12 --seed 97309

test failed: 1 failed, 1 passed, 2 pure tests
```

With `--json`, the same run's `diagnostics` hold `test.failed`: "1 of 2
tests failed", whose next step is "Fix each failure, then run `jadpo
test --failed`", marked `agent`. `(agent)` at the end of the `next:`
line means your coding agent can make the fix itself.

## How to fix it

The question is which is right, the test or the code.

**If the code is right**, and a todo is overdue only after its due
time, correct the test to say so:

```jadpo fragment
test "a todo is overdue once its due time has passed" {
    var due: DueAt = "2026-01-15T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:01Z"
    assert due_dates.is_overdue(due, now)
}
```

```jadpo check project=e-test-failed-fixed file=domains/planning/libraries/due_dates.test.jadpo hidden
use planning/types { DueAt }

test "a todo due yesterday is overdue" {
    var due: DueAt = "2026-01-14T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}

test "a todo is overdue once its due time has passed" {
    var due: DueAt = "2026-01-15T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:01Z"
    assert due_dates.is_overdue(due, now)
}
```

```jadpo check project=e-test-failed-fixed file=app.jadpo hidden
application {}
```

```jadpo check project=e-test-failed-fixed file=domains/planning/types/due_at.jadpo hidden
type DueAt = Instant {}
```

```jadpo check project=e-test-failed-fixed file=domains/planning/types/overdue.jadpo hidden
type Overdue = Bool {}
```

```jadpo check project=e-test-failed-fixed file=domains/planning/libraries/due_dates.jadpo hidden
use planning/types { DueAt, Overdue }

library due_dates {
    /// Whether a todo due at `due` is overdue at `now`.
    function is_overdue(due: DueAt, now: DueAt) -> Overdue {
        return Overdue(due < now)
    }
}
```

```jadpo check project=e-test-failed-fixed file=domains/planning/actions/remind_if_overdue.jadpo hidden
use planning/types { DueAt, Overdue }
use planning/libraries { due_dates }

action remind_if_overdue(due: DueAt, now: DueAt) -> Overdue {
    return due_dates.is_overdue(due, now)
}
```

Then rerun what failed:

```sh
jadpo test --failed
```

Both tests pass.

**If the test is right**, and a todo is overdue from its due time on,
change the code:

```jadpo fragment
library due_dates {
    ...
    function is_overdue(due: DueAt, now: DueAt) -> Overdue {
        return Overdue(due <= now)
    }
}
```

```jadpo check project=e-test-failed-code file=domains/planning/libraries/due_dates.jadpo hidden
use planning/types { DueAt, Overdue }

library due_dates {
    /// Whether a todo due at `due` is overdue at `now`.
    function is_overdue(due: DueAt, now: DueAt) -> Overdue {
        return Overdue(due <= now)
    }
}
```

```jadpo check project=e-test-failed-code file=domains/planning/libraries/due_dates.test.jadpo hidden
use planning/types { DueAt }

test "a todo due yesterday is overdue" {
    var due: DueAt = "2026-01-14T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}

test "a todo is overdue once its due time comes" {
    var due: DueAt = "2026-01-15T12:00:00Z"
    var now: DueAt = "2026-01-15T12:00:00Z"
    assert due_dates.is_overdue(due, now)
}
```

```jadpo check project=e-test-failed-code file=app.jadpo hidden
application {}
```

```jadpo check project=e-test-failed-code file=domains/planning/types/due_at.jadpo hidden
type DueAt = Instant {}
```

```jadpo check project=e-test-failed-code file=domains/planning/types/overdue.jadpo hidden
type Overdue = Bool {}
```

```jadpo check project=e-test-failed-code file=domains/planning/actions/remind_if_overdue.jadpo hidden
use planning/types { DueAt, Overdue }
use planning/libraries { due_dates }

action remind_if_overdue(due: DueAt, now: DueAt) -> Overdue {
    return due_dates.is_overdue(due, now)
}
```

`jadpo test --failed` then passes, and every reminder that checks
`is_overdue` goes out at the due time rather than after it.

## Related

- [`jadpo test`](../language/14-tooling.md#jadpo-test)
- [Running tests](../language/14-tooling.md#running-tests)
- [Writing a test](../language/13-testing.md#writing-a-test)
