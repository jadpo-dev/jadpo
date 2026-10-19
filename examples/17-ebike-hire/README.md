# 17 · An e-bike hire fleet

A fleet of hire e-bikes: batteries charged and worn out, bikes checked
before they go out, rides summed up on a receipt, hills measured, docks
laid out, panniers packed and trackers that report home. It teaches
physical quantities: lengths, masses, energies, temperatures and the
rest, each held in its kind's base unit, with the unit chosen only when
a value is made or shown.

A quantity is never a bare number. `5km`, `250W` and `45 psi` each say
their unit, and inside a program a quantity has no unit at all, so
kilometres plus metres is just a length plus a length. Kinds combine as
physics says (volts times amps is watts, a length over a time is a
speed) and any other mix doesn't compile. Maths on them is exact
decimal maths, the same digits on every target.

## What it teaches

1. **Quantity types.** A type built on `Power`, `Energy`, `Pressure` or any other kind, with bounds written in a unit (`max: 250W`), checked whatever unit a value arrives in. See `domains/fleet/types/` and [area 19, Physical quantities](../../docs/language/19-standard-library.md#physical-quantities).
2. **Kinds that combine.** Volts times amps is a power, power for an hour an energy, an energy times a `Percentage` an energy; a temperature takes a difference, never another temperature. See `domains/fleet/actions/plan_charge.jadpo`.
3. **Units only at the edges.** `units.sum` adds lengths given in any unit, `units.in` gives a number in one, and `text.format` shows one in the reader's unit and digits. See `domains/fleet/actions/summarise_ride.jadpo`.
4. **Exact maths.** `decimal.sin`, `cos`, `atan2`, `pi`, `sqrt`, `power`, `exp` and `ln` work exact decimals to the places asked for, so an angle's height, a wheel's rollout, a dock's diagonal and a battery's fade are the same on every target. See `domains/fleet/actions/measure_climb.jadpo`, `domains/fleet/actions/survey_hill.jadpo`, `domains/fleet/actions/plan_dock.jadpo` and `domains/fleet/actions/assess_battery.jadpo`.
5. **Reading a quantity a person typed.** `text.parse` reads `45 psi` or `3.2 bar` as a pressure, fails `unknown_unit` for `45 kg`, and checks the type's bounds. See `domains/fleet/actions/check_bike.jadpo`.
6. **Areas, volumes, concentrations and data.** A length times a length is an area, a mass over a volume a concentration, a size over a time a bitrate. See `domains/fleet/actions/plan_dock.jadpo`, `domains/fleet/actions/pack_pannier.jadpo` and `domains/fleet/actions/size_tracker_uplink.jadpo`.
7. **Every kind's units by name.** A partner's catalogue prints bare numbers, each in its column's unit: `units.in` with a unit from the kind's own list, `PressureUnit.psi`, `VolumeUnit.us_gallon` or `TemperatureUnit.fahrenheit`. See `domains/fleet/actions/list_for_partner.jadpo` and `domains/fleet/types/model_spec.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/17-ebike-hire
target/release/jadpo test examples/17-ebike-hire
target/release/jadpo fmt --check examples/17-ebike-hire
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 14 passed, 14 action tests`.

## Try next

- In `plan_charge.test.jadpo`, charge at `36` instead of `36V`. `jadpo check` reports `type.unit_required`: a bare number is the mistake a quantity type exists to prevent.
  <!-- try {"file": "domains/fleet/actions/plan_charge.test.jadpo", "replace": "plan_charge(500Wh, 20%, 36V, 4A, 20degC)", "with": "plan_charge(500Wh, 20%, 36, 4A, 20degC)", "check": ["type.unit_required"]} -->
- In `plan_charge.jadpo`, give the charger's power as `voltage + current`. `jadpo check` reports `type.arithmetic_operand`: volts and amps don't add, they multiply.
  <!-- try {"file": "domains/fleet/actions/plan_charge.jadpo", "replace": "var charger_power: Power = voltage * current", "with": "var charger_power: Power = voltage + current", "check": ["type.arithmetic_operand"]} -->
- In `check_bike.test.jadpo`, fit the `6 bar` bike with a `300W` motor. `jadpo check` reports `type.invalid_literal`: `MotorPower` stops at the law's 250 W.
  <!-- try {"file": "domains/fleet/actions/check_bike.test.jadpo", "replace": "check_bike(\"6 bar\", 15lx, 2Hz, 250W,", "with": "check_bike(\"6 bar\", 15lx, 2Hz, 300W,", "check": ["type.invalid_literal"]} -->
- In `list_for_partner.jadpo`, print the tyre pressure in `VolumeUnit.us_gallon`. `jadpo check` reports `type.mismatch`: a unit from another kind's list doesn't fit a pressure.
  <!-- try {"file": "domains/fleet/actions/list_for_partner.jadpo", "replace": "            PressureUnit.psi,", "with": "            VolumeUnit.us_gallon,", "check": ["type.mismatch"]} -->
- Then read [18-village-newsletter](../18-village-newsletter/README.md), which works with text for the web.
