//! A failed test as data that leaves the thread it ran on (values are
//! shared behind `Rc` there, so everything here is text): its stable code,
//! where in the test it failed, and what area 13 says a report shows: the
//! assertion with each part's value, expected and actual, a record's
//! differences field by field, or the failure or fault that happened
//! instead with its `.jadpo` frames. It is read from the runtime's outcome,
//! the frame table and the sources alone, so it is the same whichever
//! executor ran the test.

use jadpo_diag::{Rule, rule};
use jadpo_interp::{CallError, Value};
use jadpo_runtime::crashes::{Boundary, Reached};
use jadpo_runtime::test_steps::Undelivered;
use jadpo_runtime::{
    Assertion, AssertionDetail, Failure, Fault, FaultKind, Frame, Got, Outcome, Places, Report,
    Sources,
};

use crate::diff::diff;
use crate::{Rerun, Test};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestFailure {
    /// `test.assertion_failed`, `test.unexpected_failure`, `test.fault` or
    /// `test.subscriber_failed`.
    pub rule: &'static Rule,
    /// For `test.subscriber_failed`: the delivery between the test's steps
    /// that failed for good, whose failure or fault `detail` holds.
    pub delivery: Option<Undelivered>,
    /// The crash the run that failed was made with, at a step of an
    /// action with several changing calls (chunk 7.5d); `None` for the
    /// run without one.
    pub crash: Option<CrashPoint>,
    /// Where in the test it failed.
    pub location: Located,
    pub detail: Detail,
    /// The command that runs this test alone in the same order.
    pub rerun: String,
    /// The databases kept for inspection (`--keep-failed`).
    pub kept: Vec<String>,
}

/// Where `jadpo test` crashed a run of a test that reaches an action with
/// several changing calls (area 13, "Provider calls and capabilities in
/// tests"; chunk 7.5d): the step and the boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashPoint {
    pub step: Reached,
    pub boundary: Boundary,
}

impl CrashPoint {
    /// What happened, as the report says it.
    pub fn text(&self) -> String {
        let step = format!(
            "step {} of `{}`, `{}`,",
            self.step.position, self.step.action, self.step.operation
        );
        let when = match self.boundary {
            Boundary::BeforeSend => format!("before {step} went out"),
            Boundary::AfterSend => format!("after {step} was answered, before its answer was kept"),
            Boundary::AfterAnswer => format!("after {step} had its answer kept"),
        };
        format!("{when}; then it ran again")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    Assertion(Box<AssertionFailure>),
    /// A failure `attempt` passed on out of the test.
    Failure(Box<FailureInfo>),
    Fault(Box<FaultInfo>),
    /// A faked provider operation called more or fewer times than the
    /// test's `fakes:` scripts (chunk 7.2d).
    Fake(Box<FakeInfo>),
    /// A recording its operation's decoders don't read (chunk 7.2d2).
    Recording(Box<RecordingInfo>),
}

/// Why a recording of one of the app's own integrations isn't read by its
/// operation's decoders (area 11, "Provenance and confirmation"; chunk
/// 7.2d2): `test.recording_undecodable` or `test.round_trip_failed`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingInfo {
    /// `pickups.create`.
    pub operation: String,
    /// `SwiftCouriers.v1`.
    pub package: String,
    /// What happened, as the report says it.
    pub subject: String,
    /// For a field's decoder: the field, dotted from the output, and the
    /// rule it broke.
    pub field: Option<String>,
    pub rule: Option<String>,
    /// For `test.round_trip_failed`: the value recorded, and the one its
    /// `encode:` writes back.
    pub recorded: Option<String>,
    pub written: Option<String>,
    pub reason: String,
    pub next: String,
    /// For a recording of a prompt's answer (chunk 5.5v4): the prompt's
    /// name and the recording's.
    pub prompt: Option<(String, String)>,
}

/// The failure a recording of a prompt's answer at `test.file` gives when
/// `unread` says why it isn't read (area 21, "Tests"; chunk 5.5v4):
/// `test.recording_undecodable`.
pub(crate) fn prompt_recording(
    sources: &dyn Sources,
    test: &Test,
    unread: &jadpo_runtime::prompt_recordings::Unread,
    rerun: &Rerun,
) -> TestFailure {
    use jadpo_runtime::prompt_recordings::Unread;
    let (prompt, name) = jadpo_db::prompt_recording(&test.file)
        .map(|(_, prompt, name)| (prompt.to_string(), name.to_string()))
        .unwrap_or_default();
    let called = format!("`{prompt}`'s recording `{name}`");
    let shape = format!(
        "A recording is one JSON object: the model's `answer`, written as `json.write` writes its type, and the `version` of the prompt it answered, as `jadpo ai show {prompt}` prints it."
    );
    let (subject, reason, next, field, rule) = match unread {
        Unread::NoPrompt => (
            format!("No prompt `{prompt}` is beside {called}"),
            "A recording is kept beside the prompt it records, in `recordings/<prompt>/<name>.json` in the prompt's own `prompts/` folder, and no prompt of its folder's name is there.".to_string(),
            "Move it beside its prompt, or remove it if the prompt is gone (agent)".to_string(),
            None,
            None,
        ),
        Unread::NotJson => (
            format!("{called} isn't a JSON object"),
            shape,
            "Write it as `{ \"answer\": ..., \"version\": ... }` (agent)".to_string(),
            None,
            None,
        ),
        Unread::NoAnswer => (
            format!("{called} keeps no `answer`"),
            shape,
            "Add the model's `answer`, a value of the type the prompt is asked for as (agent)"
                .to_string(),
            None,
            None,
        ),
        Unread::NoVersion => (
            format!("{called} doesn't say which version of the prompt it answered"),
            format!(
                "A recording keeps the `version` of the prompt it was made from, so a change to `{prompt}` shows which recordings may no longer be what the model says."
            ),
            format!("Add `\"version\"`, as `jadpo ai show {prompt}` prints it (agent)"),
            None,
            None,
        ),
        Unread::NotAsked => (
            format!("No call asks `{prompt}` for an answer to read {called} as"),
            format!(
                "A recording is decoded through each type an `ai.generate` call of its prompt answers as, and no action asks `{prompt}`, so nothing would read it."
            ),
            format!("Remove it, or ask `{prompt}` from an action (agent)"),
            None,
            None,
        ),
        Unread::Undecoded { field, rule } => (
            if field.is_empty() {
                format!("{called} doesn't decode: `{rule}`")
            } else {
                format!("{called} doesn't decode: `{rule}` on `{field}`")
            },
            "Every build decodes every recording through each type its prompt is asked for as, every rule included, so an answer no call could read, or one its type has changed under since it was recorded, shows here before a test answers with it.".to_string(),
            "Correct its answer to the type, or record it again (agent)".to_string(),
            (!field.is_empty()).then(|| field.clone()),
            Some(rule.clone()),
        ),
    };
    TestFailure {
        rule: &rule::TEST_RECORDING_UNDECODABLE,
        delivery: None,
        crash: None,
        location: Located::new(sources, &test.file, None, test.line, ""),
        rerun: rerun.command(&test.file, test.line),
        kept: Vec::new(),
        detail: Detail::Recording(Box::new(RecordingInfo {
            operation: prompt.clone(),
            package: String::new(),
            subject,
            field,
            rule,
            recorded: None,
            written: None,
            reason,
            next,
            prompt: Some((prompt, name)),
        })),
    }
}

/// The failure a recording at `test.file`, of `package`'s operation
/// `operation`, gives when `unread` says why its decoders don't read it.
pub(crate) fn recording(
    sources: &dyn Sources,
    test: &Test,
    (package, operation): (&str, &str),
    unread: &jadpo_runtime::recorded::Unread,
    rerun: &Rerun,
) -> TestFailure {
    use jadpo_runtime::recorded::Unread;
    let mut info = RecordingInfo {
        operation: operation.to_string(),
        package: package.to_string(),
        subject: String::new(),
        field: None,
        rule: None,
        recorded: None,
        written: None,
        reason: String::new(),
        next: format!(
            "Correct the operation against the provider's documentation, or record `{operation}` again with `jadpo package record` (agent)"
        ),
        prompt: None,
    };
    let mut failed = &rule::TEST_RECORDING_UNDECODABLE;
    match unread {
        Unread::NoOperation => {
            info.subject = format!("`{package}` has no operation `{operation}` to read it");
            info.reason = "A recording is named after the operation it records, `recordings/<resource>.<operation>.json`, and only that operation's decoders read it.".to_string();
            info.next = format!(
                "Rename it after one of `{package}`'s operations, or remove it if the operation is gone (agent)"
            );
        }
        Unread::NotJson => {
            info.subject = format!("`{operation}`'s recording isn't JSON");
            info.reason = "A recording is the provider's answer as it sent it, one JSON document, with its values replaced by placeholders such as `\"{Text}\"`.".to_string();
        }
        Unread::NothingAt(path) => {
            info.subject = format!("`{operation}`'s recording has nothing at `{path}`");
            info.reason = format!(
                "The operation reads its answer from `output_at: \"{path}\"`, and the recorded answer holds nothing there."
            );
        }
        Unread::Undecoded { field, rule } => {
            info.subject = if field.is_empty() {
                format!("`{operation}`'s recording doesn't decode: `{rule}`")
            } else {
                format!("`{operation}`'s recording doesn't decode: `{rule}` on `{field}`")
            };
            info.field = (!field.is_empty()).then(|| field.clone());
            info.rule = Some(rule.clone());
            info.reason = "Every decoder decodes every recorded answer: the operation's output type, or a decoder of its own, refuses what the provider sent here, so a real answer like it would fault the call.".to_string();
            info.next = "Correct the type or its decoder against the recording and the provider's documentation (agent)".to_string();
        }
        Unread::DecoderFailed(how) => {
            info.subject =
                format!("`{operation}`'s `decode:` fails on its recording, ending with {how}");
            info.reason = "Every decoder decodes every recorded answer: the operation's own `decode:` turns the answer as sent into its output, and it doesn't for this one.".to_string();
            info.next = "Correct the operation's `decode:` so it reads the recorded answer (agent)"
                .to_string();
        }
        Unread::RoundTrip {
            decode,
            encode,
            recorded,
            back,
        } => {
            failed = &rule::TEST_ROUND_TRIP_FAILED;
            info.subject =
                format!("`{decode}` and `{encode}` don't round-trip `{operation}`'s recording");
            info.recorded = Some(recorded.clone());
            info.written = Some(back.clone());
            info.reason = format!(
                "A field's `encode:` writes back what its `decode:` read, so what the app sends is what the provider sent; `{encode}` doesn't give back the value `{decode}` read from the recording."
            );
            info.next =
                format!("Correct `{decode}` or `{encode}` so each undoes the other (agent)");
        }
    }
    TestFailure {
        rule: failed,
        delivery: None,
        crash: None,
        location: Located::new(sources, &test.file, None, test.line, ""),
        rerun: rerun.command(&test.file, test.line),
        kept: Vec::new(),
        detail: Detail::Recording(Box::new(info)),
    }
}

/// The failure a test gives when a crash at `point` left its fake's
/// effects other than the run without one did (chunk 7.5d): a keyed call
/// that acted again under a new key, which the runtime rules out, so a
/// defect in Jadpo, reported as a fault.
pub(crate) fn effects_differ(
    sources: &dyn Sources,
    test: &Test,
    point: CrashPoint,
    first: &[(String, usize)],
    crashed: &[(String, usize)],
    rerun: &Rerun,
) -> TestFailure {
    let times = |effects: &[(String, usize)], name: &str| {
        effects
            .iter()
            .find(|(operation, _)| operation == name)
            .map_or(0, |(_, times)| *times)
    };
    let operation = crashed
        .iter()
        .chain(first)
        .map(|(operation, _)| operation.as_str())
        .find(|name| times(first, name) != times(crashed, name))
        .unwrap_or_default()
        .to_string();
    let said = |n: usize| match n {
        0 => "never".to_string(),
        1 => "once".to_string(),
        2 => "twice".to_string(),
        n => format!("{n} times"),
    };
    let location = Located::new(sources, &test.file, None, test.line, "");
    TestFailure {
        rule: &rule::TEST_FAULT,
        delivery: None,
        crash: Some(point),
        rerun: rerun.command(&location.path, location.line),
        kept: Vec::new(),
        location,
        detail: Detail::Fault(Box::new(FaultInfo {
            kind: "internal_fault".to_string(),
            subject: format!(
                "`{operation}` acted {} at the provider, {} without a crash",
                said(times(crashed, &operation)),
                said(times(first, &operation))
            ),
            rule: None,
            at: None,
            reason: Some("A step sent again after a crash goes under the key it went under first, so the provider acts once; this one acted under a new key, which the runtime should never send.".to_string()),
            next: Some("Report it to Jadpo with this test (human)".to_string()),
            stack: Vec::new(),
        })),
    }
}

/// How a scripted provider operation was used (area 13, "Fakes for
/// integrations"): `test.fake_exhausted` or `test.fake_unused`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeInfo {
    /// `couriers.pickups.create`.
    pub operation: String,
    pub scripted: usize,
    pub called: usize,
}

impl FakeInfo {
    /// What happened, as the report says it.
    pub fn subject(&self) -> String {
        let times = |n: usize| match n {
            0 => "never".to_string(),
            1 => "once".to_string(),
            2 => "twice".to_string(),
            n => format!("{n} times"),
        };
        let answers = match self.scripted {
            1 => "1 answer".to_string(),
            n => format!("{n} answers"),
        };
        format!(
            "`{}` was called {}, and its `fakes:` scripts {answers}",
            self.operation,
            times(self.called)
        )
    }

    /// Why that fails the test.
    pub fn reason(&self) -> String {
        if self.called > self.scripted {
            "A scripted operation answers each call with the next scripted answer, so a call past the last means the code calls it more often than the test says; that call was answered with the generated default.".to_string()
        } else {
            "Each scripted answer is for a call the test expects, so one left over means the code calls the operation less often than the test says.".to_string()
        }
    }

    /// What to do.
    pub fn next(&self) -> String {
        if self.called > self.scripted {
            format!(
                "Script an answer for every call of `{}`, or find why the code calls it again (agent)",
                self.operation
            )
        } else {
            format!(
                "Remove the answers no call takes, or find why the code doesn't call `{}` (agent)",
                self.operation
            )
        }
    }
}

/// The failure a scripted operation's use gives a test that otherwise
/// ended as `failed` says (chunk 7.2d): one called past its script fails
/// it, whatever else happened, since that is what went wrong first; one
/// with answers left fails a test that otherwise passed.
pub(crate) fn fakes_used(
    places: &Places,
    sources: &dyn Sources,
    test: &Test,
    used: &[jadpo_runtime::fakes::Usage],
    failed: Option<Box<TestFailure>>,
    rerun: &Rerun,
) -> Option<Box<TestFailure>> {
    let Some((usage, rule)) = fake_rule(
        used.iter().map(|usage| (usage.exhausted(), usage.unused())),
        failed.is_some(),
    )
    .and_then(|(at, rule)| Some((used.get(at)?, rule))) else {
        return failed;
    };
    let location = located_frame(places, sources, usage.from)
        .unwrap_or_else(|| Located::new(sources, &test.file, None, test.line, ""));
    Some(Box::new(TestFailure {
        rule,
        delivery: None,
        crash: None,
        rerun: rerun.command(&location.path, location.line),
        kept: failed.map(|failed| failed.kept).unwrap_or_default(),
        location,
        detail: Detail::Fake(Box::new(FakeInfo {
            operation: usage.operation.clone(),
            scripted: usage.scripted,
            called: usage.called,
        })),
    }))
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
    /// Whether the statement goes on past this line.
    pub continues: bool,
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

/// A site, in the one location shape every `--json` document uses
/// (diagnostics too): its project-relative path, byte range, 1-based start
/// and end line and column, and the label beside its carets; with its line
/// of source for the code frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub line: u32,
    /// 1-based, in characters.
    pub column: usize,
    pub end_line: u32,
    pub end_column: usize,
    pub source: String,
    /// How many characters of the line the carets cover.
    pub length: usize,
    pub label: String,
}

impl Located {
    /// The site at `span` of `path` (or, without one, the start of `line`),
    /// its positions read from the sources.
    pub(crate) fn new(
        sources: &dyn Sources,
        path: &str,
        span: Option<(u32, u32)>,
        line: u32,
        label: &str,
    ) -> Located {
        let text = sources.text(path).unwrap_or_default();
        let (start, end) = span.unwrap_or_else(|| {
            let offset = line_offset(text, line);
            (offset, offset)
        });
        let from = jadpo_term::Position::of(text, start);
        let to = jadpo_term::Position::of(text, end);
        let source = jadpo_term::line_text(text, from.line).to_string();
        Located {
            path: path.to_string(),
            start,
            end,
            line: from.line as u32,
            column: from.column,
            end_line: to.line as u32,
            end_column: to.column,
            length: if to.line == from.line {
                to.column.saturating_sub(from.column).max(1)
            } else {
                source
                    .chars()
                    .count()
                    .saturating_sub(from.column - 1)
                    .max(1)
            },
            source,
            label: label.to_string(),
        }
    }
}

/// The byte offset where 1-based `line` starts.
fn line_offset(text: &str, line: u32) -> u32 {
    let mut offset = 0;
    for _ in 1..line {
        match text[offset..].find('\n') {
            Some(i) => offset += i + 1,
            None => break,
        }
    }
    offset as u32
}

/// One frame of a fault's stack: the site, the callable, and how many
/// frames in a row it stands for (a recursion shown once).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackFrame {
    pub location: Located,
    /// `function line_total`, `test "…"`.
    pub callable: String,
    pub repeats: usize,
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
    /// Innermost first, a run of one frame shown once with how many times
    /// it repeats.
    pub stack: Vec<StackFrame>,
}

/// Where a frame stands, with its code frame; without a site, the start
/// of its line.
fn located_frame(places: &Places, sources: &dyn Sources, frame: Frame) -> Option<Located> {
    let place = places.place(frame)?;
    let span = places
        .site(frame)
        .map(|(_, site)| (site.start, site.end.max(site.start)));
    Some(Located::new(sources, place.file, span, place.line, ""))
}

/// Where the test frame stands: the outermost frame's site.
fn place_in_test(places: &Places, sources: &dyn Sources, frames: &[Frame], test: &Test) -> Located {
    match frames.last().and_then(|frame| places.site(*frame)) {
        Some((file, site)) => Located::new(
            sources,
            file,
            Some((site.start, site.end.max(site.start))),
            site.line,
            "",
        ),
        None => Located::new(sources, &test.file, None, test.line, ""),
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

/// How a test's run ended, which decides the rule it fails with: the
/// same whichever executor ran it and wherever, so a run inside workerd
/// (chunk 8.2e) is judged as one here is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ending {
    /// It gave a value: it passed, unless a fake says otherwise.
    Value,
    Assertion,
    Failed,
    Fault,
}

impl Ending {
    pub(crate) fn of(outcome: &Outcome) -> Ending {
        match outcome {
            Outcome::Value(_) => Ending::Value,
            Outcome::Assertion(_) => Ending::Assertion,
            Outcome::Failed(_) => Ending::Failed,
            Outcome::Fault(_) => Ending::Fault,
        }
    }

    /// The rule a run that ended so fails with, `undelivered` when a
    /// delivery between its steps failed for good (chunk 6.11): `None`
    /// when it gave a value.
    pub(crate) fn rule(self, undelivered: bool) -> Option<&'static Rule> {
        match self {
            Ending::Value => None,
            _ if undelivered => Some(&rule::TEST_SUBSCRIBER_FAILED),
            Ending::Assertion => Some(&rule::TEST_ASSERTION_FAILED),
            Ending::Failed => Some(&rule::TEST_UNEXPECTED_FAILURE),
            Ending::Fault => Some(&rule::TEST_FAULT),
        }
    }
}

/// Which scripted operation's use fails a test, given each one's use as
/// whether it was called past its script and whether it has answers left,
/// and whether the test `failed` otherwise (chunk 7.2d): the first called
/// past its script, whatever else happened, since that is what went wrong
/// first; else the first with answers left, for a test that otherwise
/// passed. Its place among them and the rule.
pub(crate) fn fake_rule(
    used: impl IntoIterator<Item = (bool, bool)>,
    failed: bool,
) -> Option<(usize, &'static Rule)> {
    let used: Vec<(bool, bool)> = used.into_iter().collect();
    if let Some(at) = used.iter().position(|(exhausted, _)| *exhausted) {
        return Some((at, &rule::TEST_FAKE_EXHAUSTED));
    }
    if failed {
        return None;
    }
    used.iter()
        .position(|(_, unused)| *unused)
        .map(|at| (at, &rule::TEST_FAKE_UNUSED))
}

/// How `test` failed, from the outcome it ended with; `undelivered` when
/// that outcome is a delivery's between its steps, which failed for good
/// (chunk 6.11).
pub(crate) fn failure(
    places: &Places,
    sources: &dyn Sources,
    test: &Test,
    outcome: &Outcome,
    rerun: &Rerun,
    undelivered: Option<&Undelivered>,
) -> TestFailure {
    let mut failure = ended(places, sources, test, outcome, rerun);
    if let Some(rule) = Ending::of(outcome).rule(undelivered.is_some()) {
        failure.rule = rule;
    }
    failure.delivery = undelivered.cloned();
    failure
}

fn ended(
    places: &Places,
    sources: &dyn Sources,
    test: &Test,
    outcome: &Outcome,
    rerun: &Rerun,
) -> TestFailure {
    match outcome {
        Outcome::Assertion(assertion) => {
            let location = place_in_test(places, sources, &assertion.frames, test);
            TestFailure {
                rule: &rule::TEST_ASSERTION_FAILED,
                delivery: None,
                crash: None,
                rerun: rerun.command(&location.path, location.line),
                kept: Vec::new(),
                location,
                detail: Detail::Assertion(Box::new(assertion_failure(places, sources, assertion))),
            }
        }
        Outcome::Failed(failure) => {
            let location = place_in_test(places, sources, &failure.frames, test);
            TestFailure {
                rule: &rule::TEST_UNEXPECTED_FAILURE,
                delivery: None,
                crash: None,
                rerun: rerun.command(&location.path, location.line),
                kept: Vec::new(),
                location,
                detail: Detail::Failure(Box::new(failure_info(places, sources, failure))),
            }
        }
        Outcome::Fault(fault) => {
            let location = place_in_test(places, sources, &fault.frames, test);
            TestFailure {
                rule: &rule::TEST_FAULT,
                delivery: None,
                crash: None,
                rerun: rerun.command(&location.path, location.line),
                kept: Vec::new(),
                location,
                detail: Detail::Fault(Box::new(fault_info(places, sources, fault))),
            }
        }
        // A test that gives a value passed; the caller never asks.
        Outcome::Value(_) => not_runnable(
            sources,
            test,
            &CallError::Unknown("a test that passed".to_string()),
            rerun,
        ),
    }
}

/// A test the executor couldn't start: a defect, since every test the
/// graph lists has a body.
pub(crate) fn not_runnable(
    sources: &dyn Sources,
    test: &Test,
    error: &CallError,
    rerun: &Rerun,
) -> TestFailure {
    TestFailure {
        rule: &rule::TEST_FAULT,
        delivery: None,
        crash: None,
        location: Located::new(sources, &test.file, None, test.line, ""),
        rerun: rerun.command(&test.file, test.line),
        kept: Vec::new(),
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

fn located(sources: &dyn Sources, report: &Report) -> Option<Located> {
    report.location.as_ref().map(|location| {
        let mut located = Located::new(
            sources,
            &location.path,
            location.span,
            location.line,
            &location.label,
        );
        // The report measured the carets as the line is shown.
        if !location.source.is_empty() {
            located.length = location.length;
        }
        located
    })
}

fn texts(fields: &[(String, Value)]) -> Vec<(String, String)> {
    fields
        .iter()
        .map(|(name, value)| (name.clone(), value.to_string()))
        .collect()
}

fn failure_info(places: &Places, sources: &dyn Sources, failure: &Failure) -> FailureInfo {
    let report = Report::failure(failure, places, sources);
    FailureInfo {
        name: failure.name.clone(),
        code: failure.code.clone(),
        kind: failure.kind.clone(),
        message: failure.message.clone(),
        client: texts(&failure.client),
        internal: texts(&failure.internal),
        reason: report.reason().map(str::to_string),
        at: located(sources, &report),
        path: failure
            .frames
            .iter()
            .rev()
            .map(|frame| places.label(frame.callable).to_string())
            .collect(),
        cause: report.cause().to_vec(),
        subject: report.subject,
    }
}

fn fault_info(places: &Places, sources: &dyn Sources, fault: &Fault) -> FaultInfo {
    let report = Report::fault(fault, places, sources);
    let stack = places
        .collapse(&fault.frames)
        .into_iter()
        .filter_map(|run| {
            Some(StackFrame {
                location: located_frame(places, sources, run.frame)?,
                callable: run.place.label.to_string(),
                repeats: run.repeats,
            })
        })
        .collect();
    FaultInfo {
        kind: fault.kind.classification().to_string(),
        rule: matches!(fault.kind, FaultKind::NotBuiltYet(_))
            .then_some(rule::TOOLCHAIN_NOT_BUILT_YET.id),
        at: located(sources, &report),
        reason: report.reason().map(str::to_string),
        next: report.next().map(str::to_string),
        subject: report.subject,
        stack,
    }
}

fn assertion_failure(
    places: &Places,
    sources: &dyn Sources,
    assertion: &Assertion,
) -> AssertionFailure {
    let statement = assertion
        .frames
        .first()
        .and_then(|frame| places.site(*frame));
    let (source, lines, parts) = match statement {
        Some((file, site)) => {
            let text = sources.text(file).unwrap_or_default();
            shown(
                text,
                site.start as usize,
                site.end as usize,
                site.line,
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
                    failure.instead = Some(Box::new(failure_info(places, sources, instead)));
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
                continues: i + 1 < raw_lines.len(),
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
    // A statement over several lines (a record written member by member)
    // shows its first line: the values hang under it, and `--json` keeps
    // the rest.
    lines.truncate(1);
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
