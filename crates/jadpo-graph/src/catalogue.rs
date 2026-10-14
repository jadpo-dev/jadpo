//! The catalogues (areas 1, 4 and 14): views of the graph that `jadpo
//! check` writes to `build/catalogue/` for agents and people, each as
//! Markdown with a JSON twin for tools. The library catalogue lists every
//! library function with its signature, description and callers, read
//! before writing a helper; the failure catalogue lists every failure with
//! its code, kind, status, message and client fields.

use crate::json::Value;
use crate::{Detail, EdgeKind, Graph, Kind, Node, NodeId, Section};

/// The HTTP status each failure kind sets (area 4, "Kinds").
pub fn status_of(kind: &str) -> Option<u16> {
    Some(match kind {
        "invalid_request" => 400,
        "unauthenticated" => 401,
        "not_permitted" => 403,
        "not_visible" | "not_found" => 404,
        "conflict" => 409,
        "precondition_failed" => 412,
        "invalid_value" | "rejected" => 422,
        "rate_limited" => 429,
        "outcome_unknown" | "misconfigured" | "internal_fault" => 500,
        "unavailable" => 503,
        "timed_out" => 504,
        _ => return None,
    })
}

/// A catalogue: its Markdown and its JSON twin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalogue {
    pub markdown: String,
    pub json: Value,
}

fn location(node: &Node) -> String {
    node.place.as_ref().map_or_else(String::new, |place| {
        format!("{}:{}", place.file, place.lines.0)
    })
}

/// `name` (place), for lists of callers.
fn reference(graph: &Graph, id: NodeId) -> String {
    let node = graph.node(id);
    match &node.place {
        Some(place) => format!("`{}` ({}:{})", node.name, place.file, place.lines.0),
        None => format!("`{}`", node.name),
    }
}

fn reference_json(graph: &Graph, id: NodeId) -> Value {
    let node = graph.node(id);
    let mut out = Value::object();
    out.set("id", node.key.as_str());
    if let Some(place) = &node.place {
        out.set("file", place.file.as_str());
        out.set("line", place.lines.0);
    }
    out
}

/// The callers of `id`, in key order. Tests never count as users (area 1).
fn callers(graph: &Graph, id: NodeId) -> Vec<NodeId> {
    graph
        .incoming(id, EdgeKind::Calls)
        .filter(|&caller| graph.node(caller).kind != Kind::Test)
        .collect()
}

fn doc_paragraph(markdown: &mut String, doc: Option<&str>) {
    if let Some(doc) = doc {
        markdown.push_str(doc);
        markdown.push_str("\n\n");
    }
}

/// Every library function (`build/catalogue/libraries.md` and `.json`).
pub fn libraries(graph: &Graph) -> Catalogue {
    let mut markdown = String::from(
        "# Library functions\n\nEvery library function, with its signature, description and callers. \
         Written by `jadpo check`; read it before writing a helper.\n\n",
    );
    let mut entries = Vec::new();
    let libraries: Vec<NodeId> = graph
        .ids()
        .filter(|&id| graph.node(id).kind == Kind::Library)
        .collect();
    if libraries.is_empty() {
        markdown.push_str("This project has no libraries yet.\n");
    }
    for library in libraries {
        let node = graph.node(library);
        markdown.push_str(&format!("## {}\n\n", node.name));
        let mut facts = vec![format!("`{}`", location(node))];
        if let Some(domain) = &node.domain {
            facts.push(format!("domain {domain}"));
        }
        if node.public {
            facts.push("public".to_string());
        }
        markdown.push_str(&facts.join(" · "));
        markdown.push_str("\n\n");
        doc_paragraph(&mut markdown, node.doc.as_deref());
        let mut functions = Vec::new();
        let members: Vec<NodeId> = graph
            .children(library)
            .filter(|&id| graph.node(id).kind == Kind::Function)
            .collect();
        for function in members {
            let child = graph.node(function);
            let signature = child
                .signature()
                .map(|s| s.render(&|key| crate::manifest::name_now(graph, key)))
                .unwrap_or_default();
            let short = child
                .name
                .rsplit_once('.')
                .map_or(child.name.as_str(), |(_, short)| short);
            markdown.push_str(&format!("### {}\n\n", child.name));
            markdown.push_str(&format!("```jadpo\nfunction {short}{signature}\n```\n\n"));
            doc_paragraph(&mut markdown, child.doc.as_deref());
            let calls = callers(graph, function);
            markdown.push_str(&format!("Defined at `{}`. ", location(child)));
            if calls.is_empty() {
                markdown.push_str("Nothing calls it yet.\n\n");
            } else {
                let list: Vec<String> = calls.iter().map(|&id| reference(graph, id)).collect();
                markdown.push_str(&format!("Called by {}.\n\n", list.join(", ")));
            }
            let mut entry = reference_json(graph, function);
            entry.set("name", child.name.as_str());
            entry.set("signature", signature);
            entry.set_some("doc", child.doc.as_deref());
            entry.set(
                "callers",
                calls
                    .iter()
                    .map(|&id| reference_json(graph, id))
                    .collect::<Vec<_>>(),
            );
            functions.push(entry);
        }
        let mut entry = reference_json(graph, library);
        entry.set("name", node.name.as_str());
        entry.set_some("domain", node.domain.as_deref());
        entry.set_some("public", node.public);
        entry.set_some("doc", node.doc.as_deref());
        entry.set("functions", functions);
        entries.push(entry);
    }
    let mut json = Value::object();
    json.set("format", crate::manifest::FORMAT);
    json.set("libraries", entries);
    Catalogue {
        markdown: one_final_newline(markdown),
        json,
    }
}

/// Every failure (`build/catalogue/failures.md` and `.json`), by code.
pub fn failures(graph: &Graph) -> Catalogue {
    let mut markdown = String::from(
        "# Failures\n\nEvery failure the project declares, by public code: its kind, HTTP status, \
         message and the fields sent to the client. Written by `jadpo check`.\n\n",
    );
    let mut found: Vec<(String, NodeId)> = graph
        .ids()
        .filter_map(|id| match &graph.node(id).detail {
            Detail::Type {
                failure: Some(failure),
                ..
            } => Some((failure.code.clone(), id)),
            _ => None,
        })
        .collect();
    found.sort();
    if found.is_empty() {
        markdown.push_str("This project declares no failures yet.\n");
    }
    let mut entries = Vec::new();
    for (code, id) in found {
        let node = graph.node(id);
        let Detail::Type {
            failure: Some(failure),
            ..
        } = &node.detail
        else {
            continue;
        };
        markdown.push_str(&format!("## {code}\n\n"));
        let mut facts = vec![format!("`{}`", node.name)];
        if let Some(kind) = &failure.kind {
            facts.push(kind.clone());
        }
        if let Some(status) = failure.status {
            facts.push(status.to_string());
        }
        facts.push(format!("`{}`", location(node)));
        markdown.push_str(&facts.join(" · "));
        markdown.push_str("\n\n");
        doc_paragraph(&mut markdown, node.doc.as_deref());
        let client: Vec<NodeId> = graph
            .fields(id)
            .into_iter()
            .filter(|&field| {
                matches!(
                    graph.node(field).detail,
                    Detail::Field {
                        section: Some(Section::Client),
                        ..
                    }
                )
            })
            .collect();
        let field_text = |field: NodeId| {
            let child = graph.node(field);
            let short = child
                .name
                .rsplit_once('.')
                .map_or(child.name.as_str(), |(_, short)| short);
            match &child.detail {
                Detail::Field { ty, .. } => format!("`{short}: {ty}`"),
                _ => format!("`{short}`"),
            }
        };
        let message = failure
            .message
            .clone()
            .unwrap_or_else(|| "the kind's generic message".to_string());
        markdown.push_str(&format!("- message: {message}\n"));
        let fields: Vec<String> = client.iter().map(|&field| field_text(field)).collect();
        markdown.push_str(&format!(
            "- client fields: {}\n",
            if fields.is_empty() {
                "none".to_string()
            } else {
                fields.join(", ")
            }
        ));
        let raisers: Vec<NodeId> = graph.incoming(id, EdgeKind::Raises).collect();
        if !raisers.is_empty() {
            let list: Vec<String> = raisers.iter().map(|&r| reference(graph, r)).collect();
            markdown.push_str(&format!("- raised by: {}\n", list.join(", ")));
        }
        markdown.push('\n');
        let mut entry = reference_json(graph, id);
        entry.set("code", code.as_str());
        entry.set("name", node.name.as_str());
        entry.set_some("kind", failure.kind.as_deref());
        entry.set_some("status", failure.status);
        entry.set_some("message", failure.message.as_deref());
        entry.set_some("doc", node.doc.as_deref());
        entry.set(
            "client",
            client
                .iter()
                .map(|&field| {
                    let child = graph.node(field);
                    let mut out = Value::object();
                    out.set("id", child.key.as_str());
                    if let Detail::Field { ty, .. } = &child.detail {
                        out.set(
                            "type",
                            ty.render(&|key| crate::manifest::name_now(graph, key)),
                        );
                    }
                    out
                })
                .collect::<Vec<_>>(),
        );
        entry.set(
            "raisedBy",
            raisers
                .iter()
                .map(|&r| reference_json(graph, r))
                .collect::<Vec<_>>(),
        );
        entries.push(entry);
    }
    let mut json = Value::object();
    json.set("format", crate::manifest::FORMAT);
    json.set("failures", entries);
    Catalogue {
        markdown: one_final_newline(markdown),
        json,
    }
}

/// One newline at the end, never a blank line before it.
fn one_final_newline(mut markdown: String) -> String {
    while markdown.ends_with("\n\n") {
        markdown.pop();
    }
    markdown
}
