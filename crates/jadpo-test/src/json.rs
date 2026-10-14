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
        };
        entries.push((
            "failure".to_string(),
            Json::object([
                ("code", failure.rule.id.into()),
                ("help", failure.rule.help().into()),
                (
                    "location",
                    Json::object([
                        ("path", Json::from(&failure.file)),
                        ("line", failure.line.into()),
                        ("column", failure.column.into()),
                    ]),
                ),
                ("assertion", assertion),
                ("failure", raised),
                ("fault", fault),
                ("rerun", Json::from(&failure.rerun)),
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

fn located(located: &Option<Located>) -> Json {
    located.as_ref().map_or(Json::Null, |located| {
        Json::object([
            ("path", Json::from(&located.path)),
            ("line", located.line.into()),
            ("column", located.column.into()),
        ])
    })
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
            Json::array(info.stack.iter().map(|(place, label)| {
                Json::object([
                    ("place", Json::from(place)),
                    ("callable", Json::from(label)),
                ])
            })),
        ),
    ])
}
