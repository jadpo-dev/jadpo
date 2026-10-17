//! JSON documents: one per command, newline-terminated, `schemaVersion`
//! first, one key style (camelCase), and diagnostics with line and column
//! beside byte offsets.

use std::collections::BTreeMap;

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_term::{Json, SCHEMA_VERSION, envelope};

#[test]
fn the_writer_indents_two_spaces_and_ends_with_a_newline() {
    let value = Json::object([
        ("name", Json::from("jadpo")),
        ("count", Json::from(2)),
        ("ok", Json::from(true)),
        ("none", Json::Null),
        ("empty", Json::Array(vec![])),
        ("nothing", Json::object::<&str>([])),
        ("list", Json::Array(vec![Json::from("a"), Json::from(-1)])),
    ]);
    assert_eq!(
        value.to_document(),
        "{\n  \"name\": \"jadpo\",\n  \"count\": 2,\n  \"ok\": true,\n  \"none\": null,\n  \"empty\": [],\n  \"nothing\": {},\n  \"list\": [\n    \"a\",\n    -1\n  ]\n}\n"
    );
}

#[test]
fn strings_are_escaped() {
    let value = Json::from("quote \" backslash \\ newline \n tab \t bell \u{7} café ✓");
    assert_eq!(
        value.to_document(),
        "\"quote \\\" backslash \\\\ newline \\n tab \\t bell \\u0007 café ✓\"\n"
    );
    let parsed: serde_json::Value = serde_json::from_str(&value.to_document()).expect("valid JSON");
    assert_eq!(
        parsed,
        serde_json::Value::String(
            "quote \" backslash \\ newline \n tab \t bell \u{7} café ✓".into()
        )
    );
}

#[test]
fn an_envelope_carries_schema_version_command_ok_data_and_diagnostics() {
    let document = envelope(
        Some("version"),
        Some(Json::object([("version", Json::from("0.1.0"))])),
        &[],
        &(),
    )
    .to_document();
    let parsed: serde_json::Value = serde_json::from_str(&document).expect("valid JSON");
    let keys: Vec<&String> = parsed.as_object().expect("object").keys().collect();
    assert_eq!(
        keys,
        ["command", "data", "diagnostics", "ok", "schemaVersion"]
    );
    assert!(
        document.starts_with(
            "{\n  \"schemaVersion\": 2,\n  \"command\": \"version\",\n  \"ok\": true,"
        ),
        "{document}"
    );
    assert_eq!(parsed["schemaVersion"], SCHEMA_VERSION);
    assert_eq!(parsed["data"]["version"], "0.1.0");
    assert_eq!(parsed["diagnostics"], serde_json::json!([]));
}

#[test]
fn a_failed_envelope_has_null_data_and_its_diagnostics() {
    let diagnostic = Diagnostic::new(&rule::CLI_UNKNOWN_COMMAND, "`hlep` isn't a jadpo command")
        .reason("Jadpo has no command with this name. Did you mean `help`?")
        .next("Use `jadpo help` instead of `jadpo hlep`");
    let document = envelope(None, None, &[diagnostic], &()).to_document();
    let parsed: serde_json::Value = serde_json::from_str(&document).expect("valid JSON");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["command"], serde_json::Value::Null);
    assert_eq!(parsed["data"], serde_json::Value::Null);
    assert_eq!(
        parsed["diagnostics"][0],
        serde_json::json!({
            "rule": "cli.unknown_command",
            "severity": "error",
            "declinable": false,
            "message": "`hlep` isn't a jadpo command",
            "location": null,
            "related": [],
            "reason": "Jadpo has no command with this name. Did you mean `help`?",
            "next": { "text": "Use `jadpo help` instead of `jadpo hlep`", "decides": "agent", "edits": [] },
            "inUse": [],
            "impact": null,
            "help": "docs/language/14-tooling.html#command-errors"
        })
    );
}

#[test]
fn no_diagnostics_leave_the_envelope_ok() {
    // Every finding is an error, so only an empty list is ok.
    let ok = envelope(Some("help"), Some(Json::Null), &[], &()).to_document();
    assert!(ok.contains("\"ok\": true"));
}

#[test]
fn locations_give_line_and_column_beside_byte_offsets() {
    let path = "domains/planning/libraries/titles.jadpo";
    let text = "function f() -> Title {\n    return titel\n}\n";
    let start = text.find("titel").expect("titel") as u32;
    let sources = BTreeMap::from([(path.to_string(), text.to_string())]);
    let diagnostic = Diagnostic::new(
        &rule::CLI_UNKNOWN_COMMAND,
        "`titel` isn't defined in this scope",
    )
    .at(Span::new(path, start, start + 5), "not defined")
    .next("Use `title` instead of `titel`")
    .edit(Span::new(path, start, start + 5), "title");
    let document = envelope(Some("check"), None, &[diagnostic], &sources).to_document();
    let parsed: serde_json::Value = serde_json::from_str(&document).expect("valid JSON");
    let found = &parsed["diagnostics"][0];
    assert_eq!(
        found["location"],
        serde_json::json!({
            "path": path, "start": start, "end": start + 5,
            "line": 2, "column": 12, "endLine": 2, "endColumn": 17, "label": "not defined"
        })
    );
    assert_eq!(
        found["next"]["edits"][0],
        serde_json::json!({ "path": path, "start": start, "end": start + 5, "line": 2, "column": 12, "endLine": 2, "endColumn": 17, "replacement": "title" })
    );
}
