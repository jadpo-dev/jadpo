//! Keeps tokio-postgres from looking up a column type it doesn't know.
//!
//! A column of an extension's type (pgvector's `vector`, PostGIS's
//! `geography`) has a number that differs in each database. Given one in a
//! `RowDescription`, tokio-postgres asks the server what it is through a
//! *named* statement, a `Parse` and later a `Bind`. A pooler that hands out a
//! server connection a transaction at a time, as Hyperdrive does, may run
//! them on different connections, and a statement the client never closes is
//! left on one the next client gets. The real deploy read `vector` so and
//! failed in 8 of 12 runs (chunk 8.2n), and tokio-postgres offers no way to
//! tell it a type or to skip the lookup.
//!
//! So the driver names no statement. [`Described`] reads what the server
//! sends and, in each `RowDescription`, marks a column of a type
//! tokio-postgres doesn't know as [`HIDDEN`], a type it does, with the real
//! number in the column's table number, which nothing here reads otherwise.
//! The driver then asks for the marked types' names itself in one unnamed
//! statement (`driver::names_of`), and reads those columns by name.

use std::mem;

use tokio_postgres::types::Type;

/// The type a column of an unknown type is marked with: PostgreSQL's
/// `unknown`, which no column of a result has had since PostgreSQL 10.
pub(super) const HIDDEN: Type = Type::UNKNOWN;

/// A message's type byte and length.
const HEAD: usize = 5;

/// A field's fixed part after its name: table, column, type, size, modifier
/// and format.
const FIELD: usize = 18;

/// What's left of the messages read so far, across reads: a read may end
/// anywhere, and a message is marked only once all of it has come.
#[derive(Debug, Default)]
pub(super) struct Described {
    /// Bytes of a message already looked at, still to pass through.
    skip: usize,
    /// A head or a `RowDescription` not yet complete.
    held: Vec<u8>,
    /// Bytes that were held, now ready to give out, from `given`.
    ready: Vec<u8>,
    given: usize,
}

impl Described {
    /// Marks what `read` completes. Gives how many of its leading bytes are
    /// ready to pass on as they are; the rest it has kept.
    pub(super) fn scan(&mut self, read: &mut [u8]) -> usize {
        if self.held.is_empty() {
            return self.walk(read);
        }
        let mut whole = mem::take(&mut self.held);
        whole.extend_from_slice(read);
        let ready = self.walk(&mut whole);
        self.compact();
        self.ready
            .extend_from_slice(whole.get(..ready).unwrap_or_default());
        0
    }

    /// Bytes kept earlier that are ready to give out.
    pub(super) fn ready(&self) -> &[u8] {
        self.ready.get(self.given..).unwrap_or_default()
    }

    /// Notes that `count` of [`Described::ready`]'s bytes went out.
    pub(super) fn gave(&mut self, count: usize) {
        self.given += count;
        self.compact();
    }

    /// The stream ended: whatever it left incomplete goes out as it came,
    /// for the client to find the stream broken.
    pub(super) fn finish(&mut self) {
        let held = mem::take(&mut self.held);
        self.compact();
        self.ready.extend_from_slice(&held);
    }

    fn compact(&mut self) {
        if self.given >= self.ready.len() {
            self.ready.clear();
            self.given = 0;
        }
    }

    /// Walks `bytes` message by message, marking each complete
    /// `RowDescription`. Gives how many leading bytes are done with, and
    /// holds the rest when a head or a `RowDescription` is cut off.
    fn walk(&mut self, bytes: &mut [u8]) -> usize {
        let mut at = 0;
        loop {
            if self.skip > 0 {
                let passed = self.skip.min(bytes.len() - at);
                at += passed;
                self.skip -= passed;
            }
            let Some(rest) = bytes.get_mut(at..) else {
                return at;
            };
            if rest.is_empty() {
                return at;
            }
            let Some(total) = message_length(rest) else {
                self.held.extend_from_slice(rest);
                return at;
            };
            if rest.first() == Some(&b'T') {
                let Some(message) = rest.get_mut(..total) else {
                    self.held.extend_from_slice(rest);
                    return at;
                };
                mark(message);
                at += total;
            } else {
                at += HEAD;
                self.skip = total.saturating_sub(HEAD);
            }
        }
    }
}

/// A complete message's length from its head, when all of its head has
/// come. One that claims less than its head is passed as its head alone.
fn message_length(bytes: &[u8]) -> Option<usize> {
    let length = bytes.get(1..HEAD)?;
    let length: [u8; 4] = length.try_into().ok()?;
    Some((u32::from_be_bytes(length) as usize + 1).max(HEAD))
}

/// Marks each field of a `RowDescription` whose type tokio-postgres doesn't
/// know. It leaves one it can't read as it came, for tokio-postgres to refuse.
fn mark(message: &mut [u8]) {
    let Some((_, body)) = message.split_at_mut_checked(HEAD) else {
        return;
    };
    let Some((count, mut fields)) = body.split_at_mut_checked(2) else {
        return;
    };
    let Ok(count) = <[u8; 2]>::try_from(&*count) else {
        return;
    };
    for _ in 0..u16::from_be_bytes(count) {
        let Some(name) = fields.iter().position(|byte| *byte == 0) else {
            return;
        };
        let Some((_, after_name)) = mem::take(&mut fields).split_at_mut_checked(name + 1) else {
            return;
        };
        let Some((field, rest)) = after_name.split_at_mut_checked(FIELD) else {
            return;
        };
        fields = rest;
        let oid = field
            .get(6..10)
            .and_then(|oid| <[u8; 4]>::try_from(oid).ok());
        let Some(oid) = oid.map(u32::from_be_bytes) else {
            return;
        };
        if Type::from_oid(oid).is_some() {
            continue;
        }
        if let Some(table) = field.get_mut(..4) {
            table.copy_from_slice(&oid.to_be_bytes());
        }
        if let Some(column_type) = field.get_mut(6..10) {
            column_type.copy_from_slice(&HIDDEN.oid().to_be_bytes());
        }
    }
}

/// The number of the type a column marked [`HIDDEN`] really has.
pub(super) fn hidden_oid(column: &tokio_postgres::Column) -> Option<u32> {
    (*column.type_() == HIDDEN)
        .then(|| column.table_oid())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A message of `kind` holding `body`.
    fn message(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&((body.len() + 4) as u32).to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    /// A `RowDescription` of columns named with their types' numbers, each
    /// from table 7.
    fn described(columns: &[(&str, u32)]) -> Vec<u8> {
        let mut body = (columns.len() as u16).to_be_bytes().to_vec();
        for (name, oid) in columns {
            body.extend_from_slice(name.as_bytes());
            body.push(0);
            body.extend_from_slice(&7u32.to_be_bytes());
            body.extend_from_slice(&1i16.to_be_bytes());
            body.extend_from_slice(&oid.to_be_bytes());
            body.extend_from_slice(&(-1i16).to_be_bytes());
            body.extend_from_slice(&(-1i32).to_be_bytes());
            body.extend_from_slice(&1i16.to_be_bytes());
        }
        message(b'T', &body)
    }

    /// Reads `stream` in pieces of `size`, as a socket might give them.
    fn through(stream: &[u8], size: usize) -> Vec<u8> {
        let mut described = Described::default();
        let mut out = Vec::new();
        for piece in stream.chunks(size) {
            let mut piece = piece.to_vec();
            let passed = described.scan(&mut piece);
            let ready = described.ready().to_vec();
            described.gave(ready.len());
            out.extend_from_slice(&ready);
            out.extend_from_slice(&piece[..passed]);
        }
        described.finish();
        out.extend_from_slice(described.ready());
        out
    }

    #[test]
    fn a_column_of_a_type_it_doesnt_know_is_marked_however_the_reads_fall() {
        const VECTOR: u32 = 16_500;
        let row = message(b'D', &[9; 40]);
        let mut stream = message(b'1', b"");
        stream.extend_from_slice(&described(&[
            ("id", 23),
            ("meaning", VECTOR),
            ("label", 25),
        ]));
        stream.extend_from_slice(&row);
        stream.extend_from_slice(&message(b'Z', b"I"));

        let mut wanted = message(b'1', b"");
        let mut marked = (3u16).to_be_bytes().to_vec();
        for (name, table, oid) in [
            ("id", 7u32, 23u32),
            ("meaning", VECTOR, HIDDEN.oid()),
            ("label", 7, 25),
        ] {
            marked.extend_from_slice(name.as_bytes());
            marked.push(0);
            marked.extend_from_slice(&table.to_be_bytes());
            marked.extend_from_slice(&1i16.to_be_bytes());
            marked.extend_from_slice(&oid.to_be_bytes());
            marked.extend_from_slice(&(-1i16).to_be_bytes());
            marked.extend_from_slice(&(-1i32).to_be_bytes());
            marked.extend_from_slice(&1i16.to_be_bytes());
        }
        wanted.extend_from_slice(&message(b'T', &marked));
        wanted.extend_from_slice(&row);
        wanted.extend_from_slice(&message(b'Z', b"I"));

        for size in 1..=stream.len() {
            assert_eq!(
                through(&stream, size),
                wanted,
                "read {size} bytes at a time"
            );
        }
    }

    #[test]
    fn a_type_it_knows_and_every_other_message_pass_as_they_came() {
        // A data row whose bytes read like a `RowDescription` isn't one.
        let looks_like_one = described(&[("x", 16_500)]);
        let mut stream = described(&[("id", 23), ("at", 1184)]);
        stream.extend_from_slice(&message(b'D', &looks_like_one));
        stream.extend_from_slice(&message(b'E', b"Sfailed\0\0"));
        for size in [1, 3, 7, stream.len()] {
            assert_eq!(
                through(&stream, size),
                stream,
                "read {size} bytes at a time"
            );
        }
    }

    #[test]
    fn a_stream_that_ends_midway_gives_what_it_had() {
        let whole = described(&[("meaning", 16_500)]);
        let cut = &whole[..whole.len() - 3];
        assert_eq!(through(cut, 4), cut);
    }

    #[test]
    fn a_row_description_it_cant_read_is_left_alone() {
        let mut body = 2u16.to_be_bytes().to_vec();
        body.extend_from_slice(b"name with no end");
        let stream = message(b'T', &body);
        assert_eq!(through(&stream, 1), stream);
    }
}
