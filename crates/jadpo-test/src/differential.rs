//! Each test's outcome-and-effect record in its canonical form, and two
//! runs' records compared (docs/compiler-runtime.md §2.5, "Differential
//! test"; roadmap chunk 8.1c): what the differential run holds the
//! interpreter and compiled code to, test by test.
//!
//! A test's record ([`TestRecord`]) is the runtime's record of what it did
//! ([`jadpo_runtime::record`]: each statement with its parameters' types
//! and row count, each event written with its payload, each outside call
//! with its answer, the outcome), how the test ended, and the rows its
//! database held when it ended. Only what the test's seed and clock decide
//! is in it: there are no wall-clock times, row order is canonical, and
//! each identity is written `id#1`, `id#2`… in the order it first appears,
//! so a record says which rows and events share an identity without
//! depending on how it was drawn. [`differential`] compares two runs and
//! names the first line that differs, with both sides.

use std::collections::BTreeMap;

use jadpo_runtime::record::Entry;

use crate::{Run, Test};

/// One test's record, in its canonical form: one line per thing it did,
/// then how it ended, then its database's rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestRecord {
    lines: Vec<String>,
}

impl TestRecord {
    /// The record of a test that did `entries`, ended as `verdict` says
    /// (`passed`, or the code it failed with) and left `rows`, each
    /// identity numbered in the order it first appears.
    pub fn new(entries: &[Entry], verdict: &str, rows: &[String]) -> TestRecord {
        TestRecord::of_lines(entries.iter().map(Entry::line), verdict, rows)
    }

    /// The record of a test whose entries' lines ([`Entry::line`]) are
    /// `lines`: one kept where the test ran, in workerd say (chunk 8.2e),
    /// and handed back as text.
    pub fn of_lines(
        lines: impl IntoIterator<Item = String>,
        verdict: &str,
        rows: &[String],
    ) -> TestRecord {
        let mut ids = Ids::default();
        let lines = lines
            .into_iter()
            .chain(std::iter::once(format!("test {verdict}")))
            .chain(rows.iter().map(|row| format!("row {row}")))
            .map(|line| ids.normalise(&line))
            .collect();
        TestRecord { lines }
    }

    /// Its lines, in order.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Its canonical text: each line and a line break.
    pub fn canonical(&self) -> String {
        self.lines.iter().map(|line| format!("{line}\n")).collect()
    }
}

/// The identities a record has met, each with its number.
#[derive(Default)]
struct Ids {
    seen: BTreeMap<String, usize>,
}

impl Ids {
    /// `line` with each identity (a UUID's text, any case) written
    /// `id#<n>`.
    fn normalise(&mut self, line: &str) -> String {
        let bytes = line.as_bytes();
        let mut out = String::with_capacity(line.len());
        let mut at = 0;
        let mut copied = 0;
        while at + 36 <= bytes.len() {
            let candidate = bytes.get(at..at + 36).unwrap_or_default();
            let bounded = at == 0 || !bytes[at - 1].is_ascii_hexdigit();
            let after = bytes.get(at + 36).is_none_or(|b| !b.is_ascii_hexdigit());
            if bounded && after && is_uuid(candidate) {
                out.push_str(&line[copied..at]);
                let id = line[at..at + 36].to_ascii_lowercase();
                let next = self.seen.len() + 1;
                let number = *self.seen.entry(id).or_insert(next);
                out.push_str(&format!("id#{number}"));
                at += 36;
                copied = at;
            } else {
                at += 1;
            }
        }
        out.push_str(&line[copied..]);
        out
    }
}

/// Whether `text` is a UUID's 36 characters: hex digits with dashes after
/// the 8th, 12th, 16th and 20th.
fn is_uuid(text: &[u8]) -> bool {
    text.len() == 36
        && text.iter().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// The first place two runs' records differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The test's key, file and line.
    pub test: String,
    pub file: String,
    pub line: u32,
    /// The line of its record that differs, from 1.
    pub at: usize,
    /// The line before it, which both runs share.
    pub before: Option<String>,
    /// Each run's line there; `None` when its record had ended, or the
    /// run didn't run the test, or kept no record of it.
    pub first: Option<String>,
    pub second: Option<String>,
    /// Why there's no line to compare: the test didn't run, or no record
    /// was kept.
    pub missing: Option<&'static str>,
}

impl std::fmt::Display for Difference {
    /// `test "sums add up"` (main.test.jadpo:4) differs at line 5
    /// of its record, then both sides.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}:{})", self.test, self.file, self.line)?;
        if let Some(missing) = self.missing {
            return write!(f, " {missing}");
        }
        writeln!(f, " differs at line {} of its record", self.at)?;
        if let Some(before) = &self.before {
            writeln!(f, "  both:   {before}")?;
        }
        let side = |line: &Option<String>| line.clone().unwrap_or_else(|| "(ends)".to_string());
        writeln!(f, "  first:  {}", side(&self.first))?;
        write!(f, "  second: {}", side(&self.second))
    }
}

/// Compares the records of the tests `first` and `second` ran, test by
/// test in file and line order: how many were compared when every record
/// is the same, else the first difference, with both sides. A test one run
/// ran and the other didn't, or one with no record (a run that didn't ask
/// for records), is a difference.
pub fn differential(first: &Run, second: &Run) -> Result<usize, Box<Difference>> {
    compare(&recorded(first), &recorded(second))
}

/// Each test `run` ran, with its record when it kept one.
pub fn recorded(run: &Run) -> Vec<(&Test, Option<&TestRecord>)> {
    run.results
        .iter()
        .map(|result| (&result.test, result.record.as_ref()))
        .collect()
}

/// Compares two runs' records, each given as the tests it ran with their
/// records ([`recorded`]), as [`differential`] does: a run kept elsewhere,
/// a Worker's in workerd (chunk 8.2e), compared with one here.
pub fn compare(
    first: &[(&Test, Option<&TestRecord>)],
    second: &[(&Test, Option<&TestRecord>)],
) -> Result<usize, Box<Difference>> {
    type Ran<'r> = (&'r Test, Option<&'r TestRecord>);
    fn by_key<'r>(run: &[Ran<'r>]) -> BTreeMap<String, Ran<'r>> {
        run.iter()
            .map(|(test, record)| (test.key.clone(), (*test, *record)))
            .collect()
    }
    let (ones, twos) = (by_key(first), by_key(second));
    let mut keys: Vec<&Test> = ones.values().map(|(test, _)| *test).collect();
    keys.extend(
        twos.iter()
            .filter(|(key, _)| !ones.contains_key(*key))
            .map(|(_, (test, _))| *test),
    );
    keys.sort_by(|a, b| (&a.file, a.line, &a.key).cmp(&(&b.file, b.line, &b.key)));
    let mut compared = 0;
    for test in keys {
        let missing = |why: &'static str| {
            Box::new(Difference {
                test: test.key.clone(),
                file: test.file.clone(),
                line: test.line,
                at: 0,
                before: None,
                first: None,
                second: None,
                missing: Some(why),
            })
        };
        let (Some((_, one)), Some((_, two))) = (ones.get(&test.key), twos.get(&test.key)) else {
            return Err(missing(if ones.contains_key(&test.key) {
                "ran only in the first run"
            } else {
                "ran only in the second run"
            }));
        };
        let (Some(a), Some(b)) = (one, two) else {
            return Err(missing("has no record in one of the runs"));
        };
        if let Some(at) = first_difference(a, b) {
            return Err(Box::new(Difference {
                test: test.key.clone(),
                file: test.file.clone(),
                line: test.line,
                at: at + 1,
                before: at.checked_sub(1).and_then(|i| a.lines.get(i).cloned()),
                first: a.lines.get(at).cloned(),
                second: b.lines.get(at).cloned(),
                missing: None,
            }));
        }
        compared += 1;
    }
    Ok(compared)
}

/// The index of the first line where `a` and `b` differ, one having ended
/// among them.
fn first_difference(a: &TestRecord, b: &TestRecord) -> Option<usize> {
    let shared = a.lines.len().min(b.lines.len());
    (0..shared)
        .find(|&i| a.lines[i] != b.lines[i])
        .or_else(|| (a.lines.len() != b.lines.len()).then_some(shared))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jadpo_runtime::record::Ran;

    use super::*;
    use crate::{Selection, Test, TestKind, TestResult};

    fn statement(sql: &str, rows: usize) -> Entry {
        Entry::Statement {
            sql: sql.into(),
            types: vec!["uuid".into()],
            ran: Ran::Read(rows),
        }
    }

    /// Each identity is numbered in the order it first appears, wherever
    /// it appears, in any case; text that only looks like part of one is
    /// left alone.
    #[test]
    fn identities_are_numbered_in_the_order_they_first_appear() {
        let a = "0b1c2d3e-0000-4000-8000-00000000000a";
        let b = "0B1C2D3E-0000-4000-8000-00000000000B";
        let record = TestRecord::new(
            &[
                Entry::Event {
                    event: "type Booked".into(),
                    payload: format!("{{\"id\":\"{b}\",\"other\":\"{a}\"}}"),
                },
                Entry::Outcome(format!("value \"{a}\"")),
            ],
            "passed",
            &[
                format!("main.loans {{\"id\":\"{}\"}}", b.to_lowercase()),
                format!("x f{a}"),
            ],
        );
        assert_eq!(
            record.lines(),
            [
                "event type Booked {\"id\":\"id#1\",\"other\":\"id#2\"}",
                "outcome value \"id#2\"",
                "test passed",
                "row main.loans {\"id\":\"id#1\"}",
                &format!("row x f{a}"),
            ]
        );
        assert!(record.canonical().ends_with(&format!("row x f{a}\n")));
    }

    fn run(records: &[(&str, Option<TestRecord>)]) -> Run {
        Run {
            seed: 1,
            root: None,
            selection: Selection::default(),
            total: BTreeMap::new(),
            results: records
                .iter()
                .enumerate()
                .map(|(n, (name, record))| TestResult {
                    test: Test {
                        key: format!("test \"{name}\""),
                        name: (*name).into(),
                        file: "main.test.jadpo".into(),
                        line: u32::try_from(n).unwrap() * 10 + 1,
                        end_line: u32::try_from(n).unwrap() * 10 + 5,
                        kind: TestKind::Action,
                    },
                    failure: None,
                    record: record.clone(),
                })
                .collect(),
            alerts: Vec::new(),
            elapsed: Duration::ZERO,
        }
    }

    fn record(rows: usize) -> TestRecord {
        TestRecord::new(
            &[
                statement("SELECT 1", 1),
                statement("SELECT \"id\" FROM \"loans\"", rows),
            ],
            "passed",
            &[],
        )
    }

    /// Two runs whose records are the same compare clean, however their
    /// tests were ordered.
    #[test]
    fn the_same_records_compare_clean_in_any_order() {
        let first = run(&[("a", Some(record(1))), ("b", Some(record(2)))]);
        let mut second = run(&[("a", Some(record(1))), ("b", Some(record(2)))]);
        second.results.reverse();
        assert_eq!(differential(&first, &second), Ok(2));
    }

    /// The first difference is named by its test and line, with the line
    /// both share before it and each side.
    #[test]
    fn the_first_difference_is_named_with_both_sides() {
        let first = run(&[("a", Some(record(1))), ("b", Some(record(2)))]);
        let second = run(&[("a", Some(record(1))), ("b", Some(record(3)))]);
        let difference = differential(&first, &second).unwrap_err();
        assert_eq!(difference.at, 2);
        assert_eq!(
            difference.to_string(),
            "test \"b\" (main.test.jadpo:11) differs at line 2 of its record\n  \
             both:   sql SELECT 1 | (uuid) | read 1\n  \
             first:  sql SELECT \"id\" FROM \"loans\" | (uuid) | read 2\n  \
             second: sql SELECT \"id\" FROM \"loans\" | (uuid) | read 3"
        );

        let shorter = TestRecord::new(&[statement("SELECT 1", 1)], "passed", &[]);
        let second = run(&[("a", Some(record(1))), ("b", Some(shorter))]);
        let difference = differential(&first, &second).unwrap_err();
        assert_eq!(
            difference.first.as_deref(),
            Some("sql SELECT \"id\" FROM \"loans\" | (uuid) | read 2")
        );
        assert_eq!(difference.second.as_deref(), Some("test passed"));
    }

    /// A test only one run ran, or one without a record, is a difference
    /// too.
    #[test]
    fn a_test_missing_from_a_run_or_unrecorded_differs() {
        let first = run(&[("a", Some(record(1))), ("b", Some(record(2)))]);
        let second = run(&[("a", Some(record(1)))]);
        assert_eq!(
            differential(&first, &second).unwrap_err().to_string(),
            "test \"b\" (main.test.jadpo:11) ran only in the first run"
        );
        let unrecorded = run(&[("a", None), ("b", Some(record(2)))]);
        assert_eq!(
            differential(&first, &unrecorded).unwrap_err().missing,
            Some("has no record in one of the runs")
        );
    }
}
