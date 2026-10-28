//! "Swap for the standard ..." (area 14 and area 19, "A release adding a standard type"; chunk
//! 5.5a2): the quick fix on `type.standard_clash` and
//! `type.standard_exists`, whose edit is the one `jadpo fix <name> swap`
//! makes (both run `jadpo_check::fixes`), and which is unavailable, with
//! the error it would add, where the command stops.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_check::fixes::{self, Fix};
use jadpo_lsp::{SWAP, Server};
use lsp_server::{Message, Request, RequestId};
use serde_json::{Value, json};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/rooms/types/slot.jadpo",
        "/// When a room is booked, from its start to its end.\ntype Slot = Object {\n    starts_at: Instant\n    ends_at: Instant\n}\n",
    ),
    (
        "domains/rooms/types/slot_start.jadpo",
        "/// When a slot starts.\ntype SlotStart = Instant {}\n",
    ),
    (
        "domains/rooms/actions/start_of.jadpo",
        "use rooms/types { Slot, SlotStart }\n\n/// When a slot starts.\naction start_of(slot: Slot) -> SlotStart {\n    return SlotStart(slot.starts_at)\n}\n",
    ),
];

const SLOT: &str = "domains/rooms/types/slot.jadpo";

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
        capabilities =
            json!({"textDocument": {"codeAction": {"resolveSupport": {"properties": ["edit"]}}}});
    }
    let params = serde_json::from_value(json!({
        "processId": null,
        "rootUri": format!("file://{}", root.display()),
        "capabilities": capabilities,
    }))
    .expect("initialize params");
    Server::initialize(&params, root).0
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

/// The swap among the actions offered.
fn swap(offered: &Value) -> Value {
    offered
        .as_array()
        .expect("actions")
        .iter()
        .find(|action| {
            action["title"]
                .as_str()
                .is_some_and(|title| title.starts_with(SWAP))
        })
        .cloned()
        .expect("the swap")
}

#[test]
fn the_swap_makes_the_edit_jadpo_fix_makes() {
    let root = project("lsp-swap", FILES);
    let mut server = start(&root, true);
    // On `Slot` in `type Slot = Object {`.
    let action = swap(&actions(&mut server, &root, SLOT, 1, 6));
    assert_eq!(action["title"], "Swap for the standard `InstantRange`");
    assert_eq!(action["kind"], "quickfix");
    assert!(action.get("edit").is_none(), "resolved later: {action}");
    let resolved = ask(&mut server, 2, "codeAction/resolve", action);
    let edit = applied(&root, &resolved["edit"]);

    let mut loaded = jadpo_db::load(&root);
    let finding = fixes::find(&loaded.db, loaded.project, "Slot").expect("found");
    let plan = fixes::plan(&loaded.db, loaded.project, &finding, &Fix::Swap).expect("planned");
    let fixed = fixes::apply(&mut loaded, &plan).expect("fixed");
    let expected: BTreeMap<String, Option<String>> = fixed
        .files
        .iter()
        .map(|change| (change.path.clone(), change.after.clone()))
        .collect();
    assert_eq!(edit, expected);
    assert_eq!(edit.get(SLOT), Some(&None));
    // The server's texts are as the editor has them until it applies it.
    let loaded = server.loaded().expect("project");
    assert!(loaded.file(SLOT).is_some());
}

#[test]
fn the_swap_comes_with_its_edit_when_not_resolved() {
    let root = project("lsp-swap-eager", FILES);
    let mut server = start(&root, false);
    let action = swap(&actions(&mut server, &root, SLOT, 1, 6));
    assert!(action["edit"]["documentChanges"].is_array(), "{action}");
}

#[test]
fn a_swap_that_would_add_an_error_is_offered_unavailable() {
    let files: &[(&str, &str)] = &[
        ("app.jadpo", "application {}\n"),
        (
            "domains/shop/types/weekday.jadpo",
            "/// The days the shop opens.\ntype Weekday = Enum {\n    mon\n    sat\n}\n",
        ),
        (
            "domains/shop/types/opens_saturday.jadpo",
            "/// Whether the shop opens on a Saturday.\ntype OpensSaturday = Bool {}\n",
        ),
        (
            "domains/shop/actions/is_saturday.jadpo",
            "use shop/types { OpensSaturday, Weekday }\n\n/// Whether `day` is a Saturday.\naction is_saturday(day: Weekday) -> OpensSaturday {\n    return OpensSaturday(day == Weekday.sat)\n}\n",
        ),
    ];
    let root = project("lsp-swap-stopped", files);
    let mut server = start(&root, false);
    let action = swap(&actions(
        &mut server,
        &root,
        "domains/shop/types/weekday.jadpo",
        1,
        6,
    ));
    assert_eq!(action["title"], "Swap for the standard `Weekday`");
    assert_eq!(
        action["disabled"]["reason"],
        "The swap would add an error: `Weekday` has no variant `sat`"
    );
}
