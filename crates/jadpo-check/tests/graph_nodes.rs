//! The graph's nodes for areas 8, 10, 11 and 12 (docs/compiler-runtime.md
//! §2.3, "The outside, access and services nodes"; chunk 10.3g): each
//! connection, package, resource, operation, capability mapping, host and
//! lock entry; each principal, sign-in method, sign-in way and generated
//! route; each setting, the locale set and the counted rate limits; and
//! each schedule and subscriber with what it runs, read from the example
//! projects, which check cleanly.

use std::path::Path;

use jadpo_graph::{Detail, EdgeKind, Graph, Key, Kind, NodeId, Status};

fn load(example: &str) -> Graph {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(example);
    let loaded = jadpo_db::load(&root);
    let problems = jadpo_check::check(&loaded.db, loaded.project);
    assert!(
        problems.is_empty(),
        "{example} checks cleanly: {problems:?}"
    );
    Graph::clone(jadpo_check::graph::graph(&loaded.db, loaded.project))
}

fn id(graph: &Graph, key: &str) -> NodeId {
    graph
        .find(&Key::from_text(key))
        .unwrap_or_else(|| panic!("{key} is missing"))
}

fn targets(graph: &Graph, key: &str, kind: EdgeKind) -> Vec<String> {
    graph
        .outgoing(id(graph, key), kind)
        .map(|to| graph.node(to).key.to_string())
        .collect()
}

fn keys_of(graph: &Graph, kind: Kind) -> Vec<String> {
    graph
        .nodes()
        .iter()
        .filter(|node| node.kind == kind)
        .map(|node| node.key.to_string())
        .collect()
}

#[test]
fn every_kind_of_these_areas_is_built() {
    for kind in [
        Kind::Connection,
        Kind::Package,
        Kind::Resource,
        Kind::Operation,
        Kind::CapabilityMapping,
        Kind::Host,
        Kind::LockEntry,
        Kind::GeneratedRoute,
        Kind::Principal,
        Kind::AuthenticationMethod,
        Kind::SignInProvider,
        Kind::ConfigField,
        Kind::LocaleSet,
        Kind::RateLimit,
        Kind::Schedule,
        Kind::Subscriber,
    ] {
        assert_eq!(kind.status(), Status::Built, "{}", kind.word());
    }
    for edge in [EdgeKind::Emits, EdgeKind::Subscribes, EdgeKind::Provides] {
        assert_eq!(edge.status(), Status::Built, "{}", edge.word());
    }
}

#[test]
fn connections_name_their_package_settings_and_hosts() {
    let graph = load("26-delivery-desk");
    let couriers = graph.node(id(&graph, "connection couriers"));
    match &couriers.detail {
        Detail::Connection {
            capability,
            package,
            domains,
            credential,
        } => {
            assert_eq!(capability, "package");
            assert_eq!(package.as_deref(), Some("SwiftCouriers.v1"));
            assert_eq!(domains, &["courier_desk"]);
            assert_eq!(credential.as_deref(), Some("bearer"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        couriers.place.as_ref().map(|place| place.file.as_str()),
        Some("app.jadpo")
    );
    assert!(couriers.digest.is_some());
    let uses = targets(&graph, "connection couriers", EdgeKind::Uses);
    for used in [
        "package SwiftCouriers",
        "config_field couriers_api_key",
        "config_field couriers_environment",
        "config_field couriers_webhook_secret",
    ] {
        assert!(uses.iter().any(|key| key == used), "{used} in {uses:?}");
    }
    // A driver connection is one too, with the capability it serves.
    match &graph.node(id(&graph, "connection mail")).detail {
        Detail::Connection { capability, .. } => assert_eq!(capability, "package"),
        other => panic!("{other:?}"),
    }
    // Each host the network report lists, under its connection.
    for (key, address) in [
        ("host couriers.live", "https://api.swift-couriers.example"),
        (
            "host couriers.sandbox",
            "https://sandbox.swift-couriers.example",
        ),
    ] {
        let host = graph.node(id(&graph, key));
        assert_eq!(
            host.parent.as_ref().map(Key::as_str),
            Some("connection couriers")
        );
        match &host.detail {
            Detail::Host {
                address: written,
                environment,
                ..
            } => {
                assert_eq!(written.as_deref(), Some(address), "{key}");
                assert_eq!(
                    environment.as_ref().map(|(setting, _)| setting.as_str()),
                    Some("couriers_environment")
                );
            }
            other => panic!("{other:?}"),
        }
    }
    let mail = graph.node(id(&graph, "host mail"));
    match &mail.detail {
        Detail::Host { setting, .. } => assert_eq!(setting.as_deref(), Some("resend_host")),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        targets(&graph, "host mail", EdgeKind::Uses),
        ["config_field resend_host"]
    );
}

#[test]
fn packages_their_operations_mappings_and_lock_entries_are_nodes() {
    let graph = load("26-delivery-desk");
    match &graph.node(id(&graph, "package SwiftCouriers")).detail {
        Detail::Package {
            version,
            package_kind,
            installed,
        } => {
            assert_eq!(version, "v1");
            assert_eq!(package_kind, "provider");
            assert!(!installed);
        }
        other => panic!("{other:?}"),
    }
    match &graph.node(id(&graph, "package Postcodes.v2")).detail {
        Detail::Package {
            package_kind,
            installed,
            ..
        } => {
            assert_eq!(package_kind, "library");
            assert!(*installed);
        }
        other => panic!("{other:?}"),
    }
    let resource = graph.node(id(&graph, "resource SwiftCouriers.pickups"));
    assert_eq!(
        resource.parent.as_ref().map(Key::as_str),
        Some("package SwiftCouriers")
    );
    let create = graph.node(id(&graph, "operation SwiftCouriers.pickups.create"));
    assert_eq!(
        create.parent.as_ref().map(Key::as_str),
        Some("resource SwiftCouriers.pickups")
    );
    match &create.detail {
        Detail::Operation {
            method, changes, ..
        } => {
            assert_eq!(method, "POST");
            assert!(*changes);
        }
        other => panic!("{other:?}"),
    }
    assert!(
        targets(
            &graph,
            "operation SwiftCouriers.pickups.create",
            EdgeKind::Raises
        )
        .iter()
        .any(|failure| failure == "type SwiftCouriers.SlotTaken")
    );
    // A callable's call through a connection reaches the operation, and
    // uses the connection.
    let calls = targets(&graph, "action book_pickup", EdgeKind::Calls);
    assert!(
        calls
            .iter()
            .any(|key| key == "operation SwiftCouriers.pickups.create"),
        "{calls:?}"
    );
    assert!(
        targets(&graph, "action book_pickup", EdgeKind::Uses)
            .iter()
            .any(|key| key == "connection couriers")
    );
    // A package's own action calls its own operations.
    assert!(
        targets(
            &graph,
            "action SwiftCouriers.announce_collection",
            EdgeKind::Calls
        )
        .iter()
        .any(|key| key == "operation SwiftCouriers.pickup_collections.retrieve")
    );
    // A capability mapping, which its package provides.
    let mapping = graph.node(id(&graph, "capability_mapping Resend.email"));
    match &mapping.detail {
        Detail::CapabilityMapping { capability } => assert_eq!(capability, "email"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        targets(&graph, "package Resend", EdgeKind::Provides),
        ["capability_mapping Resend.email"]
    );
    // Each lock line.
    let locked = keys_of(&graph, Kind::LockEntry);
    assert_eq!(
        locked,
        [
            "lock_entry AmazonSqs.v2012_11_05",
            "lock_entry Postcodes.v1",
            "lock_entry Postcodes.v2",
            "lock_entry Resend.v1",
        ]
    );
    match &graph.node(id(&graph, "lock_entry Resend.v1")).detail {
        Detail::LockEntry {
            version, source, ..
        } => {
            assert_eq!(version.as_deref(), Some("1.0.0"));
            assert_eq!(source, "path");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        graph
            .node(id(&graph, "lock_entry Resend.v1"))
            .parent
            .as_ref()
            .map(Key::as_str),
        Some("package Resend")
    );
}

#[test]
fn subscribers_and_schedules_say_what_they_run_and_hear() {
    let graph = load("26-delivery-desk");
    assert_eq!(
        targets(&graph, "subscriber scanned", EdgeKind::Subscribes),
        ["type depot.ParcelScanned"]
    );
    assert_eq!(
        targets(&graph, "subscriber scanned", EdgeKind::Runs),
        ["action keep_scan"]
    );
    match &graph.node(id(&graph, "subscriber scanned")).detail {
        Detail::Subscriber { attempts, .. } => assert_eq!(*attempts, 10),
        other => panic!("{other:?}"),
    }
    // A package's own subscriber keeps its package's name.
    assert!(
        graph
            .find(&Key::from_text(
                "subscriber SwiftCouriers.collection_webhook"
            ))
            .is_some(),
        "{:?}",
        keys_of(&graph, Kind::Subscriber)
    );
    // An action emitting an event.
    assert!(
        targets(
            &graph,
            "action SwiftCouriers.announce_collection",
            EdgeKind::Emits
        )
        .iter()
        .any(|event| event == "type SwiftCouriers.ParcelsCollected")
    );
    // A body reading a setting uses it.
    assert!(
        targets(&graph, "action keep_collection", EdgeKind::Uses)
            .iter()
            .any(|key| key == "config_field desk_address")
    );
    let rents = load("25-allotment-rents");
    match &rents.node(id(&rents, "schedule overdue_rent")).detail {
        Detail::Schedule { every_ms } => assert_eq!(*every_ms, 24 * 60 * 60 * 1000),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        targets(&rents, "schedule overdue_rent", EdgeKind::Runs),
        ["action flag_overdue_rent"]
    );
}

#[test]
fn settings_and_the_locale_set_are_nodes() {
    let graph = load("27-guest-house");
    let password = graph.node(id(&graph, "config_field smtp_password"));
    match &password.detail {
        Detail::ConfigField {
            secret, variable, ..
        } => {
            assert!(*secret);
            assert_eq!(variable, "SMTP_PASSWORD");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        password.place.as_ref().map(|place| place.file.as_str()),
        Some("config.jadpo")
    );
    assert!(password.doc.is_some());
    match &graph.node(id(&graph, "config_field site")).detail {
        Detail::ConfigField { default, .. } => {
            assert_eq!(default.as_deref(), Some("https://tygwyn.example"));
        }
        other => panic!("{other:?}"),
    }
    match &graph.node(id(&graph, "locale_set application")).detail {
        Detail::LocaleSet {
            default,
            supported,
            in_progress,
        } => {
            assert_eq!(default, "en_gb");
            assert_eq!(supported, &["en_gb", "cy_gb"]);
            assert!(in_progress.is_empty());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn who_signs_in_and_how_are_nodes_with_their_generated_routes() {
    let graph = load("27-guest-house");
    match &graph.node(id(&graph, "principal user")).detail {
        Detail::Principal {
            entity, subject, ..
        } => {
            assert_eq!(entity.as_str(), "entity Guest");
            assert_eq!(subject, "email");
        }
        other => panic!("{other:?}"),
    }
    let uses = targets(&graph, "principal user", EdgeKind::Uses);
    assert!(uses.iter().any(|key| key == "entity Guest"), "{uses:?}");
    assert!(
        uses.iter().any(|key| key == "field Guest.email"),
        "{uses:?}"
    );
    match &graph
        .node(id(&graph, "authentication_method browser_session"))
        .detail
    {
        Detail::AuthenticationMethod { cookie, .. } => {
            assert_eq!(cookie.as_deref(), Some("tygwyn_session"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        targets(
            &graph,
            "authentication_method browser_session",
            EdgeKind::Uses
        ),
        ["config_field site"]
    );
    assert_eq!(
        keys_of(&graph, Kind::SignInProvider),
        ["sign_in_provider magic_link", "sign_in_provider passkey"]
    );
    match &graph.node(id(&graph, "sign_in_provider magic_link")).detail {
        Detail::SignInProvider {
            generates_routes, ..
        } => assert!(!generates_routes),
        other => panic!("{other:?}"),
    }
    // The passkey routes the runtime answers, under their way of signing in.
    let passkey: Vec<&jadpo_graph::Node> = graph
        .nodes()
        .iter()
        .filter(|node| {
            node.kind == Kind::GeneratedRoute
                && node.parent.as_ref().map(Key::as_str) == Some("sign_in_provider passkey")
        })
        .collect();
    assert!(
        passkey
            .iter()
            .any(|node| node.key.as_str() == "generated_route POST /auth/passkey/sign-in"),
        "{:?}",
        keys_of(&graph, Kind::GeneratedRoute)
    );
    // `generate_routes: false`: no magic-link routes.
    assert!(
        !graph
            .nodes()
            .iter()
            .any(|node| node.kind == Kind::GeneratedRoute
                && node.parent.as_ref().map(Key::as_str) == Some("sign_in_provider magic_link"))
    );
    // Health, beside every API.
    for key in [
        "generated_route GET /health/live",
        "generated_route GET /health/ready",
    ] {
        match &graph.node(id(&graph, key)).detail {
            Detail::GeneratedRoute { generated, api, .. } => {
                assert_eq!(generated, "health");
                assert!(api.is_none());
            }
            other => panic!("{other:?}"),
        }
    }
    // The limits every route is counted against.
    match &graph.node(id(&graph, "rate_limit caller")).detail {
        Detail::RateLimit {
            requests,
            window_ms,
        } => assert_eq!((*requests, *window_ms), (600, 60_000)),
        other => panic!("{other:?}"),
    }
    assert!(graph.find(&Key::from_text("rate_limit address")).is_some());
}

#[test]
fn a_connections_webhook_is_a_generated_route_of_its_connection() {
    let graph = load("26-delivery-desk");
    let webhook = graph.node(id(&graph, "generated_route POST /webhooks/couriers"));
    assert_eq!(
        webhook.parent.as_ref().map(Key::as_str),
        Some("connection couriers")
    );
    match &webhook.detail {
        Detail::GeneratedRoute {
            generated,
            api,
            authentication,
        } => {
            assert_eq!(generated, "webhook");
            assert_eq!(api.as_deref(), Some("webhooks"));
            assert_eq!(authentication, "signature");
        }
        other => panic!("{other:?}"),
    }
}
