//! Translations (area 10, chunk 5.3b): `translations/<locale>.json`, one
//! file per supported locale of ICU MessageFormat messages. The default
//! locale's file defines the keys, each becomes a typed
//! `translations.<key>(locale:, ...)` call, and every other file is checked
//! for the same keys and placeholders. The errors each mistake gives, with
//! where in the JSON file they point, and what a call checks.

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

use crate::support;

const APP: &str = "application {\n}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n}\n";

const EN: &str = r#"{
  "order_shipped": "Your order {order_number} has shipped.",
  "items_in_cart": "{count, plural, one {# item} other {# items}} in your cart"
}
"#;

const FR: &str = r#"{
  "order_shipped": "Votre commande {order_number} a été expédiée.",
  "items_in_cart": "{count, plural, one {# article} many {# articles} other {# articles}} dans votre panier"
}
"#;

const CALLS: &str = "subject: translations.order_shipped(locale: reader_locale, order_number: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)";

fn files(app: &str, translations: &[(&str, &str)], calls: &str) -> Vec<(String, String)> {
    let mut files = vec![
        ("app.jadpo".to_string(), app.to_string()),
        (
            "domains/shop/types/order_number.jadpo".to_string(),
            "/// An order's number.\ntype OrderNumber = Text {\n    max_length: 20\n}\n".into(),
        ),
        (
            "domains/shop/types/item_count.jadpo".to_string(),
            "/// How many items.\ntype ItemCount = Int {\n    min: 0\n}\n".into(),
        ),
        (
            "domains/shop/types/shipped_text.jadpo".to_string(),
            "/// What a customer is told.\ntype ShippedText = Output {\n    subject: PresentationText\n    cart: PresentationText\n}\n".into(),
        ),
        (
            "domains/shop/actions/show_shipped.jadpo".to_string(),
            format!(
                "use shop/types {{ ItemCount, OrderNumber, ShippedText }}\n\n/// The text, in the customer's language.\naction show_shipped(number: OrderNumber, count: ItemCount, reader_locale: Locale) -> ShippedText {{\n    return ShippedText {{\n        {calls}\n    }}\n}}\n"
            ),
        ),
    ];
    for (name, text) in translations {
        files.push((format!("translations/{name}"), (*text).to_string()));
    }
    files
}

fn diagnostics(files: &[(String, String)]) -> Vec<Diagnostic> {
    let db = Database::default();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let project = support::project(&db, Layout::Project, &borrowed);
    jadpo_check::check(&db, project)
}

fn rules_of(files: &[(String, String)]) -> Vec<String> {
    diagnostics(files)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

fn rules(translations: &[(&str, &str)]) -> Vec<String> {
    rules_of(&files(APP, translations, CALLS))
}

fn rules_with_calls(translations: &[(&str, &str)], calls: &str) -> Vec<String> {
    rules_of(&files(APP, translations, calls))
}

/// The line and column (from 1) a byte offset of `text` is at.
fn position(text: &str, offset: u32) -> (usize, usize) {
    let before = &text[..offset as usize];
    let line = before.matches('\n').count() + 1;
    let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
    (line, column)
}

/// The path, line and column the one problem points at, and the text it
/// underlines.
fn pointed_at(translations: &[(&str, &str)]) -> (String, (usize, usize), String) {
    let all = files(APP, translations, CALLS);
    let found = diagnostics(&all);
    assert_eq!(
        found.len(),
        1,
        "{:#?}",
        found.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let label = found[0].primary.as_ref().expect("a primary label");
    let text = &all
        .iter()
        .find(|(path, _)| *path == label.span.path)
        .expect("a file")
        .1;
    (
        label.span.path.clone(),
        position(text, label.span.start),
        text[label.span.start as usize..label.span.end as usize].to_string(),
    )
}

const CLEAN: &[&str] = &[];

// --- what is clean -----------------------------------------------------------------

#[test]
fn two_files_with_the_same_keys_and_placeholders_check() {
    assert_eq!(rules(&[("en_gb.json", EN), ("fr_fr.json", FR)]), CLEAN);
}

#[test]
fn a_project_with_no_translations_is_not_asked_for_any() {
    let plain = files(APP, &[], "subject: \"a\"\n        cart: \"b\"");
    assert_eq!(rules_of(&plain), CLEAN);
}

#[test]
fn the_other_locales_may_word_a_plural_differently() {
    // French has `many`, English doesn't; `#` and the cases are the
    // translator's.
    let fr = FR.replace(
        "one {# article} many {# articles} other",
        "one {# article} other",
    );
    assert_eq!(rules(&[("en_gb.json", EN), ("fr_fr.json", &fr)]), CLEAN);
}

// --- the keys -----------------------------------------------------------------------

#[test]
fn a_key_the_default_file_has_and_another_lacks_is_missing() {
    let fr = r#"{
  "order_shipped": "Votre commande {order_number} a été expédiée."
}
"#;
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", fr)]),
        ["translation.key_missing"]
    );
}

#[test]
fn a_key_only_another_file_has_is_extra() {
    let fr = FR.replace("}\n", ",\n  \"order_cancelled\": \"Annulée.\"\n}\n");
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", &fr)]),
        ["translation.key_extra"]
    );
    let (path, at, text) = pointed_at(&[("en_gb.json", EN), ("fr_fr.json", &fr)]);
    assert_eq!(
        (path.as_str(), at, text.as_str()),
        ("translations/fr_fr.json", (5, 3), "\"order_cancelled\"")
    );
}

#[test]
fn a_key_is_a_name_a_call_can_use() {
    let en = EN.replace("order_shipped", "Order-Shipped");
    let fr = FR.replace("order_shipped", "Order-Shipped");
    assert_eq!(
        rules_with_calls(
            &[("en_gb.json", &en), ("fr_fr.json", &fr)],
            "subject: \"a\"\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        ["translation.key_invalid", "translation.key_invalid"]
    );
}

#[test]
fn a_key_written_twice_in_a_file_is_an_error() {
    let en = EN.replace(
        "}\n",
        ",\n  \"order_shipped\": \"Again {order_number}.\"\n}\n",
    );
    assert_eq!(
        rules(&[("en_gb.json", &en), ("fr_fr.json", FR)]),
        ["translation.key_duplicate"]
    );
}

// --- the placeholders ---------------------------------------------------------------

#[test]
fn a_placeholder_the_default_message_has_and_another_lacks_is_a_mismatch() {
    let fr = FR.replace(
        "Votre commande {order_number} a été expédiée.",
        "Votre commande a été expédiée.",
    );
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", &fr)]),
        ["translation.placeholder_mismatch"]
    );
}

#[test]
fn a_placeholder_only_another_message_has_is_a_mismatch() {
    let fr = FR.replace("{order_number} a été", "{order_number} de {customer} a été");
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", &fr)]),
        ["translation.placeholder_mismatch"]
    );
}

#[test]
fn a_placeholder_that_takes_another_kind_of_value_is_a_type_error() {
    let fr = FR.replace("{order_number}", "{order_number, number}");
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", &fr)]),
        ["translation.placeholder_type"]
    );
}

#[test]
fn a_placeholder_used_as_two_kinds_in_one_message_is_a_type_error() {
    let en = EN.replace(
        "Your order {order_number} has shipped.",
        "Your order {order_number} ({order_number, number}) has shipped.",
    );
    assert_eq!(
        rules_with_calls(
            &[("en_gb.json", &en), ("fr_fr.json", FR)],
            "subject: \"a\"\n        cart: \"b\""
        )
        .first()
        .map(String::as_str),
        Some("translation.placeholder_type")
    );
}

// --- the messages -------------------------------------------------------------------

#[test]
fn a_plural_with_no_other_case_is_an_error() {
    let en = EN.replace("one {# item} other {# items}", "one {# item}");
    assert_eq!(
        rules(&[("en_gb.json", &en), ("fr_fr.json", FR)]),
        ["translation.other_missing"]
    );
}

#[test]
fn a_select_with_no_other_case_is_an_error() {
    let en = EN.replace(
        "Your order {order_number} has shipped.",
        "{order_number, select, a {A} b {B}}",
    );
    let fr = FR.replace(
        "Votre commande {order_number} a été expédiée.",
        "{order_number, select, a {A} other {B}}",
    );
    assert_eq!(
        rules(&[("en_gb.json", &en), ("fr_fr.json", &fr)]),
        ["translation.other_missing"]
    );
}

#[test]
fn a_message_that_does_not_parse_points_into_the_json_file() {
    let en = EN.replace("{order_number} has", "{order_number has");
    let (path, at, text) = pointed_at(&[("en_gb.json", &en), ("fr_fr.json", FR)]);
    assert_eq!(path, "translations/en_gb.json");
    // Line 2, after `  "order_shipped": "Your order `.
    assert_eq!((at, text.as_str()), ((2, 32), "{"));
}

#[test]
fn a_message_error_after_an_escape_still_points_at_the_right_character() {
    // `é` is six characters of JSON and one of the message.
    let en = EN.replace(
        "Your order {order_number} has shipped.",
        "Caf\\u00e9 {order_number has shipped.",
    );
    let (_, at, text) = pointed_at(&[("en_gb.json", &en), ("fr_fr.json", FR)]);
    assert_eq!((at, text.as_str()), ((2, 31), "{"));
}

#[test]
fn a_placeholder_is_named_not_numbered() {
    let en = EN.replace("{order_number}", "{0}");
    assert_eq!(
        rules_with_calls(
            &[("en_gb.json", &en), ("fr_fr.json", FR)],
            "subject: \"a\"\n        cart: \"b\""
        )
        .first()
        .map(String::as_str),
        Some("translation.message_invalid")
    );
}

// --- the files ------------------------------------------------------------------------

#[test]
fn a_file_for_a_locale_the_application_does_not_support_is_an_error() {
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", FR), ("de_de.json", FR)]),
        ["translation.locale_unsupported"]
    );
}

#[test]
fn a_file_is_named_for_a_locale() {
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", FR), ("en-GB.json", EN)]),
        ["translation.file_name_invalid"]
    );
    assert_eq!(
        rules(&[("en_gb.json", EN), ("fr_fr.json", FR), ("english.json", EN)]),
        ["translation.file_name_invalid"]
    );
}

#[test]
fn a_supported_locale_with_no_file_is_an_error() {
    assert_eq!(rules(&[("en_gb.json", EN)]), ["translation.file_missing"]);
}

#[test]
fn the_default_locales_file_is_the_one_that_defines_the_keys() {
    let all = rules_with_calls(&[("fr_fr.json", FR)], "subject: \"a\"\n        cart: \"b\"");
    assert_eq!(all, ["translation.file_missing"]);
}

#[test]
fn translations_need_the_locales_declared() {
    let app = "application {\n}\n";
    let all = files(
        app,
        &[("en_gb.json", EN)],
        "subject: \"a\"\n        cart: \"b\"",
    );
    assert_eq!(rules_of(&all), ["translation.locales_required"]);
}

#[test]
fn a_file_that_is_not_json_points_at_the_slip() {
    let en = EN.replace("\"order_shipped\":", "\"order_shipped\" ");
    let (path, at, _) = pointed_at(&[("en_gb.json", &en), ("fr_fr.json", FR)]);
    assert_eq!((path.as_str(), at.0), ("translations/en_gb.json", 2));
    assert_eq!(
        rules(&[("en_gb.json", &en), ("fr_fr.json", FR)]),
        ["translation.file_invalid"]
    );
}

#[test]
fn a_message_is_a_string() {
    let en = EN.replace("\"Your order {order_number} has shipped.\"", "42");
    assert_eq!(
        rules_with_calls(
            &[("en_gb.json", &en), ("fr_fr.json", FR)],
            "subject: \"a\"\n        cart: \"b\""
        )
        .first()
        .map(String::as_str),
        Some("translation.file_invalid")
    );
    assert_eq!(
        rules_with_calls(
            &[("en_gb.json", "[]"), ("fr_fr.json", FR)],
            "subject: \"a\"\n        cart: \"b\""
        )
        .first()
        .map(String::as_str),
        Some("translation.file_invalid")
    );
}

#[test]
fn a_broken_default_file_is_reported_once_not_in_every_call() {
    let rules = rules(&[("en_gb.json", "{ nope"), ("fr_fr.json", FR)]);
    assert_eq!(rules, ["translation.file_invalid"]);
}

// --- the calls --------------------------------------------------------------------------

#[test]
fn a_key_no_file_defines_is_unknown() {
    assert_eq!(
        rules_with_calls(
            &[("en_gb.json", EN), ("fr_fr.json", FR)],
            "subject: translations.order_shiped(locale: reader_locale, order_number: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        ["translation.key_unknown"]
    );
}

#[test]
fn a_key_with_no_translations_at_all_is_unknown() {
    assert_eq!(
        rules_with_calls(&[], CALLS),
        ["translation.key_unknown", "translation.key_unknown"]
    );
}

#[test]
fn a_call_gives_the_locale_and_each_placeholder() {
    let both = &[("en_gb.json", EN), ("fr_fr.json", FR)];
    // No placeholder.
    assert_eq!(
        rules_with_calls(
            both,
            "subject: translations.order_shipped(locale: reader_locale)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        ["type.argument_count"]
    );
    // An argument the message has no placeholder for.
    assert_eq!(
        rules_with_calls(
            both,
            "subject: translations.order_shipped(locale: reader_locale, order_number: number, extra: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        ["type.named_argument_unknown"]
    );
    // No locale.
    assert_eq!(
        rules_with_calls(
            both,
            "subject: translations.order_shipped(order_number: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        ["type.argument_count"]
    );
}

#[test]
fn an_argument_is_the_kind_its_placeholder_takes() {
    let both = &[("en_gb.json", EN), ("fr_fr.json", FR)];
    // A number where text is wanted, and text where a count is.
    assert_eq!(
        rules_with_calls(
            both,
            "subject: translations.order_shipped(locale: reader_locale, order_number: count)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        rules_with_calls(
            both,
            "subject: translations.order_shipped(locale: reader_locale, order_number: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: number)"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn a_locale_written_out_is_a_supported_one() {
    let both = &[("en_gb.json", EN), ("fr_fr.json", FR)];
    assert_eq!(
        rules_with_calls(
            both,
            "subject: translations.order_shipped(locale: Locale.fr_fr, order_number: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        CLEAN
    );
    assert_eq!(
        rules_with_calls(
            both,
            "subject: translations.order_shipped(locale: Locale.de_de, order_number: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)"
        ),
        ["type.locale_unsupported"]
    );
}

#[test]
fn what_a_call_gives_is_text_for_a_person() {
    let both = &[("en_gb.json", EN), ("fr_fr.json", FR)];
    // It can't go where `Text` is wanted, as formatted times can't.
    let calls = "subject: translations.order_shipped(locale: reader_locale, order_number: number)\n        cart: translations.items_in_cart(locale: reader_locale, count: count)";
    assert_eq!(rules_with_calls(both, calls), CLEAN);
}

#[test]
fn a_number_a_date_and_a_choice_take_their_own_kinds() {
    let en = r#"{
  "summary": "{who} placed {n, number} on {when, date, long} at {when, time, short}: {kind, select, gift {a gift} other {an order}}"
}
"#;
    let fr = r#"{
  "summary": "{who} a passé {n, number} le {when, date, long} à {when, time, short} : {kind, select, gift {un cadeau} other {une commande}}"
}
"#;
    let action = |when: &str| {
        let mut all = files(
            APP,
            &[("en_gb.json", en), ("fr_fr.json", fr)],
            "subject: \"a\"\n        cart: \"b\"",
        );
        all.push((
            "domains/shop/types/placed_at.jadpo".into(),
            "/// When it was placed, where.\ntype PlacedAt = ZonedTime {}\n".into(),
        ));
        all.push((
            "domains/shop/types/placed_day.jadpo".into(),
            "/// The day it was placed.\ntype PlacedDay = CalendarDate {}\n".into(),
        ));
        all.push((
            "domains/shop/types/summary_text.jadpo".into(),
            "/// What a customer is told.\ntype SummaryText = Output {\n    summary: PresentationText\n}\n".into(),
        ));
        all.push((
            "domains/shop/actions/show_summary.jadpo".into(),
            format!("use shop/types {{ ItemCount, OrderNumber, PlacedAt, PlacedDay, SummaryText }}\n\n/// The summary.\naction show_summary(who: OrderNumber, n: ItemCount, when: {when}, kind: OrderNumber, reader_locale: Locale) -> SummaryText {{\n    return SummaryText {{\n        summary: translations.summary(locale: reader_locale, who: who, n: n, when: when, kind: kind)\n    }}\n}}\n"),
        ));
        all
    };
    let found = rules_of(&action("PlacedAt"));
    assert!(
        found
            .iter()
            .all(|rule| !rule.starts_with("translation.") && !rule.starts_with("type.")),
        "{found:?}"
    );
    // A date alone takes either; shown as a time as well it needs a clock.
    let wrong = rules_of(&action("PlacedDay"));
    assert!(wrong.contains(&"type.mismatch".to_string()), "{wrong:?}");
}

// --- what an edit recomputes ------------------------------------------------------------

mod incremental {
    use super::*;
    use jadpo_db::Record;
    use salsa::Setter;

    fn count(log: &[String], query: &str) -> usize {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    }

    fn setup() -> (Database, jadpo_db::Project) {
        let db = Database::recording();
        let all = files(APP, &[("en_gb.json", EN), ("fr_fr.json", FR)], CALLS);
        let borrowed: Vec<(&str, &str)> = all
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let project = support::project(&db, Layout::Project, &borrowed);
        (db, project)
    }

    fn set(db: &mut Database, project: jadpo_db::Project, name: &str, text: &str) {
        let mut translations = project.translations(db).clone();
        let path = format!("translations/{name}");
        for record in &mut translations {
            if record.path == path {
                *record = Record {
                    path: path.clone(),
                    text: Ok(text.to_string()),
                };
            }
        }
        project.set_translations(db).to(translations);
    }

    #[test]
    fn editing_a_message_without_changing_its_placeholders_rechecks_no_call() {
        let (mut db, project) = setup();
        assert!(jadpo_check::check(&db, project).is_empty());
        let cold = db.take_executions();
        assert!(count(&cold, "typed_body") >= 1, "{cold:?}");

        // The words of the default locale's message: no call reads them.
        set(
            &mut db,
            project,
            "en_gb.json",
            &EN.replace("has shipped", "is on its way"),
        );
        assert!(jadpo_check::check(&db, project).is_empty());
        let log = db.take_executions();
        assert_eq!(count(&log, "typed_body"), 0, "{log:?}");
        assert_eq!(count(&log, "translation_files"), 1, "{log:?}");

        // The words of another locale's.
        set(
            &mut db,
            project,
            "fr_fr.json",
            &FR.replace("expédiée", "envoyée"),
        );
        assert!(jadpo_check::check(&db, project).is_empty());
        let log = db.take_executions();
        assert_eq!(count(&log, "typed_body"), 0, "{log:?}");
    }

    #[test]
    fn editing_a_placeholder_rechecks_only_the_calls_of_that_key() {
        let (mut db, project) = setup();
        assert!(jadpo_check::check(&db, project).is_empty());
        db.take_executions();

        // `order_shipped` takes `buyer` now: the action calling it is
        // rechecked, and says so.
        let en = EN.replace("{order_number}", "{buyer}");
        let fr = FR.replace("{order_number}", "{buyer}");
        set(&mut db, project, "en_gb.json", &en);
        set(&mut db, project, "fr_fr.json", &fr);
        let found = jadpo_check::check(&db, project);
        assert!(
            found
                .iter()
                .any(|d| d.rule.id == "type.named_argument_unknown"),
            "{found:#?}"
        );
        let log = db.take_executions();
        assert_eq!(count(&log, "typed_body"), 1, "{log:?}");
    }
}
