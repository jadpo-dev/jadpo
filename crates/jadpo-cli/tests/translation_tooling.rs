//! Translations, the rest of area 10's "Translations as messages" (chunk
//! 5.4d), from the command line: a locale's text direction, failures worded
//! for client apps and exported per locale, stale and unaccepted
//! translations through `sources.jadpo_record`, the `jadpo translations`
//! commands, and the pseudo-locale.
//!
//! The projects have no entities, so no database is needed.

use super::http::{jadpo, tempdir, write};

const APP: &str = "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr, Locale.ar_eg]\n}\n";

#[test]
fn a_locale_knows_its_text_direction() {
    let dir = tempdir::Dir::new("translation-direction");
    let root = dir.path().join("shop");
    write(&root, "app.jadpo", APP);
    write(
        &root,
        "tests/direction.test.jadpo",
        r#"test "arabic is written right to left and English left to right" {
    assert locale.direction(Locale.ar_eg) == TextDirection.right_to_left
    assert locale.direction(Locale.en_gb) == TextDirection.left_to_right
    assert locale.direction(Locale.he_il) == TextDirection.right_to_left
}
"#,
    );
    let done = jadpo(&root, &["test"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(done.stdout.contains("1 passed"), "{}", done.stdout);
}

/// A booking app whose one route can answer with `SlotTaken`, which has a
/// time and a number for a client to format, and with `TableNotFound`, the
/// not-found failure the compiler provides for the `Table` entity.
const BOOKING: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr, Locale.fr_ca]\n    in_progress: [Locale.fr_ca]\n}\n",
    ),
    (
        "policy.jadpo",
        "use booking\n\npolicy {\n    actions: {\n        booking.book: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/booking/types/slot_time.jadpo",
        "/// When a table is free.\ntype SlotTime = Instant {}\n",
    ),
    (
        "domains/booking/types/seat_count.jadpo",
        "/// How many seats.\ntype SeatCount = Int {\n    min: 0\n}\n",
    ),
    (
        "domains/booking/types/book_request.jadpo",
        "use booking/types { SeatCount }\n\n/// A booking to make.\ntype BookRequest = Input {\n    seats: SeatCount\n}\n",
    ),
    (
        "domains/booking/types/booked.jadpo",
        "use booking/types { SeatCount }\n\n/// A booking made.\ntype Booked = Output {\n    seats: SeatCount\n}\n",
    ),
    (
        "domains/booking/failures/slot_taken.jadpo",
        "use booking/types { SeatCount, SlotTime }\n\n/// The slot was taken while the booking was made.\ntype SlotTaken = Failure {\n    kind: FailureKind.conflict\n    client: {\n        next_slot: SlotTime\n        seats_left: SeatCount\n    }\n}\n",
    ),
    (
        "domains/booking/failures/never_leaves.jadpo",
        "/// Raised and handled inside the server.\ntype NeverLeaves = Failure {\n    kind: FailureKind.rejected\n}\n",
    ),
    (
        "domains/booking/actions/book.jadpo",
        "use booking/types { Booked, SeatCount }\nuse booking/failures { SlotTaken }\n\n/// Books seats, or says when the next slot is.\naction book(seats: SeatCount) fails SlotTaken -> Booked {\n    if (seats > 4) {\n        reject SlotTaken {\n            next_slot: \"2026-10-20T18:30:00Z\"\n            seats_left: 2\n        }\n    }\n    return Booked {\n        seats: seats\n    }\n}\n",
    ),
    (
        "domains/booking/routes/bookings.jadpo",
        "use booking/types { BookRequest, Booked }\nuse booking/actions { book }\n\nroutes Bookings {\n    prefix: /bookings\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: BookRequest\n        output: Booked\n        run: book(input.seats)\n    }\n}\n",
    ),
    (
        "domains/booking/translations/en_gb.jadpo",
        "translations {\n    slot_taken: \"That slot has just gone. The next is {next_slot}, with {seats_left} seats left.\"\n}\n",
    ),
    (
        "domains/booking/translations/fr_fr.jadpo",
        "translations {\n    slot_taken: \"Ce créneau vient d'être pris. Le prochain est {next_slot}, avec {seats_left} places.\"\n}\n",
    ),
];

fn booking(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("booking");
    for (path, text) in BOOKING {
        write(&root, path, text);
    }
    (dir, root)
}

fn checked(root: &std::path::Path) -> jadpo_cli::Output {
    for args in [&["api", "update"][..], &["check"]] {
        let done = jadpo(root, args);
        if done.code != 0 && args[0] == "check" {
            return done;
        }
    }
    jadpo(root, &["check"])
}

#[test]
fn a_failure_a_route_can_answer_with_is_worded_in_every_locale_and_exported() {
    let (_dir, root) = booking("translation-failures");
    let done = checked(&root);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let read = |locale: &str| {
        let text = std::fs::read_to_string(
            root.join("build/openapi")
                .join(format!("main.failures.{locale}.json")),
        )
        .unwrap_or_else(|error| panic!("{locale}: {error}"));
        serde_json::from_str::<serde_json::Value>(&text).expect("json")
    };
    let formats = serde_json::json!({ "next_slot": "date_time", "seats_left": "number" });
    assert_eq!(
        read("fr_fr"),
        serde_json::json!({
            "slot_taken": {
                "formats": formats,
                "text": "Ce créneau vient d'être pris. Le prochain est {next_slot}, avec {seats_left} places."
            }
        })
    );
    assert_eq!(
        read("en_gb")["slot_taken"]["text"],
        "That slot has just gone. The next is {next_slot}, with {seats_left} seats left."
    );
    // The in-progress locale has no wording of its own: it is sent its
    // sibling's, so the client always has text for every code.
    assert_eq!(read("fr_ca"), read("fr_fr"));
}

#[test]
fn a_failure_with_no_wording_in_a_locale_is_an_error_naming_the_member_to_add() {
    let (_dir, root) = booking("translation-failures-missing");
    write(
        &root,
        "domains/booking/translations/fr_fr.jadpo",
        "translations {\n}\n",
    );
    let done = checked(&root);
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    let out = format!("{}{}", done.stdout, done.stderr);
    for expected in [
        "error: `slot_taken` has no wording in `domains/booking/translations/fr_fr.jadpo`",
        "rule: translation.wording_missing",
        "Add `slot_taken: \"...\"` to `domains/booking/translations/fr_fr.jadpo`, which may write `{next_slot}` and `{seats_left}` (agent)",
    ] {
        assert!(out.contains(expected), "missing `{expected}`:\n{out}");
    }
}

#[test]
fn a_failure_no_route_answers_with_needs_no_wording() {
    let (_dir, root) = booking("translation-failures-server-only");
    // `NeverLeaves` is in no route's `fails`; the project checks without it.
    let done = checked(&root);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let text = std::fs::read_to_string(root.join("build/openapi/main.failures.fr_fr.json"))
        .expect("export");
    assert!(!text.contains("never_leaves"), "{text}");
}

#[test]
fn a_client_field_a_client_cannot_format_is_an_error() {
    let (_dir, root) = booking("translation-failures-field");
    write(
        &root,
        "domains/booking/failures/slot_taken.jadpo",
        "use booking/types { SeatCount, SlotTime }\n\n/// The slot was taken while the booking was made.\ntype SlotTaken = Failure {\n    kind: FailureKind.conflict\n    client: {\n        next_slot: SlotTime\n        seats_left: SeatCount?\n    }\n}\n",
    );
    write(
        &root,
        "domains/booking/actions/book.jadpo",
        "use booking/types { Booked, SeatCount }\nuse booking/failures { SlotTaken }\n\n/// Books seats, or says when the next slot is.\naction book(seats: SeatCount) fails SlotTaken -> Booked {\n    if (seats > 4) {\n        reject SlotTaken {\n            next_slot: \"2026-10-20T18:30:00Z\"\n            seats_left: none\n        }\n    }\n    return Booked {\n        seats: seats\n    }\n}\n",
    );
    let done = checked(&root);
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    let out = format!("{}{}", done.stdout, done.stderr);
    assert!(out.contains("rule: translation.field_type"), "{out}");
    assert!(out.contains("`SlotTaken.seats_left`"), "{out}");
}
