//! Watch mode (area 13, "Running tests"; docs/compiler-runtime.md §2.4):
//! `jadpo test --watch` reruns, on every save, the tests the change
//! reaches, using the compiler's own dependency graph.
//!
//! A [`Watch`] keeps one database for its lifetime. Each burst of saves
//! updates its inputs, so the check that follows recomputes only what the
//! saves touched; the graph it builds is compared with the graph of the
//! last run node by node, and the tests that reach a node whose digest
//! changed, by reverse reachability over `calls`, `runs`, `reads`, `uses`
//! and `covers` (and from a field, variant or site to what holds it), are
//! the ones that run. A save that breaks the build shows its errors and
//! runs nothing; the next save that checks runs everything changed since
//! the last run. A [`Poller`] finds the saves.

mod poll;

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::Instant;

use jadpo_db::{Database, Loaded};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::{Digest, EdgeKind, Graph, Kind};

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
}

/// The edges a test reaches what it depends on through (§2.4).
const REACH: [EdgeKind; 5] = [
    EdgeKind::Calls,
    EdgeKind::Runs,
    EdgeKind::Reads,
    EdgeKind::Uses,
    EdgeKind::Covers,
];

/// Every node's digest, by key.
type Digests = HashMap<String, Option<Digest>>;

/// A watch session: the options it was started with, the project's
/// long-lived database, and the graph's digests at the last run.
#[derive(Debug)]
pub struct Watch {
    target: select::Target,
    options: Options,
    loaded: Loaded,
    /// The digests the last run saw; `None` until a run has happened.
    baseline: Option<Digests>,
    /// The edits since the last run, while the build is broken.
    pending: Vec<Edit>,
    /// The keys of the tests failing now.
    failing: BTreeSet<String>,
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
        let target = select::resolve(&options)?;
        let loaded = jadpo_db::load_into(&target.folder, db);
        let mut watch = Watch {
            target,
            options,
            loaded,
            baseline: None,
            pending: Vec::new(),
            failing: BTreeSet::new(),
        };
        let event = watch.step(Vec::new());
        Ok((watch, event))
    }

    /// The folder every path is relative to.
    pub fn root(&self) -> &Path {
        &self.loaded.root
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
        Some(self.step(changed))
    }

    /// Reads one changed path again; whether anything the check reads
    /// changed.
    fn refresh(&mut self, path: &str) -> bool {
        if !self.loaded.tracks(path) {
            return false;
        }
        let db = &self.loaded.db;
        let before = (
            self.loaded.file(path).map(|file| file.text(db).clone()),
            self.loaded.project.other_files(db).clone(),
            self.loaded.unreadable.clone(),
        );
        self.loaded.refresh(path);
        let db = &self.loaded.db;
        let after = (
            self.loaded.file(path).map(|file| file.text(db).clone()),
            self.loaded.project.other_files(db).clone(),
            self.loaded.unreadable.clone(),
        );
        before != after
    }

    /// Checks, works out what the edits reach, and runs it, on the
    /// interpreter's thread.
    fn step(&mut self, edits: Vec<Edit>) -> Event {
        let these = merge(edits.clone());
        self.pending.extend(edits);
        self.pending = merge(std::mem::take(&mut self.pending));
        let started = Instant::now();
        let run = jadpo_interp::on_large_stack(|| self.check_and_run(started));
        let (reached, ran, good) = match run {
            Ok(result) => result,
            Err(error) => (
                None,
                Some(Ran::Stopped(Box::new(
                    Diagnostic::new(
                        &rule::TEST_RUNTIME_START_FAILED,
                        "The interpreter couldn't start",
                    )
                    .reason(format!(
                        "Tests run on a thread with a large stack, and starting it failed: {error}."
                    ))
                    .next("Free some memory, then save again"),
                ))),
                false,
            ),
        };
        // A save that checks shows everything changed since the last run;
        // one that doesn't, only itself.
        let edits = if good {
            std::mem::take(&mut self.pending)
        } else {
            these
        };
        Event {
            edits,
            reached,
            ran,
            failing: self.failing.len(),
            files: self.loaded.project.files(&self.loaded.db).len()
                + self.loaded.project.other_files(&self.loaded.db).len()
                + self.loaded.unreadable.len(),
        }
    }

    /// The count reached, what ran, and whether the build checked.
    fn check_and_run(&mut self, started: Instant) -> (Option<usize>, Option<Ran>, bool) {
        let checked = match crate::prepare(&self.loaded, &self.options.cwd) {
            Ok(checked) => checked,
            Err(ran) => return (None, Some(ran), false),
        };
        let graph = jadpo_check::graph::graph(&self.loaded.db, self.loaded.project);
        let now = digests(graph);
        let only = self.baseline.as_ref().map(|before| affected(graph, before));
        let ran = crate::choose_and_run(
            &self.loaded,
            &checked,
            &self.target,
            &self.options,
            only.as_ref(),
            started,
        );
        self.baseline = Some(now);
        // `--failed` picks the first run only; later runs are what each
        // change reaches.
        self.options.failed = false;
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
        }
        match (only, ran) {
            (None, ran) => (None, Some(ran), true),
            (Some(_), Ran::Done(run)) if run.results.is_empty() => (Some(0), None, true),
            (Some(_), Ran::Done(run)) => (Some(run.results.len()), Some(Ran::Done(run)), true),
            (Some(_), ran) => (None, Some(ran), true),
        }
    }
}

/// Why `options` can't start a session, if they can't: a kind not built
/// yet, a path that doesn't exist, or no project to run in.
pub fn validate(options: &Options) -> Option<Ran> {
    if let Some(kind) = options.kind
        && !kind.built()
    {
        return Some(Ran::Stopped(Box::new(select::kind_not_built(kind))));
    }
    select::resolve(options).err()
}

/// Every node's digest, by key.
fn digests(graph: &Graph) -> Digests {
    graph
        .nodes()
        .iter()
        .map(|node| (node.key.as_str().to_string(), node.digest))
        .collect()
}

/// The keys of the tests that reach a node whose digest differs from
/// `before`, or that is new: reverse reachability over [`REACH`], and from
/// a node to what holds it (a field to its type, a site to its callable),
/// since what uses a type uses its fields.
pub fn affected(graph: &Graph, before: &HashMap<String, Option<Digest>>) -> BTreeSet<String> {
    let mut seen = vec![false; graph.len()];
    let mut queue = Vec::new();
    for id in graph.ids() {
        let node = graph.node(id);
        if before.get(node.key.as_str()) != Some(&node.digest) {
            seen[id.index()] = true;
            queue.push(id);
        }
    }
    while let Some(id) = queue.pop() {
        let mut next: Vec<_> = REACH
            .iter()
            .flat_map(|kind| graph.incoming(id, *kind))
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
