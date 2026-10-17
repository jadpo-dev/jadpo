//! Area 19, "Text" (chunk 5.5f), in the runtime: each text operation given
//! to its one implementation as compiled code calls it (`jadpo_value::run`,
//! which the interpreter calls too), with the same values as the
//! interpreter's test (crates/jadpo-interp/tests/text.rs); and text a
//! request carries composed to NFC by the codecs both executors decode
//! through, before its rules are checked.

use jadpo_std::http::Codec;

use jadpo_std::types::ValidatorId;
use jadpo_std::{Builtin, Op, Rules};
use jadpo_value::json::{Codecs, Path};
use jadpo_value::{Fault, Validator, Value, run};

fn call(op: Op, args: &[Value]) -> Result<Value, Fault> {
    run(op, args, None, &mut |_| None)
}

fn ok(op: Op, args: &[Value]) -> Value {
    call(op, args).unwrap_or_else(|fault| panic!("{op:?}: {fault:?}"))
}

fn text(text: &str) -> Value {
    Value::text(text)
}

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::locale_ordinal(name).unwrap_or_else(|| panic!("Locale.{name}"));
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

#[test]
fn text_is_counted_and_cut_in_code_points() {
    let n = Value::Int;
    assert_eq!(ok(Op::TextLength, &[text("café")]), n(4));
    assert_eq!(ok(Op::TextLength, &[text("cafe\u{301}")]), n(5));
    assert_eq!(
        ok(Op::TextDropLast, &[text("cafe\u{301}"), n(1)]),
        text("cafe")
    );
    assert_eq!(
        ok(Op::TextKeepLast, &[text("cafe\u{301}"), n(1)]),
        text("\u{301}")
    );
    assert_eq!(ok(Op::TextLength, &[text("👍🏽")]), n(2));
    assert_eq!(ok(Op::TextKeepFirst, &[text("👍🏽 ok"), n(1)]), text("👍"));
    assert_eq!(ok(Op::TextLength, &[text("面包店")]), n(3));
    assert_eq!(ok(Op::TextKeepFirst, &[text("面包店"), n(2)]), text("面包"));
    assert_eq!(
        ok(Op::TextSlice, &[text("面包店"), n(1), n(9)]),
        text("包店")
    );
    assert_eq!(
        ok(Op::TextDropFirst, &[text("sourdough"), n(4)]),
        text("dough")
    );
    assert_eq!(ok(Op::TextDropLast, &[text("rye"), n(9)]), text(""));
    assert_eq!(ok(Op::TextTrim, &[text("\u{a0}\t rye \n")]), text("rye"));
    // A negative count never arrives (the checker proves it), and is a
    // fault rather than a count from the other end if it does.
    assert_eq!(
        call(Op::TextKeepFirst, &[text("rye"), n(-1)]),
        Err(Fault::Shape)
    );
}

#[test]
fn searching_replacing_splitting_and_joining() {
    let yes = Value::Bool;
    assert_eq!(
        ok(Op::TextContains, &[text("Rye loaf"), text("loa")]),
        yes(true)
    );
    assert_eq!(
        ok(Op::TextStartsWith, &[text("Rye loaf"), text("Rye")]),
        yes(true)
    );
    assert_eq!(
        ok(Op::TextEndsWith, &[text("Rye loaf"), text("Rye")]),
        yes(false)
    );
    assert_eq!(
        ok(Op::TextContains, &[text("cafe\u{301}"), text("é")]),
        yes(false)
    );
    assert_eq!(
        ok(Op::TextReplace, &[text("aaa"), text("aa"), text("b")]),
        text("ba")
    );
    assert_eq!(
        ok(Op::TextSplit, &[text("flour,sugar,,eggs"), text(",")]),
        Value::list(vec![text("flour"), text("sugar"), text(""), text("eggs")])
    );
    assert_eq!(
        ok(Op::TextSplit, &[text(""), text(",")]),
        Value::list(vec![text("")])
    );
    assert_eq!(
        ok(
            Op::TextJoin,
            &[Value::list(vec![text("面包"), text("🍞")]), text(" + ")]
        ),
        text("面包 + 🍞")
    );
    assert_eq!(
        ok(Op::TextPadStart, &[text("42"), Value::Int(6), text("0")]),
        text("000042")
    );
    assert_eq!(
        ok(
            Op::TextPadStart,
            &[text("1234567"), Value::Int(6), text("0")]
        ),
        text("1234567")
    );
    assert_eq!(
        ok(Op::TextSlug, &[text("  Crème Brûlée & Co. ")]),
        text("creme-brulee-co")
    );
    assert_eq!(ok(Op::TextSlug, &[text("Cre\u{300}me")]), text("creme"));
    assert_eq!(ok(Op::TextSlug, &[text("面包")]), text(""));
    // An empty search never arrives (`never_empty`), and is a fault.
    assert_eq!(
        call(Op::TextReplace, &[text("rye"), text(""), text("-")]),
        Err(Fault::Shape)
    );
}

#[test]
fn a_locale_s_case_and_order() {
    assert_eq!(
        ok(Op::TextUpper, &[text("istanbul"), locale("tr_tr")]),
        text("İSTANBUL")
    );
    assert_eq!(
        ok(Op::TextUpper, &[text("istanbul"), locale("en_gb")]),
        text("ISTANBUL")
    );
    assert_eq!(
        ok(Op::TextLower, &[text("DİYARBAKIR"), locale("tr_tr")]),
        text("diyarbakır")
    );
    assert_eq!(
        ok(Op::TextLower, &[text("İ"), Value::None]),
        text("i\u{307}")
    );
    assert_eq!(
        ok(Op::TextUpper, &[text("straße"), Value::None]),
        text("STRASSE")
    );
    let names = Value::list(vec![text("Zopf"), text("Ölandsbröd"), text("Oat")]);
    let sorted = |reader: Value| {
        run(
            Op::ListSortBy,
            &[names.clone(), reader],
            None,
            &mut |item| item.first().cloned(),
        )
        .unwrap_or_else(|fault| panic!("sort_by: {fault:?}"))
    };
    assert_eq!(
        sorted(Value::None),
        Value::list(vec![text("Oat"), text("Zopf"), text("Ölandsbröd")])
    );
    assert_eq!(
        sorted(locale("de_de")),
        Value::list(vec![text("Oat"), text("Ölandsbröd"), text("Zopf")])
    );
    assert_eq!(
        sorted(locale("sv_se")),
        Value::list(vec![text("Oat"), text("Zopf"), text("Ölandsbröd")])
    );
}

#[test]
fn text_a_request_carries_is_composed_before_its_rules() {
    // A text type of at most four code points: `café` decomposed is five,
    // and composed four, which is what is kept and counted.
    let codecs = [Codec::Scalar {
        builtin: Builtin::Text,
        validator: ValidatorId(0),
        int_as_number: false,
    }];
    let validators = [Validator::new(
        Some(Builtin::Text),
        vec![Rules {
            max_length: Some(4),
            ..Rules::default()
        }],
    )];
    let table = Codecs {
        codecs: &codecs,
        validators: &validators,
        shapes: &[],
    };
    let id = jadpo_std::http::CodecId(0);
    let mut problems = Vec::new();
    let body = serde_json::Value::String("cafe\u{301}".to_string());
    assert_eq!(
        table.read(id, &body, &Path::default(), &mut problems),
        Some(text("café"))
    );
    // A path placeholder, a query string or a header, read as text.
    assert_eq!(
        table.read_text(id, "cafe\u{301}", &Path::default(), &mut problems),
        Some(text("café"))
    );
    assert_eq!(
        table.read_query(
            id,
            &["cafe\u{301}".to_string()],
            &Path::default(),
            &mut problems
        ),
        Some(text("café"))
    );
    assert!(problems.is_empty(), "{problems:?}");
}
