//! The geospatial types on the wire and for people (area 19, chunk 5.5s).
//!
//! A geometry is sent as GeoJSON (RFC 7946), and this module is the only
//! place a position's `[longitude, latitude]` order exists: everywhere else
//! a `Point` is its `latitude` and `longitude` by name. Reading checks each
//! coordinate's range, each line's and ring's length, each ring's closure
//! and every member, and rewinds a ring wound the other way, as RFC 7946
//! §3.1.6 asks a parser not to refuse one; writing winds every ring by the
//! right-hand rule (the outer ring counterclockwise, each hole clockwise).
//! The data is `jadpo_std::standard::geo`'s.

use jadpo_std::standard::geo::{
    BOUNDING_BOX, EASTING_NORTHING, GEOJSON_TYPES, GEOMETRY, LINE_STRING, MAP_TILE,
    MULTI_LINE_STRING, MULTI_POINT, MULTI_POLYGON, POINT, POLYGON,
};
use jadpo_std::{QuantityKind, Rounding};

use crate::value::Decimal;
use crate::{Fault, Record, Value, present};

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

/// The part of a map tile its zoom refuses, if any: a column or a row at
/// or past 2 to the zoom.
pub fn broken_part(record: &Record) -> Option<&'static str> {
    if &*record.ty != MAP_TILE {
        return None;
    }
    let Some(Value::Int(zoom)) = record.get("zoom") else {
        return None;
    };
    let tiles = 1i64.checked_shl(u32::try_from(*zoom).ok()?)?;
    ["x", "y"]
        .into_iter()
        .find(|name| matches!(record.get(name), Some(Value::Int(n)) if *n >= tiles))
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
            let Value::Record(record) = Value::record(
                MAP_TILE,
                vec![
                    ("zoom".into(), Value::Int(zoom)),
                    ("x".into(), Value::Int(x)),
                    ("y".into(), Value::Int(y)),
                ],
            ) else {
                unreachable!()
            };
            broken_part(&record)
        };
        assert_eq!(tile(1, 1, 1), None);
        assert_eq!(tile(1, 2, 0), Some("x"));
        assert_eq!(tile(1, 0, 2), Some("y"));
        assert_eq!(tile(30, (1 << 30) - 1, 0), None);
    }
}
