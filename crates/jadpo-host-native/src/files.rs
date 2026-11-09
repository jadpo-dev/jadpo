//! The disk driver (area 16, "The disk driver"; chunks 6.4b and B2): the
//! `files` capability on the server's own disk. A native deployable keeps
//! each `Driver.disk` store at its connection's `path:` ([`DiskStore`],
//! which [`ServerDisk`] gives the driver factory); `jadpo dev` keeps every
//! store under `.jadpo/files/<connection>/` ([`DiskFiles`]), so nothing
//! leaves the machine. Both are one store's driver over one folder, its
//! root:
//!
//! - **Keys the runtime makes.** A file is kept under its key, two
//!   directory levels sharded by its id ([`jadpo_std::files::key_for`]);
//!   the driver refuses any other key, so no key names a path outside its
//!   root.
//! - **No links followed.** Each folder and file below the root is opened
//!   relative to the one above it, never following a symbolic link
//!   (`openat` with `O_NOFOLLOW` on Unix), so a link planted anywhere on
//!   the way names nothing, and nothing outside the root is read or
//!   written through one; a read opens only a regular file, never
//!   waiting on a pipe.
//! - **The app's user only.** Folders it makes are `0700` and files
//!   `0600`. A deployable's root is made `0700` when it is missing, and a
//!   root other users can write to is refused as the deployable starts
//!   ([`DiskStore::at`]), since they could plant what it reads.
//! - **Writes that land whole.** A put writes a temporary file of its own,
//!   created new, syncs it, renames it into place and syncs its folder, so
//!   a crash leaves the old file or the new, never part of one.
//! - **Space.** A full disk answers `storage_full`, and so does a put or a
//!   join that would leave less free than the store's `free_space_floor:`
//!   (5% by default), so imports and the database aren't starved of space.
//! - **Ranged reads.** A read seeks to its first byte and reads at most
//!   what it was asked for, so a batch reads a file of any size a window
//!   at a time.
//! - **Pieces joined whole.** An export's pieces (chunk 6.4b2) are each a
//!   file of their own under a key made from the file's; joining copies
//!   them in order into a temporary file, a window at a time, syncs it,
//!   renames it into place and syncs its folder, then removes the pieces,
//!   so the file appears whole or not at all, and joining it again once it
//!   is does nothing.
//!
//! - **Deletes that last.** A delete removes the file's name from its
//!   folder, a link there itself, never what it names, and syncs the
//!   folder; a key naming nothing is deleted already, so a repeat acts
//!   once (chunk B3).
//!
//! - **Uploads that land whole.** An upload ([`DiskStore::receive`]) goes
//!   to a temporary file of its own as it arrives, so a client that stops
//!   leaves nothing; the file is synced and linked into place, which a file
//!   already there refuses, so a slot keeps one file and files never
//!   change, and the folder is synced. What an upload says of itself is
//!   kept beside it ([`meta_name`]), which no key can name.
//! - **Uploads in parts.** Each part of a large upload is a file of its
//!   own in a folder beside its file's name ([`parts_folder`]), written to a
//!   temporary file and renamed over any part of that number, so a part is
//!   whole or isn't there, and may be sent again. Attaching joins them
//!   ([`DiskStore::complete`]) a window at a time into a temporary file
//!   that is linked into place, then removes them; a store's `delete`, as
//!   the sweep sends for one never attached, takes them with it.
//!
//! Its I/O is blocking: a window is at most a megabyte. A download reads
//! through `read` a window at a time, signed with the key the store keeps
//! in its folder.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_drivers::factory::Disk;
use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability, Kept, Receiving, Refused};
use jadpo_std::files::{
    COMPLETE, DELETE, DISCARD, HEAD_BYTES, JOIN, MAX_PARTS, NOT_FOUND, PARTS_MISSING,
    PARTS_UNEQUAL, PUT, PartsRefused, READ, STAT, STORAGE_FULL, WHOLE_DISK, is_key, key_id,
    part_count, parts_fit, piece_key,
};
use jadpo_value::Value;

/// One store of files kept in one folder, its root: a deployable's at its
/// connection's `path:`, and each of `jadpo dev`'s under `.jadpo/files/`.
#[derive(Debug)]
pub struct DiskStore {
    root: PathBuf,
    /// The free space, in millionths of the disk, below which a write
    /// answers `storage_full`.
    floor: u32,
    /// Whether the root is made, with its folders above, as a write needs
    /// it: `jadpo dev`'s, under the project; a deployable's is made, or
    /// refused, as it starts.
    dev_root: bool,
    /// The key its download links are signed with, once read (area 16,
    /// "Downloading"): kept in the store's own folder, never named by any
    /// file's key.
    link_key: RefCell<Option<[u8; 32]>>,
}

/// The file in a store's root holding what its links are signed with: not
/// a key [`is_key`] allows, so no download or file's key can name it.
const LINK_KEY_FILE: &str = "link-key";

impl DiskStore {
    /// The store a deployable keeps at `path`, with `floor` millionths of
    /// the disk kept free: the folder made, the app's user's alone, when
    /// it is missing, and refused when it isn't a folder written in full
    /// or other users can write to it. Why not, as the deployable lists
    /// it.
    pub fn at(path: &str, floor: u32) -> Result<DiskStore, String> {
        let written = Path::new(path);
        if !written.is_absolute() {
            return Err(format!(
                "its `path:`, `{path}`, isn't a folder written in full"
            ));
        }
        if let Err(error) = private_folders(written) {
            return Err(format!("its folder `{path}` couldn't be made: {error}"));
        }
        let root = std::fs::canonicalize(written)
            .map_err(|error| format!("its folder `{path}` couldn't be read: {error}"))?;
        let meta = std::fs::metadata(&root)
            .map_err(|error| format!("its folder `{path}` couldn't be read: {error}"))?;
        if !meta.is_dir() {
            return Err(format!("its `path:`, `{path}`, isn't a folder"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o022 != 0 {
                return Err(format!(
                    "its folder `{path}` can be written by other users, who could plant files it reads; make it the app's user's alone (`chmod 700`)"
                ));
            }
        }
        Ok(DiskStore {
            root,
            floor: floor.min(WHOLE_DISK),
            dev_root: false,
            link_key: RefCell::new(None),
        })
    }

    /// `jadpo dev`'s store at `root`, made as files are put, with `floor`
    /// millionths of the disk kept free.
    fn made_at(root: PathBuf, floor: u32) -> DiskStore {
        DiskStore {
            root,
            floor: floor.min(WHOLE_DISK),
            dev_root: true,
            link_key: RefCell::new(None),
        }
    }

    /// The root, opened; made first when `make` and it is `jadpo dev`'s.
    fn root(&self, make: bool) -> std::io::Result<os::Dir> {
        if make && self.dev_root {
            private_folders(&self.root)?;
        }
        os::Dir::root(&self.root)
    }

    /// The folder file `key` is in, opened from `root`, made as it goes
    /// when `make`; and the file's name in it.
    fn shard<'k>(root: &os::Dir, key: &'k str, make: bool) -> std::io::Result<(os::Dir, &'k str)> {
        let mut parts = key.split('/');
        let (Some(first), Some(second), Some(name), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ErrorKind::InvalidInput.into());
        };
        let first = root.child(first, make)?;
        Ok((first.child(second, make)?, name))
    }

    /// The folder file `key` is in, opened from the root, made as it goes
    /// when `make`; and the file's name in it.
    fn folder<'k>(&self, key: &'k str, make: bool) -> std::io::Result<(os::Dir, &'k str)> {
        DiskStore::shard(&self.root(make)?, key, make)
    }

    /// Whether writing `incoming` more bytes would leave less of the disk
    /// free than the floor.
    fn below_floor(&self, folder: &os::Dir, incoming: u64) -> bool {
        below_floor(folder, self.floor, incoming)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Answer {
        // The floor is checked before any folder is made for the file.
        let root = match self.root(true) {
            Ok(root) => root,
            Err(error) => return written(&error),
        };
        if self.below_floor(&root, bytes.len() as u64) {
            return Answer::Failed(STORAGE_FULL.into());
        }
        let (folder, name) = match DiskStore::shard(&root, key, true) {
            Ok(found) => found,
            Err(error) => return written(&error),
        };
        let part = part_name(name);
        let kept = (|| {
            let mut file = folder.create(&part)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            folder.rename(&part, name)?;
            folder.sync()
        })();
        match kept {
            Ok(()) => Answer::Value(Value::Unit),
            Err(error) => {
                let _ = folder.remove(&part);
                written(&error)
            }
        }
    }

    fn read(&self, key: &str, from: u64, length: u64) -> Answer {
        let opened = self
            .folder(key, false)
            .and_then(|(folder, name)| folder.open(name));
        let mut file = match opened {
            Ok(file) => file,
            Err(error) if names_nothing(&error) => return Answer::Failed(NOT_FOUND.into()),
            Err(_) => return Answer::Unavailable,
        };
        let mut bytes = Vec::new();
        let read = file
            .seek(SeekFrom::Start(from))
            .and_then(|_| file.take(length).read_to_end(&mut bytes));
        match read {
            Ok(_) => Answer::Value(Value::Bytes(Rc::from(bytes))),
            Err(_) => Answer::Unavailable,
        }
    }

    /// Joins the first `pieces` pieces of file `key`, whose id is `id`,
    /// into it, in order, then removes them.
    fn join(&self, key: &str, id: &[u8; 16], pieces: u64) -> Answer {
        let (folder, name) = match self.folder(key, true) {
            Ok(found) => found,
            Err(error) => return written(&error),
        };
        let mut parts = Vec::new();
        let mut size = 0u64;
        let mut missing = false;
        for piece in 0..pieces {
            let piece_key = piece_key(id, piece);
            let opened = self.folder(&piece_key, false).and_then(|(at, name)| {
                let file = at.open(name)?;
                Ok((at, name.to_string(), file))
            });
            match opened {
                Ok((at, name, file)) => {
                    size = size.saturating_add(file.metadata().map_or(0, |meta| meta.len()));
                    parts.push((at, name, file));
                }
                Err(error) if names_nothing(&error) => missing = true,
                Err(_) => return Answer::Unavailable,
            }
        }
        if missing {
            // Joined already: its pieces went once it was whole.
            if parts.is_empty()
                && let Ok(file) = folder.open(name)
            {
                return match hash_file(file) {
                    Ok(hash) => Answer::Value(Value::text(&hash)),
                    Err(_) => Answer::Unavailable,
                };
            }
            return Answer::Failed(NOT_FOUND.into());
        }
        if self.below_floor(&folder, size) {
            return Answer::Failed(STORAGE_FULL.into());
        }
        let staged = part_name(name);
        let mut hasher = blake3::Hasher::new();
        let kept = (|| {
            let mut file = folder.create(&staged)?;
            for (_, _, part) in &mut parts {
                copy_hashing(part, &mut file, &mut hasher)?;
            }
            file.sync_all()?;
            drop(file);
            folder.rename(&staged, name)?;
            folder.sync()
        })();
        if let Err(error) = kept {
            let _ = folder.remove(&staged);
            return written(&error);
        }
        for (at, name, _) in &parts {
            let _ = at.remove(name);
        }
        Answer::Value(Value::text(&hasher.finalize().to_hex()))
    }

    /// Removes the first `pieces` pieces of file `key`, whose id is `id`,
    /// and the file itself.
    fn discard(&self, key: &str, id: &[u8; 16], pieces: u64) -> Answer {
        let gone =
            std::iter::once(key.to_string()).chain((0..pieces).map(|piece| piece_key(id, piece)));
        for key in gone {
            if let Ok((folder, name)) = self.folder(&key, false) {
                let _ = folder.remove(name);
            }
        }
        Answer::Value(Value::Unit)
    }

    /// Deletes file `key` (chunk B3): the name removed from its folder,
    /// itself if it is a link, never what one names, and the folder
    /// synced, so the deletion survives a crash. A key that names nothing
    /// is deleted already.
    fn delete(&self, key: &str) -> Answer {
        let (folder, name) = match self.folder(key, false) {
            Ok(found) => found,
            Err(error) if names_nothing(&error) => return Answer::Value(Value::Unit),
            Err(_) => return Answer::Unavailable,
        };
        // What an upload said of itself goes with it, and an upload never
        // joined leaves its parts behind.
        let _ = folder.remove(&meta_name(name));
        remove_parts(&folder, name);
        match folder.remove(name) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Answer::Value(Value::Unit);
            }
            Err(_) => return Answer::Unavailable,
        }
        match folder.sync() {
            Ok(()) => Answer::Value(Value::Unit),
            Err(_) => Answer::Unavailable,
        }
    }

    /// Starts keeping an upload as file `key`, of at most `expected`
    /// bytes (area 16, "Uploading"): written to a temporary file of its
    /// own. A file already there refuses it, and so does a disk that would
    /// be left below the floor.
    fn receive(&self, key: &str, expected: u64) -> Result<Box<dyn Receiving + 'static>, Refused> {
        let root = self.root(true).map_err(|error| refused(&error))?;
        if self.below_floor(&root, expected) {
            return Err(Refused::Full);
        }
        let (folder, name) = DiskStore::shard(&root, key, true).map_err(|error| refused(&error))?;
        if folder.open(name).is_ok() {
            return Err(Refused::Taken);
        }
        let part = part_name(name);
        let file = folder.create(&part).map_err(|error| refused(&error))?;
        Ok(Box::new(DiskReceiving {
            root,
            floor: self.floor,
            folder,
            name: name.to_string(),
            part,
            file: Some(file),
            written: 0,
            since_floor: 0,
            whole: None,
            says: true,
        }))
    }

    /// Starts keeping part `number` of upload `key`, of at most `expected`
    /// bytes (area 16, "Large uploads go in parts"): written to a temporary
    /// file of its own and renamed over any part of that number once
    /// finished. A file already joined, or a disk that would be left below
    /// the floor, refuses it.
    fn receive_part(
        &self,
        key: &str,
        number: u64,
        expected: u64,
    ) -> Result<Box<dyn Receiving + 'static>, Refused> {
        let root = self.root(true).map_err(|error| refused(&error))?;
        if self.below_floor(&root, expected) {
            return Err(Refused::Full);
        }
        let (folder, name) = DiskStore::shard(&root, key, true).map_err(|error| refused(&error))?;
        if folder.open(name).is_ok() {
            return Err(Refused::Taken);
        }
        let parts = folder
            .child(&parts_folder(name), true)
            .map_err(|error| refused(&error))?;
        let part_file = number.to_string();
        let part = part_name(&part_file);
        let file = parts.create(&part).map_err(|error| refused(&error))?;
        Ok(Box::new(DiskReceiving {
            root,
            floor: self.floor,
            folder: parts,
            name: part_file,
            part,
            file: Some(file),
            written: 0,
            since_floor: 0,
            whole: Some((folder, name.to_string())),
            says: number == 1,
        }))
    }

    /// Joins the parts of upload `key`, in order, into its file, once every
    /// part is there and they are equal (area 16, "Large uploads go in
    /// parts"): copied a window at a time into a temporary file, synced,
    /// linked into place with what the first part said of it, then the
    /// parts removed. Joined already, it does nothing.
    fn complete(&self, key: &str, part_size: u64, max_size: u64) -> Answer {
        let (folder, name) = match self.folder(key, true) {
            Ok(found) => found,
            Err(error) => return written(&error),
        };
        if folder.open(name).is_ok() {
            return Answer::Value(Value::Unit);
        }
        let parts = match folder.child(&parts_folder(name), false) {
            Ok(parts) => parts,
            Err(error) if names_nothing(&error) => return Answer::Failed(PARTS_MISSING.into()),
            Err(_) => return Answer::Unavailable,
        };
        let mut sizes = Vec::new();
        let mut files = Vec::new();
        let mut ended = false;
        let mut gaps = false;
        for number in 1..=part_count(max_size, part_size).min(MAX_PARTS) {
            match parts.open(&number.to_string()) {
                Ok(_) if ended => {
                    gaps = true;
                    break;
                }
                Ok(file) => {
                    sizes.push(file.metadata().map_or(0, |meta| meta.len()));
                    files.push(file);
                }
                Err(error) if names_nothing(&error) => ended = true,
                Err(_) => return Answer::Unavailable,
            }
        }
        match parts_fit(&sizes, gaps, part_size) {
            Ok(()) => {}
            Err(PartsRefused::Missing) => return Answer::Failed(PARTS_MISSING.into()),
            Err(PartsRefused::Unequal) => return Answer::Failed(PARTS_UNEQUAL.into()),
        }
        if self.below_floor(&folder, sizes.iter().sum()) {
            return Answer::Failed(STORAGE_FULL.into());
        }
        let said = read_said(&parts);
        let staged = part_name(name);
        let mut hasher = blake3::Hasher::new();
        let joined = (|| {
            let mut file = folder.create(&staged)?;
            for part in &mut files {
                copy_hashing(part, &mut file, &mut hasher)?;
            }
            file.sync_all()?;
            drop(file);
            let meta = serde_json::json!({
                "s": sizes.iter().sum::<u64>(),
                "h": hasher.finalize().to_hex().as_str(),
                "n": said.get("n"),
                "c": said.get("c"),
            })
            .to_string();
            write_beside(&folder, &meta_name(name), meta.as_bytes())?;
            match folder.link(&staged, name) {
                // Another attach joined it first: the same file.
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                linked => linked?,
            }
            folder.sync()
        })();
        let _ = folder.remove(&staged);
        match joined {
            Ok(()) => {
                remove_parts(&folder, name);
                Answer::Value(Value::Unit)
            }
            Err(error) => written(&error),
        }
    }

    /// What an upload left under `key`: its size, what it said of itself
    /// and its first bytes. Nothing was uploaded to a key with no record
    /// beside it (area 16, "Uploading").
    fn stat(&self, key: &str) -> Answer {
        let opened = self.folder(key, false).and_then(|(folder, name)| {
            let file = folder.open(name)?;
            let meta = folder.open(&meta_name(name))?;
            Ok((file, meta))
        });
        let (mut file, mut meta) = match opened {
            Ok(found) => found,
            Err(error) if names_nothing(&error) => return Answer::Failed(NOT_FOUND.into()),
            Err(_) => return Answer::Unavailable,
        };
        let mut said = Vec::new();
        let mut head = Vec::with_capacity(HEAD_BYTES);
        let read = meta.read_to_end(&mut said).and_then(|_| {
            std::io::Read::by_ref(&mut file)
                .take(HEAD_BYTES as u64)
                .read_to_end(&mut head)
        });
        let (Ok(_), Ok(size)) = (read, file.metadata().map(|meta| meta.len())) else {
            return Answer::Unavailable;
        };
        let Ok(serde_json::Value::Object(said)) = serde_json::from_slice(&said) else {
            return Answer::Unavailable;
        };
        let text = |name: &str| said.get(name).and_then(serde_json::Value::as_str);
        let Some(hash) = text("h") else {
            return Answer::Unavailable;
        };
        Answer::Value(jadpo_runtime::files::uploaded(
            &head,
            size,
            Some(hash),
            text("n"),
            text("c"),
        ))
    }

    /// Every key of the erasure log kept here (area 15, "What erasure
    /// doesn't rewrite"; chunk P4c), in order: a real folder for each
    /// entity's and a regular file for each person, anything else, a
    /// `put`'s temporary file or a link among them, named by no key. Nothing
    /// kept yet is an empty list.
    fn list(&self) -> Answer {
        let top = self.root.join(jadpo_std::erasure::LOG_FOLDER);
        let entities = match std::fs::symlink_metadata(&top) {
            Ok(meta) if meta.is_dir() => match std::fs::read_dir(&top) {
                Ok(entries) => entries,
                Err(_) => return Answer::Unavailable,
            },
            Ok(_) => return Answer::Value(Value::list(Vec::new())),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Answer::Value(Value::list(Vec::new()));
            }
            Err(_) => return Answer::Unavailable,
        };
        let mut keys: Vec<String> = Vec::new();
        for entity in entities {
            let Ok(entity) = entity else {
                return Answer::Unavailable;
            };
            if !entity.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let Ok(people) = std::fs::read_dir(entity.path()) else {
                return Answer::Unavailable;
            };
            for person in people {
                let Ok(person) = person else {
                    return Answer::Unavailable;
                };
                if !person.file_type().is_ok_and(|kind| kind.is_file()) {
                    continue;
                }
                let key = jadpo_std::erasure::log_key(
                    &entity.file_name().to_string_lossy(),
                    &person.file_name().to_string_lossy(),
                );
                if jadpo_std::erasure::log_key_parts(&key).is_some() {
                    keys.push(key);
                }
            }
        }
        keys.sort();
        Answer::Value(Value::list(
            keys.iter().map(|key| Value::text(key)).collect(),
        ))
    }

    /// What its download links are signed with: read from its folder, and
    /// made there first when this is the first link of its store, by the
    /// one thread or process that gets the name; the others read what it
    /// made, and made only when `make`. `None` when it can't be read or
    /// made.
    fn link_key(&self, make: bool) -> Option<[u8; 32]> {
        if let Some(key) = *self.link_key.borrow() {
            return Some(key);
        }
        let root = self.root(make).ok()?;
        let key = match read_link_key(&root) {
            Ok(key) => key,
            Err(error) if make && error.kind() == ErrorKind::NotFound => {
                make_link_key(&root).ok()?
            }
            Err(_) => return None,
        };
        *self.link_key.borrow_mut() = Some(key);
        Some(key)
    }

    fn answer(&self, call: &Call) -> Answer {
        let Value::Record(input) = &call.input else {
            return Answer::Misconfigured;
        };
        if call.operation == jadpo_std::files::LIST {
            return self.list();
        }
        let Some(Value::Text(key)) = input.get("key") else {
            return Answer::Misconfigured;
        };
        let logged = jadpo_std::erasure::log_key_parts(key).is_some()
            && matches!(call.operation.as_str(), PUT | READ);
        if !is_key(key) && !logged {
            return Answer::Failed(NOT_FOUND.into());
        }
        match call.operation.as_str() {
            PUT => match input.get("bytes") {
                Some(Value::Bytes(bytes)) => self.put(key, bytes),
                _ => Answer::Misconfigured,
            },
            READ => match (input.get("from"), input.get("length")) {
                (Some(Value::Int(from)), Some(Value::Int(length))) => self.read(
                    key,
                    u64::try_from(*from).unwrap_or(0),
                    u64::try_from(*length).unwrap_or(0),
                ),
                _ => Answer::Misconfigured,
            },
            DELETE => self.delete(key),
            STAT => self.stat(key),
            COMPLETE => match (input.get("part_size"), input.get("max_size")) {
                (Some(Value::Int(part_size)), Some(Value::Int(max_size))) => self.complete(
                    key,
                    u64::try_from(*part_size).unwrap_or(0),
                    u64::try_from(*max_size).unwrap_or(0),
                ),
                _ => Answer::Misconfigured,
            },
            JOIN | DISCARD => match (input.get("pieces"), key_id(key)) {
                (Some(Value::Int(pieces)), Some(id)) => {
                    let pieces = u64::try_from(*pieces).unwrap_or(0);
                    if call.operation == JOIN {
                        self.join(key, &id, pieces)
                    } else {
                        self.discard(key, &id, pieces)
                    }
                }
                _ => Answer::Misconfigured,
            },
            _ => Answer::Misconfigured,
        }
    }
}

/// An upload the disk is keeping, in its temporary file until
/// [`Receiving::finish`]; dropped unfinished, the file goes with it.
struct DiskReceiving {
    root: os::Dir,
    floor: u32,
    folder: os::Dir,
    name: String,
    part: String,
    file: Option<std::fs::File>,
    written: u64,
    since_floor: u64,
    /// On a part: the folder its file is in, and that file's name, to be
    /// refused once the file has been joined. A part replaces one of its
    /// number; a whole upload never replaces anything.
    whole: Option<(os::Dir, String)>,
    /// Whether it keeps what it said of itself beside it: a whole upload
    /// and a first part.
    says: bool,
}

/// How many bytes an upload writes between looks at the disk's free space.
const FLOOR_EVERY: u64 = 1 << 20;

impl Receiving for DiskReceiving {
    fn write(&mut self, bytes: &[u8]) -> Result<(), Refused> {
        let Some(file) = self.file.as_mut() else {
            return Err(Refused::Unavailable);
        };
        file.write_all(bytes).map_err(|error| refused(&error))?;
        self.written = self.written.saturating_add(bytes.len() as u64);
        self.since_floor = self.since_floor.saturating_add(bytes.len() as u64);
        if self.since_floor >= FLOOR_EVERY {
            self.since_floor = 0;
            if below_floor(&self.root, self.floor, 0) {
                return Err(Refused::Full);
            }
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>, kept: &Kept<'_>) -> Result<(), Refused> {
        let Some(file) = self.file.take() else {
            return Err(Refused::Unavailable);
        };
        if let Some((folder, name)) = &self.whole
            && folder.open(name).is_ok()
        {
            return Err(Refused::Taken);
        }
        let said = serde_json::json!({
            "s": kept.size,
            "h": kept.hash,
            "n": kept.name,
            "c": kept.claimed,
        })
        .to_string();
        let meta = meta_name(&self.name);
        let outcome = (|| {
            file.sync_all()?;
            drop(file);
            // What it said of itself is there before it is.
            if self.says {
                write_beside(&self.folder, &meta, said.as_bytes())?;
            }
            if self.whole.is_some() {
                self.folder.rename(&self.part, &self.name)?;
            } else {
                self.folder.link(&self.part, &self.name)?;
            }
            self.folder.sync()
        })();
        match outcome {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => Err(Refused::Taken),
            Err(error) => {
                let _ = self.folder.remove(&meta);
                Err(refused(&error))
            }
        }
    }
}

impl Drop for DiskReceiving {
    fn drop(&mut self) {
        let _ = self.folder.remove(&self.part);
    }
}

/// Writes `bytes` to `folder`'s file `name`, whole: a temporary file of its
/// own, synced and renamed over it.
fn write_beside(folder: &os::Dir, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let staged = part_name(name);
    let written = (|| {
        let mut beside = folder.create(&staged)?;
        beside.write_all(bytes)?;
        beside.sync_all()?;
        drop(beside);
        folder.rename(&staged, name)
    })();
    let _ = folder.remove(&staged);
    written
}

/// The folder beside file `name` holding its upload's parts until they are
/// joined: a name no key can spell, as [`is_key`] allows none with a dot.
fn parts_folder(name: &str) -> String {
    format!("{name}.parts")
}

/// What the first part of an upload said of itself, from the folder of its
/// parts; nothing when it said nothing.
fn read_said(parts: &os::Dir) -> serde_json::Map<String, serde_json::Value> {
    let mut text = Vec::new();
    let read = parts
        .open(&meta_name("1"))
        .and_then(|mut file| file.read_to_end(&mut text));
    if read.is_err() {
        return serde_json::Map::new();
    }
    match serde_json::from_slice(&text) {
        Ok(serde_json::Value::Object(said)) => said,
        _ => serde_json::Map::new(),
    }
}

/// Removes the parts upload `name` left in `folder`, and their folder; an
/// upload never in parts has none to remove.
fn remove_parts(folder: &os::Dir, name: &str) {
    let parts_name = parts_folder(name);
    let Ok(parts) = folder.child(&parts_name, false) else {
        return;
    };
    for number in 1..=MAX_PARTS {
        let _ = parts.remove(&number.to_string());
    }
    let _ = parts.remove(&meta_name("1"));
    drop(parts);
    let _ = folder.remove_child(&parts_name);
    let _ = folder.sync();
}

/// Whether writing `incoming` more bytes to the disk `folder` is on would
/// leave less of it free than `floor` millionths.
fn below_floor(folder: &os::Dir, floor: u32, incoming: u64) -> bool {
    if floor == 0 {
        return false;
    }
    let Ok((free, total)) = folder.space() else {
        return false;
    };
    let left = free.saturating_sub(u128::from(incoming));
    left * u128::from(WHOLE_DISK) < total * u128::from(floor)
}

/// Why an upload wasn't kept, from a failed write.
fn refused(error: &std::io::Error) -> Refused {
    match error.kind() {
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Refused::Full,
        ErrorKind::AlreadyExists => Refused::Taken,
        _ => Refused::Unavailable,
    }
}

impl Capability for DiskStore {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let answer = self.answer(call);
        Box::pin(async move { answer })
    }

    fn receive(
        &self,
        _connection: &str,
        key: &str,
        expected: u64,
    ) -> Result<Box<dyn Receiving + '_>, Refused> {
        self.receive(key, expected)
    }

    fn receive_part(
        &self,
        _connection: &str,
        key: &str,
        part: u64,
        expected: u64,
    ) -> Result<Box<dyn Receiving + '_>, Refused> {
        self.receive_part(key, part, expected)
    }

    fn link_key(&self, _connection: &str, make: bool) -> Option<[u8; 32]> {
        self.link_key(make)
    }
}

/// The server's own disk, which a native deployable gives the driver
/// factory ([`crate::deployable_supplies`]): each `Driver.disk` store at
/// its connection's `path:` ([`DiskStore::at`]).
#[derive(Debug, Default)]
pub struct ServerDisk;

impl Disk for ServerDisk {
    fn store(
        &self,
        _connection: &str,
        path: Option<&str>,
        floor: u32,
    ) -> Result<Rc<dyn Capability>, String> {
        let path = path.ok_or_else(|| {
            "it names no `path:`, the folder on this server its files are kept in".to_string()
        })?;
        DiskStore::at(path, floor).map(|store| Rc::new(store) as Rc<dyn Capability>)
    }
}

/// Every store of files under one folder, each connection in a folder of
/// its own: what `jadpo dev` gives a program.
pub struct DiskFiles {
    root: PathBuf,
    /// Each store's free-space floor, by connection: none until
    /// [`DiskFiles::with_floors`] says.
    floors: Rc<dyn Fn(&str) -> u32>,
    /// Each store's link key, once read.
    keys: RefCell<HashMap<String, [u8; 32]>>,
}

impl std::fmt::Debug for DiskFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiskFiles")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl DiskFiles {
    /// The stores kept under `root`, made as files are put.
    pub fn under(root: impl Into<PathBuf>) -> DiskFiles {
        DiskFiles {
            root: root.into(),
            floors: Rc::new(|_| 0),
            keys: RefCell::new(HashMap::new()),
        }
    }

    /// Each store keeping free the share of the disk `floors` gives for
    /// its connection, in millionths, as a deployable's does.
    pub fn with_floors(mut self, floors: impl Fn(&str) -> u32 + 'static) -> DiskFiles {
        self.floors = Rc::new(floors);
        self
    }

    /// Connection `connection`'s store, when it is a name the driver keeps
    /// files under.
    fn store(&self, connection: &str) -> Option<DiskStore> {
        let named = !connection.is_empty()
            && connection
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        named.then(|| DiskStore::made_at(self.root.join(connection), (self.floors)(connection)))
    }
}

impl Capability for DiskFiles {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let answer = match self.store(&call.connection) {
            Some(store) => store.answer(call),
            None => Answer::Failed(NOT_FOUND.into()),
        };
        Box::pin(async move { answer })
    }

    fn receive(
        &self,
        connection: &str,
        key: &str,
        expected: u64,
    ) -> Result<Box<dyn Receiving + '_>, Refused> {
        match self.store(connection) {
            Some(store) => store.receive(key, expected),
            None => Err(Refused::Unavailable),
        }
    }

    fn receive_part(
        &self,
        connection: &str,
        key: &str,
        part: u64,
        expected: u64,
    ) -> Result<Box<dyn Receiving + '_>, Refused> {
        match self.store(connection) {
            Some(store) => store.receive_part(key, part, expected),
            None => Err(Refused::Unavailable),
        }
    }

    fn link_key(&self, connection: &str, make: bool) -> Option<[u8; 32]> {
        if let Some(key) = self.keys.borrow().get(connection) {
            return Some(*key);
        }
        let key = self.store(connection)?.link_key(make)?;
        self.keys.borrow_mut().insert(connection.to_string(), key);
        Some(key)
    }
}

/// The link key kept in `root`: a regular file of exactly 32 bytes.
fn read_link_key(root: &os::Dir) -> std::io::Result<[u8; 32]> {
    let mut file = root.open(LINK_KEY_FILE)?;
    let mut key = [0u8; 32];
    file.read_exact(&mut key)?;
    let mut more = [0u8; 1];
    if file.read(&mut more)? != 0 {
        return Err(ErrorKind::InvalidData.into());
    }
    Ok(key)
}

/// A new link key in `root`, written to a temporary file and linked into
/// place, which refuses a name already there, so two workers making one at
/// once agree on the first; and so a crash never leaves part of one.
fn make_link_key(root: &os::Dir) -> std::io::Result<[u8; 32]> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|_| std::io::Error::from(ErrorKind::Other))?;
    let staged = part_name(LINK_KEY_FILE);
    let mut file = root.create(&staged)?;
    let written = file
        .write_all(&key)
        .and_then(|()| file.sync_all())
        .and_then(|()| root.link(&staged, LINK_KEY_FILE));
    drop(file);
    let _ = root.remove(&staged);
    match written {
        Ok(()) => {
            root.sync()?;
            Ok(key)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => read_link_key(root),
        Err(error) => Err(error),
    }
}

/// `from` copied to `to` a window at a time, its bytes hashed as they go.
fn copy_hashing(
    from: &mut std::fs::File,
    to: &mut std::fs::File,
    hasher: &mut blake3::Hasher,
) -> std::io::Result<()> {
    let mut window = vec![0u8; 1 << 16];
    loop {
        let read = from.read(&mut window)?;
        let Some(bytes) = window.get(..read).filter(|bytes| !bytes.is_empty()) else {
            return Ok(());
        };
        hasher.update(bytes);
        to.write_all(bytes)?;
    }
}

/// The hash of what `file` holds, read a window at a time.
fn hash_file(mut file: std::fs::File) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut window = vec![0u8; 1 << 16];
    loop {
        let read = file.read(&mut window)?;
        let Some(bytes) = window.get(..read).filter(|bytes| !bytes.is_empty()) else {
            return Ok(hasher.finalize().to_hex().to_string());
        };
        hasher.update(bytes);
    }
}

/// A temporary file's name beside `name`, its own: created new, so no
/// other write, and nothing planted there, is written through.
fn part_name(name: &str) -> String {
    let mut random = [0u8; 8];
    let _ = getrandom::fill(&mut random);
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(".{name}.{suffix}.part")
}

/// The name beside file `name` keeping what its upload said of itself: a
/// hidden name no key can spell.
fn meta_name(name: &str) -> String {
    format!(".{name}.upload")
}

/// Whether `error` says the key names nothing: no such file, or a link,
/// or something that isn't a folder, on the way.
fn names_nothing(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::NotFound
        || error.kind() == ErrorKind::InvalidInput
        || os::is_link(error)
}

/// A failed write's answer: `storage_full` when the disk has no room,
/// `not_found` when a link is on the way, otherwise nothing kept, safe to
/// send again.
fn written(error: &std::io::Error) -> Answer {
    match error.kind() {
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Answer::Failed(STORAGE_FULL.into()),
        _ if os::is_link(error) || error.kind() == ErrorKind::InvalidInput => {
            Answer::Failed(NOT_FOUND.into())
        }
        _ => Answer::Unavailable,
    }
}

/// `folder` and the folders above it made, those it makes the app's
/// user's alone.
#[cfg(unix)]
fn private_folders(folder: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
}

#[cfg(not(unix))]
fn private_folders(folder: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)
}

/// Folders and files opened relative to the folder above, never through a
/// link (Unix: `openat` with `O_NOFOLLOW`).
#[cfg(unix)]
mod os {
    use std::fs::File;
    use std::os::fd::OwnedFd;
    use std::path::Path;

    use rustix::fs::{AtFlags, CWD, Mode, OFlags};
    use rustix::io::Errno;

    const FOLDER: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    /// An open folder.
    #[derive(Debug)]
    pub struct Dir(OwnedFd);

    impl Dir {
        /// The store's root, which must be a folder, not a link to one.
        pub fn root(path: &Path) -> std::io::Result<Dir> {
            Ok(Dir(rustix::fs::openat(CWD, path, FOLDER, Mode::empty())?))
        }

        /// The folder `name` in this one, made the app's user's alone when
        /// it is missing and `make`.
        pub fn child(&self, name: &str, make: bool) -> std::io::Result<Dir> {
            match rustix::fs::openat(&self.0, name, FOLDER, Mode::empty()) {
                Err(Errno::NOENT) if make => {
                    match rustix::fs::mkdirat(&self.0, name, Mode::from_raw_mode(0o700)) {
                        Ok(()) | Err(Errno::EXIST) => {}
                        Err(error) => return Err(error.into()),
                    }
                    Ok(Dir(rustix::fs::openat(
                        &self.0,
                        name,
                        FOLDER,
                        Mode::empty(),
                    )?))
                }
                opened => Ok(Dir(opened?)),
            }
        }

        /// A new file `name`, the app's user's alone; one already there,
        /// or a link, is refused.
        pub fn create(&self, name: &str) -> std::io::Result<File> {
            let flags =
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            let fd = rustix::fs::openat(&self.0, name, flags, Mode::from_raw_mode(0o600))?;
            Ok(File::from(fd))
        }

        /// File `name` to read: a regular file, never a link, and never a
        /// pipe that would wait for a writer.
        pub fn open(&self, name: &str) -> std::io::Result<File> {
            let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
            let file = File::from(rustix::fs::openat(&self.0, name, flags, Mode::empty())?);
            if !file.metadata()?.is_file() {
                return Err(std::io::ErrorKind::NotFound.into());
            }
            Ok(file)
        }

        pub fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
            Ok(rustix::fs::renameat(&self.0, from, &self.0, to)?)
        }

        /// A second name, `to`, for file `from`; a name already there is
        /// refused.
        pub fn link(&self, from: &str, to: &str) -> std::io::Result<()> {
            Ok(rustix::fs::linkat(
                &self.0,
                from,
                &self.0,
                to,
                AtFlags::empty(),
            )?)
        }

        /// Removes file `name`: a link is removed itself, never what it
        /// names.
        pub fn remove(&self, name: &str) -> std::io::Result<()> {
            Ok(rustix::fs::unlinkat(&self.0, name, AtFlags::empty())?)
        }

        /// Removes the empty folder `name`.
        pub fn remove_child(&self, name: &str) -> std::io::Result<()> {
            Ok(rustix::fs::unlinkat(&self.0, name, AtFlags::REMOVEDIR)?)
        }

        pub fn sync(&self) -> std::io::Result<()> {
            Ok(rustix::fs::fsync(&self.0)?)
        }

        /// The bytes free to the app's user on this folder's disk, and the
        /// disk's size.
        pub fn space(&self) -> std::io::Result<(u128, u128)> {
            let space = rustix::fs::fstatvfs(&self.0)?;
            let unit = u128::from(space.f_frsize);
            Ok((
                u128::from(space.f_bavail) * unit,
                u128::from(space.f_blocks) * unit,
            ))
        }
    }

    /// Whether `error` is a link refused on the way: `O_NOFOLLOW` on one,
    /// or a link where a folder was asked for.
    pub fn is_link(error: &std::io::Error) -> bool {
        let raw = error.raw_os_error();
        raw == Some(Errno::LOOP.raw_os_error()) || raw == Some(Errno::NOTDIR.raw_os_error())
    }
}

/// The same by path, refusing a link that is there as each part is opened.
#[cfg(not(unix))]
mod os {
    use std::fs::{File, OpenOptions};
    use std::path::{Path, PathBuf};

    #[derive(Debug)]
    pub struct Dir(PathBuf);

    fn unlinked(path: &Path) -> std::io::Result<()> {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                Err(std::io::Error::other("a link on the way"))
            }
            _ => Ok(()),
        }
    }

    impl Dir {
        pub fn root(path: &Path) -> std::io::Result<Dir> {
            unlinked(path)?;
            if !std::fs::metadata(path)?.is_dir() {
                return Err(std::io::ErrorKind::NotFound.into());
            }
            Ok(Dir(path.to_path_buf()))
        }

        pub fn child(&self, name: &str, make: bool) -> std::io::Result<Dir> {
            let path = self.0.join(name);
            unlinked(&path)?;
            if make && !path.exists() {
                std::fs::create_dir(&path)?;
            }
            Dir::root(&path)
        }

        pub fn create(&self, name: &str) -> std::io::Result<File> {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.0.join(name))
        }

        pub fn open(&self, name: &str) -> std::io::Result<File> {
            let path = self.0.join(name);
            unlinked(&path)?;
            let file = File::open(path)?;
            if !file.metadata()?.is_file() {
                return Err(std::io::ErrorKind::NotFound.into());
            }
            Ok(file)
        }

        pub fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
            std::fs::rename(self.0.join(from), self.0.join(to))
        }

        /// A second name, `to`, for file `from`; a name already there is
        /// refused.
        pub fn link(&self, from: &str, to: &str) -> std::io::Result<()> {
            std::fs::hard_link(self.0.join(from), self.0.join(to))
        }

        pub fn remove(&self, name: &str) -> std::io::Result<()> {
            std::fs::remove_file(self.0.join(name))
        }

        pub fn remove_child(&self, name: &str) -> std::io::Result<()> {
            std::fs::remove_dir(self.0.join(name))
        }

        pub fn sync(&self) -> std::io::Result<()> {
            Ok(())
        }

        pub fn space(&self) -> std::io::Result<(u128, u128)> {
            Err(std::io::ErrorKind::Unsupported.into())
        }
    }

    pub fn is_link(error: &std::io::Error) -> bool {
        error.kind() == std::io::ErrorKind::Other
    }
}

#[cfg(test)]
mod tests {
    /// Parts land whole and may be sent again: the last to finish stands,
    /// joining puts them in order under the key with the first part's name
    /// and removes them, a joined file refuses more, and a delete takes
    /// the parts of an upload never joined.
    #[test]
    fn parts_land_join_once_and_go_with_a_delete() {
        const PART: u64 = 4;
        const MAX: u64 = 64;
        let root = root("parts");
        let store = DiskStore::made_at(root.clone(), 0);
        let key = jadpo_std::files::key_for(&[3; 16]);
        let send = |number: u64, bytes: &[u8], name: Option<&str>| {
            let mut sink = store.receive_part(&key, number, 16).expect("it takes it");
            sink.write(bytes).expect("written");
            sink.finish(&Kept {
                size: bytes.len() as u64,
                hash: "h",
                name,
                claimed: Some("csv"),
            })
        };
        let complete = |store: &DiskStore| store.complete(&key, PART, MAX);

        assert_eq!(complete(&store), Answer::Failed(PARTS_MISSING.into()));
        // Stopped part-way: nothing, and the number is free to send.
        {
            let mut cut = store.receive_part(&key, 1, 16).expect("it takes it");
            cut.write(b"zz").expect("written");
        }
        send(1, b"oldd", Some("a.csv")).expect("a part");
        send(1, b"abcd", Some("a.csv")).expect("sent again");
        send(3, b"ij", None).expect("a part");
        assert_eq!(complete(&store), Answer::Failed(PARTS_MISSING.into()));
        send(2, b"ef", None).expect("a short middle part");
        assert_eq!(complete(&store), Answer::Failed(PARTS_UNEQUAL.into()));
        send(2, b"efgh", None).expect("the middle part again");
        assert_eq!(complete(&store), Answer::Value(Value::Unit));
        // Joined twice is joined once.
        assert_eq!(complete(&store), Answer::Value(Value::Unit));

        assert_eq!(
            std::fs::read(root.join(&key)).expect("joined"),
            b"abcdefghij"
        );
        let Answer::Value(Value::Record(about)) = store.stat(&key) else {
            panic!("a stat");
        };
        assert_eq!(about.get("name"), Some(&Value::text("a.csv")));
        assert_eq!(about.get("claimed"), Some(&Value::text("csv")));
        assert_eq!(about.get("size"), Some(&Value::Int(10)));
        assert_eq!(
            about.get("hash"),
            Some(&Value::text(blake3::hash(b"abcdefghij").to_hex().as_str()))
        );
        assert!(walk(&root).iter().all(|path| {
            !path.to_string_lossy().contains(".parts") && !path.to_string_lossy().ends_with(".part")
        }));
        assert_eq!(store.receive_part(&key, 1, 16).err(), Some(Refused::Taken));

        let unjoined = jadpo_std::files::key_for(&[4; 16]);
        let mut sink = store.receive_part(&unjoined, 2, 16).expect("it takes it");
        sink.write(b"ab").expect("written");
        sink.finish(&Kept {
            size: 2,
            hash: "h",
            name: None,
            claimed: None,
        })
        .expect("a part");
        assert_eq!(store.delete(&unjoined), Answer::Value(Value::Unit));
        assert!(
            walk(&root)
                .iter()
                .all(|path| !path.to_string_lossy().contains(".parts"))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// An upload appears only when finished, whole, once: what a client
    /// left part-way is gone, a file already there refuses another, and
    /// what it said of itself goes with it when it is deleted.
    #[test]
    fn an_upload_lands_whole_once_finished_and_only_once() {
        let dir = std::env::temp_dir().join(format!("jadpo-disk-upload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = DiskStore::made_at(dir.clone(), 0);
        let key = jadpo_std::files::key_for(&[5; 16]);
        let kept = Kept {
            size: 11,
            hash: "abc",
            name: Some("prices.csv"),
            claimed: Some("csv"),
        };
        let stray = || {
            walk(&dir)
                .into_iter()
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".part"))
                })
                .collect::<Vec<_>>()
        };
        // A client that stops part-way leaves nothing.
        {
            let mut sink = store.receive(&key, 11).expect("starts");
            sink.write(b"flour,").expect("written");
            assert!(!dir.join(&key).exists());
            assert_eq!(stray().len(), 1);
        }
        assert!(stray().is_empty());
        assert!(!dir.join(&key).exists());

        let mut sink = store.receive(&key, 11).expect("starts");
        sink.write(b"flour,").expect("written");
        sink.write(b"1.20\n").expect("written");
        sink.finish(&kept).expect("kept");
        assert_eq!(
            std::fs::read(dir.join(&key)).expect("kept"),
            b"flour,1.20\n"
        );
        assert!(stray().is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(dir.join(&key)).expect("kept");
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
        // What a store is asked about it.
        let Answer::Value(Value::Record(about)) = store.stat(&key) else {
            panic!("an answer");
        };
        assert_eq!(about.get("size"), Some(&Value::Int(11)));
        assert_eq!(about.get("hash"), Some(&Value::text("abc")));
        assert_eq!(about.get("name"), Some(&Value::text("prices.csv")));
        assert_eq!(about.get("claimed"), Some(&Value::text("csv")));
        assert!(
            matches!(about.get("head"), Some(Value::Bytes(head)) if &**head == b"flour,1.20\n"),
            "{about:?}"
        );

        // A slot keeps one file, and the file never changes.
        assert_eq!(store.receive(&key, 1).err(), Some(Refused::Taken));
        let other = jadpo_std::files::key_for(&[6; 16]);
        let mut late = store.receive(&other, 11).expect("starts");
        late.write(b"rye").expect("written");
        store.put(&other, b"made meanwhile");
        assert_eq!(late.finish(&kept).err(), Some(Refused::Taken));
        assert_eq!(
            std::fs::read(dir.join(&other)).expect("kept"),
            b"made meanwhile",
            "the file there stays"
        );

        assert!(matches!(store.delete(&key), Answer::Value(_)));
        assert!(!dir.join(&key).exists());
        let name = key.rsplit('/').next().expect("a name");
        let beside = dir
            .join(key.strip_suffix(name).expect("its folder"))
            .join(meta_name(name));
        assert!(!beside.exists(), "what it said of itself goes with it");
        assert!(matches!(store.stat(&key), Answer::Failed(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An upload that wouldn't leave the store's floor free is refused
    /// before it is read, as a put is.
    #[test]
    fn an_upload_below_the_floor_is_refused_before_a_byte_lands() {
        let dir =
            std::env::temp_dir().join(format!("jadpo-disk-upload-floor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = DiskStore::made_at(dir.clone(), WHOLE_DISK - 1);
        let key = jadpo_std::files::key_for(&[7; 16]);
        assert_eq!(store.receive(&key, 1).err(), Some(Refused::Full));
        assert!(!dir.join(&key).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn walk(at: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut folders = vec![at.to_path_buf()];
        while let Some(folder) = folders.pop() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    folders.push(path);
                } else {
                    out.push(path);
                }
            }
        }
        out
    }

    /// A store's link key is made once, kept, and can be neither read
    /// through a planted link nor named by a file's key.
    #[cfg(unix)]
    #[test]
    fn a_stores_link_key_is_made_once_and_is_no_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = std::env::temp_dir().join(format!("jadpo-link-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("store");
        let store = DiskStore::made_at(root.clone(), 0);
        assert_eq!(store.link_key(false), None, "following a link makes none");
        assert!(!root.exists());
        let key = store.link_key(true).expect("made");
        assert_eq!(store.link_key(true), Some(key), "kept");
        let again = DiskStore::made_at(root.clone(), 0);
        assert_eq!(
            again.link_key(false),
            Some(key),
            "read back, not made again"
        );
        let meta = std::fs::metadata(root.join(LINK_KEY_FILE)).expect("kept");
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert!(!is_key(LINK_KEY_FILE));

        // Racing: both get the same key, whichever got the name.
        let raced = dir.join("raced");
        let opened = DiskStore::made_at(raced.clone(), 0);
        let root_dir = opened.root(true).expect("made");
        let one = make_link_key(&root_dir).expect("first");
        let two = make_link_key(&root_dir).expect("second finds the first");
        assert_eq!(one, two);

        // A link planted in its place is refused, whatever it names.
        let planted = dir.join("planted");
        std::fs::create_dir_all(&planted).expect("made");
        symlink("/etc/hostname", planted.join(LINK_KEY_FILE)).expect("planted");
        let refused = DiskStore::made_at(planted.clone(), 0);
        assert_eq!(refused.link_key(true), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;
    use jadpo_runtime::capability::Repeat;
    use std::fs;

    fn root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("jadpo-disk-files-b2-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn put(key: &str, bytes: &[u8]) -> Call {
        Call::new(
            "imports",
            PUT,
            Value::record(
                "FilePut",
                vec![
                    (Rc::from("key"), Value::text(key)),
                    (Rc::from("type"), Value::text("text/csv")),
                    (Rc::from("bytes"), Value::Bytes(Rc::from(bytes))),
                ],
            ),
            Repeat::Keyed,
        )
    }

    /// The erasure log's files (area 15; chunk P4c): `put` and `read` take
    /// a key of the log's, `list` finds each in order, and nothing else in
    /// the store is listed, no temporary file and no link; a store with none
    /// lists nothing; and no other operation takes such a key, nor `put` one
    /// that isn't a person's.
    #[test]
    fn the_erasure_log_is_kept_listed_and_read_as_files() {
        let store = DiskStore::made_at(root("log"), 0);
        let list = || {
            let call = Call::new(
                "imports",
                jadpo_std::files::LIST,
                Value::record("FileList", Vec::new()),
                Repeat::Safe,
            );
            match store.answer(&call) {
                Answer::Value(Value::List(keys)) => keys
                    .iter()
                    .map(|key| match key {
                        Value::Text(text) => text.to_string(),
                        other => panic!("not a key: {other:?}"),
                    })
                    .collect::<Vec<_>>(),
                other => panic!("not a list: {other:?}"),
            }
        };
        assert_eq!(list(), Vec::<String>::new(), "nothing kept yet");
        let first = jadpo_std::erasure::log_key("User", "0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b");
        let second = jadpo_std::erasure::log_key("Patient", "0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5c");
        for key in [&first, &second] {
            assert!(matches!(store.answer(&put(key, b"{}")), Answer::Value(_)));
        }
        // A file of the store's own stays out of it.
        let file = jadpo_std::files::key_for(&[7; 16]);
        assert!(matches!(
            store.answer(&put(&file, b"csv")),
            Answer::Value(_)
        ));
        assert_eq!(list(), [second.clone(), first.clone()]);
        assert!(matches!(
            store.answer(&read(&first, 0, 10)),
            Answer::Value(Value::Bytes(bytes)) if &*bytes == b"{}"
        ));
        // What isn't a person's isn't listed, a link among them.
        let folder = store.root.join(jadpo_std::erasure::LOG_FOLDER).join("User");
        fs::write(folder.join("0190a2b3.part"), b"half").expect("written");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            store.root.join("elsewhere"),
            folder.join("0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5d"),
        )
        .expect("linked");
        assert_eq!(list(), [second, first.clone()]);
        // No key but a person's names a place in the log.
        for wrong in [
            "erasure-log/User/../../../etc/passwd",
            "erasure-log/../secret",
        ] {
            assert!(
                matches!(store.answer(&put(wrong, b"x")), Answer::Failed(code) if code == NOT_FOUND),
                "{wrong}"
            );
        }
        // Only `put` and `read` take a key of the log's: a log's entries stay.
        let delete = Call::new(
            "imports",
            DELETE,
            Value::record("FileDelete", vec![(Rc::from("key"), Value::text(&first))]),
            Repeat::Keyed,
        );
        assert!(matches!(store.answer(&delete), Answer::Failed(code) if code == NOT_FOUND));
        assert!(matches!(
            store.answer(&read(&first, 0, 10)),
            Answer::Value(_)
        ));
    }

    fn read(key: &str, from: i64, length: i64) -> Call {
        Call::new(
            "imports",
            READ,
            Value::record(
                "FileRead",
                vec![
                    (Rc::from("key"), Value::text(key)),
                    (Rc::from("from"), Value::Int(from)),
                    (Rc::from("length"), Value::Int(length)),
                ],
            ),
            Repeat::Safe,
        )
    }

    fn joined(key: &str, pieces: i64) -> Call {
        Call::new(
            "imports",
            JOIN,
            Value::record(
                "FileJoin",
                vec![
                    (Rc::from("key"), Value::text(key)),
                    (Rc::from("pieces"), Value::Int(pieces)),
                    (Rc::from("type"), Value::text("text/csv")),
                ],
            ),
            Repeat::Keyed,
        )
    }

    fn answered(files: &dyn Capability, call: &Call) -> Answer {
        jadpo_runtime::block_on(files.call(call)).expect("no wait")
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::symlink_metadata(path)
            .expect("there")
            .permissions()
            .mode()
            & 0o777
    }

    /// A put lands whole under its key, readable by its owner only, and a
    /// ranged read gives back the bytes asked for.
    #[test]
    fn a_put_lands_under_its_key_and_reads_back_in_ranges() {
        let root = root("put");
        let files = DiskFiles::under(&root);
        let key = jadpo_std::files::key_for(&[0x3f; 16]);
        assert_eq!(
            answered(&files, &put(&key, b"code,name\nA1,Apples\n")),
            Answer::Value(Value::Unit)
        );
        let path = root.join("imports").join(&key);
        assert_eq!(fs::read(&path).expect("kept"), b"code,name\nA1,Apples\n");
        #[cfg(unix)]
        {
            assert_eq!(mode(&path), 0o600);
            assert_eq!(mode(path.parent().expect("a shard")), 0o700);
        }
        assert_eq!(
            answered(&files, &read(&key, 5, 4)),
            Answer::Value(Value::Bytes(Rc::from(&b"name"[..])))
        );
        assert_eq!(
            answered(&files, &read(&key, 15, 100)),
            Answer::Value(Value::Bytes(Rc::from(&b"ples\n"[..])))
        );
        let missing = jadpo_std::files::key_for(&[0x40; 16]);
        assert_eq!(
            answered(&files, &read(&missing, 0, 10)),
            Answer::Failed(NOT_FOUND.into())
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// An export's pieces join into one file, in order, and go; joining it
    /// again does nothing, and discarding drops the file.
    #[test]
    fn pieces_join_into_one_file_once() {
        let root = root("join");
        let files = DiskFiles::under(&root);
        let id = [0x52; 16];
        for (piece, bytes) in [&b"code\r\n"[..], b"A1\r\n", b"A2\r\n"].iter().enumerate() {
            let key = piece_key(&id, piece as u64);
            assert_eq!(
                answered(&files, &put(&key, bytes)),
                Answer::Value(Value::Unit)
            );
        }
        let key = jadpo_std::files::key_for(&id);
        let hash = blake3::hash(b"code\r\nA1\r\nA2\r\n").to_hex().to_string();
        assert_eq!(
            answered(&files, &joined(&key, 3)),
            Answer::Value(Value::text(&hash))
        );
        let path = root.join("imports").join(&key);
        assert_eq!(fs::read(&path).expect("joined"), b"code\r\nA1\r\nA2\r\n");
        assert!(!root.join("imports").join(piece_key(&id, 0)).exists());
        assert_eq!(
            answered(&files, &joined(&key, 3)),
            Answer::Value(Value::text(&hash)),
            "joining again gives the same hash"
        );
        let discarded = Call::new(
            "imports",
            DISCARD,
            Value::record(
                "FileDiscard",
                vec![
                    (Rc::from("key"), Value::text(&key)),
                    (Rc::from("pieces"), Value::Int(3)),
                ],
            ),
            Repeat::Keyed,
        );
        assert_eq!(answered(&files, &discarded), Answer::Value(Value::Unit));
        assert!(!path.exists());
        assert_eq!(
            answered(&files, &joined(&key, 3)),
            Answer::Failed(NOT_FOUND.into())
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn deleted(key: &str) -> Call {
        Call::new(
            "imports",
            DELETE,
            Value::record("FileDelete", vec![(Rc::from("key"), Value::text(key))]),
            Repeat::Keyed,
        )
    }

    /// A delete removes the file under its key, and deleting it again, or
    /// a key naming nothing, is done already (chunk B3). A key it didn't
    /// make is refused, and a link at the key or on the way is removed
    /// itself or names nothing, never what it names: nothing outside the
    /// store is deleted.
    #[test]
    fn a_delete_removes_only_the_file_under_its_key() {
        let root = root("delete");
        let files = DiskFiles::under(&root);
        let key = jadpo_std::files::key_for(&[0x44; 16]);
        let other = jadpo_std::files::key_for(&[0x45; 16]);
        for (key, bytes) in [(&key, &b"code\n"[..]), (&other, b"name\n")] {
            assert_eq!(
                answered(&files, &put(key, bytes)),
                Answer::Value(Value::Unit)
            );
        }
        assert_eq!(answered(&files, &deleted(&key)), Answer::Value(Value::Unit));
        assert!(!root.join("imports").join(&key).exists());
        assert_eq!(
            answered(&files, &read(&key, 0, 1)),
            Answer::Failed(NOT_FOUND.into())
        );
        assert!(
            root.join("imports").join(&other).exists(),
            "another file stays"
        );
        // Again, and a key whose folders were never made: done already.
        assert_eq!(answered(&files, &deleted(&key)), Answer::Value(Value::Unit));
        let never = jadpo_std::files::key_for(&[0x99; 16]);
        assert_eq!(
            answered(&files, &deleted(&never)),
            Answer::Value(Value::Unit)
        );
        for key in ["../../etc/passwd", "3f/3f/../x", "", "/etc/passwd"] {
            assert_eq!(
                answered(&files, &deleted(key)),
                Answer::Failed(NOT_FOUND.into()),
                "{key}"
            );
        }
        #[cfg(unix)]
        {
            let outside = root.with_extension("outside");
            fs::create_dir_all(&outside).expect("outside");
            fs::write(outside.join("secret"), b"x").expect("secret");
            // A link at the key: it goes, and what it names stays.
            let linked = jadpo_std::files::key_for(&[0x46; 16]);
            fs::create_dir_all(root.join("imports").join(&linked[..5])).expect("folders");
            let file = root.join("imports").join(&linked);
            std::os::unix::fs::symlink(outside.join("secret"), &file).expect("link");
            assert_eq!(
                answered(&files, &deleted(&linked)),
                Answer::Value(Value::Unit)
            );
            assert!(fs::symlink_metadata(&file).is_err(), "the link went");
            assert_eq!(fs::read(outside.join("secret")).expect("secret"), b"x");
            // A link where a shard folder goes: nothing through it.
            let through = jadpo_std::files::key_for(&[0x47; 16]);
            fs::create_dir_all(outside.join("47")).expect("outside shard");
            fs::write(outside.join("47").join(&through[6..]), b"x").expect("outside file");
            std::os::unix::fs::symlink(&outside, root.join("imports").join(&through[..2]))
                .expect("link");
            assert_eq!(
                answered(&files, &deleted(&through)),
                Answer::Value(Value::Unit)
            );
            assert!(outside.join("47").join(&through[6..]).exists());
            let _ = fs::remove_dir_all(&outside);
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// A key the runtime didn't make, or a symbolic link on the way, names
    /// nothing: no path outside the store is read or written.
    #[test]
    fn a_key_it_didnt_make_or_a_link_names_nothing() {
        let root = root("refused");
        let files = DiskFiles::under(&root);
        for key in [
            "../../etc/passwd",
            "3f/3f/../x",
            "",
            "/etc/passwd",
            "3f/3f/3f3f3f3f-3f3f-3f3f-3f3f-3f3f3f3f3f3f/..",
        ] {
            assert_eq!(
                answered(&files, &put(key, b"x")),
                Answer::Failed(NOT_FOUND.into()),
                "{key}"
            );
            assert_eq!(
                answered(&files, &read(key, 0, 1)),
                Answer::Failed(NOT_FOUND.into()),
                "{key}"
            );
        }
        #[cfg(unix)]
        {
            let outside = root.with_extension("outside");
            fs::create_dir_all(&outside).expect("outside");
            fs::write(outside.join("secret"), b"x").expect("secret");
            // A link where a shard folder goes.
            let key = jadpo_std::files::key_for(&[0x41; 16]);
            let shard = root.join("imports").join(&key[..2]);
            fs::create_dir_all(root.join("imports")).expect("store");
            std::os::unix::fs::symlink(&outside, &shard).expect("link");
            assert_eq!(
                answered(&files, &read(&key, 0, 1)),
                Answer::Failed(NOT_FOUND.into())
            );
            assert_eq!(
                answered(&files, &put(&key, b"x")),
                Answer::Failed(NOT_FOUND.into())
            );
            assert_eq!(fs::read_dir(&outside).expect("outside").count(), 1);
            // A link where the file itself goes, to a file outside: a read
            // gives nothing, and a put replaces the link, never writing
            // through it.
            let key = jadpo_std::files::key_for(&[0x42; 16]);
            fs::create_dir_all(root.join("imports").join(&key[..5])).expect("folders");
            let file = root.join("imports").join(&key);
            std::os::unix::fs::symlink(outside.join("secret"), &file).expect("link");
            assert_eq!(
                answered(&files, &read(&key, 0, 1)),
                Answer::Failed(NOT_FOUND.into())
            );
            assert_eq!(
                answered(&files, &put(&key, b"kept")),
                Answer::Value(Value::Unit)
            );
            assert_eq!(fs::read(outside.join("secret")).expect("secret"), b"x");
            assert_eq!(fs::read(&file).expect("kept"), b"kept");
            assert!(
                !fs::symlink_metadata(&file)
                    .expect("kept")
                    .file_type()
                    .is_symlink()
            );
            // The store's own folder a link: nothing is read through it.
            let linked = root.join("linked");
            std::os::unix::fs::symlink(&outside, &linked).expect("link");
            let through = DiskFiles::under(&root);
            let call = Call::new(
                "linked",
                READ,
                Value::record(
                    "FileRead",
                    vec![
                        (Rc::from("key"), Value::text(&key)),
                        (Rc::from("from"), Value::Int(0)),
                        (Rc::from("length"), Value::Int(1)),
                    ],
                ),
                Repeat::Safe,
            );
            assert_eq!(answered(&through, &call), Answer::Failed(NOT_FOUND.into()));
            let _ = fs::remove_dir_all(&outside);
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// A pipe under a file's key is never waited on: it is no file.
    #[cfg(unix)]
    #[test]
    fn a_pipe_under_a_key_is_no_file() {
        let root = root("pipe");
        let files = DiskFiles::under(&root);
        let key = jadpo_std::files::key_for(&[0x43; 16]);
        let path = root.join("imports").join(&key);
        fs::create_dir_all(path.parent().expect("a shard")).expect("folders");
        let made = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .expect("mkfifo runs");
        assert!(made.success());
        assert_eq!(
            answered(&files, &read(&key, 0, 1)),
            Answer::Failed(NOT_FOUND.into())
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A deployable's store is kept at its `path:`, the folder made the
    /// app's user's alone; one other users can write to, one written
    /// relative, or a file, is refused.
    #[test]
    fn a_deployables_store_is_kept_at_its_path() {
        let root = root("deployed");
        let at = root.join("var/lib/bakery/sheets");
        let store = DiskStore::at(&at.to_string_lossy(), 0).expect("a store");
        #[cfg(unix)]
        assert_eq!(mode(&at), 0o700);
        let key = jadpo_std::files::key_for(&[0x51; 16]);
        assert_eq!(
            answered(&store, &put(&key, b"code\nA1\n")),
            Answer::Value(Value::Unit)
        );
        assert_eq!(fs::read(at.join(&key)).expect("kept"), b"code\nA1\n");
        assert_eq!(
            answered(&store, &read(&key, 0, 100)),
            Answer::Value(Value::Bytes(Rc::from(&b"code\nA1\n"[..])))
        );
        assert!(DiskStore::at("var/lib/sheets", 0).is_err());
        fs::write(root.join("a-file"), b"x").expect("a file");
        assert!(DiskStore::at(&root.join("a-file").to_string_lossy(), 0).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let shared = root.join("shared");
            fs::create_dir_all(&shared).expect("a folder");
            fs::set_permissions(&shared, fs::Permissions::from_mode(0o777)).expect("opened up");
            let refused = DiskStore::at(&shared.to_string_lossy(), 0).expect_err("refused");
            assert!(refused.contains("other users"), "{refused}");
            fs::set_permissions(&shared, fs::Permissions::from_mode(0o750)).expect("closed");
            assert!(DiskStore::at(&shared.to_string_lossy(), 0).is_ok());
        }
        // A store with no `path:` isn't built, saying why.
        let unbuilt = ServerDisk.store("sheets", None, 0).err().expect("unbuilt");
        assert!(unbuilt.contains("`path:`"), "{unbuilt}");
        let _ = fs::remove_dir_all(&root);
    }

    /// Below its floor a put, and a join, answer `storage_full`, and
    /// nothing is written; with no floor the same put lands.
    #[cfg(unix)]
    #[test]
    fn a_put_below_the_floor_answers_storage_full() {
        let root = root("floor");
        let key = jadpo_std::files::key_for(&[0x61; 16]);
        let at = root.join("full").to_string_lossy().into_owned();
        // Every byte of the disk kept free: no disk is that empty.
        let full = DiskStore::at(&at, WHOLE_DISK).expect("a store");
        assert_eq!(
            answered(&full, &put(&key, b"code\n")),
            Answer::Failed(STORAGE_FULL.into())
        );
        assert!(!root.join("full").join(&key).exists());
        let id = [0x62; 16];
        let roomy = DiskStore::at(&at, 0).expect("a store");
        assert_eq!(
            answered(&roomy, &put(&piece_key(&id, 0), b"code\n")),
            Answer::Value(Value::Unit)
        );
        let file = jadpo_std::files::key_for(&id);
        assert_eq!(
            answered(&full, &joined(&file, 1)),
            Answer::Failed(STORAGE_FULL.into())
        );
        assert_eq!(
            answered(&roomy, &joined(&file, 1)),
            Answer::Value(Value::text(&blake3::hash(b"code\n").to_hex()))
        );
        assert_eq!(
            answered(&roomy, &put(&key, b"code\n")),
            Answer::Value(Value::Unit)
        );
        // `jadpo dev`'s stores keep the floor they are given.
        let dev = DiskFiles::under(root.join("dev")).with_floors(|connection| {
            if connection == "imports" {
                WHOLE_DISK
            } else {
                0
            }
        });
        assert_eq!(
            answered(&dev, &put(&key, b"code\n")),
            Answer::Failed(STORAGE_FULL.into())
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// A native host gives the disk driver for the connections that keep
    /// files and for no other: a package's connection has no driver yet
    /// (chunk 7.1c).
    #[test]
    fn a_host_gives_the_disk_driver_only_for_stores_of_files() {
        use jadpo_runtime::Host;
        let root = root("narrowed");
        let host = crate::NativeHost::new().with_files(DiskFiles::under(&root), |connection| {
            connection == "imports"
        });
        assert!(host.capability("imports").is_some());
        assert!(host.capability("couriers").is_none());
        assert!(crate::NativeHost::new().capability("imports").is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
