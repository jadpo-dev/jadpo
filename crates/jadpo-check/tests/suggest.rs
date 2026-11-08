//! `jadpo suggest`'s scan (area 14, "Suggestions"; chunk 5.5x): each kind
//! of signal, read from the standard types' entries and the loop table,
//! finds what it should and leaves alone what it shouldn't, through
//! canonical fragments that make the ways of writing one thing one
//! pattern.

use jadpo_check::suggest::{self, Finding, Signal, Taught};
use jadpo_db::{Database, Layout};

use crate::support;

fn scan_with(text: &str, taught: &[Taught]) -> Vec<Finding> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("shop.jadpo", text)]);
    suggest::scan(&db, project, taught, &[])
}

fn scan(text: &str) -> Vec<Finding> {
    scan_with(text, &[])
}

/// Each finding as `signal suggestion: shown`.
fn rows(text: &str) -> Vec<String> {
    scan(text)
        .iter()
        .map(|f| format!("{} {}: {}", f.signal.word(), f.suggestion, f.snippet.shown))
        .collect()
}

/// The findings of one signal.
fn of(signal: Signal, text: &str) -> Vec<Finding> {
    scan(text)
        .into_iter()
        .filter(|f| f.signal == signal)
        .collect()
}

/// [`rows`] of one signal.
fn rows_of(signal: Signal, text: &str) -> Vec<String> {
    rows(text)
        .into_iter()
        .filter(|row| row.starts_with(&format!("{} ", signal.word())))
        .collect()
}

fn problems(text: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("shop.jadpo", text)])
}

#[test]
fn a_conversion_by_a_types_constant_on_a_name_it_goes_by_is_found() {
    let text = "/// A whole number.\ntype Whole = Int {}\n\n/// Pence from a price in pence.\nfunction pence_of(total_price: Whole) -> Whole {\n    return Whole(total_price * 100)\n}\n";
    assert_eq!(problems(text), Vec::<String>::new());
    assert_eq!(
        rows(text),
        [
            "name Money: total_price: Whole",
            "conversion Money: total_price * 100"
        ]
    );
}

#[test]
fn a_conversion_needs_both_the_constant_and_the_name() {
    let text = "/// A whole number.\ntype Whole = Int {}\n\n/// Seats.\nfunction seats_of(count: Whole) -> Whole {\n    return Whole(count * 100)\n}\n\n/// Odd.\nfunction odd_of(total_price: Whole) -> Whole {\n    return Whole(total_price * 7)\n}\n";
    assert_eq!(rows_of(Signal::Conversion, text), Vec::<String>::new());
}

#[test]
fn a_value_already_of_the_type_is_left_alone() {
    let text = "/// A wait.\ntype Wait = Duration {}\n\n/// Longer.\nfunction later(timeout_ms: Wait) -> Wait {\n    return Wait(timeout_ms * 1000)\n}\n";
    assert_eq!(rows(text), Vec::<String>::new());
}

#[test]
fn operand_order_and_exact_equivalents_are_one_pattern() {
    let forms = [
        "total_price * 100",
        "100 * total_price",
        "(total_price * 10) * 10",
    ];
    let hashes: Vec<String> = forms
        .iter()
        .map(|form| {
            let text = format!("/// A whole number.\ntype Whole = Int {{}}\n\n/// Pence.\nfunction pence_of(total_price: Whole) -> Whole {{\n    return Whole({form})\n}}\n");
            let found = of(Signal::Conversion, &text);
            assert_eq!(found.len(), 1, "{form}: {found:?}");
            found[0].snippet.hash.clone()
        })
        .collect();
    assert!(hashes.iter().all(|h| *h == hashes[0]), "{hashes:?}");
    // Dividing by a hundredth is multiplying by a hundred.
    let text = "/// An exact number.\ntype Exact = Decimal {\n    scale: 4\n}\n\n/// Pence.\nfunction pence_of(total_price: Exact) -> Exact {\n    return Exact(total_price / 0.01)\n}\n";
    let decimal = of(Signal::Conversion, text);
    assert_eq!(decimal.len(), 1);
    assert_eq!(decimal[0].snippet.canonical, "(* 100 total_price)");
}

#[test]
fn a_temporary_is_followed_to_its_value() {
    let text = "/// A whole number.\ntype Whole = Int {}\n\n/// Pence.\nfunction pence_of(total_price: Whole) -> Whole {\n    var factor: Int = 100\n    return Whole(total_price * factor)\n}\n";
    let found = of(Signal::Conversion, text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].suggestion, "Money");
    assert_eq!(found[0].snippet.text, "total_price * factor");
    assert_eq!(found[0].snippet.canonical, "(* 100 total_price)");
    // A named constant is its value too.
    let constant = "/// A whole number.\ntype Whole = Int {}\n\n/// A hundred.\nconstant hundred: Whole = 100\n\n/// Pence.\nfunction pence_of(total_price: Whole) -> Whole {\n    return Whole(hundred * total_price)\n}\n";
    let found = of(Signal::Conversion, constant);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].snippet.canonical, "(* 100 total_price)");
}

#[test]
fn a_quantitys_units_give_its_conversions_and_names() {
    let text = "/// An exact number.\ntype Exact = Decimal {\n    scale: 4\n}\n\n/// Metres.\nfunction metres_of(distance_km: Exact) -> Exact {\n    return Exact(distance_km * 1000)\n}\n";
    assert_eq!(
        rows(text),
        [
            "name Length: distance_km: Exact",
            "conversion Length: distance_km * 1000"
        ]
    );
}

#[test]
fn a_name_on_a_built_in_type_is_found() {
    let text = "/// A delivery.\ntype Delivery = Object {\n    timeout_ms: Int\n    count: Int\n    contact_email: Text\n}\n";
    assert_eq!(
        rows(text),
        [
            "name Duration: timeout_ms: Int",
            "name Email: contact_email: Text"
        ]
    );
    let typed =
        "/// A delivery.\ntype Delivery = Object {\n    timeout: Duration\n    count: Int\n}\n";
    assert_eq!(rows(typed), Vec::<String>::new());
}

#[test]
fn a_parameter_and_a_type_are_named_too() {
    let text = "/// Pence.\ntype PricePence = Int {}\n\n/// A whole number.\ntype Whole = Int {}\n\n/// Waits.\nfunction wait(delay_seconds: Whole, price: PricePence) -> Whole {\n    return delay_seconds\n}\n";
    assert_eq!(
        rows(text),
        [
            "name Money: type PricePence = Int",
            "name Duration: delay_seconds: Whole"
        ]
    );
}

#[test]
fn a_type_named_only_for_a_unit_is_left_to_check() {
    // `type.unit_as_number` reports it (area 2, "Units and money in plain
    // numbers"), as `type.standard_exists` reports exactly a shape.
    let text = "/// Pence.\ntype Pence = Int {}\n\n/// Grams.\ntype Grams = Int {}\n\n/// A parcel.\ntype Parcel = Object {\n    weight: Grams\n    price: Pence\n}\n";
    assert_eq!(rows(text), Vec::<String>::new());
}

#[test]
fn a_name_holding_percent_is_a_percentage() {
    let text = "/// A sale.\ntype Sale = Object {\n    discount_percent: Int\n}\n";
    assert_eq!(rows(text), ["name Percentage: discount_percent: Int"]);
}

#[test]
fn fields_holding_a_shape_among_others_are_found() {
    let text = "/// A shop.\ntype Shop = Object {\n    name: Text\n    latitude: Decimal\n    longitude: Decimal\n}\n";
    let found = scan(text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].signal, Signal::Shape);
    assert_eq!(found[0].suggestion, "Point");
    assert_eq!(found[0].snippet.shown, "latitude, longitude");
    let half = "/// A shop.\ntype Shop = Object {\n    name: Text\n    latitude: Decimal\n}\n";
    assert_eq!(rows(half), Vec::<String>::new());
}

#[test]
fn a_symbol_joined_to_a_value_is_found() {
    let text = "/// A whole number.\ntype Whole = Int {}\n\n/// Text for people.\ntype Label = Text {}\n\n/// A price for people.\nfunction shown(amount: Whole) -> Label {\n    return Label(\"£\" + text.from(amount))\n}\n";
    assert_eq!(
        rows(text).first().map(String::as_str),
        Some("format Money: \"£\" + text.from(amount)")
    );
    let plain = "/// A whole number.\ntype Whole = Int {}\n\n/// Text for people.\ntype Label = Text {}\n\n/// A label.\nfunction shown(count: Whole) -> Label {\n    return Label(\"Order \" + text.from(count))\n}\n";
    assert_eq!(rows(plain), Vec::<String>::new());
}

#[test]
fn a_pattern_that_is_a_types_validation_is_found() {
    let text = "/// An address.\ntype ContactAddress = Text {\n    pattern: \"[^@ ]+@[^@ ]+\\\\.[a-z.]+\"\n}\n";
    let found = scan(text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].signal, Signal::Validation);
    assert_eq!(found[0].suggestion, "Email");
    let code =
        "/// A batch code.\ntype BatchCode = Text {\n    pattern: \"[A-Z]{2}-[0-9]{3}\"\n}\n";
    assert_eq!(rows(code), Vec::<String>::new());
}

#[test]
fn a_pattern_in_matches_is_read_too() {
    let text = "/// Text.\ntype Line = Text {}\n\n/// Whether it looks like an identifier.\nfunction looks_right(written: Line) -> Bool {\n    return written matches \"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\"\n}\n";
    let found = of(Signal::Validation, text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].suggestion, "Uuid");
    let other = "/// Text.\ntype Line = Text {}\n\n/// Whether it looks like a code.\nfunction looks_right(written: Line) -> Bool {\n    return written matches \"[A-Z]+\"\n}\n";
    assert_eq!(of(Signal::Validation, other), Vec::<Finding>::new());
}

#[test]
fn a_loop_an_operation_replaces_is_found() {
    let text = "/// An item.\ntype Item = Object {\n    weight: Int\n}\n\n/// Items.\ntype Items = List<Item> {}\n\n/// A whole number.\ntype Whole = Int {}\n\n/// The total.\nfunction total(items: Items) -> Whole {\n    var mut sum: Int = 0\n    for item in items {\n        sum = sum + item.weight\n    }\n    return Whole(sum)\n}\n";
    assert_eq!(problems(text), Vec::<String>::new());
    assert_eq!(rows(text), ["loop list.sum: for item in items { …"]);
    let lightest = "/// An item.\ntype Item = Object {\n    weight: Int\n}\n\n/// Items.\ntype Items = List<Item> {}\n\n/// A whole number.\ntype Whole = Int {}\n\n/// The lightest.\nfunction lightest(items: Items, first: Item) -> Item {\n    var mut best: Item = first\n    for item in items {\n        if (best.weight > item.weight) {\n            best = item\n        }\n    }\n    return best\n}\n";
    assert_eq!(problems(lightest), Vec::<String>::new());
    assert_eq!(rows(lightest), ["loop list.min_by: for item in items { …"]);
    let busy = "/// An item.\ntype Item = Object {\n    weight: Int\n}\n\n/// Items.\ntype Items = List<Item> {}\n\n/// A whole number.\ntype Whole = Int {}\n\n/// Not a sum.\nfunction busy(items: Items) -> Whole {\n    var mut sum: Int = 0\n    var mut seen: Int = 0\n    for item in items {\n        sum = sum + item.weight\n        seen = seen + 1\n    }\n    return Whole(sum + seen)\n}\n";
    assert_eq!(rows(busy), Vec::<String>::new());
}

#[test]
fn a_taught_snippet_is_found_by_the_next_scan() {
    let text = "/// A whole number.\ntype Whole = Int {}\n\n/// Total.\nfunction total(subtotal: Whole, delivery: Whole) -> Whole {\n    return Whole(subtotal + delivery)\n}\n";
    assert_eq!(rows(text), Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("shop.jadpo", text)]);
    let snippet = suggest::snippet_at(&db, project, "shop.jadpo", 6).expect("a snippet");
    assert_eq!(snippet.text, "subtotal + delivery");
    let taught = [Taught {
        hash: snippet.hash.clone(),
        canonical: snippet.canonical.clone(),
        suggestion: "Money".into(),
        reason: "Pence held as Int".into(),
    }];
    // Written the other way round, elsewhere.
    let elsewhere = "/// A whole number.\ntype Whole = Int {}\n\n/// Total.\nfunction sum_of(subtotal: Whole, delivery: Whole) -> Whole {\n    var all: Int = delivery + subtotal\n    return Whole(all)\n}\n";
    let found = scan_with(elsewhere, &taught);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].signal, Signal::Taught);
    assert_eq!(found[0].suggestion, "Money");
    assert_eq!(found[0].snippet.text, "delivery + subtotal");
    // Another pattern isn't the taught one.
    let other = "/// A whole number.\ntype Whole = Int {}\n\n/// Change.\nfunction change(subtotal: Whole, delivery: Whole) -> Whole {\n    return Whole(subtotal - delivery)\n}\n";
    assert_eq!(scan_with(other, &taught), Vec::<Finding>::new());
}

#[test]
fn the_catalogue_is_generated_from_the_entries() {
    let catalogue = suggest::catalogue();
    let has = |suggestion: &str, signal: Signal| {
        catalogue
            .iter()
            .any(|entry| entry.suggestion == suggestion && entry.signal == signal)
    };
    assert!(has("Money", Signal::Conversion));
    assert!(has("Money", Signal::Format));
    assert!(has("Point", Signal::Shape));
    assert!(has("Duration", Signal::Name));
    assert!(has("Uuid", Signal::Validation));
    assert!(has("Length", Signal::Conversion));
    assert!(has("list.sum", Signal::Loop));
    assert!(has(suggest::CATALOGUE_OPERATIONS, Signal::Function));
    assert!(has(suggest::CATALOGUE_PACKAGES, Signal::Published));
}

// --- functions that already exist (roadmap R8) ------------------------------

/// Text and a yes or no, as a signature has to name them.
const WORDS: &str = "/// Some text.\ntype Words = Text {}\n\n/// Yes or no.\ntype Flag = Bool {}\n\n/// A whole number.\ntype Whole = Int {}\n\n";

#[test]
fn a_function_named_for_a_standard_operation_that_takes_what_it_does_is_found() {
    let text = format!(
        "{WORDS}/// A URL slug.\nfunction slug(title: Words) -> Words {{\n    return Words(text.lower(title))\n}}\n\n/// Whether it starts so.\nfunction text_starts_with(value: Words, prefix: Words) -> Flag {{\n    return Flag(text.slice(value, 0, 1) == prefix)\n}}\n"
    );
    assert_eq!(problems(&text), Vec::<String>::new());
    assert_eq!(
        rows_of(Signal::Function, &text),
        [
            "function text.slug: function slug(title: Words) -> Words",
            "function text.starts_with: function text_starts_with(value: Words, prefix: Words) -> Flag",
        ]
    );
    let found = of(Signal::Function, &text);
    assert_eq!(found[0].snippet.line, 11);
    assert!(
        found[0].reason.contains("`text.slug`"),
        "{}",
        found[0].reason
    );
    // In a library, by its own name.
    let library = format!(
        "{WORDS}/// Text helpers.\nlibrary titles {{\n    /// A URL slug.\n    function slug(title: Words) -> Words {{\n        return Words(text.lower(title))\n    }}\n}}\n"
    );
    assert_eq!(problems(&library), Vec::<String>::new());
    assert_eq!(
        rows_of(Signal::Function, &library),
        ["function text.slug: function slug(title: Words) -> Words"]
    );
}

#[test]
fn a_function_with_another_name_or_shape_is_left_alone() {
    // More in its name, a parameter the operation doesn't take, or more
    // parameters than it has: none is the standard operation.
    let text = format!(
        "{WORDS}/// A slug and more.\nfunction slug_of(title: Words) -> Words {{\n    return Words(text.lower(title))\n}}\n\n/// By number.\nfunction slug(id: Whole) -> Words {{\n    return Words(text.from(id))\n}}\n\n/// Two.\nfunction trim(a: Words, b: Words) -> Words {{\n    return Words(a + b)\n}}\n"
    );
    assert_eq!(problems(&text), Vec::<String>::new());
    assert_eq!(rows_of(Signal::Function, &text), Vec::<String>::new());
}

#[test]
fn a_copy_of_a_published_function_is_found_by_its_source_fingerprint() {
    use jadpo_check::suggest::functions::{self, Published};

    // The package's function, as its author wrote it.
    let package = format!(
        "{WORDS}/// Postcode helpers.\nlibrary uk {{\n    /// The outward half of a postcode.\n    public function outward(written: Words) -> Words {{\n        var cleaned: Text = text.upper(text.trim(written))\n        return Words(text.keep_first(cleaned, 4))\n    }}\n}}\n"
    );
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("uk.jadpo", &package)]);
    let own = functions::own_functions(&db, project);
    assert_eq!(own.len(), 1);
    assert!(own[0].source.starts_with("sha256:"), "{}", own[0].source);
    let published = [Published {
        package: "@acme/postcodes".into(),
        version: "2.1.0".into(),
        declaration: "uk.outward".into(),
        source: own[0].source.clone(),
    }];
    let hex = &own[0].source["sha256:".len()..];
    assert_eq!(functions::prefixes(&own), [hex[..5].to_string()]);

    // The app's copy: renamed, its variables too, reformatted and with a
    // description of its own, is the same function.
    let copy = format!(
        "{WORDS}/// Reads the outward code.\nfunction outward_code(postcode: Words) -> Words {{\n    var tidy: Text = text.upper(text.trim(postcode))\n    return Words(text.keep_first(tidy,   4))\n}}\n\n/// Not a copy.\nfunction inward_code(postcode: Words) -> Words {{\n    return Words(text.keep_last(postcode, 3))\n}}\n"
    );
    assert_eq!(problems(&copy), Vec::<String>::new());
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("shop.jadpo", &copy)]);
    let found: Vec<Finding> = suggest::scan(&db, project, &[], &published)
        .into_iter()
        .filter(|f| f.signal == Signal::Published)
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].suggestion, "@acme/postcodes uk.outward");
    assert_eq!(
        found[0].package,
        Some(("@acme/postcodes".to_string(), "2.1.0".to_string()))
    );
    assert_eq!(
        found[0].snippet.shown,
        "function outward_code(postcode: Words) -> Words"
    );
    assert!(found[0].reason.contains("copy"), "{}", found[0].reason);
    // Not asked, nothing is published.
    assert!(
        suggest::scan(&db, project, &[], &[])
            .iter()
            .all(|f| f.signal != Signal::Published)
    );
}
