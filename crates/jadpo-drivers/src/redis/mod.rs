//! The Redis driver (area 16, "The Redis driver"; chunk Q8): a connection
//! with `driver: Driver.redis`, natively and on Workers, over the host's
//! [`Transport`] (tokio and rustls natively, `connect()` there), speaking
//! RESP ([`resp`]) to any Redis-compatible service: Redis, Valkey,
//! Upstash, Amazon ElastiCache.
//!
//! - **Its address** is the connection's `RedisUrl` ([`Address`]):
//!   `rediss://` is TLS from the first byte, checked against the host's
//!   roots for the host's name, and `redis://` plain TCP; a user and
//!   password sign in with `AUTH`, and a database other than 0 is chosen
//!   with `SELECT`, both written before anything else on a connection.
//! - **One connection, many commands at once.** A driver keeps one
//!   connection, opened when it is first asked and opened again once it
//!   breaks, and each caller's commands are written to it as they come,
//!   their replies read back in order (RESP answers in the order it was
//!   asked), so a server's workers each need one connection however many
//!   requests they serve. A task of the host's runs the connection
//!   ([`Transport::spawn`]); it ends, closing the connection, once the
//!   driver is dropped and nothing is waiting, or when the connection
//!   breaks, failing whatever was waiting.
//! - **A bounded wait.** Each ask waits at most [`ASK_MS`] for its
//!   replies, opening the connection included; one that doesn't answer
//!   in that time breaks the connection, so the next ask opens another.
//! - **What it does.** [`Redis::count`] counts one request against a rate
//!   limit (area 16, "Where the counts are kept"): in one transaction, a
//!   key that starts its window if it isn't there, one more on its count,
//!   and the time its window has left, so every server counting in the
//!   store shares one count and one window, timed by the store's clock.
//!   [`Redis::run`] sends any commands, for what comes next (caches,
//!   entities in Redis).

pub mod address;
pub mod resp;
#[cfg(any(test, feature = "redis-testing"))]
pub mod testing;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use jadpo_runtime::ratelimit::{CountingStore, Unasked};
use jadpo_runtime::{Allowed, LocalFuture};
use jadpo_std::http::RateLimit;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub use address::Address;
use resp::Reply;

use crate::{Endpoint, Stream, Transport, Trust, within};

/// How long an ask waits for its replies, opening the connection
/// included: a store slower than this is treated as one that can't be
/// reached (area 16, "Where the counts are kept").
pub const ASK_MS: u64 = 1_000;

/// What the key of every rate limit's count starts with.
pub const RATE_LIMIT_PREFIX: &str = "jadpo:rate_limit:";

/// Why an ask got no answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The server couldn't be reached, or the connection to it was lost
    /// before it answered.
    Unreachable,
    /// It didn't answer within [`ASK_MS`].
    TimedOut,
    /// It refused: its message, which says what was wrong (a password it
    /// doesn't take, a key holding another kind of value) and never a
    /// value the command carried.
    Refused(String),
    /// What it sent isn't RESP, or isn't the reply the command has.
    Unreadable(&'static str),
    /// Its certificate or its name didn't check, or the host has no TLS.
    Insecure,
}

impl Error {
    /// How the rate limits' fallback names it (area 16): `unreachable`,
    /// `timed_out`, or `failed` for a store that answered with something
    /// other than a count.
    pub fn reason(&self) -> &'static str {
        match self {
            Error::Unreachable => "unreachable",
            Error::TimedOut => "timed_out",
            Error::Refused(_) | Error::Unreadable(_) | Error::Insecure => "failed",
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unreachable => f.write_str("couldn't be reached"),
            Error::TimedOut => write!(f, "didn't answer within {ASK_MS} ms"),
            Error::Refused(message) => write!(f, "refused: {message}"),
            Error::Unreadable(what) => write!(f, "sent what Jadpo can't read: {what}"),
            Error::Insecure => f.write_str("its certificate or name didn't check"),
        }
    }
}

/// A connection with `driver: Driver.redis`.
pub struct Redis {
    transport: Rc<dyn Transport>,
    address: Rc<Address>,
    link: RefCell<Option<Rc<Link>>>,
    ask_ms: u64,
}

impl std::fmt::Debug for Redis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redis")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Redis {
    /// The store at `url`, a `RedisUrl`, over `transport`; why not, never
    /// naming the address, when the driver can't read it. Nothing connects
    /// until it is first asked.
    pub fn new(url: &str, transport: Rc<dyn Transport>) -> Result<Redis, String> {
        Ok(Redis {
            transport,
            address: Rc::new(Address::parse(url)?),
            link: RefCell::new(None),
            ask_ms: ASK_MS,
        })
    }

    /// The same, waiting `millis` for each ask's replies.
    pub fn waiting(mut self, millis: u64) -> Redis {
        self.ask_ms = millis;
        self
    }

    /// Where it connects and how it signs in.
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Sends `commands`, each a command's words, in one write, and gives
    /// their replies in order, a refusal among them as it came; within
    /// [`ASK_MS`].
    pub async fn run(&self, commands: &[Vec<Vec<u8>>]) -> Result<Vec<Reply>, Error> {
        let link = self.link();
        let answer = Rc::new(Answer::default());
        let bytes: Vec<u8> = commands
            .iter()
            .flat_map(|words| resp::command(words))
            .collect();
        link.shared.queue.borrow_mut().push_back(Job {
            bytes,
            replies: commands.len(),
            answer: Some(Rc::clone(&answer)),
        });
        link.shared.wake();
        match within(&*self.transport, self.ask_ms, Waiting(&answer)).await {
            Some(answered) => answered,
            None => {
                link.shared.break_with(Error::TimedOut);
                Err(Error::TimedOut)
            }
        }
    }

    /// Counts one more request for `key` against `limit` (area 16,
    /// "Where the counts are kept"): in one transaction, the count's key
    /// set to 0 expiring after the window if it isn't there, one added to
    /// it, and the time it has left. Over the limit, the request is told to
    /// try again once the window ends. The key is
    /// [`RATE_LIMIT_PREFIX`] and a hash of `key`, so the store never holds
    /// an address or a user's id as it is written.
    pub async fn count(&self, key: &str, limit: RateLimit) -> Result<Allowed, Error> {
        let key = rate_limit_key(key);
        let window = limit.window_ms.max(1).to_string();
        let words = |words: &[&str]| -> Vec<Vec<u8>> {
            words.iter().map(|word| word.as_bytes().to_vec()).collect()
        };
        let replies = self
            .run(&[
                words(&["MULTI"]),
                words(&["SET", &key, "0", "PX", &window, "NX"]),
                words(&["INCR", &key]),
                words(&["PTTL", &key]),
                words(&["EXEC"]),
            ])
            .await?;
        if let Some(message) = replies.iter().find_map(Reply::refused) {
            return Err(Error::Refused(message.to_string()));
        }
        let Some(Reply::Array(Some(answers))) = replies.last() else {
            return Err(Error::Unreadable("a transaction's answers"));
        };
        let [_, Reply::Integer(count), Reply::Integer(left)] = answers.as_slice() else {
            return Err(Error::Unreadable(
                "a count and the time its window has left",
            ));
        };
        if *count <= i64::from(limit.requests) {
            return Ok(Allowed::Yes);
        }
        let window = u64::from(limit.window_ms.div_ceil(1000).max(1));
        let retry_after = match u64::try_from(*left) {
            Ok(left) if left > 0 => left.div_ceil(1000).clamp(1, window),
            _ => window,
        };
        Ok(Allowed::No { retry_after })
    }

    /// The connection, opened now when there is none or it broke.
    fn link(&self) -> Rc<Link> {
        if let Some(link) = &*self.link.borrow()
            && link.shared.broken.borrow().is_none()
        {
            return Rc::clone(link);
        }
        let shared = Rc::new(Shared::default());
        let opening = self.address.opening();
        if !opening.is_empty() {
            shared.queue.borrow_mut().push_back(Job {
                bytes: opening
                    .iter()
                    .flat_map(|words| resp::command(words))
                    .collect(),
                replies: opening.len(),
                answer: None,
            });
        }
        self.transport.spawn(Box::pin(drive(
            Rc::clone(&self.transport),
            Rc::clone(&self.address),
            Rc::clone(&shared),
        )));
        let link = Rc::new(Link { shared });
        *self.link.borrow_mut() = Some(Rc::clone(&link));
        link
    }
}

/// The key a rate limit's count for `key` is kept under.
pub fn rate_limit_key(key: &str) -> String {
    let hash = blake3::hash(key.as_bytes()).to_hex();
    format!(
        "{RATE_LIMIT_PREFIX}{}",
        hash.get(..32).unwrap_or(hash.as_str())
    )
}

/// One connection, as its driver holds it: dropped, it tells the task
/// running it that nothing more will come.
struct Link {
    shared: Rc<Shared>,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.shared.closed.set(true);
        self.shared.wake();
    }
}

/// What a connection's callers and its task share.
#[derive(Default)]
struct Shared {
    /// Commands waiting to be written.
    queue: RefCell<VecDeque<Job>>,
    /// The task's waker, once it has run.
    task: RefCell<Option<Waker>>,
    /// Why it broke, once it has.
    broken: RefCell<Option<Error>>,
    /// Whether its driver has let it go.
    closed: Cell<bool>,
}

impl Shared {
    fn wake(&self) {
        if let Some(waker) = &*self.task.borrow() {
            waker.wake_by_ref();
        }
    }

    /// Breaks it for `why`: the task fails everything waiting and ends.
    fn break_with(&self, why: Error) {
        let mut broken = self.broken.borrow_mut();
        if broken.is_none() {
            *broken = Some(why);
        }
        drop(broken);
        self.wake();
    }
}

/// One caller's commands: their bytes, how many replies they have, and
/// where those go, none for the commands that sign in.
struct Job {
    bytes: Vec<u8>,
    replies: usize,
    answer: Option<Rc<Answer>>,
}

/// Where one caller's replies are put, and who to wake when they are.
#[derive(Default)]
struct Answer {
    got: RefCell<Option<Result<Vec<Reply>, Error>>>,
    waker: RefCell<Option<Waker>>,
}

impl Answer {
    fn give(&self, answered: Result<Vec<Reply>, Error>) {
        *self.got.borrow_mut() = Some(answered);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }
}

/// Waits for an answer.
struct Waiting<'a>(&'a Answer);

impl Future for Waiting<'_> {
    type Output = Result<Vec<Reply>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(answered) = self.0.got.borrow_mut().take() {
            return Poll::Ready(answered);
        }
        *self.0.waker.borrow_mut() = Some(cx.waker().clone());
        Poll::Pending
    }
}

/// Runs one connection: opens it, secures it when its address says to,
/// then writes what callers queue and reads their replies until the
/// driver lets it go or it breaks.
async fn drive(transport: Rc<dyn Transport>, address: Rc<Address>, shared: Rc<Shared>) {
    {
        let ended = match open(&*transport, &address).await {
            Ok(stream) => {
                let mut io = Io::new(stream);
                let ended = std::future::poll_fn(|cx| io.poll(cx, &shared)).await;
                if ended.is_ok() {
                    // Its goodbye: the server closes its end in turn.
                    let mut stream = io.stream;
                    let _ = within(
                        &*transport,
                        ASK_MS,
                        std::future::poll_fn(|cx| Pin::new(&mut stream).poll_shutdown(cx)),
                    )
                    .await;
                    return;
                }
                ended
            }
            Err(why) => Err(why),
        };
        if let Err(why) = ended {
            shared.break_with(why.clone());
            let why = shared.broken.borrow().clone().unwrap_or(why);
            let waiting: Vec<Job> = shared.queue.borrow_mut().drain(..).collect();
            for job in waiting {
                if let Some(answer) = job.answer {
                    answer.give(Err(why.clone()));
                }
            }
        }
    }
}

/// A stream to `address`, TLS over it for `rediss://`.
async fn open(transport: &dyn Transport, address: &Address) -> Result<Box<dyn Stream>, Error> {
    let endpoint = Endpoint::Tcp {
        host: address.host.clone(),
        port: address.port,
    };
    let stream = transport
        .connect(&endpoint)
        .await
        .map_err(|_| Error::Unreachable)?;
    if !address.tls {
        return Ok(stream);
    }
    transport
        .upgrade(stream, &address.host, Trust::Roots)
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::InvalidData
            | std::io::ErrorKind::InvalidInput
            | std::io::ErrorKind::Unsupported => Error::Insecure,
            _ => Error::Unreachable,
        })
}

/// One caller's replies as they come in.
struct Pending {
    want: usize,
    got: Vec<Reply>,
    answer: Option<Rc<Answer>>,
}

/// A connection's stream and what is going over it.
struct Io {
    stream: Box<dyn Stream>,
    /// Bytes to write, from `written` on.
    out: Vec<u8>,
    written: usize,
    flushing: bool,
    /// Bytes read and not yet a whole reply.
    input: Vec<u8>,
    /// Each caller whose commands have gone, oldest first.
    pending: VecDeque<Pending>,
}

impl Io {
    fn new(stream: Box<dyn Stream>) -> Io {
        Io {
            stream,
            out: Vec::new(),
            written: 0,
            flushing: false,
            input: Vec::new(),
            pending: VecDeque::new(),
        }
    }

    /// Moves everything along that can move: `Ready(Ok)` once the driver
    /// has let it go and nothing is waiting, `Ready(Err)` when it broke.
    fn poll(&mut self, cx: &mut Context<'_>, shared: &Shared) -> Poll<Result<(), Error>> {
        *shared.task.borrow_mut() = Some(cx.waker().clone());
        let ended = self.moved(cx, shared);
        if let Poll::Ready(Err(why)) = &ended {
            shared.break_with(why.clone());
            let why = shared
                .broken
                .borrow()
                .clone()
                .unwrap_or_else(|| why.clone());
            for pending in self.pending.drain(..) {
                if let Some(answer) = pending.answer {
                    answer.give(Err(why.clone()));
                }
            }
        }
        ended
    }

    fn moved(&mut self, cx: &mut Context<'_>, shared: &Shared) -> Poll<Result<(), Error>> {
        loop {
            if let Some(why) = shared.broken.borrow().clone() {
                return Poll::Ready(Err(why));
            }
            for job in shared.queue.borrow_mut().drain(..) {
                self.out.extend_from_slice(&job.bytes);
                self.pending.push_back(Pending {
                    want: job.replies,
                    got: Vec::with_capacity(job.replies),
                    answer: job.answer,
                });
            }
            let mut moved = false;
            while let Some(rest) = self.out.get(self.written..).filter(|rest| !rest.is_empty()) {
                match Pin::new(&mut self.stream).poll_write(cx, rest) {
                    Poll::Ready(Ok(0)) | Poll::Ready(Err(_)) => {
                        return Poll::Ready(Err(Error::Unreachable));
                    }
                    Poll::Ready(Ok(wrote)) => {
                        self.written += wrote;
                        moved = true;
                    }
                    Poll::Pending => break,
                }
            }
            if self.written > 0 && self.written == self.out.len() {
                self.out.clear();
                self.written = 0;
                self.flushing = true;
            }
            if self.flushing {
                match Pin::new(&mut self.stream).poll_flush(cx) {
                    Poll::Ready(Ok(())) => self.flushing = false,
                    Poll::Ready(Err(_)) => return Poll::Ready(Err(Error::Unreachable)),
                    Poll::Pending => {}
                }
            }
            // Always reading, so a server that closes an idle connection
            // is seen at once, and the next ask opens another.
            let mut buffer = [0u8; 8192];
            let mut read = ReadBuf::new(&mut buffer);
            match Pin::new(&mut self.stream).poll_read(cx, &mut read) {
                Poll::Ready(Ok(())) if read.filled().is_empty() => {
                    return Poll::Ready(Err(Error::Unreachable));
                }
                Poll::Ready(Ok(())) => {
                    self.input.extend_from_slice(read.filled());
                    if self.input.len() > resp::MOST_REPLY_BYTES + 1024 {
                        return Poll::Ready(Err(Error::Unreadable(
                            "a reply longer than the driver reads",
                        )));
                    }
                    if let Err(why) = self.dispatch() {
                        return Poll::Ready(Err(why));
                    }
                    moved = true;
                }
                Poll::Ready(Err(_)) => return Poll::Ready(Err(Error::Unreachable)),
                Poll::Pending => {}
            }
            if shared.closed.get()
                && shared.queue.borrow().is_empty()
                && self.out.is_empty()
                && !self.flushing
                && self.pending.is_empty()
            {
                return Poll::Ready(Ok(()));
            }
            if !moved {
                return Poll::Pending;
            }
        }
    }

    /// Hands each whole reply read to the caller it is for, in order.
    fn dispatch(&mut self) -> Result<(), Error> {
        loop {
            let parsed = resp::parse(&self.input).map_err(|why| Error::Unreadable(why.0))?;
            let Some((reply, used)) = parsed else {
                return Ok(());
            };
            self.input.drain(..used);
            let Some(front) = self.pending.front_mut() else {
                return Err(Error::Unreadable("a reply nothing asked for"));
            };
            front.got.push(reply);
            if front.got.len() < front.want {
                continue;
            }
            let Some(done) = self.pending.pop_front() else {
                continue;
            };
            match done.answer {
                Some(answer) => answer.give(Ok(done.got)),
                // Signing in or choosing the database was refused: nothing
                // after it can run.
                None => {
                    if let Some(message) = done.got.iter().find_map(Reply::refused) {
                        return Err(Error::Refused(message.to_string()));
                    }
                }
            }
        }
    }
}

/// What a host counts its rate limits in (area 16, "Where the counts are
/// kept").
impl CountingStore for Redis {
    fn count<'a>(
        &'a self,
        key: &'a str,
        limit: RateLimit,
    ) -> LocalFuture<'a, Result<Allowed, Unasked>> {
        Box::pin(async move {
            Redis::count(self, key, limit)
                .await
                .map_err(|error| match error {
                    Error::Unreachable => Unasked::Unreachable,
                    Error::TimedOut => Unasked::TimedOut,
                    Error::Refused(_) | Error::Unreadable(_) | Error::Insecure => Unasked::Failed,
                })
        })
    }
}

#[cfg(test)]
mod tests;
