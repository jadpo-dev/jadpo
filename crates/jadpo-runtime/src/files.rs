//! The `files` capability (area 16, "Files"; chunk 6.4b), on the
//! capability seam ([`crate::effects::call`]): a store's `put` and a ranged
//! `read` of a stored file, each an outside call through the connection
//! that fills `files`, so each is refused inside an open transaction,
//! logged without its bytes, and recorded and replayed with the other
//! effects.
//!
//! - [`put`]: `store.put(bytes, type)` in an action: the bytes kept under
//!   a key the runtime makes from the invocation's randomness
//!   ([`jadpo_std::files::key_for`]), sent as a keyed call (a repeat
//!   writes the same immutable object again), giving the `File`; a store
//!   with no room is `storage_full`, the condition the call's `attempt`
//!   maps.
//! - [`read`]: a range of a stored file's bytes, what a batch run reads a
//!   page of rows from before its page's transaction opens (chunk 6.4b),
//!   and [`read_whole`], `store.read(file)`, which reads at most
//!   [`WHOLE_READ_LIMIT`] bytes into memory.
//! - [`MemoryFiles`]: the in-memory store tests run on (area 13, "Provider
//!   calls and capabilities in tests"), which can be told to have no room
//!   and to read a few bytes at a time.
//!
//! A `File` is a record of its store, key, name, size and type
//! ([`jadpo_std::files::FILE_FIELDS`]), which only the runtime builds.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::RuleKey;
use jadpo_std::files::{FILE, NOT_FOUND, PUT, READ, STORAGE_FULL, WHOLE_READ_LIMIT, key_for};
use jadpo_value::Value;

use crate::capability::{Answer, Call, Capability, Repeat, Source};
use crate::effects::{Unanswered, call};
use crate::{Ctx, Failure, FaultKind, LocalFuture};

/// The most bytes one read asks a store for: a page of rows is read in
/// windows of this size until it holds its rows (chunk 6.4b).
pub const WINDOW: u64 = 1 << 20;

/// What a store's operation gives the site that called it.
#[derive(Debug)]
pub enum Done {
    Value(Value),
    /// A condition the call's `attempt` maps: `storage_full`.
    Broke(RuleKey),
    /// What the runtime raises in the call's place: `unavailable`,
    /// `misconfigured`.
    Failed(Box<Failure>),
    Fault(FaultKind),
}

/// A stored file's parts the runtime reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    /// The connection holding its bytes.
    pub store: String,
    pub key: String,
    pub size: u64,
}

/// A `File` (area 16): what an entity field keeps and `store.put` gives.
pub fn file_value(store: &str, key: &str, name: Option<&str>, size: i64, file_type: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::FILE_TYPE, file_type).unwrap_or(0);
    Value::record(
        FILE,
        vec![
            (Rc::from("store"), Value::text(store)),
            (Rc::from("key"), Value::text(key)),
            (Rc::from("name"), name.map_or(Value::None, Value::text)),
            (Rc::from("size"), Value::Int(size)),
            (
                Rc::from("type"),
                Value::variant(jadpo_std::FILE_TYPE, ordinal, file_type, Vec::new()),
            ),
        ],
    )
}

/// The parts of `file`, a `File`; `None` for anything else.
pub fn stored(file: &Value) -> Option<Stored> {
    let Value::Record(record) = file else {
        return None;
    };
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(text.to_string()),
        _ => None,
    };
    let size = match record.get("size") {
        Some(Value::Int(size)) => u64::try_from(*size).ok()?,
        _ => return None,
    };
    Some(Stored {
        store: text("store")?,
        key: text("key")?,
        size,
    })
}

/// The media type of a `FileType` value.
fn media_type(file_type: &Value) -> Option<&'static str> {
    let Value::Variant(variant) = file_type else {
        return None;
    };
    jadpo_std::FILE_TYPES
        .iter()
        .find(|(name, _)| *name == &*variant.name)
        .map(|(_, media)| *media)
}

/// What an outside call's refusal is at the site that made it.
fn unanswered(refused: Unanswered, what: &str) -> Done {
    match refused {
        Unanswered::Failure(failure) => Done::Failed(failure),
        Unanswered::Fault(kind) => Done::Fault(kind),
        Unanswered::Failed(code) => Done::Fault(FaultKind::Defect(format!(
            "{what} answered `{code}`, which nothing maps"
        ))),
    }
}

/// `store.put(bytes, type)` through the `files` connection `connection`
/// (area 16): the bytes kept under a key the runtime makes, giving the
/// `File`.
pub async fn put(ctx: &mut Ctx<'_>, connection: &str, bytes: &Value, file_type: &Value) -> Done {
    let Value::Bytes(held) = bytes else {
        return Done::Fault(FaultKind::Defect("a put given no bytes".into()));
    };
    let (Some(media), Value::Variant(variant)) = (media_type(file_type), file_type) else {
        return Done::Fault(FaultKind::Defect("a put given no file type".into()));
    };
    let Some(id) = ctx.new_id() else {
        return Done::Fault(FaultKind::Defect(
            "a file's key with no randomness to make it".into(),
        ));
    };
    let key = key_for(&id);
    let size = i64::try_from(held.len()).unwrap_or(i64::MAX);
    let input = Value::record(
        "FilePut",
        vec![
            (Rc::from("key"), Value::text(&key)),
            (Rc::from("type"), Value::text(media)),
            (Rc::from("bytes"), bytes.clone()),
        ],
    );
    let sent = call(ctx, Call::new(connection, PUT, input, Repeat::Keyed)).await;
    match sent {
        Ok(_) => Done::Value(file_value(connection, &key, None, size, &variant.name)),
        Err(Unanswered::Failed(code)) if code == STORAGE_FULL => Done::Broke(RuleKey::StorageFull),
        Err(refused) => unanswered(refused, &format!("`{connection}.put`")),
    }
}

/// At most `length` of `file`'s bytes from `from`, through its store:
/// fewer only at its end, or when the store reads less at once.
pub async fn read(
    ctx: &mut Ctx<'_>,
    file: &Stored,
    from: u64,
    length: u64,
) -> Result<Vec<u8>, Unanswered> {
    let input = Value::record(
        "FileRead",
        vec![
            (Rc::from("key"), Value::text(&file.key)),
            (
                Rc::from("from"),
                Value::Int(i64::try_from(from).unwrap_or(i64::MAX)),
            ),
            (
                Rc::from("length"),
                Value::Int(i64::try_from(length).unwrap_or(i64::MAX)),
            ),
        ],
    );
    match call(ctx, Call::new(&file.store, READ, input, Repeat::Safe)).await? {
        Value::Bytes(bytes) => Ok(bytes.to_vec()),
        _ => Err(Unanswered::Fault(FaultKind::Defect(format!(
            "`{}.read` answered with no bytes",
            file.store
        )))),
    }
}

/// `store.read(file)` (area 16): the file's bytes whole, as
/// `FileContents`, read in windows; a file over [`WHOLE_READ_LIMIT`] is
/// refused before anything is read.
pub async fn read_whole(ctx: &mut Ctx<'_>, file: &Value) -> Done {
    let Some(stored) = stored(file) else {
        return Done::Fault(FaultKind::Defect("a read given no stored file".into()));
    };
    if stored.size > WHOLE_READ_LIMIT {
        return Done::Fault(FaultKind::Defect(format!(
            "a whole read of a file over {}, which only the format helpers and `batches.each` read",
            jadpo_std::files::shown_size(WHOLE_READ_LIMIT)
        )));
    }
    let mut bytes: Vec<u8> = Vec::with_capacity(usize::try_from(stored.size).unwrap_or(0));
    while (bytes.len() as u64) < stored.size {
        let from = bytes.len() as u64;
        match read(ctx, &stored, from, (stored.size - from).min(WINDOW)).await {
            Ok(more) if more.is_empty() => {
                return Done::Fault(FaultKind::Defect(
                    "a stored file shorter than its size".into(),
                ));
            }
            Ok(more) => bytes.extend_from_slice(&more),
            Err(Unanswered::Failed(code)) if code == NOT_FOUND => {
                return Done::Fault(FaultKind::Defect(
                    "a stored file whose bytes are no longer in its store".into(),
                ));
            }
            Err(refused) => return unanswered(refused, &format!("`{}.read`", stored.store)),
        }
    }
    Done::Value(Value::Bytes(Rc::from(bytes)))
}

/// The `files` capability in memory (area 13, "Provider calls and
/// capabilities in tests"): each key's bytes kept in a map, read `read_at_most`
/// bytes at a time at most, as a store reading in pieces would; told to,
/// it answers a put `storage_full`.
#[derive(Debug, Default)]
pub struct MemoryFiles {
    files: RefCell<BTreeMap<String, Vec<u8>>>,
    read_at_most: Option<usize>,
    full: Cell<bool>,
    reads: Cell<usize>,
    kept: Cell<u64>,
}

impl MemoryFiles {
    pub fn new() -> MemoryFiles {
        MemoryFiles::default()
    }

    /// Reads at most `bytes` at a time.
    pub fn reading_at_most(mut self, bytes: usize) -> MemoryFiles {
        self.read_at_most = Some(bytes.max(1));
        self
    }

    /// Has no room from now on: every put answers `storage_full`.
    pub fn fill(&self) {
        self.full.set(true);
    }

    /// The bytes kept under `key`.
    pub fn bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.files.borrow().get(key).cloned()
    }

    /// How many reads it has answered.
    pub fn reads(&self) -> usize {
        self.reads.get()
    }

    /// `bytes` kept as a file of the `FileType` variant `file_type` in
    /// connection `store`, as a test's fixture file is (area 13): the
    /// `File` an action is given.
    pub fn keep(&self, store: &str, bytes: &[u8], file_type: &str) -> Value {
        self.kept.set(self.kept.get() + 1);
        let mut id = [0xf0u8; 16];
        for (slot, byte) in id.iter_mut().rev().zip(self.kept.get().to_le_bytes()) {
            *slot = byte;
        }
        let key = key_for(&id);
        self.files.borrow_mut().insert(key.clone(), bytes.to_vec());
        file_value(
            store,
            &key,
            None,
            i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            file_type,
        )
    }

    fn answer(&self, call: &Call) -> Answer {
        let Value::Record(input) = &call.input else {
            return Answer::Misconfigured;
        };
        let Some(Value::Text(key)) = input.get("key") else {
            return Answer::Misconfigured;
        };
        if !jadpo_std::files::is_key(key) {
            return Answer::Failed(NOT_FOUND.into());
        }
        match call.operation.as_str() {
            PUT => {
                if self.full.get() {
                    return Answer::Failed(STORAGE_FULL.into());
                }
                let Some(Value::Bytes(bytes)) = input.get("bytes") else {
                    return Answer::Misconfigured;
                };
                self.files
                    .borrow_mut()
                    .insert(key.to_string(), bytes.to_vec());
                Answer::Value(Value::Unit)
            }
            READ => {
                self.reads.set(self.reads.get() + 1);
                let (Some(Value::Int(from)), Some(Value::Int(length))) =
                    (input.get("from"), input.get("length"))
                else {
                    return Answer::Misconfigured;
                };
                let files = self.files.borrow();
                let Some(bytes) = files.get(&**key) else {
                    return Answer::Failed(NOT_FOUND.into());
                };
                let from = usize::try_from(*from)
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                let mut length = usize::try_from(*length).unwrap_or(0);
                if let Some(most) = self.read_at_most {
                    length = length.min(most);
                }
                let to = from.saturating_add(length).min(bytes.len());
                Answer::Value(Value::Bytes(Rc::from(
                    bytes.get(from..to).unwrap_or_default(),
                )))
            }
            _ => Answer::Misconfigured,
        }
    }
}

impl Capability for MemoryFiles {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let answer = self.answer(call);
        Box::pin(async move { answer })
    }

    fn source(&self) -> Source {
        Source::Fake
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Faked;
    use crate::{Clock, Env, MemoryHost, Places};

    fn env() -> Env {
        Env {
            seed: Some(1),
            clock: Clock::TEST,
        }
    }

    /// A put keeps the bytes under a key the runtime makes and gives the
    /// `File`; a ranged read gives them back in pieces, and a whole read
    /// all of them; a full store answers `storage_full`, which the call's
    /// `attempt` maps.
    #[test]
    fn a_put_keeps_bytes_a_read_gives_them_back_and_a_full_store_refuses() {
        let files = Rc::new(MemoryFiles::new().reading_at_most(3));
        let host =
            Faked::new(MemoryHost::new()).with("imports", Rc::clone(&files) as Rc<dyn Capability>);
        let places = Places::new();
        let mut ctx = Ctx::new(&places, &host, env());
        let csv = Value::variant(
            jadpo_std::FILE_TYPE,
            jadpo_std::variant_ordinal(jadpo_std::FILE_TYPE, "csv").unwrap_or(0),
            "csv",
            Vec::new(),
        );
        let bytes = Value::Bytes(Rc::from(&b"a,b\n1,2\n"[..]));
        let kept = crate::block_on(put(&mut ctx, "imports", &bytes, &csv)).expect("no wait");
        let Done::Value(file) = kept else {
            panic!("a file: {kept:?}");
        };
        let kept = stored(&file).expect("a stored file");
        assert_eq!(kept.store, "imports");
        assert_eq!(kept.size, 8);
        assert!(jadpo_std::files::is_key(&kept.key), "{}", kept.key);
        assert_eq!(files.bytes(&kept.key).as_deref(), Some(&b"a,b\n1,2\n"[..]));

        let piece = crate::block_on(read(&mut ctx, &kept, 2, 100))
            .expect("no wait")
            .expect("bytes");
        assert_eq!(piece, b",b\n", "three at a time");
        let whole = crate::block_on(read_whole(&mut ctx, &file)).expect("no wait");
        assert!(
            matches!(&whole, Done::Value(Value::Bytes(all)) if &**all == b"a,b\n1,2\n"),
            "{whole:?}"
        );
        // Logged without their bytes.
        let shown = format!("{:?}", ctx.capabilities().calls());
        assert!(!shown.contains("1,2"), "{shown}");

        files.fill();
        let full = crate::block_on(put(&mut ctx, "imports", &bytes, &csv)).expect("no wait");
        assert!(
            matches!(full, Done::Broke(RuleKey::StorageFull)),
            "{full:?}"
        );

        let large = file_value(
            "imports",
            &kept.key,
            None,
            WHOLE_READ_LIMIT as i64 + 1,
            "csv",
        );
        let refused = crate::block_on(read_whole(&mut ctx, &large)).expect("no wait");
        assert!(
            matches!(&refused, Done::Fault(FaultKind::Defect(what)) if what.contains("10 MB")),
            "{refused:?}"
        );
    }
}
