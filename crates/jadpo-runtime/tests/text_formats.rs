//! Area 19, "Text formats" (chunks 5.5o and 5.5o2), in the runtime: the
//! values the page's tests (`project=text-formats`, `project=rich-text`
//! and `project=localised-text`) and the interpreter's `text_formats`
//! tests assert, given to `markup`'s operations and `text.pick` as
//! compiled code calls them (`jadpo_value::run`); an `Html` value built,
//! read from its text and read from a request's JSON through the
//! sanitiser, as the plan's validator and codec hold it; and a type built
//! on `Html` counting what the sanitiser keeps. Their text, JSON and
//! column are the one harness's (`jadpo-interp`'s `standard_types`).

use jadpo_std::http::{Codec, CodecId};
use jadpo_std::types::ValidatorId;
use jadpo_std::{Builtin, Op, RuleKey, Rules};
use jadpo_value::json::{Codecs, Path};
use jadpo_value::{Validator, Value, run, standard};

fn call(op: Op, arg: &str) -> Value {
    run(op, &[Value::text(arg)], None, &mut |_| None)
        .unwrap_or_else(|fault| panic!("{op:?}({arg}): {fault:?}"))
}

/// `Html`'s chain as the plan holds it: its own check, and the rules of a
/// type built on it.
fn html_validator(own: Option<Rules>) -> Validator {
    let mut chain: Vec<Rules> = own.into_iter().collect();
    chain.extend(jadpo_std::standard::text_rules("Html"));
    Validator::new(Some(Builtin::Text), chain)
}

#[test]
fn markup_runs_as_compiled_code_calls_it() {
    // The page's test.
    assert_eq!(
        call(Op::MarkupToHtml, "Soup of the **day**"),
        Value::text("<p>Soup of the <strong>day</strong></p>\n")
    );
    assert_eq!(
        call(
            Op::MarkupToHtml,
            "[menu](https://example.com/menu) or [prize](javascript:steal())"
        ),
        Value::text(
            "<p><a href=\"https://example.com/menu\" rel=\"noopener noreferrer nofollow\">menu</a> or <a rel=\"noopener noreferrer nofollow\">prize</a></p>\n"
        )
    );
    assert_eq!(
        call(
            Op::MarkupToPlainText,
            "<h2>Menu</h2><ul><li>Soup</li><li>Bread</li></ul>"
        ),
        Value::text("Menu\nSoup\nBread")
    );
    // Raw HTML in Markdown is made safe with the rest.
    assert_eq!(
        call(Op::MarkupToHtml, "Hi <img src=x onerror=alert(1)>"),
        Value::text("<p>Hi </p>\n")
    );
}

#[test]
fn every_html_value_is_made_safe_as_it_is_built() {
    let validator = html_validator(None);
    let built = validator
        .store(Value::text(
            "<p onclick=\"steal()\">Chef</p><script>steal()</script>",
        ))
        .expect("stored");
    assert_eq!(built, Value::text("<p>Chef</p>"));
    assert!(validator.check(&built).is_ok(), "its own check passes");
    // An attacker's text is refused by nothing; it is only ever kept safe.
    assert!(
        validator
            .check(&Value::text("<script>steal()</script>"))
            .is_err(),
        "text that isn't safe as written isn't an `Html` until it is built"
    );
    // `text.parse` reads any text, made safe.
    assert_eq!(
        standard::parse("<a href=\"javascript:x\">x</a>", "Html"),
        Ok(Value::text("<a rel=\"noopener noreferrer nofollow\">x</a>"))
    );
    assert_eq!(
        standard::parse("bell\u{7}", "Markdown"),
        Err(jadpo_value::Fault::Broke(RuleKey::Malformed))
    );
}

#[test]
fn a_request_s_html_is_made_safe_as_it_is_read() {
    let codecs = [Codec::Scalar {
        builtin: Builtin::Text,
        validator: ValidatorId(0),
        int_as_number: false,
    }];
    let validators = [html_validator(None)];
    let table = Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &[],
    };
    let mut problems = Vec::new();
    let json = serde_json::Value::String(
        "<p>Hi</p><iframe src=\"javascript:alert(1)\"></iframe><a href=\"//evil.example/\">x</a>"
            .to_string(),
    );
    assert_eq!(
        table.read(CodecId(0), &json, &Path::default(), &mut problems),
        Some(Value::text(
            "<p>Hi</p><a rel=\"noopener noreferrer nofollow\">x</a>"
        )),
        "{problems:?}"
    );
}

#[test]
fn a_type_built_on_html_counts_what_the_sanitiser_keeps() {
    let bio = html_validator(Some(Rules {
        max_length: Some(20),
        ..Rules::default()
    }));
    let kept = bio
        .store(Value::text(
            "<p>Chef</p><script>alert('a long script')</script>",
        ))
        .expect("stored");
    assert_eq!(kept, Value::text("<p>Chef</p>"));
    assert!(bio.check(&kept).is_ok());
    let long = bio
        .store(Value::text("<p>A chef who writes about soup</p>"))
        .expect("stored");
    assert_eq!(
        bio.check(&long).map_err(|invalid| invalid.key()),
        Err(RuleKey::MaxLength)
    );
}

/// A span: its text, its marks by name, and its link.
fn span(text: &str, marks: &[&str], link: Option<&str>) -> Value {
    let marks = marks
        .iter()
        .map(|mark| {
            let ordinal = jadpo_std::variant_ordinal("RichTextMark", mark).expect("a mark");
            Value::variant("RichTextMark", ordinal, mark, Vec::new())
        })
        .collect();
    Value::record(
        "RichTextSpan",
        vec![
            ("text".into(), Value::text(text)),
            ("marks".into(), Value::list(marks)),
            ("link".into(), link.map_or(Value::None, Value::text)),
        ],
    )
}

/// A block: its variant, its level where it has one, and its spans.
fn block(variant: &str, level: Option<i64>, spans: Vec<Value>) -> Value {
    let ordinal = jadpo_std::variant_ordinal("RichTextBlock", variant).expect("a block");
    let mut fields = Vec::new();
    if let Some(level) = level {
        fields.push(("level".into(), Value::Int(level)));
    }
    fields.push(("spans".into(), Value::list(spans)));
    Value::variant("RichTextBlock", ordinal, variant, fields)
}

fn rich_to_html(blocks: Vec<Value>) -> Value {
    run(
        Op::MarkupRichToHtml,
        &[Value::list(blocks)],
        None,
        &mut |_| None,
    )
    .expect("written as HTML")
}

#[test]
fn rich_text_is_written_as_compiled_code_calls_it() {
    // The page's test.
    let method = vec![
        block("heading", Some(2), vec![span("Method", &[], None)]),
        block(
            "numbered_item",
            Some(1),
            vec![span("Soak the oats", &[], None)],
        ),
        block(
            "numbered_item",
            Some(1),
            vec![
                span("Bake for ", &[], None),
                span("40 minutes", &["strong"], None),
            ],
        ),
        block(
            "paragraph",
            None,
            vec![
                span("Flour from ", &[], None),
                span("our mill", &["emphasis"], Some("https://example.com/mill")),
            ],
        ),
    ];
    let page = rich_to_html(method);
    let written = "<h2>Method</h2><ol><li>Soak the oats</li><li>Bake for <strong>40 minutes</strong></li></ol><p>Flour from <a href=\"https://example.com/mill\" rel=\"noopener noreferrer nofollow\"><em>our mill</em></a></p>";
    assert_eq!(page, Value::text(written));
    assert_eq!(jadpo_std::standard::markup::sanitise(written), written);
    assert_eq!(
        rich_to_html(vec![block(
            "paragraph",
            None,
            vec![span("Write to us", &[], Some("mailto:hello@example.com"))]
        )]),
        Value::text(
            "<p><a href=\"mailto:hello@example.com\" rel=\"noopener noreferrer nofollow\">Write to us</a></p>"
        )
    );
    // The interpreter's documents.
    assert_eq!(
        rich_to_html(vec![block(
            "quote",
            None,
            vec![span(
                "<script>alert(1)</script> & co",
                &["code", "underline", "strike_through"],
                None
            )]
        )]),
        Value::text(
            "<blockquote><code><u><s>&lt;script&gt;alert(1)&lt;/script&gt; &amp; co</s></u></code></blockquote>"
        )
    );
    assert_eq!(
        rich_to_html(vec![
            block("bullet_item", Some(1), vec![span("a", &[], None)]),
            block("numbered_item", Some(3), vec![span("b", &[], None)]),
            block("bullet_item", Some(1), vec![span("c", &[], None)]),
            block("numbered_item", Some(1), vec![span("d", &[], None)]),
        ]),
        Value::text("<ul><li>a<ol><li>b</li></ol></li><li>c</li></ul><ol><li>d</li></ol>")
    );
    assert_eq!(rich_to_html(Vec::new()), Value::text(""));
}

fn locale(variant: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, variant).expect("a locale");
    Value::variant(jadpo_std::LOCALE, ordinal, variant, Vec::new())
}

fn localised(entries: &[(&str, &str)]) -> Value {
    Value::Map(std::rc::Rc::new(
        entries
            .iter()
            .map(|(at, text)| (locale(at), Value::text(text)))
            .collect(),
    ))
}

#[test]
fn pick_runs_as_compiled_code_calls_it() {
    let shop = jadpo_std::LocalePlan {
        default: jadpo_std::variant_ordinal(jadpo_std::LOCALE, "en_gb").unwrap(),
        supported: ["en_gb", "en_us", "fr_fr", "fr_ca", "de_de"]
            .iter()
            .map(|at| jadpo_std::variant_ordinal(jadpo_std::LOCALE, at).unwrap())
            .collect(),
        in_progress: Vec::new(),
    };
    let pick = |name: &Value, shopper: &str| {
        run(
            Op::TextPick,
            &[name.clone(), locale(shopper)],
            Some(&shop),
            &mut |_| None,
        )
        .expect("picked")
    };
    // The page's test.
    let rye = localised(&[("fr_fr", "Pain de seigle"), ("en_gb", "Rye loaf")]);
    assert_eq!(pick(&rye, "fr_fr"), Value::text("Pain de seigle"));
    assert_eq!(pick(&rye, "fr_ca"), Value::text("Pain de seigle"));
    assert_eq!(pick(&rye, "de_de"), Value::text("Rye loaf"));
    let roggen = localised(&[("de_de", "Roggenbrot")]);
    assert_eq!(pick(&roggen, "fr_fr"), Value::text("Roggenbrot"));
    // The default locale's before the first in the canonical order.
    let both = localised(&[("de_de", "Roggenbrot"), ("en_gb", "Rye loaf")]);
    assert_eq!(pick(&both, "fr_ca"), Value::text("Rye loaf"));
    assert_eq!(pick(&localised(&[]), "en_gb"), Value::text(""));
}

#[test]
fn localised_text_is_sent_keyed_by_each_locale_s_tag() {
    use jadpo_std::http::VariantCodec;
    let supported = ["en_gb", "fr_fr"];
    let codecs = [
        Codec::Map {
            key: CodecId(1),
            value: CodecId(2),
            validator: ValidatorId(0),
        },
        // A `Locale` codec lists only the supported locales, as the plan
        // makes it.
        Codec::Enum {
            ty: jadpo_std::LOCALE.to_string(),
            variants: supported
                .iter()
                .map(|at| {
                    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, at).unwrap();
                    VariantCodec {
                        name: (*at).to_string(),
                        wire: jadpo_std::LOCALES[ordinal as usize].1.to_string(),
                        ordinal,
                        fields: None,
                    }
                })
                .collect(),
        },
        Codec::Scalar {
            builtin: Builtin::Text,
            validator: ValidatorId(0),
            int_as_number: false,
        },
    ];
    let validators = [Validator::new(None, Vec::new())];
    let table = Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &[],
    };
    let read = |json: &str| {
        let mut problems = Vec::new();
        let json: serde_json::Value = serde_json::from_str(json).unwrap();
        table.read(CodecId(0), &json, &Path::default(), &mut problems)
    };
    let rye = read(r#"{"fr-FR": "Pain de seigle", "en-GB": "Rye loaf"}"#).expect("read");
    assert_eq!(
        rye,
        localised(&[("en_gb", "Rye loaf"), ("fr_fr", "Pain de seigle")])
    );
    let mut json = String::new();
    table.write(CodecId(0), &rye, &mut json);
    assert_eq!(json, r#"{"en-GB":"Rye loaf","fr-FR":"Pain de seigle"}"#);
    assert_eq!(
        read(r#"{"de-DE": "Roggenbrot"}"#),
        None,
        "only a supported locale"
    );
    assert_eq!(
        read(r#"{"en-gb": "Rye loaf"}"#),
        Some(localised(&[("en_gb", "Rye loaf")])),
        "a tag in any case"
    );
}
