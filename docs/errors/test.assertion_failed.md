# `test.assertion_failed`

A test's `assert` doesn't hold: its condition is false, or a call checked with `assert … fails` returned or failed differently.

## Why Jadpo refuses it

An `assert` is the test saying what the code should do. When it doesn't
hold, the test and the code disagree, and only one of them is right.
The report shows each side of the comparison, records field by field
with a diff of what differs, so you can tell which without running the
test again.

## What it looks like

todos.test.jadpo, testing `draft` in todos.jadpo, which makes a todo with
priority 3:

```jadpo check project=e-assertion-failed file=todos.jadpo hidden
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 9
}

type TodoDraft = Object {
    title: TodoTitle
    priority: Priority
}

function draft(title: TodoTitle) -> TodoDraft {
    return TodoDraft {
        title: title
        priority: 3
    }
}
```

```jadpo check project=e-assertion-failed file=todos.test.jadpo expect=test.assertion_failed
test "a new draft has normal priority" {
    var fresh: TodoDraft = draft("Buy milk")
    assert fresh == TodoDraft {
        title: "Buy milk"
        priority: 2
    }
}
```

```text output
FAIL  a new draft has normal priority
 --> todos.test.jadpo:3

  assert fresh == TodoDraft { ...
         |        |
         |        TodoDraft { title: "Buy milk", priority: 2 }
         TodoDraft { title: "Buy milk", priority: 3 }

  expected: TodoDraft { title: "Buy milk", priority: 2 }
  actual: TodoDraft { title: "Buy milk", priority: 3 }
  diff: ~ priority: expected 2, actual 3

  rule: test.assertion_failed
  rerun: jadpo test todos.test.jadpo:3 --seed 1

test failed: 1 failed, 1 pure test
```

A failed test's report has no `next:` line, and the rule is your agent's
to act on. `(agent)` at the end of the `next:` line means your coding
agent can make the fix itself. The `rerun:` line runs this one test
again with the same seed.

## How to fix it

The question is which is right, the test or the code.

**If the code is right**, change what the test expects. In
`todos.test.jadpo`:

```jadpo fragment
        priority: 3
```

```jadpo check project=e-assertion-failed-fix-1 file=todos.test.jadpo hidden
test "a new draft has normal priority" {
    var fresh: TodoDraft = draft("Buy milk")
    assert fresh == TodoDraft {
        title: "Buy milk"
        priority: 3
    }
}
```

```jadpo check project=e-assertion-failed-fix-1 file=todos.jadpo hidden
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 9
}

type TodoDraft = Object {
    title: TodoTitle
    priority: Priority
}

function draft(title: TodoTitle) -> TodoDraft {
    return TodoDraft {
        title: title
        priority: 3
    }
}
```

The test now expects the priority `draft` gives, and passes.

**If the test is right**, change the code. In `todos.jadpo`:

```jadpo fragment
        priority: 2
```

```jadpo check project=e-assertion-failed-fix-2 file=todos.jadpo hidden
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 9
}

type TodoDraft = Object {
    title: TodoTitle
    priority: Priority
}

function draft(title: TodoTitle) -> TodoDraft {
    return TodoDraft {
        title: title
        priority: 2
    }
}
```

```jadpo check project=e-assertion-failed-fix-2 file=todos.test.jadpo hidden
test "a new draft has normal priority" {
    var fresh: TodoDraft = draft("Buy milk")
    assert fresh == TodoDraft {
        title: "Buy milk"
        priority: 2
    }
}
```

Every new draft now has priority 2, and the test passes.

## Related

- [Assertions](../language/13-testing.md#assertions)
- [Comparing values](../language/13-testing.md#comparing-values)
- [What a failed test reports](../language/13-testing.md#what-a-failed-test-reports)
- [Checking that a call fails](../language/13-testing.md#checking-that-a-call-fails)
