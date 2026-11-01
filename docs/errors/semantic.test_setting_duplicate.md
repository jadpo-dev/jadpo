# `semantic.test_setting_duplicate`

A test gives one of its settings, `clock:`, `config:`, `fakes:` or `flags:`, twice.

## Why Jadpo refuses it

A test's settings at the top of its body shape everything after them:
the instant its clock starts at, the configuration values it changes,
and the answers its fakes give. Each is written once, so a reader sees
the one value the result depends on. Written twice, one would be
ignored, and the test might pass or fail for a reason its reader never
sees.

## What it looks like

main.jadpo:

```jadpo check project=e-test-setting-duplicate expect=semantic.test_setting_duplicate
type Memo = Text {
    max_length: 200
}

type Reminder = Output {
    text: Memo
    due_at: Instant
}

action remind_in_an_hour(note: Memo) -> Reminder {
    return Reminder {
        text: note
        due_at: clock.now + 1h
    }
}

policy {
    actions: {
        remind_in_an_hour: [Access.public]
    }
}

test "a reminder is due an hour from now" {
    clock: "2026-01-15T12:00:00Z"
    clock: "2026-01-16T09:00:00Z"

    var reminder: Reminder = remind_in_an_hour("Call the plumber")
    assert reminder.due_at == "2026-01-15T13:00:00Z"
}
```

```text output
error: `clock:` is given twice
 --> main.jadpo:25:5
   |
24 |     clock: "2026-01-15T12:00:00Z"
   |     ----- first given here
25 |     clock: "2026-01-16T09:00:00Z"
   |     ^^^^^ given again
  reason: A test writes each of its settings once, so a reader sees the one value that shapes the result.
  next: Remove one `clock:` (agent)
  rule: semantic.test_setting_duplicate
  help: https://jadpo.dev/docs/errors/semantic.test_setting_duplicate

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If one `clock:` is a leftover**, remove it, keeping the instant the
assertions are written against. In `main.jadpo`:

```jadpo fragment
test "a reminder is due an hour from now" {
    clock: "2026-01-15T12:00:00Z"
    ...
}
```

```jadpo check project=e-test-setting-duplicate-fix-1 hidden
type Memo = Text {
    max_length: 200
}

type Reminder = Output {
    text: Memo
    due_at: Instant
}

action remind_in_an_hour(note: Memo) -> Reminder {
    return Reminder {
        text: note
        due_at: clock.now + 1h
    }
}

policy {
    actions: {
        remind_in_an_hour: [Access.public]
    }
}

test "a reminder is due an hour from now" {
    clock: "2026-01-15T12:00:00Z"

    var reminder: Reminder = remind_in_an_hour("Call the plumber")
    assert reminder.due_at == "2026-01-15T13:00:00Z"
}
```

The test now starts at the one instant it shows, and the reminder is due
an hour after it.

A `config:` or `fakes:` given twice is fixed the same way, by putting
every entry in one block: one `config:` holding each setting the test
changes, and one `fakes:` holding each operation's answers. A test that
needs two instants keeps one `clock:` and moves the clock on with
`clock.advance(...)` where it needs the later one.

## Related

- [Test settings](../language/13-testing.md#test-settings)
- [Controlling time](../language/13-testing.md#controlling-time)
