//! Standard operations in `where:` (area 6, "Operations in conditions";
//! chunk 6.3b): an operation area 19 gives an exact SQL form stands where
//! a field does, the field its first argument and every other argument
//! written out, and the compiler derives an index on its expression; an
//! operation without a form, or whose form an index can't hold, is
//! refused, as is one given a value the query is called with.

use crate::support;

use jadpo_check::data::queries::{Call, Compare, Condition, Test, Value};
use jadpo_db::{Database, Layout};
use jadpo_std::Op;

const NONE: &[&str] = &[];

/// A book with a field of each kind an operation reads, and the types a
/// query's parameters take.
const BOOK: &str = "type Title = Text {\n    max_length: 80\n}\n\ntype Pages = Int {\n    min: 1\n    max: 5000\n}\n\ntype ShelfPrice = Decimal {\n    scale: 2\n}\n\ntype Count = Int {}\n\ntype CoverHash = Bytes {}\n\nentity Book {\n    id: Uuid\n    title: Title\n    subtitle: Title?\n    pages: Pages\n    price: ShelfPrice\n    published_on: CalendarDate\n    cover: Bytes\n    sold: Money\n}\n\ntype Filter = Input {\n    longest: Count {\n        optional: true\n    }\n}\n\n";

/// A query of books with `lines` as its conditions.
fn source(lines: &str) -> String {
    format!(
        "{BOOK}query found(title: Book.title, n: Count, year: Count, digest: CoverHash, filter: Filter) -> List<Book> {{\n    from: Book\n{lines}    order: [Book.id asc]\n    limit: 20\n}}\n"
    )
}

fn query(lines: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", &source(lines))])
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

#[test]
fn an_operation_with_an_exact_form_stands_where_a_field_does() {
    for line in [
        "    where: text.length(Book.title) <= 24\n",
        "    where: temporal.year(Book.published_on) == year\n",
        "    where: text.keep_first(Book.title, 3) == title\n",
        "    where: text.length(Book.title) in [1, 2, 3]\n",
        // A `Bool` is the condition itself, `not` and `or` included,
        // since the database works the operation out exactly.
        "    where: text.starts_with(Book.title, \"The \")\n",
        "    where: not text.ends_with(Book.title, \"!\")\n",
        "    where: text.length(Book.title) > 3 or Book.pages > 100\n",
        // Options written out, a whole number and a rounding mode.
        "    where: integer.divide(Book.pages, 100, rounding: Rounding.down) == n\n",
        "    where: decimal.round(Book.price, 0, rounding: Rounding.half_even) <= 10.0\n",
        "    where: text.pad_start(Book.title, 5, fill: \"0\") == title\n",
        // A field that may be `none`, and bytes.
        "    where: text.length(Book.subtitle) < 10\n",
        "    where: encoding.to_hex(Book.cover) == title\n",
        // A value that may be absent, on a `where_present:` line.
        "    where_present: text.length(Book.title) <= filter.longest\n",
    ] {
        assert_eq!(query(line), NONE, "{line}");
    }
}

#[test]
fn the_model_holds_the_operation_and_its_test() {
    assert_eq!(
        filter("    where: text.length(Book.title) <= 24\n"),
        [Condition::Call {
            call: Call {
                op: Op::TextLength,
                field: "title".to_string(),
                args: Vec::new(),
                result: jadpo_sql::form::Held::Builtin(jadpo_std::Builtin::Int),
            },
            test: Test::Compare {
                op: Compare::LessEqual,
                value: Value::Literal(jadpo_std::Literal::Number {
                    value: jadpo_std::Num::parse("24").expect("a number"),
                    decimal: false,
                }),
            },
        }]
    );
    assert!(matches!(
        filter("    where: text.starts_with(Book.title, \"The \")\n").as_slice(),
        [Condition::Call {
            call: Call {
                op: Op::TextStartsWith,
                ..
            },
            test: Test::Holds,
        }]
    ));
}

#[test]
fn an_operation_without_an_exact_form_is_refused() {
    for (line, why) in [
        (
            "    where: text.lower(Book.title) == title\n",
            "no SQL form: its result would follow the collation",
        ),
        (
            "    where: hash.sha256(Book.title) == digest\n",
            "a form through the database's encoding, which no index holds",
        ),
        (
            "    where: text.keep_first(Book.title, n) == title\n",
            "a value the query is called with, which no index holds",
        ),
        (
            "    where: text.length(text.trim(Book.title)) > 3\n",
            "an operation of another operation's result",
        ),
        (
            "    where: text.contains(title, Book.title)\n",
            "the field not first",
        ),
        (
            "    where: text.starts_with(Book.title, \"a\") > true\n",
            "a `Bool` isn't ordered",
        ),
    ] {
        assert_eq!(query(line), ["query.condition_unsupported"], "{why}");
    }
    assert_eq!(
        query("    where: money.minor(Book.sold) > 100\n"),
        ["toolchain.not_built_yet"],
        "a field in several columns isn't compared in a query yet"
    );
}

#[test]
fn an_operation_s_arguments_are_typed_as_in_code() {
    assert_eq!(
        query("    where: text.length(Book.pages) > 3\n"),
        ["type.mismatch"],
        "a whole number given for text"
    );
    assert_eq!(
        query("    where: text.length(Book.title) == title\n"),
        ["type.mismatch"],
        "a length compared with a title"
    );
    assert_eq!(
        query("    where: text.keep_first(Book.title, -1) == title\n"),
        ["type.argument_negative"]
    );
    assert_eq!(
        query("    where: integer.divide(Book.pages, 0, rounding: Rounding.down) == n\n"),
        ["type.divisor_zero"]
    );
    assert_eq!(
        query("    where: text.replace(Book.title, \"\", \"x\") == title\n"),
        ["type.argument_empty"]
    );
    assert_eq!(
        query("    where: text.length(Book.title) == none\n"),
        ["type.incomparable"],
        "a length is never `none`"
    );
}

#[test]
fn each_operation_derives_an_index_on_its_expression() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            &format!(
                "{BOOK}/// Short titles.\nquery short() -> List<Book> {{\n    from: Book\n    where: text.length(Book.title) <= 24\n    order: [Book.id asc]\n    limit: 20\n}}\n\n/// Shorter titles, by the same expression.\nquery shorter() -> List<Book> {{\n    from: Book\n    where: text.length(Book.title) <= 12\n    order: [Book.id asc]\n    limit: 20\n}}\n\n/// A year's books.\nquery of_year(year: Count) -> List<Book> {{\n    from: Book\n    where: temporal.year(Book.published_on) == year\n    order: [Book.id asc]\n    limit: 20\n}}\n"
            ),
        )],
    );
    let problems = jadpo_check::check(&db, project);
    assert!(problems.is_empty(), "{problems:#?}");
    let schema = jadpo_check::data::schema(&db, project);
    let table = &schema.stores["main"].tables["book"];
    let expressions: Vec<(String, Vec<String>, String)> = table
        .indexes
        .iter()
        .filter_map(|(name, index)| {
            Some((
                name.clone(),
                index.columns.clone(),
                index.expression.clone()?,
            ))
        })
        .collect();
    assert_eq!(
        expressions,
        [
            (
                "book_published_on_year_idx".to_string(),
                vec!["published_on".to_string()],
                "(CASE WHEN extract(year FROM \"published_on\") < 0 THEN extract(year FROM \"published_on\") + 1 ELSE extract(year FROM \"published_on\") END)::bigint".to_string()
            ),
            (
                "book_title_length_idx".to_string(),
                vec!["title".to_string()],
                "char_length(\"title\")::bigint".to_string()
            ),
        ],
        "one index for each expression, however many queries test it"
    );
}

#[test]
fn an_operation_stands_in_an_include_and_an_aggregate_query() {
    // An include's list, whose records the database decides exactly.
    assert_eq!(
        support::rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                &format!(
                    "{BOOK}entity Chapter {{\n    id: Uuid\n    book_id: Book.id {{\n        on_delete: OnDelete.cascade\n    }}\n    heading: Title\n}}\n\ntype ChapterView = Output {{\n    pick: [Chapter.id, Chapter.heading]\n}}\n\ntype BookChapters = Output {{\n    pick: [Book.id]\n    chapters: List<ChapterView>\n}}\n\n/// A book's short chapters.\nquery short_chapters(book_id: Book.id) -> BookChapters? {{\n    from: Book\n    where: Book.id == book_id\n    include: {{\n        chapters: {{\n            via: Chapter.book_id\n            where: text.length(Chapter.heading) <= 12\n            order: [Chapter.id asc]\n            limit: 10\n        }}\n    }}\n}}\n"
                ),
            )]
        ),
        NONE
    );
    // An aggregate query, whose totals the database works out.
    assert_eq!(
        support::rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                &format!(
                    "{BOOK}type ShortTitles = Output {{\n    books: Int\n}}\n\n/// How many titles fit on a spine.\nquery spine_count() -> ShortTitles {{\n    from: Book\n    where: text.length(Book.title) <= 24\n    aggregate: {{\n        books: count()\n    }}\n}}\n"
                ),
            )]
        ),
        NONE
    );
}
