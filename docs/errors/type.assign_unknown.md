# `type.assign_unknown`

An assignment gives a new value to a name that isn't declared in the block it is in or one around it.

## Why Jadpo refuses it

A variable is declared once, with `var` or `var mut` and its type, and
`name = value` only changes one declared earlier. Jadpo never declares a
variable for you on its first assignment, so a misspelt name can't
quietly become a new variable, and every variable says its type.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.assign_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the line meant to name a new value**, declare it with `var` and its
type:

```jadpo fragment
    var running: Count = count
    return running
```

```jadpo check project=e-assign-unknown-fix hidden
type Count = Int {}

function total(count: Count) -> Count {
    var running: Count = count
    return running
}
```

`running` is declared where it is first given a value. A value it is
given again later needs `var mut` instead of `var`.

## Related

- [Variables](../language/03-logic.md#variables)
- [Values never change](../language/02-values-and-types.md#values-never-change)
