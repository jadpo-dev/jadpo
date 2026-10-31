//! Rules across fields over real HTTP (area 2, "Rules across fields",
//! chunk 5.5t): `jadpo dev` runs a project whose route takes a booking
//! with a rule across its fields, and a client sends one:
//!
//! - a request that breaks the rule is refused as invalid input, the
//!   problem naming the rule at the object's path, never a value;
//! - one that follows it is answered;
//! - the OpenAPI document lists the rule in the input's description.
//!
//! The project has no entities, so no database is needed.

use std::path::Path;

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/bookings/actions/book.jadpo",
        r#"use bookings/types { BookingRequest, BookingView }

/// Books a table for the times asked for.
action book(input: BookingRequest) -> BookingView {
    return BookingView {
        starts: input.starts
        ends: input.ends
    }
}
"#,
    ),
    (
        "domains/bookings/routes/bookings.jadpo",
        r#"use bookings/types { BookingRequest, BookingView }
use bookings/actions { book }

routes Bookings {
    prefix: /bookings

    POST / {
        authentication: RouteAuthentication.none
        input: BookingRequest
        output: BookingView
        run: book(input)
    }
}
"#,
    ),
    (
        "domains/bookings/types/booking_request.jadpo",
        r#"use bookings/types { Moment }

/// The times a table is asked for.
type BookingRequest = Input {
    starts: Moment
    ends: Moment
    rule ends_after_start: ends > starts
}
"#,
    ),
    (
        "domains/bookings/types/booking_view.jadpo",
        r#"use bookings/types { Moment }

/// A table booked.
type BookingView = Output {
    starts: Moment
    ends: Moment
}
"#,
    ),
    (
        "domains/bookings/types/moment.jadpo",
        "/// A moment in time.\ntype Moment = Instant {}\n",
    ),
    (
        "policy.jadpo",
        "use bookings\n\npolicy {\n    actions: {\n        bookings.book: [Access.public]\n    }\n}\n",
    ),
];

fn project() -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new("across-fields");
    let root = dir.path().join("bookings");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["records", "update"][..], &["check"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

const NINE: &str = "2026-03-02T09:00:00Z";
const TEN: &str = "2026-03-02T10:00:00Z";

#[test]
fn a_request_that_breaks_a_rule_is_refused_by_its_name() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);
    let body = |starts: &str, ends: &str| {
        serde_json::json!({ "starts": starts, "ends": ends }).to_string()
    };

    let booked = dev.send("POST", "/bookings", &[], &body(NINE, TEN));
    assert_eq!(booked.status, 200, "{booked:?}");
    assert_eq!(
        booked.json(),
        serde_json::json!({ "starts": NINE, "ends": TEN })
    );

    let backwards = dev.send("POST", "/bookings", &[], &body(TEN, NINE));
    assert_eq!(backwards.status, 422, "{backwards:?}");
    let shown = backwards.json().to_string();
    assert!(shown.contains("ends_after_start"), "{shown}");
    assert!(!shown.contains("2026"), "{shown}");
}

#[test]
fn the_openapi_document_lists_the_rule() {
    let (_dir, root) = project();
    let text = std::fs::read_to_string(Path::new(&root).join("build/openapi/main.json"))
        .expect("written by api update");
    let document: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(
        document["components"]["schemas"]["BookingRequest"]["description"],
        "The times a table is asked for.\n\nRules across its fields: `ends_after_start`: `ends > starts`."
    );
}
