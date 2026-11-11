//! `jadpo files` (area 14; area 16, "The disk driver" and "The S3 driver"):
//! a store of files from the command line. A disk store's files, a large
//! one among them, and its erasure log are copied into an S3 bucket on the
//! stand-in S3 server (`support::fake_s3`), which checks every signature,
//! and a second copy leaves what is there; `ls`, `show`, `get` and `put`
//! work on either driver and on `jadpo dev`'s copy; and each mistake is
//! refused with its own rule.

use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_std::files::key_for;
use jadpo_term::Environment;

use crate::support::fake_s3::FakeS3;

const KEY_ID: &str = "AKIAJADPOSTANDIN";
const SECRET: &str = "stand-in-secret-access-key";
const BUCKET: &str = "uploads";

const APP: &str = "application {
    connections: {
        uploads: {
            driver: Driver.s3
            host: config.s3_host
            region: \"auto\"
            bucket: \"uploads\"
            credential: {
                key_id: config.s3_key_id
                secret: config.s3_secret
            }
        }
        sheets: {
            driver: Driver.disk
            path: \"/var/lib/jadpo-files-test/sheets\"
        }
    }
}
";

const CONFIG: &str = "config {
    /// The S3 service's address.
    s3_host: Text
    /// The S3 credential's key id.
    s3_key_id: Text
    /// The S3 credential's secret access key.
    s3_secret: Text {
        secret: true
    }
}
";

fn jadpo(cwd: &Path, args: &[&str], vars: &[(&str, &str)]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("JADPO_PLAIN", "1");
    for (name, value) in vars {
        env = env.with_var(name, value);
    }
    run(&args, &env.with_cwd(cwd))
}

/// A project of its own, `name`, holding the two stores.
fn project(name: &str) -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target.join("jadpo-files-commands").join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a folder");
    std::fs::write(root.join("app.jadpo"), APP).expect("app.jadpo");
    std::fs::write(root.join("config.jadpo"), CONFIG).expect("config.jadpo");
    let updated = jadpo(&root, &["records", "update"], &[]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    root
}

fn key(n: u8) -> String {
    let mut id = [0x11u8; 16];
    id[15] = n;
    id[6] = 0x40;
    key_for(&id)
}

fn shown(out: &Output) -> String {
    format!("{}{}", out.stdout, out.stderr)
}

/// A disk store's files and its erasure log are copied into the bucket
/// its connection now names, a file over a piece's size in pieces joined
/// there; a second copy leaves them, and the bucket reads back the same.
#[test]
fn a_disk_stores_files_are_copied_to_s3_and_read_back() {
    let root = project("copy");
    let s3 = FakeS3::start(BUCKET, KEY_ID, SECRET);
    let host = s3.address();
    let vars = [
        ("S3_HOST", host.as_str()),
        ("S3_KEY_ID", KEY_ID),
        ("S3_SECRET", SECRET),
    ];
    // The disk store the app kept its files in before, a folder of its own.
    let folder = root.join("old-sheets");
    let folder_text = folder.to_string_lossy().into_owned();
    let small = root.join("small.csv");
    std::fs::write(&small, b"code,price\nrye,3.50\n").expect("a file");
    let large = root.join("large.bin");
    let large_bytes: Vec<u8> = (0..(17u64 << 20)).map(|at| (at % 251) as u8).collect();
    std::fs::write(&large, &large_bytes).expect("a file");
    for (key, path) in [(key(1), &small), (key(2), &large)] {
        let put = jadpo(
            &root,
            &["files", "put", &folder_text, &key, &path.to_string_lossy()],
            &[],
        );
        assert_eq!(put.code, 0, "{}", shown(&put));
        assert!(put.stdout.contains("Put "), "{}", shown(&put));
    }
    let logged = "erasure-log/User/0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    std::fs::create_dir_all(folder.join("erasure-log/User")).expect("a folder");
    std::fs::write(folder.join(logged), b"erased").expect("a log entry");

    let listed = jadpo(&root, &["files", "ls", &folder_text], &[]);
    assert_eq!(listed.code, 0, "{}", shown(&listed));
    assert!(listed.stdout.contains(&key(1)), "{}", shown(&listed));
    assert!(listed.stdout.contains("erasure log"), "{}", shown(&listed));
    assert!(listed.stdout.contains("3 files"), "{}", shown(&listed));

    let copied = jadpo(&root, &["files", "copy", &folder_text, "uploads"], &vars);
    assert_eq!(copied.code, 0, "{}", shown(&copied));
    assert!(
        copied.stdout.contains("Copied 3 files"),
        "{}",
        shown(&copied)
    );
    assert!(
        !shown(&copied).contains(SECRET),
        "a credential is never printed: {}",
        shown(&copied)
    );
    assert_eq!(s3.unverified(), 0);
    assert_eq!(s3.open_uploads(), 0);
    assert_eq!(
        s3.object(&key(1)).expect("copied").bytes,
        b"code,price\nrye,3.50\n"
    );
    assert_eq!(s3.object(&key(2)).expect("copied").bytes, large_bytes);
    assert_eq!(s3.object(logged).expect("copied").bytes, b"erased");
    let mut keys = s3.keys();
    keys.sort();
    assert_eq!(keys.len(), 3, "only the files, no piece left: {keys:?}");

    // Again: nothing to copy, what is there left alone.
    let again = jadpo(&root, &["files", "copy", &folder_text, "uploads"], &vars);
    assert_eq!(again.code, 0, "{}", shown(&again));
    assert!(again.stdout.contains("Copied 0 files"), "{}", shown(&again));
    assert!(
        again.stdout.contains("3 files already there"),
        "{}",
        shown(&again)
    );

    // The bucket lists, shows and gives back what it holds.
    let listed = jadpo(&root, &["files", "ls", "uploads", "--json"], &vars);
    assert_eq!(listed.code, 0, "{}", shown(&listed));
    let document: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let files = document["data"]["files"].as_array().expect("files");
    assert_eq!(files.len(), 3, "{document}");
    assert_eq!(files[0]["key"], key(1).as_str());
    assert_eq!(files[0]["size"], 20);
    let showed = jadpo(&root, &["files", "show", "uploads", &key(1)], &vars);
    assert_eq!(showed.code, 0, "{}", shown(&showed));
    assert!(
        showed
            .stdout
            .contains(&jadpo_runtime::files::hash_of(b"code,price\nrye,3.50\n")),
        "{}",
        shown(&showed)
    );
    let back = root.join("back.bin");
    let got = jadpo(
        &root,
        &["files", "get", "uploads", &key(2), "back.bin"],
        &vars,
    );
    assert_eq!(got.code, 0, "{}", shown(&got));
    assert_eq!(std::fs::read(&back).expect("written"), large_bytes);
    // Never over a file that is there.
    let again = jadpo(
        &root,
        &["files", "get", "uploads", &key(2), "back.bin"],
        &vars,
    );
    assert_eq!(again.code, 2, "{}", shown(&again));
    assert!(shown(&again).contains("cli.files_arguments"));
}

/// `jadpo dev`'s copy is reached with `--dev`, a file put back under its
/// key once and never again, and the stores listed by their connections.
#[test]
fn jadpo_devs_copy_and_the_stores_are_listed() {
    let root = project("dev");
    let listed = jadpo(&root, &["files"], &[]);
    assert_eq!(listed.code, 0, "{}", shown(&listed));
    assert!(listed.stdout.contains("uploads"), "{}", shown(&listed));
    assert!(
        listed.stdout.contains("/var/lib/jadpo-files-test/sheets"),
        "{}",
        shown(&listed)
    );
    let empty = jadpo(&root, &["files", "ls", "sheets", "--dev"], &[]);
    assert_eq!(empty.code, 0, "{}", shown(&empty));
    assert!(empty.stdout.contains("No files"), "{}", shown(&empty));
    std::fs::write(root.join("note.csv"), b"a,b\n").expect("a file");
    let put = jadpo(
        &root,
        &["files", "put", "sheets", &key(7), "note.csv", "--dev"],
        &[],
    );
    assert_eq!(put.code, 0, "{}", shown(&put));
    assert!(root.join(".jadpo/files/sheets").join(key(7)).is_file());
    let twice = jadpo(
        &root,
        &["files", "put", "sheets", &key(7), "note.csv", "--dev"],
        &[],
    );
    assert_eq!(twice.code, 2, "{}", shown(&twice));
    assert!(shown(&twice).contains("already holds"), "{}", shown(&twice));
}

/// Each mistake is refused with its rule, and with the nearest name.
#[test]
fn mistakes_are_refused_with_their_rules() {
    let root = project("mistakes");
    let unknown = jadpo(&root, &["files", "lss"], &[]);
    assert_eq!(unknown.code, 2, "{}", shown(&unknown));
    assert!(shown(&unknown).contains("Did you mean `ls`?"));
    let store = jadpo(&root, &["files", "ls", "upload"], &[]);
    assert_eq!(store.code, 2, "{}", shown(&store));
    assert!(shown(&store).contains("Use `uploads`"), "{}", shown(&store));
    let unset = jadpo(&root, &["files", "ls", "uploads"], &[]);
    assert_eq!(unset.code, 1, "{}", shown(&unset));
    assert!(
        shown(&unset).contains("files.unavailable"),
        "{}",
        shown(&unset)
    );
    assert!(shown(&unset).contains("S3_HOST"), "{}", shown(&unset));
    let away = jadpo(&root, &["files", "ls", "sheets"], &[]);
    assert_eq!(away.code, 1, "{}", shown(&away));
    assert!(
        shown(&away).contains("isn't on this machine"),
        "{}",
        shown(&away)
    );
    let not_a_key = jadpo(
        &root,
        &["files", "show", "sheets", "../../etc/passwd", "--dev"],
        &[],
    );
    assert_eq!(not_a_key.code, 2, "{}", shown(&not_a_key));
    assert!(shown(&not_a_key).contains("isn't a file's key"));
    let missing = jadpo(&root, &["files", "show", "sheets", &key(9), "--dev"], &[]);
    assert_eq!(missing.code, 2, "{}", shown(&missing));
    assert!(
        shown(&missing).contains("holds no file"),
        "{}",
        shown(&missing)
    );
}
