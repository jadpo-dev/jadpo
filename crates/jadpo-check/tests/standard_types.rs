//! The one harness every standard type runs through, in the checker (area
//! 19, "Every standard type meets one contract", question 2): each type's
//! stub declaration agrees with its data entry in `jadpo-std`, and the
//! contract's calls and `<` accept a value of the type exactly as its entry
//! says. `jadpo-runtime`'s `standard_types` tests are the runtime's half,
//! over the same entries. A new type is a stub and an entry, not a new
//! test.

use crate::support;

use jadpo_check::StandardStub;
use jadpo_db::Layout;
use jadpo_std::Builtin;
use jadpo_std::standard::{Holds, STANDARD_TYPES, StandardType};

fn check(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", source)])
}

/// The declarations a check of `entry`'s values starts from: a named type
/// built on a built-in standard type (a signature takes named types), and
/// the types the calls give and fail with.
fn types(entry: &StandardType) -> (String, String) {
    let declared = match entry.holds {
        Holds::Builtin(_) => format!("type Shown = {} {{}}\n\n", entry.name),
        _ => String::new(),
    };
    let value = match entry.holds {
        Holds::Builtin(_) => "Shown".to_string(),
        _ => entry.name.to_string(),
    };
    (
        format!(
            "{declared}type Wire = Text {{}}\n\ntype Answer = Bool {{}}\n\ntype Unreadable = Failure {{\n    kind: FailureKind.invalid_value\n}}\n\n"
        ),
        value,
    )
}

#[test]
fn every_stub_agrees_with_its_data_entry() {
    let stubs = jadpo_check::standard_stubs();
    let names: Vec<&str> = stubs.keys().map(String::as_str).collect();
    let entries: Vec<&str> = STANDARD_TYPES.iter().map(|entry| entry.name).collect();
    assert_eq!(names, entries, "a stub for every entry, an entry for every stub");
    for entry in STANDARD_TYPES {
        let StandardStub { base, members } = &stubs[entry.name];
        match entry.holds {
            Holds::Builtin(builtin) => {
                assert_eq!(base, builtin.name(), "{} is built in", entry.name);
                assert_eq!(base, entry.name);
                assert!(members.is_empty());
            }
            Holds::Enum(codes) => {
                assert_eq!(base, "Enum", "{}", entry.name);
                let listed: Vec<&str> = members.iter().map(|(name, _)| name.as_str()).collect();
                let variants: Vec<&str> = codes.iter().map(|(name, _)| *name).collect();
                // A long table isn't repeated in the stub.
                assert!(
                    listed.is_empty() || listed == variants,
                    "{}'s stub lists its variants as its entry does",
                    entry.name
                );
                let known = jadpo_std::enums()
                    .into_iter()
                    .find(|(name, _)| *name == entry.name)
                    .map(|(_, variants)| variants)
                    .unwrap_or_default();
                assert_eq!(known, variants, "{} is the enum the runtime reads", entry.name);
            }
            Holds::Object(fields) => {
                assert_eq!(base, "Object", "{}", entry.name);
                let declared: Vec<(String, String)> = members
                    .iter()
                    .map(|(name, ty)| (name.clone(), format!("{ty:?}")))
                    .collect();
                let expected: Vec<(String, String)> = fields
                    .iter()
                    .map(|(name, builtin): &(&str, Builtin)| {
                        let ty = jadpo_check::types::TypeRef::Path(
                            vec![builtin.name().to_string()],
                            Vec::new(),
                        );
                        ((*name).to_string(), format!("{:?}", Some(ty)))
                    })
                    .collect();
                assert_eq!(declared, expected, "{}'s fields", entry.name);
            }
        }
    }
}

#[test]
fn every_standard_type_meets_the_contract_in_the_checker() {
    for entry in STANDARD_TYPES {
        let name = entry.name;
        let (declared, value) = types(entry);
        // `text.format`, with the options its entry takes, and none other.
        let mut options = String::new();
        if entry.format_option("zone").is_some() {
            options.push_str(", zone: Zone.utc");
        }
        if let Some(format) = entry.examples.format {
            options.push_str(&format!(", format: TimeFormat.{format}"));
        }
        let format = |options: &str| {
            check(&format!(
                "{declared}function f(value: {value}, shown: Locale) -> PresentationText {{\n    return text.format(value, locale: shown{options})\n}}\n"
            ))
        };
        assert_eq!(format(&options), Vec::<String>::new(), "{name}: text.format");
        if entry.format_option("zone").is_none() {
            assert_eq!(
                format(&format!("{options}, zone: Zone.utc")),
                ["type.temporal_option_unused"],
                "{name}: `zone:` means nothing"
            );
        } else {
            let without = options.replace(", zone: Zone.utc", "");
            assert_eq!(
                format(&without),
                ["type.temporal_zone_required"],
                "{name}: `zone:` is needed"
            );
        }
        // `text.from`: the canonical text, except for a type built on
        // `Text`, which is text already.
        let has_text = entry.text.is_some()
            && !matches!(entry.holds, Holds::Builtin(builtin) if builtin.representation() == Builtin::Text);
        let from = check(&format!(
            "{declared}function f(value: {value}) -> Wire {{\n    return Wire(text.from(value))\n}}\n"
        ));
        if has_text {
            assert_eq!(from, Vec::<String>::new(), "{name}: text.from");
        } else {
            assert_eq!(from, ["type.mismatch"], "{name}: text.from takes no text");
        }
        // `text.parse`: the type, failing `malformed`.
        assert_eq!(
            check(&format!(
                "{declared}function f(raw: Wire) fails Unreadable -> {value} {{\n    var read: {value} = attempt text.parse(raw, as: {value}) {{\n        malformed: Unreadable\n    }}\n    return read\n}}\n"
            )),
            Vec::<String>::new(),
            "{name}: text.parse"
        );
        // `<`, where the type is ordered.
        let ordered = check(&format!(
            "{declared}function f(a: {value}, b: {value}) -> Answer {{\n    return Answer(a < b)\n}}\n"
        ));
        assert_eq!(
            ordered.is_empty(),
            entry.ordered,
            "{name}: `<` exactly when ordered ({ordered:?})"
        );
    }
}

#[test]
fn the_contract_refuses_what_has_no_contract() {
    let types = "type Wire = Text {}\n\ntype Count = Int {}\n\ntype Shade = Enum {\n    plain\n    tinted\n}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";
    let cases: &[(&str, &[&str])] = &[
        // A number has canonical text, but no format for people yet.
        (
            "function f(n: Count) -> Wire {\n    return Wire(text.from(n))\n}\n",
            &[],
        ),
        (
            "function f(n: Count, shown: Locale) -> PresentationText {\n    return text.format(n, locale: shown)\n}\n",
            &["type.mismatch"],
        ),
        // An application's enum has no text of its own.
        (
            "function f(shade: Shade) -> Wire {\n    return Wire(text.from(shade))\n}\n",
            &["type.mismatch"],
        ),
        // `as:` is a type written out.
        (
            "function f(raw: Wire, count: Count) fails Unreadable -> Count {\n    var read: Count = attempt text.parse(raw, as: count) {\n        invalid: Unreadable\n    }\n    return read\n}\n",
            &["type.mismatch"],
        ),
        // Text read as text can't be malformed.
        (
            "function f(raw: Wire) -> Wire {\n    return text.parse(raw, as: Wire)\n}\n",
            &[],
        ),
        // A number read from text: `malformed`, and its rules.
        (
            "function f(raw: Wire) fails Unreadable -> Count {\n    var read: Count = attempt text.parse(raw, as: Count) {\n        malformed: Unreadable\n    }\n    return read\n}\n",
            &[],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&check(&format!("{types}{source}")), expected, "{source}");
    }
}
