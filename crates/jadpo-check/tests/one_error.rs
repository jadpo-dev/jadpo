//! One mistake, one error (docs/compiler-runtime.md §2.2): a type name
//! that names nothing, written anywhere outside a body (an entity's field,
//! a reference, a lifecycle's values, an input's field, a query's result,
//! a type's copying line), is reported once where it is written, and what
//! is built from it absorbs it, so no rule that reads it reports a second.
//! Each case misspells one name in the library-loans teaching example.

use std::path::{Path, PathBuf};

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/10-library-loans")
}

/// Copies the example into a folder of its own, leaving out `build/`.
fn copy(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("example folder") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        if name == "build" {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type().expect("type").is_dir() {
            std::fs::create_dir_all(&target).expect("folder");
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// The example with `from` replaced by `to` in `file`, checked: each
/// problem as `rule: message`.
fn problems(case: &str, file: &str, from: &str, to: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("one-error-{case}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("folder");
    copy(&example(), &root);
    let path = root.join(file);
    let text = std::fs::read_to_string(&path).expect("file");
    assert_eq!(text.matches(from).count(), 1, "{from} in {file}");
    std::fs::write(&path, text.replace(from, to)).expect("write");
    let loaded = jadpo_db::load(&root);
    jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

#[test]
fn the_example_checks_cleanly_as_it_is() {
    let loaded = jadpo_db::load(&example());
    let found: Vec<String> = jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

/// Richard's repro: every literal passed as `Member.email`, the conflict
/// arm naming its unique rule and the query reading by it used to report
/// errors of their own, 19 in all.
#[test]
fn an_unknown_type_in_an_entitys_field_is_one_error() {
    let found = problems(
        "entity-field",
        "domains/lending/entities/member.jadpo",
        "email: Email {",
        "email: Emadil {",
    );
    assert_eq!(found, ["semantic.unknown_name: `Emadil` isn't declared"]);
    let db_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("one-error-entity-field");
    let loaded = jadpo_db::load(&db_root);
    let problem = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .next()
        .expect("the problem");
    assert!(
        problem
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("Did you mean `Email`?")),
        "{problem:?}"
    );
    let primary = problem.primary.expect("a place");
    assert_eq!(primary.span.path, "domains/lending/entities/member.jadpo");
}

#[test]
fn an_unknown_type_anywhere_outside_a_body_is_one_error() {
    let cases = [
        (
            "reference",
            "domains/lending/entities/card.jadpo",
            "member_id: Member.id {",
            "member_id: Membr.id {",
            "semantic.unknown_name: `Membr` isn't declared",
        ),
        (
            "lifecycle-initial",
            "domains/lending/entities/loan.jadpo",
            "            status: LoanStatus.out\n",
            "            status: LoanStats.out\n",
            "semantic.unknown_name: `LoanStats` isn't declared",
        ),
        (
            "lifecycle-set",
            "domains/lending/entities/loan.jadpo",
            "                    status: LoanStatus.returned\n",
            "                    status: LoanStatu.returned\n",
            "semantic.unknown_name: `LoanStatu` isn't declared",
        ),
        (
            "plain-field",
            "domains/lending/entities/book.jadpo",
            "    wear: Wear\n",
            "    wear: Waer\n",
            "semantic.unknown_name: `Waer` isn't declared",
        ),
        (
            "unknown-field-of-a-reference",
            "domains/lending/entities/loan.jadpo",
            "vouched_by: Member.id? {",
            "vouched_by: Member.idd? {",
            "type.unknown_field: `Member` has no field `idd`",
        ),
        (
            "input-field",
            "domains/lending/types/catalogue_filter.jadpo",
            "    wear: Wear {",
            "    wear: Waer {",
            "semantic.unknown_name: `Waer` isn't declared",
        ),
        (
            "query-result",
            "domains/lending/queries/browse_books.jadpo",
            "-> Page<BookView> {",
            "-> Page<BokView> {",
            "semantic.unknown_name: `BokView` isn't declared",
        ),
        (
            "copying-line",
            "domains/lending/types/book_view.jadpo",
            "Book.wear]",
            "Bok.wear]",
            "semantic.unknown_name: `Bok` isn't declared",
        ),
    ];
    for (case, file, from, to, expected) in cases {
        assert_eq!(problems(case, file, from, to), [expected], "{case}");
    }
}
