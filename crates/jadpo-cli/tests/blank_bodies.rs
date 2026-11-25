//! A request with no body (area 7, "A request with no body"; chunk
//! 10.7b): `jadpo dev` serves a project with two `POST` routes, and each
//! test sends one a body that is empty or only whitespace, as a client
//! that has nothing to say does:
//!
//! - `POST /tickets/solve`, whose input's one field may be left out, reads it as
//!   `{}`, with or without a JSON content type;
//! - `POST /tickets/reply`, whose input's field must be sent, refuses it as
//!   before: 400 `invalid_request`, saying a body is JSON when it isn't
//!   sent as JSON, and that the request couldn't be read when it is.
//!
//! The project has no entities, so no database is needed.

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "policy.jadpo",
        "use desk\n\npolicy {\n    actions: {\n        desk.solve: [Access.public]\n        desk.reply: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/desk/types/note.jadpo",
        "/// What an agent writes on a ticket.\ntype Note = Text {\n    max_length: 200\n}\n",
    ),
    (
        "domains/desk/types/solve_ticket.jadpo",
        "use desk/types { Note }\n\n/// What solving a ticket may say.\ntype SolveTicket = Input {\n    note: Note {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/desk/types/reply_ticket.jadpo",
        "use desk/types { Note }\n\n/// What replying to a ticket says.\ntype ReplyTicket = Input {\n    text: Note\n}\n",
    ),
    (
        "domains/desk/types/noted.jadpo",
        "use desk/types { Note }\n\n/// What was written, if anything.\ntype Noted = Output {\n    note: Note?\n}\n",
    ),
    (
        "domains/desk/actions/solve.jadpo",
        "use desk/types { Noted, SolveTicket }\n\n/// Solves a ticket, with a note when one is given.\naction solve(input: SolveTicket) -> Noted {\n    return Noted {\n        note: input.note\n    }\n}\n",
    ),
    (
        "domains/desk/actions/reply.jadpo",
        "use desk/types { Noted, ReplyTicket }\n\n/// Replies to a ticket.\naction reply(input: ReplyTicket) -> Noted {\n    return Noted {\n        note: input.text\n    }\n}\n",
    ),
    (
        "domains/desk/routes/tickets.jadpo",
        r#"use desk/types { Noted, ReplyTicket, SolveTicket }
use desk/actions { reply, solve }

/// A ticket's answers.
routes Tickets {
    prefix: /tickets

    /// Solves a ticket.
    POST /solve {
        authentication: RouteAuthentication.none
        input: SolveTicket
        output: Noted
        run: solve(input)
    }

    /// Replies to a ticket.
    POST /reply {
        authentication: RouteAuthentication.none
        input: ReplyTicket
        output: Noted
        run: reply(input)
    }
}
"#,
    ),
];

fn project() -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new("blank-bodies");
    let root = dir.path().join("desk");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["records", "update"][..], &["check"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

/// Each blank body, with the content type it is sent with: none, JSON
/// (which the client adds to a body that isn't empty) and plain text.
const BLANKS: [(&str, Option<&str>); 3] = [("", None), ("  ", None), ("\r\n", Some("text/plain"))];

fn sent(dev: &Dev, path: &str, body: &str, content_type: Option<&str>) -> super::http::Reply {
    let headers: Vec<(&str, &str)> = content_type
        .map(|value| ("content-type", value))
        .into_iter()
        .collect();
    dev.send("POST", path, &headers, body)
}

#[test]
fn a_bodyless_post_reads_as_empty_where_no_field_must_be_sent() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);
    for (body, content_type) in BLANKS {
        let solved = sent(&dev, "/tickets/solve", body, content_type);
        assert_eq!(solved.status, 200, "{body:?} {content_type:?}: {solved:?}");
        assert_eq!(solved.json(), serde_json::json!({ "note": null }));
    }
    let noted = sent(
        &dev,
        "/tickets/solve",
        r#"{"note":"Reset the router"}"#,
        None,
    );
    assert_eq!(
        noted.json(),
        serde_json::json!({ "note": "Reset the router" })
    );
}

#[test]
fn a_bodyless_post_is_refused_where_a_field_must_be_sent() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);
    for (body, content_type, message) in [
        (
            "",
            None,
            "A request body is JSON, sent with `Content-Type: application/json`.",
        ),
        ("  ", None, "Request could not be read."),
        (
            "\r\n",
            Some("text/plain"),
            "A request body is JSON, sent with `Content-Type: application/json`.",
        ),
    ] {
        let refused = sent(&dev, "/tickets/reply", body, content_type);
        assert_eq!(refused.status, 400, "{body:?}: {refused:?}");
        assert_eq!(refused.json()["error"]["code"], "invalid_request");
        assert_eq!(refused.json()["error"]["message"], message, "{body:?}");
    }
}
