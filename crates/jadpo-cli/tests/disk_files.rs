//! The disk driver at a store's `path:` (area 16, "The disk driver";
//! chunk B2): `jadpo check` refuses a `path:` inside the project's own
//! folder, among its source or in `build/`, and records in the manifest
//! that a deployable reaching a disk store runs on one server; and
//! `jadpo dev`, the interpreter, keeps each store's files under
//! `.jadpo/files/<store>/`, the app's user's alone, reads a put file's
//! rows back in, and answers a put below the store's `free_space_floor:`
//! with `storage_full`, as a native deployable does (`deployable.rs`).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! `jadpo dev` test.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::Dev;
use crate::http::tempdir;

/// A bakery keeping its suppliers' price sheets in `sheets`, at
/// `sheets_at` with no floor, and reading their prices in; and an
/// overflow store at `full_at` that keeps 99.99% of its disk free, so no
/// put to it ever fits.
pub(crate) fn bakery(sheets_at: &Path, full_at: &Path) -> Vec<(String, String)> {
    let app = format!(
        "application {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n            url: config.database_url\n        }}\n        sheets: {{\n            driver: Driver.disk\n            path: \"{}\"\n            free_space_floor: 0%\n        }}\n        full: {{\n            driver: Driver.disk\n            path: \"{}\"\n            free_space_floor: 99.99%\n        }}\n    }}\n}}\n",
        sheets_at.display(),
        full_at.display()
    );
    let files: Vec<(&str, &str)> = vec![
        ("app.jadpo", &app),
        (
            "config.jadpo",
            "config {\n    /// The bakery's database.\n    database_url: PostgresUrl\n}\n",
        ),
        (
            "domains/bakery/types/supplier_name.jadpo",
            "/// Who sent a price sheet.\ntype SupplierName = Text {\n    min_length: 1\n    max_length: 80\n}\n",
        ),
        (
            "domains/bakery/types/sheet_text.jadpo",
            "/// A price sheet as a supplier sends it.\ntype SheetText = Text {\n    min_length: 1\n    max_length: 1000\n}\n",
        ),
        (
            "domains/bakery/types/price_code.jadpo",
            "/// A product's code on a price sheet.\ntype PriceCode = Text {\n    min_length: 1\n    max_length: 20\n}\n",
        ),
        (
            "domains/bakery/types/price_row.jadpo",
            "use bakery/types { PriceCode }\n\n/// One row of a price sheet.\ntype PriceRow = Input {\n    code: PriceCode {\n        column: \"code\"\n    }\n}\n",
        ),
        (
            "domains/bakery/types/sheet_request.jadpo",
            "use bakery/types { SheetText, SupplierName }\n\n/// A sheet a supplier sends.\ntype SheetRequest = Input {\n    supplier: SupplierName\n    sheet_text: SheetText\n}\n",
        ),
        (
            "domains/bakery/types/sheet_view.jadpo",
            "use bakery/entities { PriceSheet }\n\n/// A kept sheet as its supplier sees it.\ntype SheetView = Output {\n    pick: [PriceSheet.id, PriceSheet.supplier]\n}\n",
        ),
        (
            "domains/bakery/failures/sheet_not_kept.jadpo",
            "/// The store had no room for a sheet.\ntype SheetNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the sheet.\"\n}\n",
        ),
        (
            "domains/bakery/entities/price_sheet.jadpo",
            "use bakery/types { SupplierName }\n\n/// A supplier's price sheet, kept as it was sent.\nentity PriceSheet {\n    id: Uuid\n    supplier: SupplierName\n    sheet: File {\n        store: sheets\n        max_size: 10kb\n        types: [FileType.csv]\n    }\n    reading: BatchRun\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/bakery/entities/overflow_sheet.jadpo",
            "use bakery/types { SupplierName }\n\n/// A sheet kept in the overflow store.\nentity OverflowSheet {\n    id: Uuid\n    supplier: SupplierName\n    sheet: File {\n        store: full\n        max_size: 10kb\n        types: [FileType.csv]\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/bakery/entities/price_line.jadpo",
            "use bakery/types { PriceCode }\n\n/// A product's price, read in from a sheet.\nentity PriceLine {\n    id: Uuid\n    code: PriceCode\n\n    policy: {\n        create: [Access.public, Access.system]\n        read: [Access.public, Access.system]\n    }\n}\n",
        ),
        (
            "domains/bakery/actions/save_price.jadpo",
            "use bakery/entities { PriceLine }\nuse bakery/types { PriceRow }\n\n/// Keeps one row's price.\naction save_price(row: PriceRow) -> PriceLine {\n    return PriceLine.create {\n        set: {\n            code: row.code\n        }\n    }\n}\n",
        ),
        (
            "domains/bakery/actions/keep_prices.jadpo",
            "use app { sheets }\nuse bakery/entities { PriceSheet }\nuse bakery/types { PriceRow, SheetRequest, SheetView }\nuse bakery/failures { SheetNotKept }\nuse bakery/actions { save_price }\n\n/// Keeps a price sheet as its supplier sent it, and reads its prices in.\naction keep_prices(request: SheetRequest) fails SheetNotKept -> SheetView {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(request.sheet_text), FileType.csv) {\n        storage_full: SheetNotKept\n    }\n    var reading: BatchRun = csv.read(sheet, as: PriceRow, each: row -> save_price(row))\n    var kept: PriceSheet = PriceSheet.create {\n        set: {\n            supplier: request.supplier\n            sheet: sheet\n            reading: reading\n        }\n    }\n    return SheetView {\n        id: kept.id\n        supplier: kept.supplier\n    }\n}\n",
        ),
        (
            "domains/bakery/actions/keep_overflow.jadpo",
            "use app { full }\nuse bakery/entities { OverflowSheet }\nuse bakery/types { SheetRequest }\nuse bakery/failures { SheetNotKept }\n\n/// Keeps a sheet in the overflow store.\naction keep_overflow(request: SheetRequest) fails SheetNotKept -> Unit {\n    var sheet: File = attempt full.put(encoding.to_utf8(request.sheet_text), FileType.csv) {\n        storage_full: SheetNotKept\n    }\n    var kept: OverflowSheet = OverflowSheet.create {\n        set: {\n            supplier: request.supplier\n            sheet: sheet\n        }\n    }\n    assert kept.supplier == request.supplier\n}\n",
        ),
        (
            "domains/bakery/routes/sheets.jadpo",
            "use bakery/types { SheetRequest, SheetView }\nuse bakery/actions { keep_overflow, keep_prices }\n\n/// The bakery's price sheets.\nroutes Sheets {\n    prefix: /sheets\n\n    /// Keeps a sheet and reads its prices in.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: SheetRequest\n        output: SheetView\n        status: SuccessStatus.created\n        run: keep_prices(input)\n    }\n\n    /// Keeps a sheet in the overflow store.\n    POST /overflow {\n        authentication: RouteAuthentication.none\n        input: SheetRequest\n        status: SuccessStatus.no_content\n        run: keep_overflow(input)\n    }\n}\n",
        ),
    ];
    files
        .into_iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

/// The sheet the tests send, and the prices it holds.
pub(crate) const SHEET: &str = r#"{"supplier":"Millers","sheet_text":"code\nA1\nB2\n"}"#;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// `files` written at `root`, a file left as it was keeping its time.
pub(crate) fn write_all(root: &Path, files: &[(String, String)]) {
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            fs::write(&path, text).expect("written");
        }
    }
}

/// The project's folder as the checker sees it, its links resolved.
fn real(path: &Path) -> PathBuf {
    fs::canonicalize(path).expect("a folder")
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

/// A store's `path:` inside the project, in `build/`, among its source or
/// the project's folder itself, is refused, naming where; outside it, the
/// project checks, and its manifest records the stores that keep its
/// deployable on one server.
#[test]
fn a_disk_store_inside_the_project_is_refused_and_one_outside_keeps_it_on_one_server() {
    let dir = tempdir::Dir::new("disk-files-check");
    let root = real(dir.path()).join("bakery");
    let outside = real(dir.path()).join("var");
    for (inside, label) in [
        (root.join("build/files"), "inside `build/`"),
        (root.join("domains/bakery/sheets"), "inside `domains/`"),
        (root.join(".jadpo/files"), "inside `.jadpo/`"),
        (root.clone(), "the project's own folder"),
    ] {
        let _ = fs::remove_dir_all(&root);
        write_all(&root, &bakery(&inside, &outside.join("full")));
        let checked = jadpo(&root, &["check"]);
        assert_eq!(checked.code, 1, "{}{}", checked.stdout, checked.stderr);
        assert!(
            checked
                .stdout
                .contains("error: `sheets`'s `path:` is inside the project"),
            "{}",
            checked.stdout
        );
        assert!(
            checked.stdout.contains(label),
            "{label}: {}",
            checked.stdout
        );
        assert!(
            checked
                .stdout
                .contains("rule: connection.disk_path_invalid"),
            "{}",
            checked.stdout
        );
    }
    // A link to the project is the project.
    #[cfg(unix)]
    {
        let link = real(dir.path()).join("linked");
        std::os::unix::fs::symlink(&root, &link).expect("a link");
        write_all(
            &root,
            &bakery(&link.join("build/files"), &outside.join("full")),
        );
        let checked = jadpo(&root, &["check"]);
        assert!(
            checked.stdout.contains("inside `build/`"),
            "{}",
            checked.stdout
        );
    }

    let _ = fs::remove_dir_all(&root);
    write_all(
        &root,
        &bakery(&outside.join("sheets"), &outside.join("full")),
    );
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let output = jadpo(&root, args);
        assert_eq!(
            output.code, 0,
            "{args:?}: {}{}",
            output.stdout, output.stderr
        );
    }
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/manifest.json")).expect("the manifest"),
    )
    .expect("JSON");
    assert_eq!(
        manifest["deployables"]["app"]["one_server"],
        serde_json::json!(["full", "sheets"]),
        "{}",
        manifest["deployables"]
    );
    // Checking makes no store's folder: only a deployable, starting, does.
    assert!(!outside.exists());
}

/// Waits up to `seconds` for `ready` to hold.
pub(crate) fn eventually(seconds: u64, ready: impl Fn() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        if ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    ready()
}

/// The one file under `folder`, sharded two levels deep, as a store keeps
/// it: its path.
pub(crate) fn kept_file(folder: &Path) -> PathBuf {
    let mut found = Vec::new();
    for first in fs::read_dir(folder).expect("the store") {
        let first = first.expect("an entry").path();
        for second in fs::read_dir(&first).expect("a shard") {
            let second = second.expect("an entry").path();
            for file in fs::read_dir(&second).expect("a shard") {
                found.push(file.expect("an entry").path());
            }
        }
    }
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

/// How many files are under `folder`, at any depth: none when it is
/// missing.
pub(crate) fn files_under(folder: &Path) -> usize {
    let Ok(entries) = fs::read_dir(folder) else {
        return 0;
    };
    entries
        .map(|entry| entry.expect("an entry").path())
        .map(|path| if path.is_dir() { files_under(&path) } else { 1 })
        .sum()
}

/// `jadpo dev`, interpreted, keeps a put sheet under `.jadpo/files/`, the
/// app's user's alone, reads its rows back in, and answers a put to the
/// store below its floor with `storage_full`.
#[test]
fn jadpo_dev_keeps_files_under_the_project_and_keeps_its_floor() {
    if !jadpo_test::live() {
        return;
    }
    let dir = tempdir::Dir::new("disk-files-dev");
    let root = real(dir.path()).join("bakery");
    let outside = real(dir.path()).join("var");
    write_all(
        &root,
        &bakery(&outside.join("sheets"), &outside.join("full")),
    );
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let output = jadpo(&root, args);
        assert_eq!(
            output.code, 0,
            "{args:?}: {}{}",
            output.stdout, output.stderr
        );
    }
    let dev = Dev::start(&root);
    let reply = dev.send("POST", "/sheets", &[], SHEET);
    assert_eq!(reply.status, 201, "{reply:?}");
    assert_eq!(reply.json()["supplier"], "Millers", "{reply:?}");
    let store = root.join(".jadpo/files/sheets");
    let file = kept_file(&store);
    assert_eq!(fs::read(&file).expect("kept"), b"code\nA1\nB2\n");
    #[cfg(unix)]
    {
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(file.parent().expect("a shard")), 0o700);
    }
    let url = dev.url().to_string();
    assert!(
        eventually(30, || crate::deployable::count(&url, "price_line") == 2),
        "the sheet's two prices were read in: {}",
        dev.log()
    );
    let refused = dev.send("POST", "/sheets/overflow", &[], SHEET);
    assert_eq!(refused.status, 503, "{refused:?}");
    assert_eq!(
        refused.json()["error"]["code"],
        "sheet_not_kept",
        "{refused:?}"
    );
    assert_eq!(files_under(&root.join(".jadpo/files/full")), 0);
    // `jadpo dev` keeps nothing at a store's production `path:`.
    assert!(!outside.exists());
}
