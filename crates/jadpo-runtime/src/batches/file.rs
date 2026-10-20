//! A file helper's run (area 12, "Batches"; area 19, "Data formats";
//! chunk 6.4b): `csv.read`, `tsv.read`, `json_lines.read` or `json.read`'s
//! file form (chunk 6.4b3), walking a stored `File`'s rows a batch at a
//! time.
//!
//! An item's action runs inside its batch's transaction, and no outside
//! call goes out while a transaction is open (chunk 7.0c), so each page's
//! rows are read from the store first, through the capability seam, from
//! the position the last page committed ([`PLACE`]); only then does the
//! batch open its transaction, lock the run's row, hold it to the same
//! position, run the rows' items and commit their writes with the problems
//! and the position after the last row read. A runner reads the file's
//! opening once per invocation ([`Opened`]): its encoding and, for CSV and
//! TSV, its header, held to the row type's fields, and for JSON the way to
//! its list of items.
//!
//! A gzip file is read only when the helper is given `max_unpacked_size:`
//! (chunk 6.4b3): it is unpacked as it is read ([`Source`]), its position
//! counted in its unpacked text, and a runner that picks a run up unpacks
//! it again from its start. Its text is never read past the size the
//! helper allows: the run stops at the row that would.
//!
//! A row that can't be read as the row type is a problem with its row (as
//! a spreadsheet numbers it, the header row 1, for JSON Lines its line,
//! and for JSON its place in the list) and its column (the header's, or
//! the JSON field's path), never its value; so is a row whose action
//! fails. What can't be read on from stops the run, before or at the place
//! it is: a missing or unknown column, a quote that never closes, text that
//! isn't text, a JSON document with no list where `items_at:` says or that
//! isn't JSON, a gzip file read without `max_unpacked_size:`, one unpacking
//! past it, damaged or cut short, a row past [`MAX_ROW_BYTES`]. The rows
//! before it are done.
//!
//! Each page keeps the total the run estimates (chunk 6.4b3): the items
//! done so far scaled by the file's size over how far into it they end,
//! for a gzip file the size it is unpacking to at the rate it has so far.

use std::ops::ControlFlow;

use jadpo_std::BatchState;
use jadpo_std::batch::{BatchPlan, FileRows, RowFormat};
use jadpo_std::http::CodecId;
use jadpo_value::Value;

use super::{
    ADVANCE, Batches, Item, LOCK, PROBLEM, Ran, Saved, Step, abandoned, arguments, at, batch,
    end_in, finish, problem, total,
};
use crate::effects::Unanswered;
use crate::files::{Stored, WINDOW, read, stored};
use crate::rows::gzip::{GzipFault, Unpacker};
use crate::rows::json::{Walked, Walker};
use crate::rows::{Encoding, OPENING_BYTES, Opening, Record, RowFault, Stop, opening, records};
use crate::runner::Batch;
use crate::{Ctx, Executor, SqlValue, StoreError};

use super::BatchStart;

/// Where a run has got to, read before its page's transaction opens.
pub(crate) const PLACE: &str =
    "SELECT \"state\", \"position\", \"read_all\" FROM \"jadpo_batch_runs\" WHERE \"id\" = $1";

/// The most bytes a page reads looking for its rows: past it, a page takes
/// the rows it has, and a row longer than it stops the run.
pub const MAX_ROW_BYTES: u64 = 16_000_000;

/// Where the next row begins: its byte, its row as a spreadsheet numbers
/// it, and its line. A run's row keeps it as JSON text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Position {
    at: u64,
    row: i64,
    line: u64,
}

impl Position {
    fn text(self) -> String {
        format!(
            "{{\"at\":{},\"row\":{},\"line\":{}}}",
            self.at, self.row, self.line
        )
    }

    fn read(text: &str) -> Option<Position> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        Some(Position {
            at: value.get("at")?.as_u64()?,
            row: value.get("row")?.as_i64()?,
            line: value.get("line")?.as_u64()?,
        })
    }
}

/// What a runner learnt of a run's file as it opened it, kept for each
/// page it runs in its invocation.
pub(crate) struct Opened {
    source: Source,
    encoding: Encoding,
    /// Where the first row begins.
    first: Position,
    /// For CSV and TSV, the field each of the header's columns is read
    /// into, by its place among the row type's fields.
    columns: Vec<usize>,
    /// The row type's fields' names, and the column each is read from.
    fields: Vec<String>,
    names: Vec<String>,
    codec: CodecId,
    /// For JSON, a walk through the document standing just inside its list
    /// of items, which each page starts a copy of from its position.
    walker: Option<Walker>,
}

/// How many unpacked bytes a gzip file's reading makes at a time.
const UNPACKED_CHUNK: usize = 1 << 16;

/// Where a file's text comes from: its stored bytes, or, for a gzip file,
/// what they unpack to (chunk 6.4b3).
pub(crate) enum Source {
    Plain(Stored),
    Gzip(Box<Unpacking>),
}

/// A gzip file being unpacked as a run reads it: its packed bytes read
/// from the store a window at a time and unpacked into the text a page
/// asks for, never past `limit`.
pub(crate) struct Unpacking {
    file: Stored,
    limit: u64,
    unpacker: Unpacker,
    /// The packed bytes read and not yet used, and where the store is read
    /// from next.
    packed: Vec<u8>,
    read_to: u64,
    /// The unpacked text held, its first byte at `held_at`.
    held: Vec<u8>,
    held_at: u64,
    /// Every member is unpacked.
    done: bool,
}

impl Unpacking {
    fn new(file: Stored, limit: u64) -> Unpacking {
        Unpacking {
            file,
            limit,
            unpacker: Unpacker::new(),
            packed: Vec::new(),
            read_to: 0,
            held: Vec::new(),
            held_at: 0,
            done: false,
        }
    }

    /// How much text it has unpacked.
    fn produced(&self) -> u64 {
        self.held_at + self.held.len() as u64
    }

    /// How many packed bytes it has used.
    fn used(&self) -> u64 {
        self.read_to.saturating_sub(self.packed.len() as u64)
    }

    fn too_large(&self) -> Halt {
        Halt::Fail(format!(
            "its file unpacks to more than {}, its `max_unpacked_size:`",
            jadpo_std::files::shown_size(self.limit)
        ))
    }

    /// Up to `most` bytes of text from `at`, and whether the text ends
    /// with them.
    async fn read(
        &mut self,
        ctx: &mut Ctx<'_>,
        at: u64,
        most: u64,
    ) -> Result<(Vec<u8>, bool), Halt> {
        // What was let go is unpacked again from the start.
        if at < self.held_at {
            *self = Unpacking::new(self.file.clone(), self.limit);
        }
        loop {
            let produced = self.produced();
            if at < produced {
                if at >= self.limit {
                    return Err(self.too_large());
                }
                let before = usize::try_from(at - self.held_at).unwrap_or(usize::MAX);
                self.held.drain(..before.min(self.held.len()));
                self.held_at = at;
                let n = most.min(self.held.len() as u64).min(self.limit - at);
                let bytes = self
                    .held
                    .get(..usize::try_from(n).unwrap_or(0))
                    .unwrap_or_default()
                    .to_vec();
                return Ok((bytes, self.done && at + n == produced));
            }
            if self.done {
                return Ok((Vec::new(), true));
            }
            if produced > self.limit {
                return Err(self.too_large());
            }
            self.more(ctx).await?;
        }
    }

    /// Unpacks some more, reading packed bytes from the store as it needs.
    async fn more(&mut self, ctx: &mut Ctx<'_>) -> Result<(), Halt> {
        loop {
            let ends = self.read_to >= self.file.size;
            let before = self.held.len();
            let unpacked = self
                .unpacker
                .unpack(&self.packed, ends, &mut self.held, UNPACKED_CHUNK)
                .map_err(|fault| {
                    Halt::Fail(match fault {
                        GzipFault::Damaged => "its gzip file is damaged".to_string(),
                        GzipFault::Cut => "its gzip file ends before its data does".to_string(),
                    })
                })?;
            self.packed.drain(..unpacked.used.min(self.packed.len()));
            if unpacked.done {
                self.done = true;
                return Ok(());
            }
            if self.held.len() > before || unpacked.used > 0 && !self.packed.is_empty() {
                return Ok(());
            }
            if ends {
                return Err(Halt::Fail("its gzip file ends before its data does".into()));
            }
            let more = read(
                ctx,
                &self.file,
                self.read_to,
                (self.file.size - self.read_to).min(WINDOW),
            )
            .await
            .map_err(|refused| halted(refused, &self.file.store))?;
            if more.is_empty() {
                return Err(Halt::Fail("its file is shorter than its size".into()));
            }
            self.read_to += more.len() as u64;
            self.packed.extend_from_slice(&more);
        }
    }
}

impl Source {
    /// Up to `most` bytes of the file's text from `at`, and whether the
    /// text ends with them; never none unless it has ended.
    async fn read(
        &mut self,
        ctx: &mut Ctx<'_>,
        at: u64,
        most: u64,
    ) -> Result<(Vec<u8>, bool), Halt> {
        match self {
            Source::Plain(file) => {
                if at >= file.size {
                    return Ok((Vec::new(), true));
                }
                let more = read(ctx, file, at, (file.size - at).min(most))
                    .await
                    .map_err(|refused| halted(refused, &file.store))?;
                if more.is_empty() {
                    return Err(Halt::Fail("its file is shorter than its size".into()));
                }
                let ends = at + more.len() as u64 >= file.size;
                Ok((more, ends))
            }
            Source::Gzip(unpacking) => unpacking.read(ctx, at, most).await,
        }
    }

    /// How large the file's text is, or for a gzip file is likely to be,
    /// in bytes: what a run's estimate scales by.
    fn text_size(&self) -> Option<u64> {
        match self {
            Source::Plain(file) => Some(file.size),
            Source::Gzip(unpacking) => {
                let used = unpacking.used();
                (used > 0).then(|| {
                    let size = u128::from(unpacking.file.size) * u128::from(unpacking.produced())
                        / u128::from(used);
                    u64::try_from(size).unwrap_or(u64::MAX)
                })
            }
        }
    }
}

/// The total a file's run estimates once `done` items end `at` bytes into
/// its text: the items scaled by the text's size, never fewer than done.
fn estimated(source: &Source, done: i64, at: u64) -> Option<i64> {
    let size = source.text_size()?;
    if at == 0 || done <= 0 {
        return None;
    }
    let done_wide = u128::try_from(done).ok()?;
    let scaled = (done_wide * u128::from(size) + u128::from(at) / 2) / u128::from(at);
    Some(i64::try_from(scaled).unwrap_or(i64::MAX).max(done))
}

/// A row read as the row type, or its problem's column and rule.
type Decoded = Result<Value, (String, String)>;

/// Why a page stopped short of its batch.
enum Halt {
    /// The run can't go on: it fails, for this reason.
    Fail(String),
    /// Something passing: the run waits.
    Pause(String),
}

fn halted(refused: Unanswered, store: &str) -> Halt {
    match refused {
        Unanswered::Failed(code) if code == jadpo_std::files::NOT_FOUND => {
            Halt::Fail("its file is no longer in its store".into())
        }
        Unanswered::Failed(code) => Halt::Fail(format!(
            "reading its file from `{store}` failed with `{code}`"
        )),
        Unanswered::Failure(failure) => Halt::Pause(format!(
            "reading its file from `{store}` failed with `{}`",
            failure.code
        )),
        Unanswered::Fault(kind) => Halt::Fail(format!(
            "reading its file faulted with `{}`",
            kind.classification()
        )),
    }
}

/// The rows from `from` (beginning on `line`) up to `most`, read from
/// `source` in windows until they are whole or the file ends.
async fn window(
    ctx: &mut Ctx<'_>,
    source: &mut Source,
    (from, line): (u64, u64),
    encoding: Encoding,
    delimiter: Option<u8>,
    most: usize,
) -> Result<(Vec<Record>, Stop), Halt> {
    let mut held: Vec<u8> = Vec::new();
    let mut ends = false;
    loop {
        if !ends {
            let reached = from + held.len() as u64;
            let (more, last) = source.read(ctx, reached, WINDOW).await?;
            ends = last;
            held.extend_from_slice(&more);
        }
        let (rows, stop) = records(&held, from, line, encoding, delimiter, ends, most);
        if stop != Stop::More {
            return Ok((rows, stop));
        }
        if held.len() as u64 >= MAX_ROW_BYTES {
            if rows.is_empty() {
                let at = rows.last().map_or(line, |row| row.next_line);
                return Err(Halt::Fail(format!(
                    "the row beginning on line {at} of its file is longer than {}",
                    jadpo_std::files::shown_size(MAX_ROW_BYTES)
                )));
            }
            return Ok((rows, Stop::Full));
        }
    }
}

/// A JSON document's items, up to `most`, as `walker` walks the text from
/// its place on, reading windows of it until it stops.
async fn walked(
    ctx: &mut Ctx<'_>,
    source: &mut Source,
    walker: &mut Walker,
    most: usize,
) -> Result<(Vec<crate::rows::json::Item>, Walked), Halt> {
    let mut items = Vec::new();
    // What was read past the walker's place, a character a window cut
    // among it.
    let mut pending: Vec<u8> = Vec::new();
    let mut ends = false;
    loop {
        if !ends {
            let (at, _) = walker.place();
            let (more, last) = source.read(ctx, at + pending.len() as u64, WINDOW).await?;
            ends = last;
            pending.extend_from_slice(&more);
        }
        let (used, walk) = walker.walk(&pending, ends, most, &mut items);
        pending.drain(..used.min(pending.len()));
        match walk {
            Walked::More if !ends => {}
            Walked::More => return Ok((items, Walked::Fault(RowFault::Ended))),
            other => return Ok((items, other)),
        }
    }
}

/// Opens the run's file: its encoding from its first bytes and, for CSV
/// and TSV, its header, held to `rows`' row type.
async fn open(
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    rows: &FileRows,
    file: &Value,
) -> Result<Opened, Halt> {
    let Some(file) = stored(file) else {
        return Err(Halt::Fail("its saved file isn't a stored `File`".into()));
    };
    let first = if file.size == 0 {
        Vec::new()
    } else {
        read(ctx, &file, 0, OPENING_BYTES.min(file.size))
            .await
            .map_err(|refused| halted(refused, &file.store))?
    };
    // A gzip file is unpacked as it is read, up to `max_unpacked_size:`.
    let (mut source, first) = match (opening(&first), rows.max_unpacked) {
        (Opening::Gzip, None) => {
            return Err(Halt::Fail(format!(
                "its file is gzip, which `{}.read` reads only when given `max_unpacked_size:`",
                rows.format.namespace()
            )));
        }
        (Opening::Gzip, Some(limit)) => {
            let mut source = Source::Gzip(Box::new(Unpacking::new(file, limit)));
            let mut first: Vec<u8> = Vec::new();
            loop {
                let (more, ends) = source
                    .read(ctx, first.len() as u64, OPENING_BYTES - first.len() as u64)
                    .await?;
                first.extend_from_slice(&more);
                if ends || first.len() as u64 >= OPENING_BYTES {
                    break;
                }
            }
            (source, first)
        }
        (Opening::Text { .. }, _) => (Source::Plain(file), first),
    };
    // What a gzip file unpacks to is text, never gzip again.
    let (encoding, start) = match opening(&first) {
        Opening::Gzip => (Encoding::Utf8, 0),
        Opening::Text { encoding, start } => (encoding, start.min(first.len() as u64)),
    };
    let codec = batches
        .json
        .codec_of(rows.row)
        .ok_or_else(|| Halt::Fail("its row type has no codec".into()))?;
    let (ty, described) = batches
        .json
        .codecs
        .input_fields(codec)
        .ok_or_else(|| Halt::Fail("its row type isn't an `Input`".into()))?;
    let ty = ty.rsplit('.').next().unwrap_or(&ty).to_string();
    let fields: Vec<String> = described.iter().map(|(name, _)| name.clone()).collect();
    let names: Vec<String> = fields
        .iter()
        .enumerate()
        .map(|(slot, name)| rows.columns.get(slot).unwrap_or(name).clone())
        .collect();
    // JSON: the walk to its list of items, whose first item is row 1.
    if rows.format == RowFormat::Json {
        let longest = usize::try_from(MAX_ROW_BYTES).unwrap_or(usize::MAX);
        let mut walker = Walker::new(&rows.items_at, encoding, start, longest);
        let (_, walk) = walked(ctx, &mut source, &mut walker, usize::MAX).await?;
        match walk {
            Walked::Opened => {}
            Walked::Fault(fault) => return Err(Halt::Fail(fault.why())),
            _ => return Err(Halt::Fail(RowFault::Ended.why())),
        }
        let (at, line) = walker.place();
        return Ok(Opened {
            source,
            encoding,
            first: Position { at, row: 1, line },
            columns: Vec::new(),
            fields,
            names,
            codec,
            walker: Some(walker),
        });
    }
    let Some(delimiter) = rows.format.delimiter() else {
        return Ok(Opened {
            source,
            encoding,
            first: Position {
                at: start,
                row: 1,
                line: 1,
            },
            columns: Vec::new(),
            fields,
            names,
            codec,
            walker: None,
        });
    };
    let (header, stop) = window(ctx, &mut source, (start, 1), encoding, Some(delimiter), 1).await?;
    if let (None, Stop::Fault(fault)) = (header.first(), &stop) {
        return Err(Halt::Fail(fault.why()));
    }
    let (cells, after) = match header.first() {
        Some(row) => (
            row.cells.clone(),
            Position {
                at: row.next,
                row: 2,
                line: row.next_line,
            },
        ),
        None => (
            Vec::new(),
            Position {
                at: start,
                row: 2,
                line: 1,
            },
        ),
    };
    let mut columns = Vec::with_capacity(cells.len());
    for cell in cells
        .iter()
        .filter(|cell| !(cells.len() == 1 && cell.is_empty()))
    {
        let Some(slot) = names.iter().position(|name| name == cell) else {
            return Err(Halt::Fail(format!(
                "its file's column `{cell}` is no field of `{ty}`"
            )));
        };
        if columns.contains(&slot) {
            return Err(Halt::Fail(format!("its file names column `{cell}` twice")));
        }
        columns.push(slot);
    }
    for (slot, (field, absent)) in described.iter().enumerate() {
        if !absent && !columns.contains(&slot) {
            let column = names.get(slot).unwrap_or(field);
            return Err(Halt::Fail(format!(
                "its file has no column `{column}`, which `{ty}.{field}` is read from"
            )));
        }
    }
    Ok(Opened {
        source,
        encoding,
        first: after,
        columns,
        fields,
        names,
        codec,
        walker: None,
    })
}

/// One row read as the row type: its value, or its problem's column and
/// rule, never its value.
fn decode(batches: Batches<'_>, opened: &Opened, format: RowFormat, record: &Record) -> Decoded {
    let codecs = &batches.json.codecs;
    if format == RowFormat::JsonLines {
        let line = record.cells.first().map_or("", String::as_str);
        return decode_json(batches, opened.codec, line);
    }
    if record.cells.len() > opened.columns.len() {
        return Err((String::new(), "unknown_field".to_string()));
    }
    let mut cells: Vec<Option<&str>> = vec![None; opened.fields.len()];
    for (cell, slot) in record.cells.iter().zip(&opened.columns) {
        if let Some(place) = cells.get_mut(*slot)
            && !cell.is_empty()
        {
            *place = Some(cell.as_str());
        }
    }
    codecs.read_cells(opened.codec, &cells).map_err(|problem| {
        let column = opened
            .fields
            .iter()
            .position(|field| *field == problem.field)
            .and_then(|slot| opened.names.get(slot))
            .cloned()
            .unwrap_or_default();
        (column, problem.rule)
    })
}

/// One JSON value, a JSON Lines line or an item of a JSON list, read as
/// the row type: its value, or its problem's field path and rule.
fn decode_json(batches: Batches<'_>, codec: CodecId, text: &str) -> Decoded {
    match batches.json.codecs.read_value(codec, &Value::text(text)) {
        Ok(value) => Ok(value),
        Err(jadpo_value::json::Unread::Malformed) => Err((String::new(), "malformed".to_string())),
        Err(jadpo_value::json::Unread::Mismatch(problem)) => Err((problem.field, problem.rule)),
    }
}

/// A page's rows, each its number and its value or its problem; the
/// position after them; whether every row is read; and why it stopped
/// short, when the file can't be read on from there.
struct Read {
    items: Vec<(i64, Decoded)>,
    next: Position,
    read_all: bool,
    fault: Option<String>,
}

/// The rows of a CSV, TSV or JSON Lines file from `from`, at most `most`.
async fn read_records(
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    opened: &mut Opened,
    format: RowFormat,
    from: Position,
    most: usize,
) -> Result<Read, Halt> {
    let (read, stop) = window(
        ctx,
        &mut opened.source,
        (from.at, from.line),
        opened.encoding,
        format.delimiter(),
        most,
    )
    .await?;
    let mut items: Vec<(i64, Decoded)> = Vec::new();
    for (place, record) in read.iter().enumerate() {
        if record.blank {
            continue;
        }
        let row = match format {
            RowFormat::JsonLines => i64::try_from(record.line).unwrap_or(i64::MAX),
            _ => from.row + place as i64,
        };
        items.push((row, decode(batches, opened, format, record)));
    }
    let next = match read.last() {
        Some(last) => Position {
            at: last.next,
            row: from.row + read.len() as i64,
            line: last.next_line,
        },
        None => from,
    };
    Ok(Read {
        items,
        next,
        read_all: matches!(stop, Stop::End),
        fault: match stop {
            Stop::Fault(fault) => Some(fault.why()),
            _ => None,
        },
    })
}

/// The items of a JSON file's list from `from`, at most `most`, each row
/// its place in the list: walked from a copy of the walk that opened it,
/// set to the position, and on to the document's end once the list
/// closes (chunk 6.4b3).
async fn read_items(
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    opened: &mut Opened,
    from: Position,
    most: usize,
) -> Result<Read, Halt> {
    let Some(opening) = &opened.walker else {
        return Err(Halt::Fail("its list wasn't found as it was opened".into()));
    };
    let mut walker = opening.at_item(from.at, from.line, from.row > 1);
    let (found, walk) = walked(ctx, &mut opened.source, &mut walker, most).await?;
    let items: Vec<(i64, Decoded)> = found
        .iter()
        .enumerate()
        .map(|(place, item)| {
            (
                from.row + place as i64,
                decode_json(batches, opened.codec, &item.text),
            )
        })
        .collect();
    let next = match found.last() {
        Some(last) => Position {
            at: last.next,
            row: from.row + found.len() as i64,
            line: last.next_line,
        },
        None => from,
    };
    Ok(Read {
        items,
        next,
        read_all: walk == Walked::End,
        fault: match walk {
            Walked::Fault(fault) => Some(fault.why()),
            _ => None,
        },
    })
}

/// One batch of a file helper's run at `now`, its lease renewed until
/// `lease_until` as its position commits: the next rows, read before the
/// batch's transaction opens, or, once every row is read, `finished:`.
#[allow(clippy::too_many_arguments)]
pub(super) async fn page(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    rows: &FileRows,
    start: &BatchStart,
    saved: Saved<'_>,
    opened: &mut Option<Opened>,
    (now, lease_until): (i64, i64),
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let Some(database) = ctx.host().database(&plan.store) else {
        return Step::Ended(Ran::Paused(format!("no store called `{}`", plan.store)));
    };
    let placed = match database.query_once(PLACE, std::slice::from_ref(&id)).await {
        Ok(rows) => rows,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    let (position, read_all) = match placed.first().map(Vec::as_slice) {
        Some([SqlValue::Text(state), position, SqlValue::Bool(read_all)])
            if state == BatchState::Running.name() =>
        {
            (
                match position {
                    SqlValue::Text(text) => Some(text.clone()),
                    _ => None,
                },
                *read_all,
            )
        }
        _ => return Step::Ended(Ran::NotClaimed),
    };
    // Every row read: `finished:`, as any run's last batch.
    if read_all {
        return batch(
            executor,
            ctx,
            batches,
            plan,
            start,
            saved,
            (now, lease_until),
        )
        .await;
    }
    if opened.is_none() {
        let Some(file) = saved.values.first() else {
            return failed(ctx, start, "it saved no file").await;
        };
        match open(ctx, batches, rows, file).await {
            Ok(file) => *opened = Some(file),
            Err(Halt::Fail(why)) => return failed(ctx, start, &why).await,
            Err(Halt::Pause(why)) => return Step::Ended(Ran::Paused(why)),
        }
    }
    let Some(opened) = opened.as_mut() else {
        return Step::Ended(Ran::Paused("its file couldn't be opened".into()));
    };
    let from = match &position {
        Some(text) => match Position::read(text) {
            Some(position) => position,
            None => return failed(ctx, start, "its position isn't one the runtime wrote").await,
        },
        None => opened.first,
    };
    let most = usize::try_from(ctx.host().rows_per_batch().max(1)).unwrap_or(1);
    let read = match rows.format {
        RowFormat::Json => read_items(ctx, batches, opened, from, most).await,
        format => read_records(ctx, batches, opened, format, from, most).await,
    };
    let Read {
        items,
        next,
        read_all,
        fault,
    } = match read {
        Ok(read) => read,
        Err(Halt::Fail(why)) => return failed(ctx, start, &why).await,
        Err(Halt::Pause(why)) => return Step::Ended(Ran::Paused(why)),
    };
    // A fault with no row before it ends the run here; with rows before
    // it, they are done first, and the next page meets it.
    if let (true, Some(why)) = (next == from, &fault) {
        return failed(ctx, start, why).await;
    }
    // The batch's transaction, now that nothing more goes out.
    let mut open = match Batch::open(ctx, &plan.store, plan.isolation, saved.system).await {
        Ok(open) => open,
        Err(StoreError::ConcurrentChange) => return Step::Changed,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    let locked = match open.transaction() {
        Some(transaction) => transaction.query(LOCK, std::slice::from_ref(&id)).await,
        None => Err(StoreError::Fault("a batch without its transaction".into())),
    };
    let (done, had) = match locked {
        Ok(found) => match found.first().map(Vec::as_slice) {
            Some(
                [
                    SqlValue::Text(state),
                    held,
                    SqlValue::Bool(false),
                    SqlValue::Int8(done),
                    SqlValue::Int8(problems),
                    SqlValue::Bool(cancelled),
                ],
            ) if state == BatchState::Running.name() => {
                let held = match held {
                    SqlValue::Text(text) => Some(text.clone()),
                    _ => None,
                };
                // Another runner moved it on since its place was read.
                if held != position {
                    open.rollback().await;
                    return Step::Changed;
                }
                if *cancelled {
                    let ended = end_in(&mut open, &id, BatchState::Cancelled, None, now).await;
                    return finish(open, ended, Ran::Cancelled).await;
                }
                (*done, *problems)
            }
            _ => {
                open.rollback().await;
                return Step::Ended(Ran::NotClaimed);
            }
        },
        Err(StoreError::ConcurrentChange) => {
            open.rollback().await;
            return Step::Changed;
        }
        Err(error) => {
            open.rollback().await;
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    let Some(each) = plan.each.action() else {
        open.rollback().await;
        return Step::Ended(Ran::Failed("a run with no action to call".into()));
    };
    // The rows read, each its action's item; the rest are problems.
    let mut problems: Vec<(i64, String, String)> = Vec::new();
    let mut called: Vec<i64> = Vec::new();
    let mut calls: Vec<Vec<Value>> = Vec::new();
    for (row, item) in &items {
        match item {
            Ok(value) => {
                called.push(*row);
                calls.push(arguments(&each.args, value, saved.values));
            }
            Err((column, rule)) => problems.push((*row, column.clone(), rule.clone())),
        }
    }
    let mut paused: Option<String> = None;
    open.items(
        executor,
        each.callable,
        calls,
        &mut |at, outcome| match problem(&outcome) {
            Item::Done => ControlFlow::Continue(()),
            Item::Problem(rule) => {
                if let Some(row) = called.get(at) {
                    problems.push((*row, String::new(), rule));
                }
                ControlFlow::Continue(())
            }
            Item::Pause(why) => {
                paused = Some(why);
                ControlFlow::Break(())
            }
        },
    )
    .await;
    if open.changed() {
        open.rollback().await;
        return Step::Changed;
    }
    if let Some(why) = paused {
        open.rollback().await;
        return Step::Ended(Ran::Paused(why));
    }
    problems.sort_by_key(|(row, _, _)| *row);
    let Some(transaction) = open.transaction() else {
        return Step::Ended(Ran::Paused("a batch without its transaction".into()));
    };
    for (row, column, rule) in &problems {
        let written = transaction
            .execute(
                PROBLEM,
                &[
                    id.clone(),
                    SqlValue::Int8(*row),
                    SqlValue::Text(column.clone()),
                    SqlValue::Text(rule.clone()),
                ],
            )
            .await;
        if let Err(error) = written {
            return abandoned(open, error).await;
        }
    }
    // The total it estimates from how far into its file its items end.
    let done = done + items.len() as i64;
    let estimate = if read_all {
        Some(done)
    } else {
        estimated(&opened.source, done, next.at)
    };
    let advanced = transaction
        .execute(
            ADVANCE,
            &[
                id.clone(),
                SqlValue::Text(next.text()),
                SqlValue::Bool(read_all),
                SqlValue::Int8(done),
                SqlValue::Int8(had + problems.len() as i64),
                at(lease_until),
                at(now),
                total(estimate),
            ],
        )
        .await;
    if let Err(error) = advanced {
        return abandoned(open, error).await;
    }
    match open.commit().await {
        Ok(()) => Step::Next,
        Err(StoreError::ConcurrentChange) => Step::Changed,
        Err(error) => Step::Ended(Ran::Paused(format!(
            "its batch couldn't commit ({error:?})"
        ))),
    }
}

/// Ends the run `failed`, for `why`, in a statement of its own: its file
/// can't be read on from where it is.
async fn failed(ctx: &mut Ctx<'_>, start: &BatchStart, why: &str) -> Step {
    super::end(ctx.host(), start, BatchState::Failed, Some(why)).await;
    Step::Ended(Ran::Failed(why.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_is_kept_as_json_and_read_back() {
        let position = Position {
            at: 1234,
            row: 57,
            line: 60,
        };
        assert_eq!(position.text(), "{\"at\":1234,\"row\":57,\"line\":60}");
        assert_eq!(Position::read(&position.text()), Some(position));
        assert_eq!(Position::read("abc"), None);
    }
}
