//! What the editing tests share: a project written to disk, a server
//! started over it, and requests asked of it.

use std::path::{Path, PathBuf};

use jadpo_lsp::Server;
use lsp_server::{Message, Request, RequestId};
use serde_json::{Value, json};

/// `files` written under a fresh folder named `name`, as its real path.
pub fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root.canonicalize().expect("real path")
}

pub fn uri(root: &Path, path: &str) -> String {
    format!("file://{}", root.join(path).display())
}

/// A server over `root`, for an editor with `capabilities`.
pub fn start(root: &Path, capabilities: Value) -> Server {
    let params = serde_json::from_value(json!({
        "processId": null,
        "rootUri": format!("file://{}", root.display()),
        "capabilities": capabilities,
    }))
    .expect("initialize params");
    Server::initialize(&params, root).0
}

/// The answer to one request.
pub fn ask(server: &mut Server, method: &str, params: Value) -> Value {
    let answer = server.handle(Message::Request(Request::new(
        RequestId::from(1),
        method.to_string(),
        params,
    )));
    let Some(Message::Response(response)) = answer.first() else {
        panic!("{answer:?}");
    };
    match &response.response_result {
        Ok(result) => result.clone(),
        Err(error) => panic!("{method}: {error:?}"),
    }
}

/// The position of the `nth` (from 0) `needle` in `text`, `ahead` bytes
/// into it, in UTF-16 columns (these texts are ASCII).
pub fn at(text: &str, needle: &str, nth: usize, ahead: usize) -> Value {
    let offset = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("no {nth} `{needle}` in {text}"))
        .0
        + ahead;
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let character = offset - before.rfind('\n').map_or(0, |i| i + 1);
    json!({"line": line, "character": character})
}

/// `text` with one text edit made.
pub fn edited(text: &str, edit: &Value) -> String {
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
