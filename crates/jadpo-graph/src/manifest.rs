//! The manifest (area 18): the graph as canonical JSON, written by `jadpo
//! check` to `build/manifest.json`, and its schema, which `jadpo help
//! --json` prints.
//!
//! `format` changes only when a field is removed or changes meaning;
//! additions keep the number (area 18). The build stamp's part is written
//! since chunk 8.1o2: `source`, the commit and whether the tree was
//! modified, and `deployables`, each one's target and build id (areas 14
//! and 18). Fields a later area fills are not written until it does:
//! routes, schedules,
//! subscribers and their runs (7, 12), connections and hosts (11), policy
//! (9), personal data (15), thresholds (16) and the control flow path
//! numbers decode against (16, 18).

use std::collections::BTreeMap;

use crate::json::Value;
use crate::{Detail, EdgeKind, Graph, Key, Kind, Node, NodeId, Rules, rule_pairs};

/// The manifest's format number: 2 since chunk 5.3d, when the key of a
/// function or type declared in a domain began to carry the domain, `type
/// billing.Address` (area 18, "Stable identity"). Manifests are never stored, so
/// nothing reads an older one.
pub const FORMAT: i64 = 2;

/// What a manifest says about the check that wrote it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    /// The compiler's version.
    pub compiler: String,
    /// How many errors the check found: a graph from a project with
    /// errors leaves out what could not be resolved.
    pub errors: usize,
    /// The commit the source is at, and whether the tree holds changes it
    /// doesn't (area 18, "The manifest"; chunk 8.1o2); `None` writes no
    /// `source`.
    pub source: Option<Source>,
    /// Each deployable, in name order, with the target it is built for and
    /// its build id (area 18; chunk 8.1o2).
    pub deployables: Vec<DeployableStamp>,
}

/// Where the source a manifest was written from is (area 18).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    /// The commit `git` says the project is at; `None` when git doesn't
    /// track it.
    pub commit: Option<String>,
    /// Whether the tree holds changes the commit doesn't, `build/` and
    /// `.jadpo/` aside; always, without a commit.
    pub modified: bool,
}

/// One deployable as the manifest lists it (area 18).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployableStamp {
    pub name: String,
    /// `native` or `wasm`: what `jadpo build` builds it for.
    pub target: String,
    /// Its build id, `b_` and twelve hex digits; `None` for a project
    /// with errors, which builds nothing (chunk 8.1o3).
    pub build: Option<String>,
}

/// The manifest of `graph`.
pub fn manifest(graph: &Graph, stamp: &Stamp) -> Value {
    let mut out = Value::object();
    out.set("format", FORMAT);
    out.set("compiler", stamp.compiler.as_str());
    out.set("errors", stamp.errors);
    if let Some(source) = &stamp.source {
        let mut written = Value::object();
        written.set_some("commit", source.commit.as_deref());
        written.set("modified", source.modified);
        out.set("source", written);
    }
    if !stamp.deployables.is_empty() {
        let mut deployables = Value::object();
        for deployable in &stamp.deployables {
            let mut entry = Value::object();
            entry.set("target", deployable.target.as_str());
            entry.set_some("build", deployable.build.as_deref());
            deployables.set(&deployable.name, entry);
        }
        out.set("deployables", deployables);
    }
    out.set(
        "nodes",
        Value::Array(graph.ids().map(|id| node_json(graph, id)).collect()),
    );
    out
}

/// One node as the manifest writes it.
pub fn node_json(graph: &Graph, id: NodeId) -> Value {
    let node = graph.node(id);
    let mut out = Value::object();
    out.set("id", node.key.as_str());
    out.set("kind", node.kind.word());
    out.set("name", node.name.as_str());
    if let Some(place) = &node.place {
        out.set("file", place.file.as_str());
        out.set(
            "lines",
            vec![Value::from(place.lines.0), Value::from(place.lines.1)],
        );
    }
    if let Some(digest) = &node.digest {
        out.set("digest", digest.short());
    }
    out.set_some("domain", node.domain.as_deref());
    out.set_some("parent", node.parent.as_ref().map(|key| key.as_str()));
    out.set_some("public", node.public);
    out.set_some("doc", node.doc.as_deref());
    detail_json(node, &|key| name_now(graph, key), &mut out);
    for (kind, targets) in graph.edges_from(id) {
        out.set(
            kind.word(),
            targets
                .into_iter()
                .map(|to| Value::from(graph.node(to).key.as_str()))
                .collect::<Vec<_>>(),
        );
    }
    out
}

/// The name a key's node has now, which differs from the key's born name
/// after a rename; the key's own name when it has no node.
pub fn name_now(graph: &Graph, key: &Key) -> String {
    graph
        .get(key)
        .map_or_else(|| key.written().to_string(), |node| node.name.clone())
}

fn rules_json(rules: &Rules) -> Value {
    Value::Object(
        rule_pairs(rules)
            .into_iter()
            .map(|(name, value)| (name.to_string(), Value::from(value.as_str())))
            .collect(),
    )
}

fn detail_json(node: &Node, name: &dyn Fn(&Key) -> String, out: &mut Value) {
    match &node.detail {
        Detail::None => {}
        Detail::Type {
            base,
            rules,
            failure,
        } => {
            out.set("base", base.render(name));
            out.set_some("rules", rules_json(rules));
            if let Some(failure) = failure {
                let mut entry = Value::object();
                entry.set("code", failure.code.as_str());
                entry.set_some("kind", failure.kind.as_deref());
                entry.set_some("status", failure.status);
                entry.set_some("message", failure.message.as_deref());
                out.set("failure", entry);
            }
        }
        Detail::Field {
            order,
            ty,
            optional,
            default,
            header,
            column,
            wire_name,
            coding,
            section,
            copied_from,
            replaces,
            rules,
        } => {
            out.set("order", i64::from(*order));
            out.set("type", ty.render(name));
            out.set_some("optional", *optional);
            out.set_some("default", default.as_ref().map(|d| d.render(name)));
            out.set_some("header", header.as_deref());
            out.set_some("column", column.as_deref());
            out.set_some("wireName", wire_name.as_deref());
            out.set_some(
                "wire",
                coding.as_ref().map(|coding| match coding {
                    crate::FieldCoding::Encoding(encoding) => format!("Wire.{encoding}"),
                    crate::FieldCoding::Functions { decode, encode, .. } => {
                        format!("decode {}, encode {}", name(decode), name(encode))
                    }
                    crate::FieldCoding::Parts { parts, .. } => format!(
                        "parts {}",
                        parts
                            .iter()
                            .map(|(part, spelling)| format!("{part} \"{spelling}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }),
            );
            out.set_some("section", section.map(|s| s.word()));
            out.set_some("copiedFrom", copied_from.as_ref().map(|key| key.as_str()));
            out.set_some("replaces", replaces.as_ref().map(|key| key.as_str()));
            out.set_some("rules", rules_json(rules));
        }
        Detail::Variant {
            order,
            payload,
            wire_name,
        } => {
            out.set("order", i64::from(*order));
            out.set_some("payload", *payload);
            out.set_some("wireName", wire_name.as_deref());
        }
        Detail::Rule { order, condition } => {
            out.set("order", i64::from(*order));
            out.set("condition", condition.as_str());
        }
        Detail::Callable {
            signature,
            isolation,
        } => {
            out.set_some("isolation", isolation.map(|level| level.word()));
            let mut entry = Value::object();
            entry.set(
                "params",
                signature
                    .params
                    .iter()
                    .map(|(param_name, ty)| {
                        let mut param = Value::object();
                        param.set("name", param_name.as_str());
                        param.set("type", ty.render(name));
                        param
                    })
                    .collect::<Vec<_>>(),
            );
            entry.set("result", signature.result.render(name));
            entry.set(
                "fails",
                signature
                    .fails
                    .iter()
                    .map(|failure| Value::from(failure.render(name)))
                    .collect::<Vec<_>>(),
            );
            out.set("signature", entry);
        }
        Detail::Constant { ty, value } => {
            out.set("type", ty.render(name));
            out.set("value", value.as_str());
        }
        Detail::Entity { store, table } | Detail::SystemTable { store, table } => {
            out.set("store", store.as_str());
            out.set("table", table.as_str());
        }
        Detail::Columns { fields, constraint } => {
            out.set("fields", fields.iter().map(Value::from).collect::<Vec<_>>());
            out.set("constraint", constraint.as_str());
        }
        Detail::Reference {
            target,
            on_delete,
            constraint,
        } => {
            out.set("target", target.as_str());
            out.set("onDelete", on_delete.as_str());
            out.set("constraint", constraint.as_str());
        }
        Detail::Placement { store } => out.set("store", store.as_str()),
        Detail::Lifecycle { visible, purge } => {
            out.set_some("visible", visible.as_deref());
            out.set_some("purge", purge.as_deref());
        }
        Detail::Route {
            api,
            status,
            input,
            output,
            deadline_ms,
        } => {
            out.set("api", api.as_str());
            out.set("status", *status);
            out.set_some("input", input.as_ref().map(|ty| ty.render(name)));
            out.set_some("output", output.as_ref().map(|ty| ty.render(name)));
            out.set("deadline", i64::from(*deadline_ms));
        }
        Detail::Api { prefix } => out.set("prefix", prefix.as_str()),
        Detail::Transition {
            when,
            guarded,
            sets,
        } => {
            out.set_some("when", when.as_deref());
            out.set_some("guarded", *guarded);
            out.set("sets", sets.iter().map(Value::from).collect::<Vec<_>>());
        }
        Detail::Grant { effect, who } => {
            out.set("effect", effect.as_str());
            out.set("who", who.iter().map(Value::from).collect::<Vec<_>>());
        }
        Detail::FieldRestriction {
            server_only,
            grants,
        } => {
            out.set_some("serverOnly", *server_only);
            if !*server_only {
                out.set(
                    "who",
                    Value::Object(
                        grants
                            .iter()
                            .map(|(effect, who)| {
                                (
                                    effect.clone(),
                                    Value::Array(who.iter().map(Value::from).collect()),
                                )
                            })
                            .collect(),
                    ),
                );
            }
        }
        Detail::Tenancy { tenant, via } => {
            out.set("tenant", tenant.as_str());
            out.set("via", via.as_str());
        }
        Detail::ActionException { who } => {
            out.set("who", who.iter().map(Value::from).collect::<Vec<_>>());
        }
        Detail::Prompt { version } => out.set("version", version.as_str()),
    }
}

/// The fields a manifest node can have, with what each holds.
const NODE_FIELDS: &[(&str, &str)] = &[
    (
        "id",
        "The node's identity: its kind's word and its born name",
    ),
    ("kind", "What the node is"),
    (
        "name",
        "Its name now, as written; differs from the id only after a rename",
    ),
    ("file", "The project-relative file it is written in"),
    ("lines", "The first and last line it covers"),
    (
        "digest",
        "BLAKE3 of its tokens without whitespace, comments or positions",
    ),
    ("domain", "The domain that owns it"),
    ("parent", "The id of what it belongs to"),
    ("public", "Visible to other domains"),
    ("doc", "Its /// description"),
    ("base", "A type's base, or the type it refines"),
    ("rules", "Rules on a type or field, by name"),
    ("failure", "A failure's code, kind, status and message"),
    (
        "order",
        "A field's or variant's place among those beside it, from 0, as declared; copied fields sit at the line that copied them",
    ),
    ("type", "A field's declared type"),
    ("optional", "An input field the sender may leave out"),
    ("default", "An input field's default, as written"),
    ("header", "The request header an input field is read from"),
    (
        "column",
        "The file column an input field is read from, or an output field written to, when not its name",
    ),
    (
        "wireName",
        "A provider package's field's or variant's spelling on the wire, when not its name",
    ),
    (
        "wire",
        "How a provider package's field is sent: its built-in encoding, or the package's functions that decode and encode it",
    ),
    ("section", "A failure field's block: client or internal"),
    ("copiedFrom", "The field an include or pick line copied"),
    ("replaces", "The copied field a written field replaces"),
    ("payload", "A variant that carries data"),
    (
        "signature",
        "A callable's parameters, result and fails list",
    ),
    (
        "isolation",
        "The level an action's or fixture's transaction runs at: read_committed when it loads no record, itself or through any action it calls, otherwise repeatable_read",
    ),
    (
        "store",
        "The connection an entity, its placement or a system table is in",
    ),
    (
        "table",
        "The table an entity's records or a system table's rows live in",
    ),
    ("fields", "A unique rule's or index's fields, in order"),
    (
        "constraint",
        "The name the database knows a unique rule, index or reference by",
    ),
    ("target", "The id of the entity a reference points at"),
    (
        "onDelete",
        "What deleting a referenced record does: restricted, cascade or cleared",
    ),
    (
        "visible",
        "A lifecycle's condition for the records reads see, as written",
    ),
    (
        "purge",
        "When a lifecycle deletes hidden records for good: after how long, since which field",
    ),
    ("when", "When a lifecycle transition is allowed, as written"),
    (
        "guarded",
        "A transition whose `when:` can refuse a visible record, a conflict its callers map",
    ),
    ("sets", "The fields a lifecycle transition sets"),
    ("api", "The API a route belongs to"),
    ("status", "A route's success status"),
    ("input", "A route's input type"),
    ("output", "A route's output type"),
    ("deadline", "A route's deadline, in milliseconds"),
    ("prefix", "The path an API's routes start with"),
    (
        "effect",
        "What a grant lets someone do to its entity's records: create, read, update or delete",
    ),
    (
        "who",
        "Who a grant or a listed action names, each as written: a user field, a role or Access",
    ),
    (
        "serverOnly",
        "A field no client may read or write (FieldAccess.server_only)",
    ),
    (
        "grants",
        "A field restriction's own lists, by the effect each narrows; an effect left out follows the record",
    ),
    (
        "tenant",
        "The id of the entity a tenancy's records belong to",
    ),
    (
        "via",
        "The reference a tenancy reaches its tenant through; id on the tenant itself",
    ),
    (
        "version",
        "A prompt's name and the first 8 hex characters of a SHA-256 over its two files, as every call's trace carries it",
    ),
];

/// The manifest's schema, for `jadpo help --json`.
pub fn schema() -> Value {
    let mut out = Value::object();
    out.set("file", "build/manifest.json");
    out.set("format", FORMAT);
    out.set(
        "top",
        Value::Object(BTreeMap::from([
            ("format".to_string(), Value::from("The format number")),
            (
                "compiler".to_string(),
                Value::from("The version of jadpo that wrote it"),
            ),
            (
                "errors".to_string(),
                Value::from("How many errors the check found"),
            ),
            (
                "source".to_string(),
                Value::from(
                    "The commit git says the project is at (`commit`, left out when git doesn't track it) and whether the tree holds changes it doesn't (`modified`)",
                ),
            ),
            (
                "deployables".to_string(),
                Value::from(
                    "Each deployable by name: the target `jadpo build` builds it for (`native` or `wasm`) and its build id (`build`)",
                ),
            ),
            (
                "nodes".to_string(),
                Value::from("Every node, sorted by id; each lists its edges by kind"),
            ),
        ])),
    );
    out.set(
        "nodeFields",
        Value::Object(
            NODE_FIELDS
                .iter()
                .map(|(name, meaning)| (name.to_string(), Value::from(*meaning)))
                .collect(),
        ),
    );
    out.set(
        "nodeKinds",
        Kind::ALL
            .iter()
            .map(|kind| {
                let mut entry = Value::object();
                entry.set("kind", kind.word());
                entry.set("group", kind.group().word());
                entry.set("status", kind.status().word());
                entry.set("area", kind.area());
                entry
            })
            .collect::<Vec<_>>(),
    );
    out.set(
        "edgeKinds",
        EdgeKind::ALL
            .iter()
            .map(|kind| {
                let mut entry = Value::object();
                entry.set("kind", kind.word());
                entry.set("status", kind.status().word());
                entry.set("meaning", kind.meaning());
                entry
            })
            .collect::<Vec<_>>(),
    );
    out
}
