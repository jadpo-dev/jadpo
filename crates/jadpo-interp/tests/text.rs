//! Area 19, "Text" (chunk 5.5f), run: each text operation gives what the
//! page says through the interpreter, in code points whatever the script,
//! with combining marks, emoji and CJK; a locale's case and order; and the
//! proofs `at_most_length_of` gives hold as the cut text runs. The
//! runtime's test (crates/jadpo-runtime/tests/text.rs) holds the same
//! operations to the same values.

use crate::support;

use jadpo_interp::Value;
use support::{int, list, main, text, value};

const SOURCE: &str = "type ProductName = Text {\n    min_length: 1\n    max_length: 60\n}\n\ntype Caption = Text {\n    max_length: 60\n}\n\ntype Separator = Text {\n    min_length: 1\n    max_length: 3\n}\n\ntype Words = Text {}\n\ntype Count = Int {\n    min: 0\n}\n\ntype Found = Bool {}\n\nfunction caption(name: ProductName, n: Count) -> Caption {\n    return Caption(text.keep_first(name, n))\n}\n\nfunction tail(name: ProductName, n: Count) -> Caption {\n    return Caption(text.keep_last(name, n))\n}\n\nfunction without_first(name: ProductName, n: Count) -> Caption {\n    return Caption(text.drop_first(name, n))\n}\n\nfunction without_last(name: ProductName, n: Count) -> Caption {\n    return Caption(text.drop_last(name, n))\n}\n\nfunction window(name: ProductName, from: Count, n: Count) -> Caption {\n    return Caption(text.slice(name, from, n))\n}\n\nfunction tidy(name: ProductName) -> Caption {\n    return Caption(text.trim(name))\n}\n\nfunction size(name: ProductName) -> Count {\n    return Count(text.length(name))\n}\n\nfunction lower(name: ProductName) -> Words {\n    return Words(text.lower(name))\n}\n\nfunction upper(name: ProductName, reader: Locale) -> Words {\n    return Words(text.upper(name, locale: reader))\n}\n\nfunction lower_in(name: ProductName, reader: Locale) -> Words {\n    return Words(text.lower(name, locale: reader))\n}\n\nfunction plain_upper(name: ProductName) -> Words {\n    return Words(text.upper(name))\n}\n\nfunction has(name: ProductName, part: Separator) -> Found {\n    return Found(text.contains(name, part))\n}\n\nfunction starts(name: ProductName, part: Separator) -> Found {\n    return Found(text.starts_with(name, part))\n}\n\nfunction ends(name: ProductName, part: Separator) -> Found {\n    return Found(text.ends_with(name, part))\n}\n\nfunction swap(name: ProductName, find: Separator, with: Separator) -> Words {\n    return Words(text.replace(name, find, with))\n}\n\nfunction pieces(line: ProductName, separator: Separator) -> Count {\n    return Count(list.count(text.split(line, separator)))\n}\n\nfunction rejoined(line: ProductName, separator: Separator) -> Words {\n    return Words(text.join(text.split(line, separator), \" + \"))\n}\n\nfunction padded(n: Count) -> Words {\n    return Words(text.pad_start(text.from(n), 6, fill: \"0\"))\n}\n\nfunction slug(name: ProductName) -> Words {\n    return Words(text.slug(name))\n}\n\nfunction by_point(names: List<ProductName>) -> List<ProductName> {\n    return list.sort_by(names, name -> name)\n}\n\nfunction by_name(names: List<ProductName>, reader: Locale) -> List<ProductName> {\n    return list.sort_by(names, name -> name, locale: reader)\n}\n";

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::locale_ordinal(name).unwrap_or_else(|| panic!("Locale.{name}"));
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

fn yes(found: bool) -> Value {
    Value::Bool(found)
}

#[test]
fn text_is_counted_and_cut_in_code_points() {
    // Every construction here is proved by `at_most_length_of`, and the
    // proofs are checked again as the program runs.
    let program = main(SOURCE).with_proof_checks();
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    // `é` as one code point, and as `e` with a combining acute.
    assert_eq!(call("size", vec![text("café")]), int(4));
    assert_eq!(call("size", vec![text("cafe\u{301}")]), int(5));
    assert_eq!(
        call("without_last", vec![text("cafe\u{301}"), int(1)]),
        text("cafe")
    );
    assert_eq!(
        call("tail", vec![text("cafe\u{301}"), int(1)]),
        text("\u{301}")
    );
    // An emoji with a skin tone is two code points.
    assert_eq!(call("size", vec![text("👍🏽")]), int(2));
    assert_eq!(call("caption", vec![text("👍🏽 ok"), int(1)]), text("👍"));
    // CJK, one code point a character.
    assert_eq!(call("size", vec![text("面包店")]), int(3));
    assert_eq!(call("caption", vec![text("面包店"), int(2)]), text("面包"));
    assert_eq!(
        call("window", vec![text("面包店"), int(1), int(9)]),
        text("包店")
    );
    assert_eq!(
        call("without_first", vec![text("sourdough"), int(4)]),
        text("dough")
    );
    assert_eq!(call("without_last", vec![text("rye"), int(9)]), text(""));
    assert_eq!(call("tidy", vec![text("\u{a0}\t rye \n")]), text("rye"));
}

#[test]
fn searching_replacing_splitting_and_joining() {
    let program = main(SOURCE);
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(call("has", vec![text("Rye loaf"), text("loa")]), yes(true));
    assert_eq!(
        call("starts", vec![text("Rye loaf"), text("Rye")]),
        yes(true)
    );
    assert_eq!(
        call("ends", vec![text("Rye loaf"), text("Rye")]),
        yes(false)
    );
    // Composed and decomposed text differ in code: only a boundary
    // composes it.
    assert_eq!(
        call("has", vec![text("cafe\u{301}"), text("é")]),
        yes(false)
    );
    assert_eq!(
        call("swap", vec![text("salt&pepper&oil"), text("&"), text(", ")]),
        text("salt, pepper, oil")
    );
    assert_eq!(
        call("swap", vec![text("aaa"), text("aa"), text("b")]),
        text("ba"),
        "left to right"
    );
    assert_eq!(
        call("pieces", vec![text("flour,sugar,,eggs"), text(",")]),
        int(4)
    );
    assert_eq!(call("pieces", vec![text("rye"), text(",")]), int(1));
    assert_eq!(
        call("rejoined", vec![text("面包,🍞"), text(",")]),
        text("面包 + 🍞")
    );
    assert_eq!(call("padded", vec![int(42)]), text("000042"));
    assert_eq!(call("padded", vec![int(1234567)]), text("1234567"));
    assert_eq!(
        call("slug", vec![text("  Crème Brûlée & Co. ")]),
        text("creme-brulee-co")
    );
    assert_eq!(
        call("slug", vec![text("Cre\u{300}me")]),
        text("creme"),
        "a decomposed accent is dropped too"
    );
    assert_eq!(call("slug", vec![text("面包")]), text(""));
}

#[test]
fn a_locale_s_case_and_order() {
    let program = main(SOURCE);
    let call = |name: &str, args: Vec<Value>| value(&program, name, args);
    assert_eq!(
        call("upper", vec![text("istanbul"), locale("tr_tr")]),
        text("İSTANBUL")
    );
    assert_eq!(
        call("upper", vec![text("istanbul"), locale("en_gb")]),
        text("ISTANBUL")
    );
    assert_eq!(
        call("lower_in", vec![text("DİYARBAKIR"), locale("tr_tr")]),
        text("diyarbakır")
    );
    assert_eq!(call("lower", vec![text("İ")]), text("i\u{307}"));
    assert_eq!(call("plain_upper", vec![text("straße")]), text("STRASSE"));
    let names = list(vec![text("Zopf"), text("Ölandsbröd"), text("Oat")]);
    assert_eq!(
        call("by_point", vec![names.clone()]),
        list(vec![text("Oat"), text("Zopf"), text("Ölandsbröd")])
    );
    assert_eq!(
        call("by_name", vec![names.clone(), locale("de_de")]),
        list(vec![text("Oat"), text("Ölandsbröd"), text("Zopf")])
    );
    assert_eq!(
        call("by_name", vec![names, locale("sv_se")]),
        list(vec![text("Oat"), text("Zopf"), text("Ölandsbröd")])
    );
}
