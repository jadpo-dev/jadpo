//! Connections to packages and the network report (area 11, "Connecting
//! in `app.jadpo`"; chunk 7.1c): a connection names a provider package by
//! its name and version, lists the domains that may call it, gives its
//! credential, environment or host from settings and the package's own
//! settings by name; `jadpo check` refuses what the package doesn't take
//! or needs, and writes `build/report/network.md` and `.json`.

use std::path::{Path, PathBuf};

use jadpo_db::{Locked, Source, package_digest, write_lock};

use super::http::{jadpo, tempdir, write};

/// An app with two connections to its own integrations: one with fixed
/// live and sandbox hosts, one at an address a setting gives, with a
/// setting of its own.
const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 5s
        }
        ledger: {
            package: Ledger.v1
            domains: [billing]
            credential: config.ledger_token
            host: config.ledger_address
            account: "shop"
        }
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
    ledger_address: Url
    ledger_token: Text {
        secret: true
    }
}
"#,
    ),
    (
        "domains/billing/actions/file_invoice.jadpo",
        r#"use billing/types { InvoiceCode }

/// Files an invoice under its code.
action file_invoice(code: InvoiceCode) -> InvoiceCode {
    return code
}
"#,
    ),
    (
        "domains/billing/types/invoice_code.jadpo",
        r#"/// The code an invoice is filed under.
type InvoiceCode = Text {
    max_length: 20
}
"#,
    ),
    (
        "domains/shipping/actions/read_reference.jadpo",
        r#"use shipping/types { ParcelReference }

/// Reads a parcel's reference.
action read_reference(reference: ParcelReference) -> ParcelReference {
    return reference
}
"#,
    ),
    (
        "domains/shipping/types/parcel_reference.jadpo",
        r#"/// A courier's reference for a parcel.
type ParcelReference = Text {
    max_length: 40
}
"#,
    ),
    (
        "shared/providers/acme_couriers/v1/package.jadpo",
        r#"/// Acme Couriers' booking API, written from its documentation.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#,
    ),
    (
        "shared/providers/acme_couriers/v1/provider.jadpo",
        r#"/// Acme Couriers' booking and tracking API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-couriers.example"
        sandbox: "https://sandbox.acme-couriers.example"
    }
    credential: CredentialScheme.bearer
}
"#,
    ),
    (
        "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
        r#"public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        changes: true
        idempotent: true
        source: PackageSource.written
    }
}
"#,
    ),
    (
        "shared/providers/acme_couriers/v1/types/booking.jadpo",
        r#"/// A booked collection.
public type Booking = Object {
    id: Text
}
"#,
    ),
    (
        "shared/providers/acme_couriers/v1/types/create_booking.jadpo",
        r#"/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Text
    slot: Text
}
"#,
    ),
    (
        "shared/providers/ledger/v1/package.jadpo",
        r#"/// The shop's ledger service, run by its finance team.
package Ledger.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#,
    ),
    (
        "shared/providers/ledger/v1/provider.jadpo",
        r#"/// The ledger's API, at the address each deployment gives.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    settings: {
        account: Text
    }
}
"#,
    ),
    (
        "shared/providers/ledger/v1/resources/postings.jadpo",
        r#"public resource postings {
    create: {
        method: HttpMethod.post
        path: "/v1/postings"
        input: NewPosting
        output: Posting
        changes: true
        idempotent: false
        source: PackageSource.written
    }
}
"#,
    ),
    (
        "shared/providers/ledger/v1/types/posting.jadpo",
        r#"/// A recorded amount.
public type Posting = Object {
    id: Text
}
"#,
    ),
    (
        "shared/providers/ledger/v1/types/new_posting.jadpo",
        r#"/// An amount to record.
public type NewPosting = Object {
    reference: Text
    pence: Int
}
"#,
    ),
];

fn app(name: &str) -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("shop");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    (dir, root)
}

/// `app.jadpo` with `from` replaced by `to`.
fn change(root: &Path, from: &str, to: &str) {
    let path = root.join("app.jadpo");
    let text = std::fs::read_to_string(&path).expect("app.jadpo");
    assert!(text.contains(from), "{from}");
    std::fs::write(path, text.replacen(from, to, 1)).expect("app.jadpo");
}

/// Checks the app, and gives the rules of what it reports.
fn rules(root: &Path) -> Vec<String> {
    let done = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&done.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .map(|all| {
            all.iter()
                .map(|d| d["rule"].as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_connected_app_checks_and_writes_the_network_report() {
    let (_dir, root) = app("connections-report");
    let done = jadpo(&root, &["check"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let report = std::fs::read_to_string(root.join("build/report/network.md")).expect("report");
    assert_eq!(
        report,
        "# Network

Every host the app can reach, and the connection that reaches it. Jadpo code reaches the network only through connections in `app.jadpo`. Written by `jadpo check`.

| Connection | Host | When | For | Domains |
|---|---|---|---|---|
| `couriers` | `https://api.acme-couriers.example` | `config.couriers_environment` is `live` | `AcmeCouriers.v1` | `shipping` |
| `couriers` | `https://sandbox.acme-couriers.example` | `config.couriers_environment` is `sandbox` | `AcmeCouriers.v1` | `shipping` |
| `ledger` | the address in `config.ledger_address` | always | `Ledger.v1` | `billing` |
"
    );
    let json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/report/network.json")).expect("twin"),
    )
    .expect("JSON");
    assert_eq!(
        json["hosts"][0]["address"],
        "https://api.acme-couriers.example"
    );
    assert_eq!(json["hosts"][0]["when"]["setting"], "couriers_environment");
    assert_eq!(json["hosts"][0]["when"]["is"], "live");
    assert_eq!(json["hosts"][2]["setting"], "ledger_address");
    assert_eq!(json["hosts"][2]["for"], "Ledger.v1");
    assert_eq!(json["hosts"][2]["domains"][0], "billing");
}

#[test]
fn the_report_lists_a_database_and_says_when_nothing_is_reached() {
    let (_dir, root) = app("connections-database");
    write(&root, "app.jadpo", "application {}\n");
    assert_eq!(rules(&root), Vec::<String>::new());
    let report = std::fs::read_to_string(root.join("build/report/network.md")).expect("report");
    assert!(report.ends_with("The app reaches no host.\n"), "{report}");
    write(
        &root,
        "config.jadpo",
        "config {\n    database_url: PostgresUrl\n}\n",
    );
    assert_eq!(rules(&root), Vec::<String>::new());
    let report = std::fs::read_to_string(root.join("build/report/network.md")).expect("report");
    assert!(
        report.ends_with(
            "| `main` | the address in `config.database_url` | always | database | none |\n"
        ),
        "{report}"
    );
}

/// A database reached without TLS, and one checked against a private
/// authority, say so in the report, where a review sees them (area 5,
/// "PostgreSQL over TLS"; chunk 8.1n).
#[test]
fn the_report_lists_how_a_database_is_secured() {
    let (_dir, root) = app("connections-database-tls");
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n            authority: config.database_authority\n        }\n        legacy: {\n            driver: Driver.postgres\n            url: config.legacy_url\n            tls: false\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    database_authority: Certificate\n    database_url: PostgresUrl\n    legacy_url: PostgresUrl\n}\n",
    );
    assert_eq!(rules(&root), Vec::<String>::new());
    let report = std::fs::read_to_string(root.join("build/report/network.md")).expect("report");
    assert!(
        report.ends_with(
            "| `legacy` | the address in `config.legacy_url` | always | database, without TLS | none |\n\
             | `main` | the address in `config.database_url` | always | database, its certificate checked against `config.database_authority` | none |\n"
        ),
        "{report}"
    );
    let json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/report/network.json")).expect("twin"),
    )
    .expect("JSON");
    assert_eq!(json["hosts"][0]["for"], "database");
    assert_eq!(json["hosts"][0]["tls"], false);
    assert_eq!(json["hosts"][1]["authority"], "database_authority");
    // Saying `tls: true` says nothing, and is refused.
    change(&root, "tls: false", "tls: true");
    assert_eq!(rules(&root), ["connection.value_invalid"]);
}

#[test]
fn a_connection_names_a_provider_package_the_app_holds() {
    let (_dir, root) = app("connections-unknown");
    change(&root, "AcmeCouriers.v1", "AcmeCouriers.v2");
    assert_eq!(rules(&root), ["connection.package_unknown"]);
    // No report while the connections don't read cleanly.
    assert!(!root.join("build/report/network.md").exists());
}

#[test]
fn a_library_package_needs_no_connection() {
    let (_dir, root) = app("connections-library");
    let index = "/// UK postcodes.\npackage Postcodes.v2 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n";
    let ty = "/// A UK postcode.\npublic type UkPostcode = Text {\n    max_length: 8\n}\n";
    write(&root, "packages/postcodes/v2/package.jadpo", index);
    write(&root, "packages/postcodes/v2/types/uk_postcode.jadpo", ty);
    let held: Vec<(&str, &[u8])> = vec![
        ("package.jadpo", index.as_bytes()),
        ("types/uk_postcode.jadpo", ty.as_bytes()),
    ];
    write(
        &root,
        "jadpo.lock",
        &write_lock(&[Locked {
            package: "Postcodes.v2".to_string(),
            source: Source::Path {
                version: "2.0.0".to_string(),
                path: "../postcodes".to_string(),
                digest: package_digest(&held),
            },
        }]),
    );
    change(
        &root,
        "    connections: {\n",
        "    connections: {\n        postcodes: {\n            package: Postcodes.v2\n            domains: [shipping]\n        }\n",
    );
    let done = jadpo(&root, &["check"]);
    assert!(
        done.stdout
            .contains("error: `postcodes` connects to library package `Postcodes.v2`"),
        "{}",
        done.stdout
    );
    assert!(
        done.stdout.contains("use packages/postcodes"),
        "{}",
        done.stdout
    );
    assert_eq!(rules(&root), ["connection.package_unknown"]);
}

#[test]
fn a_connection_gives_only_the_keys_its_package_takes() {
    let cases: &[(&str, &str, &[&str])] = &[
        ("timeout: 5s", "timout: 5s", &["connection.key_invalid"]),
        (
            "timeout: 5s",
            "host: config.ledger_address",
            &["connection.key_invalid"],
        ),
        (
            "            account: \"shop\"\n",
            "            account: \"shop\"\n            environment: config.couriers_environment\n",
            &["connection.key_invalid"],
        ),
        (
            "            domains: [shipping]\n",
            "",
            &["connection.key_missing"],
        ),
        (
            "            credential: config.couriers_api_key\n",
            "",
            &["connection.key_missing"],
        ),
        (
            "            environment: config.couriers_environment\n",
            "",
            &["connection.key_missing"],
        ),
        (
            "            host: config.ledger_address\n",
            "",
            &["connection.key_missing"],
        ),
        (
            "            account: \"shop\"\n",
            "",
            &["connection.key_missing"],
        ),
        (
            "domains: [shipping]",
            "domains: [shiping]",
            &["connection.value_invalid"],
        ),
        (
            "domains: [shipping]",
            "domains: []",
            &["connection.value_invalid"],
        ),
        (
            "domains: [shipping]",
            "domains: [shipping, shipping]",
            &["connection.value_invalid"],
        ),
        (
            "credential: config.couriers_api_key",
            "credential: config.couriers_environment",
            &["connection.value_invalid"],
        ),
        (
            "credential: config.couriers_api_key",
            "credential: \"sk_live\"",
            &["connection.value_invalid"],
        ),
        (
            "environment: config.couriers_environment",
            "environment: config.ledger_address",
            &["connection.value_invalid"],
        ),
        (
            "host: config.ledger_address",
            "host: config.couriers_environment",
            &["connection.value_invalid"],
        ),
        (
            "account: \"shop\"",
            "account: 3",
            &["connection.value_invalid"],
        ),
    ];
    for (from, to, expected) in cases {
        let (_dir, root) = app("connections-keys");
        change(&root, from, to);
        assert_eq!(rules(&root), *expected, "{from} -> {to}");
    }
}

#[test]
fn a_providers_settings_are_plain_values() {
    let (_dir, root) = app("connections-settings");
    let path = root.join("shared/providers/ledger/v1/provider.jadpo");
    let text = std::fs::read_to_string(&path).expect("provider");
    std::fs::write(path, text.replace("account: Text", "account: Uuid")).expect("provider");
    let found = rules(&root);
    assert!(
        found.contains(&"package.provider_invalid".to_string()),
        "{found:?}"
    );
}

#[test]
fn readiness_plan_names_declared_provider_connections_in_order() {
    let (_dir, root) = app("connections-health");
    write(
        &root,
        "app.jadpo",
        r#"application {
    connections: {
        ledger: {
            package: Ledger.v1
            domains: [billing]
            credential: config.ledger_token
            host: config.ledger_address
            account: "shop"
        }
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
}
"#,
    );
    let loaded = jadpo_db::load(&root);
    let diagnostics = jadpo_check::check(&loaded.db, loaded.project);
    assert!(
        !diagnostics.iter().any(jadpo_check::blocks_running),
        "{diagnostics:?}"
    );
    let plan = jadpo_plan::Plan::build(&loaded.db, loaded.project);
    let routes = serde_json::to_value(plan.routes()).expect("route metadata");
    assert_eq!(
        routes["integrations"],
        serde_json::json!(["couriers", "ledger"])
    );
    assert!(
        plan.routes().stores.is_empty(),
        "no entity store was declared"
    );
}
