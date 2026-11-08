# `type.record_type_arguments`

A record names its type with type arguments, which come from its place or its values.

## Why Jadpo refuses it

A `Range`, `Feature` or `FeatureCollection` written as a record takes its type
arguments from the type its place expects, or else from its values, so
`Feature { ... }` is the one way to write one. A second place to say the same
thing could disagree with the first.

## What it looks like

main.jadpo:

```jadpo check project=e-record-type-arguments expect=type.record_type_arguments
type Pin = Object {
    label: Text
}

function pinned(point: Point) -> Feature<Point, Pin> {
    return Feature<Point, Pin> {
        geometry: point
        properties: Pin {
            label: "Crumbs"
        }
    }
}
```

```text output
error: `Feature<Point, Pin>` is written `Feature { ... }`
 --> main.jadpo:6:12
  |
6 |     return Feature<Point, Pin> {
  |            ^^^^^^^^^^^^^^^^^^^ type arguments on a record
  reason: A record names its type alone. Its type arguments come from the type its place expects, or else from its values, so there is one way to write a `Feature`.
  next: Write `Feature { ... }`, with `Feature<Point, Pin>` on the `var` or the return type if its values don't say (agent)
  rule: type.record_type_arguments
  help: https://jadpo.dev/docs/errors/type.record_type_arguments

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the record's place or its values say its type**, name the type alone:

```jadpo fragment
return Feature {
    ...
}
```

```jadpo check project=e-record-type-arguments-fixed hidden
type Pin = Object {
    label: Text
}

function pinned(point: Point) -> Feature<Point, Pin> {
    return Feature {
        geometry: point
        properties: Pin {
            label: "Crumbs"
        }
    }
}
```

It means the same `Feature<Point, Pin>`, read from the function's return type.

## Related

- [A record names its type alone](../language/02-values-and-types.md#a-record-names-its-type-alone)
- [GeoJSON and other formats](../language/20-geospatial.md#geojson-and-other-formats)
