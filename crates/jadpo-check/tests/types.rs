//! Area 2, values and types: its decided examples check cleanly, each of
//! its errors is caught on its own mistake, one mistake gives one error,
//! and the widening table holds.

use crate::support;

use jadpo_check::StubSignature;
use jadpo_db::Layout;
use support::rules;

fn check(source: &str) -> Vec<String> {
    rules(Layout::Loose, &[("main.jadpo", source)])
}

const NAMES: &str = "type ProductName = Text {\n    min_length: 1\n    max_length: 120\n}\n\ntype Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype Price = Decimal {\n    scale: 2\n    min: 0.00\n}\n\n";

fn with_names(source: &str) -> Vec<String> {
    check(&format!("{NAMES}{source}"))
}

#[test]
fn named_types_refinements_and_literals_check() {
    assert_eq!(
        with_names(
            "type OrderId = Uuid {}\n\ntype ShortProductName = ProductName {\n    max_length: 40\n}\n\nfunction catalogue_name(name: ShortProductName) -> ProductName {\n    return name\n}\n\nfunction starting_quantity() -> Quantity {\n    return 1\n}\n\nfunction free() -> Price {\n    return 0.00\n}\n\nfunction short() -> ShortProductName {\n    var name: ShortProductName = \"Mug\"\n    return name\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn objects_inputs_outputs_enums_and_collections_check() {
    assert_eq!(
        with_names(
            "type PriceTag = Object {\n    amount: Pence\n}\n\ntype CreateProduct = Input {\n    name: ProductName\n    price: Pence {\n        default: 0\n    }\n    subtitle: ProductName? {\n        default: none\n    }\n}\n\ntype ProductView = Output {\n    name: ProductName\n    price: PriceTag\n    tags: List<ProductName> {\n        max_length: 10\n    }\n}\n\ntype TicketStatus = Enum {\n    open\n    closed\n}\n\nfunction view(request: CreateProduct) -> ProductView {\n    return ProductView {\n        name: request.name\n        price: PriceTag {\n            amount: request.price\n        }\n        tags: [\"mugs\", \"kitchen\"]\n    }\n}\n\nfunction reopen() -> TicketStatus {\n    return TicketStatus.open\n}\n\nfunction sample() -> CreateProduct {\n    return CreateProduct {\n        name: \"Mug\"\n    }\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn each_mistake_gives_its_rule() {
    let cases: &[(&str, &str)] = &[
        (
            "function add(a: Pence, b: Pence) -> Pence {\n    return a + b\n}\n",
            "type.mismatch",
        ),
        (
            "function first() -> Quantity {\n    return 0\n}\n",
            "type.invalid_literal",
        ),
        (
            "function first() -> Quantity {\n    return Quantity(0)\n}\n",
            "type.invalid_literal",
        ),
        (
            "function cheap() -> Price {\n    return 1.255\n}\n",
            "type.invalid_literal",
        ),
        (
            "function cheap() -> Price {\n    return 1\n}\n",
            "type.mismatch",
        ),
        (
            "type OrderId = Uuid {}\n\nfunction order_for(name: ProductName) -> OrderId {\n    return OrderId(name)\n}\n",
            "type.constructor_input",
        ),
        (
            "function pair() -> Quantity {\n    return Quantity(1, 2)\n}\n",
            "type.constructor_argument_count",
        ),
        (
            "type Receipt = Output {\n    sent_to: Email\n}\n\nfunction blank() -> Receipt {\n    return Receipt {}\n}\n",
            "type.missing_field",
        ),
        (
            "type Thing = Object {\n    name: ProductName\n}\n\nfunction make(name: ProductName) -> Thing {\n    return Thing {\n        name: name\n        label: name\n    }\n}\n",
            "type.unknown_field",
        ),
        (
            "type Thing = Object {\n    name: ProductName\n}\n\nfunction make(name: ProductName) -> Thing {\n    return Thing {\n        name: name\n        name: name\n    }\n}\n",
            "type.duplicate_field",
        ),
        (
            "function blank() -> ProductName {\n    return ProductName {}\n}\n",
            "type.not_record",
        ),
        (
            "function raw(name: ProductName) -> ProductName {\n    return name.value\n}\n",
            "type.field_on_non_record",
        ),
        (
            "type Thing = Object {\n    name: ProductName\n}\n\nfunction name_of(thing: Thing?) -> ProductName {\n    return thing.name\n}\n",
            "type.nullable_selection",
        ),
        (
            "type Listing = Object {\n    subtitle: ProductName?\n}\n\ntype Preview = Object {\n    subtitle: Listing.subtitle?\n}\n",
            "type.redundant_nullability",
        ),
        (
            "function normalise(value: Text) -> ProductName {\n    return ProductName(value)\n}\n",
            "type.primitive_signature",
        ),
        (
            "function all(names: List<Text>) -> ProductName {\n    return \"x\"\n}\n",
            "type.primitive_signature",
        ),
        (
            "type Status = Enum {\n    open\n}\n\nfunction reopen() -> Status {\n    return Status.opne\n}\n",
            "type.unknown_enum_variant",
        ),
        (
            "type Status = Enum {\n    open\n}\n\nfunction reopen() -> Status {\n    return Status(\"open\")\n}\n",
            "type.constructor_input",
        ),
        ("type Status = Enum {}\n", "type.enum_empty"),
        (
            "type Status = Enum {\n    open\n    open\n}\n",
            "type.duplicate_variant",
        ),
        (
            "type Outcome = Enum {\n    paid: {\n        amount: Pence\n    }\n}\n\nfunction paid() -> Outcome {\n    return Outcome.paid {}\n}\n",
            "type.missing_variant_field",
        ),
        (
            "type Outcome = Enum {\n    paid: {\n        amount: Pence\n    }\n}\n\nfunction paid(amount: Pence) -> Outcome {\n    return Outcome.paid {\n        amount: amount\n        extra: amount\n    }\n}\n",
            "type.unknown_variant_field",
        ),
        (
            "type Outcome = Enum {\n    paid: {\n        amount: Pence\n    }\n}\n\nfunction paid(amount: Pence) -> Outcome {\n    return Outcome.paid {\n        amount: amount\n        amount: amount\n    }\n}\n",
            "type.duplicate_variant_field",
        ),
        (
            "type Label = Text {\n    maxlength: 3\n}\n",
            "type.unknown_setting",
        ),
        (
            "type Label = Text {\n    max_length: many\n}\n",
            "type.setting_value",
        ),
        (
            "type Label = Text {\n    format: email\n}\n",
            "type.setting_value",
        ),
        (
            "type Label = Text {\n    min: 1\n}\n",
            "type.constraint_base",
        ),
        (
            "type Label = Text {\n    max_length: 3\n    max_length: 4\n}\n",
            "type.constraint_repeated",
        ),
        (
            "type Label = Text {\n    min_length: 4\n    max_length: 3\n}\n",
            "type.constraint_contradiction",
        ),
        (
            "type Count = Int {\n    min: 5\n    max: 1\n}\n",
            "type.constraint_contradiction",
        ),
        (
            "type Label = Text {\n    pattern: \"(?=a)a\"\n}\n",
            "type.invalid_pattern",
        ),
        (
            "type Label = Text {\n    pattern: \"[a-\"\n}\n",
            "type.invalid_pattern",
        ),
        (
            "type Amount = Decimal {\n    min: 0.0\n}\n",
            "type.decimal_scale_required",
        ),
        (
            "type Profile = Object {\n    name: ProductName {\n        optional: true\n    }\n}\n",
            "type.optional_outside_input",
        ),
        (
            "type Profile = Output {\n    name: ProductName\n    optional: all\n}\n",
            "type.optional_outside_input",
        ),
        (
            "type Flag = Bool {}\n\ntype Settings = Object {\n    optional: Flag\n}\n",
            "type.reserved_field_name",
        ),
        (
            "type Holder = Object {\n    details: Object\n}\n",
            "type.base_outside_declaration",
        ),
        (
            "type Holder = Object {\n    names: List<ProductName, ProductName>\n}\n",
            "type.type_argument_count",
        ),
        (
            "type Holder = Object {\n    names: List\n}\n",
            "type.type_argument_count",
        ),
        (
            "entity Item {\n    id: Uuid\n    name: ProductName\n}\n\ntype CreateItem = Input {\n    include: [Item]\n}\n",
            "type.copy_into_input",
        ),
        (
            "entity Item {\n    id: Uuid\n    name: ProductName\n}\n\ntype ItemView = Output {\n    include: [Item]\n    omit: [Item.colour]\n}\n",
            "type.absent_field",
        ),
        (
            "entity Item {\n    id: Uuid\n    name: ProductName\n}\n\nentity Box {\n    id: Uuid\n}\n\ntype ItemView = Output {\n    include: [Item]\n    pick: [Box.id]\n}\n",
            "type.copy_clash",
        ),
        (
            "entity Item {\n    id: Uuid\n}\n\ntype ItemView = Output {\n    pick: [Item.colour]\n}\n",
            "type.unknown_field",
        ),
        (
            "type ItemView = Output {\n    include: [ProductName]\n}\n",
            "type.not_record",
        ),
        (
            "type ItemView = Output {\n    pick: Pence\n}\n",
            "type.setting_value",
        ),
        (
            "type Search = Input {\n    size: Quantity {\n        default: 0\n    }\n}\n",
            "type.invalid_literal",
        ),
        (
            "type Search = Input {\n    size: Quantity {\n        default: none\n    }\n}\n",
            "type.mismatch",
        ),
        (
            "entity Item {\n    id: Uuid\n}\n\ntype ItemView = Output {\n    id: Item.colour\n}\n",
            "type.unknown_field",
        ),
        (
            "entity Customer {\n    id: Uuid\n}\n\nentity Supplier {\n    id: Uuid\n}\n\nfunction keep(id: Customer.id) -> Customer.id {\n    return id\n}\n\nfunction misuse(supplier: Supplier) -> Customer.id {\n    return keep(supplier.id)\n}\n",
            "type.sibling_mismatch",
        ),
    ];
    for (source, rule) in cases {
        assert_eq!(with_names(source), [*rule], "for:\n{source}");
    }
}

#[test]
fn one_mistake_gives_one_error() {
    // An undeclared type is reported once, by resolution; everything built
    // from it is `Error` and checks quietly.
    assert_eq!(
        with_names(
            "function add(a: Pence, b: Missing) -> Pence {\n    var c: Pence = b\n    var d: Pence = b.amount + a\n    return Pence(b.total + c + d)\n}\n"
        ),
        ["semantic.unknown_name"]
    );
    // A field type naming a field that doesn't exist: one error at the
    // declaration, none where the field is read or built.
    assert_eq!(
        with_names(
            "type Holder = Object {\n    name: ProductName\n    price: Holder.cost\n}\n\nfunction make(name: ProductName, price: Pence) -> Holder {\n    return Holder {\n        name: name\n        price: price\n    }\n}\n"
        ),
        ["type.unknown_field"]
    );
    // An unknown variant gives one error, not a mismatch on the return too.
    assert_eq!(
        with_names(
            "type Status = Enum {\n    open\n}\n\nfunction f() -> Status {\n    var s: Status = Status.shut\n    return s\n}\n"
        ),
        ["type.unknown_enum_variant"]
    );
}

/// The table "When one type can stand in for another", row by row.
#[test]
fn widening_follows_declared_steps_only() {
    let entities = "entity Customer {\n    id: Uuid\n    email: Email\n}\n\nentity Supplier {\n    id: Uuid\n    email: Email\n}\n\ntype Note = Text {}\n\n";
    let case = |param: &str, result: &str| {
        check(&format!(
            "{entities}function f(value: {param}) -> {result} {{\n    return value\n}}\n"
        ))
    };
    let ok = Vec::<String>::new();
    assert_eq!(case("Customer.email", "Customer.email"), ok);
    assert_eq!(case("Customer.email", "Email"), ok);
    assert_eq!(case("Supplier.email", "Email"), ok);
    assert_eq!(case("Email", "Email"), ok);
    assert_eq!(
        case("Supplier.email", "Customer.email"),
        ["type.sibling_mismatch"]
    );
    assert_eq!(case("Email", "Customer.email"), ["type.mismatch"]);
    assert_eq!(case("Customer.email", "Note"), ["type.mismatch"]);
    // Nullable lifting: `Customer.email` where `Email?` is expected.
    assert_eq!(case("Customer.email", "Email?"), ok);
    assert_eq!(case("Email?", "Email"), ["type.mismatch"]);
    // Collections keep their element type.
    assert_eq!(
        case("List<Customer.email>", "List<Email>"),
        ["type.mismatch"]
    );
    // Converting on purpose goes through the field's constructor.
    assert_eq!(
        check(&format!(
            "{entities}function f(value: Email) -> Customer.email {{\n    return Customer.email(value)\n}}\n"
        )),
        ok
    );
}

#[test]
fn reading_through_objects_gives_field_types() {
    let source = "type ShippingCode = Text {\n    max_length: 10\n}\n\ntype Address = Object {\n    postal_code: ShippingCode\n}\n\ntype Customer = Object {\n    id: Uuid\n    billing_address: Address\n    shipping_address: Address\n}\n\nfunction billing(customer: Customer) -> Address.postal_code {\n    return customer.billing_address.postal_code\n}\n\nfunction shipping(customer: Customer) -> Address.postal_code {\n    return customer.shipping_address.postal_code\n}\n\nfunction plain(customer: Customer) -> ShippingCode {\n    return customer.billing_address.postal_code\n}\n";
    assert_eq!(check(source), Vec::<String>::new());
}

#[test]
fn copied_fields_follow_their_source() {
    let source = "type Title = Text {\n    max_length: 80\n}\n\ntype Notes = Text {\n    max_length: 500\n}\n\nentity Task {\n    id: Uuid\n    name: Title\n    notes: Notes?\n}\n\ntype TaskView = Output {\n    include: [Task]\n    omit: [Task.notes]\n}\n\ntype PatchTask = Input {\n    pick: [Task.name, Task.notes]\n    optional: all\n}\n\nfunction view(task: Task) -> TaskView {\n    return TaskView {\n        id: task.id\n        name: task.name\n    }\n}\n\nfunction new_name(patch: PatchTask) -> Task.name? {\n    return patch.name\n}\n\nfunction short_name() -> TaskView {\n    return TaskView {\n        id: \"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\"\n        name: \"Too long a name for this field, at well over eighty characters in total, which the title type does not allow\"\n    }\n}\n";
    // The literal takes `TaskView.name`, whose rules come from `Task.name`
    // and `Title`.
    assert_eq!(check(source), ["type.invalid_literal"]);
}

#[test]
fn standard_library_calls_take_their_types_from_the_arguments() {
    let source = "type Tag = Text {\n    max_length: 5\n}\n\ntype TagCount = Int {\n    min: 0\n}\n\ntype HasTag = Bool {}\n\nfunction count(tags: List<Tag>) -> TagCount {\n    return TagCount(list.count(tags))\n}\n\nfunction has(tags: List<Tag>) -> HasTag {\n    return HasTag(list.contains(tags, \"home\"))\n}\n\nfunction too_long(tags: List<Tag>) -> HasTag {\n    return HasTag(list.contains(tags, \"kitchen\"))\n}\n";
    assert_eq!(check(source), ["type.invalid_literal"]);
}

#[test]
fn types_later_areas_define_are_left_to_them() {
    // `Page<T>` (area 6) passes around as itself; `Response`'s fields are
    // typed since chunk 4.1 (area 13), so a status is an `Int`.
    assert_eq!(
        with_names(
            "function first(page: Page<ProductName>) -> Page<ProductName> {\n    return page\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn built_in_values_are_built_from_literals_only() {
    assert_eq!(
        with_names(
            "type DueAt = Instant {}\n\nfunction due() -> DueAt {\n    return DueAt(Instant(\"2026-10-25T08:30:00Z\"))\n}\n\nfunction later(at: DueAt) -> DueAt {\n    return at\n}\n"
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        with_names(
            "function raw(name: ProductName) -> ProductName {\n    var raw: Text = Text(name)\n    return ProductName(raw)\n}\n"
        ),
        // `ProductName(text)` is a second mistake: a `Text` value may break
        // `ProductName`'s rules, so building one can fail (area 4).
        ["type.constructor_input", "failure.attempt_required"]
    );
}

#[test]
fn loops_and_failures_read_their_types() {
    assert_eq!(
        with_names(
            "type Line = Object {\n    price: Pence\n}\n\ntype Basket = Object {\n    lines: List<Line>\n}\n\ntype PriceTooHigh = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        limit: Pence\n    }\n}\n\nfunction first(basket: Basket) -> Pence {\n    for line in basket.lines {\n        return line.price\n    }\n    return 0\n}\n"
        ),
        Vec::<String>::new()
    );
}

/// How a stub writes a parameter of this kind: a value limited to some
/// built-in types is a type variable, and a clock time is text.
fn stub_kind(kind: jadpo_std::Kind) -> jadpo_std::Kind {
    match kind {
        jadpo_std::Kind::OneOf(_) | jadpo_std::Kind::Contract => jadpo_std::Kind::Any,
        jadpo_std::Kind::Clock => jadpo_std::Kind::Builtin(jadpo_std::Builtin::Text),
        other => other,
    }
}

#[test]
fn every_stub_matches_the_operation_table() {
    // The stubs (crates/jadpo-check/std) declare what programs call; the
    // table in jadpo-std lists the same operations, typed, each with the
    // Rust function in jadpo-value that runs it.
    let stubs: Vec<StubSignature> = jadpo_check::standard_operations();
    let table: Vec<StubSignature> = jadpo_std::OPERATIONS
        .iter()
        .map(|operation| StubSignature {
            name: operation.qualified(),
            // A value the checker fills is never written, so no stub
            // declares it (`list.sum`'s `zero`).
            params: operation
                .params
                .iter()
                .filter(|param| !param.implicit)
                .map(|param| {
                    (
                        param.name.to_string(),
                        Some(stub_kind(param.kind)),
                        param.optional || param.nullable,
                    )
                })
                .collect(),
            result: Some(stub_kind(operation.result)),
        })
        .collect();
    assert_eq!(stubs, table);
}

#[test]
fn standard_operations_check_their_arguments_shape_and_kind() {
    let cases: &[(&str, &[&str])] = &[
        // A list operation takes a list, whatever its items are.
        (
            "function f(n: Pence) -> Quantity {\n    return Quantity(list.count(n))\n}\n",
            &["type.mismatch"],
        ),
        (
            "function f(n: Pence) -> List<Pence> {\n    return list.map(n, x -> x)\n}\n",
            &["type.mismatch"],
        ),
        // Division takes numbers, built on `Int` for `integer.divide`.
        (
            "function f(a: ProductName, b: ProductName) -> Pence {\n    return Pence(integer.divide(a, b, rounding: Rounding.up))\n}\n",
            &["type.mismatch", "type.mismatch"],
        ),
        (
            "function f(a: Price, b: Quantity) -> Pence {\n    return Pence(integer.divide(a, b, rounding: Rounding.up))\n}\n",
            &["type.mismatch"],
        ),
        (
            "function f(total: Price, people: Quantity) -> Tip {\n    return attempt Tip(decimal.divide(total, people, rounding: Rounding.half_even)) {\n        invalid: Refused\n    }\n}\n\ntype Tip = Decimal {\n    scale: 2\n    min: 0.00\n    max: 1.00\n}\n\ntype Refused = Failure {\n    kind: FailureKind.invalid_value\n}\n",
            &["failure.undeclared_propagation"],
        ),
        (
            "function f(total: Pence, boxes: Quantity) -> Quantity {\n    return Quantity(integer.divide(total, boxes, rounding: Rounding.up))\n}\n",
            &["failure.attempt_required"],
        ),
        // A division of a value never below zero by a divisor of at least
        // 1 is proved like `/`: from zero up to the dividend.
        (
            "function f(total: Pence, boxes: Quantity) -> Pence {\n    return Pence(integer.divide(total, boxes, rounding: Rounding.up))\n}\n",
            &[],
        ),
        // An empty list fixes no type variable, and needs none.
        (
            "function f() -> Pence {\n    return Pence(list.count([]))\n}\n",
            &[],
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(&with_names(source), expected, "{source}");
    }
}

#[test]
fn formatting_is_checked_like_any_operation() {
    // `text.format` is checked (areas 10 and 19): it needs a locale, and
    // it gives text for people, which isn't a number.
    assert_eq!(
        with_names(
            "function f(n: Pence) -> Pence {\n    var formatted_price: Pence = text.format(n)\n    return formatted_price\n}\n"
        ),
        ["type.mismatch", "type.argument_count"]
    );
    // `temporal.format` moved to `text.format` (area 19, question 2).
    assert_eq!(
        with_names(
            "type At = Instant {}\n\nfunction f(at: At) -> PresentationText {\n    return temporal.format(at, zone: Zone.utc, locale: Locale.en_gb, format: TimeFormat.date_long)\n}\n"
        ),
        ["semantic.standard_operation_moved"]
    );
}
