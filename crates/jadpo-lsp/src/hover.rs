//! What hovering over a name shows (areas 1 and 14): the declaration as
//! Jadpo, its `///` description, then a few facts in the CLI design
//! language's words: the failures a callable can produce, a failure's
//! code, kind and status, who calls or uses it, and where it is written.
//! Everything comes from the graph, the same one the manifest is written
//! from.

use jadpo_check::graph::Found;
use jadpo_graph::manifest::name_now;
use jadpo_graph::{Base, Detail, EdgeKind, Graph, Key, Kind, NodeId, Section, rule_pairs};

/// How many callers or users a fact names before counting the rest.
const NAMED: usize = 5;

/// The hover for what a name refers to, as Markdown.
pub fn render(graph: &Graph, found: &Found) -> Option<String> {
    match found {
        Found::Local { name, ty, what } => {
            let code = match ty {
                Some(ty) => format!("{name}: {ty}"),
                None => name.clone(),
            };
            Some(format!("{}\n\n{what}", block(&code)))
        }
        Found::Node(key) => {
            let id = graph.find(key)?;
            Some(node(graph, id))
        }
    }
}

fn block(code: &str) -> String {
    format!("```jadpo\n{code}\n```")
}

/// The part of a name after its parent's: `keep` in `wording.keep`.
fn short(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn node(graph: &Graph, id: NodeId) -> String {
    let node = graph.node(id);
    let mut out = block(&declaration(graph, id));
    if let Some(doc) = &node.doc {
        out.push_str("\n\n");
        out.push_str(doc);
    }
    let facts = facts(graph, id);
    if !facts.is_empty() {
        out.push_str("\n\n");
        out.push_str(&facts.join("  \n"));
    }
    out
}

/// The node written as Jadpo, without bodies.
fn declaration(graph: &Graph, id: NodeId) -> String {
    let node = graph.node(id);
    let word = node.kind.word();
    let name = |key: &Key| name_now(graph, key);
    match &node.detail {
        Detail::Callable(signature) => {
            let keyword = match node.kind {
                Kind::Function => "function",
                Kind::Query => "query",
                Kind::Fixture => "fixture",
                _ => "action",
            };
            format!("{keyword} {}{}", short(&node.name), signature.render(&name))
        }
        Detail::Type {
            base,
            rules,
            failure,
        } => {
            let mut lines: Vec<String> = Vec::new();
            if let Some(failure) = failure {
                if let Some(kind) = &failure.kind {
                    lines.push(format!("kind: FailureKind.{kind}"));
                }
                lines.push(format!("code: \"{}\"", failure.code));
                if let Some(message) = &failure.message {
                    lines.push(format!("message: \"{message}\""));
                }
            }
            lines.extend(
                rule_pairs(rules)
                    .into_iter()
                    .map(|(rule, value)| format!("{rule}: {value}")),
            );
            lines.extend(members(graph, id, base));
            let head = format!("type {} = {}", node.name, base.render(&name));
            if lines.is_empty() {
                head
            } else {
                format!("{head} {{\n    {}\n}}", lines.join("\n    "))
            }
        }
        Detail::Field {
            ty,
            optional,
            default,
            section,
            rules,
            ..
        } => {
            let mut settings: Vec<String> = rule_pairs(rules)
                .into_iter()
                .map(|(rule, value)| format!("{rule}: {value}"))
                .collect();
            if *optional {
                settings.push("optional: true".to_string());
            }
            if *default {
                settings.push("default".to_string());
            }
            let place = match section {
                Some(Section::Client) => "client field ",
                Some(Section::Internal) => "internal field ",
                None => "",
            };
            let field = format!("{place}{}: {}", node.name, ty.render(&name));
            if settings.is_empty() {
                field
            } else {
                format!("{field} {{ {} }}", settings.join(" "))
            }
        }
        Detail::Variant { payload: true } => {
            let fields: Vec<String> = fields_of(graph, id);
            format!("{}: {{ {} }}", node.name, fields.join(" "))
        }
        Detail::Variant { payload: false } => node.name.clone(),
        Detail::None => match node.kind {
            Kind::Entity => {
                let fields = fields_of(graph, id);
                if fields.is_empty() {
                    format!("entity {}", node.name)
                } else {
                    format!("entity {} {{\n    {}\n}}", node.name, fields.join("\n    "))
                }
            }
            Kind::RouteGroup => format!("routes {}", node.name),
            Kind::Test => format!("test \"{}\"", node.name),
            _ => format!("{word} {}", node.name),
        },
    }
}

/// A type's fields or variants, in the order they are written.
fn members(graph: &Graph, id: NodeId, base: &Base) -> Vec<String> {
    match base {
        Base::Enum => {
            let mut variants: Vec<NodeId> = graph
                .children(id)
                .filter(|&child| graph.node(child).kind == Kind::Variant)
                .collect();
            variants.sort_by_key(|&child| graph.node(child).place.as_ref().map(|p| p.start));
            variants
                .into_iter()
                .map(|variant| {
                    let fields = fields_of(graph, variant);
                    let name = short(&graph.node(variant).name);
                    if fields.is_empty() {
                        name.to_string()
                    } else {
                        format!("{name}: {{ {} }}", fields.join(" "))
                    }
                })
                .collect()
        }
        Base::Failure => {
            let mut out = Vec::new();
            for (section, word) in [(Section::Client, "client"), (Section::Internal, "internal")] {
                let fields: Vec<String> = ordered_children(graph, id)
                    .into_iter()
                    .filter(|&child| {
                        matches!(graph.node(child).detail, Detail::Field { section: Some(s), .. } if s == section)
                    })
                    .map(|child| field_line(graph, child))
                    .collect();
                if !fields.is_empty() {
                    out.push(format!("{word}: {{ {} }}", fields.join(" ")));
                }
            }
            out
        }
        Base::Refines(_) => Vec::new(),
        _ => fields_of(graph, id),
    }
}

fn ordered_children(graph: &Graph, id: NodeId) -> Vec<NodeId> {
    let mut children: Vec<NodeId> = graph
        .children(id)
        .filter(|&child| graph.node(child).kind == Kind::Field)
        .collect();
    // In the order they are written; a copied field sits at the entry that
    // copied it.
    children.sort_by_key(|&child| graph.node(child).place.as_ref().map(|p| p.start));
    children
}

fn field_line(graph: &Graph, id: NodeId) -> String {
    let node = graph.node(id);
    match &node.detail {
        Detail::Field { ty, .. } => format!(
            "{}: {}",
            short(&node.name),
            ty.render(&|key| name_now(graph, key))
        ),
        _ => short(&node.name).to_string(),
    }
}

fn fields_of(graph: &Graph, id: NodeId) -> Vec<String> {
    ordered_children(graph, id)
        .into_iter()
        .map(|child| field_line(graph, child))
        .collect()
}

/// `a`, `b` and 3 more.
fn named(graph: &Graph, ids: &[NodeId]) -> String {
    let mut names: Vec<String> = ids
        .iter()
        .take(NAMED)
        .map(|&id| format!("`{}`", graph.node(id).name))
        .collect();
    if ids.len() > NAMED {
        names.push(format!("{} more", ids.len() - NAMED));
    }
    match names.as_slice() {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn facts(graph: &Graph, id: NodeId) -> Vec<String> {
    let node = graph.node(id);
    let mut out = Vec::new();
    if node.kind.is_callable() {
        let raises: Vec<String> = graph
            .outgoing(id, EdgeKind::Raises)
            .map(|failure| {
                let failure = graph.node(failure);
                match &failure.detail {
                    Detail::Type {
                        failure: Some(detail),
                        ..
                    } => {
                        let mut about = Vec::new();
                        if let Some(kind) = &detail.kind {
                            about.push(kind.clone());
                        }
                        if let Some(status) = detail.status {
                            about.push(status.to_string());
                        }
                        if about.is_empty() {
                            format!("`{}`", failure.name)
                        } else {
                            format!("`{}` ({})", failure.name, about.join(" · "))
                        }
                    }
                    _ => format!("`{}`", failure.name),
                }
            })
            .collect();
        if !raises.is_empty() {
            out.push(format!("fails with {}", raises.join(", ")));
        }
    }
    if let Detail::Type {
        failure: Some(failure),
        ..
    } = &node.detail
    {
        let mut about = vec![format!("code `{}`", failure.code)];
        about.extend(failure.kind.clone());
        about.extend(failure.status.map(|status| status.to_string()));
        out.push(about.join(" · "));
        let raisers: Vec<NodeId> = graph.incoming(id, EdgeKind::Raises).collect();
        if !raisers.is_empty() {
            out.push(format!("raised by {}", named(graph, &raisers)));
        }
    }
    let callers: Vec<NodeId> = graph.incoming(id, EdgeKind::Calls).collect();
    if !callers.is_empty() {
        out.push(format!("called by {}", named(graph, &callers)));
    } else if matches!(node.kind, Kind::Function) {
        out.push("nothing calls it yet".to_string());
    }
    if matches!(
        node.kind,
        Kind::Type | Kind::Entity | Kind::Field | Kind::Variant
    ) {
        // A failure's raisers are listed already.
        let raisers: Vec<NodeId> = graph.incoming(id, EdgeKind::Raises).collect();
        let users: Vec<NodeId> = graph
            .incoming(id, EdgeKind::Uses)
            .filter(|user| !raisers.contains(user))
            .collect();
        if !users.is_empty() {
            out.push(format!("used by {}", named(graph, &users)));
        }
    }
    if node.kind == Kind::Domain {
        let held = graph
            .nodes()
            .iter()
            .filter(|other| {
                other.domain == node.domain
                    && other.parent.is_none()
                    && !matches!(other.kind, Kind::Domain | Kind::File)
            })
            .count();
        out.push(match held {
            1 => "holds 1 declaration".to_string(),
            n => format!("holds {n} declarations"),
        });
    }
    if node.kind == Kind::Library {
        let functions: Vec<NodeId> = graph.children(id).collect();
        if !functions.is_empty() {
            let names: Vec<String> = functions
                .iter()
                .map(|&f| format!("`{}`", short(&graph.node(f).name)))
                .collect();
            out.push(format!("functions {}", names.join(", ")));
        }
    }
    let mut place = Vec::new();
    if let Some(at) = &node.place
        && node.kind != Kind::File
    {
        place.push(format!("`{}:{}`", at.file, at.lines.0));
    }
    if let Some(domain) = &node.domain
        && node.kind != Kind::Domain
    {
        place.push(format!("domain {domain}"));
    }
    if node.public {
        place.push("public".to_string());
    }
    if !place.is_empty() {
        out.push(place.join(" · "));
    }
    out
}
