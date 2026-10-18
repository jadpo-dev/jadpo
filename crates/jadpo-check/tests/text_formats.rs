//! Area 19, "Text formats" (chunks 5.5o and 5.5o2), in the checker: every
//! way text becomes `Html` is the sanitiser and never fails, so it needs no
//! `attempt`; a plain `Text` is never an `Html` without it; an `Html`
//! literal must already be what the sanitiser keeps, or it is
//! `type.invalid_literal` naming what would be kept; `markup`'s operations
//! are typed by their stubs; a `RichText` is built as a named list of its
//! blocks, its rules and its links proved where they are written; a
//! `LocalisedText` is built as a named map; `text.pick`'s locale is checked
//! as formatting's is; and an application's own type of any of their
//! names is a clash.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type Wire = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn text_becomes_html_only_through_the_sanitiser_which_never_fails() {
    assert_eq!(
        rules("function made(raw: Wire) -> Html {\n    return Html(raw)\n}\n"),
        NONE,
        "building one is the sanitiser, which keeps the safe part of anything"
    );
    assert_eq!(
        rules("function read(raw: Wire) -> Html {\n    return text.parse(raw, as: Html)\n}\n"),
        NONE,
        "reading one is the sanitiser too"
    );
    assert_eq!(
        rules(
            "function read(raw: Wire) fails Unreadable -> Html {\n    return attempt text.parse(raw, as: Html) {\n        malformed: Unreadable\n    }\n}\n"
        ),
        ["failure.attempt_infallible"],
        "nothing is malformed"
    );
    assert_eq!(
        rules("function made(raw: Wire) -> Html {\n    var safe: Html = raw\n    return safe\n}\n"),
        ["type.mismatch"],
        "a plain text is never an `Html` without the sanitiser"
    );
    assert_eq!(
        rules("function made(raw: Wire) -> Html {\n    return raw\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function made(raw: Html, extra: Wire) -> Html {\n    return text.replace(raw, \"x\", extra)\n}\n"
        ),
        ["type.mismatch"],
        "a text operation on `Html` gives plain text, which must be made safe again"
    );
    assert_eq!(
        rules(
            "function made(raw: Html, extra: Wire) -> Html {\n    return Html(text.replace(raw, \"x\", extra))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function shown(page: Html) -> Wire {\n    return Wire(page)\n}\n"),
        NONE,
        "an `Html` is text, so it widens to a type built on `Text`"
    );
}

#[test]
fn a_type_built_on_html_keeps_its_own_rules() {
    let bio = "type Bio = Html {\n    max_length: 200\n}\n\n";
    assert_eq!(
        rules(&format!(
            "{bio}function made(raw: Wire) fails Unreadable -> Bio {{\n    return attempt Bio(raw) {{\n        max_length: Unreadable\n    }}\n}}\n"
        )),
        NONE,
        "only its own rule, counted on what the sanitiser keeps, can break"
    );
    assert_eq!(
        rules(&format!(
            "{bio}function made(raw: Wire) -> Bio {{\n    return Bio(raw)\n}}\n"
        )),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules(&format!(
            "{bio}function made(raw: Wire) fails Unreadable -> Bio {{\n    return attempt Bio(raw) {{\n        format: Unreadable\n        max_length: Unreadable\n    }}\n}}\n"
        )),
        ["failure.rule_cannot_break"],
        "its sanitiser never breaks `format`"
    );
}

#[test]
fn an_html_literal_must_already_be_safe() {
    let literal = |text: &str| {
        rules(&format!(
            "function made() -> Html {{\n    return \"{text}\"\n}}\n"
        ))
    };
    for safe in [
        "<p>Hello, <strong>world</strong>.</p>",
        "Tom &amp; Jerry",
        "<a href=\\\"https://example.com/\\\" rel=\\\"noopener noreferrer nofollow\\\">x</a>",
        "",
    ] {
        assert_eq!(literal(safe), NONE, "`{safe}`");
    }
    for changed in [
        "<script>alert(1)</script>",
        "<p onclick=\\\"steal()\\\">x</p>",
        "<a href=\\\"javascript:alert(1)\\\">x</a>",
        "<a href=\\\"https://example.com/\\\">x</a>",
        "<img src=\\\"https://example.com/cat.png\\\">",
        "Tom & Jerry",
        "<p>unclosed",
    ] {
        assert_eq!(literal(changed), ["type.invalid_literal"], "`{changed}`");
    }
    assert_eq!(
        rules("function made() -> Html {\n    return Html(\"<b onclick=\\\"x\\\">hi</b>\")\n}\n"),
        ["type.invalid_literal"],
        "built from a literal too"
    );
}

#[test]
fn the_invalid_literal_names_what_the_sanitiser_keeps() {
    let db = jadpo_db::Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            "function made() -> Html {\n    return \"<p onclick=\\\"steal()\\\">Hi</p><script>x</script>\"\n}\n",
        )],
    );
    let found = jadpo_check::check(&db, project);
    let [problem] = found.as_slice() else {
        panic!("one problem: {found:?}");
    };
    let reason = problem.reason.clone().unwrap_or_default();
    assert!(
        reason.contains("the sanitiser keeps `<p>Hi</p>`"),
        "{reason}"
    );
    let next = format!("{:?}", problem.next);
    assert!(
        next.contains("Write it as the sanitiser keeps it: `<p>Hi</p>`"),
        "{next}"
    );
}

#[test]
fn markup_operations_take_markdown_and_html() {
    assert_eq!(
        rules(
            "function page(written: Markdown) -> Html {\n    return markup.to_html(written)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function page() -> Html {\n    return markup.to_html(\"# Menu\")\n}\n"),
        NONE,
        "a Markdown literal is checked where it is taken"
    );
    assert_eq!(
        rules(
            "function words(page: Html) -> Wire {\n    return Wire(markup.to_plain_text(page))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function page(raw: Wire) -> Html {\n    return markup.to_html(raw)\n}\n"),
        ["type.mismatch"],
        "text is read as Markdown first"
    );
    assert_eq!(
        rules(
            "function words(raw: Wire) -> Wire {\n    return Wire(markup.to_plain_text(raw))\n}\n"
        ),
        ["type.mismatch"],
        "and as `Html`, through the sanitiser"
    );
    assert_eq!(
        rules(
            "function read(raw: Wire) fails Unreadable -> Markdown {\n    return attempt text.parse(raw, as: Markdown) {\n        malformed: Unreadable\n    }\n}\n"
        ),
        NONE,
        "Markdown with a control character is malformed"
    );
}

#[test]
fn an_application_s_own_html_is_a_clash() {
    assert_eq!(rules("type Html = Text {}\n"), ["type.standard_clash"]);
    assert_eq!(rules("type Markdown = Text {}\n"), ["type.standard_clash"]);
    assert_eq!(rules("type RichText = Text {}\n"), ["type.standard_clash"]);
    assert_eq!(
        rules("type LocalisedText = Text {}\n"),
        ["type.standard_clash"]
    );
}

/// A one-block document holding one span, with what is written for its
/// heading's level, its text and its link.
fn document(level: &str, text: &str, link: &str) -> String {
    format!(
        "function note() -> RichText {{\n    return RichText([\n        RichTextBlock.heading {{\n            level: {level}\n            spans: [\n                RichTextSpan {{\n                    text: {text}\n                    marks: [RichTextMark.strong, RichTextMark.emphasis]\n                    link: {link}\n                }}\n            ]\n        }}\n    ])\n}}\n"
    )
}

#[test]
fn a_document_is_its_blocks_with_their_rules_proved_where_written() {
    assert_eq!(
        rules(&document("2", "\"Menu\"", "\"https://example.com/menu\"")),
        NONE
    );
    assert_eq!(
        rules(&document(
            "1",
            "\"Write to us\"",
            "\"mailto:hello@example.com\""
        )),
        NONE
    );
    assert_eq!(rules(&document("6", "\"Menu\"", "none")), NONE);
    assert_eq!(
        rules(&document("7", "\"Menu\"", "none")),
        ["type.invalid_literal"],
        "a heading is level 1 to 6"
    );
    assert_eq!(
        rules(&document("2", "\"\"", "none")),
        ["type.invalid_literal"],
        "a span's text is never empty"
    );
    for link in [
        "javascript:alert(1)",
        "http://example.com/",
        "/menu",
        "//example.com/",
        "https://192.168.0.1/",
    ] {
        assert_eq!(
            rules(&document("2", "\"Menu\"", &format!("\"{link}\""))),
            ["type.invalid_literal"],
            "`{link}` isn't a link a document holds"
        );
    }
    assert_eq!(
        rules(
            "function blocks(written: RichText) -> RichText {\n    return RichText(written)\n}\n"
        ),
        NONE,
        "built from a list of its blocks"
    );
    assert_eq!(
        rules(
            "function page(written: RichText) -> Html {\n    return markup.rich_to_html(written)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function page(written: Markdown) -> Html {\n    return markup.rich_to_html(written)\n}\n"
        ),
        ["type.mismatch"],
        "only a `RichText` is written as HTML by it"
    );
    assert_eq!(
        rules(
            "function shown(written: RichText, at: Locale) -> PresentationText {\n    return text.format(written, locale: at)\n}\n"
        ),
        NONE,
        "shown as the text people read in it"
    );
    assert_eq!(
        rules(
            "function sent(written: RichText) -> Wire {\n    return Wire(text.from(written))\n}\n"
        ),
        ["type.mismatch"],
        "no single text form"
    );
}

#[test]
fn a_bad_link_is_said_in_words() {
    let db = jadpo_db::Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            &document("2", "\"Menu\"", "\"javascript:steal()\""),
        )],
    );
    let found = jadpo_check::check(&db, project);
    let [problem] = found.as_slice() else {
        panic!("one problem: {found:?}");
    };
    assert!(
        problem
            .message
            .contains("isn't a link a `RichText` can hold"),
        "{}",
        problem.message
    );
    let reason = problem.reason.clone().unwrap_or_default();
    assert!(!reason.contains("pattern"), "{reason}");
    assert!(reason.contains("`https:`"), "{reason}");
}

#[test]
fn localised_text_is_a_map_picked_in_a_supported_locale() {
    let pick = "function shelf(name: LocalisedText, shopper: Locale) -> Wire {\n    return Wire(text.pick(name, locale: shopper))\n}\n";
    assert_eq!(rules(pick), NONE);
    assert_eq!(
        rules(
            "function name() -> LocalisedText {\n    return LocalisedText(map.with([], Locale.fr_fr, \"Pain de seigle\"))\n}\n"
        ),
        NONE,
        "built from a map of locales to text"
    );
    assert_eq!(
        rules(
            "function shelf(name: Html, shopper: Locale) -> Wire {\n    return Wire(text.pick(name, locale: shopper))\n}\n"
        ),
        ["type.mismatch"],
        "only a `LocalisedText` is picked from"
    );
    let app = "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n}\n";
    let action = |shopper: &str| {
        format!(
            "type ShelfName = Text {{}}\n\naction shelf_name(name: LocalisedText) -> ShelfName {{\n    return ShelfName(text.pick(name, locale: {shopper}))\n}}\n"
        )
    };
    let project = |shopper: &str, app: &str| {
        support::rules(
            Layout::Project,
            &[
                ("app.jadpo", app),
                (
                    "domains/catalogue/actions/shelf_name.jadpo",
                    &action(shopper),
                ),
            ],
        )
    };
    assert_eq!(project("Locale.fr_fr", app), NONE);
    assert_eq!(
        project("Locale.es_es", app),
        ["type.locale_unsupported"],
        "a locale written out is one the application supports"
    );
    assert_eq!(
        project("Locale.fr_fr", "application {}\n"),
        ["type.temporal_locales_required"],
        "the application declares its locales"
    );
}
