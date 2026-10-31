# `effect.recursive_action`

An action calls itself, or calls other actions that end up calling it again.

## Why Jadpo refuses it

Only functions may call themselves. A function only works out a value, so
calling itself again is how it walks a thread of replies or counts down.
An action reads and changes stored data and calls outside services, and
each pass round a cycle does all of that again, with nothing in the data
saying how many times. A cycle with no end never finishes.

Jadpo won't choose the fix for you, because the two fixes do different
things: one keeps the repeated work but takes the effects out of it, and
the other runs the effects once for each item of a list you give.

## What it looks like

main.jadpo:

```jadpo check project=e-recursive-action expect=effect.recursive_action
type Count = Int {
    min: 0
}

action ping(count: Count) -> Unit {
    pong(count)
}

action pong(count: Count) -> Unit {
    ping(count)
}
```

```text output
error: Action `pong` is in a call cycle: `ping` → `pong` → `ping`
 --> main.jadpo:10:5
   |
10 |     ping(count)
   |     ^^^^ calls back round
  reason: Only functions may call themselves; an action in a call cycle repeats its effects with no data reason, and an unbounded one never finishes.
  next: Move the repeated logic into a function, or loop over a list with `for` (human)
  rule: effect.recursive_action
  help: https://jadpo.dev/docs/errors/effect.recursive_action

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides how many times the action's reads and writes happen.

The error is at the call that closes the cycle. An action that calls
itself directly is reported the same way, as "Action `ping` calls itself".

## How to fix it

The question is what repeats: working out a value, or the action's effects.

**If what repeats is working out a value**, move it into a function, which
may call itself. Here `countdown` steps down to zero, and `ping` calls it
once:

```jadpo fragment
function countdown(count: Count) -> Count {
    if (count == 0) {
        return 0
    }
    return countdown(Count(count - 1))
}
```

```jadpo check project=e-recursive-action-fix-1 hidden
type Count = Int {
    min: 0
}

function countdown(count: Count) -> Count {
    if (count == 0) {
        return 0
    }
    return countdown(Count(count - 1))
}

action ping(count: Count) -> Unit {
    var rest: Count = countdown(count)
    pong(rest)
}

action pong(count: Count) -> Unit {}
```

The action's effects now happen once, and only the function repeats.
`pong` no longer calls `ping`, so there is no cycle. How deep a function
can call itself is limited while the app runs
([Recursion](../language/03-logic.md#recursion)).

**If the action should run once for each item**, give it the list and loop
over it with `for`:

```jadpo fragment
action ping(counts: List<Count>) -> Unit {
    for count in counts {
        pong(count)
    }
}
```

```jadpo check project=e-recursive-action-fix-2 hidden
type Count = Int {
    min: 0
}

action ping(counts: List<Count>) -> Unit {
    for count in counts {
        pong(count)
    }
}

action pong(count: Count) -> Unit {}
```

`pong` runs once for each count in the list, so the list's length says how
many times. A call that leaves your app, such as a provider operation,
still can't be made in the loop's body
([No outside calls inside a loop](../language/03-logic.md#no-outside-calls-inside-a-loop)).

## Related

- [Recursion](../language/03-logic.md#recursion)
- [Functions and actions](../language/03-logic.md#functions-and-actions)
- [Loops](../language/03-logic.md#loops)
