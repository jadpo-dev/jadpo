//! `jadpo lsp` driven over standard input and output as an editor drives
//! it: the handshake, the diagnostics `jadpo check` gives, cleared and
//! raised again as the editor edits a file, hover and go to definition,
//! then shutdown and exit.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use serde_json::{Value, json};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/notes/types/body.jadpo",
        "/// The text of a note.\ntype Body = Text {\n    min_length: 1\n}\n",
    ),
    (
        "domains/notes/failures/note_refused.jadpo",
        "type NoteRefused = Failure {\n    kind: FailureKind.rejected\n}\n",
    ),
    (
        "domains/notes/libraries/wording.jadpo",
        "library wording {\n    /// Keeps a body as it is.\n    function keep(body: Body) -> Body {\n        return body\n    }\n}\n",
    ),
    (
        "domains/notes/actions/save_note.jadpo",
        "/// Saves a note.\naction save_note(body: Body) fails NoteRefused -> Body {\n    if (body == Body(\"no\")) {\n        reject NoteRefused\n    }\n    return wording.keep(body)\n}\n",
    ),
    ("domains/notes/actions/broken.jadpo", BROKEN),
];

const BROKEN: &str = "action broken(body: Body) -> Body {\n    return kepe(body)\n}\n";
const FIXED: &str = "action broken(body: Body) -> Body {\n    return wording.keep(body)\n}\n";

fn project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lsp-project");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in FILES {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root.canonicalize().expect("real path")
}

fn uri(root: &Path, path: &str) -> String {
    format!("file://{}", root.join(path).display())
}

struct Editor {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
}

impl Editor {
    fn start(root: &Path) -> Editor {
        let mut child = Command::new(env!("CARGO_BIN_EXE_jadpo"))
            .arg("lsp")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("jadpo lsp starts");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let (sender, messages) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let line = line.trim_end();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length: ") {
                        length = value.parse().expect("length");
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).expect("body");
                let message: Value = serde_json::from_slice(&body).expect("JSON");
                if sender.send(message).is_err() {
                    return;
                }
            }
        });
        Editor {
            child,
            stdin,
            messages,
        }
    }

    fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write");
        self.stdin.flush().expect("flush");
    }

    fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.wait(|message| message["id"] == id)
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    /// The next message `wanted` accepts, skipping the others.
    fn wait(&self, wanted: impl Fn(&Value) -> bool) -> Value {
        loop {
            let message = self
                .messages
                .recv_timeout(Duration::from_secs(20))
                .expect("a message from the server");
            if wanted(&message) {
                return message;
            }
        }
    }

    /// The next diagnostics published for `uri`.
    fn diagnostics(&self, uri: &str) -> Vec<Value> {
        let message = self.wait(|message| {
            message["method"] == "textDocument/publishDiagnostics"
                && message["params"]["uri"] == uri
        });
        message["params"]["diagnostics"]
            .as_array()
            .expect("list")
            .clone()
    }
}

fn position(line: u32, character: u32) -> Value {
    json!({"line": line, "character": character})
}

#[test]
fn an_editor_gets_diagnostics_hover_and_definitions_over_stdio() {
    let root = project();
    let mut editor = Editor::start(&root);
    let initialized = editor.request(
        1,
        "initialize",
        json!({
            "processId": null,
            "rootUri": format!("file://{}", root.display()),
            "capabilities": {"general": {"positionEncodings": ["utf-16"]}},
        }),
    );
    let capabilities = &initialized["result"]["capabilities"];
    assert_eq!(capabilities["hoverProvider"], true);
    assert_eq!(capabilities["definitionProvider"], true);
    assert_eq!(capabilities["positionEncoding"], "utf-16");
    assert_eq!(initialized["result"]["serverInfo"]["name"], "jadpo");
    editor.notify("initialized", json!({}));

    // The same problem `jadpo check` reports, in the error shape.
    let broken = uri(&root, "domains/notes/actions/broken.jadpo");
    let problems = editor.diagnostics(&broken);
    assert_eq!(problems.len(), 1, "{problems:?}");
    let problem = &problems[0];
    assert_eq!(problem["code"], "semantic.unknown_callee");
    assert_eq!(problem["source"], "jadpo");
    assert_eq!(problem["severity"], 1);
    assert_eq!(problem["range"]["start"], position(1, 11));
    assert_eq!(problem["range"]["end"], position(1, 15));
    let message = problem["message"].as_str().expect("message");
    assert!(
        message.starts_with("`kepe` isn't a function, action or query\n"),
        "{message}"
    );
    assert!(message.contains("\nnext: "), "{message}");

    // Fixing it in the editor clears it; breaking it again raises it.
    editor.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": broken, "languageId": "jadpo", "version": 1, "text": FIXED}}),
    );
    assert_eq!(editor.diagnostics(&broken), Vec::<Value>::new());
    editor.notify(
        "textDocument/didChange",
        json!({"textDocument": {"uri": broken, "version": 2}, "contentChanges": [{"text": BROKEN.replace("kepe", "kpee")}]}),
    );
    let again = editor.diagnostics(&broken);
    assert_eq!(again.len(), 1);
    assert!(
        again[0]["message"]
            .as_str()
            .expect("message")
            .contains("`kpee`")
    );

    // Hover over a call names the function, its description and callers.
    let save = uri(&root, "domains/notes/actions/save_note.jadpo");
    let hover = editor.request(
        2,
        "textDocument/hover",
        json!({"textDocument": {"uri": save}, "position": position(5, 21)}),
    );
    let value = hover["result"]["contents"]["value"]
        .as_str()
        .expect("hover");
    assert!(
        value.starts_with(
            "```jadpo\nfunction keep(body: Body) -> Body\n```\n\nKeeps a body as it is."
        ),
        "{value}"
    );
    assert!(value.contains("called by `save_note`"), "{value}");
    assert_eq!(hover["result"]["range"]["start"], position(5, 19));

    // Hover over an action names the failures it can produce.
    let hover = editor.request(
        3,
        "textDocument/hover",
        json!({"textDocument": {"uri": save}, "position": position(1, 9)}),
    );
    let value = hover["result"]["contents"]["value"]
        .as_str()
        .expect("hover");
    assert!(
        value.starts_with(
            "```jadpo\naction save_note(body: Body) fails NoteRefused -> Body\n```\n\nSaves a note."
        ),
        "{value}"
    );
    assert!(
        value.contains("fails with `NoteRefused` (rejected · 422)"),
        "{value}"
    );

    // Go to definition lands on the declaration.
    let found = editor.request(
        4,
        "textDocument/definition",
        json!({"textDocument": {"uri": save}, "position": position(1, 25)}),
    );
    assert_eq!(
        found["result"]["uri"],
        uri(&root, "domains/notes/types/body.jadpo")
    );
    assert_eq!(found["result"]["range"]["start"], position(1, 0));

    let shutdown = editor.request(5, "shutdown", Value::Null);
    assert_eq!(shutdown["result"], Value::Null);
    editor.notify("exit", Value::Null);
    let status = editor.child.wait().expect("exits");
    assert!(status.success(), "{status}");
}
