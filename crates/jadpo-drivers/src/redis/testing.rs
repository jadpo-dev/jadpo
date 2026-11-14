//! What tests reach Redis through (chunk Q8), never built into a
//! deployable: the `redis-testing` feature, which only tests turn on.
//!
//! - [`Double`]: an in-process stand-in for a Redis server, answering the
//!   commands the driver sends (`AUTH`, `SELECT`, `PING`, `MULTI`, `EXEC`,
//!   `SET` with `PX` and `NX`, `INCR`, `PTTL`, `GET`, `DEL`, `FLUSHDB`) as
//!   Redis does, over RESP, with expiry on a clock the caller gives, and
//!   keeping what each connection sent.
//! - [`TcpDouble`]: the double on a port of this machine, each connection
//!   on a thread of its own, for a deployable or a Worker to reach.
//! - [`real`]: a real Redis server, when the run has one: the one
//!   `JADPO_TEST_REDIS_URL` names, as CI's does, or one started from
//!   `redis-server` or `valkey-server` on `PATH`. A run with neither says
//!   so on a `skipped:` line, and under CI fails instead.

use std::collections::BTreeMap;

use super::resp::{self, Reply};

/// The variable that names the run's Redis server, as CI's does.
pub const REDIS_URL_VARIABLE: &str = "JADPO_TEST_REDIS_URL";

/// One stored value and when it expires, in milliseconds.
#[derive(Clone, Debug)]
struct Entry {
    value: Vec<u8>,
    expires: Option<i64>,
}

/// One connection's state: whether it has signed in, its database and the
/// transaction it is queueing.
#[derive(Clone, Debug, Default)]
pub struct Session {
    signed_in: bool,
    database: u32,
    queued: Option<Vec<Vec<Vec<u8>>>>,
}

/// A stand-in Redis server's data and what it has been sent.
#[derive(Clone, Debug, Default)]
pub struct Double {
    /// The password `AUTH` must give, with the user when it names one;
    /// none signs in everyone.
    password: Option<(Option<String>, String)>,
    databases: BTreeMap<u32, BTreeMap<Vec<u8>, Entry>>,
    /// Each command's name, as sent, in order.
    pub sent: Vec<String>,
}

impl Double {
    /// A server everyone may use.
    pub fn new() -> Double {
        Double::default()
    }

    /// A server that takes only `password`, for `user` when it names one.
    pub fn signing_in(user: Option<&str>, password: &str) -> Double {
        Double {
            password: Some((user.map(str::to_string), password.to_string())),
            ..Double::default()
        }
    }

    /// The keys database `database` holds at `now`.
    pub fn keys(&self, database: u32, now: i64) -> Vec<String> {
        self.databases
            .get(&database)
            .into_iter()
            .flatten()
            .filter(|(_, entry)| entry.expires.is_none_or(|at| at > now))
            .map(|(key, _)| String::from_utf8_lossy(key).into_owned())
            .collect()
    }

    /// The text `key` holds in database `database` at `now`.
    pub fn get(&self, database: u32, key: &str, now: i64) -> Option<String> {
        let entry = self.databases.get(&database)?.get(key.as_bytes())?;
        entry
            .expires
            .is_none_or(|at| at > now)
            .then(|| String::from_utf8_lossy(&entry.value).into_owned())
    }

    /// Its answer to `words`, one command `session` sent, at `now`.
    pub fn answer(&mut self, session: &mut Session, words: Vec<Vec<u8>>, now: i64) -> Reply {
        let name = words
            .first()
            .map(|word| String::from_utf8_lossy(word).to_ascii_uppercase())
            .unwrap_or_default();
        self.sent.push(name.clone());
        if name == "AUTH" {
            return self.auth(session, &words);
        }
        if self.password.is_some() && !session.signed_in {
            return Reply::Error("NOAUTH Authentication required.".into());
        }
        if session.queued.is_some() {
            return match name.as_str() {
                "EXEC" => {
                    let queued = session.queued.take().unwrap_or_default();
                    let answers = queued
                        .into_iter()
                        .map(|words| self.run(session.database, &words, now))
                        .collect();
                    Reply::Array(Some(answers))
                }
                "DISCARD" => {
                    session.queued = None;
                    Reply::Status("OK".into())
                }
                "MULTI" => Reply::Error("ERR MULTI calls can not be nested".into()),
                _ => {
                    session.queued.get_or_insert_default().push(words);
                    Reply::Status("QUEUED".into())
                }
            };
        }
        match name.as_str() {
            "MULTI" => {
                session.queued = Some(Vec::new());
                Reply::Status("OK".into())
            }
            "EXEC" => Reply::Error("ERR EXEC without MULTI".into()),
            "SELECT" => match words.get(1).and_then(|word| text(word).parse::<u32>().ok()) {
                Some(database) if database < 16 => {
                    session.database = database;
                    Reply::Status("OK".into())
                }
                _ => Reply::Error("ERR DB index is out of range".into()),
            },
            _ => self.run(session.database, &words, now),
        }
    }

    fn auth(&self, session: &mut Session, words: &[Vec<u8>]) -> Reply {
        let (user, password) = match words {
            [_, password] => (None, text(password)),
            [_, user, password] => (Some(text(user)), text(password)),
            _ => return Reply::Error("ERR wrong number of arguments for 'auth' command".into()),
        };
        match &self.password {
            None => Reply::Error(
                "ERR AUTH <password> called without any password configured for the default user."
                    .into(),
            ),
            Some((wanted_user, wanted)) => {
                let user_matches = match (wanted_user.as_deref(), user.as_deref()) {
                    (Some(wanted), Some(user)) => wanted == user,
                    (None, None | Some("default")) => true,
                    _ => false,
                };
                if user_matches && *wanted == password {
                    session.signed_in = true;
                    Reply::Status("OK".into())
                } else {
                    Reply::Error(
                        "WRONGPASS invalid username-password pair or user is disabled.".into(),
                    )
                }
            }
        }
    }

    /// One command run against database `database`.
    fn run(&mut self, database: u32, words: &[Vec<u8>], now: i64) -> Reply {
        let name = words
            .first()
            .map(|word| String::from_utf8_lossy(word).to_ascii_uppercase())
            .unwrap_or_default();
        let keys = self.databases.entry(database).or_default();
        keys.retain(|_, entry| entry.expires.is_none_or(|at| at > now));
        let key = words.get(1).cloned().unwrap_or_default();
        match name.as_str() {
            "PING" => Reply::Status("PONG".into()),
            "FLUSHDB" => {
                keys.clear();
                Reply::Status("OK".into())
            }
            "GET" => Reply::Bulk(keys.get(&key).map(|entry| entry.value.clone())),
            "DEL" => Reply::Integer(i64::from(keys.remove(&key).is_some())),
            "SET" => {
                let Some(value) = words.get(2) else {
                    return Reply::Error("ERR wrong number of arguments for 'set' command".into());
                };
                let mut expires = None;
                let mut only_new = false;
                let mut options = words.iter().skip(3);
                while let Some(option) = options.next() {
                    match text(option).to_ascii_uppercase().as_str() {
                        "NX" => only_new = true,
                        "PX" => {
                            let Some(ms) = options.next().and_then(|ms| text(ms).parse::<i64>().ok())
                            else {
                                return Reply::Error("ERR syntax error".into());
                            };
                            expires = Some(now + ms);
                        }
                        _ => return Reply::Error("ERR syntax error".into()),
                    }
                }
                if only_new && keys.contains_key(&key) {
                    return Reply::Bulk(None);
                }
                keys.insert(
                    key,
                    Entry {
                        value: value.clone(),
                        expires,
                    },
                );
                Reply::Status("OK".into())
            }
            "INCR" => {
                let entry = keys.entry(key).or_insert(Entry {
                    value: b"0".to_vec(),
                    expires: None,
                });
                match text(&entry.value).parse::<i64>() {
                    Ok(count) => {
                        entry.value = (count + 1).to_string().into_bytes();
                        Reply::Integer(count + 1)
                    }
                    Err(_) => Reply::Error("ERR value is not an integer or out of range".into()),
                }
            }
            "PTTL" => match keys.get(&key) {
                None => Reply::Integer(-2),
                Some(Entry { expires: None, .. }) => Reply::Integer(-1),
                Some(Entry {
                    expires: Some(at), ..
                }) => Reply::Integer(at - now),
            },
            _ => Reply::Error(format!("ERR unknown command '{name}'")),
        }
    }
}

/// `reply` as RESP.
pub fn encode(reply: &Reply) -> Vec<u8> {
    match reply {
        Reply::Status(text) => format!("+{text}\r\n").into_bytes(),
        Reply::Error(text) => format!("-{text}\r\n").into_bytes(),
        Reply::Integer(number) => format!(":{number}\r\n").into_bytes(),
        Reply::Bulk(None) => b"$-1\r\n".to_vec(),
        Reply::Bulk(Some(bytes)) => {
            let mut out = format!("${}\r\n", bytes.len()).into_bytes();
            out.extend_from_slice(bytes);
            out.extend_from_slice(b"\r\n");
            out
        }
        Reply::Array(None) => b"*-1\r\n".to_vec(),
        Reply::Array(Some(items)) => {
            let mut out = format!("*{}\r\n", items.len()).into_bytes();
            for item in items {
                out.extend(encode(item));
            }
            out
        }
    }
}

/// The commands whole in `input`, each as its words, taken from its front:
/// `Err` for bytes that aren't commands.
pub fn commands(input: &mut Vec<u8>) -> Result<Vec<Vec<Vec<u8>>>, resp::Unreadable> {
    let mut out = Vec::new();
    while let Some((reply, used)) = resp::parse(input)? {
        input.drain(..used);
        let Reply::Array(Some(words)) = reply else {
            return Err(resp::Unreadable("a command that isn't an array"));
        };
        let words = words
            .into_iter()
            .map(|word| match word {
                Reply::Bulk(Some(bytes)) => Ok(bytes),
                _ => Err(resp::Unreadable("a command's word that isn't a bulk string")),
            })
            .collect::<Result<_, _>>()?;
        out.push(words);
    }
    Ok(out)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(not(target_arch = "wasm32"))]
pub use tcp::{RealRedis, TcpDouble, real};

#[cfg(not(target_arch = "wasm32"))]
mod tcp {
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::{Duration, Instant};

    use super::{Double, REDIS_URL_VARIABLE, Session, commands, encode};
    use crate::redis::resp::{self, Reply};

    /// The time now, in milliseconds since 1970.
    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| i64::try_from(since.as_millis()).unwrap_or(0))
    }

    /// The double on a port of this machine, stopped when dropped.
    #[derive(Debug)]
    pub struct TcpDouble {
        port: u16,
        state: Arc<Mutex<Double>>,
        /// While set, it reads what comes and answers nothing.
        silent: Arc<AtomicBool>,
        stopped: Arc<AtomicBool>,
        /// How many connections it has taken.
        connections: Arc<Mutex<usize>>,
    }

    impl TcpDouble {
        /// `double` listening on `127.0.0.1`, on a port the system picks.
        pub fn start(double: Double) -> TcpDouble {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a port for the Redis double");
            let port = listener.local_addr().expect("its port").port();
            let state = Arc::new(Mutex::new(double));
            let silent = Arc::new(AtomicBool::new(false));
            let stopped = Arc::new(AtomicBool::new(false));
            let connections = Arc::new(Mutex::new(0));
            let (served, quiet, stop, counted) = (
                Arc::clone(&state),
                Arc::clone(&silent),
                Arc::clone(&stopped),
                Arc::clone(&connections),
            );
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(stream) = stream else {
                        continue;
                    };
                    *counted.lock().unwrap_or_else(PoisonError::into_inner) += 1;
                    let (served, quiet, stop) =
                        (Arc::clone(&served), Arc::clone(&quiet), Arc::clone(&stop));
                    std::thread::spawn(move || serve(stream, &served, &quiet, &stop));
                }
            });
            TcpDouble {
                port,
                state,
                silent,
                stopped,
                connections,
            }
        }

        pub fn port(&self) -> u16 {
            self.port
        }

        /// Its address, signing in as `password` when it is given one.
        pub fn url(&self, password: Option<&str>) -> String {
            match password {
                Some(password) => format!("redis://:{password}@127.0.0.1:{}", self.port),
                None => format!("redis://127.0.0.1:{}", self.port),
            }
        }

        /// What it holds and has been sent.
        pub fn state(&self) -> Double {
            self.state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }

        /// The keys database 0 holds now.
        pub fn keys(&self) -> Vec<String> {
            self.state().keys(0, now_ms())
        }

        /// From now on reads what comes and answers nothing, as a server
        /// behind a network that drops its replies, until told otherwise.
        pub fn go_silent(&self, silent: bool) {
            self.silent.store(silent, Ordering::SeqCst);
        }

        /// How many connections it has taken.
        pub fn connections(&self) -> usize {
            *self
                .connections
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
        }
    }

    impl Drop for TcpDouble {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::SeqCst);
            // Wakes the acceptor, which then sees it has stopped.
            let _ = TcpStream::connect(("127.0.0.1", self.port));
        }
    }

    /// Answers each command `stream` sends, until it closes or the double
    /// stops.
    fn serve(
        mut stream: TcpStream,
        state: &Mutex<Double>,
        silent: &AtomicBool,
        stopped: &AtomicBool,
    ) {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
        let mut session = Session::default();
        let mut input = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            if stopped.load(Ordering::SeqCst) {
                let _ = stream.shutdown(Shutdown::Both);
                return;
            }
            match stream.read(&mut buffer) {
                Ok(0) => return,
                Ok(read) => input.extend_from_slice(&buffer[..read]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(_) => return,
            }
            let Ok(sent) = commands(&mut input) else {
                let _ = stream.write_all(&encode(&Reply::Error("ERR Protocol error".into())));
                return;
            };
            let mut out = Vec::new();
            {
                let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
                for words in sent {
                    out.extend(encode(&state.answer(&mut session, words, now_ms())));
                }
            }
            if !silent.load(Ordering::SeqCst) && stream.write_all(&out).is_err() {
                return;
            }
        }
    }

    /// A real Redis server for a test, its database emptied first.
    #[derive(Debug)]
    pub struct RealRedis {
        /// Its address, naming the test's database.
        pub url: String,
        /// The server this test started, stopped when it ends.
        _started: Option<Started>,
    }

    #[derive(Debug)]
    struct Started(Child);

    impl Drop for Started {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// A real Redis server's database `database`, emptied, when the run
    /// has a server: the one [`REDIS_URL_VARIABLE`] names, or one started
    /// from `redis-server` or `valkey-server` on `PATH`. `None`, after a
    /// `skipped:` line, when it has neither; under CI, whose jobs name
    /// one, a panic instead. Each test that reaches a real server takes a
    /// database of its own, from 1 to 15, so tests side by side never
    /// count in each other's keys.
    pub fn real(database: u32) -> Option<RealRedis> {
        assert!(
            (1..16).contains(&database),
            "a test's database is one from 1 to 15"
        );
        let (base, started) = match std::env::var(REDIS_URL_VARIABLE) {
            Ok(url) if !url.is_empty() => (url, None),
            _ => match start() {
                Some((url, child)) => (url, Some(Started(child))),
                None => {
                    let ci = std::env::var_os("CI").is_some_and(|value| !value.is_empty());
                    assert!(
                        !ci,
                        "a real Redis test under CI, where {REDIS_URL_VARIABLE} names the job's server, found none"
                    );
                    println!(
                        "skipped: no real Redis server: {REDIS_URL_VARIABLE} is unset and neither `redis-server` nor `valkey-server` is on PATH; the in-process double ran instead"
                    );
                    return None;
                }
            },
        };
        let url = with_database(&base, database);
        flush(&url).unwrap_or_else(|why| panic!("the run's Redis server: {why}"));
        Some(RealRedis {
            url,
            _started: started,
        })
    }

    /// `url` naming database `database` in place of its own.
    fn with_database(url: &str, database: u32) -> String {
        let (scheme, rest) = url.split_once("://").unwrap_or(("redis", url));
        let authority = rest.split(['/', '?']).next().unwrap_or(rest);
        format!("{scheme}://{authority}/{database}")
    }

    /// A server started from the machine's tools on a free port, once it
    /// answers.
    fn start() -> Option<(String, Child)> {
        let port = TcpListener::bind("127.0.0.1:0")
            .ok()?
            .local_addr()
            .ok()?
            .port();
        for program in ["redis-server", "valkey-server"] {
            let Ok(child) = Command::new(program)
                .args([
                    "--port",
                    &port.to_string(),
                    "--bind",
                    "127.0.0.1",
                    "--save",
                    "",
                    "--appendonly",
                    "no",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue;
            };
            let url = format!("redis://127.0.0.1:{port}");
            let until = Instant::now() + Duration::from_secs(10);
            while Instant::now() < until {
                if flush(&url).is_ok() {
                    return Some((url, child));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
        }
        None
    }

    /// Empties the database `url` names, signing in as it says: plain TCP
    /// only, which a test's server is.
    fn flush(url: &str) -> Result<(), String> {
        let address = crate::redis::address::Address::parse(url)?;
        if address.tls {
            return Err("a test's server is reached over plain TCP".into());
        }
        let mut stream = TcpStream::connect((address.host.as_str(), address.port))
            .map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| error.to_string())?;
        let mut sent = Vec::new();
        let mut words: Vec<Vec<Vec<u8>>> = address.hello();
        words.push(vec![b"FLUSHDB".to_vec()]);
        for command in &words {
            sent.extend(resp::command(command));
        }
        stream.write_all(&sent).map_err(|error| error.to_string())?;
        let mut input = Vec::new();
        let mut buffer = [0u8; 1024];
        let mut answered = 0;
        while answered < words.len() {
            match resp::parse(&input).map_err(|why| why.0.to_string())? {
                Some((reply, used)) => {
                    input.drain(..used);
                    if let Reply::Error(message) = reply {
                        return Err(message);
                    }
                    answered += 1;
                }
                None => {
                    let read = stream
                        .read(&mut buffer)
                        .map_err(|error| error.to_string())?;
                    if read == 0 {
                        return Err("the server closed the connection".into());
                    }
                    input.extend_from_slice(&buffer[..read]);
                }
            }
        }
        Ok(())
    }
}
