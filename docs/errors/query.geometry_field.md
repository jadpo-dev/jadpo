# `query.geometry_field`

A map-layer query's `geometry:` names a field whose type isn't the shape the layer's result type says each feature holds.

## Why Jadpo refuses it

A query that returns a `FeatureCollection<G, P>` gives a map layer: one
feature per record, each with a geometry of type `G`. `geometry:` names
the stored field that becomes that geometry, and the stored value goes
into the layer as it is, with nothing built from it.

Here the layer holds `Point`s, and `delivery_area` is a `Polygon`. A
client reading the layer expects each feature to be a point, so a polygon
would break it.

## What it looks like

main.jadpo:

```jadpo check project=e-geometry-field expect=query.geometry_field
type BakeryName = Text {
    max_length: 80
}

entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point
    delivery_area: Polygon
}

type BakeryPin = Object {
    id: Bakery.id
    name: BakeryName
}

query bakery_map() -> FeatureCollection<Point, BakeryPin> {
    from: Bakery
    geometry: Bakery.delivery_area
    order: [Bakery.name asc]
    limit: 500
}
```

```text output
error: `Bakery.delivery_area` isn't a `Point`, so it can't be each feature's geometry
 --> main.jadpo:19:15
   |
19 |     geometry: Bakery.delivery_area
   |               ^^^^^^^^^^^^^^^^^^^^ a `Polygon`
  reason: `FeatureCollection<Point, BakeryPin>`'s features each hold a `Point`, and a stored value goes into the layer as it is, so `geometry:` names a field of that type.
  next: Use `Bakery.spot` (agent)
  rule: query.geometry_field
  help: https://jadpo.dev/docs/errors/query.geometry_field

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the map shows each bakery as a point**, name the `Point` field:

```jadpo fragment
    geometry: Bakery.spot
```

```jadpo check project=e-geometry-field-fix-1 hidden
type BakeryName = Text {
    max_length: 80
}

entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point
    delivery_area: Polygon
}

type BakeryPin = Object {
    id: Bakery.id
    name: BakeryName
}

query bakery_map() -> FeatureCollection<Point, BakeryPin> {
    from: Bakery
    geometry: Bakery.spot
    order: [Bakery.name asc]
    limit: 500
}
```

Each feature in the layer is a bakery's `spot`, with its `id` and `name`
as properties.

**If the map shows each bakery's delivery area**, make the layer one of
`Polygon`s:

```jadpo fragment
query delivery_map() -> FeatureCollection<Polygon, BakeryPin> {
    from: Bakery
    geometry: Bakery.delivery_area
    ...
}
```

```jadpo check project=e-geometry-field-fix-2 hidden
type BakeryName = Text {
    max_length: 80
}

entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point
    delivery_area: Polygon
}

type BakeryPin = Object {
    id: Bakery.id
    name: BakeryName
}

query delivery_map() -> FeatureCollection<Polygon, BakeryPin> {
    from: Bakery
    geometry: Bakery.delivery_area
    order: [Bakery.name asc]
    limit: 500
}
```

Each feature is now a bakery's delivery area, drawn as a polygon, so
clients of the query get polygons where they used to expect points.

## Related

- [A query giving a map layer](../language/06-reads-and-writes.md#a-query-giving-a-map-layer)
- [Storage](../language/20-geospatial.md#storage)
