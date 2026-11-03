//! Which of a commit's files a release may hold (area 11, "Releasing"):
//! Jadpo source and the package's text files, within the size limit. A
//! release is decided from the tree's listing alone, before any file is
//! written anywhere.
//!
//! What a repository needs for itself (`.gitignore`, `.github/`, a README, a
//! licence file) is left out of the release instead: nobody who installs the
//! package receives it, and it isn't in the digest.

use std::path::Path;

use jadpo_registry::archive::is_package_path;

/// The most a release's archive may come to. The registry is a Jadpo
/// application, and receives the archive in base64 in a provider's answer,
/// which is at most 10 MiB (`MOST_ANSWER`, jadpo-runtime): 7,000,000 bytes
/// fit with the rest of the answer, and a release past that would pass here
/// and never go live.
pub const SIZE_LIMIT: u64 = 7_000_000;

/// A tar header, and the unit its contents are padded to.
const BLOCK: u64 = 512;

/// The most files a release may hold.
pub const COUNT_LIMIT: usize = 10_000;

/// One file in a commit's tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub mode: String,
    pub object: String,
    /// `None` for what isn't a blob.
    pub size: Option<u64>,
}

/// What a path is, by its name alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Judged {
    Release,
    LeftOut,
    Refused,
}

/// What `path` is.
pub fn judge(path: &str) -> Judged {
    if path.split('/').any(|part| part.starts_with('.')) {
        return Judged::LeftOut;
    }
    if !path.contains('/') && is_repository_file(path) {
        return Judged::LeftOut;
    }
    if !is_package_path(path) || path.starts_with("build/") {
        return Judged::Refused;
    }
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    let allowed = match name.rsplit_once('.').map_or("", |(_, extension)| extension) {
        // The index of an app is its own kind of project: a package's root
        // holds none, so a launcher never reads one here.
        "jadpo" => !folder.is_empty() || name != "app.jadpo",
        "jadpo_record" => true,
        "md" => folder.is_empty() && matches!(name, "GUIDE.md" | "CHANGELOG.md"),
        "json" => path
            .split('/')
            .rev()
            .skip(1)
            .any(|part| part == "recordings"),
        "lock" => name == "jadpo.lock" && !folder.is_empty(),
        _ => false,
    };
    if allowed {
        Judged::Release
    } else {
        Judged::Refused
    }
}

/// A file every repository may carry beside its package.
fn is_repository_file(name: &str) -> bool {
    let stem = name.split_once('.').map_or(name, |(stem, _)| stem);
    let stem = stem.to_ascii_uppercase();
    matches!(stem.as_str(), "README" | "LICENSE" | "LICENCE" | "COPYING")
}

/// What a whole tree comes to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Judgement {
    /// The files of the release, by path.
    pub release: Vec<Entry>,
    pub left_out: Vec<String>,
    /// Each file refused, with why.
    pub refused: Vec<(String, String)>,
    /// What the release's archive comes to, as
    /// [`jadpo_registry::archive::pack`] writes it.
    pub size: u64,
}

impl Judgement {
    pub fn too_large(&self) -> bool {
        self.size > SIZE_LIMIT || self.release.len() > COUNT_LIMIT
    }

    /// What is wrong with it, for a row of the bar; `None` when it's fine.
    pub fn problem(&self) -> Option<String> {
        if !self.refused.is_empty() {
            let count = self.refused.len();
            return Some(format!(
                "{count} {} not allowed",
                if count == 1 { "file" } else { "files" }
            ));
        }
        self.too_large().then(|| {
            format!(
                "{} over the {} limit",
                megabytes(self.size),
                megabytes(SIZE_LIMIT)
            )
        })
    }

    /// The bar's row when it passes.
    pub fn summary(&self) -> String {
        let count = self.release.len();
        format!(
            "{count} {}, {}",
            if count == 1 { "file" } else { "files" },
            megabytes(self.size)
        )
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.2} MB", bytes as f64 / 1_000_000.0)
}

/// Judges every file of a tree.
pub fn judge_tree(entries: Vec<Entry>) -> Judgement {
    // An archive ends in two empty blocks.
    let mut judged = Judgement {
        size: 2 * BLOCK,
        ..Judgement::default()
    };
    for entry in entries {
        match judge(&entry.path) {
            Judged::LeftOut => judged.left_out.push(entry.path),
            Judged::Refused => judged.refused.push((
                entry.path,
                "it isn't Jadpo source or one of a package's text files".to_string(),
            )),
            Judged::Release => match entry.mode.as_str() {
                "100644" | "100755" => {
                    let contents = entry
                        .size
                        .unwrap_or(0)
                        .div_ceil(BLOCK)
                        .saturating_mul(BLOCK);
                    judged.size = judged.size.saturating_add(BLOCK).saturating_add(contents);
                    judged.release.push(entry);
                }
                "120000" => judged
                    .refused
                    .push((entry.path, "it is a symbolic link".to_string())),
                "160000" => judged
                    .refused
                    .push((entry.path, "it is a submodule".to_string())),
                _ => judged
                    .refused
                    .push((entry.path, "it isn't a regular file".to_string())),
            },
        }
    }
    judged
}

/// Whether `bytes` are text: UTF-8 with no NUL.
pub fn is_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// The reason a file that isn't text is refused.
pub const NOT_TEXT: &str = "it isn't text";

/// Judges a package's folder as it stands, as `jadpo package verify <folder>`
/// does: hidden files and the `build/` folder a package's own commands write
/// aren't part of a commit, and a file's contents are read to see it's text.
pub fn judge_folder(folder: &Path) -> std::io::Result<Judgement> {
    let mut entries = Vec::new();
    let mut unreadable = Vec::new();
    let mut pending = vec![(folder.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                unreadable.push((
                    format!("{prefix}{}", name.to_string_lossy()),
                    "its name isn't UTF-8".to_string(),
                ));
                continue;
            };
            let path = format!("{prefix}{name}");
            let kind = entry.file_type()?;
            if kind.is_dir() {
                if !name.starts_with('.') && path != "build" {
                    pending.push((entry.path(), format!("{path}/")));
                }
            } else if kind.is_symlink() {
                entries.push(Entry {
                    path,
                    mode: "120000".to_string(),
                    object: String::new(),
                    size: None,
                });
            } else {
                entries.push(Entry {
                    path,
                    mode: "100644".to_string(),
                    object: String::new(),
                    size: Some(entry.metadata()?.len()),
                });
            }
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let mut judged = judge_tree(entries);
    judged.refused.extend(unreadable);
    if !judged.too_large() {
        for entry in &judged.release {
            if !is_text(&std::fs::read(folder.join(&entry.path))?) {
                judged
                    .refused
                    .push((entry.path.clone(), NOT_TEXT.to_string()));
            }
        }
    }
    judged.refused.sort();
    Ok(judged)
}
