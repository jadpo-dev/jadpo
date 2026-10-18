//! Area 19, "People and contact" and "Places", question 22 (chunk 5.5l):
//! the closed lists as built-in enums, their variants the authority's
//! codes, `.all` on every enum whose variants carry no data, `all` refused
//! as a variant's name, a withdrawn code read but refused where new code
//! writes it (`type.variant_retired`, declinable), `form:` on `text.parse`
//! and `text.from` for a list with data in that code system, and the
//! objects with optional parts, built, read and stored.

use crate::support;

use jadpo_db::{Database, Layout};

const NAMES: &str = "type Count = Int {}\n\ntype Wire = Text {}\n\ntype Answer = Bool {}\n\ntype Countries = List<Country> {}\n\ntype Shade = Enum {\n    plain\n    tinted\n}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_closed_list_s_variants_are_its_codes() {
    assert_eq!(
        rules("function home() -> Country {\n    return Country.gb\n}\n"),
        NONE
    );
    assert_eq!(
        rules("function india() -> Country {\n    return Country.in_\n}\n"),
        NONE,
        "a code that is a reserved word takes a trailing `_`"
    );
    assert_eq!(
        rules(
            "function england() -> CountrySubdivision {\n    return CountrySubdivision.gb_eng\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function odia() -> Language {\n    return Language.or_\n}\n"),
        NONE
    );
    assert_eq!(
        rules("function home() -> Country {\n    return Country.uk\n}\n"),
        ["type.unknown_enum_variant"],
        "`UK` isn't ISO 3166-1's code"
    );
    assert_eq!(
        rules(
            "function same(a: Country, b: Nationality) -> Answer {\n    return Answer(a == b)\n}\n"
        ),
        ["type.incomparable"],
        "a nationality is named by a country, but isn't one"
    );
}

#[test]
fn all_lists_an_enum_s_variants() {
    assert_eq!(
        rules("function every() -> Countries {\n    return Countries(Country.all)\n}\n"),
        NONE
    );
    assert_eq!(
        rules(
            "type Shades = List<Shade> {}\n\nfunction every() -> Shades {\n    return Shades(Shade.all)\n}\n"
        ),
        NONE,
        "an application's own enum has `.all` too"
    );
    assert_eq!(
        rules("function many() -> Count {\n    return Count(list.count(Weekday.all))\n}\n"),
        NONE
    );
    assert_eq!(
        rules(
            "type Payment = Enum {\n    cash\n    card: {\n        last_four: Wire\n    }\n}\n\ntype Payments = List<Payment> {}\n\nfunction every() -> Payments {\n    return Payments(Payment.all)\n}\n"
        ),
        ["type.unknown_enum_variant"],
        "a variant with data has no one value to list"
    );
    assert_eq!(
        rules("type Scope = Enum {\n    mine\n    all\n}\n"),
        ["type.reserved_variant_name"],
        "`all` lists an enum's variants, so no variant takes it"
    );
}

#[test]
fn a_retired_code_is_read_but_not_written() {
    let written = "function old() -> Country {\n    return Country.yu\n}\n";
    assert_eq!(rules(written), ["type.variant_retired"]);
    assert_eq!(
        rules("function kuna() -> Currency {\n    return Currency.hrk\n}\n"),
        ["type.variant_retired"],
        "the currencies ISO withdrew since 2021 too"
    );
    assert_eq!(
        rules(&format!(
            "decline type.variant_retired: \"Records from before 2003 name Yugoslavia\"\n{written}"
        )),
        NONE,
        "declined with a reason"
    );
    assert_eq!(
        rules(
            "function was_yugoslavia(country: Country) -> Answer {\n    return Answer(country == Country.yu)\n}\n"
        ),
        ["type.variant_retired"],
        "comparing with one is what a decline's reason is for"
    );
}

#[test]
fn form_reads_and_writes_another_code_system() {
    assert_eq!(
        rules(
            "function from_alpha3(raw: Wire) fails Unreadable -> Country {\n    var country: Country = attempt text.parse(raw, as: Country, form: CodeForm.alpha3) {\n        malformed: Unreadable\n    }\n    return country\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function numeric(country: Country) -> Wire {\n    return Wire(text.from(country, form: CodeForm.numeric))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function numeric(subdivision: CountrySubdivision) -> Wire {\n    return Wire(text.from(subdivision, form: CodeForm.numeric))\n}\n"
        ),
        ["type.temporal_option_unused"],
        "ISO 3166-2 has no numbers"
    );
    assert_eq!(
        rules(
            "function alpha3(language: Language) -> Wire {\n    return Wire(text.from(language, form: CodeForm.numeric))\n}\n"
        ),
        ["type.temporal_option_unused"]
    );
    assert_eq!(
        rules(
            "function shown(day: Weekday) -> Wire {\n    return Wire(text.from(day, form: CodeForm.alpha3))\n}\n"
        ),
        ["type.temporal_option_unused"]
    );
    assert_eq!(
        rules(
            "function chosen(country: Country, form: CodeForm) -> Wire {\n    return Wire(text.from(country, form: form))\n}\n"
        ),
        ["type.mismatch"],
        "a form is written out"
    );
}

#[test]
fn an_object_with_optional_parts_is_built_and_read() {
    let address = "function downing() -> PostalAddress {\n    return PostalAddress {\n        lines: [\"10 Downing Street\"]\n        locality: \"London\"\n        region: none\n        postal_code: \"SW1A 2AA\"\n        country: Country.gb\n    }\n}\n";
    assert_eq!(rules(address), NONE);
    assert_eq!(
        rules(&address.replace("        region: none\n", "")),
        ["type.missing_field"],
        "a part that may be `none` is still given"
    );
    assert_eq!(
        rules(&address.replace("\"London\"", "\" London\"")),
        ["type.invalid_literal"],
        "a part is one line, with no space at either end"
    );
    assert_eq!(
        rules(
            "function town(address: PostalAddress) -> Wire {\n    return Wire(address.locality)\n}\n"
        ),
        ["type.constructor_input"],
        "a part that may be `none` is matched first"
    );
    assert_eq!(
        rules(
            "function ada() -> PersonName {\n    return PersonName {\n        given: \"Ada\"\n        family: \"Lovelace\"\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function sukarno() -> PersonName {\n    return PersonName {\n        given: \"Sukarno\"\n        family: none\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function nobody() -> PersonName {\n    return PersonName {\n        given: \"\"\n        family: none\n    }\n}\n"
        ),
        ["type.invalid_literal"]
    );
}

#[test]
fn an_object_is_stored_in_a_column_per_part() {
    let db = Database::default();
    let source = "entity Customer {\n    id: Uuid\n    name: PersonName\n    address: PostalAddress\n    contact: EmailWithName?\n}\n";
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    let rules: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    assert_eq!(rules, NONE);
    let model = jadpo_check::data::data_model(&db, project);
    let customer = model.entity("Customer").expect("Customer");
    let columns: Vec<(String, String)> = customer
        .fields
        .iter()
        .flat_map(|field| field.columns())
        .collect();
    let named: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        named,
        [
            "id",
            "name_given",
            "name_family",
            "address_lines",
            "address_locality",
            "address_region",
            "address_postal_code",
            "address_country",
            "contact_name",
            "contact_address",
        ]
    );
    assert!(columns.contains(&("address_lines".to_string(), "text[]".to_string())));
}

#[test]
fn a_part_that_may_be_none_is_a_column_that_may_be_null() {
    let db = Database::default();
    let source = "entity Customer {\n    id: Uuid\n    address: PostalAddress\n}\n";
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    let schema = jadpo_check::data::schema(&db, project);
    let table = schema
        .stores
        .values()
        .flat_map(|store| store.tables.values())
        .next()
        .expect("a table");
    let nullable = |name: &str| table.columns.get(name).map(|column| column.nullable);
    assert_eq!(nullable("address_lines"), Some(false));
    assert_eq!(nullable("address_region"), Some(true));
    assert_eq!(nullable("address_country"), Some(false));
}
