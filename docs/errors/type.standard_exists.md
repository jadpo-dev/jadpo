# `type.standard_exists`

A declaration whose only fields are a standard type's, by their common
names in any order and what they are built on, is that type under another
name. It is declinable: a type that means something else keeps its shape
with a reason on the declaration.

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

`Point`'s shape is a latitude and a longitude by their common names, so a
van's last position is the standard `Point`:

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

A list of points is the standard `LineString`, or a `Polygon`'s ring, and
only the application knows which, so the error names both (area 20):

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

Declined, with the reason review reads:

```jadpo check project=standard-exists-declined
/// A shift as the rota plans it, before anyone has worked it.
decline type.standard_exists: "Planned hours on the rota, not a range anyone has worked"
type PlannedShift = Object {
    starts_at: Instant
    ends_at: Instant
}
```
