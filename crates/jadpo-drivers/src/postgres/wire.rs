//! PostgreSQL's binary formats for the types the runtime sends and reads
//! (`SqlValue`), by type OID, so the codecs need none of tokio-postgres's
//! `with-*` features and are tested without a server.

use std::fmt::Write as _;

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
}

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
        SqlType::Json => oid::JSONB,
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
            // Arrays of arrays are never sent.
            SqlType::Array(_) => oid::TEXT_ARRAY,
        },
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
        oid::TEXT | oid::VARCHAR | oid::BPCHAR | oid::NAME => SqlType::Text,
        oid::BYTEA => SqlType::Bytes,
        oid::UUID => SqlType::Uuid,
        oid::TIMESTAMPTZ | oid::TIMESTAMP => SqlType::Timestamp,
        oid::DATE => SqlType::Date,
        oid::TIME => SqlType::Time,
        oid::JSONB | oid::JSON => SqlType::Json,
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
                .ok_or_else(|| format!("the timestamp {micros} is out of PostgreSQL's range"))?;
            out.extend_from_slice(&wire.to_be_bytes());
        }
        SqlValue::Date(date) => {
            let wire = date
                .checked_sub(EPOCH_DATE)
                .ok_or_else(|| format!("the date {date} is out of PostgreSQL's range"))?;
            out.extend_from_slice(&wire.to_be_bytes());
        }
        SqlValue::Time(micros) => out.extend_from_slice(&micros.to_be_bytes()),
        SqlValue::Json(text) => {
            out.push(JSONB_VERSION);
            out.extend_from_slice(text.as_bytes());
        }
        SqlValue::Array(element, items) => array_to_wire(*element, items, out)?,
    }
    Ok(true)
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
        out[at..at + 4].copy_from_slice(&length.to_be_bytes());
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
            SqlValue::Timestamp(
                wire.checked_add(EPOCH_MICROS)
                    .ok_or_else(|| "a timestamp is out of the runtime's range".to_string())?,
            )
        }
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
        _ => return Err(format!("the runtime doesn't read type OID {oid}")),
    })
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
    let invalid = || format!("`{text}` isn't a decimal in canonical form");
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

    fn numeric_round_trip(text: &str) -> String {
        let mut out = Vec::new();
        numeric_to_wire(text, &mut out).unwrap();
        numeric_from_wire(&out).unwrap()
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
        assert_eq!(column_type(oid::TIMESTAMP), Some(SqlType::Timestamp));
        assert_eq!(column_type(oid::NAME), Some(SqlType::Text));
        assert_eq!(column_type(600), None);
    }
}
