//! A project laid out as area 1 decides checks cleanly, and each layout,
//! naming, visibility and resolution rule catches the mistake it names.

use crate::support;

use jadpo_db::Layout;
use support::{APP, problems, rules};

/// A small shop in the decided layout: shared types and libraries, two
/// domains, a public action called across them, a route group, an event,
/// failures, tests beside their subjects, a root scenario and a seed.
const SHOP: &[(&str, &str)] = &[
    APP,
    (
        "shared/types/item_price.jadpo",
        "/// An amount in pounds, to the penny.\ntype ItemPrice = Decimal {\n    scale: 2\n    min: 0.00\n}\n",
    ),
    (
        "shared/types/quantity.jadpo",
        "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n",
    ),
    (
        "shared/libraries/rounding.jadpo",
        "use shared/types { ItemPrice }\n\n/// Rounding rules for prices.\nlibrary rounding {\n    /// Rounds to the nearest penny, ties to the even penny.\n    function half_even(amount: ItemPrice) -> ItemPrice {\n        return nearest(amount)\n    }\n\n    function nearest(amount: ItemPrice) -> ItemPrice {\n        return amount\n    }\n}\n",
    ),
    (
        "domains/inventory/entities/stock_level.jadpo",
        "use shared/types { Quantity }\n\nentity StockLevel {\n    id: Uuid\n    quantity: Quantity\n}\n",
    ),
    (
        "domains/inventory/actions/reserve_stock.jadpo",
        "use shared/types { Quantity }\nuse inventory/entities { StockLevel }\n\npublic action reserve_stock(quantity: Quantity) -> Unit {\n    var level: StockLevel = StockLevel.create {\n        set: {\n            quantity: quantity\n        }\n    }\n    if (level.quantity == quantity) {\n        return\n    }\n}\n",
    ),
    (
        "domains/inventory/actions/reserve_stock.test.jadpo",
        "use shared/types { Quantity }\n\ntest \"reserving stock\" {\n    reserve_stock(Quantity(1))\n}\n",
    ),
    (
        "domains/ordering/failures/cart_empty.jadpo",
        "public type CartEmpty = Failure {\n    kind: FailureKind.rejected\n}\n",
    ),
    (
        "domains/ordering/actions/checkout.jadpo",
        "use shared/types { ItemPrice, Quantity }\nuse shared/libraries { rounding }\nuse ordering/failures { CartEmpty }\nuse ordering/events { OrderPlaced }\nuse inventory\n\npublic type CheckoutTotal = Output {\n    total: ItemPrice\n}\n\npublic action checkout(subtotal: ItemPrice, quantity: Quantity) fails CartEmpty -> CheckoutTotal {\n    if (subtotal == ItemPrice(0.00)) {\n        reject CartEmpty\n    }\n    inventory.reserve_stock(quantity)\n    var total: ItemPrice = rounding.half_even(subtotal)\n    emit OrderPlaced {\n        total: total\n    }\n    return CheckoutTotal {\n        total: total\n    }\n}\n",
    ),
    (
        "domains/ordering/events/order_placed.jadpo",
        "use shared/types { ItemPrice }\n\ntype OrderPlaced = Event {\n    total: ItemPrice\n}\n",
    ),
    (
        "domains/ordering/routes/checkouts.jadpo",
        "routes Checkouts {\n    prefix: /checkouts\n}\n",
    ),
    (
        "tests/checkout_reserves_stock.test.jadpo",
        "use shared/types { ItemPrice, Quantity }\nuse ordering\n\ntest \"checkout reserves stock\" {\n    attempt ordering.checkout(ItemPrice(1.00), Quantity(1))\n}\n",
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
        "use shared/types { ItemPrice }\n\naction refund(total: Missing) -> Unit {}\n\naction refund(total: ItemPrice) -> Unit {}\n",
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
            "entity Basket {\n    id: Uuid\n}\n\ntype BasketMissing = Failure {\n    kind: FailureKind.not_found\n}\n"
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
fn a_test_in_a_code_file_is_sent_beside_it() {
    let db = jadpo_db::Database::default();
    let mut files = SHOP.to_vec();
    files.push((
        "domains/ordering/actions/cancel.jadpo",
        "action cancel() -> Unit {}\n\ntest \"cancelling\" {\n    cancel()\n}\n",
    ));
    let project = support::project(&db, Layout::Project, &files);
    let found = jadpo_check::check(&db, project);
    let wrong: Vec<_> = found
        .iter()
        .filter(|d| d.rule.id == "layout.wrong_folder")
        .collect();
    assert_eq!(wrong.len(), 1, "{found:?}");
    assert_eq!(
        wrong[0].next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Move `cancelling` to `domains/ordering/actions/cancel.test.jadpo`, beside the file it tests"
        )
    );
    assert_eq!(
        wrong[0].reason.as_deref(),
        Some(
            "`actions/` holds one action per file; a test sits beside what it tests, in a file whose name ends in `.test.jadpo`."
        )
    );
}

#[test]
fn public_is_only_for_what_another_domain_uses() {
    // Area 1, "Unused `public`" (Richard, 2026-10-14).
    let db = jadpo_db::Database::default();
    let mut files = SHOP.to_vec();
    files.push((
        "domains/inventory/actions/count_stock.jadpo",
        "use shared/types { Quantity }\n\npublic action count_stock(quantity: Quantity) -> Quantity {\n    return quantity\n}\n",
    ));
    // The domain's own tests don't count.
    files.push((
        "domains/inventory/actions/count_stock.test.jadpo",
        "use shared/types { Quantity }\n\ntest \"counting stock\" {\n    assert count_stock(Quantity(2)) == 2\n}\n",
    ));
    let project = support::project(&db, Layout::Project, &files);
    let found = jadpo_check::check(&db, project);
    assert_eq!(found.len(), 1, "{found:?}");
    let problem = &found[0];
    assert_eq!(problem.rule.id, "visibility.public_unused");
    assert_eq!(
        problem.message,
        "`count_stock` is `public`, but nothing outside `inventory` uses it"
    );
    let primary = problem.primary.as_ref().expect("a location");
    assert_eq!(
        (
            primary.span.path.as_str(),
            primary.span.start,
            primary.span.end
        ),
        ("domains/inventory/actions/count_stock.jadpo", 31, 37)
    );
    let next = problem.next.as_ref().expect("a next step");
    assert_eq!(next.text, "Remove `public`");
    assert_eq!(next.edits.len(), 1);
    assert_eq!((next.edits[0].span.start, next.edits[0].span.end), (31, 38));
    assert_eq!(next.edits[0].replacement, "");

    // Another domain's code, another domain's tests and a root scenario
    // are users; so is the signature of a public declaration that one of
    // them uses.
    assert_eq!(
        with(&[
            (
                "domains/inventory/types/bin_code.jadpo",
                "public type BinCode = Text {}\n"
            ),
            (
                "domains/inventory/actions/pick_bin.jadpo",
                "use inventory/types { BinCode }\n\npublic action pick_bin(code: BinCode) -> Unit {}\n"
            ),
            (
                "domains/ordering/actions/pick.jadpo",
                "use inventory\n\naction pick() -> Unit {\n    inventory.pick_bin(\"A1\")\n}\n"
            ),
            stocked(true),
            (
                "domains/ordering/actions/checkout.test.jadpo",
                "use shared/types { ItemPrice }\nuse inventory\n\ntest \"checking out stocked items\" {\n    attempt checkout(ItemPrice(1.00), inventory.stocked())\n}\n"
            ),
        ]),
        Vec::<String>::new()
    );
    // A public type only an unused public action's signature names is
    // unused too.
    assert_eq!(
        with(&[
            (
                "domains/inventory/types/bin_code.jadpo",
                "public type BinCode = Text {}\n"
            ),
            (
                "domains/inventory/actions/pick_bin.jadpo",
                "use inventory/types { BinCode }\n\npublic action pick_bin(code: BinCode) -> Unit {}\n"
            ),
        ]),
        [
            "domains/inventory/actions/pick_bin.jadpo visibility.public_unused: `pick_bin` is `public`, but nothing outside `inventory` uses it",
            "domains/inventory/types/bin_code.jadpo visibility.public_unused: `BinCode` is `public`, but nothing outside `inventory` uses it",
        ]
    );
}

#[test]
fn calls_name_their_owner() {
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reorder.jadpo",
            "use shared/types { Quantity }\n\naction reorder(quantity: Quantity) -> Unit {\n    reserve_stock(quantity)\n}\n"
        )]),
        [
            "domains/ordering/actions/reorder.jadpo semantic.unknown_callee: `reserve_stock` belongs to domain `inventory`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reprice.jadpo",
            "use shared/types { ItemPrice }\n\naction reprice(price: ItemPrice) -> ItemPrice {\n    return half_even(price)\n}\n"
        )]),
        [
            "domains/ordering/actions/reprice.jadpo semantic.unknown_callee: `half_even` is in library `rounding`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reprice.jadpo",
            "use shared/types { ItemPrice }\nuse shared/libraries { rounding }\n\naction reprice(price: ItemPrice) -> ItemPrice {\n    return rounding.half_odd(price)\n}\n"
        )]),
        [
            "domains/ordering/actions/reprice.jadpo semantic.unknown_callee: Library `rounding` has no function `half_odd`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reprice.jadpo",
            "use shared/types { ItemPrice }\n\naction reprice(price: ItemPrice) -> ItemPrice {\n    return temporal.nope(price)\n}\n"
        )]),
        [
            "domains/ordering/actions/reprice.jadpo semantic.unknown_callee: `temporal` has no operation `nope`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/reorder.jadpo",
            "use shared/types { Quantity }\n\naction reorder(quantity: Quantity) -> Unit {\n    invntory.reserve_stock(quantity)\n}\n"
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
                "use inventory/types { BinCode }\n\naction pick(code: BinCode) -> Unit {}\n"
            ),
        ]),
        [
            "domains/ordering/actions/pick.jadpo visibility.private_cross_domain: `BinCode` is private to domain `inventory`"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/peek.jadpo",
            "use shared/types { Quantity }\n\naction peek(quantity: Quantity) -> Quantity {\n    var level: StockLevel = StockLevel.get(quantity)\n    return level.quantity\n}\n"
        )]),
        [
            "domains/ordering/actions/peek.jadpo visibility.private_cross_domain: `StockLevel` is private to domain `inventory`",
            "domains/ordering/actions/peek.jadpo visibility.private_cross_domain: `StockLevel` belongs to domain `inventory`",
        ]
    );
}

#[test]
fn a_scenario_sees_only_what_domains_make_public() {
    // Area 13, question 1: a test in the root `tests/` folder sees what
    // another domain would see. A test beside its subject sees its whole
    // domain.
    assert_eq!(
        with(&[
            (
                "domains/inventory/actions/count_stock.jadpo",
                "use shared/types { Quantity }\n\naction count_stock(quantity: Quantity) -> Quantity {\n    return quantity\n}\n"
            ),
            (
                "domains/inventory/actions/count_stock.test.jadpo",
                "use shared/types { Quantity }\n\ntest \"counting stock\" {\n    assert count_stock(Quantity(2)) == 2\n}\n"
            ),
            (
                "tests/stock_is_counted.test.jadpo",
                "use shared/types { Quantity }\nuse inventory\n\ntest \"stock is counted\" {\n    assert inventory.count_stock(Quantity(2)) == 2\n}\n"
            ),
        ]),
        [
            "tests/stock_is_counted.test.jadpo visibility.private_cross_domain: `count_stock` is private to domain `inventory`"
        ]
    );
}

/// A fixture of `inventory`'s, public or not.
fn stocked(public: bool) -> (&'static str, &'static str) {
    (
        "domains/inventory/fixtures/stocked.jadpo",
        if public {
            "use shared/types { Quantity }\n\npublic fixture stocked() -> Quantity {\n    return Quantity(5)\n}\n"
        } else {
            "use shared/types { Quantity }\n\nfixture stocked() -> Quantity {\n    return Quantity(5)\n}\n"
        },
    )
}

#[test]
fn a_public_fixture_is_offered_to_other_domains_tests_fixtures_and_seeds() {
    // Area 13, decided 2026-10-14: a public fixture is called through its
    // domain from another domain's tests, fixtures and seeds, and from a
    // scenario in root `tests/`.
    assert_eq!(
        with(&[
            stocked(true),
            (
                "domains/ordering/actions/checkout.test.jadpo",
                "use shared/types { ItemPrice }\nuse inventory\n\ntest \"checking out stocked items\" {\n    attempt checkout(ItemPrice(1.00), inventory.stocked())\n}\n"
            ),
            (
                "domains/ordering/fixtures/stocked_order.jadpo",
                "use shared/types { Quantity }\nuse inventory\n\nfixture stocked_order() -> Quantity {\n    return inventory.stocked()\n}\n"
            ),
            (
                "tests/stocked_items_check_out.test.jadpo",
                "use shared/types { ItemPrice }\nuse inventory\nuse ordering\n\ntest \"stocked items check out\" {\n    attempt ordering.checkout(ItemPrice(1.00), inventory.stocked())\n}\n"
            ),
            (
                "database/seeds/stocked_demo.jadpo",
                "use inventory\n\nseed stocked_demo {\n    inventory.reserve_stock(inventory.stocked())\n}\n"
            ),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_private_fixture_stays_in_its_domain() {
    let calls = [
        (
            "domains/ordering/actions/checkout.test.jadpo",
            "use shared/types { ItemPrice }\nuse inventory\n\ntest \"checking out stocked items\" {\n    attempt checkout(ItemPrice(1.00), inventory.stocked())\n}\n",
        ),
        (
            "tests/stocked_items_check_out.test.jadpo",
            "use shared/types { ItemPrice }\nuse inventory\nuse ordering\n\ntest \"stocked items check out\" {\n    attempt ordering.checkout(ItemPrice(1.00), inventory.stocked())\n}\n",
        ),
        (
            "database/seeds/stocked_demo.jadpo",
            "use inventory\n\nseed stocked_demo {\n    inventory.reserve_stock(inventory.stocked())\n}\n",
        ),
        (
            "domains/inventory/actions/reserve_stock.test.jadpo",
            "use inventory/fixtures { stocked }\n\ntest \"reserving stock\" {\n    reserve_stock(stocked())\n}\n",
        ),
    ];
    let mut files = vec![stocked(false)];
    files.extend_from_slice(&calls);
    assert_eq!(
        with(&files),
        [
            "database/seeds/stocked_demo.jadpo visibility.private_cross_domain: `stocked` is private to domain `inventory`",
            "domains/ordering/actions/checkout.test.jadpo visibility.private_cross_domain: `stocked` is private to domain `inventory`",
            "tests/stocked_items_check_out.test.jadpo visibility.private_cross_domain: `stocked` is private to domain `inventory`",
        ]
    );
    let db = jadpo_db::Database::default();
    let mut all = SHOP.to_vec();
    all.extend_from_slice(&files);
    let project = support::project(&db, Layout::Project, &all);
    let private = jadpo_check::check(&db, project)
        .iter()
        .find(|d| d.rule.id == "visibility.private_cross_domain")
        .and_then(|d| d.next.clone())
        .map(|next| next.text);
    assert_eq!(
        private.as_deref(),
        Some("Mark `stocked` `public` if `inventory` should offer it to other domains' tests")
    );
    // Another domain calls a fixture through its domain, like an action.
    assert_eq!(
        with(&[
            stocked(true),
            (
                "domains/ordering/actions/checkout.test.jadpo",
                "use shared/types { ItemPrice }\n\ntest \"checking out stocked items\" {\n    attempt checkout(ItemPrice(1.00), stocked())\n}\n"
            ),
        ]),
        [
            "domains/ordering/actions/checkout.test.jadpo semantic.unknown_callee: `stocked` belongs to domain `inventory`"
        ]
    );
}

#[test]
fn only_tests_fixtures_and_seeds_call_a_fixture() {
    // Area 13: application code never calls a fixture, public or not.
    assert_eq!(
        with(&[
            stocked(true),
            (
                "domains/inventory/actions/restock.jadpo",
                "use shared/types { Quantity }\nuse inventory/fixtures { stocked }\n\naction restock() -> Quantity {\n    return stocked()\n}\n"
            ),
            (
                "domains/ordering/actions/reorder.jadpo",
                "use shared/types { Quantity }\nuse inventory\n\naction reorder() -> Quantity {\n    return inventory.stocked()\n}\n"
            ),
        ]),
        [
            "domains/inventory/actions/restock.jadpo semantic.not_callable: An action can't call fixture `stocked`",
            "domains/ordering/actions/reorder.jadpo semantic.not_callable: An action can't call fixture `inventory.stocked`",
        ]
    );
}

#[test]
fn shared_code_never_depends_on_a_domain() {
    assert_eq!(
        with(&[(
            "shared/libraries/stock_maths.jadpo",
            "use shared/types { Quantity }\n\nlibrary stock_maths {\n    function spare(level: StockLevel) -> Quantity {\n        return level.quantity\n    }\n}\n"
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
            "use shared/types { ItemPrice }\nuse ordering/actions { CheckoutTotal }\n\naction quote(subtotal: ItemPrice) -> CheckoutTotal {\n    return CheckoutTotal {\n        total: subtotal\n    }\n}\n"
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
                "use inventory/failures { OutOfStock }\n\naction release_stock() fails OutOfStock -> Unit {\n    reject OutOfStock\n}\n"
            ),
            (
                "domains/ordering/actions/hold.jadpo",
                "use inventory/failures { OutOfStock }\n\naction hold() fails OutOfStock -> Unit {\n    reject OutOfStock\n}\n"
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
                "use inventory/types { BinCode }\n\naction pick(code: BinCode) -> Unit {}\n"
            ),
        ]),
        [
            "domains/inventory/types/bin_code.jadpo layout.promotion_required: `BinCode` is used beyond this domain"
        ]
    );
}

#[test]
fn a_route_counts_as_a_user_only_when_nothing_else_uses_the_type() {
    // The routes run nothing: only how they use the type is checked here.
    let with = |files: &[(&str, &str)]| -> Vec<String> {
        with(files)
            .into_iter()
            .filter(|problem| !problem.contains(" route."))
            .collect()
    };
    let route = (
        "domains/ordering/routes/quotes.jadpo",
        "use ordering/actions { CheckoutTotal }\n\nroutes Quotes {\n    prefix: /quotes\n\n    GET / {\n        output: CheckoutTotal\n    }\n}\n",
    );
    assert_eq!(with(&[route]), Vec::<String>::new());
    assert_eq!(
        with(&[
            (
                "domains/ordering/routes/carts.jadpo",
                "use shared/types { ItemPrice }\n\ntype CartView = Output {\n    total: ItemPrice\n}\n\nroutes Carts {\n    prefix: /carts\n\n    GET / {\n        output: CartView\n    }\n}\n"
            ),
            (
                "domains/ordering/routes/quotes.jadpo",
                "use ordering/routes { CartView }\n\nroutes Quotes {\n    prefix: /quotes\n\n    GET / {\n        output: CartView\n    }\n}\n"
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
            "use shared/types { Quantity }\n\naction cancel(quantity: Quantity) -> Unit {\n    var inventory: Quantity = quantity\n    var temporal: Quantity = quantity\n}\n"
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
fn types_built_from_other_types_resolve_what_they_copy() {
    assert_eq!(
        problems(
            Layout::Loose,
            &[(
                "a.jadpo",
                "type BasketNote = Text {}\n\nentity Basket {\n    id: Uuid\n    note: BasketNote\n}\n\ntype BasketView = Output {\n    include: [Basket]\n    omit: [Basket.note]\n}\n\ntype PatchBasket = Input {\n    pick: [Basket.note, Baskit.size]\n    optional: all\n}\n"
            )]
        ),
        ["a.jadpo semantic.unknown_name: `Baskit` isn't declared"]
    );
}

#[test]
fn loose_files_have_no_layout_rules() {
    assert_eq!(
        problems(
            Layout::Loose,
            &[(
                "anything/goes.jadpo",
                "entity Basket {\n    id: Uuid\n}\n\naction empty_basket() -> Unit {}\n"
            )]
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_file_holds_only_what_its_primary_declaration_uses() {
    // Area 1, "Files and names": everything beside the primary declaration
    // is used by it, directly or through one another. The file's tests
    // don't count as users.
    assert_eq!(
        with(&[
            (
                "domains/ordering/actions/quote.jadpo",
                "use shared/types { ItemPrice }\n\naction quote(subtotal: ItemPrice) -> ItemPrice {\n    return doubled(subtotal)\n}\n\nfunction doubled(amount: ItemPrice) -> ItemPrice {\n    return summed(amount, amount)\n}\n\nfunction summed(a: ItemPrice, b: ItemPrice) -> ItemPrice {\n    return ItemPrice(a + b)\n}\n\ntype Note = Text {}\n\nfunction noted(note: Note) -> Note {\n    return note\n}\n"
            ),
            (
                "domains/ordering/actions/quote.test.jadpo",
                "test \"noting\" {\n    assert noted(\"hi\") == \"hi\"\n}\n"
            ),
        ]),
        [
            "domains/ordering/actions/quote.jadpo layout.unused_declaration: `Note` isn't used by `quote`",
            "domains/ordering/actions/quote.jadpo layout.unused_declaration: `noted` isn't used by `quote`",
        ]
    );
    // The same in `types/`, beside a shared type, which is itself unused
    // when no other file uses it.
    assert_eq!(
        with(&[(
            "shared/types/parcel.jadpo",
            "type Parcel = Object {\n    weight: Grams\n}\n\ntype Grams = Int {\n    min: 0\n}\n\ntype Ounces = Int {}\n"
        )]),
        [
            "shared/types/parcel.jadpo layout.unused_declaration: `Parcel` isn't used",
            "shared/types/parcel.jadpo layout.unused_declaration: `Ounces` isn't used by `Parcel`",
        ]
    );
    // While the primary writes a name that names nothing, perhaps a
    // misspelling of the helper it meant, only that is reported.
    assert_eq!(
        with(&[(
            "domains/ordering/actions/quote.jadpo",
            "use shared/types { ItemPrice }\n\naction quote(subtotal: ItemPrice) -> ItemPrice {\n    return dobuled(subtotal)\n}\n\nfunction doubled(amount: ItemPrice) -> ItemPrice {\n    return ItemPrice(amount + amount)\n}\n"
        )]),
        [
            "domains/ordering/actions/quote.jadpo semantic.unknown_callee: `dobuled` isn't a function, action or query"
        ]
    );
    assert_eq!(
        with(&[(
            "domains/ordering/actions/quote.jadpo",
            "action quote(note: Ntoe) -> Unit {}\n\ntype Note = Text {}\n"
        )]),
        ["domains/ordering/actions/quote.jadpo semantic.unknown_name: `Ntoe` isn't declared"]
    );
    // One that another file uses is moved, not removed.
    assert_eq!(
        with(&[
            (
                "domains/ordering/actions/quote.jadpo",
                "use shared/types { ItemPrice }\n\naction quote(subtotal: ItemPrice) -> ItemPrice {\n    return subtotal\n}\n\ntype Note = Text {}\n"
            ),
            (
                "domains/ordering/actions/annotate.jadpo",
                "use ordering/actions { Note }\n\naction annotate(note: Note) -> Note {\n    return note\n}\n"
            ),
        ]),
        [
            "domains/ordering/actions/quote.jadpo layout.promotion_required: `Note` is used beyond this file"
        ]
    );
}
