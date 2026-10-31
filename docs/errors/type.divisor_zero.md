# `type.divisor_zero`

A division's divisor has a type that allows zero, so the division could fail when the program runs.

## Why Jadpo refuses it

Dividing by zero has no answer. Rather than stop when it happens, Jadpo
rules it out while compiling: a divisor's type must exclude zero, as a
type with `min: 1` does. Here `Stock` allows `0`, so nothing stops
`portions` being zero.

## What it looks like

main.jadpo:

```jadpo check project=e-divisor expect=type.divisor_zero
type Stock = Int {
    min: 0
}

function split_evenly(total: Stock, portions: Stock) -> Stock {
    return Stock(total / portions)
}
```

```text output
error: `portions` may be zero
 --> main.jadpo:6:26
  |
6 |     return Stock(total / portions)
  |                          ^^^^^^^^ a `Stock` can be `0`
  reason: Division by zero is ruled out while compiling: a divisor's type excludes zero, as `Int { min: 1 }` does, and `Stock` allows `0`.
  next: Divide by a value whose type excludes zero, building it with its constructor where the zero case is handled (agent)
  rule: type.divisor_zero
  help: https://jadpo.dev/docs/errors/type.divisor_zero

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

The divisor of `integer.divide` is checked the same way.

## How to fix it

Both fixes divide by `Portions`, a type built on `Stock` that excludes
zero. Because it is built on `Stock`, it can stand in for a `Stock` on
either side of `/`.

**If the caller always has at least one portion**, declare `Portions` and
make the parameter that type, so callers pass a value already checked:

```jadpo fragment
type Portions = Stock {
    min: 1
}

function split_evenly(total: Stock, portions: Portions) -> Stock {
```

```jadpo check project=e-divisor-fix-1 hidden
type Stock = Int {
    min: 0
}

type Portions = Stock {
    min: 1
}

function split_evenly(total: Stock, portions: Portions) -> Stock {
    return Stock(total / portions)
}
```

Zero portions is refused where the value is built, before the call, so
the division can't fail.

**If the value can be zero**, declare `Portions` as above and a failure
for the zero case, `NoPortions` here. Handle that case first, then build
the divisor. The check before it proves the construction can't fail:

```jadpo fragment
function split_evenly(total: Stock, portions: Stock) fails NoPortions -> Stock {
    if (portions == 0) {
        reject NoPortions
    }
    var divisor: Portions = Portions(portions)
    return Stock(total / divisor)
```

```jadpo check project=e-divisor-fix-2 hidden
type Stock = Int {
    min: 0
}

type Portions = Stock {
    min: 1
}

type NoPortions = Failure {
    kind: FailureKind.invalid_value
}

function split_evenly(total: Stock, portions: Stock) fails NoPortions -> Stock {
    if (portions == 0) {
        reject NoPortions
    }
    var divisor: Portions = Portions(portions)
    return Stock(total / divisor)
}
```

Zero portions now ends `split_evenly` with `NoPortions`, which its
`fails` list names. Any other value is divided as before.

## Related

- [Expressions and operators](../language/03-logic.md#expressions-and-operators)
- [Conditions as proof](../language/02-values-and-types.md#conditions-as-proof)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)
