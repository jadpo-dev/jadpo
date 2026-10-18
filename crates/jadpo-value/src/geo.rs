//! The geospatial types on the wire and for people (area 19, chunk 5.5s),
//! and the measures between them (area 20, chunk 5.5u2).
//!
//! A geometry is sent as GeoJSON (RFC 7946), and this module is the only
//! place a position's `[longitude, latitude]` order exists: everywhere else
//! a `Point` is its `latitude` and `longitude` by name. Reading checks each
//! coordinate's range, each line's and ring's length, each ring's closure
//! and every member, and rewinds a ring wound the other way, as RFC 7946
//! §3.1.6 asks a parser not to refuse one; writing winds every ring by the
//! right-hand rule (the outer ring counterclockwise, each hole clockwise).
//! The data is `jadpo_std::standard::geo`'s.

use jadpo_std::geography::{Coordinates, Shape};
use jadpo_std::standard::geo::{
    BOUNDING_BOX, EASTING_NORTHING, GEOJSON_TYPES, GEOMETRY, LINE_STRING, MAP_TILE,
    MULTI_LINE_STRING, MULTI_POINT, MULTI_POLYGON, POINT, POLYGON,
};
use jadpo_std::{QuantityKind, Rounding};

use crate::value::Decimal;
use crate::{Fault, Record, Value, present};

mod shapes;

pub use shapes::{bounding_box, centroid, contains, intersects, within};

/// Why a GeoJSON geometry wasn't read: where in it, below the field being
/// read (`coordinates.0.3.1`), and the rule it broke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    pub at: Vec<String>,
    pub rule: &'static str,
}

/// A refusal at `at` for breaking `rule`.
fn refused<T>(at: &[String], rule: &'static str) -> Result<T, Unreadable> {
    Err(Unreadable {
        at: at.to_vec(),
        rule,
    })
}

/// `at` with one more step.
fn step(at: &[String], next: impl ToString) -> Vec<String> {
    let mut out = at.to_vec();
    out.push(next.to_string());
    out
}

/// Whether standard type `ty` is sent as GeoJSON.
pub fn is_geojson(ty: &str) -> bool {
    jadpo_std::standard_type(ty)
        .is_some_and(|entry| entry.json == jadpo_std::standard::Json::GeoJson)
}

/// A value of the geometry type `ty` (`Point`, ..., or `Geometry`) read
/// from its GeoJSON object: its `type`, its `coordinates` and, if given,
/// the `bbox` RFC 7946 §5 lets it carry, which is checked as numbers and
/// otherwise ignored, since a value's box is its points'. Any other member
/// is `unknown_field`, a `crs` included: WGS84 is the only system.
pub fn read(ty: &str, json: &serde_json::Value) -> Result<Value, Unreadable> {
    let at: Vec<String> = Vec::new();
    let Some(members) = json.as_object() else {
        return refused(&at, "type");
    };
    for name in members.keys() {
        if !matches!(name.as_str(), "type" | "coordinates" | "bbox") {
            return refused(&step(&at, name), "unknown_field");
        }
    }
    let given = match members.get("type") {
        Some(serde_json::Value::String(given)) => given.as_str(),
        Some(_) => return refused(&step(&at, "type"), "type"),
        None => return refused(&step(&at, "type"), "required"),
    };
    let variant = GEOJSON_TYPES
        .iter()
        .position(|(_, listed)| *listed == given);
    let wanted = if ty == GEOMETRY { given } else { ty };
    if variant.is_none() || given != wanted {
        return refused(&step(&at, "type"), "variant");
    }
    if let Some(bbox) = members.get("bbox") {
        let numbers = bbox.as_array().is_some_and(|items| {
            matches!(items.len(), 4 | 6) && items.iter().all(|n| n.is_number())
        });
        if !numbers {
            return refused(&step(&at, "bbox"), "type");
        }
    }
    let Some(coordinates) = members.get("coordinates") else {
        return refused(&step(&at, "coordinates"), "required");
    };
    let value = geometry(wanted, coordinates, &step(&at, "coordinates"))?;
    let listed =
        variant.and_then(|ordinal| GEOJSON_TYPES.get(ordinal).map(|(name, _)| (ordinal, *name)));
    match (ty == GEOMETRY, listed) {
        (true, Some((ordinal, name))) => Ok(Value::variant(
            GEOMETRY,
            ordinal as u32,
            name,
            vec![(name.into(), value)],
        )),
        _ => Ok(value),
    }
}

/// The geometry `ty` from its `coordinates`.
fn geometry(ty: &str, json: &serde_json::Value, at: &[String]) -> Result<Value, Unreadable> {
    Ok(match ty {
        POINT => position(json, at)?,
        LINE_STRING => line(json, at)?,
        POLYGON => polygon(json, at)?,
        MULTI_POINT => Value::record(
            MULTI_POINT,
            vec![("points".into(), Value::list(positions(json, at, 1)?))],
        ),
        MULTI_LINE_STRING => {
            let lines = items(json, at, 1)?
                .iter()
                .enumerate()
                .map(|(n, item)| line(item, &step(at, n)))
                .collect::<Result<Vec<_>, _>>()?;
            Value::record(
                MULTI_LINE_STRING,
                vec![("lines".into(), Value::list(lines))],
            )
        }
        MULTI_POLYGON => {
            let polygons = items(json, at, 1)?
                .iter()
                .enumerate()
                .map(|(n, item)| polygon(item, &step(at, n)))
                .collect::<Result<Vec<_>, _>>()?;
            Value::record(
                MULTI_POLYGON,
                vec![("polygons".into(), Value::list(polygons))],
            )
        }
        _ => return refused(at, "type"),
    })
}

/// A JSON array of at least `least` items.
fn items<'j>(
    json: &'j serde_json::Value,
    at: &[String],
    least: usize,
) -> Result<&'j Vec<serde_json::Value>, Unreadable> {
    let Some(items) = json.as_array() else {
        return refused(at, "type");
    };
    if items.len() < least {
        return refused(at, "min_length");
    }
    Ok(items)
}

/// At least `least` positions, each a `Point`.
fn positions(
    json: &serde_json::Value,
    at: &[String],
    least: usize,
) -> Result<Vec<Value>, Unreadable> {
    items(json, at, least)?
        .iter()
        .enumerate()
        .map(|(n, item)| position(item, &step(at, n)))
        .collect()
}

/// A `LineString` of two positions or more.
fn line(json: &serde_json::Value, at: &[String]) -> Result<Value, Unreadable> {
    Ok(Value::record(
        LINE_STRING,
        vec![("points".into(), Value::list(positions(json, at, 2)?))],
    ))
}

/// A `Polygon`: at least its outer ring, each ring closed with four
/// positions or more, rewound to the right-hand rule.
fn polygon(json: &serde_json::Value, at: &[String]) -> Result<Value, Unreadable> {
    let mut rings = Vec::new();
    for (n, item) in items(json, at, 1)?.iter().enumerate() {
        let at = step(at, n);
        let points = positions(item, &at, 4)?;
        let closed = match (points.first(), points.last()) {
            (Some(first), Some(last)) => crate::ops::equal(first, last),
            _ => false,
        };
        if !closed {
            return refused(&at, "format");
        }
        rings.push(wound(points, n == 0));
    }
    // Valid on the sphere (area 20, "Polygons are valid when built"), or
    // refused at the first ring that isn't.
    let positions: Vec<Vec<jadpo_std::sphere::Position>> = rings
        .iter()
        .map(|ring| ring.iter().filter_map(position_num).collect())
        .collect();
    if let Some((outer, holes)) = positions.split_first()
        && let Some(broken) = jadpo_std::sphere::rings_broken(outer, holes)
    {
        return refused(&step(at, broken), "format");
    }
    let outer = rings.remove(0);
    Ok(Value::record(
        POLYGON,
        vec![
            ("outer".into(), Value::list(outer)),
            (
                "holes".into(),
                Value::list(rings.into_iter().map(Value::list).collect()),
            ),
        ],
    ))
}

/// A position, `[longitude, latitude]` or `[longitude, latitude,
/// altitude]`, as a `Point`: the longitude from -180 to 180, the latitude
/// from -90 to 90, and the altitude in metres, a `Length` rounded half to
/// even to its micrometre.
fn position(json: &serde_json::Value, at: &[String]) -> Result<Value, Unreadable> {
    let Some(numbers) = json.as_array() else {
        return refused(at, "type");
    };
    if !matches!(numbers.len(), 2 | 3) {
        return refused(at, "format");
    }
    let mut read = Vec::new();
    for (n, number) in numbers.iter().enumerate() {
        match number {
            serde_json::Value::Number(number) => match decimal_of(number) {
                Some(decimal) => read.push(decimal),
                None => return refused(&step(at, n), "type"),
            },
            _ => return refused(&step(at, n), "type"),
        }
    }
    let within = |n: usize, bound: i128| -> Result<Decimal, Unreadable> {
        let Some(value) = read.get(n).copied() else {
            return refused(at, "format");
        };
        let limit = Decimal::new(bound, 0);
        if value < Decimal::new(-bound, 0) {
            return refused(&step(at, n), "min");
        }
        if value > limit {
            return refused(&step(at, n), "max");
        }
        Ok(value)
    };
    let longitude = within(0, 180)?;
    let latitude = within(1, 90)?;
    let altitude = match read.get(2) {
        Some(metres) => {
            let unit = QuantityKind::Length.base_unit();
            match jadpo_std::quantity::to_base(metres.mantissa, metres.scale, unit) {
                Some(amount) => Value::Quantity(crate::Quantity {
                    kind: QuantityKind::Length,
                    amount,
                }),
                None => return refused(&step(at, 2), "max"),
            }
        }
        None => Value::None,
    };
    Ok(point(latitude, longitude, altitude))
}

/// A `Point` from its parts.
pub fn point(latitude: Decimal, longitude: Decimal, altitude: Value) -> Value {
    Value::record(
        POINT,
        vec![
            ("latitude".into(), Value::Decimal(latitude)),
            ("longitude".into(), Value::Decimal(longitude)),
            ("altitude".into(), altitude),
        ],
    )
}

/// A JSON number as a decimal: a whole number exactly, any other as the
/// shortest decimal that is the IEEE double it was read as, since RFC 7946
/// is I-JSON (RFC 7493), whose numbers are doubles.
fn decimal_of(number: &serde_json::Number) -> Option<Decimal> {
    if let Some(whole) = number.as_i64() {
        return Some(Decimal::new(i128::from(whole), 0));
    }
    let double = number.as_f64().filter(|double| double.is_finite())?;
    // Rust's shortest round-trip form, with no exponent.
    let text = format!("{double}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    Decimal::parse(text)
}

/// A position's latitude and longitude.
fn axes(point: &Value) -> Option<(Decimal, Decimal)> {
    let Value::Record(record) = point else {
        return None;
    };
    match (record.get("latitude"), record.get("longitude")) {
        (Some(Value::Decimal(latitude)), Some(Value::Decimal(longitude))) => {
            Some((*latitude, *longitude))
        }
        _ => None,
    }
}

/// A point's latitude and longitude as the sphere's geometry reads them.
fn position_num(point: &Value) -> Option<jadpo_std::sphere::Position> {
    let (latitude, longitude) = axes(point)?;
    Some((num(latitude), num(longitude)))
}

/// A decimal as an exact number.
fn num(value: Decimal) -> jadpo_std::Num {
    jadpo_std::Num {
        mantissa: value.mantissa,
        scale: value.scale,
    }
}

/// A coordinate at 9 places, a tenth of a millimetre at the equator,
/// as a whole number, so a ring's winding is found exactly.
fn fixed_point(value: Decimal) -> i128 {
    value
        .rescale(9, Rounding::HalfEven)
        .map_or(0, |fixed| fixed.mantissa)
}

/// Twice a ring's signed area on the plane of longitude and latitude, the
/// shoelace formula: above 0 when it runs counterclockwise.
fn twice_area(ring: &[Value]) -> i128 {
    let corners: Vec<(i128, i128)> = ring
        .iter()
        .filter_map(axes)
        .map(|(latitude, longitude)| (fixed_point(longitude), fixed_point(latitude)))
        .collect();
    corners
        .iter()
        .zip(corners.iter().skip(1))
        .map(|(from, to)| from.0 * to.1 - to.0 * from.1)
        .sum()
}

/// A ring wound by the right-hand rule: counterclockwise for an outer ring,
/// clockwise for a hole; one with no area is kept as it is.
fn wound(mut ring: Vec<Value>, outer: bool) -> Vec<Value> {
    let area = twice_area(&ring);
    if area != 0 && (area > 0) != outer {
        ring.reverse();
    }
    ring
}

/// A geometry's value as its GeoJSON object, appended to `out`; `false`
/// when `value` isn't one.
pub fn write(value: &Value, out: &mut String) -> bool {
    let (ty, value) = match value {
        Value::Variant(variant) if &*variant.ty == GEOMETRY => {
            let Some((_, inner)) = variant.fields.first() else {
                return false;
            };
            let Value::Record(record) = inner else {
                return false;
            };
            (record.ty.to_string(), inner)
        }
        Value::Record(record) => (record.ty.to_string(), value),
        _ => return false,
    };
    let mut coordinates = String::new();
    if !coordinates_of(&ty, value, &mut coordinates) {
        return false;
    }
    out.push_str("{\"type\":\"");
    out.push_str(&ty);
    out.push_str("\",\"coordinates\":");
    out.push_str(&coordinates);
    out.push('}');
    true
}

/// A list's items, from a record's field.
fn listed<'v>(record: &'v Record, name: &str) -> Option<&'v [Value]> {
    record.get(name).and_then(Value::items)
}

/// A geometry's `coordinates`, appended to `out`.
fn coordinates_of(ty: &str, value: &Value, out: &mut String) -> bool {
    let Value::Record(record) = value else {
        return false;
    };
    match ty {
        POINT => write_position(value, out),
        LINE_STRING | MULTI_POINT => {
            listed(record, "points").is_some_and(|points| write_positions(points, out))
        }
        POLYGON => write_rings(record, out),
        MULTI_LINE_STRING => listed(record, "lines").is_some_and(|lines| {
            write_each(lines, out, |line, out| {
                coordinates_of(LINE_STRING, line, out)
            })
        }),
        MULTI_POLYGON => listed(record, "polygons").is_some_and(|polygons| {
            write_each(polygons, out, |polygon, out| {
                coordinates_of(POLYGON, polygon, out)
            })
        }),
        _ => false,
    }
}

/// Each item as `write` writes it, in a JSON array.
fn write_each(
    items: &[Value],
    out: &mut String,
    write: impl Fn(&Value, &mut String) -> bool,
) -> bool {
    out.push('[');
    for (at, item) in items.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        if !write(item, out) {
            return false;
        }
    }
    out.push(']');
    true
}

fn write_positions(points: &[Value], out: &mut String) -> bool {
    write_each(points, out, write_position)
}

/// A polygon's rings, the outer first, each wound by the right-hand rule.
fn write_rings(record: &Record, out: &mut String) -> bool {
    let (Some(outer), Some(holes)) = (listed(record, "outer"), listed(record, "holes")) else {
        return false;
    };
    let mut rings = vec![wound(outer.to_vec(), true)];
    for hole in holes {
        let Some(points) = hole.items() else {
            return false;
        };
        rings.push(wound(points.to_vec(), false));
    }
    write_each(
        &rings.into_iter().map(Value::list).collect::<Vec<_>>(),
        out,
        |ring, out| {
            ring.items()
                .is_some_and(|points| write_positions(points, out))
        },
    )
}

/// A position, `[longitude, latitude]`, with the altitude in metres third
/// where it has one.
fn write_position(point: &Value, out: &mut String) -> bool {
    let (Some((latitude, longitude)), Value::Record(record)) = (axes(point), point) else {
        return false;
    };
    out.push('[');
    out.push_str(&longitude.to_string());
    out.push(',');
    out.push_str(&latitude.to_string());
    match record.get("altitude") {
        Some(Value::Quantity(quantity)) => {
            out.push(',');
            out.push_str(&trimmed(quantity.decimal()).to_string());
        }
        Some(Value::None) | None => {}
        Some(_) => return false,
    }
    out.push(']');
    true
}

/// `value` with no trailing zeros among its places.
fn trimmed(mut value: Decimal) -> Decimal {
    while value.scale > 0 && value.mantissa % 10 == 0 {
        value = Decimal::new(value.mantissa / 10, value.scale - 1);
    }
    value
}

// --- In PostGIS's `geography` (area 20, "Storage"; chunk 5.5u4) -------------

/// A position as `geography` holds it: each coordinate the double its
/// decimal is, which, held as the shortest decimal of a double (area 20,
/// question 2), it is exactly; an altitude the double nearest its metres.
fn coordinates_of_point(point: &Value) -> Option<Coordinates> {
    let (latitude, longitude) = axes(point)?;
    let Value::Record(record) = point else {
        return None;
    };
    let double = |value: Decimal| value.to_string().parse::<f64>().ok();
    let altitude = match record.get("altitude") {
        Some(Value::Quantity(quantity)) => Some(double(quantity.decimal())?),
        Some(Value::None) | None => None,
        Some(_) => return None,
    };
    Some(Coordinates::new(
        double(longitude)?,
        double(latitude)?,
        altitude,
    ))
}

fn points_of(points: &[Value]) -> Option<Vec<Coordinates>> {
    points.iter().map(coordinates_of_point).collect()
}

/// A polygon's rings, the outer first, each in the order it was given.
fn rings_of(polygon: &Value) -> Option<Vec<Vec<Coordinates>>> {
    let Value::Record(record) = polygon else {
        return None;
    };
    let mut rings = vec![points_of(listed(record, "outer")?)?];
    for hole in listed(record, "holes")? {
        rings.push(points_of(hole.items()?)?);
    }
    Some(rings)
}

/// A geometry's value as PostGIS's `geography` holds it: one of the six,
/// or the one a `Geometry` holds; `None` for any other value. Rings are
/// kept as they were given, so what is stored reads back unchanged.
pub fn shape(value: &Value) -> Option<Shape> {
    let value = match value {
        Value::Variant(variant) if &*variant.ty == GEOMETRY => &variant.fields.first()?.1,
        other => other,
    };
    let Value::Record(record) = value else {
        return None;
    };
    Some(match &*record.ty {
        POINT => Shape::Point(coordinates_of_point(value)?),
        LINE_STRING => Shape::LineString(points_of(listed(record, "points")?)?),
        MULTI_POINT => Shape::MultiPoint(points_of(listed(record, "points")?)?),
        POLYGON => Shape::Polygon(rings_of(value)?),
        MULTI_LINE_STRING => Shape::MultiLineString(
            listed(record, "lines")?
                .iter()
                .map(|line| match line {
                    Value::Record(line) => points_of(listed(line, "points")?),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        ),
        MULTI_POLYGON => Shape::MultiPolygon(
            listed(record, "polygons")?
                .iter()
                .map(rings_of)
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

/// A coordinate read back from `geography`: the shortest decimal of its
/// double, as a coordinate built in code is held.
fn coordinate_value(double: f64, bound: i128) -> Result<Decimal, String> {
    let text = jadpo_std::standard::geo::nearest_double(&format!("{double}"))
        .ok_or("a stored coordinate that isn't a number")?;
    let value = Decimal::parse(&text).ok_or("a stored coordinate that isn't a number")?;
    if value < Decimal::new(-bound, 0) || value > Decimal::new(bound, 0) {
        return Err("a stored coordinate out of its range".into());
    }
    Ok(value)
}

/// A `Point` from a stored position: its altitude the `Length` its double's
/// shortest decimal is, rounded half to even to the micrometre.
fn point_of(position: &Coordinates) -> Result<Value, String> {
    let altitude = match position.altitude() {
        Some(metres) => {
            let text = jadpo_std::standard::geo::nearest_double(&format!("{metres}"))
                .ok_or("a stored altitude that isn't a number")?;
            let metres = Decimal::parse(&text).ok_or("a stored altitude past a decimal")?;
            let unit = QuantityKind::Length.base_unit();
            let amount = jadpo_std::quantity::to_base(metres.mantissa, metres.scale, unit)
                .ok_or("a stored altitude past a `Length`")?;
            Value::Quantity(crate::Quantity {
                kind: QuantityKind::Length,
                amount,
            })
        }
        None => Value::None,
    };
    Ok(point(
        coordinate_value(position.latitude(), 90)?,
        coordinate_value(position.longitude(), 180)?,
        altitude,
    ))
}

fn points_value(points: &[Coordinates], least: usize) -> Result<Vec<Value>, String> {
    if points.len() < least {
        return Err("a stored geometry with too few points".into());
    }
    points.iter().map(point_of).collect()
}

fn line_value(points: &[Coordinates]) -> Result<Value, String> {
    Ok(Value::record(
        LINE_STRING,
        vec![("points".into(), Value::list(points_value(points, 2)?))],
    ))
}

/// A `Polygon` from its stored rings, checked as a polygon read from
/// GeoJSON is (area 20, "Polygons are valid when built"), since a row
/// another program wrote may hold one that isn't valid; its rings stay in
/// the order they were stored.
fn polygon_value(rings: &[Vec<Coordinates>]) -> Result<Value, String> {
    let mut values = Vec::new();
    for ring in rings {
        let points = points_value(ring, 4)?;
        let closed = matches!((points.first(), points.last()), (Some(first), Some(last)) if crate::ops::equal(first, last));
        if !closed {
            return Err("a stored polygon whose ring isn't closed".into());
        }
        values.push(points);
    }
    let positions: Vec<Vec<jadpo_std::sphere::Position>> = values
        .iter()
        .map(|ring| ring.iter().filter_map(position_num).collect())
        .collect();
    let Some((outer, holes)) = positions.split_first() else {
        return Err("a stored polygon without a ring".into());
    };
    if jadpo_std::sphere::rings_broken(outer, holes).is_some() {
        return Err("a stored polygon whose rings break `rings_valid`".into());
    }
    let outer = values.remove(0);
    Ok(Value::record(
        POLYGON,
        vec![
            ("outer".into(), Value::list(outer)),
            (
                "holes".into(),
                Value::list(values.into_iter().map(Value::list).collect()),
            ),
        ],
    ))
}

/// The value of standard type `ty` (one of the six geometries, or
/// `Geometry` for any of them) a `geography` column held as `shape`. A
/// shape of another kind than `ty` is refused: the column's check holds
/// it to its type. The error names the rule, never the value.
pub fn from_shape(ty: &str, shape: &Shape) -> Result<Value, String> {
    let value = match shape {
        Shape::Point(position) => point_of(position)?,
        Shape::LineString(points) => line_value(points)?,
        Shape::MultiPoint(points) => Value::record(
            MULTI_POINT,
            vec![("points".into(), Value::list(points_value(points, 1)?))],
        ),
        Shape::Polygon(rings) => polygon_value(rings)?,
        Shape::MultiLineString(lines) => {
            if lines.is_empty() {
                return Err("a stored geometry with too few points".into());
            }
            Value::record(
                MULTI_LINE_STRING,
                vec![(
                    "lines".into(),
                    Value::list(
                        lines
                            .iter()
                            .map(|line| line_value(line))
                            .collect::<Result<_, _>>()?,
                    ),
                )],
            )
        }
        Shape::MultiPolygon(polygons) => {
            if polygons.is_empty() {
                return Err("a stored geometry with too few points".into());
            }
            Value::record(
                MULTI_POLYGON,
                vec![(
                    "polygons".into(),
                    Value::list(
                        polygons
                            .iter()
                            .map(|rings| polygon_value(rings))
                            .collect::<Result<_, _>>()?,
                    ),
                )],
            )
        }
    };
    let kind = shape.standard_type();
    if ty == GEOMETRY {
        let ordinal = GEOJSON_TYPES
            .iter()
            .position(|(_, name)| *name == kind)
            .ok_or("a stored geometry of no known kind")?;
        let (variant, _) = GEOJSON_TYPES[ordinal];
        return Ok(Value::variant(
            GEOMETRY,
            ordinal as u32,
            variant,
            vec![(variant.into(), value)],
        ));
    }
    if ty != kind {
        return Err(format!("a stored `{kind}` where a `{ty}` was kept"));
    }
    Ok(value)
}

/// A point's latitude and longitude, or a fault for anything else.
fn position_of(point: &Value) -> Result<(Decimal, Decimal), Fault> {
    axes(point).ok_or(Fault::Shape)
}

/// A length in metres at its kind's scale, as the quantity it is.
fn length_of(metres: i128) -> Value {
    Value::Quantity(crate::Quantity {
        kind: QuantityKind::Length,
        amount: metres,
    })
}

/// `geo.distance(from, to)` (area 20): the geodesic between two points on
/// the WGS84 ellipsoid, rounded half to even to the micrometre.
pub fn distance(from: &Value, to: &Value) -> Result<Value, Fault> {
    crate::maths::geodesic::distance(position_of(from)?, position_of(to)?)
        .map(length_of)
        .ok_or(Fault::DecimalOverflow)
}

/// Whether two positions are the same place: the same latitude and
/// longitude, a longitude of -180 being 180's, or both at one pole,
/// where every longitude meets.
fn same_place((lat1, lon1): (Decimal, Decimal), (lat2, lon2): (Decimal, Decimal)) -> bool {
    let pole = Decimal::new(90, 0);
    let antimeridian = Decimal::new(180, 0);
    let east = |lon: Decimal| {
        if lon == Decimal::new(-180, 0) {
            antimeridian
        } else {
            lon
        }
    };
    lat1 == lat2 && (east(lon1) == east(lon2) || lat1 == pole || lat1 == Decimal::new(-90, 0))
}

/// `geo.bearing(from, to)` (area 20): the geodesic's direction as it
/// leaves `from`, in degrees clockwise from true north, at least 0 and
/// below 360, to 9 places; two equal points fail `same_point`.
pub fn bearing(from: &Value, to: &Value) -> Result<Value, Fault> {
    let (from, to) = (position_of(from)?, position_of(to)?);
    if same_place(from, to) {
        return Err(Fault::Broke(jadpo_std::RuleKey::SamePoint));
    }
    crate::maths::geodesic::bearing(from, to)
        .map(|degrees| Value::Decimal(Decimal::new(degrees, 9)))
        .ok_or(Fault::DecimalOverflow)
}

/// A line's points' positions.
fn path(line: &Value) -> Result<Vec<(Decimal, Decimal)>, Fault> {
    let Value::Record(record) = line else {
        return Err(Fault::Shape);
    };
    list_of(record, "points")?.iter().map(position_of).collect()
}

/// `geo.length(line)` (area 20): a `LineString`'s or `MultiLineString`'s
/// edges, each measured as `geo.distance` measures it, rounded to the
/// micrometre, and summed, so a line of two points is their distance.
pub fn length(line: &Value) -> Result<Value, Fault> {
    let Value::Record(record) = line else {
        return Err(Fault::Shape);
    };
    let lines = match &*record.ty {
        LINE_STRING => vec![path(line)?],
        MULTI_LINE_STRING => list_of(record, "lines")?
            .iter()
            .map(path)
            .collect::<Result<_, _>>()?,
        _ => return Err(Fault::Shape),
    };
    let mut total: i128 = 0;
    for points in &lines {
        for pair in points.windows(2) {
            let [from, to] = pair else { continue };
            let edge =
                crate::maths::geodesic::distance(*from, *to).ok_or(Fault::DecimalOverflow)?;
            total = total.checked_add(edge).ok_or(Fault::DecimalOverflow)?;
        }
    }
    Ok(length_of(total))
}

/// A polygon's area, unrounded: its outer ring's less its holes'.
fn polygon_area(polygon: &Value) -> Result<crate::maths::geodesic::Area, Fault> {
    let Value::Record(record) = polygon else {
        return Err(Fault::Shape);
    };
    let ring = |points: &[Value]| -> Result<Vec<(Decimal, Decimal)>, Fault> {
        points.iter().map(position_of).collect()
    };
    let mut area = crate::maths::geodesic::Area::of_ring(&ring(list_of(record, "outer")?)?);
    for hole in list_of(record, "holes")? {
        let points = hole.items().ok_or(Fault::Shape)?;
        area = area.less(&crate::maths::geodesic::Area::of_ring(&ring(points)?));
    }
    Ok(area)
}

/// `geo.area(shape)` (area 20): a `Polygon`'s or `MultiPolygon`'s area on
/// the WGS84 ellipsoid, holes taken away, rounded half to even to the
/// square millimetre (`Area`'s scale) once, after summing.
pub fn area(shape: &Value) -> Result<Value, Fault> {
    let Value::Record(record) = shape else {
        return Err(Fault::Shape);
    };
    let total = match &*record.ty {
        POLYGON => polygon_area(shape)?,
        MULTI_POLYGON => {
            let mut total = crate::maths::geodesic::Area::zero();
            for polygon in list_of(record, "polygons")? {
                total = total.plus(&polygon_area(polygon)?);
            }
            total
        }
        _ => return Err(Fault::Shape),
    };
    total
        .square_metres()
        .map(|amount| {
            Value::Quantity(crate::Quantity {
                kind: QuantityKind::Area,
                amount,
            })
        })
        .ok_or(Fault::DecimalOverflow)
}

/// A latitude or a longitude for people: the locale's digits for its size,
/// a degree sign, and its hemisphere's letter, none for 0.
pub fn coordinate(
    value: Decimal,
    positive: &str,
    negative: &str,
    tag: &str,
) -> Result<String, Fault> {
    let size = Decimal::new(value.mantissa.abs(), value.scale);
    let number = present::number(&Value::Decimal(size), tag)?;
    Ok(match value.mantissa.signum() {
        1 => format!("{number}° {positive}"),
        -1 => format!("{number}° {negative}"),
        _ => format!("{number}°"),
    })
}

/// A point for people: its latitude, then its longitude, then its altitude
/// as a `Length` is shown.
fn point_shown(point: &Value, tag: &str) -> Result<String, Fault> {
    let (Some((latitude, longitude)), Value::Record(record)) = (axes(point), point) else {
        return Err(Fault::Shape);
    };
    let mut text = format!(
        "{}, {}",
        coordinate(latitude, "N", "S", tag)?,
        coordinate(longitude, "E", "W", tag)?
    );
    if let Some(Value::Quantity(altitude)) = record.get("altitude") {
        text.push_str(", ");
        text.push_str(&present::units::format(
            altitude.amount,
            altitude.kind.base_unit(),
            tag,
        )?);
    }
    Ok(text)
}

/// Each item shown, in brackets, joined by `separator`.
fn bracketed(
    items: &[Value],
    separator: &str,
    show: impl Fn(&Value) -> Result<String, Fault>,
) -> Result<String, Fault> {
    let shown = items.iter().map(show).collect::<Result<Vec<_>, _>>()?;
    Ok(format!("({})", shown.join(separator)))
}

/// A list of points, in brackets, joined by `; `.
fn points_shown(points: &[Value], tag: &str) -> Result<String, Fault> {
    bracketed(points, "; ", |point| point_shown(point, tag))
}

/// A record's field as a list, or a fault.
fn list_of<'v>(record: &'v Record, name: &str) -> Result<&'v [Value], Fault> {
    listed(record, name).ok_or(Fault::Shape)
}

/// A geospatial record for people, as `FormatKey::Geometry` says.
pub fn shown(value: &Value, tag: &str) -> Result<String, Fault> {
    if let Value::Variant(variant) = value
        && &*variant.ty == GEOMETRY
    {
        let (_, inner) = variant.fields.first().ok_or(Fault::Shape)?;
        return shown(inner, tag);
    }
    let Value::Record(record) = value else {
        return Err(Fault::Shape);
    };
    let points = |record: &Record| points_shown(list_of(record, "points")?, tag);
    let polygon = |polygon: &Value| -> Result<String, Fault> {
        let Value::Record(polygon) = polygon else {
            return Err(Fault::Shape);
        };
        let mut rings = vec![Value::list(list_of(polygon, "outer")?.to_vec())];
        rings.extend(list_of(polygon, "holes")?.iter().cloned());
        bracketed(&rings, ", ", |ring| {
            points_shown(ring.items().ok_or(Fault::Shape)?, tag)
        })
    };
    match &*record.ty {
        POINT => point_shown(value, tag),
        LINE_STRING | MULTI_POINT => points(record),
        POLYGON => polygon(value),
        MULTI_LINE_STRING => bracketed(list_of(record, "lines")?, ", ", |line| match line {
            Value::Record(line) => points(line),
            _ => Err(Fault::Shape),
        }),
        MULTI_POLYGON => bracketed(list_of(record, "polygons")?, ", ", polygon),
        BOUNDING_BOX => {
            let side = |name: &str, positive: &str, negative: &str| match record.get(name) {
                Some(Value::Decimal(value)) => coordinate(*value, positive, negative, tag),
                _ => Err(Fault::Shape),
            };
            Ok(format!(
                "{}, {} – {}, {}",
                side("south", "N", "S")?,
                side("west", "E", "W")?,
                side("north", "N", "S")?,
                side("east", "E", "W")?
            ))
        }
        EASTING_NORTHING => {
            let (
                Some(easting @ Value::Decimal(_)),
                Some(northing @ Value::Decimal(_)),
                Some(Value::Text(crs)),
            ) = (
                record.get("easting"),
                record.get("northing"),
                record.get("crs"),
            )
            else {
                return Err(Fault::Shape);
            };
            Ok(format!(
                "easting {}, northing {} ({crs})",
                present::number(easting, tag)?,
                present::number(northing, tag)?
            ))
        }
        MAP_TILE => {
            let part = |name: &str| match record.get(name) {
                Some(Value::Int(n)) => Ok(*n),
                _ => Err(Fault::Shape),
            };
            Ok(format!("{}/{}/{}", part("zoom")?, part("x")?, part("y")?))
        }
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_text(ty: &str, text: &str) -> Result<Value, Unreadable> {
        read(ty, &serde_json::from_str(text).expect("JSON"))
    }

    fn written(value: &Value) -> String {
        let mut out = String::new();
        assert!(write(value, &mut out));
        out
    }

    /// Every geometry, each with and without altitude, as GeoJSON writes
    /// them: rings by the right-hand rule, so reading gives them back.
    fn every_geometry() -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        for z in [None, Some("8848.86")] {
            let p = |lon: &str, lat: &str| match z {
                Some(alt) => format!("[{lon},{lat},{alt}]"),
                None => format!("[{lon},{lat}]"),
            };
            let ring = format!(
                "[{},{},{},{},{}]",
                p("-0.2", "51.49"),
                p("-0.05", "51.49"),
                p("-0.05", "51.54"),
                p("-0.2", "51.54"),
                p("-0.2", "51.49")
            );
            let hole = format!(
                "[{},{},{},{},{}]",
                p("-0.15", "51.5"),
                p("-0.15", "51.52"),
                p("-0.1", "51.52"),
                p("-0.1", "51.5"),
                p("-0.15", "51.5")
            );
            let line = format!("[{},{}]", p("-0.1276", "51.5072"), p("2.3522", "48.8566"));
            for (ty, coordinates) in [
                (POINT, p("-0.1276", "51.5072")),
                (LINE_STRING, line.clone()),
                (POLYGON, format!("[{ring},{hole}]")),
                (
                    MULTI_POINT,
                    format!("[{},{}]", p("0", "0"), p("151.2093", "-33.8688")),
                ),
                (MULTI_LINE_STRING, format!("[{line},{line}]")),
                (MULTI_POLYGON, format!("[[{ring},{hole}],[{ring}]]")),
            ] {
                out.push((
                    ty,
                    format!(r#"{{"type":"{ty}","coordinates":{coordinates}}}"#),
                ));
            }
        }
        out
    }

    #[test]
    fn every_geometry_reads_back_from_geography_unchanged() {
        for (ty, text) in every_geometry() {
            let value = read_text(ty, &text).expect(&text);
            let held = shape(&value).expect("a shape");
            assert_eq!(held.standard_type(), ty);
            assert_eq!(from_shape(ty, &held).as_ref(), Ok(&value), "{text}");
            // As a `Geometry`, the variant its kind is.
            let geometry = read_text(GEOMETRY, &text).expect(&text);
            assert_eq!(shape(&geometry), Some(held.clone()));
            assert_eq!(from_shape(GEOMETRY, &held).as_ref(), Ok(&geometry));
            // Held in another type's column, refused.
            let other = if ty == POINT { LINE_STRING } else { POINT };
            assert!(from_shape(other, &held).is_err());
        }
        // A ring wound clockwise is stored as it was given, not rewound.
        let clockwise = Value::record(
            POLYGON,
            vec![
                (
                    "outer".into(),
                    Value::list(
                        [(0, 0), (1, 0), (1, 1), (0, 1), (0, 0)]
                            .into_iter()
                            .map(|(lat, lon)| {
                                point(Decimal::new(lat, 0), Decimal::new(lon, 0), Value::None)
                            })
                            .collect(),
                    ),
                ),
                ("holes".into(), Value::list(Vec::new())),
            ],
        );
        let held = shape(&clockwise).expect("a shape");
        assert_eq!(from_shape(POLYGON, &held).as_ref(), Ok(&clockwise));
        // A coordinate with more digits than a double holds is the double's.
        let far = Shape::Point(Coordinates::new(
            -0.1276,
            51.50720000001234567,
            Some(0.1 + 0.2),
        ));
        let Ok(Value::Record(record)) = from_shape(POINT, &far) else {
            panic!("a point");
        };
        assert_eq!(
            record.get("latitude"),
            Some(&Value::Decimal(
                Decimal::parse("51.50720000001235").expect("decimal")
            ))
        );
        assert_eq!(
            record.get("altitude"),
            Some(&Value::Quantity(crate::Quantity {
                kind: QuantityKind::Length,
                amount: 300_000,
            }))
        );
        // A row whose polygon isn't valid, written by another program.
        let bow_tie = Shape::Polygon(vec![
            [
                (0.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (10.0, 0.0),
                (0.0, 0.0),
            ]
            .into_iter()
            .map(|(lon, lat)| Coordinates::new(lon, lat, None))
            .collect(),
        ]);
        assert!(from_shape(POLYGON, &bow_tie).is_err());
        // Out of range.
        let beyond = Shape::Point(Coordinates::new(0.0, 95.0, None));
        assert!(from_shape(POINT, &beyond).is_err());
    }

    #[test]
    fn a_geometry_mixing_altitudes_isn_t_bound() {
        let mixed = Shape::MultiPoint(vec![
            Coordinates::new(0.0, 0.0, None),
            Coordinates::new(1.0, 1.0, Some(5.0)),
        ]);
        let stored = jadpo_std::standard::storage::Stored::Shape(mixed);
        assert!(
            jadpo_std::standard::storage::bind(&stored, jadpo_std::SqlType::Geography).is_err()
        );
        let value = read_text(POINT, r#"{"type":"Point","coordinates":[1,2]}"#).expect("point");
        let bound = jadpo_std::standard::storage::bind(
            &value.stored().expect("stored"),
            jadpo_std::SqlType::Geography,
        );
        assert_eq!(
            bound,
            Ok(jadpo_std::SqlValue::Geography(Shape::Point(
                Coordinates::new(1.0, 2.0, None)
            )))
        );
    }

    /// A geometry kept in a Redis entity's JSON is its GeoJSON (area 20,
    /// "A store with no PostGIS"), the contract's own reading and writing:
    /// every geometry, with and without altitude, comes back unchanged.
    #[test]
    fn every_geometry_round_trips_through_its_geojson() {
        for (ty, text) in every_geometry() {
            let value = read_text(ty, &text).expect(&text);
            assert_eq!(written(&value), text);
            assert_eq!(read_text(ty, &written(&value)).as_ref(), Ok(&value));
        }
    }

    #[test]
    fn a_point_s_axes_are_named_and_only_geojson_orders_them() {
        let london = read_text(POINT, r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#)
            .expect("London");
        let Value::Record(record) = &london else {
            panic!("a record");
        };
        assert_eq!(
            record.get("latitude"),
            Some(&Value::Decimal(Decimal::new(515_072, 4)))
        );
        assert_eq!(
            record.get("longitude"),
            Some(&Value::Decimal(Decimal::new(-1276, 4)))
        );
        assert_eq!(
            written(&london),
            r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#
        );
    }

    #[test]
    fn a_ring_wound_clockwise_is_read_and_written_counterclockwise() {
        let clockwise = r#"{"type":"Polygon","coordinates":[[[0,0],[0,1],[1,1],[1,0],[0,0]]]}"#;
        let polygon = read_text(POLYGON, clockwise).expect("read, not refused");
        assert_eq!(
            written(&polygon),
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#
        );
    }

    #[test]
    fn refusals_say_where_and_which_rule() {
        let refusal = |ty: &str, text: &str| read_text(ty, text).expect_err(text);
        assert_eq!(
            refusal(POINT, r#"{"type":"Point","coordinates":[0,91]}"#),
            Unreadable {
                at: vec!["coordinates".into(), "1".into()],
                rule: "max"
            }
        );
        assert_eq!(
            refusal(POINT, r#"{"type":"Point","coordinates":[-181,0]}"#).rule,
            "min"
        );
        assert_eq!(
            refusal(
                POLYGON,
                r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1]]]}"#
            ),
            Unreadable {
                at: vec!["coordinates".into(), "0".into()],
                rule: "format"
            }
        );
        assert_eq!(
            refusal(
                LINE_STRING,
                r#"{"type":"LineString","coordinates":[[0,0]]}"#
            )
            .rule,
            "min_length"
        );
        assert_eq!(
            refusal(GEOMETRY, r#"{"type":"GeometryCollection","geometries":[]}"#).rule,
            "unknown_field"
        );
        assert_eq!(refusal(GEOMETRY, r#"{"type":"Feature"}"#).rule, "variant");
    }

    #[test]
    fn a_polygon_that_isn_t_valid_is_refused_at_its_ring() {
        // Area 20, "Polygons are valid when built": each rule, read from
        // GeoJSON, refused as `format` at the ring that breaks it.
        let square = "[0,0],[10,0],[10,10],[0,10],[0,0]";
        let cases = [
            ("[[0,0],[10,10],[0,10],[10,0],[0,0]]", 0),
            ("[[0,0],[10,0],[5,0],[5,10],[0,0]]", 0),
            ("[[0,0],[10,0],[10,0],[10,10],[0,0]]", 0),
            ("[[0,10],[180,-10],[90,0],[0,10]]", 0),
            ("[[0,0],[90,0],[180,0],[-90,0],[0,0]]", 0),
        ];
        for (rings, ring) in cases {
            let text = format!(r#"{{"type":"Polygon","coordinates":[{rings}]}}"#);
            assert_eq!(
                read_text(POLYGON, &text),
                Err(Unreadable {
                    at: vec!["coordinates".into(), ring.to_string()],
                    rule: "format"
                }),
                "{text}"
            );
        }
        let holes = [
            ("[[20,20],[20,21],[21,21],[20,20]]", 1),
            ("[[5,5],[15,5],[15,6],[5,5]]", 1),
            ("[[0,0],[2,1],[1,2],[0,0]]", 1),
            (
                "[[1,1],[9,1],[9,9],[1,9],[1,1]],[[2,2],[2,3],[3,3],[2,2]]",
                2,
            ),
        ];
        for (rings, ring) in holes {
            let text = format!(r#"{{"type":"Polygon","coordinates":[[{square}],{rings}]}}"#);
            assert_eq!(
                read_text(POLYGON, &text),
                Err(Unreadable {
                    at: vec!["coordinates".into(), ring.to_string()],
                    rule: "format"
                }),
                "{text}"
            );
        }
        // Inside a multi-polygon, at its polygon and its ring.
        let text = r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]],[[[0,0],[10,10],[0,10],[10,0],[0,0]]]]}"#;
        assert_eq!(
            read_text(MULTI_POLYGON, text),
            Err(Unreadable {
                at: vec!["coordinates".into(), "1".into(), "0".into()],
                rule: "format"
            })
        );
    }

    #[test]
    fn a_double_is_read_as_its_shortest_decimal() {
        let number = |text: &str| decimal_of(&serde_json::from_str(text).expect("a number"));
        assert_eq!(number("51.5072"), Some(Decimal::new(515_072, 4)));
        assert_eq!(number("1e-7"), Some(Decimal::new(1, 7)));
        assert_eq!(number("100.0"), Some(Decimal::new(100, 0)));
        assert_eq!(number("-0.1276"), Some(Decimal::new(-1276, 4)));
    }

    #[test]
    fn map_tiles_stay_within_their_zoom() {
        let tile = |zoom: i64, x: i64, y: i64| {
            crate::across::broken_standard(&Value::record(
                MAP_TILE,
                vec![
                    ("zoom".into(), Value::Int(zoom)),
                    ("x".into(), Value::Int(x)),
                    ("y".into(), Value::Int(y)),
                ],
            ))
        };
        assert_eq!(tile(1, 1, 1), None);
        assert_eq!(tile(1, 2, 0), Some("within_zoom"));
        assert_eq!(tile(1, 0, 2), Some("within_zoom"));
        assert_eq!(tile(30, (1 << 30) - 1, 0), None);
    }
}
