# `type.assign_unknown`

```jadpo check project=e-assign-unknown expect=type.assign_unknown
type Count = Int {}

function total(count: Count) -> Count {
    running = count
    return count
}
```

```text output
error: `running` isn't declared
 --> main.jadpo:4:5
  |
4 |     running = count
  |     ^^^^^^^ not declared
  reason: An assignment changes a `var mut` declared earlier in this block or one around it.
  next: Declare it first, as in `var mut running: Count = ...` (agent)
  rule: type.assign_unknown
  help: docs/errors/type.assign_unknown.md

check failed: 1 error, 1 file
```
