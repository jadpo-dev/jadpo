# `type.unit_as_number`

You declared a type named for a unit, such as `Grams`, `Pence` or `Percent`, on a plain number.

## Why Jadpo refuses it

A type built on `Int` or `Decimal` holds a number and nothing else, so
its name is the only place the unit is written. A value in kilograms
passes for one in grams, a price in pounds for one in pence, and the
number adds to any other. The standard types keep the unit with the
number: a `Mass` converts between units exactly and shows itself in the
one you ask for, and a `Money` knows its currency.

Jadpo goes by the type's whole name, so it can be wrong: a name can be a
unit by chance. That is a judgement call, so you can decline it with a
reason, which review reads.

## What it looks like

main.jadpo:

```jadpo check project=e-unit-as-number expect=type.unit_as_number
/// What a parcel weighs.
type Grams = Int {
    min: 1
    max: 30000
}
```

```text output
error: `Grams` holds a `Mass` as a plain number
 --> main.jadpo:2:6
  |
2 | type Grams = Int {
  |      ^^^^^ named for a unit
  reason: `Grams` is named for a unit of `Mass` but is a plain `Int`, so nothing keeps the unit with the number: a value in another unit passes for one in this one, and it adds to any other number. A `Mass` carries what the number means, and converts and shows itself by its own rules.
  next: Build it on `Mass`, named for what it holds rather than its unit, or write `decline type.unit_as_number: "<why>"` above it if this is deliberate (agent)
  rule: type.unit_as_number
  help: https://jadpo.dev/docs/errors/type.unit_as_number

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A currency's minor unit is `Money` held as a whole number, so a price in
pence is refused too.

main.jadpo:

```jadpo check project=e-unit-as-number-pence expect=type.unit_as_number
/// What a loaf costs.
type Pence = Int {
    min: 0
}
```

```text output
error: `Pence` holds a `Money` as a plain number
 --> main.jadpo:2:6
  |
2 | type Pence = Int {
  |      ^^^^^ named for a unit
  reason: `Pence` is named for a unit of `Money` but is a plain `Int`, so nothing keeps the unit with the number: a value in another unit passes for one in this one, and it adds to any other number. A `Money` carries what the number means, and converts and shows itself by its own rules.
  next: Build it on `Money`, named for what it holds rather than its unit, or write `decline type.unit_as_number: "<why>"` above it if this is deliberate (agent)
  rule: type.unit_as_number
  help: https://jadpo.dev/docs/errors/type.unit_as_number

check failed: 1 error, 1 file
```

## How to fix it

The question is whether the type holds what its name says.

**If it holds that unit's kind of value**, build it on the standard type
and name it for what it holds. main.jadpo becomes:

```jadpo check project=unit-as-number-mass
/// What a parcel weighs.
type ParcelWeight = Mass {
    min: 1g
    max: 30kg
}
```

A value now names its unit when it is made, as `250g` or
`units.of(grams, MassUnit.gram)`, and a bare number where a `ParcelWeight`
is expected is `type.unit_required`. A price is the same with `Money`:

```jadpo check project=unit-as-number-money
/// What a loaf costs.
type LoafPrice = Money {
    currency: Currency.gbp
    min: 0
}
```

A `LoafPrice` is made from pence with `money.from_minor`, and adding it
to a price in another currency doesn't compile.

**If the name is a unit by chance**, keep the type, and write a decline
line with the reason directly above the declaration:

```jadpo check project=unit-as-number-declined
/// Knots the scouts can tie.
decline type.unit_as_number: "Knots the scouts can tie, not a speed"
type Knots = Int {
    min: 0
}
```

`jadpo approval` lists every decline with its reason, so a reviewer sees
why it isn't the standard type.

## Related

- [Units and money in plain numbers](../language/02-values-and-types.md#units-and-money-in-plain-numbers)
- [Physical quantities](../language/19-standard-library.md#physical-quantities)
- [Money and payments](../language/19-standard-library.md#money-and-payments)
- [Signals](../language/14-tooling.md#signals)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
