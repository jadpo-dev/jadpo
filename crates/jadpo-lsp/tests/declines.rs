//! "Decline with a reason" and "Remove decline" (area 1, "Declining a
//! judgement call", chunk 5.4b): quick fixes whose edits are the ones
//! `jadpo_check::declines` makes for `jadpo declines clear`.

use std::path::{Path, PathBuf};

use jadpo_lsp::{DECLINE, REMOVE_DECLINE, Server};
use lsp_server::{Message, Request, RequestId};
use serde_json::{Value, json};

const LIBRARY: &str = "shared/libraries/rounding.jadpo";

const ROUNDING: &str = "use shared/types { Coins }\n\nlibrary rounding {\n    function capped(amount: Coins) -> Coins {\n        return amount\n    }\n\n    /// The basket total.\n    function cap_total(total: Coins) -> Coins {\n        return total\n    }\n}\n";

fn project(name: &str, rounding: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in [
        ("app.jadpo", "application {}\n"),
        (
            "shared/types/coins.jadpo",
            "type Coins = Int {\n    min: 0\n}\n",
        ),
        (LIBRARY, rounding),
        (
            "domains/sales/actions/basket_total.jadpo",
            "use shared/types { Coins }\nuse shared/libraries { rounding }\n\naction basket_total(price: Coins) -> Coins {\n    return rounding.cap_total(rounding.capped(price))\n}\n",
        ),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root.canonicalize().expect("real path")
}

fn start(root: &Path) -> Server {
    let params = serde_json::from_value(json!({
        "processId": null,
        "rootUri": format!("file://{}", root.display()),
        "capabilities": {},
    }))
    .expect("initialize params");
    Server::initialize(&params, root).0
}

fn actions(server: &mut Server, root: &Path, line: u32, character: u32) -> Value {
    let at = json!({"line": line, "character": character});
    let answer = server.handle(Message::Request(Request::new(
        RequestId::from(1),
        "textDocument/codeAction".to_string(),
        json!({
            "textDocument": {"uri": format!("file://{}", root.join(LIBRARY).display())},
            "range": {"start": at, "end": at},
            "context": {"diagnostics": []},
        }),
    )));
    let Some(Message::Response(response)) = answer.first() else {
        panic!("{answer:?}");
    };
    response.response_result.clone().expect("a result")
}

/// `text` with an action's one edit made.
fn applied(text: &str, action: &Value) -> String {
    let edit = &action["edit"]["documentChanges"][0]["edits"][0];
    let offset = |position: &Value| {
        let line = position["line"].as_u64().expect("line") as usize;
        let character = position["character"].as_u64().expect("character") as usize;
        text.split_inclusive('\n')
            .take(line)
            .map(str::len)
            .sum::<usize>()
            + character
    };
    let mut out = text.to_string();
    out.replace_range(
        offset(&edit["range"]["start"])..offset(&edit["range"]["end"]),
        edit["newText"].as_str().expect("text"),
    );
    out
}

#[test]
fn decline_with_a_reason_writes_the_line_above_the_declaration() {
    let root = project("lsp-decline", ROUNDING);
    let mut server = start(&root);
    // On `cap_total`'s name, where `library.similar_exists` points.
    let offered = actions(&mut server, &root, 8, 14);
    let action = &offered[0];
    assert_eq!(
        action["title"],
        format!("{DECLINE} (library.similar_exists)")
    );
    assert_eq!(action["kind"], "quickfix");
    assert_eq!(
        applied(ROUNDING, action),
        ROUNDING.replace(
            "    /// The basket total.\n",
            "    /// The basket total.\n    decline library.similar_exists: \"\"\n"
        )
    );
}

#[test]
fn remove_decline_takes_the_line_away() {
    let declined = ROUNDING.replace(
        "    /// The basket total.\n",
        "    /// The basket total.\n    decline library.similar_exists: \"Caps the basket\"\n",
    );
    let root = project("lsp-remove-decline", &declined);
    let mut server = start(&root);
    let offered = actions(&mut server, &root, 8, 8);
    let action = &offered[0];
    assert_eq!(action["title"], REMOVE_DECLINE);
    assert_eq!(applied(&declined, action), ROUNDING);
}

#[test]
fn no_decline_is_offered_where_nothing_is_declinable() {
    let root = project("lsp-decline-none", ROUNDING);
    let mut server = start(&root);
    assert_eq!(actions(&mut server, &root, 0, 4), Value::Null);
}
