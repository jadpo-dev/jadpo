//! The graph model on its own: keys and the registry, building, edges in
//! both directions, the manifest's shape and the diff's matching.

use jadpo_graph::diff::{What, diff};
use jadpo_graph::manifest::{Stamp, manifest, schema};
use jadpo_graph::{Builder, Detail, Digest, EdgeKind, Key, Kind, Node, Registry, Status};

fn node(kind: Kind, name: &str) -> Node {
    let mut node = Node::new(Key::new(kind, name), kind, name);
    node.digest = Some(Digest::of([name]));
    node
}

#[test]
fn a_key_is_the_kind_and_the_born_name() {
    let key = Key::new(Kind::Action, "save_note");
    assert_eq!(key.as_str(), "action save_note");
    assert_eq!(key.kind(), Some(Kind::Action));
    assert_eq!(key.name(), "save_note");
    assert_eq!(
        Key::new(Kind::RouteGroup, "Notes").as_str(),
        "route_group Notes"
    );
}

#[test]
fn the_registry_maps_names_now_to_born_names_and_children_follow() {
    let mut registry = Registry::new();
    registry.rename(Kind::Entity, "Task", "Todo");
    registry.rename(Kind::Field, "Task.importance", "Todo.priority");
    assert_eq!(
        registry.key(Kind::Entity, "Task", None).as_str(),
        "entity Todo"
    );
    // A child follows its parent without a line of its own.
    assert_eq!(
        registry
            .key(Kind::Field, "Task.title", Some(("Task", "Todo")))
            .as_str(),
        "field Todo.title"
    );
    // A child renamed itself has its own line.
    assert_eq!(
        registry
            .key(Kind::Field, "Task.importance", Some(("Task", "Todo")))
            .as_str(),
        "field Todo.priority"
    );
    // Nothing renamed: the born name is the name now.
    assert_eq!(
        registry.key(Kind::Type, "Count", None).as_str(),
        "type Count"
    );
}

#[test]
fn building_sorts_by_key_keeps_the_first_of_a_key_and_drops_dangling_edges() {
    let mut builder = Builder::new();
    let mut first = node(Kind::Type, "Count");
    first.doc = Some("first".into());
    let mut second = node(Kind::Type, "Count");
    second.doc = Some("second".into());
    builder.node(node(Kind::Function, "double"));
    builder.node(first);
    builder.node(second);
    builder.edge(
        Key::new(Kind::Function, "double"),
        EdgeKind::Uses,
        Key::new(Kind::Type, "Count"),
    );
    builder.edge(
        Key::new(Kind::Function, "double"),
        EdgeKind::Calls,
        Key::new(Kind::Function, "missing"),
    );
    let graph = builder.build();
    let keys: Vec<&str> = graph.nodes().iter().map(|n| n.key.as_str()).collect();
    assert_eq!(keys, ["function double", "type Count"]);
    let count = graph.find(&Key::new(Kind::Type, "Count")).expect("node");
    assert_eq!(graph.node(count).doc.as_deref(), Some("first"));
    assert_eq!(graph.edges().len(), 1, "the call to nothing is dropped");
    let double = graph
        .find(&Key::new(Kind::Function, "double"))
        .expect("node");
    assert_eq!(
        graph.outgoing(double, EdgeKind::Uses).collect::<Vec<_>>(),
        [count]
    );
    assert_eq!(
        graph.incoming(count, EdgeKind::Uses).collect::<Vec<_>>(),
        [double]
    );
}

#[test]
fn the_manifest_names_nodes_by_key_and_lists_edges_on_them() {
    let mut builder = Builder::new();
    builder.node(node(Kind::Function, "double"));
    builder.node(node(Kind::Type, "Count"));
    builder.edge(
        Key::new(Kind::Function, "double"),
        EdgeKind::Uses,
        Key::new(Kind::Type, "Count"),
    );
    let graph = builder.build();
    let stamp = Stamp {
        compiler: "1.2.3".into(),
        errors: 0,
        ..Stamp::default()
    };
    let document = manifest(&graph, &stamp).to_document();
    assert!(
        document.starts_with("{\n  \"compiler\": \"1.2.3\",\n  \"errors\": 0,\n  \"format\": 2,\n")
    );
    assert!(document.contains("\"uses\": [\n        \"type Count\"\n      ]"));
    assert!(document.ends_with("}\n") && !document.contains('\r'));
    assert_eq!(document, manifest(&graph, &stamp).to_document());
}

#[test]
fn the_schema_lists_every_kind_with_reserved_ones_marked() {
    let schema = schema();
    let kinds = schema
        .get("nodeKinds")
        .and_then(|k| k.as_array())
        .expect("kinds");
    assert_eq!(kinds.len(), Kind::ALL.len());
    let status = |word: &str| {
        kinds
            .iter()
            .find(|k| k.get("kind").and_then(|v| v.as_str()) == Some(word))
            .and_then(|k| k.get("status"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    assert_eq!(status("action").as_deref(), Some("built"));
    assert_eq!(status("route").as_deref(), Some("built"));
    assert_eq!(status("grant").as_deref(), Some("built"));
    assert_eq!(status("oauth_scope").as_deref(), Some("reserved"));
    assert_eq!(status("entity").as_deref(), Some("built"));
    assert_eq!(status("query").as_deref(), Some("built"));
    assert_eq!(status("fixture").as_deref(), Some("declared"));
    assert_eq!(status("person").as_deref(), Some("built"));
    assert_eq!(status("retention").as_deref(), Some("built"));
    assert_eq!(status("personal_mark").as_deref(), Some("reserved"));
    for kind in Kind::ALL {
        assert_eq!(Kind::from_word(kind.word()), Some(*kind));
    }
    for kind in EdgeKind::ALL {
        assert_eq!(EdgeKind::from_word(kind.word()), Some(*kind));
    }
    assert_eq!(Kind::Site.status(), Status::Built);
    assert!(Kind::Site.per_build() && Kind::Branch.per_build() && !Kind::Action.per_build());
    assert_eq!(Kind::Deployable.status(), Status::Reserved);
}

#[test]
fn a_diff_matches_by_key_so_a_registered_rename_reads_as_renamed() {
    let mut before = Builder::new();
    let mut domain = node(Kind::Domain, "planning");
    domain.digest = None;
    before.node(domain);
    before.node(node(Kind::Type, "Count"));
    before.node(node(Kind::Function, "double"));
    let before = before.build();

    let mut after = Builder::new();
    // `planning` renamed `scheduling`, registered: same key, new name.
    let mut renamed = Node::new(
        Key::new(Kind::Domain, "planning"),
        Kind::Domain,
        "scheduling",
    );
    renamed.digest = None;
    after.node(renamed);
    let mut count = node(Kind::Type, "Count");
    count.detail = Detail::Type {
        base: jadpo_graph::Base::Refines(jadpo_graph::TypeRef::Builtin(jadpo_std::Builtin::Int)),
        rules: jadpo_graph::Rules {
            min: jadpo_std::Num::parse("0"),
            ..jadpo_graph::Rules::default()
        },
        failure: None,
    };
    after.node(count);
    let mut domain = node(Kind::Domain, "billing");
    domain.digest = None;
    after.node(domain);
    after.edge(
        Key::new(Kind::Type, "Count"),
        EdgeKind::Uses,
        Key::new(Kind::Domain, "billing"),
    );
    let after = after.build();

    let changes = diff(&before, &after);
    let words: Vec<(&str, &str)> = changes
        .changes
        .iter()
        .map(|c| (c.key.as_str(), c.what.word()))
        .collect();
    assert_eq!(
        words,
        [
            ("domain billing", "added"),
            ("domain planning", "changed"),
            ("function double", "removed"),
            ("type Count", "changed"),
        ]
    );
    let billing = changes.get("domain billing").expect("added");
    assert_eq!(billing.needs, ["DomainChange.created"]);
    let planning = changes.get("domain planning").expect("renamed");
    assert_eq!(planning.needs, ["DomainChange.renamed"]);
    let What::Changed(changed) = &changes.get("type Count").expect("changed").what else {
        panic!("Count changed");
    };
    assert!(changed.detail && !changed.digest);
    assert_eq!(changed.edges[0].kind, EdgeKind::Uses);
    assert_eq!(changed.edges[0].added, [Key::new(Kind::Domain, "billing")]);
    assert!(diff(&after, &after).is_empty());
}

fn host(connection: &str, word: &str, address: &str, service: Option<&str>) -> Node {
    let name = format!("{connection}.{word}");
    let mut host = node(Kind::Host, &name);
    host.digest = Some(Digest::of([address, service.unwrap_or_default()]));
    host.parent = Some(Key::new(Kind::Connection, connection));
    host.detail = Detail::Host {
        address: Some(address.to_string()),
        setting: None,
        service: service.map(str::to_string),
        environment: None,
        reached_for: "SwiftCouriers.v1".to_string(),
    };
    host
}

fn connection(name: &str, credential: &str) -> Node {
    let mut connection = node(Kind::Connection, name);
    connection.digest = Some(Digest::of([name, credential]));
    connection.detail = Detail::Connection {
        capability: "package".to_string(),
        package: Some("SwiftCouriers.v1".to_string()),
        domains: vec!["desk".to_string()],
        credential: Some(credential.to_string()),
    };
    connection
}

fn locked(package: &str, source: &str) -> Node {
    let mut entry = node(Kind::LockEntry, package);
    entry.digest = Some(Digest::of([package, source]));
    entry.detail = Detail::LockEntry {
        version: Some("1.0.0".to_string()),
        source: source.to_string(),
        location: Some("github.com/acme/couriers".to_string()),
        tag: None,
        commit: None,
        digest: Some("sha256:00".to_string()),
        own_source: None,
    };
    entry
}

/// A lock entry of `package` from git at `repository`, under `own_source`
/// when it is one of the company's.
fn locked_git(package: &str, repository: &str, own_source: Option<&str>) -> Node {
    let mut entry = locked(package, "git");
    if let Detail::LockEntry {
        location,
        own_source: own,
        ..
    } = &mut entry.detail
    {
        *location = Some(repository.to_string());
        *own = own_source.map(str::to_string);
    }
    entry
}

/// A package from git needs a person when it is new to a repository, and
/// that repository is under none of `packages:`'s `own_sources:` (chunk
/// V1).
#[test]
fn a_diff_asks_for_a_person_only_for_a_third_partys_git_repository() {
    const OWN: &str = "github.com/acme/";
    let mut before = Builder::new();
    before.node(locked_git("Moved.v1", "github.com/acme/moved", Some(OWN)));
    before.node(locked_git("Kept.v1", "github.com/stranger/kept", None));
    before.node(locked_git("Listed.v1", "github.com/acme/listed", Some(OWN)));
    before.node(locked("Registered.v1", "registry"));
    let before = before.build();
    let mut after = Builder::new();
    after.node(locked_git("New.v1", "github.com/acme/new", Some(OWN)));
    after.node(locked_git("Stranger.v1", "github.com/stranger/x", None));
    after.node(locked_git("Moved.v1", "github.com/stranger/moved", None));
    let mut bumped = locked_git("Kept.v1", "github.com/stranger/kept", None);
    if let Detail::LockEntry { version, .. } = &mut bumped.detail {
        *version = Some("2.0.0".to_string());
    }
    after.node(bumped);
    // Its entry removed, its repository the same.
    after.node(locked_git("Listed.v1", "github.com/acme/listed", None));
    after.node(locked_git(
        "Registered.v1",
        "github.com/acme/own",
        Some(OWN),
    ));
    let after = after.build();
    let changes = diff(&before, &after);
    let needs = |key: &str| -> Vec<String> {
        changes
            .get(key)
            .map(|change| change.needs.clone())
            .unwrap_or_default()
    };
    assert!(needs("lock_entry New.v1").is_empty());
    assert_eq!(
        needs("lock_entry Stranger.v1"),
        ["PackageChange.git_source"]
    );
    assert_eq!(needs("lock_entry Moved.v1"), ["PackageChange.git_source"]);
    assert!(needs("lock_entry Kept.v1").is_empty());
    assert!(needs("lock_entry Listed.v1").is_empty());
    assert!(needs("lock_entry Registered.v1").is_empty());
}

/// What area 18 says needs a person in a package or sign-in change
/// (chunk 10.3g): a used operation removed, changed or failing in a new
/// way, a host or credential scheme changed, a service's image, a package
/// from git, and any new way of signing in or kind of caller.
#[test]
fn a_diff_says_which_package_and_sign_in_changes_need_a_person() {
    let action = Key::new(Kind::Action, "book_pickup");
    let used = Key::new(Kind::Operation, "SwiftCouriers.pickups.create");
    let unused = Key::new(Kind::Operation, "SwiftCouriers.pickups.cancel");
    let changed = Key::new(Kind::Operation, "SwiftCouriers.tariffs.get");
    let failing = Key::new(Kind::Operation, "SwiftCouriers.tariffs.list");
    let slot_taken = Key::new(Kind::Type, "SwiftCouriers.SlotTaken");

    let mut before = Builder::new();
    before.node(node(Kind::Action, "book_pickup"));
    before.node(node(Kind::Type, "SwiftCouriers.SlotTaken"));
    for key in [&used, &unused, &changed, &failing] {
        before.node(node(Kind::Operation, key.name()));
    }
    for key in [&used, &changed, &failing] {
        before.edge(action.clone(), EdgeKind::Calls, key.clone());
    }
    before.node(connection("couriers", "bearer"));
    before.node(host("couriers", "live", "https://api.swift.example", None));
    before.node(host("runner", "service", "", Some("swift:1")));
    before.node(locked("Tables.v1", "registry"));
    before.node(node(Kind::Principal, "user"));
    before.node(node(Kind::AuthenticationMethod, "browser_session"));
    let before = before.build();

    let mut after = Builder::new();
    after.node(node(Kind::Action, "book_pickup"));
    after.node(node(Kind::Type, "SwiftCouriers.SlotTaken"));
    let mut retyped = node(Kind::Operation, changed.name());
    retyped.detail = Detail::Operation {
        method: "GET".to_string(),
        path: "/v2/tariffs".to_string(),
        input: None,
        output: jadpo_graph::TypeRef::Builtin(jadpo_std::Builtin::Unit),
        changes: false,
        idempotent: false,
    };
    after.node(retyped);
    after.node(node(Kind::Operation, failing.name()));
    for key in [&changed, &failing] {
        after.edge(action.clone(), EdgeKind::Calls, key.clone());
    }
    after.edge(failing.clone(), EdgeKind::Raises, slot_taken);
    after.node(connection("couriers", "basic"));
    after.node(host(
        "couriers",
        "live",
        "https://api.eu.swift.example",
        None,
    ));
    after.node(host(
        "couriers",
        "sandbox",
        "https://sandbox.swift.example",
        None,
    ));
    after.node(host("runner", "service", "", Some("swift:2")));
    after.node(connection("rates", "bearer"));
    after.node(host("rates", "live", "https://rates.example", None));
    after.node(locked("Tables.v1", "git"));
    after.node(locked("Couriers.v1", "git"));
    after.node(locked("Postcodes.v2", "registry"));
    after.node(node(Kind::Principal, "user"));
    after.node(node(Kind::Principal, "service"));
    after.node(node(Kind::AuthenticationMethod, "browser_session"));
    after.node(node(Kind::AuthenticationMethod, "api_token"));
    after.node(node(Kind::SignInProvider, "passkey"));
    let after = after.build();

    let changes = diff(&before, &after);
    let needs = |key: &str| -> Vec<String> {
        changes
            .get(key)
            .map(|change| change.needs.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        needs("operation SwiftCouriers.pickups.create"),
        ["PackageChange.operation_removed"]
    );
    assert!(needs("operation SwiftCouriers.pickups.cancel").is_empty());
    assert_eq!(
        needs("operation SwiftCouriers.tariffs.get"),
        ["PackageChange.operation_changed"]
    );
    assert_eq!(
        needs("operation SwiftCouriers.tariffs.list"),
        ["PackageChange.failure_added"]
    );
    assert_eq!(
        needs("connection couriers"),
        ["PackageChange.credential_changed"]
    );
    assert_eq!(needs("host couriers.live"), ["PackageChange.host_changed"]);
    assert_eq!(
        needs("host couriers.sandbox"),
        ["PackageChange.host_changed"]
    );
    assert_eq!(
        needs("host runner.service"),
        ["PackageChange.service_image"]
    );
    // A new connection and its hosts appear without needing a person.
    assert!(needs("connection rates").is_empty());
    assert!(needs("host rates.live").is_empty());
    assert_eq!(needs("lock_entry Tables.v1"), ["PackageChange.git_source"]);
    assert_eq!(
        needs("lock_entry Couriers.v1"),
        ["PackageChange.git_source"]
    );
    assert!(needs("lock_entry Postcodes.v2").is_empty());
    assert_eq!(needs("principal service"), ["PolicyChange.principal_added"]);
    assert_eq!(
        needs("authentication_method api_token"),
        ["PolicyChange.sign_in_method_added"]
    );
    assert_eq!(
        needs("sign_in_provider passkey"),
        ["PolicyChange.sign_in_added"]
    );
}
