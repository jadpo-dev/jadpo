//! The protocol's requests (area 11, "The registry protocol"): each is
//! one row of the table, written as the URL and body the `jadpo` tool
//! sends and read back from them by the local registry.

use jadpo_registry::names::{Digest, ScopedName, Version};
use jadpo_registry::protocol::{
    AdvisoryQuery, BASE, ENDPOINTS, Installed, Method, PackageKind, Request, StartRelease,
};
use jadpo_registry::shape::{Shape, check_value};

fn digest() -> Digest {
    Digest::parse("sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        .unwrap()
}

fn requests() -> Vec<Request> {
    let package = ScopedName::parse("@acme/postcodes").unwrap();
    vec![
        Request::Package {
            package: package.clone(),
        },
        Request::Release {
            package: package.clone(),
            version: Version::parse("2.1.0-beta.1").unwrap(),
        },
        Request::Archive { digest: digest() },
        Request::LogHead,
        Request::LogProof {
            digest: digest(),
            size: 12,
        },
        Request::LogConsistency { from: 3, to: 12 },
        Request::Search {
            q: "uk postcode & h3 / 50%".into(),
            from: None,
            to: None,
            kind: None,
        },
        Request::Search {
            q: "geocode".into(),
            from: Some("Text".into()),
            to: Some("GeoPoint?".into()),
            kind: Some(PackageKind::Library),
        },
        Request::QueryAdvisories(AdvisoryQuery {
            packages: vec![Installed {
                package,
                version: Version::parse("2.1.0").unwrap(),
            }],
        }),
        Request::StartRelease(StartRelease {
            repository: "https://github.com/acme/postcodes".into(),
            tag: "v2.1.0".into(),
            dry_run: true,
        }),
        Request::ReleaseRequest {
            id: "rel_01".into(),
        },
    ]
}

#[test]
fn the_protocol_is_served_under_its_versioned_path() {
    assert_eq!(BASE, "/registry/v1/");
    assert_eq!(
        Request::Package {
            package: ScopedName::parse("@acme/postcodes").unwrap()
        }
        .target(),
        "/registry/v1/packages/@acme/postcodes"
    );
    assert_eq!(Request::LogHead.target(), "/registry/v1/log/head");
    assert_eq!(
        Request::LogProof {
            digest: digest(),
            size: 4
        }
        .target(),
        format!("/registry/v1/log/proof/{}?size=4", digest())
    );
    assert_eq!(
        Request::LogConsistency { from: 1, to: 4 }.target(),
        "/registry/v1/log/consistency?from=1&to=4"
    );
}

#[test]
fn every_request_reads_back_from_what_is_sent() {
    for request in requests() {
        let body = request.body();
        let read = Request::parse(
            request.method(),
            &request.target(),
            body.as_deref().unwrap_or("").as_bytes(),
        )
        .unwrap_or_else(|e| panic!("{} {}: {e:?}", request.method(), request.target()));
        assert_eq!(read, request);
    }
}

#[test]
fn every_request_is_a_row_of_the_table() {
    let mut seen: Vec<&str> = requests().iter().map(|r| r.endpoint().id).collect();
    seen.sort();
    seen.dedup();
    let mut all: Vec<&str> = ENDPOINTS.iter().map(|e| e.id).collect();
    all.sort();
    assert_eq!(seen, all);
    for request in requests() {
        assert_eq!(request.endpoint().method, request.method());
    }
}

#[test]
fn a_request_off_the_table_is_refused() {
    for (method, target) in [
        (Method::Get, "/registry/v1/packages/acme/postcodes"),
        (
            Method::Get,
            "/registry/v1/packages/@acme/postcodes/releases/2.1",
        ),
        (Method::Get, "/registry/v1/archives/sha256:00"),
        (Method::Get, "/registry/v1/log/proof/sha256:00?size=1"),
        (Method::Get, &format!("/registry/v1/log/proof/{}", digest())),
        (
            Method::Get,
            &format!("/registry/v1/log/proof/{}?size=x", digest()),
        ),
        (Method::Get, "/registry/v1/log/consistency?from=1"),
        (Method::Get, "/registry/v1/log/head?extra=1"),
        (Method::Get, "/registry/v1/search"),
        (Method::Get, "/registry/v1/search?q=a&kind=widget"),
        (Method::Get, "/registry/v1/nothing"),
        (Method::Get, "/registry/v2/log/head"),
        (Method::Post, "/registry/v1/log/head"),
        (Method::Delete, "/registry/v1/packages/@acme/postcodes"),
    ] {
        assert!(
            Request::parse(method, target, b"").is_err(),
            "{method} {target}"
        );
    }
    assert!(
        Request::parse(
            Method::Post,
            "/registry/v1/releases",
            b"{\"repository\":\"x\"}"
        )
        .is_err()
    );
    assert!(Request::parse(Method::Post, "/registry/v1/releases", b"not json").is_err());
    assert!(
        Request::parse(
            Method::Post,
            "/registry/v1/releases",
            br#"{"repository":"https://github.com/a/b","tag":"v1","dry_run":false,"token":"x"}"#
        )
        .is_err(),
        "an unknown member is refused"
    );
}

/// What the verifier writes (`jadpo package verify --json`) reads as the
/// protocol's types and their shapes alike, field for field.
#[test]
fn a_release_s_facts_and_change_record_have_one_shape() {
    use jadpo_registry::protocol::{ChangeEntry, Facts};

    let facts = serde_json::json!({
        "kind": "provider",
        "pure": false,
        "hosts": [{"host": "https://api.acme-pay.example", "operations": ["customers.create"]}],
        "changing_calls": ["customers.create"],
        "personal_data": [{
            "field": "CustomerRequest.notes",
            "host": "https://api.acme-pay.example",
            "operation": "customers.create",
            "categories": ["health"],
            "decided_only": false,
        }],
        "erasure": false,
        "storable_types": ["CustomerId"],
        "failures": [{"declaration": "customers.create", "failures": ["CardDeclined"]}],
        "confirmed_operations": [],
        "last_contract_run": null,
        "standard_candidates": [{"declaration": "Customer.created", "standard": "Instant"}],
    });
    check_value(&Shape::Named("Facts"), &facts).expect("the shape");
    let read: Facts = serde_json::from_value(facts.clone()).expect("the type");
    assert_eq!(serde_json::to_value(read).expect("writes"), facts);

    let entry = serde_json::json!({
        "declaration": "uk.parse",
        "kind": "function",
        "change": "behaviour_changed",
        "detail": "example of uk.parse",
        "why": "Upper case broke the courier's lookup.",
    });
    check_value(&Shape::Named("ChangeEntry"), &entry).expect("the shape");
    let read: ChangeEntry = serde_json::from_value(entry.clone()).expect("the type");
    assert_eq!(serde_json::to_value(read).expect("writes"), entry);
}
