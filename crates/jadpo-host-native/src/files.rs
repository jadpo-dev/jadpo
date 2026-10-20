//! The disk driver (area 16, "The disk driver"; chunk 6.4b): the `files`
//! capability on the server's own disk, which `jadpo dev` keeps every
//! store of files in, each under `.jadpo/files/<connection>/`, so nothing
//! leaves the machine.
//!
//! - **Keys the runtime makes.** A file is kept under its key, two
//!   directory levels sharded by its id ([`jadpo_std::files::key_for`]);
//!   the driver refuses any other key, so no key names a path outside its
//!   own, and refuses a symbolic link on the way to a file, so nothing
//!   outside its root is read or written through one.
//! - **Writes that land whole.** A put writes a temporary file readable by
//!   the application's user only, syncs it, renames it into place and
//!   syncs its directory, so a crash leaves the old file or the new, never
//!   part of one; a full disk answers `storage_full`.
//! - **Ranged reads.** A read seeks to its first byte and reads at most
//!   what it was asked for, so a batch reads a file of any size a window
//!   at a time.
//!
//! Its I/O is blocking: a window is at most a megabyte, on the developer's
//! own disk. A deployable's disk driver, at its connection's `path:`, with
//! uploads, downloads and the free-space floor, comes with the rest of the
//! files capability (roadmap "Remaining built-in services").

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability};
use jadpo_std::files::{NOT_FOUND, PUT, READ, STORAGE_FULL, is_key};
use jadpo_value::Value;

/// Every store of files under one folder, each connection in a folder of
/// its own: what `jadpo dev` gives a program.
#[derive(Debug)]
pub struct DiskFiles {
    root: PathBuf,
}

impl DiskFiles {
    /// The stores kept under `root`, made as files are put.
    pub fn under(root: impl Into<PathBuf>) -> DiskFiles {
        DiskFiles { root: root.into() }
    }

    /// Where connection `connection`'s file `key` is kept, when both are
    /// names the driver keeps files under.
    fn path(&self, connection: &str, key: &str) -> Option<PathBuf> {
        let named = !connection.is_empty()
            && connection
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        (named && is_key(key)).then(|| self.root.join(connection).join(key))
    }

    /// Whether anything from the root to `path` is a symbolic link.
    fn through_link(&self, path: &Path) -> bool {
        let mut at = self.root.clone();
        let Ok(rest) = path.strip_prefix(&self.root) else {
            return true;
        };
        for part in rest.components() {
            at.push(part);
            match fs::symlink_metadata(&at) {
                Ok(meta) if meta.file_type().is_symlink() => return true,
                _ => {}
            }
        }
        false
    }

    fn put(&self, path: &Path, bytes: &[u8]) -> Answer {
        let Some(folder) = path.parent() else {
            return Answer::Misconfigured;
        };
        if self.through_link(path) {
            return Answer::Failed(NOT_FOUND.into());
        }
        if let Err(error) = private_folder(folder) {
            return written(&error);
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return Answer::Misconfigured;
        };
        let part = folder.join(format!(".{name}.part"));
        let kept = (|| {
            let mut file = private_file(&part)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&part, path)?;
            File::open(folder)?.sync_all()
        })();
        match kept {
            Ok(()) => Answer::Value(Value::Unit),
            Err(error) => {
                let _ = fs::remove_file(&part);
                written(&error)
            }
        }
    }

    fn read(&self, path: &Path, from: u64, length: u64) -> Answer {
        if self.through_link(path) {
            return Answer::Failed(NOT_FOUND.into());
        }
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Answer::Failed(NOT_FOUND.into());
            }
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

    fn answer(&self, call: &Call) -> Answer {
        let Value::Record(input) = &call.input else {
            return Answer::Misconfigured;
        };
        let Some(Value::Text(key)) = input.get("key") else {
            return Answer::Misconfigured;
        };
        let Some(path) = self.path(&call.connection, key) else {
            return Answer::Failed(NOT_FOUND.into());
        };
        match call.operation.as_str() {
            PUT => match input.get("bytes") {
                Some(Value::Bytes(bytes)) => self.put(&path, bytes),
                _ => Answer::Misconfigured,
            },
            READ => match (input.get("from"), input.get("length")) {
                (Some(Value::Int(from)), Some(Value::Int(length))) => self.read(
                    &path,
                    u64::try_from(*from).unwrap_or(0),
                    u64::try_from(*length).unwrap_or(0),
                ),
                _ => Answer::Misconfigured,
            },
            _ => Answer::Misconfigured,
        }
    }
}

impl Capability for DiskFiles {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let answer = self.answer(call);
        Box::pin(async move { answer })
    }
}

/// A failed write's answer: `storage_full` when the disk has no room,
/// otherwise nothing kept, safe to send again.
fn written(error: &std::io::Error) -> Answer {
    match error.kind() {
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Answer::Failed(STORAGE_FULL.into()),
        _ => Answer::Unavailable,
    }
}

#[cfg(unix)]
fn private_folder(folder: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
}

#[cfg(not(unix))]
fn private_folder(folder: &Path) -> std::io::Result<()> {
    fs::create_dir_all(folder)
}

#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_runtime::capability::Repeat;

    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "jadpo-disk-files-6.4b-{name}-{}",
            std::process::id()
        ));
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

    fn answered(files: &DiskFiles, call: &Call) -> Answer {
        jadpo_runtime::block_on(files.call(call)).expect("no wait")
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
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).expect("kept").permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
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

    /// A key the runtime didn't make, or a symbolic link on the way, names
    /// nothing: no path outside the store is read or written.
    #[test]
    fn a_key_it_didnt_make_or_a_link_names_nothing() {
        let root = root("refused");
        let files = DiskFiles::under(&root);
        for key in ["../../etc/passwd", "3f/3f/../x", ""] {
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
            let _ = fs::remove_dir_all(&outside);
        }
        let _ = fs::remove_dir_all(&root);
    }
}
