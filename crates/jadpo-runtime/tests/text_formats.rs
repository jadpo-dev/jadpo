//! Area 19, "Text formats" (chunk 5.5o), in the runtime: the values the
//! page's test (`project=text-formats`) and the interpreter's
//! `text_formats` tests assert, given to `markup`'s operations as compiled
//! code calls them (`jadpo_value::run`); an `Html` value built, read from
//! its text and read from a request's JSON through the sanitiser, as the
//! plan's validator and codec hold it; and a type built on `Html`
//! counting what the sanitiser keeps. Its text, JSON and column are the
//! one harness's (`standard_types`).

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
