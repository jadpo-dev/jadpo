//! Locales over real HTTP (area 10, chunk 5.3): `jadpo dev` runs a project
//! whose `locales { }` supports two of the standard locales, and each test
//! sends requests as a client would:
//!
//! - a `Locale` in a request is one of the supported ones or an invalid
//!   value, like any enum with a variant the sender made up;
//! - the text a route gives for a person is written in that locale, in its
//!   own words and digits, whatever the server's own locale is;
//! - the OpenAPI document lists the supported locales and no others.
//!
//! The project has no entities, so no database is needed.

use std::path::Path;

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr, Locale.ar_eg]\n}\n",
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
        "domains/shop/types/place_request.jadpo",
        "use shop/types { OrderedAt }\n\n/// An order's placing, to be shown in a language.\ntype PlaceRequest = Input {\n    ordered_at: OrderedAt\n    locale: Locale\n}\n",
    ),
    (
        "domains/shop/types/placed_text.jadpo",
        "/// An order's placing, shown to its customer.\ntype PlacedText = Output {\n    placed: PresentationText\n    relative: PresentationText\n}\n",
    ),
    (
        "domains/shop/actions/show_placed.jadpo",
        r#"use shop/types { PlaceRequest, PlacedText }

/// The placing, in the customer's language and the shop's zone.
action show_placed(input: PlaceRequest) -> PlacedText {
    return PlacedText {
        placed: temporal.format(input.ordered_at, zone: Zone.europe_london, locale: input.locale, format: TimeFormat.date_long)
        relative: temporal.format_relative(input.ordered_at, now: clock.now, zone: Zone.europe_london, locale: input.locale)
    }
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
        run: show_placed(input)
    }
}
"#,
    ),
];

fn project() -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new("locales");
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

fn placed(dev: &Dev, locale: &str) -> super::http::Reply {
    dev.send(
        "POST",
        "/placed",
        &[],
        &format!(r#"{{"ordered_at":"2026-09-27T13:30:05Z","locale":"{locale}"}}"#),
    )
}

#[test]
fn a_locale_in_a_request_is_a_supported_one() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);
    // The long date, in each locale's words and digits. Seven days or more
    // from now, the relative text is the same long date.
    for (locale, shown) in [
        ("en-GB", "27 September 2026"),
        ("fr-FR", "27 septembre 2026"),
        ("ar-EG", "٢٧ سبتمبر ٢٠٢٦"),
    ] {
        let reply = placed(&dev, locale);
        assert_eq!(reply.status, 200, "{locale}: {reply:?}");
        assert_eq!(
            reply.json(),
            serde_json::json!({ "placed": shown, "relative": shown }),
            "{locale}"
        );
    }
    // A real locale the application does not support, and one that is none.
    for locale in ["de-DE", "klingon"] {
        let refused = placed(&dev, locale);
        assert_eq!(refused.status, 422, "{locale}: {refused:?}");
        let problem = &refused.json()["error"]["details"]["problems"][0];
        assert_eq!(problem["field"], "locale", "{refused:?}");
        assert_eq!(problem["rule"], "variant", "{refused:?}");
    }
}

#[test]
fn the_openapi_document_lists_the_supported_locales_only() {
    let (_dir, root) = project();
    let text = std::fs::read_to_string(Path::new(&root).join("build/openapi/main.json"))
        .expect("written by api update");
    let document: serde_json::Value = serde_json::from_str(&text).expect("json");
    let schemas = &document["components"]["schemas"];
    let locale = &schemas["PlaceRequest"]["properties"]["locale"];
    assert_eq!(
        locale["enum"],
        serde_json::json!(["ar-EG", "en-GB", "fr-FR"]),
        "{locale}"
    );
}
