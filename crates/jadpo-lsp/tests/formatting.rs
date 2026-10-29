//! Formatting (area 14, "Formatting"; the editor formats on save): the
//! text `jadpo fmt` writes for the file, imports fixed and declarations in
//! definition-before-use order inside a project, as one edit of the whole
//! document, of the text the editor has rather than the one on disk.

use serde_json::{Value, json};

use crate::support::{ask, edited, project, start, uri};

const ACTION: &str = "domains/notes/actions/save_note.jadpo";

fn files(action: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("app.jadpo", "application {}\n"),
        (
            "domains/notes/types/body.jadpo",
            "/// The text of a note.\ntype Body = Text {\n    min_length: 1\n}\n",
        ),
        (ACTION, action),
    ]
}

fn format(server: &mut jadpo_lsp::Server, root: &std::path::Path) -> Value {
    ask(
        server,
        "textDocument/formatting",
        json!({
            "textDocument": {"uri": uri(root, ACTION)},
            "options": {"tabSize": 4, "insertSpaces": true},
        }),
    )
}

fn open(server: &mut jadpo_lsp::Server, root: &std::path::Path, text: &str) {
    server.handle(lsp_server::Message::Notification(
        lsp_server::Notification::new(
            "textDocument/didOpen".to_string(),
            json!({"textDocument": {
                "uri": uri(root, ACTION), "languageId": "jadpo", "version": 1, "text": text,
            }}),
        ),
    ));
}

#[test]
fn the_open_text_is_formatted_as_jadpo_fmt_writes_it() {
    let root = project(
        "formatting-open",
        &files("action save_note(body: Body) -> Body {\n    return body\n}\n"),
    );
    let mut server = start(&root, json!({}));
    // Unsaved: the import is missing, the layout is off and a helper comes
    // after the action that uses it.
    let typed = "action save_note(body:Body)->Body {\n        return trimmed( body )\n}\nfunction trimmed(body: Body) -> Body {\n  return body\n}\n";
    open(&mut server, &root, typed);
    let edits = format(&mut server, &root);
    let edits = edits.as_array().expect("edits");
    assert_eq!(edits.len(), 1, "{edits:?}");
    assert_eq!(
        edited(typed, &edits[0]),
        "use notes/types { Body }\n\nfunction trimmed(body: Body) -> Body {\n    return body\n}\n\naction save_note(body: Body) -> Body {\n    return trimmed(body)\n}\n"
    );
    // The server's own text is still what the editor sent.
    let again = format(&mut server, &root);
    assert_eq!(again.as_array().map(Vec::len), Some(1));
}

#[test]
fn a_formatted_file_has_no_edits() {
    let root = project(
        "formatting-unchanged",
        &files(
            "use notes/types { Body }\n\naction save_note(body: Body) -> Body {\n    return body\n}\n",
        ),
    );
    let mut server = start(&root, json!({}));
    assert_eq!(format(&mut server, &root), json!([]));
}

#[test]
fn a_file_that_does_not_parse_is_left_alone() {
    let root = project(
        "formatting-broken",
        &files("action save_note(body: Body) -> Body {\n    return (\n}\n"),
    );
    let mut server = start(&root, json!({}));
    assert_eq!(format(&mut server, &root), json!([]));
}

#[test]
fn the_server_says_it_formats() {
    let root = project("formatting-capability", &files("action a() -> Unit {}\n"));
    let params = serde_json::from_value(json!({
        "processId": null,
        "rootUri": format!("file://{}", root.display()),
        "capabilities": {},
    }))
    .expect("params");
    let (_, result) = jadpo_lsp::Server::initialize(&params, &root);
    let capabilities = serde_json::to_value(result.capabilities).expect("json");
    assert_eq!(capabilities["documentFormattingProvider"], json!(true));
    assert_eq!(capabilities["referencesProvider"], json!(true));
}
