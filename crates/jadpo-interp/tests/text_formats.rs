//! Area 19, "Text formats" (chunks 5.5o and 5.5o2), run: the page's
//! examples (`project=text-formats`, `project=rich-text` and
//! `project=localised-text`), every way text becomes `Html` (its
//! construction, `text.parse`, a JSON document and `markup.to_html`)
//! keeping only what the sanitiser keeps, whatever the attack, with each
//! value's own check proved as it is built; a `RichText` written as HTML
//! the sanitiser keeps unchanged; and `text.pick` walking area 10's
//! fallback chain to the application's default locale.

use crate::support;

use jadpo_interp::Value;
use support::{failure, main, text, value};

const SOURCE: &str = "type FormText = Text {}

type PreviewText = Text {}

/// A post's body as its author wrote it, made into the page's HTML.
function post_body(post_markdown: Markdown) -> Html {
    return markup.to_html(post_markdown)
}

/// A profile's about section, pasted from a rich text editor: whatever it
/// holds, only the safe part is kept.
function about_section(pasted_about: FormText) -> Html {
    return Html(pasted_about)
}

/// The words a notification shows from a post, without its markup.
function preview(post_page: Html) -> PreviewText {
    return PreviewText(markup.to_plain_text(post_page))
}
";

#[test]
fn the_page_s_example_keeps_only_what_is_safe() {
    let program = main(SOURCE).with_proof_checks();
    assert_eq!(
        value(&program, "post_body", vec![text("Soup of the **day**")]),
        text("<p>Soup of the <strong>day</strong></p>\n")
    );
    assert_eq!(
        value(
            &program,
            "post_body",
            vec![text(
                "[menu](https://example.com/menu) or [prize](javascript:steal())"
            )]
        ),
        text(
            "<p><a href=\"https://example.com/menu\" rel=\"noopener noreferrer nofollow\">menu</a> or <a rel=\"noopener noreferrer nofollow\">prize</a></p>\n"
        )
    );
    assert_eq!(
        value(
            &program,
            "about_section",
            vec![text(
                "<p onclick=\"steal()\">Chef</p><script>steal()</script>"
            )]
        ),
        text("<p>Chef</p>")
    );
    let about = value(
        &program,
        "about_section",
        vec![text("<h2>Menu</h2><ul><li>Soup</li><li>Bread</li></ul>")],
    );
    assert_eq!(
        value(&program, "preview", vec![about]),
        text("Menu\nSoup\nBread")
    );
}

const PATHS: &str = "type FormText = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

type TooLong = Failure {
    kind: FailureKind.invalid_value
}

type Bio = Html {
    max_length: 40
}

type ProfileForm = Input {
    about: Html
}

function built(raw: FormText) -> Html {
    return Html(raw)
}

function read(raw: FormText) -> Html {
    return text.parse(raw, as: Html)
}

function from_json(body: FormText) fails Unreadable -> Html {
    var form: ProfileForm = attempt json.read(body, as: ProfileForm) {
        malformed: Unreadable
        mismatch: Unreadable
    }
    return form.about
}

function bio(raw: FormText) fails TooLong -> Bio {
    return attempt Bio(raw) {
        max_length: TooLong
    }
}

function rendered(raw: Markdown) -> Html {
    return markup.to_html(raw)
}
";

/// What each attack is kept as, by every path text takes into `Html`.
const ATTACKS: &[(&str, &str)] = &[
    ("<script>alert(1)</script>", ""),
    ("<img src=x onerror=alert(1)>", ""),
    (
        "<a href=\"jav&#x09;ascript:alert(1)\">x</a>",
        "<a rel=\"noopener noreferrer nofollow\">x</a>",
    ),
    (
        "<a href=\"//evil.example/\">x</a>",
        "<a rel=\"noopener noreferrer nofollow\">x</a>",
    ),
    ("<svg><a xlink:href=\"javascript:alert(1)\">x</a></svg>", ""),
    (
        "<base href=\"https://evil.example/\"><meta http-equiv=\"refresh\" content=\"0;url=javascript:alert(1)\">",
        "",
    ),
    (
        "<p style=\"background:url(javascript:alert(1))\">x</p>",
        "<p>x</p>",
    ),
    // `noscript` goes with its content, leaving only stray text.
    (
        "<noscript><p title=\"</noscript><img src=x onerror=alert(1)>\">",
        "\"&gt;",
    ),
];

#[test]
fn every_path_into_html_runs_the_sanitiser() {
    let program = main(PATHS).with_proof_checks();
    for (attack, kept) in ATTACKS {
        assert_eq!(
            value(&program, "built", vec![text(attack)]),
            text(kept),
            "{attack}"
        );
        assert_eq!(
            value(&program, "read", vec![text(attack)]),
            text(kept),
            "{attack}"
        );
        let quoted = attack.replace('\\', "\\\\").replace('"', "\\\"");
        let body = format!("{{\"about\": \"{quoted}\"}}");
        assert_eq!(
            value(&program, "from_json", vec![text(&body)]),
            text(kept),
            "{attack} in JSON"
        );
    }
}

#[test]
fn markdown_s_raw_html_is_never_passed_through_unsanitised() {
    let program = main(PATHS).with_proof_checks();
    let cases: &[(&str, &str)] = &[
        ("Hi <script>alert(1)</script>", "<p>Hi </p>\n"),
        (
            "<div onmouseover=\"alert(1)\">\n\n**bold**\n\n</div>",
            "\n<p><strong>bold</strong></p>\n",
        ),
        (
            "[x](javascript:alert(1))",
            "<p><a rel=\"noopener noreferrer nofollow\">x</a></p>\n",
        ),
        ("![cat](https://example.com/cat.png)", "<p>cat</p>\n"),
        (
            "<a href=\"https://example.com/\" onclick=\"x\">site</a>",
            "<p><a href=\"https://example.com/\" rel=\"noopener noreferrer nofollow\">site</a></p>\n",
        ),
    ];
    for (markdown, kept) in cases {
        assert_eq!(
            value(&program, "rendered", vec![text(markdown)]),
            text(kept),
            "{markdown}"
        );
    }
}

#[test]
fn a_type_built_on_html_counts_what_the_sanitiser_keeps() {
    let program = main(PATHS).with_proof_checks();
    // 61 characters as written, 14 once its script is gone.
    assert_eq!(
        value(
            &program,
            "bio",
            vec![text("<p>Chef</p><script>alert('a long script')</script>")]
        ),
        text("<p>Chef</p>")
    );
    assert_eq!(
        failure(
            &program,
            "bio",
            vec![text("<p>A chef who writes far too much about soup</p>")]
        )
        .name,
        "TooLong"
    );
    assert!(matches!(
        value(&program, "built", vec![text("<b>ok</b>")]),
        Value::Text(_)
    ));
}

const RICH: &str = "type EditorJson = Text {}

type MethodUnreadable = Failure {
    kind: FailureKind.invalid_value
}

/// A recipe's method as the editor sent it: its blocks and marks, with
/// links only to `https:` and `mailto:` addresses.
function method_from(sent: EditorJson) fails MethodUnreadable -> RichText {
    return attempt json.read(sent, as: RichText) {
        malformed: MethodUnreadable
        mismatch: MethodUnreadable
    }
}

/// The method on the recipe's page.
function method_page(recipe_method: RichText) -> Html {
    return markup.rich_to_html(recipe_method)
}
";

#[test]
fn the_page_s_method_becomes_safe_html() {
    let program = main(RICH).with_proof_checks();
    let sent = "[{\"tag\":\"heading\",\"level\":2,\"spans\":[{\"text\":\"Method\",\"marks\":[]}]},{\"tag\":\"numbered_item\",\"level\":1,\"spans\":[{\"text\":\"Soak the oats\",\"marks\":[]}]},{\"tag\":\"numbered_item\",\"level\":1,\"spans\":[{\"text\":\"Bake for \",\"marks\":[]},{\"text\":\"40 minutes\",\"marks\":[\"strong\"]}]},{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"Flour from \",\"marks\":[]},{\"text\":\"our mill\",\"marks\":[\"emphasis\"],\"link\":\"https://example.com/mill\"}]}]";
    let method = value(&program, "method_from", vec![text(sent)]);
    let page = value(&program, "method_page", vec![method]);
    assert_eq!(
        page,
        text(
            "<h2>Method</h2><ol><li>Soak the oats</li><li>Bake for <strong>40 minutes</strong></li></ol><p>Flour from <a href=\"https://example.com/mill\" rel=\"noopener noreferrer nofollow\"><em>our mill</em></a></p>"
        )
    );
    let Value::Text(html) = &page else {
        panic!("HTML is text");
    };
    assert_eq!(
        jadpo_std::standard::markup::sanitise(html),
        html.to_string(),
        "the sanitiser keeps it unchanged"
    );
    assert_eq!(
        failure(
            &program,
            "method_from",
            vec![text(
                "[{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"prize\",\"marks\":[],\"link\":\"javascript:steal()\"}]}]"
            )]
        )
        .name,
        "MethodUnreadable"
    );
}

/// Documents an editor might send, and the HTML each is written as.
const DOCUMENTS: &[(&str, &str)] = &[
    (
        "[{\"tag\":\"quote\",\"spans\":[{\"text\":\"<script>alert(1)</script> & co\",\"marks\":[\"code\",\"underline\",\"strike_through\"]}]}]",
        "<blockquote><code><u><s>&lt;script&gt;alert(1)&lt;/script&gt; &amp; co</s></u></code></blockquote>",
    ),
    (
        "[{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"one\\ntwo\",\"marks\":[\"strike_through\",\"strong\"]}]}]",
        "<p><strong><s>one<br>two</s></strong></p>",
    ),
    (
        "[{\"tag\":\"bullet_item\",\"level\":1,\"spans\":[{\"text\":\"a\",\"marks\":[]}]},{\"tag\":\"numbered_item\",\"level\":3,\"spans\":[{\"text\":\"b\",\"marks\":[]}]},{\"tag\":\"bullet_item\",\"level\":1,\"spans\":[{\"text\":\"c\",\"marks\":[]}]},{\"tag\":\"numbered_item\",\"level\":1,\"spans\":[{\"text\":\"d\",\"marks\":[]}]}]",
        "<ul><li>a<ol><li>b</li></ol></li><li>c</li></ul><ol><li>d</li></ol>",
    ),
    (
        "[{\"tag\":\"heading\",\"level\":6,\"spans\":[{\"text\":\"\\\"quoted\\\" \\u00a0\",\"marks\":[],\"link\":\"mailto:a@example.com?subject=a&b=c\"}]}]",
        "<h6><a href=\"mailto:a@example.com?subject=a&amp;b=c\" rel=\"noopener noreferrer nofollow\">\"quoted\" &nbsp;</a></h6>",
    ),
];

#[test]
fn a_document_is_written_as_html_the_sanitiser_keeps_unchanged() {
    let program = main(RICH).with_proof_checks();
    for (sent, written) in DOCUMENTS {
        let method = value(&program, "method_from", vec![text(sent)]);
        let page = value(&program, "method_page", vec![method]);
        assert_eq!(page, text(written), "{sent}");
        assert_eq!(
            jadpo_std::standard::markup::sanitise(written),
            *written,
            "{written} is a fixed point"
        );
    }
    for refused in [
        "[{\"tag\":\"heading\",\"level\":7,\"spans\":[]}]",
        "[{\"tag\":\"bullet_item\",\"level\":10,\"spans\":[]}]",
        "[{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"\",\"marks\":[]}]}]",
        "[{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"x\",\"marks\":[\"blink\"]}]}]",
        "[{\"tag\":\"paragraph\",\"spans\":[{\"text\":\"x\",\"marks\":[],\"link\":\"http://example.com/\"}]}]",
        "[{\"tag\":\"image\",\"spans\":[]}]",
        "{\"tag\":\"paragraph\",\"spans\":[]}",
    ] {
        assert_eq!(
            failure(&program, "method_from", vec![text(refused)]).name,
            "MethodUnreadable",
            "{refused}"
        );
    }
}

const SHOP_APP: &str = "application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.en_us, Locale.fr_fr, Locale.fr_ca, Locale.de_de]
}
";

const SHELF_NAME: &str = "type ShelfName = Text {}

/// A loaf's name in the shopper's language: their locale's entry, else
/// one in their language, else the shop's default locale's.
action shelf_name(name: LocalisedText, shopper: Locale) -> ShelfName {
    return ShelfName(text.pick(name, locale: shopper))
}
";

const NAME_UNREADABLE: &str = "type NameUnreadable = Failure {
    kind: FailureKind.invalid_value
}
";

const NAME_FROM: &str = "use catalogue/failures { NameUnreadable }

type CatalogueJson = Text {}

action name_from(sent: CatalogueJson) fails NameUnreadable -> LocalisedText {
    return attempt json.read(sent, as: LocalisedText) {
        malformed: NameUnreadable
        mismatch: NameUnreadable
    }
}
";

const NAME_SHOWN: &str = "type ShownName = PresentationText {}

action name_shown(name: LocalisedText, shopper: Locale) -> ShownName {
    return ShownName(text.format(name, locale: shopper))
}
";

/// The page's shop, with its locales: a project, so `text.pick` has the
/// application's default locale to fall back to.
fn shop() -> jadpo_interp::Program {
    let db = jadpo_db::Database::default();
    let inputs: Vec<jadpo_db::File> = [
        ("app.jadpo", SHOP_APP),
        ("domains/catalogue/actions/shelf_name.jadpo", SHELF_NAME),
        (
            "domains/catalogue/failures/name_unreadable.jadpo",
            NAME_UNREADABLE,
        ),
        ("domains/catalogue/actions/name_from.jadpo", NAME_FROM),
        ("domains/catalogue/actions/name_shown.jadpo", NAME_SHOWN),
    ]
    .iter()
    .map(|(path, text)| jadpo_db::File::new(&db, path.to_string(), text.to_string()))
    .collect();
    let project = jadpo_db::Project::new(
        &db,
        inputs,
        Vec::new(),
        jadpo_db::Layout::Project,
        Default::default(),
        Vec::new(),
    );
    match jadpo_interp::Program::load(&db, project) {
        Ok(program) => program.with_proof_checks(),
        Err(errors) => panic!(
            "the example doesn't check: {:#?}",
            errors
                .iter()
                .map(|d| format!("{}: {}", d.rule.id, d.message))
                .collect::<Vec<_>>()
        ),
    }
}

fn locale(variant: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, variant).expect("a locale");
    Value::variant(jadpo_std::LOCALE, ordinal, variant, Vec::new())
}

#[test]
fn pick_walks_the_fallback_chain_to_the_default_locale() {
    let program = shop();
    let name = |sent: &str| value(&program, "name_from", vec![text(sent)]);
    let shelf = |name: &Value, shopper: &str| {
        value(&program, "shelf_name", vec![name.clone(), locale(shopper)])
    };
    // The page's test.
    let rye = name("{\"fr-FR\":\"Pain de seigle\",\"en-GB\":\"Rye loaf\"}");
    assert_eq!(
        shelf(&rye, "fr_fr"),
        text("Pain de seigle"),
        "its own entry"
    );
    assert_eq!(
        shelf(&rye, "fr_ca"),
        text("Pain de seigle"),
        "its language's"
    );
    assert_eq!(
        shelf(&rye, "de_de"),
        text("Rye loaf"),
        "the default locale's"
    );
    let roggen = name("{\"de-DE\":\"Roggenbrot\"}");
    assert_eq!(shelf(&roggen, "fr_fr"), text("Roggenbrot"), "the only one");
    // The default locale's before the first in the canonical order.
    let both = name("{\"de-DE\":\"Roggenbrot\",\"en-GB\":\"Rye loaf\"}");
    assert_eq!(shelf(&both, "fr_ca"), text("Rye loaf"));
    // Its own entry before its language's; another of its language before
    // the default locale's.
    let french = name("{\"fr-CA\":\"Pain de seigle (Québec)\",\"fr-FR\":\"Pain de seigle\"}");
    assert_eq!(
        shelf(&french, "fr_ca"),
        text("Pain de seigle (Québec)"),
        "its own entry first"
    );
    let canadian = name("{\"fr-CA\":\"Pain de seigle (Québec)\",\"en-GB\":\"Rye loaf\"}");
    assert_eq!(shelf(&canadian, "fr_fr"), text("Pain de seigle (Québec)"));
    assert_eq!(shelf(&name("{}"), "en_gb"), text(""), "nothing to show");
    // `text.format` shows the same entry.
    assert_eq!(
        value(&program, "name_shown", vec![rye.clone(), locale("de_de")]),
        text("Rye loaf")
    );
    assert_eq!(
        value(&program, "name_shown", vec![rye, locale("fr_ca")]),
        text("Pain de seigle")
    );
}

#[test]
fn only_a_supported_locale_is_read_as_an_entry() {
    let program = shop();
    for refused in [
        "{\"es-ES\":\"Pan de centeno\"}",
        "{\"english\":\"Rye loaf\"}",
        "{\"en-GB\":42}",
        "[{\"locale\":\"en-GB\",\"text\":\"Rye loaf\"}]",
    ] {
        assert_eq!(
            failure(&program, "name_from", vec![text(refused)]).name,
            "NameUnreadable",
            "{refused}"
        );
    }
}
