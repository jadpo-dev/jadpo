//! Area 19, "A UK postcode's parts", checked: each `postal` operation
//! takes a `PostalCode` and fails with `not_uk_format` unless its
//! `attempt` maps it, or its postcode is written out in Royal Mail's
//! format; `sub_district` may be `none`.

use crate::support;

use jadpo_db::{Database, Layout};

const TYPES: &str = "type DistrictCode = Text {}\n\ntype NotAUkPostcode = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

/// Each problem as its rule, and its reason when `reasons` asks.
fn check(source: &str, reasons: bool) -> Vec<String> {
    let db = Database::default();
    let text = format!("{TYPES}{source}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| match (&d.reason, reasons) {
            (Some(reason), true) => format!("{}: {reason}", d.rule.id),
            _ => d.rule.id.to_string(),
        })
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn every_part_checks_with_its_arm() {
    for name in [
        "outward_code",
        "inward_code",
        "area",
        "district",
        "sector",
        "unit",
    ] {
        let source = format!(
            "function part(postcode: PostalCode) fails NotAUkPostcode -> DistrictCode {{\n    var part: Text = attempt postal.{name}(postcode) {{\n        not_uk_format: NotAUkPostcode\n    }}\n    return DistrictCode(part)\n}}\n"
        );
        assert_eq!(check(&source, false), NONE, "{name}");
    }
    let sub_district = "type Divided = Bool {}\n\nfunction divided(postcode: PostalCode) fails NotAUkPostcode -> Divided {\n    var part: Text? = attempt postal.sub_district(postcode) {\n        not_uk_format: NotAUkPostcode\n    }\n    return Divided(part != none)\n}\n";
    assert_eq!(check(sub_district, false), NONE);
}

#[test]
fn a_postcode_not_proved_needs_an_arm() {
    let problems = check(
        "function part(postcode: PostalCode) -> DistrictCode {\n    return DistrictCode(postal.district(postcode))\n}\n",
        true,
    );
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with(
            "failure.attempt_required: Only a postcode in Royal Mail's format has these parts"
        ),
        "{problems:?}"
    );
}

#[test]
fn a_postcode_written_out_in_the_format_needs_none() {
    let written = "function palace() -> DistrictCode {\n    return DistrictCode(postal.district(\"SW1A 1AA\"))\n}\n\nfunction unspaced() -> DistrictCode {\n    return DistrictCode(postal.sector(\"SW1A1AA\"))\n}\n";
    assert_eq!(check(written, false), NONE);
    for outside in ["75004", "GIR 0AA", "BFPO 61", "SW1A 2A0"] {
        let source = format!(
            "function elsewhere() -> DistrictCode {{\n    return DistrictCode(postal.district(\"{outside}\"))\n}}\n"
        );
        assert_eq!(
            check(&source, false),
            ["failure.attempt_required"],
            "{outside}"
        );
    }
}

#[test]
fn a_sub_district_may_be_none() {
    assert_eq!(
        check(
            "function part() -> DistrictCode {\n    return DistrictCode(postal.sub_district(\"SW1A 1AA\"))\n}\n",
            false
        ),
        ["type.constructor_input"]
    );
}
