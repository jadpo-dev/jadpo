//! A view Jadpo could have written is out of date, never edited by hand
//! (area 18, "Checking a record"; chunk 10.4e): one `jadpo check` wrote
//! from an earlier state of a change reads exactly like one edited with
//! care, so neither is blamed on a person. Outside CI `jadpo check`
//! writes it again; in CI it is `record.out_of_date`. Only a file Jadpo
//! couldn't have written, such as one with a comment, is `record.edited`.
//! Nothing builds the base commit to judge a view.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::acceptances::{edit, git, jadpo};
use crate::http::tempdir;
use crate::records::{FILES, RECORD};

const API: &str = "api/main.jadpo_record";
const MEMO: &str = "domains/memos/types/memo.jadpo";

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

/// Each record problem `jadpo check --json` reports in CI, `rule: message`.
fn problems(root: &Path) -> Vec<String> {
    let output = crate::acceptances::jadpo_ci(root, &["check", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
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

const STALE: &str = "record.out_of_date: `api/main.jadpo_record` is out of date";

/// An agent changes a type, `jadpo check` writes the record from it, and
/// the agent changes its mind: the record holds what the source said a
/// moment ago, which the base never had. In 10.5's first round this read
/// as "edited by hand" and cost a failed run each time.
#[test]
fn a_record_written_from_an_earlier_state_of_the_change_is_stale() {
    let (_dir, root) = committed("record-base-earlier");
    edit(&root, MEMO, "max_length: 200", "max_length: 300");
    let wrote = jadpo(&root, &["check"]);
    assert!(
        wrote.stdout.contains("Wrote api/main.jadpo_record"),
        "{}",
        wrote.stdout
    );
    edit(&root, MEMO, "max_length: 300", "max_length: 200");
    assert_eq!(problems(&root), [STALE]);
    // Outside CI, `jadpo check` writes it again and passes.
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output.stdout.contains("Wrote api/main.jadpo_record"),
        "{}",
        output.stdout
    );
    assert_eq!(
        std::fs::read_to_string(root.join(API)).expect("record"),
        RECORD
    );
    assert!(!root.join("build/before").exists());
}

#[test]
fn a_careful_edit_of_one_fact_or_entry_is_stale_too() {
    let (_dir, root) = committed("record-base-fact");
    edit(&root, API, "    status 200\n\nGET", "    status 201\n\nGET");
    assert_eq!(problems(&root), [STALE]);
    write(&root, API, RECORD);
    edit(
        &root,
        API,
        "GET /openapi.json\n    generated openapi\n    authentication none\n    status 200\n\n",
        "",
    );
    assert_eq!(problems(&root), [STALE]);
    write(&root, API, RECORD);
    edit(
        &root,
        API,
        "\nPOST /memos\n",
        "\nGET /memos\n    authentication none\n    status 200\n\nPOST /memos\n",
    );
    assert_eq!(problems(&root), [STALE]);
    write(&root, API, RECORD);
    edit(
        &root,
        "access.jadpo_record",
        "who Access.public",
        "who Access.signed_in",
    );
    assert_eq!(
        problems(&root),
        ["record.out_of_date: `access.jadpo_record` is out of date"]
    );
}

#[test]
fn a_comment_is_still_edited_by_hand() {
    let (_dir, root) = committed("record-base-comment");
    edit(
        &root,
        API,
        "    status 200\n\nGET",
        "    // Clients rely on 200 here.\n    status 200\n\nGET",
    );
    assert_eq!(
        problems(&root),
        [
            "record.edited: `api/main.jadpo_record` was edited by hand: a comment isn't part of a record"
        ]
    );
}
