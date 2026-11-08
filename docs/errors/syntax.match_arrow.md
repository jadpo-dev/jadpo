# `syntax.match_arrow`

A `match` arm is written with `=>` instead of `->`.

## Why Jadpo refuses it

`=>` ends a match arm's pattern in Rust, Scala, C# and PHP. Jadpo has one
arrow for "gives this": a `match` arm and a function's result both use `->`,
so a reader only ever has one form to recognise.

## What it looks like

main.jadpo:

```jadpo check project=e-match-arrow expect=syntax.match_arrow
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open => 1
        TicketStatus.closed -> 2
    }
}
```

```text output
error: Jadpo writes `->`, not `=>`
 --> main.jadpo:12:27
   |
12 |         TicketStatus.open => 1
   |                           ^^ write `->`
  reason: `=>` ends a match arm's pattern in Rust, Scala, C# and PHP; a Jadpo arm uses `->`, the same arrow as a result type.
  next: Use `->` instead of `=>` (agent)
  rule: syntax.match_arrow
  help: https://jadpo.dev/docs/errors/syntax.match_arrow

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you wrote `=>` in an arm**, write `->`:

```jadpo fragment
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
```

```jadpo check project=e-match-arrow-fix hidden
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

Each arm now gives the rank for the status it names.

## Related

- [Match](../language/03-logic.md#match)
- [Habits from other languages](../language/03-logic.md#habits-from-other-languages)
