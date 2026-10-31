# `type.assign_immutable`

A variable declared without `mut`, or a parameter, is given a new value.

## Why Jadpo refuses it

A `var` never changes, so a reader who sees its line knows its value
everywhere below it. Only a `var mut` can be pointed at a new value, and
the `mut` warns the reader to look for where it changes. Parameters never
change either. Letting a plain `var` change would break what its line
promises.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/type.assign_immutable

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Assigning a parameter is the same error:

main.jadpo:

```jadpo check project=e-assign-parameter expect=type.assign_immutable
type Choice = Text {}

function replace(initial: Choice, replacement: Choice) -> Choice {
    initial = replacement
    return initial
}
```

```text output
error: `initial` can't be changed
 --> main.jadpo:4:5
  |
3 | function replace(initial: Choice, replacement: Choice) -> Choice {
  |                  ------- a parameter
4 |     initial = replacement
  |     ^^^^^^^ assigned here
  reason: Parameters never change; only a `var mut` can be pointed at a new value.
  next: Copy `initial` into a `var mut` with a name of its own, and change that (agent)
  rule: type.assign_immutable
  help: https://jadpo.dev/docs/errors/type.assign_immutable

check failed: 1 error, 1 file
```

## How to fix it

**If the variable is meant to change**, declare it `var mut`:

```jadpo fragment
    var mut fixed: Choice = initial
```

```jadpo check project=e-assign-immutable-fix-1 hidden
type Choice = Text {}

function replace(initial: Choice, replacement: Choice) -> Choice {
    var mut fixed: Choice = initial
    fixed = replacement
    return fixed
}
```

`fixed` starts as `initial` and is then pointed at `replacement`. It can
only ever hold a `Choice`.

**If a parameter is meant to change**, copy it into a `var mut` with a
name of its own, and change that:

```jadpo fragment
    var mut chosen: Choice = initial
    chosen = replacement
    return chosen
```

```jadpo check project=e-assign-parameter-fix hidden
type Choice = Text {}

function replace(initial: Choice, replacement: Choice) -> Choice {
    var mut chosen: Choice = initial
    chosen = replacement
    return chosen
}
```

`initial` keeps the value the caller passed, and `chosen` is the one that
changes.

## Related

- [Variables](../language/03-logic.md#variables)
- [Values never change](../language/02-values-and-types.md#values-never-change)
