//! Translations, the rest of area 10's "Translations as messages" (chunk
//! 5.4d), from the command line: a locale's text direction, failures worded
//! for client apps and exported per locale, stale and unaccepted
//! translations through `sources.jadpo_record`, the `jadpo translations`
//! commands, and the pseudo-locale.
//!
//! The projects have no entities, so no database is needed.

use super::http::{jadpo, tempdir, write};

const APP: &str = "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr, Locale.ar_eg]\n}\n";

#[test]
fn a_locale_knows_its_text_direction() {
    let dir = tempdir::Dir::new("translation-direction");
    let root = dir.path().join("shop");
    write(&root, "app.jadpo", APP);
    write(
        &root,
        "tests/direction.test.jadpo",
        r#"test "arabic is written right to left and English left to right" {
    assert locale.direction(Locale.ar_eg) == TextDirection.right_to_left
    assert locale.direction(Locale.en_gb) == TextDirection.left_to_right
    assert locale.direction(Locale.he_il) == TextDirection.right_to_left
}
"#,
    );
    let done = jadpo(&root, &["test"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(done.stdout.contains("1 passed"), "{}", done.stdout);
}
