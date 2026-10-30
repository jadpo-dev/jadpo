//! The Rust toolchain `jadpo build` compiles with (roadmap chunk 9.2c;
//! docs/compiler-runtime.md §2.8, "Toolchain pinning"; area 14, "A
//! native deployable"): the release `rust-toolchain.toml` pins, fetched
//! into Jadpo's cache the first time a build needs it, so nobody installs
//! Rust.
//!
//! [`ensure`] keeps `toolchains/<version>-<host>/` there: `rustc`,
//! `cargo` and the standard library for the machine `jadpo` runs on, and
//! the standard library for wasm32 once a Worker is built, each fetched
//! once from Rust's dist server as rustup fetches it. The channel
//! manifest, `dist/channel-rust-<version>.toml`, names each archive and
//! its SHA-256; this `jadpo` pins the manifest's own SHA-256
//! ([`MANIFEST_SHA256`]), so what `static.rust-lang.org` serves is checked
//! against what this release was built with. `JADPO_RUST_DIST_SERVER`
//! names another server (a mirror, or a test's stand-in), whose manifest
//! is checked against the `.sha256` it serves beside it, as rustup checks
//! one from `RUSTUP_DIST_SERVER`. Each archive is checked against the
//! manifest's SHA-256 before it is unpacked, and a component counts as
//! there only once it is unpacked whole (`.jadpo/<package>-<target>`);
//! the folder's lock is held throughout, so two builds at once fetch it
//! once.
//!
//! The archives are the `.tar.gz` ones, unpacked with the gzip reader the
//! runtime already has over miniz_oxide: the `.tar.xz` ones are about a
//! third smaller, but would need liblzma or another decoder.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::download;

/// The variable naming the dist server, and the default.
pub(crate) const SERVER_VARIABLE: &str = "JADPO_RUST_DIST_SERVER";
pub(crate) const DEFAULT_SERVER: &str = "https://static.rust-lang.org";

/// The SHA-256 of `static.rust-lang.org`'s channel manifest for
/// [`MANIFEST_VERSION`], the release `rust-toolchain.toml` pins.
pub(crate) const MANIFEST_SHA256: &str =
    "ce6dddc886364f8d786514771212cebe9b731ba82d6b859951c6b0ccc516b6a2";
const MANIFEST_VERSION: &str = "1.99.0";

/// The target this `jadpo` runs on, whose toolchain it fetches.
pub(crate) const HOST: &str = env!("JADPO_HOST");

/// The target a Worker is built for.
pub(crate) const WASM: &str = jadpo_codegen::deployable::WASM;

/// The most a channel manifest may be.
const LONGEST_MANIFEST: usize = 16 << 20;

/// The Rust release `rust-toolchain.toml` pins: `1.99.0`.
pub(crate) fn version() -> &'static str {
    crate::runtime_source::TOOLCHAIN
        .lines()
        .find_map(|line| line.trim().strip_prefix("channel = \"")?.strip_suffix('"'))
        .unwrap_or_default()
}

/// What a build said it fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub(crate) version: String,
    /// The targets whose parts were fetched, the host's first.
    pub(crate) targets: Vec<String>,
    /// The archives' bytes.
    pub(crate) bytes: u64,
}

/// The toolchain's folder, and what was fetched for it this time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) folder: PathBuf,
    pub(crate) fetched: Option<Fetched>,
}

impl Installed {
    pub(crate) fn bin(&self) -> PathBuf {
        self.folder.join("bin")
    }
}

/// How a fetch is reported as it goes.
pub(crate) trait Report {
    /// A fetch starts: what it fetches.
    fn fetching(&self, words: &str);
    /// How far one archive has got: what it is, and its bytes so far of
    /// how many, when that is known.
    fn progress(&self, words: &str, done: u64, total: Option<u64>);
}

/// One part of a toolchain: `rustc`, `cargo` or `rust-std`, for a target.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Part {
    package: &'static str,
    target: String,
}

impl Part {
    fn marker(&self, folder: &Path) -> PathBuf {
        folder
            .join(".jadpo")
            .join(format!("{}-{}", self.package, self.target))
    }
}

/// The pinned toolchain in `cache`, Jadpo's cache folder, for this host
/// and, when `wasm`, for wasm32 too: fetched from `server` for what isn't
/// there yet.
pub(crate) fn ensure(
    cache: &Path,
    wasm: bool,
    server: &str,
    report: &dyn Report,
) -> Result<Installed, String> {
    let version = version();
    let parent = cache.join("toolchains");
    let name = format!("{version}-{HOST}");
    let folder = parent.join(&name);
    let io = |error: std::io::Error| format!("`{}`: {error}.", parent.display());
    std::fs::create_dir_all(&parent).map_err(io)?;
    let lock = std::fs::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(parent.join(format!("{name}.lock")))
        .map_err(io)?;
    lock.lock().map_err(io)?;
    let mut wanted = vec![
        Part {
            package: "rustc",
            target: HOST.to_string(),
        },
        Part {
            package: "cargo",
            target: HOST.to_string(),
        },
        Part {
            package: "rust-std",
            target: HOST.to_string(),
        },
    ];
    if wasm {
        wanted.push(Part {
            package: "rust-std",
            target: WASM.to_string(),
        });
    }
    let missing: Vec<Part> = wanted
        .into_iter()
        .filter(|part| !part.marker(&folder).exists())
        .collect();
    if missing.is_empty() {
        return Ok(Installed {
            folder,
            fetched: None,
        });
    }
    let mut targets: Vec<String> = Vec::new();
    for part in &missing {
        if !targets.contains(&part.target) {
            targets.push(part.target.clone());
        }
    }
    report.fetching(&format!("Fetching Rust {version} for {}", joined(&targets)));
    let server = server.trim_end_matches('/');
    let manifest = manifest(server, version)?;
    let downloads = parent.join(".downloads");
    std::fs::create_dir_all(&downloads).map_err(io)?;
    let mut bytes = 0;
    for part in &missing {
        let archive = archive(&manifest, part.package, &part.target).ok_or_else(|| {
            format!(
                "Rust {version}'s manifest offers no `{}` for `{}`.",
                part.package, part.target
            )
        })?;
        let url = match archive.url.strip_prefix(DEFAULT_SERVER) {
            Some(rest) => format!("{server}{rest}"),
            None => archive.url.clone(),
        };
        let file = downloads.join(format!("{name}-{}-{}.tar.gz", part.package, part.target));
        let words = format!("Fetching {} {version} for {}", part.package, part.target);
        bytes += fetched(&url, &file, &archive.hash, &words, report)?;
        report.progress(
            &format!("Unpacking {} {version} for {}", part.package, part.target),
            0,
            None,
        );
        let unpacked = std::fs::File::open(&file)
            .map_err(|error| error.to_string())
            .and_then(|archive| unpack(archive, &folder));
        let _ = std::fs::remove_file(&file);
        unpacked.map_err(|error| {
            format!(
                "Rust {version}'s `{}` for `{}` couldn't be unpacked into `{}`: {error}",
                part.package,
                part.target,
                folder.display()
            )
        })?;
        let marker = part.marker(&folder);
        std::fs::create_dir_all(marker.parent().unwrap_or(&folder))
            .and_then(|()| std::fs::write(&marker, &archive.hash))
            .map_err(io)?;
    }
    drop(lock);
    Ok(Installed {
        folder,
        fetched: Some(Fetched {
            version: version.to_string(),
            targets,
            bytes,
        }),
    })
}

/// `a`, `a and b`.
fn joined(names: &[String]) -> String {
    match names {
        [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => names.join(""),
    }
}

/// Rust `version`'s channel manifest from `server`, checked.
fn manifest(server: &str, version: &str) -> Result<String, String> {
    let url = format!("{server}/dist/channel-rust-{version}.toml");
    let bytes = download::get_all(&url, LONGEST_MANIFEST)?;
    let digest = sha256(&bytes);
    let expected = if server == DEFAULT_SERVER {
        if version != MANIFEST_VERSION {
            return Err(format!(
                "This Jadpo holds the manifest digest of Rust {MANIFEST_VERSION}, not {version}."
            ));
        }
        MANIFEST_SHA256.to_string()
    } else {
        let sidecar = download::get_all(&format!("{url}.sha256"), 4096)?;
        String::from_utf8_lossy(&sidecar)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase()
    };
    if digest != expected {
        return Err(format!(
            "`{url}` isn't the manifest this Jadpo expects: its SHA-256 is {digest}, not {expected}."
        ));
    }
    String::from_utf8(bytes).map_err(|_| format!("`{url}` isn't text."))
}

/// One archive the manifest names.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Archive {
    url: String,
    hash: String,
}

/// The `.tar.gz` the manifest offers of `package` for `target`, if it is
/// available: `[pkg.<package>.target.<target>]`'s `url` and `hash`.
fn archive(manifest: &str, package: &str, target: &str) -> Option<Archive> {
    let header = format!("[pkg.{package}.target.{target}]");
    let mut lines = manifest.lines().skip_while(|line| line.trim() != header);
    lines.next()?;
    let (mut available, mut url, mut hash) = (false, None, None);
    for line in lines {
        let line = line.trim();
        if line.starts_with('[') {
            break;
        }
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        let text = value.trim_matches('"').to_string();
        match key {
            "available" => available = value == "true",
            "url" => url = Some(text),
            "hash" => hash = Some(text),
            _ => {}
        }
    }
    available.then_some(())?;
    Some(Archive {
        url: url?,
        hash: hash?,
    })
}

/// Fetches `url` into `file`, checking its SHA-256 is `hash`: its size.
fn fetched(
    url: &str,
    file: &Path,
    hash: &str,
    words: &str,
    report: &dyn Report,
) -> Result<u64, String> {
    use sha2::{Digest, Sha256};
    let mut out = std::fs::File::create(file).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let total = std::cell::Cell::new(None);
    report.progress(words, 0, None);
    download::get(
        url,
        &mut |started| {
            total.set(started.length);
            report.progress(words, 0, started.length);
        },
        &mut |piece| {
            hasher.update(piece);
            out.write_all(piece)?;
            done += piece.len() as u64;
            report.progress(words, done, total.get());
            Ok(())
        },
    )?;
    out.flush().map_err(|error| error.to_string())?;
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != hash {
        let _ = std::fs::remove_file(file);
        return Err(format!(
            "`{url}` isn't the archive the manifest names: its SHA-256 is {digest}, not {hash}."
        ));
    }
    Ok(done)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Unpacks one of the dist server's archives into `folder`: each
/// component's files (`<archive>/<component>/<path>`) at `<path>`, as
/// rustup installs them, leaving out each component's `manifest.in` and
/// the installer's own files beside the components.
fn unpack(archive: impl Read, folder: &Path) -> Result<(), String> {
    let mut tar = Gunzip::new(archive);
    let mut header = [0u8; 512];
    let mut long_name: Option<String> = None;
    let mut long_link: Option<String> = None;
    loop {
        if !full(&mut tar, &mut header).map_err(|error| error.to_string())? {
            return Ok(());
        }
        if header.iter().all(|byte| *byte == 0) {
            return Ok(());
        }
        let size = number(&header[124..136]).ok_or("an entry's size isn't a number")?;
        let mode = number(&header[100..108]).unwrap_or(0o644);
        let kind = header[156];
        // A long name, a long link or a PAX header is for the entry after
        // it.
        match kind {
            b'L' => {
                long_name = Some(text(&read_data(&mut tar, size)?));
                continue;
            }
            b'K' => {
                long_link = Some(text(&read_data(&mut tar, size)?));
                continue;
            }
            b'x' => {
                let data = read_data(&mut tar, size)?;
                long_name = pax(&data, "path").or(long_name);
                long_link = pax(&data, "linkpath").or(long_link);
                continue;
            }
            _ => {}
        }
        let mut name = text(&header[..100]);
        if &header[257..263] == b"ustar\0" {
            let prefix = text(&header[345..500]);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
        }
        if let Some(long) = long_name.take() {
            name = long;
        }
        let link = long_link.take().unwrap_or_else(|| text(&header[157..257]));
        match kind {
            b'0' | 0 | b'7' => {
                let to = placed(&name, folder)?;
                match to {
                    Some(to) => {
                        if let Some(parent) = to.parent() {
                            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                        }
                        let _ = std::fs::remove_file(&to);
                        let mut out =
                            std::fs::File::create(&to).map_err(|error| error.to_string())?;
                        copy_data(&mut tar, size, &mut out)?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let mode = if mode & 0o111 != 0 { 0o755 } else { 0o644 };
                            std::fs::set_permissions(&to, std::fs::Permissions::from_mode(mode))
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    None => copy_data(&mut tar, size, &mut std::io::sink())?,
                }
            }
            b'5' => {
                if let Some(to) = placed(&name, folder)? {
                    std::fs::create_dir_all(to).map_err(|error| error.to_string())?;
                }
                copy_data(&mut tar, size, &mut std::io::sink())?;
            }
            b'2' => {
                if let Some(to) = placed(&name, folder)? {
                    if link.starts_with('/') || link.split('/').any(|part| part == "..") {
                        return Err(format!("`{name}` links outside the toolchain"));
                    }
                    if let Some(parent) = to.parent() {
                        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                    }
                    let _ = std::fs::remove_file(&to);
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(&link, &to).map_err(|error| error.to_string())?;
                }
                copy_data(&mut tar, size, &mut std::io::sink())?;
            }
            _ => copy_data(&mut tar, size, &mut std::io::sink())?,
        }
    }
}

/// The bytes of the first regular file in a `.tar.gz` whose name `wanted`
/// accepts, read with the same reader as [`unpack`]: how the toolchain
/// launcher takes `jadpo` out of a release's archive (area 14, "Which
/// jadpo runs"). `None` when no entry is accepted.
pub(crate) fn tar_gz_member(
    archive: impl Read,
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Option<Vec<u8>>, String> {
    let mut tar = Gunzip::new(archive);
    let mut header = [0u8; 512];
    let mut long_name: Option<String> = None;
    loop {
        if !full(&mut tar, &mut header).map_err(|error| error.to_string())?
            || header.iter().all(|byte| *byte == 0)
        {
            return Ok(None);
        }
        let size = number(&header[124..136]).ok_or("an entry's size isn't a number")?;
        let kind = header[156];
        match kind {
            b'L' => {
                long_name = Some(text(&read_data(&mut tar, size)?));
                continue;
            }
            b'x' => {
                let data = read_data(&mut tar, size)?;
                long_name = pax(&data, "path").or(long_name);
                continue;
            }
            _ => {}
        }
        let mut name = text(&header[..100]);
        if &header[257..263] == b"ustar\0" {
            let prefix = text(&header[345..500]);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
        }
        if let Some(long) = long_name.take() {
            name = long;
        }
        if matches!(kind, b'0' | 0 | b'7') && wanted(&name) {
            return read_data(&mut tar, size).map(Some);
        }
        copy_data(&mut tar, size, &mut std::io::sink())?;
    }
}

/// Where the archive's entry `name` goes in `folder`: `None` for what
/// isn't a component's file, an error for a name that would leave it.
fn placed(name: &str, folder: &Path) -> Result<Option<PathBuf>, String> {
    let parts: Vec<&str> = name
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if name.starts_with('/') || parts.contains(&"..") {
        return Err(format!("`{name}` is outside the archive"));
    }
    match parts.as_slice() {
        [_, _, rest @ ..] if !rest.is_empty() && rest != ["manifest.in"] => Ok(Some(
            rest.iter()
                .fold(folder.to_path_buf(), |path, part| path.join(part)),
        )),
        _ => Ok(None),
    }
}

/// The text of a header field: up to its first zero byte.
fn text(field: &[u8]) -> String {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// A header's number: octal, or base-256 when its first byte's top bit is
/// set.
fn number(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|byte| byte & 0x80 != 0) {
        return Some(
            field[1..]
                .iter()
                .fold(u64::from(field[0] & 0x7f), |value, byte| {
                    (value << 8) | u64::from(*byte)
                }),
        );
    }
    let digits = text(field);
    let digits = digits.trim_matches(|c: char| c == ' ' || c == '\0');
    if digits.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(digits, 8).ok()
}

/// A PAX header's `key` (`path`).
fn pax(data: &[u8], key: &str) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    text.lines().find_map(|record| {
        let (_, pair) = record.split_once(' ')?;
        let (found, value) = pair.split_once('=')?;
        (found == key).then(|| value.to_string())
    })
}

/// Fills `block` from `reader`: false when it had already ended.
fn full(reader: &mut impl Read, block: &mut [u8]) -> std::io::Result<bool> {
    let mut filled = 0;
    while filled < block.len() {
        match reader.read(&mut block[filled..])? {
            0 if filled == 0 => return Ok(false),
            0 => return Err(std::io::Error::other("the archive ends inside an entry")),
            read => filled += read,
        }
    }
    Ok(true)
}

/// An entry's data and the padding after it.
fn read_data(reader: &mut impl Read, size: u64) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    copy_data(reader, size, &mut data)?;
    Ok(data)
}

fn copy_data(reader: &mut impl Read, size: u64, out: &mut impl Write) -> Result<(), String> {
    let padded = size.div_ceil(512) * 512;
    let mut left = padded;
    let mut buffer = vec![0u8; 1 << 16];
    while left > 0 {
        let want = left.min(buffer.len() as u64) as usize;
        if !full(reader, &mut buffer[..want]).map_err(|error| error.to_string())? {
            return Err("the archive ends inside an entry".to_string());
        }
        let used = padded - left;
        let keep = size.saturating_sub(used).min(want as u64) as usize;
        out.write_all(&buffer[..keep])
            .map_err(|error| error.to_string())?;
        left -= want as u64;
    }
    Ok(())
}

/// A gzip file read as what it unpacks to, through the runtime's reader
/// (RFC 1952, every member's CRC-32 and size checked).
struct Gunzip<R: Read> {
    inner: R,
    unpacker: jadpo_runtime::rows::gzip::Unpacker,
    input: Vec<u8>,
    start: usize,
    end: usize,
    ended: bool,
    out: Vec<u8>,
    at: usize,
    done: bool,
}

impl<R: Read> Gunzip<R> {
    fn new(inner: R) -> Gunzip<R> {
        Gunzip {
            inner,
            unpacker: jadpo_runtime::rows::gzip::Unpacker::new(),
            input: vec![0; 1 << 16],
            start: 0,
            end: 0,
            ended: false,
            out: Vec::new(),
            at: 0,
            done: false,
        }
    }
}

impl<R: Read> Read for Gunzip<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.at < self.out.len() {
                let given = buf.len().min(self.out.len() - self.at);
                buf[..given].copy_from_slice(&self.out[self.at..self.at + given]);
                self.at += given;
                return Ok(given);
            }
            if self.done {
                return Ok(0);
            }
            if self.start == self.end && !self.ended {
                self.start = 0;
                self.end = self.inner.read(&mut self.input)?;
                self.ended = self.end == 0;
            }
            self.out.clear();
            self.at = 0;
            let unpacked = self
                .unpacker
                .unpack(
                    &self.input[self.start..self.end],
                    self.ended,
                    &mut self.out,
                    1 << 16,
                )
                .map_err(|fault| {
                    std::io::Error::other(format!("the archive's gzip is {fault:?}"))
                })?;
            self.start += unpacked.used;
            self.done = unpacked.done;
            if self.out.is_empty() && !self.done && self.ended && self.start == self.end {
                return Err(std::io::Error::other("the archive is cut short"));
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn the_manifest_digest_pinned_is_of_the_release_rust_toolchain_toml_pins() {
        assert_eq!(version(), MANIFEST_VERSION);
    }

    const MANIFEST: &str = "manifest-version = \"2\"\ndate = \"2026-10-01\"\n\n\
        [pkg.rustc.target.aarch64-apple-darwin]\navailable = true\n\
        url = \"https://static.rust-lang.org/dist/2026-10-01/rustc-1.99.0-aarch64-apple-darwin.tar.gz\"\n\
        hash = \"2f6b\"\nxz_url = \"https://static.rust-lang.org/x.tar.xz\"\nxz_hash = \"334a\"\n\n\
        [pkg.rustc.target.aarch64-pc-windows-gnullvm]\navailable = false\n\n\
        [pkg.rust-std.target.wasm32-unknown-unknown]\navailable = true\n\
        url = \"https://static.rust-lang.org/dist/2026-10-01/rust-std-1.99.0-wasm32-unknown-unknown.tar.gz\"\n\
        hash = \"4dff\"\n";

    #[test]
    fn an_archive_is_read_from_its_targets_section_when_it_is_available() {
        assert_eq!(
            archive(MANIFEST, "rustc", "aarch64-apple-darwin"),
            Some(Archive {
                url: "https://static.rust-lang.org/dist/2026-10-01/rustc-1.99.0-aarch64-apple-darwin.tar.gz".to_string(),
                hash: "2f6b".to_string(),
            })
        );
        assert_eq!(
            archive(MANIFEST, "rust-std", "wasm32-unknown-unknown").map(|archive| archive.hash),
            Some("4dff".to_string())
        );
        assert_eq!(
            archive(MANIFEST, "rustc", "aarch64-pc-windows-gnullvm"),
            None
        );
        assert_eq!(archive(MANIFEST, "cargo", "aarch64-apple-darwin"), None);
    }

    #[test]
    fn a_component_is_unpacked_without_the_installers_files_and_nothing_leaves_the_folder() {
        let folder = std::env::temp_dir().join(format!("jadpo-unpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let long = format!(
            "rustc-1.99.0-x/rustc/lib/rustlib/{}/bin/rust-lld",
            "t".repeat(90)
        );
        let archive = gzip(&tar(&[
            ("rustc-1.99.0-x/components", b"rustc\n", 0o644),
            (
                "rustc-1.99.0-x/rustc/manifest.in",
                b"file:bin/rustc\n",
                0o644,
            ),
            ("rustc-1.99.0-x/rustc/bin/rustc", b"#!rustc", 0o755),
            (&long, b"lld", 0o755),
        ]));
        unpack(&archive[..], &folder).expect("unpacked");
        assert_eq!(
            std::fs::read(folder.join("bin/rustc")).expect("rustc"),
            b"#!rustc"
        );
        assert!(!folder.join("manifest.in").exists());
        assert!(!folder.join("components").exists());
        let lld = folder.join(format!("lib/rustlib/{}/bin/rust-lld", "t".repeat(90)));
        assert_eq!(std::fs::read(&lld).expect("a long name"), b"lld");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.join("bin/rustc"))
                .expect("rustc")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let escaping = gzip(&tar(&[("x/rustc/../../../escaped", b"no", 0o644)]));
        assert!(unpack(&escaping[..], &folder).is_err());
        let cut = &archive[..archive.len() / 2];
        assert!(unpack(cut, &folder).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_number_is_octal_or_base_256() {
        assert_eq!(number(b"00000000644\0"), Some(0o644));
        assert_eq!(number(b"     755 \0"), Some(0o755));
        assert_eq!(
            number(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0]),
            Some(1 << 16)
        );
        assert_eq!(number(b"zz\0"), None);
    }

    /// A GNU tar of `entries`, a long name as a `././@LongLink` entry.
    pub(crate) fn tar(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        let header = |out: &mut Vec<u8>, name: &str, size: usize, mode: u32, kind: u8| {
            let mut block = [0u8; 512];
            let short = &name.as_bytes()[..name.len().min(100)];
            block[..short.len()].copy_from_slice(short);
            block[100..107].copy_from_slice(format!("{mode:07o}").as_bytes());
            block[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
            block[156] = kind;
            block[257..265].copy_from_slice(b"ustar  \0");
            out.extend_from_slice(&block);
        };
        let pad = |out: &mut Vec<u8>| out.resize(out.len().div_ceil(512) * 512, 0);
        for (name, bytes, mode) in entries {
            if name.len() > 100 {
                let long = [name.as_bytes(), b"\0"].concat();
                header(&mut out, "././@LongLink", long.len(), 0o644, b'L');
                out.extend_from_slice(&long);
                pad(&mut out);
            }
            header(&mut out, name, bytes.len(), *mode, b'0');
            out.extend_from_slice(bytes);
            pad(&mut out);
        }
        out.extend_from_slice(&[0; 1024]);
        out
    }

    pub(crate) fn gzip(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];
        out.extend(miniz_oxide::deflate::compress_to_vec(data, 1));
        out.extend(jadpo_runtime::rows::gzip::crc32(0, data).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out
    }

    /// A report that keeps what it is told.
    #[derive(Default)]
    struct Kept(std::sync::Mutex<Vec<String>>);

    impl Report for Kept {
        fn fetching(&self, words: &str) {
            self.0.lock().expect("kept").push(words.to_string());
        }
        fn progress(&self, _: &str, _: u64, _: Option<u64>) {}
    }

    /// A dist server on this machine serving `files`, each path's bytes.
    fn serve(files: Vec<(String, Vec<u8>)>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut head = [0; 2048];
                let read = stream.read(&mut head).unwrap_or(0);
                let request = String::from_utf8_lossy(&head[..read]).into_owned();
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                match files.iter().find(|(served, _)| *served == path) {
                    Some((_, bytes)) => {
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                            bytes.len()
                        );
                        let _ = stream.write_all(bytes);
                    }
                    None => {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                    }
                }
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    #[test]
    fn an_archive_whose_digest_isnt_the_manifests_is_refused_and_nothing_installed() {
        let version = version();
        let archive = gzip(&tar(&[(
            &format!("rustc-{version}-{HOST}/rustc/bin/rustc"),
            b"rustc",
            0o755,
        )]));
        let path = format!("/dist/2026-10-01/rustc-{version}-{HOST}.tar.gz");
        let manifest = |hash: &str| {
            format!(
                "[pkg.rustc.target.{HOST}]\navailable = true\nurl = \"{DEFAULT_SERVER}{path}\"\nhash = \"{hash}\"\n"
            )
        };
        let wrong = manifest(&"0".repeat(64));
        let server = serve(vec![
            (
                format!("/dist/channel-rust-{version}.toml"),
                wrong.clone().into_bytes(),
            ),
            (
                format!("/dist/channel-rust-{version}.toml.sha256"),
                format!(
                    "{}  channel-rust-{version}.toml\n",
                    sha256(wrong.as_bytes())
                )
                .into_bytes(),
            ),
            (path, archive),
        ]);
        let cache = std::env::temp_dir().join(format!("jadpo-toolchain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        let kept = Kept::default();
        let refused = ensure(&cache, false, &server, &kept).expect_err("a wrong digest");
        assert!(
            refused.contains("isn't the archive the manifest names"),
            "{refused}"
        );
        assert_eq!(
            *kept.0.lock().expect("kept"),
            [format!("Fetching Rust {version} for {HOST}")]
        );
        let folder = cache.join(format!("toolchains/{version}-{HOST}"));
        assert!(!folder.join("bin/rustc").exists());
        assert!(!folder.join(".jadpo").exists());
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_manifest_whose_digest_isnt_the_one_beside_it_is_refused() {
        let version = version();
        let server = serve(vec![
            (
                format!("/dist/channel-rust-{version}.toml"),
                b"manifest-version = \"2\"\n".to_vec(),
            ),
            (
                format!("/dist/channel-rust-{version}.toml.sha256"),
                format!("{}  channel-rust-{version}.toml\n", "0".repeat(64)).into_bytes(),
            ),
        ]);
        let refused = manifest(&server, version).expect_err("a wrong digest");
        assert!(
            refused.contains("isn't the manifest this Jadpo expects"),
            "{refused}"
        );
    }
}
