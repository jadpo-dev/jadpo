//! Conditions and orders on a field kept in several columns or whole as
//! its JSON (area 5, "A stored value is checked when it is read" and
//! "Structured values"; area 6, "Filters"; area 19, "SQL literals"): a
//! standard type in named columns is compared part for part and ordered
//! where its columns sort as it does, money within one currency; a value
//! kept as JSON is compared as the JSON the codec writes, where that is one
//! spelling for each value, and never ordered.

use crate::support;

use jadpo_db::{Database, Layout};

const NONE: &[&str] = &[];

/// A shop's orders, with a field of each way a value is kept, and the
/// types a query's parameters take.
const SHOP: &str = r#"type CakePrice = Money {
    currency: Currency.gbp
    min: 0
}

type EuroPrice = Money {
    currency: Currency.eur
}

type Deposit = Money {}

type Tag = Text {
    max_length: 20
}

type PlacedAt = ZonedTime {}

type Heft = Decimal {
    scale: 2
}

type Label = Object {
    line: Tag
    note: Tag?
}

type Measured = Object {
    share: Percentage
}

type Weighed = Object {
    heft: Heft
}

type Delivery = Enum {
    pickup
    courier: {
        company: Tag
    }
}

entity Order {
    id: Uuid
    total: CakePrice
    deposit: Deposit?
    month: YearMonth
    opens: LocalTime
    placed: PlacedAt
    nights: DateRange
    version: SemanticVersion
    address: PostalAddress?
    zone: Circle
    tags: List<Tag>
    label: Label
    delivery: Delivery?
    notes: RichText?
    measured: Measured
    weighed: Weighed
}

"#;

/// The rules a query of orders with `lines` and `order` gives.
fn query(lines: &str, order: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[(
            "main.jadpo",
            &format!(
                "{SHOP}query found(total: CakePrice, euros: EuroPrice, deposit: Deposit, month: YearMonth, opens: LocalTime, placed: PlacedAt, nights: DateRange, version: SemanticVersion, address: PostalAddress, zone: Circle, tags: List<Tag>, label: Label, notes: RichText, measured: Measured, weighed: Weighed, prices: List<CakePrice>, months: List<YearMonth>, addresses: List<PostalAddress>, labels: List<Label>, maybe: CakePrice?) -> List<Order> {{\n    from: Order\n{lines}    order: [{order}]\n    limit: 20\n}}\n"
            ),
        )],
    )
}

/// The rules a query of orders with `line` as its one condition gives.
fn condition(line: &str) -> Vec<String> {
    query(&format!("    {line}\n"), "Order.id asc")
}

#[test]
fn a_standard_type_in_named_columns_is_compared_part_for_part() {
    for line in [
        "where: Order.total == total",
        "where: Order.total != total",
        "where: Order.total in [total, total]",
        "where: Order.total in prices",
        "where: not (Order.total in prices)",
        "where: Order.deposit == deposit",
        "where: Order.deposit == none",
        "where: Order.deposit != none",
        "where: Order.month == month",
        "where: Order.month in months",
        "where: Order.opens == opens",
        "where: Order.placed == placed",
        "where: Order.nights == nights",
        "where: Order.version == version",
        "where: Order.address == address",
        "where: Order.address in [address]",
        "where: Order.address == none or Order.total == total",
        "where_present: Order.total == maybe",
    ] {
        assert_eq!(condition(line), NONE, "{line}");
    }
}

#[test]
fn a_type_ordered_by_its_columns_is_ordered_by_them() {
    for line in [
        "where: Order.month < month",
        "where: Order.opens > opens",
        "where: not (Order.month <= month)",
        // Amounts of money whose types fix one currency.
        "where: Order.total < total",
        "where: Order.total >= total",
    ] {
        assert_eq!(condition(line), NONE, "{line}");
    }
    for order in [
        "Order.month desc",
        "Order.opens asc",
        "Order.total desc",
        "Order.month asc, Order.total desc",
    ] {
        assert_eq!(query("", order), NONE, "{order}");
    }
}

#[test]
fn money_is_ordered_only_within_one_currency() {
    assert_eq!(
        condition("where: Order.deposit < deposit"),
        ["query.condition_unsupported"],
        "the field's type fixes no currency"
    );
    assert_eq!(
        condition("where: Order.total < deposit"),
        ["type.mismatch"],
        "two types of money, as code refuses them"
    );
    assert_eq!(
        condition("where: Order.total < euros"),
        ["type.mismatch"],
        "two currencies, each fixed"
    );
    assert_eq!(
        query("", "Order.deposit asc"),
        ["query.order_field"],
        "records in several currencies"
    );
    // Two types of money are compared only where one widens to the other.
    assert_eq!(condition("where: Order.total == euros"), ["type.mismatch"]);
}

#[test]
fn a_type_with_no_order_of_its_columns_is_not_ordered() {
    for line in [
        "where: Order.placed < placed",
        "where: Order.nights < nights",
        "where: Order.version < version",
    ] {
        assert_eq!(condition(line), ["query.condition_unsupported"], "{line}");
    }
    for order in ["Order.placed asc", "Order.version asc", "Order.nights asc"] {
        assert_eq!(query("", order), ["query.order_field"], "{order}");
    }
}

#[test]
fn what_the_database_cant_compare_as_code_does_is_refused() {
    assert_eq!(
        condition("where: Order.zone == zone"),
        ["query.condition_unsupported"],
        "a geometry, which PostGIS compares by its bounding box"
    );
    assert_eq!(
        condition("where: Order.address in addresses"),
        ["query.condition_unsupported"],
        "a list given as a parameter, whose parts may be `none`"
    );
}

#[test]
fn a_value_kept_as_json_is_compared_as_its_json() {
    for line in [
        "where: Order.tags == tags",
        "where: Order.tags != tags",
        "where: Order.label == label",
        "where: Order.label in labels",
        "where: Order.label in [label, label]",
        "where: Order.delivery == Delivery.pickup",
        "where: Order.delivery != Delivery.pickup",
        "where: Order.delivery == none",
        "where: Order.notes == notes",
        "where: Order.weighed == weighed",
        "where: list.count(Order.tags) >= 2",
    ] {
        assert_eq!(condition(line), NONE, "{line}");
    }
    for (line, why) in [
        ("where: Order.tags < tags", "JSON has no order"),
        (
            "where: Order.measured == measured",
            "a percentage is written with its places",
        ),
        (
            "where: Order.tags == [\"fragile\"]",
            "a list written out isn't written by the codec",
        ),
    ] {
        assert_eq!(condition(line), ["query.condition_unsupported"], "{why}");
    }
    assert_eq!(query("", "Order.tags asc"), ["query.order_field"]);
}

#[test]
fn an_amount_of_money_s_operations_stand_where_it_does() {
    for line in [
        "where: money.minor(Order.total) >= 1000",
        "where: money.minor(Order.deposit) < 500",
        "where: money.currency(Order.deposit) == Currency.eur",
    ] {
        assert_eq!(condition(line), NONE, "{line}");
    }
}

/// The indexes `found`'s conditions derive, by their columns.
fn indexes(lines: &str) -> Vec<Vec<String>> {
    let db = Database::default();
    let source = format!(
        "{SHOP}query found(total: CakePrice, tags: List<Tag>) -> List<Order> {{\n    from: Order\n{lines}    order: [Order.id asc]\n    limit: 20\n}}\n"
    );
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let problems = jadpo_check::check(&db, project);
    assert!(problems.is_empty(), "{problems:#?}");
    let schema = jadpo_check::data::schema(&db, project);
    let table = &schema.stores["main"].tables["order"];
    table
        .indexes
        .values()
        .filter(|index| index.reason == "query found")
        .map(|index| index.columns.clone())
        .collect()
}

#[test]
fn an_index_holds_every_column_compared_and_no_json() {
    assert_eq!(
        indexes("    where: Order.total == total\n"),
        [vec![
            "total_amount".to_string(),
            "total_currency".to_string(),
            "id".to_string()
        ]]
    );
    // A B-tree's entry holds about 2.7 kB, and a stored value up to 1 MiB.
    assert_eq!(
        indexes("    where: Order.tags == tags\n"),
        Vec::<Vec<String>>::new()
    );
}
