//! Area 19, "Text" (chunk 5.5f): every text operation checks, giving plain
//! `Text` or what its table says; `never_empty` text is proved so
//! (`type.argument_empty`), `fill:` is one character written out,
//! `text.join` takes a list of any text, `trim` and the cuts keep their
//! text's `max_length` (`at_most_length_of`) where `upper` doesn't,
//! `locale:` orders text keys only, and `text` names no variable.

use crate::support;

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

const NAMES: &str = "type Title = Text {\n    min_length: 1\n    max_length: 60\n}\n\ntype Caption = Text {\n    max_length: 60\n}\n\ntype Short = Text {\n    max_length: 12\n}\n\ntype Separator = Text {\n    min_length: 1\n    max_length: 3\n}\n\ntype Loose = Text {\n    max_length: 3\n}\n\ntype Words = Text {}\n\ntype TagName = Text {\n    min_length: 1\n    max_length: 20\n}\n\ntype Grams = Int {\n    min: 0\n}\n\ntype Signed = Int {}\n\ntype Found = Bool {}\n\ntype Batch = Object {\n    name: Title\n    grams: Grams\n}\n\n";

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    );
    jadpo_check::check(&db, project)
}

fn rules(source: &str) -> Vec<String> {
    diagnostics(source)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn every_text_operation_checks() {
    let source = "function words(t: Title, s: Separator, n: Grams, reader: Locale) -> List<Words> {\n    return [Words(text.trim(t)), Words(text.lower(t)), Words(text.upper(t, locale: reader)), Words(text.replace(t, s, \"and\")), Words(text.slice(t, 1, n)), Words(text.keep_first(t, 3)), Words(text.keep_last(t, n)), Words(text.drop_first(t, 1)), Words(text.drop_last(t, n)), Words(text.pad_start(t, 6, fill: \"0\")), Words(text.slug(t)), Words(text.join(text.split(t, s), \"-\"))]\n}\n\nfunction checks(t: Title, s: Separator) -> List<Found> {\n    return [Found(text.contains(t, s)), Found(text.starts_with(t, \"R\")), Found(text.ends_with(t, s))]\n}\n\nfunction size(t: Title) -> Grams {\n    return Grams(text.length(t))\n}\n\nfunction joined(tags: List<TagName>) -> Words {\n    return Words(text.join(tags, \", \"))\n}\n\nfunction ordered(bakings: List<Batch>, reader: Locale) -> List<Batch> {\n    return list.sort_by(bakings, batch -> batch.name, locale: reader)\n}\n";
    assert_eq!(rules(source), NONE);
}

#[test]
fn results_are_plain_text_or_what_the_table_says() {
    // Plain `Text` is built into a named type, as any representation is.
    assert_eq!(
        rules("function f(t: Title) -> Words {\n    return text.trim(t)\n}\n"),
        ["type.mismatch"]
    );
    // `split` gives a list of text; `length` an `Int` never below 0.
    assert_eq!(
        rules(
            "function f(t: Title, s: Separator) -> Grams {\n    return Grams(list.count(text.split(t, s)))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function f(t: Title) -> Signed {\n    return Signed(text.length(t))\n}\n"),
        NONE
    );
    // Text operations take text.
    assert_eq!(
        rules("function f(n: Grams) -> Words {\n    return Words(text.trim(n))\n}\n"),
        ["type.mismatch"]
    );
}

#[test]
fn cutting_never_lengthens_text_and_case_can() {
    // `trim`, `slice` and the four cuts keep the text's `max_length`.
    for cut in [
        "text.trim(t)",
        "text.slice(t, 1, 4)",
        "text.keep_first(t, 4)",
        "text.keep_last(t, 4)",
        "text.drop_first(t, 4)",
        "text.drop_last(t, 4)",
        "text.trim(text.keep_first(t, 4))",
    ] {
        assert_eq!(
            rules(&format!(
                "function f(t: Title) -> Caption {{\n    return Caption({cut})\n}}\n"
            )),
            NONE,
            "{cut}"
        );
    }
    // A shorter `max_length` than the text's can still break, as can
    // `min_length`: only the text's own bound carries over.
    assert_eq!(
        rules("function f(t: Title) -> Short {\n    return Short(text.keep_first(t, 4))\n}\n"),
        ["failure.attempt_required"]
    );
    assert_eq!(
        rules("function f(t: Caption) -> Title {\n    return Title(text.trim(t))\n}\n"),
        ["failure.attempt_required"]
    );
    // `upper` can lengthen text (`ß` becomes `SS`).
    assert_eq!(
        rules("function f(t: Title) -> Caption {\n    return Caption(text.upper(t))\n}\n"),
        ["failure.attempt_required"]
    );
}

#[test]
fn never_empty_text_is_proved() {
    let found = diagnostics(
        "function f(t: Title, s: Loose) -> Words {\n    return Words(text.replace(t, s, \"-\"))\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.argument_empty");
    assert_eq!(found[0].message, "`s` may be empty");
    assert_eq!(
        rules(
            "function f(t: Title) -> List<Words> {\n    return [Words(text.replace(t, \"\", \"-\"))]\n}\n"
        ),
        ["type.argument_empty"]
    );
    assert_eq!(
        rules(
            "function f(t: Title) -> Grams {\n    return Grams(list.count(text.split(t, text.trim(t))))\n}\n"
        ),
        ["type.argument_empty"]
    );
    // A literal that isn't empty, and a type with `min_length: 1`, are.
    assert_eq!(
        rules(
            "function f(t: Title, s: Separator) -> Words {\n    return Words(text.replace(text.replace(t, s, \"\"), \",\", \"\"))\n}\n"
        ),
        NONE
    );
}

#[test]
fn fill_is_one_character_written_out() {
    assert_eq!(
        rules(
            "function f(t: Title) -> Words {\n    return Words(text.pad_start(t, 6, fill: \"00\"))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(t: Title, s: Separator) -> Words {\n    return Words(text.pad_start(t, 6, fill: s))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules("function f(t: Title) -> Words {\n    return Words(text.pad_start(t, 6))\n}\n"),
        ["type.argument_count"]
    );
    // One code point, whatever its script.
    assert_eq!(
        rules(
            "function f(t: Title) -> Words {\n    return Words(text.pad_start(t, 6, fill: \"·\"))\n}\n"
        ),
        NONE
    );
}

#[test]
fn join_takes_a_list_of_any_text() {
    assert_eq!(
        rules(
            "function f(ns: List<Grams>) -> Words {\n    return Words(text.join(ns, \",\"))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function f(t: Title, s: Separator) -> Words {\n    return Words(text.join([t, t], \",\"))\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_locale_orders_text_keys_only() {
    let found = diagnostics(
        "function f(bakings: List<Batch>, reader: Locale) -> List<Batch> {\n    return list.sort_by(bakings, batch -> batch.grams, locale: reader)\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule.id, "type.temporal_option_unused");
    assert_eq!(
        found[0].message,
        "`locale:` means nothing for a `Batch.grams` key"
    );
    assert_eq!(
        rules(
            "function f(t: Title) -> Words {\n    return Words(text.lower(t, locale: Locale.tr_tr))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function f(t: Title, n: Grams) -> Words {\n    return Words(text.lower(t, locale: n))\n}\n"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn a_list_written_out_of_values_fixes_the_item_type() {
    assert_eq!(
        rules(
            "function f(a: Batch, b: Batch) -> List<Batch> {\n    return list.sort_by([a, b], batch -> batch.name)\n}\n"
        ),
        NONE
    );
}

#[test]
fn text_names_no_variable_or_parameter() {
    for source in [
        "function f(t: Title) -> Title {\n    var text: Title = t\n    return text\n}\n",
        "function f(text: Title) -> Title {\n    return Title(text)\n}\n",
    ] {
        assert!(
            rules(source).contains(&"semantic.standard_namespace_reserved".to_string()),
            "{source}"
        );
    }
    // A field and a named argument keep the word.
    assert_eq!(
        rules(
            "type Note = Object {\n    text: Title\n}\n\nfunction f(note: Note) -> Title {\n    return note.text\n}\n"
        ),
        NONE
    );
}
