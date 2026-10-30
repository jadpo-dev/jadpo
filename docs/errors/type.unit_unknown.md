# `type.unit_unknown`

A number written with a suffix that is no unit: neither a duration's
(`ms`, `s`, `m`, `h`, `d`) nor a quantity's ("Suffixed literals"). Units
are written as SI writes them, and case matters, so a suffix that misses
one only by case names each it was near: `mw` is between milliwatts and
megawatts, and the program can't guess which.

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
