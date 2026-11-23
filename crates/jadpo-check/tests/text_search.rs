//! A search inside text (area 6, "Searching inside text"; chunk 10.3):
//! `text.contains_ignoring_case(Entity.field, value)` takes text written
//! out or given when the query runs, on a `where:` line of its own or
//! joined by `and`, and on a `where_present:` line, which applies it only
//! when its text is there; the compiler derives a trigram index on the
//! field. Under `not` or `or`, compared with `== true`, or given anything
//! but text, it is refused, and `text.contains` with a value given when the
//! query runs names it.

use crate::support;

use jadpo_check::data::queries::{Call, CallArg, Condition, Test, Value};
use jadpo_db::{Database, Layout};
use jadpo_std::{Builtin, Op};

const NONE: &[&str] = &[];

/// A book with text fields, a reference and a number, and the input a
/// list of them is narrowed by.
const BOOK: &str = "type Title = Text {\n    max_length: 80\n}\n\ntype Pages = Int {\n    min: 1\n    max: 5000\n}\n\nentity Book {\n    id: Uuid\n    title: Title\n    subtitle: Title?\n    pages: Pages\n}\n\ntype FindBooks = Input {\n    q: Title {\n        optional: true\n    }\n}\n\n";

/// A query of books with `lines` as its conditions.
fn source(lines: &str) -> String {
    format!(
        "{BOOK}query found(title: Book.title, pages: Book.pages, asked: FindBooks) -> List<Book> {{\n    from: Book\n{lines}    order: [Book.id asc]\n    limit: 20\n}}\n"
    )
}

fn query(lines: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", &source(lines))])
}

/// Each problem's message with its `next:`, for the lines given.
fn told(lines: &str) -> Vec<(String, String)> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source(lines))]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            (
                d.rule.id.to_string(),
                d.next
                    .as_ref()
                    .map(|next| next.text.clone())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

/// The query's conditions, as its model holds them.
fn filter(lines: &str) -> Vec<Condition> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source(lines))]);
    let problems = jadpo_check::check(&db, project);
    assert!(problems.is_empty(), "{lines}: {problems:#?}");
    let queries = jadpo_check::data::queries::query_models(&db, project);
    queries.get("found").expect("the query").filter.clone()
}

fn search(value: Value) -> Condition {
    Condition::Call {
        call: Call {
            op: Op::TextContainsIgnoringCase,
            field: "title".to_string(),
            args: vec![CallArg::Value(
                value,
                jadpo_sql::form::Held::Builtin(Builtin::Text),
            )],
            result: jadpo_sql::form::Held::Builtin(Builtin::Bool),
        },
        test: Test::Holds,
    }
}

#[test]
fn a_search_takes_text_given_when_the_query_runs() {
    for line in [
        "    where: text.contains_ignoring_case(Book.title, title)\n",
        "    where: text.contains_ignoring_case(Book.title, \"milk\")\n",
        "    where: text.contains_ignoring_case(Book.subtitle, title)\n",
        "    where: text.contains_ignoring_case(Book.title, title) and Book.pages > pages\n",
        "    where_present: text.contains_ignoring_case(Book.title, asked.q)\n",
        "    where: text.contains_ignoring_case(value: Book.title, part: title)\n",
    ] {
        assert_eq!(query(line), NONE, "{line}");
    }
    let param = |index: u32, path: &[&str]| Value::Param {
        index,
        path: path.iter().map(|p| p.to_string()).collect(),
    };
    assert_eq!(
        filter("    where: text.contains_ignoring_case(Book.title, title)\n"),
        [search(param(0, &[]))]
    );
    assert_eq!(
        filter("    where_present: text.contains_ignoring_case(Book.title, asked.q)\n"),
        [Condition::Present {
            value: param(2, &["q"]),
            condition: Box::new(search(param(2, &["q"]))),
        }]
    );
}

#[test]
fn a_search_stands_only_where_its_index_finds_every_record() {
    for (line, why) in [
        (
            "    where: not text.contains_ignoring_case(Book.title, title)\n",
            "under `not`",
        ),
        (
            "    where: text.contains_ignoring_case(Book.title, title) or Book.pages > pages\n",
            "under `or`",
        ),
        (
            "    where: text.contains_ignoring_case(Book.title, title) == true\n",
            "compared with `true`",
        ),
        (
            "    where: text.contains_ignoring_case(title, Book.title)\n",
            "the field not first",
        ),
        (
            "    where: text.contains_ignoring_case(Book.title, Book.subtitle)\n",
            "a field for its text",
        ),
    ] {
        assert_eq!(query(line), ["query.condition_unsupported"], "{why}");
    }
    assert_eq!(
        query("    where: text.contains_ignoring_case(Book.pages, title)\n"),
        ["type.mismatch"],
        "a whole number searched as text"
    );
    assert_eq!(
        query("    where: text.contains_ignoring_case(Book.title, asked.q)\n"),
        ["query.optional_value"],
        "text that may be absent belongs on a `where_present:` line"
    );
    assert_eq!(
        query("    where_present: text.contains_ignoring_case(Book.title, title)\n"),
        ["query.optional_value"],
        "text that is always there belongs on a `where:` line"
    );
}

#[test]
fn contains_given_text_when_the_query_runs_names_the_search() {
    assert_eq!(
        told("    where: text.contains(Book.title, title)\n"),
        [(
            "query.condition_unsupported".to_string(),
            "Write `text.contains_ignoring_case(Book.title, title)`".to_string()
        )]
    );
    // Written out, it is an operation like any other.
    assert_eq!(
        query("    where: text.contains(Book.title, \"milk\")\n"),
        NONE
    );
}

#[test]
fn a_position_that_may_be_none_isnt_compared() {
    for line in [
        "    where: text.index_of(Book.title, \":\") == 0\n",
        "    where: text.last_index_of(Book.title, \":\") != 3\n",
    ] {
        assert_eq!(query(line), ["query.condition_unsupported"], "{line}");
    }
}

#[test]
fn each_searched_field_derives_one_trigram_index() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            &format!(
                "{BOOK}/// Books by a word of their title.\nquery titled(title: Book.title) -> List<Book> {{\n    from: Book\n    where: text.contains_ignoring_case(Book.title, title)\n    order: [Book.id asc]\n    limit: 20\n}}\n\n/// The same search, narrowed when asked.\nquery maybe_titled(asked: FindBooks) -> List<Book> {{\n    from: Book\n    where_present: text.contains_ignoring_case(Book.title, asked.q)\n    order: [Book.id asc]\n    limit: 20\n}}\n"
            ),
        )],
    );
    let problems = jadpo_check::check(&db, project);
    assert!(problems.is_empty(), "{problems:#?}");
    let schema = jadpo_check::data::schema(&db, project);
    let table = &schema.stores["main"].tables["book"];
    let trigrams: Vec<(String, Option<String>, String, String)> = table
        .indexes
        .iter()
        .filter_map(|(name, index)| {
            let method = index.method.as_ref()?;
            Some((
                name.clone(),
                index.expression.clone(),
                method.name.clone(),
                method.operator_class.clone(),
            ))
        })
        .collect();
    assert_eq!(
        trigrams,
        [(
            "book_title_trgm_idx".to_string(),
            Some("lower(\"title\" COLLATE \"C\")".to_string()),
            "gin".to_string(),
            "gin_trgm_ops".to_string()
        )],
        "one index for the field, however many queries search it"
    );
}

#[test]
fn a_search_stands_in_an_include_and_an_aggregate_query() {
    assert_eq!(
        support::rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                &format!(
                    "{BOOK}entity Chapter {{\n    id: Uuid\n    book_id: Book.id {{\n        on_delete: OnDelete.cascade\n    }}\n    heading: Title\n}}\n\ntype ChapterView = Output {{\n    pick: [Chapter.id, Chapter.heading]\n}}\n\ntype BookChapters = Output {{\n    pick: [Book.id]\n    chapters: List<ChapterView>\n}}\n\n/// A book's chapters whose heading holds the text.\nquery chapters_on(book_id: Book.id, title: Book.title) -> BookChapters? {{\n    from: Book\n    where: Book.id == book_id\n    include: {{\n        chapters: {{\n            via: Chapter.book_id\n            where: text.contains_ignoring_case(Chapter.heading, title)\n            order: [Chapter.id asc]\n            limit: 10\n        }}\n    }}\n}}\n\ntype Matching = Output {{\n    books: Int\n}}\n\n/// How many titles hold the text.\nquery matching(title: Book.title) -> Matching {{\n    from: Book\n    where: text.contains_ignoring_case(Book.title, title)\n    aggregate: {{\n        books: count()\n    }}\n}}\n"
                ),
            )]
        ),
        NONE
    );
}
