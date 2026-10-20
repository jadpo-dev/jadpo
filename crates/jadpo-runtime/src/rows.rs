//! A stored file's rows, as the file helpers read them (area 19, "Data
//! formats"; area 12, "Batches"; chunk 6.4b): pure, over a window of the
//! file's bytes that a batch run read before its page's transaction
//! opened, so a page is read in as many windows as it needs and a run
//! resumes at the byte its last page committed.
//!
//! - **Encodings.** UTF-8, a byte-order mark removed, and UTF-16 with a
//!   mark, little- or big-endian ([`opening`]); never guessed. A gzip file
//!   is said to be one, since it is read only with `max_unpacked_size:`
//!   (roadmap 6.4b2). Bytes that aren't text in the file's encoding, a
//!   NUL among them, stop the run, named by their line.
//! - **CSV and TSV** ([`records`] with a delimiter): RFC 4180's records,
//!   cells in double quotes holding the delimiter, line breaks and `""`
//!   for a quote; a line break is LF, CRLF or a lone CR, kept as written
//!   inside a quoted cell. A quote that never closes stops the run, named
//!   by the line it opened on. A quote inside an unquoted cell, or text
//!   after a cell's closing quote, is kept as written (chosen in chunk
//!   6.4b, as spreadsheets read them). An empty line is a blank record:
//!   it has a row number, as a spreadsheet shows it, and is no item.
//! - **JSON Lines** ([`records`] without one): a record a line, its one
//!   cell the line's text, LF or CRLF ending it.
//!
//! - **JSON** ([`json`], chunk 6.4b3): `json.read`'s file form walks the
//!   document to the list `items_at:` names, an item a record.
//! - **gzip** ([`gzip`], chunk 6.4b3): a file unpacked as it is read, when
//!   the helper is given `max_unpacked_size:`.
//!
//! A record is whole only once what ends it has been read: the last in
//! a window that the file goes on past waits for the next window.

pub mod gzip;
pub mod json;

/// A file's text encoding, from the mark it starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

impl Encoding {
    /// As a fault names it.
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf16Le | Encoding::Utf16Be => "UTF-16",
        }
    }
}

/// What a file's first bytes say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// Text in `encoding`, starting `start` bytes in, after its mark.
    Text { encoding: Encoding, start: u64 },
    /// gzip, by its magic number.
    Gzip,
}

/// How many of a file's first bytes [`opening`] reads.
pub const OPENING_BYTES: u64 = 3;

/// What `first`, a file's first bytes (all of them when it is shorter
/// than [`OPENING_BYTES`]), say it is.
pub fn opening(first: &[u8]) -> Opening {
    match first {
        [0x1f, 0x8b, ..] => Opening::Gzip,
        [0xef, 0xbb, 0xbf, ..] => Opening::Text {
            encoding: Encoding::Utf8,
            start: 3,
        },
        [0xff, 0xfe, ..] => Opening::Text {
            encoding: Encoding::Utf16Le,
            start: 2,
        },
        [0xfe, 0xff, ..] => Opening::Text {
            encoding: Encoding::Utf16Be,
            start: 2,
        },
        _ => Opening::Text {
            encoding: Encoding::Utf8,
            start: 0,
        },
    }
}

/// One record read: where the next begins in the file, the line it began
/// on and the line after it, and its cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub next: u64,
    pub line: u64,
    pub next_line: u64,
    pub cells: Vec<String>,
    /// An empty line: no cells at all, no quote.
    pub blank: bool,
}

/// Why reading stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// It has as many records as it was asked for.
    Full,
    /// The window ended before the record in hand did, and the file goes
    /// on: read more.
    More,
    /// The file ended, every record in it read.
    End,
    /// The file can't be read on from here.
    Fault(RowFault),
}

/// What stops a run at the file's level (area 12, "Problems never stop a
/// run").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowFault {
    /// A quote opened on this line never closes.
    Unterminated { line: u64 },
    /// Bytes on this line aren't text in the file's encoding.
    NotText { line: u64, encoding: Encoding },
    /// A JSON document with no list where `items_at:` says, as it writes
    /// it (chunk 6.4b3).
    NoList(String),
    /// A JSON document read as a list that isn't one.
    NotList,
    /// What is on this line isn't JSON.
    NotJson { line: u64 },
    /// The document ends before its JSON does.
    Ended,
    /// An object on the way to the list names this member twice.
    Twice(String),
    /// The item beginning on this line is longer than a row may be.
    TooLong { line: u64 },
}

impl RowFault {
    /// Why the run stopped, as its row keeps it: never a value.
    pub fn why(&self) -> String {
        match self {
            RowFault::Unterminated { line } => {
                format!("its file has a quote opened on line {line} that never closes")
            }
            RowFault::NotText { line, encoding } => {
                format!("line {line} of its file isn't {} text", encoding.name())
            }
            RowFault::NoList(path) => format!("its file has no list at `{path}`"),
            RowFault::NotList => "its file isn't a JSON list".to_string(),
            RowFault::NotJson { line } => format!("line {line} of its file isn't JSON"),
            RowFault::Ended => "its file ends before its JSON does".to_string(),
            RowFault::Twice(name) => format!("its file names `{name}` twice"),
            RowFault::TooLong { line } => format!(
                "the item beginning on line {line} of its file is longer than {}",
                jadpo_std::files::shown_size(crate::batches::MAX_ROW_BYTES)
            ),
        }
    }
}

/// One character decoded, with its bytes.
enum Next {
    Char(char, usize),
    /// The window ends inside it.
    Partial,
    Invalid,
    /// The window ends here.
    Ended,
}

fn next(bytes: &[u8], at: usize, encoding: Encoding) -> Next {
    let Some(rest) = bytes.get(at..) else {
        return Next::Ended;
    };
    if rest.is_empty() {
        return Next::Ended;
    }
    let decoded = match encoding {
        Encoding::Utf8 => {
            let Some(&lead) = rest.first() else {
                return Next::Ended;
            };
            let width = match lead {
                0x00..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Next::Invalid,
            };
            let Some(held) = rest.get(..width) else {
                // A lead byte whose continuation the window hasn't reached,
                // unless what it has already isn't one.
                let partial_ok = rest.iter().skip(1).all(|byte| (0x80..=0xbf).contains(byte));
                return if partial_ok {
                    Next::Partial
                } else {
                    Next::Invalid
                };
            };
            match std::str::from_utf8(held)
                .ok()
                .and_then(|text| text.chars().next())
            {
                Some(c) => Next::Char(c, width),
                None => return Next::Invalid,
            }
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let unit = |pair: &[u8]| -> Option<u16> {
                let [a, b] = <[u8; 2]>::try_from(pair).ok()?;
                Some(match encoding {
                    Encoding::Utf16Le => u16::from_le_bytes([a, b]),
                    _ => u16::from_be_bytes([a, b]),
                })
            };
            let Some(first) = rest.get(..2).and_then(unit) else {
                return Next::Partial;
            };
            if (0xd800..0xdc00).contains(&first) {
                let Some(second) = rest.get(2..4).and_then(unit) else {
                    return Next::Partial;
                };
                match char::decode_utf16([first, second]).next() {
                    Some(Ok(c)) => Next::Char(c, 4),
                    _ => return Next::Invalid,
                }
            } else {
                match char::decode_utf16([first]).next() {
                    Some(Ok(c)) => Next::Char(c, 2),
                    _ => return Next::Invalid,
                }
            }
        }
    };
    match decoded {
        // A NUL is no text's.
        Next::Char('\0', _) => Next::Invalid,
        other => other,
    }
}

/// At most `most` records from `bytes`, the file's bytes from offset
/// `from`, the first beginning on line `line`; `delimiter` the byte
/// between a CSV or TSV row's cells, or `None` for JSON Lines' lines.
/// `ends` says the window reaches the file's end. With the records read,
/// why it stopped.
pub fn records(
    bytes: &[u8],
    from: u64,
    line: u64,
    encoding: Encoding,
    delimiter: Option<u8>,
    ends: bool,
    most: usize,
) -> (Vec<Record>, Stop) {
    let mut out = Vec::new();
    let mut at = 0usize;
    let mut line = line;
    loop {
        if out.len() >= most {
            return (out, Stop::Full);
        }
        if at >= bytes.len() {
            return (out, if ends { Stop::End } else { Stop::More });
        }
        match record(bytes, at, line, encoding, delimiter, ends) {
            Read::Record {
                end,
                next_line,
                cells,
                blank,
            } => {
                out.push(Record {
                    next: from + end as u64,
                    line,
                    next_line,
                    cells,
                    blank,
                });
                at = end;
                line = next_line;
            }
            Read::More => return (out, Stop::More),
            Read::Fault(fault) => return (out, Stop::Fault(fault)),
        }
    }
}

enum Read {
    Record {
        end: usize,
        next_line: u64,
        cells: Vec<String>,
        blank: bool,
    },
    More,
    Fault(RowFault),
}

/// One record from `at`, which begins on `line`.
fn record(
    bytes: &[u8],
    mut at: usize,
    line: u64,
    encoding: Encoding,
    delimiter: Option<u8>,
    ends: bool,
) -> Read {
    let mut cells: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoting = false;
    let mut quoted = false;
    let mut opened_on = line;
    let mut on = line;
    let not_text = |on: u64| Read::Fault(RowFault::NotText { line: on, encoding });
    loop {
        let (c, width) = match next(bytes, at, encoding) {
            Next::Char(c, width) => (c, width),
            Next::Invalid => return not_text(on),
            Next::Partial if ends => return not_text(on),
            Next::Partial => return Read::More,
            Next::Ended if !ends => return Read::More,
            Next::Ended if quoting => {
                return Read::Fault(RowFault::Unterminated { line: opened_on });
            }
            Next::Ended => {
                let blank = cells.is_empty() && cell.is_empty() && !quoted;
                cells.push(cell);
                return Read::Record {
                    end: at,
                    next_line: on,
                    cells,
                    blank,
                };
            }
        };
        if quoting {
            if c == '"' {
                match next(bytes, at + width, encoding) {
                    Next::Char('"', second) => {
                        cell.push('"');
                        at += width + second;
                    }
                    Next::Partial | Next::Ended if !ends => return Read::More,
                    _ => {
                        quoting = false;
                        at += width;
                    }
                }
                continue;
            }
            if c == '\n' {
                on += 1;
            }
            cell.push(c);
            at += width;
            continue;
        }
        let ending = |on: u64, end: usize, mut cells: Vec<String>, cell: String| {
            let blank = cells.is_empty() && cell.is_empty() && !quoted;
            cells.push(cell);
            Read::Record {
                end,
                next_line: on + 1,
                cells,
                blank,
            }
        };
        match c {
            '\n' => return ending(on, at + width, cells, cell),
            '\r' => match next(bytes, at + width, encoding) {
                Next::Char('\n', second) => {
                    return ending(on, at + width + second, cells, cell);
                }
                Next::Partial | Next::Ended if !ends => return Read::More,
                _ => return ending(on, at + width, cells, cell),
            },
            '"' if delimiter.is_some() && cell.is_empty() && !quoted => {
                quoting = true;
                quoted = true;
                opened_on = on;
                at += width;
            }
            c if delimiter.is_some_and(|d| c.is_ascii() && c as u32 == u32::from(d)) => {
                cells.push(std::mem::take(&mut cell));
                quoted = false;
                at += width;
            }
            c => {
                cell.push(c);
                at += width;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv(text: &[u8], ends: bool, most: usize) -> (Vec<Record>, Stop) {
        records(text, 0, 1, Encoding::Utf8, Some(b','), ends, most)
    }

    fn cells(records: &[Record]) -> Vec<Vec<&str>> {
        records
            .iter()
            .map(|record| record.cells.iter().map(String::as_str).collect())
            .collect()
    }

    #[test]
    fn quoted_cells_hold_the_delimiter_line_breaks_and_quotes() {
        let text = b"a,\"b, c\",\"d\r\ne\"\r\n\"say \"\"hi\"\"\",,f\n\nlast";
        let (read, stop) = csv(text, true, 10);
        assert_eq!(stop, Stop::End);
        assert_eq!(
            cells(&read),
            [
                vec!["a", "b, c", "d\r\ne"],
                vec!["say \"hi\"", "", "f"],
                vec![""],
                vec!["last"],
            ]
        );
        assert_eq!(
            read.iter()
                .map(|r| (r.line, r.next_line, r.blank))
                .collect::<Vec<_>>(),
            [(1, 3, false), (3, 4, false), (4, 5, true), (5, 5, false)]
        );
        assert_eq!(read.last().map(|r| r.next), Some(text.len() as u64));
        // A quoted empty cell is no blank line.
        let (quoted, _) = csv(b"\"\"\n", true, 10);
        assert!(!quoted[0].blank);
    }

    #[test]
    fn a_record_the_window_cuts_waits_for_the_next() {
        let text = b"a,b\nc,\"d\ne\"\nf";
        for cut in 0..text.len() {
            let (read, stop) = csv(&text[..cut], false, 10);
            let whole = read.last().map_or(0, |r| r.next as usize);
            assert!(whole <= cut, "{cut}");
            assert_eq!(stop, Stop::More, "{cut}");
            // What it read is what the whole file reads, up to there.
            let (all, _) = csv(text, true, 10);
            assert_eq!(cells(&read), cells(&all[..read.len()]), "{cut}");
        }
        // CRLF split across windows.
        let (read, stop) = csv(b"a\r", false, 10);
        assert!(read.is_empty() && stop == Stop::More);
        let (read, stop) = csv(b"a,b\nc", true, 1);
        assert_eq!((read.len(), stop), (1, Stop::Full));
    }

    #[test]
    fn a_quote_that_never_closes_and_bytes_that_arent_text_stop_the_run() {
        let (read, stop) = csv(b"a\n\"b\nc\nd", true, 10);
        assert_eq!(cells(&read), [vec!["a"]]);
        assert_eq!(stop, Stop::Fault(RowFault::Unterminated { line: 2 }));
        let (read, stop) = csv(b"a\nb\xff\n", true, 10);
        assert_eq!(read.len(), 1);
        assert_eq!(
            stop,
            Stop::Fault(RowFault::NotText {
                line: 2,
                encoding: Encoding::Utf8
            })
        );
        assert_eq!(
            csv(b"a\0b\n", true, 10).1,
            Stop::Fault(RowFault::NotText {
                line: 1,
                encoding: Encoding::Utf8
            })
        );
        // A character the window cuts is waited for; one the file cuts
        // isn't text.
        let cafe = "café\n".as_bytes();
        assert_eq!(csv(&cafe[..4], false, 10).1, Stop::More);
        assert!(matches!(csv(&cafe[..4], true, 10).1, Stop::Fault(_)));
        assert_eq!(
            RowFault::Unterminated { line: 3 }.why(),
            "its file has a quote opened on line 3 that never closes"
        );
    }

    #[test]
    fn utf16_with_its_mark_and_json_lines_are_read() {
        let text = "a,b\r\nc,☕\r\n";
        let mut little = vec![0xff, 0xfe];
        for unit in text.encode_utf16() {
            little.extend_from_slice(&unit.to_le_bytes());
        }
        let Opening::Text { encoding, start } = opening(&little) else {
            panic!("text");
        };
        assert_eq!((encoding, start), (Encoding::Utf16Le, 2));
        let (read, stop) = records(&little[2..], 2, 1, encoding, Some(b','), true, 10);
        assert_eq!(stop, Stop::End);
        assert_eq!(cells(&read), [vec!["a", "b"], vec!["c", "☕"]]);
        assert_eq!(read[0].next, 2 + 10);
        assert_eq!(opening(&[0x1f, 0x8b, 0x08]), Opening::Gzip);
        assert_eq!(
            opening(b"\xef\xbb\xbfa"),
            Opening::Text {
                encoding: Encoding::Utf8,
                start: 3
            }
        );
        let (lines, stop) = records(
            b"{\"a\": \"x,\\\"y\"}\r\n\n[1]",
            0,
            1,
            Encoding::Utf8,
            None,
            true,
            10,
        );
        assert_eq!(stop, Stop::End);
        assert_eq!(
            cells(&lines),
            [vec!["{\"a\": \"x,\\\"y\"}"], vec![""], vec!["[1]"]]
        );
        assert!(lines[1].blank);
        assert_eq!(lines[2].line, 3);
    }
}
