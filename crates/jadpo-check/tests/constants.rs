//! Area 3, "Constants" (decided 2026-10-16, chunk 5.3e): a `constant` at
//! the top of a file or in a library, its type named as a signature's is,
//! its value worked out and checked while compiling, read in bodies and
//! queries, placed and shared as helper functions are, and two of one type
//! holding one value warned about.

use crate::support;

use jadpo_db::Layout;
use support::{APP, problems};

fn loose(text: &str) -> Vec<String> {
    problems(Layout::Loose, &[("main.jadpo", text)])
}

const PENCE: &str = "type Coins = Int {\n    min: 0\n}\n\n";

#[test]
fn a_constant_beside_its_use_checks() {
    let text = format!(
        "{PENCE}constant free_delivery_from: Coins = 5000\nconstant small_order_fee: Coins = 250\n\nfunction delivery_fee(subtotal: Coins) -> Coins {{\n    if (subtotal >= free_delivery_from) {{\n        return 0\n    }}\n    return small_order_fee\n}}\n"
    );
    assert_eq!(loose(&text), Vec::<String>::new());
}

#[test]
fn a_constant_built_from_others_constructs_without_attempt() {
    let text = format!(
        "{PENCE}constant fee: Coins = 250\nconstant double_fee: Coins = Coins(fee * 2)\nconstant fees: List<Coins> = [fee, double_fee]\n\nfunction total(subtotal: Coins) -> Coins {{\n    return Coins(subtotal + double_fee)\n}}\n"
    );
    assert_eq!(loose(&text), Vec::<String>::new());
}

#[test]
fn a_construction_breaking_a_rule_is_refused_while_compiling() {
    let text = "type Small = Int {\n    max: 100\n}\n\nconstant base: Small = 60\nconstant doubled: Small = Small(base * 2)\n";
    assert_eq!(
        loose(text),
        ["main.jadpo type.invalid_literal: `120` isn't a valid `Small`"]
    );
}

#[test]
fn a_built_in_type_is_refused_as_in_a_signature() {
    assert_eq!(
        loose("constant limit: Int = 5\n"),
        ["main.jadpo type.primitive_signature: This constant uses `Int` directly"]
    );
}

#[test]
fn a_call_config_and_clock_are_not_fixed() {
    let call = format!(
        "{PENCE}constant fee: Coins = fee_of(250)\n\nfunction fee_of(amount: Coins) -> Coins {{\n    return amount\n}}\n"
    );
    assert_eq!(
        loose(&call),
        ["main.jadpo constant.value_not_fixed: A constant can't call `fee_of`"]
    );
    let clock = "type At = Instant {}\n\nconstant started: At = clock.now\n";
    assert_eq!(
        loose(clock),
        ["main.jadpo constant.value_not_fixed: A constant can't read `clock.now`"]
    );
}

#[test]
fn a_constant_that_depends_on_itself_is_refused_once() {
    let text = format!("{PENCE}constant a: Coins = b\nconstant b: Coins = a\n");
    assert_eq!(
        loose(&text),
        ["main.jadpo constant.cycle: `b` depends on itself"]
    );
}

#[test]
fn constant_in_a_body_const_and_constant_mut_each_give_one_error() {
    assert_eq!(
        loose(&format!(
            "{PENCE}function fee() -> Coins {{\n    constant x: Coins = 5\n    return x\n}}\n"
        )),
        ["main.jadpo syntax.expected_statement: `constant` doesn't start a statement"]
    );
    assert_eq!(
        loose(&format!("{PENCE}const fee: Coins = 5\n")),
        ["main.jadpo syntax.variable_keyword: Jadpo writes `constant`, not `const`"]
    );
    assert_eq!(
        loose(&format!("{PENCE}constant mut fee: Coins = 5\n")),
        ["main.jadpo syntax.expected_name: A constant can't be `mut`"]
    );
    assert_eq!(
        loose(&format!("{PENCE}var fee: Coins = 5\n")),
        ["main.jadpo syntax.expected_declaration: `var` doesn't start a declaration"]
    );
}

#[test]
fn constant_is_reserved() {
    assert_eq!(
        support::rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                &format!(
                    "{PENCE}function f(constant: Coins) -> Coins {{\n    return constant\n}}\n"
                )
            )]
        )
        .first()
        .map(String::as_str),
        Some("syntax.expected_name")
    );
}

#[test]
fn two_constants_of_one_type_holding_one_value_is_a_declinable_error() {
    let text = format!(
        "{PENCE}constant fee: Coins = 250\nconstant charge: Coins = 250\n\nfunction total() -> Coins {{\n    return Coins(fee + charge)\n}}\n"
    );
    assert_eq!(
        loose(&text),
        ["main.jadpo constant.similar_exists: A constant with this value exists: `fee`"]
    );
}

#[test]
fn a_query_takes_a_constant_where_it_takes_a_literal() {
    let text = "type PageCount = Int {\n    min: 1\n    max: 5000\n}\n\nentity Manual {\n    id: Uuid\n    page_count: PageCount\n}\n\nconstant longest_manual: PageCount = 900\n\nquery long_manuals() -> List<Manual> {\n    from: Manual\n    where: Manual.page_count > longest_manual\n    order: [Manual.page_count asc]\n    limit: 20\n}\n";
    assert_eq!(loose(text), Vec::<String>::new());
    let computed = text.replace("> longest_manual", "> longest_manual * 2");
    assert_eq!(
        loose(&computed),
        [
            "main.jadpo query.value_unsupported: `longest_manual * 2` is worked out, and a query computes nothing"
        ]
    );
}

#[test]
fn an_update_by_id_may_use_a_constant() {
    let text = "type Count = Int {\n    min: 0\n}\n\nentity Counter {\n    id: Uuid\n    hits: Count\n}\n\nconstant step: Count = 1\n\naction hit(counter_id: Counter.id) fails CounterNotFound -> Counter {\n    return attempt Counter.update(counter_id) {\n        set: {\n            hits: Count(Counter.hits + step)\n        }\n    }\n}\n";
    assert_eq!(loose(text), Vec::<String>::new());
}

#[test]
fn a_setting_default_never_names_a_constant() {
    let text = format!(
        "{PENCE}constant largest_page: Coins = 100\n\nconfig {{\n    page_size: Coins {{\n        default: largest_page\n    }}\n}}\n\nfunction page() -> Coins {{\n    return Coins(config.page_size + largest_page)\n}}\n"
    );
    assert_eq!(
        loose(&text),
        [
            "main.jadpo configuration.default_literal_required: `default:` on `page_size` isn't a literal"
        ]
    );
}

/// A domain with a library constant, a constant beside its use, and the
/// files that read them.
const SHOP: &[(&str, &str)] = &[
    APP,
    (
        "shared/types/coins.jadpo",
        "type Coins = Int {\n    min: 0\n}\n",
    ),
    (
        "domains/shop/types/vat_rate.jadpo",
        "type VatRate = Decimal {\n    scale: 4\n    min: 0.0000\n    max: 1.0000\n}\n",
    ),
    (
        "domains/shop/libraries/pricing.jadpo",
        "use shop/types { VatRate }\n\nlibrary pricing {\n    /// The UK standard rate of VAT.\n    constant vat_rate: VatRate = 0.2000\n}\n",
    ),
    (
        "domains/shop/actions/quote.jadpo",
        "use shared/types { Coins }\nuse shop/libraries { pricing }\nuse shop/types { VatRate }\n\nconstant free_delivery_from: Coins = 5000\n\naction quote(subtotal: Coins) -> VatRate {\n    if (subtotal >= free_delivery_from) {\n        return pricing.vat_rate\n    }\n    return VatRate(0.0000)\n}\n",
    ),
];

fn shop(extra: &[(&str, &str)]) -> Vec<String> {
    let mut files = SHOP.to_vec();
    for (path, text) in extra {
        files.retain(|(p, _)| p != path);
        files.push((path, text));
    }
    problems(Layout::Project, &files)
}

#[test]
fn constants_in_a_project_check() {
    assert_eq!(shop(&[]), Vec::<String>::new());
}

#[test]
fn a_constant_a_second_file_reads_moves_into_a_library() {
    assert_eq!(
        shop(&[(
            "domains/shop/actions/other.jadpo",
            "use shared/types { Coins }\n\naction other(subtotal: Coins) -> Coins {\n    if (subtotal >= free_delivery_from) {\n        return subtotal\n    }\n    return Coins(0)\n}\n",
        )]),
        [
            "domains/shop/actions/quote.jadpo layout.promotion_required: `free_delivery_from` is used beyond this file"
        ]
    );
}

#[test]
fn an_unused_constant_is_dead_code() {
    assert_eq!(
        shop(&[(
            "domains/shop/libraries/pricing.jadpo",
            "use shop/types { VatRate }\n\nlibrary pricing {\n    /// The UK standard rate of VAT.\n    constant vat_rate: VatRate = 0.2000\n\n    constant reduced_rate: VatRate = 0.0500\n}\n",
        )]),
        [
            "domains/shop/libraries/pricing.jadpo layout.unused_declaration: `reduced_rate` isn't used"
        ]
    );
}

#[test]
fn a_private_constant_is_unique_only_within_its_domain() {
    let found = shop(&[(
        "domains/billing/actions/invoice.jadpo",
        "use shared/types { Coins }\n\nconstant free_delivery_from: Coins = 4000\n\naction invoice(subtotal: Coins) -> Coins {\n    if (subtotal >= free_delivery_from) {\n        return subtotal\n    }\n    return Coins(0)\n}\n",
    )]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_constant_is_a_graph_node_with_its_value() {
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, SHOP);
    let graph = jadpo_check::graph::graph(&db, project);
    let found: Vec<(String, String)> = graph
        .nodes()
        .iter()
        .filter(|node| node.kind == jadpo_graph::Kind::Constant)
        .map(|node| match &node.detail {
            jadpo_graph::Detail::Constant { value, .. } => (node.key.to_string(), value.clone()),
            _ => (node.key.to_string(), String::new()),
        })
        .collect();
    assert_eq!(
        found,
        [
            (
                "constant pricing.vat_rate".to_string(),
                "0.2000".to_string()
            ),
            (
                "constant shop.free_delivery_from".to_string(),
                "5000".to_string()
            ),
        ]
    );
}
