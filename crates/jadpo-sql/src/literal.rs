//! A value written into a statement's text (area 19, "Every boundary
//! through the contract"; chunk 5.5w3b), for the places a statement can't
//! take a parameter: a partial index's predicate and a check constraint
//! (DDL takes none), a migration's backfill (a `postgres.sql` file run as
//! it is), and the identity statements whose parameters are fixed. Every
//! other statement binds a literal as a parameter, through [`crate::Bind`].
//!
//! What is written is a [`SqlValue`], the one the storage codec
//! (`jadpo_std::standard::storage::bind`) makes and the runtime would bind,
//! so a literal means in SQL text exactly what it means as a parameter. Its
//! text is PostgreSQL's own spelling of that value, typed by a cast where
//! the bare spelling could be read as another type, and means the same
//! whatever the session's settings are:
//!
//! - text is quoted with each `'` doubled; text holding a `\` or an ASCII
//!   control character is written `E'...'` with each `\` doubled and each
//!   control character escaped (`\n`, `\x0b`), so
//!   `standard_conforming_strings` doesn't change it and it stays on one
//!   line of the file it is written into (a migration's sections are read
//!   line by line, which drops a `\r` and would take a line of text
//!   starting `-- store:` for a section of its own); text holding the NUL
//!   character, which PostgreSQL text can't hold, isn't written at all;
//! - an instant is written in UTC with its offset (`+00`), so the session's
//!   `TimeZone` doesn't change it, and a year before 1 as `BC`;
//! - a number is written bare, and only when it is digits, a sign and a
//!   point.

use jadpo_std::{SqlType, SqlValue};

/// `value` as PostgreSQL text meaning exactly that value; `None` for one
/// SQL text can't hold (text with the NUL character, a malformed number,
/// a range of a type PostgreSQL has none of).
pub fn sql(value: &SqlValue) -> Option<String> {
    Some(match value {
        SqlValue::Null(ty) => format!("NULL::{}", type_name(*ty)?),
        SqlValue::Bool(true) => "TRUE".to_string(),
        SqlValue::Bool(false) => "FALSE".to_string(),
        SqlValue::Int2(n) => n.to_string(),
        SqlValue::Int4(n) => n.to_string(),
        SqlValue::Int8(n) => n.to_string(),
        SqlValue::Numeric(text) => {
            // A constant with a point is `numeric`, and a whole one meets
            // a `numeric` column as one.
            if !is_number(text) {
                return None;
            }
            text.clone()
        }
        SqlValue::Text(text) => text_value(text)?,
        SqlValue::Json(text) => format!("{}::jsonb", text_value(text)?),
        SqlValue::Bytes(bytes) => {
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            format!("E'\\\\x{hex}'::bytea")
        }
        SqlValue::Uuid(bytes) => format!("'{}'::uuid", uuid(bytes)),
        SqlValue::Timestamp(micros) => format!("'{}'::timestamptz", timestamp(*micros, "+00")),
        SqlValue::LocalTimestamp(micros) => format!("'{}'::timestamp", timestamp(*micros, "")),
        SqlValue::Date(day) => {
            let (date, era) = date(*day);
            format!("'{date}{era}'::date")
        }
        SqlValue::Time(micros) => format!("'{}'::time", time_of_day(*micros)),
        // A distance condition's bound (chunk 5.8c): the shortest decimal
        // that reads back as the same double, which PostgreSQL reads
        // exactly.
        SqlValue::Float8(bits) => {
            let double = f64::from_bits(*bits);
            if double.is_nan() {
                "'NaN'::float8".to_string()
            } else if double.is_infinite() {
                format!("'{}Infinity'::float8", if double < 0.0 { "-" } else { "" })
            } else {
                format!("'{double:e}'::float8")
            }
        }
        SqlValue::Array(element, items) => {
            let ty = type_name(*element)?;
            if items.is_empty() {
                format!("'{{}}'::{ty}[]")
            } else {
                let mut written = Vec::with_capacity(items.len());
                for item in items {
                    written.push(sql(item)?);
                }
                format!("ARRAY[{}]::{ty}[]", written.join(", "))
            }
        }
        SqlValue::Range {
            element,
            lower,
            upper,
            upper_held,
        } => {
            let ty = type_name(*element)?;
            let end = |end: &Option<Box<SqlValue>>| match end {
                Some(end) => sql(end),
                None => Some(format!("NULL::{ty}")),
            };
            format!(
                "{}({}, {}, '[{}')",
                range_name(*element)?,
                end(lower)?,
                end(upper)?,
                if *upper_held { "]" } else { ")" }
            )
        }
        // pgvector's text, each item the shortest decimal that is its float
        // (area 21, chunk 5.5v1); one that isn't finite isn't a vector.
        SqlValue::Vector(bits) => {
            let mut items = Vec::with_capacity(bits.len());
            for item in bits.iter().copied().map(f32::from_bits) {
                if !item.is_finite() {
                    return None;
                }
                items.push(format!("{item}"));
            }
            format!("'[{}]'::vector", items.join(","))
        }
        // EWKT in WGS84, each coordinate the shortest decimal that is its
        // double, so PostGIS reads back the double the runtime binds (area
        // 20, chunk 5.5u4).
        SqlValue::Geography(shape) => format!("'{}'::geography", ewkt(shape)?),
    })
}

/// A geometry as PostGIS's extended WKT: `SRID=4326;POINT Z (-0.1276
/// 51.5072 35)`, longitude first; `None` for one whose positions mix with
/// and without an altitude, which PostGIS can't hold.
fn ewkt(shape: &jadpo_std::geography::Shape) -> Option<String> {
    use jadpo_std::geography::{Coordinates, Shape};
    let z = shape.has_altitude()?;
    let position = |at: &Coordinates| match at.altitude() {
        Some(altitude) => format!("{} {} {altitude}", at.longitude(), at.latitude()),
        None => format!("{} {}", at.longitude(), at.latitude()),
    };
    let points = |points: &[Coordinates]| {
        format!(
            "({})",
            points.iter().map(position).collect::<Vec<_>>().join(", ")
        )
    };
    let rings = |rings: &[Vec<Coordinates>]| {
        format!(
            "({})",
            rings
                .iter()
                .map(|ring| points(ring))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let body = match shape {
        Shape::Point(at) => format!("({})", position(at)),
        Shape::LineString(line) | Shape::MultiPoint(line) => points(line),
        Shape::Polygon(polygon) | Shape::MultiLineString(polygon) => rings(polygon),
        Shape::MultiPolygon(polygons) => format!(
            "({})",
            polygons
                .iter()
                .map(|polygon| rings(polygon))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    Some(format!(
        "SRID={};{}{} {body}",
        jadpo_std::geography::SRID,
        shape.geometry_type(),
        if z { " Z" } else { "" }
    ))
}

/// Text as a quoted literal: `'it''s'`, or an escape string when it holds
/// a backslash or an ASCII control character, `E'a\\b\r\n'`, so it is
/// one line; `None` when it holds the NUL character, which PostgreSQL text
/// can't hold.
pub fn text_value(text: &str) -> Option<String> {
    if text.contains('\0') {
        return None;
    }
    if !text.chars().any(|c| c == '\\' || c.is_ascii_control()) {
        return Some(format!("'{}'", text.replace('\'', "''")));
    }
    let mut out = String::from("E'");
    for c in text.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('\'');
    Some(out)
}

/// Whether `text` is a number as the codec writes one: an optional `-`,
/// digits, and optionally a point and more digits.
fn is_number(text: &str) -> bool {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (whole, places) = match unsigned.split_once('.') {
        Some((whole, places)) => (whole, Some(places)),
        None => (unsigned, None),
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    digits(whole) && places.is_none_or(digits)
}

/// The PostgreSQL type a value of `ty` is cast to; `None` for a range of
/// a type PostgreSQL has none of.
fn type_name(ty: SqlType) -> Option<String> {
    Some(match ty {
        SqlType::Bool => "boolean".into(),
        SqlType::Int2 => "smallint".into(),
        SqlType::Int4 => "integer".into(),
        SqlType::Int8 => "bigint".into(),
        SqlType::Numeric => "numeric".into(),
        SqlType::Text => "text".into(),
        SqlType::Bytes => "bytea".into(),
        SqlType::Uuid => "uuid".into(),
        SqlType::Timestamp => "timestamptz".into(),
        SqlType::Date => "date".into(),
        SqlType::Time => "time".into(),
        SqlType::LocalTimestamp => "timestamp".into(),
        SqlType::Json => "jsonb".into(),
        SqlType::Vector => "vector".into(),
        SqlType::Geography => "geography".into(),
        SqlType::Float8 => "double precision".into(),
        SqlType::Array(inner) => format!("{}[]", type_name(*inner)?),
        SqlType::Range(element) => range_name(*element)?.into(),
    })
}

/// The range type of `element`'s values, whose name is also its
/// constructor.
fn range_name(element: SqlType) -> Option<&'static str> {
    Some(match element {
        SqlType::Date => "daterange",
        SqlType::Timestamp => "tstzrange",
        SqlType::Int8 => "int8range",
        SqlType::Numeric => "numrange",
        _ => return None,
    })
}

/// A UUID's text: lowercase hexadecimal in groups of 8-4-4-4-12.
fn uuid(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(36);
    for (i, byte) in bytes.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

const MICROS_PER_DAY: i64 = 86_400_000_000;

/// The date of day `day` since 1970-01-01 as PostgreSQL reads it, and
/// ` BC` for a year before 1, which PostgreSQL counts without a year 0.
fn date(day: i32) -> (String, &'static str) {
    let (year, month, day) = jadpo_std::civil_date(day);
    let (year, era) = if year < 1 {
        (1 - year, " BC")
    } else {
        (year, "")
    };
    (format!("{year:04}-{month:02}-{day:02}"), era)
}

/// A time of day from microseconds since midnight: `09:30:00.000000`.
fn time_of_day(micros: i64) -> String {
    let seconds = micros.div_euclid(1_000_000);
    format!(
        "{:02}:{:02}:{:02}.{:06}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        micros.rem_euclid(1_000_000)
    )
}

/// Microseconds since 1970-01-01T00:00:00 as a timestamp's text, with
/// `offset` after its time and its era last.
fn timestamp(micros: i64, offset: &str) -> String {
    let day = micros.div_euclid(MICROS_PER_DAY);
    let (date, era) = date(i32::try_from(day).unwrap_or(i32::MAX));
    format!(
        "{date} {}{offset}{era}",
        time_of_day(micros.rem_euclid(MICROS_PER_DAY))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_geometry_is_postgis_s_extended_wkt() {
        use jadpo_std::geography::{Coordinates, Shape};
        let at = |lon: f64, lat: f64, z: Option<f64>| Coordinates::new(lon, lat, z);
        let point = SqlValue::Geography(Shape::Point(at(-0.1276, 51.5072, Some(8848.86))));
        assert_eq!(
            sql(&point).as_deref(),
            Some("'SRID=4326;POINT Z (-0.1276 51.5072 8848.86)'::geography")
        );
        let ring = vec![
            at(0.0, 0.0, None),
            at(1.0, 0.0, None),
            at(1.0, 1.0, None),
            at(0.0, 0.0, None),
        ];
        let patches =
            SqlValue::Geography(Shape::MultiPolygon(vec![vec![ring.clone()], vec![ring]]));
        assert_eq!(
            sql(&patches).as_deref(),
            Some(
                "'SRID=4326;MULTIPOLYGON (((0 0, 1 0, 1 1, 0 0)), ((0 0, 1 0, 1 1, 0 0)))'::geography"
            )
        );
        let mixed = SqlValue::Geography(Shape::MultiPoint(vec![
            at(0.0, 0.0, None),
            at(1.0, 1.0, Some(2.0)),
        ]));
        assert_eq!(sql(&mixed), None);
        assert_eq!(
            sql(&SqlValue::Null(SqlType::Geography)).as_deref(),
            Some("NULL::geography")
        );
    }

    #[test]
    fn a_vector_is_pgvector_s_text_of_its_floats() {
        let items = [0.5f32, -0.25, 0.123_456_79, 1e-7];
        let vector = SqlValue::Vector(items.iter().map(|item| item.to_bits()).collect());
        assert_eq!(
            sql(&vector).as_deref(),
            Some("'[0.5,-0.25,0.12345679,0.0000001]'::vector")
        );
        assert_eq!(sql(&SqlValue::Vector(vec![f32::NAN.to_bits()])), None);
        assert_eq!(
            sql(&SqlValue::Null(SqlType::Vector)).as_deref(),
            Some("NULL::vector")
        );
    }

    #[test]
    fn text_is_quoted_so_nothing_in_it_ends_the_literal() {
        assert_eq!(text_value("plain").as_deref(), Some("'plain'"));
        assert_eq!(text_value("it's").as_deref(), Some("'it''s'"));
        assert_eq!(
            text_value("'; DROP TABLE \"todo\"; --").as_deref(),
            Some("'''; DROP TABLE \"todo\"; --'")
        );
        // A backslash is doubled in an escape string, which reads the same
        // whatever `standard_conforming_strings` says, so `\'` can't end it.
        assert_eq!(text_value("a\\b").as_deref(), Some("E'a\\\\b'"));
        assert_eq!(text_value("x\\").as_deref(), Some("E'x\\\\'"));
        assert_eq!(text_value("\\'").as_deref(), Some("E'\\\\'''"));
        // Unicode is itself; an ASCII control character is escaped, so the
        // literal stays on one line, and no line of text can pass for a
        // migration's next section.
        assert_eq!(text_value("café ☕ 𝄞").as_deref(), Some("'café ☕ 𝄞'"));
        assert_eq!(
            text_value("a\r\nb\t\u{b}'").as_deref(),
            Some("E'a\\r\\nb\\t\\x0b'''")
        );
        assert_eq!(
            text_value("x\n-- store: other\nDROP TABLE \"t\"").as_deref(),
            Some("E'x\\n-- store: other\\nDROP TABLE \"t\"'")
        );
        // PostgreSQL text can't hold the NUL character.
        assert_eq!(text_value("a\0b"), None);
        assert_eq!(sql(&SqlValue::Text("a\0".into())), None);
        assert_eq!(sql(&SqlValue::Json("\"\0\"".into())), None);
    }

    #[test]
    fn every_value_is_written_as_postgresql_reads_it() {
        let cases: Vec<(SqlValue, &str)> = vec![
            (SqlValue::Null(SqlType::Int8), "NULL::bigint"),
            (SqlValue::Bool(true), "TRUE"),
            (SqlValue::Int2(-3), "-3"),
            (SqlValue::Int4(7), "7"),
            (SqlValue::Int8(i64::MIN), "-9223372036854775808"),
            (SqlValue::Numeric("-12.50".into()), "-12.50"),
            (SqlValue::Text("open".into()), "'open'"),
            (
                SqlValue::Json("{\"a\":\"it's\"}".into()),
                "'{\"a\":\"it''s\"}'::jsonb",
            ),
            (SqlValue::Bytes(vec![0, 0xab]), "E'\\\\x00ab'::bytea"),
            (
                SqlValue::Uuid([
                    0x01, 0x92, 0xf0, 0xc4, 0x3b, 0x5e, 0x7c, 0x1a, 0x9d, 0x2e, 0x4f, 0x6a, 0x8b,
                    0x0c, 0x1d, 0x2e,
                ]),
                "'0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e'::uuid",
            ),
            (
                SqlValue::Timestamp(1_792_504_800_123_456),
                "'2026-10-20 14:00:00.123456+00'::timestamptz",
            ),
            (
                SqlValue::Timestamp(-1),
                "'1969-12-31 23:59:59.999999+00'::timestamptz",
            ),
            (
                SqlValue::LocalTimestamp(0),
                "'1970-01-01 00:00:00.000000'::timestamp",
            ),
            (SqlValue::Date(20_386), "'2025-10-25'::date"),
            (SqlValue::Date(-719_163), "'0001-12-31 BC'::date"),
            (SqlValue::Time(34_200_000_000), "'09:30:00.000000'::time"),
            (
                SqlValue::Array(SqlType::Text, vec![SqlValue::Text("a'b".into())]),
                "ARRAY['a''b']::text[]",
            ),
            (SqlValue::Array(SqlType::Int8, Vec::new()), "'{}'::bigint[]"),
            (
                SqlValue::Range {
                    element: SqlType::Date,
                    lower: Some(Box::new(SqlValue::Date(0))),
                    upper: None,
                    upper_held: false,
                },
                "daterange('1970-01-01'::date, NULL::date, '[)')",
            ),
        ];
        for (value, written) in cases {
            assert_eq!(sql(&value).as_deref(), Some(written), "{value:?}");
        }
    }

    #[test]
    fn a_number_is_written_only_when_it_is_one() {
        for text in ["0", "-1", "12.50", "-0.001"] {
            assert!(sql(&SqlValue::Numeric(text.into())).is_some(), "{text}");
        }
        for text in ["", "-", "1.", ".5", "1e5", "1; DROP", "1--", "NaN"] {
            assert_eq!(sql(&SqlValue::Numeric(text.into())), None, "{text}");
        }
    }
}
