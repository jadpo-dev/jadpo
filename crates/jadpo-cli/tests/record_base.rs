//! A careful hand edit of a view told from a stale one (area 18,
//! "Checking a record"): with a base commit to compare with, `jadpo check`
//! builds it, as `jadpo approval` does, and an entry of a view that changed
//! since the base where its source didn't is `record.edited`, however
//! carefully it was written. A record nobody touched since the base, or one
//! a command wrote from source that has moved on again, stays
//! `record.out_of_date`. The base is built only when a view changed since
//! it and disagrees with the source, and kept in `build/before/`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::acceptances::{edit, git, jadpo};
use crate::http::tempdir;
use crate::records::{FILES, RECORD};

const API: &str = "api/main.jadpo_record";

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    std::fs::write(path, text).expect("write");
}

/// The memos project with its records written, committed on `main`, and a
/// branch `change` checked out.
fn committed(stem: &str) -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new(stem);
    let root = dir.path().join("memos");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    write(&root, API, RECORD);
    write(&root, ".gitignore", "build/\n");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "Memos"]);
    git(&root, &["checkout", "-q", "-b", "change"]);
    (dir, root)
}

fn commit(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}

/// Each record problem `jadpo check --json` reports, `rule: message`.
fn problems(root: &Path) -> Vec<String> {
    let output = jadpo(root, &["check", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    // What a change to the routes needs besides is the acceptances' to say.
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter(|d| {
            d["rule"]
                .as_str()
                .is_some_and(|rule| rule.starts_with("record."))
        })
        .map(|d| {
            format!(
                "{}: {}",
                d["rule"].as_str().unwrap_or(""),
                d["message"].as_str().unwrap_or("")
            )
        })
        .collect()
}

/// The base commit's short hash, as the messages name it.
fn base(root: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short=7", "main"])
        .current_dir(root)
        .output()
        .expect("git runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn built_bases(root: &Path) -> usize {
    std::fs::read_dir(root.join("build/before")).map_or(0, |found| {
        found
            .filter_map(Result::ok)
            .filter(|entry| entry.path().join("records.json").exists())
            .count()
    })
}

#[test]
fn a_careful_edit_of_one_fact_is_edited_by_hand() {
    let (_dir, root) = committed("record-base-fact");
    edit(&root, API, "    status 200\n\nGET", "    status 201\n\nGET");
    let at = base(&root);
    assert_eq!(
        problems(&root),
        [format!(
            "record.edited: `api/main.jadpo_record` was edited by hand: `POST /memos` changed since main at {at}, and the routes didn't"
        )]
    );
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 1);
    assert!(
        output.stdout.contains(
            " --> api/main.jadpo_record:8:1\n  |\n8 |     status 201\n  | ^^^^^^^^^^^^^^ not what Jadpo writes\n"
        ),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .contains("  next: Run `jadpo api update`, which writes it again (agent)\n"),
        "{}",
        output.stdout
    );
    // `jadpo records` says the same.
    let listed = jadpo(&root, &["records"]);
    assert!(
        listed
            .stdout
            .contains("  api/main.jadpo_record  edited by hand\n"),
        "{}",
        listed.stdout
    );
    // The command writes it again, and nothing is wrong.
    assert_eq!(jadpo(&root, &["api", "update"]).code, 0);
    assert_eq!(problems(&root), Vec::<String>::new());
}

#[test]
fn without_a_base_the_same_edit_reads_as_out_of_date() {
    let dir = tempdir::Dir::new("record-base-none");
    let root = dir.path().join("memos");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    write(
        &root,
        API,
        &RECORD.replace("status 200\n\nGET", "status 201\n\nGET"),
    );
    assert_eq!(
        problems(&root),
        ["record.out_of_date: `api/main.jadpo_record` is out of date"]
    );
}

#[test]
fn an_entry_removed_or_added_by_hand_is_edited() {
    let (_dir, root) = committed("record-base-entries");
    edit(
        &root,
        API,
        "GET /openapi.json\n    generated openapi\n    authentication none\n    status 200\n\n",
        "",
    );
    let at = base(&root);
    assert_eq!(
        problems(&root),
        [format!(
            "record.edited: `api/main.jadpo_record` was edited by hand: `GET /openapi.json` was removed since main at {at}, and the routes still have it"
        )]
    );
    write(&root, API, RECORD);
    edit(
        &root,
        API,
        "\nPOST /memos\n",
        "\nGET /memos\n    authentication none\n    status 200\n\nPOST /memos\n",
    );
    assert_eq!(
        problems(&root),
        [format!(
            "record.edited: `api/main.jadpo_record` was edited by hand: `GET /memos` was added since main at {at}, and the routes don't have it"
        )]
    );
}

#[test]
fn a_record_untouched_since_the_base_is_out_of_date_and_builds_nothing() {
    let (_dir, root) = committed("record-base-stale");
    edit(
        &root,
        "domains/memos/types/memo.jadpo",
        "max_length: 200",
        "max_length: 300",
    );
    assert_eq!(
        problems(&root),
        ["record.out_of_date: `api/main.jadpo_record` is out of date"]
    );
    assert_eq!(built_bases(&root), 0);
}

#[test]
fn a_record_written_by_its_command_then_left_behind_is_out_of_date() {
    let (_dir, root) = committed("record-base-again");
    let memo = "domains/memos/types/memo.jadpo";
    edit(&root, memo, "max_length: 200", "max_length: 300");
    assert_eq!(jadpo(&root, &["api", "update"]).code, 0);
    commit(&root, "Longer memos");
    edit(&root, memo, "max_length: 300", "max_length: 400");
    assert_eq!(
        problems(&root),
        ["record.out_of_date: `api/main.jadpo_record` is out of date"]
    );
    // The base was built once and kept.
    assert_eq!(built_bases(&root), 1);
    assert_eq!(
        problems(&root),
        ["record.out_of_date: `api/main.jadpo_record` is out of date"]
    );
}

#[test]
fn every_view_is_judged_by_its_base() {
    let (_dir, root) = committed("record-base-access");
    edit(
        &root,
        "access.jadpo_record",
        "who Access.public",
        "who Access.signed_in",
    );
    let at = base(&root);
    assert_eq!(
        problems(&root),
        [format!(
            "record.edited: `access.jadpo_record` was edited by hand: `action memos.add_memo` changed since main at {at}, and the policy didn't"
        )]
    );
}
