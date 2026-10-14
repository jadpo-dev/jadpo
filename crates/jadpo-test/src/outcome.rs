//! A failed test as data that leaves the interpreter's thread (values are
//! shared behind `Rc` there, so everything here is text): its stable code,
//! where in the test it failed, and what area 13 says a report shows: the
//! assertion with each part's value, expected and actual, a record's
//! differences field by field, or the failure or fault that happened
//! instead with its `.jadpo` frames.

use jadpo_diag::{Rule, rule};
use jadpo_interp::{
    Assertion, AssertionDetail, CallError, Failure, Fault, Frame, Got, Outcome, Program, Report,
    Value,
};

use crate::diff::diff;
use crate::{Rerun, Test};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestFailure {
    /// `test.assertion_failed`, `test.unexpected_failure` or `test.fault`.
    pub rule: &'static Rule,
    /// Where in the test it failed.
    pub file: String,
    pub line: u32,
    /// 1-based, in characters.
    pub column: usize,
    pub detail: Detail,
    /// The command that runs this test alone in the same order.
    pub rerun: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    Assertion(Box<AssertionFailure>),
    /// A failure `attempt` passed on out of the test.
    Failure(Box<FailureInfo>),
    Fault(Box<FaultInfo>),
}

/// An `assert` that didn't hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssertionFailure {
    /// The statement as written.
    pub source: String,
    /// Its lines without their indentation, each with the values hung
    /// under it.
    pub lines: Vec<ShownLine>,
    /// Every part's value, in the order their anchors are written.
    pub parts: Vec<ShownPart>,
    /// A comparison's operator, as written.
    pub operator: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
    /// The failure `assert … fails` named.
    pub fails: Option<String>,
    /// What the call returned instead of failing.
    pub returned: Option<String>,
    /// The failure that happened instead of the one named.
    pub instead: Option<Box<FailureInfo>>,
    /// For `==` on records, lists, sets and maps: each difference.
    pub diff: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownLine {
    pub line: u32,
    pub source: String,
    /// 0-based character column in `source`, the value, and whether it is
    /// the condition's own value (a plain `assert`'s `false`).
    pub marks: Vec<(usize, String, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownPart {
    pub source: String,
    pub line: u32,
    /// 1-based, in characters, of the part's anchor.
    pub column: usize,
    pub value: String,
}

/// One difference between the expected and the actual value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub mark: ChangeMark,
    /// `x`, `address.city`, `[2]`, `items[1].title`.
    pub path: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeMark {
    /// Both have it, with different values.
    Changed,
    /// Only the actual value has it.
    Extra,
    /// Only the expected value has it.
    Missing,
}

impl ChangeMark {
    pub fn symbol(self) -> &'static str {
        match self {
            ChangeMark::Changed => "~",
            ChangeMark::Extra => "+",
            ChangeMark::Missing => "-",
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            ChangeMark::Changed => "changed",
            ChangeMark::Extra => "extra",
            ChangeMark::Missing => "missing",
        }
    }
}

/// A site with its line of source and carets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub path: String,
    pub line: u32,
    /// 1-based, in characters.
    pub column: usize,
    pub source: String,
    pub length: usize,
    pub label: String,
}

/// A declared failure (area 4) and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureInfo {
    pub name: String,
    pub code: String,
    /// `FailureKind.x` as `x`.
    pub kind: Option<String>,
    pub message: Option<String>,
    pub client: Vec<(String, String)>,
    pub internal: Vec<(String, String)>,
    /// `` `TooBig` from action check_size ``.
    pub subject: String,
    /// Why, when a construction's rule was broken.
    pub reason: Option<String>,
    pub at: Option<Located>,
    /// The callables it travelled through, outermost first.
    pub path: Vec<String>,
    /// The failures it was mapped from, as the interpreter's report names
    /// them.
    pub cause: Vec<String>,
}

/// A fault the runtime raised (area 4, "Faults").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaultInfo {
    /// `int_overflow`, `not_built_yet`, …
    pub kind: String,
    /// `int_overflow in function line_total`.
    pub subject: String,
    /// The catalogue rule the fault has, if any (`toolchain.not_built_yet`).
    pub rule: Option<&'static str>,
    pub at: Option<Located>,
    pub reason: Option<String>,
    /// The next step, with who decides: `… (agent)`.
    pub next: Option<String>,
    /// Innermost first: `file:line` and the callable, a run of one frame
    /// shown once with how many times it repeats.
    pub stack: Vec<(String, String)>,
}

/// Where the test frame stands: the outermost frame's site.
fn place_in_test(program: &Program, frames: &[Frame], test: &Test) -> (String, u32, usize) {
    let found = frames.last().and_then(|frame| program.site(*frame));
    match found {
        Some(span) => {
            let column = program
                .text(span.file)
                .map_or(1, |text| column_of(text, span.start as usize));
            (span.file.to_string(), span.line, column)
        }
        None => (test.file.clone(), test.line, 1),
    }
}

/// The 1-based character column of byte `offset` on its line.
fn column_of(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    text.get(line_start..offset)
        .map_or(0, |before| before.chars().count())
        + 1
}

pub(crate) fn failure(
    program: &Program,
    test: &Test,
    outcome: &Outcome,
    rerun: &Rerun,
) -> TestFailure {
    match outcome {
        Outcome::Assertion(assertion) => {
            let (file, line, column) = place_in_test(program, &assertion.frames, test);
            TestFailure {
                rule: &rule::TEST_ASSERTION_FAILED,
                rerun: rerun.command(&file, line),
                file,
                line,
                column,
                detail: Detail::Assertion(Box::new(assertion_failure(program, assertion))),
            }
        }
        Outcome::Failed(failure) => {
            let (file, line, column) = place_in_test(program, &failure.frames, test);
            TestFailure {
                rule: &rule::TEST_UNEXPECTED_FAILURE,
                rerun: rerun.command(&file, line),
                file,
                line,
                column,
                detail: Detail::Failure(Box::new(failure_info(program, failure))),
            }
        }
        Outcome::Fault(fault) => {
            let (file, line, column) = place_in_test(program, &fault.frames, test);
            TestFailure {
                rule: &rule::TEST_FAULT,
                rerun: rerun.command(&file, line),
                file,
                line,
                column,
                detail: Detail::Fault(Box::new(fault_info(program, fault))),
            }
        }
        // A test that gives a value passed; the caller never asks.
        Outcome::Value(_) => not_runnable(
            test,
            &CallError::Unknown("a test that passed".to_string()),
            rerun,
        ),
    }
}

/// A test the interpreter couldn't start: a defect, since every test the
/// graph lists has a body.
pub(crate) fn not_runnable(test: &Test, error: &CallError, rerun: &Rerun) -> TestFailure {
    TestFailure {
        rule: &rule::TEST_FAULT,
        file: test.file.clone(),
        line: test.line,
        column: 1,
        rerun: rerun.command(&test.file, test.line),
        detail: Detail::Fault(Box::new(FaultInfo {
            kind: "internal_fault".to_string(),
            subject: format!("internal_fault in test \"{}\"", test.name),
            rule: None,
            at: None,
            reason: Some(format!(
                "The interpreter couldn't run the test: {error}. This is a mistake in Jadpo, not in this test."
            )),
            next: Some("Report it to Jadpo's maintainers with this report (human)".to_string()),
            stack: Vec::new(),
        })),
    }
}

fn located(report: &Report) -> Option<Located> {
    report.location.as_ref().map(|location| Located {
        path: location.path.clone(),
        line: location.line,
        column: location.column,
        source: location.source.clone(),
        length: location.length,
        label: location.label.clone(),
    })
}

fn field<'r>(report: &'r Report, label: &str) -> Option<&'r [String]> {
    report
        .fields
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, lines)| lines.as_slice())
}

fn texts(fields: &[(String, Value)]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|(name, value)| (name.clone(), value.to_string()))
        .collect()
}

fn failure_info(program: &Program, failure: &Failure) -> FailureInfo {
    let report = program.report(&Outcome::Failed(Box::new(failure.clone())));
    let places = program.places();
    FailureInfo {
        name: failure.name.clone(),
        code: failure.code.clone(),
        kind: failure.kind.clone(),
        message: failure.message.clone(),
        client: texts(&failure.client),
        internal: texts(&failure.internal),
        subject: report
            .as_ref()
            .map_or_else(|| format!("`{}`", failure.name), |r| r.subject.clone()),
        reason: report
            .as_ref()
            .and_then(|r| field(r, "reason"))
            .and_then(|lines| lines.first().cloned()),
        at: report.as_ref().and_then(located),
        path: failure
            .frames
            .iter()
            .rev()
            .map(|frame| places.label(frame.callable).to_string())
            .collect(),
        cause: report
            .as_ref()
            .and_then(|r| field(r, "cause"))
            .map(<[String]>::to_vec)
            .unwrap_or_default(),
    }
}

fn fault_info(program: &Program, fault: &Fault) -> FaultInfo {
    let report = program.report(&Outcome::Fault(Box::new(fault.clone())));
    let places = program.places();
    let mut stack: Vec<(String, String)> = Vec::new();
    let mut runs: Vec<usize> = Vec::new();
    for frame in &fault.frames {
        let Some(place) = places.place(*frame) else {
            continue;
        };
        let entry = (
            format!("{}:{}", place.file, place.line),
            place.label.to_string(),
        );
        if stack.last() == Some(&entry) {
            if let Some(run) = runs.last_mut() {
                *run += 1;
            }
        } else {
            stack.push(entry);
            runs.push(1);
        }
    }
    for ((_, label), run) in stack.iter_mut().zip(runs) {
        if run > 1 {
            label.push_str(&format!(" ({run} times)"));
        }
    }
    FaultInfo {
        kind: fault.kind.classification().to_string(),
        subject: report.as_ref().map_or_else(
            || fault.kind.classification().to_string(),
            |r| r.subject.clone(),
        ),
        rule: match fault.kind {
            jadpo_interp::FaultKind::NotBuiltYet(_) => Some(rule::TOOLCHAIN_NOT_BUILT_YET.id),
            _ => None,
        },
        at: report.as_ref().and_then(located),
        reason: report
            .as_ref()
            .and_then(|r| field(r, "reason"))
            .and_then(|lines| lines.first().cloned()),
        next: report
            .as_ref()
            .and_then(|r| field(r, "next"))
            .and_then(|lines| lines.first().cloned()),
        stack,
    }
}

fn assertion_failure(program: &Program, assertion: &Assertion) -> AssertionFailure {
    let statement = assertion
        .frames
        .first()
        .and_then(|frame| program.site(*frame));
    let (source, lines, parts) = match statement {
        Some(span) => {
            let text = program.text(span.file).unwrap_or_default();
            shown(
                text,
                span.start as usize,
                span.end as usize,
                span.line,
                assertion,
            )
        }
        None => (String::new(), Vec::new(), Vec::new()),
    };
    let mut failure = AssertionFailure {
        source,
        lines,
        parts,
        operator: None,
        expected: None,
        actual: None,
        fails: None,
        returned: None,
        instead: None,
        diff: Vec::new(),
    };
    match &assertion.detail {
        AssertionDetail::False {
            comparison: Some((op, left, right)),
        } => {
            failure.operator = Some(op.to_string());
            failure.actual = Some(left.to_string());
            failure.expected = Some(match *op {
                "==" => right.to_string(),
                "!=" => format!("not {right}"),
                op => format!("{op} {right}"),
            });
            if *op == "==" {
                failure.diff = diff(right, left);
            }
        }
        AssertionDetail::False { comparison: None } => {}
        AssertionDetail::Fails { expected, got } => {
            failure.fails = Some(expected.clone());
            failure.expected = Some(format!("failure `{expected}`"));
            match got {
                Got::Value(value) => {
                    failure.returned = Some(value.to_string());
                    failure.actual = Some(format!("returned {value}"));
                }
                Got::Failure(instead) => {
                    failure.actual = Some(format!("failure `{}`", instead.name));
                    failure.instead = Some(Box::new(failure_info(program, instead)));
                }
            }
        }
    }
    failure
}

/// The statement at `start..end` of `text` as written, its lines without
/// their indentation with each part's value under its anchor, and the
/// parts themselves.
fn shown(
    text: &str,
    start: usize,
    end: usize,
    first_line: u32,
    assertion: &Assertion,
) -> (String, Vec<ShownLine>, Vec<ShownPart>) {
    let Some(statement) = text.get(start..end.max(start)) else {
        return (String::new(), Vec::new(), Vec::new());
    };
    let indent = column_of(text, start) - 1;
    let raw_lines: Vec<&str> = statement.split('\n').collect();
    // Each line's first byte in the file, and how many characters of
    // indentation it loses.
    let mut line_starts = Vec::with_capacity(raw_lines.len());
    let mut offset = start;
    for line in &raw_lines {
        line_starts.push(offset);
        offset += line.len() + 1;
    }
    let mut lines: Vec<ShownLine> = raw_lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let line = line.trim_end_matches('\r');
            let source = if i == 0 {
                line.to_string()
            } else {
                strip_indent(line, indent)
            };
            ShownLine {
                line: first_line + i as u32,
                source,
                marks: Vec::new(),
            }
        })
        .collect();
    // The condition starts after `assert`; a plain condition's own value
    // is the one that is false.
    let condition = start
        + statement
            .strip_prefix("assert")
            .map_or(0, |rest| 6 + (rest.len() - rest.trim_start().len()));
    let plain = matches!(
        assertion.detail,
        AssertionDetail::False { comparison: None }
    );
    let widest = assertion
        .parts
        .iter()
        .filter(|part| part.start as usize == condition)
        .map(|part| part.end)
        .max();
    let mut parts = Vec::new();
    for part in &assertion.parts {
        let anchor = part.anchor as usize;
        if anchor < start || anchor >= end {
            continue;
        }
        let index = line_starts
            .iter()
            .rposition(|&line_start| line_start <= anchor)
            .unwrap_or(0);
        let before = text.get(line_starts[index]..anchor).unwrap_or_default();
        let mut column = before.chars().count();
        if index > 0 {
            column = column.saturating_sub(lost_indent(raw_lines[index], indent));
        }
        let value = part.value.to_string();
        let root = plain && part.start as usize == condition && Some(part.end) == widest;
        lines[index].marks.push((column, value.clone(), root));
        parts.push(ShownPart {
            source: text
                .get(part.start as usize..part.end as usize)
                .unwrap_or_default()
                .to_string(),
            line: first_line + index as u32,
            column: column_of(text, anchor),
            value,
        });
    }
    (statement.to_string(), lines, parts)
}

/// How many leading whitespace characters up to `indent` a line loses.
fn lost_indent(line: &str, indent: usize) -> usize {
    line.chars()
        .take(indent)
        .take_while(|c| c.is_whitespace())
        .count()
}

fn strip_indent(line: &str, indent: usize) -> String {
    line.chars().skip(lost_indent(line, indent)).collect()
}
