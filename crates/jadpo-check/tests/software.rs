//! Area 19, "Web and software" (chunk 5.5n2), in the checker:
//! `semver.satisfies` typed by its stub, a `SemanticVersion` ordered by
//! `<` and read by its parts, the software types' literals checked while
//! compiling by their standards (SemVer 2.0.0, node-semver's ranges, `git
//! check-ref-format`, ECMA-427, SPDX, W3C Trace Context, OCI, the ULID
//! specification, the DOI Handbook, area 2's patterns), `LogLevel`'s
//! variants, and an application's own type of a new standard name refused.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type Wire = Text {}\n\ntype Answer = Bool {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn satisfies_takes_a_version_and_a_range() {
    assert_eq!(
        rules(
            "function fits(release: SemanticVersion, wanted: VersionRange) -> Answer {\n    return Answer(semver.satisfies(release, wanted))\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function fits(release: SemanticVersion) -> Answer {\n    return Answer(semver.satisfies(release, \"^1.2.3\"))\n}\n"
        ),
        NONE,
        "a range literal is checked where it is taken"
    );
    assert_eq!(
        rules(
            "function fits(release: SemanticVersion) -> Answer {\n    return Answer(semver.satisfies(release, \"latest\"))\n}\n"
        ),
        ["type.invalid_literal"],
        "`latest` isn't a range"
    );
    assert_eq!(
        rules(
            "function fits(release: Wire, wanted: VersionRange) -> Answer {\n    return Answer(semver.satisfies(release, wanted))\n}\n"
        ),
        ["type.mismatch"],
        "text is read as a version first"
    );
}

#[test]
fn a_version_is_ordered_and_read_by_its_parts() {
    assert_eq!(
        rules(
            "function newer(installed: SemanticVersion, candidate: SemanticVersion) -> Answer {\n    return Answer(candidate > installed)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function stable(release: SemanticVersion) -> Answer {\n    return Answer(release.major >= 1 and release.pre_release == none)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function later(a: VersionRange, b: VersionRange) -> Answer {\n    return Answer(a > b)\n}\n"
        ),
        ["type.ordering_operand"],
        "a range has no order"
    );
    let version = |pre: &str| {
        rules(&format!(
            "function made() -> SemanticVersion {{\n    return SemanticVersion {{\n        major: 1\n        minor: 0\n        patch: 0\n        pre_release: {pre}\n        build: none\n    }}\n}}\n"
        ))
    };
    assert_eq!(version("\"rc.1\""), NONE);
    assert_eq!(version("none"), NONE);
    assert_eq!(
        version("\"rc.01\""),
        ["type.invalid_literal"],
        "SemVer 2.0.0 §9: no leading zero"
    );
    assert_eq!(
        rules(
            "function read(raw: Wire) fails Unreadable -> SemanticVersion {\n    var release: SemanticVersion = attempt text.parse(raw, as: SemanticVersion) {\n        malformed: Unreadable\n    }\n    return release\n}\n\nfunction written(release: SemanticVersion) -> Wire {\n    return Wire(text.from(release))\n}\n"
        ),
        NONE
    );
}

#[test]
fn literals_are_checked_by_their_standards_while_compiling() {
    let literal = |ty: &str, text: &str| {
        rules(&format!(
            "function made() -> {ty} {{\n    return {ty}(\"{text}\")\n}}\n"
        ))
    };
    let cases: &[(&str, &str, &str)] = &[
        ("VersionRange", ">=1.0.0 <2.0.0", ">= 1.0.0"),
        ("GitRef", "refs/heads/main", "feature..fix"),
        (
            "PackageUrl",
            "pkg:pypi/django@1.11.1",
            "pkg:pypi/Django@1.11.1+local",
        ),
        ("Cve", "CVE-2021-44228", "CVE-21-44228"),
        ("SpdxLicence", "MIT OR Apache-2.0", "MIT or Apache-2.0"),
        (
            "TraceId",
            "4bf92f3577b34da6a3ce929d0e0e4736",
            "00000000000000000000000000000000",
        ),
        ("SpanId", "00f067aa0ba902b7", "0000000000000000"),
        (
            "TraceParent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        ),
        ("FilePath", "/var/lib/acme/uploads", "reports/../logo.png"),
        ("FileName", "report.pdf", "CON"),
        ("RegularExpression", "[A-Z]{2}[0-9]{2}", "foo(?=bar)"),
        (
            "ContainerImage",
            "docker.io/library/nginx:1.27",
            "nginx:1.27",
        ),
        (
            "Digest",
            "sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
            "sha256:6c3c",
        ),
        (
            "Ulid",
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            "01ARZ3NDEKTSV4RRFFQ69G5FAU",
        ),
        ("IdempotencyKey", "order-1042-attempt", " order-1042"),
        ("Doi", "10.1038/nphys1170", "10.1038/NPHYS1170"),
    ];
    for (ty, valid, invalid) in cases {
        assert_eq!(literal(ty, valid), NONE, "{ty}: `{valid}`");
        assert_eq!(
            literal(ty, invalid),
            ["type.invalid_literal"],
            "{ty}: `{invalid}`"
        );
    }
}

#[test]
fn a_log_level_is_one_of_opentelemetry_s_severities() {
    assert_eq!(
        rules("function quiet() -> LogLevel {\n    return LogLevel.warn\n}\n"),
        NONE
    );
    assert_eq!(
        rules("function loud() -> LogLevel {\n    return LogLevel.critical\n}\n"),
        ["type.unknown_enum_variant"]
    );
}

#[test]
fn an_application_s_own_log_level_is_a_clash() {
    assert_eq!(
        rules("type LogLevel = Enum {\n    verbose\n    terse\n}\n"),
        ["type.standard_clash"]
    );
}
