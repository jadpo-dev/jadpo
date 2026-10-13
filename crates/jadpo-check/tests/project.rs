//! A project laid out as area 1 decides checks cleanly, and each layout,
//! naming, visibility and resolution rule catches the mistake it names.

mod support;

use jadpo_db::Layout;
use support::{APP, problems, rules};

/// A small shop in the decided layout: shared types and libraries, two
/// domains, a public action called across them, a route group, an event,
/// failures, tests beside their subjects, a root scenario and a seed.
const SHOP: &[(&str, &str)] = &[
    APP,
    (
        "shared/types/price.jadpo",
        "/// An amount in pounds, to the penny.\ntype Price = Decimal {\n    scale: 2\n    min: 0.00\n}\n",
    ),
    (
        "shared/types/quantity.jadpo",
        "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n",
    ),
    (
        "shared/libraries/rounding.jadpo",
        "/// Rounding rules for prices.\nlibrary rounding {\n    /// Rounds to the nearest penny, ties to the even penny.\n    function half_even(amount: Price) -> Price {\n        return nearest(amount)\n    }\n\n    function nearest(amount: Price) -> Price {\n        return amount\n    }\n}\n",
    ),
    (
        "domains/inventory/entities/stock_level.jadpo",
        "entity StockLevel {\n    quantity: Quantity\n}\n",
    ),
    (
        "domains/inventory/actions/reserve_stock.jadpo",
        "public action reserve_stock(quantity: Quantity) -> Unit {\n    var level: StockLevel = StockLevel.get(quantity)\n}\n",
    ),
    (
        "domains/inventory/actions/reserve_stock.test.jadpo",
        "test \"reserving stock\" {\n    reserve_stock(Quantity(1))\n}\n",
    ),
    (
        "domains/ordering/failures/cart_empty.jadpo",
        "type CartEmpty = Failure {\n    kind: FailureKind.rejected\n}\n",
    ),
    (
        "domains/ordering/actions/checkout.jadpo",
        "type CheckoutTotal = Output {\n    total: Price\n}\n\naction checkout(subtotal: Price, quantity: Quantity) fails CartEmpty -> CheckoutTotal {\n    if (subtotal == Price(0.00)) {\n        reject CartEmpty\n    }\n    inventory.reserve_stock(quantity)\n    var total: Price = rounding.half_even(subtotal)\n    emit OrderPlaced {\n        total: total\n    }\n    return CheckoutTotal {\n        total: total\n    }\n}\n",
    ),
    (
        "domains/ordering/events/order_placed.jadpo",
        "public type OrderPlaced = Event {\n    total: Price\n}\n",
    ),
    (
        "domains/ordering/routes/checkouts.jadpo",
        "routes Checkouts {\n    prefix: /checkouts\n\n    POST / {}\n}\n",
    ),
    (
        "tests/checkout_reserves_stock.test.jadpo",
        "test \"checkout reserves stock\" {\n    ordering.checkout(Price(1.00), Quantity(1))\n}\n",
    ),
    ("database/seeds/local_demo.jadpo", "seed local_demo {}\n"),
];

#[test]
fn a_project_in_the_decided_layout_checks_cleanly() {
    assert_eq!(problems(Layout::Project, SHOP), Vec::<String>::new());
}

#[test]
fn the_same_files_in_any_order_give_the_same_result() {
    let mut reversed = SHOP.to_vec();
    reversed.reverse();
    reversed.push((
        "domains/ordering/actions/refund.jadpo",
        "action refund(total: Missing) -> Unit {}\n\naction refund(total: Price) -> Unit {}\n",
    ));
    let mut forwards = SHOP.to_vec();
    forwards.push(reversed.last().copied().expect("the extra file"));
    assert_eq!(
        problems(Layout::Project, &reversed),
        problems(Layout::Project, &forwards)
    );
}

fn with(files: &[(&str, &str)]) -> Vec<String> {
    let mut all = SHOP.to_vec();
    all.extend_from_slice(files);
    problems(Layout::Project, &all)
}

#[test]
fn files_outside_the_layout_are_reported() {
    assert_eq!(
        with(&[("utils.jadpo", "type Spare = Int {}\n")]),
        ["utils.jadpo layout.unknown_location: `utils.jadpo` isn't one of the root files"]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/old/cancel.jadpo",
            "action cancel() -> Unit {}\n"
        )]),
        [
            "domains/ordering/actions/old/cancel.jadpo layout.unknown_location: `domains/ordering/actions/old/` is a folder inside a fixed folder"
        ]
    );
    assert_eq!(
        with(&[("tests/notes.jadpo", "type Note = Int {}\n")]),
        ["tests/notes.jadpo layout.unknown_location: `notes.jadpo` isn't a test file"]
    );
    assert_eq!(
        with(&[(
            "shared/actions/cancel.jadpo",
            "action cancel() -> Unit {}\n"
        )]),
        [
            "shared/actions/cancel.jadpo layout.unknown_location: `shared/actions/` isn't part of `shared/`"
        ]
    );
}

#[test]
fn folders_hold_their_own_kind() {
    assert_eq!(
        with(&[(
            "domains/ordering/actions/cancel.jadpo",
            "action cancel() -> Unit {}\n\naction refund() -> Unit {}\n"
        )]),
        [
            "domains/ordering/actions/cancel.jadpo layout.file_name_mismatch: `refund` needs a file of its own"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/entities/basket.jadpo",
            "entity Basket {}\n\ntype BasketMissing = Failure {\n    kind: FailureKind.not_found\n}\n"
        )]),
        [
            "domains/ordering/entities/basket.jadpo layout.wrong_folder: Failure `BasketMissing` is in `entities/`"
        ]
    );
    assert_eq!(
        with(&[("config.jadpo", "action reset() -> Unit {}\n")]),
        ["config.jadpo layout.wrong_folder: Action `reset` is in `config.jadpo`"]
    );
}

#[test]
fn calls_name_their_owner() {
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reorder.jadpo",
            "action reorder(quantity: Quantity) -> Unit {\n    reserve_stock(quantity)\n}\n"
        )]),
        [
            "domains/ordering/actions/reorder.jadpo semantic.unknown_callee: `reserve_stock` belongs to domain `inventory`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reprice.jadpo",
            "action reprice(price: Price) -> Price {\n    return half_even(price)\n}\n"
        )]),
        [
            "domains/ordering/actions/reprice.jadpo semantic.unknown_callee: `half_even` is in library `rounding`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reprice.jadpo",
            "action reprice(price: Price) -> Price {\n    return rounding.half_odd(price)\n}\n"
        )]),
        [
            "domains/ordering/actions/reprice.jadpo semantic.unknown_callee: Library `rounding` has no function `half_odd`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reprice.jadpo",
            "action reprice(price: Price) -> Price {\n    return temporal.nope(price)\n}\n"
        )]),
        [
            "domains/ordering/actions/reprice.jadpo semantic.unknown_callee: `temporal` has no operation `nope`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reorder.jadpo",
            "action reorder(quantity: Quantity) -> Unit {\n    invntory.reserve_stock(quantity)\n}\n"
        )]),
        [
            "domains/ordering/actions/reorder.jadpo semantic.unknown_name: `invntory` isn't a domain, library or namespace"
        ]
    );
}

#[test]
fn domains_see_only_what_others_make_public() {
    assert_eq!(
        with(&[
            (
                "domains/inventory/types/bin_code.jadpo",
                "type BinCode = Text {}\n"
            ),
            (
                "domains/ordering/actions/pick.jadpo",
                "action pick(code: BinCode) -> Unit {}\n"
            ),
        ]),
        [
            "domains/ordering/actions/pick.jadpo visibility.private_cross_domain: `BinCode` is private to domain `inventory`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/peek.jadpo",
            "action peek(quantity: Quantity) -> Unit {\n    var level: StockLevel = StockLevel.get(quantity)\n}\n"
        )]),
        [
            "domains/ordering/actions/peek.jadpo visibility.private_cross_domain: `StockLevel` is private to domain `inventory`",
            "domains/ordering/actions/peek.jadpo visibility.private_cross_domain: `StockLevel` belongs to domain `inventory`",
        ]
    );
}

#[test]
fn shared_code_never_depends_on_a_domain() {
    assert_eq!(
        with(&[(
            "shared/libraries/stock_maths.jadpo",
            "library stock_maths {\n    function spare(level: StockLevel) -> Quantity {\n        return level.quantity\n    }\n}\n"
        )]),
        [
            "shared/libraries/stock_maths.jadpo layout.unused_declaration: `spare` isn't used",
            "shared/libraries/stock_maths.jadpo layout.call_direction: Shared code uses `StockLevel` from domain `inventory`",
        ]
    );
}

#[test]
fn declarations_move_when_a_second_file_or_domain_uses_them() {
    assert_eq!(
        with(&[(
            "domains/ordering/actions/quote.jadpo",
            "action quote(subtotal: Price) -> CheckoutTotal {\n    return CheckoutTotal {\n        total: subtotal\n    }\n}\n"
        )]),
        [
            "domains/ordering/actions/checkout.jadpo layout.promotion_required: `CheckoutTotal` is used beyond this file"
        ]
    );
    assert_eq!(
        with(&[
            (
                "domains/inventory/failures/out_of_stock.jadpo",
                "public type OutOfStock = Failure {\n    kind: FailureKind.conflict\n}\n"
            ),
            (
                "domains/inventory/actions/release_stock.jadpo",
                "action release_stock() fails OutOfStock -> Unit {\n    reject OutOfStock\n}\n"
            ),
            (
                "domains/ordering/actions/hold.jadpo",
                "action hold() fails OutOfStock -> Unit {\n    reject OutOfStock\n}\n"
            ),
        ]),
        [
            "domains/inventory/failures/out_of_stock.jadpo layout.promotion_required: `OutOfStock` is raised by 2 domains"
        ]
    );
    assert_eq!(
        with(&[
            (
                "domains/inventory/types/bin_code.jadpo",
                "public type BinCode = Text {}\n"
            ),
            (
                "domains/ordering/actions/pick.jadpo",
                "action pick(code: BinCode) -> Unit {}\n"
            ),
        ]),
        [
            "domains/inventory/types/bin_code.jadpo layout.promotion_required: `BinCode` is used beyond this domain"
        ]
    );
}

#[test]
fn a_route_counts_as_a_user_only_when_nothing_else_uses_the_type() {
    let route = (
        "domains/ordering/routes/quotes.jadpo",
        "routes Quotes {\n    prefix: /quotes\n\n    GET / {\n        output: CheckoutTotal\n    }\n}\n",
    );
    assert_eq!(with(&[route]), Vec::<String>::new());
    assert_eq!(
        with(&[
            (
                "domains/ordering/routes/carts.jadpo",
                "type CartView = Output {\n    total: Price\n}\n\nroutes Carts {\n    prefix: /carts\n\n    GET / {\n        output: CartView\n    }\n}\n"
            ),
            (
                "domains/ordering/routes/quotes.jadpo",
                "routes Quotes {\n    prefix: /quotes\n\n    GET / {\n        output: CartView\n    }\n}\n"
            ),
        ]),
        [
            "domains/ordering/routes/carts.jadpo layout.promotion_required: `CartView` is used beyond this file"
        ]
    );
}

#[test]
fn names_follow_their_kind_and_stay_unique() {
    assert_eq!(
        with(&[(
            "domains/ordering/actions/cancel.jadpo",
            "action cancel(quantity: Quantity) -> Unit {\n    var inventory: Quantity = quantity\n    var temporal: Quantity = quantity\n}\n"
        )]),
        [
            "domains/ordering/actions/cancel.jadpo semantic.duplicate_declaration: `inventory` is already a domain",
            "domains/ordering/actions/cancel.jadpo semantic.standard_namespace_reserved: `temporal` is a standard-library namespace",
        ]
    );
    assert_eq!(
        rules(Layout::Loose, &[("a.jadpo", "type Text = Int {}\n")]),
        ["semantic.duplicate_declaration"]
    );
    assert_eq!(
        with(&[(
            "domains/utils/actions/tidy.jadpo",
            "action tidy() -> Unit {}\n"
        )]),
        [
            "domains/utils/actions/tidy.jadpo naming.generic_name: `utils` is too generic for a domain"
        ]
    );
}

#[test]
fn names_resolve_to_the_right_kind() {
    assert_eq!(
        rules(
            Layout::Loose,
            &[(
                "a.jadpo",
                "type Count = Int {}\n\nfunction f(x: Count) fails Count -> Count {\n    return x\n}\n"
            )]
        ),
        ["semantic.wrong_name_kind"]
    );
    assert_eq!(
        problems(
            Layout::Loose,
            &[(
                "a.jadpo",
                "type Count = Int {}\n\nfunction f(x: Count) -> Count {\n    return x()\n}\n"
            )]
        ),
        ["a.jadpo semantic.not_callable: `x` is a variable and can't be called"]
    );
}

#[test]
fn loose_files_have_no_layout_rules() {
    assert_eq!(
        problems(
            Layout::Loose,
            &[(
                "anything/goes.jadpo",
                "entity Basket {}\n\naction empty_basket() -> Unit {}\n"
            )]
        ),
        Vec::<String>::new()
    );
}
