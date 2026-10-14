//! What a person reads when a call ends in a fault, a failure or a failed
//! assertion: the shape every Jadpo error shares (CLI design language, "One
//! shape everywhere"), so a runtime problem reads like a compiler error.
//! The headline word is `fault` or `failed`, from the closed set; the
//! location is the `.jadpo` site with its code frame; then the fields:
//! `reason` and `next` as a diagnostic has them, a failure's code, kind and
//! fields, the cause it was mapped from, and the frames.
//!
//! ```text
//! fault: int_overflow in function line_total
//!  --> main.jadpo:3:30
//!   |
//! 3 |     var total: Pence = Pence(price * quantity)
//!   |                              ^^^^^^^^^^^^^^^^ out of range
//!   reason: `price * quantity` gives a whole number outside `Int`'s range, …
//!   next: Bound the values that feed it with `max`, … (agent)
//!   stack: main.jadpo:3  function line_total
//!          main.jadpo:8  action price_line
//! ```
//!
//! [`Report::plain`] is the piped form, with no colour or symbols; the
//! test runner gives it the rich form on the same grid.

use jadpo_runtime::{Failure, Fault, FaultKind, Frame, MAX_DEPTH};
use jadpo_value::Value;

use crate::program::Program;
use crate::{Assertion, AssertionDetail, Got, Outcome};

/// Where a report points: a site, with its line of source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: String,
    pub line: u32,
    /// 1-based, in characters.
    pub column: usize,
    /// The source line, as written.
    pub source: String,
    /// How many characters of the line the site covers.
    pub length: usize,
    /// The few words beside the carets.
    pub label: String,
}

/// A rendered runtime problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// `fault` or `failed`.
    pub word: &'static str,
    /// What happened, in the reader's terms.
    pub subject: String,
    pub location: Option<Location>,
    /// Each label with its lines, in order.
    pub fields: Vec<(&'static str, Vec<String>)>,
}

/// The text of a site and its place, from the program's sources.
struct SiteText {
    location: Option<Location>,
    /// The site's source on its first line: `price * quantity`.
    written: String,
}

/// Text as it may reach a terminal: control characters and the characters
/// that reorder text are written as `\u{...}`, as compiler errors write
/// them.
fn visible(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let hidden = (c.is_control() && c != '\t')
            || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}');
        if hidden {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn site_text(program: &Program, frame: Frame, label: &str) -> SiteText {
    let callable = program.callable(frame.callable);
    let site = callable.and_then(|callable| callable.body.sites.get(frame.site as usize));
    let text = callable.and_then(|callable| program.text(&callable.file));
    let framed = match (callable, site, text) {
        (Some(callable), Some(site), Some(text)) => framed(callable, site, text, label),
        _ => None,
    };
    if let Some(framed) = framed {
        return framed;
    }
    let location = program.places().place(frame).map(|place| Location {
        path: place.file.to_string(),
        line: place.line,
        column: 1,
        source: String::new(),
        length: 0,
        label: String::new(),
    });
    SiteText {
        location,
        written: String::new(),
    }
}

/// A site's place with its line of source; `None` when its range doesn't
/// fall on the text's characters.
fn framed(
    callable: &crate::program::Callable,
    site: &jadpo_graph::body::Site,
    text: &str,
    label: &str,
) -> Option<SiteText> {
    let start = site.start as usize;
    let end = (site.end as usize).max(start);
    let line_start = text.get(..start)?.rfind('\n').map_or(0, |i| i + 1);
    let line_end = text
        .get(start..)?
        .find('\n')
        .map_or(text.len(), |i| start + i);
    let source = text.get(line_start..line_end)?.trim_end_matches('\r');
    let before = visible(text.get(line_start..start)?);
    let covered = visible(text.get(start..end.min(line_end))?);
    let written = text
        .get(start..end)?
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    Some(SiteText {
        location: Some(Location {
            path: callable.file.clone(),
            line: site.line,
            column: before.chars().count() + 1,
            source: source.to_string(),
            length: covered.chars().count().max(1),
            label: label.to_string(),
        }),
        written,
    })
}

/// Frames as area 17 prints them, innermost first, a run of the same
/// frame shown once with how many times it repeats.
fn stack(program: &Program, frames: &[Frame]) -> Vec<String> {
    let lines: Vec<String> = frames
        .iter()
        .filter_map(|frame| program.places().place(*frame))
        .map(|place| place.to_string())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let run = lines[i..]
            .iter()
            .take_while(|line| **line == lines[i])
            .count();
        out.push(if run > 1 {
            format!("{} ({run} times)", lines[i])
        } else {
            lines[i].clone()
        });
        i += run;
    }
    out
}

/// The callables a failure travelled through, outermost first, each once
/// per frame: `test "…" -> action place_order -> action reserve`.
fn path(program: &Program, frames: &[Frame]) -> String {
    let labels: Vec<&str> = frames
        .iter()
        .rev()
        .map(|frame| program.places().label(frame.callable))
        .collect();
    labels.join(" -> ")
}

/// A failure's fields as Jadpo writes a value's: `{ available: 5 }`.
fn fields_value(fields: &[(String, Value)]) -> String {
    let shown: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    format!("{{ {} }}", shown.join(", "))
}

impl Report {
    pub(crate) fn of(program: &Program, outcome: &Outcome) -> Option<Report> {
        match outcome {
            Outcome::Value(_) => None,
            Outcome::Fault(fault) => Some(Report::fault(program, fault)),
            Outcome::Failed(failure) => Some(Report::failure(program, failure)),
            Outcome::Assertion(assertion) => Some(Report::assertion(program, assertion)),
        }
    }

    fn fault(program: &Program, fault: &Fault) -> Report {
        let site = fault.site();
        let label = site.map_or("", |frame| program.places().label(frame.callable));
        let caret = match &fault.kind {
            FaultKind::IntOverflow => "out of range".to_string(),
            FaultKind::DecimalOverflow => "past 38 digits".to_string(),
            FaultKind::RecursionLimit => format!("{MAX_DEPTH} calls deep"),
            FaultKind::NotBuiltYet(_) => "not built yet".to_string(),
            FaultKind::Defect(_) => "here".to_string(),
        };
        let text = site.map(|frame| site_text(program, frame, &caret));
        let written = text.as_ref().map_or("", |text| text.written.as_str());
        let mut fields = Vec::new();
        let (reason, next, decider) = match &fault.kind {
            FaultKind::IntOverflow => (
                format!(
                    "`{written}` gives a whole number outside `Int`'s range, {} to {}; Jadpo stops rather than wrap around.",
                    i64::MIN,
                    i64::MAX
                ),
                "Bound the values that feed it with `max`, so the result always fits".to_string(),
                "agent",
            ),
            FaultKind::DecimalOverflow => (
                format!(
                    "`{written}` gives a `Decimal` of more than 38 digits, more than an exact number holds."
                ),
                "Bound the values that feed it with `max`, or keep fewer decimal places"
                    .to_string(),
                "agent",
            ),
            FaultKind::RecursionLimit => (
                format!(
                    "The call stack reached {MAX_DEPTH} frames, the most Jadpo allows, so the recursion stopped here instead of exhausting the stack."
                ),
                "Make the recursion end sooner, or recurse over smaller values".to_string(),
                "agent",
            ),
            FaultKind::NotBuiltYet(what) => (
                format!(
                    "The language declares `{what}`, but this toolchain has nothing to run it with yet, so the call stopped here."
                ),
                format!("Do without `{what}` for now, or test this part once its area is built"),
                "agent",
            ),
            FaultKind::Defect(what) => (
                format!(
                    "The program reached {what}: a mistake in Jadpo, not in this program, since the checker should have ruled it out."
                ),
                "Report it to Jadpo's maintainers with this report".to_string(),
                "human",
            ),
        };
        fields.push(("reason", vec![reason]));
        fields.push(("next", vec![format!("{next} ({decider})")]));
        fields.push(("stack", stack(program, &fault.frames)));
        // The one runtime problem the catalogue has a rule for.
        if let FaultKind::NotBuiltYet(_) = fault.kind {
            fields.push(("rule", vec!["toolchain.not_built_yet".to_string()]));
            fields.push((
                "help",
                vec!["docs/language/03-logic.html#toolchain-not-built-yet".to_string()],
            ));
        }
        Report {
            word: "fault",
            subject: format!("{} in {label}", fault.kind.classification()),
            location: text.and_then(|text| text.location),
            fields,
        }
    }

    fn failure(program: &Program, failure: &Failure) -> Report {
        let site = failure.site();
        let label = site.map_or("", |frame| program.places().label(frame.callable));
        let caret = match &failure.broken {
            Some(broken) => format!("breaks {broken}"),
            None => "rejected here".to_string(),
        };
        let location = site.and_then(|frame| site_text(program, frame, &caret).location);
        let mut fields = Vec::new();
        if let Some(broken) = &failure.broken {
            fields.push((
                "reason",
                vec![format!(
                    "The value built here breaks {broken}, so `attempt` gives the failure its `invalid:` names."
                )],
            ));
        }
        fields.push(("code", vec![failure.code.clone()]));
        if let Some(kind) = &failure.kind {
            let status = failure
                .status
                .map_or_else(String::new, |status| format!(" ({status})"));
            fields.push(("kind", vec![format!("{kind}{status}")]));
        }
        if let Some(message) = &failure.message {
            fields.push(("message", vec![message.clone()]));
        }
        if !failure.client.is_empty() {
            fields.push(("client", vec![fields_value(&failure.client)]));
        }
        if !failure.internal.is_empty() {
            fields.push(("internal", vec![fields_value(&failure.internal)]));
        }
        let mut causes = Vec::new();
        let mut cause = failure.cause.as_deref();
        while let Some(found) = cause {
            let from = found.site().map_or_else(String::new, |frame| {
                let place = program
                    .places()
                    .place(frame)
                    .map_or_else(String::new, |place| {
                        format!(", {}:{}", place.file, place.line)
                    });
                format!(" from {}{place}", program.places().label(frame.callable))
            });
            // Its internal fields by name, as area 4's log example shows
            // a cause: never the values.
            let internal: Vec<&str> = found
                .internal
                .iter()
                .map(|(name, _)| name.as_str())
                .collect();
            let internal = if internal.is_empty() {
                String::new()
            } else {
                format!(" (internal: {})", internal.join(", "))
            };
            causes.push(format!("`{}`{from}{internal}", found.name));
            cause = found.cause.as_deref();
        }
        if !causes.is_empty() {
            fields.push(("cause", causes));
        }
        fields.push(("path", vec![path(program, &failure.frames)]));
        Report {
            word: "failed",
            subject: format!("`{}` from {label}", failure.name),
            location,
            fields,
        }
    }

    fn assertion(program: &Program, assertion: &Assertion) -> Report {
        let site = assertion.frames.first().copied();
        let label = site.map_or("", |frame| program.places().label(frame.callable));
        let (caret, fields) = match &assertion.detail {
            AssertionDetail::False {
                comparison: Some(("==", actual, expected)),
            } => (
                "doesn't hold",
                vec![
                    ("expected", vec![expected.to_string()]),
                    ("actual", vec![actual.to_string()]),
                ],
            ),
            AssertionDetail::False {
                comparison: Some((op, left, right)),
            } => (
                "doesn't hold",
                vec![
                    ("left", vec![left.to_string()]),
                    ("right", vec![right.to_string()]),
                    ("compared", vec![format!("{left} {op} {right} is false")]),
                ],
            ),
            AssertionDetail::False { comparison: None } => ("is false", Vec::new()),
            AssertionDetail::Fails { expected, got } => (
                "didn't fail as expected",
                vec![
                    ("expected", vec![format!("fails with `{expected}`")]),
                    (
                        "actual",
                        vec![match got {
                            Got::Value(value) => format!("gave {value}"),
                            Got::Failure(failure) => format!("failed with `{}`", failure.name),
                        }],
                    ),
                ],
            ),
        };
        let location = site.and_then(|frame| site_text(program, frame, caret).location);
        Report {
            word: "failed",
            subject: format!("an assertion in {label}"),
            location,
            fields,
        }
    }

    /// `fault: int_overflow in function line_total`.
    pub fn headline(&self) -> String {
        format!("{}: {}", self.word, self.subject)
    }

    /// The plain form, as piped output and logs show it: area 14's
    /// `-->` location and rustc's code frame, then `label: value` lines.
    pub fn plain(&self) -> String {
        let mut out = vec![self.headline()];
        if let Some(location) = &self.location {
            out.push(format!(
                " --> {}:{}:{}",
                location.path, location.line, location.column
            ));
            if !location.source.is_empty() {
                let number = location.line.to_string();
                let gutter = " ".repeat(number.len());
                out.push(format!("{gutter} |"));
                out.push(format!("{number} | {}", visible(&location.source)));
                let mut under = format!(
                    "{gutter} | {}{}",
                    " ".repeat(location.column - 1),
                    "^".repeat(location.length)
                );
                if !location.label.is_empty() {
                    under.push(' ');
                    under.push_str(&location.label);
                }
                out.push(under);
            }
        }
        for (label, lines) in &self.fields {
            let indent = " ".repeat(label.len() + 4);
            for (i, line) in lines.iter().enumerate() {
                if i == 0 {
                    out.push(format!("  {label}: {line}"));
                } else {
                    out.push(format!("{indent}{line}"));
                }
            }
        }
        let mut text = out.join("\n");
        text.push('\n');
        text
    }
}
