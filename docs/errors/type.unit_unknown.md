# `type.unit_unknown`

A number is written with a suffix that is no unit, often a unit in the wrong case.

## Why Jadpo refuses it

A number's suffix says what it measures: a duration's `ms`, `s`, `m`, `h`
or `d`, or a quantity's unit. Units are written as SI writes them, and
case matters, so a suffix that misses one only by case names each it was
near. `mw` is between milliwatts and megawatts, a factor of a billion
apart, and Jadpo won't guess which you meant.

## What it looks like

main.jadpo:

```jadpo check project=e-unit-unknown expect=type.unit_unknown
type HeaterPower = Power {}

/// The fan heater the workshop runs in winter.
function workshop_heater() -> HeaterPower {
    return HeaterPower(2mw)
}
```

```text output
error: `2mw` has no unit `mw`: did you mean `2mW` (milliwatts) or `2MW` (megawatts)?
 --> main.jadpo:5:24
  |
5 |     return HeaterPower(2mw)
  |                        ^^^ unit in the wrong case
  reason: Units are written as SI writes them, and case matters: `mW` is milliwatts and `MW` is megawatts.
  next: Write `2mW` (milliwatts) or `2MW` (megawatts), whichever is meant (agent)
  rule: type.unit_unknown
  help: https://jadpo.dev/docs/errors/type.unit_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you meant a unit the error names**, write it in that unit's case:

```jadpo fragment
    return HeaterPower(2mW)
```

```jadpo check project=e-unit-unknown-fix-1 hidden
type HeaterPower = Power {}

/// The fan heater the workshop runs in winter.
function workshop_heater() -> HeaterPower {
    return HeaterPower(2mW)
}
```

The value is now two milliwatts.

**If you meant another unit**, write that unit's suffix:

```jadpo fragment
    return HeaterPower(2kW)
```

```jadpo check project=e-unit-unknown-fix-2 hidden
type HeaterPower = Power {}

/// The fan heater the workshop runs in winter.
function workshop_heater() -> HeaterPower {
    return HeaterPower(2kW)
}
```

The value is now two kilowatts.

## Related

- [Suffixed literals](../language/19-standard-library.md#suffixed-literals)
- [Units](../language/19-standard-library.md#units)
