//! The server keeps one database (docs/compiler-runtime.md §2.4): editing
//! one file's body rechecks that file and reuses everything else, and the
//! diagnostics it publishes are the ones `jadpo check` gives.

use std::path::{Path, PathBuf};

use jadpo_db::Database;
use jadpo_lsp::{Encoding, Server};
use lsp_server::{Message, Notification, Request, RequestId};
use serde_json::{Value, json};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/notes/types/body.jadpo",
        "type Body = Text {\n    min_length: 1\n}\n",
    ),
    (
        "domains/notes/actions/save_note.jadpo",
        "use notes/types { Body }\n\naction save_note(body: Body) -> Body {\n    return body\n}\n",
    ),
    (
        "domains/notes/actions/broken.jadpo",
        "use notes/types { Body }\n\naction broken(body: Body) -> Body {\n    return missing(body)\n}\n",
    ),
];

fn project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-incremental");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root.canonicalize().expect("real path")
}

fn notify(method: &str, params: Value) -> Message {
    Message::Notification(Notification::new(method.to_string(), params))
}

fn published(messages: &[Message]) -> Vec<(String, Vec<String>)> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Notification(n) if n.method == "textDocument/publishDiagnostics" => Some((
                n.params["uri"]
                    .as_str()
                    .unwrap_or_default()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                n.params["diagnostics"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| d["code"].as_str().unwrap_or_default().to_string())
                    .collect(),
            )),
            _ => None,
        })
        .collect()
}

fn count(log: &[String], query: &str) -> usize {
    log.iter()
        .filter(|key| key.starts_with(&format!("{query}(")))
        .count()
}

#[test]
fn an_edit_rechecks_only_what_it_changes() {
    let root = project();
    let loaded = jadpo_db::load_into(&root, Database::recording());
    let mut server = Server::new(Some(loaded), Encoding::Utf16);
    let save = format!(
        "file://{}",
        root.join("domains/notes/actions/save_note.jadpo").display()
    );

    server.handle(notify("initialized", json!({})));
    let first = server.flush();
    assert_eq!(
        published(&first),
        [(
            "broken.jadpo".to_string(),
            vec!["semantic.unknown_callee".to_string()]
        )]
    );
    let db = &server.loaded().expect("project").db;
    db.take_executions();

    server.handle(notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": save, "languageId": "jadpo", "version": 1,
            "text": "use notes/types { Body }\n\naction save_note(body: Body) -> Body {\n    var kept: Body = body\n    return kept\n}\n"}}),
    ));
    let second = server.flush();
    assert!(
        published(&second).is_empty(),
        "nothing changed for the editor: {second:?}"
    );
    let log = server.loaded().expect("project").db.take_executions();
    assert_eq!(
        count(&log, "typed_body"),
        1,
        "only the edited body: {log:?}"
    );
    assert_eq!(
        count(&log, "body_problems"),
        1,
        "only the edited file's problems: {log:?}"
    );
    assert_eq!(count(&log, "symbols"), 0, "{log:?}");
    assert_eq!(count(&log, "type_table"), 0, "{log:?}");
    assert_eq!(count(&log, "resolve_file"), 1, "{log:?}");
    // Its imports, read from its own text, are the only ones checked again.
    assert_eq!(count(&log, "import_problems"), 1, "{log:?}");

    // Hover builds the graph once; after another edit, only that file's
    // part is built again.
    let hover = |server: &mut Server, id: i32| {
        server.handle(Message::Request(Request::new(
            RequestId::from(id),
            "textDocument/hover".to_string(),
            json!({"textDocument": {"uri": save}, "position": {"line": 2, "character": 9}}),
        )))
    };
    let answer = hover(&mut server, 1);
    let Some(Message::Response(response)) = answer.first() else {
        panic!("{answer:?}");
    };
    let value = response.response_result.as_ref().expect("hover")["contents"]["value"]
        .as_str()
        .expect("text")
        .to_string();
    assert!(
        value.starts_with("```jadpo\naction save_note(body: Body) -> Body\n```"),
        "{value}"
    );
    server.loaded().expect("project").db.take_executions();
    server.handle(notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": save, "version": 2},
            "contentChanges": [{"text": "use notes/types { Body }\n\naction save_note(body: Body) -> Body {\n    return body\n}\n"}]}),
    ));
    hover(&mut server, 2);
    let log = server.loaded().expect("project").db.take_executions();
    assert_eq!(count(&log, "file_part"), 1, "{log:?}");
    assert_eq!(count(&log, "graph"), 1, "{log:?}");
}

/// The language server reads a 5,000-branch `else if` chain without a
/// depth error or a stack overflow (Phase 1 review, finding 18).
#[test]
fn a_long_else_if_chain_is_served() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-else-if");
    let _ = std::fs::remove_dir_all(&root);
    let chain = " else if (body == \"b\") {\n        return body\n    }".repeat(5_000);
    let files = [
        ("app.jadpo", "application {}\n".to_string()),
        (
            "domains/notes/types/body.jadpo",
            "type Body = Text {}\n".to_string(),
        ),
        (
            "domains/notes/actions/save_note.jadpo",
            format!(
                "use notes/types {{ Body }}\n\naction save_note(body: Body) -> Body {{\n    if (body == \"a\") {{\n        return body\n    }}{chain}\n    return body\n}}\n"
            ),
        ),
    ];
    for (path, text) in &files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(&root.canonicalize().expect("real path"));
    let mut server = Server::new(Some(loaded), Encoding::Utf16);
    server.handle(notify("initialized", json!({})));
    assert!(published(&server.flush()).is_empty());
}
