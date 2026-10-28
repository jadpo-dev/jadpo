# `failure.propagate_placement`

`propagate` passes a failure arm's failure on, so it has no meaning anywhere else:

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
  help: docs/language/03-logic.html#failure-propagate-placement

check failed: 1 error, 1 file
```
