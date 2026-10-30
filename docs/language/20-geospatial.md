---
title: Geospatial
area: 20
status: decided
checked-with: this checkout's jadpo on every gate run
rules: []
---

# Geospatial
<!-- topic: geospatial -->

Geospatial values say where things are, and what is near or inside what:
a bakery's position, the patches it delivers to, the shops within five
kilometres of a customer, the pins inside the map a person is looking at.

The types are standard types, listed under
[Geospatial types](19-standard-library.md#geospatial-types): `Point`,
with a named `latitude` and `longitude`, five other geometries such as
`Polygon`, and `Geometry`, which holds any one of the six. All of them
are in WGS84, the coordinate system GPS uses, and are sent as GeoJSON.
This page covers what you do with them:

| Part | What | Builds on |
|---|---|---|
| [Operations](#operations) | 10 in `geo`: distance, length, bearing, area, intersects, contains, within (with or without `distance:`), centroid, bounding box; and the two for WKT text | The standard library's [operations](19-standard-library.md#what-every-operation-follows) and its whole-number maths |
| [Storage](#storage) | One PostGIS `geography` column per geometry, checked to its type and WGS84; GiST indexes derived from your queries | [Storage types](05-data.md#storage-types) |
| [Queries](#queries) | `geo.within`, `geo.contains`, `geo.intersects` as `where:` conditions; `order:` by `geo.distance` inside a radius | [Reads and writes](06-reads-and-writes.md) |
| [Interop](#interop) | GeoJSON `Feature` and `FeatureCollection`; WKT where other systems use it | The standard library's GeoJSON |

PostGIS is the PostgreSQL extension for geographic data. Where this page
says a PostGIS behaviour was verified, it was checked against PostGIS
3.4.2 on PostgreSQL 16. Some compiler messages cite a decision by number,
as in `question 10`; the Decisions table in the
[design notes](notes/20-geospatial.md#decisions) lists them.

## What every geospatial operation follows
<!-- topic: what-every-geospatial-operation-follows -->

Every `geo.` operation works the same way in the respects below, so you
can rely on them without checking each operation.

### Latitude and longitude by name

A point names its latitude and its longitude, so you never have to
remember which comes first. An order appears only where a format fixes
one: in `geo.from_geojson`, `geo.from_lng_lat`, `geo.from_lat_lng` and
the WKT operations under [Interop](#interop).

### Edges are great circles

An edge between two points is a great circle: the shortest path between
them on the globe, as PostGIS's `geography` draws it. It isn't a straight
line on a flat map of longitude and latitude, which is how RFC 7946, the
GeoJSON standard, describes an edge (its §3.1.1). Code and queries both
use great circles, so a query and `geo.contains` mean the same shape.

At city scale the difference is small: an east-west edge 5 km long at
51.5° N bows 0.6 m north at its middle, and one 50 km long bows 61 m
(verified). Over long edges it matters: in a box 20° wide with corners at
50° N and 60° N, a point at 60.1° N is inside and one at 50.1° N is
outside, because both edges bow towards the pole (verified). If a zone is
drawn on a web map with long straight edges, add points along them.
City-sized delivery patches are unaffected.

### Measures are on the ellipsoid

Distance, length, bearing and area are measured on the WGS84 ellipsoid,
the slightly flattened shape of the Earth that GPS uses. Jadpo uses
Karney's geodesic algorithms, which GeographicLib implements and which
PostGIS's `geography` uses by default for `ST_Distance`, `ST_DWithin`,
`ST_Length` and `ST_Area`, so code and database measure alike. Every
operation ignores altitude.

### Inside includes the edge

A point on a patch's boundary is in the patch. This holds for every
operation that asks whether one thing is inside another.

### The same answer everywhere

Every operation is pure, and is worked out by Jadpo's own whole-number
code, fixed point in 256 bits. So the interpreter, native builds and
wasm32 builds give the same digits.
[Precision and determinism](#precision-and-determinism) says where floats
still appear.

### What each operation takes

An operation that takes a geometry takes any of the six geometries or a
`Geometry`, as `list.` operations take any list. The operation kind
`Shapes` lists the types such a parameter takes. The four measures are
narrower:

- `distance` and `bearing` take `Point`s only;
- `length` takes a `LineString` or a `MultiLineString`;
- `area` takes a `Polygon` or a `MultiPolygon`.

Anything else, a `Geometry` included, is `type.mismatch`, and the message
names this page. To measure a `Geometry`, `match` it first, so your code
decides what, say, a point's area is.

## Operations
<!-- topic: geometry-operations -->

The `geo.` operations measure distances, lengths, directions and areas,
and say whether one shape touches or holds another:

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

Each SQL form is PostGIS's own function for the operation, which
[aggregate queries](06-reads-and-writes.md#the-aggregates) can use. A
`where:` condition doesn't compile to its form: it searches a little
wider and leaves the answer to Jadpo's code, as [Queries](#queries)
explains. `bounding_box` and `centroid` have no use in a query.

Here a bakery measures its van's trips, its round and its patches:

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

A `Point` literal sets every field, its `altitude` as `none` when it has
none ([Objects](02-values-and-types.md#objects)). The tests check the
figures above:

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
    var van_round: LineString = LineString {
        points: [
            Point {
                latitude: 51.5072
                longitude: -0.1276
                altitude: none
            },
            Point {
                latitude: 48.8566
                longitude: 2.3522
                altitude: none
            },
            Point {
                latitude: 51.5072
                longitude: -0.1276
                altitude: none
            }
        ]
    }
    assert round_length(van_round) == 687793.782534metre
}
```

### Distance and length

`geo.distance` agrees with GeographicLib to well under a micrometre
before it rounds: it is within 75 femtometres of every distance in
GeographicLib's published test set
([Precision and determinism](#precision-and-determinism)). PostGIS's
doubles are good to 15 nanometres. London (51.5072° N, 0.1276° W) to
Paris (48.8566° N, 2.3522° E) is 343,896.891267 m. On a sphere it would
be 343,530.338 m, 367 m short (both verified with PostGIS).

`geo.length` adds up each edge's distance as `geo.distance` rounds it, so
a line of two points is exactly their distance. A `MultiLineString`'s
lines are added up the same way. No distance on Earth needs more than 38
digits, and a sum that does is a defect, as it is for `+`.

### Bearing

`geo.bearing` gives a direction to 9 decimal places, as an `Angle` keeps
radians to 9. Two equal points have no direction, so a call maps the
failure `same_point`, as a local time that doesn't exist is mapped
([Zones and local times](10-configuration-and-time.md#zones-and-local-times)).

Two points are the same point when they have the same latitude and the
same longitude, -180 being 180, or are the same pole at any longitude.
Two points a picometre apart do have a direction. Whether two points are
equal is known only when the program runs, so `same_point` is settled
only by `attempt`, as decoding is. `same_point` exists only in code:
`ST_Azimuth` gives `NULL` for two equal points (verified), and a query
never needs a bearing.

A bearing that rounds to 360 is 0. At a pole the bearing is
GeographicLib's, measured from the meridian of the longitude the pole is
given with.

### Area

`geo.area` uses Karney's method for polygons, each edge a geodesic.
PostGIS's `ST_Area` gives 12,308,778,361.47 m² for the 1° square at the
equator (verified); Jadpo gives 12,308,778,361.469453 m², and each
geodesic's area matches GeographicLib's test set to the square
millimetre.

It rounds once. Each ring's area, the smaller of the two sides the ring
bounds, is worked out without rounding. The holes are taken away and a
`MultiPolygon`'s polygons added, and then the total is rounded half to
even to the square millimetre. A hole always lies inside its outer ring
(the `rings_valid` rule under
[Polygons are valid when built](#polygons-are-valid-when-built)), so holes
never outweigh it.

### Centroid

`geo.centroid` gives a point to put a shape's label at. It isn't a
measure. It is worked out on a sphere, as PostGIS's
`ST_Centroid(x, false)` is: each point becomes a unit vector, the vectors
are weighted and added up, and the sum is turned back into latitude and
longitude.

It gives PostGIS's answer, quirks included:

- each point of a `MultiPoint` weighs one;
- each edge of a line gives both of its ends the edge's angle as their
  weight;
- a polygon is cut into triangles, from the first corner of its first
  outer ring to every edge of every ring. Each triangle's centre, its
  corners added up, is weighted by the triangle's area, always counted
  positive.

So a hole adds weight rather than taking it away. A concave polygon's
centroid moves with the corner its ring starts at, as PostGIS's does, and
may lie outside the polygon. Verified: an L-shaped polygon's centroid is
3.214° N, 3.236° E from one starting corner and 4.151° N, 4.159° E from
another.

A point's centroid is the point itself, without its altitude. When the
weights cancel, as for two opposite points, the call is a defect, as
PostGIS gives no number there either. The result is rounded to the
nearest double, as every coordinate is.

### `contains`, `within` and `intersects`

`geo.contains(a, b)` is true when no point of `b` lies outside `a`, and
`geo.within(a, b)` is `geo.contains(b, a)`. `geo.intersects(a, b)` is
true when the two share at least one point, edges included. A
`MultiPolygon` is the union of its parts, which may overlap.

The SQL form, `ST_Covers` on `geography`, compares two polygons ring by
ring, so it parts from that meaning in four cases:

- it doesn't see a hole of `a` that `b` covers: it says a square with a
  hole covers a square over the hole;
- it counts rings lying along each other as crossing: it says a square
  doesn't cover the same square with a hole;
- with an `a` of several parts, it wants all of `b` in one part, so two
  points each in a different part aren't covered.

`geo.contains` keeps to "no point of `b` lies outside `a`" in each of
these cases, in code and in a query ([Queries](#queries)).

### A box as the container

With a `BoundingBox` as `a`, `geo.contains` compares latitudes and
longitudes directly, because a box's edges are parallels and meridians,
not great circles. In code a box can hold any geometry; an edge's highest
or lowest point, and the longitudes between its ends, count. In a query
the box filter works on `Point` fields only ([Queries](#queries)).

### A circle as the container

With a `Circle`, `geo.contains` is `geo.within(distance:)` of the
circle's centre, the radius being the distance. `within(distance:)` measures between points only, so a
circle holds points only. `geo.contains(circle, shape)` and
`geo.within(shape, circle)` with anything but a `Point` are
`type.mismatch`, and so is `geo.within(a, b, distance:)` unless both are
points. The message cites the decision on point-only distance.

### `within` a distance

`geo.within(a, b, distance: d)` is true when the two points are at most
`d` apart. The distance must be proved never below 0 while compiling, as
a count is, or the call is `type.argument_negative`. A length written
out, or a value of a type with `min: 0metre`, is proved.

### Bounding box

`geo.bounding_box` gives the smallest box of parallels and meridians that
holds the shape. A side worked out from an edge is rounded outwards to a
double, so a shape is always inside its own box. The box crosses the
antimeridian when that box is narrower; when two boxes are equally
narrow, it takes the one that doesn't cross.

A shape that holds a pole reaches every longitude: a point at a pole, a
line through one, or a polygon around or touching one. Its box runs from
-180 to 180, and only such a box holds it, whatever longitude a point at
the pole is written with, since every meridian meets there.

### An edge between opposite points

There is no one great circle between two opposite points of the globe,
so a line's edge between them is just its two ends.

### Operations Jadpo doesn't have

`buffer` isn't an operation ([What geospatial leaves out, and what to use instead](#what-geospatial-leaves-out-and-what-to-use-instead)).
Neither is the distance between a point and a line or polygon. It comes
when one of the standard library's
[scenario applications](19-standard-library.md#scenario-applications)
needs it.

## Predicates and shapes
<!-- topic: predicates-and-shapes -->

The predicates `geo.contains`, `geo.within` and `geo.intersects` answer
yes-or-no questions about where things are, and `geo.centroid` and
`geo.bounding_box` give a shape a place for its label and a view that
shows it. Here the bakery uses them on its delivery patches, its pins on
a map and its shops:

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
function passes_through(van_round: LineString, patch: Polygon) -> Delivers {
    return Delivers(geo.intersects(van_round, patch))
}

/// Whether a shop's pin is inside the map a person is looking at.
function in_view(shop_pin: Point, view: BoundingBox) -> InView {
    return InView(geo.within(shop_pin, view))
}

/// Whether two shops are near enough to walk between.
function walkable(from: Point, to: Point, walking_distance: WalkingReach) -> InView {
    return InView(geo.within(from, to, distance: walking_distance))
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

A customer on a patch's edge is inside it. The patch's northern edge is a
great circle, so it bows north past 51.54° N between its corners. A box's
sides are parallels and meridians, so the box that shows the whole patch
reaches a little further north than its corners:

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

Jadpo checks every polygon when it is built or read, so no operation,
query or other system ever meets one whose edges cross. A `Polygon` is
valid when, on the sphere:

- each ring is closed, with four points or more;
- no ring crosses or touches itself, except where its last point meets
  its first. A point repeated in a row, or a ring that turns straight
  back along an edge, touches itself;
- each hole lies inside the outer ring, and no two rings cross, overlap
  or share even one point. The OGC standard lets a hole touch the outer
  ring at one point; Jadpo doesn't, which also keeps a polygon's inside
  in one piece without a check of its own;
- every edge is shorter than half the globe (180° of arc), so the great
  circle it follows is the only one;
- the polygon covers less than half the globe. PostGIS's `geography`
  reads a ring wound either way as the same, smaller area (verified), so
  a ring meant to enclose the larger side couldn't be stored as meant.

A ring's inside is its smaller side, as PostGIS reads it, whichever way
the ring is wound. Jadpo picks the smaller side from the ring's area on
the sphere, added up by Van Oosterom and Strackee's formula in its whole
numbers. The choice is exact unless the two sides are within about
10⁻³⁵ of each other. A ring that lies exactly on one great circle, so
both sides are equal, breaks the rule. A `MultiPolygon` has no rule of
its own: each of its polygons is valid, and they may overlap.

A polygon read from GeoJSON or WKT that isn't valid is refused, `format`
at the ring's place, as the standard library's other checks on reading
are ([Every boundary through the contract](19-standard-library.md#every-boundary-through-the-contract)).
A polygon built in code that isn't valid breaks a named rule,
`rings_valid`, which you map like any rule across fields
([Rules across fields](02-values-and-types.md#rules-across-fields)).

Checking takes time in proportion to about n log n of the points, since
it sweeps over the edges: 0.6 ms for 100 points, 6.8 ms for 1,000 and
68 ms for a country's outline of 10,000 (measured natively on an Apple
M1 Pro).

### Altitudes are all or nothing

Every position in one geometry has an altitude, or none does. WKT can't
write a mix, and PostGIS refuses one ("can not mix dimensionality in a
geometry", verified), so a geometry that mixed them couldn't be stored or
exchanged. A `Point` has one position, so it always agrees. A
multi-geometry's parts must agree with each other.

A mixed geometry read from GeoJSON or WKT is refused, `format` at the
first position of the other size. One built in code breaks a named rule,
`altitudes_agree`, which you map as you map `rings_valid`.

- **Reading.** GeoJSON is checked once the whole geometry is read, every
  position in the order written, and refused `format` at the first of the
  other size (`coordinates.1.0.2`). A multi-geometry's parts are checked
  against each other. WKT's reader refuses a mix at that position too, as
  `malformed`, its one condition for text that isn't a geometry.
- **What a literal decides.** `altitudes_agree` reads each `Point`
  written out in a geometry literal, however deep: a line's points, a
  polygon's rings, a multi-polygon's polygons. An altitude written `none`
  is none. One written out, or of a type that can't be `none` (a
  distance), is one. Anything else, such as a point or a list held in a
  variable, is known only when the program runs. If some positions are
  known to have an altitude and others known to have none, the literal is
  `type.invalid_literal` at the part holding the first of the other size,
  whatever else is unknown. Otherwise the literal is attempted, as it is
  for `rings_valid`. So a line from points held in variables is
  attempted, even when each variable holds a literal: the checker doesn't
  follow a variable's value, here or for `rings_valid`.
- **A polygon's rules in order.** `rings_valid` is checked first, then
  `altitudes_agree`, so a literal or a value that breaks both names
  `rings_valid`. GeoJSON and WKT likewise check a ring before the
  positions' sizes.
- **Nothing later meets a mix.** Storage stores every geometry without
  checking it again, and `geo.to_wkt` never meets a geometry it can't
  write. A mixed geometry that somehow got past every check is the
  runtime's "value of a kind the checker would have refused". The
  PostgreSQL driver refuses to encode a shape that mixes them, since
  EWKB's type flag covers the whole geometry. Each geometry's examples
  under [Geospatial types](19-standard-library.md#geospatial-types) hold
  every point at a height or none, and its refused examples include a
  mix.

A polygon built from values known only when the program runs is
attempted, and you map both rules:

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
        altitudes_agree: NotAPatch  // corners at a height and on the ground
    }
}
```

```jadpo check project=geo-shapes file=patches.test.jadpo
test "a patch whose edges cross isn't one" {
    var bow_start: Point = Point {
        latitude: 0.0
        longitude: 0.0
        altitude: none
    }
    var bow_tie: List<Point> = [
        bow_start,
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
    assert patch_from(bow_tie, bow_start) fails NotAPatch
}

test "a patch with one corner at a height isn't one" {
    var corner: Point = Point {
        latitude: 0.0
        longitude: 0.0
        altitude: none
    }
    var clicked: List<Point> = [
        corner,
        Point {
            latitude: 0.0
            longitude: 1.0
            altitude: 35metre
        },
        Point {
            latitude: 1.0
            longitude: 1.0
            altitude: none
        }
    ]
    assert patch_from(clicked, corner) fails NotAPatch
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
  reason: A `Polygon`'s rings are closed with four points or more, never cross or meet themselves or each other, keep each hole inside the outer ring, and have every edge shorter than half the globe, so the polygon is the smaller side its outer ring bounds (area 20, "Polygons are valid when built"). A literal is checked while compiling.
  next: Close each ring and move its points so no edge meets another, with each hole inside the outer ring (agent)
  rule: type.invalid_literal
  help: https://jadpo.dev/docs/errors/type.invalid_literal

check failed: 1 error, 1 file
```

## Storage
<!-- topic: geospatial-storage (was storage) -->

A geometry field of an entity is stored in one PostgreSQL column of
PostGIS's `geography` type. `geography` holds longitude and latitude on
WGS84 and measures in metres, so nothing converts between the column and
`geo.distance`. The other geospatial types follow the usual rule: native
types where PostgreSQL has them, and a named column for each part of a
value with several ([Storage types](05-data.md#storage-types)).

A `BoundingBox` and a `Circle` aren't `geography`. A box's edges are
parallels and meridians, which a `geography` polygon would turn into
great circles, and a circle has no `geography` form at all.

| Field type | PostgreSQL |
|---|---|
| `Point`, `LineString`, `Polygon`, `MultiPoint`, `MultiLineString`, `MultiPolygon` | `geography`, with `CHECK (ST_SRID(x) = 4326 AND GeometryType(x) = 'POINT')` naming its own type |
| `Geometry` | `geography`, the check allowing any of the six |
| `BoundingBox` | four `numeric` columns, `<field>_south`, `_west`, `_north`, `_east` |
| `Circle` | `<field>_centre geography`, checked as a `Point`'s, and `<field>_radius numeric(38, 6)` in metres |
| `Latitude`, `Longitude`, `Bearing`, `Crs`, `Geohash`, `H3Cell`, `MapTile`, `EastingNorthing` | as [Geospatial types](19-standard-library.md#geospatial-types) has them: `numeric`, `text`, or a column per part |

You never write or change these columns yourself; Jadpo's migrations
make them ([Migrations](05-data.md#migrations)). Here a bakery's position
and its delivery patches are stored, each as it was given:

```jadpo check project=geo-storage file=app.jadpo hidden
application {}
```

```jadpo check project=geo-storage file=domains/baking/types/bakery_name.jadpo hidden
/// What a bakery is called.
type BakeryName = Text {
    max_length: 80
}
```

```jadpo check project=geo-storage file=domains/baking/types/patch_name.jadpo hidden
/// What a delivery patch is called.
type PatchName = Text {
    max_length: 80
}
```

```jadpo check project=geo-storage file=domains/baking/entities/bakery.jadpo
use baking/types { BakeryName }

/// A shop that bakes and delivers.
entity Bakery {
    id: Uuid
    name: BakeryName
    spot: Point

    indexes: {
        by_spot: {
            fields: [spot]
            kind: IndexKind.geographic  // the map service's own lookups
        }
    }
}
```

```jadpo check project=geo-storage file=domains/baking/entities/delivery_patch.jadpo
use baking/entities { Bakery }
use baking/types { PatchName }

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

```jadpo check project=geo-storage file=domains/baking/actions/open_bakery.jadpo hidden migrate=initial
use baking/entities { Bakery }
use baking/types { BakeryName }

/// Opens a bakery at its spot.
action open_bakery(name: BakeryName, spot: Point) -> Bakery {
    return Bakery.create {
        set: {
            name: name
            spot: spot
        }
    }
}
```

```jadpo check project=geo-storage file=domains/baking/actions/open_bakery.test.jadpo
use baking/entities { Bakery }
use baking/types { BakeryName }

test "a bakery's spot reads back as it was given" {
    var spot: Point = Point {
        latitude: 51.5072
        longitude: -0.1276
        altitude: 35metre
    }
    var new_bakery: Bakery = open_bakery(BakeryName("Crumbs"), spot)
    var stored_bakery: Bakery = attempt Bakery.get(new_bakery.id)
    assert stored_bakery.spot == spot
}
```

The initial migration's SQL for `Bakery` creates PostGIS first, in the
same transaction as the table:

```sql
CREATE EXTENSION IF NOT EXISTS "postgis";
CREATE TABLE "bakery" (
    "id" uuid NOT NULL,
    "name" text NOT NULL,
    "spot" geography NOT NULL,
    CONSTRAINT "bakery_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "bakery" ADD CONSTRAINT "bakery_spot_check" CHECK (ST_SRID("spot") = 4326 AND GeometryType("spot") = 'POINT');
CREATE INDEX "bakery_by_spot_idx" ON "bakery" USING gist ("spot");
```

### PostGIS, only when something needs it

The first migration that adds a geometry column or a geometry index
starts with `CREATE EXTENSION IF NOT EXISTS postgis;`, in the migration's
transaction like the rest. An application with no geometry column or
index never creates the extension and never needs PostGIS on its server,
as an application with no entity needs no database
([Data](05-data.md)). [A store with no PostGIS](#a-store-with-no-postgis)
says what happens when the server can't create it.

### One `geography` column, with a check

The column is `geography` with no type of its own, and a check
constraint says which geometry it holds. A typed column such as
`geography(Point, 4326)` would refuse some points: a `Point`'s altitude
may be `none` or present, value by value, and `geography(Point, 4326)`
and even `geography(Geometry, 4326)` refuse a point with an altitude
("Geometry has Z dimension but column does not", verified). An untyped
column takes both. The check refuses a line in a point's column, and a
point in another coordinate system, such as ETRS89's `SRID=4258` (both
verified).

An altitude is stored as the third coordinate. `GeometryType` gives
`POINT` for a point with an altitude (verified, PostGIS 3.4.2), so the
same check holds for both.

The check is added after the table, as
`ALTER TABLE ... ADD CONSTRAINT "<table>_<column>_check"`, as Jadpo adds
every check constraint. A `Geometry`'s check lists the six types with
`IN`, and a circle's centre has a point's check. A box's sides aren't
checked in the database, like every compound type's rules, and a
circle's radius column is commented as a `Length` in metres.

### Stored exactly

PostGIS keeps each coordinate as a double. A coordinate held as the
shortest decimal of a double comes back unchanged; one with more digits
than a double holds doesn't (`51.50720000001234567` came back
`51.50720000001235`, verified). Jadpo holds every coordinate as the
shortest decimal of a double
([Precision and determinism](#precision-and-determinism)), as it holds a
`Vector`'s items as single-precision floats, so every coordinate comes
back unchanged. An altitude has 6 decimal places, so it comes back
unchanged up to a million kilometres (15 significant digits).

### Stored as given, read back as stored

A ring keeps the order its points were given, clockwise or not, as
PostGIS keeps it (verified). Only GeoJSON winds rings by the right-hand
rule. Each coordinate read back is the shortest decimal of its double,
held to its range, and an altitude is that decimal rounded half to even
to the micrometre.

Another program may have written the row, so a polygon read back is
checked as a polygon read from GeoJSON is: closed rings of four points or
more, and `rings_valid`. A polygon that fails, or a geometry of another
kind than the field's, is a fault that names the rule, never the value.

### How a geometry travels to the database

A geometry is sent as a shape of positions, each coordinate the double
its decimal is, so the doubles keep their exact bits. The PostgreSQL
driver sends it in PostGIS's extended WKB (EWKB), a binary form, with
SRID 4326 and a third coordinate where the positions have altitudes. It
sends no type number, so the server takes the column's own type, as
pgvector's values are sent. It reads back the EWKB of `geography_send`.
When the compiler writes a geometry into a statement, it writes EWKT,
each coordinate the shortest decimal of its double. WKB is used only
inside the driver ([Interop](#interop)).

### Indexes

You don't declare the indexes your queries need: the compiler derives
them ([Indexes](05-data.md#indexes)), and a special kind of index takes a
`kind:` option. For geometry fields it derives GiST indexes, the kind
PostGIS searches shapes with:

| Needed by | Index |
|---|---|
| `geo.within`, `geo.contains`, `geo.intersects` or `within(distance:)` on a field | `USING gist ("<field>")`, named `<table>_<field>_idx`, partial for `visible:` as every derived index is |
| `geo.within(field, box)` with a `BoundingBox` | `USING gist (("<field>"::geometry))`, named `<table>_<field>_box_idx` |
| A lookup made from outside the application | `indexes: { by_spot: { fields: [spot], kind: IndexKind.geographic } }` |

A GiST index holds one geometry column. A query that also filters on
other fields uses their ordinary indexes beside it, and PostgreSQL
combines them. `ST_DWithin`, `ST_Covers` and the box test each use these
indexes (verified): the first two through `&&` on the geography's box,
then the exact test.

- **Derived indexes.** A field gets one index however many queries test
  it, and none where an index you declared as geographic already holds
  the field. When the name is taken, the index is named `_gist_idx`
  instead. The box index records its cast to `geometry` in its method.
- **Declared indexes.** For lookups made from outside the application,
  an `indexes:` entry may be a block of `fields:` and `kind:`.
  `kind: IndexKind.geographic` is a GiST index with `geography`'s own
  operator class, on exactly one geometry field (one of the six or
  `Geometry`); anything else is refused as `type.setting_value`. A
  setting a block doesn't have is `type.unknown_setting`, a block without
  `fields:` is `data.field_list_required`, and the same fields and kind
  twice is `data.duplicate_rule`. `IndexKind` has `geographic` and
  `hnsw`, the vector index from [AI](21-ai.md). A plain index on a
  geometry field stays a plain one.

### A store with no PostGIS

An application with a geometry field needs PostGIS on its PostgreSQL
server. What happens on each kind of store:

| Store | What happens |
|---|---|
| Any store, in an application with no geometry column or index | Nothing: PostGIS is never needed |
| PostgreSQL with PostGIS available | The first migration with a geometry column or index creates the extension; nothing to configure. Neon, Supabase, Amazon RDS and Google Cloud SQL all offer PostGIS (assumption from their documentation, not tested here) |
| PostgreSQL without it, or a role that can't create it | `jadpo migrate apply` stops before changing anything, with `migration.extension_missing`. The error names the extension and says that a role allowed to create it runs `CREATE EXTENSION postgis` once. PostGIS isn't a trusted extension, so creating it needs a superuser or the provider's own admin role (verified). The application already refuses to start against a database whose migrations aren't applied ([Migrations](05-data.md#migrations)), which covers the rest |
| Redis ([Entities in Redis](06-reads-and-writes.md#entities-in-redis)), and D1 when it arrives | A geometry field is kept in the entity's JSON as its GeoJSON, as a Redis entity keeps all its fields. Any `geo.` condition or order is the compile error that names PostgreSQL, as for any query a Redis entity can't serve |

So the compiler refuses what it can see, a `geo.` condition on a store
that has no spatial queries, and `jadpo migrate apply` refuses what only
the server knows. Nothing waits for the first query to fail.

`jadpo migrate apply` finds out whether its role may create the extension
by trying. Before anything runs, it creates each extension that a pending
migration needs and the database lacks, inside a savepoint, and rolls it
back. Whether a role may create an extension that isn't trusted depends
on the server and the provider's roles: Amazon RDS's `rds_superuser` and
Neon's `neon_superuser` aren't PostgreSQL superusers (assumption from
their documentation). Once the database has the extension, the
migration's `IF NOT EXISTS` needs nothing more. The same holds for
pgvector.

What `jadpo migrate apply` prints when its role can't create PostGIS:

```text
error: `main` has no `postgis` extension, and its role can't create it
  reason: `postgis` is PostGIS, which keeps the project's geometries in `geography` columns; there is no fallback, since a geometry kept any other way would be measured and compared differently. The project's migrations create it, and its database server offers it, but the role applying migrations isn't allowed to create it, so nothing was applied. PostGIS isn't a trusted extension: a superuser, or the provider's own admin role, creates it.
  next: Have a role allowed to create it run `CREATE EXTENSION postgis` once in this database, then run the command again (human)
  rule: migration.extension_missing
  help: https://jadpo.dev/docs/errors/migration.extension_missing
```

## Queries
<!-- topic: geospatial-queries (was queries) -->

You find records by where they are with a `geo.` call as a query's
`where:` condition: the bakeries within six kilometres of a customer, the
pins inside a map's view, the patches that deliver to a door. You can
also list records nearest first inside a radius.

A query's condition must be one an index can serve: a field, named
first, compared with a value
([Filters](06-reads-and-writes.md#filters)). A GiST index serves each
`geo.` condition, so they are allowed too, with the field first and the
values after. Any other function of a field is still refused.

Here are the bakery's queries, on the bakeries and patches stored above:

```jadpo check project=geo-storage file=domains/baking/types/search_reach.jadpo hidden
/// How far a customer looks for a bakery.
type SearchReach = Length {
    min: 0metre
    max: 50km
}
```

```jadpo check project=geo-storage file=domains/baking/types/bakery_card.jadpo hidden
use baking/entities { Bakery }

/// A bakery in a list or on a map.
type BakeryCard = Output {
    pick: [Bakery.id, Bakery.name, Bakery.spot]
}
```

```jadpo check project=geo-storage file=domains/baking/types/patch_card.jadpo hidden
use baking/entities { DeliveryPatch }

/// A delivery patch in a list.
type PatchCard = Output {
    pick: [DeliveryPatch.id, DeliveryPatch.name]
}
```

```jadpo check project=geo-storage file=domains/baking/actions/draw_patch.jadpo hidden
use baking/entities { Bakery, DeliveryPatch }
use baking/types { PatchName }

/// Draws a patch a bakery delivers to.
action draw_patch(bakery: Bakery, name: PatchName, outline: Polygon) -> DeliveryPatch {
    return DeliveryPatch.create {
        set: {
            bakery_id: bakery.id  // a loaded record's id: the bakery is there
            name: name
            outline: outline
        }
    }
}
```

```jadpo check project=geo-storage file=domains/baking/queries/bakeries_near.jadpo
use baking/entities { Bakery }
use baking/types { BakeryCard, SearchReach }

/// Bakeries near a customer, nearest first.
query bakeries_near(doorstep: Point, max_trip: SearchReach) -> List<BakeryCard> {
    from: Bakery
    where: geo.within(Bakery.spot, doorstep, distance: max_trip)
    order: [geo.distance(Bakery.spot, doorstep) asc]
    limit: 20
}
```

```jadpo check project=geo-storage file=domains/baking/queries/bakeries_in_view.jadpo
use baking/entities { Bakery }
use baking/types { BakeryCard }

/// The pins inside the map a person is looking at.
query bakeries_in_view(view: BoundingBox) -> List<BakeryCard> {
    from: Bakery
    where: geo.within(Bakery.spot, view)
    order: [Bakery.name asc]
    limit: 200
}
```

```jadpo check project=geo-storage file=domains/baking/queries/patches_for.jadpo migrate=geo_queries
use baking/entities { DeliveryPatch }
use baking/types { PatchCard }

/// The patches that deliver to a customer's door.
query patches_for(customer: Point) -> List<PatchCard> {
    from: DeliveryPatch
    where: geo.contains(DeliveryPatch.outline, customer)
    order: [DeliveryPatch.name asc]
    limit: 10
}
```

The tests run on PostGIS. A customer on a patch's edge is delivered to
by the query, as by `geo.contains`, and the bakery 5.3 km away is inside
a six-kilometre trip and outside a five-kilometre one:

```jadpo check project=geo-storage file=domains/baking/queries/patches_for.test.jadpo
use baking/entities { Bakery, DeliveryPatch }
use baking/types { BakeryCard, BakeryName, PatchName }
use baking/actions { draw_patch, open_bakery }
use baking/queries { bakeries_in_view, bakeries_near }

test "bakeries near a door come nearest first, within the trip" {
    var big_ben: Point = Point {
        latitude: 51.5007
        longitude: -0.1246
        altitude: none
    }
    var crumbs: Bakery = open_bakery(
        BakeryName("Crumbs"),
        Point {
            latitude: 51.5
            longitude: -0.2
            altitude: none
        }
    )
    var loaf: Bakery = open_bakery(
        BakeryName("Loaf"),
        Point {
            latitude: 51.5072
            longitude: -0.1276
            altitude: none
        }
    )
    var in_reach: List<BakeryCard> = bakeries_near(big_ben, 6km)
    assert text.join(list.map(in_reach, card -> card.name), ", ") == text.join(
        [loaf.name, crumbs.name],
        ", "
    )
    var within_five: List<BakeryCard> = bakeries_near(big_ben, 5km)
    assert text.join(list.map(within_five, card -> card.name), ", ") == loaf.name
}

test "a customer on a patch's edge is delivered to" {
    var crumbs: Bakery = open_bakery(
        BakeryName("Crumbs"),
        Point {
            latitude: 51.5
            longitude: -0.2
            altitude: none
        }
    )
    var outline: Polygon = Polygon {
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
    var westminster: DeliveryPatch = draw_patch(crumbs, PatchName("Westminster"), outline)
    assert text.join(
        list.map(patches_for(crumbs.spot), card -> card.name),
        ", "
    ) == westminster.name
    var in_view: List<BakeryCard> = bakeries_in_view(geo.bounding_box(outline))
    assert text.join(list.map(in_view, card -> card.name), ", ") == crumbs.name
}
```

`jadpo sql` shows each query's statement, and under `then:` what runs
after it. The box's envelopes and the number of rows to fetch are worked
out as the read runs. Below are the statements and the GiST indexes
derived for them. `Bakery`'s declared `by_spot` index already serves
`spot`, so the only index derived on `spot` is the box's:

```sql
-- bakeries_near
SELECT "id", "name", "spot", ST_Distance("spot", $1) AS "jadpo_measured"
FROM "bakery"
WHERE ST_DWithin("spot", $1, $2 + 0.001)
ORDER BY "jadpo_measured", "id"
LIMIT $3

-- bakeries_in_view
SELECT "id", "name", "spot"
FROM "bakery"
WHERE ("spot"::geometry && ST_MakeEnvelope($3, $1, $4, $2, 4326) OR "spot"::geometry && ST_MakeEnvelope($5, $1, $6, $2, 4326))
ORDER BY "name" COLLATE "C" ASC, "id" ASC
LIMIT $7

-- patches_for, its outline selected for the code to decide
SELECT "id", "name", "outline"
FROM "delivery_patch"
WHERE ST_DWithin("outline", $1, 0.001)
ORDER BY "name" COLLATE "C" ASC, "id" ASC
LIMIT $2

CREATE INDEX "bakery_spot_box_idx" ON "bakery" USING gist (("spot"::geometry));
CREATE INDEX "delivery_patch_outline_idx" ON "delivery_patch" USING gist ("outline");
```

### What each condition means

| In a query | Means | The index searches | Then the code decides |
|---|---|---|---|
| `where: geo.within(Bakery.spot, doorstep, distance: max_trip)` | Bakeries within `max_trip` of `doorstep`; a `Point` field and value | `ST_DWithin("spot", $1, $2 + 0.001)` | `geo.within(spot, doorstep, distance: max_trip)` |
| `where: geo.within(Bakery.spot, area)` | Inside a polygon or multi-polygon value | `ST_DWithin("spot", $1, 0.001)` | `geo.within(spot, area)` |
| `where: geo.within(Bakery.spot, view)` | Inside a `BoundingBox`; a `Point` field only | `"spot"::geometry && ST_MakeEnvelope(...)`, two envelopes (below) | `geo.within(spot, view)` |
| `where: geo.within(Bakery.spot, circle)` | Inside a `Circle`; a `Point` field only | `ST_DWithin("spot", $centre, $radius + 0.001)` | `geo.within(spot, circle)` |
| `where: geo.contains(DeliveryPatch.outline, customer)` | Patches holding a point or shape | `ST_DWithin("outline", $1, 0.001)` | `geo.contains(outline, customer)` |
| `where: geo.intersects(DeliveryPatch.outline, area)` | Patches touching a shape | `ST_DWithin("outline", $1, 0.001)` | `geo.intersects(outline, area)` |
| `order: [geo.distance(Bakery.spot, doorstep) asc]` | Nearest first; needs a `within(distance:)` on the same field and point | `ORDER BY ST_Distance("spot", $1), "id"` | sorted again by `geo.distance`, then `id` |

### The index finds, the code decides

A query gives exactly the records that the code's own `geo.` operation
keeps, comparing the field's stored value with the call's values.
`list.filter` over every record with the same operation would give the
same list.

PostGIS's own predicates can't be the condition. PostGIS decides in
doubles with its own tolerances, and its `ST_Covers` on `geography`
compares two polygons ring by ring, which parts from `geo.contains` in
the cases under
[`contains`, `within` and `intersects`](#contains-within-and-intersects).
So the statement uses the GiST index to find every record within a
millimetre of what the condition says, and no record the code keeps is
left out. The runtime then works the condition out again on each record
found, with the same `geo.within`, `geo.contains` and `geo.intersects`
that a body calls, so the interpreter and every compiled target decide
alike. A point on an edge is in, and those cases give this page's
answer.

### A margin of a millimetre

The search is a millimetre wider than the condition. For a radius or a
circle it is `ST_DWithin(field, point, distance + 0.001)`. For `within`,
`contains` and `intersects` it is `ST_DWithin(field, shape, 0.001)`,
since anything inside a shape, on its edge or touching it is no distance
from it, whether PostGIS compares ring by ring or not.

PostGIS's distances are good to some nanometres, so a millimetre, some
hundred thousand times its error, is far more than it can miss. It is
also close enough that the search rarely finds a row the code turns down.
The cases where `ST_Covers` parts from `geo.contains` are each within the
margin: `ST_DWithin` is true where `ST_Covers` is false (verified).

### Searching a box

A box's sides are parallels and meridians, so the box filter searches the
point cast to `geometry`, whose `&&` compares longitude and latitude
directly. It has an index of its own.

The filter is always two envelopes, so the statement's text stays fixed.
A box across the antimeridian searches from its west side to 180 and from
-180 to its east side. A box with a side on the antimeridian also
searches the line at the other end, -180 being 180. Any other box is
searched twice as one envelope. The envelopes are worked out from the
box's west and east sides when the read runs.

`&&` rounds to single precision, outwards. A point at a pole is in only a
box of every longitude, which the code decides.

### Nearest first inside a radius

`order: [geo.distance(field, point) asc]` lists records nearest first,
and needs a `geo.within(distance:)` condition on the same field and
point. The statement orders the rows inside the radius by `ST_Distance`,
and the runtime sorts them again by `geo.distance`, rounded to the
micrometre as code gives it, then by `id`. So two bakeries the same
distance away come in `id` order, and the order is the code's to the
last digit.

PostGIS's index-ordered `<->` operator can't give that order, because it
measures on a sphere: London to Paris is 343,530 m by `<->` and 343,897 m
by `ST_Distance` (verified). Ordering by it would disagree with
`geo.distance` for two shops a few hundred metres apart in distance.

### Only `asc`, and only in a `List`

For "nearest anywhere", use a large radius. Farthest first can't use the
radius to limit anything, and a page by distance would need a cursor on
a computed value, so both are refused, as `query.order_direction` and
`query.order_field`, with reasons naming this page. `geo.distance` is the
query's only order.

### How many rows a query fetches

The statement asks for as many rows as the read keeps: a list's
`limit:` as `LIMIT $n`, a page's size and one more, or every row for a
query of one record found by a unique field.

It may have to ask again. The code may turn some rows down, or a row past
those fetched could still be nearer than the last one kept: PostGIS's
distance to it, less the margin, isn't past the code's distance to the
last. Then it asks for four times as many, until it has enough or the
rows run out. Each ask runs the whole statement again in the same
transaction and keeps the last answer, so it reads one state of the
table. A page's cursor comes from the rows the code kept. The margin is a
millimetre, so a second ask is rare: it needs rows within a millimetre of
the condition, or one of the cases where `ST_Covers` parts from
`geo.contains`.

### The result doesn't carry the distance

A result type picks fields
([Reads and writes](06-reads-and-writes.md)), and a distance isn't one.
The action works the distance out with `geo.distance`, which gives the
same metres the query ordered by.

### Where a condition can go

A `geo.` condition is a `where:` line of its own, or joined to the others
with `and`. An index finds the records a condition holds for, never those
it doesn't, so a `geo.` condition can't be under `not` or `or`. Under
`or`, the rows the code turns down would need the other side's answer,
which the statement doesn't give. Its value is always given, so it isn't
a `where_present:` line.

A record whose field is `none` meets no `geo.` condition. The values are
parameters, or fields inside a parameter. `within`'s `distance:` may also
be a length written out, proved never negative as in code.

### What each form takes

Each form takes what it takes in code:

- `within(distance:)` and a circle need a `Point` field, or the query is
  `type.mismatch`;
- a box needs a `Point` field, or the query is
  `query.condition_unsupported`;
- `within` takes a `Polygon`, a `MultiPolygon`, a `BoundingBox` or a
  `Circle`;
- `contains` and `intersects` take any geometry.

The messages cite the decisions on point-only distance and on the box
filter. A `BoundingBox` or `Circle` field, kept in named columns, can't
be tested.

### Comparing geometries with `==`

`==` on a geometry field compares exactly, as PostGIS's `=` on
`geography` does: an altitude, a ring's winding and a line's direction
all count (verified), as they do for Jadpo's own `==`. You can't order by
a geometry; that is still `query.order_field`.

### Errors in a `geo.` query

These are the usual query rules, with reasons of their own:

- a `geo.` call with the field anywhere but first, a box on a field that
  isn't a `Point`, a `BoundingBox` or `Circle` field, or a condition under
  `not` or `or` is `query.condition_unsupported`;
- a value of the wrong kind is `type.mismatch`, and a distance that may
  be negative is `type.argument_negative`, as in code;
- an order by `geo.distance` without its radius, beside another order or
  in a page, and any other function of a field in `order:`, is
  `query.order_field`.

`migration.extension_missing` is the only rule of this page's own.

## Precision and determinism
<!-- topic: precision-and-determinism -->

A `geo.` operation gives the same digits on the interpreter, a native
build and a wasm32 build, and a query gives the same records the code
would keep. Floats appear only where PostGIS stores and searches:

| Where | Arithmetic | Same on every target? |
|---|---|---|
| `geo.` operations in code: interpreter, native, wasm32 | Jadpo's own whole-number fixed point, 256 bits with 136 after the point (about 41 decimal places), its `sqrt` a whole square root and its `sin`, `cos` and `atan2` the series of the standard library's decimal maths ([Numbers](19-standard-library.md#numbers)); results rounded half to even to the result type's scale | Yes: integer arithmetic only, no platform maths library |
| Reading GeoJSON | The double JSON gives, held as its shortest decimal | Yes |
| Coordinates built in code | Rounded to the nearest double's shortest decimal when built | Yes |
| Storage | PostGIS doubles; exact, given the row above | Yes |
| Query conditions and order | PostGIS's doubles find the rows, through the index and a millimetre wider than the condition; Jadpo's own code decides each and sorts them | Yes: the code's answer |

### No floats in Jadpo's own code

Karney's method needs additions, multiplications, divisions, square
roots, `sin`, `cos` and `atan2`. All of it runs on whole numbers far
wider than the result, and is rounded once at the end. A float's `sin`
differs between platforms' maths libraries, so a float version could give
different digits on wasm32.

Each result is rounded half to even from the exact binary value. It isn't
correctly rounded, since the series are finite, but it is the same digits
on every target. The checker and the runtime share the code, so a polygon
literal is read by the same code the runtime runs.

### Coordinates are doubles' shortest decimals

A `Latitude` or a `Longitude`, as a value, a field or a point's part, is
rounded to the shortest decimal of the nearest IEEE double as it is
built, before its range is checked, as a `Vector`'s items are. That is
the same on every target. Every coordinate with up to 15 significant
digits, which is all a person writes and every GPS gives, is unchanged.

Rounding can't take a value in range out of it, since ±90 and ±180 are
doubles. A double whose shortest decimal has more than 38 places (below
10⁻²³ of a degree) is rounded half to even to 38. A literal is checked as
written.

### Checked against reference answers

`geo.distance` gives the reference distance of Karney's published test
set of 10,000 geodesics, rounded half to even to the micrometre. Before
rounding, every distance is within 75 femtometres of the reference.
`geo.bearing` and `geo.area` are checked against the same set, and every
predicate, centroid and box against PostGIS's answers on 169 shapes:
small, continental and pole-holding polygons, polygons across the
antimeridian and the equator, holes, concave shapes, lines and
multi-part shapes. Every yes or no is PostGIS's, except where `ST_Covers`
parts from `geo.contains`
([`contains`, `within` and `intersects`](#contains-within-and-intersects)).
The gate runs both checks natively and under WebAssembly, which give the
same answers. The [design notes](notes/20-geospatial.md#how-the-geodesics-are-checked)
give the method and its tolerances.

### Predicates are exact

Whether a point is inside a shape, or two shapes meet, is decided
exactly, by whole numbers, with no tolerance. So a point exactly on an
edge, a corner, a meridian or the equator is found on it, and inside
includes the edge. Where three points lie exactly on one great circle,
the tie is broken by a fixed rule (Simulation of Simplicity, as Google's
S2 library does). No float decides anything.

### What it costs

These figures are measured natively on an Apple M1 Pro
(`cargo run --release -p jadpo-value --example geodesics -- bench`).

- **Measures:** a distance along a meridian takes 9 µs; 2 km across
  London 31 µs; London to Paris 42 µs; London to Sydney 58 µs; nearly
  antipodal 64 µs. A bearing takes 47 µs, an 11-point line's length
  315 µs, and 71 µs for the 1° square's area (meridians and the equator
  are quick). A 100-point patch's area takes 4.5 ms, about 45 µs an edge.
- **Under WebAssembly:** checking 500 of the test set's geodesics takes
  0.30 s natively and 0.83 s under Node's WebAssembly, about 2.8 times, so a distance there
  takes about 110 µs, and 10 ms of CPU measures about 90 distances.
- **Predicates and shapes:** a point in a 100-point patch takes 0.44 ms,
  in a 1,000-point one 4.3 ms. A 100-point patch in another takes 30 ms,
  since each piece of every edge is classified, about n² in the points.
  Two 100-point patches intersecting take 0.9 ms, `within(distance:)`
  33 µs, a 100-point patch's centroid 0.37 ms and its box 1.2 ms. Turning
  each corner into a direction (two sines and cosines) is most of a small
  polygon's cost.

A request measures a handful of things. A batch measuring millions, or
the area of a 10,000-point outline (about half a second), would notice.
No time limit is enforced, as none is elsewhere; the figures are the
baseline that tuning measures against.

### Where code and the database could disagree

They could disagree on a point on, or within a hair of, a polygon's edge,
and on a distance within a hair of a radius: PostGIS decides in doubles
with its own tolerances, and Jadpo exactly. `ST_Covers` also parts from
`geo.contains` on four kinds of polygon. So a query's condition uses
PostGIS only to find the rows, through the index, a millimetre wider than
the condition, and the runtime decides each row with the same code a body
runs ([Queries](#queries)). The index is used, and the answer is the
code's.

The cost is the code's test on each row found: about 40 µs a distance
natively, and 0.44 ms for a point in a 100-point patch. So a radius
holding a thousand bakeries spends some 40 ms sorting them by distance.

## Interop
<!-- topic: geospatial-interop (was interop) -->

Other systems send and expect geometries in their own formats: map
libraries take GeoJSON layers, spreadsheets and other systems' APIs hold
WKT text, and trackers send pairs of numbers. Jadpo reads and writes each
at the edge of your app, and only there does the order of latitude and
longitude matter:

| Format | What Jadpo does | Where the axis order lives |
|---|---|---|
| GeoJSON geometries | As [Geospatial types](19-standard-library.md#geospatial-types) has them: strict RFC 7946, every ring written by the right-hand rule | `geo.from_geojson` and the wire |
| GeoJSON `Feature`, `FeatureCollection` | Two standard generic types, `Feature<G, P>` (a `geometry` of type `G` and `properties` of type `P`) and `FeatureCollection<G, P>` (its `features`), read and written as RFC 7946 objects | The geometry inside |
| `GeometryCollection` | Still refused | |
| WKT and EWKT | `geo.from_wkt(text)` failing `malformed` or `out_of_range`, and `geo.to_wkt(shape)`, longitude first as PostGIS writes it; `SRID=4326;` accepted, any other SRID `out_of_range` | Those two operations |
| WKB | The PostgreSQL driver's own encoding; no operation | Inside the driver |
| Other coordinate systems | Left out: `EastingNorthing` stays a value with its `Crs`, not converted | |

### GeoJSON features and layers

A map library such as Leaflet or MapLibre takes a layer of features: a
`FeatureCollection`, each `Feature` a geometry with properties such as a
name. Jadpo has the two as standard types, so you don't declare your own.

Here is the bakery map's delivery layer. A feature's identity goes in its
properties, so the bakery a patch belongs to is a property. A layer read
from a mapping library goes back to it byte for byte:

```jadpo check project=geo-interop file=bakeries.jadpo hidden
type PatchName = Text {
    max_length: 80
}

type LayerJson = Text {}

type NotALayer = Failure {
    kind: FailureKind.invalid_value
}

/// A shop that bakes and delivers.
entity Bakery {
    id: Uuid
    spot: Point
}

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

```jadpo check project=geo-interop
type PatchProperties = Object {
    name: PatchName
    bakery_id: Bakery.id
}

/// The map's delivery layer, as a mapping library reads it.
function patch_layer(patches: List<DeliveryPatch>) -> FeatureCollection<Polygon, PatchProperties> {
    return FeatureCollection {
        features: list.map(
            patches,
            patch -> Feature {
                geometry: patch.outline
                properties: PatchProperties {
                    name: patch.name
                    bakery_id: patch.bakery_id
                }
            }
        )
    }
}

/// A layer as the map is sent it.
function layer_sent(layer: FeatureCollection<Polygon, PatchProperties>) -> LayerJson {
    return LayerJson(json.write(layer))
}

/// A layer the planner drew, as its mapping library sends it.
function drawn_layer(
    sent: LayerJson
) fails NotALayer -> FeatureCollection<Polygon, PatchProperties> {
    return attempt json.read(sent, as: FeatureCollection<Polygon, PatchProperties>) {
        malformed: NotALayer
        mismatch: NotALayer  // an `id`, a missing geometry, altitudes on some corners only
    }
}
```

```jadpo check project=geo-interop file=main.test.jadpo
test "a drawn layer goes back to the map as it came" {
    var sent: LayerJson = LayerJson(
        "{\"type\":\"FeatureCollection\",\"features\":[{\"type\":\"Feature\",\"geometry\":{\"type\":\"Polygon\",\"coordinates\":[[[-0.2,51.49],[-0.05,51.49],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]]]},\"properties\":{\"name\":\"Soho\",\"bakery_id\":\"7b0c1f8e-3a52-4d7e-9a41-2f1d5c6e8b90\"}}]}"
    )
    var layer: FeatureCollection<Polygon, PatchProperties> = attempt drawn_layer(sent)
    assert layer_sent(layer) == sent
    assert list.count(layer.features) == 1
}

test "a feature's identity is in its properties" {
    assert drawn_layer(
        LayerJson(
            "{\"type\":\"FeatureCollection\",\"features\":[{\"type\":\"Feature\",\"id\":7,\"geometry\":{\"type\":\"Polygon\",\"coordinates\":[[[-0.2,51.49],[-0.05,51.49],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]]]},\"properties\":{\"name\":\"Soho\",\"bakery_id\":\"7b0c1f8e-3a52-4d7e-9a41-2f1d5c6e8b90\"}}]}"
        )
    ) fails NotALayer
}

test "a patch with one corner at a height is refused" {
    assert drawn_layer(
        LayerJson(
            "{\"type\":\"FeatureCollection\",\"features\":[{\"type\":\"Feature\",\"geometry\":{\"type\":\"Polygon\",\"coordinates\":[[[-0.2,51.49],[-0.05,51.49,35],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]]]},\"properties\":{\"name\":\"Soho\",\"bakery_id\":\"7b0c1f8e-3a52-4d7e-9a41-2f1d5c6e8b90\"}}]}"
        )
    ) fails NotALayer
}
```

- **A feature's arguments.** `G` is one of the six geometries or
  `Geometry`. `P` is an `Object` type of your application's, whose fields
  are the properties; an `Input`, an `Output` or a standard object isn't
  allowed. Either kind of wrong argument is refused as `type.mismatch`,
  where the type is written or where a literal's values give it. A
  literal takes its arguments from the type its place expects, or else
  from its values: a feature's from its geometry and its properties, a
  collection's from its features.
- **Read strictly.** A feature's members are `type`, which must be
  `Feature` (another is `variant`), `geometry`, `properties`, and the
  `bbox` that RFC 7946 §5 lets any object carry. A `bbox` is checked as
  four or six numbers and ignored, as a geometry's is. An `id` or any
  other member is `unknown_field`: your application's identity for a
  feature goes in `properties`. A `geometry` of `null`, which RFC 7946
  allows, is `required`, since a feature with no geometry is left out;
  so are `properties` of `null` or none. A collection's members are
  `type`, `features` and `bbox`. A collection of no features is read, as
  an empty layer, though a multi-geometry of no parts isn't. Each problem
  is reported at its place, such as `features.0.properties.vans`. A
  feature or collection is written in that order, `type` first, with no
  `bbox`.
- **No text form, and no form for people.** `text.from`, `text.parse`
  and `text.format` on a feature are `type.mismatch`; its geometry and
  its properties have theirs. A feature field in an entity is stored as
  its GeoJSON in `jsonb`, as
  [structured values](05-data.md#structured-values) are.

A route sends a layer bare, as it sends a `List` or `Page` of an `Output`
([Output and status](07-http.md#output-and-status)), so a mapping library
fetches it as the whole response, plain RFC 7946 GeoJSON. Its properties
hold only what an `Output` may, so only fields declared for clients leave
the server. A layer may also be a field of an `Output`. OpenAPI describes
it as RFC 7946's object with each feature's geometry and properties, and
the API record names it, as, say, `FeatureCollection<Point, PinProperties>`:

```jadpo check project=geo-layer-route
type PinLabel = Text {
    max_length: 80
}

type LayerJson = Text {}

type PinProperties = Object {
    label: PinLabel
}

/// The bakeries on the map, each pin labelled.
action bakery_pins() -> FeatureCollection<Point, PinProperties> {
    return FeatureCollection {
        features: [
            Feature {
                geometry: Point {
                    latitude: 51.5136
                    longitude: -0.1365
                    altitude: none
                }
                properties: PinProperties {
                    label: PinLabel("Soho")
                }
            }
        ]
    }
}

routes Pins {
    prefix: /pins

    POST / {
        authentication: RouteAuthentication.none
        output: FeatureCollection<Point, PinProperties>
        run: bakery_pins()
    }
}

policy {
    actions: {
        bakery_pins: [Access.public]
    }
}

test "the map fetches its pins as GeoJSON" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/pins"
    }
    assert response.status == 200
    assert response.output == bakery_pins()
    assert LayerJson(json.write(bakery_pins())) == LayerJson(
        "{\"type\":\"FeatureCollection\",\"features\":[{\"type\":\"Feature\",\"geometry\":{\"type\":\"Point\",\"coordinates\":[-0.1365,51.5136]},\"properties\":{\"label\":\"Soho\"}}]}"
    )
}
```

This route answers `POST`, since an action builds its layer. A layer of
stored records comes from a query, which a `GET` route runs, so a mapping
library fetches it from the route's address
([A query giving a map layer](06-reads-and-writes.md#a-query-giving-a-map-layer)).

### WKT and EWKT

WKT is the text form of a geometry that spreadsheets, CSV exports and
other systems' APIs use, such as `POINT(-0.1276 51.5072)`, longitude
first. EWKT is PostGIS's form of it, which starts with the coordinate
system, as in `SRID=4326;POINT(...)`. WKB, the binary form, only passes
between programs and databases, so it stays inside the PostgreSQL driver.

The planner's spreadsheet holds each outline as WKT:

```jadpo check project=geo-wkt
type SheetText = Text {}

type NotAShape = Failure {
    kind: FailureKind.invalid_value
}

/// A patch's outline from the planner's spreadsheet, as WKT.
function outline_from(sheet_text: SheetText) fails NotAShape -> Geometry {
    return attempt geo.from_wkt(sheet_text) {
        malformed: NotAShape  // not WKT, rings that cross, or a height on some corners only
        out_of_range: NotAShape  // past the poles, or another system
    }
}

/// An outline for the planner's spreadsheet, longitude first.
function sheet_text_for(outline: Geometry) -> SheetText {
    return SheetText(geo.to_wkt(outline))
}
```

```jadpo check project=geo-wkt file=main.test.jadpo
test "a patch comes back from the spreadsheet as it went" {
    var sheet_text: SheetText = SheetText(
        "POLYGON((-0.2 51.49,-0.05 51.49,-0.05 51.54,-0.2 51.54,-0.2 51.49))"
    )
    var outline: Geometry = attempt outline_from(sheet_text)
    assert sheet_text_for(outline) == sheet_text
    var respaced: Geometry = attempt outline_from(
        SheetText(
            "SRID=4326;polygon ((-0.2 51.49, -0.05 51.49, -0.05 51.54, -0.2 51.54, -0.2 51.49))"
        )
    )
    assert respaced == outline
}

test "what isn't an outline is refused" {
    assert outline_from(SheetText("POINT(-0.1276 91)")) fails NotAShape
    assert outline_from(SheetText("SRID=4258;POINT(-0.1276 51.5072)")) fails NotAShape
    assert outline_from(SheetText("POLYGON((0 0,10 10,0 10,10 0,0 0))")) fails NotAShape
    assert outline_from(SheetText("LINESTRING(0 0,1 1 35)")) fails NotAShape
}
```

- **`geo.from_wkt` gives a `Geometry`,** the variant of the kind the
  text names, which your code matches, as it matches a `Geometry` read
  from GeoJSON. There is no `as:` to pick a kind, so you never have to
  say what a polygon read as a line fails with.
- **What is read.** The six geometries, keywords in any case, `Z`
  written apart or joined (`POINT Z (...)`, `POINTZ(...)`), a
  multi-point's points with or without brackets, any spacing, numbers
  with an exponent, and `SRID=4326;` before it. A position is two or
  three numbers, three for `Z`. Each is read as the double it is and held
  as that double's shortest decimal
  ([Precision and determinism](#precision-and-determinism)). A third
  number without `Z` is an altitude, as PostGIS reads it.
- **What is refused.** `M` and `ZM`, `EMPTY`, `GEOMETRYCOLLECTION`, the
  curves and every other kind are `malformed`, and so are positions
  mixing altitudes ([Altitudes are all or nothing](#altitudes-are-all-or-nothing)).
  A coordinate out of its range is `out_of_range`, where PostGIS's
  `ST_GeogFromText` coerces it into range with a notice (verified,
  PostGIS 3.4.2). Any SRID but 4326 is `out_of_range` too, though
  PostGIS's `geography` takes other geographic systems (`SRID=4258;` is
  read, verified).
- **Rings are kept as written,** as storage keeps them, not rewound: only
  GeoJSON winds rings by the right-hand rule. A polygon is checked as one
  read from GeoJSON is, closed rings of four points or more and
  `rings_valid`, and one that isn't valid is `malformed`.
- **The exact place.** The reader reports the character it stopped at,
  counted from 0, with the condition: the start of a word that isn't a
  geometry, the bracket of a ring or line that isn't one, the number out
  of range, or the SRID's code. The operation's failure carries the
  condition only, as `encoding.from_hex`'s does; the place is for the
  tests, and for a later failure that receives it.
- **`geo.to_wkt` writes `ST_AsText`'s form:** the keyword in capitals,
  ` Z ` before the brackets when every position has an altitude, no
  space after a comma, a multi-point's points each in brackets, each
  coordinate the decimal the value holds, and an altitude in metres with
  no trailing zeros. `ST_AsText` keeps at most 15 places after the
  point, so a coordinate with more (below 10⁻¹⁵ of a degree) comes back
  from PostGIS rounded: `0.30000000000000004` is written `0.3`
  (verified). Every other geometry, such as a patch with a hole, a patch
  wound the other way or one at a height, goes into PostGIS as WKT or
  EWKT and comes back exactly as written.

### Pairs of numbers and GeoJSON text

A van's tracker sends its position latitude first, the map's pin
longitude first, and a planner pastes a shape from a drawing tool as
GeoJSON text. Each operation's name says which number comes first, so a
swapped pair is a call that reads wrong, not a value that looks right:

```jadpo check project=geo-pairs
type TrackerReading = Decimal {
    scale: 9
}

type PastedText = Text {}

type NotAPlace = Failure {
    kind: FailureKind.invalid_value
}

/// Where a van's tracker says it is, latitude first.
function van_at(tracker_fix: List<TrackerReading>) fails NotAPlace -> Point {
    return attempt geo.from_lat_lng(tracker_fix) {
        malformed: NotAPlace  // not two numbers, or three with a height
        out_of_range: NotAPlace  // past a pole, or past the antimeridian
    }
}

/// Where a pin dropped on the map is, longitude first, as GeoJSON writes it.
function pin_at(pin_dropped: List<TrackerReading>) fails NotAPlace -> Point {
    return attempt geo.from_lng_lat(pin_dropped) {
        malformed: NotAPlace
        out_of_range: NotAPlace
    }
}

/// A patch pasted from a drawing tool, as GeoJSON.
function pasted_patch(pasted: PastedText) fails NotAPlace -> Geometry {
    return attempt geo.from_geojson(pasted) {
        malformed: NotAPlace  // not GeoJSON, a feature, or rings that cross
        out_of_range: NotAPlace  // past a pole, or past the antimeridian
    }
}
```

```jadpo check project=geo-pairs file=main.test.jadpo
test "each pair says which way round it is" {
    var van_spot: Point = attempt van_at([51.5136, -0.1365])
    var map_pin: Point = attempt pin_at([-0.1365, 51.5136])
    assert van_spot == map_pin
    assert geo.to_wkt(map_pin) == "POINT(-0.1365 51.5136)"
    assert van_spot == Point {
        latitude: 51.5136
        longitude: -0.1365
        altitude: none
    }
    assert van_at([91.0, 0.0]) fails NotAPlace
    var far_east: Point = attempt pin_at([91.0, 0.0])
    assert far_east.longitude == 91.0
    assert van_at([51.5136]) fails NotAPlace
}

test "a pasted patch is read longitude first" {
    var patch_text: PastedText = PastedText(
        "{\"type\":\"Polygon\",\"coordinates\":[[[-0.2,51.49],[-0.05,51.49],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]]]}"
    )
    var patch: Geometry = attempt pasted_patch(patch_text)
    assert PastedText(json.write(patch)) == patch_text
    assert pasted_patch(
        PastedText("{\"type\":\"Point\",\"coordinates\":[-0.1365,91]}")
    ) fails NotAPlace
    assert pasted_patch(PastedText("POINT(-0.1365 51.5136)")) fails NotAPlace
}
```

- **`geo.from_geojson(text)` gives a `Geometry`,** the variant of the
  kind the text names, as `geo.from_wkt` does, read by the one GeoJSON
  reader a geometry on the wire is: strict RFC 7946, each ring closed,
  valid and rewound by the right-hand rule, every position's altitude
  given or none. Only the six geometries are read; a `Feature`, a
  `FeatureCollection` or a `GeometryCollection` is `malformed`, since a
  layer is read with `json.read(text, as: FeatureCollection<G, P>)`.
  Text that isn't one JSON document or isn't one of the six is
  `malformed`, and so is a ring that isn't closed or valid, or positions
  of mixed sizes. A coordinate out of its range, or an altitude past a
  `Length`'s, is `out_of_range`. The reader reports the place
  (`coordinates.0.3`) and the rule; the operation's failure carries the
  condition only, as `geo.from_wkt`'s does. It takes `Text` only, not
  `Bytes`.
- **A pair is two numbers or three.** `geo.from_lng_lat(numbers)` and
  `geo.from_lat_lng(numbers)` take a `List` of values built on `Int` or
  `Decimal` and give a `Point`. A third number is the altitude in
  metres, as GeoJSON's third is, a `Length` rounded half to even to the
  micrometre. Any other count is `malformed`. Each coordinate is first
  held as the shortest decimal of its nearest double, as a `Point` built
  in code is ([Precision and determinism](#precision-and-determinism)),
  and that value is then held to its range, as GeoJSON's numbers are. So
  `90.00000000000000001` is 90 and in range; past it is `out_of_range`. The order lives only in the name: a
  latitude of 91 is `out_of_range` from `from_lat_lng` and a longitude of
  91 from `from_lng_lat`.
- **Always attempted.** What a list or a text holds is known only when
  the program runs, so each call maps both conditions, or `invalid:`,
  even for a list written out, as `geo.from_wkt` does. No literal is
  decided while compiling.
- **No `geo.to_geojson`.** `json.write` of a geometry already writes its
  GeoJSON, longitude first, so a pair or a text read here goes back out
  through it or `geo.to_wkt`.

## What geospatial leaves out, and what to use instead
<!-- topic: geospatial-left-out -->

Some geospatial work is left out of the language. Each row says why, and
what to use instead:

| Left out | Why | Instead |
|---|---|---|
| `geo.buffer` | PostGIS buffers a `geography` by projecting it to a planar system it picks, so the result depends on PROJ's choice and can't be reproduced in Jadpo's code | `within(distance:)` and `Circle` for "near"; a provider package for drawn buffers |
| Coordinate systems other than WGS84 (British National Grid and the rest) | Accurate conversion needs datum grids (OSTN15 for Great Britain), megabytes of data per system that every wasm32 build would carry | `EastingNorthing` kept with its `Crs`; a provider package later |
| Straight edges on longitude and latitude (`geometry` columns) | Two edge meanings would split every operation in two; the standard library stores geometries as `geography` | Great circles ([Edges are great circles](#edges-are-great-circles)) |
| Routing, travel times, geocoding | Network data and paid services | Provider packages ([Integrations](11-integrations.md)) |
| Nearest first without a radius, farthest first, pages by distance | Disagree with `geo.distance` or can't use the index | A radius ([Nearest first inside a radius](#nearest-first-inside-a-radius)) |
| Simplifying, unions, splitting at the antimeridian | None of the standard library's [scenario applications](19-standard-library.md#scenario-applications) needs them; each is a large algorithm | Later, when one does |
| `GeometryCollection`, and a `Feature` with no geometry | Rare, and each makes every consumer handle an extra case | A `MultiPolygon`, or the right geometry type |
| Altitude in any operation | WGS84's operations are on the surface; height above the ellipsoid isn't height above the ground | Kept and stored, never measured |

## What is built
<!-- topic: geospatial-built -->

| Feature | Status |
|---|---|
| Everything on this page but the rows below | Built. |
| Geometries in Redis | Not built yet: waits for entities in Redis ([Entities in Redis](06-reads-and-writes.md#entities-in-redis)). Until then nothing stores a geometry there. A Redis entity will keep a geometry as its GeoJSON, and every geometry, with and without altitude, comes back from its GeoJSON unchanged, except that a polygon wound clockwise comes back wound by the right-hand rule, which PostGIS storage doesn't do. |
| A `geo.` condition in an include or an aggregate query | Not built yet (`toolchain.not_built_yet`): an include's records aren't checked again by the code yet, and an aggregate's totals are the database's ([The aggregates](06-reads-and-writes.md#the-aggregates)). |
| `IndexKind`s other than `geographic` and `hnsw` | Not built yet ([Special kinds of index](05-data.md#special-kinds-of-index)). |
