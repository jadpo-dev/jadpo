//! The software types (area 19, "Web and software", chunk 5.5n2): each
//! held as its canonical text and checked by a pattern and, where a
//! pattern can't say it, a named validator here; or held as an object of
//! parts (`SemanticVersion`) or an enum (`LogLevel`). The governing
//! standards are cited at each: SemVer 2.0.0 for versions, npm's
//! node-semver range grammar for ranges, `git check-ref-format` for ref
//! names, ECMA-427 (the purl specification) for package URLs, the CVE
//! programme's ID syntax, the SPDX licence list and expression syntax
//! (SPDX 2.3 Annex D), W3C Trace Context for trace ids, the OCI
//! distribution reference grammar and the OCI image specification's digest
//! grammar, the ULID specification, and the DOI Handbook. As everywhere,
//! only the canonical text is read.
//!
//! The function `semver.satisfies` runs is here too, beside the version
//! grammar it shares with the validators.

use std::cmp::Ordering;

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm, Validator,
};
use crate::read::SqlType;

// ---------------------------------------------------------------------
// SemVer 2.0.0
// ---------------------------------------------------------------------

/// `SemanticVersion` (area 19, "Web and software").
pub const SEMANTIC_VERSION: &str = "SemanticVersion";

/// A pre-release identifier (SemVer 2.0.0 §9): a numeric identifier with
/// no leading zero, or an alphanumeric one with at least one non-digit.
macro_rules! pre_identifier {
    () => {
        r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z\-][0-9A-Za-z\-]*)"
    };
}

/// A version's pre-release (SemVer 2.0.0 §9): its identifiers joined by
/// dots, `alpha.1`.
pub const PRE_RELEASE_PATTERN: &str = concat!(pre_identifier!(), r"(?:\.", pre_identifier!(), ")*");

/// A version's build metadata (SemVer 2.0.0 §10): identifiers of ASCII
/// letters, digits and hyphens joined by dots, leading zeros allowed.
pub const BUILD_PATTERN: &str = r"[0-9A-Za-z\-]+(?:\.[0-9A-Za-z\-]+)*";

/// `SemanticVersion`'s parts, in order: its three numbers, each from 0 up,
/// and its pre-release and build metadata, each where it has one.
pub const SEMANTIC_VERSION_FIELDS: [(&str, Part); 5] = [
    ("major", Part::Whole(0, i64::MAX)),
    ("minor", Part::Whole(0, i64::MAX)),
    ("patch", Part::Whole(0, i64::MAX)),
    (
        "pre_release",
        Part::Optional(&Part::Matching(PRE_RELEASE_PATTERN)),
    ),
    ("build", Part::Optional(&Part::Matching(BUILD_PATTERN))),
];

/// A version read from its text (SemVer 2.0.0 §2, §9, §10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version<'a> {
    pub major: i64,
    pub minor: i64,
    pub patch: i64,
    pub pre_release: Option<&'a str>,
    pub build: Option<&'a str>,
}

/// A numeric identifier (SemVer 2.0.0 §2): `0`, or digits with no leading
/// zero, within an `Int`.
fn numeric(text: &str) -> Option<i64> {
    let plain = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    plain.then(|| text.parse::<i64>().ok()).flatten()
}

/// Whether `text` is a version's pre-release.
pub fn is_pre_release(text: &str) -> bool {
    !text.is_empty() && text.split('.').all(pre_release_identifier)
}

fn pre_release_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && (!text.bytes().all(|b| b.is_ascii_digit()) || text == "0" || !text.starts_with('0'))
}

/// Whether `text` is a version's build metadata.
pub fn is_build(text: &str) -> bool {
    !text.is_empty()
        && text.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// A version from SemVer 2.0.0's text, `1.2.3-alpha.1+build.5`, with no
/// `v` before it; `None` for anything else, a number past an `Int`
/// included.
pub fn read_version(text: &str) -> Option<Version<'_>> {
    let (rest, build) = match text.split_once('+') {
        Some((rest, build)) if is_build(build) => (rest, Some(build)),
        Some(_) => return None,
        None => (text, None),
    };
    let (core, pre_release) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(Some(pre).filter(|p| is_pre_release(p))?)),
        None => (rest, None),
    };
    let mut numbers = core.split('.');
    let (major, minor, patch) = (numbers.next()?, numbers.next()?, numbers.next()?);
    if numbers.next().is_some() {
        return None;
    }
    Some(Version {
        major: numeric(major)?,
        minor: numeric(minor)?,
        patch: numeric(patch)?,
        pre_release,
        build,
    })
}

/// A version's canonical text.
pub fn version_text(version: &Version<'_>) -> String {
    let mut out = format!("{}.{}.{}", version.major, version.minor, version.patch);
    if let Some(pre) = version.pre_release {
        out.push('-');
        out.push_str(pre);
    }
    if let Some(build) = version.build {
        out.push('+');
        out.push_str(build);
    }
    out
}

/// Two pre-release identifiers by SemVer 2.0.0 §11.4: numeric ones
/// numerically, alphanumeric ones in ASCII order, numeric before
/// alphanumeric.
fn identifier_order(a: &str, b: &str) -> Ordering {
    let digits = |text: &str| text.bytes().all(|b| b.is_ascii_digit());
    match (digits(a), digits(b)) {
        // No leading zeros, so the longer is the larger.
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

/// Two pre-releases by SemVer 2.0.0 §11.3 and §11.4: none is above any;
/// otherwise identifier by identifier, a longer list above a shorter one
/// it starts with.
fn pre_release_order(a: Option<&str>, b: Option<&str>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => {
            let (mut a, mut b) = (a.split('.'), b.split('.'));
            loop {
                match (a.next(), b.next()) {
                    (None, None) => return Ordering::Equal,
                    (None, Some(_)) => return Ordering::Less,
                    (Some(_), None) => return Ordering::Greater,
                    (Some(x), Some(y)) => match identifier_order(x, y) {
                        Ordering::Equal => {}
                        other => return other,
                    },
                }
            }
        }
    }
}

/// Two versions' precedence (SemVer 2.0.0 §11): major, minor and patch
/// numerically, then the pre-release; build metadata is ignored, so
/// `1.0.0+a` and `1.0.0+b` have the same precedence.
pub fn precedence(a: &Version<'_>, b: &Version<'_>) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| pre_release_order(a.pre_release, b.pre_release))
}

// ---------------------------------------------------------------------
// Version ranges: node-semver's grammar, in one spelling
// ---------------------------------------------------------------------

/// One part of a partial version: a number, or `x` for any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part3 {
    Number(i64),
    Any,
}

/// A partial version (node-semver's `partial`): one to three numbers, a
/// trailing run of them `x` or left out, and a pre-release only after
/// three numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Partial<'a> {
    major: Option<i64>,
    minor: Option<i64>,
    patch: Option<i64>,
    pre_release: Option<&'a str>,
}

/// A version bound: `(major, minor, patch, pre-release)`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Bound {
    major: i64,
    minor: i64,
    patch: i64,
    pre_release: Option<String>,
}

impl Bound {
    fn new(major: i64, minor: i64, patch: i64, pre_release: Option<&str>) -> Bound {
        Bound {
            major,
            minor,
            patch,
            pre_release: pre_release.map(str::to_string),
        }
    }

    /// The first_of version of `major.minor.patch`, its `-0` pre-release,
    /// which node-semver writes for an excluded upper end.
    fn first_of(major: i64, minor: i64, patch: i64) -> Bound {
        Bound::new(major, minor, patch, Some("0"))
    }

    fn version(&self) -> Version<'_> {
        Version {
            major: self.major,
            minor: self.minor,
            patch: self.patch,
            pre_release: self.pre_release.as_deref(),
            build: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Less,
    AtMost,
    Greater,
    AtLeast,
    Equal,
}

/// One comparator of a desugared range.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Comparator {
    op: Op,
    bound: Bound,
}

/// A range desugared as node-semver desugars one: sets joined by `||`,
/// each a list of comparators all of which a version meets; an empty set
/// is `*`.
type Sets = Vec<Vec<Comparator>>;

fn partial(text: &str) -> Option<Partial<'_>> {
    let (core, pre_release) = match text.split_once('-') {
        Some((core, pre)) => (core, Some(Some(pre).filter(|p| is_pre_release(p))?)),
        None => (text, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut read = Vec::with_capacity(3);
    for part in &parts {
        read.push(if *part == "x" {
            Part3::Any
        } else {
            Part3::Number(numeric(part)?)
        });
    }
    // A wildcard starts a range only alone (`*`, below), and once a part
    // is `x` every part after it is left out.
    if read.first() == Some(&Part3::Any) {
        return None;
    }
    if let Some(at) = read.iter().position(|p| *p == Part3::Any)
        && at + 1 != read.len()
    {
        return None;
    }
    let number = |at: usize| match read.get(at) {
        Some(Part3::Number(n)) => Some(*n),
        _ => None,
    };
    let full = read.len() == 3 && !read.contains(&Part3::Any);
    if pre_release.is_some() && !full {
        return None;
    }
    Some(Partial {
        major: number(0),
        minor: number(1),
        patch: number(2),
        pre_release,
    })
}

/// `n + 1`, refusing a number past an `Int`.
fn next(n: i64) -> Option<i64> {
    n.checked_add(1)
}

/// A simple range's comparators (node-semver's `simple`): a comparison, a
/// tilde, a caret, or a partial version alone.
fn simple(text: &str) -> Option<Vec<Comparator>> {
    use Op::*;
    let at_least = |b: Bound| Comparator {
        op: AtLeast,
        bound: b,
    };
    let less = |b: Bound| Comparator { op: Less, bound: b };
    if text == "*" {
        return Some(Vec::new());
    }
    if let Some(rest) = text.strip_prefix('~') {
        let p = partial(rest)?;
        let major = p.major?;
        return Some(match (p.minor, p.patch) {
            (Some(minor), Some(patch)) => vec![
                at_least(Bound::new(major, minor, patch, p.pre_release)),
                less(Bound::first_of(major, next(minor)?, 0)),
            ],
            (Some(minor), None) => vec![
                at_least(Bound::new(major, minor, 0, None)),
                less(Bound::first_of(major, next(minor)?, 0)),
            ],
            _ => vec![
                at_least(Bound::new(major, 0, 0, None)),
                less(Bound::first_of(next(major)?, 0, 0)),
            ],
        });
    }
    if let Some(rest) = text.strip_prefix('^') {
        let p = partial(rest)?;
        let major = p.major?;
        // The first number that isn't zero may not grow (node-semver's
        // `replaceCaret`).
        let upper = match (p.minor, p.patch) {
            (None, _) => Bound::first_of(next(major)?, 0, 0),
            (Some(minor), None) if major == 0 => Bound::first_of(0, next(minor)?, 0),
            (Some(0), Some(patch)) if major == 0 => Bound::first_of(0, 0, next(patch)?),
            (Some(minor), Some(_)) if major == 0 => Bound::first_of(0, next(minor)?, 0),
            _ => Bound::first_of(next(major)?, 0, 0),
        };
        let lower = Bound::new(
            major,
            p.minor.unwrap_or(0),
            p.patch.unwrap_or(0),
            p.pre_release,
        );
        return Some(vec![at_least(lower), less(upper)]);
    }
    let (op, rest) = [(">=", AtLeast), ("<=", AtMost), (">", Greater), ("<", Less)]
        .into_iter()
        .find_map(|(sign, op)| text.strip_prefix(sign).map(|rest| (Some(op), rest)))
        .unwrap_or((None, text));
    let p = partial(rest)?;
    let major = p.major?;
    if let (Some(minor), Some(patch)) = (p.minor, p.patch) {
        let bound = Bound::new(major, minor, patch, p.pre_release);
        return Some(vec![Comparator {
            op: op.unwrap_or(Equal),
            bound,
        }]);
    }
    // A partial: its first missing part is any.
    let (low, high) = match p.minor {
        Some(minor) => (
            Bound::new(major, minor, 0, None),
            Bound::first_of(major, next(minor)?, 0),
        ),
        None => (
            Bound::new(major, 0, 0, None),
            Bound::first_of(next(major)?, 0, 0),
        ),
    };
    Some(match op {
        None => vec![at_least(low), less(high)],
        Some(AtLeast) => vec![at_least(low)],
        Some(Less) => vec![less(Bound::first_of(low.major, low.minor, low.patch))],
        // `>1.2` is `>=1.3.0`, as node-semver has it.
        Some(Greater) => vec![at_least(Bound::new(
            high.major, high.minor, high.patch, None,
        ))],
        Some(AtMost | Equal) => vec![less(high)],
    })
}

/// A hyphen range's comparators (node-semver's `hyphen`): from the first
/// partial's first_of version to the second's highest, both included.
fn hyphen(low: &str, high: &str) -> Option<Vec<Comparator>> {
    let (low, high) = (partial(low)?, partial(high)?);
    let lower = Bound::new(
        low.major?,
        low.minor.unwrap_or(0),
        low.patch.unwrap_or(0),
        low.pre_release,
    );
    let major = high.major?;
    let upper = match (high.minor, high.patch) {
        (Some(minor), Some(patch)) => Comparator {
            op: Op::AtMost,
            bound: Bound::new(major, minor, patch, high.pre_release),
        },
        (Some(minor), None) => Comparator {
            op: Op::Less,
            bound: Bound::first_of(major, next(minor)?, 0),
        },
        _ => Comparator {
            op: Op::Less,
            bound: Bound::first_of(next(major)?, 0, 0),
        },
    };
    Some(vec![
        Comparator {
            op: Op::AtLeast,
            bound: lower,
        },
        upper,
    ])
}

/// A range read from its canonical text: node-semver's grammar in one
/// spelling, sets joined by ` || `, a set's simple ranges by one space, a
/// hyphen range as ` - `, no `v` or `=` before a version, a wildcard
/// written `x` (`1.2.x`) or, for any version, `*`, and no build metadata.
fn read_range(text: &str) -> Option<Sets> {
    if text.is_empty() {
        return None;
    }
    text.split(" || ")
        .map(|set| {
            if set.is_empty() {
                return None;
            }
            let words: Vec<&str> = set.split(' ').collect();
            match words.as_slice() {
                [low, "-", high] => hyphen(low, high),
                _ => {
                    let mut all = Vec::new();
                    for word in &words {
                        if word.is_empty() || *word == "-" || (*word == "*" && words.len() > 1) {
                            return None;
                        }
                        all.extend(simple(word)?);
                    }
                    Some(all)
                }
            }
        })
        .collect()
}

/// Whether `text` is a `VersionRange`.
pub fn is_version_range(text: &str) -> bool {
    read_range(text).is_some()
}

fn meets(version: &Version<'_>, comparator: &Comparator) -> bool {
    let order = precedence(version, &comparator.bound.version());
    match comparator.op {
        Op::Less => order == Ordering::Less,
        Op::AtMost => order != Ordering::Greater,
        Op::Greater => order == Ordering::Greater,
        Op::AtLeast => order != Ordering::Less,
        Op::Equal => order == Ordering::Equal,
    }
}

/// Whether `version` is inside `range` (`semver.satisfies`): inside one of
/// its sets, meeting every comparator of it; and, for a version with a
/// pre-release, only where one of that set's comparators names a
/// pre-release of the same major, minor and patch, as node-semver and
/// Cargo have it, so `^1.2.3` never takes `2.0.0-beta`, nor `>=1.0.0`
/// `1.5.0-rc.1`. `None` where the range isn't one.
pub fn satisfies(version: &Version<'_>, range: &str) -> Option<bool> {
    let sets = read_range(range)?;
    Some(sets.iter().any(|set| {
        set.iter().all(|comparator| meets(version, comparator))
            && (version.pre_release.is_none()
                || set.iter().any(|comparator| {
                    let bound = &comparator.bound;
                    bound.pre_release.is_some()
                        && (bound.major, bound.minor, bound.patch)
                            == (version.major, version.minor, version.patch)
                }))
    }))
}

// ---------------------------------------------------------------------
// Git ref names
// ---------------------------------------------------------------------

/// A ref name as `git check-ref-format --allow-onelevel` accepts it (the
/// command's ten rules, cited by number), so a branch (`main`), a tag
/// (`v1.2.0`), a full ref (`refs/heads/main`) and a commit's full hash are
/// each one.
pub fn is_git_ref(text: &str) -> bool {
    // 9: not `@` alone. 6: no slash at either end and none doubled.
    if text.is_empty() || text == "@" || text.starts_with('/') || text.ends_with('/') {
        return false;
    }
    // 7: no dot at the end. 3: no `..`. 8: no `@{`.
    if text.ends_with('.') || text.contains("..") || text.contains("@{") {
        return false;
    }
    // 4, 5, 10: no control character, space, `~`, `^`, `:`, `?`, `*`,
    // `[` or `\`.
    if text.chars().any(|c| {
        c.is_ascii_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    }) {
        return false;
    }
    // 1, 6: each component not empty, not starting with a dot, not ending
    // `.lock`.
    text.split('/').all(|component| {
        !component.is_empty() && !component.starts_with('.') && !component.ends_with(".lock")
    })
}

// ---------------------------------------------------------------------
// Package URLs (ECMA-427)
// ---------------------------------------------------------------------

/// A character a component keeps as it is (ECMA-427 §5.4): the
/// alphanumerics, `.-_~`, and `:` wherever it is.
fn purl_plain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'~' | b':')
}

/// A component percent-encoded as ECMA-427 §5.4 has it, decoded: every
/// other byte as `%` and two upper-case hexadecimal digits, and none of
/// those kept as they are encoded; `None` for anything else, or bytes that
/// aren't UTF-8.
fn purl_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = |offset: usize| {
                bytes
                    .get(at + offset)
                    .filter(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b))
                    .and_then(|b| char::from(*b).to_digit(16))
            };
            let value = u8::try_from(hex(1)? * 16 + hex(2)?).ok()?;
            if purl_plain(value) {
                return None;
            }
            out.push(value);
            at += 3;
        } else if purl_plain(byte) {
            out.push(byte);
            at += 1;
        } else {
            return None;
        }
    }
    String::from_utf8(out).ok()
}

/// A package URL in ECMA-427's canonical form (§5.6, and "How to build a
/// PURL"): `pkg:`, a lower-case type, its namespace's segments and its
/// name, a version after `@`, qualifiers after `?` with lower-case keys in
/// order, each once and none empty, and a subpath after `#` with no `.`,
/// `..` or empty segment; each component percent-encoded only where
/// §5.4 says. A registered type's own rules aren't checked.
pub fn is_package_url(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("pkg:") else {
        return false;
    };
    let (rest, subpath) = match rest.split_once('#') {
        Some((rest, subpath)) => (rest, Some(subpath)),
        None => (rest, None),
    };
    let (rest, qualifiers) = match rest.split_once('?') {
        Some((rest, qualifiers)) => (rest, Some(qualifiers)),
        None => (rest, None),
    };
    let (rest, version) = match rest.split_once('@') {
        Some((rest, version)) => (rest, Some(version)),
        None => (rest, None),
    };
    let Some((kind, path)) = rest.split_once('/') else {
        return false;
    };
    let kind_ok = kind.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'));
    // Namespace segments and the name, none empty and none with a `/`
    // once decoded but the name.
    let segments: Vec<&str> = path.split('/').collect();
    let Some((name, namespace)) = segments.split_last() else {
        return false;
    };
    let segment_ok =
        |segment: &str| purl_decoded(segment).is_some_and(|d| !d.is_empty() && !d.contains('/'));
    let path_ok = namespace.iter().all(|s| segment_ok(s))
        && purl_decoded(name).is_some_and(|d| !d.is_empty());
    let version_ok = version.is_none_or(|v| purl_decoded(v).is_some_and(|d| !d.is_empty()));
    let qualifiers_ok = qualifiers.is_none_or(|qualifiers| {
        let mut last: Option<&str> = None;
        qualifiers.split('&').all(|pair| {
            let Some((key, value)) = pair.split_once('=') else {
                return false;
            };
            let key_ok = key.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
                && key.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
                });
            // In order, each once: the qualifiers sort as their text does.
            let ordered =
                last.is_none_or(|before| before < pair && before.split('=').next() != Some(key));
            last = Some(pair);
            key_ok && ordered && purl_decoded(value).is_some_and(|d| !d.is_empty())
        })
    });
    let subpath_ok = subpath.is_none_or(|subpath| {
        subpath.split('/').all(|segment| {
            purl_decoded(segment)
                .is_some_and(|d| !d.is_empty() && d != "." && d != ".." && !d.contains('/'))
        })
    });
    kind_ok && path_ok && version_ok && qualifiers_ok && subpath_ok
}

// ---------------------------------------------------------------------
// SPDX licence expressions
// ---------------------------------------------------------------------

/// The SPDX licence list release whose identifiers `SpdxLicence` reads:
/// the `spdx` crate's bundled copy (CC0 data).
pub fn spdx_list_version() -> &'static str {
    spdx::license_version()
}

/// An SPDX `idstring`: letters, digits, `-` and `.`.
fn idstring(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
}

/// A `license-ref`: `LicenseRef-` and an idstring, after
/// `DocumentRef-<idstring>:` where given.
fn licence_ref(text: &str) -> bool {
    let local = match text.split_once(':') {
        Some((document, local)) => match document.strip_prefix("DocumentRef-") {
            Some(id) if idstring(id) => local,
            _ => return false,
        },
        None => text,
    };
    local.strip_prefix("LicenseRef-").is_some_and(idstring)
}

/// A listed licence's identifier, exactly as the list spells it. The
/// crate also lists `NOASSERTION`, a document's value for "not stated",
/// which isn't a licence.
fn listed(id: &str) -> bool {
    id != "NOASSERTION" && spdx::license_id(id).is_some_and(|l| l.name == id)
}

/// A `simple-expression`: a listed licence's identifier as the list spells
/// it, with `+` for "or later", or a licence reference.
fn simple_licence(text: &str) -> bool {
    match text.strip_suffix('+') {
        Some(id) => !id.ends_with('+') && listed(id),
        None => listed(text) || licence_ref(text),
    }
}

/// Splits an SPDX expression into its tokens: `(`, `)` and words.
fn spdx_tokens(text: &str) -> Option<Vec<&str>> {
    let mut tokens = Vec::new();
    for word in text.split(' ') {
        let (mut start, mut end) = (0, word.len());
        let bytes = word.as_bytes();
        while bytes.get(start) == Some(&b'(') {
            tokens.push("(");
            start += 1;
        }
        let mut closing = 0;
        while end > start && bytes.get(end - 1) == Some(&b')') {
            closing += 1;
            end -= 1;
        }
        if start == end {
            return None;
        }
        tokens.push(word.get(start..end)?);
        tokens.extend(std::iter::repeat_n(")", closing));
    }
    Some(tokens)
}

/// SPDX 2.3 Annex D's grammar, `OR` binding loosest and `WITH` tightest.
struct Expression<'t> {
    tokens: &'t [&'t str],
    at: usize,
}

impl<'t> Expression<'t> {
    fn peek(&self) -> Option<&'t str> {
        self.tokens.get(self.at).copied()
    }

    fn or(&mut self) -> bool {
        if !self.and() {
            return false;
        }
        while self.peek() == Some("OR") {
            self.at += 1;
            if !self.and() {
                return false;
            }
        }
        true
    }

    fn and(&mut self) -> bool {
        if !self.with() {
            return false;
        }
        while self.peek() == Some("AND") {
            self.at += 1;
            if !self.with() {
                return false;
            }
        }
        true
    }

    fn with(&mut self) -> bool {
        match self.peek() {
            Some("(") => {
                self.at += 1;
                if !self.or() || self.peek() != Some(")") {
                    return false;
                }
                self.at += 1;
                true
            }
            Some(word) if !matches!(word, "AND" | "OR" | "WITH" | ")") && simple_licence(word) => {
                self.at += 1;
                if self.peek() == Some("WITH") {
                    self.at += 1;
                    let Some(exception) = self.peek() else {
                        return false;
                    };
                    self.at += 1;
                    return spdx::exception_id(exception).is_some_and(|e| e.name == exception);
                }
                true
            }
            _ => false,
        }
    }
}

/// An SPDX licence expression in its canonical spelling: each identifier
/// as the SPDX licence list spells it (deprecated ones included, since
/// they still name a licence), `AND`, `OR` and `WITH` in upper case, one
/// space between words and none inside parentheses.
pub fn is_spdx_licence(text: &str) -> bool {
    let Some(tokens) = spdx_tokens(text) else {
        return false;
    };
    let mut expression = Expression {
        tokens: &tokens,
        at: 0,
    };
    expression.or() && expression.at == tokens.len()
}

// ---------------------------------------------------------------------
// W3C Trace Context
// ---------------------------------------------------------------------

/// Lower-case hexadecimal digits, not all zero, as W3C Trace Context's
/// trace and parent ids are (§3.2.2.3, §3.2.2.4).
pub fn is_trace_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && text.bytes().any(|b| b != b'0')
}

/// A `traceparent` of version `00` (W3C Trace Context §3.2): `00`, a trace
/// id, a parent id and the flags, joined by `-`, each in lower-case
/// hexadecimal, neither id all zero.
pub fn is_trace_parent(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let hex = |part: &str| {
        part.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    matches!(parts.as_slice(), [version, trace, parent, flags]
        if *version == "00"
            && trace.len() == 32
            && is_trace_hex(trace)
            && parent.len() == 16
            && is_trace_hex(parent)
            && flags.len() == 2
            && hex(flags))
}

// ---------------------------------------------------------------------
// Files: portable names and paths
// ---------------------------------------------------------------------

/// Windows' reserved device names, which name a device whatever extension
/// follows (Microsoft's "Naming Files, Paths, and Namespaces").
const RESERVED_NAMES: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

/// The most bytes a file name has: 255, what ext4, APFS and NTFS each
/// allow at least.
pub const FILE_NAME_MAX: usize = 255;

/// The most bytes a path has: 4096, Linux's `PATH_MAX`.
pub const FILE_PATH_MAX: usize = 4096;

/// A file name every common file system takes: 1 to 255 bytes of UTF-8,
/// no `/`, `\`, NUL or other control character, none of Windows' `<>:"|?*`,
/// not `.` or `..`, no space or dot at the end (Windows drops them), and
/// not one of Windows' device names (`CON`, `nul.txt`, `COM1`), in any
/// case.
pub fn is_file_name(text: &str) -> bool {
    if text.is_empty() || text.len() > FILE_NAME_MAX || text == "." || text == ".." {
        return false;
    }
    if text.ends_with(' ') || text.ends_with('.') {
        return false;
    }
    if text.chars().any(|c| {
        c.is_control() || matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*')
    }) {
        return false;
    }
    let stem = text
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    let numbered = ["COM", "LPT"].iter().any(|device| {
        upper.strip_prefix(device).is_some_and(|n| {
            matches!(
                n,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    });
    !RESERVED_NAMES.contains(&upper.as_str()) && !numbered
}

/// A portable path, written with `/` as POSIX and URLs write one: `/` for
/// the root, or names joined by `/`, absolute after a leading `/` or
/// relative without one, each a [`is_file_name`], and in its shortest
/// form: no `.` segment, `..` only at the start of a relative path, no
/// empty segment and no `/` at the end; at most 4096 bytes.
pub fn is_file_path(text: &str) -> bool {
    if text.is_empty() || text.len() > FILE_PATH_MAX {
        return false;
    }
    if text == "/" {
        return true;
    }
    let (absolute, rest) = match text.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let mut climbing = !absolute;
    rest.split('/').all(|segment| {
        if segment == ".." {
            climbing
        } else {
            climbing = false;
            is_file_name(segment)
        }
    })
}

// ---------------------------------------------------------------------
// Container images and digests (OCI)
// ---------------------------------------------------------------------

/// A digest by the OCI image specification's grammar ("Digests"): a
/// lower-case algorithm, `:` and its encoded part; a registered algorithm's
/// part as it registers it, `sha256` and `blake3` 64 and `sha512` 128
/// lower-case hexadecimal digits. An unregistered algorithm passes when it
/// meets the grammar, as the specification asks.
pub fn is_digest(text: &str) -> bool {
    let Some((algorithm, encoded)) = text.split_once(':') else {
        return false;
    };
    let component = |c: &str| {
        !c.is_empty()
            && c.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    };
    let algorithm_ok = algorithm.split(['+', '.', '_', '-']).all(component);
    let encoded_ok = !encoded.is_empty()
        && encoded
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'=' | b'_' | b'-'));
    let hex = |size: usize| {
        encoded.len() == size
            && encoded
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    algorithm_ok
        && encoded_ok
        && match algorithm {
            "sha256" | "blake3" => hex(64),
            "sha512" => hex(128),
            _ => true,
        }
}

/// The most characters an image's name has (the distribution reference's
/// `NameTotalLengthMax`).
const IMAGE_NAME_MAX: usize = 255;

/// A path component (`path-component`): lower-case letters and digits in
/// runs joined by `.`, `_`, `__` or hyphens.
fn path_component(text: &str) -> bool {
    let alnum = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let bytes = text.as_bytes();
    if !bytes.first().is_some_and(alnum) || !bytes.last().is_some_and(alnum) {
        return false;
    }
    // The runs between the letters and digits are its separators.
    text.split(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .all(|run| run == "." || run == "_" || run == "__" || run.bytes().all(|b| b == b'-'))
}

/// A registry host (`domain`): a domain name in lower case, an IPv4
/// address, or an IPv6 address in brackets, each in its canonical text,
/// and a port from 1 to 65535 where given.
fn registry(text: &str) -> bool {
    let (host, port) = match text.rfind(':') {
        Some(at) if !text[at..].contains(']') => (&text[..at], Some(&text[at + 1..])),
        _ => (text, None),
    };
    let host_ok = match host.strip_prefix('[') {
        Some(rest) => rest
            .strip_suffix(']')
            .and_then(super::web::canonical_ip)
            .is_some_and(|address| address.is_ipv6()),
        None => {
            super::web::canonical_ip(host).is_some_and(|address| address.is_ipv4())
                || super::web::is_domain_name(host)
        }
    };
    host_ok && port.is_none_or(|port| super::web::whole_in(port, 1, 65_535))
}

/// A container image reference by the OCI distribution reference grammar,
/// fully qualified: its registry first (a host with a `.` or a port, or
/// `localhost`, as the grammar tells a registry from a path), its
/// repository's path components in lower case, and a tag after `:`, a
/// digest after `@`, or both, since a reference with neither means
/// `latest` and one image is written one way. The name is at most 255
/// characters; the digest is a [`is_digest`] in hexadecimal.
pub fn is_container_image(text: &str) -> bool {
    let (rest, digest) = match text.split_once('@') {
        Some((rest, digest)) => (rest, Some(digest)),
        None => (text, None),
    };
    // The tag follows the last `:` after the last `/`.
    let slash = rest.rfind('/').unwrap_or(0);
    let (name, tag) = match rest[slash..].rfind(':') {
        Some(at) => (&rest[..slash + at], Some(&rest[slash + at + 1..])),
        None => (rest, None),
    };
    if tag.is_none() && digest.is_none() {
        return false;
    }
    let tag_ok = tag.is_none_or(|tag| {
        let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        (1..=128).contains(&tag.len())
            && tag.bytes().next().is_some_and(word)
            && tag.bytes().all(|b| word(b) || matches!(b, b'.' | b'-'))
    });
    let digest_ok = digest.is_none_or(|digest| {
        is_digest(digest)
            && digest.split_once(':').is_some_and(|(algorithm, encoded)| {
                algorithm
                    .split(['+', '.', '_', '-'])
                    .all(|c| c.bytes().next().is_some_and(|b| b.is_ascii_lowercase()))
                    && encoded.len() >= 32
                    && encoded
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
    });
    let Some((domain, path)) = name.split_once('/') else {
        return false;
    };
    let is_registry = domain.contains(['.', ':']) || domain == "localhost";
    is_registry
        && registry(domain)
        && name.len() <= IMAGE_NAME_MAX
        && path.split('/').all(path_component)
        && tag_ok
        && digest_ok
}

/// A regular expression area 2's patterns take (`crate::compile_pattern`):
/// matched whole, in linear time, with no backreferences or lookaround.
pub fn is_regular_expression(text: &str) -> bool {
    !text.is_empty() && crate::compile_pattern(text).is_ok()
}

// ---------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------

/// A standard type held as text, shown as its canonical text.
const fn text_type(
    name: &'static str,
    pattern: &'static str,
    validator: Option<Validator>,
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Text {
            pattern,
            personal: None,
        },
        text: Some(TextForm::Checked),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key: FormatKey::Canonical,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator,
        shape: None,
        examples: Examples {
            valid,
            format: None,
            malformed,
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
    }
}

/// A container image reference (OCI distribution), fully qualified.
pub const CONTAINER_IMAGE: StandardType = text_type(
    "ContainerImage",
    r"[0-9A-Za-z.:\-_/@+=\[\]]+",
    Some(Validator::ContainerImage),
    &[
        (
            "docker.io/library/nginx:1.27",
            "docker.io/library/nginx:1.27",
        ),
        (
            "ghcr.io/acme/shop-api@sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
            "ghcr.io/acme/shop-api@sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
        ),
        (
            "localhost:5000/infra/web_app:v2.1.0-rc.1",
            "localhost:5000/infra/web_app:v2.1.0-rc.1",
        ),
        (
            "registry.example.com:8443/a/b/c:latest@sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
            "registry.example.com:8443/a/b/c:latest@sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
        ),
    ],
    &[
        // No registry, the familiar short form.
        "nginx:1.27",
        "library/nginx:1.27",
        // Neither a tag nor a digest: `latest` left implied.
        "docker.io/library/nginx",
        // Upper case in the repository or the registry.
        "docker.io/library/Nginx:1.27",
        "Docker.io/library/nginx:1.27",
        // A separator at a component's end, and a tag too long or badly
        // started.
        "docker.io/library/nginx-:1",
        "docker.io/library/nginx:.hidden",
        "docker.io/library/nginx:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        // A digest that isn't one.
        "docker.io/library/nginx@sha256:abc",
        "docker.io/library/nginx:1.27 ",
        "",
    ],
);

/// A CVE record's id (the CVE programme's ID syntax): `CVE-`, a year from
/// 1999, and a sequence of four digits, zeros leading where needed, or of
/// five to nineteen with no leading zero.
pub const CVE: StandardType = text_type(
    "Cve",
    r"CVE-(?:1999|[2-9][0-9]{3})-(?:[0-9]{4}|[1-9][0-9]{4,18})",
    None,
    &[
        ("CVE-2014-0160", "CVE-2014-0160"),
        ("CVE-2021-44228", "CVE-2021-44228"),
        ("CVE-1999-0001", "CVE-1999-0001"),
        ("CVE-2024-1234567", "CVE-2024-1234567"),
    ],
    &[
        // Lower case, before 1999, too few digits, and five or more with a
        // leading zero.
        "cve-2014-0160",
        "CVE-1998-0001",
        "CVE-2014-160",
        "CVE-2014-01600",
        "CVE-14-0160",
        "CVE-2014-0160 ",
        "",
    ],
);

/// A digest (OCI image specification).
pub const DIGEST: StandardType = text_type(
    "Digest",
    r"[a-z0-9]+(?:[+._\-][a-z0-9]+)*:[a-zA-Z0-9=_\-]+",
    Some(Validator::Digest),
    &[
        (
            "sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
            "sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
        ),
        (
            "sha512:401b09eab3c013d4ca54922bb802bec8fd5318192b0a75f201d8b3727429080fb337591abd3e44453b954555b7a0812e1081c39b740293f765eae731f5a65ed1",
            "sha512:401b09eab3c013d4ca54922bb802bec8fd5318192b0a75f201d8b3727429080fb337591abd3e44453b954555b7a0812e1081c39b740293f765eae731f5a65ed1",
        ),
        (
            "multihash+base58:QmRZxt2b1FVZPNqd8hsiykDL3TdBDeTSPX9Kv46HmX4Gx8",
            "multihash+base58:QmRZxt2b1FVZPNqd8hsiykDL3TdBDeTSPX9Kv46HmX4Gx8",
        ),
    ],
    &[
        // Upper case where the algorithm registers lower, the wrong length,
        // an upper-case algorithm, and no algorithm.
        "sha256:6C3C624B58DBBCD3C0DD82B4C53F04194D1247C6EEBDAAB7C610CF7D66709B3B",
        "sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3",
        "SHA256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
        "6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b",
        "sha256:",
        "",
    ],
);

/// A digital object identifier (the DOI Handbook): `10.`, the registrant's
/// code of four to nine digits with any subdivisions after `.`, `/`, and a
/// suffix of printable characters, ASCII letters in lower case, since a
/// DOI ignores their case; no `doi:` or resolver before it.
pub const DOI: StandardType = text_type(
    "Doi",
    r"10\.[0-9]{4,9}(?:\.[0-9]+)*/[^A-Z\p{Cc}\p{Z}]+",
    None,
    &[
        ("10.1000/182", "10.1000/182"),
        ("10.1038/nphys1170", "10.1038/nphys1170"),
        (
            "10.1002/(sici)1097-4571(199806)49:8<693::aid-asi4>3.0.co;2-o",
            "10.1002/(sici)1097-4571(199806)49:8<693::aid-asi4>3.0.co;2-o",
        ),
        ("10.1000.10/123456", "10.1000.10/123456"),
    ],
    &[
        // A resolver or scheme before it, upper case, no suffix, a short
        // registrant code, and a space.
        "doi:10.1000/182",
        "https://doi.org/10.1000/182",
        "10.1038/NPHYS1170",
        "10.1000/",
        "10.100/182",
        "10.1000/18 2",
        "11.1000/182",
        "",
    ],
);

/// A portable file's name.
pub const FILE_NAME: StandardType = text_type(
    "FileName",
    r"[^/\\\x00-\x1F\x7F]{1,255}",
    Some(Validator::FileName),
    &[
        ("report.pdf", "report.pdf"),
        ("Résumé 2026.docx", "Résumé 2026.docx"),
        (".gitignore", ".gitignore"),
        ("archive.tar.gz", "archive.tar.gz"),
        ("CONSOLE.txt", "CONSOLE.txt"),
    ],
    &[
        // A separator, a NUL, Windows' reserved characters and device
        // names, a dot or space at the end, and `.` and `..`.
        "notes/today.txt",
        "notes\\today.txt",
        "a\u{0}b",
        "what?.txt",
        "a:b",
        "CON",
        "nul.txt",
        "com1",
        "LPT9.log",
        "draft.",
        "draft ",
        ".",
        "..",
        "",
    ],
);

/// A portable path, with `/` between its names.
pub const FILE_PATH: StandardType = text_type(
    "FilePath",
    r"[^\\\x00-\x1F\x7F]{1,4096}",
    Some(Validator::FilePath),
    &[
        ("/var/lib/acme/uploads", "/var/lib/acme/uploads"),
        ("reports/2026/october.csv", "reports/2026/october.csv"),
        ("../shared/logo.png", "../shared/logo.png"),
        ("/", "/"),
    ],
    &[
        // Not in its shortest form: a `.`, a `..` past a name, an empty
        // segment, a `/` at the end.
        "./reports",
        "reports/../logo.png",
        "/../etc/passwd",
        "reports//october.csv",
        "reports/",
        // Windows' separators and drive, and a reserved name.
        "C:\\Users\\me",
        "C:/Users/me",
        "logs/aux/today.txt",
        "",
    ],
);

/// A git ref name (`git check-ref-format --allow-onelevel`).
pub const GIT_REF: StandardType = text_type(
    "GitRef",
    r"[^\x00-\x20\x7F~^:?*\[\\]+",
    Some(Validator::GitRef),
    &[
        ("main", "main"),
        ("refs/heads/feature/x-1", "refs/heads/feature/x-1"),
        ("v1.2.0", "v1.2.0"),
        (
            "8d3f2a1c9b7e6d5f4a3b2c1d0e9f8a7b6c5d4e3f",
            "8d3f2a1c9b7e6d5f4a3b2c1d0e9f8a7b6c5d4e3f",
        ),
    ],
    &[
        // The command's rules, by number.
        ".hidden",
        "refs/heads/.hidden",
        "main.lock",
        "feature..fix",
        "feature fix",
        "feature~1",
        "main^",
        "a:b",
        "what?",
        "feat*",
        "feat[1]",
        "/main",
        "main/",
        "refs//heads",
        "main.",
        "main@{1}",
        "@",
        "feature\\fix",
        "",
    ],
);

/// A request's idempotency key (the IETF's `Idempotency-Key` header): 1 to
/// 255 printable ASCII characters, as a structured field's string carries
/// them unescaped, so no `"` or `\`, and no space at either end.
pub const IDEMPOTENCY_KEY: StandardType = text_type(
    "IdempotencyKey",
    r#"[\x21\x23-\x5B\x5D-\x7E](?:[\x20\x21\x23-\x5B\x5D-\x7E]{0,253}[\x21\x23-\x5B\x5D-\x7E])?"#,
    None,
    &[
        (
            "8e03978e-40d5-43e8-bc93-6894a57f9324",
            "8e03978e-40d5-43e8-bc93-6894a57f9324",
        ),
        ("order-1042-attempt", "order-1042-attempt"),
        ("a", "a"),
    ],
    &[
        " key",
        "key ",
        "say \"hi\"",
        "back\\slash",
        "café",
        "",
        "line\nbreak",
    ],
);

/// How severe a log record is (OpenTelemetry's six severities, each its
/// short name as its code), from least to most.
pub const LOG_LEVELS: &[(&str, &str)] = &[
    ("trace", "TRACE"),
    ("debug", "DEBUG"),
    ("info", "INFO"),
    ("warn", "WARN"),
    ("error", "ERROR"),
    ("fatal", "FATAL"),
];

/// `LogLevel`: OpenTelemetry's severities, written as their short names.
pub const LOG_LEVEL: StandardType = StandardType {
    name: "LogLevel",
    holds: Holds::Enum(LOG_LEVELS),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[("TRACE", "TRACE"), ("INFO", "INFO"), ("FATAL", "FATAL")],
        format: None,
        malformed: &["WARNING", "CRITICAL", "INFO ", ""],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A package URL (ECMA-427).
pub const PACKAGE_URL: StandardType = text_type(
    "PackageUrl",
    r"pkg:[a-z][a-z0-9.\-]*/[A-Za-z0-9.\-_~:%/@?=&#]+",
    Some(Validator::PackageUrl),
    // The specification's own examples.
    &[
        (
            "pkg:npm/%40angular/animation@12.3.1",
            "pkg:npm/%40angular/animation@12.3.1",
        ),
        ("pkg:pypi/django@1.11.1", "pkg:pypi/django@1.11.1"),
        (
            "pkg:maven/org.apache.xmlgraphics/batik-anim@1.9.1?classifier=sources&packaging=jar",
            "pkg:maven/org.apache.xmlgraphics/batik-anim@1.9.1?classifier=sources&packaging=jar",
        ),
        (
            "pkg:deb/debian/curl@7.50.3-1?arch=i386&distro=jessie",
            "pkg:deb/debian/curl@7.50.3-1?arch=i386&distro=jessie",
        ),
        (
            "pkg:golang/google.golang.org/genproto#googleapis/api/annotations",
            "pkg:golang/google.golang.org/genproto#googleapis/api/annotations",
        ),
        (
            "pkg:docker/cassandra@sha256:244fd47e07d1004f0aed9c",
            "pkg:docker/cassandra@sha256:244fd47e07d1004f0aed9c",
        ),
    ],
    &[
        // Upper case scheme or type, `//` after the scheme, no name.
        "PKG:npm/left-pad@1.3.0",
        "pkg:NPM/left-pad@1.3.0",
        "pkg://npm/left-pad@1.3.0",
        "pkg:npm/",
        "pkg:npm",
        // An `@` that isn't the version's, a `+` not encoded, a
        // lower-case or needless percent-encoding.
        "pkg:npm/@angular/animation@12.3.1",
        "pkg:pypi/django@1.11.1+local",
        "pkg:npm/%40angular/animation@12.3.1?vcs_url=git%2bhttps",
        "pkg:pypi/%64jango@1.11.1",
        // Qualifiers out of order, twice, empty, with an upper-case key.
        "pkg:deb/debian/curl@7.50.3-1?distro=jessie&arch=i386",
        "pkg:deb/debian/curl@7.50.3-1?arch=i386&arch=amd64",
        "pkg:deb/debian/curl@7.50.3-1?arch=",
        "pkg:deb/debian/curl@7.50.3-1?Arch=i386",
        // A subpath with `..` or an empty segment.
        "pkg:golang/google.golang.org/genproto#googleapis/../api",
        "pkg:golang/google.golang.org/genproto#googleapis//api",
        "",
    ],
);

/// A regular expression on area 2's linear-time engine.
pub const REGULAR_EXPRESSION: StandardType = text_type(
    "RegularExpression",
    r"(?s:.+)",
    Some(Validator::RegularExpression),
    &[
        ("[A-Z]{2}[0-9]{2}", "[A-Z]{2}[0-9]{2}"),
        ("(?i)grey|gray", "(?i)grey|gray"),
        (r"\p{Lu}\w*", r"\p{Lu}\w*"),
    ],
    &[
        // Backreferences, lookaround, and patterns that don't compile.
        r"(a+)\1",
        r"(?<name>a)\k<name>",
        "foo(?=bar)",
        "(?<!un)done",
        "[a-",
        "(unclosed",
        "",
    ],
);

/// A version (SemVer 2.0.0): its numbers, pre-release and build metadata,
/// written `1.2.3-alpha.1+build.5`, ordered by its precedence.
pub const SEMANTIC_VERSION_ENTRY: StandardType = StandardType {
    name: SEMANTIC_VERSION,
    holds: Holds::Object(&SEMANTIC_VERSION_FIELDS),
    text: Some(TextForm::SemanticVersion),
    json: Json::Text,
    storage: Storage::Columns {
        parts: &[
            ("major", "bigint"),
            ("minor", "bigint"),
            ("patch", "bigint"),
            ("pre_release", "text"),
            ("build", "text"),
        ],
        fixed: None,
    },
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    // SemVer 2.0.0 §11's own order, ascending.
    examples: Examples {
        valid: &[
            ("1.0.0-alpha", "1.0.0-alpha"),
            ("1.0.0-alpha.1", "1.0.0-alpha.1"),
            ("1.0.0-alpha.beta", "1.0.0-alpha.beta"),
            ("1.0.0-beta", "1.0.0-beta"),
            ("1.0.0-beta.2", "1.0.0-beta.2"),
            ("1.0.0-beta.11", "1.0.0-beta.11"),
            ("1.0.0-rc.1", "1.0.0-rc.1"),
            ("1.0.0", "1.0.0"),
            ("1.9.0", "1.9.0"),
            ("1.10.0+build.5", "1.10.0+build.5"),
            ("2.0.0", "2.0.0"),
        ],
        format: None,
        malformed: &[
            // A `v`, too few or too many numbers, leading zeros, an empty
            // identifier, and a number past an `Int`.
            "v1.0.0",
            "1.0",
            "1.0.0.0",
            "01.0.0",
            "1.0.0-01",
            "1.0.0-",
            "1.0.0-alpha..1",
            "1.0.0+",
            "1.0.0+build..5",
            "1.0.0_alpha",
            "9223372036854775808.0.0",
            "",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A span's id (W3C Trace Context's `parent-id`): 16 lower-case
/// hexadecimal digits, not all zero.
pub const SPAN_ID: StandardType = text_type(
    "SpanId",
    r"[0-9a-f]{16}",
    Some(Validator::TraceHex),
    &[
        ("00f067aa0ba902b7", "00f067aa0ba902b7"),
        ("ffffffffffffffff", "ffffffffffffffff"),
    ],
    &[
        "0000000000000000",
        "00F067AA0BA902B7",
        "00f067aa0ba902b",
        "00f067aa0ba902b7a",
        "",
    ],
);

/// An SPDX licence expression.
pub const SPDX_LICENCE: StandardType = text_type(
    "SpdxLicence",
    r"[A-Za-z0-9.\-+:() ]+",
    Some(Validator::SpdxLicence),
    &[
        ("MIT", "MIT"),
        ("Apache-2.0", "Apache-2.0"),
        ("MIT OR Apache-2.0", "MIT OR Apache-2.0"),
        (
            "GPL-2.0-or-later WITH Classpath-exception-2.0",
            "GPL-2.0-or-later WITH Classpath-exception-2.0",
        ),
        (
            "(MIT OR Apache-2.0) AND BSD-3-Clause",
            "(MIT OR Apache-2.0) AND BSD-3-Clause",
        ),
        ("LGPL-2.1+", "LGPL-2.1+"),
        ("LicenseRef-acme-internal", "LicenseRef-acme-internal"),
        (
            "DocumentRef-spdx-tool-1.2:LicenseRef-MIT-Style-2",
            "DocumentRef-spdx-tool-1.2:LicenseRef-MIT-Style-2",
        ),
    ],
    &[
        // Not as the list spells it, not on it, a lower-case operator, a
        // space inside parentheses, unbalanced, an exception that isn't
        // one, and a licence's name rather than its identifier.
        "mit",
        "Apache-2",
        "MIT or Apache-2.0",
        "( MIT OR Apache-2.0 )",
        "(MIT OR Apache-2.0",
        "MIT  OR Apache-2.0",
        "MIT WITH Apache-2.0",
        "MIT License",
        "LicenseRef-acme-internal+",
        "MIT OR",
        "NOASSERTION",
        "",
    ],
);

/// A trace's id (W3C Trace Context's `trace-id`): 32 lower-case
/// hexadecimal digits, not all zero.
pub const TRACE_ID: StandardType = text_type(
    "TraceId",
    r"[0-9a-f]{32}",
    Some(Validator::TraceHex),
    &[
        (
            "4bf92f3577b34da6a3ce929d0e0e4736",
            "4bf92f3577b34da6a3ce929d0e0e4736",
        ),
        (
            "0af7651916cd43dd8448eb211c80319c",
            "0af7651916cd43dd8448eb211c80319c",
        ),
    ],
    &[
        "00000000000000000000000000000000",
        "4BF92F3577B34DA6A3CE929D0E0E4736",
        "4bf92f3577b34da6a3ce929d0e0e473",
        "4bf92f3577b34da6-a3ce929d0e0e4736",
        "",
    ],
);

/// A `traceparent` header's value (W3C Trace Context, version `00`).
pub const TRACE_PARENT: StandardType = text_type(
    "TraceParent",
    r"[0-9a-f]{2}-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}",
    Some(Validator::TraceParent),
    // The specification's own examples.
    &[
        (
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        ),
        (
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00",
        ),
    ],
    &[
        // Another version, an id all zero, upper case, a part missing.
        "ff-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        "00-00000000000000000000000000000000-b7ad6b7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01",
        "00-0AF7651916CD43DD8448EB211C80319C-B7AD6B7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331",
        "",
    ],
);

/// A ULID (the ULID specification): 26 characters of Crockford's base 32
/// in upper case, the first at most `7`, since a ULID is 128 bits.
pub const ULID: StandardType = text_type(
    "Ulid",
    r"[0-7][0-9A-HJKMNP-TV-Z]{25}",
    None,
    &[
        ("01ARZ3NDEKTSV4RRFFQ69G5FAV", "01ARZ3NDEKTSV4RRFFQ69G5FAV"),
        ("7ZZZZZZZZZZZZZZZZZZZZZZZZZ", "7ZZZZZZZZZZZZZZZZZZZZZZZZZ"),
    ],
    &[
        // Past 128 bits, lower case, Crockford's excluded letters, the
        // wrong length.
        "8ZZZZZZZZZZZZZZZZZZZZZZZZZ",
        "01arz3ndektsv4rrffq69g5fav",
        "01ARZ3NDEKTSV4RRFFQ69G5FAI",
        "01ARZ3NDEKTSV4RRFFQ69G5FAL",
        "01ARZ3NDEKTSV4RRFFQ69G5FAO",
        "01ARZ3NDEKTSV4RRFFQ69G5FAU",
        "01ARZ3NDEKTSV4RRFFQ69G5FA",
        "",
    ],
);

/// A version range (node-semver's grammar, in one spelling).
pub const VERSION_RANGE: StandardType = text_type(
    "VersionRange",
    r"[0-9A-Za-z.\-<>=^~*| ]+",
    Some(Validator::VersionRange),
    &[
        ("^1.2.3", "^1.2.3"),
        ("~1.2", "~1.2"),
        (">=1.0.0 <2.0.0", ">=1.0.0 <2.0.0"),
        ("1.2.x || >=2.5.0-beta.1", "1.2.x || >=2.5.0-beta.1"),
        ("1.0.0 - 1.4.2", "1.0.0 - 1.4.2"),
        ("*", "*"),
        ("2.0.0", "2.0.0"),
    ],
    &[
        // Another spelling: `v` or `=` before a version, a space after an
        // operator, `X` or `*` inside a version, `||` without its spaces.
        "^v1.2.3",
        "=1.2.3",
        ">= 1.0.0",
        "1.2.X",
        "1.*",
        "^1.2.3||^2.0.0",
        "1.0.0  2.0.0",
        // Build metadata, a pre-release on a partial, a wildcard before a
        // number, an empty range, and not a range.
        "^1.2.3+build",
        "^1.2-beta",
        "1.x.3",
        "",
        "latest",
        "1.0.0 -",
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version<'_> {
        read_version(text).unwrap_or_else(|| panic!("`{text}` is a version"))
    }

    fn inside(text: &str, range: &str) -> bool {
        satisfies(&version(text), range).unwrap_or_else(|| panic!("`{range}` is a range"))
    }

    /// SemVer 2.0.0 §11's examples, each below the next.
    #[test]
    fn versions_order_by_semver_precedence() {
        let order = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "2.0.0",
            "2.1.0",
            "2.1.1",
        ];
        for pair in order.windows(2) {
            assert_eq!(
                precedence(&version(pair[0]), &version(pair[1])),
                Ordering::Less,
                "{} < {}",
                pair[0],
                pair[1]
            );
        }
        // §10: build metadata is ignored.
        assert_eq!(
            precedence(
                &version("1.0.0+20130313144700"),
                &version("1.0.0+exp.sha.5114f85")
            ),
            Ordering::Equal
        );
        assert_eq!(
            precedence(&version("1.10.0"), &version("1.9.0")),
            Ordering::Greater
        );
        assert_eq!(
            version_text(&version("1.0.0-x.7.z.92+meta")),
            "1.0.0-x.7.z.92+meta"
        );
        assert_eq!(
            read_version("1.0.0-0.3.7").map(|v| v.pre_release),
            Some(Some("0.3.7"))
        );
        assert_eq!(
            read_version("1.0.0-0abc").map(|v| v.pre_release),
            Some(Some("0abc"))
        );
        assert!(
            read_version("1.0.0+001").is_some(),
            "§10: build may lead with zeros"
        );
    }

    /// node-semver's README examples for each form.
    #[test]
    fn ranges_desugar_as_node_semver_does() {
        // Carets.
        assert!(inside("1.9.9", "^1.2.3"));
        assert!(!inside("2.0.0", "^1.2.3"));
        assert!(!inside("1.2.2", "^1.2.3"));
        assert!(inside("0.2.9", "^0.2.3"));
        assert!(!inside("0.3.0", "^0.2.3"));
        assert!(inside("0.0.3", "^0.0.3"));
        assert!(!inside("0.0.4", "^0.0.3"));
        assert!(inside("0.0.9", "^0.0"));
        assert!(!inside("0.1.0", "^0.0"));
        assert!(inside("0.9.0", "^0.x"));
        assert!(!inside("1.0.0", "^0.x"));
        assert!(inside("1.9.0", "^1.2"));
        // Tildes.
        assert!(inside("1.2.9", "~1.2.3"));
        assert!(!inside("1.3.0", "~1.2.3"));
        assert!(inside("1.2.0", "~1.2"));
        assert!(inside("1.9.0", "~1"));
        assert!(!inside("2.0.0", "~1"));
        // Partials and wildcards.
        assert!(inside("1.2.7", "1.2.x"));
        assert!(!inside("1.3.0", "1.2.x"));
        assert!(inside("1.2.7", "1.2"));
        assert!(inside("1.7.0", "1.x"));
        assert!(inside("3.1.4", "*"));
        // Comparisons with partials.
        assert!(inside("1.3.0", ">1.2"));
        assert!(!inside("1.2.9", ">1.2"));
        assert!(!inside("1.2.0", "<1.2"));
        assert!(inside("1.2.9", "<=1.2"));
        assert!(!inside("1.3.0", "<=1.2"));
        assert!(inside("1.2.0", ">=1.2"));
        // Hyphens.
        assert!(inside("2.3.4", "1.2.3 - 2.3.4"));
        assert!(!inside("2.3.5", "1.2.3 - 2.3.4"));
        assert!(inside("2.3.9", "1.2.3 - 2.3"));
        assert!(!inside("2.4.0", "1.2.3 - 2.3"));
        assert!(inside("2.9.9", "1.2 - 2"));
        assert!(inside("1.2.0", "1.2 - 2"));
        // Sets.
        assert!(inside("1.2.7", "1.2.7 || >=1.2.9 <2.0.0"));
        assert!(!inside("1.2.8", "1.2.7 || >=1.2.9 <2.0.0"));
        assert!(inside("1.4.6", "1.2.7 || >=1.2.9 <2.0.0"));
        assert!(!inside("2.0.0", "1.2.7 || >=1.2.9 <2.0.0"));
        // An exact version.
        assert!(inside("2.0.0", "2.0.0"));
        assert!(!inside("2.0.1", "2.0.0"));
        // Build metadata doesn't change which range a version is in.
        assert!(inside("1.2.3+build.7", "1.2.3"));
    }

    /// node-semver's pre-release rule: a pre-release is inside a range only
    /// where a comparator of its set names a pre-release of the same
    /// numbers.
    #[test]
    fn a_pre_release_is_inside_only_where_its_numbers_are_named() {
        assert!(inside("1.2.3-alpha.7", ">1.2.3-alpha.3"));
        assert!(!inside("3.4.5-alpha.9", ">1.2.3-alpha.3"));
        assert!(inside("3.4.5", ">1.2.3-alpha.3"));
        assert!(!inside("1.2.3-alpha.2", ">1.2.3-alpha.3"));
        assert!(!inside("1.5.0-rc.1", ">=1.0.0"));
        assert!(!inside("2.0.0-beta", "^1.2.3"));
        assert!(inside("1.2.3-beta.4", "^1.2.3-beta.2"));
        assert!(!inside("1.2.4-beta.2", "^1.2.3-beta.2"));
        assert!(inside("1.2.3-beta.2", "~1.2.3-beta.2"));
        assert!(!inside("1.0.0-beta", "*"));
        assert!(inside("2.5.0-beta.3", "1.2.x || >=2.5.0-beta.1"));
        assert!(inside("1.2.3-rc.1", "1.2.3-rc.1"));
        assert!(!inside("1.2.3-rc.2", "1.2.3-rc.1"));
    }

    #[test]
    fn ref_names_follow_git_check_ref_format() {
        for good in [
            "main",
            "refs/heads/main",
            "release/2026.10",
            "v1.0.0",
            "a.b",
            "naïve",
        ] {
            assert!(is_git_ref(good), "{good}");
        }
        for bad in [
            "a/.b", "a.lock/b", "a..b", "a b", "a~b", "a^b", "a:b", "a?b", "a*b", "a[b", "/a",
            "a/", "a//b", "a.", "a@{b", "@", "a\\b", "a\u{7F}b",
        ] {
            assert!(!is_git_ref(bad), "{bad}");
        }
        assert!(is_git_ref("a@b"), "only `@{{` and a lone `@` are refused");
    }

    #[test]
    fn spdx_ids_come_from_the_bundled_list() {
        assert_eq!(spdx_list_version(), "3.29.0");
        assert!(is_spdx_licence("GPL-2.0"), "deprecated, still a licence");
        assert!(is_spdx_licence("MIT AND (Apache-2.0 OR BSD-2-Clause)"));
        assert!(is_spdx_licence("((MIT))"));
        assert!(!is_spdx_licence("MIT AND"));
        assert!(!is_spdx_licence("()"));
        assert!(!is_spdx_licence("MIT WITH"));
        assert!(!is_spdx_licence("MIT++"));
    }

    #[test]
    fn file_names_are_portable() {
        assert!(is_file_name("COM10"), "only COM0 to COM9 are devices");
        assert!(!is_file_name("COM¹"));
        assert!(!is_file_name("aux .txt"));
        assert!(is_file_name(&"a".repeat(255)));
        assert!(!is_file_name(&"a".repeat(256)));
        assert!(is_file_path("../../shared"));
        assert!(!is_file_path("shared/.."));
    }
}
