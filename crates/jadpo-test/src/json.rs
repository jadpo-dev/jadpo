//! A run as data for `--json` (area 13: "versioned JSON"): the `data` of
//! the envelope every command prints, so `schemaVersion`, `ok` and the
//! diagnostics sit around it as for `jadpo check --json`. Values are
//! written as Jadpo writes them, whole; tests are listed in the order
//! they ran.

use jadpo_term::Json;

use crate::outcome::{AssertionFailure, Detail, FailureInfo, FaultInfo, Located};
use crate::{Run, Selection, TestKind, TestResult};

/// The run as the `data` of a `--json` document.
pub fn json(run: &Run) -> Json {
    let mut ran = std::collections::BTreeMap::new();
    for result in &run.results {
        *ran.entry(result.test.kind).or_insert(0usize) += 1;
    }
    let kinds = |counts: &dyn Fn(TestKind) -> usize| {
        Json::object(
            TestKind::ALL
                .into_iter()
                .map(|kind| (kind.word(), counts(kind).into())),
        )
    };
    Json::object([
        (
            "seed",
            Json::Int(i64::try_from(run.seed).unwrap_or(i64::MAX)),
        ),
        ("root", Json::from(run.root.as_deref().unwrap_or("."))),
        ("selection", selection(&run.selection)),
        (
            "counts",
            Json::object([
                ("total", run.total().into()),
                ("ran", run.results.len().into()),
                ("passed", run.passed().into()),
                ("failed", run.failed().into()),
                (
                    "totalByKind",
                    kinds(&|kind| run.total.get(&kind).copied().unwrap_or(0)),
                ),
                (
                    "ranByKind",
                    kinds(&|kind| ran.get(&kind).copied().unwrap_or(0)),
                ),
            ]),
        ),
        (
            "tests",
            Json::array(
                run.results
                    .iter()
                    .enumerate()
                    .map(|(i, result)| test(i + 1, result)),
            ),
        ),
    ])
}

fn selection(selection: &Selection) -> Json {
    Json::object([
        ("path", selection.path.as_ref().into()),
        ("line", selection.line.into()),
        ("name", selection.name.as_ref().into()),
        ("kind", selection.kind.map(TestKind::word).into()),
        ("failed", selection.failed.into()),
    ])
}

fn test(order: usize, result: &TestResult) -> Json {
    let test = &result.test;
    let mut entries = vec![
        ("order".to_string(), order.into()),
        ("key".to_string(), Json::from(&test.key)),
        ("name".to_string(), Json::from(&test.name)),
        ("file".to_string(), Json::from(&test.file)),
        ("line".to_string(), test.line.into()),
        ("endLine".to_string(), test.end_line.into()),
        ("kind".to_string(), test.kind.word().into()),
        (
            "outcome".to_string(),
            if result.failure.is_some() {
                "failed"
            } else {
                "passed"
            }
            .into(),
        ),
    ];
    if let Some(failure) = &result.failure {
        let (assertion, raised, fault) = match &failure.detail {
            Detail::Assertion(assertion) => (assertion_json(assertion), Json::Null, Json::Null),
            Detail::Failure(info) => (Json::Null, failure_json(info), Json::Null),
            Detail::Fault(info) => (Json::Null, Json::Null, fault_json(info)),
            Detail::Fake(_) | Detail::Recording(_) => (Json::Null, Json::Null, Json::Null),
        };
        let fake = match &failure.detail {
            Detail::Fake(info) => Json::object([
                ("operation", Json::from(&info.operation)),
                ("scripted", Json::from(info.scripted as i64)),
                ("called", Json::from(info.called as i64)),
            ]),
            _ => Json::Null,
        };
        let recording = match &failure.detail {
            Detail::Recording(info) => Json::object([
                ("package", Json::from(&info.package)),
                ("operation", Json::from(&info.operation)),
                ("field", info.field.as_ref().into()),
                ("rule", info.rule.as_ref().into()),
                ("recorded", info.recorded.as_ref().into()),
                ("written", info.written.as_ref().into()),
                ("message", Json::from(&info.subject)),
            ]),
            _ => Json::Null,
        };
        entries.push((
            "failure".to_string(),
            Json::object([
                ("code", failure.rule.id.into()),
                ("help", failure.rule.help().into()),
                ("location", location(&failure.location)),
                ("assertion", assertion),
                ("failure", raised),
                ("fault", fault),
                ("fake", fake),
                ("recording", recording),
                (
                    "delivery",
                    failure.delivery.as_ref().map_or(Json::Null, |delivery| {
                        Json::object([
                            ("event", Json::from(&delivery.event)),
                            ("subscriber", Json::from(&delivery.subscriber)),
                            ("attempt", Json::from(i64::from(delivery.attempt))),
                            ("attempts", Json::from(i64::from(delivery.attempts))),
                        ])
                    }),
                ),
                ("rerun", Json::from(&failure.rerun)),
                ("kept", Json::array(failure.kept.iter().map(String::as_str))),
            ]),
        ));
    }
    Json::Object(entries)
}

fn pairs(fields: &[(String, String)]) -> Json {
    Json::object(
        fields
            .iter()
            .map(|(name, value)| (name.clone(), Json::from(value))),
    )
}

/// The location object every `--json` document shares, diagnostics'
/// included.
fn location(located: &Located) -> Json {
    Json::object([
        ("path", Json::from(&located.path)),
        ("start", located.start.into()),
        ("end", located.end.into()),
        ("line", located.line.into()),
        ("column", located.column.into()),
        ("endLine", located.end_line.into()),
        ("endColumn", located.end_column.into()),
        ("label", Json::from(&located.label)),
    ])
}

fn located(located: &Option<Located>) -> Json {
    located.as_ref().map_or(Json::Null, location)
}

fn assertion_json(assertion: &AssertionFailure) -> Json {
    Json::object([
        ("source", Json::from(&assertion.source)),
        ("operator", assertion.operator.as_ref().into()),
        ("expected", assertion.expected.as_ref().into()),
        ("actual", assertion.actual.as_ref().into()),
        ("fails", assertion.fails.as_ref().into()),
        ("returned", assertion.returned.as_ref().into()),
        (
            "instead",
            assertion
                .instead
                .as_ref()
                .map_or(Json::Null, |info| failure_json(info)),
        ),
        (
            "parts",
            Json::array(assertion.parts.iter().map(|part| {
                Json::object([
                    ("source", Json::from(&part.source)),
                    ("line", part.line.into()),
                    ("column", part.column.into()),
                    ("value", Json::from(&part.value)),
                ])
            })),
        ),
        (
            "diff",
            Json::array(assertion.diff.iter().map(|change| {
                Json::object([
                    ("change", change.mark.word().into()),
                    ("path", Json::from(&change.path)),
                    ("expected", change.expected.as_ref().into()),
                    ("actual", change.actual.as_ref().into()),
                ])
            })),
        ),
    ])
}

fn failure_json(info: &FailureInfo) -> Json {
    Json::object([
        ("name", Json::from(&info.name)),
        ("code", Json::from(&info.code)),
        ("kind", info.kind.as_ref().into()),
        ("message", info.message.as_ref().into()),
        ("client", pairs(&info.client)),
        ("internal", pairs(&info.internal)),
        ("reason", info.reason.as_ref().into()),
        ("at", located(&info.at)),
        ("path", Json::array(info.path.iter())),
        ("cause", Json::array(info.cause.iter())),
    ])
}

fn fault_json(info: &FaultInfo) -> Json {
    Json::object([
        ("kind", Json::from(&info.kind)),
        ("message", Json::from(&info.subject)),
        ("rule", info.rule.into()),
        ("at", located(&info.at)),
        ("reason", info.reason.as_ref().into()),
        ("next", info.next.as_ref().into()),
        (
            "stack",
            Json::array(info.stack.iter().map(|frame| {
                Json::object([
                    ("location", location(&frame.location)),
                    ("callable", Json::from(&frame.callable)),
                    ("repeats", frame.repeats.into()),
                ])
            })),
        ),
    ])
}

/// What each field of `jadpo test --json` means, for `jadpo help --json`
/// (area 13, "What a failed test reports"): the `data` of a run, of a
/// refused run, of nothing to rerun and of a watch step, each test, each
/// failure, and the one location shape every document shares.
pub fn schema() -> Json {
    let fields = |pairs: &[(&str, &str)]| {
        Json::object(
            pairs
                .iter()
                .map(|(name, meaning)| (name.to_string(), Json::from(*meaning))),
        )
    };
    Json::object([
        ("command", "test".into()),
        (
            "data",
            fields(&[
                ("seed", "The order seed the run used"),
                (
                    "root",
                    "The project folder as seen from where the command ran (\".\" from inside it); every path is relative to it",
                ),
                (
                    "selection",
                    "What was asked for: path, line, name, kind, failed",
                ),
                (
                    "counts",
                    "total, ran, passed, failed, and totalByKind and ranByKind for pure, action, route and schedule",
                ),
                ("tests", "Every test that ran, in the order it ran"),
            ]),
        ),
        (
            "refused",
            fields(&[
                (
                    "ran",
                    "false: the project has errors, which are the document's diagnostics",
                ),
                ("root", "As for a run"),
                ("fileCount", "How many files the project holds"),
                ("errors", "How many errors"),
            ]),
        ),
        (
            "nothingToRerun",
            fields(&[
                ("selection", "{ failed: true }"),
                (
                    "gone",
                    "How many tests failed last time and are no longer in the project; 0 when none failed",
                ),
                ("tests", "Empty"),
            ]),
        ),
        (
            "watch",
            fields(&[
                ("event", "start or change"),
                (
                    "changed",
                    "The files that changed, each { path, change: added, saved or removed }",
                ),
                (
                    "reached",
                    "How many tests reach the change; null for the first run and while the build has errors",
                ),
                (
                    "failing",
                    "How many tests fail now, as far as the session has run them",
                ),
                ("files", "How many files the project holds"),
                (
                    "fixed",
                    "Whether the build had errors before this step and checks now",
                ),
            ]),
        ),
        (
            "test",
            fields(&[
                ("order", "Its place in the run, from 1"),
                (
                    "key",
                    "Its identity: the subject it sits beside and its sentence",
                ),
                ("name", "Its sentence"),
                ("file", "Its file"),
                ("line", "Its first line"),
                ("endLine", "Its last line"),
                (
                    "kind",
                    "pure, action, route, schedule or recording (a recording of the app's own integration, read by its operation's decoders)",
                ),
                ("outcome", "passed or failed"),
                ("failure", "Only when it failed"),
            ]),
        ),
        (
            "failure",
            fields(&[
                (
                    "code",
                    "test.assertion_failed, test.unexpected_failure, test.fault, test.subscriber_failed, test.fake_exhausted, test.fake_unused, test.recording_undecodable or test.round_trip_failed",
                ),
                ("help", "The code's entry on the language pages"),
                ("location", "Where in the test it failed (a location)"),
                (
                    "assertion",
                    "For an assert that didn't hold: source, operator, expected, actual, fails, returned, instead (a failure), parts (each source, line, column, value) and diff (each change, path, expected, actual)",
                ),
                (
                    "failure",
                    "For a failure that left the test: name, code, kind, message, client, internal, reason, at (a location), path (the callables it travelled through, outermost first) and cause",
                ),
                (
                    "fault",
                    "For a fault: kind, message, rule, at (a location), reason, next and stack (stack frames, innermost first)",
                ),
                (
                    "fake",
                    "For test.fake_exhausted and test.fake_unused: the faked operation (connection.resource.operation), how many answers its fakes: scripts and how many times it was called",
                ),
                (
                    "recording",
                    "For test.recording_undecodable and test.round_trip_failed: the package, the operation, the field and rule a decoder refused (when one did), the value recorded and the one written back (for a round trip), and the message",
                ),
                (
                    "delivery",
                    "For test.subscriber_failed: the delivery between the test's steps that failed for good, its event, subscriber, attempt and attempts (how many its retry: allows); its failure or fault is beside it",
                ),
                (
                    "rerun",
                    "The command that reruns this test alone in the same order, from where the command ran",
                ),
                (
                    "kept",
                    "The databases kept for inspection with --keep-failed; empty otherwise",
                ),
            ]),
        ),
        (
            "stackFrame",
            fields(&[
                ("location", "The call's site (a location)"),
                (
                    "callable",
                    "The callable it is in: its kind and name, such as `function <name>`",
                ),
                (
                    "repeats",
                    "How many frames in a row it stands for: a recursion is one frame",
                ),
            ]),
        ),
        (
            "location",
            fields(&[
                ("path", "The file, relative to the project root"),
                ("start", "The first byte"),
                ("end", "The byte after the last"),
                ("line", "The first line, from 1"),
                ("column", "The first column, from 1, in characters"),
                ("endLine", "The last line"),
                ("endColumn", "The column after the last character"),
                (
                    "label",
                    "The few words beside the carets; empty when there are none",
                ),
            ]),
        ),
    ])
}
