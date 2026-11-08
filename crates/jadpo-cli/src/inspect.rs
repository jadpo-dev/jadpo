//! `jadpo inspect` and `jadpo impact` (areas 1, 2, 14 and 18): the project
//! map and what a change to one name reaches, read from the graph `jadpo
//! check` builds, so an agent asks rather than reading files to work it
//! out. Neither keeps an index of its own.
//!
//! - `jadpo inspect` prints the project map: each domain's declarations,
//!   with what each calls, runs and raises. `--json` prints the manifest.
//! - `jadpo inspect <name>` prints one node: where it is, its description,
//!   its signature or fields (a built type's final fields, each with where
//!   it came from, area 2), and its edges both ways. `--json` prints the
//!   node as the manifest has it, with its fields.
//! - `jadpo impact <name>` lists the domains that call or use it, the
//!   entry points and the tests a change to it reaches, and whether it is
//!   public ([`jadpo_graph::impact`]).
//!
//! Both refuse a project with errors, since a graph built from one leaves
//! out what couldn't be resolved, and what it leaves out would be missing
//! from the answer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, Rule, rule};
use jadpo_graph::impact::{self, Impact};
use jadpo_graph::manifest::{self, name_now};
use jadpo_graph::{Detail, EdgeKind, Graph, Key, Kind, Node, NodeId};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::fmt::project_not_found;
use crate::help::from_graph;
use crate::run::{Call, Reply};
use crate::suggest::nearest;

/// The kinds the map lists, in the order it lists them in each domain.
const MAP: &[Kind] = &[
    Kind::Entity,
    Kind::Type,
    Kind::Constant,
    Kind::Function,
    Kind::Query,
    Kind::Action,
    Kind::Fixture,
    Kind::Seed,
    Kind::Route,
    Kind::Schedule,
    Kind::Subscriber,
    Kind::Prompt,
    Kind::Connection,
];

/// The kinds a name never means: numbered per build, or the folders
/// themselves.
fn named_kind(kind: Kind) -> bool {
    !kind.per_build() && !matches!(kind, Kind::File | Kind::Domain)
}

/// The project, checked, and its graph; or the reply that stops the
/// command.
fn graph_of(
    call: &Call,
    command: &str,
) -> Result<(jadpo_db::Loaded, std::sync::Arc<Graph>), Reply> {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return Err(project_not_found(command));
    };
    let loaded = jadpo_db::load(&root);
    let errors: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project);
    if !errors.is_empty() {
        return Err(crate::migrate::blocked(
            &loaded,
            &cwd,
            errors,
            &format!("{command} stopped"),
        ));
    }
    let graph = std::sync::Arc::clone(jadpo_check::graph::graph(&loaded.db, loaded.project));
    Ok((loaded, graph))
}

/// Every name the project's declarations go by, for Tab completion.
pub(crate) fn names(cwd: &std::path::Path) -> Vec<(String, &'static str)> {
    let Some(root) = jadpo_db::find_root(cwd) else {
        return Vec::new();
    };
    let loaded = jadpo_db::load(&root);
    let graph = jadpo_check::graph::graph(&loaded.db, loaded.project);
    let mut out: Vec<(String, &'static str)> = graph
        .nodes()
        .iter()
        .filter(|node| MAP.contains(&node.kind) || node.kind == Kind::Library)
        .map(|node| (node.name.clone(), describe(node.kind)))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A kind as completion describes it.
fn describe(kind: Kind) -> &'static str {
    match kind {
        Kind::Entity => "Entity",
        Kind::Type => "Type",
        Kind::Constant => "Constant",
        Kind::Function => "Function",
        Kind::Query => "Query",
        Kind::Action => "Action",
        Kind::Fixture => "Fixture",
        Kind::Seed => "Seed",
        Kind::Route => "Route",
        Kind::Schedule => "Schedule",
        Kind::Subscriber => "Subscriber",
        Kind::Prompt => "Prompt",
        Kind::Connection => "Connection",
        Kind::Library => "Library",
        _ => "Declaration",
    }
}

/// The node `text` names: a key (`function sales.flour_for`), a name as
/// code writes it (`flour_for`, `sacks.weight_in`, `FlourShort.available`,
/// `POST /orders`), or a born name with its domain (`sales.flour_for`).
fn resolve(
    graph: &Graph,
    text: &str,
    command: &str,
    rule: &'static Rule,
) -> Result<NodeId, Box<Diagnostic>> {
    if let Some(id) = Key::from_text(text)
        .kind()
        .and_then(|_| graph.find(&Key::from_text(text)))
    {
        return Ok(id);
    }
    let found: Vec<NodeId> = graph
        .ids()
        .filter(|&id| {
            let node = graph.node(id);
            named_kind(node.kind)
                && (node.name == text || node.key.name() == text || node.key.written() == text)
        })
        .collect();
    // An entity's placement, tenancy and the like go by its name too; the
    // declaration itself is what the name means.
    let declared = |id: &NodeId| {
        let kind = graph.node(*id).kind;
        MAP.contains(&kind) || matches!(kind, Kind::Library | Kind::Field | Kind::Variant)
    };
    let found: Vec<NodeId> = if found.iter().any(declared) {
        found.into_iter().filter(declared).collect()
    } else {
        found
    };
    match found.as_slice() {
        [one] => Ok(*one),
        [] => {
            let known: Vec<&str> = graph
                .nodes()
                .iter()
                .filter(|node| named_kind(node.kind) && node.kind != Kind::Test)
                .map(|node| node.name.as_str())
                .collect();
            let diagnostic = Diagnostic::new(rule, format!("`{text}` isn't a declaration"));
            Err(Box::new(match nearest(text, known.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!(
                        "This project declares nothing called `{text}`. Did you mean `{near}`?"
                    ))
                    .next(format!("Run `jadpo {command} {}`", quoted(near))),
                None => diagnostic
                    .reason(format!("This project declares nothing called `{text}`."))
                    .next("Run `jadpo inspect` to list the project's declarations"),
            }))
        }
        many => {
            // Each by its born name with its domain, or by its whole key
            // when two kinds share one.
            let born: Vec<&str> = many.iter().map(|&id| graph.node(id).key.name()).collect();
            let distinct = born.iter().collect::<BTreeSet<_>>().len() == born.len();
            let names: Vec<String> = many
                .iter()
                .map(|&id| {
                    let key = &graph.node(id).key;
                    if distinct { key.name() } else { key.as_str() }.to_string()
                })
                .collect();
            let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
            Err(Box::new(
                Diagnostic::new(rule, format!("`{text}` names more than one declaration"))
                    .reason(format!(
                        "More than one declaration goes by that name: {}.",
                        and(&listed)
                    ))
                    .next(format!(
                        "Name one as it is listed, for example `jadpo {command} {}`",
                        quoted(&names[0])
                    )),
            ))
        }
    }
}

/// A name as typed at a shell: quoted when it holds a space.
fn quoted(name: &str) -> String {
    if name.contains(' ') {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `file:line`, from the project's root.
fn place(node: &Node) -> String {
    node.place.as_ref().map_or_else(
        || "generated".to_string(),
        |place| format!("{}:{}", place.file, place.lines.0),
    )
}

/// `action take_flour`: the kind's word and the name now.
fn title(node: &Node) -> String {
    format!("{} {}", node.kind.word(), node.name)
}

/// The domain a node belongs to, or `shared` for what `shared/` and the
/// root hold.
fn domain(node: &Node) -> String {
    node.domain.clone().unwrap_or_else(|| "shared".to_string())
}

fn reference(graph: &Graph, id: NodeId) -> Json {
    let node = graph.node(id);
    let mut entries = vec![
        ("id".to_string(), Json::from(node.key.as_str())),
        ("kind".to_string(), Json::from(node.kind.word())),
        ("name".to_string(), Json::from(node.name.as_str())),
    ];
    if let Some(place) = &node.place {
        entries.push(("file".to_string(), Json::from(place.file.as_str())));
        entries.push(("line".to_string(), Json::Int(i64::from(place.lines.0))));
    }
    Json::Object(entries)
}

// --- jadpo inspect -----------------------------------------------------------

pub fn inspect(call: &Call) -> Reply {
    let (loaded, graph) = match graph_of(call, "inspect") {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    match call.arguments.first() {
        None => map(&loaded, &graph),
        Some(name) => match resolve(&graph, name, "inspect", &rule::CLI_INSPECT_ARGUMENTS) {
            Ok(id) => one(&graph, id),
            Err(diagnostic) => Reply::Failed(diagnostic),
        },
    }
}

/// The project map, and the manifest as its `--json`.
fn map(loaded: &jadpo_db::Loaded, graph: &Graph) -> Reply {
    let mut domains: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
    for id in graph.ids() {
        let node = graph.node(id);
        // What the compiler makes itself, as the main connection, has no
        // place and is no declaration.
        if MAP.contains(&node.kind) && node.place.is_some() {
            domains.entry(domain(node)).or_default().push(id);
        }
    }
    // Domains by name, `shared` last.
    let mut order: Vec<String> = domains.keys().filter(|d| *d != "shared").cloned().collect();
    if domains.contains_key("shared") {
        order.push("shared".to_string());
    }
    // One column for places across the screen, set by the titles whose
    // line fits 80 columns with it; a longer title keeps two spaces.
    let width = domains
        .values()
        .flatten()
        .map(|&id| {
            let node = graph.node(id);
            (title(node).chars().count(), place(node).chars().count())
        })
        .filter(|(title, at)| 2 + title + 2 + at <= 80)
        .map(|(title, _)| title)
        .max()
        .unwrap_or(0);
    let mut screen = Screen::new();
    let mut declarations = 0;
    for name in &order {
        let mut ids = domains[name].clone();
        ids.sort_by_key(|&id| {
            let node = graph.node(id);
            (
                MAP.iter().position(|&kind| kind == node.kind),
                node.name.clone(),
            )
        });
        declarations += ids.len();
        let mut lines = Vec::new();
        for id in ids {
            let node = graph.node(id);
            let heading = title(node);
            let at = place(node);
            let pad = width.saturating_sub(heading.chars().count()) + 2;
            lines.push(format!("{heading}{}{at}", " ".repeat(pad)));
            for (label, value) in map_facts(graph, id) {
                lines.push(format!("  {label:<9}{value}"));
            }
        }
        screen.push(Block::section(name.as_str(), lines));
    }
    let tests = graph
        .nodes()
        .iter()
        .filter(|node| node.kind == Kind::Test)
        .count();
    if order.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "No declarations yet",
            [Fact::plain(plural(tests, "test", "tests"))],
        ));
    } else {
        screen.push(Block::summary(
            Outcome::Success,
            plural(declarations, "declaration", "declarations"),
            [
                Fact::plain(plural(
                    order.iter().filter(|d| *d != "shared").count(),
                    "domain",
                    "domains",
                )),
                Fact::plain(plural(tests, "test", "tests")),
            ],
        ));
    }
    let stamp = crate::derived::stamp(&loaded.db, loaded.project, &loaded.root, 0);
    let document = manifest::manifest(graph, &stamp);
    Reply::screen(screen, from_graph(&document))
}

/// What the map says under a declaration: what it calls, runs and raises,
/// and a failure's code, kind and status.
fn map_facts(graph: &Graph, id: NodeId) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let node = graph.node(id);
    if let Detail::Type {
        failure: Some(failure),
        ..
    } = &node.detail
    {
        let mut facts = vec![failure.code.clone()];
        facts.extend(failure.kind.clone());
        facts.extend(failure.status.map(|status| status.to_string()));
        out.push(("failure", facts.join(", ")));
    }
    for (kind, label) in [
        (EdgeKind::Runs, "runs"),
        (EdgeKind::Calls, "calls"),
        (EdgeKind::Raises, "raises"),
    ] {
        let names: Vec<String> = graph
            .outgoing(id, kind)
            .map(|to| graph.node(to).name.clone())
            .collect();
        if !names.is_empty() {
            out.push((label, names.join(", ")));
        }
    }
    out
}

/// One node.
fn one(graph: &Graph, id: NodeId) -> Reply {
    let node = graph.node(id);
    let name = |key: &Key| name_now(graph, key);
    let mut fields: Vec<(String, String)> = vec![("at".to_string(), place(node))];
    if node.public {
        fields.push(("public".to_string(), "yes".to_string()));
    }
    if let Some(doc) = &node.doc {
        fields.push(("about".to_string(), doc.clone()));
    }
    match &node.detail {
        Detail::Callable { signature, .. } => {
            fields.push(("type".to_string(), signature.render(&name)));
        }
        Detail::Type { base, .. } => {
            fields.push(("base".to_string(), base.render(&name)));
        }
        Detail::Constant { ty, value } => {
            fields.push((
                "value".to_string(),
                format!("{value}: {}", ty.render(&name)),
            ));
        }
        Detail::Entity { store, table } => {
            fields.push(("table".to_string(), format!("{table} in {store}")));
        }
        Detail::Field { ty, .. } => {
            fields.push(("type".to_string(), ty.render(&name)));
        }
        _ => {}
    }
    if node.kind == Kind::Library {
        let names: Vec<String> = graph
            .children(id)
            .map(|child| graph.node(child).name.clone())
            .collect();
        if !names.is_empty() {
            fields.push(("members".to_string(), names.join(", ")));
        }
    }
    let members = members(graph, id);
    if !members.is_empty() {
        if graph.fields(id).is_empty() {
            fields.push(("variants".to_string(), members.join(", ")));
        } else {
            fields.push(("fields".to_string(), members.join("\n")));
        }
    }
    for (kind, targets) in graph.edges_from(id) {
        let names: Vec<String> = targets
            .iter()
            .map(|&to| graph.node(to).name.clone())
            .collect();
        fields.push((edge_label(kind).to_string(), names.join(", ")));
    }
    for (kind, label) in [
        (EdgeKind::Calls, "callers"),
        (EdgeKind::Runs, "run by"),
        (EdgeKind::Uses, "used by"),
    ] {
        let names: Vec<String> = graph
            .incoming(id, kind)
            .filter(|&from| graph.node(from).kind != Kind::Test)
            .map(|from| graph.node(from).name.clone())
            .collect();
        if !names.is_empty() {
            fields.push((label.to_string(), names.join(", ")));
        }
    }
    let screen = Screen::new().with(Block::Answer {
        headline: title(node),
        fields,
    });
    let children: Vec<Json> = graph
        .fields(id)
        .into_iter()
        .chain(
            graph
                .children(id)
                .filter(|&child| graph.node(child).kind == Kind::Variant),
        )
        .map(|child| from_graph(&manifest::node_json(graph, child)))
        .collect();
    let mut data = vec![(
        "node".to_string(),
        from_graph(&manifest::node_json(graph, id)),
    )];
    if !children.is_empty() {
        data.push(("fields".to_string(), Json::Array(children)));
    }
    Reply::screen(screen, Json::Object(data))
}

/// An edge's word as a label, which the CLI design language keeps to
/// eight characters so values line up at column 12.
fn edge_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Subscribes => "hears",
        EdgeKind::References => "refers",
        EdgeKind::Invalidates => "clears",
        other => other.word(),
    }
}

/// A type's or entity's final fields in their order, each with where it
/// came from when any was copied (area 2): `title  from Todo.title`,
/// `lines: List<OrderLineView>  written, replaces Order.lines`; an enum's
/// variants.
fn members(graph: &Graph, id: NodeId) -> Vec<String> {
    let name = |key: &Key| name_now(graph, key);
    let fields = graph.fields(id);
    if fields.is_empty() {
        let mut variants: Vec<(u32, String)> = graph
            .children(id)
            .filter_map(|child| match &graph.node(child).detail {
                Detail::Variant { order, .. } => Some((*order, short(&graph.node(child).name))),
                _ => None,
            })
            .collect();
        variants.sort();
        return variants.into_iter().map(|(_, name)| name).collect();
    }
    let copies = fields.iter().any(|&field| {
        matches!(
            &graph.node(field).detail,
            Detail::Field {
                copied_from: Some(_),
                ..
            }
        )
    });
    fields
        .into_iter()
        .map(|field| {
            let node = graph.node(field);
            let Detail::Field {
                ty,
                optional,
                copied_from,
                replaces,
                ..
            } = &node.detail
            else {
                return short(&node.name);
            };
            let field = short(&node.name);
            let typed = format!(
                "{field}{}: {}",
                if *optional && !ty.render(&name).ends_with('?') {
                    "?"
                } else {
                    ""
                },
                ty.render(&name)
            );
            match (copies, copied_from, replaces) {
                (false, _, _) => typed,
                (true, Some(source), _) => format!("{field}  from {}", name(source)),
                (true, None, Some(replaced)) => {
                    format!("{typed}  written, replaces {}", name(replaced))
                }
                (true, None, None) => format!("{typed}  written"),
            }
        })
        .collect()
}

/// `title` for `Todo.title`.
fn short(name: &str) -> String {
    name.rsplit_once('.')
        .map_or(name, |(_, short)| short)
        .to_string()
}

// --- jadpo impact ------------------------------------------------------------

pub fn impact(call: &Call) -> Reply {
    let Some(name) = call.arguments.first() else {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_IMPACT_ARGUMENTS,
                "`jadpo impact` needs a declaration",
            )
            .reason("It lists what a change to one declaration reaches: the domains that call it, the entry points and the tests.")
            .next("Name one, for example `jadpo impact <name>`; `jadpo inspect` lists them"),
        );
    };
    let (loaded, graph) = match graph_of(call, "impact") {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let id = match resolve(&graph, name, "impact", &rule::CLI_IMPACT_ARGUMENTS) {
        Ok(id) => id,
        Err(diagnostic) => return Reply::Failed(diagnostic),
    };
    let target = graph.node(id);
    let found = reach(&graph, id);
    let calls = target.kind.is_callable() || target.kind == Kind::Library;
    // Call sites, from the typed bodies, for a callable's callers.
    let sites: BTreeMap<NodeId, usize> = if calls {
        let bodies = jadpo_check::deployables::bodies(&loaded.db, loaded.project);
        let targets: BTreeSet<&Key> = found
            .changed
            .iter()
            .map(|&changed| &graph.node(changed).key)
            .collect();
        found
            .users
            .iter()
            .map(|&user| {
                let count = bodies
                    .get(&graph.node(user).key)
                    .map(|body| {
                        body.calls()
                            .filter(|(_, callee)| {
                                callee.key().is_some_and(|key| targets.contains(key))
                            })
                            .count()
                    })
                    .unwrap_or(0)
                    .max(1);
                (user, count)
            })
            .collect()
    } else {
        found.users.iter().map(|&user| (user, 1)).collect()
    };
    let mut by_domain: BTreeMap<String, usize> = BTreeMap::new();
    for (&user, &count) in &sites {
        *by_domain.entry(domain(graph.node(user))).or_default() += count;
    }
    let (one, many) = if calls {
        ("call", "calls")
    } else {
        ("use", "uses")
    };
    let users = if by_domain.is_empty() {
        "none".to_string()
    } else {
        by_domain
            .iter()
            .map(|(domain, &count)| format!("{domain} ({})", plural(count, one, many)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let entry_points: Vec<NodeId> = sorted_entry_points(&graph, &found.entry_points);
    let entry_text = if entry_points.is_empty() {
        "none".to_string()
    } else {
        entry_points
            .iter()
            .map(|&entry| entry_name(graph.node(entry)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let test_files: BTreeSet<String> = found
        .tests
        .iter()
        .filter_map(|&test| graph.node(test).place.as_ref().map(|p| p.file.clone()))
        .collect();
    let tests_text = if test_files.is_empty() {
        "none".to_string()
    } else {
        test_files.iter().cloned().collect::<Vec<_>>().join("\n")
    };
    let fields = vec![
        ("at".to_string(), place(target)),
        (if calls { "callers" } else { "used in" }.to_string(), users),
        ("triggers".to_string(), entry_text),
        ("tests".to_string(), tests_text),
        (
            "public".to_string(),
            if target.public { "yes" } else { "no" }.to_string(),
        ),
    ];
    let screen = Screen::new().with(Block::Answer {
        headline: title(target),
        fields,
    });
    let mut data = match reference(&graph, id) {
        Json::Object(entries) => entries,
        _ => Vec::new(),
    };
    data.push(("public".to_string(), Json::Bool(target.public)));
    data.push((
        "users".to_string(),
        Json::Array(
            sites
                .iter()
                .map(|(&user, &count)| {
                    let mut entry = match reference(&graph, user) {
                        Json::Object(entries) => entries,
                        _ => Vec::new(),
                    };
                    entry.push(("domain".to_string(), Json::from(domain(graph.node(user)))));
                    if calls {
                        entry.push((
                            "calls".to_string(),
                            Json::Int(i64::try_from(count).unwrap_or(i64::MAX)),
                        ));
                    }
                    Json::Object(entry)
                })
                .collect(),
        ),
    ));
    data.push((
        "triggers".to_string(),
        Json::Array(
            entry_points
                .iter()
                .map(|&entry| reference(&graph, entry))
                .collect(),
        ),
    ));
    data.push((
        "tests".to_string(),
        Json::Array(
            found
                .tests
                .iter()
                .map(|&test| reference(&graph, test))
                .collect(),
        ),
    ));
    Reply::screen(screen, Json::Object(data))
}

/// What a change to `id` reaches; a library's is its members'.
fn reach(graph: &Graph, id: NodeId) -> Impact {
    if graph.node(id).kind != Kind::Library {
        return impact::impact(graph, id);
    }
    let mut out = Impact {
        target: id,
        changed: vec![id],
        users: Vec::new(),
        entry_points: Vec::new(),
        tests: Vec::new(),
        reached: Vec::new(),
    };
    let members: Vec<NodeId> = graph.children(id).collect();
    for &member in &members {
        let found = impact::impact(graph, member);
        out.changed.push(member);
        out.users.extend(found.users);
        out.entry_points.extend(found.entry_points);
        out.tests.extend(found.tests);
        out.reached.extend(found.reached);
    }
    for list in [
        &mut out.users,
        &mut out.entry_points,
        &mut out.tests,
        &mut out.reached,
    ] {
        list.retain(|node| !members.contains(node));
        list.sort();
        list.dedup();
    }
    out
}

/// Routes, then schedules, then subscribers, each by name.
fn sorted_entry_points(graph: &Graph, entries: &[NodeId]) -> Vec<NodeId> {
    let mut out = entries.to_vec();
    out.sort_by_key(|&id| {
        let node = graph.node(id);
        let rank = match node.kind {
            Kind::Route | Kind::GeneratedRoute => 0,
            Kind::Schedule => 1,
            _ => 2,
        };
        (rank, node.name.clone())
    });
    out
}

/// A route as its method and path; a schedule or subscriber with its kind.
fn entry_name(node: &Node) -> String {
    match node.kind {
        Kind::Route | Kind::GeneratedRoute => node.name.clone(),
        _ => title(node),
    }
}
