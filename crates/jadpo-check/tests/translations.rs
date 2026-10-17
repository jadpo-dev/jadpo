//! Translations as messages (area 10, chunk 5.4c): a message is a type in a
//! domain's `translations/` folder, each language is a `translations { }`
//! file named for its locale, and each message becomes a typed call. The
//! errors each mistake gives and where they point, what the checker lets
//! through, and what a call checks.

use jadpo_db::{Database, Layout};
use jadpo_diag::Diagnostic;

use crate::support;

const APP: &str = "application {\n}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr]\n}\n";

const TONE: &str = "type Tone = Enum {\n    formal\n    casual\n}\n";

const ORDER_SHIPPED: &str = "use shipping/types { Tone }\n\n/// Subject of the email sent when a parcel leaves the warehouse.\ntype OrderShipped = Message {\n    tone: Tone\n    name: Text\n}\n";

const PARCELS: &str = "/// How many parcels an order has, on the tracking page.\ntype Parcels = Message {\n    count: PluralCount\n}\n";

const EN: &str = r#"translations {
    order_shipped: {
        formal: "Good day {name}, your order has shipped."
        casual: "Hi {name}, your order is on its way!"
    }
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
"#;

const FR: &str = r#"translations {
    order_shipped: {
        formal: "Bonjour {name}, votre commande a été expédiée."
        casual: "Salut {name}, votre commande est en route !"
    }
    parcels: {
        one: "{count} colis"
        many: "{count} de colis"
        other: "{count} colis"
    }
}
"#;

const CALLER: &str = "use shipping/types { CustomerName, ParcelCount, ShippingText, Tone }\n\n/// The text, in the customer's language.\naction describe_order(tone: Tone, name: CustomerName, count: ParcelCount, reader_locale: Locale) -> ShippingText {\n    return ShippingText {\n        subject: translations.shipping.order_shipped(tone: tone, name: name, locale: reader_locale)\n        parcels: translations.shipping.parcels(count: count, locale: reader_locale)\n    }\n}\n";

type Files = Vec<(String, String)>;

/// The shipping domain with its two messages and both languages, and the
/// action that calls them; each of `replace` swaps one file, or removes it
/// when its text is empty.
fn base(replace: &[(&str, &str)]) -> Files {
    let mut files: Files = vec![
        ("app.jadpo", APP),
        ("domains/shipping/types/tone.jadpo", TONE),
        (
            "domains/shipping/types/customer_name.jadpo",
            "/// A customer's first name.\ntype CustomerName = Text {\n    max_length: 80\n}\n",
        ),
        (
            "domains/shipping/types/parcel_count.jadpo",
            "/// How many parcels.\ntype ParcelCount = Int {\n    min: 0\n}\n",
        ),
        (
            "domains/shipping/types/shipping_text.jadpo",
            "/// What a customer is told.\ntype ShippingText = Output {\n    subject: PresentationText\n    parcels: PresentationText\n}\n",
        ),
        (
            "domains/shipping/translations/order_shipped.jadpo",
            ORDER_SHIPPED,
        ),
        ("domains/shipping/translations/parcels.jadpo", PARCELS),
        ("domains/shipping/translations/en_gb.jadpo", EN),
        ("domains/shipping/translations/fr_fr.jadpo", FR),
        ("domains/shipping/actions/describe_order.jadpo", CALLER),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect();
    for (path, text) in replace {
        match files.iter_mut().find(|(seen, _)| seen == path) {
            Some(found) => found.1 = (*text).to_string(),
            None => files.push(((*path).to_string(), (*text).to_string())),
        }
    }
    files.retain(|(_, text)| !text.is_empty());
    files
}

fn diagnostics(files: &Files) -> Vec<Diagnostic> {
    let db = Database::default();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let project = support::project(&db, Layout::Project, &borrowed);
    jadpo_check::check(&db, project)
}

fn rules_of(files: &Files) -> Vec<String> {
    diagnostics(files)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

fn rules(replace: &[(&str, &str)]) -> Vec<String> {
    rules_of(&base(replace))
}

/// The path, the text it underlines, and the rule, of the one problem.
fn pointed_at(replace: &[(&str, &str)]) -> (String, String, String) {
    let all = base(replace);
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
        text[label.span.start as usize..label.span.end as usize].to_string(),
        found[0].rule.id.to_string(),
    )
}

const CLEAN: &[&str] = &[];

const EN_PATH: &str = "domains/shipping/translations/en_gb.jadpo";
const FR_PATH: &str = "domains/shipping/translations/fr_fr.jadpo";

// --- what is clean --------------------------------------------------------------------

#[test]
fn messages_worded_in_every_supported_locale_check() {
    assert_eq!(rules(&[]), CLEAN);
}

#[test]
fn a_project_with_no_translations_is_not_asked_for_any() {
    let files: Files = vec![("app.jadpo".to_string(), "application {}\n".to_string())];
    assert_eq!(rules_of(&files), CLEAN);
}

#[test]
fn the_plural_forms_are_the_languages_own() {
    // English needs `one` and `other`; French adds `many`. A language that
    // needs only `other` words just that.
    let japanese = r#"translations {
    order_shipped: {
        formal: "{name}様、ご注文を発送しました。"
        casual: "{name}さん、ご注文を発送しました。"
    }
    parcels: {
        other: "荷物{count}個"
    }
}
"#;
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]",
        "supported: [Locale.en_gb, Locale.fr_fr, Locale.ja_jp]",
    );
    assert_eq!(
        rules(&[
            ("app.jadpo", &app),
            ("domains/shipping/translations/ja_jp.jadpo", japanese)
        ]),
        CLEAN
    );
}

#[test]
fn a_wording_may_use_a_field_the_other_languages_do_not() {
    // One value leaving a field out is normal.
    let en = EN.replace("Hi {name}, your", "Hi, your");
    assert_eq!(rules(&[(EN_PATH, &en)]), CLEAN);
}

#[test]
fn doubled_braces_are_literal_braces() {
    let en = EN.replace(
        "Good day {name}, your order has shipped.",
        "Good day {name}, your order has shipped {{today}}.",
    );
    assert_eq!(rules(&[(EN_PATH, &en)]), CLEAN);
}

#[test]
fn a_message_with_no_fields_is_a_plain_string() {
    let message = "/// Shown when the basket is empty.\ntype EmptyBasket = Message {\n}\n";
    let en = EN.replace(
        "parcels: {",
        "empty_basket: \"Your basket is empty.\"\n    parcels: {",
    );
    let fr = FR.replace(
        "parcels: {",
        "empty_basket: \"Votre panier est vide.\"\n    parcels: {",
    );
    assert_eq!(
        rules(&[
            ("domains/shipping/translations/empty_basket.jadpo", message),
            (EN_PATH, &en),
            (FR_PATH, &fr),
        ]),
        CLEAN
    );
}

// --- the language files ---------------------------------------------------------------

#[test]
fn a_language_file_is_named_for_a_locale() {
    assert_eq!(
        rules(&[("domains/shipping/translations/pt-BR.jadpo", EN)]),
        ["translation.file_name_invalid"]
    );
    assert_eq!(
        rules(&[("domains/shipping/translations/english.jadpo", EN)]),
        ["translation.file_name_invalid"]
    );
}

#[test]
fn a_file_for_a_locale_the_application_does_not_support_is_an_error() {
    assert_eq!(
        rules(&[("domains/shipping/translations/de_de.jadpo", EN)]),
        ["translation.locale_unsupported"]
    );
}

#[test]
fn a_supported_locale_with_no_file_is_one_error_naming_what_it_needs() {
    let found = diagnostics(&base(&[(FR_PATH, "")]));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].rule.id, "translation.file_missing");
    assert!(
        found[0].message.contains("`order_shipped` and `parcels`"),
        "{}",
        found[0].message
    );
    let label = found[0].primary.as_ref().expect("a label");
    assert_eq!(label.span.path, "app.jadpo");
}

#[test]
fn translations_need_the_applications_locales() {
    let files: Files = base(&[
        ("app.jadpo", "application {}\n"),
        ("domains/shipping/actions/describe_order.jadpo", ""),
    ]);
    assert!(
        rules_of(&files).contains(&"translation.locales_required".to_string()),
        "{:?}",
        rules_of(&files)
    );
}

#[test]
fn a_second_translations_declaration_in_a_file_needs_a_file_of_its_own() {
    let en = format!("{EN}\n{EN}");
    let found = rules(&[(EN_PATH, &en)]);
    assert_eq!(found, ["layout.file_name_mismatch"]);
}

#[test]
fn another_declaration_in_a_translations_folder_is_misplaced() {
    let found = rules(&[(
        "domains/shipping/translations/settings.jadpo",
        "config {\n}\n",
    )]);
    assert_eq!(found, ["layout.wrong_folder"]);
}

#[test]
fn a_message_outside_a_translations_folder_is_misplaced() {
    let found = rules(&[
        ("domains/shipping/translations/parcels.jadpo", ""),
        ("domains/shipping/types/parcels.jadpo", PARCELS),
    ]);
    assert!(
        found.contains(&"layout.wrong_folder".to_string()),
        "{found:?}"
    );
}

// --- the members ----------------------------------------------------------------------

#[test]
fn a_member_that_words_nothing_is_unknown_with_the_nearest_name() {
    let en = EN.replace("order_shipped:", "order_shiped:");
    // The misspelt member is the unknown one; `order_shipped` is then missing
    // from the file too, so there are two problems.
    let found = rules(&[(EN_PATH, &en)]);
    assert_eq!(
        found,
        ["translation.wording_missing", "translation.member_unknown"]
    );
}

#[test]
fn a_member_written_twice_is_an_error() {
    let en = EN.replace(
        "    parcels: {",
        "    parcels: {\n        one: \"{count} box\"\n        other: \"{count} boxes\"\n    }\n    parcels: {",
    );
    assert_eq!(rules(&[(EN_PATH, &en)]), ["translation.member_duplicate"]);
}

#[test]
fn a_member_with_the_wrong_shape_is_an_error() {
    // A string where a value-by-value block is needed, and the reverse.
    let en = EN.replace(
        "order_shipped: {\n        formal: \"Good day {name}, your order has shipped.\"\n        casual: \"Hi {name}, your order is on its way!\"\n    }",
        "order_shipped: \"Your order has shipped.\"",
    );
    assert_eq!(rules(&[(EN_PATH, &en)]), ["translation.wording_shape"]);
    let message = "/// Shown when the basket is empty.\ntype EmptyBasket = Message {\n}\n";
    let en = EN.replace(
        "parcels: {",
        "empty_basket: {\n        other: \"Empty\"\n    }\n    parcels: {",
    );
    let fr = FR.replace("parcels: {", "empty_basket: \"Vide\"\n    parcels: {");
    assert_eq!(
        rules(&[
            ("domains/shipping/translations/empty_basket.jadpo", message),
            (EN_PATH, &en),
            (FR_PATH, &fr),
        ]),
        ["translation.wording_shape"]
    );
}

#[test]
fn a_wording_that_is_not_a_string_is_a_shape_error() {
    let en = EN.replace("\"{count} parcel\"", "12");
    assert_eq!(rules(&[(EN_PATH, &en)]), ["translation.wording_shape"]);
}

// --- the values of an enum and the plural forms ------------------------------------------

#[test]
fn a_value_the_enum_does_not_have_is_unknown() {
    let en = EN.replace("casual:", "informal:");
    let found = rules(&[(EN_PATH, &en)]);
    assert_eq!(
        found,
        ["translation.variant_missing", "translation.variant_unknown"]
    );
}

#[test]
fn a_value_with_no_wording_is_missing() {
    let en = EN.replace(
        "        casual: \"Hi {name}, your order is on its way!\"\n",
        "",
    );
    let (path, text, rule) = pointed_at(&[(EN_PATH, &en)]);
    assert_eq!(
        (path.as_str(), text.as_str(), rule.as_str()),
        (EN_PATH, "order_shipped", "translation.variant_missing")
    );
}

#[test]
fn a_plural_form_the_language_has_no_use_for_is_unknown() {
    let en = EN.replace(
        "        one: \"{count} parcel\"",
        "        one: \"{count} parcel\"\n        few: \"{count} few\"",
    );
    assert_eq!(rules(&[(EN_PATH, &en)]), ["translation.variant_unknown"]);
}

#[test]
fn a_plural_form_the_language_needs_is_missing() {
    let fr = FR.replace("        many: \"{count} de colis\"\n", "");
    assert_eq!(rules(&[(FR_PATH, &fr)]), ["translation.variant_missing"]);
}

#[test]
fn a_word_that_is_no_plural_form_is_unknown() {
    let en = EN.replace("other: \"{count} parcels\"", "plenty: \"{count} parcels\"");
    assert_eq!(
        rules(&[(EN_PATH, &en)]),
        ["translation.variant_missing", "translation.variant_unknown"]
    );
}

// --- completeness ---------------------------------------------------------------------------

#[test]
fn a_message_with_no_wording_in_a_supported_locale_is_missing_there() {
    let fr = FR.replace(
        "    parcels: {\n        one: \"{count} colis\"\n        many: \"{count} de colis\"\n        other: \"{count} colis\"\n    }\n",
        "",
    );
    let (path, text, rule) = pointed_at(&[(FR_PATH, &fr)]);
    assert_eq!(
        (path.as_str(), text.as_str(), rule.as_str()),
        (FR_PATH, "translations", "translation.wording_missing")
    );
}

#[test]
fn a_locale_in_progress_falls_back_with_no_diagnostic() {
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]\n",
        "supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: [Locale.fr_fr]\n",
    );
    // No parcels wording, and no `casual` for the order.
    let fr = r#"translations {
    order_shipped: {
        formal: "Bonjour {name}, votre commande a été expédiée."
    }
}
"#;
    assert_eq!(rules(&[("app.jadpo", &app), (FR_PATH, fr)]), CLEAN);
    // Or no file at all.
    assert_eq!(rules(&[("app.jadpo", &app), (FR_PATH, "")]), CLEAN);
}

#[test]
fn a_locale_in_progress_still_has_its_mistakes_reported() {
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]\n",
        "supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: [Locale.fr_fr]\n",
    );
    let fr = "translations {\n    order_shiped: \"x\"\n}\n";
    assert_eq!(
        rules(&[("app.jadpo", &app), (FR_PATH, fr)]),
        ["translation.member_unknown"]
    );
}

#[test]
fn an_in_progress_locale_is_a_supported_one() {
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]\n",
        "supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: [Locale.de_de]\n",
    );
    assert_eq!(
        rules(&[("app.jadpo", &app)]),
        ["type.locale_in_progress_unsupported"]
    );
}

#[test]
fn the_default_locale_is_never_in_progress() {
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]\n",
        "supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: [Locale.en_gb]\n",
    );
    assert_eq!(
        rules(&[("app.jadpo", &app)]),
        ["type.locale_in_progress_default"]
    );
}

#[test]
fn in_progress_is_a_list_of_locales_listed_once() {
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]\n",
        "supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: [Locale.fr_fr, Locale.fr_fr]\n",
    );
    assert_eq!(rules(&[("app.jadpo", &app)]), ["type.locale_duplicate"]);
    let app = APP.replace(
        "supported: [Locale.en_gb, Locale.fr_fr]\n",
        "supported: [Locale.en_gb, Locale.fr_fr]\n    in_progress: Locale.fr_fr\n",
    );
    assert_eq!(rules(&[("app.jadpo", &app)]), ["type.locale_invalid"]);
}

// --- the placeholders --------------------------------------------------------------------------

#[test]
fn a_placeholder_that_is_not_a_field_is_unknown() {
    let en = EN.replace("Hi {name}", "Hi {nmae}");
    let (path, text, rule) = pointed_at(&[(EN_PATH, &en)]);
    assert_eq!(
        (path.as_str(), text.as_str(), rule.as_str()),
        (EN_PATH, "{nmae}", "translation.placeholder_unknown")
    );
}

#[test]
fn the_field_that_chooses_the_wording_is_not_a_placeholder() {
    let en = EN.replace("Hi {name}", "Hi {name} ({tone})");
    assert_eq!(
        rules(&[(EN_PATH, &en)]),
        ["translation.placeholder_unknown"]
    );
}

#[test]
fn a_wording_is_a_plain_string() {
    for (broken, text) in [
        ("unclosed", "Hi {name, your order"),
        ("lonely", "Hi } there"),
        ("empty", "Hi {} there"),
        ("icu", "{count, plural, one {parcel} other {parcels}}"),
        ("spaced", "Hi { name }"),
    ] {
        let en = EN.replace("Hi {name}, your order is on its way!", text);
        assert_eq!(
            rules(&[(EN_PATH, &en)]),
            ["translation.wording_invalid"],
            "{broken}"
        );
    }
}

#[test]
fn a_wording_error_points_at_the_character_after_an_escape() {
    // The tab is one character of the wording and two of the file.
    let en = EN.replace("Hi {name}, your order is on its way!", "A\\tB {name");
    let (path, text, rule) = pointed_at(&[(EN_PATH, &en)]);
    assert_eq!(
        (path.as_str(), text.as_str(), rule.as_str()),
        (EN_PATH, "{name", "translation.wording_invalid")
    );
}

#[test]
fn a_field_no_wording_of_a_language_writes_is_a_judgement_call() {
    let en = EN
        .replace("Good day {name}", "Good day")
        .replace("Hi {name}", "Hi");
    let (path, text, rule) = pointed_at(&[(EN_PATH, &en)]);
    assert_eq!(
        (path.as_str(), text.as_str(), rule.as_str()),
        (EN_PATH, "order_shipped", "translation.field_unused")
    );
}

#[test]
fn a_field_unused_can_be_declined_with_a_reason() {
    let en = EN
        .replace("Good day {name}", "Good day")
        .replace("Hi {name}", "Hi");
    let en = en.replace(
        "    order_shipped: {",
        "    decline translation.field_unused: \"English addresses no one by name\"\n    order_shipped: {",
    );
    assert_eq!(rules(&[(EN_PATH, &en)]), CLEAN);
}

#[test]
fn a_plural_count_a_language_does_not_write_is_unused_too() {
    let en = EN
        .replace("{count} parcels", "parcels")
        .replace("{count} parcel", "parcel");
    assert_eq!(rules(&[(EN_PATH, &en)]), ["translation.field_unused"]);
}

// --- the messages as declared ------------------------------------------------------------------------

#[test]
fn a_field_a_wording_cannot_hold_is_an_error() {
    let message = "/// When it left.\ntype Left = Message {\n    when: Instant\n}\n";
    assert_eq!(
        rules(&[("domains/shipping/translations/left.jadpo", message)]),
        ["translation.field_type"]
    );
    let message = "/// Maybe a name.\ntype Greeting = Message {\n    name: Text?\n}\n";
    assert_eq!(
        rules(&[("domains/shipping/translations/greeting.jadpo", message)]),
        ["translation.field_type"]
    );
}

#[test]
fn two_fields_that_choose_the_wording_are_an_error() {
    let message = "use shipping/types { Tone }\n\n/// A grid.\ntype Grid = Message {\n    tone: Tone\n    count: PluralCount\n}\n";
    assert_eq!(
        rules(&[("domains/shipping/translations/grid.jadpo", message)]),
        ["translation.too_many_selectors"]
    );
}

#[test]
fn a_type_built_on_text_or_a_number_is_a_placeholder() {
    let message = "use shipping/types { CustomerName, ParcelCount }\n\n/// A line.\ntype Line = Message {\n    name: CustomerName\n    boxes: ParcelCount\n    shown: PresentationText\n    price: Decimal\n}\n";
    let en = EN.replace(
        "parcels: {",
        "line: \"{name} has {boxes} boxes costing {price}: {shown}\"\n    parcels: {",
    );
    let fr = FR.replace(
        "parcels: {",
        "line: \"{name} a {boxes} colis à {price} : {shown}\"\n    parcels: {",
    );
    assert_eq!(
        rules(&[
            ("domains/shipping/translations/line.jadpo", message),
            (EN_PATH, &en),
            (FR_PATH, &fr),
        ]),
        CLEAN
    );
}

/// A message with an amount of money, worded in both languages (area 19,
/// question 16, chunk 5.5h2), and an action calling it.
fn priced(en_wording: &str, fr_wording: &str, call_with: &str) -> Vec<String> {
    let price =
        "/// A price for people.\ntype CakePrice = Money {\n    currency: Currency.gbp\n}\n";
    let message = "use shipping/types { CakePrice, ParcelCount }\n\n/// What a basket costs, on the checkout page.\ntype BasketTotal = Message {\n    total: CakePrice\n    boxes: ParcelCount\n}\n";
    let caller = format!(
        "use shipping/types {{ CakePrice, ParcelCount }}\n\n/// The basket's total, in the customer's language.\naction checkout_line(total: {call_with}, boxes: ParcelCount, reader_locale: Locale) -> PresentationText {{\n    return translations.shipping.basket_total(total: total, boxes: boxes, locale: reader_locale)\n}}\n"
    );
    let en = EN.replace(
        "parcels: {",
        &format!("basket_total: \"{en_wording}\"\n    parcels: {{"),
    );
    let fr = FR.replace(
        "parcels: {",
        &format!("basket_total: \"{fr_wording}\"\n    parcels: {{"),
    );
    rules(&[
        ("domains/shipping/types/cake_price.jadpo", price),
        ("domains/shipping/translations/basket_total.jadpo", message),
        ("domains/shipping/actions/checkout_line.jadpo", &caller),
        (EN_PATH, &en),
        (FR_PATH, &fr),
    ])
}

#[test]
fn an_amount_of_money_is_written_with_its_style() {
    assert_eq!(
        priced(
            "{boxes} boxes: {total, number, currency}",
            "{boxes} colis : {total, number, currency}",
            "CakePrice"
        ),
        CLEAN
    );
    assert_eq!(
        priced(
            "{boxes} boxes: {total}",
            "{boxes} colis : {total, number, currency}",
            "CakePrice"
        ),
        ["translation.placeholder_style"],
        "money without its style"
    );
    assert_eq!(
        priced(
            "{boxes, number, currency} boxes: {total, number, currency}",
            "{boxes} colis : {total, number, currency}",
            "CakePrice"
        ),
        ["translation.placeholder_style"],
        "a number written as money"
    );
    assert_eq!(
        priced(
            "{boxes} boxes: {total, number, percent}",
            "{boxes} colis : {total, number, currency}",
            "CakePrice"
        ),
        ["translation.wording_invalid"],
        "the one style there is"
    );
    assert_eq!(
        priced(
            "{boxes} boxes: {total, number, currency}",
            "{boxes} colis : {total, number, currency}",
            "ParcelCount"
        ),
        ["import.unused", "type.mismatch"],
        "a call gives the message an amount, and `CakePrice` goes unused"
    );
}

#[test]
fn an_enum_with_data_gives_its_values_fields_to_their_wordings() {
    let status = "use shipping/types { CustomerName }\n\ntype OrderStatus = Enum {\n    pending\n    held: {\n        reason: CustomerName\n    }\n}\n";
    let used = CALLER
        .replace(
            "CustomerName, ParcelCount",
            "CustomerName, OrderStatus, ParcelCount",
        )
        .replace("tone: Tone,", "tone: Tone, status: OrderStatus,")
        .replace(
            "parcels: translations.shipping.parcels(count: count, locale: reader_locale)",
            "parcels: translations.shipping.order_status(status, locale: reader_locale)",
        );
    let en = EN.replace(
        "parcels: {",
        "order_status: {\n        pending: \"Waiting to be packed\"\n        held: \"Held: {reason}\"\n    }\n    parcels: {",
    );
    let fr = FR.replace(
        "parcels: {",
        "order_status: {\n        pending: \"En attente\"\n        held: \"Suspendue : {reason}\"\n    }\n    parcels: {",
    );
    assert_eq!(
        rules(&[
            ("domains/shipping/types/order_status.jadpo", status),
            ("domains/shipping/actions/describe_order.jadpo", &used),
            (EN_PATH, &en),
            (FR_PATH, &fr),
        ]),
        CLEAN
    );
    // A field of another value is not available.
    let en = en.replace(
        "pending: \"Waiting to be packed\"",
        "pending: \"Waiting: {reason}\"",
    );
    assert_eq!(
        rules(&[
            ("domains/shipping/types/order_status.jadpo", status),
            ("domains/shipping/actions/describe_order.jadpo", &used),
            (EN_PATH, &en),
            (FR_PATH, &fr),
        ]),
        ["translation.placeholder_unknown"]
    );
}

#[test]
fn an_enum_is_worded_in_every_locale_once_any_language_words_it() {
    let en = EN.replace(
        "parcels: {",
        "tone: {\n        formal: \"Formal\"\n        casual: \"Casual\"\n    }\n    parcels: {",
    );
    let (path, text, rule) = pointed_at(&[(EN_PATH, &en)]);
    assert_eq!(
        (path.as_str(), text.as_str(), rule.as_str()),
        (FR_PATH, "translations", "translation.wording_missing")
    );
}

#[test]
fn an_enum_nobody_words_needs_no_wording() {
    assert_eq!(rules(&[]), CLEAN);
}

// --- email subjects ----------------------------------------------------------------------------------

fn email_files() -> Vec<(&'static str, String)> {
    vec![
        ("domains/accounts/emails/welcome_email.jadpo", "/// Sent when someone signs up.\ntype WelcomeEmail = EmailMessage {\n    display_name: Text\n}\n".to_string()),
        (
            "domains/accounts/translations/en_gb.jadpo",
            "translations {\n    welcome_email: \"Welcome, {display_name}\"\n}\n".to_string(),
        ),
        (
            "domains/accounts/translations/fr_fr.jadpo",
            "translations {\n    welcome_email: \"Bienvenue, {display_name}\"\n}\n".to_string(),
        ),
    ]
}

#[test]
fn an_emails_member_is_its_subject_with_the_emails_fields_as_placeholders() {
    let owned = email_files();
    let borrowed: Vec<(&str, &str)> = owned.iter().map(|(p, t)| (*p, t.as_str())).collect();
    assert_eq!(rules(&borrowed), CLEAN);
    let mut bad = owned.clone();
    bad[1].1 = "translations {\n    welcome_email: \"Welcome, {name}\"\n}\n".to_string();
    let borrowed: Vec<(&str, &str)> = bad.iter().map(|(p, t)| (*p, t.as_str())).collect();
    assert_eq!(rules(&borrowed), ["translation.placeholder_unknown"]);
}

// --- shared messages ---------------------------------------------------------------------------------

#[test]
fn a_message_in_shared_translations_is_called_through_shared() {
    let files = [
        (
            "shared/translations/saved.jadpo",
            "/// Shown after a save.\ntype Saved = Message {\n}\n",
        ),
        (
            "shared/translations/en_gb.jadpo",
            "translations {\n    saved: \"Saved.\"\n}\n",
        ),
        (
            "shared/translations/fr_fr.jadpo",
            "translations {\n    saved: \"Enregistré.\"\n}\n",
        ),
        (
            "domains/shipping/types/note.jadpo",
            "/// A note.\ntype Note = Output {\n    text: PresentationText\n}\n",
        ),
        (
            "domains/shipping/actions/save_note.jadpo",
            "use shipping/types { Note }\n\n/// Says it saved.\naction save_note(reader_locale: Locale) -> Note {\n    return Note {\n        text: translations.shared.saved(locale: reader_locale)\n    }\n}\n",
        ),
    ];
    assert_eq!(rules(&files), CLEAN);
}

// --- the calls ---------------------------------------------------------------------------------------

fn with_call(call: &str) -> Vec<String> {
    let action = CALLER.replace(
        "translations.shipping.order_shipped(tone: tone, name: name, locale: reader_locale)",
        call,
    );
    rules(&[("domains/shipping/actions/describe_order.jadpo", &action)])
}

#[test]
fn a_call_names_a_message_of_a_domain() {
    assert_eq!(
        with_call(
            "translations.shipping.order_shiped(tone: tone, name: name, locale: reader_locale)"
        ),
        ["translation.message_unknown"]
    );
    assert_eq!(
        with_call(
            "translations.shiping.order_shipped(tone: tone, name: name, locale: reader_locale)"
        ),
        ["translation.message_unknown"]
    );
    assert_eq!(
        with_call("translations.order_shipped(tone: tone, name: name, locale: reader_locale)"),
        ["translation.message_unknown"]
    );
}

#[test]
fn a_call_gives_every_field_by_name_and_a_locale() {
    assert_eq!(
        with_call("translations.shipping.order_shipped(tone: tone, name: name)"),
        ["type.argument_count"]
    );
    assert_eq!(
        with_call(
            "translations.shipping.order_shipped(tone: tone, name: name, locale: reader_locale, extra: name)"
        ),
        ["type.named_argument_unknown"]
    );
}

#[test]
fn a_calls_values_are_the_kinds_the_fields_take() {
    // A choice field takes its enum, a plain text field text.
    assert_eq!(
        with_call(
            "translations.shipping.order_shipped(tone: name, name: name, locale: reader_locale)"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        with_call(
            "translations.shipping.order_shipped(tone: tone, name: count, locale: reader_locale)"
        ),
        ["type.mismatch"]
    );
}

#[test]
fn a_calls_locale_is_a_supported_one() {
    assert_eq!(
        with_call(
            "translations.shipping.order_shipped(tone: tone, name: name, locale: Locale.de_de)"
        ),
        ["type.locale_unsupported"]
    );
}

#[test]
fn a_message_of_another_domain_is_private_to_it() {
    let action = "use billing/types { Note }\nuse shipping/types { Tone }\n\n/// Tries another domain's message.\naction ask(tone: Tone) -> Note {\n    return Note {\n        text: translations.shipping.order_shipped(tone: tone, name: \"Ana\", locale: Locale.en_gb)\n    }\n}\n";
    let note = "/// A note.\ntype Note = Output {\n    text: PresentationText\n}\n";
    let found = rules(&[
        ("domains/billing/types/note.jadpo", note),
        ("domains/billing/actions/ask.jadpo", action),
    ]);
    assert!(
        found.contains(&"visibility.private_cross_domain".to_string()),
        "{found:?}"
    );
}

#[test]
fn an_enum_with_wording_is_called_with_its_value() {
    let en = EN.replace(
        "parcels: {",
        "tone: {\n        formal: \"Formal\"\n        casual: \"Casual\"\n    }\n    parcels: {",
    );
    let fr = FR.replace(
        "parcels: {",
        "tone: {\n        formal: \"Formel\"\n        casual: \"Familier\"\n    }\n    parcels: {",
    );
    let action = CALLER.replace(
        "parcels: translations.shipping.parcels(count: count, locale: reader_locale)",
        "parcels: translations.shipping.tone(tone, locale: reader_locale)",
    );
    assert_eq!(
        rules(&[
            (EN_PATH, &en),
            (FR_PATH, &fr),
            ("domains/shipping/actions/describe_order.jadpo", &action),
        ]),
        CLEAN
    );
}

#[test]
fn an_email_subject_is_not_called() {
    let mut owned = email_files();
    owned.push((
        "domains/accounts/types/line.jadpo",
        "/// A line.\ntype Line = Output {\n    text: PresentationText\n}\n".to_string(),
    ));
    owned.push((
        "domains/accounts/actions/greet.jadpo",
        "use accounts/types { Line }\n\n/// Greets.\naction greet() -> Line {\n    return Line {\n        text: translations.accounts.welcome_email(display_name: \"Ana\", locale: Locale.en_gb)\n    }\n}\n".to_string(),
    ));
    let borrowed: Vec<(&str, &str)> = owned.iter().map(|(p, t)| (*p, t.as_str())).collect();
    assert_eq!(rules(&borrowed), ["translation.message_unknown"]);
}
