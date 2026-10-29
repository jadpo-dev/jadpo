---
title: Geospatial, design notes
area: 20
---

# Geospatial: design notes

Why [Geospatial](../20-geospatial.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

- **Area 2:** `rings_valid` and `altitudes_agree` are cross-field rules on
  standard types, as the others are. `Feature<G, P>` and
  `FeatureCollection<G, P>` are generic standard types like `Range<T>` and
  `Page<T>`.
- **Area 5:** the storage table has `geography`; `kind:` on `indexes:` is
  built for `IndexKind.geographic`; derived indexes include GiST.
- **Area 6:** "A condition must be one the database can use an index for"
  includes the `geo.` conditions; `order:` includes `geo.distance`.
- **Area 13:** the test server needs PostGIS. Postgres.app's PostgreSQL 16
  ships PostGIS 3.4.2 (verified). CI runs `tools/fixtures/postgres/Dockerfile`,
  PostgreSQL 16 with PostGIS 3.6 and pgvector from apt.postgresql.org,
  since every `postgis/postgis` 16 tag is built on Debian bullseye, whose
  support has ended (area 21, "Without pgvector").
- **Area 14:** `migration.extension_missing` is raised by `jadpo migrate
  apply`, like the other `migration.*` rules no source can trigger.
- **Area 15:** a position isn't personal data by type; an application
  marks a person's with `location`.
- **Area 19:** `geo` is a reserved namespace. Measures are on the
  ellipsoid (question 3), coordinates are held as doubles' shortest
  decimals (question 2), and `BoundingBox` and `Circle` are stored in
  named columns (question 7). `LineString`'s and `Polygon`'s
  `type.standard_exists` shapes are one shape: a single field `points` or
  `coordinates` holding a list whose items are `Point`s or an `Object` of
  `Point`'s shape, named list types included; a list that may hold
  `none`, a set, or another field beside it isn't the shape. A list of
  points is a path or an area's ring, and only the application knows
  which, so the error names both, and `jadpo fix` swaps to `LineString`,
  the list read as its `points`; a `Polygon` is the application's own
  edit. `jadpo suggest` leaves the shape to the error, as it is only ever
  a type's whole. `Feature` and `FeatureCollection` are in release 0.1.0
  beside the others, so 259 standard types are built, and
  `type.type_argument_count` lists them beside `List`, `Set`, `Map` and
  `Range`.

## Decisions

The numbers are the ones the compiler's messages cite.

| Topic | Decision |
|---|---|
| Edges (question 1) | Great circles in code and in the database, as PostGIS's `geography` draws them, not RFC 7946's straight lines |
| Coordinates (question 2) | Held as the shortest decimal of the nearest double, so storage and GeoJSON are exact |
| Measures (question 3) | `geo.distance`, `length`, `bearing` and `area` on the WGS84 ellipsoid by Karney's methods, in Jadpo's own whole-number code, the same on every target |
| Order by distance (question 4) | Only `asc`, only beside `geo.within(field, point, distance:)` on the same field and point, only in a `List`, sorted by `ST_Distance`; pages by distance stay out |
| PostGIS (question 5) | Required only once an application has a geometry column or index: the first migration adding one writes `CREATE EXTENSION postgis`; `jadpo migrate apply` fails with `migration.extension_missing` when it can't; Redis and D1 keep GeoJSON and refuse `geo.` conditions at compile time |
| GeoJSON features (question 6) | `Feature<G, P>` and `FeatureCollection<G, P>` as standard types, `G` a geometry type and `P` an `Object`, read strictly, no `id` |
| `BoundingBox` and `Circle` storage (question 7) | Named columns: a box's four sides as `numeric`, a circle's centre as `geography` and radius as `numeric(38, 6)` |
| Polygon validity (question 8) | Simple rings, holes inside, edges under 180° of arc, under half the globe: `format` when read, the rule `rings_valid` when built |
| WKT and WKB (question 9) | `geo.from_wkt` and `geo.to_wkt`, longitude first, `SRID=4326;` read and any other refused; WKB inside the driver only |
| Point-only distance (question 10) | `geo.distance` and `within(distance:)` between points only; a line or polygon is `type.mismatch` naming this page |
| Box filter (question 11) | On `Point` fields only, through an index on the column cast to `geometry`, which is exact for points; a shape field uses `geo.intersects` with a `Polygon` |
| The rest of the page | Operations, storage, queries, precision, interop and what is left out, as written |
| Altitudes | Every position in a geometry has an altitude or none does: `format` when read, the rule `altitudes_agree` when built |
| A layer from a route | A route may send a `FeatureCollection<G, P>` bare, as it sends a `List` or `Page` of an `Output`, so a mapping library fetches the layer itself; `P` holds only what clients may see (area 7). A query gives one for a `GET` route (area 6) |
