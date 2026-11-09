//! Downloads against live PostgreSQL (area 16, "Downloading"; chunk B4): a
//! bakery keeps its suppliers' price sheets, and a route that picks a
//! sheet's `File` sends a signed link. The link's download, whole and by
//! range, an expired link, an altered one, and a person's export reaching
//! a `File` are this project's route tests, run interpreted here and
//! compiled, compared with the interpreter's records, in the gate's
//! compiled step (`compiled.rs`, "a file downloaded").
//!
//! What only a real client can see, headers and hostile requests against a
//! real disk store, is [`serves_a_file`]: over real HTTP from `jadpo dev`
//! (`disk_files.rs`) and a compiled deployable (`deployable.rs`).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const OPEN: &str = "    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
";

/// A bakery's price sheets, a route that sends one, and a person whose
/// export reaches a sheet.
pub(crate) fn files(path: &str) -> Vec<(String, String)> {
    let app = format!(
        "application {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n            url: config.database_url\n        }}\n        sheets: {{\n            driver: Driver.disk\n            path: \"{path}\"\n        }}\n    }}\n    privacy: {{\n        erasure_log: {{\n            store: sheets\n        }}\n    }}\n}}\n"
    );
    let plain: Vec<(&str, String)> = vec![
        ("app.jadpo", app),
        (
            "config.jadpo",
            "config {\n    /// The bakery's database.\n    database_url: PostgresUrl\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/supplier_name.jadpo",
            "/// Who sends price sheets.\ntype SupplierName = Text {\n    min_length: 1\n    max_length: 80\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/sheet_text.jadpo",
            "/// A price sheet as a supplier sends it.\ntype SheetText = Text {\n    min_length: 1\n    max_length: 100000\n}\n".to_string(),
        ),
        (
            "domains/bakery/failures/sheet_not_kept.jadpo",
            "/// The sheets store had no room for a sheet.\ntype SheetNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the sheet.\"\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/new_sheet.jadpo",
            "use bakery/types { SheetText, SupplierName }\n\n/// A sheet to keep.\ntype NewSheet = Input {\n    supplier: SupplierName\n    sheet_text: SheetText\n}\n".to_string(),
        ),
        (
            "domains/bakery/entities/price_sheet.jadpo",
            format!("use bakery/types {{ SupplierName }}\n\n/// A supplier's price sheet, kept as it was sent.\nentity PriceSheet {{\n    id: Uuid\n    supplier: SupplierName\n    sheet: File {{\n        store: sheets\n        max_size: 1mb\n        types: [FileType.csv]\n    }}\n\n{OPEN}}}\n"),
        ),
        (
            "domains/bakery/types/sheet_view.jadpo",
            "use bakery/entities { PriceSheet }\n\n/// A sheet and the link to its file.\ntype SheetView = Output {\n    pick: [PriceSheet.id, PriceSheet.supplier, PriceSheet.sheet]\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/keep_sheet.jadpo",
            "use app { sheets }\nuse bakery/entities { PriceSheet }\nuse bakery/types { NewSheet, SheetView }\nuse bakery/failures { SheetNotKept }\n\n/// Keeps a supplier's price sheet.\naction keep_sheet(input: NewSheet) fails SheetNotKept -> SheetView {\n    var sheet: File = attempt sheets.put(encoding.to_utf8(input.sheet_text), FileType.csv) {\n        storage_full: SheetNotKept\n    }\n    var kept: PriceSheet = PriceSheet.create {\n        set: {\n            supplier: input.supplier\n            sheet: sheet\n        }\n    }\n    return SheetView {\n        id: kept.id\n        supplier: kept.supplier\n        sheet: kept.sheet\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/routes/sheets.jadpo",
            "use bakery/actions { keep_sheet }\nuse bakery/types { NewSheet, SheetView }\n\nroutes Sheets {\n    prefix: /sheets\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NewSheet\n        output: SheetView\n        status: SuccessStatus.created\n        run: keep_sheet(input)\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/routes/sheets.test.jadpo",
            r#"use bakery/types { NewSheet }

test "a sheet's link downloads it whole and by range" {
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet_text: "flour,1.20\nrye,2.40\n"
        }
    }
    assert kept.status == 201
    var sheet: File? = match kept.output {
        some(view) -> view.sheet
        none -> none
    }
    var whole: Response = request {
        method: HttpMethod.get
        link: sheet
    }
    assert whole.status == 200
    assert whole.code == none
    assert whole.body == encoding.to_utf8("flour,1.20\nrye,2.40\n")

    var first: Response = request {
        method: HttpMethod.get
        link: sheet
        range: "bytes=0-4"
    }
    assert first.status == 206
    assert first.body == encoding.to_utf8("flour")

    var tail: Response = request {
        method: HttpMethod.get
        link: sheet
        range: "bytes=-5"
    }
    assert tail.status == 206
    assert tail.body == encoding.to_utf8("2.40\n")

    var open_ended: Response = request {
        method: HttpMethod.get
        link: sheet
        range: "bytes=15-"
    }
    assert open_ended.status == 206
    assert open_ended.body == encoding.to_utf8("2.40\n")

    var beyond: Response = request {
        method: HttpMethod.get
        link: sheet
        range: "bytes=100-200"
    }
    assert beyond.status == 416
    assert beyond.code == "range_not_satisfiable"
}

test "a malformed or a many-part range is the whole file" {
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet_text: "flour,1.20\n"
        }
    }
    var sheet: File? = match kept.output {
        some(view) -> view.sheet
        none -> none
    }
    var malformed: Response = request {
        method: HttpMethod.get
        link: sheet
        range: "bytes=abc"
    }
    assert malformed.status == 200
    assert malformed.body == encoding.to_utf8("flour,1.20\n")
    var overlapping: Response = request {
        method: HttpMethod.get
        link: sheet
        range: "bytes=0-4,2-6"
    }
    assert overlapping.status == 200
    assert overlapping.body == encoding.to_utf8("flour,1.20\n")
}

test "a link expires after five minutes" {
    clock: "2026-01-15T12:00:00Z"

    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet_text: "flour,1.20\n"
        }
    }
    var sheet: File? = match kept.output {
        some(view) -> view.sheet
        none -> none
    }
    clock.advance(4m)
    var still: Response = request {
        method: HttpMethod.get
        link: sheet
    }
    assert still.status == 200
    clock.advance(2m)
    var refused: Response = request {
        method: HttpMethod.get
        link: sheet
    }
    assert refused.status == 403
    assert refused.code == "link_expired"
    assert refused.body == none
}

test "an altered link is refused" {
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet_text: "flour,1.20\n"
        }
    }
    assert kept.status == 201
    var refused: Response = request {
        method: HttpMethod.get
        link: "/files/sheets/eyJrIjoiLi4vLi4vZXRjL3Bhc3N3ZCJ9.AAAA"
    }
    assert refused.status == 403
    assert refused.code == "link_invalid"
    var escaped: Response = request {
        method: HttpMethod.get
        link: "/files/sheets/..%2f..%2fetc%2fpasswd"
    }
    assert escaped.status == 403
    assert escaped.code == "link_invalid"
}

test "a path that names no store is no download" {
    var refused: Response = request {
        method: HttpMethod.get
        link: "/files/nowhere/abc.def"
    }
    assert refused.status == 404
    assert refused.code == "route_not_found"
}
"#
            .to_string(),
        ),
        (
            "domains/accounts/failures/scan_not_kept.jadpo",
            "/// The sheets store had no room for a scan.\ntype ScanNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the scan.\"\n}\n".to_string(),
        ),
        (
            "domains/accounts/entities/member.jadpo",
            format!("/// Someone who keeps documents.\nentity Member {{\n    id: Uuid\n    handle: Text {{\n        max_length: 80\n        personal: PersonalData.identifying\n    }}\n\n    person: true\n\n{OPEN}}}\n"),
        ),
        (
            "domains/accounts/entities/scan.jadpo",
            format!("use accounts/entities {{ Member }}\n\n/// A member's scanned document.\nentity Scan {{\n    id: Uuid\n    member_id: Member.id {{\n        on_delete: OnDelete.cascade\n    }}\n    document: File {{\n        store: sheets\n        max_size: 1mb\n        personal: PersonalData.identifying\n    }}\n\n{OPEN}}}\n"),
        ),
        (
            "domains/accounts/actions/keep_scan.jadpo",
            "use app { sheets }\nuse accounts/entities { Member, Scan }\nuse accounts/failures { ScanNotKept }\n\n/// Keeps a member and a scan of theirs.\naction keep_scan() fails ScanNotKept -> Scan {\n    var document: File = attempt sheets.put(encoding.to_utf8(\"id\"), FileType.pdf) {\n        storage_full: ScanNotKept\n    }\n    var member: Member = Member.create {\n        set: {\n            handle: \"ada\"\n        }\n    }\n    return Scan.create {\n        set: {\n            member_id: member.id\n            document: document\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/accounts/actions/keep_scan.test.jadpo",
            "use accounts/entities { Member, Scan }\n\ntest \"a member's export links their file\" {\n    var scan: Scan = attempt keep_scan()\n    var export: PersonalDataExport = Member.export(scan.member_id)\n    assert export.records == 2\n    assert text.contains(export.json, \"/files/sheets/\")\n    assert not text.contains(export.json, \"\\\"key\\\"\")\n}\n".to_string(),
        ),
    ];
    plain
        .into_iter()
        .map(|(path, text)| (path.to_string(), text))
        .collect()
}

fn project(name: &str, files: &[(String, String)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("jadpo-downloads-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let at = root.join(path);
        fs::create_dir_all(at.parent().expect("parent")).expect("folders");
        fs::write(at, text).expect("write");
    }
    root
}

/// The store folder a project's `app.jadpo` names; never inside the
/// project, which `jadpo check` would refuse.
fn store_folder(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "jadpo-downloads-store-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn downloads_pass_their_tests() {
    if !jadpo_test::live() {
        return;
    }
    let store = store_folder("interpreted");
    let files = files(&store.display().to_string());
    let root = project("interpreted", &files);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let records = jadpo(&root, &["records", "update"]);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&store);
}

/// The headers a download's real answer is checked for.
pub(crate) const HEADERS: [&str; 10] = [
    "content-type",
    "content-length",
    "content-range",
    "content-disposition",
    "x-content-type-options",
    "content-security-policy",
    "accept-ranges",
    "etag",
    "cache-control",
    "allow",
];

type Got = (u16, Vec<(String, String)>, Vec<u8>);

/// How a request reaches a server: its method, path and headers, and what
/// came back.
type Send<'a> = &'a dyn Fn(&str, &str, &[(&str, &str)]) -> Got;

fn header<'a>(got: &'a Got, name: &str) -> &'a str {
    got.1
        .iter()
        .find(|(header, _)| header == name)
        .map_or("", |(_, value)| value)
}

/// The link in the file a route answered with, `answer` its JSON, and
/// each way a client may ask for it: over `send`, real HTTP to a real disk
/// store, whole and by range, with the headers a hostile file needs, and
/// each hostile request refused.
pub(crate) fn serves_a_file(answer: &serde_json::Value, send: Send<'_>, store: &Path) {
    const BYTES: &[u8] = b"code\nA1\nB2\n";
    let sheet = &answer["sheet"];
    assert_eq!(sheet["size"], serde_json::json!(BYTES.len()), "{answer}");
    assert_eq!(sheet["type"], "text/csv", "{answer}");
    assert!(
        sheet.get("store").is_none() && sheet.get("key").is_none(),
        "{answer}"
    );
    let url = sheet["url"].as_str().expect("a url").to_string();
    assert!(url.starts_with("/files/sheets/"), "{url}");

    // Whole, with the headers a file from the app's own origin needs.
    let whole = send("GET", &url, &[]);
    assert_eq!(whole.0, 200, "{whole:?}");
    assert_eq!(whole.2, BYTES, "{whole:?}");
    assert_eq!(header(&whole, "content-type"), "text/csv");
    assert_eq!(header(&whole, "x-content-type-options"), "nosniff");
    assert_eq!(header(&whole, "content-security-policy"), "sandbox");
    assert_eq!(header(&whole, "accept-ranges"), "bytes");
    assert_eq!(
        header(&whole, "content-disposition"),
        "attachment; filename*=UTF-8''download.csv"
    );
    let etag = header(&whole, "etag").to_string();
    assert!(etag.starts_with('"') && etag.len() > 10, "{etag}");

    // HEAD is the same without a body, and reads no range.
    let head = send("HEAD", &url, &[("Range", "bytes=0-3")]);
    assert_eq!(head.0, 200, "{head:?}");
    assert_eq!(head.2, b"", "{head:?}");
    assert_eq!(header(&head, "content-length"), BYTES.len().to_string());

    // A range, and If-Range.
    let part = send("GET", &url, &[("Range", "bytes=0-3")]);
    assert_eq!(part.0, 206, "{part:?}");
    assert_eq!(part.2, b"code", "{part:?}");
    assert_eq!(
        header(&part, "content-range"),
        format!("bytes 0-3/{}", BYTES.len())
    );
    let same = send("GET", &url, &[("Range", "bytes=0-3"), ("If-Range", &etag)]);
    assert_eq!(same.0, 206, "{same:?}");
    let changed = send(
        "GET",
        &url,
        &[("Range", "bytes=0-3"), ("If-Range", "\"other\"")],
    );
    assert_eq!(
        changed.0, 200,
        "a different file is sent whole: {changed:?}"
    );
    assert_eq!(changed.2, BYTES);

    // What isn't one well-formed range of bytes is the whole file once.
    for odd in [
        "bytes=0-3,1-2",
        "bytes=0-1,0-1,0-1,0-1,0-1,0-1",
        "bytes=abc",
        "bytes 0-3",
        "items=0-3",
        "bytes=3-1",
        "bytes=-",
        "bytes=99999999999999999999999-",
    ] {
        let got = send("GET", &url, &[("Range", odd)]);
        assert_eq!(got.0, 200, "{odd}: {got:?}");
        assert_eq!(got.2, BYTES, "{odd}");
    }
    // A well-formed range past the end is refused, saying the file's size.
    for past in ["bytes=999-", "bytes=-0"] {
        let got = send("GET", &url, &[("Range", past)]);
        assert_eq!(got.0, 416, "{past}: {got:?}");
        assert_eq!(
            header(&got, "content-range"),
            format!("bytes */{}", BYTES.len())
        );
    }

    // An altered link names nothing, however it was altered.
    let (head_part, tail_part) = url.rsplit_once('.').expect("a link is two parts");
    let flipped = format!("{head_part}.{}", tail_part.replace(|c: char| c != 'A', "A"));
    let altered = send("GET", &flipped, &[]);
    assert_eq!(altered.0, 403, "{altered:?}");
    let body = String::from_utf8_lossy(&altered.2).to_string();
    assert!(body.contains("link_invalid"), "{body}");
    for escape in [
        "/files/sheets/..%2f..%2fetc%2fpasswd",
        "/files/sheets/%2e%2e%2f%2e%2e%2fetc%2fpasswd",
        "/files/sheets/link-key",
        "/files/sheets/.",
    ] {
        let got = send("GET", escape, &[]);
        assert_eq!(got.0, 403, "{escape}: {got:?}");
    }
    // A link good for one store means nothing in another, which has signed
    // none yet and so isn't a download at all.
    let elsewhere = url.replacen("/files/sheets/", "/files/full/", 1);
    let got = send("GET", &elsewhere, &[]);
    assert_eq!(got.0, 404, "{got:?}");

    // Its signing key is the store's own, and is the app's user's alone.
    let kept = store.join("link-key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::metadata(&kept).expect("the store kept a key");
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }
    #[cfg(not(unix))]
    assert!(kept.exists());
    assert_eq!(fs::read(&kept).expect("read").len(), 32);
}
