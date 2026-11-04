//! A route that sends raw bytes and an answer that never changes (area 7,
//! "Sending raw bytes" and "Answers that never change"; chunk G3), end to
//! end: route tests read the field back in `jadpo test` here and compiled
//! beside the interpreter in `compiled.rs`, and on workerd in `workers/`;
//! against a real server, the body to the byte with its media type and
//! length, a query or a read-only action giving it, `HEAD`, failures still
//! in JSON, a repeat under its key, R4's conformance check on the document,
//! and an API change showing as a break.

use std::fs;
use std::path::{Path, PathBuf};

use crate::http::{Dev, jadpo};

/// A digest, as the registry's protocol writes one.
pub const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000abc";

/// Bytes that aren't text: a NUL, a lone continuation byte and an invalid
/// lead byte.
pub const RAW: [u8; 8] = [0x00, 0x80, 0xff, b't', b'a', b'r', 0xc3, 0x28];

/// A registry's archives: bytes kept by digest, sent back raw.
pub fn files() -> Vec<(String, String)> {
    [
        ("app.jadpo", "application {}\n"),
        (
            "policy.jadpo",
            "use registry\n\npolicy {\n    actions: {\n        registry.add_archive: [Access.public]\n        registry.worked_out: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/registry/types/archive_digest.jadpo",
            "/// A release's digest.\ntype ArchiveDigest = Text {\n    pattern: \"^sha256:[0-9a-f]{64}$\"\n}\n",
        ),
        (
            "domains/registry/types/archive.jadpo",
            "/// A release's files, as a tar archive.\ntype Archive = Output {\n    tar: Bytes\n}\n",
        ),
        (
            "domains/registry/types/new_archive.jadpo",
            "use registry/types { ArchiveDigest }\n\n/// An archive to keep.\ntype NewArchive = Input {\n    digest: ArchiveDigest\n    tar: Bytes\n}\n",
        ),
        (
            "domains/registry/entities/release.jadpo",
            "use registry/types { ArchiveDigest }\n\n/// A release.\nentity Release {\n    id: Uuid\n    digest: ArchiveDigest {\n        unique: true\n    }\n    tar: Bytes\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/registry/failures/archive_not_found.jadpo",
            "/// No release has this digest.\ntype ArchiveNotFound = Failure {\n    kind: FailureKind.not_found\n    message: \"No release has this digest.\"\n}\n",
        ),
        (
            "domains/registry/failures/archive_exists.jadpo",
            "/// A release already has this digest.\ntype ArchiveExists = Failure {\n    kind: FailureKind.conflict\n    message: \"A release already has this digest.\"\n}\n",
        ),
        (
            "domains/registry/queries/archive_of.jadpo",
            "use registry/entities { Release }\nuse registry/failures { ArchiveNotFound }\nuse registry/types { Archive, ArchiveDigest }\n\n/// A release's archive.\nquery archive_of(digest: ArchiveDigest) fails ArchiveNotFound -> Archive {\n    from: Release\n    where: Release.digest == digest\n    missing: ArchiveNotFound\n}\n",
        ),
        (
            "domains/registry/actions/worked_out.jadpo",
            "use registry/failures { ArchiveNotFound }\nuse registry/queries { archive_of }\nuse registry/types { Archive, ArchiveDigest }\n\n/// A release's archive, through an action that only reads.\naction worked_out(digest: ArchiveDigest) fails ArchiveNotFound -> Archive {\n    return attempt archive_of(digest)\n}\n",
        ),
        (
            "domains/registry/actions/add_archive.jadpo",
            "use registry/entities { Release }\nuse registry/failures { ArchiveExists }\nuse registry/types { Archive, NewArchive }\n\n/// Keeps an archive, and gives it back.\naction add_archive(input: NewArchive) fails ArchiveExists -> Archive {\n    var kept: Release = attempt Release.create {\n        set: {\n            digest: input.digest\n            tar: input.tar\n        }\n        conflict: {\n            Release.digest: ArchiveExists\n        }\n    }\n    return Archive {\n        tar: kept.tar\n    }\n}\n",
        ),
        (
            "domains/registry/routes/registry_v1.jadpo",
            "use registry/actions { add_archive, worked_out }\nuse registry/queries { archive_of }\nuse registry/types { Archive, ArchiveDigest, NewArchive }\n\n/// Archives, kept by digest.\nroutes RegistryV1 {\n    prefix: /registry/v1\n\n    /// A release's files, which a digest fixes for good.\n    GET /archives/{digest: ArchiveDigest} {\n        authentication: RouteAuthentication.none\n        output: Archive\n        sends: \"application/x-tar\"\n        cache: immutable\n        run: archive_of(digest)\n    }\n\n    /// The same, through an action, in another type and not kept.\n    GET /worked/{digest: ArchiveDigest} {\n        authentication: RouteAuthentication.none\n        output: Archive\n        sends: \"application/gzip\"\n        run: worked_out(digest)\n    }\n\n    /// The same as JSON, kept.\n    GET /as_json/{digest: ArchiveDigest} {\n        authentication: RouteAuthentication.none\n        output: Archive\n        cache: immutable\n        run: archive_of(digest)\n    }\n\n    /// Keeps an archive and sends it back.\n    POST /archives {\n        authentication: RouteAuthentication.none\n        input: NewArchive\n        output: Archive\n        sends: \"application/x-tar\"\n        status: SuccessStatus.created\n        run: add_archive(input)\n    }\n}\n",
        ),
        (
            "domains/registry/routes/registry_v1.test.jadpo",
            "use registry/types { Archive, NewArchive }\n\ntest \"what is sent reads back as its field\" {\n    var made: Response = request {\n        method: HttpMethod.post\n        path: \"/registry/v1/archives\"\n        input: NewArchive {\n            digest: \"sha256:0000000000000000000000000000000000000000000000000000000000000abc\"\n            tar: hash.sha256(\"the files\")\n        }\n    }\n    assert made.status == 201\n    assert made.output == Archive {\n        tar: hash.sha256(\"the files\")\n    }\n    var read: Response = request {\n        method: HttpMethod.get\n        path: \"/registry/v1/archives/{digest}\"\n        params: {\n            digest: \"sha256:0000000000000000000000000000000000000000000000000000000000000abc\"\n        }\n    }\n    assert read.status == 200\n    assert read.output == Archive {\n        tar: hash.sha256(\"the files\")\n    }\n    var worked: Response = request {\n        method: HttpMethod.get\n        path: \"/registry/v1/worked/{digest}\"\n        params: {\n            digest: \"sha256:0000000000000000000000000000000000000000000000000000000000000abc\"\n        }\n    }\n    assert worked.status == 200\n    assert worked.output == Archive {\n        tar: hash.sha256(\"the files\")\n    }\n}\n\ntest \"a failure still has its code\" {\n    var missing: Response = request {\n        method: HttpMethod.get\n        path: \"/registry/v1/archives/{digest}\"\n        params: {\n            digest: \"sha256:1111111111111111111111111111111111111111111111111111111111111111\"\n        }\n    }\n    assert missing.status == 404\n    assert missing.code == \"archive_not_found\"\n}\n",
        ),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect()
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sends-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// What adding an archive posts: its digest and its bytes, as JSON.
pub fn posted(digest: &str, bytes: &[u8]) -> String {
    serde_json::json!({
        "digest": digest,
        "tar": jadpo_value::encoding::to_base64(bytes),
    })
    .to_string()
}

#[test]
fn route_tests_read_what_is_sent_as_its_field() {
    if std::env::var_os("JADPO_SKIP_POSTGRES").is_some() {
        return;
    }
    let root = project("tests");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
}

#[test]
fn a_real_server_sends_the_field_to_the_byte() {
    if std::env::var_os("JADPO_SKIP_POSTGRES").is_some() {
        return;
    }
    let root = project("dev");
    let dev = Dev::start(&root);
    let path = format!("/registry/v1/archives/{DIGEST}");

    // A write sends it too, with its status and no cache's header.
    let keyed = [("Idempotency-Key", "first")];
    let made = dev.send(
        "POST",
        "/registry/v1/archives",
        &keyed,
        &posted(DIGEST, &RAW),
    );
    assert_eq!(made.status, 201, "{made:?}");
    assert_eq!(made.bytes(), RAW, "{made:?}");
    assert_eq!(made.header("content-type"), Some("application/x-tar"));
    assert_eq!(made.header("content-length"), Some("8"));
    assert_eq!(made.header("cache-control"), None);

    // A repeat under its key gets the first answer, to the byte.
    let again = dev.send(
        "POST",
        "/registry/v1/archives",
        &keyed,
        &posted(DIGEST, &RAW),
    );
    assert_eq!(again.status, 201, "{again:?}");
    assert_eq!(again.bytes(), RAW, "{again:?}");
    assert_eq!(again.header("content-type"), Some("application/x-tar"));
    assert_eq!(again.header("content-length"), Some("8"));
    assert_eq!(again.header("x-content-type-options"), Some("nosniff"));

    // From a query, kept for good.
    let read = dev.send("GET", &path, &[], "");
    assert_eq!(read.status, 200, "{read:?}");
    assert_eq!(read.bytes(), RAW, "{read:?}");
    assert_eq!(read.header("content-type"), Some("application/x-tar"));
    assert_eq!(read.header("content-length"), Some("8"));
    assert_eq!(read.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(
        read.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert!(read.header("x-request-id").is_some(), "{read:?}");

    // `HEAD`: the same head, its length too, and no body.
    let head = dev.send("HEAD", &path, &[], "");
    assert_eq!(head.status, 200, "{head:?}");
    assert_eq!(head.bytes(), b"", "{head:?}");
    assert_eq!(head.header("content-type"), Some("application/x-tar"));
    assert_eq!(head.header("content-length"), Some("8"));
    assert_eq!(
        head.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );

    // From a read-only action, as another type, and not kept.
    let worked = dev.send("GET", &format!("/registry/v1/worked/{DIGEST}"), &[], "");
    assert_eq!(worked.status, 200, "{worked:?}");
    assert_eq!(worked.bytes(), RAW, "{worked:?}");
    assert_eq!(worked.header("content-type"), Some("application/gzip"));
    assert_eq!(worked.header("cache-control"), None);

    // Kept for good as JSON: `cache:` doesn't need `sends:`.
    let json = dev.send("GET", &format!("/registry/v1/as_json/{DIGEST}"), &[], "");
    assert_eq!(json.status, 200, "{json:?}");
    assert_eq!(json.header("content-type"), Some("application/json"));
    assert_eq!(json.json()["tar"], jadpo_value::encoding::to_base64(&RAW));
    assert_eq!(
        json.header("cache-control"),
        Some("public, max-age=31536000, immutable")
    );

    // A failure stays JSON and is never kept: the release may come.
    let other = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    let missing = dev.send("GET", &format!("/registry/v1/archives/{other}"), &[], "");
    assert_eq!(missing.status, 404, "{missing:?}");
    assert_eq!(missing.header("content-type"), Some("application/json"));
    assert_eq!(missing.header("cache-control"), None);
    assert_eq!(missing.json()["error"]["code"], "archive_not_found");
    let bad = dev.send("GET", "/registry/v1/archives/nope", &[], "");
    assert_eq!(bad.status, 422, "{bad:?}");
    assert_eq!(bad.header("content-type"), Some("application/json"));
    assert_eq!(bad.header("cache-control"), None);

    // The served document passes R4's check on this route.
    let served = dev.send("GET", "/openapi.json", &[], "");
    assert_eq!(served.status, 200, "{served:?}");
    assert_eq!(
        archive_mismatches(&served.json()),
        Vec::<String>::new(),
        "{}",
        served.text()
    );
}

/// What R4's conformance check says of the archive's route.
fn archive_mismatches(document: &serde_json::Value) -> Vec<String> {
    jadpo_registry::conformance::check(document)
        .iter()
        .filter(|mismatch| mismatch.operation == "GET /registry/v1/archives/{digest}")
        .map(ToString::to_string)
        .collect()
}

#[test]
fn r4s_check_says_a_json_answer_is_wrong_and_this_one_is_right() {
    let root = project("conformance");
    let document = |root: &Path| -> serde_json::Value {
        let text = fs::read_to_string(root.join("build/openapi/main.json")).expect("written");
        serde_json::from_str(&text).expect("JSON")
    };
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    assert_eq!(archive_mismatches(&document(&root)), Vec::<String>::new());

    // Without `sends:`, it answers JSON, which is what the check caught.
    let group = root.join("domains/registry/routes/registry_v1.jadpo");
    let text = fs::read_to_string(&group).expect("read");
    let without = text.replacen("        sends: \"application/x-tar\"\n", "", 1);
    assert_ne!(text, without);
    fs::write(&group, without).expect("write");
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let found = archive_mismatches(&document(&root));
    assert!(
        found
            .iter()
            .any(|mismatch| mismatch.contains("answers application/x-tar with 200")),
        "{found:#?}"
    );
}

#[test]
fn another_media_type_is_a_break_a_person_accepts() {
    use crate::acceptances::{edit, git};

    let root = project("break");
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The registry"]);
    git(&root, &["checkout", "-q", "-b", "change"]);
    // Taking `sends:` away and sending another type both show in the record.
    edit(
        &root,
        "domains/registry/routes/registry_v1.jadpo",
        "sends: \"application/gzip\"",
        "sends: \"application/zstd\"",
    );
    edit(
        &root,
        "domains/registry/routes/registry_v1.jadpo",
        "        sends: \"application/x-tar\"\n        cache: immutable\n",
        "",
    );
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    let record = fs::read_to_string(root.join("api/main.jadpo_record")).expect("the record");
    assert!(record.contains("sends application/zstd"), "{record}");

    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("changing what a route sends can break clients"),
        "{}",
        checked.stdout
    );
    let drafted = jadpo(&root, &["acceptance", "new", "another_type"]);
    assert_eq!(drafted.code, 0, "{}{}", drafted.stdout, drafted.stderr);
    let folder = fs::read_dir(root.join("acceptances")).expect("the folder");
    let draft = folder.flatten().next().expect("a draft").path();
    let text = fs::read_to_string(&draft).expect("read");
    for route in ["archives", "worked"] {
        assert!(
            text.lines()
                .any(|line| line.contains(&format!("GET /registry/v1/{route}/"))
                    && line.ends_with("ApiChange.media_type_changed")),
            "{text}"
        );
    }
}
