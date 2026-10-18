//! The predicates and the shapes (area 20, "Operations", chunk 5.5u3):
//! `geo.intersects`, `contains` and `within`, `centroid` and
//! `bounding_box`, each a value read into `jadpo_std::sphere`'s exact
//! great-circle geometry, so the checker's literals and every target agree.
//!
//! `centroid` is PostGIS's `ST_Centroid(x, false)`, reproduced: each point
//! a unit vector, weighted and summed. A multi-point's points weigh one
//! each; a line's edges each give both ends the edge's angle; a polygon is
//! cut into triangles from the first corner of its first outer ring to each
//! edge of every ring, each triangle's centre (its corners summed) weighted
//! by its area on the sphere, as PostGIS weighs them, always counted
//! positive. Worked out in the fixed module's working values, then rounded
//! to the nearest double, as every coordinate is held (question 2).

use jadpo_std::Rounding;
use jadpo_std::fixed::{Real, Unit};
use jadpo_std::sphere::{self, Area, Latitude, Position, Shape};
use jadpo_std::standard::geo::{
    BOUNDING_BOX, CIRCLE, GEOMETRY, LINE_STRING, MULTI_LINE_STRING, MULTI_POINT, MULTI_POLYGON,
    POINT, POLYGON,
};

use super::{list_of, num, position_num, position_of};
use crate::value::Decimal;
use crate::{Fault, Record, Value};

/// The geometry a value is, as a value and as its record: a `Geometry`'s
/// variant's, or the value's own.
fn geometry(value: &Value) -> Result<(&Value, &Record), Fault> {
    match value {
        Value::Variant(variant) if &*variant.ty == GEOMETRY => match variant.fields.first() {
            Some((_, inner @ Value::Record(record))) => Ok((inner, record)),
            _ => Err(Fault::Shape),
        },
        Value::Record(record) => Ok((value, record)),
        _ => Err(Fault::Shape),
    }
}

/// A list of points' positions.
fn positions(points: &[Value]) -> Result<Vec<Position>, Fault> {
    points
        .iter()
        .map(|point| position_num(point).ok_or(Fault::Shape))
        .collect()
}

/// A polygon's rings.
fn area_of(polygon: &Value) -> Result<Area, Fault> {
    let Value::Record(record) = polygon else {
        return Err(Fault::Shape);
    };
    let holes = list_of(record, "holes")?
        .iter()
        .map(|hole| positions(hole.items().ok_or(Fault::Shape)?))
        .collect::<Result<_, _>>()?;
    Ok(Area {
        outer: positions(list_of(record, "outer")?)?,
        holes,
    })
}

/// A geometry as the sphere's geometry reads it.
fn shape(value: &Value) -> Result<Shape, Fault> {
    let (value, record) = geometry(value)?;
    let mut shape = Shape::default();
    match &*record.ty {
        POINT => shape.points.push(position_num(value).ok_or(Fault::Shape)?),
        MULTI_POINT => shape.points = positions(list_of(record, "points")?)?,
        LINE_STRING => shape.lines.push(positions(list_of(record, "points")?)?),
        MULTI_LINE_STRING => {
            for line in list_of(record, "lines")? {
                let Value::Record(line) = line else {
                    return Err(Fault::Shape);
                };
                shape.lines.push(positions(list_of(line, "points")?)?);
            }
        }
        POLYGON => shape.areas.push(area_of(value)?),
        MULTI_POLYGON => {
            for polygon in list_of(record, "polygons")? {
                shape.areas.push(area_of(polygon)?);
            }
        }
        _ => return Err(Fault::Shape),
    }
    Ok(shape)
}

/// A decimal field of a record, as an exact number.
fn side(record: &Record, name: &str) -> Result<jadpo_std::Num, Fault> {
    match record.get(name) {
        Some(Value::Decimal(value)) => Ok(num(*value)),
        _ => Err(Fault::Shape),
    }
}

/// Whether a point is within `radius` (a `Length` in micrometres) of
/// `centre`, as `geo.distance` measures it.
fn near(centre: &Value, point: &Value, radius: i128) -> Result<bool, Fault> {
    let apart = crate::maths::geodesic::distance(position_of(centre)?, position_of(point)?)
        .ok_or(Fault::DecimalOverflow)?;
    Ok(apart <= radius)
}

/// `geo.contains(a, b)` (area 20): no point of `b` outside `a`, edges
/// included; a `BoundingBox`'s sides compared directly, a `Circle` holding
/// a point within its radius of its centre.
pub fn contains(a: &Value, b: &Value) -> Result<Value, Fault> {
    if let Value::Record(record) = a {
        match &*record.ty {
            BOUNDING_BOX => {
                return Ok(Value::Bool(sphere::box_holds(
                    side(record, "south")?,
                    side(record, "west")?,
                    side(record, "north")?,
                    side(record, "east")?,
                    &shape(b)?,
                )));
            }
            CIRCLE => {
                let (Some(centre), Some(Value::Quantity(radius))) =
                    (record.get("centre"), record.get("radius"))
                else {
                    return Err(Fault::Shape);
                };
                let (point, record) = geometry(b)?;
                if &*record.ty != POINT {
                    return Err(Fault::Shape);
                }
                return near(centre, point, radius.amount).map(Value::Bool);
            }
            _ => {}
        }
    }
    Ok(Value::Bool(sphere::covers(&shape(a)?, &shape(b)?)))
}

/// `geo.within(a, b)` (area 20): `geo.contains(b, a)`; with `distance:`,
/// whether the two points are at most that far apart.
pub fn within(a: &Value, b: &Value, distance: &Value) -> Result<Value, Fault> {
    match distance {
        Value::None => contains(b, a),
        Value::Quantity(reach) => near(a, b, reach.amount).map(Value::Bool),
        _ => Err(Fault::Shape),
    }
}

/// `geo.intersects(a, b)` (area 20): whether the two share a point.
pub fn intersects(a: &Value, b: &Value) -> Result<Value, Fault> {
    Ok(Value::Bool(sphere::intersects(&shape(a)?, &shape(b)?)))
}

/// A unit vector of working values for a position.
fn unit_vector(position: &Position) -> [Real; 3] {
    jadpo_std::sphere::unit_vector(position.0, position.1)
}

fn add(a: &[Real; 3], b: &[Real; 3]) -> [Real; 3] {
    [a[0].add(&b[0]), a[1].add(&b[1]), a[2].add(&b[2])]
}

fn scaled(unit: &Unit, v: &[Real; 3], w: &Real) -> [Real; 3] {
    [unit.mul(&v[0], w), unit.mul(&v[1], w), unit.mul(&v[2], w)]
}

fn dot(unit: &Unit, a: &[Real; 3], b: &[Real; 3]) -> Real {
    unit.mul(&a[0], &b[0])
        .add(&unit.mul(&a[1], &b[1]))
        .add(&unit.mul(&a[2], &b[2]))
}

fn cross(unit: &Unit, a: &[Real; 3], b: &[Real; 3]) -> [Real; 3] {
    [
        unit.mul(&a[1], &b[2]).sub(&unit.mul(&a[2], &b[1])),
        unit.mul(&a[2], &b[0]).sub(&unit.mul(&a[0], &b[2])),
        unit.mul(&a[0], &b[1]).sub(&unit.mul(&a[1], &b[0])),
    ]
}

fn length(unit: &Unit, v: &[Real; 3]) -> Real {
    unit.sqrt(&dot(unit, v, v))
}

/// The angle between two unit vectors: the great-circle distance on a
/// sphere of radius 1, the weight PostGIS gives a line's edge.
fn angle(unit: &Unit, a: &[Real; 3], b: &[Real; 3]) -> Real {
    unit.atan2(&length(unit, &cross(unit, a, b)), &dot(unit, a, b))
}

/// A spherical triangle's area on a sphere of radius 1, its size, by Van
/// Oosterom and Strackee's formula.
fn excess(unit: &Unit, a: &[Real; 3], b: &[Real; 3], c: &[Real; 3]) -> Real {
    let numerator = dot(unit, a, &cross(unit, b, c)).abs();
    let denominator = unit
        .int(1)
        .add(&dot(unit, a, b))
        .add(&dot(unit, b, c))
        .add(&dot(unit, c, a));
    unit.atan2(&numerator, &denominator).times(2)
}

/// `v` scaled to length 1; the zero vector stays itself.
fn normalised(unit: &Unit, v: &[Real; 3]) -> [Real; 3] {
    let size = length(unit, v);
    if size.is_zero() {
        return v.clone();
    }
    [
        unit.div(&v[0], &size),
        unit.div(&v[1], &size),
        unit.div(&v[2], &size),
    ]
}

/// Degrees as a coordinate is held: the nearest double's shortest decimal.
fn held(unit: &Unit, radians: &Real) -> Result<Decimal, Fault> {
    let degrees = unit.div(&unit.mul(radians, &unit.int(180)), unit.pi());
    let places = 35;
    let mantissa = degrees
        .rounded(places, Rounding::HalfEven)
        .ok_or(Fault::DecimalOverflow)?;
    let text =
        jadpo_std::standard::geo::nearest_double(&Decimal::new(mantissa, places).to_string())
            .ok_or(Fault::DecimalOverflow)?;
    Decimal::parse(&text).ok_or(Fault::DecimalOverflow)
}

/// `geo.centroid(shape)` (area 20): PostGIS's `ST_Centroid(x, false)`.
/// Weights that cancel (two opposite points) leave no direction, a defect.
pub fn centroid(shape_value: &Value) -> Result<Value, Fault> {
    let unit = jadpo_std::sphere::working_unit();
    let (value, record) = geometry(shape_value)?;
    let mut sum = [Real::zero(), Real::zero(), Real::zero()];
    match &*record.ty {
        POINT => {
            let (latitude, longitude) = position_of(value)?;
            return Ok(super::point(latitude, longitude, Value::None));
        }
        MULTI_POINT => {
            for position in positions(list_of(record, "points")?)? {
                sum = add(&sum, &unit_vector(&position));
            }
        }
        LINE_STRING | MULTI_LINE_STRING => {
            let lines = match &*record.ty {
                LINE_STRING => vec![positions(list_of(record, "points")?)?],
                _ => list_of(record, "lines")?
                    .iter()
                    .map(|line| match line {
                        Value::Record(line) => positions(list_of(line, "points")?),
                        _ => Err(Fault::Shape),
                    })
                    .collect::<Result<_, _>>()?,
            };
            for line in &lines {
                let vectors: Vec<[Real; 3]> = line.iter().map(unit_vector).collect();
                for pair in vectors.windows(2) {
                    let [a, b] = pair else { continue };
                    let weight = angle(unit, a, b);
                    sum = add(&sum, &scaled(unit, &add(a, b), &weight));
                }
            }
        }
        POLYGON | MULTI_POLYGON => {
            let areas = match &*record.ty {
                POLYGON => vec![area_of(value)?],
                _ => list_of(record, "polygons")?
                    .iter()
                    .map(area_of)
                    .collect::<Result<_, _>>()?,
            };
            let reference = areas
                .first()
                .and_then(|area| area.outer.first())
                .map(unit_vector)
                .ok_or(Fault::Shape)?;
            for area in &areas {
                for ring in std::iter::once(&area.outer).chain(area.holes.iter()) {
                    let vectors: Vec<[Real; 3]> = ring.iter().map(unit_vector).collect();
                    for pair in vectors.windows(2) {
                        let [a, b] = pair else { continue };
                        let weight = excess(unit, a, b, &reference);
                        let centre = normalised(unit, &add(&add(a, b), &reference));
                        sum = add(&sum, &scaled(unit, &centre, &weight));
                    }
                }
            }
        }
        _ => return Err(Fault::Shape),
    }
    if sum.iter().all(Real::is_zero) {
        return Err(Fault::NoCentre);
    }
    let [x, y, z] = &sum;
    let latitude = held(unit, &unit.atan2(z, &unit.hypot(x, y)))?;
    let longitude = held(unit, &unit.atan2(y, x))?;
    Ok(super::point(latitude, longitude, Value::None))
}

/// `geo.bounding_box(shape)` (area 20): the smallest box of parallels and
/// meridians holding the shape, an edge's highest or lowest point rounded
/// outwards to a double, so the box holds it.
pub fn bounding_box(shape_value: &Value) -> Result<Value, Fault> {
    let reach = sphere::reach(&shape(shape_value)?).ok_or(Fault::Shape)?;
    let (south, west, north, east) = sphere::smallest_box(&reach);
    let latitude = |side: &Latitude, up: bool| match side {
        Latitude::Exact(n) => *n,
        Latitude::Worked(r) => sphere::outward(r, up),
    };
    let decimal = |n: jadpo_std::Num| Value::Decimal(Decimal::new(n.mantissa, n.scale));
    Ok(Value::record(
        BOUNDING_BOX,
        vec![
            ("south".into(), decimal(latitude(&south, false))),
            ("west".into(), decimal(west)),
            ("north".into(), decimal(latitude(&north, true))),
            ("east".into(), decimal(east)),
        ],
    ))
}
