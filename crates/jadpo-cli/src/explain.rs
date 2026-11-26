//! `jadpo explain` (area 14, "`jadpo explain`"): a digest of the app in
//! front of an agent, read from the graph `jadpo check` builds, so it can't
//! drift from the code. It is a map, not documentation: where the product's
//! rules are written, and that one not written is the owner's; who calls and what
//! tenancy limits them to; each entity's fields, lifecycle and policy; each
//! route with what it takes, gives and runs; the background work; the
//! acceptances already given; and, for each kind of construct the app
//! uses, the one docs topic that explains it, with the command that prints
//! it offline. An agent changing an existing app runs it first, rather than
//! reading every file and many docs pages.
//!
//! The screen is one block per part, in that order, each a bold headline
//! and rows on the label grid; routes are lines under their API. Nothing
//! in it depends on the time, the machine or the order files were read,
//! so it is the same for the same source. It refuses a project with
//! errors, as `jadpo inspect` does: a graph built from one leaves out what
//! couldn't be resolved.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_check::types::{Fact, callables_of, facts_at, typed_body};
use jadpo_graph::manifest::name_now;
use jadpo_graph::{Detail, EdgeKind, Graph, Key, Kind, Node, NodeId};
use jadpo_term::{Block, Fact as Shown, Json, Outcome, Screen};

use crate::run::{Call, Reply};

/// Each kind of construct the digest can name, the topic ID that explains
/// it, and what it is called on the screen. Every ID is a topic of an
/// area page, which `jadpo help topic <id>` prints (a test holds them to
/// it).
const TOPICS: &[(Construct, &str, &str)] = &[
    (Construct::Principal, "the-principal", "callers"),
    (Construct::SignIn, "ways-to-sign-in", "sign-in"),
    (Construct::Tenancy, "tenancy", "tenancy"),
    (Construct::Membership, "a-membership-role", "roles"),
    (Construct::ActionGrant, "action-permissions", "listed"),
    (Construct::Entity, "entities-and-the-id", "entities"),
    (
        Construct::Reference,
        "references-and-on-delete",
        "references",
    ),
    (Construct::Unique, "unique-rules", "unique"),
    (Construct::Lifecycle, "lifecycle", "lifecycle"),
    (Construct::Grant, "grants", "policy"),
    (
        Construct::FieldRestriction,
        "field-restrictions",
        "field policy",
    ),
    (Construct::Route, "route-groups", "routes"),
    (Construct::Query, "named-queries", "queries"),
    (Construct::Action, "functions-and-actions", "actions"),
    (Construct::Failure, "declaring-a-failure", "failures"),
    (Construct::Setting, "settings-config-jadpo", "settings"),
    (Construct::Schedule, "schedules", "schedules"),
    (Construct::Event, "emitting-an-event", "events"),
    (Construct::Subscriber, "subscribers", "subscribers"),
    (Construct::Batch, "batches-work-over-many-items", "batches"),
    (Construct::Acceptance, "acceptances", "acceptances"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Construct {
    Principal,
    SignIn,
    Tenancy,
    Membership,
    ActionGrant,
    Entity,
    Reference,
    Unique,
    Lifecycle,
    Grant,
    FieldRestriction,
    Route,
    Query,
    Action,
    Failure,
    Setting,
    Schedule,
    Event,
    Subscriber,
    Batch,
    Acceptance,
}

pub fn explain(call: &Call) -> Reply {
    let (loaded, graph) = match crate::inspect::graph_of(call, "explain") {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let (db, project) = (&loaded.db, loaded.project);
    let name = |key: &Key| name_now(&graph, key);
    let mut used: BTreeSet<Construct> = BTreeSet::new();
    let mut screen = Screen::new();
    let mut data: Vec<(&str, Json)> = Vec::new();

    // Where the product's rules are written, and that one not written down
    // is the owner's to decide (chunk 10.11a).
    let cwd = call
        .cwd
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let readme = jadpo_db::find_root(&cwd)
        .and_then(|root| std::fs::read_to_string(root.join("README.md")).ok());
    let sections = readme.as_deref().map(rules_sections);
    let written = match &sections {
        None => "each entity's policy and about: below; the project has no README".to_string(),
        Some(sections) if sections.is_empty() => {
            "README.md; each entity's policy and about: below".to_string()
        }
        Some(sections) => format!(
            "README.md, {}; each entity's policy and about: below",
            sections
                .iter()
                .map(|section| format!("\"{section}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let unwritten = "the owner's to decide; ask them before choosing one";
    data.push((
        "rules",
        Json::object([
            (
                "written",
                Json::array(sections.iter().flat_map(|sections| {
                    let whole = sections.is_empty().then(|| {
                        Json::object([("file", "README.md".into()), ("section", Json::Null)])
                    });
                    sections
                        .iter()
                        .map(|section| {
                            Json::object([
                                ("file", "README.md".into()),
                                ("section", section.as_str().into()),
                            ])
                        })
                        .chain(whole)
                        .collect::<Vec<_>>()
                })),
            ),
            ("unwritten", unwritten.into()),
        ]),
    ));
    screen.push(Block::Answer {
        headline: "Rules".to_string(),
        fields: vec![
            ("written".to_string(), written),
            ("unwritten".to_string(), unwritten.to_string()),
        ],
    });

    // Who calls, and what tenancy limits them to.
    let memberships = jadpo_check::policy::memberships(db, project);
    let root = jadpo_check::policy::root_policy(db, project);
    let mut access: Vec<(String, String)> = Vec::new();
    for id in of_kind(&graph, Kind::Principal) {
        let node = graph.node(id);
        if let Detail::Principal {
            entity,
            subject,
            active,
        } = &node.detail
        {
            used.insert(Construct::Principal);
            let mut value = format!("{}: {} by {subject}", node.name, name(entity));
            if let Some(active) = active {
                value.push_str(&format!(", active when {active}"));
            }
            access.push(("caller".to_string(), value));
        }
    }
    let methods: Vec<String> = of_kind(&graph, Kind::AuthenticationMethod)
        .map(|id| graph.node(id).name.clone())
        .collect();
    if !methods.is_empty() {
        used.insert(Construct::SignIn);
        access.push(("sign-in".to_string(), methods.join(", ")));
    }
    let mut scoped: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut unscoped: Vec<String> = Vec::new();
    for id in of_kind(&graph, Kind::Entity) {
        let tenancy = graph
            .children(id)
            .find_map(|child| match &graph.node(child).detail {
                Detail::Tenancy { tenant, via } => Some((name(tenant), via.clone())),
                _ => None,
            });
        match tenancy {
            Some((tenant, via)) if via != "id" => scoped
                .entry((tenant, via))
                .or_default()
                .push(graph.node(id).name.clone()),
            Some(_) => {}
            None => unscoped.push(graph.node(id).name.clone()),
        }
    }
    if let Some(tenant) = &root.tenant {
        used.insert(Construct::Tenancy);
        access.push(("tenant".to_string(), tenant.clone()));
        for ((_, via), entities) in &scoped {
            access.push((
                "scoped".to_string(),
                format!("{} by {via}", entities.join(", ")),
            ));
        }
        if !unscoped.is_empty() {
            access.push(("unscoped".to_string(), unscoped.join(", ")));
        }
    }
    for membership in memberships.iter() {
        used.insert(Construct::Membership);
        let roles = variants(&graph, &membership.roles);
        let held = match &membership.scope_entity {
            Some(scope) => format!(" in each {scope}"),
            None => String::new(),
        };
        access.push((
            "roles".to_string(),
            format!(
                "{} from {}.{}{held}: {}",
                membership.roles,
                membership.entity,
                membership.role,
                roles.join(", ")
            ),
        ));
    }
    for line in root.actions.iter().chain(&root.queries) {
        used.insert(Construct::ActionGrant);
        access.push((
            "listed".to_string(),
            format!("{}: {}", line.written, line.written_who.join(", ")),
        ));
    }
    if !access.is_empty() {
        data.push((
            "access",
            Json::Array(
                access
                    .iter()
                    .map(|(label, value)| {
                        Json::object([
                            ("label", label.as_str().into()),
                            ("value", value.as_str().into()),
                        ])
                    })
                    .collect(),
            ),
        ));
        screen.push(Block::Answer {
            headline: "Access".to_string(),
            fields: access,
        });
    }

    // Each entity: its fields, lifecycle and policy.
    let mut entities = Vec::new();
    for id in sorted(&graph, of_kind(&graph, Kind::Entity)) {
        used.insert(Construct::Entity);
        let node = graph.node(id);
        let mut rows: Vec<(String, String)> = Vec::new();
        if let Some(doc) = &node.doc {
            rows.push(("about".to_string(), first_line(doc)));
        }
        let mut fields = Vec::new();
        let mut restrictions = Vec::new();
        for field in graph.fields(id) {
            let field_node = graph.node(field);
            let Detail::Field { ty, optional, .. } = &field_node.detail else {
                continue;
            };
            let mut shown = ty.render(&name);
            if *optional && !shown.ends_with('?') {
                shown.push('?');
            }
            let short = field_node.name.rsplit('.').next().unwrap_or_default();
            let mut text = format!("{short}: {shown}");
            for child in graph.children(field) {
                match &graph.node(child).detail {
                    Detail::Reference { on_delete, .. } => {
                        used.insert(Construct::Reference);
                        text.push_str(&format!(" ({on_delete})"));
                    }
                    Detail::FieldRestriction {
                        server_only,
                        grants,
                    } => {
                        used.insert(Construct::FieldRestriction);
                        restrictions.push(if *server_only {
                            format!("{short}: server only")
                        } else {
                            let effects: Vec<String> = grants
                                .iter()
                                .map(|(effect, who)| format!("{effect} [{}]", who.join(", ")))
                                .collect();
                            format!("{short}: {}", effects.join(", "))
                        });
                    }
                    _ => {}
                }
            }
            fields.push(text);
        }
        rows.push(("fields".to_string(), fields.join(", ")));
        let children: Vec<NodeId> = graph.children(id).collect();
        for &child in &children {
            if let Detail::Columns { fields, .. } = &graph.node(child).detail
                && graph.node(child).kind == Kind::UniqueRule
            {
                used.insert(Construct::Unique);
                let rule = graph
                    .node(child)
                    .name
                    .rsplit('.')
                    .next()
                    .unwrap_or_default();
                rows.push((
                    "unique".to_string(),
                    format!("{rule} [{}]", fields.join(", ")),
                ));
            }
        }
        if let Some(membership) = memberships.iter().find(|m| m.entity == node.name) {
            let scope = membership
                .scope
                .as_ref()
                .map(|scope| format!("scope {scope}, "))
                .unwrap_or_default();
            rows.push((
                "roles".to_string(),
                format!(
                    "{scope}member {}, role {}",
                    membership.member, membership.role
                ),
            ));
        }
        for &child in &children {
            let lifecycle = graph.node(child);
            let Detail::Lifecycle { visible, purge } = &lifecycle.detail else {
                continue;
            };
            used.insert(Construct::Lifecycle);
            if let Some(visible) = visible {
                rows.push(("visible".to_string(), visible.clone()));
            }
            for transition in sorted(&graph, graph.children(child)) {
                let transition = graph.node(transition);
                let Detail::Transition { when, sets, .. } = &transition.detail else {
                    continue;
                };
                let short = transition.name.rsplit('.').next().unwrap_or_default();
                let mut text = short.to_string();
                if let Some(when) = when {
                    text.push_str(&format!(" when {when}"));
                }
                if !sets.is_empty() {
                    text.push_str(&format!(", sets {}", sets.join(", ")));
                }
                rows.push(("change".to_string(), text));
            }
            if let Some(purge) = purge {
                rows.push(("purge".to_string(), purge.clone()));
            }
        }
        let mut grants: Vec<(usize, String, String)> = children
            .iter()
            .filter_map(|&child| match &graph.node(child).detail {
                Detail::Grant { effect, who } => Some((
                    ["create", "read", "update", "delete"]
                        .iter()
                        .position(|e| e == effect)
                        .unwrap_or(4),
                    effect.clone(),
                    if who.is_empty() {
                        "nobody".to_string()
                    } else {
                        who.join(", ")
                    },
                )),
                _ => None,
            })
            .collect();
        grants.sort();
        if !grants.is_empty() {
            used.insert(Construct::Grant);
        }
        for (_, effect, who) in grants {
            rows.push((effect, who));
        }
        for restriction in restrictions {
            rows.push(("field".to_string(), restriction));
        }
        entities.push(Json::object([
            ("name", node.name.as_str().into()),
            ("at", place(node).into()),
            (
                "rows",
                Json::Array(
                    rows.iter()
                        .map(|(label, value)| {
                            Json::object([
                                ("label", label.as_str().into()),
                                ("value", value.as_str().into()),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]));
        screen.push(Block::Answer {
            headline: format!("{}  {}", node.name, place(node)),
            fields: rows,
        });
    }
    if !entities.is_empty() {
        data.push(("entities", Json::Array(entities)));
    }

    // Routes, under their API, in the order a URL is read.
    let groups = jadpo_check::routes::groups(db, project);
    let mut routes: Vec<(String, String, String, String)> = Vec::new();
    for (group, route) in groups.routes() {
        used.insert(Construct::Route);
        let path = route.path();
        let method = route.method.word().to_string();
        let mut text = format!("{method} {path} ");
        if let Some(node) = graph.get(&route.key)
            && let Detail::Route {
                input,
                output,
                status,
                ..
            } = &node.detail
        {
            if let Some(input) = input {
                text.push_str(&input.render(&name));
                text.push(' ');
            }
            text.push_str("-> ");
            match output {
                Some(output) => text.push_str(&output.render(&name)),
                None => text.push_str(&status.to_string()),
            }
            if output.is_some() && *status != 200 {
                text.push_str(&format!(" {status}"));
            }
        }
        text.push_str(&format!(
            "  {} {}",
            if route.run.query { "query" } else { "runs" },
            route.run.name
        ));
        match route.authentication {
            jadpo_std::http::Authentication::Public => text.push_str("  no sign-in"),
            jadpo_std::http::Authentication::Fresh => text.push_str("  fresh sign-in"),
            jadpo_std::http::Authentication::Required => {}
        }
        routes.push((group.api.clone(), path, method, text));
    }
    routes.sort();
    let apis = jadpo_check::routes::api_table(db, project);
    let mut route_data = Vec::new();
    for api in &apis.apis {
        let lines: Vec<String> = routes
            .iter()
            .filter(|(of, ..)| *of == api.name)
            .map(|(.., text)| text.clone())
            .collect();
        if lines.is_empty() {
            continue;
        }
        route_data.extend(lines.iter().map(|line| Json::from(line.as_str())));
        screen.push(Block::section(
            format!("Routes  {}  {}", api.name, api.path()),
            lines,
        ));
    }
    if !route_data.is_empty() {
        data.push(("routes", Json::Array(route_data)));
    }

    // Background work: schedules, events and their subscribers, batches.
    let mut background: Vec<(String, String)> = Vec::new();
    for id in sorted(&graph, of_kind(&graph, Kind::Schedule)) {
        let node = graph.node(id);
        // A purge schedule is the lifecycle's, shown with its entity.
        if node.parent.is_some() {
            continue;
        }
        let Detail::Schedule { every_ms } = node.detail else {
            continue;
        };
        used.insert(Construct::Schedule);
        let runs = targets(&graph, id, EdgeKind::Runs);
        background.push((
            "schedule".to_string(),
            format!(
                "{} every {}, runs {}",
                node.name,
                crate::schedules::every(every_ms),
                runs.join(", ")
            ),
        ));
    }
    let mut events: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for edge in graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Emits)
    {
        let from = graph.node(edge.from);
        if from.kind == Kind::Test || from.place.is_none() {
            continue;
        }
        events
            .entry(graph.node(edge.to).name.clone())
            .or_default()
            .0
            .push(from.name.clone());
    }
    for id in sorted(&graph, of_kind(&graph, Kind::Subscriber)) {
        used.insert(Construct::Subscriber);
        let node = graph.node(id);
        let runs = targets(&graph, id, EdgeKind::Runs);
        for event in targets(&graph, id, EdgeKind::Subscribes) {
            events.entry(event).or_default().1.push(format!(
                "{} runs {}",
                node.name,
                runs.join(", ")
            ));
        }
    }
    for (event, (emitted, subscribers)) in &mut events {
        used.insert(Construct::Event);
        emitted.sort();
        emitted.dedup();
        let mut text = event.clone();
        if !emitted.is_empty() {
            text.push_str(&format!(" from {}", emitted.join(", ")));
        }
        for subscriber in subscribers.iter() {
            text.push_str(&format!("; subscriber {subscriber}"));
        }
        background.push(("event".to_string(), text));
    }
    let mut batches = Vec::new();
    for &file in project.files(db) {
        for callable in callables_of(db, file) {
            let checked = typed_body(db, project, callable);
            let Some(key) = checked
                .key
                .as_ref()
                .filter(|key| key.kind() == Some(Kind::Action))
            else {
                continue;
            };
            for (fact, _) in facts_at(&checked.body) {
                let Fact::Batch(batch) = fact else {
                    continue;
                };
                let mut text = name(key);
                if let Some(query) = &batch.query {
                    text.push_str(&format!(" over {query}"));
                } else if batch.file {
                    text.push_str(" over a file's rows");
                } else if batch.listed {
                    text.push_str(" over a provider's list");
                }
                if let Some(each) = &batch.each {
                    text.push_str(&format!(", each {}", name(each)));
                }
                if let Some(finished) = &batch.finished {
                    text.push_str(&format!(", finished {}", name(finished)));
                }
                batches.push(text);
            }
        }
    }
    batches.sort();
    batches.dedup();
    for batch in batches {
        used.insert(Construct::Batch);
        background.push(("batch".to_string(), batch));
    }
    if !background.is_empty() {
        data.push((
            "background",
            Json::Array(
                background
                    .iter()
                    .map(|(label, value)| {
                        Json::object([
                            ("label", label.as_str().into()),
                            ("value", value.as_str().into()),
                        ])
                    })
                    .collect(),
            ),
        ));
        screen.push(Block::Answer {
            headline: "Background work".to_string(),
            fields: background,
        });
    }

    // The acceptances already given, which go on allowing what they list.
    let given = jadpo_check::acceptances::files::acceptances(db, project);
    let mut accepted: Vec<(String, String)> = Vec::new();
    let mut accepted_data = Vec::new();
    for acceptance in &given {
        used.insert(Construct::Acceptance);
        let file = acceptance
            .path
            .strip_prefix("acceptances/")
            .and_then(|name| name.strip_suffix(".jadpo"))
            .unwrap_or(&acceptance.path);
        let entries: Vec<String> = acceptance
            .entries
            .iter()
            .map(|entry| format!("{}: {}", entry.subject, entry.change))
            .collect();
        let mut text = entries.join(", ");
        if let Some(reason) = &acceptance.reason {
            text.push_str(&format!(" ({})", first_line(reason)));
        }
        accepted_data.push(Json::object([
            ("file", acceptance.path.as_str().into()),
            (
                "changes",
                Json::Array(entries.iter().map(|e| Json::from(e.as_str())).collect()),
            ),
            (
                "reason",
                acceptance.reason.as_deref().map_or(Json::Null, Json::from),
            ),
        ]));
        accepted.push((file.to_string(), text));
    }
    if !accepted.is_empty() {
        data.push(("acceptances", Json::Array(accepted_data)));
        screen.push(Block::Answer {
            headline: "Acceptances".to_string(),
            fields: accepted,
        });
    }

    // The rest of what the app is written with.
    let any = |kind: Kind| of_kind(&graph, kind).any(|id| graph.node(id).place.is_some());
    if any(Kind::Query) {
        used.insert(Construct::Query);
    }
    if any(Kind::Action) {
        used.insert(Construct::Action);
    }
    if any(Kind::ConfigField) {
        used.insert(Construct::Setting);
    }
    // A failure the app declares has a file of its own in `failures/`; a
    // not-found failure the compiler provides is placed at its entity.
    if of_kind(&graph, Kind::Type).any(|id| {
        let node = graph.node(id);
        matches!(
            &node.detail,
            Detail::Type {
                failure: Some(_),
                ..
            }
        ) && node
            .place
            .as_ref()
            .is_some_and(|place| place.file.contains("failures/"))
    }) {
        used.insert(Construct::Failure);
    }
    let named: Vec<(&str, &str)> = TOPICS
        .iter()
        .filter(|(construct, ..)| used.contains(construct))
        .map(|(_, topic, label)| (*label, *topic))
        .collect();
    let width = named
        .iter()
        .map(|(label, _)| label.len())
        .max()
        .unwrap_or(0);
    let docs: Vec<String> = named
        .iter()
        .map(|(label, topic)| format!("{label:<width$}  jadpo help topic {topic}"))
        .collect();
    data.push((
        "docs",
        Json::Array(
            TOPICS
                .iter()
                .filter(|(construct, ..)| used.contains(construct))
                .map(|(_, topic, label)| {
                    Json::object([("construct", (*label).into()), ("topic", (*topic).into())])
                })
                .collect(),
        ),
    ));
    screen.push(Block::section("Docs", docs));

    let count = |kind: Kind| {
        of_kind(&graph, kind)
            .filter(|&id| graph.node(id).place.is_some())
            .count()
    };
    let entities = count(Kind::Entity);
    let routes = count(Kind::Route);
    screen.push(Block::summary(
        Outcome::Success,
        plural(entities, "entity", "entities"),
        [
            (routes, "route", "routes"),
            (count(Kind::Action), "action", "actions"),
            (count(Kind::Query), "query", "queries"),
        ]
        .into_iter()
        .filter(|(n, ..)| *n > 0)
        .map(|(n, one, many)| Shown::plain(plural(n, one, many))),
    ));
    screen.push(Block::hint(
        "jadpo inspect <name>",
        "shows one declaration's fields, signature and edges",
    ));
    Reply::screen(
        screen,
        Json::Object(
            data.into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        ),
    )
}

/// The `##` headings of a README that hold its rules: those that name rules,
/// policy or permissions, or say who may do what.
fn rules_sections(readme: &str) -> Vec<String> {
    readme
        .lines()
        .filter_map(|line| line.strip_prefix("## "))
        .map(str::trim)
        .filter(|heading| {
            let lower = heading.to_lowercase();
            ["rule", "polic", "permission", "who may", "who can"]
                .iter()
                .any(|word| lower.contains(word))
        })
        .map(str::to_string)
        .collect()
}

/// The nodes of `kind`, in key order.
fn of_kind(graph: &Graph, kind: Kind) -> impl Iterator<Item = NodeId> + '_ {
    graph.ids().filter(move |&id| graph.node(id).kind == kind)
}

/// `ids` by their domain, then their name now.
fn sorted(graph: &Graph, ids: impl Iterator<Item = NodeId>) -> Vec<NodeId> {
    let mut ids: Vec<NodeId> = ids.collect();
    ids.sort_by(|&a, &b| {
        let (a, b) = (graph.node(a), graph.node(b));
        (&a.domain, &a.name).cmp(&(&b.domain, &b.name))
    });
    ids
}

/// Where `kind`'s edges from `id` lead, by their names now, sorted.
fn targets(graph: &Graph, id: NodeId, kind: EdgeKind) -> Vec<String> {
    let mut out: Vec<String> = graph
        .outgoing(id, kind)
        .map(|to| graph.node(to).name.clone())
        .collect();
    out.sort();
    out
}

/// The variants of the enum the app names `enum_name`, in the order they
/// are declared.
fn variants(graph: &Graph, enum_name: &str) -> Vec<String> {
    let Some(id) = of_kind(graph, Kind::Type).find(|&id| graph.node(id).name == enum_name) else {
        return Vec::new();
    };
    let mut out: Vec<(u32, String)> = graph
        .children(id)
        .filter_map(|child| match &graph.node(child).detail {
            Detail::Variant { order, .. } => Some((
                *order,
                graph
                    .node(child)
                    .name
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            )),
            _ => None,
        })
        .collect();
    out.sort();
    out.into_iter().map(|(_, name)| name).collect()
}

/// `file:line`, from the project's root.
fn place(node: &Node) -> String {
    node.place.as_ref().map_or_else(
        || "generated".to_string(),
        |place| format!("{}:{}", place.file, place.lines.0),
    )
}

/// The first line of a description: what the digest says of it.
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_string()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::TOPICS;

    #[test]
    fn every_topic_it_names_is_a_topic_of_the_area_pages() {
        for (construct, id, _) in TOPICS {
            let found = crate::help::find_topic(id);
            assert!(
                found.is_some(),
                "{construct:?} names `{id}`, which no area page declares"
            );
            assert_eq!(
                found.expect("found").id,
                *id,
                "`{id}` is an old ID; name the topic by its ID now"
            );
        }
    }

    #[test]
    fn each_construct_has_one_label_and_one_topic() {
        let mut labels: Vec<&str> = TOPICS.iter().map(|(_, _, label)| *label).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), TOPICS.len(), "two constructs share a label");
    }
}
