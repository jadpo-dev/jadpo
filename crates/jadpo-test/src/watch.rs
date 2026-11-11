//! Watch mode (area 13, "Running tests"; docs/compiler-runtime.md §2.4):
//! `jadpo test --watch` reruns, on every save, the tests the change
//! reaches, using the compiler's own dependency graph.
//!
//! A [`Watch`] keeps one database for its lifetime. Each burst of saves
//! updates its inputs, so the check that follows recomputes only what the
//! saves touched; the graph it builds is compared with the last run's
//! graph, which the session keeps (shared, not copied), and the tests that
//! reach what changed are the ones that run ([`affected`]). How a change
//! travels depends on the edge ([`follow`]): along most edges the source
//! depends on the target, so a change climbs back to whatever reaches the
//! source; along a few the source changes how its target behaves, so a
//! changed or removed source counts as a change to its target. A save that
//! breaks the build shows its errors and runs nothing; the next save that
//! checks runs everything changed since the last run. A [`Poller`] finds
//! the saves.

mod poll;

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use jadpo_db::{Database, Loaded};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::{Digest, EdgeKind, Graph, Kind, NodeId};

pub use poll::{Poller, merge};

use crate::{Options, Ran, select};

/// How a file changed between two looks at it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EditKind {
    Added,
    Saved,
    Removed,
}

impl EditKind {
    /// The past-tense verb a watch rule uses for one file.
    pub fn verb(self) -> &'static str {
        match self {
            EditKind::Added => "added",
            EditKind::Saved => "saved",
            EditKind::Removed => "removed",
        }
    }

    /// The change-list mark: `+`, `~` or `-`.
    pub fn mark(self) -> &'static str {
        match self {
            EditKind::Added => "+",
            EditKind::Saved => "~",
            EditKind::Removed => "-",
        }
    }
}

/// One file that changed, by its project-relative path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edit {
    pub path: String,
    pub kind: EditKind,
}

impl Edit {
    pub fn new(path: impl Into<String>, kind: EditKind) -> Edit {
        Edit {
            path: path.into(),
            kind,
        }
    }
}

/// What one look at the project found and did.
#[derive(Debug)]
pub struct Event {
    /// The files that changed since the last run, merged; empty for the
    /// first run.
    pub edits: Vec<Edit>,
    /// How many of the tests asked for reach what changed: `None` for the
    /// first run and while the build is broken.
    pub reached: Option<usize>,
    /// What ran, or why nothing did; `None` when no test reaches the
    /// change.
    pub ran: Option<Ran>,
    /// The tests failing now, as far as this session has run them.
    pub failing: usize,
    /// The files the project holds now.
    pub files: usize,
    /// The build had errors before this look and checks now, so the
    /// screen confirms they are gone even when no test reaches the change.
    pub fixed: bool,
}

/// How a change travels along an edge of one kind, when working out the
/// tests it reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follow {
    /// The source depends on the target (a call, a read, a type it uses):
    /// a change to the target reaches whatever reaches the source.
    Backwards,
    /// The source changes how the target behaves (a subscriber what an
    /// event does, a grant who may call an action): a change to the
    /// source, or its removal, is a change to the target.
    Forwards,
    /// No test's result follows it.
    Irrelevant,
}

/// How a change travels along edges of `kind`. Every kind is named, so a
/// kind added to the graph can't be left out of watch mode: this match
/// stops compiling until it is classified.
///
/// An emitted event's subscribers are reached forwards, then backwards: a
/// changed subscriber is a change to the event it subscribes to, which
/// reaches every action that emits it (`emits`, backwards), and so the
/// tests that call them.
pub fn follow(kind: EdgeKind) -> Follow {
    match kind {
        EdgeKind::Calls
        | EdgeKind::Reads
        | EdgeKind::Writes
        | EdgeKind::Emits
        | EdgeKind::Runs
        | EdgeKind::Raises
        | EdgeKind::References
        | EdgeKind::Includes
        | EdgeKind::Uses
        | EdgeKind::Covers => Follow::Backwards,
        EdgeKind::Subscribes
        | EdgeKind::Maps
        | EdgeKind::Grants
        | EdgeKind::Scopes
        | EdgeKind::Invalidates
        | EdgeKind::Provides => Follow::Forwards,
        // A deployable reaches what it ships; no test runs a deployable.
        EdgeKind::Reaches => Follow::Irrelevant,
    }
}

/// The edge kinds a change travels along `way`.
fn kinds(way: Follow) -> impl Iterator<Item = EdgeKind> {
    EdgeKind::ALL
        .iter()
        .copied()
        .filter(move |kind| follow(*kind) == way)
}

/// A watch session: the options it was started with, the project's
/// long-lived database, and the graph of the last run.
#[derive(Debug)]
pub struct Watch {
    target: select::Target,
    options: Options,
    loaded: Loaded,
    /// The graph the last run saw; `None` until a run has happened.
    baseline: Option<Arc<Graph>>,
    /// The edits since the last run, while the build is broken.
    pending: Vec<Edit>,
    /// The keys of the tests failing now.
    failing: BTreeSet<String>,
    /// The keys of the tests the last run that ran any ran, which `enter`
    /// runs again.
    last: BTreeSet<String>,
    /// The last look found errors, so nothing ran.
    refused: bool,
}

/// What a run picks, within what the session's options ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// What changed since the last run reaches; on the first run, what
    /// the options ask for (with `--affected`, what changed since its
    /// base reaches).
    Reached,
    /// Every test the options ask for: `a`.
    All,
    /// The tests failing now: `f`.
    Failing,
    /// The tests the last run ran: `enter` under `--watch`.
    Again,
    /// These tests, by key: a file or a test chosen in the keyboard view.
    Only(BTreeSet<String>),
}

impl Watch {
    /// Starts a session: loads the project and runs the tests `options`
    /// asks for (with `--failed`, last run's failures; later runs are the
    /// tests each change reaches). A command-line mistake or a missing
    /// project comes back as the error, for the command to end with.
    pub fn start(options: Options) -> Result<(Watch, Event), Ran> {
        Watch::start_with(options, Database::default())
    }

    /// [`Watch::start`] on a database of the caller's, such as one that
    /// records what it executes.
    pub fn start_with(options: Options, db: Database) -> Result<(Watch, Event), Ran> {
        if let Some(ran) = validate(&options) {
            return Err(ran);
        }
        let mut watch = Watch::open(options, db)?;
        let event = watch.step(Vec::new(), &Pick::Reached);
        Ok((watch, event))
    }

    /// Loads the project without running anything, for the keyboard view,
    /// which runs what is asked for when it is asked.
    pub fn open(options: Options, db: Database) -> Result<Watch, Ran> {
        if let Some(ran) = validate(&options) {
            return Err(ran);
        }
        let target = select::resolve(&options)?;
        let loaded = jadpo_db::load_into(&target.folder, db);
        Ok(Watch {
            target,
            options,
            loaded,
            baseline: None,
            pending: Vec::new(),
            failing: BTreeSet::new(),
            last: BTreeSet::new(),
            refused: false,
        })
    }

    /// Every test in the project as it checks now, in file and line
    /// order, or the errors that stop it checking. Before any run, what
    /// the session saw here is what the first save is measured from.
    pub fn tests(&mut self) -> Result<Vec<crate::Test>, Ran> {
        // A mutable borrow, as `step` takes: the database isn't shared
        // between threads, only lent to the interpreter's.
        let this = &mut *self;
        let checked = jadpo_interp::on_large_stack(move || {
            let tests = crate::prepare(&this.loaded, &this.options.cwd)?.tests;
            // Before any run, a save is measured from the project as it
            // was listed.
            if this.baseline.is_none() {
                this.baseline = Some(Arc::clone(jadpo_check::graph::graph(
                    &this.loaded.db,
                    this.loaded.project,
                )));
            }
            Ok(tests)
        });
        match checked {
            Ok(result) => result,
            Err(error) => Err(Ran::Stopped(Box::new(start_failed(&error.to_string())))),
        }
    }

    /// The keys of the tests that passed and failed the last time each
    /// ran, as the project's record keeps them.
    pub fn last_results(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        if !self.is_project() {
            return Default::default();
        }
        crate::record::results(&self.loaded.root)
    }

    /// Takes the tests that failed last time, by the project's record, as
    /// failing now, so `f` can run them before the session has: the
    /// keyboard view starts from each test's last result.
    pub fn adopt_last_failures(&mut self) {
        self.failing = self.last_results().0;
    }

    /// The keys of the tests failing now, as far as the session knows.
    pub fn failing(&self) -> &BTreeSet<String> {
        &self.failing
    }

    /// Brings the database up to date with `edits` without running
    /// anything; the edits whose contents changed. The next run that
    /// checks shows them.
    pub fn sync(&mut self, edits: Vec<Edit>) -> Vec<Edit> {
        let changed: Vec<Edit> = edits
            .into_iter()
            .filter(|edit| self.refresh(&edit.path))
            .collect();
        self.pending.extend(changed.iter().cloned());
        self.pending = merge(std::mem::take(&mut self.pending));
        changed
    }

    /// Runs what `pick` picks now. `None` when it picks nothing: no test
    /// is failing for [`Pick::Failing`], or no run has happened for
    /// [`Pick::Again`].
    pub fn run(&mut self, pick: Pick) -> Option<Event> {
        let empty = match &pick {
            Pick::Failing => self.failing.is_empty(),
            Pick::Again => self.last.is_empty(),
            Pick::Only(keys) => keys.is_empty(),
            Pick::Reached | Pick::All => false,
        };
        if empty {
            return None;
        }
        Some(self.step(Vec::new(), &pick))
    }

    /// The folder every path is relative to.
    pub fn root(&self) -> &Path {
        &self.loaded.root
    }

    /// The root as the command line spelt it, when the session runs from
    /// outside it (`jadpo_db::shown_root`), for the paths it prints.
    pub fn shown_root(&self) -> Option<String> {
        jadpo_db::shown_root(&self.options.cwd, &self.loaded.root)
    }

    /// Whether the session watches a project, rather than loose files.
    pub fn is_project(&self) -> bool {
        self.loaded.project.layout(&self.loaded.db) == jadpo_db::Layout::Project
    }

    /// The queries the database executed since the last call, when it
    /// records them.
    pub fn executions(&self) -> Vec<String> {
        self.loaded.db.take_executions()
    }

    /// Brings the database up to date with `edits` and runs the tests they
    /// reach. `None` when the files' contents didn't change (a save of the
    /// same text, or a file the project doesn't hold).
    pub fn apply(&mut self, edits: Vec<Edit>) -> Option<Event> {
        let changed: Vec<Edit> = edits
            .into_iter()
            .filter(|edit| self.refresh(&edit.path))
            .collect();
        if changed.is_empty() {
            return None;
        }
        Some(self.step(changed, &Pick::Reached))
    }

    /// Reads one changed path again; whether anything the check reads
    /// changed.
    fn refresh(&mut self, path: &str) -> bool {
        if !self.loaded.tracks(path) {
            return false;
        }
        let db = &self.loaded.db;
        // A change to the lock can link or unlink a package, which reads
        // its whole folder again from elsewhere.
        let before = (
            self.loaded.file(path).map(|file| file.text(db).clone()),
            self.loaded.project.other_files(db).clone(),
            self.loaded.unreadable.clone(),
            self.loaded.links.clone(),
        );
        self.loaded.refresh(path);
        let db = &self.loaded.db;
        let after = (
            self.loaded.file(path).map(|file| file.text(db).clone()),
            self.loaded.project.other_files(db).clone(),
            self.loaded.unreadable.clone(),
            self.loaded.links.clone(),
        );
        before != after
    }

    /// Checks, works out what the edits reach, and runs it, on the
    /// interpreter's thread.
    fn step(&mut self, edits: Vec<Edit>, pick: &Pick) -> Event {
        let these = merge(edits.clone());
        self.pending.extend(edits);
        self.pending = merge(std::mem::take(&mut self.pending));
        let started = Instant::now();
        let run = jadpo_interp::on_large_stack(|| self.check_and_run(started, pick));
        let (reached, ran, good) = match run {
            Ok(result) => result,
            Err(error) => (
                None,
                Some(Ran::Stopped(Box::new(start_failed(&error.to_string())))),
                false,
            ),
        };
        // A save that checks shows everything changed since the last run,
        // and always the saves that just happened: a file added while the
        // build was broken and removed again merges away from what
        // changed since, but its removal is what this look saw. One that
        // doesn't check shows only itself.
        let edits = if good {
            let mut all = std::mem::take(&mut self.pending);
            for edit in these {
                if !all.iter().any(|seen| seen.path == edit.path) {
                    all.push(edit);
                }
            }
            all.sort();
            all
        } else {
            these
        };
        let fixed = self.refused && good;
        self.refused = !good;
        Event {
            edits,
            reached,
            ran,
            fixed,
            failing: self.failing.len(),
            files: self.loaded.project.files(&self.loaded.db).len()
                + self.loaded.project.other_files(&self.loaded.db).len()
                + self.loaded.unreadable.len(),
        }
    }

    /// The count reached, what ran, and whether the build checked.
    fn check_and_run(
        &mut self,
        started: Instant,
        pick: &Pick,
    ) -> (Option<usize>, Option<Ran>, bool) {
        let checked = match crate::prepare(&self.loaded, &self.options.cwd) {
            Ok(checked) => checked,
            Err(ran) => return (None, Some(ran), false),
        };
        let graph = Arc::clone(jadpo_check::graph::graph(
            &self.loaded.db,
            self.loaded.project,
        ));
        // The first run under `--affected` runs what changed since its
        // base reaches, and says so when that is nothing.
        let since = match (pick, &self.baseline) {
            (Pick::Reached, None) => self.options.since.clone(),
            _ => None,
        };
        let only = match pick {
            Pick::Reached => match (&self.baseline, &since) {
                (Some(before), _) => Some(affected(&graph, before)),
                (None, Some(since)) => Some(crate::reached_since(&self.loaded, since)),
                (None, None) => None,
            },
            Pick::All => None,
            Pick::Failing => Some(self.failing.clone()),
            Pick::Again => Some(self.last.clone()),
            Pick::Only(keys) => Some(keys.clone()),
        };
        let ran = crate::choose_and_run(
            &self.loaded,
            &checked,
            &self.target,
            &self.options,
            only.as_ref(),
            started,
            None,
        );
        self.baseline = Some(graph);
        // `--failed` picks the first run only; later runs are what each
        // change reaches.
        self.options.failed = false;
        self.options.since = None;
        let keys: BTreeSet<&str> = checked.tests.iter().map(|test| test.key.as_str()).collect();
        self.failing.retain(|key| keys.contains(key.as_str()));
        if let Ran::Done(run) = &ran {
            for result in &run.results {
                if result.failure.is_some() {
                    self.failing.insert(result.test.key.clone());
                } else {
                    self.failing.remove(&result.test.key);
                }
            }
            if !run.results.is_empty() {
                self.last = run
                    .results
                    .iter()
                    .map(|result| result.test.key.clone())
                    .collect();
            }
        }
        if let Some(since) = &since {
            return (None, Some(crate::nothing_reached(ran, since)), true);
        }
        match (only, ran) {
            (None, ran) => (None, Some(ran), true),
            (Some(_), Ran::Done(run)) if run.results.is_empty() => (Some(0), None, true),
            (Some(_), Ran::Done(run)) => (Some(run.results.len()), Some(Ran::Done(run)), true),
            (Some(_), ran) => (None, Some(ran), true),
        }
    }
}

/// The interpreter's thread couldn't start, with `error`.
fn start_failed(error: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::TEST_RUNTIME_START_FAILED,
        "The interpreter couldn't start",
    )
    .reason(format!(
        "Tests run on a thread with a large stack, and starting it failed: {error}."
    ))
    .next("Free some memory, then save again")
}

/// Why `options` can't start a session, if they can't: a path that
/// doesn't exist, or no project to run in.
pub fn validate(options: &Options) -> Option<Ran> {
    select::resolve(options).err()
}

/// The keys of the tests a change from `before` to `graph` reaches. What
/// changed is every node that is new or whose digest differs, and the
/// target of every [`Follow::Forwards`] edge from one of those, or from a
/// node `before` had and `graph` hasn't; then reverse reachability over
/// the [`Follow::Backwards`] edges, and from a node to what holds it (a
/// field to its type, a site to its callable), since what uses a type uses
/// its fields.
pub fn affected(graph: &Graph, before: &Graph) -> BTreeSet<String> {
    let digests: HashMap<&str, Option<Digest>> = before
        .nodes()
        .iter()
        .map(|node| (node.key.as_str(), node.digest))
        .collect();
    let mut changed = vec![false; graph.len()];
    let mut queue: Vec<NodeId> = Vec::new();
    // Whether `id` is newly marked changed.
    let mut mark = |id: NodeId, queue: &mut Vec<NodeId>| {
        let new = !changed[id.index()];
        if new {
            changed[id.index()] = true;
            queue.push(id);
        }
        new
    };
    for id in graph.ids() {
        let node = graph.node(id);
        if digests.get(node.key.as_str()) != Some(&node.digest) {
            mark(id, &mut queue);
        }
    }
    // What a removed node changed the behaviour of, by key, as `before`
    // had it.
    for id in before.ids() {
        if graph.find(&before.node(id).key).is_some() {
            continue;
        }
        for kind in kinds(Follow::Forwards) {
            for target in before.outgoing(id, kind) {
                if let Some(now) = graph.find(&before.node(target).key) {
                    mark(now, &mut queue);
                }
            }
        }
    }
    // A changed node changes what it changes the behaviour of, and so on.
    let mut forwards = queue.clone();
    while let Some(id) = forwards.pop() {
        for kind in kinds(Follow::Forwards) {
            for target in graph.outgoing(id, kind) {
                if mark(target, &mut queue) {
                    forwards.push(target);
                }
            }
        }
    }
    let mut seen = vec![false; graph.len()];
    for id in &queue {
        seen[id.index()] = true;
    }
    while let Some(id) = queue.pop() {
        let mut next: Vec<_> = kinds(Follow::Backwards)
            .flat_map(|kind| graph.incoming(id, kind))
            .collect();
        if let Some(parent) = &graph.node(id).parent
            && let Some(parent) = graph.find(parent)
        {
            next.push(parent);
        }
        for from in next {
            if !seen[from.index()] {
                seen[from.index()] = true;
                queue.push(from);
            }
        }
    }
    graph
        .ids()
        .filter(|id| seen[id.index()] && graph.node(*id).kind == Kind::Test)
        .map(|id| graph.node(id).key.as_str().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use jadpo_graph::{Builder, Key, Node};

    use super::*;

    /// A graph of two tests, one calling `source` and one calling
    /// `target`, with `source` `kind` `target`; each node's digest from its
    /// version, and `source` left out when `with_source` is false.
    fn graph(kind: EdgeKind, versions: [&str; 2], with_source: bool) -> Graph {
        let mut builder = Builder::new();
        let mut node = |kind: Kind, name: &str, version: &str| {
            let mut node = Node::new(Key::new(kind, name), kind, name);
            node.digest = Some(Digest::of([name, version]));
            builder.node(node);
        };
        node(Kind::Test, "reaches the source", "1");
        node(Kind::Test, "reaches the target", "1");
        if with_source {
            node(Kind::Function, "source", versions[0]);
        }
        node(Kind::Function, "target", versions[1]);
        let key = |kind: Kind, name: &str| Key::new(kind, name);
        builder.edge(
            key(Kind::Test, "reaches the source"),
            EdgeKind::Calls,
            key(Kind::Function, "source"),
        );
        builder.edge(
            key(Kind::Test, "reaches the target"),
            EdgeKind::Calls,
            key(Kind::Function, "target"),
        );
        builder.edge(
            key(Kind::Function, "source"),
            kind,
            key(Kind::Function, "target"),
        );
        builder.build()
    }

    /// Which tests rerun after `after` replaces `before`, as `source` and
    /// `target` for the test that reaches each.
    fn rerun(before: &Graph, after: &Graph) -> Vec<&'static str> {
        let reached = affected(after, before);
        [
            ("source", "reaches the source"),
            ("target", "reaches the target"),
        ]
        .into_iter()
        .filter(|(_, test)| reached.contains(Key::new(Kind::Test, test).as_str()))
        .map(|(which, _)| which)
        .collect()
    }

    #[test]
    fn every_edge_kind_carries_a_change_the_way_it_is_classified() {
        for &kind in EdgeKind::ALL {
            let before = graph(kind, ["1", "1"], true);
            let target_changed = graph(kind, ["1", "2"], true);
            let source_changed = graph(kind, ["2", "1"], true);
            let source_removed = graph(kind, ["1", "1"], false);
            // A removed source takes its callers' edges with it: in a real
            // project their own text changes too, so only what it changed
            // the behaviour of is left to find.
            let (on_target, on_source, on_removal) = match follow(kind) {
                Follow::Backwards => (vec!["source", "target"], vec!["source"], vec![]),
                Follow::Forwards => (vec!["target"], vec!["source", "target"], vec!["target"]),
                Follow::Irrelevant => (vec!["target"], vec!["source"], vec![]),
            };
            assert_eq!(
                rerun(&before, &target_changed),
                on_target,
                "{kind:?}: target changed"
            );
            assert_eq!(
                rerun(&before, &source_changed),
                on_source,
                "{kind:?}: source changed"
            );
            assert_eq!(
                rerun(&before, &source_removed),
                on_removal,
                "{kind:?}: source removed"
            );
            assert!(
                rerun(&before, &before).is_empty(),
                "{kind:?}: nothing changed"
            );
        }
    }

    #[test]
    fn a_changed_subscriber_reaches_the_tests_of_actions_that_emit_its_event() {
        let build = |version: &str| {
            let mut builder = Builder::new();
            for (kind, name, digest) in [
                (Kind::Test, "places an order", "1"),
                (Kind::Action, "place_order", "1"),
                (Kind::Type, "OrderPlaced", "1"),
                (Kind::Subscriber, "send_receipt", version),
            ] {
                let mut node = Node::new(Key::new(kind, name), kind, name);
                node.digest = Some(Digest::of([name, digest]));
                builder.node(node);
            }
            builder.edge(
                Key::new(Kind::Test, "places an order"),
                EdgeKind::Calls,
                Key::new(Kind::Action, "place_order"),
            );
            builder.edge(
                Key::new(Kind::Action, "place_order"),
                EdgeKind::Emits,
                Key::new(Kind::Type, "OrderPlaced"),
            );
            builder.edge(
                Key::new(Kind::Subscriber, "send_receipt"),
                EdgeKind::Subscribes,
                Key::new(Kind::Type, "OrderPlaced"),
            );
            builder.build()
        };
        let reached = affected(&build("2"), &build("1"));
        assert_eq!(
            reached.into_iter().collect::<Vec<_>>(),
            [Key::new(Kind::Test, "places an order").as_str()]
        );
    }
}
