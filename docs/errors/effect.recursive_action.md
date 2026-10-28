# `effect.recursive_action`

Only functions may call themselves; an action in a call cycle is reported at the call that closes it:

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
  help: docs/language/03-logic.html#effect-recursive-action

check failed: 1 error, 1 file
```
