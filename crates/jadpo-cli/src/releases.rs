//! Jadpo's own releases, as the toolchain launcher fetches and keeps them
//! (area 14, "Which jadpo runs"; roadmap chunk D14).
//!
//! Each release on `github.com/jadpo-dev/jadpo` carries an archive per
//! target, named by dist (`dist-workspace.toml`): `jadpo-cli-<target>.tar.gz`,
//! or `.zip` on Windows, with its SHA-256 beside it in `<archive>.sha256`
//! and a GitHub artifact attestation ([`crate::attestation`]). [`Cache`]
//! keeps the releases a project needed in `jadpo/releases/<version>/` in
//! the user's cache folder: each is fetched once, its archive checked
//! against the published SHA-256 and the attestation before anything in it
//! is run, and only the `jadpo` binary taken out. A release is written to a
//! hidden folder beside its place and renamed into it whole, under a lock
//! on `.<version>.lock`, so two runs at once fetch it once and a run never
//! sees half of one. Nothing is written outside that folder.
//!
//! The network is behind [`Network`], so a test serves its own releases.

use std::path::{Path, PathBuf};

use crate::attestation::Attest;
use crate::download;

/// The repository whose releases these are.
pub(crate) const REPOSITORY: &str = "jadpo-dev/jadpo";

/// The most an archive may be.
const LONGEST_ARCHIVE: usize = 512 << 20;

/// The most one page of the release list may be.
const LONGEST_LIST: usize = 16 << 20;

/// How many pages of the release list are read, 100 releases each.
const PAGES: usize = 10;

pub(crate) use jadpo_check::jadpo_version::Unfetched;

/// Why a release couldn't be had: what kind of failure, and the failure as
/// a sentence.
pub(crate) type Failure = (Unfetched, String);

/// A failure to fetch `url`: a release that isn't published when GitHub
/// has no such file, else GitHub not reached.
pub(crate) fn unfetched(why: String) -> Failure {
    let kind = if why.contains("(404)") {
        Unfetched::Unreleased
    } else {
        Unfetched::Unreachable
    };
    (kind, why)
}

/// Where releases are fetched from.
pub(crate) trait Network {
    /// All of `url`, refusing more than `most` bytes; why not, as a
    /// sentence, when it can't be fetched.
    fn get(&self, url: &str, most: usize) -> Result<Vec<u8>, String>;

    /// [`Network::get`], telling `progress` how many bytes have come of
    /// how many, when that is known.
    fn get_reporting(
        &self,
        url: &str,
        most: usize,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<Vec<u8>, String> {
        let _ = progress;
        self.get(url, most)
    }
}

/// GitHub, over HTTPS.
pub(crate) struct Https;

impl Network for Https {
    fn get(&self, url: &str, most: usize) -> Result<Vec<u8>, String> {
        download::get_all(url, most)
    }

    fn get_reporting(
        &self,
        url: &str,
        most: usize,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<Vec<u8>, String> {
        let mut body = Vec::new();
        let total = std::cell::Cell::new(None);
        download::get(
            url,
            &mut |started| {
                total.set(started.length);
                progress(0, started.length);
            },
            &mut |piece| {
                if body.len() + piece.len() > most {
                    return Err(std::io::Error::other("the answer is too long"));
                }
                body.extend_from_slice(piece);
                progress(body.len() as u64, total.get());
                Ok(())
            },
        )?;
        Ok(body)
    }
}

/// How a fetch is reported as it goes.
pub(crate) trait Report {
    /// Fetching `release` starts.
    fn fetching(&self, release: Release);
    /// Its archive's bytes so far, of how many when that is known.
    fn progress(&self, done: u64, total: Option<u64>);
}

/// A release's version: its major, minor and patch numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Release(pub(crate) u64, pub(crate) u64, pub(crate) u64);

impl Release {
    /// `0.1.2` read: three numbers, none with a leading zero, and no
    /// pre-release.
    pub(crate) fn parse(text: &str) -> Option<Release> {
        let mut parts = text.split('.').map(|part| {
            (!part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part == "0" || !part.starts_with('0')))
            .then(|| part.parse::<u64>().ok())
            .flatten()
        });
        let release = Release(parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then_some(release)
    }

    /// The line it is on: its major and minor numbers.
    pub(crate) fn line(self) -> (u64, u64) {
        (self.0, self.1)
    }

    /// This `jadpo`'s own release.
    pub(crate) fn running() -> Release {
        Release::parse(crate::help::VERSION).unwrap_or(Release(0, 0, 0))
    }
}

impl std::fmt::Display for Release {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The archive dist names for `target`.
pub(crate) fn archive_name(target: &str) -> String {
    let extension = if windows(target) { "zip" } else { "tar.gz" };
    format!("jadpo-cli-{target}.{extension}")
}

fn windows(target: &str) -> bool {
    target.contains("windows")
}

/// The binary's name on `target`.
fn binary_name(target: &str) -> &'static str {
    if windows(target) {
        "jadpo.exe"
    } else {
        "jadpo"
    }
}

/// Where release `release`'s file `name` is downloaded from.
fn download_url(release: Release, name: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/v{release}/{name}")
}

/// The newest published release of `line`, or `None` when none is.
pub(crate) fn newest(network: &dyn Network, line: (u64, u64)) -> Result<Option<Release>, Failure> {
    let unreadable = |why: &str| (Unfetched::Unreachable, why.to_string());
    let mut best: Option<Release> = None;
    for page in 1..=PAGES {
        let url =
            format!("https://api.github.com/repos/{REPOSITORY}/releases?per_page=100&page={page}");
        let body = network
            .get(&url, LONGEST_LIST)
            .map_err(|why| (Unfetched::Unreachable, why))?;
        let list: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|_| unreadable("GitHub's list of releases isn't JSON."))?;
        let releases = list
            .as_array()
            .ok_or_else(|| unreadable("GitHub's list of releases isn't a list."))?;
        for release in releases {
            let flagged = |key: &str| release.get(key).and_then(serde_json::Value::as_bool);
            if flagged("draft") != Some(false) || flagged("prerelease") != Some(false) {
                continue;
            }
            let Some(found) = release
                .get("tag_name")
                .and_then(serde_json::Value::as_str)
                .and_then(|tag| tag.strip_prefix('v'))
                .and_then(Release::parse)
            else {
                continue;
            };
            if found.line() == line && best.is_none_or(|best| found > best) {
                best = Some(found);
            }
        }
        if releases.len() < 100 {
            break;
        }
    }
    Ok(best)
}

/// The releases kept in the user's cache folder.
#[derive(Clone, Debug)]
pub(crate) struct Cache {
    /// `jadpo/releases` in the cache folder.
    pub(crate) folder: PathBuf,
    /// The target this `jadpo` runs on, whose archives it fetches.
    target: String,
}

impl Cache {
    /// The releases kept under `home`, the user's cache folder, for
    /// `target`.
    pub(crate) fn new(home: &Path, target: &str) -> Cache {
        Cache {
            folder: home.join("jadpo").join("releases"),
            target: target.to_string(),
        }
    }

    /// Where `release`'s `jadpo` is kept.
    pub(crate) fn binary(&self, release: Release) -> PathBuf {
        self.folder
            .join(release.to_string())
            .join(binary_name(&self.target))
    }

    /// Every release kept, newest first.
    pub(crate) fn held(&self) -> Vec<Release> {
        let mut held: Vec<Release> = std::fs::read_dir(&self.folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| Release::parse(entry.file_name().to_str()?))
            .filter(|release| self.binary(*release).is_file())
            .collect();
        held.sort_by(|a, b| b.cmp(a));
        held
    }

    /// The newest release of `line` kept.
    pub(crate) fn newest_of(&self, line: (u64, u64)) -> Option<Release> {
        self.held()
            .into_iter()
            .find(|release| release.line() == line)
    }

    /// The lock held while `release` is fetched or removed.
    fn lock(&self, release: Release) -> Result<std::fs::File, String> {
        let io = |error: std::io::Error| format!("`{}`: {error}.", self.folder.display());
        std::fs::create_dir_all(&self.folder).map_err(io)?;
        let lock = std::fs::File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.folder.join(format!(".{release}.lock")))
            .map_err(io)?;
        lock.lock().map_err(io)?;
        Ok(lock)
    }

    /// `release`'s `jadpo`, fetched from `network` when it isn't kept yet:
    /// its archive checked against the SHA-256 the release publishes and
    /// against `attest` before the binary is taken out. `report` is told
    /// how a fetch goes.
    pub(crate) fn install(
        &self,
        release: Release,
        network: &dyn Network,
        attest: &dyn Attest,
        report: &dyn Report,
    ) -> Result<PathBuf, Failure> {
        let unwritable = |why: String| (Unfetched::Uncached, why);
        let unverified = |why: String| (Unfetched::Unverified, why);
        let lock = self.lock(release).map_err(unwritable)?;
        let binary = self.binary(release);
        if binary.is_file() {
            return Ok(binary);
        }
        report.fetching(release);
        let name = archive_name(&self.target);
        let published = network
            .get(&download_url(release, &format!("{name}.sha256")), 4096)
            .map_err(unfetched)?;
        let expected = String::from_utf8_lossy(&published)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(unverified(format!(
                "its `{name}.sha256` doesn't hold a SHA-256."
            )));
        }
        let archive = network
            .get_reporting(
                &download_url(release, &name),
                LONGEST_ARCHIVE,
                &|done, total| report.progress(done, total),
            )
            .map_err(unfetched)?;
        report.progress(archive.len() as u64, Some(archive.len() as u64));
        let digest = sha256(&archive);
        if digest != expected {
            return Err(unverified(format!(
                "its download's SHA-256 is {digest}, not the {expected} the release publishes, so nothing in it was run."
            )));
        }
        attest.check(network, &release.to_string(), &digest)?;
        let wanted = binary_name(&self.target);
        let accepts = |entry: &str| member_is(entry, wanted);
        let bytes = if windows(&self.target) {
            zip_member(&archive, &accepts)
        } else {
            crate::toolchain::tar_gz_member(archive.as_slice(), &accepts)
        }
        .map_err(|why| unverified(format!("its archive doesn't unpack: {why}.")))?
        .ok_or_else(|| unverified(format!("its archive holds no `{wanted}`.")))?;
        let io = |error: std::io::Error| {
            (
                Unfetched::Uncached,
                format!("`{}`: {error}.", self.folder.display()),
            )
        };
        let staging = self
            .folder
            .join(format!(".{release}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(io)?;
        let staged = staging.join(wanted);
        std::fs::write(&staged, &bytes).map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))
                .map_err(io)?;
        }
        let placed = std::fs::rename(&staging, self.folder.join(release.to_string()));
        let _ = std::fs::remove_dir_all(&staging);
        placed.map_err(io)?;
        drop(lock);
        Ok(binary)
    }

    /// Removes `release`: whether it was kept.
    pub(crate) fn remove(&self, release: Release) -> Result<bool, String> {
        let lock = self.lock(release)?;
        let folder = self.folder.join(release.to_string());
        if !folder.exists() {
            return Ok(false);
        }
        std::fs::remove_dir_all(&folder)
            .map_err(|error| format!("`{}`: {error}.", folder.display()))?;
        drop(lock);
        Ok(true)
    }
}

/// Whether an archive's entry `name` is the binary `wanted`: at its top or
/// in the one folder dist puts the files in.
fn member_is(name: &str, wanted: &str) -> bool {
    let parts: Vec<&str> = name
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    parts.len() <= 2 && !parts.contains(&"..") && parts.last() == Some(&wanted)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The bytes of the first file in a zip archive whose name `wanted`
/// accepts, stored or deflated, its CRC-32 checked: how `jadpo.exe` is
/// taken out of a Windows archive. `None` when no entry is accepted.
fn zip_member(archive: &[u8], wanted: &dyn Fn(&str) -> bool) -> Result<Option<Vec<u8>>, String> {
    let short = || "the archive isn't a zip file".to_string();
    let u16_at = |at: usize| -> Option<u64> {
        Some(u64::from(u16::from_le_bytes(
            archive.get(at..at + 2)?.try_into().ok()?,
        )))
    };
    let u32_at = |at: usize| -> Option<u64> {
        Some(u64::from(u32::from_le_bytes(
            archive.get(at..at + 4)?.try_into().ok()?,
        )))
    };
    // The end of the central directory: its 22 bytes and a comment of at
    // most 64 KiB after them.
    let earliest = archive.len().saturating_sub(22 + 0xffff);
    let end = (earliest..=archive.len().saturating_sub(22))
        .rev()
        .find(|&at| archive.get(at..at + 4) == Some(&[0x50, 0x4b, 0x05, 0x06]))
        .ok_or_else(short)?;
    let count = u16_at(end + 10).ok_or_else(short)?;
    let mut at = usize::try_from(u32_at(end + 16).ok_or_else(short)?).map_err(|_| short())?;
    for _ in 0..count {
        if archive.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err(short());
        }
        let field = |offset: usize, size: usize| {
            if size == 2 {
                u16_at(at + offset)
            } else {
                u32_at(at + offset)
            }
        };
        let (method, crc, packed, size) = (
            field(10, 2).ok_or_else(short)?,
            field(16, 4).ok_or_else(short)?,
            field(20, 4).ok_or_else(short)?,
            field(24, 4).ok_or_else(short)?,
        );
        let (name_length, extra, comment, local) = (
            field(28, 2).ok_or_else(short)? as usize,
            field(30, 2).ok_or_else(short)? as usize,
            field(32, 2).ok_or_else(short)? as usize,
            field(42, 4).ok_or_else(short)? as usize,
        );
        let name = archive
            .get(at + 46..at + 46 + name_length)
            .ok_or_else(short)?;
        let name = String::from_utf8_lossy(name).replace('\\', "/");
        at += 46 + name_length + extra + comment;
        if name.ends_with('/') || !wanted(&name) {
            continue;
        }
        if archive.get(local..local + 4) != Some(&[0x50, 0x4b, 0x03, 0x04]) {
            return Err(short());
        }
        let start = local
            + 30
            + u16_at(local + 26).ok_or_else(short)? as usize
            + u16_at(local + 28).ok_or_else(short)? as usize;
        let data = archive
            .get(start..start + packed as usize)
            .ok_or_else(short)?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size as usize)
                .map_err(|_| format!("`{name}` in the archive doesn't inflate"))?,
            other => {
                return Err(format!(
                    "`{name}` in the archive is packed with method {other}"
                ));
            }
        };
        if bytes.len() as u64 != size
            || u64::from(jadpo_runtime::rows::gzip::crc32(0, &bytes)) != crc
        {
            return Err(format!("`{name}` in the archive is damaged"));
        }
        return Ok(Some(bytes));
    }
    Ok(None)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::toolchain::tests::{gzip, tar};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// GitHub as a test serves it: each URL's bytes, and every URL asked
    /// for. A URL it doesn't serve fails as a missing network would.
    #[derive(Default)]
    pub(crate) struct Served {
        pub(crate) files: BTreeMap<String, Vec<u8>>,
        pub(crate) asked: Mutex<Vec<String>>,
    }

    impl Network for Served {
        fn get(&self, url: &str, most: usize) -> Result<Vec<u8>, String> {
            self.asked.lock().expect("asked").push(url.to_string());
            match self.files.get(url) {
                Some(bytes) if bytes.len() <= most => Ok(bytes.clone()),
                Some(_) => Err("the answer is too long.".to_string()),
                None => Err("No connection to `github.com`: offline.".to_string()),
            }
        }
    }

    /// An attestation check that passes, or fails saying `why`.
    pub(crate) struct Attested(pub(crate) Option<&'static str>);

    impl Attest for Attested {
        fn check(&self, _: &dyn Network, _: &str, _: &str) -> Result<(), Failure> {
            match self.0 {
                None => Ok(()),
                Some(why) => Err((Unfetched::Unverified, why.to_string())),
            }
        }
    }

    /// A report that keeps the releases it was told were fetched.
    #[derive(Default)]
    pub(crate) struct Told(pub(crate) Mutex<Vec<Release>>);

    impl Report for Told {
        fn fetching(&self, release: Release) {
            self.0.lock().expect("told").push(release);
        }
        fn progress(&self, _: u64, _: Option<u64>) {}
    }

    /// The archive dist builds for `target`, holding `binary` as its
    /// `jadpo`, with a README beside it.
    pub(crate) fn archive(target: &str, binary: &[u8]) -> Vec<u8> {
        let folder = format!("jadpo-cli-{target}");
        gzip(&tar(&[
            (&format!("{folder}/README.md"), b"Jadpo", 0o644),
            (&format!("{folder}/jadpo"), binary, 0o755),
        ]))
    }

    /// A release `release` served for `target` with `binary` as its
    /// `jadpo`, its published SHA-256 `published` (the archive's own when
    /// `None`).
    pub(crate) fn serve(
        served: &mut Served,
        release: &str,
        target: &str,
        binary: &[u8],
        published: Option<&str>,
    ) {
        let name = archive_name(target);
        let bytes = archive(target, binary);
        let digest = published.map_or_else(|| sha256(&bytes), str::to_string);
        let base = format!("https://github.com/{REPOSITORY}/releases/download/v{release}");
        served.files.insert(
            format!("{base}/{name}.sha256"),
            format!("{digest}  {name}\n").into_bytes(),
        );
        served.files.insert(format!("{base}/{name}"), bytes);
    }

    /// GitHub's release list, holding `tags`, the last a draft.
    pub(crate) fn list(served: &mut Served, tags: &[&str], prerelease: &[&str]) {
        let entries: Vec<String> = tags
            .iter()
            .map(|tag| {
                format!(
                    r#"{{"tag_name":"{tag}","draft":false,"prerelease":{}}}"#,
                    prerelease.contains(tag)
                )
            })
            .chain([r#"{"tag_name":"v9.9.9","draft":true,"prerelease":false}"#.to_string()])
            .collect();
        served.files.insert(
            format!("https://api.github.com/repos/{REPOSITORY}/releases?per_page=100&page=1"),
            format!("[{}]", entries.join(",")).into_bytes(),
        );
    }

    const TARGET: &str = "aarch64-apple-darwin";

    fn cache() -> (tempdir::Dir, Cache) {
        let dir = tempdir::Dir::new();
        let cache = Cache::new(dir.path(), TARGET);
        (dir, cache)
    }

    #[test]
    fn a_release_reads_as_three_numbers() {
        assert_eq!(Release::parse("0.1.2"), Some(Release(0, 1, 2)));
        for text in ["0.1", "0.1.2.3", "01.1.2", "0.1.2-beta", "", "v0.1.2"] {
            assert_eq!(Release::parse(text), None, "{text}");
        }
        assert_eq!(Release(1, 10, 0).to_string(), "1.10.0");
        assert!(Release(0, 2, 0) > Release(0, 1, 9));
    }

    #[test]
    fn the_newest_published_release_of_a_line_is_chosen() {
        let mut served = Served::default();
        list(
            &mut served,
            &["v0.3.0", "v0.2.10", "v0.2.9", "v0.2.11", "v0.1.4"],
            &["v0.2.11"],
        );
        assert_eq!(newest(&served, (0, 2)), Ok(Some(Release(0, 2, 10))));
        assert_eq!(newest(&served, (0, 4)), Ok(None));
        assert_eq!(newest(&served, (9, 9)), Ok(None), "a draft isn't published");
        assert_eq!(
            newest(&Served::default(), (0, 2)).map_err(|(kind, _)| kind),
            Err(Unfetched::Unreachable)
        );
    }

    #[test]
    fn a_fetched_release_is_checked_and_kept() {
        let (_dir, cache) = cache();
        let mut served = Served::default();
        serve(&mut served, "0.2.3", TARGET, b"#!jadpo 0.2.3", None);
        let told = Told::default();
        let binary = cache
            .install(Release(0, 2, 3), &served, &Attested(None), &told)
            .expect("installs");
        assert_eq!(binary, cache.folder.join("0.2.3/jadpo"));
        assert_eq!(std::fs::read(&binary).expect("kept"), b"#!jadpo 0.2.3");
        assert_eq!(*told.0.lock().expect("told"), [Release(0, 2, 3)]);
        assert_eq!(cache.held(), [Release(0, 2, 3)]);
        assert_eq!(cache.newest_of((0, 2)), Some(Release(0, 2, 3)));
        // Kept, it is never fetched again.
        let asked = served.asked.lock().expect("asked").len();
        cache
            .install(
                Release(0, 2, 3),
                &Served::default(),
                &Attested(Some("no")),
                &Told::default(),
            )
            .expect("kept");
        assert_eq!(served.asked.lock().expect("asked").len(), asked);
        // Nothing but the release's folder and its lock is written.
        let names: Vec<String> = std::fs::read_dir(&cache.folder)
            .expect("the cache")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let mut names = names;
        names.sort();
        assert_eq!(names, [".0.2.3.lock", "0.2.3"]);
        assert!(cache.remove(Release(0, 2, 3)).expect("removes"));
        assert!(!cache.remove(Release(0, 2, 3)).expect("gone"));
        assert_eq!(cache.held(), []);
    }

    #[test]
    fn an_archive_that_isnt_the_published_one_is_refused() {
        let (_dir, cache) = cache();
        let mut served = Served::default();
        serve(
            &mut served,
            "0.2.3",
            TARGET,
            b"#!evil",
            Some(&"0".repeat(64)),
        );
        let (kind, refused) = cache
            .install(Release(0, 2, 3), &served, &Attested(None), &Told::default())
            .expect_err("refused");
        assert_eq!(kind, Unfetched::Unverified);
        assert!(refused.contains("SHA-256"), "{refused}");
        assert!(refused.contains("nothing in it was run"), "{refused}");
        assert_eq!(cache.held(), []);
        assert!(!cache.binary(Release(0, 2, 3)).exists());
    }

    #[test]
    fn an_archive_whose_attestation_fails_is_refused() {
        let (_dir, cache) = cache();
        let mut served = Served::default();
        serve(&mut served, "0.2.3", TARGET, b"#!jadpo", None);
        let refused = cache
            .install(
                Release(0, 2, 3),
                &served,
                &Attested(Some("not attested.")),
                &Told::default(),
            )
            .expect_err("refused");
        assert_eq!(
            refused,
            (Unfetched::Unverified, "not attested.".to_string())
        );
        assert_eq!(cache.held(), []);
    }

    #[test]
    fn two_runs_at_once_fetch_a_release_once() {
        let (_dir, cache) = cache();
        let mut served = Served::default();
        serve(&mut served, "0.2.3", TARGET, b"#!jadpo", None);
        let served = std::sync::Arc::new(served);
        let runs: Vec<_> = (0..4)
            .map(|_| {
                let (cache, served) = (cache.clone(), std::sync::Arc::clone(&served));
                std::thread::spawn(move || {
                    cache.install(
                        Release(0, 2, 3),
                        served.as_ref(),
                        &Attested(None),
                        &Told::default(),
                    )
                })
            })
            .collect();
        for run in runs {
            run.join().expect("ran").expect("installs");
        }
        let archives = served
            .asked
            .lock()
            .expect("asked")
            .iter()
            .filter(|url| url.ends_with(".tar.gz"))
            .count();
        assert_eq!(archives, 1);
    }

    #[test]
    fn a_windows_archive_is_read_as_a_zip() {
        // One stored and one deflated entry, as dist's zip may hold.
        let entries: [(&str, &[u8], u16); 2] = [
            ("README.md", b"Jadpo", 0),
            ("jadpo.exe", b"MZ jadpo for Windows", 8),
        ];
        let mut zip = Vec::new();
        let mut central = Vec::new();
        for (name, bytes, method) in entries {
            let packed = if method == 8 {
                miniz_oxide::deflate::compress_to_vec(bytes, 6)
            } else {
                bytes.to_vec()
            };
            let crc = jadpo_runtime::rows::gzip::crc32(0, bytes);
            let offset = zip.len() as u32;
            let common = |out: &mut Vec<u8>| {
                out.extend(20u16.to_le_bytes());
                out.extend(0u16.to_le_bytes());
                out.extend(method.to_le_bytes());
                out.extend([0; 4]);
                out.extend(crc.to_le_bytes());
                out.extend((packed.len() as u32).to_le_bytes());
                out.extend((bytes.len() as u32).to_le_bytes());
                out.extend((name.len() as u16).to_le_bytes());
                out.extend(0u16.to_le_bytes());
            };
            zip.extend([0x50, 0x4b, 0x03, 0x04]);
            common(&mut zip);
            zip.extend(name.as_bytes());
            zip.extend(&packed);
            central.extend([0x50, 0x4b, 0x01, 0x02]);
            central.extend(20u16.to_le_bytes());
            common(&mut central);
            // Its comment's length, disk, and internal and external
            // attributes.
            central.extend([0; 10]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let start = zip.len() as u32;
        zip.extend(&central);
        zip.extend([0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        zip.extend(2u16.to_le_bytes());
        zip.extend(2u16.to_le_bytes());
        zip.extend((central.len() as u32).to_le_bytes());
        zip.extend(start.to_le_bytes());
        zip.extend(0u16.to_le_bytes());
        let found = zip_member(&zip, &|name| member_is(name, "jadpo.exe")).expect("reads");
        assert_eq!(found.as_deref(), Some(&b"MZ jadpo for Windows"[..]));
        let readme = zip_member(&zip, &|name| name == "README.md").expect("reads");
        assert_eq!(readme.as_deref(), Some(&b"Jadpo"[..]));
        assert!(zip_member(b"not a zip", &|_| true).is_err());
    }

    #[test]
    fn only_the_binary_at_the_archives_top_is_taken() {
        assert!(member_is("jadpo-cli-x/jadpo", "jadpo"));
        assert!(member_is("jadpo", "jadpo"));
        assert!(!member_is("jadpo-cli-x/bin/jadpo", "jadpo"));
        assert!(!member_is("../jadpo", "jadpo"));
        assert!(!member_is("jadpo-cli-x/jadpo.md", "jadpo"));
    }

    /// A folder of its own for a test, removed after it.
    pub(crate) mod tempdir {
        use std::path::{Path, PathBuf};

        pub(crate) struct Dir(PathBuf);

        impl Dir {
            pub(crate) fn new() -> Dir {
                use std::sync::atomic::{AtomicU64, Ordering};
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let path = std::env::temp_dir().join(format!(
                    "jadpo-releases-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                let _ = std::fs::remove_dir_all(&path);
                std::fs::create_dir_all(&path).expect("a folder");
                Dir(path)
            }

            pub(crate) fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
