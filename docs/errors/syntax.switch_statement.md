# `syntax.switch_statement`

A choice between cases is written with `switch`, `when`, `case` or `default` instead of `match`.

## Why Jadpo refuses it

`switch` chooses between cases in JavaScript, Java, C#, Go and Swift, and
`when` in Kotlin. Jadpo has one form for that, `match`, with one arm per line
written `pattern -> result`, and it has to cover every case. Arms end at their
result, so nothing can fall through to the next, and `_` stands for the rest
only where the cases can't be listed.

## What it looks like

main.jadpo:

```jadpo check project=e-habit-switch expect=syntax.switch_statement
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return switch (status) {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

```text output
error: Jadpo writes `match`, not `switch`
 --> main.jadpo:11:12
   |
11 |     return switch (status) {
   |            ^^^^^^ write `match`
  reason: `switch` chooses between cases in JavaScript, Java, C#, Go and Swift, and `when` in Kotlin; Jadpo's `match` does, with one arm per line, `pattern -> result`, covering every case.
  next: Use `match` instead of `switch` (agent)
  rule: syntax.switch_statement
  help: https://jadpo.dev/docs/errors/syntax.switch_statement

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

`case` before an arm is the same error, with its own message:

main.jadpo:

```jadpo check project=e-habit-switch-case expect=syntax.switch_statement
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        case TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

```text output
error: A Jadpo `match` arm has no `case`
 --> main.jadpo:12:9
   |
12 |         case TicketStatus.open -> 1
   |         ^^^^ remove `case`
  reason: `case` starts an arm of a `switch` in JavaScript, Java, C#, Go and Swift; a Jadpo `match` arm is `pattern -> result`, and ends at its result, with no `break`.
  next: Write the arm as `TicketStatus.open -> 1` (agent)
  rule: syntax.switch_statement
  help: https://jadpo.dev/docs/errors/syntax.switch_statement

check failed: 1 error, 1 file
```

So is `default`, or `else` in Kotlin's `when`:

main.jadpo:

```jadpo check project=e-habit-switch-default expect=syntax.switch_statement
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open -> 1
        default -> 2
    }
}
```

```text output
error: Jadpo writes `_ -> 2`, not `default ->`
 --> main.jadpo:13:9
   |
13 |         default -> 2
   |         ^^^^^^^^^^ write `_ -> 2`
  reason: `default` is a `switch`'s catch-all in JavaScript, Java, C#, Go and Swift, and `else` a `when`'s in Kotlin; Jadpo's is `_`, used only where the cases can't be listed.
  next: Write `_ -> 2` (agent)
  rule: syntax.switch_statement
  help: https://jadpo.dev/docs/errors/syntax.switch_statement

check failed: 1 error, 1 file
```

## How to fix it

**If you wrote `switch`, `when` or `case`**, write `match`, with each arm
as `pattern -> result`:

```jadpo fragment
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
```

```jadpo check project=e-habit-switch-fix-1 hidden
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

`rank` now gives one result for each status, and adding a status later is an
error here until it has an arm.

**If the arm is a catch-all over an enum**, give each case its own arm, as
`_` is only for values whose cases can't be listed:

```jadpo fragment
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
```

```jadpo check project=e-habit-switch-fix-3 hidden
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

Each case now has an arm of its own.

## Related

- [Match](../language/03-logic.md#match)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
