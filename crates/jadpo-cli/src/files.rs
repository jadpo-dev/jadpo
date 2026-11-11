//! `jadpo files` (area 14, "Commands for services, calls, email and stored
//! data"; area 16, "The disk driver" and "The S3 driver"): a store of
//! files from the command line, on any driver, in production or in
//! development.
//!
//! - `jadpo files` lists the app's stores of files: each one's driver and
//!   where it keeps its files.
//! - `ls <store>` lists the files a store keeps, with their sizes: every
//!   file's key and every key of the erasure log, never an upload still
//!   waiting.
//! - `show <store> <key>` reads one file through and says its size, the
//!   type its first bytes look like and its hash.
//! - `get <store> <key> <path>` writes a file's bytes to a local file,
//!   which it never overwrites.
//! - `put <store> <key> <path>` puts a local file's bytes back under a
//!   file's key, which must hold nothing yet, since files never change.
//! - `copy <from> <to>` copies every file and the erasure log from one
//!   store into another, leaving those already there of the same size, so
//!   it can be run again to catch up: how an app moves off one server.
//!
//! A store is named by its connection in `app.jadpo` and reached as the
//! deployed app reaches it: a disk store at its `path:`, an S3 store with
//! the settings the environment gives, over `.env.local`. `--dev` reaches
//! `jadpo dev`'s copy under `.jadpo/files/<store>/` instead. `copy` also
//! takes a folder written in full for a disk store no connection names
//! any more. A credential is never printed.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_diag::{Diagnostic, rule};
use jadpo_runtime::capability::{Answer, Call as StoreCall, Capability, Repeat};
use jadpo_runtime::config::Variables;
use jadpo_std::connections::{Connection, ConnectionDriver, Said};
use jadpo_std::files::{JOIN, KEYS, NOT_FOUND, PUT, READ, STORAGE_FULL, is_key, key_id, piece_key};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Table};
use jadpo_value::Value;

use crate::run::{Call, Reply};

pub(crate) const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "ls",
        description: "List the files a store keeps",
    },
    crate::table::Value {
        name: "show",
        description: "Show one file's size, type and hash",
    },
    crate::table::Value {
        name: "get",
        description: "Write a file's bytes to a local file",
    },
    crate::table::Value {
        name: "put",
        description: "Put a local file back under a file's key",
    },
    crate::table::Value {
        name: "copy",
        description: "Copy every file from one store into another",
    },
];

/// How much of a file one read asks a store for.
const WINDOW: u64 = 1 << 20;

/// The most of a file kept in one piece: a larger file is put in pieces
/// of this size and joined in the store, so no more than a piece is held
/// in memory. Under what a store's answer may hold, which an S3 join
/// reads each piece as, and over the least part S3 takes.
const PIECE: u64 = 8 << 20;

pub fn files(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_needed("files");
    };
    let arguments = &call.arguments;
    let action = arguments.first().map(String::as_str);
    let dev = call.flag("--dev");
    if dev && action == Some("copy") {
        return Reply::failed(problem(
            "`jadpo files copy` takes no `--dev`".to_string(),
            "It copies between the stores it names: a connection as the deployed app reaches it, or a disk store's folder written in full.".to_string(),
            "Name `jadpo dev`'s copy by its folder, `<project>/.jadpo/files/<store>`".to_string(),
        ));
    }
    let wants = |count: usize, usage: &str| -> Result<(), Reply> {
        if arguments.len() == count + 1 {
            return Ok(());
        }
        Err(Reply::failed(problem(
            format!("`jadpo files {usage}` takes {count} arguments"),
            format!(
                "It was given {}.",
                match arguments.len().saturating_sub(1) {
                    0 => "none".to_string(),
                    n => format!(
                        "{n}: `{}`",
                        arguments.get(1..).unwrap_or_default().join(" ")
                    ),
                }
            ),
            format!("Run `jadpo files {usage}`"),
        )))
    };
    let project = || Project::load(&root, &cwd);
    let done = match action {
        None => project().map(|project| stores(&project)),
        Some("ls") => wants(1, "ls <store>").and_then(|()| {
            let project = project()?;
            let store = project.open(call, &arguments[1], dev, Side::Read)?;
            ls(&store)
        }),
        Some("show") => wants(2, "show <store> <key>").and_then(|()| {
            let project = project()?;
            let store = project.open(call, &arguments[1], dev, Side::Read)?;
            show(&store, &arguments[2])
        }),
        Some("get") => wants(3, "get <store> <key> <path>").and_then(|()| {
            let project = project()?;
            let store = project.open(call, &arguments[1], dev, Side::Read)?;
            get(
                &store,
                &arguments[2],
                &cwd.join(&arguments[3]),
                &arguments[3],
            )
        }),
        Some("put") => wants(3, "put <store> <key> <path>").and_then(|()| {
            let project = project()?;
            let store = project.open(call, &arguments[1], dev, Side::Write)?;
            put(
                &store,
                &arguments[2],
                &cwd.join(&arguments[3]),
                &arguments[3],
            )
        }),
        Some("copy") => wants(2, "copy <from> <to>").and_then(|()| {
            let project = project()?;
            let from = project.open(call, &arguments[1], false, Side::Read)?;
            let to = project.open(call, &arguments[2], false, Side::Write)?;
            copy(&from, &to)
        }),
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let near = jadpo_diag::nearest(other, names.iter().copied());
            Err(Reply::failed(problem(
                format!("`{other}` isn't something `jadpo files` does"),
                format!(
                    "`jadpo files` lists the app's stores of files, and {} act on one.{}",
                    crate::run::both(&crate::run::quoted(names.iter())),
                    near.map(|near| format!(" Did you mean `{near}`?"))
                        .unwrap_or_default()
                ),
                match near {
                    Some(near) => format!("Use `jadpo files {near}`"),
                    None => "Run `jadpo files` to list the stores".to_string(),
                },
            )))
        }
    };
    done.unwrap_or_else(|reply| reply)
}

fn problem(headline: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_FILES_ARGUMENTS, headline)
        .reason(reason)
        .next(next)
}

/// `files.unavailable`: the store couldn't be reached, read or written.
fn unavailable(headline: String, reason: String, next: String) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::FILES_UNAVAILABLE, headline)
            .reason(reason)
            .next(next),
    )
}

// --- the project and its stores ------------------------------------------------

/// The project's stores of files, as its plan holds them, and the
/// settings this machine gives them.
struct Project {
    root: PathBuf,
    stores: Vec<Connection>,
    config: jadpo_std::config::ConfigPlan,
}

/// Whether a command reads a store or writes to it: a disk store's folder
/// is made only for a write, so reading one that isn't on this machine
/// says so rather than making it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Read,
    Write,
}

/// One store opened: its driver, the connection name its calls go
/// through, and how it is shown.
struct Store {
    driver: Rc<dyn Capability>,
    connection: String,
    /// `imports`, or the folder written in full.
    name: String,
    /// `disk`, `s3`, or `jadpo dev`'s copy.
    driver_word: &'static str,
    /// Where it keeps its files: a folder, or a bucket at a service.
    place: String,
    runtime: jadpo_host_native::Runtime,
}

impl Project {
    fn load(root: &Path, cwd: &Path) -> Result<Project, Reply> {
        let loaded = jadpo_db::load(root);
        let prepared = match jadpo_interp::Prepared::load(&loaded.db, loaded.project) {
            Ok(prepared) => prepared,
            Err(problems) => {
                return Err(crate::migrate::blocked(
                    &loaded,
                    cwd,
                    problems,
                    "files stopped",
                ));
            }
        };
        let plan = prepared.plan();
        let stores = plan
            .connections()
            .iter()
            .filter(|connection| {
                matches!(
                    connection.driver,
                    ConnectionDriver::Disk { .. } | ConnectionDriver::S3(_)
                )
            })
            .cloned()
            .collect();
        Ok(Project {
            root: loaded.root.clone(),
            stores,
            config: plan.config().clone(),
        })
    }

    fn named(&self, written: &str) -> Result<&Connection, Reply> {
        if let Some(found) = self.stores.iter().find(|store| store.name == written) {
            return Ok(found);
        }
        let names: Vec<&str> = self
            .stores
            .iter()
            .map(|store| store.name.as_str())
            .collect();
        let near = jadpo_diag::nearest(written, names.iter().copied());
        Err(Reply::failed(problem(
            format!("`{written}` isn't a store of files"),
            match names.as_slice() {
                [] => "A store of files is a connection in `app.jadpo` whose driver keeps files, `Driver.disk` or `Driver.s3`, and this app has none.".to_string(),
                _ => format!(
                    "A store is named by its connection in `app.jadpo`: {}.",
                    crate::run::either(&crate::run::quoted(names.iter())),
                ),
            },
            match near {
                Some(near) => format!("Use `{near}`"),
                None => "Run `jadpo files` to list the stores".to_string(),
            },
        )))
    }

    /// The store `written` names: a connection, or for `copy` a folder
    /// written in full; `jadpo dev`'s copy when `dev`.
    fn open(&self, call: &Call, written: &str, dev: bool, side: Side) -> Result<Store, Reply> {
        let runtime = jadpo_host_native::Runtime::new().map_err(|error| {
            unavailable(
                format!("`{written}` couldn't be opened"),
                format!("The runtime that reaches a store couldn't start: {error}."),
                "Free some memory, then run it again".to_string(),
            )
        })?;
        if Path::new(written).is_absolute() {
            if dev {
                return Err(Reply::failed(problem(
                    format!("`{written}` is a folder, which `--dev` doesn't take"),
                    "`--dev` reaches `jadpo dev`'s copy of a store named by its connection."
                        .to_string(),
                    "Leave out `--dev`, or name the store by its connection".to_string(),
                )));
            }
            return disk(
                written,
                written,
                written,
                jadpo_std::files::DEFAULT_FREE_SPACE_FLOOR,
                side,
                runtime,
            );
        }
        let connection = self.named(written)?;
        if dev {
            let folder = self.root.join(".jadpo").join("files");
            return Ok(Store {
                driver: Rc::new(jadpo_host_native::files::DiskFiles::under(folder)),
                connection: connection.name.clone(),
                name: connection.name.clone(),
                driver_word: "jadpo dev",
                place: format!(".jadpo/files/{}", connection.name),
                runtime,
            });
        }
        match &connection.driver {
            ConnectionDriver::Disk { path, floor } => {
                let Some(path) = path else {
                    return Err(unavailable(
                        format!("`{}` has no folder", connection.name),
                        "A disk store keeps its files at its `path:`, and this one gives none."
                            .to_string(),
                        format!(
                            "Give `{}` a `path:` in `app.jadpo`, or add `--dev` for `jadpo dev`'s copy",
                            connection.name
                        ),
                    ));
                };
                disk(
                    &connection.name,
                    &connection.name,
                    path,
                    *floor,
                    side,
                    runtime,
                )
            }
            ConnectionDriver::S3(bucket) => {
                let layered = crate::config::Layered::new(&self.root, &call.environment)
                    .map_err(|problem| Reply::failed(*problem))?;
                let setting = |at: u32| -> Result<String, Reply> {
                    let field = self.config.fields.get(at as usize);
                    let name = field.map_or("?", |field| field.name.as_str());
                    let variable = field.map_or("?", |field| field.variable.as_str());
                    layered.variable(variable).ok_or_else(|| {
                        unavailable(
                            format!("`{}`'s setting `{name}` has no value here", connection.name),
                            format!(
                                "An S3 store is reached with the settings the deployed app reads, from the environment and then `.env.local`, and `{variable}` gives none."
                            ),
                            format!(
                                "Set `{variable}` as the deployment does, or add `--dev` for `jadpo dev`'s copy"
                            ),
                        )
                    })
                };
                let said = |value: &Said| -> Result<String, Reply> {
                    match value {
                        Said::Written(text) => Ok(text.clone()),
                        Said::Setting(at) => setting(*at),
                    }
                };
                let host = said(&bucket.host)?;
                let region = said(&bucket.region)?;
                let name = said(&bucket.bucket)?;
                let key_id = setting(bucket.key_id)?;
                let secret = setting(bucket.secret)?;
                let supplies = jadpo_host_native::supplies();
                let (Some(fetch), Some(clock)) = (supplies.fetch, supplies.clock) else {
                    return Err(unavailable(
                        format!("`{}` couldn't be opened", connection.name),
                        "This build of Jadpo has no HTTP to reach an S3 service with.".to_string(),
                        "Use a release of Jadpo".to_string(),
                    ));
                };
                let store = jadpo_drivers::s3::S3Store::new(
                    fetch,
                    clock,
                    (&host, &region, &name),
                    (&key_id, &secret),
                )
                .map_err(|why| {
                    unavailable(
                        format!("`{}` couldn't be opened", connection.name),
                        format!("Its settings don't make a store: {why}."),
                        format!("Correct `{}`'s settings", connection.name),
                    )
                })?;
                Ok(Store {
                    driver: Rc::new(store),
                    connection: connection.name.clone(),
                    name: connection.name.clone(),
                    driver_word: "s3",
                    place: format!("bucket {name} at {host}"),
                    runtime,
                })
            }
            _ => Err(Reply::failed(problem(
                format!("`{written}` isn't a store of files"),
                "Only a connection whose driver keeps files has files to list.".to_string(),
                "Run `jadpo files` to list the stores".to_string(),
            ))),
        }
    }
}

/// The disk store `name` at the folder `path`, its calls through
/// `connection`: refused for a read when the folder isn't on this machine,
/// which a write makes, the app's user's alone, as a deployable does.
fn disk(
    name: &str,
    connection: &str,
    path: &str,
    floor: u32,
    side: Side,
    runtime: jadpo_host_native::Runtime,
) -> Result<Store, Reply> {
    if side == Side::Read && !Path::new(path).is_dir() {
        return Err(unavailable(
            format!("The folder `{path}` isn't on this machine"),
            format!(
                "`{name}` keeps its files on the server's own disk, at `{path}`, and there is no such folder here."
            ),
            "Run the command on the server that keeps the files, or add `--dev` for `jadpo dev`'s copy".to_string(),
        ));
    }
    let store = jadpo_host_native::files::DiskStore::at(path, floor).map_err(|why| {
        unavailable(
            format!("`{name}` couldn't be opened"),
            format!("Its folder can't be used: {why}."),
            "Make the folder the app's user's alone, then run the command again".to_string(),
        )
    })?;
    Ok(Store {
        driver: Rc::new(store),
        connection: connection.to_string(),
        name: name.to_string(),
        driver_word: "disk",
        place: path.to_string(),
        runtime,
    })
}

impl Store {
    /// `operation` sent with `input`, and its answer.
    fn ask(&self, operation: &str, input: Vec<(&str, Value)>) -> Answer {
        let input = Value::record(
            "FilesCall",
            input
                .into_iter()
                .map(|(name, value)| (Rc::from(name), value))
                .collect(),
        );
        let call = StoreCall::new(&self.connection, operation, input, Repeat::Safe);
        self.runtime.block_on(self.driver.call(&call))
    }

    /// `files.unavailable` for an answer that isn't what was asked.
    fn refused(&self, doing: &str, answer: &Answer) -> Reply {
        let (reason, next) = match answer {
            Answer::Failed(code) if code == STORAGE_FULL => (
                format!("`{}` has no room for it.", self.name),
                "Free space in the store, then run the command again".to_string(),
            ),
            Answer::Misconfigured => (
                format!(
                    "`{}`'s service refused its credential or settings.",
                    self.name
                ),
                format!(
                    "Check `{}`'s settings, then run the command again",
                    self.name
                ),
            ),
            _ => (
                format!("`{}`, {}, couldn't be reached.", self.name, self.place),
                "Check the store is up and reachable from here, then run the command again"
                    .to_string(),
            ),
        };
        unavailable(format!("`{}` couldn't {doing}", self.name), reason, next)
    }

    /// Every key it lists, with its size, in order.
    fn keys(&self) -> Result<Vec<(String, u64)>, Reply> {
        let mut out = Vec::new();
        let mut after = String::new();
        loop {
            let answer = self.ask(KEYS, vec![("after", Value::text(&after))]);
            let Answer::Value(value) = &answer else {
                return Err(self.refused("list its files", &answer));
            };
            let Some((keys, next)) = jadpo_runtime::files::keys_of(value) else {
                return Err(self.refused("list its files", &Answer::Unknown));
            };
            out.extend(keys);
            match next {
                Some(next) if next > after => after = next,
                _ => return Ok(out),
            }
        }
    }

    /// Reads file `key` through, a window at a time, handing each to
    /// `take`: its size; `None` when the store holds no such file.
    fn read(
        &self,
        key: &str,
        mut take: impl FnMut(&[u8]) -> Result<(), Reply>,
    ) -> Result<Option<u64>, Reply> {
        let mut from = 0u64;
        loop {
            let answer = self.ask(
                READ,
                vec![
                    ("key", Value::text(key)),
                    ("from", Value::Int(i64::try_from(from).unwrap_or(i64::MAX))),
                    ("length", Value::Int(WINDOW as i64)),
                ],
            );
            let bytes = match &answer {
                Answer::Value(Value::Bytes(bytes)) => bytes.clone(),
                Answer::Failed(code) if code == NOT_FOUND && from == 0 => return Ok(None),
                _ => return Err(self.refused("read its files", &answer)),
            };
            take(&bytes)?;
            from += bytes.len() as u64;
            if (bytes.len() as u64) < WINDOW {
                return Ok(Some(from));
            }
        }
    }

    /// Whether it holds a file under `key`.
    fn holds(&self, key: &str) -> Result<bool, Reply> {
        let answer = self.ask(
            READ,
            vec![
                ("key", Value::text(key)),
                ("from", Value::Int(0)),
                ("length", Value::Int(1)),
            ],
        );
        match &answer {
            Answer::Value(_) => Ok(true),
            Answer::Failed(code) if code == NOT_FOUND => Ok(false),
            _ => Err(self.refused("read its files", &answer)),
        }
    }

    /// Keeps `size` bytes under `key`, read in pieces by `next`: whole for
    /// a small file, and for a large file's key in pieces of
    /// [`PIECE`] joined into it, so no more than a piece is held in
    /// memory and every S3-compatible service takes them.
    fn send(
        &self,
        key: &str,
        size: u64,
        media: &str,
        mut next: impl FnMut(u64) -> Result<Vec<u8>, Reply>,
    ) -> Result<(), Reply> {
        let pieced = key_id(key).filter(|_| size > PIECE);
        let Some(id) = pieced else {
            let bytes = next(size)?;
            let answer = self.ask(
                PUT,
                vec![
                    ("key", Value::text(key)),
                    ("type", Value::text(media)),
                    ("bytes", Value::Bytes(Rc::from(bytes))),
                ],
            );
            return match answer {
                Answer::Value(_) => Ok(()),
                other => Err(self.refused(&format!("keep `{key}`"), &other)),
            };
        };
        let mut pieces = 0u64;
        let mut sent = 0u64;
        while sent < size {
            let bytes = next(PIECE.min(size - sent))?;
            if bytes.is_empty() {
                break;
            }
            sent += bytes.len() as u64;
            let answer = self.ask(
                PUT,
                vec![
                    ("key", Value::text(&piece_key(&id, pieces))),
                    ("type", Value::text(media)),
                    ("bytes", Value::Bytes(Rc::from(bytes))),
                ],
            );
            if !matches!(answer, Answer::Value(_)) {
                return Err(self.refused(&format!("keep `{key}`"), &answer));
            }
            pieces += 1;
        }
        let answer = self.ask(
            JOIN,
            vec![
                ("key", Value::text(key)),
                (
                    "pieces",
                    Value::Int(i64::try_from(pieces).unwrap_or(i64::MAX)),
                ),
                ("type", Value::text(media)),
            ],
        );
        match answer {
            Answer::Value(_) => Ok(()),
            other => Err(self.refused(&format!("keep `{key}`"), &other)),
        }
    }

    /// `imports on disk at /var/lib/shop/imports`.
    fn described(&self) -> String {
        match self.driver_word {
            "jadpo dev" => format!("{}, jadpo dev's copy at {}", self.name, self.place),
            "s3" => format!("{} on S3, {}", self.name, self.place),
            _ if self.name == self.place => format!("the folder {}", self.place),
            _ => format!("{} on disk at {}", self.name, self.place),
        }
    }

    /// `imports on disk`.
    fn short(&self) -> String {
        match self.driver_word {
            "jadpo dev" => format!("{}, jadpo dev's copy", self.name),
            "s3" => format!("{} on S3", self.name),
            _ if self.name == self.place => format!("the folder {}", self.place),
            _ => format!("{} on disk", self.name),
        }
    }

    fn json(&self) -> Json {
        Json::object([
            ("store", Json::from(self.name.as_str())),
            ("driver", Json::from(self.driver_word)),
            ("where", Json::from(self.place.as_str())),
        ])
    }
}

/// What a key is: a file's, or the erasure log's.
fn kind_of(key: &str) -> &'static str {
    if is_key(key) { "file" } else { "erasure log" }
}

/// The media type a file's first bytes look like, for a store that
/// keeps one beside it; `application/octet-stream` when they could be
/// several.
fn media_of(head: &[u8]) -> &'static str {
    jadpo_std::files::type_of(head, None, &[])
        .and_then(jadpo_runtime::files::media_of)
        .unwrap_or("application/octet-stream")
}

/// A size as a person reads it: `31 bytes`, `1.2 MB`, `2.3 GB`, in SI
/// units as area 16 writes sizes; `--json` gives the bytes.
fn readable(bytes: u64) -> String {
    let units = [(1_000_000_000u64, "GB"), (1_000_000, "MB"), (1_000, "kB")];
    for (unit, name) in units {
        if bytes >= unit {
            let tenths = (bytes * 10 + unit / 2) / unit;
            return if tenths.is_multiple_of(10) {
                format!("{} {name}", tenths / 10)
            } else {
                format!("{}.{} {name}", tenths / 10, tenths % 10)
            };
        }
    }
    if bytes == 1 {
        "1 byte".to_string()
    } else {
        format!("{bytes} bytes")
    }
}

fn int(n: u64) -> Json {
    Json::from(i64::try_from(n).unwrap_or(i64::MAX))
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

// --- the stores ------------------------------------------------------------------

fn stores(project: &Project) -> Reply {
    let data = Json::object([(
        "stores",
        Json::array(project.stores.iter().map(|store| {
            let (driver, place) = place_of(store);
            Json::object([
                ("store", Json::from(store.name.as_str())),
                ("driver", Json::from(driver)),
                ("where", Json::from(place.as_str())),
            ])
        })),
    )]);
    let mut screen = Screen::new();
    if project.stores.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "No stores of files".to_string(),
            Vec::new(),
        ));
        return Reply::screen(screen, data);
    }
    let mut table = Table::new(["store", "driver", "where"]).cut(2);
    for store in &project.stores {
        let (driver, place) = place_of(store);
        table = table.row(vec![
            Cell::text(store.name.clone()),
            Cell::text(driver),
            Cell::muted(place),
        ]);
    }
    screen.push(Block::Table(table));
    screen.push(
        Block::summary(
            Outcome::Success,
            count(project.stores.len(), "store", "stores"),
            Vec::new(),
        )
        .with_plain_headline("files"),
    );
    screen.push(Block::hint(
        format!("jadpo files ls {}", project.stores[0].name),
        "lists the files it keeps",
    ));
    Reply::screen(screen, data)
}

/// A store's driver and where it keeps its files, as its connection says,
/// a setting named rather than read.
fn place_of(store: &Connection) -> (&'static str, String) {
    match &store.driver {
        ConnectionDriver::Disk { path, .. } => (
            "disk",
            path.clone().unwrap_or_else(|| "no path".to_string()),
        ),
        ConnectionDriver::S3(bucket) => {
            let said = |value: &Said| match value {
                Said::Written(text) => text.clone(),
                Said::Setting(_) => "a setting".to_string(),
            };
            (
                "s3",
                format!("bucket {} at {}", said(&bucket.bucket), said(&bucket.host)),
            )
        }
        _ => ("", String::new()),
    }
}

// --- ls ---------------------------------------------------------------------------

fn ls(store: &Store) -> Result<Reply, Reply> {
    let keys = store.keys()?;
    let total: u64 = keys.iter().map(|(_, size)| size).sum();
    let mut data = vec![
        ("store", store.json()),
        (
            "files",
            Json::array(keys.iter().map(|(key, size)| {
                Json::object([
                    ("key", Json::from(key.as_str())),
                    ("size", int(*size)),
                    ("kind", Json::from(kind_of(key))),
                ])
            })),
        ),
    ];
    data.push(("bytes", int(total)));
    let data = Json::object(data);
    let mut screen = Screen::new();
    if keys.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "No files".to_string(),
            vec![Fact::plain(store.described())],
        ));
        return Ok(Reply::screen(screen, data));
    }
    let mut table = Table::new(["key", "size", "kind"]).right(1);
    for (key, size) in &keys {
        table = table.row(vec![
            Cell::text(key.clone()),
            Cell::text(readable(*size)),
            Cell::muted(kind_of(key)),
        ]);
    }
    screen.push(Block::Table(table));
    screen.push(
        Block::summary(
            Outcome::Success,
            count(keys.len(), "file", "files"),
            vec![Fact::plain(readable(total)), Fact::plain(store.short())],
        )
        .with_plain_headline("files"),
    );
    if let Some((key, _)) = keys.iter().find(|(key, _)| is_key(key)) {
        screen.push(Block::hint(
            format!("jadpo files show {} {key}", store.name),
            "reads one through",
        ));
    }
    Ok(Reply::screen(screen, data))
}

// --- show -------------------------------------------------------------------------

/// The key `written`, when a store could hold a file under it.
fn a_key(store: &Store, written: &str) -> Result<(), Reply> {
    if jadpo_std::files::is_listed(written) {
        return Ok(());
    }
    Err(Reply::failed(problem(
        format!("`{written}` isn't a file's key"),
        "A file is kept under the key the runtime made for it, `3f/a2/3fa2…`, which `jadpo files ls` lists.".to_string(),
        format!("Run `jadpo files ls {}` to see its keys", store.name),
    )))
}

fn no_file(store: &Store, key: &str) -> Reply {
    Reply::failed(problem(
        format!("`{}` holds no file `{key}`", store.name),
        format!("Nothing is kept under that key in {}.", store.described()),
        format!("Run `jadpo files ls {}` to see its keys", store.name),
    ))
}

fn show(store: &Store, key: &str) -> Result<Reply, Reply> {
    a_key(store, key)?;
    let mut head: Vec<u8> = Vec::new();
    let mut hasher = jadpo_runtime::files::Hashing::new();
    let read = store.read(key, |bytes| {
        if head.len() < jadpo_std::files::HEAD_BYTES {
            let room = jadpo_std::files::HEAD_BYTES - head.len();
            head.extend_from_slice(&bytes[..bytes.len().min(room)]);
        }
        hasher.update(bytes);
        Ok(())
    })?;
    let Some(size) = read else {
        return Err(no_file(store, key));
    };
    let file_type = jadpo_std::files::type_of(&head, None, &[]);
    let hash = hasher.finish();
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: key.to_string(),
        rows: vec![
            ("store".to_string(), store.described()),
            ("kind".to_string(), kind_of(key).to_string()),
            ("size".to_string(), readable(size)),
            (
                "type".to_string(),
                file_type.map_or_else(
                    || "can't be told from its first bytes".to_string(),
                    |variant| format!("FileType.{variant}, from its first bytes"),
                ),
            ),
            ("hash".to_string(), hash.clone()),
        ],
    });
    Ok(Reply::screen(
        screen,
        Json::object([
            ("store", store.json()),
            ("key", Json::from(key)),
            ("kind", Json::from(kind_of(key))),
            ("size", int(size)),
            ("type", file_type.map_or(Json::Null, Json::from)),
            ("hash", Json::from(hash.as_str())),
        ]),
    ))
}

// --- get --------------------------------------------------------------------------

fn get(store: &Store, key: &str, path: &Path, shown: &str) -> Result<Reply, Reply> {
    a_key(store, key)?;
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(Reply::failed(problem(
            format!("`{shown}` is there already"),
            "`jadpo files get` writes a new file, and never replaces one.".to_string(),
            "Name a file that isn't there yet".to_string(),
        )));
    }
    let local = |error: std::io::Error| {
        Reply::failed(problem(
            format!("`{shown}` couldn't be written"),
            format!("Writing it failed: {error}."),
            "Name a file in a folder you can write to".to_string(),
        ))
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(local)?;
    let read = store.read(key, |bytes| {
        use std::io::Write;
        file.write_all(bytes).map_err(|error| {
            Reply::failed(problem(
                format!("`{shown}` couldn't be written"),
                format!("Writing it failed: {error}."),
                "Free some space, then run it again".to_string(),
            ))
        })
    });
    let size = match read {
        Ok(Some(size)) => size,
        Ok(None) => {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(no_file(store, key));
        }
        Err(reply) => {
            drop(file);
            let _ = std::fs::remove_file(path);
            return Err(reply);
        }
    };
    file.sync_all().map_err(local)?;
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Wrote {shown}"),
        rows: vec![
            ("from".to_string(), format!("{} {key}", store.name)),
            ("size".to_string(), readable(size)),
        ],
    });
    Ok(Reply::screen(
        screen,
        Json::object([
            ("store", store.json()),
            ("key", Json::from(key)),
            ("path", Json::from(shown)),
            ("size", int(size)),
        ]),
    ))
}

// --- put --------------------------------------------------------------------------

fn put(store: &Store, key: &str, path: &Path, shown: &str) -> Result<Reply, Reply> {
    if !is_key(key) {
        return Err(Reply::failed(problem(
            format!("`{key}` isn't a file's key"),
            "`put` puts a file back under the key the runtime made for it, `3f/a2/3fa2…`, as the record holding it names it.".to_string(),
            "Copy the key from the record, or from `jadpo files ls` of a store that holds it".to_string(),
        )));
    }
    let unread = |error: std::io::Error| {
        Reply::failed(problem(
            format!("`{shown}` couldn't be read"),
            format!("Reading it failed: {error}."),
            "Name a file you can read".to_string(),
        ))
    };
    let meta = std::fs::metadata(path).map_err(unread)?;
    if !meta.is_file() {
        return Err(Reply::failed(problem(
            format!("`{shown}` isn't a file"),
            "`put` sends one file's bytes.".to_string(),
            "Name a file".to_string(),
        )));
    }
    if store.holds(key)? {
        return Err(Reply::failed(problem(
            format!("`{}` already holds `{key}`", store.name),
            "A file never changes once it is kept, so a key that holds one takes no other bytes."
                .to_string(),
            "Put it under a key that holds nothing, such as one whose bytes were lost".to_string(),
        )));
    }
    let mut file = std::fs::File::open(path).map_err(unread)?;
    let size = meta.len();
    let mut head = [0u8; jadpo_std::files::HEAD_BYTES];
    let headed = {
        use std::io::{Read, Seek};
        let read = file.read(&mut head).map_err(unread)?;
        file.seek(std::io::SeekFrom::Start(0)).map_err(unread)?;
        read
    };
    let media = media_of(&head[..headed]);
    store.send(key, size, media, |most| {
        use std::io::Read;
        let mut bytes = Vec::new();
        (&mut file)
            .take(most)
            .read_to_end(&mut bytes)
            .map_err(unread)?;
        Ok(bytes)
    })?;
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Put {shown}"),
        rows: vec![
            ("into".to_string(), format!("{} {key}", store.name)),
            ("size".to_string(), readable(size)),
        ],
    });
    Ok(Reply::screen(
        screen,
        Json::object([
            ("store", store.json()),
            ("key", Json::from(key)),
            ("path", Json::from(shown)),
            ("size", int(size)),
        ]),
    ))
}

// --- copy -------------------------------------------------------------------------

fn copy(from: &Store, to: &Store) -> Result<Reply, Reply> {
    if from.driver_word == to.driver_word && from.place == to.place {
        return Err(Reply::failed(problem(
            format!("`{}` and `{}` are the same store", from.name, to.name),
            "`copy` copies one store's files into another.".to_string(),
            "Name the store to copy into".to_string(),
        )));
    }
    let wanted = from.keys()?;
    let there: std::collections::BTreeMap<String, u64> = to.keys()?.into_iter().collect();
    let mut copied = 0usize;
    let mut bytes = 0u64;
    let mut kept = 0usize;
    for (key, size) in &wanted {
        if there.get(key) == Some(size) {
            kept += 1;
            continue;
        }
        let sent = if *size <= PIECE {
            // A small file read whole and sent whole.
            let mut whole = Vec::new();
            let read = from.read(key, |window| {
                whole.extend_from_slice(window);
                Ok(())
            })?;
            if read.is_none() {
                // Deleted since it was listed.
                continue;
            }
            let media = media_of(&whole[..whole.len().min(jadpo_std::files::HEAD_BYTES)]);
            let sent = whole.len() as u64;
            let mut once = Some(whole);
            to.send(key, sent, media, |_| Ok(once.take().unwrap_or_default()))?;
            sent
        } else {
            copy_large(from, to, key, *size)?;
            *size
        };
        copied += 1;
        bytes += sent;
    }
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!(
            "Copied {}{}",
            count(copied, "file", "files"),
            if copied > 0 {
                format!(" · {}", readable(bytes))
            } else {
                String::new()
            }
        ),
        rows: {
            let mut rows = vec![
                ("from".to_string(), from.described()),
                ("to".to_string(), to.described()),
            ];
            if kept > 0 {
                rows.push((
                    "left".to_string(),
                    format!("{} already there", count(kept, "file", "files")),
                ));
            }
            rows
        },
    });
    Ok(Reply::screen(
        screen,
        Json::object([
            ("from", from.json()),
            ("to", to.json()),
            ("copied", int(copied as u64)),
            ("bytes", int(bytes)),
            ("alreadyThere", int(kept as u64)),
        ]),
    ))
}

/// Copies large file `key`, `size` bytes, from `from` into `to` a piece
/// at a time, so no more than a piece is held in memory.
fn copy_large(from: &Store, to: &Store, key: &str, size: u64) -> Result<(), Reply> {
    let read = |at: u64, length: u64| -> Result<Vec<u8>, Reply> {
        let answer = from.ask(
            READ,
            vec![
                ("key", Value::text(key)),
                ("from", Value::Int(i64::try_from(at).unwrap_or(i64::MAX))),
                ("length", Value::Int(i64::try_from(length).unwrap_or(0))),
            ],
        );
        match answer {
            Answer::Value(Value::Bytes(bytes)) => Ok(bytes.to_vec()),
            other => Err(from.refused("read its files", &other)),
        }
    };
    let head = read(0, jadpo_std::files::HEAD_BYTES as u64)?;
    let media = media_of(&head);
    let mut at = 0u64;
    to.send(key, size, media, |most| {
        let mut piece = Vec::with_capacity(usize::try_from(most).unwrap_or(0));
        while (piece.len() as u64) < most {
            let bytes = read(at, (most - piece.len() as u64).min(WINDOW))?;
            if bytes.is_empty() {
                break;
            }
            at += bytes.len() as u64;
            piece.extend_from_slice(&bytes);
        }
        Ok(piece)
    })
}

#[cfg(test)]
mod tests {
    use super::readable;

    #[test]
    fn a_size_reads_in_si_units() {
        assert_eq!(readable(1), "1 byte");
        assert_eq!(readable(31), "31 bytes");
        assert_eq!(readable(1_000), "1 kB");
        assert_eq!(readable(1_250_000), "1.3 MB");
        assert_eq!(readable(2_300_000_000), "2.3 GB");
    }
}
