//! Conditions and orders on fields kept in several columns or whole as
//! their JSON, against live PostgreSQL (area 5, "A stored value is checked
//! when it is read"; area 6, "Filters" and "Order and pages"; area 19,
//! "SQL literals"): a bakery's orders, each query run through `jadpo test`
//! and checked against what code's `==`, `<` and `list.filter` keep over
//! the same records:
//!
//! - an amount of money compared by both its columns, amounts in another
//!   currency unequal, and ordered and paged by its minor units where its
//!   type fixes the currency;
//! - a `YearMonth` compared and ordered by its two columns in turn, and
//!   tested `in` a list given as a parameter;
//! - `none` meeting no comparison but `!=`, `not` keeping that meaning, a
//!   `where_present:` line applying only when its value is there;
//! - a list, an object, an enum with data and a `LocalisedText` compared
//!   as the JSON the codec keeps, a variant without data among them and a
//!   map built in another order, and a list counted through `list.count`;
//! - a `DateRange` compared as PostgreSQL's `daterange`;
//! - each condition's statement and the index it derives, through `jadpo
//!   sql`.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live test.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const TYPES: &[(&str, &str)] = &[
    (
        "cake_price",
        "/// What a cake costs, in pounds.\ntype CakePrice = Money {\n    currency: Currency.gbp\n    min: 0\n}\n",
    ),
    (
        "deposit",
        "/// Money paid ahead, in the customer's currency.\ntype Deposit = Money {}\n",
    ),
    (
        "tag",
        "/// A word an order is filed under.\ntype Tag = Text {\n    max_length: 20\n}\n",
    ),
    (
        "label",
        "use bakery/types { Tag }\n\n/// What is iced on a cake.\ntype Label = Object {\n    line: Tag\n    note: Tag?\n}\n",
    ),
    (
        "delivery",
        "use bakery/types { Tag }\n\n/// How a cake leaves the bakery.\ntype Delivery = Enum {\n    pickup\n    courier: {\n        company: Tag\n    }\n}\n",
    ),
    (
        "order_card",
        "use bakery/entities { Order }\n\n/// An order in a list.\ntype OrderCard = Output {\n    pick: [Order.id, Order.number]\n}\n",
    ),
    (
        "order_number",
        "/// The number on an order's ticket.\ntype OrderNumber = Int {\n    min: 1\n    max: 1000\n}\n",
    ),
    (
        "order_filter",
        "use bakery/types { CakePrice }\n\n/// What a list of orders may be narrowed by.\ntype OrderFilter = Input {\n    total: CakePrice {\n        optional: true\n    }\n}\n",
    ),
];

const ENTITY: &str = r#"use bakery/types { CakePrice, Delivery, Deposit, Label, OrderNumber, Tag }

/// A cake ordered for collection or delivery.
entity Order {
    id: Uuid
    number: OrderNumber
    total: CakePrice
    deposit: Deposit?
    month: YearMonth
    tags: List<Tag>
    label: Label
    delivery: Delivery?
    name: LocalisedText
    pickup_days: DateRange

    policy: {
        read: [Access.public]
    }
}
"#;

const PLACE: &str = r#"use bakery/entities { Order }
use bakery/types { CakePrice, Delivery, Deposit, Label, OrderNumber, Tag }

/// Takes an order.
action place(
    number: OrderNumber,
    total: CakePrice,
    deposit: Deposit?,
    month: YearMonth,
    tags: List<Tag>,
    label: Label,
    delivery: Delivery?,
    name: LocalisedText,
    pickup_days: DateRange
) -> Order {
    return Order.create {
        set: {
            number: number
            total: total
            deposit: deposit
            month: month
            tags: tags
            label: label
            delivery: delivery
            name: name
            pickup_days: pickup_days
        }
    }
}
"#;

/// Each query: its name, parameters, lines and order.
const QUERIES: &[(&str, &str, &str, &str)] = &[
    ("everything", "", "", "Order.number asc"),
    (
        "costing",
        "total: CakePrice",
        "where: Order.total == total",
        "Order.number asc",
    ),
    (
        "not_costing",
        "total: CakePrice",
        "where: Order.total != total",
        "Order.number asc",
    ),
    (
        "dearer",
        "than: CakePrice",
        "where: Order.total > than",
        "Order.total desc",
    ),
    (
        "not_dearer",
        "than: CakePrice",
        "where: not (Order.total > than)",
        "Order.total asc, Order.number asc",
    ),
    (
        "deposited",
        "deposit: Deposit",
        "where: Order.deposit == deposit",
        "Order.number asc",
    ),
    (
        "not_deposited",
        "deposit: Deposit",
        "where: Order.deposit != deposit",
        "Order.number asc",
    ),
    (
        "no_deposit",
        "",
        "where: Order.deposit == none",
        "Order.number asc",
    ),
    (
        "in_euros",
        "",
        "where: money.currency(Order.deposit) == Currency.eur",
        "Order.number asc",
    ),
    (
        "large_deposit",
        "",
        "where: money.minor(Order.deposit) >= 1000",
        "Order.number asc",
    ),
    (
        "from_month",
        "month: YearMonth",
        "where: Order.month >= month",
        "Order.month asc, Order.number asc",
    ),
    (
        "before",
        "month: YearMonth",
        "where: Order.month < month",
        "Order.month desc, Order.number asc",
    ),
    (
        "in_months",
        "months: List<YearMonth>",
        "where: Order.month in months",
        "Order.number asc",
    ),
    (
        "tagged",
        "tags: List<Tag>",
        "where: Order.tags == tags",
        "Order.number asc",
    ),
    (
        "many_tags",
        "",
        "where: list.count(Order.tags) >= 2",
        "Order.number asc",
    ),
    (
        "labelled",
        "label: Label",
        "where: Order.label == label",
        "Order.number asc",
    ),
    (
        "labelled_any",
        "labels: List<Label>",
        "where: Order.label in labels",
        "Order.number asc",
    ),
    (
        "collected",
        "",
        "where: Order.delivery == Delivery.pickup",
        "Order.number asc",
    ),
    (
        "not_collected",
        "",
        "where: Order.delivery != Delivery.pickup",
        "Order.number asc",
    ),
    (
        "named",
        "name: LocalisedText",
        "where: Order.name == name",
        "Order.number asc",
    ),
    (
        "collected_over",
        "days: DateRange",
        "where: Order.pickup_days == days",
        "Order.number asc",
    ),
    (
        "narrowed",
        "filter: OrderFilter",
        "where_present: Order.total == filter.total",
        "Order.number asc",
    ),
];

/// A bakery's orders, each query above, and the tests that hold each one
/// to code.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("kept-conditions-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    let domain = |path: &str, text: &str| write(&root, &format!("domains/bakery/{path}"), text);
    for (file, text) in TYPES {
        domain(&format!("types/{file}.jadpo"), text);
    }
    domain("entities/order.jadpo", ENTITY);
    domain("actions/place.jadpo", PLACE);
    domain("actions/open_bakery.jadpo", OPEN);
    for (name, params, condition, order) in QUERIES {
        domain(
            &format!("queries/{name}.jadpo"),
            &format!(
                "use bakery/entities {{ Order }}\nuse bakery/types {{ CakePrice, Delivery, Deposit, Label, OrderCard, OrderFilter, Tag }}\n\n/// Orders found by a held value.\nquery {name}({params}) -> List<OrderCard> {{\n    from: Order\n    {condition}\n    order: [{order}]\n    limit: 100\n}}\n"
            ),
        );
    }
    domain(
        "queries/by_price.jadpo",
        "use bakery/entities { Order }\nuse bakery/types { OrderCard }\n\n/// Orders a page at a time, cheapest first.\nquery by_price(after: PageCursor?) -> Page<OrderCard> {\n    from: Order\n    order: [Order.total asc, Order.number asc]\n    page: {\n        size: 2\n        after: after\n    }\n}\n",
    );
    domain("actions/place.test.jadpo", TESTS);
    let formatted = jadpo(&root, &["fmt"]);
    assert_eq!(
        formatted.code, 0,
        "{}{}",
        formatted.stdout, formatted.stderr
    );
    for args in [&["records", "update"][..], &["migrate", "new", "initial"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

/// The tests: each query's records, by number, beside what code keeps of
/// the six orders `open_bakery` places.
const TESTS: &str = r#"use bakery/actions { open_bakery }
use bakery/entities { Order }
use bakery/queries { before, by_price, collected, collected_over, costing, dearer, deposited, everything, from_month, in_euros, in_months, labelled, labelled_any, large_deposit, many_tags, named, narrowed, no_deposit, not_collected, not_costing, not_dearer, not_deposited, tagged }
use bakery/types { CakePrice, Deposit, Label, OrderCard, OrderFilter, Tag }

test "an amount of money is compared by its minor units and its currency" {
    var placed: List<Order.id> = open_bakery()
    assert list.count(placed) == 6
    assert list.map(everything(), card -> card.number) == [1, 2, 3, 4, 5, 6]
    var price: CakePrice = CakePrice(money.from_minor(4250, Currency.gbp))
    assert list.map(costing(price), card -> card.number) == [1, 2]
    assert list.map(not_costing(price), card -> card.number) == [3, 4, 5, 6]
    assert list.map(dearer(price), card -> card.number) == [6, 4]
    assert list.map(not_dearer(price), card -> card.number) == [5, 3, 1, 2]
    var pounds: Deposit = Deposit(money.from_minor(1000, Currency.gbp))
    // £10 and €10 are unequal; an order with no deposit meets `!=`.
    assert list.map(deposited(pounds), card -> card.number) == [1]
    assert list.map(not_deposited(pounds), card -> card.number) == [2, 3, 4, 5, 6]
    assert list.map(no_deposit(), card -> card.number) == [3, 5]
    assert list.map(in_euros(), card -> card.number) == [2, 4]
    assert list.map(large_deposit(), card -> card.number) == [1, 2, 6]
}

test "a page of orders by price continues after its cursor's minor units" {
    var placed: List<Order.id> = open_bakery()
    assert list.count(placed) == 6
    var first: Page<OrderCard> = by_price(none)
    assert list.map(first.items, card -> card.number) == [5, 3]
    var second: Page<OrderCard> = by_price(first.next)
    assert list.map(second.items, card -> card.number) == [1, 2]
    var third: Page<OrderCard> = by_price(second.next)
    assert list.map(third.items, card -> card.number) == [4, 6]
    assert third.next == none
}

test "a month is compared and ordered by its year then its month" {
    var placed: List<Order.id> = open_bakery()
    assert list.count(placed) == 6
    assert list.map(from_month("2026-12"), card -> card.number) == [2, 6, 3, 4, 5]
    assert list.map(before("2027-03"), card -> card.number) == [4, 3, 2, 6, 1]
    assert list.map(in_months(["2026-11", "2027-02"]), card -> card.number) == [1, 4]
    assert list.map(in_months([]), card -> card.number) == []
}

test "a value kept as JSON is compared as code compares it" {
    var placed: List<Order.id> = open_bakery()
    assert list.count(placed) == 6
    var birthday: Tag = "birthday"
    var chocolate: Tag = "chocolate"
    assert list.map(tagged([birthday]), card -> card.number) == [1, 5]
    // A list keeps its order: `[chocolate, birthday]` isn't `[birthday, chocolate]`.
    assert list.map(tagged([birthday, chocolate]), card -> card.number) == [2]
    assert list.map(tagged([]), card -> card.number) == [3]
    assert list.map(many_tags(), card -> card.number) == [2, 4]
    var plain: Label = Label {
        line: "Happy birthday"
        note: none
    }
    var noted: Label = Label {
        line: "Happy birthday"
        note: "in blue"
    }
    assert list.map(labelled(plain), card -> card.number) == [1, 3, 5]
    assert list.map(labelled_any([noted]), card -> card.number) == [2, 4, 6]
    assert list.map(collected(), card -> card.number) == [1, 4]
    // A map's keys are kept in their canonical order, however it was built.
    var french: LocalisedText = LocalisedText(map.with([], Locale.fr_fr, "Pain de seigle"))
    var bilingual: LocalisedText = LocalisedText(map.with(french, Locale.en_gb, "Rye loaf"))
    assert list.map(named(bilingual), card -> card.number) == [1, 3, 6]
    assert list.map(named(french), card -> card.number) == [4]
    var christmas: DateRange = DateRange {
        start: "2026-12-24"
        end: "2026-12-26"
    }
    assert list.map(collected_over(christmas), card -> card.number) == [1, 4, 5]
    // An order with no delivery meets `!=`, as `none != Delivery.pickup`.
    assert list.map(not_collected(), card -> card.number) == [2, 3, 5, 6]
}

test "a where_present line applies only when its value is there" {
    var placed: List<Order.id> = open_bakery()
    assert list.count(placed) == 6
    assert list.map(narrowed(OrderFilter {}), card -> card.number) == [1, 2, 3, 4, 5, 6]
    var price: CakePrice = CakePrice(money.from_minor(9900, Currency.gbp))
    var filter: OrderFilter = OrderFilter {
        total: price
    }
    assert list.map(narrowed(filter), card -> card.number) == [4]
}
"#;

/// Six orders: totals around £42.50, deposits in pounds and euros and
/// none, months either side of New Year, tags, labels, deliveries, names
/// in one language or two, and the days they are collected over.
const OPEN: &str = r#"use bakery/actions { place }
use bakery/entities { Order }
use bakery/types { CakePrice, Delivery, Deposit, Label }

/// Places the six orders the tests read.
action open_bakery() -> List<Order.id> {
    var plain: Label = Label {
        line: "Happy birthday"
        note: none
    }
    var noted: Label = Label {
        line: "Happy birthday"
        note: "in blue"
    }
    var swift: Delivery = Delivery.courier {
        company: "Swift"
    }
    var french: LocalisedText = LocalisedText(map.with([], Locale.fr_fr, "Pain de seigle"))
    var english: LocalisedText = LocalisedText(map.with([], Locale.en_gb, "Rye loaf"))
    var both: LocalisedText = LocalisedText(map.with(english, Locale.fr_fr, "Pain de seigle"))
    var christmas: DateRange = DateRange {
        start: "2026-12-24"
        end: "2026-12-26"
    }
    var new_year: DateRange = DateRange {
        start: "2026-12-31"
        end: "2027-01-01"
    }
    var one: Order = place(1, CakePrice(money.from_minor(4250, Currency.gbp)), Deposit(money.from_minor(1000, Currency.gbp)), "2026-11", ["birthday"], plain, Delivery.pickup, both, christmas)
    var two: Order = place(2, CakePrice(money.from_minor(4250, Currency.gbp)), Deposit(money.from_minor(1000, Currency.eur)), "2026-12", ["birthday", "chocolate"], noted, swift, english, new_year)
    var three: Order = place(3, CakePrice(money.from_minor(4249, Currency.gbp)), none, "2027-01", [], plain, none, both, new_year)
    var four: Order = place(4, CakePrice(money.from_minor(9900, Currency.gbp)), Deposit(money.from_minor(500, Currency.eur)), "2027-02", ["chocolate", "birthday"], noted, Delivery.pickup, french, christmas)
    var five: Order = place(5, CakePrice(money.from_minor(0, Currency.gbp)), none, "2027-03", ["birthday"], plain, swift, english, christmas)
    var six: Order = place(6, CakePrice(money.from_minor(12000, Currency.gbp)), Deposit(money.from_minor(2000, Currency.gbp)), "2026-12", ["chocolate"], noted, none, both, new_year)
    return [one.id, two.id, three.id, four.id, five.id, six.id]
}
"#;

#[test]
fn each_condition_on_a_held_value_gives_what_code_gives() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests");
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("5 passed"), "{}", tested.stdout);
}

#[test]
fn each_statement_compares_the_columns_a_value_is_kept_in() {
    let root = project("sql");
    for (query, wanted) in [
        (
            "costing",
            "WHERE (\"total_amount\" = $1 AND \"total_currency\" = $2)",
        ),
        (
            "not_deposited",
            "WHERE (\"deposit_amount\" IS NULL OR NOT (\"deposit_amount\" = $1 AND \"deposit_currency\" = $2))",
        ),
        ("dearer", "WHERE \"total_amount\" > $1"),
        ("no_deposit", "WHERE \"deposit_amount\" IS NULL"),
        (
            "from_month",
            "WHERE (\"month_year\", \"month_month\") >= ($1, $2)",
        ),
        ("tagged", "WHERE \"tags\" = $1"),
        ("labelled_any", "WHERE \"label\" = ANY($1)"),
        (
            "in_months",
            "EXISTS (SELECT 1 FROM unnest($1, $2) AS \"held\"(\"p0\", \"p1\") WHERE \"month_year\" = \"held\".\"p0\" AND \"month_month\" = \"held\".\"p1\")",
        ),
    ] {
        let shown = jadpo(&root, &["sql", query]);
        assert_eq!(shown.code, 0, "{query}: {}{}", shown.stdout, shown.stderr);
        assert!(shown.stdout.contains(wanted), "{query}: {}", shown.stdout);
    }
    let folder = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("a migration");
    let sql = fs::read_to_string(folder.join("postgres.sql")).expect("postgres.sql");
    for index in [
        "ON \"order\" (\"total_amount\", \"total_currency\", \"number\", \"id\")",
        "ON \"order\" (\"month_year\", \"month_month\", \"number\", \"id\")",
        // Sorted by its minor units alone.
        "ON \"order\" (\"total_amount\", \"id\")",
        "ON \"order\" ((jsonb_array_length(\"tags\")::bigint))",
    ] {
        assert!(sql.contains(index), "{index}\n{sql}");
    }
    // A value kept as JSON is in no derived index.
    assert!(!sql.contains("ON \"order\" (\"tags\""), "{sql}");
}
