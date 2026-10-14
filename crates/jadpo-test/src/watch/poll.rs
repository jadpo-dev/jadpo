//! Finding saves by looking: every few tens of milliseconds the poller
//! lists the project's files and compares each one's modification time and
//! size with the last look. Polling needs no platform's file-event API and
//! no dependency, sees saves on every file system (network and container
//! mounts included), and a look at a few hundred files costs well under a
//! millisecond; the interval grows with the cost of a look, so a large
//! project is never polled harder than a tenth of the time.
//!
//! An editor's save is often several writes (a temporary file, a rename, a
//! formatter's rewrite), so a burst of changes is gathered until the files
//! have been quiet for a moment, then handed over as one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::{Edit, EditKind};

/// The shortest time between two looks.
pub const INTERVAL: Duration = Duration::from_millis(40);
/// How long the files must stay unchanged before a burst is handed over.
pub const QUIET: Duration = Duration::from_millis(50);
/// The longest a burst is gathered, so a file written continuously still
/// gets its tests run.
const LONGEST_BURST: Duration = Duration::from_millis(1000);
/// How often to look while a burst is being gathered.
const BURST_STEP: Duration = Duration::from_millis(10);

/// What a look saw of one file: when it was last modified and its size,
/// or nothing when its metadata couldn't be read.
type Stamp = Option<(SystemTime, u64)>;

/// Watches the files a project (or a folder of loose files) holds.
#[derive(Debug)]
pub struct Poller {
    root: PathBuf,
    project: bool,
    stamps: BTreeMap<String, Stamp>,
    /// The time between looks, from what the last look cost.
    interval: Duration,
}

impl Poller {
    /// Takes the first look at `root`.
    pub fn new(root: &Path, project: bool) -> Poller {
        let mut poller = Poller {
            root: root.to_path_buf(),
            project,
            stamps: BTreeMap::new(),
            interval: INTERVAL,
        };
        poller.stamps = poller.look();
        poller
    }

    /// How many files it watches.
    pub fn len(&self) -> usize {
        self.stamps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }

    fn look(&mut self) -> BTreeMap<String, Stamp> {
        let started = Instant::now();
        let stamps = jadpo_db::files_under(&self.root, self.project)
            .into_iter()
            .map(|path| {
                let stamp = std::fs::metadata(self.root.join(&path)).ok().map(|meta| {
                    (
                        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        meta.len(),
                    )
                });
                (path, stamp)
            })
            .collect();
        self.interval = INTERVAL.max(started.elapsed() * 10);
        stamps
    }

    /// The files that changed since the last look, in path order.
    pub fn scan(&mut self) -> Vec<Edit> {
        let now = self.look();
        let mut edits = Vec::new();
        for (path, stamp) in &now {
            match self.stamps.get(path) {
                None => edits.push(Edit::new(path.as_str(), EditKind::Added)),
                Some(before) if before != stamp => {
                    edits.push(Edit::new(path.as_str(), EditKind::Saved))
                }
                Some(_) => {}
            }
        }
        for path in self.stamps.keys() {
            if !now.contains_key(path) {
                edits.push(Edit::new(path.as_str(), EditKind::Removed));
            }
        }
        edits.sort();
        self.stamps = now;
        edits
    }

    /// Waits for the next burst of changes and returns it, merged; `None`
    /// once `stop` says so.
    pub fn wait(&mut self, stop: &dyn Fn() -> bool) -> Option<Vec<Edit>> {
        loop {
            if stop() {
                return None;
            }
            let first = self.scan();
            if first.is_empty() {
                std::thread::sleep(self.interval);
                continue;
            }
            let mut burst = first;
            let began = Instant::now();
            let mut calm = Instant::now();
            while calm.elapsed() < QUIET && began.elapsed() < LONGEST_BURST {
                std::thread::sleep(BURST_STEP);
                let more = self.scan();
                if !more.is_empty() {
                    burst.extend(more);
                    calm = Instant::now();
                }
            }
            return Some(merge(burst));
        }
    }
}

/// Edits in the order they happened, as one edit per file: a file added
/// then saved was added, one added then removed never was, one removed
/// then added again was saved.
pub fn merge(edits: Vec<Edit>) -> Vec<Edit> {
    let mut merged: BTreeMap<String, Option<EditKind>> = BTreeMap::new();
    for edit in edits {
        let kind = match (merged.get(&edit.path).copied().flatten(), edit.kind) {
            // New, or added and removed again, which is as if it never
            // happened.
            (None, kind) => Some(kind),
            (Some(EditKind::Added), EditKind::Removed) => None,
            (Some(EditKind::Added), _) => Some(EditKind::Added),
            (Some(EditKind::Removed), EditKind::Removed) => Some(EditKind::Removed),
            (Some(EditKind::Removed), _) => Some(EditKind::Saved),
            (Some(EditKind::Saved), EditKind::Removed) => Some(EditKind::Removed),
            (Some(EditKind::Saved), _) => Some(EditKind::Saved),
        };
        merged.insert(edit.path, kind);
    }
    merged
        .into_iter()
        .filter_map(|(path, kind)| kind.map(|kind| Edit { path, kind }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(path: &str, kind: EditKind) -> Edit {
        Edit::new(path, kind)
    }

    #[test]
    fn edits_merge_to_one_per_file() {
        use EditKind::*;
        assert_eq!(
            merge(vec![
                edit("b", Added),
                edit("a", Saved),
                edit("b", Saved),
                edit("a", Saved),
                edit("c", Added),
                edit("c", Removed),
                edit("d", Removed),
                edit("d", Added),
                edit("e", Saved),
                edit("e", Removed),
            ]),
            vec![
                edit("a", Saved),
                edit("b", Added),
                edit("d", Saved),
                edit("e", Removed),
            ]
        );
    }
}
