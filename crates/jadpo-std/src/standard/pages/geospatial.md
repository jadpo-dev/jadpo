# Point

A position on Earth: its latitude, its longitude and, if you know it, its height.

Use it for anywhere you'd drop a pin, such as a shop or a delivery drop-off. For
a path, use `LineString`. For an area, use `Polygon`. For an address someone
can post to, use `PostalAddress`.

```jadpo
Point { latitude: 51.5072 longitude: -0.1276 altitude: none }
```

- Fields are named, so latitude and longitude can't be swapped by accident. In
  JSON they follow GeoJSON's order, longitude first.
- `geo.distance` gives the distance between two, and a route takes and returns
  one as GeoJSON.

# Latitude

Degrees north of the equator, from -90 to 90: `51.5072`.

Use it where you hold a latitude apart from its longitude, such as in a search
query's parameters. When you have both, use `Point`.

```jadpo
Latitude(51.5072)
```

- South is negative. `text.format` shows the letter: `33.8688° S`.
- In JSON it is a string, so no decimal places are lost on the way.

# Longitude

Degrees east of the prime meridian, from -180 to 180: `-0.1276`.

Use it where you hold a longitude apart from its latitude. When you have both,
use `Point`.

```jadpo
Longitude(-0.1276)
```

- West is negative. `text.format` shows the letter: `0.1276° W`.
- In JSON it is a string, so no decimal places are lost on the way.

# LineString

A path of two points or more, in order.

Use it for a route, a road or a cable. For points in no order, use `MultiPoint`.
For an enclosed area, use `Polygon`.

```jadpo
LineString {
    points: [
        Point { latitude: 51.5072 longitude: -0.1276 altitude: none },
        Point { latitude: 48.8566 longitude: 2.3522 altitude: none }
    ]
}
```

- Every point has an altitude, or none does.
- `geo.length` gives its length as a `Length`.

# Polygon

An area on a map, with any holes it has.

Use it for a delivery zone, a field or a building's outline. A simple rectangle
for a map view is a `BoundingBox`, and a distance around a place is a `Circle`.

```jadpo
Polygon {
    outer: [
        Point { latitude: 0.0 longitude: 0.0 altitude: none },
        Point { latitude: 0.0 longitude: 1.0 altitude: none },
        Point { latitude: 1.0 longitude: 1.0 altitude: none },
        Point { latitude: 1.0 longitude: 0.0 altitude: none },
        Point { latitude: 0.0 longitude: 0.0 altitude: none }
    ]
    holes: []
}
```

- Close each ring by repeating its first point, so a triangle has four points.
  List the outer ring counterclockwise and each hole clockwise.
- A ring can't cross itself, rings can't touch, and holes sit inside the outer
  ring. A value built from data can fail these checks, so handle `rings_valid`.

# MultiPoint

A set of points, at least one, that belong together.

Use it for the branches of one chain, held as one value. For one location, use
`Point`. To hold each with its own data, use a `FeatureCollection` or your own
records.

```jadpo
MultiPoint {
    points: [
        Point { latitude: 51.5072 longitude: -0.1276 altitude: none },
        Point { latitude: 48.8566 longitude: 2.3522 altitude: none }
    ]
}
```

# MultiLineString

Several paths, at least one, that belong together.

Use it for a route with gaps, such as a ferry line split at each port. For one
unbroken path, use `LineString`.

```jadpo
MultiLineString {
    lines: [
        LineString {
            points: [
                Point { latitude: 0.0 longitude: 0.0 altitude: none },
                Point { latitude: 1.0 longitude: 1.0 altitude: none }
            ]
        }
    ]
}
```

# MultiPolygon

Several areas, at least one, that belong together.

Use it for one region made of separate pieces, such as a country with islands.
For one connected area, use `Polygon`.

```jadpo
MultiPolygon {
    polygons: [
        Polygon {
            outer: [
                Point { latitude: 0.0 longitude: 0.0 altitude: none },
                Point { latitude: 0.0 longitude: 1.0 altitude: none },
                Point { latitude: 1.0 longitude: 1.0 altitude: none },
                Point { latitude: 0.0 longitude: 0.0 altitude: none }
            ]
            holes: []
        }
    ]
}
```

# Geometry

Any one of the six shapes: a point, a path, an area or a set of any of those.

Use it when one field must hold whichever shape comes in, such as uploaded map
data. When you know the shape, use that type, since the compiler then checks it.

```jadpo
Geometry.point { point: Point { latitude: 51.5072 longitude: -0.1276 altitude: none } }
```

- `geo.from_geojson` and `geo.from_wkt` read one from outside text.
- `match` takes it apart, with one arm for each shape.

# BoundingBox

A rectangle on a map, given by its four sides.

Use it for the visible part of a map or a rough search area. For an exact
outline, use `Polygon`. For a distance around a place, use `Circle`.

```jadpo
BoundingBox { south: 51.28 west: -0.51 north: 51.69 east: 0.33 }
```

- A box may cross the 180° meridian: its `west` is then further east than its
  `east`. But `south` can't be above `north`.
- `geo.bounding_box` gives the box around any shape.

# Circle

A centre and a distance around it.

Use it for "within 5 km of here", such as a delivery radius. For a drawn area,
use `Polygon`.

```jadpo
Circle {
    centre: Point { latitude: 51.5072 longitude: -0.1276 altitude: none }
    radius: 1km
}
```

- The radius can't be below zero, and it is measured on the ground.

# Bearing

A compass direction: degrees clockwise from true north, from 0 to 360.

Use it for which way something is heading. `geo.bearing` gives one between two
points. For an angle that isn't a direction, use `Angle`.

```jadpo
Bearing(247.5)
```

- `text.format` shows `247.5°`.

# Crs

A coordinate reference system, by its EPSG code: `EPSG:27700`.

Use it to record which map system a set of coordinates is in, as `EastingNorthing`
does. `Point` is always in EPSG:4326, so it needs no `Crs`.

```jadpo
Crs("EPSG:27700")
```

- `EPSG` is in capitals, with no space.

# EastingNorthing

Map coordinates on a projected grid: an easting, a northing and which grid.

Use it for data on a national grid, such as Ordnance Survey's in Great Britain.
For latitude and longitude, use `Point`.

```jadpo
EastingNorthing { easting: 530000.0 northing: 180000.0 crs: "EPSG:27700" }
```

- Jadpo stores it as written and doesn't convert it to or from a `Point`.

# MapTile

One square of a web map at one zoom level: `10/511/340`.

Use it to cache or address map images. For a place on Earth, use `Point`.

```jadpo
MapTile { zoom: 10 x: 511 y: 340 }
```

- Zoom runs from 0 to 30, and `x` and `y` must each be below 2 to the zoom.

# H3Cell

A cell of Uber's H3 grid of hexagons, as 15 hexadecimal digits: `8928308280fffff`.

Use it to group data by area on a hexagon grid, such as demand by neighbourhood.
For a grid of rectangles that shortens by prefix, use `Geohash`.

```jadpo
H3Cell("8928308280fffff")
```

- It checks the bits inside as well as the form, so not every 15 digits pass.

# Geohash

A rectangle on Earth, as up to 12 letters and digits: `gcpvj0duq`.

Use it to group or look up nearby points by a shared text prefix. For hexagons of
near-equal size, use `H3Cell`. For an exact position, use `Point`.

```jadpo
Geohash("gcpvj0duq")
```

- It is lower case, and leaves out `a`, `i`, `l` and `o`, which is how geohashes
  are written.

# Feature

A shape with your own data attached, in GeoJSON's form, as a map library takes one.

Use it to send one shape and its properties to a map. To send a whole layer, use
`FeatureCollection`. Inside your app, store your own records with a `Point` or
`Polygon` field.

```jadpo
type StopInfo = Object {
    name: Text
}

var stop: Feature<Point, StopInfo> = Feature {
    geometry: Point { latitude: 51.5072 longitude: -0.1276 altitude: none }
    properties: StopInfo { name: "Charing Cross" }
}
assert stop.properties.name == "Charing Cross"
```

- Name both types when you use it, the geometry and the properties:
  `Feature<Point, StopInfo>`. The geometry can be one of the six shapes or
  `Geometry`.
- It has no `id`, so an incoming feature with one is refused.

# FeatureCollection

A list of features, all with the same shape and properties types, as a map layer.

Use it to send many shapes to a map library in one response. For one shape, use
`Feature`.

```jadpo
type LayerStop = Object {
    name: Text
}

var layer: FeatureCollection<Point, LayerStop> = FeatureCollection { features: [] }
assert layer.features == []
```

- Name both types when you use it: `FeatureCollection<Point, LayerStop>`.
