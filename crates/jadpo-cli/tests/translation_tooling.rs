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
        "translations/fr_fr.jadpo",
        "translations {\n    jadpo: {\n        deadline_exceeded: \"fr: deadline_exceeded.\"\n        idempotency_key_reused: \"fr: idempotency_key_reused.\"\n        internal_fault: \"fr: internal_fault.\"\n        invalid_input: \"fr: invalid_input.\"\n        invalid_request: \"fr: invalid_request.\"\n        method_not_allowed: \"fr: method_not_allowed.\"\n        rate_limited: \"fr: rate_limited.\"\n        request_too_large: \"fr: request_too_large.\"\n        route_not_found: \"fr: route_not_found.\"\n        unavailable: \"fr: unavailable.\"\n    }\n}\n",
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
        read("fr_fr")["slot_taken"],
        serde_json::json!({
            "formats": formats,
            "text": "Ce créneau vient d'être pris. Le prochain est {next_slot}, avec {seats_left} places."
        })
    );
    // Jadpo's own failures join the app's: the French is the app's.
    assert_eq!(read("fr_fr")["invalid_input"]["text"], "fr: invalid_input.");
    assert_eq!(
        read("en_gb")["invalid_input"]["text"],
        "Some of the information sent isn't valid. Check it and try again."
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

// --- the commands --------------------------------------------------------------------

/// A shop in English with German complete, French lacking a message, and a
/// Canadian French still in progress, reviewed by a person.
const SHOP: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    translations: {\n        review: TranslationReview.required\n    }\n}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.de_de, Locale.fr_fr, Locale.fr_ca]\n    in_progress: [Locale.fr_ca]\n}\n",
    ),
    (
        "policy.jadpo",
        "use shop\n\npolicy {\n    actions: {\n        shop.describe: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/shop/types/customer_name.jadpo",
        "/// A customer's first name.\ntype CustomerName = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/shop/types/parcel_count.jadpo",
        "/// How many parcels.\ntype ParcelCount = Int {\n    min: 0\n}\n",
    ),
    (
        "domains/shop/types/described.jadpo",
        "/// What a customer is told.\ntype Described = Output {\n    subject: PresentationText\n    parcels: PresentationText\n}\n",
    ),
    (
        "domains/shop/actions/describe.jadpo",
        "use shop/types { CustomerName, Described, ParcelCount }\n\n/// What a customer is told about their order.\naction describe(name: CustomerName, count: ParcelCount, reader_locale: Locale) -> Described {\n    return Described {\n        subject: translations.shop.order_shipped(name: name, locale: reader_locale)\n        parcels: translations.shop.parcels(count: count, locale: reader_locale)\n    }\n}\n",
    ),
    (
        "domains/shop/translations/order_shipped.jadpo",
        "/// Subject of the email sent when a parcel leaves the warehouse.\ntype OrderShipped = Message {\n    name: Text\n}\n",
    ),
    (
        "domains/shop/translations/parcels.jadpo",
        "/// How many parcels an order has.\ntype Parcels = Message {\n    count: PluralCount\n}\n",
    ),
    (
        "domains/shop/translations/farewell.jadpo",
        "/// Said when a customer signs out.\ntype Farewell = Message {\n}\n",
    ),
    (
        "domains/shop/translations/en_gb.jadpo",
        "translations {\n    order_shipped: \"Good day {name}, your order has shipped.\"\n    parcels: {\n        one: \"{count} parcel\"\n        other: \"{count} parcels\"\n    }\n    farewell: \"Goodbye.\"\n}\n",
    ),
    (
        "domains/shop/translations/de_de.jadpo",
        "translations {\n    order_shipped: \"Guten Tag {name}, Ihre Bestellung wurde versandt.\"\n    parcels: {\n        one: \"{count} Paket\"\n        other: \"{count} Pakete\"\n    }\n    farewell: \"Auf Wiedersehen.\"\n}\n",
    ),
    (
        "domains/shop/translations/fr_fr.jadpo",
        "translations {\n    order_shipped: \"Bonjour {name}, votre commande a été expédiée.\"\n    parcels: {\n        one: \"{count} colis\"\n        many: \"{count} de colis\"\n        other: \"{count} colis\"\n    }\n    farewell: \"Au revoir.\"\n}\n",
    ),
    (
        "domains/shop/translations/fr_ca.jadpo",
        "translations {\n    order_shipped: \"Bonjour {name}, votre commande est partie.\"\n}\n",
    ),
];

/// A language file of the shop, as written.
fn shop_file(locale: &str) -> String {
    let path = format!("domains/shop/translations/{locale}.jadpo");
    SHOP.iter()
        .find(|(candidate, _)| *candidate == path)
        .map(|(_, text)| text.to_string())
        .expect("a language file")
}

fn shop(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("shop");
    for (path, text) in SHOP {
        write(&root, path, text);
    }
    (dir, root)
}

fn plain(root: &std::path::Path, args: &[&str]) -> (u8, String) {
    let done = jadpo(root, args);
    (done.code, format!("{}{}", done.stdout, done.stderr))
}

#[test]
fn missing_lists_what_is_left_by_file() {
    let (_dir, root) = shop("translation-missing");
    let (code, out) = plain(&root, &["translations", "missing"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "domains/shop/translations/de_de.jadpo
  farewell       unaccepted  new
  order_shipped  unaccepted  new
  parcels        unaccepted  new

domains/shop/translations/fr_ca.jadpo
  farewell       missing     falls back to fr_fr
  parcels        missing     falls back to fr_fr
  order_shipped  unaccepted  new

domains/shop/translations/fr_fr.jadpo
  farewell       unaccepted  new
  order_shipped  unaccepted  new
  parcels        unaccepted  new

9 outstanding: 2 missing, 7 unaccepted, 3 locales

next: jadpo translations accept de_de  records a person's acceptance of the locale's wording
"
    );
    // One locale.
    let (_, only) = plain(&root, &["translations", "missing", "fr_ca"]);
    assert!(
        only.starts_with("domains/shop/translations/fr_ca.jadpo\n"),
        "{only}"
    );
    assert!(
        only.contains("3 outstanding: 2 missing, 1 unaccepted, 1 locale"),
        "{only}"
    );
}

#[test]
fn missing_names_a_locale_whose_file_does_not_exist_yet() {
    let (_dir, root) = shop("translation-missing-file");
    std::fs::remove_file(root.join("domains/shop/translations/fr_fr.jadpo")).expect("remove");
    let (_, out) = plain(&root, &["translations", "missing", "fr_fr"]);
    assert!(
        out.starts_with("domains/shop/translations/fr_fr.jadpo  no file yet\n  farewell"),
        "{out}"
    );
    assert!(
        out.contains("farewell       missing     needed, or jadpo check fails"),
        "{out}"
    );
}

#[test]
fn missing_as_data() {
    let (_dir, root) = shop("translation-missing-json");
    let done = jadpo(&root, &["translations", "missing", "--json"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let document: serde_json::Value = serde_json::from_str(&done.stdout).expect("json");
    let data = &document["data"];
    assert_eq!(data["missing"], 2);
    assert_eq!(data["unaccepted"], 7);
    assert_eq!(data["stale"], 0);
    assert_eq!(
        data["outstanding"][3],
        serde_json::json!({
            "locale": "fr_ca",
            "member": "farewell",
            "state": "missing",
            "path": "domains/shop/translations/fr_ca.jadpo",
            "fileExists": true,
            "note": "falls back to fr_fr",
            "failsCheck": false
        })
    );
}

#[test]
fn unused_lists_messages_no_code_calls() {
    let (_dir, root) = shop("translation-unused");
    let (code, out) = plain(&root, &["translations", "unused"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "Unused\n  Farewell  domains/shop/translations/farewell.jadpo:2\n\n1 unused: 2 of 3 used\n"
    );
    // Calling it from code, even a test, is not the application using it.
    write(
        &root,
        "domains/shop/actions/describe.test.jadpo",
        "test \"it says goodbye\" {\n    assert translations.shop.farewell(locale: Locale.de_de) == \"Auf Wiedersehen.\"\n}\n",
    );
    let (_, with_test) = plain(&root, &["translations", "unused"]);
    assert_eq!(with_test, out);
    write(
        &root,
        "domains/shop/actions/describe.jadpo",
        "use shop/types { CustomerName, Described, ParcelCount }\n\n/// What a customer is told about their order.\naction describe(name: CustomerName, count: ParcelCount, reader_locale: Locale) -> Described {\n    var bye: PresentationText = translations.shop.farewell(locale: reader_locale)\n    return Described {\n        subject: translations.shop.order_shipped(name: name, locale: reader_locale)\n        parcels: translations.shop.parcels(count: count, locale: reader_locale)\n    }\n}\n",
    );
    let (_, used) = plain(&root, &["translations", "unused"]);
    assert_eq!(used, "Nothing unused: 3 of 3 used\n");
}

#[test]
fn update_records_what_each_translation_was_made_from_and_accept_accepts_it() {
    let (_dir, root) = shop("translation-update");
    let (code, out) = plain(&root, &["translations", "update"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "Wrote domains/shop/translations/sources.jadpo_record
    de_de.farewell       recorded
    de_de.order_shipped  recorded
    de_de.parcels        recorded
    fr_ca.order_shipped  recorded
    fr_fr.farewell       recorded
    fr_fr.order_shipped  recorded
    fr_fr.parcels        recorded

next: git diff  shows what was recorded
"
    );
    let record =
        std::fs::read_to_string(root.join("domains/shop/translations/sources.jadpo_record"))
            .expect("record");
    assert!(
        record.starts_with("// Generated by Jadpo from this folder's translations; update it with `jadpo translations update`, never by hand.\nrecord translations 1\n\ntranslation de_de.farewell from "),
        "{record}"
    );
    assert_eq!(record.lines().count(), 10, "{record}");
    // The same again records nothing.
    let (_, again) = plain(&root, &["translations", "update"]);
    assert_eq!(again, "Already up to date\n");

    // Accepting a locale accepts what it has.
    let (code, out) = plain(&root, &["translations", "accept", "de_de"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "Wrote domains/shop/translations/sources.jadpo_record
    de_de.farewell       accepted
    de_de.order_shipped  accepted
    de_de.parcels        accepted

next: git diff  shows the acceptance a reviewer approves
"
    );
    let (_, left) = plain(&root, &["translations", "missing", "de_de"]);
    assert_eq!(left, "Nothing outstanding: de_de\n");
    // Rewording one makes it unaccepted again, until it is accepted.
    write(
        &root,
        "domains/shop/translations/de_de.jadpo",
        &shop_file("de_de").replace("Auf Wiedersehen.", "Tschüss."),
    );
    let (_, reworded) = plain(&root, &["translations", "missing", "de_de"]);
    assert!(
        reworded.contains("farewell  unaccepted  reworded since it was accepted"),
        "{reworded}"
    );
    let (_, updated) = plain(&root, &["translations", "update"]);
    assert!(
        updated.contains("de_de.farewell  reworded, acceptance dropped"),
        "{updated}"
    );
}

#[test]
fn a_translation_made_from_wording_that_changed_is_stale_until_it_is_redone() {
    let (_dir, root) = shop("translation-stale");
    assert_eq!(plain(&root, &["translations", "update"]).0, 0);
    assert_eq!(plain(&root, &["api", "update"]).0, 0);
    // Nothing is stale yet.
    let (code, out) = plain(&root, &["check"]);
    assert_eq!(code, 0, "{out}");

    // The English changes: German and French are stale, Canadian French is
    // in progress, so it is listed and not an error.
    write(
        &root,
        "domains/shop/translations/en_gb.jadpo",
        &shop_file("en_gb").replace("Good day", "Hello"),
    );
    let (code, out) = plain(&root, &["check"]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(out.matches("rule: translation.stale").count(), 2, "{out}");
    assert!(
        out.contains("error: `order_shipped` in `de_de` was translated from `en_gb` wording that has since changed"),
        "{out}"
    );
    assert!(
        out.contains("next: Reword `order_shipped` in `domains/shop/translations/de_de.jadpo` to match the new `en_gb` wording, then run `jadpo translations update` (agent)"),
        "{out}"
    );
    let (_, listed) = plain(&root, &["translations", "missing"]);
    assert!(
        listed.contains("order_shipped  stale       en_gb reworded since it was translated"),
        "{listed}"
    );
    assert!(listed.contains("3 stale"), "{listed}");

    // `update` on its own does not make a translation nobody redid fresh.
    let (_, still) = plain(&root, &["translations", "update"]);
    assert!(
        still.contains("de_de.order_shipped  still stale, reword it first"),
        "{still}"
    );
    assert_eq!(plain(&root, &["check"]).0, 1);

    // Redoing the German and updating clears its error.
    write(
        &root,
        "domains/shop/translations/de_de.jadpo",
        &shop_file("de_de").replace("Guten Tag", "Hallo"),
    );
    let (_, redone) = plain(&root, &["translations", "update"]);
    assert!(
        redone.contains("de_de.order_shipped  reworded, acceptance dropped"),
        "{redone}"
    );
    let (_, out) = plain(&root, &["check"]);
    assert_eq!(out.matches("rule: translation.stale").count(), 1, "{out}");
    assert!(out.contains("`order_shipped` in `fr_fr`"), "{out}");
}

#[test]
fn accept_leaves_a_stale_translation_alone() {
    let (_dir, root) = shop("translation-accept-stale");
    write(
        &root,
        "domains/shop/translations/fr_fr.jadpo",
        &shop_file("fr_fr"),
    );
    assert_eq!(plain(&root, &["translations", "update"]).0, 0);
    write(
        &root,
        "domains/shop/translations/en_gb.jadpo",
        &shop_file("en_gb").replace("Goodbye.", "Bye."),
    );
    let (code, out) = plain(&root, &["translations", "accept", "fr_fr"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("fr_fr.farewell       still stale, reword it first"),
        "{out}"
    );
    assert!(out.contains("fr_fr.order_shipped  accepted"), "{out}");
    assert!(out.contains("next: jadpo translations missing"), "{out}");
}

#[test]
fn the_commands_refuse_what_they_cannot_do() {
    let (_dir, root) = shop("translation-arguments");
    let (code, out) = plain(&root, &["translations"]);
    assert_eq!(code, 2);
    assert!(out.contains("rule: cli.translations_arguments"), "{out}");
    let (_, slip) = plain(&root, &["translations", "mising"]);
    assert!(slip.contains("Did you mean `missing`?"), "{slip}");
    let (_, accept) = plain(&root, &["translations", "accept"]);
    assert!(accept.contains("needs a locale"), "{accept}");
    assert!(accept.contains("`de_de`, `fr_fr` or `fr_ca`"), "{accept}");
    let (_, default) = plain(&root, &["translations", "accept", "en_gb"]);
    assert!(
        default.contains("`en_gb` isn't a locale with translations"),
        "{default}"
    );
    let (_, near) = plain(&root, &["translations", "accept", "fr_f"]);
    assert!(near.contains("Did you mean `fr_fr`?"), "{near}");
    let (_, extra) = plain(&root, &["translations", "unused", "fr_fr"]);
    assert!(extra.contains("takes nothing after it"), "{extra}");
}

#[test]
fn a_project_without_locales_has_nothing_to_translate() {
    let dir = tempdir::Dir::new("translation-none");
    let root = dir.path().join("plain");
    write(&root, "app.jadpo", "application {}\n");
    assert_eq!(
        plain(&root, &["translations", "missing"]),
        (0, "Nothing outstanding\n".to_string())
    );
    assert_eq!(
        plain(&root, &["translations", "unused"]),
        (0, "No messages\n".to_string())
    );
    assert_eq!(
        plain(&root, &["translations", "update"]),
        (0, "Already up to date\n".to_string())
    );
}

// --- the pseudo-locale ---------------------------------------------------------------

#[test]
fn the_pseudo_locale_writes_every_message_accented_and_lengthened() {
    let (_dir, root) = shop("translation-pseudo");
    // List it, as any locale is; it needs no language file.
    let app = SHOP[0].1.replace(
        "supported: [Locale.en_gb, Locale.de_de, Locale.fr_fr, Locale.fr_ca]",
        "supported: [Locale.en_gb, Locale.de_de, Locale.fr_fr, Locale.fr_ca, Locale.en_xa]",
    );
    write(&root, "app.jadpo", &app);
    write(
        &root,
        "domains/shop/actions/describe.test.jadpo",
        r#"test "every message is accented and lengthened in the pseudo-locale" {
    assert translations.shop.parcels(count: 1, locale: Locale.en_xa) == "[1 þáŕçéļ ~~~~]"
    assert translations.shop.parcels(count: 1234, locale: Locale.en_xa) == "[1,234 þáŕçéļš ~~~~~~]"
    assert translations.shop.order_shipped(name: "Ana", locale: Locale.en_xa) == "[Ĝóóð ðáý Ana, ýóúŕ óŕðéŕ ĥáš šĥíþþéð. ~~~~~~~~~~~~~~~]"
    assert translations.shop.farewell(locale: Locale.en_xa) == "[Ĝóóðƀýé. ~~~~]"
}
"#,
    );
    let (code, out) = plain(&root, &["api", "update"]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = plain(&root, &["check"]);
    assert_eq!(code, 0, "{out}");
    let done = jadpo(&root, &["test"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    // Nothing is left to translate for it, and it takes no acceptance.
    let (_, listed) = plain(&root, &["translations", "missing"]);
    assert!(!listed.contains("en_xa"), "{listed}");
    let (_, refused) = plain(&root, &["translations", "accept", "en_xa"]);
    assert!(
        refused.contains("isn't a locale with translations"),
        "{refused}"
    );
}

#[test]
fn the_pseudo_locale_is_listed_like_any_other() {
    let (_dir, root) = shop("translation-pseudo-unlisted");
    write(
        &root,
        "domains/shop/actions/describe.test.jadpo",
        "test \"it is not listed\" {\n    assert translations.shop.farewell(locale: Locale.en_xa) == \"x\"\n}\n",
    );
    let (code, out) = plain(&root, &["check"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rule: type.locale_unsupported"), "{out}");
}

#[test]
fn the_pseudo_locale_is_exported_for_client_apps_with_its_placeholders_intact() {
    let (_dir, root) = booking("translation-pseudo-failures");
    write(
        &root,
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr, Locale.fr_ca, Locale.en_xa]\n    in_progress: [Locale.fr_ca]\n}\n",
    );
    let done = checked(&root);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let text = std::fs::read_to_string(root.join("build/openapi/main.failures.en_xa.json"))
        .expect("export");
    let document: serde_json::Value = serde_json::from_str(&text).expect("json");
    let shown = document["slot_taken"]["text"].as_str().expect("text");
    assert!(
        shown.starts_with(
            "[Ţĥáţ šļóţ ĥáš ĵúšţ ĝóñé. Ţĥé ñéẋţ íš {next_slot}, ŵíţĥ {seats_left} šéáţš ļéƒţ. ~"
        ),
        "{shown}"
    );
    assert!(shown.ends_with("~]"), "{shown}");
    assert_eq!(
        document["slot_taken"]["formats"],
        serde_json::json!({ "next_slot": "date_time", "seats_left": "number" })
    );
}
