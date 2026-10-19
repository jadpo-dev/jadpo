//! Area 19, "Web and software" (chunk 5.5n2), run: the page's example
//! (`project=software`), a plugin's versions read by SemVer 2.0.0, ordered
//! by its precedence and matched against a shop's range by node-semver's
//! rules, pre-releases included; and the other software types read from
//! text by their standards, refusing what isn't one.

use crate::support;

use jadpo_interp::Value;
use support::{failure, main, text, value};

const SOURCE: &str = "type FormText = Text {}

type YesNo = Bool {}

type NotAVersion = Failure {
    kind: FailureKind.invalid_value
}

/// The version a plugin's release form names.
function plugin_version(raw: FormText) fails NotAVersion -> SemanticVersion {
    var typed_version: SemanticVersion = attempt text.parse(raw, as: SemanticVersion) {
        malformed: NotAVersion
    }
    return typed_version
}

/// Whether a plugin's release fits the range a shop asks for.
function fits_shop(plugin_release: SemanticVersion, shop_wants: VersionRange) -> YesNo {
    return YesNo(semver.satisfies(plugin_release, shop_wants))
}

/// Whether an offered release is newer than the one installed.
function newer_than(installed_release: SemanticVersion, offered_release: SemanticVersion) -> YesNo {
    return YesNo(offered_release > installed_release)
}

/// Whether a release is offered to every shop: it has no pre-release.
function generally_available(plugin_release: SemanticVersion) -> YesNo {
    return YesNo(plugin_release.pre_release == none)
}

/// A release's version as its tag writes it.
function release_tag(plugin_release: SemanticVersion) -> FormText {
    return FormText(text.from(plugin_release))
}
";

#[test]
fn versions_follow_semver_s_precedence_not_their_text() {
    let program = main(SOURCE).with_proof_checks();
    let version = |raw: &str| value(&program, "plugin_version", vec![text(raw)]);
    let newer = |installed: &str, offered: &str| {
        value(
            &program,
            "newer_than",
            vec![version(installed), version(offered)],
        ) == Value::Bool(true)
    };
    assert!(newer("1.9.0", "1.10.0"), "1.10.0 isn't before 1.9.0");
    assert!(newer("1.10.0-rc.1", "1.10.0"), "a pre-release comes first");
    assert!(
        newer("1.0.0-beta.2", "1.0.0-beta.11"),
        "numeric identifiers numerically"
    );
    assert!(
        newer("1.0.0-alpha.1", "1.0.0-alpha.beta"),
        "numbers before words"
    );
    assert!(
        !newer("1.0.0+build.1", "1.0.0+build.2"),
        "build metadata is ignored"
    );
    assert_eq!(
        value(
            &program,
            "generally_available",
            vec![version("1.10.0-rc.1")]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        value(
            &program,
            "release_tag",
            vec![version("1.10.0-rc.1+sha.5114f85")]
        ),
        text("1.10.0-rc.1+sha.5114f85")
    );
    for raw in ["v1.10.0", "1.10", "01.10.0", "1.10.0-rc.01", "1.10.0+"] {
        assert_eq!(
            failure(&program, "plugin_version", vec![text(raw)]).name,
            "NotAVersion",
            "{raw}"
        );
    }
}

#[test]
fn a_range_takes_a_pre_release_only_where_it_names_one() {
    let program = main(SOURCE).with_proof_checks();
    let fits = |raw: &str, range: &str| {
        let release = value(&program, "plugin_version", vec![text(raw)]);
        value(&program, "fits_shop", vec![release, text(range)]) == Value::Bool(true)
    };
    assert!(fits("1.4.2", "^1.2.3"));
    assert!(!fits("1.4.2", "~1.2.3"));
    assert!(!fits("2.0.0-beta.1", "^1.2.3"));
    assert!(!fits("2.0.0-beta.1", ">=1.0.0"));
    assert!(fits("2.0.0-beta.1", ">=2.0.0-beta.0 <3.0.0"));
    assert!(fits("0.2.9", "^0.2.3"));
    assert!(!fits("0.3.0", "^0.2.3"));
    assert!(fits("1.3.0", "1.2.7 || >=1.2.9 <2.0.0"));
    assert!(fits("2.3.4", "1.2.3 - 2.3.4"));
    assert!(!fits("1.0.0-rc.1", "*"));
}

const READ: &str = "type FormText = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

function git_ref(raw: FormText) fails Unreadable -> GitRef {
    return attempt text.parse(raw, as: GitRef) {
        malformed: Unreadable
    }
}

function package_url(raw: FormText) fails Unreadable -> PackageUrl {
    return attempt text.parse(raw, as: PackageUrl) {
        malformed: Unreadable
    }
}

function image(raw: FormText) fails Unreadable -> ContainerImage {
    return attempt text.parse(raw, as: ContainerImage) {
        malformed: Unreadable
    }
}

function path(raw: FormText) fails Unreadable -> FilePath {
    return attempt text.parse(raw, as: FilePath) {
        malformed: Unreadable
    }
}

function pattern_text(raw: FormText) fails Unreadable -> RegularExpression {
    return attempt text.parse(raw, as: RegularExpression) {
        malformed: Unreadable
    }
}

function licence(raw: FormText) fails Unreadable -> SpdxLicence {
    return attempt text.parse(raw, as: SpdxLicence) {
        malformed: Unreadable
    }
}

function level(raw: FormText) fails Unreadable -> LogLevel {
    return attempt text.parse(raw, as: LogLevel) {
        malformed: Unreadable
    }
}
";

#[test]
fn software_types_are_read_by_their_standards() {
    let program = main(READ).with_proof_checks();
    let cases: &[(&str, &str, &str)] = &[
        ("git_ref", "refs/heads/main", "main.lock"),
        (
            "package_url",
            "pkg:npm/%40angular/animation@12.3.1",
            "pkg:npm/@angular/animation@12.3.1",
        ),
        ("image", "ghcr.io/acme/shop-api:2.1.0", "shop-api:2.1.0"),
        ("path", "/var/lib/acme/uploads", "/var/lib/../acme"),
        ("pattern_text", "(?i)tea|coffee", "(a+)\\1"),
        (
            "licence",
            "GPL-2.0-or-later WITH Classpath-exception-2.0",
            "GPL-2.0-or-later WITH MIT",
        ),
    ];
    for (function, valid, invalid) in cases {
        assert_eq!(
            value(&program, function, vec![text(valid)]),
            text(valid),
            "{function}: `{valid}`"
        );
        assert_eq!(
            failure(&program, function, vec![text(invalid)]).name,
            "Unreadable",
            "{function}: `{invalid}`"
        );
    }
    // A log level's code is read ignoring case, as every standard enum's is.
    let Value::Variant(warn) = value(&program, "level", vec![text("warn")]) else {
        panic!("a variant");
    };
    assert_eq!(&*warn.name, "warn");
    assert_eq!(
        failure(&program, "level", vec![text("WARNING")]).name,
        "Unreadable"
    );
}

/// Text written where a standard object type goes is that type's
/// canonical text, read as the object (found in chunk 5.7, where it was
/// kept as text, so comparing it or reading a field of it faulted).
#[test]
fn a_literal_of_a_standard_object_is_read_as_the_object() {
    let source = format!(
        "{SOURCE}
/// The oldest version a shop still takes.
function oldest_supported() -> SemanticVersion {{
    return \"2.1.0\"
}}

/// Whether the oldest supported version is still in a range.
function oldest_fits(shop_wants: VersionRange) -> YesNo {{
    return YesNo(semver.satisfies(oldest_supported(), shop_wants))
}}
"
    );
    let program = main(&source).with_proof_checks();
    assert_eq!(
        value(&program, "oldest_supported", vec![]),
        value(&program, "plugin_version", vec![text("2.1.0")])
    );
    assert_eq!(
        value(&program, "oldest_fits", vec![text("^2.0.0")]),
        Value::Bool(true)
    );
}
