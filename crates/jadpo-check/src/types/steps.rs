//! Several changing calls in one action (area 12, "Several changing calls
//! in one action"; area 11, the same; chunk 7.5), and no outside call
//! inside `for` (area 3).
//!
//! A *changing call* is an outside call that changes something: a provider
//! operation with `changes: true`, or a store of files' `put`. Each way
//! through an action is counted on its own, as [`super::outside`] walks it:
//! every way through an `if` or a `match`, a `return`, `reject` or
//! `propagate` ending its way, and an action called counting for the
//! changing calls the longest way through it makes. What each action
//! makes is worked out once for the project ([`action_steps`]), and each
//! file's problems read it ([`step_problems`]):
//!
//! - **`effect.call_in_loop`**: an outside call inside a `for` body, of
//!   any kind, made there or by an action called there, in any action, so
//!   every way through it has a length the compiler knows.
//! - **`step.too_many`**: more than [`MOST_STEPS`] changing calls on one
//!   way, at the call that passes it; an action called that passes it on
//!   its own is reported there instead.
//! - **`step.unkeyed`**: a changing call with no key in an action that can
//!   make another changing call, on any way through it: such a call is
//!   the only changing call in its action.
//! - **`step.large_output`**: an operation marked `large_output: true` in
//!   an action whose longest way makes several changing calls, whose
//!   answers are kept for a rerun.
//! - **`step.context`**: an action making several changing calls on one
//!   way, run where its key doesn't survive a retry: by a schedule, a
//!   route whose API doesn't require `Idempotency-Key`, or as a run's item
//!   (`each:` or `finished:`). Reported where it is run.
//! - **`step.store_required`**: such an action run by a subscriber, or a
//!   route whose API requires the key, that writes no store, with no
//!   `events: { store: }` to keep its record in; one that emits is told
//!   the same by `event.store_required`.
//! - **`step.encryption_key_required`** (chunk 7.5b): the first action
//!   making several changing calls on one way, when `application {
//!   steps: { encryption_key: } }` names no `SigningKey` setting to encrypt
//!   their answers with; `step.settings_invalid` for a `steps:` block that
//!   doesn't read ([`steps_settings`]).
//!
//! The runtime's record of such an action is planned here too
//! ([`record_plans`]): the store it is kept in and a digest of its step
//! shape.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::body::{ArmBody, Body, Callee, Expr, Node, SiteId, Stmt};
use jadpo_graph::{Key, Kind};
use jadpo_std::files::FilesOp;

use super::facts::{Fact, facts_of};
use super::outside::{Does, Names, bodies, children, does};
use crate::lookups::{Lookup, Name};

/// The most changing calls one way through an action may make (area 12),
/// counting those of the actions it calls.
pub const MOST_STEPS: u32 = 8;

/// What an outside call is to these rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Call {
    /// It changes nothing.
    Reads,
    /// It changes something under its package's key, or is safe to send
    /// again as it is (`idempotent: true`), or is a store's `put`.
    Safe,
    /// It changes something with no key.
    Unkeyed,
}

/// Each provider operation each connection reaches, by the connection's
/// name and the operation as a call writes it (`customers.create`): how it
/// may be repeated, and whether its answer is large.
struct Calls {
    operations: BTreeMap<(String, String), (Call, bool)>,
    /// Each package's operations by its folder, for its own actions'
    /// calls, which name no connection (chunk 7.6).
    own: BTreeMap<(String, String), (Call, bool)>,
}

impl Calls {
    fn new(db: &dyn Db, project: Project) -> Calls {
        let operations = crate::providers::operations(db, project);
        let call = |operation: &crate::providers::Operation| {
            let call = match (operation.changes, operation.idempotent) {
                (false, _) => Call::Reads,
                (true, true) => Call::Safe,
                (true, false) => Call::Unkeyed,
            };
            (call, operation.large_output)
        };
        let mut out = BTreeMap::new();
        for connection in &crate::connections::package_connections(db, project).connections {
            for operation in operations.of(&connection.folder) {
                out.insert(
                    (connection.name.clone(), operation.full_name()),
                    call(operation),
                );
            }
        }
        let own = operations
            .list
            .iter()
            .map(|operation| {
                (
                    (operation.folder.clone(), operation.full_name()),
                    call(operation),
                )
            })
            .collect();
        Calls {
            operations: out,
            own,
        }
    }

    /// What the outside call at `node` is, and whether its answer is
    /// large; `None` for one that isn't known, which is reported where it
    /// is made. `package` is the folder of the package whose action makes
    /// it, if one.
    fn of(&self, node: &Node, package: Option<&str>) -> Option<(Call, bool)> {
        let Node::Expr(Expr::Call { callee, .. }) = node else {
            return None;
        };
        match callee {
            // A package action's own operation (chunk 7.6).
            Callee::Provider {
                connection,
                operation,
            } if connection == jadpo_graph::body::OWN_CONNECTION => self
                .own
                .get(&(package?.to_string(), operation.clone()))
                .copied(),
            Callee::Provider {
                connection,
                operation,
            } => self
                .operations
                .get(&(connection.clone(), operation.clone()))
                .copied(),
            // A store's `put` is keyed as any step is (area 16).
            Callee::Files { op, .. } => Some(match op {
                FilesOp::Put => (Call::Safe, false),
                FilesOp::Read => (Call::Reads, false),
            }),
            _ => None,
        }
    }
}

/// What one action or fixture's outside calls come to, itself and through
/// every action it calls, each call named as written where it is made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Steps {
    /// The most changing calls one way through it makes.
    pub longest: u32,
    /// How many changing calls it can make at all, every way through it
    /// counted.
    pub changing: u32,
    /// Its first outside call of any kind.
    pub calls: Option<String>,
    /// Its first changing call.
    pub first: Option<String>,
    /// Its first changing call with no key.
    pub unkeyed: Option<String>,
    /// Its first call of an operation marked `large_output: true`.
    pub large: Option<String>,
}

impl Steps {
    /// Whether one way through it makes several changing calls, which
    /// are recorded and replayed (area 12).
    pub fn several(&self) -> bool {
        self.longest >= 2
    }
}

/// One way through a body: whether it is still running, and how many
/// changing calls it has made.
#[derive(Clone, Copy, Debug, Default)]
struct Way {
    live: bool,
    count: u32,
}

impl Way {
    fn start() -> Way {
        Way {
            live: true,
            count: 0,
        }
    }

    /// Where two ways meet again: the one with more calls is the longer
    /// from here on.
    fn join(self, other: Way) -> Way {
        match (self.live, other.live) {
            (false, _) => other,
            (_, false) => self,
            _ => Way {
                live: true,
                count: self.count.max(other.count),
            },
        }
    }
}

/// A site that makes a changing call, itself or through an action.
#[derive(Clone, Debug)]
struct Made {
    site: SiteId,
    /// The action called, with what it makes; `None` for the site's own
    /// call.
    through: Option<Steps>,
}

/// A body walked in the order it runs.
struct Walk<'a> {
    body: &'a Body,
    calls: &'a Calls,
    /// The folder of the package whose action this is, if one.
    package: Option<String>,
    /// What an action it calls makes.
    steps: &'a dyn Fn(&Key) -> Steps,
    /// How many `for` bodies the walk is inside.
    loops: u32,
    longest: u32,
    changing: u32,
    /// The first outside call, of any kind.
    first_call: Option<Made>,
    /// Each changing call, in the order walked.
    made: Vec<Made>,
    /// Each changing call with no key: its own, or an action's that makes
    /// it as its only one.
    unkeyed: Vec<Made>,
    /// Each operation with a large answer: its own, or through an action
    /// that makes no more than one changing call.
    large: Vec<Made>,
    /// Each outside call inside a loop.
    in_loops: Vec<Made>,
    /// Where a way first passes [`MOST_STEPS`].
    past: Option<Made>,
    /// An action called passes it on its own, and is reported there.
    called_past: bool,
}

impl<'a> Walk<'a> {
    fn new(body: &'a Body, calls: &'a Calls, steps: &'a dyn Fn(&Key) -> Steps) -> Walk<'a> {
        Walk {
            body,
            calls,
            package: None,
            steps,
            loops: 0,
            longest: 0,
            changing: 0,
            first_call: None,
            made: Vec::new(),
            unkeyed: Vec::new(),
            large: Vec::new(),
            in_loops: Vec::new(),
            past: None,
            called_past: false,
        }
    }

    fn run(&mut self) {
        let block = &self.body.block;
        let way = self.block(block, Way::start());
        self.end(way);
    }

    /// A way that ends here.
    fn end(&mut self, way: Way) {
        self.longest = self.longest.max(way.count);
    }

    fn block(&mut self, block: &[SiteId], mut way: Way) -> Way {
        for &id in block {
            if !way.live {
                break;
            }
            way = self.site(id, way);
        }
        way
    }

    fn all(&mut self, sites: impl IntoIterator<Item = SiteId>, mut way: Way) -> Way {
        for id in sites {
            way = self.site(id, way);
        }
        way
    }

    fn site(&mut self, id: SiteId, way: Way) -> Way {
        if !way.live {
            return way;
        }
        let body = self.body;
        let node = &body.site(id).node;
        match node {
            Node::Stmt(Stmt::Return { value }) => {
                let way = self.all(*value, way);
                self.end(way);
                Way::default()
            }
            Node::Stmt(Stmt::If {
                arms, otherwise, ..
            }) => {
                let mut way = way;
                let mut out = Way::default();
                for arm in arms {
                    way = self.all(arm.condition, way);
                    out = out.join(self.block(&arm.then, way));
                }
                out.join(self.block(otherwise, way))
            }
            // A loop's body counts once: a call in it is refused anyway.
            Node::Stmt(Stmt::For { list, body, .. }) => {
                let entry = self.all(*list, way);
                self.loops += 1;
                let once = self.block(body, entry);
                self.loops -= 1;
                entry.join(once)
            }
            Node::Expr(Expr::Match { subject, arms, .. }) if !arms.is_empty() => {
                let way = self.all(*subject, way);
                let mut out = Way::default();
                for arm in arms {
                    out = out.join(match &arm.body {
                        ArmBody::Value(site) => self.site(*site, way),
                        ArmBody::Block(block) => self.block(block, way),
                        ArmBody::Empty => way,
                    });
                }
                out
            }
            Node::Expr(Expr::Reject { .. } | Expr::Propagate { .. }) => {
                let way = self.all(children(node), way);
                self.end(way);
                Way::default()
            }
            _ => {
                let mut way = self.all(children(node), way);
                match does(node) {
                    Does::Calls => self.call(id, node, &mut way),
                    Does::Joins(callee) => self.joins(id, callee, &mut way),
                    Does::Writes | Does::Nothing => {}
                }
                way
            }
        }
    }

    /// The site's own outside call.
    fn call(&mut self, site: SiteId, node: &Node, way: &mut Way) {
        let Some((call, large)) = self.calls.of(node, self.package.as_deref()) else {
            return;
        };
        let made = Made {
            site,
            through: None,
        };
        if self.first_call.is_none() {
            self.first_call = Some(made.clone());
        }
        if self.loops > 0 {
            self.in_loops.push(made.clone());
        }
        if large {
            self.large.push(made.clone());
        }
        if call == Call::Reads {
            return;
        }
        if call == Call::Unkeyed {
            self.unkeyed.push(made.clone());
        }
        self.changing = self.changing.saturating_add(1);
        way.count = way.count.saturating_add(1);
        self.made.push(made.clone());
        if way.count > MOST_STEPS && self.past.is_none() {
            self.past = Some(made);
        }
    }

    /// A call of an action, which makes what it makes as part of this one.
    fn joins(&mut self, site: SiteId, callee: &Key, way: &mut Way) {
        let steps = (self.steps)(callee);
        if steps.calls.is_none() {
            return;
        }
        let made = Made {
            site,
            through: Some(steps.clone()),
        };
        if self.first_call.is_none() {
            self.first_call = Some(made.clone());
        }
        if self.loops > 0 {
            self.in_loops.push(made.clone());
        }
        // One the action called makes beside others of its own is
        // reported there.
        if steps.large.is_some() && !steps.several() {
            self.large.push(made.clone());
        }
        if steps.changing == 0 {
            return;
        }
        if steps.unkeyed.is_some() && steps.changing < 2 {
            self.unkeyed.push(made.clone());
        }
        self.changing = self.changing.saturating_add(steps.changing);
        way.count = way.count.saturating_add(steps.longest);
        self.made.push(made.clone());
        if steps.longest > MOST_STEPS {
            self.called_past = true;
        } else if way.count > MOST_STEPS && self.past.is_none() {
            self.past = Some(made);
        }
    }

    /// What the walk found, each call named as `names` writes it.
    fn summary(&self, names: Option<&Names<'_>>) -> Steps {
        let name = |made: &Made, part: fn(&Steps) -> Option<String>| match &made.through {
            Some(steps) => part(steps),
            None => names.map(|names| names.name(self.body, made.site)),
        };
        Steps {
            longest: self.longest,
            changing: self.changing,
            calls: self
                .first_call
                .as_ref()
                .and_then(|made| name(made, |steps| steps.calls.clone())),
            first: self
                .made
                .first()
                .and_then(|made| name(made, |steps| steps.first.clone())),
            unkeyed: self
                .unkeyed
                .first()
                .and_then(|made| name(made, |steps| steps.unkeyed.clone())),
            large: self
                .large
                .first()
                .and_then(|made| name(made, |steps| steps.large.clone())),
        }
    }
}

/// The folder of the package that owns `file`, if one.
fn package_of(db: &dyn Db, project: Project, file: File) -> Option<String> {
    jadpo_db::location(db, project, file)
        .ownership
        .package()
        .map(|home| home.folder.clone())
}

/// Every action and fixture's body, by key, with its file.
type AllBodies = BTreeMap<Key, (File, Arc<Body>)>;

/// What each action works out to, worked out once each, an action it
/// calls first.
struct Summaries<'a> {
    db: &'a dyn Db,
    project: Project,
    bodies: AllBodies,
    calls: Calls,
    done: RefCell<BTreeMap<Key, Steps>>,
    /// The actions being worked out: one met again is a cycle, reported
    /// as `effect.recursive_action`, and counts as nothing.
    visiting: RefCell<BTreeSet<Key>>,
}

impl Summaries<'_> {
    fn of(&self, key: &Key) -> Steps {
        if let Some(found) = self.done.borrow().get(key) {
            return found.clone();
        }
        let Some((file, body)) = self.bodies.get(key) else {
            return Steps::default();
        };
        if !self.visiting.borrow_mut().insert(key.clone()) {
            return Steps::default();
        }
        let names = Names::of(self.db, *file);
        let steps = {
            let callee = |callee: &Key| self.of(callee);
            let mut walk = Walk::new(body, &self.calls, &callee);
            walk.package = package_of(self.db, self.project, *file);
            walk.run();
            walk.summary(names.as_ref())
        };
        self.visiting.borrow_mut().remove(key);
        self.done.borrow_mut().insert(key.clone(), steps.clone());
        steps
    }
}

/// What each action and fixture's outside calls come to, by key.
#[salsa::tracked(returns(ref))]
pub fn action_steps(db: &dyn Db, project: Project) -> BTreeMap<Key, Steps> {
    let mut all = AllBodies::new();
    for &file in project.files(db) {
        for (key, body) in bodies(db, project, file) {
            all.entry(key).or_insert((file, body));
        }
    }
    let keys: Vec<Key> = all.keys().cloned().collect();
    let summaries = Summaries {
        db,
        project,
        bodies: all,
        calls: Calls::new(db, project),
        done: RefCell::new(BTreeMap::new()),
        visiting: RefCell::new(BTreeSet::new()),
    };
    for key in &keys {
        summaries.of(key);
    }
    summaries.done.into_inner()
}

/// Whether the route `api` names keeps its key across a retry: its API
/// requires `Idempotency-Key` (area 7).
fn requires_key(db: &dyn Db, project: Project, api: &str) -> bool {
    crate::routes::api_table(db, project)
        .get(api)
        .is_some_and(|api| api.idempotency == jadpo_std::Idempotency::Required)
}

/// The actions run where a record of several changing calls can be
/// resumed, so each needs a store to keep it in: each subscriber's, and
/// each route's whose API requires `Idempotency-Key`.
#[salsa::tracked(returns(ref))]
pub fn resumable_runs(db: &dyn Db, project: Project) -> BTreeSet<Key> {
    let mut out: BTreeSet<Key> = crate::events::subscribers(db, project)
        .iter()
        .map(|subscriber| subscriber.run.clone())
        .collect();
    let lookup = Lookup::new(db, project);
    for (group, route) in crate::routes::groups(db, project).routes() {
        if route.run.query || route.run.generated.is_some() {
            continue;
        }
        if requires_key(db, project, &group.api)
            && let Some(key) = crate::graph::declaration_key(&lookup, &route.run.name)
        {
            out.insert(key);
        }
    }
    out
}

/// The problems in `file`: each action's and fixture's own, and those of
/// the routes, schedules and runs it declares.
#[salsa::tracked(returns(ref))]
pub fn step_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let all = action_steps(db, project);
    let own = bodies(db, project, file);
    if !own.is_empty()
        && let Some(names) = Names::of(db, file)
    {
        let calls = Calls::new(db, project);
        for (key, body) in &own {
            let package = package_of(db, project, file);
            body_problems(
                db,
                project,
                (&names, package),
                &calls,
                all,
                key,
                body,
                &mut out,
            );
        }
    }
    route_problems(db, project, file, all, &mut out);
    schedule_problems(db, project, file, all, &mut out);
    out
}

/// The problems of one action or fixture `key`'s body.
#[allow(clippy::too_many_arguments)]
fn body_problems(
    db: &dyn Db,
    project: Project,
    (names, package): (&Names<'_>, Option<String>),
    calls: &Calls,
    all: &BTreeMap<Key, Steps>,
    key: &Key,
    body: &Body,
    out: &mut Vec<Diagnostic>,
) {
    let callee = |callee: &Key| all.get(callee).cloned().unwrap_or_default();
    let mut walk = Walk::new(body, calls, &callee);
    walk.package = package;
    walk.run();
    // A package's action is named as the package writes it.
    let action = key.name().rsplit('.').next().unwrap_or_default();
    let steps = all.get(key).cloned().unwrap_or_default();
    // A call through an action, named as it is made there.
    let made_name = |made: &Made, part: fn(&Steps) -> Option<String>| match &made.through {
        Some(through) => part(through).unwrap_or_default(),
        None => names.name(body, made.site),
    };
    for made in &walk.in_loops {
        let written = names.name(body, made.site);
        let message = match &made.through {
            None => format!("`{written}` is called once for every item of the loop"),
            Some(through) => format!(
                "`{written}` calls `{}` once for every item of the loop",
                through.calls.clone().unwrap_or_default()
            ),
        };
        out.push(
            Diagnostic::new(&rule::EFFECT_CALL_IN_LOOP, message)
                .at(names.span(body, made.site), "inside `for`")
                .reason("An action's outside calls lie along a way through it whose length is known before it runs, so a retry can replay them in order; a loop makes one for each item. A provider that takes a list is called once with the list.")
                .next("Make the calls one item at a time with `batches.each`"),
        );
    }
    if let Some(made) = walk.past.as_ref().filter(|_| !walk.called_past) {
        let label = match &made.through {
            None => format!("the {}th changing call", MOST_STEPS + 1),
            Some(_) => format!("makes the {}th changing call", MOST_STEPS + 1),
        };
        out.push(
            Diagnostic::new(
                &rule::STEP_TOO_MANY,
                format!(
                    "`{action}` makes {} changing calls in a row, more than {MOST_STEPS}",
                    walk.longest
                ),
            )
            .at(names.span(body, made.site), label)
            .reason(format!(
                "Each changing call is a step its action records before sending and replays on a retry, and an action makes at most {MOST_STEPS} on any one way through it, counting those of the actions it calls."
            ))
            .next(format!(
                "Finish part of the work in a subscriber of an event `{action}` emits"
            )),
        );
    }
    if steps.changing >= 2
        && let Some(made) = walk.unkeyed.first()
    {
        let unkeyed = made_name(made, |steps| steps.unkeyed.clone());
        let other = walk
            .made
            .iter()
            .find(|other| other.site != made.site)
            .map(|other| made_name(other, |steps| steps.first.clone()))
            .unwrap_or_default();
        let message = match &made.through {
            None => format!(
                "`{unkeyed}` takes no idempotency key, but `{action}` makes another changing call, `{other}`"
            ),
            Some(_) => format!(
                "`{}` calls `{unkeyed}`, which takes no idempotency key, but `{action}` makes another changing call, `{other}`",
                names.name(body, made.site)
            ),
        };
        out.push(
            Diagnostic::new(&rule::STEP_UNKEYED, message)
                .at(names.span(body, made.site), "no key")
                .reason("A changing call with no key may act twice if it is sent again, and once its answer is lost only an operator can say whether it went through, so it stays the only changing call in its action.")
                .next(format!(
                    "Move `{unkeyed}` to a subscriber of an event `{action}` emits"
                )),
        );
    }
    if steps.several()
        && let Some(made) = walk.large.first()
    {
        let large = made_name(made, |steps| steps.large.clone());
        let message = match &made.through {
            None => format!(
                "`{large}` gives a large answer, and `{action}` makes several changing calls"
            ),
            Some(_) => format!(
                "`{}` calls `{large}`, which gives a large answer, and `{action}` makes several changing calls",
                names.name(body, made.site)
            ),
        };
        out.push(
            Diagnostic::new(&rule::STEP_LARGE_OUTPUT, message)
                .at(names.span(body, made.site), "`large_output: true`")
                .reason("In an action with several changing calls, each answer is kept, up to 1 MB, for a retry to replay, and its package marks this operation's answer as large.")
                .next(format!(
                    "Move `{large}` to an action of its own, run by a subscriber of an event `{action}` emits"
                )),
        );
    }
    if steps.several()
        && key.kind() == Some(Kind::Action)
        && resumable_runs(db, project).contains(key)
        && crate::types::written_stores(db, project)
            .get(action)
            .is_none_or(|stores| stores.is_empty())
        && crate::events::events_settings(db, project).store.is_none()
        // One that emits is told the same fix by `event.store_required`.
        && !crate::events::emitting(db, project).contains_key(action)
        && let Some(made) = walk.made.first()
    {
        let store = crate::data::connections(db, project)
            .databases()
            .first()
            .cloned()
            .unwrap_or_else(|| "main".into());
        out.push(
            Diagnostic::new(
                &rule::STEP_STORE_REQUIRED,
                format!(
                    "`{action}` makes {} changing calls but writes no store to keep their record in",
                    steps.longest
                ),
            )
            .at(names.span(body, made.site), "no store for the record")
            .reason(format!(
                "Each changing call is recorded before it goes out, beside the outbox of the store its action writes, so a retry can replay it; `{action}` writes no store, itself or through what it calls, and `app.jadpo` names none with `events: {{ store: }}`."
            ))
            .next(format!(
                "Add `events: {{ store: {store} }}` to `application {{ }}` in `app.jadpo`"
            )),
        );
    }
    // The answers of an action with several changing calls are kept
    // encrypted, under the key `application { steps: }` names (decided
    // 2026-10-20): reported once, at the first such action.
    if steps.several()
        && !steps_settings(db, project).written
        && first_several(db, project).as_ref() == Some(key)
        && let Some(made) = walk.made.first()
    {
        out.push(
            Diagnostic::new(
                &rule::STEP_ENCRYPTION_KEY_REQUIRED,
                format!(
                    "`{action}` makes {} changing calls, but `app.jadpo` names no key to encrypt their answers with",
                    steps.longest
                ),
            )
            .at(names.span(body, made.site), "its answers are kept")
            .reason(format!(
                "Each answer of an action with several changing calls is kept, encrypted, until the action commits, so a retry can replay it; `application {{ steps: {{ encryption_key: }} }}` names the `SigningKey` setting it is encrypted with, and `{action}` is the first action that needs it."
            ))
            .next("Add `steps: { encryption_key: config.step_encryption_key }` to `application { }` in `app.jadpo`, and declare `step_encryption_key: SigningKey` in `config.jadpo`"),
        );
    }
    // A run's items and its end, each run as a batch of its own.
    for (fact, site) in facts_of(body) {
        let Fact::Batch(batch) = fact else {
            continue;
        };
        let written = names.name(body, site);
        for (run, what, may) in [
            (&batch.each, "Each item of", "a run's item"),
            (&batch.finished, "The end of", "a run's end"),
        ] {
            let Some(run) = run else {
                continue;
            };
            let Some(steps) = all.get(run).filter(|steps| steps.several()) else {
                continue;
            };
            out.push(
                Diagnostic::new(
                    &rule::STEP_CONTEXT,
                    format!(
                        "{what} `{written}` runs `{}`, which makes {} changing calls, more than {may} may",
                        run.name(),
                        steps.longest
                    ),
                )
                .at(names.span(body, site), "a retry can't resume them")
                .reason("Several changing calls are each recorded under their action's key, so a retry carries on from the last answer instead of sending them again; that record is a subscriber's or a route's, and a run keeps each item to one changing call.")
                .next(format!(
                    "Emit an event from `{}` and make the calls in its subscriber",
                    run.name()
                )),
            );
        }
    }
}

/// `step.context` at each route in `file` whose action makes several
/// changing calls when its API doesn't require `Idempotency-Key`.
fn route_problems(
    db: &dyn Db,
    project: Project,
    file: File,
    all: &BTreeMap<Key, Steps>,
    out: &mut Vec<Diagnostic>,
) {
    let path = file.path(db);
    let lookup = Lookup::new(db, project);
    let mut seen = BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Routes || !seen.insert(item.name.as_str()) {
            continue;
        }
        let Some(read) = crate::routes::group_read(db, project, Name::new(db, item.name.clone()))
        else {
            continue;
        };
        if read.path != *path {
            continue;
        }
        let Some(group) = &read.model else {
            continue;
        };
        if requires_key(db, project, &group.api) {
            continue;
        }
        for route in &group.routes {
            if route.run.query || route.run.generated.is_some() {
                continue;
            }
            let Some(steps) = crate::graph::declaration_key(&lookup, &route.run.name)
                .and_then(|key| all.get(&key))
                .filter(|steps| steps.several())
            else {
                continue;
            };
            let label = route.label();
            let Some((_, span)) = read.spans.iter().find(|(known, _)| *known == label) else {
                continue;
            };
            let api = &group.api;
            out.push(
                Diagnostic::new(
                    &rule::STEP_CONTEXT,
                    format!(
                        "`{label}` runs `{}`, which makes {} changing calls, but API `{api}` doesn't require `Idempotency-Key`",
                        route.run.name, steps.longest
                    ),
                )
                .at(span.clone(), "a retry can't resume them")
                .reason("Several changing calls are each recorded under their action's key, so a retry carries on from the last answer instead of sending them again; a route keeps its key across a retry only when its API requires the client's `Idempotency-Key`.")
                .next(format!(
                    "Require the key with `idempotency: Idempotency.required` on API `{api}`, or answer `202` and make the calls in a subscriber"
                )),
            );
        }
    }
}

/// `step.context` at each schedule in `file` whose action makes several
/// changing calls.
fn schedule_problems(
    db: &dyn Db,
    project: Project,
    file: File,
    all: &BTreeMap<Key, Steps>,
    out: &mut Vec<Diagnostic>,
) {
    for read in crate::schedules::schedules_in(db, project, file) {
        let Some(model) = &read.model else {
            continue;
        };
        let Some(action) = model.run.action() else {
            continue;
        };
        let Some(steps) = all.get(action).filter(|steps| steps.several()) else {
            continue;
        };
        out.push(
            Diagnostic::new(
                &rule::STEP_CONTEXT,
                format!(
                    "`{}` runs `{}`, which makes {} changing calls, but a schedule's retry is its next run",
                    read.name,
                    action.name(),
                    steps.longest
                ),
            )
            .at(read.span.clone(), "a retry can't resume them")
            .reason("Several changing calls are each recorded under their action's key, so a retry carries on from the last answer instead of sending them again; a schedule's retry is its next run, under a key of its own.")
            .next("Emit an event for each piece of work and make the calls in its subscriber"),
        );
    }
}

// --- the record (chunk 7.5b) ----------------------------------------------------

/// `application { steps: { encryption_key: config.<name> } }`, read (area
/// 12, "Several changing calls in one action"; decided 2026-10-20): the
/// `SigningKey` setting a record's answers are encrypted with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepsRead {
    /// The setting `encryption_key:` reads.
    pub encryption_key: Option<String>,
    /// Whether `app.jadpo` writes a `steps:` block at all: one that doesn't
    /// read is reported as itself, not as a missing key.
    pub written: bool,
    pub problems: Vec<Diagnostic>,
}

/// The project's `steps:` settings, with `step.settings_invalid` for each
/// member that doesn't read as one.
#[salsa::tracked(returns(ref))]
pub fn steps_settings(db: &dyn Db, project: Project) -> Arc<StepsRead> {
    use jadpo_syntax::ast::{AstNode, Decl, Expr};

    use crate::tree::{self, dotted, node_span};
    use crate::types::TypeView;
    let mut out = StepsRead::default();
    let Some(file) = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE)
    else {
        return Arc::new(out);
    };
    let Some(source) = tree::source(db, *file) else {
        return Arc::new(out);
    };
    let path = file.path(db);
    let declare = "Declare `step_encryption_key: SigningKey` in `config.jadpo`, and write `encryption_key: config.step_encryption_key`";
    for decl in source.decls() {
        let Decl::SingletonDecl(singleton) = decl else {
            continue;
        };
        if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
            continue;
        }
        for (key, member) in tree::members(singleton.member_list()) {
            if tree::name_text(&key).as_deref() != Some("steps") {
                continue;
            }
            out.written = true;
            let Some(block) = member.block() else {
                out.problems.push(
                    Diagnostic::new(&rule::STEP_SETTINGS_INVALID, "`steps:` holds no block")
                        .at(node_span(path, member.syntax()), "not a block")
                        .reason("`steps: { encryption_key: config.<name> }` names the `SigningKey` setting the answers of an action with several changing calls are encrypted with while they are kept.")
                        .next(declare),
                );
                continue;
            };
            for (setting, entry) in tree::members(Some(block)) {
                let name = tree::name_text(&setting).unwrap_or_default();
                if name != "encryption_key" {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::STEP_SETTINGS_INVALID,
                            format!("`{name}` isn't a steps setting"),
                        )
                        .at(node_span(path, setting.syntax()), "not a setting")
                        .reason("`steps:` holds one setting, `encryption_key:`, the `SigningKey` setting the answers of an action with several changing calls are encrypted with.")
                        .next(format!("Remove `{name}:`")),
                    );
                    continue;
                }
                let value = entry.value();
                let span = value.as_ref().map_or_else(
                    || node_span(path, entry.syntax()),
                    |value| node_span(path, value.syntax()),
                );
                let read = value
                    .as_ref()
                    .and_then(dotted)
                    .filter(|(first, rest)| {
                        tree::name_ref_text(first).as_deref() == Some("config") && rest.len() == 1
                    })
                    .and_then(|(_, rest)| rest.first().and_then(tree::name_ref_text));
                let Some(read) = read else {
                    let literal = matches!(value, Some(Expr::Literal(_)));
                    let (message, label) = if literal {
                        (
                            "An encryption key is written in the source",
                            "a secret in source",
                        )
                    } else {
                        (
                            "`encryption_key:` doesn't read a setting",
                            "not `config.<name>`",
                        )
                    };
                    out.problems.push(
                        Diagnostic::new(&rule::STEP_SETTINGS_INVALID, message)
                            .at(span, label)
                            .reason("The key a record's answers are encrypted with comes only from secret configuration (area 10): a `SigningKey` setting, read as `config.<name>`.")
                            .next(declare),
                    );
                    continue;
                };
                let config = crate::config::config(db, project);
                if let Some(found) = config.setting(&read) {
                    let lookup = Lookup::new(db, project);
                    if lookup.base_prim(&found.ty) != Some(jadpo_std::Builtin::SigningKey) {
                        out.problems.push(
                            Diagnostic::new(
                                &rule::STEP_SETTINGS_INVALID,
                                format!("`encryption_key:` reads `{read}`, which isn't a `SigningKey`"),
                            )
                            .at(span, format!("`{read}` is `{}`", found.ty))
                            .reason("A record's answers are encrypted with a `SigningKey` setting: secret, at least 32 characters, and replaced by moving the old value to the variable with `_PREVIOUS` after its name (area 10), as grants' `encryption_key:` is.")
                            .next(format!("Declare `{read}: SigningKey` in `config.jadpo`")),
                        );
                        continue;
                    }
                }
                out.encryption_key = Some(read);
            }
        }
    }
    Arc::new(out)
}

/// The first action, in key order, whose longest way makes several
/// changing calls: where `step.encryption_key_required` is reported when
/// `app.jadpo` names no key.
#[salsa::tracked(returns(ref))]
pub fn first_several(db: &dyn Db, project: Project) -> Option<Key> {
    action_steps(db, project)
        .iter()
        .find(|(key, steps)| key.kind() == Some(Kind::Action) && steps.several())
        .map(|(key, _)| key.clone())
}

/// The store the record of action `action` is kept in: the one it
/// writes, else the one `events: { store: }` names, as its handled record
/// and its outbox are (area 12, "Outbox placement, settled").
pub fn record_store(db: &dyn Db, project: Project, action: &str) -> Option<String> {
    crate::types::written_stores(db, project)
        .get(action)
        .and_then(|stores| stores.keys().next().cloned())
        .or_else(|| crate::events::events_settings(db, project).store.clone())
}

/// What the runtime keeps for each action that makes several changing
/// calls and is run where its key survives a retry (chunk 7.5b), by key:
/// the store its record is kept in and a digest of its step shape.
#[salsa::tracked(returns(ref))]
pub fn record_plans(db: &dyn Db, project: Project) -> BTreeMap<Key, jadpo_std::steps::RecordPlan> {
    let all = action_steps(db, project);
    let mut bodies_by_key = AllBodies::new();
    for &file in project.files(db) {
        for (key, body) in bodies(db, project, file) {
            bodies_by_key.entry(key).or_insert((file, body));
        }
    }
    let calls = Calls::new(db, project);
    let mut out = BTreeMap::new();
    for key in resumable_runs(db, project) {
        if !all.get(key).is_some_and(Steps::several) {
            continue;
        }
        let Some(store) = record_store(db, project, key.name()) else {
            continue;
        };
        let mut text = String::new();
        let mut seen = BTreeSet::new();
        shape(
            db,
            project,
            &bodies_by_key,
            &calls,
            key,
            &mut seen,
            &mut text,
        );
        let digest = blake3::hash(text.as_bytes()).to_hex().to_string();
        out.insert(
            key.clone(),
            jadpo_std::steps::RecordPlan {
                store,
                shape: digest,
            },
        );
    }
    out
}

/// Writes the step shape of action `key` to `out`: each outside call by
/// its operation (`pay.payments.create`, `sheets.put`) in the order its
/// body makes them, each way through an `if` or a `match` in brackets,
/// a `for` body in braces, and an action it calls in place.
fn shape(
    db: &dyn Db,
    project: Project,
    bodies: &AllBodies,
    calls: &Calls,
    key: &Key,
    seen: &mut BTreeSet<Key>,
    out: &mut String,
) {
    let Some((file, body)) = bodies.get(key) else {
        return;
    };
    if !seen.insert(key.clone()) {
        return;
    }
    let names = Names::of(db, *file);
    let package = package_of(db, project, *file);
    let mut walker = Shape {
        db,
        project,
        package: package.as_deref(),
        bodies,
        calls,
        body,
        names: names.as_ref(),
        seen,
        out,
    };
    walker.block(&body.block);
    walker.seen.remove(key);
}

struct Shape<'a, 'o> {
    db: &'a dyn Db,
    project: Project,
    package: Option<&'a str>,
    bodies: &'a AllBodies,
    calls: &'a Calls,
    body: &'a Body,
    names: Option<&'a Names<'a>>,
    seen: &'o mut BTreeSet<Key>,
    out: &'o mut String,
}

impl Shape<'_, '_> {
    fn block(&mut self, block: &[SiteId]) {
        for &id in block {
            self.site(id);
        }
    }

    fn site(&mut self, id: SiteId) {
        let body = self.body;
        let node = &body.site(id).node;
        match node {
            Node::Stmt(Stmt::If {
                arms, otherwise, ..
            }) => {
                self.out.push_str("if(");
                for arm in arms {
                    if let Some(site) = arm.condition {
                        self.site(site);
                    }
                    self.out.push('[');
                    self.block(&arm.then);
                    self.out.push(']');
                }
                self.out.push('[');
                self.block(otherwise);
                self.out.push_str("])");
            }
            Node::Stmt(Stmt::For { list, body, .. }) => {
                if let Some(site) = *list {
                    self.site(site);
                }
                self.out.push('{');
                self.block(body);
                self.out.push('}');
            }
            Node::Expr(Expr::Match { subject, arms, .. }) if !arms.is_empty() => {
                if let Some(site) = *subject {
                    self.site(site);
                }
                self.out.push_str("match(");
                for arm in arms {
                    self.out.push('[');
                    match &arm.body {
                        ArmBody::Value(site) => self.site(*site),
                        ArmBody::Block(block) => self.block(block),
                        ArmBody::Empty => {}
                    }
                    self.out.push(']');
                }
                self.out.push(')');
            }
            _ => {
                for site in children(node) {
                    self.site(site);
                }
                match does(node) {
                    Does::Calls => {
                        if self.calls.of(node, self.package).is_some()
                            && let Some(names) = self.names
                        {
                            self.out.push_str(&names.name(body, id));
                            self.out.push(';');
                        }
                    }
                    Does::Joins(callee) => {
                        let callee = callee.clone();
                        shape(
                            self.db,
                            self.project,
                            self.bodies,
                            self.calls,
                            &callee,
                            self.seen,
                            self.out,
                        );
                    }
                    Does::Writes | Does::Nothing => {}
                }
            }
        }
    }
}
