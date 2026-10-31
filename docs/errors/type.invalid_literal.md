# `type.invalid_literal`

A literal value breaks a rule of the type it goes into, such as a number below the type's `min`.

## Why Jadpo refuses it

A literal takes the type it goes into, whether it is returned, assigned,
passed or written as `Quantity(0)`. Its value is known while compiling,
so Jadpo checks it against every rule of that type then, rather than
leave a value that can never be valid to fail when the code runs.

## What it looks like

main.jadpo:

```jadpo check project=e-literal expect=type.invalid_literal
type Quantity = Int {
    min: 1
    max: 1000
}

function starting_quantity() -> Quantity {
    return 0
}
```

```text output
error: `0` isn't a valid `Quantity`
 --> main.jadpo:7:12
  |
7 |     return 0
  |            ^ breaks `min: 1`
  reason: `Quantity` declares `min: 1`, and the value is below 1. A literal is checked against every rule while compiling.
  next: Use a value that `Quantity` allows (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value is wrong**, use one the type allows:

```jadpo fragment
    return 1
```

```jadpo check project=e-literal-fix-1 hidden
type Quantity = Int {
    min: 1
    max: 1000
}

function starting_quantity() -> Quantity {
    return 1
}
```

`starting_quantity` now returns 1, the smallest `Quantity`.

**If the value is right and the rule is wrong**, change the type's rule:

```jadpo fragment
    min: 0
```

```jadpo check project=e-literal-fix-2 hidden
type Quantity = Int {
    min: 0
    max: 1000
}

function starting_quantity() -> Quantity {
    return 0
}
```

Every `Quantity` may now be 0, not only this one, so check the other
places that rely on a quantity of at least 1.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Constraints](../language/02-values-and-types.md#constraints)
- [Named types](../language/02-values-and-types.md#named-types)
