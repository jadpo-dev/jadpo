//! The lock, `jadpo.lock` (area 18; the `lock` record): each installed
//! package's version, digest and source, one line per package, sorted by
//! name and major version, so git's `union` merge combines two branches'
//! additions.
//!
//! ```text
//! package AcmeCouriers.v1 version 1.4.0 git github.com/acme/acme-couriers tag v1.4.0 commit 9c2f…a1 digest sha256:5be1…c0
//! package Postcodes.v2 version 2.1.0 registry digest sha256:e3b0…55
//! package Tables.v1 version 1.0.0 path ../tables digest sha256:0c1d…9e
//! package AcmePricing.v2 link ../acme-pricing
//! ```
//!
//! A source is `registry`, `git` (the repository, the tag and the tag's
//! commit), `path` or `link`; a link is a live folder, so it has no
//! version or digest of its own, and production builds and CI refuse it.
//! Packages come in chunk 7.1; this is the format they are locked in.
//!
//! The lock may also pin the exact Jadpo release the project runs with,
//! one `jadpo version 0.1.2` line before the packages, which `jadpo
//! toolchain pin` writes (area 14, "Which jadpo runs"; roadmap chunk D14).

use crate::record::{self, Entry};

/// The lock's path in a project; it keeps its usual name.
pub const LOCK: &str = "jadpo.lock";

/// What the lock's header says it is generated from.
const FROM: &str = "the installed packages";

/// One installed package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked {
    /// Its name and major version, `AcmeCouriers.v1`.
    pub package: String,
    pub source: Source,
}

/// Where an installed package came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Registry {
        version: String,
        digest: String,
    },
    Git {
        version: String,
        repository: String,
        tag: String,
        commit: String,
        digest: String,
    },
    Path {
        version: String,
        path: String,
        digest: String,
    },
    Link {
        path: String,
    },
}

impl Locked {
    /// The package's line.
    pub fn line(&self) -> String {
        let package = &self.package;
        match &self.source {
            Source::Registry { version, digest } => {
                format!("package {package} version {version} registry digest {digest}")
            }
            Source::Git {
                version,
                repository,
                tag,
                commit,
                digest,
            } => format!(
                "package {package} version {version} git {repository} tag {tag} commit {commit} digest {digest}"
            ),
            Source::Path {
                version,
                path,
                digest,
            } => format!("package {package} version {version} path {path} digest {digest}"),
            Source::Link { path } => format!("package {package} link {path}"),
        }
    }

    /// Its folder in a project: `packages/acme_pricing/v2` for
    /// `AcmePricing.v2`.
    pub fn folder(&self) -> String {
        package_folder(&self.package)
    }

    /// What it sorts by: the name, then the major version as numbers.
    fn order(&self) -> (String, Vec<u64>) {
        let (name, major) = split_package(&self.package).unwrap_or((&self.package, Vec::new()));
        (name.to_string(), major)
    }
}

/// The folder `packages/<name>/<version>` that holds `Name.vN`.
pub fn package_folder(package: &str) -> String {
    match package.rsplit_once('.') {
        Some((name, version)) => format!("{}/{}/{version}", crate::PACKAGES, crate::to_snake(name)),
        None => format!("{}/{package}", crate::PACKAGES),
    }
}

/// `AcmeCouriers.v1` as its name and major version, the version's groups
/// as numbers: `Stripe.v2024_06_20` is `[2024, 6, 20]` (area 11, question
/// 16).
fn split_package(package: &str) -> Option<(&str, Vec<u64>)> {
    let (name, label) = package.rsplit_once('.')?;
    let starts_upper = name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    let named = starts_upper && name.chars().all(|c| c.is_ascii_alphanumeric());
    if !named || !crate::places::is_version_label(label) {
        return None;
    }
    let groups: Option<Vec<u64>> = label[1..].split('_').map(|g| g.parse().ok()).collect();
    let groups = groups?;
    // `v01` would sort as `v1`: one spelling per version.
    let canonical = label[1..]
        .split('_')
        .next()
        .is_some_and(|first| first == "0" || !first.starts_with('0'));
    canonical.then_some((name, groups))
}

/// A package's digest (area 18, "The lock"): `sha256:` and the SHA-256,
/// in lower-case hex, of one line for each of its files, `<the SHA-256 of
/// its bytes>  <its path>`, each ended by a newline, the paths relative to
/// the package's folder with `/` and in byte order, as Go's `dirhash`
/// summarises a module. Hidden files are never part of a package.
pub fn package_digest(files: &[(&str, &[u8])]) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<&(&str, &[u8])> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut summary = String::new();
    for (path, bytes) in sorted {
        summary.push_str(&hex(&Sha256::digest(bytes)));
        summary.push_str("  ");
        summary.push_str(path);
        summary.push('\n');
    }
    format!("sha256:{}", hex(&Sha256::digest(summary.as_bytes())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The lock's text for `packages`: sorted by name and major version.
pub fn write_lock(packages: &[Locked]) -> String {
    write_pinned_lock(None, packages)
}

/// The lock's text pinning Jadpo `jadpo` (`0.1.2`), when it is given,
/// before `packages`.
pub fn write_pinned_lock(jadpo: Option<&str>, packages: &[Locked]) -> String {
    let mut sorted: Vec<&Locked> = packages.iter().collect();
    sorted.sort_by_key(|locked| locked.order());
    let entries: Vec<Entry> = jadpo
        .map(|version| Entry::new(format!("{JADPO} version {version}")))
        .into_iter()
        .chain(sorted.into_iter().map(|locked| Entry::new(locked.line())))
        .collect();
    record::write(&record::LOCK, FROM, &entries)
}

/// The word a pin's line starts with.
const JADPO: &str = "jadpo";

/// The Jadpo release the lock pins (`0.1.2`), with the line it is written
/// on (from 0), when its text has a pin that reads; a lock that doesn't
/// read is [`parse_lock`]'s to report.
pub fn locked_jadpo(text: &str) -> Option<(String, usize)> {
    text.lines().enumerate().skip(2).find_map(|(at, line)| {
        let version = pin(line)?.ok()?;
        Some((version.to_string(), at))
    })
}

/// A pin's version, or why the pin doesn't read; `None` for a line that
/// isn't a pin.
fn pin(line: &str) -> Option<Result<&str, String>> {
    let rest = line.strip_prefix(JADPO)?.strip_prefix(' ')?;
    Some(match rest.split(' ').collect::<Vec<_>>().as_slice() {
        ["version", version] if is_release(version) => Ok(version),
        ["version", version] => Err(format!(
            "`{version}` isn't a Jadpo release, such as `0.1.2`"
        )),
        _ => Err("a pin is `jadpo version <release>`".into()),
    })
}

/// A release's version: three numbers and no pre-release.
fn is_release(text: &str) -> bool {
    !text.contains('-') && is_version(text)
}

/// The lock's packages, or the line that can't be read (from 0) and why:
/// each line has to be one this Jadpo writes, and each package is locked
/// once.
pub fn parse_lock(text: &str) -> Result<Vec<Locked>, (usize, String)> {
    let text = record::normalise(text);
    record::read_header(&text, &record::LOCK, FROM)
        .map_err(|problem| (problem.line(), problem.describe(&record::LOCK)))?;
    record::read(&text, &record::LOCK)?;
    let mut out: Vec<(usize, Locked)> = Vec::new();
    let mut pinned: Option<usize> = None;
    for (at, line) in text.lines().enumerate().skip(2) {
        if line.is_empty() {
            continue;
        }
        if let Some(read) = pin(line) {
            read.map_err(|why| (at, why))?;
            if let Some(first) = pinned {
                return Err((
                    at,
                    format!(
                        "Jadpo is pinned on lines {} and {}: two pins, as a merge of two branches can leave",
                        first + 1,
                        at + 1
                    ),
                ));
            }
            pinned = Some(at);
            continue;
        }
        let locked = parse_line(line).map_err(|why| (at, why))?;
        if let Some((first, _)) = out
            .iter()
            .find(|(_, other)| other.package == locked.package)
        {
            return Err((
                at,
                format!(
                    "`{}` is locked on lines {} and {}: two versions of one package, as a merge of two branches can leave",
                    locked.package,
                    first + 1,
                    at + 1
                ),
            ));
        }
        out.push((at, locked));
    }
    Ok(out.into_iter().map(|(_, locked)| locked).collect())
}

fn parse_line(line: &str) -> Result<Locked, String> {
    let words: Vec<&str> = line.split(' ').collect();
    let shape = "a line is `package <Name>.v<major>` then its version and source";
    let (package, rest) = match words.as_slice() {
        ["package", package, rest @ ..] => (*package, rest),
        _ => return Err(shape.into()),
    };
    if split_package(package).is_none() {
        return Err(format!(
            "`{package}` isn't a package and major version, `<Name>.v<major>`"
        ));
    }
    let source = match rest {
        ["link", path] if !path.is_empty() => Source::Link {
            path: path.to_string(),
        },
        ["version", version, rest @ ..] => {
            if !is_version(version) {
                return Err(format!("`{version}` isn't a version, such as `1.4.0`"));
            }
            let digest = |digest: &str| -> Result<String, String> {
                is_digest(digest)
                    .then(|| digest.to_string())
                    .ok_or_else(|| {
                        format!("`{digest}` isn't a digest: `sha256:` and 64 lower-case hex digits")
                    })
            };
            let version = version.to_string();
            match rest {
                ["registry", "digest", d] => Source::Registry {
                    version,
                    digest: digest(d)?,
                },
                ["git", repository, "tag", tag, "commit", commit, "digest", d]
                    if !repository.is_empty() && !tag.is_empty() =>
                {
                    if !is_commit(commit) {
                        return Err(format!(
                            "`{commit}` isn't a commit: 40 or 64 lower-case hex digits"
                        ));
                    }
                    Source::Git {
                        version,
                        repository: repository.to_string(),
                        tag: tag.to_string(),
                        commit: commit.to_string(),
                        digest: digest(d)?,
                    }
                }
                ["path", path, "digest", d] if !path.is_empty() => Source::Path {
                    version,
                    path: path.to_string(),
                    digest: digest(d)?,
                },
                _ => return Err(shape.into()),
            }
        }
        _ => return Err(shape.into()),
    };
    Ok(Locked {
        package: package.to_string(),
        source,
    })
}

fn is_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && is_hex(hex))
}

fn is_commit(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64) && is_hex(text)
}

/// `1.4.0`, or with a pre-release, `2.0.0-beta.1`.
fn is_version(text: &str) -> bool {
    let (core, pre) = match text.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (text, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|n| {
            !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
                && (*n == "0" || !n.starts_with('0'))
        })
        && pre.is_none_or(|pre| {
            !pre.is_empty()
                && pre
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_comes_first_and_reads_back() {
        let text = write_pinned_lock(Some("0.1.2"), &packages());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[3], "jadpo version 0.1.2", "{text}");
        assert_eq!(locked_jadpo(&text), Some(("0.1.2".to_string(), 3)));
        assert_eq!(parse_lock(&text).expect("reads").len(), packages().len());
        assert_eq!(locked_jadpo(&write_lock(&packages())), None);
    }

    #[test]
    fn a_pin_is_one_release() {
        let base = write_pinned_lock(Some("0.1.2"), &[]);
        let bad = base.replace("0.1.2", "0.1");
        assert!(
            parse_lock(&bad)
                .expect_err("not a release")
                .1
                .contains("`0.1`")
        );
        let twice = format!("{base}jadpo version 0.1.3\n");
        assert!(
            parse_lock(&twice)
                .expect_err("two pins")
                .1
                .contains("two pins")
        );
        assert_eq!(locked_jadpo(&bad), None);
    }

    fn digest(c: char) -> String {
        format!("sha256:{}", c.to_string().repeat(64))
    }

    fn packages() -> Vec<Locked> {
        vec![
            Locked {
                package: "Postcodes.v2".into(),
                source: Source::Registry {
                    version: "2.1.0".into(),
                    digest: digest('e'),
                },
            },
            Locked {
                package: "AcmeCouriers.v1".into(),
                source: Source::Git {
                    version: "1.4.0".into(),
                    repository: "github.com/acme/acme-couriers".into(),
                    tag: "v1.4.0".into(),
                    commit: "9".repeat(40),
                    digest: digest('5'),
                },
            },
            Locked {
                package: "AcmePricing.v10".into(),
                source: Source::Link {
                    path: "../acme-pricing".into(),
                },
            },
            Locked {
                package: "AcmePricing.v2".into(),
                source: Source::Path {
                    version: "2.0.0-beta.1".into(),
                    path: "../pricing".into(),
                    digest: digest('0'),
                },
            },
        ]
    }

    #[test]
    fn the_lock_is_a_ledger_of_one_line_per_package() {
        let text = write_lock(&packages());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "// Generated by Jadpo from the installed packages; update it with `jadpo package add`, `update` or `remove`, never by hand."
        );
        assert_eq!(lines[1], "record lock 1");
        assert_eq!(lines[2], "");
        assert_eq!(
            lines[3],
            format!(
                "package AcmeCouriers.v1 version 1.4.0 git github.com/acme/acme-couriers tag v1.4.0 commit {} digest {}",
                "9".repeat(40),
                digest('5')
            )
        );
        // By name, then major version as a number.
        assert!(
            lines[4]
                .starts_with("package AcmePricing.v2 version 2.0.0-beta.1 path ../pricing digest")
        );
        assert_eq!(lines[5], "package AcmePricing.v10 link ../acme-pricing");
        assert!(lines[6].starts_with("package Postcodes.v2 version 2.1.0 registry digest sha256:"));
        let mut sorted = packages();
        sorted.sort_by_key(Locked::order);
        assert_eq!(parse_lock(&text), Ok(sorted));
    }

    #[test]
    fn a_lock_line_jadpo_wouldnt_write_is_refused() {
        let text = write_lock(&packages());
        let twice = format!("{text}package Postcodes.v2 link ../postcodes\n");
        assert_eq!(parse_lock(&twice).map_err(|(at, _)| at), Err(7));
        let short = text.replace(&digest('e'), "sha256:e3b0");
        assert_eq!(parse_lock(&short).map_err(|(at, _)| at), Err(6));
        assert!(parse_lock(&text.replace("version 1.4.0", "version 1.4")).is_err());
        assert!(parse_lock(&text.replace("AcmeCouriers.v1", "acme_couriers")).is_err());
        assert!(parse_lock(&text.replace(" tag v1.4.0", "")).is_err());
        assert_eq!(parse_lock(&write_lock(&[])), Ok(Vec::new()));
    }
}
