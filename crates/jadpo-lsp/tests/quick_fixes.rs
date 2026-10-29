//! Quick fixes from an error's own edit (area 14, "Error messages": an
//! obvious repair carries its edit, so the editor and agents can apply
//! it), and promotion as a quick fix (area 1; area 14, "Formatting"):
//! `layout.promotion_required`'s move, made as `jadpo fmt --layout` makes
//! it, the imports it touches updated.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use crate::support::{ask, at, project, start, uri};

fn actions(server: &mut jadpo_lsp::Server, root: &Path, path: &str, position: Value) -> Vec<Value> {
    let answer = ask(
        server,
        "textDocument/codeAction",
        json!({
            "textDocument": {"uri": uri(root, path)},
            "range": {"start": position, "end": position},
            "context": {"diagnostics": []},
        }),
    );
    answer.as_array().cloned().unwrap_or_default()
}

fn titled<'a>(offered: &'a [Value], title: &str) -> &'a Value {
    offered
        .iter()
        .find(|action| action["title"] == title)
        .unwrap_or_else(|| panic!("no `{title}` in {offered:#?}"))
}

/// What a workspace edit does to each file, by project path, each file
/// replaced whole; created files included.
fn applied(root: &Path, edit: &Value) -> BTreeMap<String, String> {
    let prefix = format!("file://{}/", root.display());
    let mut out = BTreeMap::new();
    for change in edit["documentChanges"].as_array().expect("changes") {
        if change.get("kind").is_some() {
            continue;
        }
        out.insert(
            change["textDocument"]["uri"]
                .as_str()
                .expect("uri")
                .trim_start_matches(&prefix)
                .to_string(),
            change["edits"][0]["newText"]
                .as_str()
                .expect("text")
                .to_string(),
        );
    }
    out
}

const BROKEN: &str = "domains/notes/actions/broken.jadpo";
const BROKEN_TEXT: &str =
    "use notes/types { Body }\n\naction broken(body: Body) -> Body {\n    return kepe(body)\n}\n";
const LOOSE: &str = "domains/notes/actions/loose.jadpo";
const LOOSE_TEXT: &str = "action loose(note: Body) -> Body {\n    return note\n}\n";

fn notes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("app.jadpo", "application {}\n"),
        (
            "domains/notes/types/body.jadpo",
            "/// The text of a note.\ntype Body = Text {\n    min_length: 1\n}\n",
        ),
        (
            "domains/notes/actions/keep.jadpo",
            "use notes/types { Body }\n\naction keep(body: Body) -> Body {\n    return body\n}\n",
        ),
        (BROKEN, BROKEN_TEXT),
        (LOOSE, LOOSE_TEXT),
    ]
}

#[test]
fn an_errors_edit_is_its_quick_fix() {
    let root = project("quick-fix-typo", &notes());
    let mut server = start(&root, json!({}));
    let offered = actions(&mut server, &root, BROKEN, at(BROKEN_TEXT, "kepe", 0, 1));
    let fix = titled(&offered, "Use `keep` instead of `kepe`");
    assert_eq!(fix["kind"], "quickfix");
    assert_eq!(fix["isPreferred"], true);
    assert_eq!(fix["diagnostics"][0]["code"], "semantic.unknown_callee");
    assert_eq!(
        applied(&root, &fix["edit"]),
        BTreeMap::from([(BROKEN.to_string(), BROKEN_TEXT.replace("kepe", "keep"))])
    );
}

#[test]
fn a_missing_import_is_added_and_the_title_is_the_editors_step() {
    let root = project("quick-fix-import", &notes());
    let mut server = start(&root, json!({}));
    let offered = actions(&mut server, &root, LOOSE, at(LOOSE_TEXT, "Body", 0, 0));
    // The command's alternative ("or run `jadpo fmt`") is left to the CLI.
    let fix = titled(&offered, "Add `use notes/types { Body }`");
    assert_eq!(
        applied(&root, &fix["edit"])[LOOSE],
        format!("use notes/types {{ Body }}\n\n{LOOSE_TEXT}")
    );
}

#[test]
fn nothing_is_offered_away_from_an_error() {
    let root = project("quick-fix-none", &notes());
    let mut server = start(&root, json!({}));
    let offered = actions(&mut server, &root, BROKEN, at(BROKEN_TEXT, "action", 0, 0));
    assert!(offered.is_empty(), "{offered:#?}");
}

const SNOOZE: &str = "domains/planning/actions/snooze_todo.jadpo";
const SNOOZE_TEXT: &str = "type SnoozeDays = Int {\n    min: 1\n    max: 30\n}\n\naction snooze_todo(days: SnoozeDays) -> Unit {}\n";
const POSTPONE: &str = "domains/planning/actions/postpone_all.jadpo";

fn planning() -> Vec<(&'static str, &'static str)> {
    vec![
        ("app.jadpo", "application {}\n"),
        (
            POSTPONE,
            "use planning/actions { SnoozeDays }\n\naction postpone_all(days: SnoozeDays) -> Unit {}\n",
        ),
        (SNOOZE, SNOOZE_TEXT),
    ]
}

fn expected_move() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "domains/planning/types/snooze_days.jadpo".to_string(),
            "type SnoozeDays = Int {\n    min: 1\n    max: 30\n}\n".to_string(),
        ),
        (
            SNOOZE.to_string(),
            "use planning/types { SnoozeDays }\n\naction snooze_todo(days: SnoozeDays) -> Unit {}\n"
                .to_string(),
        ),
        (
            POSTPONE.to_string(),
            "use planning/types { SnoozeDays }\n\naction postpone_all(days: SnoozeDays) -> Unit {}\n"
                .to_string(),
        ),
    ])
}

#[test]
fn promotion_is_a_quick_fix_that_moves_and_updates_the_imports() {
    let root = project("quick-fix-promotion", &planning());
    let mut server = start(&root, json!({}));
    let offered = actions(
        &mut server,
        &root,
        SNOOZE,
        at(SNOOZE_TEXT, "SnoozeDays", 0, 3),
    );
    let fix = titled(
        &offered,
        "Move `SnoozeDays` to `domains/planning/types/snooze_days.jadpo`",
    );
    assert_eq!(fix["diagnostics"][0]["code"], "layout.promotion_required");
    let creates: Vec<&Value> = fix["edit"]["documentChanges"]
        .as_array()
        .expect("changes")
        .iter()
        .filter(|change| change["kind"] == "create")
        .collect();
    assert_eq!(creates.len(), 1, "{creates:?}");
    assert!(
        creates[0]["uri"]
            .as_str()
            .is_some_and(|uri| uri.ends_with("domains/planning/types/snooze_days.jadpo"))
    );
    assert_eq!(applied(&root, &fix["edit"]), expected_move());
    // The server's texts are as they were: the editor makes the change.
    let again = actions(
        &mut server,
        &root,
        SNOOZE,
        at(SNOOZE_TEXT, "SnoozeDays", 0, 3),
    );
    let title = fix["title"].as_str().expect("title");
    assert_eq!(
        applied(&root, &titled(&again, title)["edit"]),
        expected_move()
    );
}

#[test]
fn an_editor_that_resolves_gets_the_edit_once_chosen() {
    let root = project("quick-fix-resolve", &planning());
    let mut server = start(
        &root,
        json!({"textDocument": {"codeAction": {"resolveSupport": {"properties": ["edit"]}}}}),
    );
    let offered = actions(
        &mut server,
        &root,
        SNOOZE,
        at(SNOOZE_TEXT, "SnoozeDays", 0, 3),
    );
    let fix = titled(
        &offered,
        "Move `SnoozeDays` to `domains/planning/types/snooze_days.jadpo`",
    );
    assert!(fix.get("edit").is_none_or(Value::is_null), "{fix:#}");
    let resolved = ask(&mut server, "codeAction/resolve", fix.clone());
    assert_eq!(applied(&root, &resolved["edit"]), expected_move());
}
