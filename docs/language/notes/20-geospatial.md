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

## Evidence and reasoning behind the area page

What the area page states as rules rests on the reasons and checks
below.
Facts marked "verified" were checked against PostGIS 3.4.2 on
PostgreSQL 16; "estimate" marks a number the build measures.

### Measures on the ellipsoid, not a sphere

Distance, length, bearing and area use Karney's geodesic algorithms
(C. F. F. Karney, "Algorithms for geodesics", 2013), which GeographicLib
implements and PostGIS's `geography` uses by default. A sphere would be
up to 0.5% off (367 m between London and Paris, verified) and disagree
with every query. Vincenty's simpler method fails to converge for points
nearly opposite each other.

### Inside includes the edge

PostGIS's `geography` has `ST_Covers` and `ST_CoveredBy` but no
`ST_Contains` or `ST_Within` (verified), and the boundary-inclusive
meaning is the one an application expects.

### The centroid on the sphere

`geo.centroid` is worked out on the sphere, as PostGIS's
`ST_Centroid(x, false)` is, because that is simpler to state and to
reproduce exactly than the ellipsoid's.

### No distance from a point to a line or polygon

That distance is a further geodesic algorithm, which PostGIS works out by
finding the closest points on the sphere first. Jadpo would have to
reproduce it exactly to agree with PostGIS, so it waits until a scenario
needs it (question 10 keeps `geo.distance` to points).

### Creating PostGIS in a migration

Leaving the extension to the operator would add a setup step to every
new database for no safety gained, so the first migration that needs it
creates it (question 5). Whether a role may create an extension that
isn't trusted depends on the server and the provider's roles, so
`jadpo migrate apply` tries it in a savepoint: trying is the one sure
test.

### Storage, checked

Every geometry, with and without altitude, a `Geometry` of each kind, a
box and a circle is stored and read back equal on PostGIS 3.4.2 and CI's
3.6.4 (`jadpo-cli`'s `geography` test). Coordinates come back unchanged
because question 2 holds every coordinate as the shortest decimal of a
double.

### Queries: names in the code, and the checks

- The margin is the constant `GEO_MARGIN`. The box's envelopes are worked
  out by `EnvelopePart` as the read runs, and the rows to fetch by
  `Source::Fetch`. The runtime rechecks each row through `jadpo-value`'s
  `geo.within`, `contains` and `intersects`, as a body calls them.
- Verified: `ST_Covers`'s four cases are each within the margin
  (`ST_DWithin` true where `ST_Covers` is false), and every case of
  `tools/references/geo_cases.py`, run as a query against every case's
  shape at once, gives the code's answer on PostGIS 3.4.2 (Postgres.app)
  and on CI's 3.6.4.
- Verified plan for nearest first inside a radius: an index scan on
  `&&`, then `ST_DWithin`, then a sort of the matches.
- Beyond the page's examples, `jadpo-cli`'s `geo_queries` test runs the
  page's queries over two thousand bakeries and three hundred patches,
  each giving the code's answer through its index (`EXPLAIN`), with boxes
  across and onto the antimeridian and round the pole, the recheck asking
  again, ties by `id`, and every case of `tools/references/geo_cases.py`
  as a query.

### GeoJSON features as standard types

Map libraries (Leaflet, MapLibre) take a `FeatureCollection`, so without
`Feature` and `FeatureCollection` every application would declare the
same wrapper, which `type.standard_exists` couldn't catch (question 6).

### WKT, checked

The one WKT reader is `jadpo-value`'s `geo::wkt`. A patch with a hole, a
patch wound the other way, one at a height and every other kind come back
from `ST_GeogFromText` and `ST_AsText`, as WKT and as EWKT, exactly as
written, and read back as the value they started from (`jadpo-cli`'s
`geography` test).

## How the geodesics are computed

How Jadpo's own code reaches the same digits on every target, behind
the area page's "Precision and determinism".

- **No floats in Jadpo's own code.** Karney's inverse method is
  additions, multiplications, divisions, square roots, `sin`, `cos` and
  `atan2`, with a few rounds of Newton's method and series to the eighth
  order in the ellipsoid's third flattening; all of it runs on whole
  numbers far wider than the result, rounded once at the end. A float's
  `sin` differs between platforms' maths libraries (area 19, "Maths on `Decimal`"), so a float version could give different digits on wasm32.
- **Karney's inverse problem, ported, not called.** GeographicLib's
  `Geodesic::GenInverse` and `PolygonArea` (C++, MIT licence) are ported
  step for step, with the coefficients of its highest series order, 8:
  the terms left out are below ε⁹, about 10⁻²⁵ of the leading one.
  GeographicLib's floating-point devices become whole-number ones: its
  `tiny_` is one unit of the last place and its machine epsilon 10⁴ of
  them (about 10⁻³⁷), so Newton's method on the azimuth runs until the
  longitude it reaches is that close, its bisection kept; exact decimals
  need no `AngRound`.
- **The working precision.** A working value is a 256-bit
  two's-complement whole number over 2¹³⁶, on the stack; each product and
  quotient is cut toward zero, `sqrt` and `hypot` are whole square roots,
  `sin` and `cos` Taylor's series after taking out quarter turns, and
  `atan2` Euler's series after taking out the nearest `atan(k/16)`. Binary
  places give the same determinism as decimal ones, 15 times faster than
  area 19's decimal big numbers (600 µs a distance there); 18 decimal places
  in an `i128` can't keep a continent's area to the square millimetre.
  Each result is rounded half to even from the exact binary value: not
  correctly rounded, the series being finite, but the same digits on
  every target. (The code is `jadpo-std`'s `sphere` module, which the
  checker and the runtime share, so a polygon literal is read by the code
  the runtime runs.)
- **Predicates are exact on the sphere, by whole numbers.** A position's
  direction is `(cos φ cos λ, cos φ sin λ, sin φ)`, each sine and cosine a
  working value taken to within 45° of 0 on the decimal before it becomes
  radians, and each product kept whole: every component is a whole number
  over 2²⁷². Points on one meridian are then exactly in a plane with the
  poles, points on the equator exactly at `z = 0`, a pole is `(0, 0, ±1)`
  at any longitude, -180 is 180, and opposite points are exact negations.
  Every predicate is the sign of a polynomial in the components, worked
  out in whole numbers as wide as it needs (about 820 bits for three
  points, 1,100 for where two edges cross), after a 40-bit unit vector
  beside each point settles the clear cases. So a point exactly on an
  edge, a corner, a meridian or the equator is found on it, and inside
  includes the edge with no tolerance. Where three points are exactly on
  one great circle, counting the edges a path crosses breaks the tie by
  Simulation of Simplicity, as Google's S2 does (its `Sign`,
  `CrossingSign`, `VertexCrossing` and `AngleContainsVertex`, Apache 2.0):
  whether a point is inside a ring is the inside known at a corner from
  the angle there, flipped by each edge the path from that corner to the
  point crosses. No float decides anything.

## How the geodesics are checked

The method and tolerances behind the area page's "Checked against
reference answers".

- **Checked against Karney's test set.** C. F. F. Karney's "Test set for
  geodesics" (2010, doi:10.5281/zenodo.32156, Creative Commons Zero),
  GeographicLib's `GeodTest-short.dat`, is 10,000 geodesics in nine
  groups. `tools/references/geodtest_sample.py` writes every 20th line, 500 of them,
  with its group and line number, to
  `crates/jadpo-value/examples/geodesics/reference.tsv`, checking the
  file's SHA-256, and `--check` holds the sample to it; the full file
  isn't committed, so the gate needs no network. The gate checks the
  sample natively and under WebAssembly, and both must give one digest of
  every answer. The test set's distance and point 1 are exact; point 2 is
  rounded to 10⁻¹⁸ degrees, which moves it up to 1.6 × 10⁻¹³ m. So
  `geo.distance` must be the reference distance rounded half to even to
  the micrometre (either way for one within a nanometre of a tie);
  `geo.bearing` the reference azimuth within half its 9th place plus how
  far moving point 2 can turn it, `1.6 × 10⁻¹³ m / m₁₂`; and `geo.area`,
  of the polygon from point 1 down its meridian, along the equator and up
  to point 2, the reference's within its stated square millimetre, half
  `Area`'s last place, and what moving point 2 can change it by. Where the
  reduced length `m₁₂` is below a metre (a geodesic ending at or near the
  point conjugate to its start: the "between vertices" and "ending close
  to vertices" groups, and one line between the poles), only the distance
  is checked, the direction and area being too sensitive to point 2's
  rounding. All 10,000 lines pass, not only the sample: before rounding,
  every distance is within 7.5 × 10⁻¹⁴ m (75 femtometres) of the
  reference, as close as rounding point 2 allows; every azimuth within
  `6.4 × 10⁻¹² m / m₁₂` degrees, the same limit; and each geodesic's area
  within 8.4 × 10⁻⁷ m² for the short and near-pole groups, and within the
  allowance elsewhere (at most 46% of it, near-antipodal lines).
- **Checked against PostGIS.** `tools/references/geo_cases.py` writes 169 cases,
  every operation, with PostGIS 3.4.2's answers on PostgreSQL 16.3, to
  `crates/jadpo-value/examples/geodesics/shapes.tsv`: small, continental
  and pole-holding polygons, polygons across the antimeridian and the
  equator, holes, concave shapes, lines and multi-part shapes, and the
  verified box whose edges bow towards the pole, away from every boundary
  but exact ones (a polygon's own corner, a point on a meridian edge). The
  gate checks them natively and under WebAssembly, which give one digest.
  Every yes or no is PostGIS's but the four cases under "Operations",
  where the file records PostGIS's answer beside the page's; each
  centroid is within 4 × 10⁻¹² of a degree of `ST_Centroid(x, false)`'s
  (half a micrometre); each box's side within 3 × 10⁻¹⁴ of a degree of the
  box round PostGIS's points every 100 m along the shape.
