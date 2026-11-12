//! Hovering over a typed hole shows what its error says (area 2, "Typed
//! holes"): the type that goes there and the values in scope that fit,
//! closest first.

use serde_json::json;

use crate::support::{ask, at, project, start, uri};

const MAIN: &str = "main.jadpo";
const MAIN_TEXT: &str = "type BookTitle = Text {\n    max_length: 200\n}\n\ntype Book = Object {\n    title: BookTitle\n    author: PersonName\n}\n\nfunction shelve(title: BookTitle, author: PersonName) -> Book {\n    return Book {\n        title: title\n        author: _\n    }\n}\n";

#[test]
fn a_hole_shows_its_type_and_what_fits() {
    let root = project("hole-hover", &[(MAIN, MAIN_TEXT)]);
    let mut server = start(&root, json!({}));
    let answer = ask(
        &mut server,
        "textDocument/hover",
        json!({
            "textDocument": {"uri": uri(&root, MAIN)},
            "position": at(MAIN_TEXT, "_\n", 0, 0),
        }),
    );
    let shown = answer["contents"]["value"].as_str().unwrap_or_default();
    assert!(shown.contains("_: PersonName"), "{answer:#}");
    assert!(shown.contains("- `author`: `PersonName`"), "{answer:#}");
}
