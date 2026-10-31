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
//! Its I/O is blocking: a window is at most a megabyte. Uploads, downloads
//! and deleting come with the rest of the files capability (roadmap
//! "Remaining built-in services").

use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_drivers::factory::Disk;
use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
use jadpo_std::files::{
    DISCARD, JOIN, NOT_FOUND, PUT, READ, STORAGE_FULL, WHOLE_DISK, is_key, key_id, piece_key,
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
    made_on_write: bool,
}

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
            made_on_write: false,
        })
    }

    /// `jadpo dev`'s store at `root`, made as files are put, with `floor`
    /// millionths of the disk kept free.
    fn made_at(root: PathBuf, floor: u32) -> DiskStore {
        DiskStore {
            root,
            floor: floor.min(WHOLE_DISK),
            made_on_write: true,
        }
    }

    /// The root, opened; made first when `make` and it is `jadpo dev`'s.
    fn root(&self, make: bool) -> std::io::Result<os::Dir> {
        if make && self.made_on_write {
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
        if self.floor == 0 {
            return false;
        }
        let Ok((free, total)) = folder.space() else {
            return false;
        };
        let left = free.saturating_sub(u128::from(incoming));
        left * u128::from(WHOLE_DISK) < total * u128::from(self.floor)
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
            if parts.is_empty() && folder.open(name).is_ok() {
                return Answer::Value(Value::Unit);
            }
            return Answer::Failed(NOT_FOUND.into());
        }
        if self.below_floor(&folder, size) {
            return Answer::Failed(STORAGE_FULL.into());
        }
        let staged = part_name(name);
        let kept = (|| {
            let mut file = folder.create(&staged)?;
            for (_, _, part) in &mut parts {
                std::io::copy(part, &mut file)?;
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
        Answer::Value(Value::Unit)
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

    fn answer(&self, call: &Call) -> Answer {
        let Value::Record(input) = &call.input else {
            return Answer::Misconfigured;
        };
        let Some(Value::Text(key)) = input.get("key") else {
            return Answer::Misconfigured;
        };
        if !is_key(key) {
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

impl Capability for DiskStore {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let answer = self.answer(call);
        Box::pin(async move { answer })
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
}

/// A temporary file's name beside `name`, its own: created new, so no
/// other write, and nothing planted there, is written through.
fn part_name(name: &str) -> String {
    let mut random = [0u8; 8];
    let _ = getrandom::fill(&mut random);
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(".{name}.{suffix}.part")
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

        /// Removes file `name`: a link is removed itself, never what it
        /// names.
        pub fn remove(&self, name: &str) -> std::io::Result<()> {
            Ok(rustix::fs::unlinkat(&self.0, name, AtFlags::empty())?)
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

        pub fn remove(&self, name: &str) -> std::io::Result<()> {
            std::fs::remove_file(self.0.join(name))
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
        assert_eq!(
            answered(&files, &joined(&key, 3)),
            Answer::Value(Value::Unit)
        );
        let path = root.join("imports").join(&key);
        assert_eq!(fs::read(&path).expect("joined"), b"code\r\nA1\r\nA2\r\n");
        assert!(!root.join("imports").join(piece_key(&id, 0)).exists());
        assert_eq!(
            answered(&files, &joined(&key, 3)),
            Answer::Value(Value::Unit)
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
            Answer::Value(Value::Unit)
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
