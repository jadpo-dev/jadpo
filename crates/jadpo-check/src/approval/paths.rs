//! The ways through the app the approval report shows (area 17,
//! "Reviewing a change"): for each decision, the path that makes it true,
//! actor → route → action or query → field → output; and the
//! declarations that didn't change but are now reached from a new place.
//! Both are read from the manifest's edges: a trigger (a route, schedule
//! or subscriber) runs an action or query, which calls others in turn.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::view::{Node, View, unwrapped};
use crate::acceptances::{Family, Need};

/// The most paths a decision shows; the rest are counted.
pub const MOST_PATHS: usize = 3;

/// One step of a path: a caller, or a declaration and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub name: String,
    /// `actor`, a node's kind (`route`, `query`), or `output` for what the
    /// data leaves by.
    pub kind: String,
    pub place: Option<(String, i64)>,
}

impl Step {
    fn actor(name: impl Into<String>) -> Step {
        Step {
            name: name.into(),
            kind: "actor".to_string(),
            place: None,
        }
    }

    fn node(node: &Node<'_>) -> Step {
        Step {
            name: node.shown(),
            kind: node.kind.to_string(),
            place: node.place(),
        }
    }

    fn output(name: impl Into<String>, place: Option<(String, i64)>) -> Step {
        Step {
            name: name.into(),
            kind: "output".to_string(),
            place,
        }
    }
}

/// A decision's paths: the shortest few, and how many more there are.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paths {
    pub shown: Vec<Vec<Step>>,
    pub more: usize,
    /// Whether the decision is about something a path can reach. A new
    /// domain or a sign-in setting isn't, so it has no path to look for.
    pub sought: bool,
}

const CALLABLES: &[&str] = &["action", "query", "function", "fixture"];
const TRIGGERS: &[&str] = &["route", "schedule", "subscriber"];

fn callable(node: &Node<'_>) -> bool {
    CALLABLES.contains(&node.kind)
}

/// How a caller a grant list names reads at the start of a path.
fn actor(who: &str) -> String {
    match who {
        "Access.public" => "anyone".to_string(),
        "Access.authenticated" => "anyone signed in".to_string(),
        "Access.system" => "a schedule or subscriber".to_string(),
        who if who.contains('.') => who.to_string(),
        field => format!("whoever {field} names"),
    }
}

/// What a decision is about, in the graph.
enum Target {
    /// A callable that reads an entity, and the field when it is one.
    Reads {
        entity: String,
        field: Option<String>,
    },
    /// A callable that creates, changes or deletes an entity's records.
    Writes { entity: String },
    /// An action or query by id.
    Callable { id: String },
    /// A route by its API and name, with what it runs.
    Route { api: String, name: String },
    /// A type clients send or read, and its field.
    ApiType {
        name: String,
        field: Option<String>,
        sent: bool,
    },
    /// A failure by its code.
    Failure { code: String },
    /// An event, its field, and the subscribers that read it.
    Event { name: String, field: Option<String> },
    /// The callables that go through a connection.
    Connection { name: String },
    /// The connections a package serves.
    Package { name: String },
}

/// `lending.Book.title.read` read as its domain, entity, field and effect.
fn grant_parts(subject: &str) -> (Option<&str>, Option<&str>, Option<&str>, Option<&str>) {
    let parts: Vec<&str> = subject.split('.').collect();
    let entity_at = parts
        .iter()
        .position(|part| part.chars().next().is_some_and(char::is_uppercase));
    let Some(at) = entity_at else {
        return (parts.first().copied(), None, None, None);
    };
    let domain = (at > 0).then(|| parts[at - 1]);
    let rest = &parts[at + 1..];
    let effects = ["create", "read", "update", "delete"];
    let (field, effect) = match rest {
        [] => (None, None),
        [one] if effects.contains(one) => (None, Some(*one)),
        [one] => (Some(*one), None),
        [field, effect, ..] => (Some(*field), Some(*effect)),
    };
    (domain, Some(parts[at]), field, effect)
}

fn target(view: &View<'_>, need: &Need) -> Option<Target> {
    let subject = need.subject.as_str();
    let callable_named = |subject: &str| {
        let (domain, name) = subject
            .rsplit_once('.')
            .map_or((None, subject), |(d, n)| (Some(d), n));
        CALLABLES
            .iter()
            .find_map(|kind| view.find(kind, domain, name))
            .map(|node| Target::Callable {
                id: node.id.to_string(),
            })
    };
    match need.change.as_str() {
        "PolicyChange.route_public" | "ApiChange.route_removed" => {
            let text = subject.trim_matches('"');
            let (api, name) = text.split_once(' ')?;
            Some(Target::Route {
                api: api.to_string(),
                name: name.to_string(),
            })
        }
        "PolicyChange.widened"
        | "PolicyChange.restriction_removed"
        | "PolicyChange.tenancy_loosened"
        | "PolicyChange.shared_entity_added" => {
            let (_, entity, field, effect) = grant_parts(subject);
            match (entity, effect) {
                (Some(entity), Some("create" | "update" | "delete")) => Some(Target::Writes {
                    entity: entity.to_string(),
                }),
                (Some(entity), _) => Some(Target::Reads {
                    entity: entity.to_string(),
                    field: field.map(str::to_string),
                }),
                (None, _) => callable_named(subject),
            }
        }
        "PolicyChange.listing_added" | "PolicyChange.listing_removed" => callable_named(subject),
        change if change.ends_with("_declined") => callable_named(subject),
        "NetworkChange.host_added" => Some(Target::Connection {
            name: subject.to_string(),
        }),
        "PackageChange.git_source" => Some(Target::Package {
            name: subject.to_string(),
        }),
        _ if need.family == Family::Events => {
            let mut parts = subject.splitn(3, '.');
            let _domain = parts.next();
            Some(Target::Event {
                name: parts.next()?.to_string(),
                field: parts.next().map(str::to_string),
            })
        }
        "ApiChange.failure_changed" => Some(Target::Failure {
            code: subject.split_once('.')?.1.to_string(),
        }),
        change if change.starts_with("ApiChange.") => {
            let mut parts = subject.splitn(3, '.');
            let _api = parts.next();
            Some(Target::ApiType {
                name: parts.next()?.to_string(),
                field: parts.next().map(str::to_string),
                sent: matches!(
                    change,
                    "ApiChange.input_field_required" | "ApiChange.rule_tightened"
                ),
            })
        }
        _ => None,
    }
}

/// Each trigger's shortest way to a callable `hit` accepts: the trigger
/// and the callables it runs and calls, in turn.
fn ways<'m>(
    view: &View<'m>,
    triggers: &[&Node<'m>],
    hit: &dyn Fn(&Node<'m>) -> bool,
) -> Vec<Vec<&'m str>> {
    let mut out = Vec::new();
    for trigger in triggers {
        let mut came: BTreeMap<&str, &str> = BTreeMap::new();
        let mut queue: VecDeque<&str> = VecDeque::new();
        for to in trigger.edges("runs") {
            if !came.contains_key(to) {
                came.insert(to, trigger.id);
                queue.push_back(to);
            }
        }
        while let Some(id) = queue.pop_front() {
            let Some(node) = view.get(id) else { continue };
            if !callable(node) {
                continue;
            }
            if hit(node) {
                let mut chain = vec![id];
                let mut at = id;
                while let Some(from) = came.get(at) {
                    chain.push(from);
                    if *from == trigger.id {
                        break;
                    }
                    at = from;
                }
                chain.reverse();
                out.push(chain);
                break;
            }
            for to in node.edges("calls") {
                if !came.contains_key(to) && to != trigger.id {
                    came.insert(to, id);
                    queue.push_back(to);
                }
            }
        }
    }
    out
}

/// The routes anyone may call, by API and name, from the API records.
pub type Public = BTreeSet<(String, String)>;

/// A route as a graph node names it: `POST /books/{book_id: Uuid}/borrow`,
/// as the API record writes it, read as `POST /books/{book_id}/borrow`.
pub fn route_name(written: &str) -> String {
    let mut out = String::new();
    let mut in_type = false;
    for c in written.chars() {
        match c {
            ':' if out.rfind('{') > out.rfind('}') => in_type = true,
            '}' => {
                in_type = false;
                out.push(c);
            }
            _ if in_type => {}
            _ => out.push(c),
        }
    }
    out
}

/// Whether `caller`, as a grant list names them, can take the way `chain`
/// goes: anyone only through a route anyone may call, schedules and
/// subscribers only as those triggers, and a person only through a route;
/// and each action or query on the way listed under `actions:` lets them
/// in, since its line is checked as it starts (area 9, "Listing a query").
fn may_start(view: &View<'_>, public: &Public, caller: &str, chain: &[&str]) -> bool {
    let Some(trigger) = chain.first().and_then(|id| view.get(id)) else {
        return false;
    };
    let system = caller == "Access.system";
    let starts = match trigger.kind {
        "route" if caller == "Access.public" => public.contains(&(
            trigger.text("api").unwrap_or("main").to_string(),
            trigger.name.to_string(),
        )),
        "route" => !system,
        _ => system,
    };
    starts
        && chain.iter().skip(1).all(|id| {
            view.incoming(id, "grants")
                .iter()
                .filter_map(|from| view.get(from))
                .filter(|from| from.kind == "action_exception")
                .all(|line| {
                    let who = line
                        .value
                        .get("who")
                        .and_then(|who| who.as_array())
                        .unwrap_or_default();
                    who.iter().filter_map(|who| who.as_str()).any(|who| {
                        who == caller
                            || (who == "Access.authenticated"
                                && caller != "Access.public"
                                && !system)
                    })
                })
        })
}

/// Who a path starts with, by the kind of change: the same for every path
/// of a change that names nobody; `None` for a change that names callers,
/// whose paths each start with those who can take them.
fn fixed_actor(need: &Need) -> Option<String> {
    match (&need.family, need.change.as_str()) {
        (_, "PolicyChange.route_public") => Some("anyone".to_string()),
        (Family::Api, _) => Some("a client".to_string()),
        _ => None,
    }
}

/// The triggers a path can start at, routes first.
fn triggers<'v, 'm>(view: &'v View<'m>, need: &Need) -> Vec<&'v Node<'m>> {
    let kinds: &[&str] = match &need.family {
        Family::Api => &["route"],
        _ => TRIGGERS,
    };
    let mut found: Vec<&Node<'m>> = view
        .nodes
        .values()
        .filter(|node| kinds.contains(&node.kind))
        .collect();
    found.sort_by_key(|node| (node.kind != "route", node.name));
    found
}

/// The steps of `chain`, a trigger and the callables it reaches.
fn steps(view: &View<'_>, chain: &[&str]) -> Vec<Step> {
    chain
        .iter()
        .filter_map(|id| view.get(id))
        .map(Step::node)
        .collect()
}

/// Whether the type written `from` is the type `name`, or names it in its
/// fields, at any depth: what a client sends or reads through it.
fn holds(view: &View<'_>, from: &str, domain: Option<&str>, name: &str) -> bool {
    if from == name {
        return true;
    }
    let Some(start) = view.type_named(from, domain) else {
        return false;
    };
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty.id) {
            continue;
        }
        if ty.name == name {
            return true;
        }
        // A type's fields name the types they hold.
        let fields = view
            .nodes
            .values()
            .filter(|node| node.kind == "field" && node.text("parent") == Some(ty.id));
        for id in fields
            .flat_map(|field| field.edges("uses"))
            .chain(ty.edges("uses"))
        {
            if let Some(next) = view.get(id).filter(|next| next.kind == "type") {
                pending.push(next);
            }
        }
    }
    false
}

/// What a route answers with, or the callable's result.
fn output_of(trigger: &Node<'_>, last: Option<&Node<'_>>) -> Option<String> {
    trigger.text("output").map(str::to_string).or_else(|| {
        last?
            .value
            .get("signature")?
            .get("result")?
            .as_str()
            .filter(|result| *result != "Unit")
            .map(str::to_string)
    })
}

/// The paths that make `need` true in `view`.
pub fn paths(view: &View<'_>, need: &Need, public: &Public) -> Paths {
    let Some(target) = target(view, need) else {
        return Paths::default();
    };
    let triggers = triggers(view, need);
    let fixed = fixed_actor(need);
    let mut found: Vec<Vec<Step>> = Vec::new();
    // The path with who starts it, or `None` when nobody the change lets
    // in can take the way it goes.
    let started = |mut path: Vec<Step>, chain: &[&str]| -> Option<Vec<Step>> {
        if need.who.is_empty() {
            if let Some(actor) = &fixed {
                path.insert(0, Step::actor(actor.clone()));
            }
            return Some(path);
        }
        let who: Vec<String> = need
            .who
            .iter()
            .filter(|caller| may_start(view, public, caller, chain))
            .map(|caller| actor(caller))
            .collect();
        if who.is_empty() {
            return None;
        }
        path.insert(0, Step::actor(who.join(", ")));
        Some(path)
    };
    match &target {
        Target::Reads { entity, field } => {
            let entity_id = format!("entity {entity}");
            let field_id = field
                .as_ref()
                .map(|field| format!("field {entity}.{field}"));
            // A query's includes read their entity's records too.
            let reads = |node: &Node<'_>| {
                node.edges("reads").contains(&entity_id.as_str())
                    || node.edges("includes").contains(&entity_id.as_str())
            };
            for chain in ways(view, &triggers, &reads) {
                let trigger = &view.nodes[chain[0]];
                let last = chain.last().and_then(|id| view.get(id));
                let output = output_of(trigger, last);
                let mut path = steps(view, &chain);
                match &field_id {
                    Some(field_id) => {
                        let field = field.as_deref().unwrap_or("");
                        let carried = output
                            .as_deref()
                            .and_then(|output| view.type_named(output, trigger.domain()))
                            .filter(|ty| ty.edges("uses").contains(&field_id.as_str()));
                        let used = carried.is_some()
                            || chain
                                .iter()
                                .filter_map(|id| view.get(id))
                                .any(|node| node.edges("uses").contains(&field_id.as_str()));
                        if !used {
                            continue;
                        }
                        path.push(match carried {
                            Some(ty) => Step::output(format!("{}.{field}", ty.name), ty.place()),
                            None => Step::output(
                                format!("{entity}.{field}"),
                                view.get(field_id).and_then(Node::place),
                            ),
                        });
                    }
                    None => path.push(match output {
                        Some(output) => Step::output(
                            output.clone(),
                            view.type_named(&output, trigger.domain())
                                .and_then(Node::place),
                        ),
                        None => {
                            Step::output(entity.clone(), view.get(&entity_id).and_then(Node::place))
                        }
                    }),
                }
                found.extend(started(path, &chain));
            }
        }
        Target::Writes { entity } => {
            let entity_id = format!("entity {entity}");
            let writes = |node: &Node<'_>| node.edges("writes").contains(&entity_id.as_str());
            for chain in ways(view, &triggers, &writes) {
                let mut path = steps(view, &chain);
                path.push(Step::output(
                    entity.clone(),
                    view.get(&entity_id).and_then(Node::place),
                ));
                found.extend(started(path, &chain));
            }
        }
        Target::Callable { id } => {
            let is = |node: &Node<'_>| node.id == id;
            for chain in ways(view, &triggers, &is) {
                found.extend(started(steps(view, &chain), &chain));
            }
        }
        Target::Route { api, name } => {
            let name = route_name(name);
            if let Some(route) = triggers.iter().find(|node| {
                node.kind == "route"
                    && node.name == name
                    && node.text("api").unwrap_or("main") == api
            }) {
                let mut chain = vec![route.id];
                chain.extend(route.edges("runs").into_iter().take(1));
                let last = chain.get(1).and_then(|id| view.get(id));
                let mut path = steps(view, &chain);
                if let Some(output) = output_of(route, last) {
                    let place = view
                        .type_named(&output, route.domain())
                        .and_then(Node::place);
                    path.push(Step::output(output, place));
                }
                found.extend(started(path, &chain));
            }
        }
        Target::ApiType { name, field, sent } => {
            let key = if *sent { "input" } else { "output" };
            for route in triggers.iter().filter(|node| node.kind == "route") {
                let Some(written) = route.text(key) else {
                    continue;
                };
                let held = unwrapped(written.split(' ').next().unwrap_or(written));
                if !holds(view, held, route.domain(), name) {
                    continue;
                }
                let ty = view.type_named(name, route.domain());
                let shown = match field {
                    Some(field) => format!("{name}.{field}"),
                    None => name.clone(),
                };
                found.extend(started(
                    vec![
                        Step::node(route),
                        Step::output(shown, ty.and_then(Node::place)),
                    ],
                    &[route.id],
                ));
            }
        }
        Target::Failure { code } => {
            let failure = view.of_kind("type").into_iter().find(|node| {
                node.value
                    .get("failure")
                    .and_then(|failure| failure.get("code"))
                    .and_then(|code| code.as_str())
                    == Some(code.as_str())
            });
            if let Some(failure) = failure {
                let raises = |node: &Node<'_>| node.edges("raises").contains(&failure.id);
                for chain in ways(view, &triggers, &raises) {
                    let mut path = steps(view, &chain);
                    path.push(Step::output(failure.name, failure.place()));
                    found.extend(started(path, &chain));
                }
            }
        }
        Target::Event { name, field } => {
            if let Some(event) = view.find("type", None, name) {
                let emits = |node: &Node<'_>| node.edges("emits").contains(&event.id);
                let shown = match field {
                    Some(field) => format!("{name}.{field}"),
                    None => name.clone(),
                };
                let subscribers: Vec<Step> = view
                    .incoming(event.id, "subscribes")
                    .iter()
                    .filter_map(|id| view.get(id))
                    .map(Step::node)
                    .take(1)
                    .collect();
                for chain in ways(view, &triggers, &emits) {
                    let mut path = steps(view, &chain);
                    path.push(Step::output(shown.clone(), event.place()));
                    path.extend(subscribers.iter().cloned());
                    found.extend(started(path, &chain));
                }
            }
        }
        Target::Connection { name } => {
            let id = format!("connection {name}");
            let through = |node: &Node<'_>| node.edges("uses").contains(&id.as_str());
            for chain in ways(view, &triggers, &through) {
                let mut path = steps(view, &chain);
                if let Some(connection) = view.get(&id) {
                    path.push(Step::node(connection));
                }
                found.extend(started(path, &chain));
            }
        }
        Target::Package { name } => {
            let package = format!("package {name}");
            let connections: Vec<&Node<'_>> = view
                .of_kind("connection")
                .into_iter()
                .filter(|node| node.edges("uses").contains(&package.as_str()))
                .collect();
            for connection in connections {
                let through = |node: &Node<'_>| node.edges("uses").contains(&connection.id);
                for chain in ways(view, &triggers, &through) {
                    let mut path = steps(view, &chain);
                    path.push(Step::node(connection));
                    path.push(Step::output(name.clone(), None));
                    found.extend(started(path, &chain));
                }
            }
        }
    }
    found.sort_by_key(|path| path.len());
    let more = found.len().saturating_sub(MOST_PATHS);
    found.truncate(MOST_PATHS);
    Paths {
        shown: found,
        more,
        sought: true,
    }
}

/// Every callable each trigger reaches, by the callable's id: the names
/// of the triggers that reach it.
fn reached<'m>(view: &View<'m>) -> BTreeMap<&'m str, BTreeSet<String>> {
    let mut out: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for trigger in view
        .nodes
        .values()
        .filter(|node| TRIGGERS.contains(&node.kind))
    {
        let label = trigger_label(trigger);
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut queue: Vec<&str> = trigger.edges("runs");
        while let Some(id) = queue.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(node) = view.get(id) else { continue };
            if !callable(node) {
                continue;
            }
            out.entry(node.id).or_default().insert(label.clone());
            queue.extend(node.edges("calls"));
        }
    }
    out
}

/// A trigger as the report names it: a route by its method and path, a
/// schedule or subscriber with its kind.
fn trigger_label(trigger: &Node<'_>) -> String {
    match trigger.kind {
        "route" => trigger.name.to_string(),
        kind => format!("{kind} {}", trigger.name),
    }
}

/// A declaration that didn't change but is now reached from somewhere new.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reachable {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub place: Option<(String, i64)>,
    /// The triggers that reach it now and didn't before.
    pub from: Vec<String>,
}

/// The callables on both sides whose source is the same and which a
/// trigger reaches now that none reached before: what a line diff never
/// shows. A package's own code is left to its upgrade's review.
pub fn newly_reachable(before: &View<'_>, after: &View<'_>) -> Vec<Reachable> {
    let was = reached(before);
    let now = reached(after);
    let mut out = Vec::new();
    for (id, from) in &now {
        let Some(node) = after.get(id) else { continue };
        let Some(old) = before.get(id) else { continue };
        if node.digest() != old.digest() || node.file().is_some_and(|f| f.starts_with("packages/"))
        {
            continue;
        }
        let empty = BTreeSet::new();
        let had = was.get(id).unwrap_or(&empty);
        let new: Vec<String> = from.difference(had).cloned().collect();
        if new.is_empty() {
            continue;
        }
        out.push(Reachable {
            id: id.to_string(),
            kind: node.kind.to_string(),
            name: node.shown(),
            place: node.place(),
            from: new,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::{grant_parts, route_name};

    #[test]
    fn a_route_as_its_record_writes_it_is_read_as_its_node_names_it() {
        assert_eq!(
            route_name("POST /books/{book_id: Uuid}/borrow"),
            "POST /books/{book_id}/borrow"
        );
        assert_eq!(route_name("GET /books"), "GET /books");
    }

    #[test]
    fn a_grant_subject_is_read_as_its_parts() {
        assert_eq!(
            grant_parts("lending.Book.title.read"),
            (Some("lending"), Some("Book"), Some("title"), Some("read"))
        );
        assert_eq!(
            grant_parts("lending.Book.read"),
            (Some("lending"), Some("Book"), None, Some("read"))
        );
        assert_eq!(
            grant_parts("lending.Book.cost"),
            (Some("lending"), Some("Book"), Some("cost"), None)
        );
    }
}
