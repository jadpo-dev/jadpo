# `type.assign_immutable`

```jadpo check project=e-assign-immutable expect=type.assign_immutable
type Choice = Text {}

function replace(initial: Choice, replacement: Choice) -> Choice {
    var fixed: Choice = initial
    fixed = replacement
    return fixed
}
```

```text output
error: `fixed` can't be changed
 --> main.jadpo:5:5
  |
4 |     var fixed: Choice = initial
  |         ----- declared without `mut`
5 |     fixed = replacement
  |     ^^^^^ assigned here
  reason: `var` names a value that never changes; only a `var mut` can be pointed at a new value.
  next: Declare it `var mut fixed` (agent)
  rule: type.assign_immutable
  help: docs/errors/type.assign_immutable.md

check failed: 1 error, 1 file
```
