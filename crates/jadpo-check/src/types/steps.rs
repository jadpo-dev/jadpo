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
//! - **`step.late_failure`** (chunk 7.5e, declinable): a point that can
//!   end an action for good after a changing call took effect on its way:
//!   an `attempt` whose call or write can fail with a failure that isn't
//!   retried, a `reject` or a `propagate`. Reported once for each action,
//!   at the first such point, the others beside it. A package's action is
//!   never told: the failures it can end with after its own changing call
//!   are its [`Steps::after_effect`], listed on its surface, and the app's
//!   call of it is the point.
//! - **`route.calls_past_deadline`** (chunk 7.5e): a route whose action's
//!   outside calls, each its `timeout:` times its attempts, add up on the
//!   longest way through it to more than the route's `deadline:`; a
//!   provider's operation and an `ai` call alike, `ai.stream`'s and an
//!   embedding's counted once (area 11, "Retries"; area 21; chunk 7.7e).
//!
//! The runtime's record of such an action is planned here too
//! ([`record_plans`]): the store it is kept in and a digest of its step
//! shape.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::body::{ArmBody, Body, Callee, Expr, Fallible, Node, SiteId, Stmt};
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
    /// A provider change protected by its finite idempotency window.
    Keyed,
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
    /// How long a call through each connection can take, by its name: its
    /// `timeout:` times its `retry: { attempts: }`, in milliseconds.
    budgets: BTreeMap<String, u64>,
    /// The failures that are retried rather than ending a run for good,
    /// by name: those of kind `unavailable` or `rate_limited` (area 12,
    /// "Retries and giving up").
    retried: BTreeSet<String>,
}

impl Calls {
    fn new(db: &dyn Db, project: Project) -> Calls {
        let operations = crate::providers::operations(db, project);
        let keyed: BTreeSet<String> = crate::surface::homes(db, project)
            .into_values()
            .filter(|home| {
                crate::connections::reach(db, project, home)
                    .and_then(|reach| reach.idempotency)
                    .is_some_and(|key| !key.header.is_empty())
            })
            .map(|home| home.folder)
            .collect();
        let call = |operation: &crate::providers::Operation| {
            let call = match (operation.changes, operation.idempotent) {
                (false, _) => Call::Reads,
                (true, true) if keyed.contains(&operation.folder) => Call::Keyed,
                (true, true) => Call::Safe,
                (true, false) => Call::Unkeyed,
            };
            (call, operation.large_output)
        };
        let mut out = BTreeMap::new();
        let mut budgets = BTreeMap::new();
        for connection in &crate::connections::package_connections(db, project).connections {
            let timeout = connection
                .timeout_ms
                .unwrap_or(jadpo_std::provider::DEFAULT_TIMEOUT_MS);
            let attempts = connection.retry.unwrap_or_default().attempts;
            budgets.insert(
                connection.name.clone(),
                timeout.saturating_mul(u64::from(attempts)),
            );
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
        let table = crate::failures::failure_table(db, project);
        let retried = table
            .names()
            .filter(|name| {
                table.get(name).is_some_and(|info| {
                    matches!(info.kind.as_deref(), Some("unavailable" | "rate_limited"))
                })
            })
            .map(str::to_string)
            .collect();
        Calls {
            operations: out,
            own,
            budgets,
            retried,
        }
    }

    /// Whether failure `key` ends a run for good: it isn't retried.
    fn ends(&self, key: &Key) -> bool {
        !(self.retried.contains(key.name()) || self.retried.contains(key.written()))
    }

    /// How long the outside call at `node` can take, in milliseconds, and
    /// how many calls it makes through the connection its package's action
    /// was called through (chunk 7.6), which the caller prices.
    fn time(&self, node: &Node) -> (u64, u32) {
        let Node::Expr(Expr::Call { callee, .. }) = node else {
            return (0, 0);
        };
        match callee {
            Callee::Provider { connection, .. }
                if connection == jadpo_graph::body::OWN_CONNECTION =>
            {
                (0, 1)
            }
            Callee::Provider { connection, .. } => {
                (self.budgets.get(connection).copied().unwrap_or(0), 0)
            }
            Callee::Generate {
                timeout_ms,
                attempts,
                ..
            } => (timeout_ms.saturating_mul(u64::from(*attempts)), 0),
            // A stream and an embedding ask once, held to their
            // `timeout:` (chunk 7.7e).
            Callee::Stream { timeout_ms, .. } | Callee::Embed { timeout_ms, .. } => {
                (*timeout_ms, 0)
            }
            _ => (0, 0),
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
            // A model asked changes nothing (area 21; chunk 5.5v3).
            Callee::Generate { .. } | Callee::Stream { .. } | Callee::Embed { .. } => {
                Some((Call::Reads, false))
            }
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
    /// Whether a changing provider call relies on a finite key window.
    pub keyed: bool,
    /// Its first outside call of any kind.
    pub calls: Option<String>,
    /// Its first changing call.
    pub first: Option<String>,
    /// Its first changing call with no key.
    pub unkeyed: Option<String>,
    /// Its first call of an operation marked `large_output: true`.
    pub large: Option<String>,
    /// The longest its outside calls can take on one way through it, each
    /// its `timeout:` times its attempts, in milliseconds (chunk 7.5e).
    pub slowest_ms: u64,
    /// For a package's action, the most calls one way through it makes
    /// through the connection it was called through, which its caller
    /// prices with that connection's budget.
    pub own_calls: u32,
    /// The failures that can end it for good after one of its changing
    /// calls took effect, sorted: what a package's surface lists for its
    /// action, and what makes the app's call of that action warned (area
    /// 11, "The surface"; chunk 7.5e).
    pub after_effect: Vec<Key>,
}

impl Steps {
    /// Whether one way through it makes several changing calls, which
    /// are recorded and replayed (area 12).
    pub fn several(&self) -> bool {
        self.longest >= 2
    }
}

/// One way through a body: whether it is still running, how many
/// changing calls it has made, the first of them, and how long its outside
/// calls can take.
#[derive(Clone, Copy, Debug, Default)]
struct Way {
    live: bool,
    count: u32,
    /// The first changing call made on it, by its place in
    /// [`Walk::made`]: after it, a failure ends the action for good with
    /// something already done.
    effect: Option<usize>,
    /// How long its outside calls can take, in milliseconds.
    ms: u64,
    /// How many calls it makes through the connection its package's
    /// action was called through.
    own: u32,
}

impl Way {
    fn start() -> Way {
        Way {
            live: true,
            ..Way::default()
        }
    }

    /// Where two ways meet again: the one with more calls is the longer
    /// from here on, and a changing call made on either may have been
    /// made.
    fn join(self, other: Way) -> Way {
        match (self.live, other.live) {
            (false, _) => other,
            (_, false) => self,
            _ => Way {
                live: true,
                count: self.count.max(other.count),
                effect: match (self.effect, other.effect) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                },
                ms: self.ms.max(other.ms),
                own: self.own.max(other.own),
            },
        }
    }
}

/// What a point that can end an action for good is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ending {
    /// An `attempt` of a call or a write, at the call or write.
    Attempt,
    /// An `attempt` of a package's action that can fail for good after
    /// its own changing call, with none made before it.
    Inside,
    Reject,
    Propagate,
}

/// A point that can end an action for good after a changing call took
/// effect (area 12, "Ending for good after a step took effect is
/// warned").
#[derive(Clone, Debug)]
struct Late {
    site: SiteId,
    ending: Ending,
    /// The changing call before it, in [`Walk::made`]; `None` for
    /// [`Ending::Inside`].
    effect: Option<usize>,
    /// The failures it can end the action with, each one that isn't
    /// retried.
    failures: Vec<Key>,
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
    keyed: bool,
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
    /// The longest any way's outside calls can take, and the most calls
    /// any makes through its package action's connection.
    slowest_ms: u64,
    own_calls: u32,
    /// Each point that can end it for good after a changing call, in the
    /// order walked.
    late: Vec<Late>,
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
            keyed: false,
            first_call: None,
            made: Vec::new(),
            unkeyed: Vec::new(),
            large: Vec::new(),
            in_loops: Vec::new(),
            past: None,
            called_past: false,
            slowest_ms: 0,
            own_calls: 0,
            late: Vec::new(),
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
        self.slowest_ms = self.slowest_ms.max(way.ms);
        self.own_calls = self.own_calls.max(way.own);
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
            Node::Expr(Expr::Reject { failure, .. } | Expr::Propagate { failure }) => {
                let way = self.all(children(node), way);
                let ending = if matches!(node, Node::Expr(Expr::Reject { .. })) {
                    Ending::Reject
                } else {
                    Ending::Propagate
                };
                if let Some(effect) = way.effect {
                    self.late_at(id, ending, Some(effect), failure.iter().cloned().collect());
                }
                self.end(way);
                Way::default()
            }
            _ => {
                let before = way.effect;
                let mut way = self.all(children(node), way);
                match does(node) {
                    Does::Calls => self.call(id, node, &mut way),
                    Does::Joins(callee) => self.joins(id, node, callee, &mut way),
                    Does::Writes | Does::Nothing => {}
                }
                if let Node::Expr(Expr::Attempt {
                    inner: Some(inner),
                    arms,
                }) = node
                {
                    let failures: Vec<Key> = arms.iter().map(|arm| arm.failure.clone()).collect();
                    self.attempt(*inner, before, failures);
                }
                way
            }
        }
    }

    /// The site's own outside call.
    fn call(&mut self, site: SiteId, node: &Node, way: &mut Way) {
        let (ms, own) = self.calls.time(node);
        way.ms = way.ms.saturating_add(ms);
        way.own = way.own.saturating_add(own);
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
        self.keyed |= call == Call::Keyed;
        if call == Call::Unkeyed {
            self.unkeyed.push(made.clone());
        }
        self.changing = self.changing.saturating_add(1);
        way.count = way.count.saturating_add(1);
        way.effect.get_or_insert(self.made.len());
        self.made.push(made.clone());
        if way.count > MOST_STEPS && self.past.is_none() {
            self.past = Some(made);
        }
    }

    /// A call of an action, which makes what it makes as part of this one.
    fn joins(&mut self, site: SiteId, node: &Node, callee: &Key, way: &mut Way) {
        let steps = (self.steps)(callee);
        self.keyed |= steps.keyed;
        // A package's action's own calls go through the connection the
        // app called it through.
        let budget = match node {
            Node::Expr(Expr::Call {
                callee: Callee::PackageAction { connection, .. },
                ..
            }) => Some(self.calls.budgets.get(connection).copied().unwrap_or(0)),
            _ => None,
        };
        match budget {
            Some(budget) => {
                way.ms = way
                    .ms
                    .saturating_add(steps.slowest_ms)
                    .saturating_add(budget.saturating_mul(u64::from(steps.own_calls)));
            }
            None => {
                way.ms = way.ms.saturating_add(steps.slowest_ms);
                way.own = way.own.saturating_add(steps.own_calls);
            }
        }
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
        way.effect.get_or_insert(self.made.len());
        self.made.push(made.clone());
        if steps.longest > MOST_STEPS {
            self.called_past = true;
        } else if way.count > MOST_STEPS && self.past.is_none() {
            self.past = Some(made);
        }
    }

    /// An `attempt` of the call or write at `inner`, with `mapped` the
    /// failures its arms map to, a provider's or a construction's;
    /// `effect` is the first changing call made before it on its way.
    /// After one, every failure it passes on that isn't retried ends the
    /// action for good. Before one, only a package's action can: with the
    /// failures it lists as following its own changing call. An app's
    /// action called is told of its own.
    fn attempt(&mut self, inner: SiteId, effect: Option<usize>, mapped: Vec<Key>) {
        let node = &self.body.site(inner).node;
        let mut ending = Ending::Attempt;
        let mut failures = mapped;
        // What it passes on unmapped: an action's or a write's own.
        if failures.is_empty()
            && let Node::Expr(
                Expr::Call {
                    fallible: Fallible::Fails(keys),
                    ..
                }
                | Expr::Write {
                    fallible: Fallible::Fails(keys),
                    ..
                },
            ) = node
        {
            failures.extend(keys.iter().cloned());
        }
        if effect.is_none() {
            let Node::Expr(Expr::Call {
                callee: Callee::PackageAction { action, .. },
                ..
            }) = node
            else {
                return;
            };
            failures = (self.steps)(action).after_effect;
            ending = Ending::Inside;
        }
        self.late_at(inner, ending, effect, failures);
    }

    /// Keeps a point that can end the action with `failures`, those that
    /// aren't retried, if any.
    fn late_at(&mut self, site: SiteId, ending: Ending, effect: Option<usize>, failures: Vec<Key>) {
        let mut failures: Vec<Key> = failures
            .into_iter()
            .filter(|failure| self.calls.ends(failure))
            .collect();
        failures.sort();
        failures.dedup();
        if failures.is_empty() {
            return;
        }
        self.late.push(Late {
            site,
            ending,
            effect,
            failures,
        });
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
            keyed: self.keyed,
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
            slowest_ms: self.slowest_ms,
            own_calls: self.own_calls,
            after_effect: {
                let mut all: Vec<Key> = self
                    .late
                    .iter()
                    .flat_map(|late| late.failures.iter().cloned())
                    .collect();
                all.sort();
                all.dedup();
                all
            },
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
    // Ending for good after a changing call took effect (chunk 7.5e):
    // once for each action, at its first such point. A package's action
    // lists its own on its surface instead, and the app's call of it is
    // told.
    if walk.package.is_none()
        && key.kind() == Some(Kind::Action)
        && let Some(first) = walk.late.first()
    {
        out.push(late_failure(names, body, action, &walk, first));
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

/// Where a point that can end an action is written, and how it reads:
/// the call or write an `attempt` makes, or `reject X` or `propagate` up to
/// its block.
fn point(names: &Names<'_>, body: &Body, late: &Late) -> (jadpo_diag::Span, String) {
    let span = names.span(body, late.site);
    let written = names.name(body, late.site);
    match late.ending {
        Ending::Attempt | Ending::Inside => (span, written),
        Ending::Reject | Ending::Propagate => {
            let end = span
                .start
                .saturating_add(u32::try_from(written.len()).unwrap_or(0));
            (
                jadpo_diag::Span::new(span.path.clone(), span.start, end.min(span.end)),
                written,
            )
        }
    }
}

/// `step.late_failure` for action `action`, at its first point `first`.
fn late_failure(
    names: &Names<'_>,
    body: &Body,
    action: &str,
    walk: &Walk<'_>,
    first: &Late,
) -> Diagnostic {
    let (span, written) = point(names, body, first);
    let effect = first.effect.and_then(|at| walk.made.get(at));
    // The call that took effect as written where it is made: an
    // operation, or the action that makes it.
    let effect_name = effect.map(|made| names.name(body, made.site));
    let message = match &effect_name {
        Some(effect) => {
            format!("`{action}` can fail for good at `{written}`, after `{effect}` took effect")
        }
        None => {
            let inside = match &body.site(first.site).node {
                Node::Expr(Expr::Call {
                    callee: Callee::PackageAction { action: called, .. },
                    ..
                }) => (walk.steps)(called).first,
                _ => None,
            }
            .unwrap_or_default();
            format!(
                "`{action}` can fail for good inside `{written}`, after its `{inside}` took effect"
            )
        }
    };
    let failures: Vec<String> = first
        .failures
        .iter()
        .map(|failure| {
            format!(
                "`{}`",
                failure.written().rsplit('.').next().unwrap_or_default()
            )
        })
        .collect();
    let mut diagnostic = Diagnostic::new(&rule::STEP_LATE_FAILURE, message)
        .at(span, format!("can fail with {}", crate::or_list(&failures)));
    if let Some(made) = effect {
        diagnostic = diagnostic.also(names.span(body, made.site), "takes effect first");
    }
    for other in walk.late.iter().skip(1) {
        let (span, _) = point(names, body, other);
        diagnostic = diagnostic.also(span, "can fail for good too");
    }
    let before = effect_name.clone().unwrap_or_else(|| written.clone());
    diagnostic
        .reason(format!(
            "Once a changing call has taken effect, a failure that ends `{action}` for good rolls back its writes but not what `{before}` did, and a retry fails the same way; a failure of kind `unavailable` or `rate_limited` is retried instead, and one handled with `match` doesn't end it."
        ))
        .next(format!(
            "Check the condition before `{before}`, record what happened and finish in a subscriber of an event `{action}` emits, or undo `{before}` as another step in a `match` arm"
        ))
}

/// At each route in `file`: `step.context` when its action makes several
/// changing calls and its API doesn't require `Idempotency-Key`, and
/// `route.calls_past_deadline` when its action's outside calls can take
/// longer than its deadline (chunk 7.5e).
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
        let required = requires_key(db, project, &group.api);
        for route in &group.routes {
            if route.run.query || route.run.generated.is_some() {
                continue;
            }
            let Some(steps) = crate::graph::declaration_key(&lookup, &route.run.name)
                .and_then(|key| all.get(&key))
            else {
                continue;
            };
            let label = route.label();
            let Some((_, span)) = read.spans.iter().find(|(known, _)| *known == label) else {
                continue;
            };
            let api = &group.api;
            if !required && steps.several() {
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
            if steps.slowest_ms > u64::from(route.deadline_ms) {
                out.push(past_deadline(
                    &label,
                    &route.run.name,
                    steps.slowest_ms,
                    route.deadline_ms,
                    span.clone(),
                ));
            }
        }
    }
}

/// A duration as Jadpo writes one: `30s`, `1500ms`.
fn duration(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
    }
}

/// `route.calls_past_deadline` at route `label`, which runs `action`,
/// whose outside calls can take `slowest_ms` against its `deadline_ms`.
fn past_deadline(
    label: &str,
    action: &str,
    slowest_ms: u64,
    deadline_ms: u32,
    span: jadpo_diag::Span,
) -> Diagnostic {
    let most = u64::from(jadpo_std::http::MAX_DEADLINE_MS);
    let needed = slowest_ms.div_ceil(1000) * 1000;
    let lower = "the `timeout:` or the attempts of the calls (a connection's `timeout:` and `retry: { attempts: }`, an `ai` call's `timeout:` and `attempts:`)";
    let next = if needed <= most {
        format!(
            "Give the route `deadline: {}`, lower {lower}, or answer `202` and make the calls in a subscriber",
            duration(needed)
        )
    } else {
        format!(
            "Lower {lower}, or answer `202` and make the calls in a subscriber, since a route's deadline is at most {}",
            duration(most)
        )
    };
    Diagnostic::new(
        &rule::ROUTE_CALLS_PAST_DEADLINE,
        format!(
            "`{label}` runs `{action}`, whose outside calls can take {}, longer than its {} deadline",
            duration(slowest_ms),
            duration(u64::from(deadline_ms))
        ),
    )
    .at(span, format!("deadline {}", duration(u64::from(deadline_ms))))
    .reason(format!(
        "Each outside call can wait its `timeout:` on each of its attempts, and a route's deadline covers every call on the longest way through `{action}`, those of the actions it calls included; at the deadline the route stops, so a call still waiting, or still to be tried again, never finishes."
    ))
    .next(next)
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

/// What the runtime keeps for resumable several-changing-call actions
/// (encrypted replay), or single keyed changes (dispatch metadata), by key:
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
        let Some(steps) = all.get(key).filter(|steps| steps.several() || steps.keyed) else {
            continue;
        };
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
                shape: if steps.several() {
                    digest
                } else {
                    format!("{}{digest}", jadpo_std::steps::DISPATCH_SHAPE_PREFIX)
                },
                replay: steps.several(),
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
