//! Area 19, "Text formats" (chunk 5.5o), run: the page's example
//! (`project=text-formats`), and every way text becomes `Html` (its
//! construction, `text.parse`, a JSON document and `markup.to_html`)
//! keeping only what the sanitiser keeps, whatever the attack, with each
//! value's own check proved as it is built.

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
