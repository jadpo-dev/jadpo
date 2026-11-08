# `type.constructor_argument_count`

A type's constructor, such as `Quantity(...)`, is given more or fewer than one value.

## Why Jadpo refuses it

`Quantity(value)` builds a `Quantity` by checking one value against its
rules. It is validation, never a function with parameters, so there is
nothing for a second value to be, and nothing to check without one.
Jadpo won't drop or combine the extra values, because which one you
meant decides the value you get.

## What it looks like

main.jadpo:

```jadpo check project=e-ctor-count expect=type.constructor_argument_count
type Quantity = Int {
    min: 1
}

function pair() -> Quantity {
    return Quantity(1, 2)
}
```

```text output
error: `Quantity(...)` takes one value, not 2
 --> main.jadpo:6:12
  |
6 |     return Quantity(1, 2)
  |            ^^^^^^^^^^^^^^ 2 given
  reason: Constructing a `Quantity` checks one value against its rules.
  next: Pass one value to `Quantity(...)` (agent)
  rule: type.constructor_argument_count
  help: https://jadpo.dev/docs/errors/type.constructor_argument_count

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is which value the `Quantity` is.

**If it is one of the values**, pass only that one:

```jadpo fragment
    return Quantity(2)
```

```jadpo check project=e-ctor-count-fix-1 hidden
type Quantity = Int {
    min: 1
}

function pair() -> Quantity {
    return Quantity(2)
}
```

The function returns a `Quantity` of 2, checked against `min: 1` while
compiling, since it is a literal.

**If it is worked out from the values**, work it out inside the
constructor, so it is still given one value:

```jadpo fragment
    return Quantity(1 + 2)
```

```jadpo check project=e-ctor-count-fix-2 hidden
type Quantity = Int {
    min: 1
}

function pair() -> Quantity {
    return Quantity(1 + 2)
}
```

The sum is checked against `Quantity`'s rules like any other value. A
type that holds several values is an `Object`, built with a value for
each of its fields by name.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Objects](../language/02-values-and-types.md#objects)
