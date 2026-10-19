# 20 · A fell race

A fell race from Keswick to the top of Skiddaw and back: its legs
measured, the course checked against the open land it may use, runners
tracked from their watches, the race map framed and labelled, the land
read from a land agent's spreadsheet, and the checkpoints sent to a
mapping tool and read back. It teaches geography in code.

Every position is a `Point` with a named `latitude` and `longitude`, in
WGS84 as GPS gives them, and maybe an `altitude`. Lines, polygons and
their multi-part forms are built from points, and edges are great
circles, the shortest path on the globe. The `geo.` operations measure
on the WGS84 ellipsoid in Jadpo's own whole-number arithmetic, so the
interpreter, native builds and WebAssembly give the same digits. An axis
order (longitude first) exists only at the edges: GeoJSON and WKT.

## What it teaches

1. **Points and shapes.** A checkpoint's `Point` with its height, the course as a `LineString` and the common as a `Polygon` with a hole; a shape written out is checked while compiling. See `domains/fellrace/types/race_checkpoint.jadpo`, `domains/fellrace/fixtures/` and [area 20, Geospatial](../../docs/language/20-geospatial.md).
2. **Measures.** `geo.distance` and `geo.bearing` for a leg, which fails `same_point` for two equal points; `geo.length` for the course and `geo.area` for the land, its hole taken away. Heights are ignored. See `domains/fellrace/actions/measure_leg.jadpo`, `domains/fellrace/actions/measure_course.jadpo`, `domains/fellrace/actions/open_land.jadpo` and [area 20, Operations](../../docs/language/20-geospatial.md#operations).
3. **Inside and across.** `geo.contains` and `geo.intersects` between any two shapes, inside including the edge. See `domains/fellrace/actions/check_course.jadpo`.
4. **Near, and inside a circle or a box.** `geo.within(a, b, distance:)` for two points that close, and `geo.within` of a `Circle` and of a `BoundingBox`, whose sides are parallels and meridians. See `domains/fellrace/actions/track_runner.jadpo`.
5. **Framing a map.** `geo.bounding_box` and `geo.centroid`. See `domains/fellrace/actions/frame_map.jadpo`.
6. **A polygon built from values.** Its rings must not cross, `rings_valid`, and every corner has a height or none does, `altitudes_agree`: two rules mapped with `attempt`. See `domains/fellrace/actions/outline_land.jadpo` and [area 20, Polygons are valid when built](../../docs/language/20-geospatial.md#polygons-are-valid-when-built).
7. **WKT.** `geo.from_wkt` reads a `Geometry`, a choice of the six shapes matched like any enum with data, and fails `malformed` or `out_of_range`; `geo.to_wkt` writes one back. See `domains/fellrace/actions/land_from_sheet.jadpo`, `domains/fellrace/actions/sheet_text_for.jadpo` and [area 20, Interop](../../docs/language/20-geospatial.md#interop).
8. **GeoJSON layers.** A `FeatureCollection` of `Feature`s, each a geometry and its properties, written with `json.write` and read back with `json.read`. See `domains/fellrace/actions/checkpoint_layer.jadpo` and `domains/fellrace/actions/moved_checkpoints.jadpo`.
9. **GeoJSON shapes and number pairs.** `geo.from_geojson` reads one shape from GeoJSON text, and `geo.from_lat_lng` and `geo.from_lng_lat` a position from two or three numbers, each named for the order it reads them in, so a swapped pair reads wrong in the code. See `domains/fellrace/actions/land_from_drawing.jadpo`, `domains/fellrace/actions/runner_fix.jadpo`, `domains/fellrace/actions/marshal_pin.jadpo` and [area 20, Interop](../../docs/language/20-geospatial.md#interop).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/20-fell-race
target/release/jadpo test examples/20-fell-race
target/release/jadpo fmt --check examples/20-fell-race
```

- `check` ends with `checked: <n> files` and no errors.
- `test` ends with `tested: 22 passed, 22 action tests`.

## Try next

- In `skiddaw_common.jadpo`, swap the common's second and third corners, so two of its edges cross. `jadpo check` reports `type.invalid_literal`: a polygon written out is checked while compiling.
  <!-- try {"file": "domains/fellrace/fixtures/skiddaw_common.jadpo", "replace": "latitude: 54.6\n                longitude: -3.1\n                altitude: none\n            },\n            Point {\n                latitude: 54.66\n", "with": "latitude: 54.66\n                longitude: -3.1\n                altitude: none\n            },\n            Point {\n                latitude: 54.6\n", "check": ["type.invalid_literal"]} -->
- In `measure_leg.jadpo`, call `geo.bearing` without its `attempt`. `jadpo check` reports `failure.attempt_required`: two checkpoints in one place have no direction between them, and the leg has to say what that means.
  <!-- try {"file": "domains/fellrace/actions/measure_leg.jadpo", "replace": "attempt geo.bearing(from.spot, to.spot) {\n        same_point: SameSpot\n    }", "with": "geo.bearing(from.spot, to.spot)", "check": ["failure.attempt_required"]} -->
- In `track_runner.test.jadpo`, let a runner check in from `150metre` away. `jadpo check` reports `type.invalid_literal`: `CheckInReach` stops at 100 m.
  <!-- try {"file": "domains/fellrace/actions/track_runner.test.jadpo", "replace": "        10metre,\n", "with": "        150metre,\n", "check": ["type.invalid_literal"]} -->
- Then read [21-veg-box-rounds](../21-veg-box-rounds/README.md), which stores places and searches by them.
