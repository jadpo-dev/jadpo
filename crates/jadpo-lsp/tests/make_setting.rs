//! "Make this a setting" (area 10 and area 14, chunk 5.3f): a code action
//! on a constant's declaration or a read of it, whose edit is the one
//! `jadpo config move` makes (both run `jadpo_check::setting_move`), and
//! which is unavailable, with the reason, where the move is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_check::setting_move;
use jadpo_lsp::{MAKE_SETTING, Server};
use lsp_server::{Message, Request, RequestId};
use serde_json::{Value, json};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/accounts/types/login_attempts.jadpo",
        "type LoginAttempts = Int {\n    min: 1\n    max: 100\n}\n",
    ),
    (
        "domains/accounts/types/account_locked.jadpo",
        "type AccountLocked = Bool {}\n",
    ),
    (
        "domains/accounts/libraries/security.jadpo",
        "use accounts/types { LoginAttempts }\n\nlibrary security {\n    /// Failed sign-ins before an account is locked.\n    constant max_login_attempts: LoginAttempts = 5\n}\n",
    ),
    (
        "domains/accounts/actions/sign_in.jadpo",
        "use accounts/types { AccountLocked, LoginAttempts }\nuse accounts/libraries { security }\n\naction sign_in(failures: LoginAttempts) -> AccountLocked {\n    return AccountLocked(failures >= security.max_login_attempts)\n}\n",
    ),
];

const SIGN_IN: &str = "domains/accounts/actions/sign_in.jadpo";

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root.canonicalize().expect("real path")
}

fn uri(root: &Path, path: &str) -> String {
    format!("file://{}", root.join(path).display())
}

fn start(root: &Path, resolves: bool) -> Server {
    let mut capabilities = json!({});
    if resolves {
        capabilities = json!({"textDocument": {"codeAction": {"resolveSupport": {"properties": ["edit"]}}}});
    }
    let params = serde_json::from_value(json!({
        "processId": null,
        "rootUri": format!("file://{}", root.display()),
        "capabilities": capabilities,
    }))
    .expect("initialize params");
    let (server, result) = Server::initialize(&params, root);
    let result = serde_json::to_value(result).expect("result");
    assert_eq!(
        result["capabilities"]["codeActionProvider"]["resolveProvider"],
        true
    );
    server
}

fn ask(server: &mut Server, id: i32, method: &str, params: Value) -> Value {
    let answer = server.handle(Message::Request(Request::new(
        RequestId::from(id),
        method.to_string(),
        params,
    )));
    let Some(Message::Response(response)) = answer.first() else {
        panic!("{answer:?}");
    };
    response.response_result.clone().expect("a result")
}

/// The code actions at `line`, `character` of `path`.
fn actions(server: &mut Server, root: &Path, path: &str, line: u32, character: u32) -> Value {
    let at = json!({"line": line, "character": character});
    ask(
        server,
        1,
        "textDocument/codeAction",
        json!({
            "textDocument": {"uri": uri(root, path)},
            "range": {"start": at, "end": at},
            "context": {"diagnostics": []},
        }),
    )
}

/// What a workspace edit does to each file, by project path: its new
/// text, or `None` for a deleted one.
fn applied(root: &Path, edit: &Value) -> BTreeMap<String, Option<String>> {
    let prefix = format!("file://{}/", root.display());
    let mut out = BTreeMap::new();
    for change in edit["documentChanges"].as_array().expect("changes") {
        let (uri, text) = match change["kind"].as_str() {
            Some("delete") => (change["uri"].as_str().expect("uri"), None),
            Some(_) => continue,
            None => (
                change["textDocument"]["uri"].as_str().expect("uri"),
                change["edits"][0]["newText"].as_str().map(str::to_string),
            ),
        };
        out.insert(uri.trim_start_matches(&prefix).to_string(), text);
    }
    out
}

#[test]
fn the_action_on_a_read_makes_the_edit_config_move_makes() {
    let root = project("lsp-make-setting", FILES);
    let mut server = start(&root, true);
    // On `max_login_attempts` in `security.max_login_attempts`.
    let offered = actions(&mut server, &root, SIGN_IN, 4, 50);
    let action = &offered[0];
    assert_eq!(action["title"], MAKE_SETTING);
    assert_eq!(action["kind"], "refactor.rewrite");
    assert!(action.get("edit").is_none(), "resolved later: {action}");
    assert!(action.get("disabled").is_none(), "{action}");
    let resolved = ask(&mut server, 2, "codeAction/resolve", action.clone());
    let edit = applied(&root, &resolved["edit"]);

    // The same move, made by the shared code over the same files.
    let mut loaded = jadpo_db::load(&root);
    let id = setting_move::find(&loaded.db, loaded.project, "security.max_login_attempts")
        .expect("found");
    let plan = setting_move::plan(&loaded.db, loaded.project, &id).expect("planned");
    let moved = setting_move::apply(&mut loaded, &plan).expect("moved");
    let expected: BTreeMap<String, Option<String>> = moved
        .files
        .iter()
        .map(|change| (change.path.clone(), change.after.clone()))
        .collect();
    assert_eq!(edit, expected);
    assert_eq!(
        edit.get("domains/accounts/libraries/security.jadpo"),
        Some(&None)
    );
    let creates = resolved["edit"]["documentChanges"]
        .as_array()
        .expect("changes")
        .iter()
        .filter(|change| change["kind"] == "create")
        .count();
    assert_eq!(creates, 1, "config.jadpo is created");

    // The server's texts are as the editor has them until it applies the
    // edit.
    let server_text = |server: &Server| {
        let loaded = server.loaded().expect("project");
        loaded
            .file(SIGN_IN)
            .map(|file| file.text(&loaded.db).clone())
    };
    assert_eq!(server_text(&server).as_deref(), Some(FILES[4].1));
    assert!(server.loaded().expect("project").file("config.jadpo").is_none());
}

#[test]
fn the_action_on_the_declaration_comes_with_its_edit_when_not_resolved() {
    let root = project("lsp-make-setting-eager", FILES);
    let mut server = start(&root, false);
    let offered = actions(
        &mut server,
        &root,
        "domains/accounts/libraries/security.jadpo",
        4,
        15,
    );
    assert_eq!(offered[0]["title"], MAKE_SETTING);
    assert!(offered[0]["edit"]["documentChanges"].is_array(), "{offered}");
}

#[test]
fn a_refused_move_is_offered_unavailable_with_its_reason() {
    let mut files = FILES.to_vec();
    files.push((
        "config.jadpo",
        "use accounts/types { LoginAttempts }\n\nconfig {\n    max_login_attempts: LoginAttempts {\n        default: 3\n    }\n}\n",
    ));
    files[4] = (
        SIGN_IN,
        "use accounts/types { AccountLocked, LoginAttempts }\nuse accounts/libraries { security }\n\naction sign_in(failures: LoginAttempts) -> AccountLocked {\n    return AccountLocked(failures >= security.max_login_attempts or failures >= config.max_login_attempts)\n}\n",
    );
    let root = project("lsp-make-setting-refused", &files);
    let mut server = start(&root, true);
    let offered = actions(&mut server, &root, SIGN_IN, 4, 50);
    assert_eq!(
        offered[0]["disabled"]["reason"],
        "A setting is already called `max_login_attempts`"
    );
}

#[test]
fn no_action_is_offered_away_from_a_constant() {
    let root = project("lsp-make-setting-none", FILES);
    let mut server = start(&root, true);
    assert_eq!(actions(&mut server, &root, SIGN_IN, 3, 17), Value::Null);
}
