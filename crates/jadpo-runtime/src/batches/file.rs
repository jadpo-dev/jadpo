//! A file helper's run (area 12, "Batches"; area 19, "Data formats";
//! chunk 6.4b): `csv.read`, `tsv.read` or `json_lines.read`, walking a
//! stored `File`'s rows a batch at a time.
//!
//! An item's action runs inside its batch's transaction, and no outside
//! call goes out while a transaction is open (chunk 7.0c), so each page's
//! rows are read from the store first, through the capability seam, from
//! the position the last page committed ([`PLACE`]); only then does the
//! batch open its transaction, lock the run's row, hold it to the same
//! position, run the rows' items and commit their writes with the problems
//! and the position after the last row read. A runner reads the file's
//! opening once per invocation ([`Opened`]): its encoding and, for CSV and
//! TSV, its header, held to the row type's fields.
//!
//! A row that can't be read as the row type is a problem with its row (as
//! a spreadsheet numbers it, the header row 1, or for JSON Lines its line)
//! and its column (the header's, or the JSON field's path), never its
//! value; so is a row whose action fails. What can't be read on from
//! stops the run, before or at the place it is: a missing or unknown
//! column, a quote that never closes, text that isn't text, a gzip file,
//! a row past [`MAX_ROW_BYTES`]. The rows before it are done.

use std::ops::ControlFlow;

use jadpo_std::BatchState;
use jadpo_std::batch::{BatchPlan, FileRows, RowFormat};
use jadpo_std::http::CodecId;
use jadpo_value::Value;

use super::{
    ADVANCE, Batches, Item, LOCK, PROBLEM, Ran, Saved, Step, abandoned, arguments, at, batch,
    end_in, finish, problem,
};
use crate::effects::Unanswered;
use crate::files::{Stored, WINDOW, read, stored};
use crate::rows::{Encoding, OPENING_BYTES, Opening, Record, Stop, opening, records};
use crate::runner::Batch;
use crate::{Ctx, Executor, SqlValue, StoreError};

use super::BatchStart;

/// Where a run has got to, read before its page's transaction opens.
pub(crate) const PLACE: &str =
    "SELECT \"state\", \"position\", \"read_all\" FROM \"jadpo_batch_runs\" WHERE \"id\" = $1";

/// The most bytes a page reads looking for its rows: past it, a page takes
/// the rows it has, and a row longer than it stops the run.
pub const MAX_ROW_BYTES: u64 = 16 << 20;

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
    file: Stored,
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
}

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
/// `file` in windows until they are whole or the file ends.
async fn window(
    ctx: &mut Ctx<'_>,
    file: &Stored,
    (from, line): (u64, u64),
    encoding: Encoding,
    delimiter: Option<u8>,
    most: usize,
) -> Result<(Vec<Record>, Stop), Halt> {
    let mut held: Vec<u8> = Vec::new();
    loop {
        let reached = from + held.len() as u64;
        if reached < file.size {
            let more = read(ctx, file, reached, (file.size - reached).min(WINDOW))
                .await
                .map_err(|refused| halted(refused, &file.store))?;
            if more.is_empty() {
                return Err(Halt::Fail("its file is shorter than its size".into()));
            }
            held.extend_from_slice(&more);
        }
        let ends = from + held.len() as u64 >= file.size;
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
    let (encoding, start) = match opening(&first) {
        Opening::Gzip => {
            return Err(Halt::Fail(format!(
                "its file is gzip, which `{}.read` reads only when given `max_unpacked_size:`",
                rows.format.namespace()
            )));
        }
        Opening::Text { encoding, start } => (encoding, start.min(file.size)),
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
    let Some(delimiter) = rows.format.delimiter() else {
        return Ok(Opened {
            file,
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
        });
    };
    let (header, stop) = window(ctx, &file, (start, 1), encoding, Some(delimiter), 1).await?;
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
        file,
        encoding,
        first: after,
        columns,
        fields,
        names,
        codec,
    })
}

/// One row read as the row type: its value, or its problem's column and
/// rule, never its value.
fn decode(
    batches: Batches<'_>,
    opened: &Opened,
    format: RowFormat,
    record: &Record,
) -> Result<Value, (String, String)> {
    let codecs = &batches.json.codecs;
    if format == RowFormat::JsonLines {
        let line = record.cells.first().map_or("", String::as_str);
        return match codecs.read_value(opened.codec, &Value::text(line)) {
            Ok(value) => Ok(value),
            Err(jadpo_value::json::Unread::Malformed) => {
                Err((String::new(), "malformed".to_string()))
            }
            Err(jadpo_value::json::Unread::Mismatch(problem)) => Err((problem.field, problem.rule)),
        };
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
    let Some(opened) = opened.as_ref() else {
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
    let read = window(
        ctx,
        &opened.file,
        (from.at, from.line),
        opened.encoding,
        rows.format.delimiter(),
        most,
    )
    .await;
    let (read, stop) = match read {
        Ok(read) => read,
        Err(Halt::Fail(why)) => return failed(ctx, start, &why).await,
        Err(Halt::Pause(why)) => return Step::Ended(Ran::Paused(why)),
    };
    if let (true, Stop::Fault(fault)) = (read.is_empty(), &stop) {
        return failed(ctx, start, &fault.why()).await;
    }
    // Each row's number, and its value or its problem.
    let mut items: Vec<(i64, Result<Value, (String, String)>)> = Vec::new();
    for (place, record) in read.iter().enumerate() {
        if record.blank {
            continue;
        }
        let row = match rows.format {
            RowFormat::JsonLines => i64::try_from(record.line).unwrap_or(i64::MAX),
            _ => from.row + place as i64,
        };
        items.push((row, decode(batches, opened, rows.format, record)));
    }
    let next = match read.last() {
        Some(last) => Position {
            at: last.next,
            row: from.row + read.len() as i64,
            line: last.next_line,
        },
        None => from,
    };
    let read_all = matches!(stop, Stop::End);
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
    // The rows read, each its action's item; the rest are problems.
    let mut problems: Vec<(i64, String, String)> = Vec::new();
    let mut called: Vec<i64> = Vec::new();
    let mut calls: Vec<Vec<Value>> = Vec::new();
    for (row, item) in &items {
        match item {
            Ok(value) => {
                called.push(*row);
                calls.push(arguments(&plan.each.args, value, saved.values));
            }
            Err((column, rule)) => problems.push((*row, column.clone(), rule.clone())),
        }
    }
    let mut paused: Option<String> = None;
    open.items(
        executor,
        plan.each.callable,
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
    let advanced = transaction
        .execute(
            ADVANCE,
            &[
                id.clone(),
                SqlValue::Text(next.text()),
                SqlValue::Bool(read_all),
                SqlValue::Int8(done + items.len() as i64),
                SqlValue::Int8(had + problems.len() as i64),
                at(lease_until),
                at(now),
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
