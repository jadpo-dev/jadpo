# `type.standard_exists`

You declared a type with the same fields as a standard type, such as a start and an end, under a name of your own.

## Why Jadpo refuses it

A standard type, such as `InstantRange` or `Point`, has one way to be
built, one shape in JSON and one in storage. A declaration with only its
fields, by their common names and built on the same types, is that type
under another name. Two declarations of one shape disagree on names and
meaning, and every package and app that meets both has to convert
between them.

Jadpo matches by shape, so it can be wrong: a type can mean something
else while having the same fields. That is a judgement call, so you can
decline it with a reason, which review reads.

## What it looks like

main.jadpo:

```jadpo check project=e-standard-exists expect=type.standard_exists
/// When a room is booked, from its start to its end.
type Booking = Object {
    starts_at: Instant
    ends_at: Instant
}
```

```text output
error: `Booking` is the standard `InstantRange` under another name
 --> main.jadpo:2:6
  |
2 | type Booking = Object {
  |      ^^^^^^^ a start and an end
  reason: `Booking` has only a start (`starts_at`) and an end (`ends_at`), each built on `Instant`, which is the standard `InstantRange`. Declarations of one shape under different names disagree on names and meaning, and every package and application has to convert between them; the standard type has one construction, one wire shape and one storage shape.
  next: Run `jadpo fix Booking swap` to use `InstantRange` where `Booking` is used, then convert at the edge where data arrives in another shape, or write `decline type.standard_exists: "<why>"` above it if this is deliberate (agent)
  rule: type.standard_exists
  help: https://jadpo.dev/docs/errors/type.standard_exists

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A latitude and a longitude, by their common names, are the standard
`Point`, so a van's last position is refused too.

main.jadpo:

```jadpo check project=e-standard-exists-point expect=type.standard_exists
/// Where a delivery van was when its tracker last reported.
type VanPosition = Object {
    lat: Decimal
    lng: Decimal
}
```

```text output
error: `VanPosition` is the standard `Point` under another name
 --> main.jadpo:2:6
  |
2 | type VanPosition = Object {
  |      ^^^^^^^^^^^ a latitude and a longitude
  reason: `VanPosition` has only a latitude (`lat`) and a longitude (`lng`), each built on `Decimal` or `Int`, which is the standard `Point`. Declarations of one shape under different names disagree on names and meaning, and every package and application has to convert between them; the standard type has one construction, one wire shape and one storage shape.
  next: Run `jadpo fix VanPosition swap` to use `Point` where `VanPosition` is used, then convert at the edge where data arrives in another shape, or write `decline type.standard_exists: "<why>"` above it if this is deliberate (agent)
  rule: type.standard_exists
  help: https://jadpo.dev/docs/errors/type.standard_exists

check failed: 1 error, 1 file
```

A list of points is either the standard `LineString` or a `Polygon`'s
ring. Only your app knows which, so the error names both.

main.jadpo:

```jadpo check project=e-standard-exists-line expect=type.standard_exists
/// A van's round, stop to stop, as its tracker sends it.
type VanRound = Object {
    coordinates: List<Point>
}
```

```text output
error: `VanRound` is the standard `LineString` or `Polygon` under another name
 --> main.jadpo:2:6
  |
2 | type VanRound = Object {
  |      ^^^^^^^^ a list of points
  reason: `VanRound` has only a list of points (`coordinates`), each a `Point`, which is the standard `LineString` or `Polygon`. Declarations of one shape under different names disagree on names and meaning, and every package and application has to convert between them; the standard type has one construction, one wire shape and one storage shape.
  next: Run `jadpo fix VanRound swap` to use `LineString` where `VanRound` is used, or use `Polygon` if that is what it means, then convert at the edge where data arrives in another shape, or write `decline type.standard_exists: "<why>"` above it if this is deliberate (agent)
  rule: type.standard_exists
  help: https://jadpo.dev/docs/errors/type.standard_exists

check failed: 1 error, 1 file
```

## How to fix it

The question is whether your type means what the standard type means.

**If it means the same thing**, swap it for the standard type:

```sh
jadpo fix Booking swap
```

The declaration is removed, and every place that used `Booking` now uses
`InstantRange`, with its fields read by `InstantRange`'s names, such as
`start`. Where data arrives in another shape, such as a provider's
`starts_at`, convert it at the edge where it arrives. A type that is
already stored, or in an API's record, also gets the data migration, and
the change goes through review as any API or data change does.

**If it means something else**, keep it, and write a decline line with
the reason directly above the declaration. main.jadpo becomes:

```jadpo check project=standard-exists-declined
/// A shift as the rota plans it, before anyone has worked it.
decline type.standard_exists: "Planned hours on the rota, not a range anyone has worked"
type PlannedShift = Object {
    starts_at: Instant
    ends_at: Instant
}
```

The type keeps its own name and shape. `jadpo approval` lists every
decline with its reason, so a reviewer sees why it isn't the standard
type.

## Related

- [Swap to the standard type](../language/19-standard-library.md#swap-to-the-standard-type)
- [Decline, with a reason, on the type itself](../language/19-standard-library.md#decline-with-a-reason-on-the-type-itself)
- [False matches](../language/19-standard-library.md#false-matches)
- [The standard types](../language/19-standard-library.md#the-standard-types)
- [Writing a decline](../language/01-project-layout-and-modules.md#writing-a-decline)
- [Latitude and longitude by name](../language/20-geospatial.md#latitude-and-longitude-by-name)
