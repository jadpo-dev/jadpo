# `type.mismatch`

A value of one type is used where a different type is required.

## Why Jadpo refuses it

Every named type says what its values mean and which rules they follow.
A value can stand in only for its own type, or for a type it builds on.
Going the other way, such as from a plain `Int` to a `Stock`, always
goes through a constructor, which checks the target's rules, so a value
never takes on a type whose rules nobody checked.

## What it looks like

main.jadpo:

```jadpo check project=e-mismatch expect=type.mismatch
type Stock = Int {
    min: 0
}

function add(a: Stock, b: Stock) -> Stock {
    return a + b
}
```

```text output
error: `Int` can't be used where `Stock` is required
 --> main.jadpo:6:12
  |
6 |     return a + b
  |            ^^^^^ this is `Int`
  reason: Arithmetic works on the representation, so this is a plain `Int`; only `Stock(...)` makes it a `Stock`, checking its rules.
  next: Convert it with `Stock(a + b)` (agent)
  rule: type.mismatch
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the value should become the required type**, convert it with that
type's constructor:

```jadpo fragment
    return Stock(a + b)
```

```jadpo check project=e-mismatch-fix-1 hidden
type Stock = Int {
    min: 0
}

function add(a: Stock, b: Stock) -> Stock {
    return Stock(a + b)
}
```

The sum is checked against `Stock`'s rules. Here that check can't fail:
`a` and `b` are never below zero and `Stock` has no maximum, so their
declared ranges prove the sum is a valid `Stock`. When nothing proves it,
the construction starts with `attempt` and says what happens when it
fails.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)
- [Named types](../language/02-values-and-types.md#named-types)
