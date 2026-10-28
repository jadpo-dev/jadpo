# `query.geometry_field`

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
  help: docs/language/06-reads-and-writes.html#query-geometry-field

check failed: 1 error, 1 file
```
