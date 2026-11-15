//! What tests reach Redis through (chunk Q8), never built into a
//! deployable: the `redis-testing` feature, which only tests turn on.
//!
//! - [`Double`]: an in-process stand-in for a Redis server, answering the
//!   commands the driver sends (`AUTH`, `SELECT`, `PING`, `MULTI`, `EXEC`,
//!   `SET` with `PX` and `NX`, `INCR`, `PTTL`, `GET`, `MGET`, `DEL`,
//!   `SADD`, `SMEMBERS`, `PEXPIRE`, `FLUSHDB`) as
//!   Redis does, over RESP, with expiry on a clock the caller gives, and
//!   keeping what each connection sent.
//! - [`TcpDouble`]: the double on a port of this machine, each connection
//!   on a thread of its own, for a deployable or a Worker to reach.
//! - [`real`]: a real Redis server, when the run has one: the one
//!   `JADPO_TEST_REDIS_URL` names, as CI's does, or one started from
//!   `redis-server` or `valkey-server` on `PATH`. A run with neither says
//!   so on a `skipped:` line, and under CI fails instead.

// Test support, which only tests build: a broken double or a missing
// server should stop the test that reached it.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;

use super::resp::{self, Reply};

/// The variable that names the run's Redis server, as CI's does.
pub const REDIS_URL_VARIABLE: &str = "JADPO_TEST_REDIS_URL";

/// One stored value and when it expires, in milliseconds: text, or a set
/// of members.
#[derive(Clone, Debug)]
struct Entry {
    value: Vec<u8>,
    members: Option<std::collections::BTreeSet<Vec<u8>>>,
    /// A sorted set's members, each with its score (chunk Q10).
    scores: Option<BTreeMap<Vec<u8>, i64>>,
    expires: Option<i64>,
}

/// Redis's answer to a command on a key holding the other kind of value.
const WRONG_TYPE: &str = "WRONGTYPE Operation against a key holding the wrong kind of value";

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
        if name == "EVAL" {
            return self.eval(database, words, now);
        }
        let keys = self.databases.entry(database).or_default();
        keys.retain(|_, entry| entry.expires.is_none_or(|at| at > now));
        let key = words.get(1).cloned().unwrap_or_default();
        match name.as_str() {
            "PING" => Reply::Status("PONG".into()),
            "FLUSHDB" => {
                keys.clear();
                Reply::Status("OK".into())
            }
            "GET" => match keys.get(&key) {
                Some(Entry {
                    members: Some(_), ..
                })
                | Some(Entry {
                    scores: Some(_), ..
                }) => Reply::Error(WRONG_TYPE.into()),
                found => Reply::Bulk(found.map(|entry| entry.value.clone())),
            },
            "MGET" => Reply::Array(Some(
                words
                    .iter()
                    .skip(1)
                    .map(|key| {
                        Reply::Bulk(
                            keys.get(key)
                                .filter(|entry| entry.members.is_none() && entry.scores.is_none())
                                .map(|entry| entry.value.clone()),
                        )
                    })
                    .collect(),
            )),
            "DEL" => Reply::Integer(
                words
                    .iter()
                    .skip(1)
                    .filter(|key| keys.remove(*key).is_some())
                    .count() as i64,
            ),
            "SADD" => {
                let entry = keys.entry(key).or_insert(Entry {
                    value: Vec::new(),
                    members: Some(std::collections::BTreeSet::new()),
                    scores: None,
                    expires: None,
                });
                let Some(members) = entry.members.as_mut() else {
                    return Reply::Error(WRONG_TYPE.into());
                };
                let added = words
                    .iter()
                    .skip(2)
                    .filter(|member| members.insert((*member).clone()))
                    .count();
                Reply::Integer(added as i64)
            }
            "SMEMBERS" => match keys.get(&key) {
                None => Reply::Array(Some(Vec::new())),
                Some(Entry {
                    members: Some(members),
                    ..
                }) => Reply::Array(Some(
                    members
                        .iter()
                        .map(|member| Reply::Bulk(Some(member.clone())))
                        .collect(),
                )),
                Some(_) => Reply::Error(WRONG_TYPE.into()),
            },
            "PEXPIRE" => {
                let Some(ms) = words.get(2).and_then(|ms| text(ms).parse::<i64>().ok()) else {
                    return Reply::Error("ERR value is not an integer or out of range".into());
                };
                match keys.get_mut(&key) {
                    Some(entry) => {
                        entry.expires = Some(now + ms);
                        Reply::Integer(1)
                    }
                    None => Reply::Integer(0),
                }
            }
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
                            let Some(ms) =
                                options.next().and_then(|ms| text(ms).parse::<i64>().ok())
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
                        members: None,
                        scores: None,
                        expires,
                    },
                );
                Reply::Status("OK".into())
            }
            "INCR" => {
                let entry = keys.entry(key).or_insert(Entry {
                    value: b"0".to_vec(),
                    members: None,
                    scores: None,
                    expires: None,
                });
                if entry.members.is_some() || entry.scores.is_some() {
                    return Reply::Error(WRONG_TYPE.into());
                }
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
            "ZADD" => {
                let entry = keys.entry(key).or_insert(Entry {
                    value: Vec::new(),
                    members: None,
                    scores: Some(BTreeMap::new()),
                    expires: None,
                });
                let Some(scores) = entry.scores.as_mut() else {
                    return Reply::Error(WRONG_TYPE.into());
                };
                let mut added = 0;
                for pair in words.get(2..).unwrap_or_default().chunks(2) {
                    let [score, member] = pair else {
                        return Reply::Error("ERR syntax error".into());
                    };
                    let Ok(score) = text(score).parse::<i64>() else {
                        return Reply::Error("ERR value is not a valid float".into());
                    };
                    if scores.insert(member.clone(), score).is_none() {
                        added += 1;
                    }
                }
                Reply::Integer(added)
            }
            "ZREM" => {
                let Some(entry) = keys.get_mut(&key) else {
                    return Reply::Integer(0);
                };
                let Some(scores) = entry.scores.as_mut() else {
                    return Reply::Error(WRONG_TYPE.into());
                };
                let removed = words
                    .iter()
                    .skip(2)
                    .filter(|member| scores.remove(*member).is_some())
                    .count();
                if scores.is_empty() {
                    keys.remove(&key);
                }
                Reply::Integer(removed as i64)
            }
            "ZRANGEBYLEX" => {
                let scores = match keys.get(&key) {
                    None => return Reply::Array(Some(Vec::new())),
                    Some(Entry {
                        scores: Some(scores),
                        ..
                    }) => scores,
                    Some(_) => return Reply::Error(WRONG_TYPE.into()),
                };
                let (Some(min), Some(max)) = (words.get(2), words.get(3)) else {
                    return Reply::Error("ERR wrong number of arguments".into());
                };
                let above = |member: &[u8]| match min.split_first() {
                    Some((b'-', [])) => true,
                    Some((b'(', from)) => member > from,
                    Some((b'[', from)) => member >= from,
                    _ => false,
                };
                let below = |member: &[u8]| match max.split_first() {
                    Some((b'+', [])) => true,
                    Some((b'(', to)) => member < to,
                    Some((b'[', to)) => member <= to,
                    _ => false,
                };
                let (offset, count) = limit(words.get(4..).unwrap_or_default());
                Reply::Array(Some(
                    scores
                        .keys()
                        .filter(|member| above(member) && below(member))
                        .skip(offset)
                        .take(count)
                        .map(|member| Reply::Bulk(Some(member.clone())))
                        .collect(),
                ))
            }
            "ZRANGEBYSCORE" | "ZCOUNT" => {
                let scores = match keys.get(&key) {
                    None if name == "ZCOUNT" => return Reply::Integer(0),
                    None => return Reply::Array(Some(Vec::new())),
                    Some(Entry {
                        scores: Some(scores),
                        ..
                    }) => scores,
                    Some(_) => return Reply::Error(WRONG_TYPE.into()),
                };
                let (Some(min), Some(max)) = (
                    words.get(2).and_then(|word| bound(word)),
                    words.get(3).and_then(|word| bound(word)),
                ) else {
                    return Reply::Error("ERR min or max is not a float".into());
                };
                let within = |score: i64| min.starts_before(score) && max.ends_after(score);
                let mut found: Vec<(&i64, &Vec<u8>)> = scores
                    .iter()
                    .filter(|(_, score)| within(**score))
                    .map(|(member, score)| (score, member))
                    .collect();
                if name == "ZCOUNT" {
                    return Reply::Integer(found.len() as i64);
                }
                found.sort();
                Reply::Array(Some(
                    found
                        .into_iter()
                        .map(|(_, member)| Reply::Bulk(Some(member.clone())))
                        .collect(),
                ))
            }
            _ => Reply::Error(format!("ERR unknown command '{name}'")),
        }
    }

    /// `EVAL`: the two scripts the driver keeps entities in Redis with
    /// (chunk Q10), each run as [`super::entities`] says Redis runs it, its
    /// checks asked as the runtime asks them, and nothing else.
    fn eval(&mut self, database: u32, words: &[Vec<u8>], now: i64) -> Reply {
        let script = words.get(1).map(|word| text(word)).unwrap_or_default();
        let args: Vec<Vec<u8>> = words.iter().skip(3).cloned().collect();
        if script == super::entities::COMMIT {
            let Some((asked_at, commit)) = super::entities::read_commit(&args) else {
                return Reply::Error("ERR Error running script: bad arguments".into());
            };
            let failed = {
                let keys = self.databases.entry(database).or_default();
                keys.retain(|_, entry| entry.expires.is_none_or(|at| at > now));
                let get = |key: &str| {
                    keys.get(key.as_bytes())
                        .filter(|entry| entry.members.is_none() && entry.scores.is_none())
                        .map(|entry| entry.value.clone())
                };
                commit
                    .checks
                    .iter()
                    .position(|check| !jadpo_runtime::redis::holds(check, &get, asked_at))
            };
            if let Some(at) = failed {
                return Reply::Integer(at as i64 + 1);
            }
            use jadpo_runtime::redis::Change;
            for change in &commit.changes {
                let command: Vec<Vec<u8>> = match change {
                    Change::Set { key, value, ttl_ms } => vec![
                        b"SET".to_vec(),
                        key.clone().into_bytes(),
                        value.clone(),
                        b"PX".to_vec(),
                        ttl_ms.to_string().into_bytes(),
                    ],
                    Change::Delete { key } => vec![b"DEL".to_vec(), key.clone().into_bytes()],
                    Change::Add { set, score, member } => vec![
                        b"ZADD".to_vec(),
                        set.clone().into_bytes(),
                        score.to_string().into_bytes(),
                        member.clone().into_bytes(),
                    ],
                    Change::Remove { set, member } => vec![
                        b"ZREM".to_vec(),
                        set.clone().into_bytes(),
                        member.clone().into_bytes(),
                    ],
                    Change::Expire { key, ttl_ms } => vec![
                        b"PEXPIRE".to_vec(),
                        key.clone().into_bytes(),
                        ttl_ms.to_string().into_bytes(),
                    ],
                };
                let reply = self.run(database, &command, now);
                if reply.refused().is_some() {
                    return reply;
                }
            }
            return Reply::Integer(0);
        }
        if script == super::entities::PAGE {
            let [index, expiry, records, asked_at, after, size] = args.as_slice() else {
                return Reply::Error("ERR Error running script: bad arguments".into());
            };
            let gone = self.run(
                database,
                &[
                    b"ZRANGEBYSCORE".to_vec(),
                    expiry.clone(),
                    b"-inf".to_vec(),
                    asked_at.clone(),
                ],
                now,
            );
            let Reply::Array(Some(gone)) = gone else {
                return gone;
            };
            for member in gone {
                let Reply::Bulk(Some(member)) = member else {
                    continue;
                };
                for set in [index, expiry] {
                    self.run(
                        database,
                        &[b"ZREM".to_vec(), set.clone(), member.clone()],
                        now,
                    );
                }
            }
            let from = if after.is_empty() {
                b"-".to_vec()
            } else {
                [b"(".as_slice(), after].concat()
            };
            let ids = self.run(
                database,
                &[
                    b"ZRANGEBYLEX".to_vec(),
                    index.clone(),
                    from,
                    b"+".to_vec(),
                    b"LIMIT".to_vec(),
                    b"0".to_vec(),
                    size.clone(),
                ],
                now,
            );
            let Reply::Array(Some(ids)) = ids else {
                return ids;
            };
            let mut out = Vec::new();
            for id in ids {
                let Reply::Bulk(Some(id)) = id else {
                    continue;
                };
                let record = self.run(
                    database,
                    &[
                        b"GET".to_vec(),
                        [records.as_slice(), id.as_slice()].concat(),
                    ],
                    now,
                );
                out.push(Reply::Bulk(Some(id)));
                out.push(record);
            }
            return Reply::Array(Some(out));
        }
        Reply::Error("NOSCRIPT No matching script. Please use EVAL.".into())
    }
}

/// `LIMIT offset count` after a range's bounds: everything without one.
fn limit(words: &[Vec<u8>]) -> (usize, usize) {
    match words {
        [word, offset, count, ..] if text(word).eq_ignore_ascii_case("LIMIT") => (
            text(offset).parse().unwrap_or(0),
            text(count).parse().unwrap_or(usize::MAX),
        ),
        _ => (0, usize::MAX),
    }
}

/// One end of a score range: `-inf`, `+inf`, a whole number, or one after
/// `(`, which leaves it out.
#[derive(Clone, Copy, Debug)]
enum Bound {
    Least,
    Most,
    At(i64, bool),
}

impl Bound {
    /// Whether `score` is past it, as a range's lower end.
    fn starts_before(self, score: i64) -> bool {
        match self {
            Bound::Least => true,
            Bound::Most => false,
            Bound::At(at, open) => score > at || (!open && score == at),
        }
    }

    /// Whether `score` is short of it, as a range's upper end.
    fn ends_after(self, score: i64) -> bool {
        match self {
            Bound::Least => false,
            Bound::Most => true,
            Bound::At(at, open) => score < at || (!open && score == at),
        }
    }
}

fn bound(word: &[u8]) -> Option<Bound> {
    let written = text(word);
    match written.as_str() {
        "-inf" => Some(Bound::Least),
        "+inf" | "inf" => Some(Bound::Most),
        _ => match written.strip_prefix('(') {
            Some(rest) => rest.parse().ok().map(|at| Bound::At(at, true)),
            None => written.parse().ok().map(|at| Bound::At(at, false)),
        },
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
                _ => Err(resp::Unreadable(
                    "a command's word that isn't a bulk string",
                )),
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
        let mut words: Vec<Vec<Vec<u8>>> = address.opening();
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

/// What every store of an entity's records does (area 5, "Entities in
/// Redis"; chunk Q10), asked of `records` at `now`, the operation's clock:
/// the Redis driver against the double or a real server, and the runtime's
/// store in memory, alike. A record is made with its unique value and its
/// indexed value in one commit; read back by key, by page and by count;
/// refused a second commit at the revision it was made at, and refused to
/// another record taking its unique value, each changing nothing; changed
/// at its revision; and once past its expiry by the clock, gone from the
/// page and the count, its unique value free.
pub async fn records_behave(records: &dyn jadpo_runtime::redis::Records, now: i64) {
    use jadpo_runtime::redis::{Change, Check, Commit, PageAsk};
    use jadpo_std::redis::{expiry_key, index_key, record_key, record_prefix, unique_key};
    let space = "tally";
    let (a, b) = (
        "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e",
        "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2f",
    );
    let record = |id: &str| record_key(space, id);
    let unique = unique_key(space, "email", "ab");
    let (index, expiry) = (
        index_key(space, "step", "cd"),
        expiry_key(space, "step", "cd"),
    );
    let ttl = 60_000;
    let kept = |revision: u64| -> Vec<u8> {
        format!(
            "{{\"revision\":{revision},\"expires\":{},\"record\":{{}}}}",
            now + ttl
        )
        .into_bytes()
    };
    let made = |id: &str, revision: u64| Commit {
        checks: vec![
            Check::Revision {
                key: record(id),
                revision,
            },
            Check::Free {
                key: unique.clone(),
                id: id.to_string(),
                records: record_prefix(space),
            },
        ],
        changes: vec![
            Change::Set {
                key: record(id),
                value: kept(revision + 1),
                ttl_ms: ttl,
            },
            Change::Set {
                key: unique.clone(),
                value: id.as_bytes().to_vec(),
                ttl_ms: ttl,
            },
            Change::Add {
                set: index.clone(),
                score: 0,
                member: id.to_string(),
            },
            Change::Add {
                set: expiry.clone(),
                score: now + ttl,
                member: id.to_string(),
            },
            Change::Expire {
                key: index.clone(),
                ttl_ms: ttl,
            },
            Change::Expire {
                key: expiry.clone(),
                ttl_ms: ttl,
            },
        ],
    };
    assert_eq!(records.get(&[record(a)], now).await, Ok(vec![None]));
    assert_eq!(records.commit(&made(a, 0), now).await, Ok(None), "made");
    assert_eq!(
        records.get(&[record(a), record(b)], now).await,
        Ok(vec![Some(kept(1)), None])
    );
    let ask = PageAsk {
        index: index.clone(),
        expiry: expiry.clone(),
        records: record_prefix(space),
        after: None,
        size: 10,
    };
    assert_eq!(
        records.page(&ask, now).await,
        Ok(vec![(a.to_string(), Some(kept(1)))])
    );
    assert_eq!(records.count(&expiry, now).await, Ok(1));
    // Made again at revision 0: the first check fails, nothing changes.
    assert_eq!(records.commit(&made(a, 0), now).await, Ok(Some(0)));
    // Another record taking its unique value: the second check fails.
    assert_eq!(records.commit(&made(b, 0), now).await, Ok(Some(1)));
    assert_eq!(records.get(&[record(b)], now).await, Ok(vec![None]));
    assert_eq!(
        records
            .page(
                &PageAsk {
                    after: Some(a.to_string()),
                    ..ask.clone()
                },
                now
            )
            .await,
        Ok(Vec::new()),
        "nothing after the last"
    );
    // Changed at its revision.
    let changed = Commit {
        checks: vec![Check::Revision {
            key: record(a),
            revision: 1,
        }],
        changes: vec![Change::Set {
            key: record(a),
            value: kept(2),
            ttl_ms: ttl,
        }],
    };
    assert_eq!(records.commit(&changed, now).await, Ok(None));
    assert_eq!(
        records.get(&[record(a)], now).await,
        Ok(vec![Some(kept(2))])
    );
    // Past its expiry by the clock: gone from the page and the count, and
    // its unique value free for another record.
    let later = now + ttl + 1;
    assert_eq!(records.count(&expiry, later).await, Ok(0));
    assert_eq!(records.page(&ask, later).await, Ok(Vec::new()));
    let taken = Commit {
        checks: vec![Check::Free {
            key: unique.clone(),
            id: b.to_string(),
            records: record_prefix(space),
        }],
        changes: Vec::new(),
    };
    assert_eq!(records.commit(&taken, later).await, Ok(None));
}
