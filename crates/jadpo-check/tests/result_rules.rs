//! A query's result type with rules across its fields (area 6, chunk
//! 5.8c): a value of it would be made from stored records without its
//! rules being checked, and a read has no `attempt` to map a broken one,
//! so such a result type is refused while compiling, wherever it stands:
//! the records themselves, a `Nearest<T>`'s item, a map layer's
//! properties, an aggregate's rows, or an include's records.

use crate::support;

use jadpo_db::Layout;

const ENTITY: &str = "type PageCount = Int {
    min: 1
    max: 5000
}

type ShelfName = Text {
    max_length: 40
}

entity Book {
    id: Uuid
    shelf: ShelfName
    pages: PageCount
    shelf_id: Shelf.id {
        on_delete: OnDelete.cascade
    }
}

entity Shelf {
    id: Uuid
    name: ShelfName
}

";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{ENTITY}{source}"))],
    )
}

fn problems(source: &str) -> Vec<String> {
    support::problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{ENTITY}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_result_type_with_a_rule_is_refused() {
    let found = problems(
        "type ThickBook = Output {
    pick: [Book.id, Book.pages]
    rule thick: pages > 1000
}

query thick_books() -> List<ThickBook> {
    from: Book
    order: [Book.id asc]
    limit: 10
}
",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with(
            "main.jadpo query.result_rule: `thick_books` returns `ThickBook`, whose rule `thick` no read can check"
        ),
        "{found:?}"
    );
    // The same type without its rule is a result as any.
    assert_eq!(
        rules(
            "type ThickBook = Output {
    pick: [Book.id, Book.pages]
}

query thick_books() -> List<ThickBook> {
    from: Book
    order: [Book.id asc]
    limit: 10
}
"
        ),
        NONE
    );
}

#[test]
fn every_shape_of_result_is_held_to_it() {
    // One record, or none.
    assert_eq!(
        rules(
            "type Pages = Output {
    pick: [Book.id, Book.pages]
    rule some: pages > 0
}

query one_book(id: Book.id) -> Pages? {
    from: Book
    where: Book.id == id
}
"
        ),
        ["query.result_rule"]
    );
    // An aggregate's rows.
    assert_eq!(
        rules(
            "type ShelfSpan = Output {
    shelf: Book.shelf
    thinnest: Book.pages
    thickest: Book.pages
    rule thin_first: thinnest <= thickest
}

query shelf_spans() -> List<ShelfSpan> {
    from: Book
    group: {
        shelf: Book.shelf
    }
    aggregate: {
        thinnest: min(Book.pages)
        thickest: max(Book.pages)
    }
    order: [shelf asc]
    limit: 10
}
"
        ),
        ["query.result_rule"]
    );
    // An include's records.
    assert_eq!(
        rules(
            "type BookLine = Output {
    pick: [Book.id, Book.pages]
    rule some: pages > 0
}

type ShelfView = Output {
    pick: [Shelf.id, Shelf.name]
    books: List<BookLine>
}

query shelf_view(id: Shelf.id) -> ShelfView? {
    from: Shelf
    where: Shelf.id == id
    include: {
        books: {
            via: Book.shelf_id
            order: [Book.id asc]
            limit: 20
        }
    }
}
"
        ),
        ["query.result_rule"]
    );
}

#[test]
fn a_type_with_rules_is_still_built_where_they_are_checked() {
    // An `Output` with a rule stays a route's or an action's result: it is
    // built by an object literal, which checks it.
    assert_eq!(
        rules(
            "type Span = Output {
    thinnest: PageCount
    thickest: PageCount
    rule thin_first: thinnest <= thickest
}

type SpanBackwards = Failure {
    kind: FailureKind.invalid_value
}

function span(a: PageCount, b: PageCount) fails SpanBackwards -> Span {
    var made: Span = attempt Span {
        thinnest: a
        thickest: b
    } {
        thin_first: SpanBackwards
    }
    return made
}
"
        ),
        NONE
    );
}
