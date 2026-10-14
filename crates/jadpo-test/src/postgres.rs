//! A PostgreSQL server for tests (docs/compiler-runtime.md §2.7): the one
//! `JADPO_TEST_DATABASE_URL` names, or a disposable one started from the
//! machine's own PostgreSQL tools on a Unix socket in a new temporary
//! folder, with durability off. Nothing else on the machine is touched.
//!
//! [`Server::start`] finds or starts it; [`Server::url`] names a database
//! on it, for `jadpo_host_native::postgres`; [`Server::admin`] runs what
//! must run outside a transaction, such as `CREATE DATABASE`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// The variable naming a server to use instead of a disposable one: an
/// admin URL, on which test databases are created.
pub const DATABASE_URL_VARIABLE: &str = "JADPO_TEST_DATABASE_URL";

/// The database every server has, to connect to while creating others.
pub const ADMIN_DATABASE: &str = "postgres";

/// The superuser a disposable server is made with.
const USER: &str = "jadpo";

/// The disposable server's port: only the name of its socket file, since
/// it listens on no TCP port.
const PORT: u16 = 5432;

/// Unix socket paths are limited to about 103 bytes on macOS.
const MAX_SOCKET_PATH: usize = 100;

/// Where PostgreSQL's tools are installed when they aren't on `PATH`; a
/// `*` is a version, and the newest is used.
const INSTALLS: [&str; 5] = [
    "/Applications/Postgres.app/Contents/Versions/*/bin",
    "/usr/lib/postgresql/*/bin",
    "/opt/homebrew/opt/postgresql@*/bin",
    "/usr/local/opt/postgresql@*/bin",
    "/usr/local/pgsql/bin",
];

/// The tools a disposable server needs.
const TOOLS: [&str; 3] = ["initdb", "pg_ctl", "postgres"];

/// A server tests can create databases on. A disposable one stops, and its
/// folder is removed, when the `Server` is dropped or, failing that, when
/// the process that started it ends.
pub struct Server {
    kind: Kind,
}

enum Kind {
    /// The server `JADPO_TEST_DATABASE_URL` names.
    External { url: String },
    /// A server this process started.
    Disposable {
        root: PathBuf,
        bin: PathBuf,
        watchdog: Mutex<Option<Child>>,
    },
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            // Never the URL: it may hold a password.
            Kind::External { .. } => f.write_str("Server(JADPO_TEST_DATABASE_URL)"),
            Kind::Disposable { root, .. } => write!(f, "Server({})", root.display()),
        }
    }
}

/// Why no server could be had.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerError {
    /// No `JADPO_TEST_DATABASE_URL`, and no PostgreSQL tools anywhere this
    /// looks.
    NotFound,
    /// A step of starting or using the server failed.
    Failed { step: String, detail: String },
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::NotFound => write!(
                f,
                "no PostgreSQL server for the tests: install PostgreSQL, put initdb and \
                 pg_ctl on PATH, or set {DATABASE_URL_VARIABLE} to a server's admin URL"
            ),
            ServerError::Failed { step, detail } => {
                write!(f, "PostgreSQL for the tests: {step} failed: {detail}")
            }
        }
    }
}

impl std::error::Error for ServerError {}

fn failed(step: &str, detail: impl fmt::Display) -> ServerError {
    ServerError::Failed {
        step: step.to_string(),
        detail: detail.to_string(),
    }
}

impl Server {
    /// The server `JADPO_TEST_DATABASE_URL` names, or else a new disposable
    /// one from the PostgreSQL tools found on `PATH`, through `pg_config`,
    /// or where PostgreSQL is usually installed.
    pub fn start() -> Result<Server, ServerError> {
        if let Ok(url) = std::env::var(DATABASE_URL_VARIABLE)
            && !url.trim().is_empty()
        {
            return Ok(Server {
                kind: Kind::External { url },
            });
        }
        let bin = find_tools().ok_or(ServerError::NotFound)?;
        start_disposable(bin)
    }

    /// A URL for `database` on this server, as `jadpo_host_native::postgres`
    /// takes it. [`ADMIN_DATABASE`] always exists.
    pub fn url(&self, database: &str) -> String {
        match &self.kind {
            Kind::External { url } => with_database(url, database),
            Kind::Disposable { root, .. } => format!(
                "host={} port={PORT} user={USER} dbname={}",
                literal(&root.to_string_lossy()),
                literal(database)
            ),
        }
    }

    /// Runs `sql` (statements without parameters) on the admin database,
    /// outside any transaction. It runs on a thread of its own, so it may
    /// be called from anywhere, inside a runtime or not.
    pub fn admin(&self, sql: &str) -> Result<(), ServerError> {
        let url = self.url(ADMIN_DATABASE);
        let sql = sql.to_string();
        let ran = std::thread::spawn(move || -> Result<(), String> {
            let runtime = jadpo_host_native::Runtime::new().map_err(|error| error.to_string())?;
            let store = jadpo_host_native::postgres(&url).map_err(|error| format!("{error:?}"))?;
            runtime
                .block_on(store.batch_outside_transaction(&sql))
                .map_err(|error| format!("{error:?}"))
        })
        .join()
        .map_err(|_| failed("an admin statement", "its thread panicked"))?;
        ran.map_err(|detail| failed("an admin statement", detail))
    }

    /// Creates a new, empty database named after `stem` and gives its URL.
    /// The name is made unique to this process and call.
    pub fn create_database(&self, stem: &str) -> Result<String, ServerError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .take(30)
            .collect();
        let name = format!(
            "jadpo_{}_{}_{stem}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        self.admin(&format!("CREATE DATABASE \"{name}\""))?;
        Ok(self.url(&name))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Kind::Disposable {
            root,
            bin,
            watchdog,
        } = &self.kind
        {
            stop(bin, root);
            if let Ok(mut watchdog) = watchdog.lock()
                && let Some(mut child) = watchdog.take()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// `url` with its database replaced by `database`, in either of libpq's
/// forms: a later `dbname` wins in both.
fn with_database(url: &str, database: &str) -> String {
    let url = url.trim();
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        let encoded: String = database
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                    char::from(b).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        let joiner = if url.contains('?') { '&' } else { '?' };
        format!("{url}{joiner}dbname={encoded}")
    } else {
        format!("{url} dbname={}", literal(database))
    }
}

/// A value in libpq's key=value form.
fn literal(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

/// The folder holding PostgreSQL's tools: on `PATH`, then where
/// `pg_config` says, then the newest of the usual installs.
fn find_tools() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH")
        && let Some(dir) = std::env::split_paths(&path).find(|dir| has_tools(dir))
    {
        return Some(dir);
    }
    if let Ok(output) = Command::new("pg_config").arg("--bindir").output()
        && output.status.success()
    {
        let dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        if has_tools(&dir) {
            return Some(dir);
        }
    }
    INSTALLS.iter().find_map(|pattern| newest(pattern))
}

fn has_tools(dir: &Path) -> bool {
    TOOLS.iter().all(|tool| dir.join(tool).is_file())
}

/// The newest folder matching `pattern` (one `*`, a version) that has the
/// tools; a version that isn't a number, such as Postgres.app's `latest`
/// link, comes last.
fn newest(pattern: &str) -> Option<PathBuf> {
    let Some((before, after)) = pattern.split_once('*') else {
        let dir = PathBuf::from(pattern);
        return has_tools(&dir).then_some(dir);
    };
    let (parent, prefix) = before.split_at(before.rfind('/')? + 1);
    let mut found: Vec<(Vec<u64>, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(version) = name.strip_prefix(prefix) else {
            continue;
        };
        let dir = PathBuf::from(format!("{before}{version}{after}"));
        if has_tools(&dir) {
            let number = version
                .split('.')
                .map(|part| part.parse::<u64>().ok())
                .collect::<Option<Vec<u64>>>()
                .unwrap_or_default();
            found.push((number, dir));
        }
    }
    found.sort();
    found.pop().map(|(_, dir)| dir)
}

/// A new folder for a cluster, short enough that its socket's path fits.
fn new_root() -> Result<PathBuf, ServerError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        "jadpo-pg-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let socket = format!("/.s.PGSQL.{PORT}");
    let temp = std::env::temp_dir().join(&name);
    let root = if temp.as_os_str().len() + socket.len() <= MAX_SOCKET_PATH {
        temp
    } else {
        Path::new("/tmp").join(&name)
    };
    std::fs::create_dir_all(&root).map_err(|error| failed("making the server's folder", error))?;
    // Resolve links (/tmp is /private/tmp on macOS), so the path the server
    // writes is the path clients look for.
    root.canonicalize()
        .map_err(|error| failed("making the server's folder", error))
}

fn start_disposable(bin: PathBuf) -> Result<Server, ServerError> {
    let root = new_root()?;
    let data = root.join("data");
    let run = |step: &str, command: &mut Command| -> Result<(), ServerError> {
        let output = command
            .stdin(Stdio::null())
            .output()
            .map_err(|error| failed(step, error))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failed(
                step,
                format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ),
            ))
        }
    };
    let started = (|| {
        run(
            "initdb",
            Command::new(bin.join("initdb")).arg("-D").arg(&data).args([
                "-U",
                USER,
                "-A",
                "trust",
                "-E",
                "UTF8",
                "--locale=C",
                "--no-sync",
            ]),
        )?;
        let settings = format!(
            "\n# Written by jadpo-test: a disposable server for tests.\n\
             listen_addresses = ''\n\
             unix_socket_directories = '{}'\n\
             port = {PORT}\n\
             fsync = off\n\
             synchronous_commit = off\n\
             full_page_writes = off\n\
             max_connections = 300\n",
            root.to_string_lossy().replace('\'', "''")
        );
        append(&data.join("postgresql.conf"), &settings)
            .map_err(|error| failed("configuring the server", error))?;
        run(
            "pg_ctl start",
            Command::new(bin.join("pg_ctl"))
                .arg("-D")
                .arg(&data)
                .arg("-l")
                .arg(root.join("server.log"))
                .args(["-w", "-t", "60", "start"]),
        )
    })();
    if let Err(error) = started {
        let log = std::fs::read_to_string(root.join("server.log")).unwrap_or_default();
        stop(&bin, &root);
        return Err(match error {
            ServerError::Failed { step, detail } if !log.is_empty() => ServerError::Failed {
                step,
                detail: format!("{detail}\n{log}"),
            },
            other => other,
        });
    }
    let watchdog = watchdog(&bin, &root);
    Ok(Server {
        kind: Kind::Disposable {
            root,
            bin,
            watchdog: Mutex::new(watchdog),
        },
    })
}

fn append(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(text.as_bytes())
}

/// Stops the server in `root`, if it runs, and removes the folder.
fn stop(bin: &Path, root: &Path) {
    let data = root.join("data");
    if data.join("postmaster.pid").exists() {
        let _ = Command::new(bin.join("pg_ctl"))
            .arg("-D")
            .arg(&data)
            .args(["-m", "immediate", "-w", "stop"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = std::fs::remove_dir_all(root);
}

/// A process that stops the server and removes its folder once this
/// process has ended, however it ended: a server shared by a test binary
/// lives in a static, which is never dropped. It ignores the interrupt a
/// terminal sends the whole process group, so it still cleans up after
/// Ctrl-C.
fn watchdog(bin: &Path, root: &Path) -> Option<Child> {
    let script = r#"trap '' INT HUP
while kill -0 "$0" 2>/dev/null; do sleep 1; done
"$1" -D "$2" -m immediate -w stop >/dev/null 2>&1
rm -rf "$3""#;
    Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg(std::process::id().to_string())
        .arg(bin.join("pg_ctl"))
        .arg(root.join("data"))
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_database_replaces_the_one_an_admin_url_names() {
        assert_eq!(
            with_database("postgres://u:p@localhost:5432/postgres", "t 1"),
            "postgres://u:p@localhost:5432/postgres?dbname=t%201"
        );
        assert_eq!(
            with_database("postgresql://localhost/postgres?sslmode=disable", "t"),
            "postgresql://localhost/postgres?sslmode=disable&dbname=t"
        );
        assert_eq!(
            with_database("host=/tmp user=me", "it's"),
            "host=/tmp user=me dbname='it\\'s'"
        );
    }

    #[test]
    fn the_newest_install_with_every_tool_is_found() {
        let root = std::env::temp_dir().join(format!("jadpo-find-{}", std::process::id()));
        for version in ["9.6", "16", "15", "latest"] {
            let bin = root.join(format!("pg{version}")).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            for tool in TOOLS {
                std::fs::write(bin.join(tool), "").unwrap();
            }
        }
        std::fs::create_dir_all(root.join("pg17/bin")).unwrap();
        let pattern = format!("{}/pg*/bin", root.display());
        assert_eq!(newest(&pattern), Some(root.join("pg16/bin")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn nothing_found_says_how_to_get_a_server() {
        let message = ServerError::NotFound.to_string();
        assert!(message.contains("install PostgreSQL"));
        assert!(message.contains("initdb and pg_ctl on PATH"));
        assert!(message.contains(DATABASE_URL_VARIABLE));
    }
}
