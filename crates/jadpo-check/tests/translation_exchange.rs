//! What the exchange with translation services reads and writes on
//! Jadpo's side (area 10, "Tooling"; chunk 5.4f): the units of wording a
//! project has, and a wording written into a language file, in place,
//! without touching what else the file holds.

use jadpo_check::translations::exchange::{
    self, Shape, Wording, catalogue, default_hash, string_literal, write_members,
};
use jadpo_check::translations::{Body, language_file};
use jadpo_db::{Database, Layout};

use crate::support;

const APP: &str = "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.pl_pl]\n}\n";

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", APP),
    (
        "domains/shop/types/mood.jadpo",
        "/// How cheerful.\ntype Mood = Enum {\n    glad\n    sad\n}\n",
    ),
    (
        "domains/shop/translations/greeting.jadpo",
        "use shop/types { Mood }\n\n/// Greets a customer.\ntype Greeting = Message {\n    mood: Mood\n    name: Text\n}\n",
    ),
    (
        "domains/shop/translations/parcels.jadpo",
        "/// How many parcels.\ntype Parcels = Message {\n    count: PluralCount\n}\n",
    ),
    (
        "domains/shop/translations/en_gb.jadpo",
        "translations {\n    // First.\n    greeting: {\n        glad: \"Hi {name}!\"\n        sad: \"Hello {name}.\"\n    }\n    parcels: {\n        one: \"{count} parcel\"\n        other: \"{count} parcels\"\n    }\n}\n",
    ),
];

#[test]
fn the_catalogue_lists_what_can_be_worded() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, FILES);
    let found = catalogue(&db, project).expect("locales");
    assert_eq!(found.default.tag, "en_gb");
    assert_eq!(found.default.bcp47, "en-GB");
    assert_eq!(found.targets.len(), 1);
    assert_eq!(found.targets[0].bcp47, "pl-PL");
    let members: Vec<(&str, &str)> = found
        .entries
        .iter()
        .map(|entry| (entry.scope.as_str(), entry.member.as_str()))
        .collect();
    assert_eq!(members, [("shop", "greeting"), ("shop", "parcels")]);
    let greeting = &found.entries[0];
    assert_eq!(greeting.description.as_deref(), Some("Greets a customer."));
    assert_eq!(greeting.placeholders, ["name"]);
    assert_eq!(
        greeting.shape,
        Shape::Choice {
            field: "mood".to_string(),
            values: vec!["glad".to_string(), "sad".to_string()]
        }
    );
    assert_eq!(greeting.id(Some("glad")), "shop.greeting.glad");
    assert_eq!(greeting.source_for(Some("sad")), Some("Hello {name}."));
    assert_eq!(
        greeting.context(Some("sad")).as_deref(),
        Some("Used when `mood` is `sad`.")
    );
    // Polish has four plural forms, English two: the others are translated
    // from `other`.
    let parcels = &found.entries[1];
    let forms: Vec<String> = parcels
        .variants(found.targets[0].ordinal)
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(forms, ["one", "few", "many", "other"]);
    assert_eq!(parcels.source_for(Some("few")), Some("{count} parcels"));
    assert_eq!(parcels.source_for(Some("one")), Some("{count} parcel"));
    assert_eq!(parcels.record_member(), "parcels");
}

#[test]
fn what_a_locale_has_is_found_with_its_standing() {
    let mut files = FILES.to_vec();
    files.push((
        "domains/shop/translations/pl_pl.jadpo",
        "translations {\n    parcels: {\n        one: \"{count} paczka\"\n        few: \"{count} paczki\"\n        many: \"{count} paczek\"\n        other: \"{count} paczki\"\n    }\n}\n",
    ));
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let found = catalogue(&db, project).expect("locales");
    let parcels = &found.entries[1];
    let have = exchange::existing(&db, project, parcels, "pl_pl").expect("worded");
    assert_eq!(have.wording.len(), 4);
    assert!(!have.stale && !have.accepted);
    assert!(exchange::existing(&db, project, &found.entries[0], "pl_pl").is_none());
}

#[test]
fn a_catalogue_needs_locales() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &FILES[1..]);
    assert!(catalogue(&db, project).is_none());
}

#[test]
fn a_domain_named_like_jadpos_group_is_a_clash() {
    let files = vec![
        ("app.jadpo", APP),
        (
            "domains/jadpo/translations/hello.jadpo",
            "/// Hello.\ntype Hello = Message {}\n",
        ),
        (
            "domains/jadpo/translations/en_gb.jadpo",
            "translations {\n    hello: \"Hello.\"\n}\n",
        ),
    ];
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    assert!(catalogue(&db, project).expect("locales").clash.is_some());
}

#[test]
fn the_default_hash_is_the_one_a_record_holds() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, FILES);
    let found = catalogue(&db, project).expect("locales");
    let greeting = &found.entries[0];
    let expected = Wording::Variants(vec![
        ("glad".to_string(), "Hi {name}!".to_string()),
        ("sad".to_string(), "Hello {name}.".to_string()),
    ]);
    assert_eq!(
        default_hash(greeting),
        jadpo_check::translations::sources::hash(&expected.canonical())
    );
    // Order does not matter to a hash.
    let reversed = Wording::Variants(vec![
        ("sad".to_string(), "Hello {name}.".to_string()),
        ("glad".to_string(), "Hi {name}!".to_string()),
    ]);
    assert_eq!(expected.canonical(), reversed.canonical());
}

/// Writes `changes` into the English file of `FILES` and returns the new
/// text.
fn rewritten(changes: &[(&str, Wording)]) -> String {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, FILES);
    let file = jadpo_check::translations::exchange::file_for(&db, project, "shop", "en_gb")
        .expect("english");
    let texts = jadpo_db::source_texts(&db, project);
    let text = &texts["domains/shop/translations/en_gb.jadpo"];
    let changes: Vec<(String, Wording)> = changes
        .iter()
        .map(|(name, wording)| (name.to_string(), wording.clone()))
        .collect();
    write_members(Some((file, text)), false, &changes)
}

#[test]
fn a_member_is_replaced_where_it_is_and_the_rest_is_kept() {
    let out = rewritten(&[(
        "parcels",
        Wording::Variants(vec![
            ("one".to_string(), "{count} box".to_string()),
            ("other".to_string(), "{count} boxes".to_string()),
        ]),
    )]);
    assert!(out.contains("// First."), "{out}");
    assert!(out.contains("glad: \"Hi {name}!\""), "{out}");
    assert!(out.contains("one: \"{count} box\""), "{out}");
    assert!(!out.contains("parcel\""), "{out}");
    assert!(out.ends_with("}\n"), "{out}");
}

#[test]
fn a_new_member_goes_before_the_closing_brace() {
    let out = rewritten(&[("farewell", Wording::Text("Bye.".to_string()))]);
    assert!(out.contains("farewell: \"Bye.\"\n}"), "{out}");
    assert!(out.starts_with("translations {\n    // First."), "{out}");
}

#[test]
fn a_new_file_and_the_root_files_group() {
    let wording = Wording::Text("Hej.".to_string());
    assert_eq!(
        write_members(None, false, &[("hello".to_string(), wording.clone())]),
        "translations {\nhello: \"Hej.\"\n}\n"
    );
    assert_eq!(
        write_members(None, true, &[("hello".to_string(), wording)]),
        "translations {\njadpo: {\nhello: \"Hej.\"\n}\n}\n"
    );
}

#[test]
fn the_root_file_gains_members_and_groups_in_place() {
    let root =
        "translations {\n    // Mine.\n    jadpo: {\n        invalid_input: \"Zle.\"\n    }\n}\n";
    let files = vec![("app.jadpo", APP), ("translations/pl_pl.jadpo", root)];
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let file = exchange::file_for(&db, project, exchange::JADPO, "pl_pl").expect("root file");
    let out = write_members(
        Some((file, root)),
        true,
        &[
            (
                "invalid_input".to_string(),
                Wording::Text("Źle.".to_string()),
            ),
            (
                "rate_limited".to_string(),
                Wording::Text("Wolniej.".to_string()),
            ),
        ],
    );
    let out = jadpo_fmt::format_file(&out).expect("formats");
    assert!(out.contains("// Mine."), "{out}");
    assert!(out.contains("invalid_input: \"Źle.\""), "{out}");
    assert!(out.contains("rate_limited: \"Wolniej.\"\n    }"), "{out}");
    // A file with no `jadpo` group gets one.
    let other = "translations {\n}\n";
    let files = vec![("app.jadpo", APP), ("translations/pl_pl.jadpo", other)];
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let file = exchange::file_for(&db, project, exchange::JADPO, "pl_pl").expect("root file");
    let out = write_members(
        Some((file, other)),
        true,
        &[(
            "rate_limited".to_string(),
            Wording::Text("Wolniej.".to_string()),
        )],
    );
    assert!(
        out.contains("jadpo: {\nrate_limited: \"Wolniej.\"\n}"),
        "{out}"
    );
}

#[test]
fn a_wording_is_written_so_that_it_reads_back_the_same() {
    for text in [
        "plain",
        "quote \" and backslash \\",
        "line\nbreak\r\n and\ttab",
        "{count} paczek, {{literal}}",
        "ünïcödé — 日本語",
        "",
    ] {
        let source = format!("translations {{\n    hello: {}\n}}\n", string_literal(text));
        let files = vec![
            ("app.jadpo", APP),
            ("domains/shop/translations/en_gb.jadpo", source.as_str()),
        ];
        let db = Database::default();
        let project = support::project(&db, Layout::Project, &files);
        let file = jadpo_check::lookups::file_at(
            &db,
            project,
            jadpo_check::lookups::Name::new(
                &db,
                "domains/shop/translations/en_gb.jadpo".to_string(),
            ),
        )
        .expect("the file");
        let read = language_file(&db, project, file)
            .as_ref()
            .expect("a language file");
        assert!(read.problems.is_empty(), "{text:?}: {:?}", read.problems);
        match &read.members[0].body {
            Body::Text(written) => assert_eq!(written.text, text),
            other => panic!("{other:?}"),
        }
    }
}
