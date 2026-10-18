//! WKT and EWKT (area 20, "Interop", question 9): the text form other
//! systems' exports and PostGIS's `ST_AsText` use, read by `geo.from_wkt`
//! and written by `geo.to_wkt`. Beside GeoJSON, this is the only place a
//! position's longitude comes first.
//!
//! Reading takes any of the six geometries, keywords in any case, `Z` for
//! an altitude, and `SRID=4326;` before it; each position is two or three
//! numbers, every position of a geometry the same, and each coordinate is
//! held as the shortest decimal of the double it is read as (question 2).
//! A refusal says where: the character the reader stopped at, counted from
//! 0, and which condition, `malformed` or `out_of_range`. Rings are kept
//! in the order they were written, as PostGIS keeps them; only GeoJSON
//! winds them by the right-hand rule.
//!
//! Writing is `ST_AsText`'s form: `POINT(-0.1276 51.5072)`, `POINT Z
//! (-0.1276 51.5072 35)`, a multi-point's points each in brackets, no
//! space after a comma, each coordinate the decimal the value holds.

use jadpo_std::QuantityKind;
use jadpo_std::standard::geo::{
    GEOJSON_TYPES, GEOMETRY, LINE_STRING, MULTI_LINE_STRING, MULTI_POINT, MULTI_POLYGON, POINT,
    POLYGON,
};

use super::{axes, listed, point, position_num, trimmed};
use crate::value::Decimal;
use crate::{Fault, Value};

/// Text that isn't one of the six geometries: anything WKT doesn't write,
/// an empty geometry, a measure (`M`), a line of one point, a ring that
/// isn't closed or valid, positions of different sizes.
pub const MALFORMED: &str = "malformed";

/// A coordinate out of its range, or a reference system other than WGS84
/// (`SRID=4258;`).
pub const OUT_OF_RANGE: &str = "out_of_range";

/// Why WKT wasn't read: the character it was refused at, counted from 0,
/// and the condition, [`MALFORMED`] or [`OUT_OF_RANGE`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub at: usize,
    pub rule: &'static str,
}

/// The `Geometry` WKT or EWKT text writes, holding the geometry of its
/// kind.
pub fn read(text: &str) -> Result<Value, Refusal> {
    let mut reader = Reader {
        chars: text.chars().collect(),
        at: 0,
        dimensions: None,
    };
    reader.ewkt()
}

/// `geo.from_wkt(text)`: the geometry, or the condition its `attempt` maps.
pub fn from_wkt(text: &Value) -> Result<Value, Fault> {
    let Value::Text(text) = text else {
        return Err(Fault::Shape);
    };
    read(text).map_err(|refusal| {
        Fault::Broke(if refusal.rule == OUT_OF_RANGE {
            jadpo_std::RuleKey::OutOfRange
        } else {
            jadpo_std::RuleKey::Malformed
        })
    })
}

/// `geo.to_wkt(shape)`: the shape as `ST_AsText` writes it. A geometry
/// whose points mix with and without an altitude has no WKT, as PostGIS
/// can't hold one either (area 20, "Chosen in chunk 5.5u4").
pub fn to_wkt(shape: &Value) -> Result<Value, Fault> {
    write(shape).map(|text| Value::text(&text))
}

/// The text `to_wkt` gives.
pub fn write(shape: &Value) -> Result<String, Fault> {
    let shape = match shape {
        Value::Variant(variant) if &*variant.ty == GEOMETRY => {
            &variant.fields.first().ok_or(Fault::Shape)?.1
        }
        other => other,
    };
    let Value::Record(record) = shape else {
        return Err(Fault::Shape);
    };
    let mut points = Vec::new();
    every_point(shape, &mut points)?;
    let with = points
        .iter()
        .filter(|point| altitude(point).is_some())
        .count();
    let z = match with {
        0 => false,
        n if n == points.len() => true,
        _ => return Err(Fault::MixedAltitudes),
    };
    let keyword = match &*record.ty {
        POINT => "POINT",
        LINE_STRING => "LINESTRING",
        POLYGON => "POLYGON",
        MULTI_POINT => "MULTIPOINT",
        MULTI_LINE_STRING => "MULTILINESTRING",
        MULTI_POLYGON => "MULTIPOLYGON",
        _ => return Err(Fault::Shape),
    };
    let mut out = String::from(keyword);
    if z {
        out.push_str(" Z ");
    }
    body(shape, &mut out)?;
    Ok(out)
}

/// Every point a geometry holds, in order.
fn every_point<'v>(shape: &'v Value, out: &mut Vec<&'v Value>) -> Result<(), Fault> {
    let Value::Record(record) = shape else {
        return Err(Fault::Shape);
    };
    let list = |name: &str| listed(record, name).ok_or(Fault::Shape);
    match &*record.ty {
        POINT => out.push(shape),
        LINE_STRING | MULTI_POINT => out.extend(list("points")?),
        POLYGON => {
            out.extend(list("outer")?);
            for hole in list("holes")? {
                out.extend(hole.items().ok_or(Fault::Shape)?);
            }
        }
        MULTI_LINE_STRING => {
            for line in list("lines")? {
                every_point(line, out)?;
            }
        }
        MULTI_POLYGON => {
            for polygon in list("polygons")? {
                every_point(polygon, out)?;
            }
        }
        _ => return Err(Fault::Shape),
    }
    Ok(())
}

/// A point's altitude in metres, if it has one.
fn altitude(point: &Value) -> Option<Decimal> {
    let Value::Record(record) = point else {
        return None;
    };
    match record.get("altitude") {
        Some(Value::Quantity(quantity)) => Some(trimmed(quantity.decimal())),
        _ => None,
    }
}

/// A geometry's bracketed body, appended to `out`.
fn body(shape: &Value, out: &mut String) -> Result<(), Fault> {
    let Value::Record(record) = shape else {
        return Err(Fault::Shape);
    };
    let list = |name: &str| listed(record, name).ok_or(Fault::Shape);
    match &*record.ty {
        POINT => {
            out.push('(');
            position(shape, out)?;
            out.push(')');
        }
        LINE_STRING => positions(list("points")?, out)?,
        MULTI_POINT => each(list("points")?, out, |point, out| {
            out.push('(');
            position(point, out)?;
            out.push(')');
            Ok(())
        })?,
        POLYGON => {
            let mut rings = vec![list("outer")?];
            for hole in list("holes")? {
                rings.push(hole.items().ok_or(Fault::Shape)?);
            }
            out.push('(');
            for (n, ring) in rings.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                positions(ring, out)?;
            }
            out.push(')');
        }
        MULTI_LINE_STRING => each(list("lines")?, out, body)?,
        MULTI_POLYGON => each(list("polygons")?, out, body)?,
        _ => return Err(Fault::Shape),
    }
    Ok(())
}

/// Each item written by `write`, in brackets, joined by commas.
fn each(
    items: &[Value],
    out: &mut String,
    write: impl Fn(&Value, &mut String) -> Result<(), Fault>,
) -> Result<(), Fault> {
    out.push('(');
    for (n, item) in items.iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        write(item, out)?;
    }
    out.push(')');
    Ok(())
}

fn positions(points: &[Value], out: &mut String) -> Result<(), Fault> {
    each(points, out, position)
}

/// A position, its longitude first, then its latitude, then its altitude
/// in metres where it has one.
fn position(point: &Value, out: &mut String) -> Result<(), Fault> {
    let (latitude, longitude) = axes(point).ok_or(Fault::Shape)?;
    out.push_str(&longitude.to_string());
    out.push(' ');
    out.push_str(&latitude.to_string());
    if let Some(metres) = altitude(point) {
        out.push(' ');
        out.push_str(&metres.to_string());
    }
    Ok(())
}

/// The reader: the text's characters, where it is, and how many numbers a
/// position has once `Z` or the first position says.
struct Reader {
    chars: Vec<char>,
    at: usize,
    dimensions: Option<usize>,
}

/// One of the six kinds, by its keyword.
const KINDS: [(&str, &str); 6] = [
    ("POINT", POINT),
    ("LINESTRING", LINE_STRING),
    ("POLYGON", POLYGON),
    ("MULTIPOINT", MULTI_POINT),
    ("MULTILINESTRING", MULTI_LINE_STRING),
    ("MULTIPOLYGON", MULTI_POLYGON),
];

impl Reader {
    fn refused<T>(&self, at: usize, rule: &'static str) -> Result<T, Refusal> {
        Err(Refusal { at, rule })
    }

    /// The characters from `start` to where the reader is.
    fn text(&self, start: usize) -> String {
        self.chars
            .get(start..self.at)
            .unwrap_or_default()
            .iter()
            .collect()
    }

    fn spaces(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    /// The next character, after any spaces, if it is `wanted`.
    fn take(&mut self, wanted: char) -> bool {
        self.spaces();
        if self.chars.get(self.at) == Some(&wanted) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, wanted: char) -> Result<(), Refusal> {
        if self.take(wanted) {
            Ok(())
        } else {
            self.refused(self.at, MALFORMED)
        }
    }

    /// A word of letters, in upper case, and where it starts.
    fn word(&mut self) -> (usize, String) {
        self.spaces();
        let start = self.at;
        while self
            .chars
            .get(self.at)
            .is_some_and(char::is_ascii_alphabetic)
        {
            self.at += 1;
        }
        let word = self.text(start);
        (start, word.to_ascii_uppercase())
    }

    /// `SRID=4326;`, if it leads, then the geometry, then nothing more.
    fn ewkt(&mut self) -> Result<Value, Refusal> {
        let start = self.at;
        let (at, word) = self.word();
        let (kind_at, kind_word) = if word == "SRID" {
            self.expect('=')?;
            self.spaces();
            let number_at = self.at;
            let digits = self.digits();
            if digits.is_empty() {
                return self.refused(number_at, MALFORMED);
            }
            if digits != "4326" {
                return self.refused(number_at, OUT_OF_RANGE);
            }
            self.expect(';')?;
            self.word()
        } else {
            (at, word)
        };
        if kind_word.is_empty() {
            return self.refused(kind_at.max(start), MALFORMED);
        }
        let value = self.geometry(kind_at, &kind_word)?;
        self.spaces();
        if self.at < self.chars.len() {
            return self.refused(self.at, MALFORMED);
        }
        Ok(value)
    }

    fn digits(&mut self) -> String {
        let start = self.at;
        while self.chars.get(self.at).is_some_and(char::is_ascii_digit) {
            self.at += 1;
        }
        self.text(start)
    }

    /// A geometry from its keyword, which may carry its `Z` (`POINTZ`), as
    /// the `Geometry` of its kind.
    fn geometry(&mut self, at: usize, word: &str) -> Result<Value, Refusal> {
        let found = KINDS.iter().find_map(|(keyword, kind)| {
            let rest = word.strip_prefix(keyword)?;
            matches!(rest, "" | "Z" | "M" | "ZM").then_some((*kind, rest.len(), rest))
        });
        let Some((kind, marker_length, marker)) = found else {
            return self.refused(at, MALFORMED);
        };
        let marker_at = at + word.len() - marker_length;
        let marker = if marker.is_empty() {
            // `POINT Z (...)`: the marker as a word of its own.
            let before = self.at;
            let (after_at, after) = self.word();
            match after.as_str() {
                "" => {
                    self.at = before;
                    None
                }
                "Z" | "M" | "ZM" => Some((after_at, after)),
                // `POINT EMPTY`: a geometry with no points isn't one of
                // the six.
                _ => return self.refused(after_at, MALFORMED),
            }
        } else {
            Some((marker_at, marker.to_string()))
        };
        match marker {
            Some((_, z)) if z == "Z" => self.dimensions = Some(3),
            // A measure, which no geometry holds.
            Some((marker_at, _)) => return self.refused(marker_at, MALFORMED),
            None => {}
        }
        let value = match kind {
            POINT => {
                self.expect('(')?;
                let point = self.position()?;
                self.expect(')')?;
                point
            }
            LINE_STRING => self.line()?,
            POLYGON => self.polygon()?,
            MULTI_POINT => {
                let points = self.list(|reader| {
                    // `MULTIPOINT((1 2),(3 4))` or `MULTIPOINT(1 2,3 4)`.
                    if reader.take('(') {
                        let point = reader.position()?;
                        reader.expect(')')?;
                        Ok(point)
                    } else {
                        reader.position()
                    }
                })?;
                Value::record(MULTI_POINT, vec![("points".into(), Value::list(points))])
            }
            MULTI_LINE_STRING => {
                let lines = self.list(Reader::line)?;
                Value::record(
                    MULTI_LINE_STRING,
                    vec![("lines".into(), Value::list(lines))],
                )
            }
            _ => {
                let polygons = self.list(Reader::polygon)?;
                Value::record(
                    MULTI_POLYGON,
                    vec![("polygons".into(), Value::list(polygons))],
                )
            }
        };
        let (ordinal, (variant, _)) = GEOJSON_TYPES
            .iter()
            .enumerate()
            .find(|(_, (_, name))| *name == kind)
            .ok_or(Refusal {
                at,
                rule: MALFORMED,
            })?;
        Ok(Value::variant(
            GEOMETRY,
            ordinal as u32,
            variant,
            vec![((*variant).into(), value)],
        ))
    }

    /// `(item, item, ...)`, at least one.
    fn list(
        &mut self,
        mut item: impl FnMut(&mut Reader) -> Result<Value, Refusal>,
    ) -> Result<Vec<Value>, Refusal> {
        self.expect('(')?;
        let mut items = vec![item(self)?];
        while self.take(',') {
            items.push(item(self)?);
        }
        self.expect(')')?;
        Ok(items)
    }

    /// A `LineString`: two positions or more.
    fn line(&mut self) -> Result<Value, Refusal> {
        self.spaces();
        let start = self.at;
        let points = self.list(Reader::position)?;
        if points.len() < 2 {
            return self.refused(start, MALFORMED);
        }
        Ok(Value::record(
            LINE_STRING,
            vec![("points".into(), Value::list(points))],
        ))
    }

    /// A `Polygon`: its outer ring and its holes, each closed with four
    /// positions or more, and valid on the sphere (area 20, "Polygons are
    /// valid when built"), refused at the ring that isn't.
    fn polygon(&mut self) -> Result<Value, Refusal> {
        let mut starts = Vec::new();
        let rings = self.list(|reader| {
            reader.spaces();
            starts.push(reader.at);
            let start = reader.at;
            let points = reader.list(Reader::position)?;
            let closed = match (points.first(), points.last()) {
                (Some(first), Some(last)) => crate::ops::equal(first, last),
                _ => false,
            };
            if points.len() < 4 || !closed {
                return reader.refused(start, MALFORMED);
            }
            Ok(Value::list(points))
        })?;
        let rings: Vec<Vec<Value>> = rings
            .into_iter()
            .map(|ring| ring.items().map(<[Value]>::to_vec).unwrap_or_default())
            .collect();
        let positions: Vec<Vec<jadpo_std::sphere::Position>> = rings
            .iter()
            .map(|ring| ring.iter().filter_map(position_num).collect())
            .collect();
        if let Some((outer, holes)) = positions.split_first()
            && let Some(broken) = jadpo_std::sphere::rings_broken(outer, holes)
        {
            let at = starts.get(broken).copied().unwrap_or(self.at);
            return self.refused(at, MALFORMED);
        }
        let mut rings = rings.into_iter();
        let outer = rings.next().unwrap_or_default();
        Ok(Value::record(
            POLYGON,
            vec![
                ("outer".into(), Value::list(outer)),
                (
                    "holes".into(),
                    Value::list(rings.map(Value::list).collect()),
                ),
            ],
        ))
    }

    /// A position, `longitude latitude` or `longitude latitude altitude`,
    /// as a `Point`; every position of a geometry has as many numbers as
    /// `Z`, or its first, says.
    fn position(&mut self) -> Result<Value, Refusal> {
        self.spaces();
        let start = self.at;
        let mut numbers = Vec::new();
        while let Some(number) = self.number()? {
            numbers.push(number);
        }
        if !matches!(numbers.len(), 2 | 3) {
            let at = if numbers.is_empty() { start } else { self.at };
            return self.refused(at, MALFORMED);
        }
        match self.dimensions {
            Some(wanted) if wanted != numbers.len() => return self.refused(start, MALFORMED),
            Some(_) => {}
            None => self.dimensions = Some(numbers.len()),
        }
        let within = |(at, value): (usize, Decimal), bound: i128| {
            if value < Decimal::new(-bound, 0) || value > Decimal::new(bound, 0) {
                Err(Refusal {
                    at,
                    rule: OUT_OF_RANGE,
                })
            } else {
                Ok(value)
            }
        };
        let (Some(&first), Some(&second)) = (numbers.first(), numbers.get(1)) else {
            return self.refused(start, MALFORMED);
        };
        let longitude = within(first, 180)?;
        let latitude = within(second, 90)?;
        let altitude = match numbers.get(2) {
            Some((at, metres)) => {
                let unit = QuantityKind::Length.base_unit();
                match jadpo_std::quantity::to_base(metres.mantissa, metres.scale, unit) {
                    Some(amount) => Value::Quantity(crate::Quantity {
                        kind: QuantityKind::Length,
                        amount,
                    }),
                    None => return self.refused(*at, OUT_OF_RANGE),
                }
            }
            None => Value::None,
        };
        Ok(point(latitude, longitude, altitude))
    }

    /// The next number, with where it starts, held as the shortest decimal
    /// of the double it is; `None` where no number starts.
    fn number(&mut self) -> Result<Option<(usize, Decimal)>, Refusal> {
        self.spaces();
        let start = self.at;
        let is = |reader: &Reader, test: fn(&char) -> bool| {
            reader.chars.get(reader.at).is_some_and(test)
        };
        if is(self, |c| matches!(c, '+' | '-')) {
            self.at += 1;
        }
        let whole = self.digits();
        let mut fraction = String::new();
        if is(self, |c| *c == '.') {
            self.at += 1;
            fraction = self.digits();
        }
        if whole.is_empty() && fraction.is_empty() {
            if self.at == start {
                return Ok(None);
            }
            return self.refused(start, MALFORMED);
        }
        if is(self, |c| matches!(c, 'e' | 'E')) {
            self.at += 1;
            if is(self, |c| matches!(c, '+' | '-')) {
                self.at += 1;
            }
            if self.digits().is_empty() {
                return self.refused(start, MALFORMED);
            }
        }
        let written = self.text(start);
        // A number past a double's range is past every coordinate's.
        let Some(nearest) = jadpo_std::standard::geo::nearest_double(&written) else {
            return self.refused(start, OUT_OF_RANGE);
        };
        let Some(value) = Decimal::parse(&nearest) else {
            return self.refused(start, OUT_OF_RANGE);
        };
        Ok(Some((start, value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polygon_value(rings: &[&[(&str, &str)]], altitude: Option<&str>) -> Value {
        let ring = |points: &[(&str, &str)]| {
            Value::list(
                points
                    .iter()
                    .map(|(lon, lat)| {
                        point(
                            Decimal::parse(lat).expect("latitude"),
                            Decimal::parse(lon).expect("longitude"),
                            match altitude {
                                Some(metres) => Value::Quantity(crate::Quantity {
                                    kind: QuantityKind::Length,
                                    amount: jadpo_std::quantity::to_base(
                                        Decimal::parse(metres).expect("metres").mantissa,
                                        Decimal::parse(metres).expect("metres").scale,
                                        QuantityKind::Length.base_unit(),
                                    )
                                    .expect("a length"),
                                }),
                                None => Value::None,
                            },
                        )
                    })
                    .collect(),
            )
        };
        let polygon = Value::record(
            POLYGON,
            vec![
                ("outer".into(), ring(rings[0])),
                (
                    "holes".into(),
                    Value::list(rings[1..].iter().map(|r| ring(r)).collect()),
                ),
            ],
        );
        Value::variant(GEOMETRY, 2, "polygon", vec![("polygon".into(), polygon)])
    }

    const PATCH: &[(&str, &str)] = &[
        ("-0.2", "51.49"),
        ("-0.05", "51.49"),
        ("-0.05", "51.54"),
        ("-0.2", "51.54"),
        ("-0.2", "51.49"),
    ];
    const HOLE: &[(&str, &str)] = &[
        ("-0.15", "51.5"),
        ("-0.15", "51.52"),
        ("-0.1", "51.52"),
        ("-0.1", "51.5"),
        ("-0.15", "51.5"),
    ];

    #[test]
    fn a_polygon_is_written_as_st_astext_writes_it_and_read_back() {
        let patch = polygon_value(&[PATCH, HOLE], None);
        let text = write(&patch).expect("WKT");
        assert_eq!(
            text,
            "POLYGON((-0.2 51.49,-0.05 51.49,-0.05 51.54,-0.2 51.54,-0.2 51.49),(-0.15 51.5,-0.15 51.52,-0.1 51.52,-0.1 51.5,-0.15 51.5))"
        );
        assert_eq!(read(&text), Ok(patch));
        let high = polygon_value(&[PATCH], Some("35.5"));
        let text = write(&high).expect("WKT");
        assert!(text.starts_with("POLYGON Z ((-0.2 51.49 35.5,"), "{text}");
        assert_eq!(read(&text), Ok(high));
    }

    #[test]
    fn every_kind_reads_and_writes_in_postgis_s_form() {
        for (given, written) in [
            ("POINT(-0.1276 51.5072)", "POINT(-0.1276 51.5072)"),
            ("point z(1 2 3.5)", "POINT Z (1 2 3.5)"),
            (
                "SRID=4326;POINT Z (-0.1276 51.5072 35)",
                "POINT Z (-0.1276 51.5072 35)",
            ),
            ("POINTZ(1 2 3)", "POINT Z (1 2 3)"),
            ("POINT(1 2 3)", "POINT Z (1 2 3)"),
            ("LINESTRING(0 0, 1 1)", "LINESTRING(0 0,1 1)"),
            ("MULTIPOINT(1 2,3 4)", "MULTIPOINT((1 2),(3 4))"),
            ("MULTIPOINT((1 2),(3 4))", "MULTIPOINT((1 2),(3 4))"),
            (
                "MULTILINESTRING((0 0,1 1),(2 2,3 3))",
                "MULTILINESTRING((0 0,1 1),(2 2,3 3))",
            ),
            (
                "MULTIPOLYGON(((0 0,1 0,1 1,0 0)),((2 2,3 2,3 3,2 2)))",
                "MULTIPOLYGON(((0 0,1 0,1 1,0 0)),((2 2,3 2,3 3,2 2)))",
            ),
            ("POINT(1.5e2 3)", "POINT(150 3)"),
            ("POINT(-0 -0)", "POINT(0 0)"),
            (
                "POINT(51.50720000001234567 1e-7)",
                "POINT(51.50720000001235 0.0000001)",
            ),
            ("  POINT ( 1   2 )  ", "POINT(1 2)"),
        ] {
            let value = read(given).unwrap_or_else(|refusal| panic!("{given}: {refusal:?}"));
            assert_eq!(write(&value).as_deref(), Ok(written), "{given}");
            assert_eq!(read(written), Ok(value), "{written}");
        }
    }

    #[test]
    fn a_refusal_names_the_place_and_the_condition() {
        let refused = |text: &str| read(text).expect_err(text);
        let at = |at: usize, rule: &'static str| Refusal { at, rule };
        // Anything that isn't one of the six, at its first character.
        assert_eq!(refused(""), at(0, MALFORMED));
        assert_eq!(refused("CIRCLE(0 0 1)"), at(0, MALFORMED));
        assert_eq!(refused("GEOMETRYCOLLECTION(POINT(0 0))"), at(0, MALFORMED));
        assert_eq!(refused("POINT EMPTY"), at(6, MALFORMED));
        assert_eq!(refused("POINT M (1 2 3)"), at(6, MALFORMED));
        assert_eq!(refused("POINTZM(1 2 3 4)"), at(5, MALFORMED));
        // A missing bracket, a stray character, trailing text.
        assert_eq!(refused("POINT 1 2"), at(6, MALFORMED));
        assert_eq!(refused("POINT(1 2"), at(9, MALFORMED));
        assert_eq!(refused("POINT(1 x)"), at(8, MALFORMED));
        assert_eq!(refused("POINT(1 2) extra"), at(11, MALFORMED));
        assert_eq!(refused("POINT(1 2 3 4)"), at(13, MALFORMED));
        assert_eq!(refused("POINT(1)"), at(7, MALFORMED));
        assert_eq!(refused("POINT(1. 2e)"), at(9, MALFORMED));
        // A coordinate out of range, at its number; another system, at
        // its code.
        assert_eq!(refused("POINT(0 91)"), at(8, OUT_OF_RANGE));
        assert_eq!(refused("LINESTRING(0 0, -181 0)"), at(16, OUT_OF_RANGE));
        assert_eq!(refused("POINT(1e400 0)"), at(6, OUT_OF_RANGE));
        assert_eq!(refused("SRID=4258;POINT(1 2)"), at(5, OUT_OF_RANGE));
        assert_eq!(refused("SRID=;POINT(1 2)"), at(5, MALFORMED));
        // A line of one point, a ring not closed or too short, a ring that
        // crosses itself, at the line's or the ring's bracket.
        assert_eq!(refused("LINESTRING(0 0)"), at(10, MALFORMED));
        assert_eq!(refused("POLYGON((0 0,1 0,1 1,0 1))"), at(8, MALFORMED));
        assert_eq!(refused("POLYGON((0 0,1 0,0 0))"), at(8, MALFORMED));
        assert_eq!(
            refused("POLYGON((0 0,10 0,10 10,0 10,0 0),(20 20,20 21,21 21,20 20))"),
            at(34, MALFORMED)
        );
        assert_eq!(
            refused("POLYGON((0 0,10 10,0 10,10 0,0 0))"),
            at(8, MALFORMED)
        );
        // Positions of different sizes (provisional: area 20, "Chosen in
        // chunk 5.5u6"), and fewer than `Z` says.
        assert_eq!(refused("LINESTRING(0 0,1 1 5)"), at(15, MALFORMED));
        assert_eq!(refused("LINESTRING Z (0 0 1,1 1)"), at(20, MALFORMED));
    }

    #[test]
    fn a_geometry_mixing_altitudes_has_no_wkt() {
        let mixed = read("MULTIPOINT(0 0 5,1 1 5)").expect("points");
        let Value::Variant(variant) = &mixed else {
            panic!("a geometry");
        };
        let Value::Record(points) = &variant.fields[0].1 else {
            panic!("a multi-point");
        };
        let mut items = listed(points, "points").expect("points").to_vec();
        items[1] = point(Decimal::new(1, 0), Decimal::new(1, 0), Value::None);
        let mixed = Value::record(MULTI_POINT, vec![("points".into(), Value::list(items))]);
        assert_eq!(write(&mixed), Err(Fault::MixedAltitudes));
    }

    #[test]
    fn the_operations_fail_with_their_conditions() {
        assert_eq!(
            from_wkt(&Value::text("POINT(0 91)")),
            Err(Fault::Broke(jadpo_std::RuleKey::OutOfRange))
        );
        assert_eq!(
            from_wkt(&Value::text("POINT(0)")),
            Err(Fault::Broke(jadpo_std::RuleKey::Malformed))
        );
        let point = from_wkt(&Value::text("POINT(-0.1276 51.5072)")).expect("a point");
        assert_eq!(to_wkt(&point), Ok(Value::text("POINT(-0.1276 51.5072)")));
    }
}
