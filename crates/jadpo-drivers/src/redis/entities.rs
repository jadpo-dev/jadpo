//! Entities in Redis (area 5, "Entities in Redis"; chunk Q10): what the
//! driver sends to keep an entity's records, as the runtime asks
//! ([`jadpo_runtime::redis::Records`]).
//!
//! - **A read** of records by key is one `MGET`, and a count of an
//!   indexed value's records one `ZCOUNT` of its expiry set.
//! - **A page** is one script, [`PAGE`]: it takes the ids whose records
//!   expired out of the value's two sets, then gives the next ids in
//!   order, by `ZRANGEBYLEX`, each with its record, so one round trip
//!   answers it.
//! - **A commit** is one script, [`COMMIT`]: it asks every check, then
//!   makes every change, as one step no other command comes between, which
//!   is how Redis runs a script; a check that doesn't hold changes nothing
//!   and says which it was. Its words are [`commit_words`]'.
//!
//! Scripts need no module and run on every Redis-compatible service the
//! driver reaches (Redis, Valkey, Upstash, ElastiCache): each is sent
//! whole with `EVAL`, naming no key in `KEYS`, since the driver talks to
//! one server.

use jadpo_runtime::LocalFuture;
use jadpo_runtime::ratelimit::Unasked;
use jadpo_runtime::redis::{Change, Check, Commit, Listed, PageAsk, Records};

use super::resp::Reply;
use super::{Error, Redis, unasked};

/// A commit's script: its `ARGV` is the operation's clock, how many
/// checks there are, then each check's words (`R key revision`, `F key id
/// records`) and each change's (`S key value ttl`, `D key`, `A set score
/// member`, `X set member`, `E key ttl`). It answers 0 once every change
/// is made, or the place, from 1, of the first check that didn't hold.
pub const COMMIT: &str = r#"local now = tonumber(ARGV[1])
local function revision(value)
  if not value then return '0' end
  local rev, expires = string.match(value, '^{"revision":(%d+),"expires":(%d+),')
  if not rev or tonumber(expires) <= now then return '0' end
  return rev
end
local checks = tonumber(ARGV[2])
local at = 3
for c = 1, checks do
  if ARGV[at] == 'R' then
    if revision(redis.call('GET', ARGV[at + 1])) ~= ARGV[at + 2] then return c end
    at = at + 3
  else
    local held = redis.call('GET', ARGV[at + 1])
    if held and held ~= ARGV[at + 2] and revision(redis.call('GET', ARGV[at + 3] .. held)) ~= '0' then return c end
    at = at + 4
  end
end
while at <= #ARGV do
  local kind = ARGV[at]
  if kind == 'S' then
    redis.call('SET', ARGV[at + 1], ARGV[at + 2], 'PX', ARGV[at + 3])
    at = at + 4
  elseif kind == 'D' then
    redis.call('DEL', ARGV[at + 1])
    at = at + 2
  elseif kind == 'A' then
    redis.call('ZADD', ARGV[at + 1], ARGV[at + 2], ARGV[at + 3])
    at = at + 4
  elseif kind == 'X' then
    redis.call('ZREM', ARGV[at + 1], ARGV[at + 2])
    at = at + 3
  else
    redis.call('PEXPIRE', ARGV[at + 1], ARGV[at + 2])
    at = at + 3
  end
end
return 0
"#;

/// A page's script: its `ARGV` is the value's set of ids, its set of
/// expiries, what every record key starts with, the operation's clock,
/// the id the page continues after (empty for the first page) and how
/// many ids at most. It answers each id with its record, in turn.
pub const PAGE: &str = r#"local index, expiry, records, now, after, size = ARGV[1], ARGV[2], ARGV[3], ARGV[4], ARGV[5], ARGV[6]
local gone = redis.call('ZRANGEBYSCORE', expiry, '-inf', now)
for i = 1, #gone, 500 do
  local some = {}
  for j = i, math.min(i + 499, #gone) do some[#some + 1] = gone[j] end
  redis.call('ZREM', index, unpack(some))
  redis.call('ZREM', expiry, unpack(some))
end
local from = '-'
if after ~= '' then from = '(' .. after end
local ids = redis.call('ZRANGEBYLEX', index, from, '+', 'LIMIT', 0, size)
local out = {}
for i, id in ipairs(ids) do
  out[2 * i - 1] = id
  out[2 * i] = redis.call('GET', records .. id)
end
return out
"#;

/// `commit`'s `ARGV`, after the script and its `0` keys, at `now`.
pub fn commit_words(commit: &Commit, now: i64) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![
        now.to_string().into_bytes(),
        commit.checks.len().to_string().into_bytes(),
    ];
    for check in &commit.checks {
        match check {
            Check::Revision { key, revision } => {
                out.push(b"R".to_vec());
                out.push(key.clone().into_bytes());
                out.push(revision.to_string().into_bytes());
            }
            Check::Free { key, id, records } => {
                out.push(b"F".to_vec());
                out.push(key.clone().into_bytes());
                out.push(id.clone().into_bytes());
                out.push(records.clone().into_bytes());
            }
        }
    }
    for change in &commit.changes {
        match change {
            Change::Set { key, value, ttl_ms } => {
                out.push(b"S".to_vec());
                out.push(key.clone().into_bytes());
                out.push(value.clone());
                out.push(ttl_ms.max(&1).to_string().into_bytes());
            }
            Change::Delete { key } => {
                out.push(b"D".to_vec());
                out.push(key.clone().into_bytes());
            }
            Change::Add { set, score, member } => {
                out.push(b"A".to_vec());
                out.push(set.clone().into_bytes());
                out.push(score.to_string().into_bytes());
                out.push(member.clone().into_bytes());
            }
            Change::Remove { set, member } => {
                out.push(b"X".to_vec());
                out.push(set.clone().into_bytes());
                out.push(member.clone().into_bytes());
            }
            Change::Expire { key, ttl_ms } => {
                out.push(b"E".to_vec());
                out.push(key.clone().into_bytes());
                out.push(ttl_ms.max(&1).to_string().into_bytes());
            }
        }
    }
    out
}

/// `EVAL script 0 words…`.
fn eval(script: &str, words: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut command = vec![b"EVAL".to_vec(), script.as_bytes().to_vec(), b"0".to_vec()];
    command.extend(words);
    command
}

/// The one reply `replies` holds, or the refusal among them.
fn only(replies: Vec<Reply>) -> Result<Reply, Error> {
    if let Some(message) = replies.iter().find_map(Reply::refused) {
        return Err(Error::Refused(message.to_string()));
    }
    replies
        .into_iter()
        .next()
        .ok_or(Error::Unreadable("a reply"))
}

impl Redis {
    /// What each of `keys` holds, in one `MGET`.
    pub async fn records(&self, keys: &[String]) -> Result<Vec<Option<Vec<u8>>>, Error> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let mut command = vec![b"MGET".to_vec()];
        command.extend(keys.iter().map(|key| key.clone().into_bytes()));
        match only(self.run(&[command]).await?)? {
            Reply::Array(Some(items)) if items.len() == keys.len() => items
                .into_iter()
                .map(|item| match item {
                    Reply::Bulk(bytes) => Ok(bytes),
                    _ => Err(Error::Unreadable("a record")),
                })
                .collect(),
            _ => Err(Error::Unreadable("the records asked for")),
        }
    }

    /// A page of an indexed value's records, in one script.
    pub async fn page(&self, ask: &PageAsk, now: i64) -> Result<Listed, Error> {
        let words = vec![
            ask.index.clone().into_bytes(),
            ask.expiry.clone().into_bytes(),
            ask.records.clone().into_bytes(),
            now.to_string().into_bytes(),
            ask.after.clone().unwrap_or_default().into_bytes(),
            ask.size.to_string().into_bytes(),
        ];
        let Reply::Array(Some(items)) = only(self.run(&[eval(PAGE, words)]).await?)? else {
            return Err(Error::Unreadable("a page of records"));
        };
        let mut out = Vec::with_capacity(items.len() / 2);
        let mut items = items.into_iter();
        while let Some(id) = items.next() {
            let Reply::Bulk(Some(id)) = id else {
                return Err(Error::Unreadable("a record's id"));
            };
            let id = String::from_utf8(id).map_err(|_| Error::Unreadable("a record's id"))?;
            let record = match items.next() {
                Some(Reply::Bulk(bytes)) => bytes,
                _ => return Err(Error::Unreadable("a record")),
            };
            out.push((id, record));
        }
        Ok(out)
    }

    /// How many ids the set `expiry` holds that expire after `now`, in one
    /// `ZCOUNT`.
    pub async fn count_live(&self, expiry: &str, now: i64) -> Result<u64, Error> {
        let command = vec![
            b"ZCOUNT".to_vec(),
            expiry.as_bytes().to_vec(),
            format!("({now}").into_bytes(),
            b"+inf".to_vec(),
        ];
        match only(self.run(&[command]).await?)? {
            Reply::Integer(count) => u64::try_from(count).map_err(|_| Error::Unreadable("a count")),
            _ => Err(Error::Unreadable("a count")),
        }
    }

    /// Asks `commit`'s checks and makes its changes, in one script: `None`
    /// once made, or the place, from 0, of the check that didn't hold.
    pub async fn commit(&self, commit: &Commit, now: i64) -> Result<Option<u32>, Error> {
        match only(self.run(&[eval(COMMIT, commit_words(commit, now))]).await?)? {
            Reply::Integer(0) => Ok(None),
            Reply::Integer(at) if at > 0 => u32::try_from(at - 1)
                .map(Some)
                .map_err(|_| Error::Unreadable("a check's place")),
            _ => Err(Error::Unreadable("what a commit made")),
        }
    }
}

/// What a host keeps entities in Redis in (area 5, "Entities in Redis").
impl Records for Redis {
    fn get<'a>(
        &'a self,
        keys: &'a [String],
        _now: i64,
    ) -> LocalFuture<'a, Result<Vec<Option<Vec<u8>>>, Unasked>> {
        Box::pin(async move { Redis::records(self, keys).await.map_err(unasked) })
    }

    fn page<'a>(&'a self, ask: &'a PageAsk, now: i64) -> LocalFuture<'a, Result<Listed, Unasked>> {
        Box::pin(async move { Redis::page(self, ask, now).await.map_err(unasked) })
    }

    fn count<'a>(&'a self, expiry: &'a str, now: i64) -> LocalFuture<'a, Result<u64, Unasked>> {
        Box::pin(async move { Redis::count_live(self, expiry, now).await.map_err(unasked) })
    }

    fn commit<'a>(
        &'a self,
        commit: &'a Commit,
        now: i64,
    ) -> LocalFuture<'a, Result<Option<u32>, Unasked>> {
        Box::pin(async move { Redis::commit(self, commit, now).await.map_err(unasked) })
    }
}

/// `words`, a commit's `ARGV`, read back: what the double runs (chunk
/// Q10), checks and changes as [`commit_words`] wrote them.
pub fn read_commit(words: &[Vec<u8>]) -> Option<(i64, Commit)> {
    let text = |at: usize| -> Option<String> { String::from_utf8(words.get(at)?.clone()).ok() };
    let number = |at: usize| -> Option<i64> { text(at)?.parse().ok() };
    let now = number(0)?;
    let checks = usize::try_from(number(1)?).ok()?;
    let mut commit = Commit::default();
    let mut at = 2;
    for _ in 0..checks {
        match text(at)?.as_str() {
            "R" => {
                commit.checks.push(Check::Revision {
                    key: text(at + 1)?,
                    revision: text(at + 2)?.parse().ok()?,
                });
                at += 3;
            }
            "F" => {
                commit.checks.push(Check::Free {
                    key: text(at + 1)?,
                    id: text(at + 2)?,
                    records: text(at + 3)?,
                });
                at += 4;
            }
            _ => return None,
        }
    }
    while at < words.len() {
        match text(at)?.as_str() {
            "S" => {
                commit.changes.push(Change::Set {
                    key: text(at + 1)?,
                    value: words.get(at + 2)?.clone(),
                    ttl_ms: number(at + 3)?,
                });
                at += 4;
            }
            "D" => {
                commit.changes.push(Change::Delete { key: text(at + 1)? });
                at += 2;
            }
            "A" => {
                commit.changes.push(Change::Add {
                    set: text(at + 1)?,
                    score: number(at + 2)?,
                    member: text(at + 3)?,
                });
                at += 4;
            }
            "X" => {
                commit.changes.push(Change::Remove {
                    set: text(at + 1)?,
                    member: text(at + 2)?,
                });
                at += 3;
            }
            "E" => {
                commit.changes.push(Change::Expire {
                    key: text(at + 1)?,
                    ttl_ms: number(at + 2)?,
                });
                at += 3;
            }
            _ => return None,
        }
    }
    Some((now, commit))
}
