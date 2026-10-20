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
//!
//! An editor that saves in place truncates the file, then writes it, and
//! on a loaded machine the two can be far apart. An empty source file
//! checks cleanly with nothing in it, so a file that had text and is
//! empty now is taken as a save under way, not yet saved: the burst waits
//! for its text, however long the gap, up to [`TRUNCATED_GRACE`], and a
//! file still empty then is handed over as saved (chunk 5.13b).

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
/// How long a burst waits for the text of a file it emptied, before the
/// empty file is taken as saved.
pub const TRUNCATED_GRACE: Duration = Duration::from_secs(2);

/// What a look saw of one file: when it was last modified and its size,
/// or nothing when its metadata couldn't be read.
type Stamp = Option<(SystemTime, u64)>;

/// Watches the files a project (or a folder of loose files) holds.
#[derive(Debug)]
pub struct Poller {
    root: PathBuf,
    project: bool,
    stamps: BTreeMap<String, Stamp>,
    /// Files outside the project's sources that are watched too, by their
    /// project-relative paths: `.env.local`, whose values `jadpo dev`
    /// reads (area 10).
    extra: Vec<String>,
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
            extra: Vec::new(),
            interval: INTERVAL,
        };
        poller.stamps = poller.look();
        poller
    }

    /// Also watches `path`, relative to the root, which need not exist: it
    /// is added when it appears and removed when it goes.
    pub fn watching(mut self, path: &str) -> Poller {
        self.extra.push(path.to_string());
        self.stamps = self.look();
        self
    }

    /// How many files it watches.
    pub fn len(&self) -> usize {
        self.stamps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }

    /// Every file it watches, project-relative, in path order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.stamps.keys().map(String::as_str)
    }

    fn look(&mut self) -> BTreeMap<String, Stamp> {
        let started = Instant::now();
        let stamps = jadpo_db::files_under(&self.root, self.project)
            .into_iter()
            .chain(
                self.extra
                    .iter()
                    .filter(|path| self.root.join(path).is_file())
                    .cloned(),
            )
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
        self.rescan().0
    }

    /// The files that changed since the last look, in path order, and
    /// what that look saw.
    fn rescan(&mut self) -> (Vec<Edit>, BTreeMap<String, Stamp>) {
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
        (edits, std::mem::replace(&mut self.stamps, now))
    }

    /// Whether `burst` holds a file that had text `before` it and is empty
    /// at the last look: a save in place between its truncate and its
    /// write.
    fn truncated(&self, before: &BTreeMap<String, Stamp>, burst: &[Edit]) -> bool {
        let size = |stamps: &BTreeMap<String, Stamp>, path: &str| {
            stamps.get(path).copied().flatten().map(|(_, size)| size)
        };
        burst.iter().any(|edit| {
            size(before, &edit.path).is_some_and(|size| size > 0)
                && size(&self.stamps, &edit.path) == Some(0)
        })
    }

    /// Waits for the next burst of changes and returns it, merged; `None`
    /// once `stop` says so.
    pub fn wait(&mut self, stop: &dyn Fn() -> bool) -> Option<Vec<Edit>> {
        loop {
            if stop() {
                return None;
            }
            let (first, before) = self.rescan();
            if first.is_empty() {
                std::thread::sleep(self.interval);
                continue;
            }
            let mut burst = first;
            let began = Instant::now();
            loop {
                let mut calm = Instant::now();
                while calm.elapsed() < QUIET && began.elapsed() < LONGEST_BURST {
                    std::thread::sleep(BURST_STEP);
                    let more = self.scan();
                    if !more.is_empty() {
                        burst.extend(more);
                        calm = Instant::now();
                    }
                }
                // A file emptied by a save in place waits for its text.
                if !self.truncated(&before, &burst) || began.elapsed() >= TRUNCATED_GRACE {
                    break;
                }
                if stop() {
                    return None;
                }
                std::thread::sleep(BURST_STEP);
                burst.extend(self.scan());
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

    /// A folder of loose files of its own, removed after.
    struct Folder(PathBuf);

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn folder(name: &str) -> Folder {
        let path =
            std::env::temp_dir().join(format!("jadpo-poll-5.13b-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a folder");
        Folder(path)
    }

    #[test]
    fn a_save_in_place_is_handed_over_once_its_text_is_written() {
        let dir = folder("in-place");
        let file = dir.0.join("routes.jadpo");
        std::fs::write(&file, "routes {}\n").expect("write");
        let mut poller = Poller::new(&dir.0, false);
        assert_eq!(poller.len(), 1);
        // Truncated, then written far later than the quiet a burst waits.
        let writer = {
            let file = file.clone();
            std::thread::spawn(move || {
                std::fs::write(&file, "").expect("truncate");
                std::thread::sleep(Duration::from_millis(400));
                std::fs::write(&file, "routes { }\n").expect("write");
            })
        };
        let edits = poller.wait(&|| false).expect("a burst");
        writer.join().expect("the writer");
        assert_eq!(edits, vec![edit("routes.jadpo", EditKind::Saved)]);
        let text = std::fs::read_to_string(&file).expect("read");
        assert_eq!(text, "routes { }\n", "handed over with its text");
    }

    #[test]
    fn a_file_left_empty_is_handed_over_as_saved() {
        let dir = folder("emptied");
        let file = dir.0.join("routes.jadpo");
        std::fs::write(&file, "routes {}\n").expect("write");
        let mut poller = Poller::new(&dir.0, false);
        let started = Instant::now();
        std::fs::write(&file, "").expect("empty");
        let edits = poller.wait(&|| false).expect("a burst");
        assert_eq!(edits, vec![edit("routes.jadpo", EditKind::Saved)]);
        assert!(
            started.elapsed() >= TRUNCATED_GRACE,
            "{:?}",
            started.elapsed()
        );
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
