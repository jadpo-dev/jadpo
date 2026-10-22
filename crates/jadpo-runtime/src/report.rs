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
//! A report is read from the outcome, the frame table ([`Places`]) and the
//! sources, so whichever executor ran the call, its report is the same.
//! [`Report::plain`] is the piped form, with no colour or symbols; the test
//! runner gives the rich form on the same grid.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;

use crate::outcome::{Assertion, AssertionDetail, Got, Outcome};
use crate::{Failure, Fault, FaultKind, Frame, MAX_DEPTH, MAX_INLINE_DEPTH, Places};

/// The text of each source file, by project-relative path.
pub trait Sources {
    fn text(&self, path: &str) -> Option<&str>;
}

impl Sources for BTreeMap<String, String> {
    fn text(&self, path: &str) -> Option<&str> {
        self.get(path).map(String::as_str)
    }
}

impl Sources for HashMap<String, String> {
    fn text(&self, path: &str) -> Option<&str> {
        self.get(path).map(String::as_str)
    }
}

/// Fields as Jadpo writes a value's: `{ available: 5, requested: 7 }`.
pub fn fields_text<N: Display, V: Display>(fields: impl IntoIterator<Item = (N, V)>) -> String {
    let shown: Vec<String> = fields
        .into_iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    format!("{{ {} }}", shown.join(", "))
}

/// Where a report points: a site, with its line of source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: String,
    /// The site's byte range in the file, when the site is known.
    pub span: Option<(u32, u32)>,
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

/// The catalogue rule a runtime problem has, when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportRule {
    pub id: &'static str,
    pub help: &'static str,
}

/// What a report says besides its headline and location.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    Fault {
        reason: String,
        /// The next step, with who decides: `… (agent)`.
        next: String,
        /// Innermost first, each recursion once with its count.
        stack: Vec<String>,
        rule: Option<ReportRule>,
    },
    Failure {
        /// Why, when a construction's rule was broken.
        reason: Option<String>,
        code: String,
        /// `conflict (409)`.
        kind: Option<String>,
        message: Option<String>,
        client: Option<String>,
        internal: Option<String>,
        /// The failures it was mapped from.
        cause: Vec<String>,
        /// The callables it travelled through, outermost first.
        path: String,
    },
    Assertion {
        /// Each label with its value: `expected`, `actual`, …
        values: Vec<(&'static str, String)>,
    },
}

/// A rendered runtime problem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// `fault` or `failed`.
    pub word: &'static str,
    /// What happened, in the reader's terms.
    pub subject: String,
    pub location: Option<Location>,
    pub detail: Detail,
}

/// The text of a site and its place, from the sources.
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

fn site_text(places: &Places, sources: &dyn Sources, frame: Frame, label: &str) -> SiteText {
    let framed = places.site(frame).and_then(|(file, site)| {
        let text = sources.text(file)?;
        framed(file, site.start, site.end, site.line, text, label)
    });
    if let Some(framed) = framed {
        return framed;
    }
    let location = places.place(frame).map(|place| Location {
        path: place.file.to_string(),
        span: None,
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
    file: &str,
    start: u32,
    end: u32,
    line: u32,
    text: &str,
    label: &str,
) -> Option<SiteText> {
    let first = start as usize;
    let last = (end as usize).max(first);
    let line_start = text.get(..first)?.rfind('\n').map_or(0, |i| i + 1);
    let line_end = text
        .get(first..)?
        .find('\n')
        .map_or(text.len(), |i| first + i);
    let source = text.get(line_start..line_end)?.trim_end_matches('\r');
    let before = visible(text.get(line_start..first)?);
    let covered = visible(text.get(first..last.min(line_end))?);
    let written = text
        .get(first..last)?
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    Some(SiteText {
        location: Some(Location {
            path: file.to_string(),
            span: Some((start, end.max(start))),
            line,
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
fn stack(places: &Places, frames: &[Frame]) -> Vec<String> {
    places
        .collapse(frames)
        .into_iter()
        .map(|run| match run.repeats {
            1 => run.place.to_string(),
            n => format!("{} ({n} times)", run.place),
        })
        .collect()
}

/// The callables a failure travelled through, outermost first, each once
/// per frame: `test "…" -> action place_order -> action reserve`.
fn path(places: &Places, frames: &[Frame]) -> String {
    let labels: Vec<&str> = frames
        .iter()
        .rev()
        .map(|frame| places.label(frame.callable))
        .collect();
    labels.join(" -> ")
}

impl Report {
    /// The report for an outcome that isn't a value.
    pub fn of(outcome: &Outcome, places: &Places, sources: &dyn Sources) -> Option<Report> {
        match outcome {
            Outcome::Value(_) => None,
            Outcome::Fault(fault) => Some(Report::fault(fault, places, sources)),
            Outcome::Failed(failure) => Some(Report::failure(failure, places, sources)),
            Outcome::Assertion(assertion) => Some(Report::assertion(assertion, places, sources)),
        }
    }

    pub fn fault(fault: &Fault, places: &Places, sources: &dyn Sources) -> Report {
        let site = fault.site();
        let label = site.map_or("", |frame| places.label(frame.callable));
        let caret = match &fault.kind {
            FaultKind::IntOverflow => "out of range".to_string(),
            FaultKind::DecimalOverflow => "past 38 digits".to_string(),
            FaultKind::TimeRange => "past the years a time holds".to_string(),
            FaultKind::ClockBackward => "moves the clock back".to_string(),
            FaultKind::RecursionLimit if fault.frames.len() < MAX_DEPTH => {
                format!("{MAX_INLINE_DEPTH} inline functions deep")
            }
            FaultKind::RecursionLimit => format!("{MAX_DEPTH} calls deep"),
            FaultKind::DeadlineExceeded => "out of time".to_string(),
            FaultKind::NotBuiltYet(_) => "not built yet".to_string(),
            FaultKind::Defect(_) => "here".to_string(),
            FaultKind::Store(_) => "the store refused".to_string(),
            FaultKind::Provider(_) => "the provider's answer".to_string(),
            FaultKind::CallInTransaction(_) => "not sent".to_string(),
            FaultKind::Diverged(_) => "differs from its record".to_string(),
            FaultKind::Crashed => "crashed here".to_string(),
        };
        let text = site.map(|frame| site_text(places, sources, frame, &caret));
        let written = text.as_ref().map_or("", |text| text.written.as_str());
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
            FaultKind::TimeRange => (
                format!(
                    "`{written}` gives a time outside the years 0000 to 9999, the years an `Instant` and a `CalendarDate` hold."
                ),
                "Bound the durations and counts that feed it with `max`".to_string(),
                "agent",
            ),
            FaultKind::ClockBackward => (
                format!(
                    "`{written}` moves the test's clock backwards; a clock only moves forward, as time does."
                ),
                "Advance by a duration that isn't negative, or start the test earlier with `clock:`"
                    .to_string(),
                "agent",
            ),
            // Fewer frames than the limit: it was the inline functions
            // running at once (chunk 8.4a).
            FaultKind::RecursionLimit if fault.frames.len() < MAX_DEPTH => (
                format!(
                    "{MAX_INLINE_DEPTH} inline functions were running one inside another, the most Jadpo allows, so the recursion stopped here instead of exhausting the stack."
                ),
                "Make the recursion end sooner, or recurse with `for` rather than through an inline function".to_string(),
                "agent",
            ),
            FaultKind::RecursionLimit => (
                format!(
                    "The call stack reached {MAX_DEPTH} frames, the most Jadpo allows, so the recursion stopped here instead of exhausting the stack."
                ),
                "Make the recursion end sooner, or recurse over smaller values".to_string(),
                "agent",
            ),
            FaultKind::DeadlineExceeded => (
                "The route's deadline passed while this ran, so it stopped here and nothing it did was kept.".to_string(),
                "Do less in one request, or give the route a longer `deadline:`".to_string(),
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
            FaultKind::Store(what) => (
                format!(
                    "The store refused `{written}` in a way the runtime can't classify: {what}."
                ),
                "Check the database it runs against, then run it again".to_string(),
                "human",
            ),
            FaultKind::Provider(what) => (
                format!(
                    "{what}, so the call's site can't give its operation's output or map its failure."
                ),
                "Correct the package's types or `failures:` to what the provider sends, and confirm them against a recorded answer".to_string(),
                "agent",
            ),
            FaultKind::CallInTransaction(call) => (
                format!(
                    "`{call}` would have left the process while this action's transaction was open, after its first write; no transaction is held while an outside call is out, so it wasn't sent."
                ),
                "Make the call before the action's first write".to_string(),
                "agent",
            ),
            FaultKind::Crashed => (
                "`jadpo test` crashed the run here, at a step of an action with several changing calls, and nothing started it again.".to_string(),
                "Run the action from a subscriber, or from a route whose API requires `Idempotency-Key`, so a crash resumes it".to_string(),
                "agent",
            ),
            FaultKind::Diverged(what) => (
                format!(
                    "This run no longer matches the record of the steps an earlier run of it took: {what}. It stopped before sending anything more, and is held for an operator."
                ),
                "Deploy code that makes the recorded steps again and retry it, or abandon it and set right at the provider what it did".to_string(),
                "human",
            ),
        };
        Report {
            word: "fault",
            subject: format!("{} in {label}", fault.kind.classification()),
            location: text.and_then(|text| text.location),
            detail: Detail::Fault {
                reason,
                next: format!("{next} ({decider})"),
                stack: stack(places, &fault.frames),
                // The one runtime problem the catalogue has a rule for.
                rule: matches!(fault.kind, FaultKind::NotBuiltYet(_)).then_some(ReportRule {
                    id: "toolchain.not_built_yet",
                    help: "docs/language/03-logic.html#toolchain-not-built-yet",
                }),
            },
        }
    }

    pub fn failure(failure: &Failure, places: &Places, sources: &dyn Sources) -> Report {
        let site = failure.site();
        let label = site.map_or("", |frame| places.label(frame.callable));
        let caret = match &failure.broken {
            Some(broken) => format!("breaks {broken}"),
            None => "rejected here".to_string(),
        };
        let location = site.and_then(|frame| site_text(places, sources, frame, &caret).location);
        let mut cause = Vec::new();
        let mut from = failure.cause.as_deref();
        while let Some(found) = from {
            let raised = found.site().map_or_else(String::new, |frame| {
                let place = places.place(frame).map_or_else(String::new, |place| {
                    format!(", {}:{}", place.file, place.line)
                });
                format!(" from {}{place}", places.label(frame.callable))
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
            cause.push(format!("`{}`{raised}{internal}", found.name));
            from = found.cause.as_deref();
        }
        Report {
            word: "failed",
            subject: format!("`{}` from {label}", failure.name),
            location,
            detail: Detail::Failure {
                reason: failure.broken.as_ref().map(|broken| {
                    format!(
                        "The value built here breaks {broken}, so `attempt` raises the failure its block maps that rule to."
                    )
                }),
                code: failure.code.clone(),
                kind: failure.kind.as_ref().map(|kind| {
                    let status = failure
                        .status
                        .map_or_else(String::new, |status| format!(" ({status})"));
                    format!("{kind}{status}")
                }),
                message: failure.message.clone(),
                client: (!failure.client.is_empty()).then(|| {
                    fields_text(failure.client.iter().map(|(name, value)| (name, value)))
                }),
                internal: (!failure.internal.is_empty()).then(|| {
                    fields_text(failure.internal.iter().map(|(name, value)| (name, value)))
                }),
                cause,
                path: path(places, &failure.frames),
            },
        }
    }

    pub fn assertion(assertion: &Assertion, places: &Places, sources: &dyn Sources) -> Report {
        let site = assertion.frames.first().copied();
        let label = site.map_or("", |frame| places.label(frame.callable));
        let (caret, values) = match &assertion.detail {
            AssertionDetail::False {
                comparison: Some(("==", actual, expected)),
            } => (
                "doesn't hold",
                vec![
                    ("expected", expected.to_string()),
                    ("actual", actual.to_string()),
                ],
            ),
            AssertionDetail::False {
                comparison: Some((op, left, right)),
            } => (
                "doesn't hold",
                vec![
                    ("left", left.to_string()),
                    ("right", right.to_string()),
                    ("compared", format!("{left} {op} {right} is false")),
                ],
            ),
            AssertionDetail::False { comparison: None } => ("is false", Vec::new()),
            AssertionDetail::Fails { expected, got } => (
                "didn't fail as expected",
                vec![
                    ("expected", format!("fails with `{expected}`")),
                    (
                        "actual",
                        match got {
                            Got::Value(value) => format!("gave {value}"),
                            Got::Failure(failure) => format!("failed with `{}`", failure.name),
                        },
                    ),
                ],
            ),
        };
        let location = site.and_then(|frame| site_text(places, sources, frame, caret).location);
        Report {
            word: "failed",
            subject: format!("an assertion in {label}"),
            location,
            detail: Detail::Assertion { values },
        }
    }

    /// `fault: int_overflow in function line_total`.
    pub fn headline(&self) -> String {
        format!("{}: {}", self.word, self.subject)
    }

    /// The `reason` line, when the report has one.
    pub fn reason(&self) -> Option<&str> {
        match &self.detail {
            Detail::Fault { reason, .. } => Some(reason),
            Detail::Failure { reason, .. } => reason.as_deref(),
            Detail::Assertion { .. } => None,
        }
    }

    /// The `next` line, when the report has one.
    pub fn next(&self) -> Option<&str> {
        match &self.detail {
            Detail::Fault { next, .. } => Some(next),
            _ => None,
        }
    }

    /// The failures a failure was mapped from, each as its line reads.
    pub fn cause(&self) -> &[String] {
        match &self.detail {
            Detail::Failure { cause, .. } => cause,
            _ => &[],
        }
    }

    /// Each label with its lines, in the order a report shows them.
    pub fn fields(&self) -> Vec<(&'static str, Vec<String>)> {
        let mut out = Vec::new();
        match &self.detail {
            Detail::Fault {
                reason,
                next,
                stack,
                rule,
            } => {
                out.push(("reason", vec![reason.clone()]));
                out.push(("next", vec![next.clone()]));
                out.push(("stack", stack.clone()));
                if let Some(rule) = rule {
                    out.push(("rule", vec![rule.id.to_string()]));
                    out.push(("help", vec![rule.help.to_string()]));
                }
            }
            Detail::Failure {
                reason,
                code,
                kind,
                message,
                client,
                internal,
                cause,
                path,
            } => {
                if let Some(reason) = reason {
                    out.push(("reason", vec![reason.clone()]));
                }
                out.push(("code", vec![code.clone()]));
                for (label, line) in [
                    ("kind", kind),
                    ("message", message),
                    ("client", client),
                    ("internal", internal),
                ] {
                    if let Some(line) = line {
                        out.push((label, vec![line.clone()]));
                    }
                }
                if !cause.is_empty() {
                    out.push(("cause", cause.clone()));
                }
                out.push(("path", vec![path.clone()]));
            }
            Detail::Assertion { values } => {
                out.extend(
                    values
                        .iter()
                        .map(|(label, value)| (*label, vec![value.clone()])),
                );
            }
        }
        out
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
        for (label, lines) in self.fields() {
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
