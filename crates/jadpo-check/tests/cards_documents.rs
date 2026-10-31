//! Area 19, "Money and banking" and "Tax and identity numbers"
//! (chunk 5.5i2): the card types, with `money.mask` making a masked card
//! from a card number, and the identity documents, each number checked by
//! its country's form where a literal writes both, and each stored in a
//! column per part.

use crate::support;

use jadpo_db::{Database, Layout};

const NAMES: &str = "type Wire = Text {}\n\ntype Answer = Bool {}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_card_number_is_masked_to_its_brand_and_last_four() {
    assert_eq!(
        rules(
            "function kept(card: PaymentCardNumber) -> MaskedCardNumber {\n    return money.mask(card)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function kept(card: PaymentCardNumber) -> Answer {\n    return Answer(money.mask(card).brand == CardBrand.visa)\n}\n"
        ),
        NONE,
        "a masked card's brand is read as a `CardBrand`"
    );
    assert_eq!(
        rules("function kept(raw: Wire) -> MaskedCardNumber {\n    return money.mask(raw)\n}\n"),
        ["type.mismatch"],
        "only a card number is masked"
    );
}

#[test]
fn a_masked_card_and_an_expiry_are_built_and_compared() {
    let masked = "function on_file() -> MaskedCardNumber {\n    return MaskedCardNumber {\n        brand: CardBrand.visa\n        last_four: \"4242\"\n    }\n}\n";
    assert_eq!(rules(masked), NONE);
    assert_eq!(
        rules(&masked.replace("\"4242\"", "\"424\"")),
        ["type.invalid_literal"],
        "a last four is four digits"
    );
    assert_eq!(
        rules(&masked.replace("CardBrand.visa", "none")),
        NONE,
        "a card no listed scheme issued has no brand"
    );
    let expiry = "function runs_to() -> CardExpiry {\n    return CardExpiry {\n        year: 2028\n        month: 12\n    }\n}\n";
    assert_eq!(rules(expiry), NONE);
    assert_eq!(
        rules(&expiry.replace("2028", "1999")),
        ["type.invalid_literal"],
        "an expiry's year is from 2000 to 2099"
    );
    assert_eq!(
        rules(&expiry.replace("12\n", "13\n")),
        ["type.invalid_literal"]
    );
    assert_eq!(
        rules(
            "function expired(card: CardExpiry, now: CardExpiry) -> Answer {\n    return Answer(card < now)\n}\n"
        ),
        NONE,
        "expiries are ordered"
    );
}

#[test]
fn a_document_s_number_fits_its_country() {
    let passport = "function travel() -> PassportNumber {\n    return PassportNumber {\n        country: Country.gb\n        number: \"123456789\"\n    }\n}\n";
    assert_eq!(rules(passport), NONE);
    assert_eq!(
        rules(&passport.replace("Country.gb", "Country.fr")),
        ["type.invalid_literal"],
        "a French passport number is two digits, two letters and five digits"
    );
    assert_eq!(
        rules(&passport.replace("\"123456789\"", "\"1234 56789\"")),
        ["type.invalid_literal"],
        "a number is upper case with no spaces"
    );
    let vat = "function supplier() -> VatNumber {\n    return VatNumber {\n        country: Country.de\n        number: \"136695976\"\n    }\n}\n";
    assert_eq!(rules(vat), NONE);
    assert_eq!(
        rules(&vat.replace("136695976", "136695975")),
        ["type.invalid_literal"],
        "a German VAT number's check digit"
    );
    // Built from values, the rule is attempted by its name (chunk 5.5t2).
    assert_eq!(
        rules(
            "type Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction typed(raw: Wire, to: Country) fails Unreadable -> DrivingLicenceNumber {\n    var number: DrivingLicenceNumber.number = attempt DrivingLicenceNumber.number(raw) {\n        pattern: Unreadable\n    }\n    return attempt DrivingLicenceNumber {\n        country: to\n        number: number\n    } {\n        number_of_country: Unreadable\n    }\n}\n"
        ),
        NONE,
        "a number from a value is attempted by its rule"
    );
    assert_eq!(
        rules(
            "function licence(country: Country, number: Wire) -> DrivingLicenceNumber {\n    return DrivingLicenceNumber {\n        country: country\n        number: number\n    }\n}\n"
        ),
        ["type.mismatch"],
        "other text isn't a document's number"
    );
}

#[test]
fn an_identity_document_s_number_fits_its_kind_and_country() {
    let document = "function held() -> IdentityDocument {\n    return IdentityDocument {\n        kind: IdentityDocumentKind.passport\n        country: Country.gb\n        number: \"123456789\"\n        expires: \"2030-01-31\"\n    }\n}\n";
    assert_eq!(rules(document), NONE);
    assert_eq!(
        rules(&document.replace(
            "IdentityDocumentKind.passport",
            "IdentityDocumentKind.driving_licence"
        )),
        ["type.invalid_literal"],
        "nine digits aren't a British driving licence's number"
    );
    assert_eq!(
        rules(&document.replace("\"2030-01-31\"", "none")),
        NONE,
        "a document that never expires"
    );
}

#[test]
fn documents_are_stored_in_a_column_per_part() {
    let db = Database::default();
    let source = "entity Traveller {\n    id: Uuid\n    passport: PassportNumber\n    document: IdentityDocument?\n    card: MaskedCardNumber\n    expiry: CardExpiry\n    supplier_vat: VatNumber\n}\n";
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    assert_eq!(found, NONE);
    let model = jadpo_check::data::data_model(&db, project);
    let traveller = model.entity("Traveller").expect("Traveller");
    let columns: Vec<(String, String)> = traveller
        .fields
        .iter()
        .flat_map(|field| field.columns())
        .collect();
    let named: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        named,
        [
            "id",
            "passport_country",
            "passport_number",
            "document_kind",
            "document_country",
            "document_number",
            "document_expires",
            "card_brand",
            "card_last_four",
            "expiry_year",
            "expiry_month",
            "supplier_vat_country",
            "supplier_vat_number",
        ]
    );
    assert!(columns.contains(&("document_expires".to_string(), "date".to_string())));
}
