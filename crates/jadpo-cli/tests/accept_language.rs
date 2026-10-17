//! Choosing a locale from a request (area 10, chunk 5.3c): `jadpo dev` runs
//! a project whose route reads the `Accept-Language` header into an
//! optional field, and each test sends requests as a browser would:
//!
//! - `locale.from_accept_language` gives the supported locale the header
//!   asks for, weights and languages as RFC 9110 reads them, and the
//!   `default:` for a header that asks for none, is malformed or is left
//!   out;
//! - the locale it gives writes the text the route answers with;
//! - `jadpo test` runs the same call in the project's own tests.
//!
//! The project has no entities, so no database is needed.

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr, Locale.ar_eg]\n}\n",
    ),
    (
        "policy.jadpo",
        "use shop\n\npolicy {\n    actions: {\n        shop.show_placed: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/shop/types/ordered_at.jadpo",
        "/// When an order was placed.\ntype OrderedAt = Instant {}\n",
    ),
    (
        "domains/shop/types/accept_language.jadpo",
        "/// What a browser says its reader's languages are.\ntype AcceptLanguage = Text {\n    max_length: 2000\n}\n",
    ),
    (
        "domains/shop/types/place_request.jadpo",
        "use shop/types { AcceptLanguage, OrderedAt }\n\n/// An order's placing, to be shown in the reader's language.\ntype PlaceRequest = Input {\n    ordered_at: OrderedAt\n    accept_language: AcceptLanguage {\n        header: \"Accept-Language\"\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/shop/types/placed_text.jadpo",
        "/// An order's placing, shown to its customer.\ntype PlacedText = Output {\n    language: Locale\n    placed: PresentationText\n}\n",
    ),
    (
        "domains/shop/actions/show_placed.jadpo",
        r#"use shop/types { AcceptLanguage, OrderedAt, PlacedText }

/// The placing, in the language the customer's browser asks for.
action show_placed(ordered_at: OrderedAt, accept_language: AcceptLanguage?) -> PlacedText {
    var chosen: Locale = locale.from_accept_language(accept_language)
    return PlacedText {
        language: chosen
        placed: text.format(ordered_at, zone: Zone.europe_london, locale: chosen, format: TimeFormat.date_long)
    }
}
"#,
    ),
    (
        "domains/shop/actions/show_placed.test.jadpo",
        r#"test "the browser's languages choose the locale" {
    assert show_placed("2026-09-27T13:30:05Z", "fr-CA, en;q=0.5").language == Locale.fr_fr
    assert show_placed("2026-09-27T13:30:05Z", "en-AU").language == Locale.en_us
    assert show_placed("2026-09-27T13:30:05Z", "en-GB;q=0.3, ar-EG;q=0.9").placed == "٢٧ سبتمبر ٢٠٢٦"
    assert show_placed("2026-09-27T13:30:05Z", "de-DE, ja;q=0.5").language == Locale.en_gb
    assert show_placed("2026-09-27T13:30:05Z", none).language == Locale.en_gb
}
"#,
    ),
    (
        "domains/shop/routes/placed.jadpo",
        r#"use shop/types { PlaceRequest, PlacedText }
use shop/actions { show_placed }

routes Placed {
    prefix: /placed

    POST / {
        authentication: RouteAuthentication.none
        input: PlaceRequest
        output: PlacedText
        run: show_placed(input.ordered_at, input.accept_language)
    }
}
"#,
    ),
];

fn project() -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new("accept-language");
    let root = dir.path().join("shop");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["api", "update"][..], &["check"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

fn placed(dev: &Dev, accept_language: Option<&str>) -> super::http::Reply {
    let headers: Vec<(&str, &str)> = accept_language
        .map(|value| ("Accept-Language", value))
        .into_iter()
        .collect();
    dev.send(
        "POST",
        "/placed",
        &headers,
        r#"{"ordered_at":"2026-09-27T13:30:05Z"}"#,
    )
}

#[test]
fn a_request_is_answered_in_the_locale_its_header_asks_for() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);
    for (header, language, shown) in [
        // A browser's usual header: French, with English behind it.
        ("fr-FR,fr;q=0.9,en;q=0.8", "fr-FR", "27 septembre 2026"),
        // A region the application does not support has the language's own.
        ("fr-CA", "fr-FR", "27 septembre 2026"),
        ("en-GB", "en-GB", "27 September 2026"),
        // English with no region means the United States.
        ("en", "en-US", "September 27, 2026"),
        // The heaviest tag that matches wins, not the first written.
        ("en-GB;q=0.3, ar-EG;q=0.9", "ar-EG", "٢٧ سبتمبر ٢٠٢٦"),
        // Weight zero excludes a tag, and a tag nothing supports is passed.
        (
            "fr-FR;q=0, de-DE, en-US;q=0.1",
            "en-US",
            "September 27, 2026",
        ),
        // Nothing usable is the default.
        ("de-DE, ja;q=0.8", "en-GB", "27 September 2026"),
        ("*", "en-GB", "27 September 2026"),
        ("fr-FR;q=nonsense", "en-GB", "27 September 2026"),
    ] {
        let reply = placed(&dev, Some(header));
        assert_eq!(reply.status, 200, "{header}: {reply:?}");
        assert_eq!(
            reply.json(),
            serde_json::json!({ "language": language, "placed": shown }),
            "{header}"
        );
    }
}

#[test]
fn a_request_with_no_header_is_answered_in_the_default() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);
    let reply = placed(&dev, None);
    assert_eq!(reply.status, 200, "{reply:?}");
    assert_eq!(
        reply.json(),
        serde_json::json!({ "language": "en-GB", "placed": "27 September 2026" })
    );
}

#[test]
fn the_same_call_runs_in_the_projects_own_tests() {
    let (_dir, root) = project();
    let done = jadpo(&root, &["test"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
}
