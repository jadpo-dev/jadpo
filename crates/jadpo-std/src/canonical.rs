//! The canonical byte encoding (chunk 8.1a): one way to write a value of
//! plain data as bytes, so the same value always gives the same bytes and
//! the bytes give back an equal value. The runtime's plan is written in it
//! ([`crate::RuntimePlan::encode`]), which compiled code holds as a
//! constant and decodes once as it starts (docs/compiler-runtime.md §2.5).
//!
//! It is a format on serde's data model, so a type takes part by deriving
//! `Serialize` and `Deserialize`, and it says nothing about its own shape,
//! as a plan is only ever read by the build that wrote it:
//!
//! - an unsigned whole number is LEB128, seven bits a byte, lowest first,
//!   in as few bytes as it needs; a signed one is zigzagged first, so a
//!   small negative number stays short; `true` and `false` are 1 and 0;
//! - text and bytes are their length, then their bytes; a character is its
//!   code point;
//! - a list is its length, then its items; a tuple and a struct are their
//!   fields in declared order, with no names and no count;
//! - an option is 0, or 1 and its value; an enum's variant is its place
//!   among the enum's variants, then what the variant holds;
//! - a map is its length, then its entries ordered by their keys' bytes,
//!   so a map gives the same bytes whatever order it holds its entries in,
//!   and a reader refuses keys out of that order or written twice.
//!
//! A floating-point number has no canonical form here, and is refused, as
//! the canonical JSON refuses one (§2.3). So is a length serde doesn't know
//! before it writes, and a field a type leaves out. Reading refuses a
//! number written longer than it needs, bytes left over after the value,
//! and nesting deeper than [`DEPTH`], so every value has exactly one
//! encoding and reading one never runs out of stack.

use std::fmt;

use serde::de::{self, DeserializeSeed, IntoDeserializer, Visitor};
use serde::ser::{self, Serialize};

/// How deep values may nest, containers within containers, before writing
/// or reading one is refused: far past any plan (a body nests at most 96
/// levels, area 1), and well within a thread's stack.
pub const DEPTH: u32 = 256;

/// Why a value couldn't be written or read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A floating-point number, which has no canonical form here.
    Float,
    /// A list or map whose length wasn't known before it was written.
    UnknownLength,
    /// A field the type leaves out of some values, which a reader couldn't
    /// tell was missing.
    Skipped,
    /// Values nested deeper than [`DEPTH`].
    TooDeep,
    /// The bytes end inside a value.
    Truncated,
    /// Bytes are left over after the value.
    Trailing,
    /// A number written in more bytes than it needs.
    Overlong,
    /// A number past what its type holds.
    OutOfRange,
    /// A map's keys out of order, or one written twice.
    Unordered,
    /// A byte that isn't what the value's type allows there: a flag other
    /// than 0 or 1, an option's tag, a code point, text that isn't UTF-8.
    Invalid(&'static str),
    /// A reader asked for a value by what the bytes say it is, which
    /// they never say.
    NotSelfDescribing,
    /// The value's own refusal, as its type gives it.
    Custom(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Float => f.write_str("a floating-point number has no canonical form"),
            Error::UnknownLength => f.write_str("a list or map's length wasn't known first"),
            Error::Skipped => f.write_str("a field was left out"),
            Error::TooDeep => write!(f, "values nest deeper than {DEPTH} levels"),
            Error::Truncated => f.write_str("the bytes end inside a value"),
            Error::Trailing => f.write_str("bytes are left over after the value"),
            Error::Overlong => f.write_str("a number is written longer than it needs"),
            Error::OutOfRange => f.write_str("a number is past what its type holds"),
            Error::Unordered => f.write_str("a map's keys are out of order or repeated"),
            Error::Invalid(what) => write!(f, "{what} isn't valid here"),
            Error::NotSelfDescribing => f.write_str("the bytes don't say what they hold"),
            Error::Custom(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Error {
        Error::Custom(message.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Error {
        Error::Custom(message.to_string())
    }
}

/// `value`'s canonical bytes.
pub fn to_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Error> {
    let mut writer = Writer::default();
    value.serialize(&mut writer)?;
    Ok(writer.out)
}

/// The value `bytes` hold, all of them.
pub fn from_bytes<'de, T: de::Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, Error> {
    let mut reader = Reader {
        input: bytes,
        at: 0,
        depth: 0,
    };
    let value = T::deserialize(&mut reader)?;
    if reader.at != bytes.len() {
        return Err(Error::Trailing);
    }
    Ok(value)
}

// --- writing ----------------------------------------------------------------

/// Writes values' canonical bytes.
#[derive(Debug, Default)]
struct Writer {
    out: Vec<u8>,
    depth: u32,
}

impl Writer {
    fn unsigned(&mut self, mut n: u128) {
        loop {
            let low = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                self.out.push(low);
                return;
            }
            self.out.push(low | 0x80);
        }
    }

    fn signed(&mut self, n: i128) {
        // Zigzag: 0, -1, 1, -2 … as 0, 1, 2, 3 …
        self.unsigned(((n << 1) ^ (n >> 127)) as u128);
    }

    fn length(&mut self, len: Option<usize>) -> Result<(), Error> {
        let len = len.ok_or(Error::UnknownLength)?;
        self.unsigned(len as u128);
        Ok(())
    }

    fn enter(&mut self) -> Result<(), Error> {
        if self.depth >= DEPTH {
            return Err(Error::TooDeep);
        }
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// A writer for a part written apart (a map's key or value), as deep
    /// as this one.
    fn apart(&self) -> Writer {
        Writer {
            out: Vec::new(),
            depth: self.depth,
        }
    }
}

impl<'a> ser::Serializer for &'a mut Writer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = MapWriter<'a>;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        self.out.push(u8::from(v));
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        self.signed(i128::from(v));
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        self.signed(i128::from(v));
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        self.signed(i128::from(v));
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.signed(i128::from(v));
        Ok(())
    }

    fn serialize_i128(self, v: i128) -> Result<(), Error> {
        self.signed(v);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        self.unsigned(u128::from(v));
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        self.unsigned(u128::from(v));
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        self.unsigned(u128::from(v));
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        self.unsigned(u128::from(v));
        Ok(())
    }

    fn serialize_u128(self, v: u128) -> Result<(), Error> {
        self.unsigned(v);
        Ok(())
    }

    fn serialize_f32(self, _: f32) -> Result<(), Error> {
        Err(Error::Float)
    }

    fn serialize_f64(self, _: f64) -> Result<(), Error> {
        Err(Error::Float)
    }

    fn serialize_char(self, v: char) -> Result<(), Error> {
        self.unsigned(u128::from(u32::from(v)));
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.serialize_bytes(v.as_bytes())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        self.unsigned(v.len() as u128);
        self.out.extend_from_slice(v);
        Ok(())
    }

    fn serialize_none(self) -> Result<(), Error> {
        self.out.push(0);
        Ok(())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Error> {
        self.out.push(1);
        self.enter()?;
        value.serialize(&mut *self)?;
        self.leave();
        Ok(())
    }

    fn serialize_unit(self) -> Result<(), Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
    ) -> Result<(), Error> {
        self.unsigned(u128::from(index));
        Ok(())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.enter()?;
        value.serialize(&mut *self)?;
        self.leave();
        Ok(())
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.unsigned(u128::from(index));
        self.enter()?;
        value.serialize(&mut *self)?;
        self.leave();
        Ok(())
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self, Error> {
        self.length(len)?;
        self.enter()?;
        Ok(self)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self, Error> {
        self.enter()?;
        Ok(self)
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, Error> {
        self.enter()?;
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, Error> {
        self.unsigned(u128::from(index));
        self.enter()?;
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<MapWriter<'a>, Error> {
        len.ok_or(Error::UnknownLength)?;
        self.enter()?;
        Ok(MapWriter {
            writer: self,
            entries: Vec::new(),
            key: None,
        })
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, Error> {
        self.enter()?;
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        index: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self, Error> {
        self.unsigned(u128::from(index));
        self.enter()?;
        Ok(self)
    }
}

impl ser::SerializeSeq for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Error> {
        self.leave();
        Ok(())
    }
}

impl ser::SerializeTuple for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Error> {
        self.leave();
        Ok(())
    }
}

impl ser::SerializeTupleStruct for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Error> {
        self.leave();
        Ok(())
    }
}

impl ser::SerializeTupleVariant for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), Error> {
        self.leave();
        Ok(())
    }
}

impl ser::SerializeStruct for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        value.serialize(&mut **self)
    }

    fn skip_field(&mut self, _: &'static str) -> Result<(), Error> {
        Err(Error::Skipped)
    }

    fn end(self) -> Result<(), Error> {
        self.leave();
        Ok(())
    }
}

impl ser::SerializeStructVariant for &mut Writer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        value.serialize(&mut **self)
    }

    fn skip_field(&mut self, _: &'static str) -> Result<(), Error> {
        Err(Error::Skipped)
    }

    fn end(self) -> Result<(), Error> {
        self.leave();
        Ok(())
    }
}

/// A map being written: each entry's key and value written apart, then
/// ordered by the key's bytes as the map ends.
#[derive(Debug)]
struct MapWriter<'a> {
    writer: &'a mut Writer,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    /// The key written whose value comes next.
    key: Option<Vec<u8>>,
}

impl ser::SerializeMap for MapWriter<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        let mut apart = self.writer.apart();
        key.serialize(&mut apart)?;
        self.key = Some(apart.out);
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let key = self
            .key
            .take()
            .ok_or_else(|| Error::Custom("a map's value came before its key".into()))?;
        let mut apart = self.writer.apart();
        value.serialize(&mut apart)?;
        self.entries.push((key, apart.out));
        Ok(())
    }

    fn end(mut self) -> Result<(), Error> {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
        if self.entries.windows(2).any(|pair| match pair {
            [a, b] => a.0 == b.0,
            _ => false,
        }) {
            return Err(Error::Unordered);
        }
        self.writer.unsigned(self.entries.len() as u128);
        for (key, value) in &self.entries {
            self.writer.out.extend_from_slice(key);
            self.writer.out.extend_from_slice(value);
        }
        self.writer.leave();
        Ok(())
    }
}

// --- reading ----------------------------------------------------------------

/// Reads a value from its canonical bytes.
#[derive(Debug)]
struct Reader<'de> {
    input: &'de [u8],
    at: usize,
    depth: u32,
}

impl<'de> Reader<'de> {
    fn byte(&mut self) -> Result<u8, Error> {
        let byte = *self.input.get(self.at).ok_or(Error::Truncated)?;
        self.at += 1;
        Ok(byte)
    }

    fn take(&mut self, len: usize) -> Result<&'de [u8], Error> {
        let end = self.at.checked_add(len).ok_or(Error::Truncated)?;
        let bytes = self.input.get(self.at..end).ok_or(Error::Truncated)?;
        self.at = end;
        Ok(bytes)
    }

    fn left(&self) -> usize {
        self.input.len().saturating_sub(self.at)
    }

    fn unsigned(&mut self) -> Result<u128, Error> {
        let mut n: u128 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.byte()?;
            let low = u128::from(byte & 0x7f);
            // The nineteenth byte holds the last two of 128 bits.
            if shift == 126 && low > 0b11 {
                return Err(Error::OutOfRange);
            }
            n |= low << shift;
            if byte & 0x80 == 0 {
                // A last byte of 0 after others adds nothing: written
                // longer than it needs.
                if byte == 0 && shift > 0 {
                    return Err(Error::Overlong);
                }
                return Ok(n);
            }
            shift += 7;
            if shift > 126 {
                return Err(Error::OutOfRange);
            }
        }
    }

    fn signed(&mut self) -> Result<i128, Error> {
        let n = self.unsigned()?;
        Ok(((n >> 1) as i128) ^ -((n & 1) as i128))
    }

    fn whole<T: TryFrom<u128>>(&mut self) -> Result<T, Error> {
        T::try_from(self.unsigned()?).map_err(|_| Error::OutOfRange)
    }

    fn whole_signed<T: TryFrom<i128>>(&mut self) -> Result<T, Error> {
        T::try_from(self.signed()?).map_err(|_| Error::OutOfRange)
    }

    /// A list's or map's length, which can't be longer than the bytes
    /// left, since every item takes at least one.
    fn length(&mut self) -> Result<usize, Error> {
        let len: usize = self.whole()?;
        if len > self.left() {
            return Err(Error::Truncated);
        }
        Ok(len)
    }

    fn bytes(&mut self) -> Result<&'de [u8], Error> {
        let len = self.length()?;
        self.take(len)
    }

    fn text(&mut self) -> Result<&'de str, Error> {
        std::str::from_utf8(self.bytes()?).map_err(|_| Error::Invalid("text that isn't UTF-8"))
    }

    fn enter(&mut self) -> Result<(), Error> {
        if self.depth >= DEPTH {
            return Err(Error::TooDeep);
        }
        self.depth += 1;
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// `visit` run one level deeper.
    fn nested<T>(&mut self, visit: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        self.enter()?;
        let value = visit(self)?;
        self.leave();
        Ok(value)
    }
}

impl<'de> de::Deserializer<'de> for &mut Reader<'de> {
    type Error = Error;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
        Err(Error::NotSelfDescribing)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
        Err(Error::NotSelfDescribing)
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.byte()? {
            0 => visitor.visit_bool(false),
            1 => visitor.visit_bool(true),
            _ => Err(Error::Invalid("a flag other than 0 or 1")),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i8(self.whole_signed()?)
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i16(self.whole_signed()?)
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i32(self.whole_signed()?)
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i64(self.whole_signed()?)
    }

    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_i128(self.signed()?)
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u8(self.whole()?)
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u16(self.whole()?)
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u32(self.whole()?)
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u64(self.whole()?)
    }

    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u128(self.unsigned()?)
    }

    fn deserialize_f32<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
        Err(Error::Float)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Error> {
        Err(Error::Float)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let point: u32 = self.whole()?;
        visitor.visit_char(char::from_u32(point).ok_or(Error::Invalid("a code point"))?)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_borrowed_str(self.text()?)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_borrowed_str(self.text()?)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_borrowed_bytes(self.bytes()?)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_borrowed_bytes(self.bytes()?)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.byte()? {
            0 => visitor.visit_none(),
            1 => self.nested(|reader| visitor.visit_some(reader)),
            _ => Err(Error::Invalid("an option's tag other than 0 or 1")),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.nested(|reader| visitor.visit_newtype_struct(reader))
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let left = self.length()?;
        self.nested(|reader| visitor.visit_seq(Items { reader, left }))
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        self.nested(|reader| visitor.visit_seq(Items { reader, left: len }))
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.nested(|reader| visitor.visit_seq(Items { reader, left: len }))
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let left = self.length()?;
        self.nested(|reader| {
            visitor.visit_map(Entries {
                reader,
                left,
                last: None,
            })
        })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.nested(|reader| {
            visitor.visit_seq(Items {
                reader,
                left: fields.len(),
            })
        })
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        _: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.nested(|reader| visitor.visit_enum(reader))
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_u32(self.whole()?)
    }
}

/// A list's items, a tuple's or a struct's fields, `left` of them still to
/// read.
struct Items<'a, 'de> {
    reader: &'a mut Reader<'de>,
    left: usize,
}

impl<'de> de::SeqAccess<'de> for Items<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(&mut *self.reader).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}

/// A map's entries, `left` of them still to read, each key's bytes after
/// the last's.
struct Entries<'a, 'de> {
    reader: &'a mut Reader<'de>,
    left: usize,
    /// Where the last key's bytes were.
    last: Option<(usize, usize)>,
}

impl<'de> de::MapAccess<'de> for Entries<'_, 'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        let start = self.reader.at;
        let key = seed.deserialize(&mut *self.reader)?;
        let end = self.reader.at;
        if let Some((last_start, last_end)) = self.last {
            let before = self.reader.input.get(last_start..last_end);
            let this = self.reader.input.get(start..end);
            if before >= this {
                return Err(Error::Unordered);
            }
        }
        self.last = Some((start, end));
        Ok(Some(key))
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        seed.deserialize(&mut *self.reader)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.left)
    }
}

impl<'de> de::EnumAccess<'de> for &mut Reader<'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self), Error> {
        let index: u32 = self.whole()?;
        let value = seed.deserialize(index.into_deserializer())?;
        Ok((value, self))
    }
}

impl<'de> de::VariantAccess<'de> for &mut Reader<'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        self.nested(|reader| seed.deserialize(reader))
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        self.nested(|reader| visitor.visit_seq(Items { reader, left: len }))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.nested(|reader| {
            visitor.visit_seq(Items {
                reader,
                left: fields.len(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Plan {
        name: String,
        count: u32,
        offset: i64,
        flag: bool,
        maybe: Option<Box<Plan>>,
        kinds: Vec<Kind>,
        by_name: BTreeMap<String, u16>,
        pair: (u8, char),
        wide: i128,
        id: Id,
        empty: Unit,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Id(u32);

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Unit;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    enum Kind {
        Plain,
        Wrapped(String),
        Pair(u8, u8),
        Named { at: u32, label: Option<String> },
    }

    fn sample() -> Plan {
        Plan {
            name: "orders".into(),
            count: 300,
            offset: -2,
            flag: true,
            maybe: Some(Box::new(Plan {
                name: "é".into(),
                count: 0,
                offset: i64::MIN,
                flag: false,
                maybe: None,
                kinds: Vec::new(),
                by_name: BTreeMap::new(),
                pair: (255, '\u{10ffff}'),
                wide: i128::MAX,
                id: Id(u32::MAX),
                empty: Unit,
            })),
            kinds: vec![
                Kind::Plain,
                Kind::Wrapped("a".into()),
                Kind::Pair(1, 2),
                Kind::Named {
                    at: 7,
                    label: Some("seven".into()),
                },
            ],
            by_name: [("b".to_string(), 2), ("a".to_string(), 1)].into(),
            pair: (3, 'x'),
            wide: i128::MIN,
            id: Id(9),
            empty: Unit,
        }
    }

    #[test]
    fn a_value_reads_back_equal_and_writes_the_same_bytes_again() {
        let plan = sample();
        let bytes = to_bytes(&plan).unwrap();
        let read: Plan = from_bytes(&bytes).unwrap();
        assert_eq!(read, plan);
        assert_eq!(to_bytes(&read).unwrap(), bytes);
    }

    #[test]
    fn numbers_are_as_short_as_they_can_be() {
        assert_eq!(to_bytes(&0u64).unwrap(), [0]);
        assert_eq!(to_bytes(&127u64).unwrap(), [0x7f]);
        assert_eq!(to_bytes(&128u64).unwrap(), [0x80, 0x01]);
        assert_eq!(to_bytes(&300u32).unwrap(), [0xac, 0x02]);
        // Zigzag: -1 is 1, 1 is 2, -64 is 127.
        assert_eq!(to_bytes(&-1i32).unwrap(), [1]);
        assert_eq!(to_bytes(&1i32).unwrap(), [2]);
        assert_eq!(to_bytes(&-64i64).unwrap(), [0x7f]);
        for n in [0, 1, -1, i64::MAX, i64::MIN, 1 << 40, -(1 << 40)] {
            assert_eq!(from_bytes::<i64>(&to_bytes(&n).unwrap()), Ok(n));
        }
        for n in [0, u128::MAX, 1 << 126, (1 << 127) + 5] {
            assert_eq!(from_bytes::<u128>(&to_bytes(&n).unwrap()), Ok(n));
        }
        assert_eq!(to_bytes(&u128::MAX).unwrap().len(), 19);
    }

    #[test]
    fn the_layout_is_the_one_the_module_describes() {
        // A struct is its fields in order: text as its length and bytes,
        // an option as its tag, an enum's variant by its place.
        #[derive(Serialize)]
        struct Small {
            text: &'static str,
            none: Option<u8>,
            some: Option<u8>,
            kind: Kind,
            list: Vec<bool>,
        }
        let bytes = to_bytes(&Small {
            text: "ab",
            none: None,
            some: Some(5),
            kind: Kind::Pair(4, 6),
            list: vec![true, false],
        })
        .unwrap();
        assert_eq!(bytes, [2, b'a', b'b', 0, 1, 5, 2, 4, 6, 2, 1, 0]);
    }

    #[test]
    fn a_map_gives_the_same_bytes_whatever_order_it_holds() {
        let ordered: BTreeMap<String, u32> = (0..50).map(|n| (format!("key {n}"), n)).collect();
        let hashed: HashMap<String, u32> = ordered.clone().into_iter().collect();
        let mut reversed: Vec<(String, u32)> = ordered.clone().into_iter().collect();
        reversed.reverse();
        let reversed: HashMap<String, u32> = reversed.into_iter().collect();
        let bytes = to_bytes(&ordered).unwrap();
        assert_eq!(to_bytes(&hashed).unwrap(), bytes);
        assert_eq!(to_bytes(&reversed).unwrap(), bytes);
        assert_eq!(from_bytes::<BTreeMap<String, u32>>(&bytes), Ok(ordered));
    }

    #[test]
    fn a_map_read_out_of_order_or_twice_is_refused() {
        // Two entries, keys "b" then "a".
        let unordered = [2, 1, b'b', 2, 1, b'a', 1];
        assert_eq!(
            from_bytes::<BTreeMap<String, u8>>(&unordered),
            Err(Error::Unordered)
        );
        let twice = [2, 1, b'a', 1, 1, b'a', 2];
        assert_eq!(
            from_bytes::<BTreeMap<String, u8>>(&twice),
            Err(Error::Unordered)
        );
    }

    #[test]
    fn floats_have_no_canonical_form() {
        assert_eq!(to_bytes(&1.5f64), Err(Error::Float));
        assert_eq!(to_bytes(&vec![0.5f32]), Err(Error::Float));
        assert_eq!(from_bytes::<f64>(&[0]), Err(Error::Float));
    }

    #[test]
    fn a_field_left_out_is_refused() {
        #[derive(Serialize)]
        struct Sometimes {
            #[serde(skip_serializing_if = "Option::is_none")]
            maybe: Option<u8>,
        }
        assert_eq!(to_bytes(&Sometimes { maybe: None }), Err(Error::Skipped));
    }

    #[test]
    fn bytes_that_are_not_one_value_are_refused() {
        let bytes = to_bytes(&sample()).unwrap();
        // Every cut short of the end runs out inside the value.
        for end in 0..bytes.len() {
            assert!(
                from_bytes::<Plan>(&bytes[..end]).is_err(),
                "read {end} bytes as a whole plan"
            );
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(from_bytes::<Plan>(&longer), Err(Error::Trailing));
        assert_eq!(from_bytes::<u32>(&[0x80, 0x00]), Err(Error::Overlong));
        assert_eq!(from_bytes::<u8>(&[0x80, 0x02]), Err(Error::OutOfRange));
        assert_eq!(from_bytes::<u128>(&[0xff; 19]), Err(Error::OutOfRange));
        assert_eq!(from_bytes::<u128>(&[0xff; 20]), Err(Error::OutOfRange));
        assert_eq!(
            from_bytes::<bool>(&[2]),
            Err(Error::Invalid("a flag other than 0 or 1"))
        );
        assert_eq!(
            from_bytes::<Option<u8>>(&[3]),
            Err(Error::Invalid("an option's tag other than 0 or 1"))
        );
        assert_eq!(
            from_bytes::<String>(&[1, 0xff]),
            Err(Error::Invalid("text that isn't UTF-8"))
        );
        assert_eq!(
            from_bytes::<char>(&[0x80, 0xb0, 0x03]),
            Err(Error::Invalid("a code point"))
        );
        // A list longer than the bytes left.
        assert_eq!(from_bytes::<Vec<u8>>(&[5, 1]), Err(Error::Truncated));
        // A variant the enum doesn't have.
        assert!(matches!(from_bytes::<Kind>(&[9]), Err(Error::Custom(_))));
    }

    #[test]
    fn nesting_past_the_limit_is_refused_both_ways() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Nest(Option<Box<Nest>>);
        let mut deep = Nest(None);
        for _ in 0..DEPTH {
            deep = Nest(Some(Box::new(deep)));
        }
        assert_eq!(to_bytes(&deep), Err(Error::TooDeep));
        // The same depth written by hand: each level `1`, then `0`.
        let mut bytes = vec![1u8; DEPTH as usize];
        bytes.push(0);
        assert_eq!(from_bytes::<Nest>(&bytes), Err(Error::TooDeep));
        let mut shallow = Nest(None);
        for _ in 0..40 {
            shallow = Nest(Some(Box::new(shallow)));
        }
        let bytes = to_bytes(&shallow).unwrap();
        assert_eq!(from_bytes::<Nest>(&bytes), Ok(shallow));
    }

    #[test]
    fn a_reader_never_guesses_what_the_bytes_hold() {
        assert_eq!(
            from_bytes::<serde::de::IgnoredAny>(&[0]),
            Err(Error::NotSelfDescribing)
        );
    }
}
