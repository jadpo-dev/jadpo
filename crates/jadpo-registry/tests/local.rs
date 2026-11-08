//! The local registry the `jadpo` tool's tests run against: the protocol
//! served over HTTP on this machine, from the same table the conformance
//! test reads. Every answer is decoded into the protocol's types and
//! checked against the table's shapes, and the log's proofs are verified
//! end to end, as `jadpo package add` and `update` will.

use jadpo_registry::archive::unpack;
use jadpo_registry::http::{Fetched, fetch};
use jadpo_registry::local::{LocalRegistry, ReleaseSeed};
use jadpo_registry::log::{LogEntry, LogError, check_extends, check_included, check_signed_head};
use jadpo_registry::names::{Digest, Handle, ScopedName, Version};
use jadpo_registry::protocol::{
    Advisory, AdvisoryAnswer, ConsistencyProof, DecodeFailure, FingerprintAnswer, FingerprintQuery,
    InclusionProof, LONGEST_SHAPE_PATH, LogHead, MOST_SHAPE_MEMBERS, Method, Package, PackageKind,
    PublishedFingerprint, ReceivedReport, Release, ReleaseRequest, ReleaseState, Request,
    SearchAnswer, ShapeKind, ShapeMember, SignedLogHead, StartedRelease, check_answer,
};
use jadpo_registry::report::shape_of;

fn name(text: &str) -> ScopedName {
    ScopedName::parse(text).unwrap()
}

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

fn seed(package: &str, v: &str) -> ReleaseSeed {
    ReleaseSeed::new(
        name(package),
        version(v),
        vec![
            (
                "package.jadpo".into(),
                format!("package Postcodes.v2 // {v}\n").into_bytes(),
            ),
            ("GUIDE.md".into(), b"Reads UK postcodes.\n".to_vec()),
        ],
    )
}

fn registry() -> LocalRegistry {
    let registry = LocalRegistry::new();
    registry.publisher("@acme", true, false);
    registry.release(seed("@acme/postcodes", "2.0.0"));
    registry.release(seed("@acme/postcodes", "2.1.0"));
    registry.release(seed("@acme/geocoder", "1.0.0"));
    registry
}

/// Sends `request` to the served registry and checks its answer against
/// the table before decoding it.
fn ask<T: serde::de::DeserializeOwned>(url: &str, request: &Request) -> T {
    let answer = send(url, request);
    assert_eq!(
        answer.status,
        request.endpoint().status,
        "{} {}: {}",
        request.method(),
        request.target(),
        String::from_utf8_lossy(&answer.body)
    );
    let json: serde_json::Value = serde_json::from_slice(&answer.body).expect("JSON");
    check_answer(request.endpoint(), &json).unwrap_or_else(|e| panic!("{}: {e}", request.target()));
    serde_json::from_value(json).expect("decodes")
}

fn send(url: &str, request: &Request) -> Fetched {
    let target = request.target();
    let address = format!("{}{}", url.trim_end_matches("/registry/v1/"), target);
    fetch(
        request.method(),
        &address,
        request.body().as_deref().map(str::as_bytes),
    )
    .expect("answers")
}

fn failure(answer: &Fetched) -> (u16, String) {
    let json: serde_json::Value = serde_json::from_slice(&answer.body).expect("JSON");
    let code = json["error"]["code"]
        .as_str()
        .expect("the failure envelope")
        .to_string();
    assert!(json["error"]["message"].is_string());
    assert!(json["error"]["request_id"].is_string());
    (answer.status, code)
}

#[test]
fn a_package_and_its_releases_are_read_as_the_table_says() {
    let registry = registry();
    let served = registry.serve().expect("serves");
    let url = served.url();
    assert!(
        url.starts_with("http://127.0.0.1:") && url.ends_with("/registry/v1/"),
        "{url}"
    );

    let package: Package = ask(
        &url,
        &Request::Package {
            package: name("@acme/postcodes"),
        },
    );
    assert_eq!(package.package, name("@acme/postcodes"));
    assert!(package.publisher.verified);
    assert_eq!(package.kind, PackageKind::Provider);
    let versions: Vec<String> = package
        .releases
        .iter()
        .map(|r| r.version.to_string())
        .collect();
    assert_eq!(versions, ["2.0.0", "2.1.0"]);

    let release: Release = ask(
        &url,
        &Request::Release {
            package: name("@acme/postcodes"),
            version: version("2.1.0"),
        },
    );
    assert_eq!(release.version, version("2.1.0"));
    assert_eq!(release.log_index, 1);
    assert_eq!(release.digest, package.releases[1].digest);
}

#[test]
fn an_archive_unpacks_to_the_releases_digest() {
    let registry = registry();
    let served = registry.serve().unwrap();
    let release: Release = ask(
        &served.url(),
        &Request::Release {
            package: name("@acme/postcodes"),
            version: version("2.0.0"),
        },
    );
    let answer = send(
        &served.url(),
        &Request::Archive {
            digest: release.digest.clone(),
        },
    );
    assert_eq!(answer.status, 200);
    assert_eq!(answer.content_type.as_deref(), Some("application/x-tar"));
    let files = unpack(&answer.body).expect("unpacks");
    let view: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    assert_eq!(Digest::of_files(&view), release.digest);
}

#[test]
fn every_release_is_proved_in_the_log_and_the_log_only_grows() {
    let registry = registry();
    let served = registry.serve().unwrap();
    let url = served.url();
    let seen: LogHead = ask(&url, &Request::LogHead);
    assert_eq!(seen.size, 3);
    for (package, v) in [
        ("@acme/postcodes", "2.0.0"),
        ("@acme/postcodes", "2.1.0"),
        ("@acme/geocoder", "1.0.0"),
    ] {
        let release: Release = ask(
            &url,
            &Request::Release {
                package: name(package),
                version: version(v),
            },
        );
        let proof: InclusionProof = ask(
            &url,
            &Request::LogProof {
                digest: release.digest.clone(),
                size: seen.size,
            },
        );
        let expected = LogEntry {
            package: name(package),
            version: version(v),
            digest: release.digest,
        };
        assert_eq!(check_included(&seen, &expected, &proof), Ok(()));
    }

    registry.release(seed("@acme/postcodes", "2.2.0"));
    let now: LogHead = ask(&url, &Request::LogHead);
    assert_eq!(now.size, 4);
    let proof: ConsistencyProof = ask(
        &url,
        &Request::LogConsistency {
            from: seen.size,
            to: now.size,
        },
    );
    assert_eq!(check_extends(&seen, &now, &proof), Ok(()));

    // A proof at an older head still verifies against that head.
    let release: Release = ask(
        &url,
        &Request::Release {
            package: name("@acme/postcodes"),
            version: version("2.2.0"),
        },
    );
    let at_now: InclusionProof = ask(
        &url,
        &Request::LogProof {
            digest: release.digest.clone(),
            size: 4,
        },
    );
    let entry = LogEntry {
        package: name("@acme/postcodes"),
        version: version("2.2.0"),
        digest: release.digest.clone(),
    };
    assert_eq!(check_included(&now, &entry, &at_now), Ok(()));
    let early = send(
        &url,
        &Request::LogProof {
            digest: release.digest,
            size: 3,
        },
    );
    assert_eq!(failure(&early), (404, "not_in_log".to_string()));
}

#[test]
fn a_registry_showing_another_history_is_caught() {
    let honest = registry();
    let honest_served = honest.serve().unwrap();
    let seen: LogHead = ask(&honest_served.url(), &Request::LogHead);

    // Another registry that logged a different first release, then the
    // same others: its log can't be shown to extend the head seen.
    let other = LocalRegistry::new();
    other.publisher("@acme", true, false);
    let mut swapped = seed("@acme/postcodes", "2.0.0");
    swapped.files[1].1 = b"Sends postcodes elsewhere.\n".to_vec();
    other.release(swapped);
    other.release(seed("@acme/postcodes", "2.1.0"));
    other.release(seed("@acme/geocoder", "1.0.0"));
    other.release(seed("@acme/postcodes", "2.2.0"));
    let other_served = other.serve().unwrap();
    let now: LogHead = ask(&other_served.url(), &Request::LogHead);
    let proof: ConsistencyProof = ask(
        &other_served.url(),
        &Request::LogConsistency {
            from: seen.size,
            to: now.size,
        },
    );
    assert_eq!(
        check_extends(&seen, &now, &proof),
        Err(LogError::DoesNotExtend)
    );
}

#[test]
fn failures_come_in_the_failure_envelope() {
    let registry = registry();
    let served = registry.serve().unwrap();
    let url = served.url();
    let missing = send(
        &url,
        &Request::Package {
            package: name("@acme/nothing"),
        },
    );
    assert_eq!(failure(&missing), (404, "package_not_found".into()));
    let missing = send(
        &url,
        &Request::Release {
            package: name("@acme/postcodes"),
            version: version("9.0.0"),
        },
    );
    assert_eq!(failure(&missing), (404, "release_not_found".into()));
    let missing = send(
        &url,
        &Request::Release {
            package: name("@acme/nothing"),
            version: version("1.0.0"),
        },
    );
    assert_eq!(failure(&missing), (404, "package_not_found".into()));
    let digest = Digest::parse(&format!("sha256:{}", "0".repeat(64))).unwrap();
    let missing = send(
        &url,
        &Request::Archive {
            digest: digest.clone(),
        },
    );
    assert_eq!(failure(&missing), (404, "archive_not_found".into()));
    let missing = send(&url, &Request::LogProof { digest, size: 3 });
    assert_eq!(failure(&missing), (404, "not_in_log".into()));
    let ahead = send(&url, &Request::LogConsistency { from: 1, to: 9 });
    assert_eq!(failure(&ahead), (422, "size_past_head".into()));
    let backwards = send(&url, &Request::LogConsistency { from: 3, to: 2 });
    assert_eq!(failure(&backwards), (422, "size_past_head".into()));
    let release: Release = ask(
        &url,
        &Request::Release {
            package: name("@acme/postcodes"),
            version: version("2.1.0"),
        },
    );
    let ahead = send(
        &url,
        &Request::LogProof {
            digest: release.digest,
            size: 9,
        },
    );
    assert_eq!(failure(&ahead), (422, "size_past_head".into()));
    let missing = send(
        &url,
        &Request::ReleaseRequest {
            id: "rel_nothing".into(),
        },
    );
    assert_eq!(failure(&missing), (404, "release_request_not_found".into()));

    // What the table doesn't have, and what can't be read.
    let off = fetch(Method::Get, &format!("{url}nothing"), None).unwrap();
    assert_eq!(failure(&off), (404, "not_found".into()));
    let bad = fetch(Method::Post, &format!("{url}releases"), Some(b"{")).unwrap();
    assert_eq!(failure(&bad), (400, "invalid_request".into()));
    let bad = fetch(Method::Get, &format!("{url}log/consistency?from=1"), None).unwrap();
    assert_eq!(failure(&bad), (422, "invalid_input".into()));
    let refused = fetch(
        Method::Post,
        &format!("{url}releases"),
        Some(br#"{"repository":"http://github.com/a/b","tag":"v1","dry_run":false}"#),
    )
    .unwrap();
    assert_eq!(failure(&refused), (422, "repository_refused".into()));
}

#[test]
fn a_release_request_is_followed_until_it_settles() {
    let registry = registry();
    let served = registry.serve().unwrap();
    let url = served.url();
    let started: StartedRelease = ask(
        &url,
        &Request::StartRelease(jadpo_registry::protocol::StartRelease {
            repository: "https://github.com/acme/postcodes".into(),
            tag: "v2.3.0".into(),
            dry_run: true,
            publisher: None,
        }),
    );
    let state: ReleaseRequest = ask(
        &url,
        &Request::ReleaseRequest {
            id: started.id.clone(),
        },
    );
    assert_eq!(state.state, ReleaseState::Verifying);
    assert!(state.facts.is_none() && state.change_record.is_none());
    assert_eq!(
        registry.started(),
        vec![(
            "https://github.com/acme/postcodes".to_string(),
            "v2.3.0".to_string(),
            true
        )]
    );

    registry.settle(
        &started.id,
        ReleaseState::Failed,
        vec![("size".into(), false, "Over 5 MB.".into())],
    );
    let state: ReleaseRequest = ask(
        &url,
        &Request::ReleaseRequest {
            id: started.id.clone(),
        },
    );
    assert_eq!(state.state, ReleaseState::Failed);
    assert_eq!(state.bar.len(), 1);
    assert!(!state.bar[0].passed);
}

#[test]
fn search_puts_the_standard_library_first_and_names_the_declaration() {
    let registry = registry();
    registry.standard("text.slug", "Turns text into a URL slug.");
    registry.account(
        &name("@acme/postcodes"),
        "Postcodes.uk.parse",
        "Parses a UK postcode into its parts.",
    );
    registry.account(
        &name("@acme/geocoder"),
        "Geocoder.lookup",
        "Turns a postcode into a point.",
    );
    let served = registry.serve().unwrap();
    let answer: SearchAnswer = ask(
        &served.url(),
        &Request::Search {
            q: "postcode".into(),
            from: None,
            to: None,
            kind: None,
        },
    );
    let found: Vec<&str> = answer
        .results
        .iter()
        .map(|r| r.declaration.as_str())
        .collect();
    assert_eq!(found, ["Postcodes.uk.parse", "Geocoder.lookup"]);
    assert!(answer.results.iter().all(|r| !r.why.is_empty()));

    let answer: SearchAnswer = ask(
        &served.url(),
        &Request::Search {
            q: "text slug".into(),
            from: None,
            to: None,
            kind: None,
        },
    );
    assert_eq!(answer.results[0].declaration, "text.slug");
    assert_eq!(answer.results[0].package, None);
}

#[test]
fn advisories_are_found_for_the_installed_versions() {
    let registry = registry();
    registry.advisory(Advisory {
        id: "JADPO-2026-0001".into(),
        package: name("@acme/postcodes"),
        declarations: vec!["Postcodes.uk.parse".into()],
        from: version("2.0.0"),
        to: version("2.0.9"),
        summary: "Accepts a postcode with a trailing letter.".into(),
        copies: vec![name("@acme/geocoder")],
    });
    let served = registry.serve().unwrap();
    let ask_for = |v: &str| -> AdvisoryAnswer {
        ask(
            &served.url(),
            &Request::QueryAdvisories(jadpo_registry::protocol::AdvisoryQuery {
                packages: vec![jadpo_registry::protocol::Installed {
                    package: name("@acme/postcodes"),
                    version: version(v),
                }],
            }),
        )
    };
    assert_eq!(ask_for("2.0.0").advisories.len(), 1);
    assert_eq!(
        ask_for("2.0.9").advisories[0].copies,
        vec![name("@acme/geocoder")]
    );
    assert!(ask_for("2.1.0").advisories.is_empty());
}

#[test]
fn a_fingerprint_query_answers_each_prefix_s_whole_bucket() {
    let registry = registry();
    let source = |hex: &str| Digest::parse(&format!("sha256:{hex:0<64}")).unwrap();
    let published = |package: &str, v: &str, declaration: &str, hex: &str| PublishedFingerprint {
        package: name(package),
        version: version(v),
        declaration: declaration.into(),
        source: source(hex),
    };
    registry.fingerprint(published("@acme/postcodes", "2.0.0", "uk.parse", "3fa9c1"));
    registry.fingerprint(published("@acme/postcodes", "2.1.0", "uk.parse", "3fa9c1"));
    registry.fingerprint(published(
        "@acme/postcodes",
        "2.1.0",
        "uk.outward",
        "3fa9c2",
    ));
    registry.fingerprint(published("@acme/geocoder", "1.0.0", "lookup", "00e1b7"));
    registry.yank(&name("@acme/geocoder"), &version("1.0.0"));
    let served = registry.serve().unwrap();
    let ask_for = |prefixes: &[&str]| -> Vec<String> {
        let answer: FingerprintAnswer = ask(
            &served.url(),
            &Request::QueryFingerprints(FingerprintQuery {
                prefixes: prefixes.iter().map(|p| p.to_string()).collect(),
            }),
        );
        answer
            .fingerprints
            .iter()
            .map(|f| format!("{} {} {}", f.package, f.version, f.declaration))
            .collect()
    };
    // Everything in the bucket, so the registry can't tell which one the
    // app holds; of one declaration's releases, the newest.
    assert_eq!(
        ask_for(&["3fa9c"]),
        [
            "@acme/postcodes 2.1.0 uk.parse",
            "@acme/postcodes 2.1.0 uk.outward"
        ]
    );
    // A yanked release is never offered.
    assert!(ask_for(&["00e1b"]).is_empty());
    assert!(ask_for(&["fffff"]).is_empty());
    assert!(ask_for(&[]).is_empty());
}

#[test]
fn yanked_and_urgent_releases_say_so() {
    let registry = registry();
    registry.yank(&name("@acme/postcodes"), &version("2.0.0"));
    registry.urgent(&name("@acme/postcodes"), &version("2.1.0"));
    let served = registry.serve().unwrap();
    let package: Package = ask(
        &served.url(),
        &Request::Package {
            package: name("@acme/postcodes"),
        },
    );
    assert!(package.releases[0].yanked && !package.releases[0].urgent);
    assert!(!package.releases[1].yanked && package.releases[1].urgent);
    // A yanked release stays in the log and its archive stays served.
    let answer = send(
        &served.url(),
        &Request::Archive {
            digest: package.releases[0].digest.clone(),
        },
    );
    assert_eq!(answer.status, 200);
}

// --- whose release it is ----------------------------------------------------

fn started_at(registry: &LocalRegistry, repository: &str) -> String {
    let url = registry.serve().unwrap();
    let started: StartedRelease = ask(
        &url.url(),
        &Request::StartRelease(jadpo_registry::protocol::StartRelease {
            repository: repository.into(),
            tag: "v2.3.0".into(),
            dry_run: false,
            publisher: None,
        }),
    );
    started.id
}

fn facts_of(publisher: Option<&str>) -> jadpo_registry::protocol::Facts {
    let mut facts = ReleaseSeed::new(
        ScopedName::parse("@acme/postcodes").unwrap(),
        Version::parse("2.3.0").unwrap(),
        Vec::new(),
    )
    .facts;
    facts.publisher = publisher.map(|handle| Handle::parse(handle).unwrap());
    facts
}

fn publisher_row(registry: &LocalRegistry, id: &str) -> (ReleaseState, Option<String>) {
    let served = registry.serve().unwrap();
    let state: ReleaseRequest = ask(
        &served.url(),
        &Request::ReleaseRequest { id: id.to_string() },
    );
    let row = state
        .bar
        .iter()
        .find(|row| row.check == "publisher" && !row.passed)
        .map(|row| row.detail.clone());
    (state.state, row)
}

/// A registry holding `@acme/postcodes`, released from its own repository.
fn holding_acme() -> LocalRegistry {
    let registry = LocalRegistry::new();
    let mut seed = ReleaseSeed::new(
        ScopedName::parse("@acme/postcodes").unwrap(),
        Version::parse("2.2.0").unwrap(),
        vec![(
            "package.jadpo".to_string(),
            b"package Postcodes.v2 {}\n".to_vec(),
        )],
    );
    seed.repository = "https://github.com/acme/postcodes".into();
    registry.release(seed);
    registry
}

const ROWS: fn() -> Vec<(String, bool, String)> =
    || vec![("tests".to_string(), true, "3 tests".to_string())];

#[test]
fn a_release_under_its_own_publisher_goes_on_to_a_releaser() {
    let registry = holding_acme();
    let id = started_at(&registry, "https://github.com/acme/postcodes");
    registry.checked(&id, "Postcodes", facts_of(Some("@acme")), ROWS());
    assert_eq!(publisher_row(&registry, &id), (ReleaseState::Pending, None));

    // A first release from a repository no package has: its publisher's
    // releasers are the only ones who can confirm it.
    let id = started_at(&registry, "https://github.com/other/tides");
    registry.checked(&id, "Tides", facts_of(Some("@other")), ROWS());
    assert_eq!(publisher_row(&registry, &id), (ReleaseState::Pending, None));
    let served = registry.serve().unwrap();
    let state: ReleaseRequest = ask(&served.url(), &Request::ReleaseRequest { id });
    assert_eq!(
        state.facts.and_then(|facts| facts.publisher),
        Some(Handle::parse("@other").unwrap())
    );
}

#[test]
fn a_fork_can_t_be_released_as_the_original() {
    let registry = holding_acme();
    // A fork that still names the original.
    let id = started_at(&registry, "https://github.com/mallory/postcodes");
    registry.checked(&id, "Postcodes", facts_of(Some("@acme")), ROWS());
    let (state, row) = publisher_row(&registry, &id);
    assert_eq!(state, ReleaseState::Failed);
    let row = row.expect("a failed row");
    assert!(row.contains("@acme/postcodes"), "{row}");
    assert!(row.contains("https://github.com/acme/postcodes"), "{row}");

    // A fork under its own name, from its own repository, is its own package.
    let id = started_at(&registry, "https://github.com/mallory/postcodes");
    registry.checked(&id, "Postcodes", facts_of(Some("@mallory")), ROWS());
    assert_eq!(publisher_row(&registry, &id), (ReleaseState::Pending, None));
}

#[test]
fn a_repository_s_release_can_t_name_another_publisher() {
    let registry = holding_acme();
    let id = started_at(&registry, "https://github.com/acme/postcodes");
    registry.checked(&id, "Postcodes", facts_of(Some("@mallory")), ROWS());
    let (state, row) = publisher_row(&registry, &id);
    assert_eq!(state, ReleaseState::Failed);
    let row = row.expect("a failed row");
    assert!(row.contains("@mallory"), "{row}");
    assert!(row.contains("@acme"), "{row}");
}

/// Told who a release is for, the registry refuses at once a repository
/// another publisher releases from, saying so, and starts nothing.
#[test]
fn a_repository_another_publisher_releases_from_is_refused_at_the_start() {
    let registry = holding_acme();
    let served = registry.serve().unwrap();
    let start = |publisher: &str| jadpo_registry::protocol::StartRelease {
        repository: "https://github.com/acme/postcodes".into(),
        tag: "v2.3.0".into(),
        dry_run: false,
        publisher: Some(Handle::parse(publisher).unwrap()),
    };
    let answer = send(&served.url(), &Request::StartRelease(start("@mallory")));
    assert_eq!(failure(&answer), (422, "repository_refused".to_string()));
    let body: serde_json::Value = serde_json::from_slice(&answer.body).unwrap();
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("@acme/postcodes") && message.contains("@mallory"),
        "{message}"
    );
    assert!(registry.started().is_empty());

    // Its own publisher's is started, and who it was for is kept.
    let started: StartedRelease = ask(&served.url(), &Request::StartRelease(start("@acme")));
    assert_eq!(started.id, "rel_0001");
    assert_eq!(
        registry.started_for(),
        vec![Some(Handle::parse("@acme").unwrap())]
    );
}

#[test]
fn a_release_with_no_publisher_is_refused() {
    let registry = holding_acme();
    let id = started_at(&registry, "https://github.com/other/tides");
    registry.checked(&id, "Tides", facts_of(None), ROWS());
    let (state, row) = publisher_row(&registry, &id);
    assert_eq!(state, ReleaseState::Failed);
    assert!(row.expect("a failed row").contains("names no publisher"));
}

#[test]
fn a_failing_bar_fails_whoever_it_is_from() {
    let registry = holding_acme();
    let id = started_at(&registry, "https://github.com/acme/postcodes");
    registry.checked(
        &id,
        "Postcodes",
        facts_of(Some("@acme")),
        vec![("tests".to_string(), false, "1 of 3 fails".to_string())],
    );
    assert_eq!(publisher_row(&registry, &id), (ReleaseState::Failed, None));
}

#[test]
fn a_head_is_signed_by_the_registrys_key() {
    let registry = registry();
    let served = registry.serve().expect("serves");
    let answer = fetch(
        Method::Get,
        &format!("{}log/signed-head", served.url()),
        None,
    )
    .expect("an answer");
    assert_eq!(answer.status, 200);
    let signed: SignedLogHead = serde_json::from_slice(&answer.body).expect("a signed head");
    let keys = vec![LocalRegistry::public_key()];
    let head = check_signed_head(&signed, &keys).expect("signed by the key");
    assert_eq!(head, registry.head());

    // A key the client doesn't hold is refused however good the signature.
    assert_eq!(
        check_signed_head(&signed, &["someone-else".to_string()]),
        Err(LogError::UntrustedKey)
    );
    // A signature is over this size and this root.
    let moved = SignedLogHead {
        size: signed.size + 1,
        ..signed.clone()
    };
    assert_eq!(
        check_signed_head(&moved, &keys),
        Err(LogError::BadSignature)
    );
    let unreadable = SignedLogHead {
        signature: "not base64".into(),
        ..signed
    };
    assert_eq!(
        check_signed_head(&unreadable, &keys),
        Err(LogError::BadSignature)
    );
}

#[test]
fn a_contract_runs_report_holds_a_shape_and_no_value() {
    let answer = serde_json::json!({
        "id": 42,
        "customer": { "email": "ada@example.com", "name": "Ada Lovelace" },
        "items": [{ "total": "9.99" }, { "total": null }, { "voided": true }],
        "by_user": { "usr_123": "Ada", "bob@example.com": "Bob" },
    });
    let shape = shape_of(&answer);
    let text = serde_json::to_string(&shape).unwrap();
    for value in ["Ada", "ada@example", "9.99", "42", "usr_123", "bob@example"] {
        assert!(!text.contains(value), "{value} left in {text}");
    }
    let of = |path: &str, kind: ShapeKind| ShapeMember {
        path: path.into(),
        kind,
    };
    for member in [
        of("$", ShapeKind::Object),
        of("$.id", ShapeKind::Number),
        of("$.customer.email", ShapeKind::Text),
        of("$.items", ShapeKind::List),
        of("$.items[].total", ShapeKind::Text),
        of("$.items[].total", ShapeKind::Null),
        of("$.items[].voided", ShapeKind::Bool),
        of("$.by_user.*", ShapeKind::Text),
    ] {
        assert!(shape.contains(&member), "{member:?} not in {shape:?}");
    }

    let registry = registry();
    let served = registry.serve().expect("serves");
    let report = DecodeFailure {
        package: name("@acme/couriers"),
        version: version("2.1.0"),
        operation: "bookings.create".into(),
        shape,
    };
    let answer = fetch(
        Method::Post,
        &format!("{}decode-failures", served.url()),
        Some(serde_json::to_vec(&report).unwrap().as_slice()),
    )
    .expect("an answer");
    assert_eq!(answer.status, 202);
    let received: ReceivedReport = serde_json::from_slice(&answer.body).expect("an id");
    assert!(!received.id.is_empty());
    assert_eq!(registry.decode_failures(), [report]);
}

/// However many members an answer has, a report fits what the protocol
/// takes.
#[test]
fn a_report_keeps_to_what_the_protocol_takes() {
    let members: serde_json::Map<String, serde_json::Value> = (0..MOST_SHAPE_MEMBERS * 2)
        .map(|n| (format!("m{}", "a".repeat(n % 60)), serde_json::json!(n)))
        .chain((0..2000).map(|n| (format!("x{n}"), serde_json::json!(n))))
        .collect();
    let mut deep = serde_json::Value::Null;
    for _ in 0..200 {
        deep = serde_json::json!({ "deeper": deep });
    }
    let shape = shape_of(&serde_json::json!({ "wide": members, "deep": deep }));
    assert!(shape.len() <= MOST_SHAPE_MEMBERS);
    assert!(
        shape
            .iter()
            .all(|member| member.path.len() <= LONGEST_SHAPE_PATH)
    );
}
