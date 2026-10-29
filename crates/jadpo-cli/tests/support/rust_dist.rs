//! Local stand-ins for the two places a released `jadpo build` fetches
//! from (roadmap chunk 9.2c; docs/compiler-runtime.md §2.8, "Toolchain
//! pinning"), so its test never reaches the network (§2.7):
//!
//! - Rust's dist server: the pinned toolchain this test runs on, packed
//!   again as `static.rust-lang.org` packs a release, one `.tar.gz` for
//!   each of `rustc`, `cargo` and the host's and wasm32's `rust-std`, a
//!   channel manifest naming each with its SHA-256, and the manifest's
//!   own `.sha256` beside it;
//! - crates.io: a sparse registry of the crates in this checkout's
//!   `Cargo.lock`, its index lines and `.crate` files read from the cargo
//!   cache the test's own build filled, so each checksum is crates.io's.
//!
//! [`Server`] serves both over plain HTTP on the loopback address and
//! counts what it is asked for.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use miniz_oxide::deflate::CompressionLevel;
use miniz_oxide::deflate::core::CompressorOxide;
use miniz_oxide::deflate::stream::deflate;
use miniz_oxide::{DataFormat, MZFlush, MZStatus};
use sha2::{Digest, Sha256};

/// A file server on `127.0.0.1`: each path it serves and the file behind
/// it, and every path it was asked for.
#[derive(Debug)]
pub struct Server {
    pub port: u16,
    files: Arc<Mutex<BTreeMap<String, PathBuf>>>,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Server {
    pub fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let files: Arc<Mutex<BTreeMap<String, PathBuf>>> = Arc::default();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let (served, noted) = (Arc::clone(&files), Arc::clone(&asked));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (served, noted) = (Arc::clone(&served), Arc::clone(&noted));
                std::thread::spawn(move || answer(stream, &served, &noted));
            }
        });
        Server { port, files, asked }
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Serves `file` at `path`.
    pub fn serve(&self, path: &str, file: &Path) {
        self.files
            .lock()
            .expect("the files")
            .insert(path.to_string(), file.to_path_buf());
    }

    /// Every path asked for so far, in order.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("the paths").clone()
    }
}

/// One connection's requests, answered until it closes.
fn answer(stream: TcpStream, files: &Mutex<BTreeMap<String, PathBuf>>, asked: &Mutex<Vec<String>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
    let mut stream = stream;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let path = line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_string();
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
                break;
            }
        }
        asked.lock().expect("the paths").push(path.clone());
        let file = files.lock().expect("the files").get(&path).cloned();
        let written = match file.and_then(|file| File::open(file).ok()) {
            Some(mut file) => {
                let length = file.metadata().map_or(0, |meta| meta.len());
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .and_then(|()| std::io::copy(&mut file, &mut stream).map(|_| ()))
            }
            None => stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"),
        };
        if written.and_then(|()| stream.flush()).is_err() {
            return;
        }
    }
}

/// The date Rust's dist server files a release's archives under.
const DATE: &str = "2026-10-01";

/// Packs the toolchain at `sysroot` (a rustup toolchain's folder, which
/// keeps each component's file list in `lib/rustlib/manifest-<component>`)
/// as the dist server packs Rust `version`, into `out`, and serves it:
/// `rustc`, `cargo` and `rust-std` for `host`, and `rust-std` for wasm32.
pub fn toolchain(server: &Server, sysroot: &Path, version: &str, host: &str, out: &Path) {
    let wasm = "wasm32-unknown-unknown";
    let mut manifest = format!("manifest-version = \"2\"\ndate = \"{DATE}\"\n");
    for (package, target, component) in [
        ("cargo", host, "cargo".to_string()),
        ("rust-std", host, format!("rust-std-{host}")),
        ("rust-std", wasm, format!("rust-std-{wasm}")),
        ("rustc", host, "rustc".to_string()),
    ] {
        let installed = match package {
            "rust-std" => component.clone(),
            _ => format!("{package}-{host}"),
        };
        let list =
            std::fs::read_to_string(sysroot.join(format!("lib/rustlib/manifest-{installed}")))
                .unwrap_or_else(|error| {
                    panic!("{installed}'s file list in {}: {error}", sysroot.display())
                });
        let name = format!("{package}-{version}-{target}");
        let archive = out.join(format!("{name}.tar.gz"));
        let mut tar = Tar::new(Gzip::new(File::create(&archive).expect("an archive")));
        tar.file(
            &format!("{name}/components"),
            format!("{component}\n").as_bytes(),
            0o644,
        );
        tar.file(&format!("{name}/rust-installer-version"), b"3\n", 0o644);
        let mut listed = String::new();
        for line in list.lines() {
            let Some(path) = line.strip_prefix("file:") else {
                continue;
            };
            listed.push_str(&format!("file:{path}\n"));
            tar.copy(&format!("{name}/{component}/{path}"), &sysroot.join(path));
        }
        tar.file(
            &format!("{name}/{component}/manifest.in"),
            listed.as_bytes(),
            0o644,
        );
        tar.finish().finish();
        let hash = sha256(&std::fs::read(&archive).expect("the archive"));
        let path = format!("/dist/{DATE}/{name}.tar.gz");
        server.serve(&path, &archive);
        manifest.push_str(&format!(
            "\n[pkg.{package}.target.{target}]\navailable = true\nurl = \"https://static.rust-lang.org{path}\"\nhash = \"{hash}\"\n"
        ));
    }
    let file = format!("channel-rust-{version}.toml");
    let written = out.join(&file);
    std::fs::write(&written, &manifest).expect("the manifest");
    let sidecar = out.join(format!("{file}.sha256"));
    std::fs::write(
        &sidecar,
        format!("{}  {file}\n", sha256(manifest.as_bytes())),
    )
    .expect("its digest");
    server.serve(&format!("/dist/{file}"), &written);
    server.serve(&format!("/dist/{file}.sha256"), &sidecar);
}

/// Serves a sparse registry under `/index/` of every crates.io package
/// `lock` names, from the cargo cache in `cargo_home`: each crate's index
/// lines as cargo cached them, and its `.crate` file. Writes the index
/// into `out`.
pub fn registry(server: &Server, cargo_home: &Path, lock: &str, out: &Path) {
    let mut packages: Vec<(String, String)> = Vec::new();
    for entry in lock.split("[[package]]").skip(1) {
        let field = |key: &str| {
            entry.lines().find_map(|line| {
                line.strip_prefix(&format!("{key} = \""))
                    .and_then(|rest| rest.strip_suffix('"'))
                    .map(str::to_string)
            })
        };
        if field("source").is_some_and(|source| source.starts_with("registry+")) {
            packages.push((
                field("name").expect("a name"),
                field("version").expect("a version"),
            ));
        }
    }
    let within = |folder: &str| -> Vec<PathBuf> {
        std::fs::read_dir(cargo_home.join("registry").join(folder))
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default()
    };
    let (indexes, caches) = (within("index"), within("cache"));
    let mut named: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();
    for (name, _) in &packages {
        if named.contains_key(name) {
            continue;
        }
        // Each cargo release keeps its own cache of the index, so the
        // lines of every one are merged, a version once.
        let mut lines: Vec<Vec<u8>> = Vec::new();
        let mut versions = std::collections::BTreeSet::new();
        for index in &indexes {
            let Ok(cached) = std::fs::read(index.join(".cache").join(index_path(name))) else {
                continue;
            };
            for line in index_lines(&cached) {
                let text = String::from_utf8_lossy(&line).into_owned();
                let version = text
                    .split("\"vers\":\"")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .unwrap_or_default()
                    .to_string();
                if versions.insert(version) {
                    lines.push(line);
                }
            }
        }
        assert!(
            !lines.is_empty(),
            "no index entry for `{name}` in {}",
            cargo_home.display()
        );
        named.insert(name.clone(), lines);
    }
    for (name, lines) in &named {
        let path = out.join(index_path(name));
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("index folders");
        std::fs::write(&path, [lines.join(&b"\n"[..]), b"\n".to_vec()].concat()).expect("index");
        server.serve(&format!("/index/{}", index_path(name)), &path);
    }
    let config = out.join("config.json");
    std::fs::write(
        &config,
        format!(
            "{{\"dl\":\"{}/crates/{{crate}}/{{version}}/download\"}}",
            server.url()
        ),
    )
    .expect("the registry's configuration");
    server.serve("/index/config.json", &config);
    for (name, version) in &packages {
        if let Some(file) = caches
            .iter()
            .map(|cache| cache.join(format!("{name}-{version}.crate")))
            .find(|file| file.exists())
        {
            server.serve(&format!("/crates/{name}/{version}/download"), &file);
        }
    }
}

/// Where the index keeps crate `name`: `1/a`, `3/s/syn`, `se/rd/serde`.
fn index_path(name: &str) -> String {
    let name = name.to_lowercase();
    match name.len() {
        1 | 2 => format!("{}/{name}", name.len()),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}

/// The index lines in one of cargo's cached index entries: its format's
/// version byte,
/// the index's version, the entry's revision ending in a zero byte, then
/// each version and its line, each ending in a zero byte.
fn index_lines(cached: &[u8]) -> Vec<Vec<u8>> {
    // The third version of the format, which cargo has written since 1.66.
    if cached.first() != Some(&3) || cached.len() < 5 {
        return Vec::new();
    }
    let parts: Vec<&[u8]> = cached[5..].split(|byte| *byte == 0).collect();
    parts[1..]
        .chunks(2)
        .filter(|pair| pair.len() == 2 && !pair[1].is_empty())
        .map(|pair| pair[1].to_vec())
        .collect()
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A tar file as the dist server writes one: GNU headers, a name longer
/// than a header holds given first as a `././@LongLink` entry.
pub struct Tar<W: Write> {
    out: W,
}

impl<W: Write> Tar<W> {
    pub fn new(out: W) -> Tar<W> {
        Tar { out }
    }

    /// Adds the file `name` holding `bytes`.
    pub fn file(&mut self, name: &str, bytes: &[u8], mode: u32) {
        self.header(name, bytes.len() as u64, mode, b'0');
        self.out.write_all(bytes).expect("written");
        self.pad(bytes.len() as u64);
    }

    /// Adds the file at `from` as `name`, keeping whether it is executable.
    pub fn copy(&mut self, name: &str, from: &Path) {
        let meta =
            std::fs::metadata(from).unwrap_or_else(|error| panic!("{}: {error}", from.display()));
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 != 0 {
                0o755
            } else {
                0o644
            }
        };
        #[cfg(not(unix))]
        let mode = 0o644;
        self.header(name, meta.len(), mode, b'0');
        std::io::copy(&mut File::open(from).expect("a file"), &mut self.out).expect("copied");
        self.pad(meta.len());
    }

    fn header(&mut self, name: &str, size: u64, mode: u32, kind: u8) {
        if name.len() > 100 {
            let long = [name.as_bytes(), b"\0"].concat();
            self.header("././@LongLink", long.len() as u64, 0o644, b'L');
            self.out.write_all(&long).expect("written");
            self.pad(long.len() as u64);
        }
        let mut block = [0u8; 512];
        let short = &name.as_bytes()[..name.len().min(100)];
        block[..short.len()].copy_from_slice(short);
        octal(&mut block[100..108], u64::from(mode));
        octal(&mut block[108..116], 0);
        octal(&mut block[116..124], 0);
        octal(&mut block[124..136], size);
        octal(&mut block[136..148], 0);
        block[156] = kind;
        block[257..265].copy_from_slice(b"ustar  \0");
        block[148..156].fill(b' ');
        let sum: u32 = block.iter().map(|byte| u32::from(*byte)).sum();
        octal(&mut block[148..155], u64::from(sum));
        self.out.write_all(&block).expect("written");
    }

    fn pad(&mut self, size: u64) {
        let rest = (512 - size % 512) % 512;
        self.out
            .write_all(&vec![0; rest as usize])
            .expect("written");
    }

    /// Ends the archive: what it was written to.
    pub fn finish(mut self) -> W {
        self.out.write_all(&[0; 1024]).expect("written");
        self.out
    }
}

/// `value` in octal, zero-padded, ending in a zero byte.
fn octal(field: &mut [u8], value: u64) {
    let text = format!("{value:0width$o}", width = field.len() - 1);
    field[..text.len()].copy_from_slice(text.as_bytes());
    field[field.len() - 1] = 0;
}

/// A gzip file written as it goes (RFC 1952): one member, deflated at
/// miniz_oxide's fastest level.
pub struct Gzip<W: Write> {
    out: W,
    compressor: Box<CompressorOxide>,
    crc: u32,
    size: u32,
    buffer: Vec<u8>,
}

impl<W: Write> Gzip<W> {
    pub fn new(mut out: W) -> Gzip<W> {
        out.write_all(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255])
            .expect("written");
        Gzip {
            out,
            compressor: Box::new(CompressorOxide::with_format_and_level(
                DataFormat::Raw,
                CompressionLevel::BestSpeed,
            )),
            crc: 0,
            size: 0,
            buffer: vec![0; 1 << 16],
        }
    }

    fn pump(&mut self, mut input: &[u8], flush: MZFlush) -> std::io::Result<()> {
        loop {
            let result = deflate(&mut self.compressor, input, &mut self.buffer, flush);
            self.out.write_all(&self.buffer[..result.bytes_written])?;
            input = &input[result.bytes_consumed..];
            match result.status {
                Ok(MZStatus::StreamEnd) => return Ok(()),
                Ok(_) if flush != MZFlush::Finish && input.is_empty() => return Ok(()),
                Ok(_) => {}
                Err(error) => return Err(std::io::Error::other(format!("{error:?}"))),
            }
        }
    }

    /// Ends the member with its CRC-32 and size.
    pub fn finish(mut self) -> W {
        self.pump(&[], MZFlush::Finish).expect("deflated");
        self.out
            .write_all(&self.crc.to_le_bytes())
            .expect("written");
        self.out
            .write_all(&self.size.to_le_bytes())
            .expect("written");
        self.out
    }
}

impl<W: Write> Write for Gzip<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.crc = jadpo_runtime::rows::gzip::crc32(self.crc, bytes);
        self.size = self.size.wrapping_add(bytes.len() as u32);
        self.pump(bytes, MZFlush::None)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}
