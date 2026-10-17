//! Area 2, values and types: the page's examples, run.

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, int, list, main, program, text, value};

const NAMES: &str = "type ProductName = Text {\n    min_length: 1\n    max_length: 120\n}\n\ntype Pence = Int {\n    min: 0\n}\n";

#[test]
fn a_narrower_type_stands_in_for_its_parent() {
    let program = main(
        "type ProductName = Text {\n    min_length: 1\n    max_length: 120\n}\n\ntype ShortProductName = ProductName {\n    max_length: 40\n}\n\nfunction catalogue_name(name: ShortProductName) -> ProductName {\n    return name\n}\n",
    );
    assert_eq!(
        value(&program, "catalogue_name", vec![text("Lamp")]),
        text("Lamp")
    );
}

#[test]
fn literals_take_the_type_they_go_into() {
    let program = main(
        "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype Price = Decimal {\n    scale: 2\n    min: 0.00\n}\n\nfunction starting_quantity() -> Quantity {\n    return 1\n}\n\nfunction free() -> Price {\n    return 0.00\n}\n",
    );
    assert_eq!(value(&program, "starting_quantity", vec![]), int(1));
    let free = value(&program, "free", vec![]);
    assert_eq!(free, decimal("0"));
    assert_eq!(free.to_string(), "0.00", "kept at the type's scale");
}

#[test]
fn constructing_converts_between_named_types() {
    let program = main(
        "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\nfunction line_total(price: Pence, quantity: Quantity) -> Pence {\n    return Pence(price * quantity)\n}\n",
    );
    assert_eq!(
        value(&program, "line_total", vec![int(250), int(3)]),
        int(750)
    );
}

#[test]
fn signatures_use_named_types() {
    let program = main(
        "type CustomerNote = Text {\n    max_length: 500\n}\n\ntype NoteWasEdited = Bool {}\n\nfunction edited(before: CustomerNote, after: CustomerNote) -> NoteWasEdited {\n    return NoteWasEdited(before != after)\n}\n",
    );
    assert_eq!(
        value(&program, "edited", vec![text("a"), text("b")]),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "edited", vec![text("a"), text("a")]),
        Value::Bool(false)
    );
}

#[test]
fn an_object_is_built_with_every_field() {
    let program = program(&[
        ("names.jadpo", NAMES),
        (
            "main.jadpo",
            "type PriceTag = Object {\n    amount: Pence\n}\n\ntype CreateProduct = Input {\n    name: ProductName\n    price: Pence\n}\n\ntype ProductView = Output {\n    name: ProductName\n    price: PriceTag\n}\n",
        ),
        (
            "view.jadpo",
            "function view(name: ProductName, price: Pence) -> ProductView {\n    return ProductView {\n        name: name\n        price: PriceTag {\n            amount: price\n        }\n    }\n}\n",
        ),
    ]);
    let view = value(&program, "view", vec![text("Lamp"), int(1299)]);
    assert_eq!(
        view.to_string(),
        "ProductView { name: \"Lamp\", price: PriceTag { amount: 1299 } }"
    );
    let money = program
        .codec()
        .object("PriceTag", vec![("amount", int(1299))])
        .expect("PriceTag");
    let expected = program
        .codec()
        .object(
            "ProductView",
            vec![("price", money), ("name", text("Lamp"))],
        )
        .expect("ProductView");
    assert_eq!(
        view, expected,
        "fields in declared order, whatever order they are given"
    );
}

#[test]
fn field_types_read_through_an_entity() {
    let program = main(
        "entity Customer {\n    id: Uuid\n    email: Email\n}\n\nentity Supplier {\n    id: Uuid\n    email: Email\n}\n\nfunction contact_address(email: Email) -> Email {\n    return email\n}\n\nfunction customer_contact(customer: Customer) -> Email {\n    return contact_address(customer.email)\n}\n\nfunction supplier_contact(supplier: Supplier) -> Email {\n    return contact_address(supplier.email)\n}\n",
    );
    let customer = program
        .codec()
        .object(
            "Customer",
            vec![
                ("id", Value::uuid("0192F0C4-3B5E-7C1A-9D2E-4F6A8B0C1D2E")),
                ("email", text("ada@example.test")),
            ],
        )
        .expect("Customer");
    assert_eq!(
        value(&program, "customer_contact", vec![customer]),
        text("ada@example.test")
    );
}

#[test]
fn an_absent_value_is_matched() {
    let program = program(&[
        ("names.jadpo", NAMES),
        (
            "main.jadpo",
            "type Listing = Object {\n    name: ProductName\n    subtitle: ProductName?\n}\n\nfunction headline(listing: Listing) -> ProductName {\n    return match listing.subtitle {\n        some(subtitle) -> subtitle\n        none -> listing.name\n    }\n}\n",
        ),
    ]);
    let listing = |subtitle: Value| {
        program
            .codec()
            .object(
                "Listing",
                vec![("name", text("Lamp")), ("subtitle", subtitle)],
            )
            .expect("Listing")
    };
    assert_eq!(
        value(&program, "headline", vec![listing(Value::None)]),
        text("Lamp")
    );
    assert_eq!(
        value(&program, "headline", vec![listing(text("Brass desk lamp"))]),
        text("Brass desk lamp")
    );
}

#[test]
fn an_optional_input_field_left_out_reads_as_none() {
    let program = program(&[
        (
            "names.jadpo",
            "type Nickname = Text {\n    max_length: 30\n}\n",
        ),
        (
            "update_profile.jadpo",
            "type UpdateProfile = Input {\n    nickname: Nickname {\n        optional: true\n    }\n}\n\naction update_profile(request: UpdateProfile) -> Nickname? {\n    var nickname: Nickname? = request.nickname  // left out reads as none\n    return nickname\n}\n",
        ),
    ]);
    let request = program
        .codec()
        .object("UpdateProfile", vec![("nickname", Value::None)])
        .expect("UpdateProfile");
    assert_eq!(
        value(&program, "update_profile", vec![request]),
        Value::None
    );
}

#[test]
fn enums_and_tagged_unions() {
    let program = program(&[
        (
            "names.jadpo",
            "type ReceiptId = Uuid {}\n\ntype DeclineReason = Text {\n    min_length: 1\n    max_length: 200\n}\n\ntype OutcomeLabel = Text {\n    max_length: 20\n}\n\ntype TicketStatus = Enum {\n    open\n    waiting\n    closed\n}\n\nfunction reopen() -> TicketStatus {\n    return TicketStatus.open\n}\n",
        ),
        (
            "main.jadpo",
            "type PaymentOutcome = Enum {\n    pending\n    paid: {\n        receipt_id: ReceiptId\n        paid_at: Instant\n    }\n    declined: {\n        reason: DeclineReason\n    }\n}\n\nfunction declined_payment(reason: DeclineReason) -> PaymentOutcome {\n    return PaymentOutcome.declined {\n        reason: reason\n    }\n}\n\nfunction label(outcome: PaymentOutcome) -> OutcomeLabel {\n    return match outcome {\n        PaymentOutcome.pending -> \"pending\"\n        PaymentOutcome.paid -> \"paid\"\n        PaymentOutcome.declined -> \"declined\"\n    }\n}\n",
        ),
    ]);
    assert_eq!(
        value(&program, "reopen", vec![]).to_string(),
        "TicketStatus.open"
    );
    let declined = value(&program, "declined_payment", vec![text("card expired")]);
    assert_eq!(
        declined.to_string(),
        "PaymentOutcome.declined { reason: \"card expired\" }"
    );
    assert_eq!(value(&program, "label", vec![declined]), text("declined"));
    let paid = program
        .codec()
        .variant(
            "PaymentOutcome",
            "paid",
            vec![
                (
                    "receipt_id",
                    Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e"),
                ),
                ("paid_at", Value::Instant(0)),
            ],
        )
        .expect("paid");
    assert_eq!(value(&program, "label", vec![paid]), text("paid"));
    let pending = program
        .codec()
        .variant("PaymentOutcome", "pending", vec![])
        .expect("pending");
    assert_eq!(value(&program, "label", vec![pending]), text("pending"));
}

#[test]
fn values_never_change_so_a_new_one_is_built() {
    let program = program(&[
        ("names.jadpo", NAMES),
        (
            "main.jadpo",
            "type Product = Object {\n    name: ProductName\n    price: Pence\n}\n\nfunction reprice(product: Product, price: Pence) -> Product {\n    return Product {\n        name: product.name\n        price: price\n    }\n}\n",
        ),
    ]);
    let lamp = program
        .codec()
        .object(
            "Product",
            vec![("name", text("Lamp")), ("price", int(1299))],
        )
        .expect("Product");
    let repriced = value(&program, "reprice", vec![lamp.clone(), int(999)]);
    assert_eq!(
        repriced.to_string(),
        "Product { name: \"Lamp\", price: 999 }"
    );
    assert_eq!(lamp.to_string(), "Product { name: \"Lamp\", price: 1299 }");
}

#[test]
fn a_runtime_construction_that_breaks_a_rule_fails_with_its_arms_failure() {
    // Question 1.
    let program = main(
        "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype QuantityOutOfRange = Failure {\n    kind: FailureKind.invalid_value\n}\n\naction double_quantity(quantity: Quantity) fails QuantityOutOfRange -> Quantity {\n    var doubled: Quantity = attempt Quantity(quantity * 2) {\n        max: QuantityOutOfRange\n    }\n    return doubled\n}\n",
    );
    assert_eq!(value(&program, "double_quantity", vec![int(300)]), int(600));
    let failed = failure(&program, "double_quantity", vec![int(600)]);
    assert_eq!(failed.name, "QuantityOutOfRange");
    assert_eq!(failed.code, "quantity_out_of_range");
    assert_eq!(failed.kind.as_deref(), Some("invalid_value"));
    assert_eq!(failed.broken.as_deref(), Some("`max: 1000`"));
}

#[test]
fn a_field_is_built_first_and_the_object_never_fails() {
    // Question 2.
    let program = program(&[
        (
            "users.jadpo",
            "entity User {\n    id: Uuid\n    display_name: Text {\n        max_length: 80\n    }\n}\n\ntype BadgeNameTooLong = Failure {\n    kind: FailureKind.invalid_value\n}\n",
        ),
        (
            "main.jadpo",
            "type Badge = Object {\n    name: Text {\n        max_length: 12\n    }\n}\n\naction make_badge(user: User) fails BadgeNameTooLong -> Badge {\n    // user.display_name allows 80 characters; the field allows 12, so\n    // the name is built first, where its failure is mapped.\n    var name: Badge.name = attempt Badge.name(user.display_name) {\n        max_length: BadgeNameTooLong\n    }\n    return Badge {\n        name: name\n    }\n}\n",
        ),
    ]);
    let user = |name: &str| {
        program
            .codec()
            .object(
                "User",
                vec![
                    ("id", Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e")),
                    ("display_name", text(name)),
                ],
            )
            .expect("User")
    };
    assert_eq!(
        value(&program, "make_badge", vec![user("Ada")]).to_string(),
        "Badge { name: \"Ada\" }"
    );
    let failed = failure(&program, "make_badge", vec![user("Ada Lovelace-Byron")]);
    assert_eq!(failed.name, "BadgeNameTooLong");
    assert_eq!(failed.broken.as_deref(), Some("`max_length: 12`"));
}

#[test]
fn lists_are_built_counted_searched_and_looped_over() {
    // Question 4.
    let program = main(
        "type TagName = Text {\n    min_length: 1\n    max_length: 30\n}\n\ntype TagCount = Int {\n    min: 0\n}\n\ntype HasUrgent = Bool {}\n\nfunction tag_count() -> TagCount {\n    var tags: List<TagName> = [\"urgent\", \"home\"]\n    var count: TagCount = TagCount(list.count(tags))\n    return count\n}\n\nfunction has_urgent(tags: List<TagName>) -> HasUrgent {\n    return HasUrgent(list.contains(tags, \"urgent\"))\n}\n\nfunction letters(tags: List<TagName>) -> TagCount {\n    var mut total: TagCount = 0\n    for tag in tags {\n        if (tag != \"\") {\n            total = TagCount(total + 1)\n        }\n    }\n    return total\n}\n",
    );
    assert_eq!(value(&program, "tag_count", vec![]), int(2));
    let tags = support::list(vec![text("home"), text("urgent")]);
    assert_eq!(
        value(&program, "has_urgent", vec![tags.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        value(
            &program,
            "has_urgent",
            vec![support::list(vec![text("home")])]
        ),
        Value::Bool(false)
    );
    assert_eq!(value(&program, "letters", vec![tags]), int(2));
}

#[test]
fn a_share_is_divided_with_a_named_rounding() {
    // Question 8.
    let program = main(
        "type Price = Decimal {\n    scale: 2\n    min: 0.00\n}\n\ntype People = Int {\n    min: 1\n}\n\nfunction share(total: Price, people: People) -> Price {\n    var share: Price = Price(decimal.divide(total, people, rounding: Rounding.half_even))  // from zero up to the total\n    return share\n}\n",
    );
    assert_eq!(
        value(&program, "share", vec![decimal("10.00"), int(3)]).to_string(),
        "3.33"
    );
    assert_eq!(
        value(&program, "share", vec![decimal("0.05"), int(2)]).to_string(),
        "0.02",
        "0.025 rounds half to even"
    );
}

#[test]
fn a_pattern_matches_the_whole_text() {
    // Question 9.
    let program = main(
        "type Sku = Text {\n    pattern: \"[A-Z]{3}-[0-9]{4}\"\n}\n\ntype SkuRejected = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Written = Text {\n    max_length: 40\n}\n\nfunction first_sku() -> Sku {\n    var sku: Sku = \"ABC-1234\"  // checked while compiling\n    return sku\n}\n\nfunction parse_sku(written: Written) fails SkuRejected -> Sku {\n    var sku: Sku = attempt Sku(written) {\n        pattern: SkuRejected\n    }\n    return sku\n}\n",
    );
    assert_eq!(value(&program, "first_sku", vec![]), text("ABC-1234"));
    assert_eq!(
        value(&program, "parse_sku", vec![text("XYZ-0001")]),
        text("XYZ-0001")
    );
    let failed = failure(&program, "parse_sku", vec![text("ABC-1234x")]);
    assert_eq!(
        failed.broken.as_deref(),
        Some("`pattern: \"[A-Z]{3}-[0-9]{4}\"`")
    );
}

#[test]
fn equality_compares_contents() {
    // Question 12.
    let program = main(
        "type Amount = Decimal {\n    scale: 2\n}\n\ntype Coin = Enum {\n    pound\n    euro\n}\n\ntype Purse = Object {\n    amount: Amount\n    coin: Coin\n}\n\ntype Same = Bool {}\n\nfunction same() -> Same {\n    var a: Purse = Purse {\n        amount: 5.00\n        coin: Coin.pound\n    }\n    var b: Purse = Purse {\n        coin: Coin.pound\n        amount: 5.00\n    }\n    return Same(a == b)\n}\n\nfunction differ(a: Purse, b: Purse) -> Same {\n    return Same(a != b)\n}\n",
    );
    assert_eq!(value(&program, "same", vec![]), Value::Bool(true));
    let money = |amount: &str, coin: &str| {
        let coin = program.codec().variant("Coin", coin, vec![]).expect("Coin");
        program
            .codec()
            .object("Purse", vec![("amount", decimal(amount)), ("coin", coin)])
            .expect("Purse")
    };
    assert_eq!(
        value(
            &program,
            "differ",
            vec![money("5.00", "pound"), money("5.0", "pound")]
        ),
        Value::Bool(false),
        "5.00 and 5.0 are the same amount"
    );
    assert_eq!(
        value(
            &program,
            "differ",
            vec![money("5.00", "pound"), money("5.00", "euro")]
        ),
        Value::Bool(true)
    );
}

#[test]
fn uuids_are_normalised_to_lowercase() {
    // Question 13.
    let program = main(
        "type PartnerRef = Uuid {\n    version: UuidVersion.v4\n}\n\nfunction partner() -> PartnerRef {\n    return \"0192F0C4-3B5E-4C1A-9D2E-4F6A8B0C1D2E\"\n}\n",
    );
    assert_eq!(
        value(&program, "partner", vec![]),
        Value::uuid("0192f0c4-3b5e-4c1a-9d2e-4f6a8b0c1d2e")
    );
}

#[test]
fn a_text_literal_compared_with_a_value_takes_its_type() {
    // `==` gives a literal the other side's type (area 3), so text written
    // for a UUID, an instant or a date is read as one, not compared as text.
    let program = main(
        "type Ref = Uuid {}\n\ntype When = Instant {}\n\ntype Day = CalendarDate {}\n\ntype Same = Bool {}\n\nfunction known_ref(id: Ref) -> Same {\n    return Same(id == \"6f1c2b9e-3d4a-4f5b-8c7d-9e0f1a2b3c4d\")\n}\n\nfunction new_year(at: When) -> Same {\n    return Same(\"2026-01-01T00:00:00Z\" == at)\n}\n\nfunction not_today(day: Day?) -> Same {\n    return Same(day != \"2026-01-14\")\n}\n",
    );
    let at = |name: &str, ty: &str, text: &str| {
        let given = program.codec().text(ty, text).expect("a value");
        value(&program, name, vec![given])
    };
    assert_eq!(
        at("known_ref", "Ref", "6f1c2b9e-3d4a-4f5b-8c7d-9e0f1a2b3c4d"),
        Value::Bool(true)
    );
    assert_eq!(
        at("new_year", "When", "2026-01-01T00:00:00Z"),
        Value::Bool(true)
    );
    assert_eq!(at("not_today", "Day", "2026-01-14"), Value::Bool(false));
}

#[test]
fn a_list_written_beside_a_set_is_a_set() {
    // A list literal compared with a set takes the set's type, so order
    // and repeats don't matter, as they don't for sets (area 2).
    let program = main(
        "type Tag = Text {\n    max_length: 10\n}\n\ntype Ref = Uuid {}\n\ntype Same = Bool {}\n\ntype Labels = Object {\n    tags: Set<Tag>\n}\n\nfunction same_tags(labels: Labels) -> Same {\n    return Same(labels.tags == [\"b\", \"a\"])\n}\n\nfunction one_ref(refs: List<Ref>) -> Same {\n    return Same([\"6f1c2b9e-3d4a-4f5b-8c7d-9e0f1a2b3c4d\"] == refs)\n}\n",
    );
    let tags = program.codec().text("Tag", "a").expect("a");
    let other = program.codec().text("Tag", "b").expect("b");
    let labels = program
        .codec()
        .object("Labels", vec![("tags", Value::set(vec![tags, other]))])
        .expect("Labels");
    assert_eq!(
        value(&program, "same_tags", vec![labels]),
        Value::Bool(true)
    );
    let one = program
        .codec()
        .text("Ref", "6f1c2b9e-3d4a-4f5b-8c7d-9e0f1a2b3c4d")
        .expect("a ref");
    assert_eq!(
        value(&program, "one_ref", vec![list(vec![one])]),
        Value::Bool(true)
    );
}

#[test]
fn an_input_left_without_a_field_takes_its_default_or_none() {
    // A field an `Input` may leave out reads as `none`, and one with a
    // `default:` as that value (area 2, "Absent values").
    let program = main(
        "type ProductName = Text {\n    max_length: 120\n}\n\ntype Price = Decimal {\n    scale: 2\n}\n\ntype ListProducts = Input {\n    name: ProductName {\n        optional: true\n    }\n    page_size: PageSize {\n        default: 25\n    }\n    subtitle: ProductName? {\n        default: none\n    }\n    floor: Price {\n        default: 1.5\n    }\n    sort: Sort {\n        default: Sort.newest\n    }\n}\n\ntype Sort = Enum {\n    newest\n    cheapest\n}\n\nfunction first_page() -> ListProducts {\n    return ListProducts {}\n}\n",
    );
    assert_eq!(
        value(&program, "first_page", vec![]).to_string(),
        "ListProducts { name: none, page_size: 25, subtitle: none, floor: 1.50, sort: Sort.newest }"
    );
}
