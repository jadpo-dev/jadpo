//! Area 6's aggregate queries in the checker (chunk 5.6c): what each
//! aggregate and group takes, the result they fill, the shapes a query
//! with them returns, and its order, each problem reported once.

use crate::support;

use jadpo_db::Layout;

const ENTITY: &str = "type Title = Text {\n    max_length: 80\n}\n\ntype Leaves = Int {\n    min: 1\n}\n\ntype Weight = Decimal {\n    scale: 2\n}\n\ntype ShelfMark = Enum {\n    fiction\n    history\n}\n\ntype Fine = Money {\n    currency: Currency.gbp\n}\n\ntype AnyFine = Money {}\n\nentity Book {\n    id: Uuid\n    title: Title\n    leaves: Leaves\n    weight: Weight\n    mark: ShelfMark\n    added_on: CalendarDate\n    added_at: Instant\n    fine: Fine\n    any_fine: AnyFine\n    sequel: Leaves?\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{ENTITY}{source}"))],
    )
}

const NONE: &[&str] = &[];

/// A grouped query of `entries` into an output of `fields`, ordered by
/// `order`.
fn grouped(fields: &str, group: &str, aggregate: &str, order: &str) -> String {
    format!(
        "type Row = Output {{\n{fields}}}\n\nquery rows() -> List<Row> {{\n    from: Book\n    group: {{\n{group}    }}\n    aggregate: {{\n{aggregate}    }}\n    order: [{order}]\n    limit: 10\n}}\n"
    )
}

/// An ungrouped query of `aggregate` into an output of `fields`.
fn totals(fields: &str, aggregate: &str) -> String {
    format!(
        "type Row = Output {{\n{fields}}}\n\nquery rows() -> Row {{\n    from: Book\n    aggregate: {{\n{aggregate}    }}\n}}\n"
    )
}

#[test]
fn each_aggregate_gives_its_type() {
    assert_eq!(
        rules(&grouped(
            "    mark: ShelfMark\n    books: Int\n    leaves: Int\n    weight: Decimal\n    fines: Money\n    thinnest: Book.leaves\n    first_title: Book.title\n    mean_leaves: Decimal\n    mean_weight: Decimal\n    sequels: Book.sequel\n",
            "        mark: Book.mark\n",
            "        books: count()\n        leaves: sum(Book.leaves)\n        weight: sum(Book.weight)\n        fines: sum(Book.fine)\n        thinnest: min(Book.leaves)\n        first_title: min(Book.title)\n        mean_leaves: average(Book.leaves, scale: 2, rounding: Rounding.half_even)\n        mean_weight: average(Book.weight, scale: 2, rounding: Rounding.down)\n        sequels: max(Book.sequel)\n",
            "books desc",
        )),
        NONE
    );
    // Without groups, `min`, `max` and `average` may be `none`.
    assert_eq!(
        rules(&totals(
            "    books: Int\n    thinnest: Book.leaves?\n    mean_leaves: Decimal?\n",
            "        books: count()\n        thinnest: min(Book.leaves)\n        mean_leaves: average(Book.leaves, scale: 0, rounding: Rounding.up)\n",
        )),
        NONE
    );
    assert_eq!(
        rules(&totals(
            "    thinnest: Book.leaves\n",
            "        thinnest: min(Book.leaves)\n"
        )),
        ["query.result_field"],
        "no books have no thinnest"
    );
    // A sum is the representation: `Int`, not the field's named type.
    assert_eq!(
        rules(&totals(
            "    leaves: Book.leaves\n",
            "        leaves: sum(Book.leaves)\n"
        )),
        ["query.result_field"]
    );
}

#[test]
fn what_each_aggregate_takes() {
    let one = |aggregate: &str| rules(&totals("    total: Int\n", aggregate));
    assert_eq!(
        one("        total: sum(Book.title)\n"),
        ["query.aggregate_field"]
    );
    assert_eq!(
        one("        total: sum(Book.any_fine)\n"),
        ["query.aggregate_field"],
        "money of no fixed currency"
    );
    assert_eq!(
        one("        total: max(Book.mark)\n"),
        ["query.aggregate_field"],
        "an enum has no order the database shares"
    );
    assert_eq!(
        one("        total: min(Book.id)\n"),
        ["query.aggregate_field"]
    );
    assert_eq!(
        one("        total: average(Book.weight, scale: 1, rounding: Rounding.up)\n"),
        ["query.aggregate_field"],
        "fewer places than the field keeps"
    );
    assert_eq!(
        one("        total: count(Book.leaves)\n"),
        ["query.aggregate_argument"]
    );
    assert_eq!(one("        total: sum()\n"), ["query.aggregate_argument"]);
    assert_eq!(
        one("        total: average(Book.leaves, scale: 2)\n"),
        ["query.aggregate_argument"]
    );
    assert_eq!(
        one("        total: average(Book.leaves, scale: 39, rounding: Rounding.up)\n"),
        ["query.aggregate_argument"]
    );
    assert_eq!(
        one("        total: average(Book.leaves, scale: 2, rounding: Rounding.floor)\n"),
        ["query.aggregate_argument"]
    );
    assert_eq!(
        one("        total: total(Book.leaves)\n"),
        ["query.aggregate_unknown"]
    );
    assert_eq!(
        one("        total: list.sum(Book.leaves)\n"),
        ["query.aggregate_unknown"]
    );
    assert_eq!(
        one("        total: Book.leaves\n"),
        ["query.aggregate_unknown"]
    );
}

#[test]
fn what_a_group_takes() {
    let one = |group: &str| {
        rules(&grouped(
            "    key: Int\n    books: Int\n",
            group,
            "        books: count()\n",
            "books desc",
        ))
    };
    assert_eq!(one("        key: temporal.year(Book.added_on)\n"), NONE);
    assert_eq!(one("        key: temporal.day(Book.added_on)\n"), NONE);
    assert_eq!(
        one("        key: temporal.year(Book.added_at)\n"),
        ["query.group_key"],
        "an instant has a year only in a zone"
    );
    assert_eq!(
        one("        key: temporal.weekday(Book.added_on)\n"),
        ["query.group_key"]
    );
    assert_eq!(one("        key: Book.weight\n"), ["query.group_key"]);
    assert_eq!(one("        key: Book.fine\n"), ["query.group_key"]);
    assert_eq!(one("        key: Book.added_at\n"), ["query.group_key"]);
    // A group that may be `none` fills a field that may be.
    assert_eq!(
        rules(&grouped(
            "    key: Book.sequel\n    books: Int\n",
            "        key: Book.sequel\n",
            "        books: count()\n",
            "key asc",
        )),
        NONE
    );
    assert_eq!(
        rules(&grouped(
            "    key: Leaves\n    books: Int\n",
            "        key: Book.sequel\n",
            "        books: count()\n",
            "key asc",
        )),
        ["query.result_field"]
    );
}

#[test]
fn the_result_holds_exactly_the_groups_and_aggregates() {
    assert_eq!(
        rules(&grouped(
            "    mark: ShelfMark\n    books: Int\n    title: Book.title\n",
            "        mark: Book.mark\n",
            "        books: count()\n",
            "books desc",
        )),
        ["query.result_field"],
        "a field of the entity has no one value per group"
    );
    assert_eq!(
        rules(&grouped(
            "    mark: ShelfMark\n",
            "        mark: Book.mark\n",
            "        books: count()\n",
            "mark asc",
        )),
        ["query.result_field"],
        "nowhere to put `books`"
    );
    assert_eq!(
        rules(&grouped(
            "    mark: ShelfMark\n    books: Int\n",
            "        mark: Book.mark\n",
            "        mark: count()\n        books: count()\n",
            "books desc",
        )),
        ["query.result_field"],
        "`mark` given twice"
    );
    assert_eq!(
        rules(
            "query rows() -> Book {\n    from: Book\n    aggregate: {\n        books: count()\n    }\n}\n"
        ),
        ["query.result_type"]
    );
}

#[test]
fn the_shapes_an_aggregate_query_returns() {
    let output = "type Row = Output {\n    books: Int\n}\n\n";
    assert_eq!(
        rules(&format!(
            "{output}query rows() -> List<Row> {{\n    from: Book\n    aggregate: {{\n        books: count()\n    }}\n}}\n"
        )),
        ["query.section_required"],
        "a list needs groups"
    );
    assert_eq!(
        rules(&format!(
            "{output}query rows() -> Row? {{\n    from: Book\n    aggregate: {{\n        books: count()\n    }}\n}}\n"
        )),
        ["query.section_unexpected"],
        "the one row is always there"
    );
    assert_eq!(
        rules(&format!(
            "{output}query rows() -> Row {{\n    from: Book\n    aggregate: {{\n        books: count()\n    }}\n    limit: 5\n}}\n"
        )),
        ["query.section_unexpected"]
    );
    assert_eq!(
        rules(
            "type Row = Output {\n    mark: ShelfMark\n}\n\nquery rows() -> Row {\n    from: Book\n    group: {\n        mark: Book.mark\n    }\n}\n"
        ),
        ["query.section_unexpected"],
        "groups are a list"
    );
    assert_eq!(
        rules(
            "type Row = Output {\n    mark: ShelfMark\n    books: Int\n}\n\nquery rows() -> List<Row> {\n    from: Book\n    group: {\n        mark: Book.mark\n    }\n    aggregate: {\n        books: count()\n    }\n    limit: 5\n}\n"
        ),
        ["query.section_required"],
        "a list is ordered"
    );
}

#[test]
fn a_grouped_query_is_ordered_by_its_entries() {
    let order = |order: &str| {
        rules(&grouped(
            "    mark: ShelfMark\n    books: Int\n    fines: Money\n",
            "        mark: Book.mark\n",
            "        books: count()\n        fines: sum(Book.fine)\n",
            order,
        ))
    };
    assert_eq!(order("mark asc"), NONE, "a group sorts in Jadpo's order");
    assert_eq!(order("books desc, mark desc"), NONE);
    assert_eq!(order("Book.title asc"), ["query.order_field"]);
    assert_eq!(order("bookz desc"), ["query.order_field"]);
    assert_eq!(
        order("fines desc"),
        ["query.order_field"],
        "money has no order"
    );
    assert_eq!(order("books"), ["query.order_direction"]);
    assert_eq!(order("books desc, books asc"), ["query.order_field"]);
}
