//! PostgreSQL's binary formats for the types the runtime sends and reads
//! (`SqlValue`), by type OID, so the codecs need none of tokio-postgres's
//! `with-*` features and are tested without a server.

use std::fmt::Write as _;

use jadpo_runtime::geography::{Coordinates, Shape};
use jadpo_runtime::{SqlType, SqlValue};

/// Type OIDs, as `pg_type` numbers them.
pub(crate) mod oid {
    pub const BOOL: u32 = 16;
    pub const BYTEA: u32 = 17;
    pub const NAME: u32 = 19;
    pub const INT8: u32 = 20;
    pub const INT2: u32 = 21;
    pub const INT4: u32 = 23;
    pub const TEXT: u32 = 25;
    pub const FLOAT8: u32 = 701;
    pub const JSON: u32 = 114;
    pub const BPCHAR: u32 = 1042;
    pub const VARCHAR: u32 = 1043;
    pub const DATE: u32 = 1082;
    pub const TIME: u32 = 1083;
    pub const TIMESTAMP: u32 = 1114;
    pub const TIMESTAMPTZ: u32 = 1184;
    pub const NUMERIC: u32 = 1700;
    pub const UUID: u32 = 2950;
    pub const JSONB: u32 = 3802;
    pub const BOOL_ARRAY: u32 = 1000;
    pub const BYTEA_ARRAY: u32 = 1001;
    pub const INT2_ARRAY: u32 = 1005;
    pub const INT4_ARRAY: u32 = 1007;
    pub const TEXT_ARRAY: u32 = 1009;
    pub const INT8_ARRAY: u32 = 1016;
    pub const DATE_ARRAY: u32 = 1182;
    pub const TIME_ARRAY: u32 = 1183;
    pub const TIMESTAMPTZ_ARRAY: u32 = 1185;
    pub const NUMERIC_ARRAY: u32 = 1231;
    pub const UUID_ARRAY: u32 = 2951;
    pub const JSONB_ARRAY: u32 = 3807;
    pub const NUMRANGE: u32 = 3906;
    pub const TSTZRANGE: u32 = 3910;
    pub const DATERANGE: u32 = 3912;
    pub const INT8RANGE: u32 = 3926;
    /// No type: PostgreSQL infers the parameter's from where it is used.
    /// A vector is sent so, since the extension's type has a number each
    /// database gives it (chunk 5.5v1).
    pub const UNSPECIFIED: u32 = 0;
}

/// pgvector's type, by the name its extension gives it: its number differs
/// in each database, so a column of it is known by name (area 21, chunk
/// 5.5v1).
pub(crate) const VECTOR: &str = "vector";

/// PostGIS's type, by the name its extension gives it, known by name as
/// pgvector's is (area 20, chunk 5.5u4).
pub(crate) const GEOGRAPHY: &str = "geography";

/// The most items pgvector's `vector` has.
const VECTOR_MOST: usize = 16_000;

/// A range's flags in its binary format (`rangetypes.h`).
const RANGE_EMPTY: u8 = 0x01;
const RANGE_LOWER_HELD: u8 = 0x02;
const RANGE_UPPER_HELD: u8 = 0x04;
const RANGE_LOWER_OPEN: u8 = 0x08;
const RANGE_UPPER_OPEN: u8 = 0x10;

/// PostgreSQL counts time from 2000-01-01; the runtime from 1970-01-01.
const EPOCH_MICROS: i64 = 946_684_800_000_000;
const EPOCH_DATE: i32 = 10_957;

/// JSONB's binary format: a version byte, then the text.
const JSONB_VERSION: u8 = 1;

const NUMERIC_POSITIVE: u16 = 0x0000;
const NUMERIC_NEGATIVE: u16 = 0x4000;
const NUMERIC_NAN: u16 = 0xC000;
const NUMERIC_INFINITY: u16 = 0xD000;
const NUMERIC_NEGATIVE_INFINITY: u16 = 0xF000;

/// The type a parameter of `ty` is sent as.
pub(crate) fn param_oid(ty: SqlType) -> u32 {
    match ty {
        SqlType::Bool => oid::BOOL,
        SqlType::Int2 => oid::INT2,
        SqlType::Int4 => oid::INT4,
        SqlType::Int8 => oid::INT8,
        SqlType::Numeric => oid::NUMERIC,
        SqlType::Text => oid::TEXT,
        SqlType::Bytes => oid::BYTEA,
        SqlType::Uuid => oid::UUID,
        SqlType::Timestamp => oid::TIMESTAMPTZ,
        SqlType::Date => oid::DATE,
        SqlType::Time => oid::TIME,
        SqlType::LocalTimestamp => oid::TIMESTAMP,
        SqlType::Json => oid::JSONB,
        SqlType::Vector | SqlType::Geography => oid::UNSPECIFIED,
        SqlType::Float8 => oid::FLOAT8,
        SqlType::Range(inner) => range_oid(*inner),
        SqlType::Array(inner) => match inner {
            SqlType::Bool => oid::BOOL_ARRAY,
            SqlType::Int2 => oid::INT2_ARRAY,
            SqlType::Int4 => oid::INT4_ARRAY,
            SqlType::Int8 => oid::INT8_ARRAY,
            SqlType::Numeric => oid::NUMERIC_ARRAY,
            SqlType::Text => oid::TEXT_ARRAY,
            SqlType::Bytes => oid::BYTEA_ARRAY,
            SqlType::Uuid => oid::UUID_ARRAY,
            SqlType::Timestamp => oid::TIMESTAMPTZ_ARRAY,
            SqlType::Date => oid::DATE_ARRAY,
            SqlType::Time => oid::TIME_ARRAY,
            SqlType::Json => oid::JSONB_ARRAY,
            // Arrays of arrays, of local timestamps, of ranges, of
            // vectors, of doubles and of geographies are never sent.
            SqlType::Array(_)
            | SqlType::LocalTimestamp
            | SqlType::Range(_)
            | SqlType::Vector
            | SqlType::Float8
            | SqlType::Geography => oid::TEXT_ARRAY,
        },
    }
}

/// The range type of ranges of `element` (chunk 5.5w).
fn range_oid(element: SqlType) -> u32 {
    match element {
        SqlType::Date => oid::DATERANGE,
        SqlType::Timestamp => oid::TSTZRANGE,
        SqlType::Numeric => oid::NUMRANGE,
        _ => oid::INT8RANGE,
    }
}

/// The runtime's type for a column of type `oid`, or `None` when the
/// runtime doesn't read it.
pub(crate) fn column_type(oid: u32) -> Option<SqlType> {
    Some(match oid {
        oid::BOOL => SqlType::Bool,
        oid::INT2 => SqlType::Int2,
        oid::INT4 => SqlType::Int4,
        oid::INT8 => SqlType::Int8,
        oid::NUMERIC => SqlType::Numeric,
        // A distance pgvector worked out (area 21, chunk 5.5v1b).
        oid::FLOAT8 => SqlType::Float8,
        oid::TEXT | oid::VARCHAR | oid::BPCHAR | oid::NAME => SqlType::Text,
        oid::BYTEA => SqlType::Bytes,
        oid::UUID => SqlType::Uuid,
        oid::TIMESTAMPTZ => SqlType::Timestamp,
        oid::TIMESTAMP => SqlType::LocalTimestamp,
        oid::DATE => SqlType::Date,
        // The ranges area 19 stores (chunk 5.5w).
        oid::DATERANGE => SqlType::Date.range(),
        oid::TSTZRANGE => SqlType::Timestamp.range(),
        oid::INT8RANGE => SqlType::Int8.range(),
        oid::NUMRANGE => SqlType::Numeric.range(),
        oid::TIME => SqlType::Time,
        oid::JSONB | oid::JSON => SqlType::Json,
        // An address's lines (area 19, chunk 5.5l).
        oid::TEXT_ARRAY => SqlType::Text.array(),
        _ => return None,
    })
}

/// Writes `value` in its binary format to `out`: `false` when it is
/// `NULL`, which writes nothing.
pub(crate) fn encode(value: &SqlValue, out: &mut Vec<u8>) -> Result<bool, String> {
    match value {
        SqlValue::Null(_) => return Ok(false),
        SqlValue::Bool(b) => out.push(u8::from(*b)),
        SqlValue::Int2(n) => out.extend_from_slice(&n.to_be_bytes()),
        SqlValue::Int4(n) => out.extend_from_slice(&n.to_be_bytes()),
        SqlValue::Int8(n) => out.extend_from_slice(&n.to_be_bytes()),
        SqlValue::Numeric(text) => numeric_to_wire(text, out)?,
        SqlValue::Text(text) => out.extend_from_slice(text.as_bytes()),
        SqlValue::Bytes(bytes) => out.extend_from_slice(bytes),
        SqlValue::Uuid(bytes) => out.extend_from_slice(bytes),
        SqlValue::Timestamp(micros) => {
            let wire = micros
                .checked_sub(EPOCH_MICROS)
                .ok_or("a timestamp out of PostgreSQL's range")?;
            out.extend_from_slice(&wire.to_be_bytes());
        }
        SqlValue::Date(date) => {
            let wire = date
                .checked_sub(EPOCH_DATE)
                .ok_or("a date out of PostgreSQL's range")?;
            out.extend_from_slice(&wire.to_be_bytes());
        }
        SqlValue::Time(micros) => out.extend_from_slice(&micros.to_be_bytes()),
        SqlValue::LocalTimestamp(micros) => {
            let wire = micros
                .checked_sub(EPOCH_MICROS)
                .ok_or("a timestamp out of PostgreSQL's range")?;
            out.extend_from_slice(&wire.to_be_bytes());
        }
        SqlValue::Range {
            lower,
            upper,
            upper_held,
            ..
        } => range_to_wire(lower.as_deref(), upper.as_deref(), *upper_held, out)?,
        SqlValue::Json(text) => {
            out.push(JSONB_VERSION);
            out.extend_from_slice(text.as_bytes());
        }
        SqlValue::Array(element, items) => array_to_wire(*element, items, out)?,
        SqlValue::Vector(bits) => vector_to_wire(bits, out)?,
        SqlValue::Geography(shape) => geography_to_wire(shape, out)?,
        SqlValue::Float8(bits) => out.extend_from_slice(&bits.to_be_bytes()),
    }
    Ok(true)
}

/// A pgvector `vector` in its binary format (`vector_send`): its number of
/// items and a reserved zero, each a 16-bit integer, then each item's
/// single-precision float, every number big-endian.
fn vector_to_wire(bits: &[u32], out: &mut Vec<u8>) -> Result<(), String> {
    if bits.is_empty() || bits.len() > VECTOR_MOST {
        return Err("a vector of more items than pgvector holds, or none".into());
    }
    let count = i16::try_from(bits.len()).map_err(|_| "a vector of too many items")?;
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&0i16.to_be_bytes());
    for item in bits {
        out.extend_from_slice(&item.to_be_bytes());
    }
    Ok(())
}

/// A pgvector `vector` read from its binary format (`vector_recv`'s).
pub(crate) fn vector_from_wire(raw: &[u8]) -> Result<SqlValue, String> {
    let header = |at: usize| -> Result<[u8; 2], String> {
        raw.get(at..at + 2)
            .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
            .ok_or_else(|| length("vector", raw))
    };
    let count =
        usize::try_from(i16::from_be_bytes(header(0)?)).map_err(|_| length("vector", raw))?;
    let items = raw.get(4..).unwrap_or_default();
    if header(2)? != [0, 0] || items.len() != count * 4 {
        return Err(length("vector", raw));
    }
    Ok(SqlValue::Vector(
        items
            .as_chunks::<4>()
            .0
            .iter()
            .map(|item| u32::from_be_bytes(*item))
            .collect(),
    ))
}

// --- PostGIS's `geography` as EWKB (area 20, "Interop": WKB is the
// driver's own encoding; chunk 5.5u4) -------------------------------------

/// EWKB's flag for a geometry with a third coordinate.
const EWKB_Z: u32 = 0x8000_0000;
/// EWKB's flag for a geometry with a measure, which Jadpo never stores.
const EWKB_M: u32 = 0x4000_0000;
/// EWKB's flag for a geometry that names its SRID after its type.
const EWKB_SRID: u32 = 0x2000_0000;

/// A geometry in PostGIS's extended WKB, `geography_recv`'s input:
/// little-endian, its type flagged with a third coordinate where its
/// positions have altitudes and with SRID 4326, then its parts, each
/// member of a multi-geometry a whole WKB geometry of its own.
fn geography_to_wire(shape: &Shape, out: &mut Vec<u8>) -> Result<(), String> {
    let z = shape.has_altitude().ok_or(
        "a geometry whose points mix with and without an altitude, which PostGIS can't hold",
    )?;
    shape_to_wire(shape, z, true, out);
    Ok(())
}

fn shape_to_wire(shape: &Shape, z: bool, outer: bool, out: &mut Vec<u8>) {
    out.push(1);
    let mut code = shape.code();
    if z {
        code |= EWKB_Z;
    }
    if outer {
        code |= EWKB_SRID;
    }
    out.extend_from_slice(&code.to_le_bytes());
    if outer {
        out.extend_from_slice(&jadpo_runtime::geography::SRID.to_le_bytes());
    }
    let count = |n: usize, out: &mut Vec<u8>| {
        out.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
    };
    let position = |at: &Coordinates, out: &mut Vec<u8>| {
        out.extend_from_slice(&at.longitude.to_le_bytes());
        out.extend_from_slice(&at.latitude.to_le_bytes());
        if z {
            out.extend_from_slice(&at.altitude.unwrap_or(0).to_le_bytes());
        }
    };
    let points = |points: &[Coordinates], out: &mut Vec<u8>| {
        count(points.len(), out);
        for at in points {
            position(at, out);
        }
    };
    match shape {
        Shape::Point(at) => position(at, out),
        Shape::LineString(line) => points(line, out),
        Shape::Polygon(rings) => {
            count(rings.len(), out);
            for ring in rings {
                points(ring, out);
            }
        }
        Shape::MultiPoint(members) => {
            count(members.len(), out);
            for at in members {
                shape_to_wire(&Shape::Point(*at), z, false, out);
            }
        }
        Shape::MultiLineString(members) => {
            count(members.len(), out);
            for line in members {
                shape_to_wire(&Shape::LineString(line.clone()), z, false, out);
            }
        }
        Shape::MultiPolygon(members) => {
            count(members.len(), out);
            for rings in members {
                shape_to_wire(&Shape::Polygon(rings.clone()), z, false, out);
            }
        }
    }
}

/// Reads EWKB, `geography_send`'s output.
struct Ewkb<'r> {
    raw: &'r [u8],
    at: usize,
}

impl Ewkb<'_> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let bytes = self
            .raw
            .get(self.at..self.at + N)
            .and_then(|bytes| <[u8; N]>::try_from(bytes).ok())
            .ok_or_else(|| length("geography", self.raw))?;
        self.at += N;
        Ok(bytes)
    }

    fn u32(&mut self, little: bool) -> Result<u32, String> {
        let bytes = self.bytes::<4>()?;
        Ok(if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    fn f64_bits(&mut self, little: bool) -> Result<u64, String> {
        let bytes = self.bytes::<8>()?;
        let bits = if little {
            u64::from_le_bytes(bytes)
        } else {
            u64::from_be_bytes(bytes)
        };
        if !f64::from_bits(bits).is_finite() {
            return Err("the server sent a geography with a coordinate that isn't a number".into());
        }
        Ok(bits)
    }

    /// A count of what follows, each taking at least `least` bytes, so a
    /// count past what was sent is refused before anything is reserved.
    fn count(&mut self, little: bool, least: usize) -> Result<usize, String> {
        let n = usize::try_from(self.u32(little)?).map_err(|_| length("geography", self.raw))?;
        if n.saturating_mul(least) > self.raw.len().saturating_sub(self.at) {
            return Err(length("geography", self.raw));
        }
        Ok(n)
    }

    /// One geometry: its byte order, its type with its flags, its SRID
    /// where it names one (4326, the only one a `geography` column of
    /// Jadpo's holds), then its parts; `want` the kind a member of a
    /// multi-geometry must be.
    fn shape(&mut self, want: Option<u32>) -> Result<Shape, String> {
        let little = match self.bytes::<1>()? {
            [1] => true,
            [0] => false,
            _ => return Err("the server sent a geography in no byte order".into()),
        };
        let flagged = self.u32(little)?;
        if flagged & EWKB_M != 0 {
            return Err("the server sent a geography with a measure".into());
        }
        if flagged & EWKB_SRID != 0 {
            let srid = self.u32(little)?;
            if srid != jadpo_runtime::geography::SRID {
                return Err(format!(
                    "the server sent a geography in SRID {srid}, not 4326"
                ));
            }
        }
        let z = flagged & EWKB_Z != 0;
        let code = flagged & !(EWKB_Z | EWKB_M | EWKB_SRID);
        if want.is_some_and(|want| want != code) {
            return Err("the server sent a multi-geometry with a member of another kind".into());
        }
        let width = if z { 24 } else { 16 };
        let position = |this: &mut Self| -> Result<Coordinates, String> {
            let longitude = this.f64_bits(little)?;
            let latitude = this.f64_bits(little)?;
            let altitude = if z {
                Some(this.f64_bits(little)?)
            } else {
                None
            };
            Ok(Coordinates {
                longitude,
                latitude,
                altitude,
            })
        };
        let points = |this: &mut Self| -> Result<Vec<Coordinates>, String> {
            let n = this.count(little, width)?;
            (0..n).map(|_| position(this)).collect()
        };
        Ok(match code {
            1 => Shape::Point(position(self)?),
            2 => Shape::LineString(points(self)?),
            3 => {
                let n = self.count(little, 4)?;
                Shape::Polygon((0..n).map(|_| points(self)).collect::<Result<_, _>>()?)
            }
            4..=6 => {
                let n = self.count(little, 9)?;
                let members = (0..n)
                    .map(|_| self.shape(Some(code - 3)))
                    .collect::<Result<Vec<Shape>, String>>()?;
                // Each member was read as `code - 3`, so each is that kind.
                match code {
                    4 => Shape::MultiPoint(
                        members
                            .into_iter()
                            .filter_map(|member| match member {
                                Shape::Point(at) => Some(at),
                                _ => None,
                            })
                            .collect(),
                    ),
                    5 => Shape::MultiLineString(
                        members
                            .into_iter()
                            .filter_map(|member| match member {
                                Shape::LineString(line) => Some(line),
                                _ => None,
                            })
                            .collect(),
                    ),
                    _ => Shape::MultiPolygon(
                        members
                            .into_iter()
                            .filter_map(|member| match member {
                                Shape::Polygon(rings) => Some(rings),
                                _ => None,
                            })
                            .collect(),
                    ),
                }
            }
            other => return Err(format!("the server sent a geography of WKB type {other}")),
        })
    }
}

/// A PostGIS `geography` read from its binary format, EWKB.
pub(crate) fn geography_from_wire(raw: &[u8]) -> Result<SqlValue, String> {
    let mut reader = Ewkb { raw, at: 0 };
    let shape = reader.shape(None)?;
    if reader.at != raw.len() {
        return Err(length("geography", raw));
    }
    Ok(SqlValue::Geography(shape))
}

/// A range in its binary format: its flags, then each end that isn't open,
/// its length and bytes. Its lower end is always held; the runtime never
/// sends one that holds nothing (`jadpo_runtime::to_sql`).
fn range_to_wire(
    lower: Option<&SqlValue>,
    upper: Option<&SqlValue>,
    upper_held: bool,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let mut flags = 0u8;
    match lower {
        Some(_) => flags |= RANGE_LOWER_HELD,
        None => flags |= RANGE_LOWER_OPEN,
    }
    match upper {
        Some(_) if upper_held => flags |= RANGE_UPPER_HELD,
        Some(_) => {}
        None => flags |= RANGE_UPPER_OPEN,
    }
    out.push(flags);
    for end in [lower, upper].into_iter().flatten() {
        let at = out.len();
        out.extend_from_slice(&0i32.to_be_bytes());
        if !encode(end, out)? {
            return Err("a range's end was `NULL`".into());
        }
        let length = i32::try_from(out.len() - at - 4)
            .map_err(|_| "a range's end too long to send".to_string())?;
        if let Some(prefix) = out.get_mut(at..at + 4) {
            prefix.copy_from_slice(&length.to_be_bytes());
        }
    }
    Ok(())
}

/// A range of the type `element` names, from its binary format; one that
/// is `empty` is refused, as the runtime writes none.
fn range_from_wire(element: u32, raw: &[u8]) -> Result<SqlValue, String> {
    let (&flags, mut rest) = raw.split_first().ok_or_else(|| length("range", raw))?;
    if flags & RANGE_EMPTY != 0 {
        return Err(
            "the runtime holds no empty range: PostgreSQL keeps it without its ends".into(),
        );
    }
    let mut end = |open: bool| -> Result<Option<Box<SqlValue>>, String> {
        if open {
            return Ok(None);
        }
        let (size, after) = rest
            .split_at_checked(4)
            .ok_or_else(|| length("range", raw))?;
        let size = usize::try_from(i32::from_be_bytes(fixed(size, "range")?))
            .map_err(|_| length("range", raw))?;
        let (bytes, after) = after
            .split_at_checked(size)
            .ok_or_else(|| length("range", raw))?;
        rest = after;
        Ok(Some(Box::new(decode(element, bytes)?)))
    };
    let lower = end(flags & RANGE_LOWER_OPEN != 0)?;
    let upper = end(flags & RANGE_UPPER_OPEN != 0)?;
    if flags & RANGE_LOWER_HELD == 0 && lower.is_some() {
        return Err("the runtime reads ranges that hold their lower end".into());
    }
    let element = column_type(element).ok_or_else(|| format!("a range of type OID {element}"))?;
    Ok(SqlValue::Range {
        element,
        lower,
        upper,
        upper_held: flags & RANGE_UPPER_HELD != 0,
    })
}

/// A one-dimensional array without `NULL`s in its binary format: the
/// number of dimensions, a flag for `NULL`s, the element type, the length
/// and lower bound, then each element's length and bytes.
fn array_to_wire(element: SqlType, items: &[SqlValue], out: &mut Vec<u8>) -> Result<(), String> {
    let count = i32::try_from(items.len()).map_err(|_| "an array too long to send".to_string())?;
    let dimensions: i32 = if items.is_empty() { 0 } else { 1 };
    out.extend_from_slice(&dimensions.to_be_bytes());
    out.extend_from_slice(&0i32.to_be_bytes());
    out.extend_from_slice(&param_oid(element).to_be_bytes());
    if dimensions == 1 {
        out.extend_from_slice(&count.to_be_bytes());
        out.extend_from_slice(&1i32.to_be_bytes());
    }
    for item in items {
        let at = out.len();
        out.extend_from_slice(&0i32.to_be_bytes());
        if !encode(item, out)? {
            return Err("an array element was `NULL`".into());
        }
        let length = i32::try_from(out.len() - at - 4)
            .map_err(|_| "an array element too long to send".to_string())?;
        if let Some(prefix) = out.get_mut(at..at + 4) {
            prefix.copy_from_slice(&length.to_be_bytes());
        }
    }
    Ok(())
}

/// Reads a non-null column of type `oid` from its binary format.
pub(crate) fn decode(oid: u32, raw: &[u8]) -> Result<SqlValue, String> {
    Ok(match oid {
        oid::BOOL => match raw {
            [b] => SqlValue::Bool(*b != 0),
            _ => return Err(length("bool", raw)),
        },
        oid::INT2 => SqlValue::Int2(i16::from_be_bytes(fixed(raw, "int2")?)),
        oid::INT4 => SqlValue::Int4(i32::from_be_bytes(fixed(raw, "int4")?)),
        oid::INT8 => SqlValue::Int8(i64::from_be_bytes(fixed(raw, "int8")?)),
        oid::NUMERIC => SqlValue::Numeric(numeric_from_wire(raw)?),
        oid::FLOAT8 => SqlValue::Float8(u64::from_be_bytes(fixed(raw, "float8")?)),
        oid::TEXT | oid::VARCHAR | oid::BPCHAR | oid::NAME | oid::JSON => {
            let text = String::from_utf8(raw.to_vec())
                .map_err(|_| "the server sent text that isn't UTF-8".to_string())?;
            if oid == oid::JSON {
                SqlValue::Json(text)
            } else {
                SqlValue::Text(text)
            }
        }
        oid::BYTEA => SqlValue::Bytes(raw.to_vec()),
        oid::UUID => SqlValue::Uuid(fixed(raw, "uuid")?),
        oid::TIMESTAMPTZ | oid::TIMESTAMP => {
            let wire = i64::from_be_bytes(fixed(raw, "timestamp")?);
            if wire == i64::MAX || wire == i64::MIN {
                return Err("the runtime holds no infinite timestamp".into());
            }
            let micros = wire
                .checked_add(EPOCH_MICROS)
                .ok_or_else(|| "a timestamp is out of the runtime's range".to_string())?;
            if oid == oid::TIMESTAMP {
                SqlValue::LocalTimestamp(micros)
            } else {
                SqlValue::Timestamp(micros)
            }
        }
        oid::DATERANGE => range_from_wire(oid::DATE, raw)?,
        oid::TSTZRANGE => range_from_wire(oid::TIMESTAMPTZ, raw)?,
        oid::INT8RANGE => range_from_wire(oid::INT8, raw)?,
        oid::NUMRANGE => range_from_wire(oid::NUMERIC, raw)?,
        oid::DATE => {
            let wire = i32::from_be_bytes(fixed(raw, "date")?);
            if wire == i32::MAX || wire == i32::MIN {
                return Err("the runtime holds no infinite date".into());
            }
            SqlValue::Date(
                wire.checked_add(EPOCH_DATE)
                    .ok_or_else(|| "a date is out of the runtime's range".to_string())?,
            )
        }
        oid::TIME => SqlValue::Time(i64::from_be_bytes(fixed(raw, "time")?)),
        oid::JSONB => match raw.split_first() {
            Some((&JSONB_VERSION, text)) => SqlValue::Json(
                String::from_utf8(text.to_vec())
                    .map_err(|_| "the server sent JSON that isn't UTF-8".to_string())?,
            ),
            _ => return Err("the server sent jsonb in a format the runtime doesn't read".into()),
        },
        oid::TEXT_ARRAY => SqlValue::Array(SqlType::Text, text_array_from_wire(raw)?),
        _ => return Err(format!("the runtime doesn't read type OID {oid}")),
    })
}

/// A one-dimensional `text[]` from its binary format, as
/// [`array_to_wire`] writes one; an element that is `NULL` is refused, as
/// the runtime writes none.
fn text_array_from_wire(raw: &[u8]) -> Result<Vec<SqlValue>, String> {
    let mut at = 0usize;
    let next = |at: &mut usize| -> Result<i32, String> {
        let bytes = raw.get(*at..*at + 4).ok_or_else(|| length("text[]", raw))?;
        *at += 4;
        Ok(i32::from_be_bytes(fixed(bytes, "text[]")?))
    };
    let dimensions = next(&mut at)?;
    let _nulls = next(&mut at)?;
    let _element = next(&mut at)?;
    let count = match dimensions {
        0 => 0,
        1 => {
            let count = next(&mut at)?;
            let _lower = next(&mut at)?;
            usize::try_from(count).map_err(|_| length("text[]", raw))?
        }
        _ => return Err("the runtime reads arrays of one dimension".into()),
    };
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let size = usize::try_from(next(&mut at)?)
            .map_err(|_| "the runtime holds no NULL in a list".to_string())?;
        let bytes = raw
            .get(at..at + size)
            .ok_or_else(|| length("text[]", raw))?;
        at += size;
        items.push(SqlValue::Text(String::from_utf8(bytes.to_vec()).map_err(
            |_| "the server sent text that isn't UTF-8".to_string(),
        )?));
    }
    Ok(items)
}

fn fixed<const N: usize>(raw: &[u8], name: &str) -> Result<[u8; N], String> {
    <[u8; N]>::try_from(raw).map_err(|_| length(name, raw))
}

fn length(name: &str, raw: &[u8]) -> String {
    format!("the server sent {} bytes for a {name}", raw.len())
}

/// A decimal's canonical text (`-12.50`) in NUMERIC's binary format:
/// base-10000 digits, the first at a power of 10000, with the scale
/// (places after the point) kept.
fn numeric_to_wire(text: &str, out: &mut Vec<u8>) -> Result<(), String> {
    // Never the text, which may be personal (chunk 5.5w2).
    let invalid = || "a decimal that isn't in canonical form".to_string();
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (int, frac) = match body.split_once('.') {
        Some((_, "")) => return Err(invalid()),
        Some((int, frac)) => (int, frac),
        None => (body, ""),
    };
    if int.is_empty()
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let scale = u16::try_from(frac.len()).map_err(|_| invalid())?;
    let int = int.trim_start_matches('0');
    let mut digits = String::with_capacity(int.len() + frac.len() + 6);
    digits.extend(std::iter::repeat_n('0', (4 - int.len() % 4) % 4));
    digits.push_str(int);
    let int_groups = digits.len() / 4;
    digits.push_str(frac);
    digits.extend(std::iter::repeat_n('0', (4 - frac.len() % 4) % 4));
    let mut groups: Vec<i16> = Vec::with_capacity(digits.len() / 4);
    for chunk in digits.as_bytes().chunks(4) {
        let group = chunk
            .iter()
            .fold(0i16, |acc, b| acc * 10 + i16::from(b - b'0'));
        groups.push(group);
    }
    let mut power = i64::try_from(int_groups).map_err(|_| invalid())? - 1;
    let leading = groups.iter().take_while(|g| **g == 0).count();
    groups.drain(..leading);
    power -= i64::try_from(leading).map_err(|_| invalid())?;
    while groups.last() == Some(&0) {
        groups.pop();
    }
    if groups.is_empty() {
        power = 0;
    }
    let count = i16::try_from(groups.len()).map_err(|_| invalid())?;
    let power = i16::try_from(power).map_err(|_| invalid())?;
    let sign = if negative && !groups.is_empty() {
        NUMERIC_NEGATIVE
    } else {
        NUMERIC_POSITIVE
    };
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&power.to_be_bytes());
    out.extend_from_slice(&sign.to_be_bytes());
    out.extend_from_slice(&scale.to_be_bytes());
    for group in groups {
        out.extend_from_slice(&group.to_be_bytes());
    }
    Ok(())
}

/// NUMERIC's binary format as canonical decimal text, with as many places
/// as its scale.
fn numeric_from_wire(raw: &[u8]) -> Result<String, String> {
    let malformed = || "the server sent a numeric the runtime can't read".to_string();
    let header = |at: usize| -> Result<[u8; 2], String> {
        raw.get(at..at + 2)
            .and_then(|b| <[u8; 2]>::try_from(b).ok())
            .ok_or_else(malformed)
    };
    let count = usize::from(u16::from_be_bytes(header(0)?));
    let power = i64::from(i16::from_be_bytes(header(2)?));
    let sign = u16::from_be_bytes(header(4)?);
    let scale = usize::from(u16::from_be_bytes(header(6)?));
    match sign {
        NUMERIC_POSITIVE | NUMERIC_NEGATIVE => {}
        NUMERIC_NAN => return Err("the runtime holds no NaN numeric".into()),
        NUMERIC_INFINITY | NUMERIC_NEGATIVE_INFINITY => {
            return Err("the runtime holds no infinite numeric".into());
        }
        _ => return Err(malformed()),
    }
    if raw.len() != 8 + 2 * count {
        return Err(malformed());
    }
    let mut digits = Vec::with_capacity(count);
    for at in 0..count {
        let group = i16::from_be_bytes(header(8 + 2 * at)?);
        if !(0..=9999).contains(&group) {
            return Err(malformed());
        }
        digits.push(group);
    }
    let digit = |index: i64| -> i16 {
        usize::try_from(index)
            .ok()
            .and_then(|i| digits.get(i).copied())
            .unwrap_or(0)
    };
    let mut text = String::new();
    if sign == NUMERIC_NEGATIVE && digits.iter().any(|d| *d != 0) {
        text.push('-');
    }
    let mut int = String::new();
    for index in 0..=power {
        let _ = write!(int, "{:04}", digit(index));
    }
    let int = int.trim_start_matches('0');
    text.push_str(if int.is_empty() { "0" } else { int });
    if scale > 0 {
        let mut frac = String::with_capacity(scale + 4);
        let mut index = power + 1;
        while frac.len() < scale {
            let _ = write!(frac, "{:04}", digit(index));
            index += 1;
        }
        frac.truncate(scale);
        text.push('.');
        text.push_str(&frac);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn a_geography_is_postgis_s_extended_wkb() {
        // `geography_send` of `SRID=4326;POINT(-0.1276 51.5072)` and of
        // `SRID=4326;POINT Z(-0.1276 51.5072 8848.86)`, from PostGIS 3.4.2.
        let flat = Shape::Point(Coordinates::new(-0.1276, 51.5072, None));
        let high = Shape::Point(Coordinates::new(-0.1276, 51.5072, Some(8848.86)));
        for (shape, sent) in [
            (&flat, "0101000020e6100000da1b7c613255c0bffe43faedebc04940"),
            (
                &high,
                "01010000a0e6100000da1b7c613255c0bffe43faedebc0494048e17a146e48c140",
            ),
        ] {
            let mut out = Vec::new();
            assert_eq!(
                encode(&SqlValue::Geography(shape.clone()), &mut out),
                Ok(true)
            );
            assert_eq!(hex(&out), sent);
            assert_eq!(
                geography_from_wire(&unhex(sent)),
                Ok(SqlValue::Geography(shape.clone()))
            );
        }
        // Every kind, with and without altitude, round trips; big-endian
        // and SRID-less members read.
        let at = |lon: f64, lat: f64, z: Option<f64>| Coordinates::new(lon, lat, z);
        for z in [None, Some(12.5)] {
            let ring = vec![
                at(0.0, 0.0, z),
                at(1.0, 0.0, z),
                at(1.0, 1.0, z),
                at(0.0, 0.0, z),
            ];
            for shape in [
                Shape::LineString(vec![at(0.0, 0.0, z), at(1.0, 1.0, z)]),
                Shape::Polygon(vec![ring.clone(), ring.clone()]),
                Shape::MultiPoint(vec![at(0.0, 0.0, z), at(-180.0, -90.0, z)]),
                Shape::MultiLineString(vec![vec![at(0.0, 0.0, z), at(1.0, 1.0, z)]]),
                Shape::MultiPolygon(vec![vec![ring.clone()], vec![ring.clone()]]),
            ] {
                let mut out = Vec::new();
                encode(&SqlValue::Geography(shape.clone()), &mut out).expect("encoded");
                assert_eq!(geography_from_wire(&out), Ok(SqlValue::Geography(shape)));
            }
        }
        let big = "00000000013ff00000000000004000000000000000";
        assert_eq!(
            geography_from_wire(&unhex(big)),
            Ok(SqlValue::Geography(Shape::Point(at(1.0, 2.0, None))))
        );
        // Mixed altitudes, another SRID, a measure, a short or long
        // message, a count past what was sent.
        let mixed = Shape::LineString(vec![at(0.0, 0.0, None), at(1.0, 1.0, Some(2.0))]);
        assert!(encode(&SqlValue::Geography(mixed), &mut Vec::new()).is_err());
        assert!(
            geography_from_wire(&unhex("0101000020a2100000000000000000f03f0000000000000040"))
                .is_err()
        );
        assert!(
            geography_from_wire(&unhex(
                "0101000060e6100000000000000000f03f00000000000000400000000000000000"
            ))
            .is_err()
        );
        assert!(geography_from_wire(&unhex("0101000020e6100000000000000000f03f")).is_err());
        assert!(
            geography_from_wire(&unhex(
                "0101000020e6100000000000000000f03f000000000000004000"
            ))
            .is_err()
        );
        assert!(geography_from_wire(&unhex("0102000020e6100000ffffffff")).is_err());
        assert_eq!(param_oid(SqlType::Geography), oid::UNSPECIFIED);
    }

    #[test]
    fn a_vector_is_its_count_a_reserved_zero_and_its_floats() {
        let vector = SqlValue::Vector(vec![
            0.5f32.to_bits(),
            (-0.25f32).to_bits(),
            1.0f32.to_bits(),
        ]);
        let mut out = Vec::new();
        assert_eq!(encode(&vector, &mut out), Ok(true));
        assert_eq!(
            out,
            [
                0, 3, 0, 0, 0x3F, 0, 0, 0, 0xBE, 0x80, 0, 0, 0x3F, 0x80, 0, 0
            ]
        );
        assert_eq!(vector_from_wire(&out), Ok(vector));
        assert!(vector_from_wire(&[0, 2, 0, 0, 0x3F, 0, 0, 0]).is_err());
        assert!(encode(&SqlValue::Vector(Vec::new()), &mut Vec::new()).is_err());
        assert_eq!(param_oid(SqlType::Vector), oid::UNSPECIFIED);
    }

    fn numeric_round_trip(text: &str) -> String {
        let mut out = Vec::new();
        numeric_to_wire(text, &mut out).unwrap();
        numeric_from_wire(&out).unwrap()
    }

    /// A value the wire can't carry is refused by what it is and the
    /// rule, never the value itself, which may be personal or secret and
    /// would reach a log through the fault (chunk 5.5w2).
    #[test]
    fn a_refused_value_never_appears_in_the_error() {
        let secret = "4111111111111111.99s3cret";
        let refused =
            encode(&SqlValue::Numeric(secret.into()), &mut Vec::new()).expect_err("not a decimal");
        assert!(
            !refused.contains(secret) && !refused.contains("4111"),
            "{refused}"
        );
        for value in [
            SqlValue::Timestamp(i64::MIN + 7),
            SqlValue::LocalTimestamp(i64::MIN + 7),
            SqlValue::Date(i32::MIN + 7),
        ] {
            let refused = encode(&value, &mut Vec::new()).expect_err("out of range");
            assert!(
                !refused.contains("775807") && !refused.contains("483641"),
                "{refused}"
            );
        }
    }

    #[test]
    fn decimals_keep_their_value_and_scale_through_numeric() {
        for text in [
            "0",
            "0.00",
            "-12.50",
            "12.5",
            "1",
            "10000",
            "9999",
            "123456789",
            "0.0001",
            "0.00001",
            "-0.5",
            "100000000.00000001",
            "12345678901234567890123456789012345678",
            "1234567890123456789012.3456789012345678",
            "-0.00000000000000000000000000000000000001",
        ] {
            assert_eq!(numeric_round_trip(text), text);
        }
        // Not canonical in, canonical out.
        assert_eq!(numeric_round_trip("007.10"), "7.10");
        assert_eq!(numeric_round_trip("-0.00"), "0.00");
    }

    #[test]
    fn numeric_matches_postgresqls_own_bytes() {
        // SELECT numeric_send('-12.50'): 2 digits, power 0, negative, scale 2.
        let mut out = Vec::new();
        numeric_to_wire("-12.50", &mut out).unwrap();
        assert_eq!(out, [0, 2, 0, 0, 0x40, 0, 0, 2, 0, 12, 0x13, 0x88]);
        // SELECT numeric_send('0.00'): no digits.
        let mut out = Vec::new();
        numeric_to_wire("0.00", &mut out).unwrap();
        assert_eq!(out, [0, 0, 0, 0, 0, 0, 0, 2]);
    }

    #[test]
    fn text_that_isnt_a_canonical_decimal_is_refused() {
        for text in ["", "-", ".5", "1.", "1e5", "+1", "1,5", "NaN", " 1"] {
            assert!(numeric_to_wire(text, &mut Vec::new()).is_err(), "{text:?}");
        }
        assert!(numeric_from_wire(&[0, 0, 0, 0, 0xC0, 0, 0, 0]).is_err());
        assert!(numeric_from_wire(&[0, 1, 0, 0]).is_err());
    }

    #[test]
    fn every_value_round_trips_through_its_binary_format() {
        let values = [
            SqlValue::Bool(true),
            SqlValue::Bool(false),
            SqlValue::Int2(-32768),
            SqlValue::Int4(i32::MAX),
            SqlValue::Int8(i64::MIN),
            SqlValue::Numeric("-12.50".into()),
            SqlValue::Text("naïve ☃".into()),
            SqlValue::Bytes(vec![0, 1, 255]),
            SqlValue::Uuid([7; 16]),
            SqlValue::Timestamp(1_760_000_000_123_456),
            SqlValue::Timestamp(-1),
            SqlValue::Date(-1),
            SqlValue::Date(20_000),
            SqlValue::Json("{\"a\":[1,2]}".into()),
            SqlValue::Time(45_296_789_000),
            SqlValue::LocalTimestamp(1_760_000_000_000_000),
            // A range of each kind area 19 stores (chunk 5.5w), with an
            // open end, and one that holds its upper end.
            SqlValue::Range {
                element: SqlType::Date,
                lower: Some(Box::new(SqlValue::Date(20_000))),
                upper: Some(Box::new(SqlValue::Date(20_030))),
                upper_held: false,
            },
            SqlValue::Range {
                element: SqlType::Timestamp,
                lower: Some(Box::new(SqlValue::Timestamp(1_760_000_000_000_000))),
                upper: None,
                upper_held: false,
            },
            SqlValue::Range {
                element: SqlType::Int8,
                lower: None,
                upper: Some(Box::new(SqlValue::Int8(12))),
                upper_held: false,
            },
            SqlValue::Range {
                element: SqlType::Numeric,
                lower: Some(Box::new(SqlValue::Numeric("1.5".into()))),
                upper: Some(Box::new(SqlValue::Numeric("2.25".into()))),
                upper_held: true,
            },
        ];
        for value in values {
            let mut out = Vec::new();
            assert!(encode(&value, &mut out).unwrap());
            assert_eq!(decode(param_oid(value.ty()), &out).unwrap(), value);
        }
        assert!(!encode(&SqlValue::Null(SqlType::Text), &mut Vec::new()).unwrap());
    }

    #[test]
    fn arrays_are_one_dimension_without_nulls() {
        let mut out = Vec::new();
        let array = SqlValue::Array(SqlType::Int8, vec![SqlValue::Int8(7)]);
        assert!(encode(&array, &mut out).unwrap());
        // ndim 1, no NULLs, int8, length 1 from 1, then 8 bytes of 7.
        let mut expected = vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 1, 0, 0, 0, 1];
        expected.extend_from_slice(&[0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 7]);
        assert_eq!(out, expected);
        let mut out = Vec::new();
        encode(&SqlValue::Array(SqlType::Uuid, Vec::new()), &mut out).unwrap();
        assert_eq!(out, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 134]);
        assert_eq!(param_oid(array.ty()), oid::INT8_ARRAY);
    }

    #[test]
    fn a_text_array_reads_back_as_it_is_written() {
        let lines = vec![
            SqlValue::Text("10 Downing Street".into()),
            SqlValue::Text("Westminster".into()),
        ];
        let mut out = Vec::new();
        encode(&SqlValue::Array(SqlType::Text, lines.clone()), &mut out).unwrap();
        assert_eq!(
            decode(oid::TEXT_ARRAY, &out),
            Ok(SqlValue::Array(SqlType::Text, lines))
        );
        let mut empty = Vec::new();
        encode(&SqlValue::Array(SqlType::Text, Vec::new()), &mut empty).unwrap();
        assert_eq!(
            decode(oid::TEXT_ARRAY, &empty),
            Ok(SqlValue::Array(SqlType::Text, Vec::new()))
        );
        assert_eq!(column_type(oid::TEXT_ARRAY), Some(SqlType::Text.array()));
    }

    #[test]
    fn time_counts_from_postgresqls_epoch_on_the_wire() {
        let mut out = Vec::new();
        encode(&SqlValue::Timestamp(EPOCH_MICROS), &mut out).unwrap();
        assert_eq!(out, [0; 8]);
        let mut out = Vec::new();
        encode(&SqlValue::Date(EPOCH_DATE), &mut out).unwrap();
        assert_eq!(out, [0; 4]);
        assert!(decode(oid::TIMESTAMPTZ, &i64::MAX.to_be_bytes()).is_err());
        assert!(decode(oid::DATE, &i32::MIN.to_be_bytes()).is_err());
        assert!(encode(&SqlValue::Timestamp(i64::MIN), &mut Vec::new()).is_err());
    }

    #[test]
    fn related_column_types_read_as_one_runtime_type() {
        assert_eq!(
            decode(oid::VARCHAR, b"hi").unwrap(),
            SqlValue::Text("hi".into())
        );
        assert_eq!(
            decode(oid::JSON, b"[1]").unwrap(),
            SqlValue::Json("[1]".into())
        );
        assert_eq!(column_type(oid::TIMESTAMPTZ), Some(SqlType::Timestamp));
        assert_eq!(column_type(oid::TIMESTAMP), Some(SqlType::LocalTimestamp));
        assert_eq!(column_type(oid::DATERANGE), Some(SqlType::Date.range()));
        assert_eq!(column_type(oid::NAME), Some(SqlType::Text));
        assert_eq!(column_type(600), None);
    }
}
