# `failure.propagate_placement`

A `propagate` isn't the result of a failure arm of a `match` on a call.

## Why Jadpo refuses it

`propagate` passes on the failure its arm names, to the caller's `fails` list.
Anywhere else there is no failure for it to pass on, and a reader couldn't
tell what it stopped for.

## What it looks like

main.jadpo:

```jadpo check project=e-propagate expect=failure.propagate_placement
type Count = Int {}

type TicketStatus = Enum {
    open
    closed
}

function priority(status: TicketStatus) -> Count {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> propagate
    }
}
```

```text output
error: `propagate` has no failure to pass on here
 --> main.jadpo:11:32
   |
11 |         TicketStatus.closed -> propagate
   |                                ^^^^^^^^^ not in a failure arm
  reason: `propagate` is an arm of a `match` on a call that can fail: it passes the failure that arm names on to the caller's `fails` list.
  next: Give a value, `return` or `reject` a failure here instead (agent)
  rule: failure.propagate_placement
  help: https://jadpo.dev/docs/errors/failure.propagate_placement

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

It can't be part of a value either:

main.jadpo:

```jadpo check project=e-propagate-expression expect=failure.propagate_placement
type Count = Int {}

function capped(count: Count) -> Count {
    var checked: Count = propagate
    return checked
}
```

```text output
error: `propagate` can only start a statement
 --> main.jadpo:4:26
  |
4 |     var checked: Count = propagate
  |                          ^^^^^^^^^ inside an expression
  reason: Every place a callable can stop early is at the start of a line: `propagate` is a statement of its own, or the result of a `match` arm, never part of a value.
  next: Write `propagate` as the result of a failure arm of a `match` (agent)
  rule: failure.propagate_placement
  help: https://jadpo.dev/docs/errors/failure.propagate_placement

check failed: 1 error, 1 file
```

## How to fix it

**If the arm should stop the callable**, `reject` a failure:

```jadpo fragment
function priority(status: TicketStatus) fails TicketClosed -> Count {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> reject TicketClosed
    }
}
```

```jadpo check project=e-propagate-fix-1 hidden
type Count = Int {}

type TicketStatus = Enum {
    open
    closed
}

type TicketClosed = Failure {
    kind: FailureKind.rejected
}

function priority(status: TicketStatus) fails TicketClosed -> Count {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> reject TicketClosed
    }
}
```

A closed ticket now ends `priority` with `TicketClosed`.

**If the arm has a value to give**, give it:

```jadpo fragment
function priority(status: TicketStatus) -> Count {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

```jadpo check project=e-propagate-fix-2 hidden
type Count = Int {}

type TicketStatus = Enum {
    open
    closed
}

function priority(status: TicketStatus) -> Count {
    return match status {
        TicketStatus.open -> 1
        TicketStatus.closed -> 2
    }
}
```

A closed ticket now has priority 2.

## Related

- [Handling a failure in `match`](../language/04-failures.md#handling-a-failure-in-match)
- [Match](../language/03-logic.md#match)
- [Passing a failure on: `attempt`](../language/04-failures.md#passing-a-failure-on-attempt)
