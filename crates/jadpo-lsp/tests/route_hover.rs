//! Hovering over a route shows its full method and path (area 7, "Route
//! URL shown"), the prefix joined to the route's own path.

use serde_json::json;

use crate::support::{ask, at, project, start, uri};

const ROUTES: &str = "domains/notes/routes/notes.jadpo";
const ROUTES_TEXT: &str = "use notes/actions { echo }\nuse notes/types { NoteIn, NoteOut }\n\nroutes Notes {\n    prefix: /notes\n\n    POST /echo {\n        authentication: RouteAuthentication.none\n        input: NoteIn\n        output: NoteOut\n        run: echo(input)\n    }\n}\n";

#[test]
fn a_route_shows_its_full_path() {
    let root = project(
        "route-hover",
        &[
            ("app.jadpo", "application {}\n"),
            (
                "domains/notes/types/note_text.jadpo",
                "/// A note.\ntype NoteText = Text {\n    max_length: 80\n}\n",
            ),
            (
                "domains/notes/types/note_in.jadpo",
                "use notes/types { NoteText }\n\n/// A note sent.\ntype NoteIn = Input {\n    text: NoteText\n}\n",
            ),
            (
                "domains/notes/types/note_out.jadpo",
                "use notes/types { NoteText }\n\n/// A note sent back.\ntype NoteOut = Output {\n    text: NoteText\n}\n",
            ),
            (
                "domains/notes/actions/echo.jadpo",
                "use notes/types { NoteIn, NoteOut }\n\n/// Says it back.\naction echo(input: NoteIn) -> NoteOut {\n    return NoteOut {\n        text: input.text\n    }\n}\n",
            ),
            (ROUTES, ROUTES_TEXT),
        ],
    );
    let mut server = start(&root, json!({}));
    let answer = ask(
        &mut server,
        "textDocument/hover",
        json!({
            "textDocument": {"uri": uri(&root, ROUTES)},
            "position": at(ROUTES_TEXT, "POST", 0, 1),
        }),
    );
    let shown = answer["contents"]["value"].as_str().unwrap_or_default();
    assert!(shown.contains("POST /notes/echo"), "{answer:#}");
}
