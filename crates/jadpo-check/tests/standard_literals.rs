//! Literals where a standard object type goes (chunk 5.8c), from the slice
//! the Phase 5 soundness review couldn't cover: text written where a type
//! with rules across its parts goes is the type's canonical text, which the
//! checker can't read, so it wasn't checked against them, and a backwards
//! range or a week past a year's last was kept; it is now
//! `type.invalid_literal`. Any other literal there, or text where a type
//! with no text form goes (WKT with mixed altitudes where a `LineString`
//! goes), was kept as it was written and faulted where it was used; it is
//! now `type.mismatch`. A setting
//! never holds a type with rules across its parts, since none is one piece
//! of text.

use crate::support;

use jadpo_db::Layout;

fn rules(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", source)])
}

const NONE: &[&str] = &[];

#[test]
fn text_where_a_type_with_rules_goes_is_refused() {
    for (ty, text) in [
        ("InstantRange", "2026-10-25T17:00:00Z/2026-10-25T09:00:00Z"),
        ("InstantRange", "2026-10-25T09:00:00Z/2026-10-25T17:00:00Z"),
        ("DateRange", "2026-10-31/2026-10-01"),
        ("IsoWeek", "2027-W53"),
    ] {
        assert_eq!(
            rules(&format!(
                "function made() -> {ty} {{\n    return \"{text}\"\n}}\n"
            )),
            ["type.invalid_literal"],
            "{ty} {text}"
        );
    }
    assert_eq!(
        rules("constant days: DateRange = \"2026-10-31/2026-10-01\"\n"),
        ["type.invalid_literal"]
    );
}

#[test]
fn a_literal_a_standard_object_can_t_be_is_the_wrong_kind() {
    for (ty, literal) in [
        ("PostalAddress", "\"1 High Street\""),
        ("Rating", "5"),
        ("InstantRange", "true"),
        ("SemanticVersion", "2.5"),
        // A geometry has no text form, so WKT written where one goes, with
        // altitudes `altitudes_agree` would refuse when a line is built
        // or with none, isn't one.
        ("LineString", "\"LINESTRING Z (1 1 5, 2 2 0)\""),
        ("LineString", "\"LINESTRING (1 1, 2 2 3)\""),
        ("Polygon", "\"POLYGON ((0 0, 1 0, 1 1, 0 0))\""),
    ] {
        assert_eq!(
            rules(&format!(
                "function made() -> {ty} {{\n    return {literal}\n}}\n"
            )),
            ["type.mismatch"],
            "{ty} {literal}"
        );
    }
    // A type with one text form and no rules across its parts still takes
    // its canonical text, read where the value is made.
    assert_eq!(
        rules("function made() -> SemanticVersion {\n    return \"2.1.0\"\n}\n"),
        NONE
    );
}

#[test]
fn a_setting_never_holds_a_type_with_rules_across_its_parts() {
    for ty in ["DateRange", "InstantRange", "Rating"] {
        assert_eq!(
            rules(&format!(
                "config {{\n    season: {ty} {{\n        binding: \"SEASON\"\n    }}\n}}\n"
            )),
            ["configuration.type_unsupported"],
            "{ty}"
        );
    }
    assert_eq!(
        rules(
            "type Moment = Instant {}\n\ntype Window = Object {\n    opens: Moment\n    closes: Moment\n    rule closes_after_opening: closes > opens\n}\n\nconfig {\n    season: Window {\n        binding: \"SEASON\"\n    }\n}\n"
        ),
        ["configuration.type_unsupported"]
    );
}
