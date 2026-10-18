---
title: Geospatial
area: 20
status: decided
checked-with: this checkout's jadpo on every gate run, from chunk 5.5u2; the other examples are fragments until chunks 5.5u4 to 5.5u6 build them
rules: []
---

# Geospatial

Where things are, and what is near or inside what: a bakery's position,
the delivery patches it serves, the shops within five kilometres of a
customer, the pins inside the map a person is looking at. The 17
geospatial types already exist ([area 19](19-standard-library.md), built
in chunk 5.5s): `Point` with named `latitude` and `longitude`, the five
other geometries and `Geometry`, all in WGS84 and sent as GeoJSON. This
page adds what they were waiting for: the `geo.` operations, storage in
PostGIS, query conditions and indexes, and the formats other systems use.

> Decided 2026-10-18 (Richard), as recommended, with one clarification to question 5: PostGIS is needed only once an application has a geometry column or index, and an application with none never needs it. Chunk 5.5u wrote the page; the "Decisions" table under "Review" records each decision. Chunk 5.5u2 built the measures, `geo.distance`, `length`, `bearing` and `area`, and every coordinate held as the shortest decimal of the nearest double, as "Chosen in chunk 5.5u2" records; chunk 5.5u3 built the predicates, `centroid`, `bounding_box` and `rings_valid`, as "Chosen in chunk 5.5u3" records; chunks 5.5u4 to 5.5u6 build the rest, in the order under "Building it". Where this page changes another area (questions 2, 3 and 7 amend area 19's scope and chunk 5.5s's choices), that page is amended by the chunk that builds the change. Facts marked "verified" were checked against PostGIS 3.4.2 on PostgreSQL 16 (Postgres.app, this machine); "estimate" marks a number the build measures.

| Part | Decided | Builds on |
|---|---|---|
| Operations | 10 in `geo`: distance, length, bearing, area, intersects, contains, within (with or without `distance:`), centroid, bounding box; and WKT's two | Area 19's scope; 5.5e's decimal maths |
| Storage | One PostGIS `geography` column per geometry, checked to its type and WGS84; GiST indexes derived from queries | Area 5; 5.5w's storage rule |
| Queries | `geo.within`, `geo.contains`, `geo.intersects` as `where:` conditions; `order:` by `geo.distance` inside a radius | Area 6 |
| Interop | GeoJSON `Feature` and `FeatureCollection`; WKT at named edges | 5.5s's GeoJSON |

## What every geospatial operation follows

- **WGS84, latitude and longitude by name** (area 19, decided). An axis
  order exists only inside `geo.from_geojson`, `geo.from_lng_lat`,
  `geo.from_lat_lng` and this page's WKT edges.
- **Edges are great circles,** the shortest path on the globe between two
  points, as PostGIS's `geography` draws them, not straight lines on a flat
  map of longitude and latitude (question 1). Verified: in a 20°-wide box
  with corners at 50° N and 60° N, a point at 60.1° N is inside and one at
  50.1° N is outside, since both edges bow towards the pole. At city scale
  the difference is small: an east-west edge 5 km long at 51.5° N bows
  0.6 m north at its middle, one 50 km long 61 m (verified).
- **Measures are on the WGS84 ellipsoid** (question 3): distance, length,
  bearing and area by Karney's geodesic algorithms (C. F. F. Karney,
  "Algorithms for geodesics", 2013), which GeographicLib implements and
  PostGIS's `geography` uses for `ST_Distance`, `ST_DWithin`, `ST_Length`
  and `ST_Area` by default. Altitude is ignored by every operation.
- **Inside includes the edge.** A point on a patch's boundary is in the
  patch. PostGIS's `geography` has `ST_Covers` and `ST_CoveredBy` but no
  `ST_Contains` or `ST_Within` (verified), and the boundary-inclusive
  meaning is the one an application expects.
- **Pure and the same everywhere.** Every operation is worked out by
  Jadpo's own whole-number code (fixed point in 256 bits, chunk 5.5u2),
  so the interpreter, native builds and wasm32 give the same digits.
  "Precision and determinism" says where floats still appear.
- **A geometry argument** takes any of the six geometries or a
  `Geometry`, as `list.` operations take any list.

## Operations

| Operation | Gives | Fails | SQL form |
|---|---|---|---|
| `geo.distance(from, to)` | `Length`: the geodesic between two `Point`s on the WGS84 ellipsoid, rounded half to even to the micrometre (`Length`'s scale) | | `ST_Distance(a, b)` |
| `geo.length(line)` | `Length`: a `LineString`'s or `MultiLineString`'s geodesics summed, each edge measured as by `distance` | | `ST_Length(x)` |
| `geo.bearing(from, to)` | `Bearing`: the geodesic's direction as it leaves `from`, degrees clockwise from true north, at least 0 and below 360, to 9 places | `same_point` | `degrees(ST_Azimuth(a, b))` |
| `geo.area(shape)` | `Area`: a `Polygon`'s or `MultiPolygon`'s area on the ellipsoid, holes taken away, to `Area`'s scale | | `ST_Area(x)` |
| `geo.intersects(a, b)` | `Bool`: the two share at least one point, edges included | | `ST_Intersects(a, b)` |
| `geo.contains(a, b)` | `Bool`: no point of `b` lies outside `a`; `a` may also be a `BoundingBox` or a `Circle` | | `ST_Covers(a, b)` |
| `geo.within(a, b)` | `Bool`: `geo.contains(b, a)` | | `ST_CoveredBy(a, b)` |
| `geo.within(a, b, distance:)` | `Bool`: `geo.distance(a, b) <= distance`, both `Point`s, `distance` a `Length` never below 0 | | `ST_DWithin(a, b, d)` |
| `geo.centroid(shape)` | `Point`: the centre on the sphere, weighted by area, length or count by the shape's kind; may lie outside a concave polygon | weights that cancel (two antipodal points) are a defect | `ST_Centroid(x, false)` |
| `geo.bounding_box(shape)` | `BoundingBox`: the smallest box of parallels and meridians holding the shape, an edge's highest or lowest point counted; across the antimeridian when that box is narrower | | none |

The SQL forms are what query conditions compile to (below) and what
5.6c's aggregates can use; `bounding_box` and `centroid` have no query
use. `ST_Azimuth` gives `NULL` for two equal points (verified), which a
query never needs, so `same_point` exists only in code.

- **Distance and length agree with GeographicLib to well under a
  micrometre before rounding:** within 75 femtometres of every distance
  in its published test set (chunk 5.5u2, "Chosen in chunk 5.5u2"),
  where PostGIS's doubles are good to 15 nanometres. London (51.5072° N,
  0.1276° W) to Paris (48.8566° N, 2.3522° E) is 343,896.891267 m; on a
  sphere it would be 343,530.338 m, 367 m short (both verified with
  PostGIS).
- **Bearing to 9 places,** as an `Angle` keeps radians. Two equal points
  have no direction, so a call maps `same_point`, as a local time that
  doesn't exist is mapped (area 10).
- **Area by Karney's polygon method,** each edge a geodesic. PostGIS's
  `ST_Area` gives 12,308,778,361.47 m² for the 1° square at the equator
  (verified); Jadpo gives 12,308,778,361.469453 m², and each geodesic's
  area to GeographicLib's test set's square millimetre.
- **`centroid` on the sphere** (each point a unit vector, weighted and
  summed, then back to latitude and longitude), as PostGIS's
  `ST_Centroid(x, false)`, which is simpler to state and to reproduce
  exactly than the ellipsoid's. A label's position, not a measure.
- **`contains` with a `BoundingBox`** compares latitudes and longitudes
  directly, the box's edges being parallels and meridians, not great
  circles; with a `Circle`, it is `within(distance:)` of its centre.
  `bounding_box` of a shape holding a pole runs from -180 to 180.
- **Not here:** `buffer` (see "Left out"); distance between a point and a
  line or polygon (question 10).

A bakery's van, its round and its patches, through the measures chunk
5.5u2 built:

```jadpo check project=geo-measures
type SameSpot = Failure {
    kind: FailureKind.invalid_value
}

type VanLeg = Object {
    trip_length: Length
    bearing_out: Bearing
}

type RoundLength = Length {}

type PatchArea = Area {}

/// How far a bakery's van drives to a customer, and which way it sets off.
function first_leg(bakery: Point, customer: Point) fails SameSpot -> VanLeg {
    var setting_off: Bearing = attempt geo.bearing(bakery, customer) {
        same_point: SameSpot  // a pickup at the counter
    }
    return VanLeg {
        trip_length: geo.distance(bakery, customer)
        bearing_out: setting_off
    }
}

/// How far a van drives on its round, stop to stop.
function round_length(van_round: LineString) -> RoundLength {
    return RoundLength(geo.length(van_round))
}

/// How much ground a delivery patch covers, its holes left out.
function patch_area(patch: Polygon) -> PatchArea {
    return PatchArea(geo.area(patch))
}
```

A point gives every field, its `altitude` as `none` when it has none (area
2). The page's values:

```jadpo check project=geo-measures file=main.test.jadpo
test "London to Paris is measured on the ellipsoid" {
    var big_ben: Point = Point {
        latitude: 51.5072
        longitude: -0.1276
        altitude: none
    }
    var eiffel_tower: Point = Point {
        latitude: 48.8566
        longitude: 2.3522
        altitude: none
    }
    assert geo.distance(big_ben, eiffel_tower) == 343896.891267metre
    assert geo.distance(eiffel_tower, big_ben) == geo.distance(big_ben, eiffel_tower)
    var leg: VanLeg = attempt first_leg(big_ben, eiffel_tower)
    assert leg.bearing_out == 148.046085477
    assert first_leg(big_ben, big_ben) fails SameSpot
}

test "a round is its legs summed" {
    var bakery: Point = Point {
        latitude: 51.5072
        longitude: -0.1276
        altitude: none
    }
    var customer: Point = Point {
        latitude: 48.8566
        longitude: 2.3522
        altitude: none
    }
    var van_round: LineString = LineString {
        points: [bakery, customer, bakery]
    }
    assert round_length(van_round) == 687793.782534metre
}
```

The predicates and shapes, built in chunk 5.5u3, on the bakery's
patches, its pins on a map and its shops:

```jadpo check project=geo-shapes
type Delivers = Bool {}

type InView = Bool {}

type WalkingReach = Length {
    min: 0metre
}

/// Whether a customer is inside one of the bakery's patches.
function delivers_to(patches: List<Polygon>, customer: Point) -> Delivers {
    return Delivers(list.any(patches, patch -> geo.contains(patch, customer)))
}

/// Whether a van's round passes through a patch.
function passes_through(round: LineString, patch: Polygon) -> Delivers {
    return Delivers(geo.intersects(round, patch))
}

/// Whether a shop's pin is inside the map a person is looking at.
function in_view(pin: Point, view: BoundingBox) -> InView {
    return InView(geo.within(pin, view))
}

/// Whether two shops are near enough to walk between.
function walkable(from: Point, to: Point, reach: WalkingReach) -> InView {
    return InView(geo.within(from, to, distance: reach))
}

/// Where a patch's name goes on the map.
function label_at(patch: Polygon) -> Point {
    return geo.centroid(patch)
}

/// The view that shows a whole patch.
function view_of(patch: Polygon) -> BoundingBox {
    return geo.bounding_box(patch)
}
```

A customer on a patch's edge is inside it, and a box's sides are
parallels and meridians, so the patch's northern edge, a great circle,
bows past 51.54° N between its corners and the box that shows the whole
patch reaches further:

```jadpo check project=geo-shapes file=main.test.jadpo
test "a customer on a patch's edge is delivered to" {
    var patch: Polygon = Polygon {
        outer: [
            Point {
                latitude: 51.49
                longitude: -0.2
                altitude: none
            },
            Point {
                latitude: 51.49
                longitude: -0.05
                altitude: none
            },
            Point {
                latitude: 51.54
                longitude: -0.05
                altitude: none
            },
            Point {
                latitude: 51.54
                longitude: -0.2
                altitude: none
            },
            Point {
                latitude: 51.49
                longitude: -0.2
                altitude: none
            }
        ]
        holes: []
    }
    var big_ben: Point = Point {
        latitude: 51.5007
        longitude: -0.1246
        altitude: none
    }
    var on_the_edge: Point = Point {
        latitude: 51.5
        longitude: -0.2
        altitude: none
    }
    var eiffel_tower: Point = Point {
        latitude: 48.8566
        longitude: 2.3522
        altitude: none
    }
    assert delivers_to([patch], big_ben)
    assert delivers_to([patch], on_the_edge)
    assert not delivers_to([patch], eiffel_tower)
    assert in_view(big_ben, view_of(patch))
    assert not in_view(eiffel_tower, view_of(patch))
    assert walkable(big_ben, on_the_edge, 6km)
    assert not walkable(big_ben, on_the_edge, 5km)
}

test "a patch's label and the view that shows it" {
    var patch: Polygon = Polygon {
        outer: [
            Point {
                latitude: 51.49
                longitude: -0.2
                altitude: none
            },
            Point {
                latitude: 51.49
                longitude: -0.05
                altitude: none
            },
            Point {
                latitude: 51.54
                longitude: -0.05
                altitude: none
            },
            Point {
                latitude: 51.54
                longitude: -0.2
                altitude: none
            },
            Point {
                latitude: 51.49
                longitude: -0.2
                altitude: none
            }
        ]
        holes: []
    }
    assert label_at(patch).latitude == 51.51501933808094
    assert label_at(patch).longitude == -0.12499999998684845
    assert view_of(patch).north == 51.54002390691161
    assert view_of(patch).south == 51.49
}
```

### Polygons are valid when built

Area 19's scope decided that a polygon's rings are closed and valid; 5.5s
built closing and left validity here. A `Polygon` is valid when, on the
sphere:

- no ring crosses or touches itself, except where its last point meets
  its first;
- each hole lies inside the outer ring, and no two rings cross or overlap;
- every edge is shorter than half the globe (180° of arc), so the great
  circle it follows is the only one;
- the polygon covers less than half the globe. Verified: PostGIS's
  `geography` reads a ring wound either way as the same, smaller area, so
  a ring meant to enclose the larger side can't be stored as meant.

A polygon read from GeoJSON or WKT that isn't valid is refused, `format`
at the ring's place, as 5.5s's other reading checks are. One built in
code breaks a named rule, `rings_valid`, mapped like any cross-field rule
(area 2, chunk 5.5t). Checking costs about n log n in the points (a sweep
over the edges); a country's outline of 10,000 points takes about 70 ms
(measured in chunk 5.5u3).

A polygon built from values is attempted:

```jadpo check project=geo-shapes file=patches.jadpo
type NotAPatch = Failure {
    kind: FailureKind.invalid_value
}

/// A patch from the corners a planner clicked, back to the first.
function patch_from(clicked: List<Point>, first: Point) fails NotAPatch -> Polygon {
    return attempt Polygon {
        outer: list.append(clicked, first)
        holes: []
    } {
        rings_valid: NotAPatch  // edges that cross, or a corner clicked twice
    }
}
```

```jadpo check project=geo-shapes file=patches.test.jadpo
test "a patch whose edges cross isn't one" {
    var corner: Point = Point {
        latitude: 0.0
        longitude: 0.0
        altitude: none
    }
    var bow_tie: List<Point> = [
        corner,
        Point {
            latitude: 10.0
            longitude: 10.0
            altitude: none
        },
        Point {
            latitude: 10.0
            longitude: 0.0
            altitude: none
        },
        Point {
            latitude: 0.0
            longitude: 10.0
            altitude: none
        }
    ]
    assert patch_from(bow_tie, corner) fails NotAPatch
}
```

A polygon written out is checked while compiling:

```jadpo check project=drawn-patch expect=type.invalid_literal
/// The patch a planner drew, its edges crossing.
function drawn_patch() -> Polygon {
    return Polygon {
        outer: [
            Point {
                latitude: 51.49
                longitude: -0.2
                altitude: none
            },
            Point {
                latitude: 51.54
                longitude: -0.05
                altitude: none
            },
            Point {
                latitude: 51.49
                longitude: -0.05
                altitude: none
            },
            Point {
                latitude: 51.54
                longitude: -0.2
                altitude: none
            },
            Point {
                latitude: 51.49
                longitude: -0.2
                altitude: none
            }
        ]
        holes: []
    }
}
```

```text output
error: This `Polygon` breaks its rule `rings_valid`
 --> main.jadpo:4:16
  |
4 |         outer: [
  |                ^ these rings don't make a valid polygon
  reason: A `Polygon`'s rings are closed with four points or more, never cross or touch themselves or each other, keep each hole inside the outer ring, and have every edge shorter than half the globe, so the polygon is the smaller side its outer ring bounds (area 20, "Polygons are valid when built"). A literal is checked while compiling.
  next: Close each ring and move its points so no edge meets another, with each hole inside the outer ring (agent)
  rule: type.invalid_literal
  help: docs/language/02-values-and-types.html#type-invalid-literal

check failed: 1 error, 1 file
```

## Storage

A geometry field is one PostgreSQL column, PostGIS's `geography`, which
holds longitude and latitude on WGS84 and measures in metres, so no
conversion stands between the column and `geo.distance`. 5.5w's rule
(native types where they exist, named columns for compound values) gives
the rest.

| Field type | PostgreSQL |
|---|---|
| `Point`, `LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon` | `geography`, with `CHECK (ST_SRID(x) = 4326 AND GeometryType(x) = 'POINT')` naming its own type |
| `Geometry` | `geography`, the check allowing any of the six |
| `BoundingBox` | four `numeric` columns, `<field>_south`, `_west`, `_north`, `_east` (question 7) |
| `Circle` | `<field>_centre geography`, checked as a `Point`'s, and `<field>_radius numeric(38, 6)` in metres (question 7) |
| `Latitude`, `Longitude`, `Bearing`, `Crs`, `Geohash`, `H3Cell`, `MapTile`, `EastingNorthing` | as 5.5s chose: `numeric`, `text`, or a column per part |

- **Untyped `geography` with a check, not `geography(Point, 4326)`.** A
  `Point`'s altitude may be `none` or present, value by value, and a
  typed column refuses one or the other: `geography(Point, 4326)` and
  even `geography(Geometry, 4326)` refuse a point with an altitude
  ("Geometry has Z dimension but column does not", verified). An untyped
  column takes both, and the check refuses a line in a point's column and
  a point in another system, such as ETRS89's `SRID=4258` (both verified).
  An altitude is stored as the third coordinate.
- **Stored exactly.** PostGIS keeps each coordinate as a double. A
  coordinate held as the shortest decimal of a double comes back
  unchanged; one with more digits than a double holds doesn't
  (`51.50720000001234567` came back `51.50720000001235`, verified).
  Question 2 makes every coordinate the former, as a `Vector`'s items are
  held as single-precision floats (5.5s2). An altitude has 6 places, so it
  comes back unchanged up to a million kilometres (15 significant digits).
- **The extension, only when something needs it** (question 5). The
  first migration that adds a geometry column or a geometry index starts
  with `CREATE EXTENSION IF NOT EXISTS postgis;`, in the migration's
  transaction like the rest. An application with no geometry column or
  index never creates the extension and never needs PostGIS on its
  server, as an application with no entity needs no database (area 5).

```jadpo fragment
// domains/baking/entities/bakery.jadpo
/// A shop that bakes and delivers.
entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point
}

// domains/baking/entities/delivery_patch.jadpo
/// An area one bakery delivers to.
entity DeliveryPatch {
    id: Uuid
    bakery_id: Bakery.id {
        on_delete: OnDelete.cascade
    }
    name: PatchName
    outline: Polygon
}
```

```sql
CREATE EXTENSION IF NOT EXISTS postgis;
CREATE TABLE "bakery" (
  "id" uuid PRIMARY KEY,
  "name" text NOT NULL,
  "spot" geography NOT NULL
    CONSTRAINT "bakery_spot_check" CHECK (ST_SRID("spot") = 4326 AND GeometryType("spot") = 'POINT')
);
```

### Indexes

Area 5 decided that the compiler creates every index its queries need, and
that special kinds take a `kind:` option. For geometry fields:

| Needed by | Index |
|---|---|
| `geo.within`, `geo.contains`, `geo.intersects` or `within(distance:)` on a field | `USING gist ("<field>")`, named `<table>_<field>_idx`, partial for `visible:` as every derived index is |
| `geo.within(field, box)` with a `BoundingBox` | `USING gist (("<field>"::geometry))`, named `<table>_<field>_box_idx` |
| A lookup made from outside the application | `indexes: { by_spot: { fields: [spot], kind: IndexKind.geographic } }` |

A GiST index holds one geometry column; a query that also filters on
other fields uses their ordinary indexes beside it, which PostgreSQL
combines. Verified: `ST_DWithin`, `ST_Covers` and the box test each use
these indexes, the first two through `&&` on the geography's box, then
the exact test.

### A store with no PostGIS

| Store | What happens |
|---|---|
| Any store, in an application with no geometry column or index | Nothing: PostGIS is never needed |
| PostgreSQL with PostGIS available | The first migration with a geometry column or index creates the extension; nothing to configure. Neon, Supabase, Amazon RDS and Google Cloud SQL all offer PostGIS (assumption from their documentation, not tested here) |
| PostgreSQL without it, or a role that can't create it | `jadpo migrate apply` stops before changing anything, with a new error, `migration.extension_missing`, naming the extension and that a role allowed to create it runs `CREATE EXTENSION postgis` once. PostGIS is not a trusted extension, so creating it needs a superuser or the provider's own admin role (verified). The application's refusal to start against a database whose migrations aren't applied (area 5, Phase 8) covers the rest |
| Redis (area 6), and D1 when it arrives | A geometry field is kept in the entity's JSON as its GeoJSON, as every Redis entity keeps its fields; any `geo.` condition or order is area 6's existing compile error naming PostgreSQL |

So the compiler refuses what it can see (a condition on a store that has
no spatial queries), and the deploy step refuses what only the server
knows. Nothing waits for the first query to fail.

## Queries

Area 6 allows only conditions an index can serve: a field, named first,
compared with a value. This page adds the `geo.` conditions to that list,
since a GiST index serves each one: the field first, values after. A
function of a field otherwise stays refused.

| In a query | Means | SQL |
|---|---|---|
| `where: geo.within(Bakery.spot, doorstep, distance: max_trip)` | Bakeries within `max_trip` of `doorstep`; a `Point` field and value | `ST_DWithin("spot", $1, $2)` |
| `where: geo.within(Bakery.spot, area)` | Inside a polygon or multi-polygon value | `ST_CoveredBy("spot", $1)` |
| `where: geo.within(Bakery.spot, view)` | Inside a `BoundingBox`; a `Point` field only (question 11) | `"spot"::geometry && ST_MakeEnvelope($west, $south, $east, $north, 4326)`, two envelopes when the box crosses the antimeridian |
| `where: geo.within(Bakery.spot, circle)` | Inside a `Circle` | `ST_DWithin("spot", $centre, $radius)` |
| `where: geo.contains(DeliveryPatch.outline, customer)` | Patches holding a point or shape | `ST_Covers("outline", $1)` |
| `where: geo.intersects(DeliveryPatch.outline, area)` | Patches touching a shape | `ST_Intersects("outline", $1)` |
| `order: [geo.distance(Bakery.spot, doorstep) asc]` | Nearest first; needs a `within(distance:)` on the same field and point (question 4) | `ORDER BY ST_Distance("spot", $1), "id"` |

- **Nearest first inside a radius.** PostGIS's index-ordered `<->`
  measures on a sphere: London to Paris is 343,530 m by `<->` and
  343,897 m by `ST_Distance` (verified), so ordering by it would disagree
  with `geo.distance` for two shops a few hundred metres apart in
  distance. Ordering by `ST_Distance` agrees, and the radius keeps the
  sort to the rows the index found. Verified plan: an index scan on
  `&&`, then `ST_DWithin`, then a sort of the matches.
- **Only `asc`, and only in a `List`.** Farthest first can't use the
  radius to bound anything, and a page by distance would need a cursor on
  a computed value; both are refused, `query.order_direction` and
  `query.order_field`, with reasons naming this page.
- **The result doesn't carry the distance.** A result type picks fields
  (area 6); the action works the distance out with `geo.distance`, which
  gives the same metres as the query measured.
- **Errors reuse area 6's rules** with new reasons: a `geo.` call with the
  field anywhere but first, a box on a field that isn't a `Point`, or a
  pair PostGIS's `ST_Covers` doesn't handle is
  `query.condition_unsupported`; `migration.extension_missing` is the
  only new rule.

```jadpo fragment
/// Bakeries near a customer, nearest first.
query bakeries_near(doorstep: Point, max_trip: SearchReach) -> List<BakeryCard> {
    from: Bakery
    where: geo.within(Bakery.spot, doorstep, distance: max_trip)
    order: [geo.distance(Bakery.spot, doorstep) asc]
    limit: 20
}

/// The pins inside the map a person is looking at.
query bakeries_in_view(view: BoundingBox) -> List<BakeryCard> {
    from: Bakery
    where: geo.within(Bakery.spot, view)
    order: [Bakery.name asc]
    limit: 200
}

/// The patches that deliver to a customer's door.
query patches_for(customer: Point) -> List<PatchCard> {
    from: DeliveryPatch
    where: geo.contains(DeliveryPatch.outline, customer)
    order: [DeliveryPatch.name asc]
    limit: 10
}
```

```sql
-- bakeries_near
SELECT "id", "name", "spot" FROM "bakery"
WHERE ST_DWithin("spot", $1, $2)
ORDER BY ST_Distance("spot", $1), "id"
LIMIT 20
```

## Precision and determinism

| Where | Arithmetic | Same on every target? |
|---|---|---|
| `geo.` operations in code: interpreter, native, wasm32 | Jadpo's own whole-number fixed point, 256 bits with 136 after the point (about 41 decimal places), its `sqrt` a whole square root and its `sin`, `cos` and `atan2` the series 5.5e's kernels use; results rounded half to even to the result type's scale (chunk 5.5u2) | Yes: integer arithmetic only, no platform maths library |
| Reading GeoJSON | The double JSON gives, held as its shortest decimal (5.5s) | Yes |
| Coordinates built in code | Rounded to the nearest double's shortest decimal at construction (question 2) | Yes |
| Storage | PostGIS doubles; exact, given the row above | Yes |
| Query conditions and order | PostGIS's own doubles: GeographicLib for distance, its spherical edge tests for `ST_Covers` and `ST_Intersects` | One server, one answer; may differ from code at a boundary |

- **No floats in Jadpo's own code.** Karney's inverse method is
  additions, multiplications, divisions, square roots, `sin`, `cos` and
  `atan2`, with a few rounds of Newton's method and series to the eighth
  order in the ellipsoid's third flattening; all of it runs on whole
  numbers far wider than the result, rounded once at the end. A float's
  `sin` differs between platforms' maths libraries (area 19, question
  18), so a float version could give different digits on wasm32.
- **What it costs** (measured in chunk 5.5u2, native release build):
  9 µs for a distance along a meridian, 30 to 65 µs for any other, about
  45 µs per edge of a polygon's area; WebAssembly takes about 2.8 times
  as long. A request measures a handful; a batch measuring millions, or
  the area of a 10,000-point outline (about half a second), would
  notice. "Chosen in chunk 5.5u2" has the figures.
- **Where code and the database can disagree:** a point on, or within a
  hair of, a polygon's edge, and a distance within a hair of a radius,
  since PostGIS decides in doubles with its own tolerances and Jadpo in
  decimals (estimate: under a millimetre; the build measures the band).
  Tests stay outside it. Text sorting was made to agree by forcing the
  `C` collation (area 5); no such switch exists for PostGIS's maths, and
  reimplementing its queries outside the database would lose the index.

## Interop

| Format | Decided | Where the axis order lives |
|---|---|---|
| GeoJSON geometries | As 5.5s built: strict RFC 7946, every ring written by the right-hand rule | `geo.from_geojson` and the wire |
| GeoJSON `Feature`, `FeatureCollection` | Two standard generic types, `Feature<G, P>` (a `geometry` of type `G` and `properties` of type `P`) and `FeatureCollection<G, P>` (its `features`), read and written as RFC 7946 objects (question 6) | The geometry inside |
| `GeometryCollection` | Still refused | |
| WKT and EWKT | `geo.from_wkt(text)` failing `malformed` or `out_of_range`, and `geo.to_wkt(shape)`, longitude first as PostGIS writes it; `SRID=4326;` accepted, any other SRID `out_of_range` (question 9) | Those two operations |
| WKB | The PostgreSQL driver's own encoding; no operation | Inside the driver |
| Other coordinate systems | Left out: `EastingNorthing` stays a value with its `Crs`, not converted | |

```jadpo fragment
type PatchProperties = Object {
    name: PatchName
    bakery_id: Bakery.id
}

/// The map's delivery layer, as a mapping library reads it.
function patch_layer(patches: List<DeliveryPatch>) -> FeatureCollection<Polygon, PatchProperties> {
    return FeatureCollection {
        features: list.map(patches, patch -> Feature {
            geometry: patch.outline
            properties: PatchProperties {
                name: patch.name
                bakery_id: patch.bakery_id
            }
        })
    }
}
```

## Left out, and why

| Left out | Why | Instead |
|---|---|---|
| `geo.buffer` | PostGIS buffers a `geography` by projecting it to a planar system it picks, so the result depends on PROJ's choice and can't be reproduced in Jadpo's code | `within(distance:)` and `Circle` for "near"; a provider package for drawn buffers |
| Coordinate systems other than WGS84 (British National Grid and the rest) | Accurate conversion needs datum grids (OSTN15 for Great Britain), megabytes of data per system that every wasm32 build would carry | `EastingNorthing` kept with its `Crs`; a provider package later |
| Straight edges on longitude and latitude (`geometry` columns) | Two edge meanings would split every operation in two; `geography` is area 19's decided storage | Great circles (question 1) |
| Routing, travel times, geocoding | Network data and paid services | Provider packages (area 11) |
| Nearest first without a radius, farthest first, pages by distance | Disagree with `geo.distance` or can't use the index | A radius (question 4) |
| Simplifying, unions, splitting at the antimeridian | No scenario in area 19's set needs them; each is a large algorithm | Later, when one does |
| `GeometryCollection`, and a `Feature` with no geometry | Rare, and each makes every consumer handle an extra case | A `MultiPolygon`, or the right geometry type |
| Altitude in any operation | WGS84's operations are on the surface; height above the ellipsoid isn't height above the ground | Kept and stored, never measured |

## Review

### Status

| Feature | Status |
|---|---|
| The 17 geospatial types, GeoJSON geometries, `type.standard_exists` for `Point` | Decided (area 19) and built (5.5s) |
| `geo.distance`, `length`, `bearing` and `area`; coordinates held as doubles' shortest decimals | Decided 2026-10-18; built (5.5u2) |
| `geo.intersects`, `contains`, `within` (with `distance:`, a `BoundingBox` and a `Circle`), `centroid`, `bounding_box`; `rings_valid` | Decided 2026-10-18; built (5.5u3) |
| Everything else on this page | Decided 2026-10-18; built by 5.5u4 to 5.5u6 |

### Consistency with the rest of the language

- **Area 2:** `rings_valid` is a cross-field rule on a standard type, as
  5.5t2 makes the others. `Feature<G, P>` and `FeatureCollection<G, P>`
  are generic standard types like `Range<T>` and `Page<T>`.
- **Area 5:** the storage table gains `geography`; `kind:` on `indexes:`
  is built for `IndexKind.geographic`; derived indexes gain GiST.
- **Area 6:** "A condition must be one the database can use an index for"
  gains the `geo.` conditions; `order:` gains `geo.distance`. Amended on
  area 6 when 5.5u5 builds them.
- **Area 13:** the test server needs PostGIS. Postgres.app's PostgreSQL
  16 ships PostGIS 3.4.2 (verified); CI's `postgres:16` image doesn't, so
  CI moves to `postgis/postgis:16-3.4`. *Amended by chunk 5.5v1:* CI runs
  `tools/postgres/Dockerfile`, PostgreSQL 16 with PostGIS 3.6 and pgvector
  from apt.postgresql.org, since every `postgis/postgis` 16 tag is built on
  Debian bullseye, whose support has ended (area 21, "Chosen in chunk
  5.5v1").
- **Area 14:** `migration.extension_missing` is raised by `jadpo migrate
  apply`, like the other `migration.*` rules no source can trigger.
- **Area 15:** unchanged. A position isn't personal data by type; an
  application marks a person's with `location`.
- **Area 19:** `geo` becomes a reserved namespace, as area 19 planned. The
  scope table's "metres, on the sphere" becomes the ellipsoid (question
  3); 5.5s's "held as written" for coordinates and its `geography`
  storage for `BoundingBox` and `Circle` change (questions 2 and 7);
  `LineString`'s and `Polygon`'s `type.standard_exists` shapes, which
  5.5s left here, come with 5.5u6.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Edges | Great circles in code and in the database, as PostGIS's `geography` draws them, not RFC 7946's straight lines (Richard, as recommended, question 1) | 2026-10-18 |
| Coordinates | Held as the shortest decimal of the nearest double, so storage and GeoJSON are exact; amends 5.5s's "held as written" (Richard, as recommended, question 2) | 2026-10-18 |
| Measures | `geo.distance`, `length`, `bearing` and `area` on the WGS84 ellipsoid by Karney's methods, in Jadpo's decimal code, the same on every target; amends area 19's "on the sphere" (Richard, as recommended, question 3) | 2026-10-18 |
| Order by distance | Only `asc`, only beside `geo.within(field, point, distance:)` on the same field and point, only in a `List`, sorted by `ST_Distance` (Richard, as recommended, question 4) | 2026-10-18 |
| PostGIS | Required only once an application has a geometry column or index: the first migration adding one writes `CREATE EXTENSION postgis`; `jadpo migrate apply` fails with `migration.extension_missing` when it can't; Redis and D1 keep GeoJSON and refuse `geo.` conditions at compile time (Richard, as recommended, clarified, question 5) | 2026-10-18 |
| GeoJSON features | `Feature<G, P>` and `FeatureCollection<G, P>` as standard types, read strictly, no `id` (Richard, as recommended, question 6) | 2026-10-18 |
| `BoundingBox` and `Circle` storage | Named columns: a box's four sides as `numeric`, a circle's centre as `geography` and radius as `numeric(38, 6)`; amends 5.5s (Richard, as recommended, question 7) | 2026-10-18 |
| Polygon validity | Simple rings, holes inside, edges under 180° of arc, under half the globe: `format` when read, the rule `rings_valid` when built (Richard, as recommended, question 8) | 2026-10-18 |
| WKT and WKB | `geo.from_wkt` and `geo.to_wkt`, longitude first, `SRID=4326;` read and any other refused; WKB inside the driver only (Richard, as recommended, question 9) | 2026-10-18 |
| Point-only distance | `geo.distance` and `within(distance:)` between points only for now (Richard, as recommended, question 10) | 2026-10-18 |
| Box filter | On `Point` fields only, through an index on the column cast to `geometry` (Richard, as recommended, question 11) | 2026-10-18 |
| The rest of the page | Operations, storage, queries, precision, interop and what is left out, as written (Richard, as recommended) | 2026-10-18 |

### Chosen in chunk 5.5u2

- **Karney's inverse problem, ported, not called.** GeographicLib's
  `Geodesic::GenInverse` and `PolygonArea` (C++, MIT licence), step for
  step, in `jadpo-value`'s `maths::geodesic`, with its series'
  coefficients for its highest order, 8: the terms left out are below
  ε⁹, about 10⁻²⁵ of the leading one. GeographicLib's floating-point
  devices become whole-number ones: its `tiny_` is one unit of the last
  place, its machine epsilon 10⁴ of them (about 10⁻³⁷), so Newton's method
  on the azimuth runs until the longitude it reaches is that close, its
  bisection kept; exact decimals need no `AngRound`.
- **Working precision.** A working value is a 256-bit two's-complement
  whole number over 2¹³⁶, about 41 decimal places, on the stack; each
  product and quotient is cut toward zero, `sqrt` and `hypot` are whole
  square roots, `sin` and `cos` Taylor's series after taking out quarter
  turns, `atan2` Euler's series after taking out the nearest `atan(k/16)`.
  Binary rather than decimal places: the same determinism, 15 times
  faster than 5.5e's decimal big numbers (600 µs a distance there); 18
  decimal places in an `i128`, the estimate above, can't keep a continent's
  area to the square millimetre. Each result is rounded half to even from
  the exact binary value; it is not correctly rounded, the series being
  finite, but it is the same digits on every target.
- **The test set.** C. F. F. Karney's "Test set for geodesics" (2010,
  doi:10.5281/zenodo.32156, Creative Commons Zero), as GeographicLib
  publishes it: `GeodTest-short.dat`, 10,000 geodesics in nine groups.
  `tools/geodtest_sample.py` writes every 20th line, 500 of them, with
  its group and line number, to
  `crates/jadpo-value/examples/geodesics/reference.tsv`; it checks the
  file's SHA-256 and `--check` holds the sample to it. The full file isn't
  committed, so the gate needs no network. The check runs natively
  (`jadpo-runtime`'s `geodesics` tests) and under WebAssembly (the gate's
  wasm32 steps), and both must give one digest of every answer.
- **What "to the micrometre" means for each measure.** The test set's
  distance and point 1 are exact; point 2 is rounded to 10⁻¹⁸ degrees,
  which moves it up to 1.6 × 10⁻¹³ m. `geo.distance` must be the
  reference distance rounded half to even to the micrometre (either way
  for one within a nanometre of a tie). `geo.bearing` must be the
  reference azimuth within half its 9th place plus how far moving point 2
  can turn it, `1.6 × 10⁻¹³ m / m₁₂`. `geo.area`, of the polygon from point
  1 down its meridian, along the equator and up to point 2, must be the
  reference's within its stated square millimetre, half `Area`'s last
  place, and what moving point 2 can change it by. Where the reduced
  length `m₁₂` is below a metre (a geodesic ending at or near the point
  conjugate to its start: the "between vertices" and "ending close to
  vertices" groups, and one line between the poles), the direction and
  area are too sensitive to point 2's rounding to check, and only the
  distance is.
- **Accuracy achieved.** Every one of the 10,000 lines passes, not only
  the sample. Before rounding, every distance is within 7.5 × 10⁻¹⁴ m
  (75 femtometres) of the reference, which is as close as rounding point
  2 allows; every azimuth within `6.4 × 10⁻¹² m / m₁₂` degrees, the same
  limit; and each geodesic's area within 8.4 × 10⁻⁷ m² for the short and
  near-pole groups, where rounding point 2 barely moves it, and within the
  allowance elsewhere (at most 46% of it, near-antipodal lines).
- **Benchmarks.** `cargo run --release -p jadpo-value --example
  geodesics -- bench`, natively on the machine this chunk was built on
  (an Apple M1 Pro): a distance along a meridian 9 µs; 2 km across London
  31 µs; London to Paris 42 µs; London to Sydney 58 µs; nearly antipodal
  64 µs; a bearing 47 µs; an 11-point line's length 315 µs; the 1° square's
  area 71 µs (meridians and the equator are quick); a 100-point patch's
  area 4.5 ms, about 45 µs an edge. The 500-line check takes 0.30 s
  natively and 0.83 s under Node's WebAssembly, about 2.8 times, so a
  distance there is about 110 µs and 10 ms of CPU measures about 90. No
  budget is asserted, as no timing is elsewhere; the figures are the
  baseline a tuning chunk measures against.
- **`same_point`** is the two points being one place: the same latitude
  and the same longitude, -180 being 180, or the same pole at any
  longitude. Two points a picometre apart have a direction. It is
  settled only by `attempt`, as decoding is: whether two points are equal
  is known only when the program runs.
- **A bearing that rounds to 360 is 0,** `Bearing`'s range being at
  least 0 and below 360 here; at a pole it is GeographicLib's, the
  meridian of the pole's given longitude.
- **`geo.length` sums each edge's distance as `geo.distance` rounds it,**
  so a line of two points is their distance exactly; a
  `MultiLineString`'s lines are summed the same way. A distance is never
  past 38 digits on Earth; a sum past them is a defect, as `+` is.
- **`geo.area` rounds once:** each ring's area, the smaller of the two
  sides it bounds, is worked out unrounded, the holes taken away and a
  `MultiPolygon`'s polygons added, then the total is rounded half to even
  to the square millimetre. Since chunk 5.5u3's `rings_valid`, a hole lies
  inside its outer ring, so holes never outweigh it.
- **What each measure takes.** `distance` and `bearing` take `Point`s
  only (question 10); `length` a `LineString` or a `MultiLineString`;
  `area` a `Polygon` or a `MultiPolygon`. Anything else, a `Geometry`
  included, is `type.mismatch` naming this page: the application matches
  a `Geometry` first, so it decides what a point's area is. A new
  operation kind, `Shapes`, lists the types a parameter takes.
- **Coordinates held as the nearest double** (question 2): a `Latitude`
  or a `Longitude`, as a value, a field or a point's part, is rounded to
  the shortest decimal of the nearest IEEE double as it is built, before
  its range is checked, as a `Vector`'s items are; Rust's own correctly
  rounded parsing and printing do it, the same on every target. Rounding
  can't take a value in range out of it (±90 and ±180 are doubles), and a
  double whose shortest decimal has more than 38 places (below 10⁻²³ of a
  degree) is rounded half to even to 38. A literal is checked as written.
- **The page's examples corrected:** a `Point` written in code gives
  `altitude: none`, since area 2 has every field given when a value is
  built.

### Chosen in chunk 5.5u3

- **Exact on the sphere, by whole numbers.** A position's direction is
  `(cos φ cos λ, cos φ sin λ, sin φ)` with each sine and cosine one of
  5.5u2's working values, taken to within 45° of 0 on the decimal before
  it becomes radians, and each product kept whole: every component is a
  whole number over 2²⁷². Points on one meridian are then exactly in a
  plane with the poles, points on the equator exactly at `z = 0`, a pole
  is `(0, 0, ±1)` at any longitude, -180 is 180, and opposite points are
  exact negations. Every predicate is the sign of a polynomial in the
  components, worked out in whole numbers as wide as it needs (about 820
  bits for three points, 1,100 for where two edges cross), after a 40-bit
  unit vector beside each point settles the clear cases; so a point
  exactly on an edge, a corner, a meridian or the equator, is found on it,
  and inside includes the edge with no tolerance. Where three points are
  exactly on one great circle, counting the edges a path crosses breaks
  the tie by Simulation of Simplicity, as Google's S2 does (its `Sign`,
  `CrossingSign`, `VertexCrossing` and `AngleContainsVertex`, Apache 2.0):
  whether a point is inside a ring is the inside known at a corner from
  the angle there, flipped by each edge the path from that corner to the
  point crosses. No float decides anything, so every target gives one
  answer; the native and wasm32 digests of every case below are one.
- **A ring's inside is the smaller side,** as PostGIS reads a ring either
  way round, chosen from the ring's area on the sphere, summed by Van
  Oosterom and Strackee's formula in working values: exact unless the two
  sides are within about 10⁻³⁵ of each other, and a ring exactly on one
  great circle, both sides equal, breaks `rings_valid`.
- **Where it lives.** The geometry is `jadpo-std`'s `sphere` module, and
  5.5u2's fixed-point maths moved there from `jadpo-value`, so the checker
  reads a polygon literal by the same code the runtime runs, as every
  standard rule is read; `jadpo-value`'s `geo` module reads values into it.
- **Agreement with PostGIS.** `tools/geo_cases.py` writes 169 cases, every
  operation, with PostGIS 3.4.2's answers on PostgreSQL 16.3 (this
  machine), to `crates/jadpo-value/examples/geodesics/shapes.tsv`: small,
  continental and pole-holding polygons, polygons across the antimeridian
  and the equator, holes, concave shapes, lines and multi-part shapes, the
  verified box whose edges bow towards the pole, away from every boundary
  but exact ones (a polygon's own corner, a point on a meridian edge). The
  gate checks them natively and under WebAssembly, as it does
  GeographicLib's sample. Every yes or no is PostGIS's but four; each
  centroid is within 4 × 10⁻¹² of a degree of `ST_Centroid(x, false)`'s
  (half a micrometre); each box's side within 3 × 10⁻¹⁴ of a degree of the
  box round PostGIS's points every 100 m along the shape.
- **The four, where the page's meaning and `ST_Covers` part.** `ST_Covers`
  on `geography` compares two polygons ring by ring: it doesn't see a hole
  of `a` that `b` covers (it says a square with a hole covers a square over
  the hole), and it counts rings lying along each other as crossing (it
  says a square doesn't cover the same square with a hole). With a
  multi-part `a` it wants all of `b` in one part, so two points each in one
  part aren't covered. `geo.contains` keeps the page's "no point of `b`
  lies outside `a`" in each; the file records PostGIS's answer beside the
  page's. Chunk 5.5u5 has to settle these for `where: geo.contains(...)`,
  whose SQL form is `ST_Covers`: a polygon field holding holes against a
  polygon value, and a multi-part field.
- **Rings may not touch at all.** Two rings sharing even one point break
  `rings_valid`: the page's "no two rings cross or overlap", read
  strictly, which also keeps a polygon's inside in one piece without a
  check of its own (OGC lets a hole touch the outer ring at one point).
  A ring touching itself anywhere but its closing point, a point repeated
  in a row, and a ring that turns straight back along an edge all break
  it too.
- **`rings_valid` checks closing.** A polygon built in code wasn't checked
  for closed rings of four points or more; the rule now does, as GeoJSON
  reading did. A `MultiPolygon` has no rule of its own: each of its
  polygons is valid, and they may overlap, the predicates taking the
  multi-polygon as the union of its parts.
- **A shape holding a pole reaches every longitude:** a point at a pole, a
  line through one, or a polygon around or touching one has a bounding box
  from -180 to 180, and only such a box holds it, whatever longitude a
  point at the pole is written with, since every meridian meets there.
- **A box holds any geometry in code,** its latitudes and longitudes
  compared directly with the shape's, an edge's highest or lowest point
  and the longitudes between its ends included; the query filter stays on
  `Point` fields (question 11). A side of `bounding_box` worked out from an
  edge is rounded outwards to a double, so a shape is always inside its
  own box; when two boxes are equally narrow, it takes the one that
  doesn't cross the antimeridian.
- **A `Circle` holds points only,** as `within(distance:)` measures between
  points only (question 10): `geo.contains(circle, shape)` and
  `within(shape, circle)` with anything but a `Point` are `type.mismatch`
  naming question 10, as is `within(a, b, distance:)` unless both are
  points. A distance is proved never below 0 while compiling, as a count
  is (`type.argument_negative`): a length written out, or a type with
  `min: 0metre`. A line edge between two opposite points, which no great
  circle joins, is its two ends.
- **`centroid` is PostGIS's, quirks included,** as the page chose it to be
  reproducible: a multi-point's points weigh one each; a line's edges each
  give both ends the edge's angle; a polygon is cut into triangles from the
  first corner of its first outer ring to every edge of every ring, each
  triangle's centre (its corners summed) weighted by its area, always
  counted positive. So a hole adds weight rather than taking it away, and
  a concave polygon's centroid moves with the corner its ring starts at,
  as PostGIS's does (verified: an L-shaped polygon's is 3.214° N, 3.236° E
  from one corner and 4.151° N, 4.159° E from another). A point's centroid
  is itself without its altitude; one whose weights cancel (two opposite
  points) is a defect, as PostGIS gives no number there. It is rounded to
  the nearest double, as every coordinate is.
- **Benchmarks** (`cargo run --release -p jadpo-value --example geodesics
  -- bench`, the machine this chunk was built on): a point in a 100-point
  patch 0.44 ms, in a 1,000-point one 4.3 ms; a 100-point patch in
  another 30 ms (each piece of every edge is classified, about n² in the
  points); two 100-point patches intersecting 0.9 ms; `within(distance:)`
  33 µs; a 100-point patch's centroid 0.37 ms and its box 1.2 ms;
  `rings_valid` 0.6 ms for 100 points, 6.8 ms for 1,000 and 68 ms for a
  10,000-point outline. Turning each corner into a direction (two sines
  and cosines) is most of a small polygon's cost.
- **The page's examples corrected:** `delivers_to` returned `Bool`, which
  a signature can't name (`type.primitive_signature`); it returns a
  `Delivers` built on it.

### Open questions

1. **Are edges great circles?** RFC 7946 §3.1.1 says a line between two
   positions is straight on longitude and latitude; PostGIS's `geography`
   (area 19's decided storage) follows the great circle. They differ by
   0.6 m in the middle of a 5 km edge at London's latitude and 61 m for a
   50 km edge (verified). The alternative is `geometry` columns with
   straight edges, then measuring by casting to `geography`, which needs
   expression indexes on every column and still measures edges as great
   circles. *Recommendation:* great circles everywhere, in code and in
   the database, so a query and `geo.contains` mean the same shape. A
   zone drawn with long straight edges on a web map should have points
   added along them; city-sized delivery patches are unaffected.
   *Decided 2026-10-18 (Richard):* as recommended.
2. **Is every coordinate held as the shortest decimal of a double?** 5.5s
   holds a coordinate built in code as written, so `51.50720000001234567`
   would be stored by PostGIS as `51.50720000001235` and read back as a
   different value (verified). *Recommendation:* yes, rounded at
   construction to the nearest double, as `Vector` holds single-precision
   floats (5.5s2). Every coordinate with up to 15 significant digits,
   which is all a person writes and every GPS gives, is unchanged; it
   makes storage and GeoJSON exact. Amends 5.5s's choice.
   *Decided 2026-10-18 (Richard):* as recommended.
3. **Distance on the ellipsoid by Karney's method, not on the sphere?**
   Area 19's scope said "metres, on the sphere". A sphere is up to 0.5%
   off (367 m between London and Paris, verified), and PostGIS's
   `ST_Distance` and `ST_DWithin` use the ellipsoid by default, so a
   sphere in code would disagree with every query. Vincenty's method is
   simpler but fails to converge for points nearly opposite each other.
   *Recommendation:* the ellipsoid, by Karney's method, in Jadpo's
   decimal code; amends area 19's scope row.
   *Decided 2026-10-18 (Richard):* as recommended.
4. **Does ordering by distance need a radius?** PostGIS's only
   index-ordered nearest search, `<->`, measures on a sphere (verified),
   so its order can disagree with `geo.distance`. *Recommendation:* yes:
   `order: [geo.distance(field, point) asc]` only beside
   `geo.within(field, point, distance:)` on the same field and point, in
   a `List` with `limit:`, sorted by `ST_Distance`. "Nearest anywhere" is
   a large radius. Pages by distance stay out.
   *Decided 2026-10-18 (Richard):* as recommended.
5. **What happens without PostGIS?** The compiler can't see the server.
   *Recommendation:* the first migration that adds a geometry column or
   index writes `CREATE EXTENSION IF NOT EXISTS postgis`; `jadpo migrate apply` fails before
   changing anything with `migration.extension_missing` when the server
   lacks PostGIS or the role can't create it (it is not a trusted
   extension, verified); Redis and D1 keep geometries as GeoJSON and
   refuse `geo.` conditions at compile time with area 6's existing error.
   The alternative, the operator creating the extension by hand, adds a
   setup step to every new database for no safety gained.
   *Decided 2026-10-18 (Richard):* as recommended, clarified: PostGIS is
   required only once the application has code that needs it. The first
   migration that adds a geometry column or index writes `CREATE
   EXTENSION postgis`, and an application with no geometry never needs
   PostGIS.
6. **GeoJSON `Feature` and `FeatureCollection` as standard types?** 5.5s
   refused them. Map libraries (Leaflet, MapLibre) take a
   `FeatureCollection`, so without them every application declares the
   same wrapper, with a `type: "Feature"` field, and `type.standard_exists`
   can't catch it. *Recommendation:* yes, `Feature<G, P>` and
   `FeatureCollection<G, P>`, `G` a geometry type and `P` an `Object`,
   read strictly; RFC 7946's optional `id` is left out (an application's
   identity goes in `properties`), so a feature arriving with one is
   `unknown_field`, as every input refuses a field it doesn't have. Two
   names added beside the 247, each on area 19's list.
   *Decided 2026-10-18 (Richard):* as recommended.
7. **`BoundingBox` and `Circle` stored as named columns, not
   `geography`?** 5.5s planned `geography` for both. A box's edges are
   parallels and meridians, which a `geography` polygon would turn into
   great circles; a circle has no `geography` form at all. *Recommendation:*
   named columns, by 5.5w's rule: a box's four sides as `numeric`, a
   circle's centre as `geography` and its radius as `numeric(38, 6)`.
   Amends 5.5s's storage choice.
   *Decided 2026-10-18 (Richard):* as recommended.
8. **Polygon validity as above, including no edge of half the globe
   and no polygon covering more than half of it?** Without the last two,
   a ring's meaning is ambiguous: PostGIS reads a ring either way round as
   the smaller area (verified). *Recommendation:* yes, `format` when read
   and the named rule `rings_valid` when built in code.
   *Decided 2026-10-18 (Richard):* as recommended.
9. **WKT at named edges, WKB inside the driver only?** WKT turns up in
   CSV exports and other systems' APIs; WKB only between programs and
   databases, which the driver handles. *Recommendation:* `geo.from_wkt`
   and `geo.to_wkt`, longitude first as PostGIS and every GIS tool write
   it for WGS84, `SRID=4326;` read and any other refused; no WKB
   operation.
   *Decided 2026-10-18 (Richard):* as recommended.
10. **`geo.distance` and `within(distance:)` between points only, for
    now?** Distance from a point to a line or polygon ("within 500 m of
    the route") is a further geodesic algorithm, and PostGIS measures it
    by finding the closest points on the sphere first, which Jadpo would
    have to reproduce exactly to agree. *Recommendation:* points only in
    this build; a line or polygon is `type.mismatch` naming this page, and
    the wider form comes when a scenario needs it.
    *Decided 2026-10-18 (Richard):* as recommended.
11. **The `BoundingBox` filter on `Point` fields only?** A box filter on
    a stored line or polygon would compare great-circle edges with a
    box of parallels, two shapes with different edges. *Recommendation:*
    `Point` fields only, through an index on the column cast to
    `geometry`, which is exact for points; a shape field uses
    `geo.intersects` with a `Polygon`.
    *Decided 2026-10-18 (Richard):* as recommended.

### Building it

Five chunks, each one session, placed after 5.5u on the roadmap:

- **5.5u2 Geospatial measures** `opus · xhigh`: `geo.distance`,
  `geo.length`, `geo.bearing` and `geo.area` by Karney's methods in
  Jadpo's decimal code; coordinates held as doubles' shortest decimals.
  Done when each matches GeographicLib's published test set (a sample of
  `GeodTest`) to the micrometre, gives the same digits on native and
  wasm32, and is benchmarked.
- **5.5u3 Geospatial predicates and shapes** `opus · xhigh`, after 5.5u2:
  `geo.intersects`, `contains`, `within` (with `distance:`, a
  `BoundingBox` and a `Circle`), `centroid`, `bounding_box`, and
  `rings_valid` on the sphere. Done when each agrees with PostGIS on
  cases away from a boundary, and every validity case reads and builds
  as the page says.
- **5.5u4 Geospatial storage and indexes** `opus · high`: `geography`
  columns with their checks, `BoundingBox` and `Circle` as named columns,
  `CREATE EXTENSION` in migrations, `migration.extension_missing`, GiST
  indexes derived and declared with `kind: IndexKind.geographic`,
  geometries in Redis as GeoJSON, and CI on `postgis/postgis:16-3.4`.
  Done when an entity stores and reads back every geometry, with and
  without altitude, unchanged.
- **5.5u5 Geospatial query conditions** `opus · high`, after 5.5u3 and
  5.5u4: the `geo.` conditions, the box and circle filters and order by
  distance, with area 6 amended and golden SQL for each. Done when each
  query on the page runs against PostGIS through its index and agrees with
  the code's answer on the page's examples.
- **5.5u6 Geospatial interop** `opus · high`, after 5.5u4: `Feature` and
  `FeatureCollection`, `geo.from_wkt` and `geo.to_wkt`, and
  `type.standard_exists`' shapes for `LineString` and `Polygon`. Done
  when a feature collection round-trips through JSON and a WKT polygon
  through PostGIS unchanged.
