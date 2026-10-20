//! A file helper's write (area 12, "How an export's run is kept"; area 19,
//! "Data formats"; chunk 6.4b2): `csv.write`, `tsv.write`,
//! `json_lines.write` or `json.write`'s file form, writing a named query's
//! pages into one stored file.
//!
//! No outside call goes out while a transaction is open (chunk 7.0c), so
//! each page goes in three steps. Its transaction locks the run's row,
//! reads the page after the position that committed, and commits, writing
//! nothing. Then, with no transaction open, each item's row is worked out
//! by `each:` ([`crate::Executor::row`]) and the page's rows are put in the
//! store as a piece of the file of their own, under a key made from the
//! run's id and the piece's number ([`crate::files::put_piece`]). Last, a
//! second transaction locks the run's row again, holds it to the position
//! the page was read from, and commits the problems and the position after
//! the page. A run stopped between the two puts the piece again when it
//! resumes, under the same key, so the file never holds a row twice. Once
//! every page is written the pieces are joined into the file
//! ([`crate::files::join`]), outside any transaction, and `finished:` runs
//! in a transaction of its own, given the `ExportSummary` holding it. A
//! cancelled run's pieces are dropped once its end commits.
//!
//! A CSV or TSV file starts with its header, the row type's columns, and
//! a spreadsheet's with a UTF-8 byte-order mark; every row ends with CRLF.
//! A cell holding the delimiter, a quote or a line break is quoted, its
//! quotes doubled (RFC 4180), and for a spreadsheet a text cell a
//! spreadsheet would read as a formula is written with `'` before it. A
//! JSON Lines file is each row's JSON and a line feed; a JSON file one
//! list of the rows.

use jadpo_std::BatchState;
use jadpo_std::CsvFormat;
use jadpo_std::batch::{BatchEach, BatchPlan, FileWrite, RowFormat};
use jadpo_value::Value;

use super::{
    ADVANCE, Batches, Held, Item, LOCK, PROBLEM, Ran, Saved, Step, Unread, abandoned, at, end_in,
    finish, finished, problem, read_page,
};
use crate::effects::Unanswered;
use crate::runner::Batch;
use crate::{Ctx, Executor, Outcome, SqlValue, StoreError};

use super::BatchStart;

/// Where an export has got to, kept as JSON text in its run's row: the
/// cursor of the next page, how many pieces of the file are kept, their
/// bytes, and how many rows they hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Place {
    after: Option<String>,
    pieces: u64,
    bytes: u64,
    rows: u64,
}

impl Place {
    fn text(&self) -> String {
        serde_json::json!({
            "after": self.after,
            "pieces": self.pieces,
            "bytes": self.bytes,
            "rows": self.rows,
        })
        .to_string()
    }

    fn read(text: &str) -> Option<Place> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        Some(Place {
            after: match value.get("after")? {
                serde_json::Value::Null => None,
                serde_json::Value::String(cursor) => Some(cursor.clone()),
                _ => return None,
            },
            pieces: value.get("pieces")?.as_u64()?,
            bytes: value.get("bytes")?.as_u64()?,
            rows: value.get("rows")?.as_u64()?,
        })
    }
}

/// One cell, quoted when it holds the delimiter, a quote or a line break;
/// with `escape`, text a spreadsheet would read as a formula written with
/// `'` before it (OWASP's advice against CSV injection).
fn cell(text: &str, delimiter: char, escape: bool) -> String {
    let mut held = String::with_capacity(text.len() + 1);
    if escape && text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        held.push('\'');
    }
    held.push_str(text);
    if held.contains([delimiter, '"', '\r', '\n']) {
        format!("\"{}\"", held.replace('"', "\"\""))
    } else {
        held
    }
}

/// What a piece of the file holds: a CSV's or a JSON list's opening when
/// it is the first, `rows`, and a JSON list's end when it is the `last`.
/// `before` rows are in the pieces before it.
struct Piece<'a> {
    first: bool,
    last: bool,
    before: u64,
    rows: &'a [Value],
}

/// The bytes of `piece`, written as `write` says, `spreadsheet` for a CSV
/// or TSV a spreadsheet reads; why not, never a value.
fn encode(
    batches: Batches<'_>,
    write: &FileWrite,
    spreadsheet: bool,
    piece: &Piece<'_>,
) -> Result<Vec<u8>, String> {
    let codecs = &batches.json.codecs;
    let codec = batches
        .json
        .codec_of(write.row)
        .ok_or("its row type has no codec")?;
    let mut out = String::new();
    match write.format {
        RowFormat::Csv | RowFormat::Tsv => {
            let delimiter = if write.format == RowFormat::Csv {
                ','
            } else {
                '\t'
            };
            let line = |cells: Vec<String>, out: &mut String| {
                out.push_str(&cells.join(&delimiter.to_string()));
                out.push_str("\r\n");
            };
            if piece.first {
                if spreadsheet {
                    out.push('\u{feff}');
                }
                let header = write
                    .columns
                    .iter()
                    .map(|column| cell(column, delimiter, false))
                    .collect();
                line(header, &mut out);
            }
            for row in piece.rows {
                let cells = codecs
                    .write_cells(codec, row)
                    .ok_or("a row that isn't its type")?
                    .into_iter()
                    .map(|(text, is_text)| match text {
                        Some(text) => cell(&text, delimiter, spreadsheet && is_text),
                        None => String::new(),
                    })
                    .collect();
                line(cells, &mut out);
            }
        }
        RowFormat::JsonLines => {
            for row in piece.rows {
                codecs.write(codec, row, &mut out);
                out.push('\n');
            }
        }
        RowFormat::Json => {
            if piece.first {
                out.push('[');
            }
            for (at, row) in piece.rows.iter().enumerate() {
                if piece.before + at as u64 > 0 {
                    out.push(',');
                }
                codecs.write(codec, row, &mut out);
            }
            if piece.last {
                out.push(']');
            }
        }
    }
    Ok(out.into_bytes())
}

/// What a store's refusal of a piece, or of the file's join, does to the
/// run: one with no room, or that can't be reached, pauses it, so it is
/// carried on later; anything else ends it.
fn refused(what: &str, store: &str, refusal: Unanswered) -> Ran {
    match refusal {
        Unanswered::Failed(code) if code == jadpo_std::files::STORAGE_FULL => {
            Ran::Paused(format!("`{store}` had no room for {what}"))
        }
        Unanswered::Failed(code) if code == jadpo_std::files::NOT_FOUND => {
            Ran::Failed(format!("a piece of its file is no longer in `{store}`"))
        }
        Unanswered::Failed(code) => Ran::Failed(format!("`{store}` refused {what} with `{code}`")),
        Unanswered::Failure(failure) => {
            Ran::Paused(format!("`{store}` refused {what} with `{}`", failure.code))
        }
        Unanswered::Fault(kind) => Ran::Failed(format!(
            "putting {what} in `{store}` faulted with `{}`",
            kind.classification()
        )),
    }
}

/// What the run's row held, locked in a batch's transaction, or the step
/// that ends the batch.
enum Locked {
    Held(Held),
    /// `batches.cancel` asked it to stop: where it had got to.
    Cancelled(Option<String>),
    Step(Step),
}

/// Locks the run's row in `open`'s transaction.
async fn lock(open: &mut Batch<'_, '_>, id: &SqlValue) -> Result<Locked, StoreError> {
    let Some(transaction) = open.transaction() else {
        return Err(StoreError::Fault("a batch without its transaction".into()));
    };
    let rows = transaction.query(LOCK, std::slice::from_ref(id)).await?;
    Ok(match rows.first().map(Vec::as_slice) {
        Some(
            [
                SqlValue::Text(state),
                position,
                SqlValue::Bool(read_all),
                SqlValue::Int8(done),
                SqlValue::Int8(problems),
                SqlValue::Bool(cancelled),
            ],
        ) if state == BatchState::Running.name() => {
            let position = match position {
                SqlValue::Text(text) => Some(text.clone()),
                _ => None,
            };
            if *cancelled {
                Locked::Cancelled(position)
            } else {
                Locked::Held(Held {
                    position,
                    read_all: *read_all,
                    done: *done,
                    problems: *problems,
                })
            }
        }
        _ => Locked::Step(Step::Ended(Ran::NotClaimed)),
    })
}

/// One step of an export at `now`, its lease renewed until `lease_until`
/// as its position commits: the next page written as a piece of the file,
/// or, once every page is, the pieces joined and `finished:` run.
#[allow(clippy::too_many_arguments)]
pub(super) async fn page(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    batches: Batches<'_>,
    plan: &BatchPlan,
    write: &FileWrite,
    start: &BatchStart,
    saved: Saved<'_>,
    (now, lease_until): (i64, i64),
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let BatchEach::Row(row) = &plan.each else {
        return Step::Ended(Ran::Failed("an export with no row to write".into()));
    };
    // The page, read in a transaction of its own.
    let mut open = match Batch::open(ctx, &plan.store, plan.isolation, saved.system).await {
        Ok(open) => open,
        Err(StoreError::ConcurrentChange) => return Step::Changed,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    let held = match lock(&mut open, &id).await {
        Ok(Locked::Held(held)) => held,
        Ok(Locked::Cancelled(position)) => {
            let ended = end_in(&mut open, &id, BatchState::Cancelled, None, now).await;
            let step = finish(open, ended, Ran::Cancelled).await;
            return dropped(ctx, write, start, position.as_deref(), step).await;
        }
        Ok(Locked::Step(step)) => {
            open.rollback().await;
            return step;
        }
        Err(error) => return abandoned(open, error).await,
    };
    let place = match held.position.as_deref().map(Place::read) {
        None => Place::default(),
        Some(Some(place)) => place,
        Some(None) => {
            let why = "its position isn't one the runtime wrote".to_string();
            let ended = end_in(&mut open, &id, BatchState::Failed, Some(&why), now).await;
            return finish(open, ended, Ran::Failed(why)).await;
        }
    };
    if held.read_all {
        open.rollback().await;
        return written(executor, ctx, plan, write, start, saved, &held, &place, now).await;
    }
    let (items, next) = match read_page(
        &mut open,
        batches,
        plan,
        saved.values,
        place.after.as_deref(),
    )
    .await
    {
        Ok(page) => page,
        Err(Unread::Changed) => {
            open.rollback().await;
            return Step::Changed;
        }
        Err(Unread::Paused(why)) => {
            open.rollback().await;
            return Step::Ended(Ran::Paused(why));
        }
        Err(Unread::Refused(why)) => {
            let ended = end_in(&mut open, &id, BatchState::Failed, Some(&why), now).await;
            return finish(open, ended, Ran::Failed(why)).await;
        }
    };
    // The page's transaction commits, having written nothing, before
    // anything goes out.
    match open.commit().await {
        Ok(()) => {}
        Err(StoreError::ConcurrentChange) => return Step::Changed,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "its page's read couldn't commit ({error:?})"
            )));
        }
    }
    // Each item's row, with no transaction open; one that faults is its
    // item's problem, by its place in the walk.
    let mut rows: Vec<Value> = Vec::with_capacity(items.len());
    let mut problems: Vec<(i64, String)> = Vec::new();
    for (at, item) in items.iter().enumerate() {
        match executor.row(ctx, row, item.clone(), saved.values).await {
            Outcome::Value(value) => rows.push(value),
            other => match problem(&other) {
                Item::Done => {}
                Item::Problem(rule) => problems.push((held.done + at as i64 + 1, rule)),
                Item::Pause(why) => return Step::Ended(Ran::Paused(why)),
            },
        }
    }
    let spreadsheet = write
        .csv_format
        .and_then(|at| saved.values.get(at as usize))
        .is_some_and(|format| {
            matches!(format, Value::Variant(variant) if *variant.name == *CsvFormat::Spreadsheet.name())
        });
    let piece = Piece {
        first: place.pieces == 0,
        last: next.is_none(),
        before: place.rows,
        rows: &rows,
    };
    let bytes = match encode(batches, write, spreadsheet, &piece) {
        Ok(bytes) => bytes,
        Err(why) => {
            let why = format!("its rows couldn't be written: {why}");
            super::end(ctx.host(), start, BatchState::Failed, Some(&why)).await;
            return Step::Ended(Ran::Failed(why));
        }
    };
    let size = bytes.len() as u64;
    let media = crate::files::media_of(write.format.file_type()).unwrap_or("text/plain");
    if let Err(refusal) =
        crate::files::put_piece(ctx, &write.store, (&start.run, place.pieces), media, bytes).await
    {
        let ran = refused("a piece of its file", &write.store, refusal);
        if let Ran::Failed(why) = &ran {
            super::end(ctx.host(), start, BatchState::Failed, Some(why)).await;
        }
        return Step::Ended(ran);
    }
    // The problems and the position after the page, in a transaction of
    // their own, held to the position the page was read from.
    let mut open = match Batch::open(ctx, &plan.store, plan.isolation, saved.system).await {
        Ok(open) => open,
        Err(StoreError::ConcurrentChange) => return Step::Changed,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    match lock(&mut open, &id).await {
        Ok(Locked::Held(again)) if again.position == held.position && !again.read_all => {}
        Ok(Locked::Held(_)) => {
            // Another runner moved it on since its page was read.
            open.rollback().await;
            return Step::Changed;
        }
        Ok(Locked::Cancelled(position)) => {
            let ended = end_in(&mut open, &id, BatchState::Cancelled, None, now).await;
            let step = finish(open, ended, Ran::Cancelled).await;
            return dropped(ctx, write, start, position.as_deref(), step).await;
        }
        Ok(Locked::Step(step)) => {
            open.rollback().await;
            return step;
        }
        Err(error) => return abandoned(open, error).await,
    }
    let Some(transaction) = open.transaction() else {
        return Step::Ended(Ran::Paused("a batch without its transaction".into()));
    };
    for (row, rule) in &problems {
        let kept = transaction
            .execute(
                PROBLEM,
                &[
                    id.clone(),
                    SqlValue::Int8(*row),
                    SqlValue::Text(String::new()),
                    SqlValue::Text(rule.clone()),
                ],
            )
            .await;
        if let Err(error) = kept {
            return abandoned(open, error).await;
        }
    }
    let after = Place {
        after: next.clone(),
        pieces: place.pieces + 1,
        bytes: place.bytes + size,
        rows: place.rows + rows.len() as u64,
    };
    let advanced = transaction
        .execute(
            ADVANCE,
            &[
                id.clone(),
                SqlValue::Text(after.text()),
                SqlValue::Bool(next.is_none()),
                SqlValue::Int8(held.done + items.len() as i64),
                SqlValue::Int8(held.problems + problems.len() as i64),
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

/// Every page written: the pieces joined into the file, with no
/// transaction open, then `finished:` given the `ExportSummary` holding
/// it, in a transaction of its own, and the run finished.
#[allow(clippy::too_many_arguments)]
async fn written(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: &BatchPlan,
    write: &FileWrite,
    start: &BatchStart,
    saved: Saved<'_>,
    held: &Held,
    place: &Place,
    now: i64,
) -> Step {
    let id = SqlValue::Uuid(start.run);
    let media = crate::files::media_of(write.format.file_type()).unwrap_or("text/plain");
    if let Err(refusal) =
        crate::files::join(ctx, &write.store, &start.run, place.pieces, media).await
    {
        let ran = refused("its file", &write.store, refusal);
        if let Ran::Failed(why) = &ran {
            super::end(ctx.host(), start, BatchState::Failed, Some(why)).await;
        }
        return Step::Ended(ran);
    }
    let name = match saved.values.get(write.name as usize) {
        Some(Value::Text(name)) => Some(name.to_string()),
        _ => None,
    };
    let file = crate::files::file_value(
        &write.store,
        &jadpo_std::files::key_for(&start.run),
        name.as_deref(),
        i64::try_from(place.bytes).unwrap_or(i64::MAX),
        write.format.file_type(),
    );
    let mut open = match Batch::open(ctx, &plan.store, plan.isolation, saved.system).await {
        Ok(open) => open,
        Err(StoreError::ConcurrentChange) => return Step::Changed,
        Err(error) => {
            return Step::Ended(Ran::Paused(format!(
                "the store refused its batch ({error:?})"
            )));
        }
    };
    match lock(&mut open, &id).await {
        Ok(Locked::Held(again)) if again.read_all && again.position == held.position => {}
        Ok(Locked::Held(_)) => {
            open.rollback().await;
            return Step::Changed;
        }
        // A cancel asked for once every page was written finds the file
        // whole: the run still stops, `finished:` unrun, and the file goes.
        Ok(Locked::Cancelled(position)) => {
            let ended = end_in(&mut open, &id, BatchState::Cancelled, None, now).await;
            let step = finish(open, ended, Ran::Cancelled).await;
            return dropped(ctx, write, start, position.as_deref(), step).await;
        }
        Ok(Locked::Step(step)) => {
            open.rollback().await;
            return step;
        }
        Err(error) => return abandoned(open, error).await,
    }
    finished(
        executor,
        open,
        plan,
        &id,
        saved.values,
        held,
        now,
        Some(file),
    )
    .await
}

/// Once a cancelled export's end has committed, `step` its commit's
/// outcome, drops its file's pieces, the one put since `position` last
/// committed among them, and the file if they were joined. What a store
/// that can't be reached keeps is no row of the run's, so it is let be.
async fn dropped(
    ctx: &mut Ctx<'_>,
    write: &FileWrite,
    start: &BatchStart,
    position: Option<&str>,
    step: Step,
) -> Step {
    if !matches!(step, Step::Ended(Ran::Cancelled)) {
        return step;
    }
    let pieces = position
        .and_then(Place::read)
        .map_or(0, |place| place.pieces);
    let _ = crate::files::discard(ctx, &write.store, &start.run, pieces + 1).await;
    step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_is_kept_as_json_and_read_back() {
        let place = Place {
            after: Some("abc\"d".into()),
            pieces: 3,
            bytes: 1234,
            rows: 57,
        };
        assert_eq!(Place::read(&place.text()), Some(place));
        assert_eq!(
            Place::read(&Place::default().text()),
            Some(Place::default())
        );
        assert_eq!(Place::read("abc"), None);
    }

    #[test]
    fn a_cell_is_quoted_and_escaped_as_its_reader_needs() {
        assert_eq!(cell("Apples", ',', true), "Apples");
        assert_eq!(cell("Pears, ripe", ',', false), "\"Pears, ripe\"");
        assert_eq!(cell("say \"hi\"", ',', false), "\"say \"\"hi\"\"\"");
        assert_eq!(cell("two\nlines", ',', false), "\"two\nlines\"");
        assert_eq!(cell("=SUM(A1)", ',', true), "'=SUM(A1)");
        assert_eq!(cell("=SUM(A1,B1)", ',', true), "\"'=SUM(A1,B1)\"");
        assert_eq!(cell("-12.50", ',', false), "-12.50");
        assert_eq!(cell("@here", ',', true), "'@here");
        assert_eq!(cell("tab\there", '\t', false), "\"tab\there\"");
        assert_eq!(cell("a,b", '\t', false), "a,b");
    }
}
