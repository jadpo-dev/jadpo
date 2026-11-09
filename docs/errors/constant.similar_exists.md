# `constant.similar_exists`

A constant has the same type and value as another constant.

## Why Jadpo refuses it

Agents working at the same time tend to invent the same constant twice, each
writing its own. Two copies of one value drift apart when only one is changed,
so the error names the other constant and asks you to read it.

Two constants can share a value by coincidence, though, and only you can tell.
That makes it a judgement call you can decline with a reason.

## What it looks like

```jadpo check project=e-constant-similar expect=constant.similar_exists
type HoldbackRate = Decimal {
    scale: 4
    min: 0.0000
    max: 1.0000
}

type Stock = Int {
    min: 0
}

library stocking {
    /// The standard share held back.
    constant holdback_rate: HoldbackRate = 0.2000
}

constant standard_holdback: HoldbackRate = 0.2000

function holdback_of(per_pack: Stock) -> Stock {
    if (stocking.holdback_rate == standard_holdback) {
        return Stock(per_pack / 5)
    }
    return per_pack
}
```

```text output
error: A constant with this value exists: `stocking.holdback_rate`
 --> main.jadpo:16:10
   |
16 | constant standard_holdback: HoldbackRate = 0.2000
   |          ^^^^^^^^^^^^^^^^^ same as `stocking.holdback_rate`
  reason: `standard_holdback` is a `HoldbackRate` holding `0.2000`, as `stocking.holdback_rate` in `main.jadpo` is.
  next: Read `stocking.holdback_rate` instead, and remove `standard_holdback`, or write `decline constant.similar_exists: "<why>"` above it if this is deliberate (agent)
  rule: constant.similar_exists
  help: https://jadpo.dev/docs/errors/constant.similar_exists

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the two mean the same thing.

**If they are the same value**, read the existing one and remove yours:

```jadpo fragment
function holdback_of(per_pack: Stock) -> Stock {
    if (stocking.holdback_rate == 0.2000) {
        ...
    }
    ...
}
```

```jadpo check project=e-constant-similar-fix-1 hidden
type HoldbackRate = Decimal {
    scale: 4
    min: 0.0000
    max: 1.0000
}

type Stock = Int {
    min: 0
}

library stocking {
    /// The standard share held back.
    constant holdback_rate: HoldbackRate = 0.2000
}

function holdback_of(per_pack: Stock) -> Stock {
    if (stocking.holdback_rate == 0.2000) {
        return Stock(per_pack / 5)
    }
    return per_pack
}
```

The value now has one home, and changing it changes it everywhere.

**If they only share a value by coincidence**, decline the rule above the
constant, with your reason:

```jadpo fragment
decline constant.similar_exists: "The two are set by different teams"
constant standard_holdback: HoldbackRate = 0.2000
```

```jadpo check project=e-constant-similar-declined hidden
type HoldbackRate = Decimal {
    scale: 4
    min: 0.0000
    max: 1.0000
}

type Stock = Int {
    min: 0
}

library stocking {
    /// The standard share held back.
    constant holdback_rate: HoldbackRate = 0.2000
}

decline constant.similar_exists: "The two are set by different teams"
constant standard_holdback: HoldbackRate = 0.2000

function holdback_of(per_pack: Stock) -> Stock {
    if (stocking.holdback_rate == standard_holdback) {
        return Stock(per_pack / 5)
    }
    return per_pack
}
```

Both constants stay, and the reason sits beside the one you kept for a
reviewer to read.

## Related

- [Two constants holding one value](../language/03-logic.md#two-constants-holding-one-value)
- [Keeping code as it is, with a reason](../language/01-project-layout-and-modules.md#keeping-code-as-it-is-with-a-reason)
