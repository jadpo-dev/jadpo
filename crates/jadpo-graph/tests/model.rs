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
    };
    let document = manifest(&graph, &stamp).to_document();
    assert!(
        document.starts_with("{\n  \"compiler\": \"1.2.3\",\n  \"errors\": 0,\n  \"format\": 1,\n")
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
    assert_eq!(status("route").as_deref(), Some("reserved"));
    assert_eq!(status("entity").as_deref(), Some("built"));
    assert_eq!(status("query").as_deref(), Some("declared"));
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
