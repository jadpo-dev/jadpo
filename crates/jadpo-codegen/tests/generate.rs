//! A checked project written as one Rust crate (roadmap chunk 8.1f;
//! docs/compiler-runtime.md §2.5, "Compiled code"): a module per domain,
//! each callable a function over the runtime's values, the plan's bytes
//! beside them, the same crate every time, and a body form not compiled
//! yet refused by name, so what is left shows.

use jadpo_codegen::{Generated, Options, generate};
use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::Plan;
use jadpo_std::RuntimePlan;

/// The plan of a project holding `files`, which must check cleanly.
fn plan(files: &[(&str, &str)]) -> Plan {
    let db = Database::default();
    let inputs: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(&db, path.to_string(), text.to_string()))
        .collect();
    let layout = if files.iter().any(|(path, _)| *path == "app.jadpo") {
        Layout::Project
    } else {
        Layout::Loose
    };
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        layout,
        Default::default(),
        Vec::new(),
    );
    let problems: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .filter(|diagnostic| jadpo_check::blocks_running(diagnostic))
        .map(|diagnostic| format!("{}: {}", diagnostic.rule.id, diagnostic.message))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
    Plan::build(&db, project)
}

fn options() -> Options {
    Options {
        name: "jadpo-compiled-test".into(),
        runtime: "../crates".into(),
    }
}

fn text<'g>(generated: &'g Generated, path: &str) -> &'g str {
    let bytes = generated
        .files
        .get(path)
        .unwrap_or_else(|| panic!("no {path} in {:?}", generated.files.keys()));
    std::str::from_utf8(bytes).expect("text")
}

const PRICES: &str = r#"type Basket = Object {
    loaves: Int
    each: Int
}

type Bill = Object {
    pence: Int
}

function line_total(basket: Basket) -> Bill {
    return Bill {
        pence: basket.loaves * basket.each
    }
}

function with_bag(basket: Basket) -> Bill {
    if (basket.loaves > 3) {
        return Bill {
            pence: line_total(basket).pence + 5
        }
    }
    return line_total(basket)
}
"#;

const PRICES_TEST: &str = r#"test "four loaves come with a bag" {
    var basket: Basket = Basket {
        loaves: 4
        each: 250
    }
    assert with_bag(basket).pence == 1005
}
"#;

/// A project is a crate: its manifest, its plan's bytes with and without
/// the dev and test section, the crate root with the frame table and the
/// callables by number, and a module of functions; the same project gives
/// the same bytes.
#[test]
fn a_project_is_one_crate_the_same_each_time() {
    let planned = plan(&[("main.jadpo", PRICES), ("main.test.jadpo", PRICES_TEST)]);
    let generated = generate(&planned, &options()).expect("it compiles");
    let paths: Vec<&str> = generated.files.keys().map(String::as_str).collect();
    assert_eq!(
        paths,
        [
            "Cargo.toml",
            "plan-testing.bin",
            "plan.bin",
            "src/app.rs",
            "src/entries.rs",
            "src/lib.rs"
        ]
    );
    let manifest = text(&generated, "Cargo.toml");
    assert!(
        manifest.contains("name = \"jadpo-compiled-test\""),
        "{manifest}"
    );
    assert!(
        manifest.contains("jadpo-runtime = { path = \"../crates/jadpo-runtime\" }"),
        "{manifest}"
    );
    assert!(
        manifest.contains("testing = [\"jadpo-runtime/testing\", \"jadpo-std/testing\"]"),
        "{manifest}"
    );

    // The plan as compiled code reads it back, a deployable's without the
    // dev and test section.
    let deployable = generated.files.get("plan.bin").expect("plan.bin");
    let read = RuntimePlan::decode(deployable).expect("its bytes read back");
    assert!(read.testing.tests.is_empty());
    assert_eq!(read.sites, planned.runtime().sites);
    let testing = generated
        .files
        .get("plan-testing.bin")
        .expect("plan-testing.bin");
    assert_eq!(
        RuntimePlan::decode(testing).expect("reads"),
        *planned.runtime()
    );

    let lib = text(&generated, "src/lib.rs");
    assert!(lib.contains("pub fn program()"), "{lib}");
    assert!(lib.contains("include_bytes!(\"../plan.bin\")"), "{lib}");
    assert!(lib.contains("label: \"function line_total\""), "{lib}");

    // Functions are plain functions; arithmetic is checked at its site;
    // the test is compiled only for a build with the `testing` feature.
    let module = text(&generated, "src/app.rs");
    assert!(
        module.contains("pub(crate) fn c0_function_line_total("),
        "{module}"
    );
    assert!(
        module.contains("rt::binary(cx, Operator::Multiply"),
        "{module}"
    );
    assert!(
        module.contains("#[cfg(feature = \"testing\")]\npub(crate) async fn c2_test_four_loaves_come_with_a_bag("),
        "{module}"
    );

    let again = generate(
        &plan(&[("main.jadpo", PRICES), ("main.test.jadpo", PRICES_TEST)]),
        &options(),
    )
    .expect("it compiles");
    assert_eq!(generated, again);
}

/// Several projects as modules of one crate (chunk 8.1j), so the corpus is
/// one rustc invocation: each project's files under `src/p<n>/`, its
/// plan's bytes beside its root, its code the code it has as a crate of
/// its own, and the crate's root declaring each module and listing each
/// program by its place in the list.
#[test]
fn projects_are_modules_of_one_crate() {
    let planned = plan(&[("main.jadpo", PRICES), ("main.test.jadpo", PRICES_TEST)]);
    let alone = generate(&planned, &options()).expect("it compiles");
    let module = jadpo_codegen::module(&planned, 7).expect("it compiles");
    let paths: Vec<&str> = module.files.keys().map(String::as_str).collect();
    assert_eq!(
        paths,
        [
            "src/p7/app.rs",
            "src/p7/entries.rs",
            "src/p7/mod.rs",
            "src/p7/plan-testing.bin",
            "src/p7/plan.bin"
        ]
    );
    let root = text(&module, "src/p7/mod.rs");
    assert!(root.contains("include_bytes!(\"plan.bin\")"), "{root}");
    assert!(root.contains("pub fn program()"), "{root}");
    // The callables name each other from the project's root, so a module
    // per domain reads the same in either layout.
    assert_eq!(text(&module, "src/p7/app.rs"), text(&alone, "src/app.rs"));
    assert!(
        text(&alone, "src/app.rs").contains("super::app::c0_function_line_total(cx, p,"),
        "{}",
        text(&alone, "src/app.rs")
    );
    assert_eq!(module.files["src/p7/plan.bin"], alone.files["plan.bin"]);

    let holding = jadpo_codegen::holding(&[7, 2], &options()).expect("it is Rust");
    let paths: Vec<&str> = holding.files.keys().map(String::as_str).collect();
    assert_eq!(paths, ["Cargo.toml", "src/lib.rs"]);
    assert_eq!(holding.files["Cargo.toml"], alone.files["Cargo.toml"]);
    let lib = text(&holding, "src/lib.rs");
    assert!(lib.contains("pub mod p7;\npub mod p2;"), "{lib}");
    assert!(lib.contains("= &[p7::program, p2::program];"), "{lib}");
}

/// A shop that prices a line: a route with an input of an enum, a
/// number with rules, an optional note and a list, a placeholder, and an
/// output with a payload variant.
const SHOP: &str = r#"type Quantity = Int {
    min: 1
    max: 99
}

type ShopNumber = Int {
    min: 1
}

type Coins = Int {}

type Extra = Text {
    max_length: 40
}

type Note = Text {
    max_length: 200
}

type RibbonColour = Text {
    max_length: 20
}

type Size = Enum {
    small
    large
}

type Wrapping = Enum {
    plain
    ribbon: {
        colour: RibbonColour
    }
}

type OrderLine = Input {
    size: Size
    quantity: Quantity
    note: Note?
    extras: List<Extra>
}

type Priced = Output {
    size: Size
    pence: Coins
    wrapping: Wrapping
}

action price(shop: ShopNumber, input: OrderLine) -> Priced {
    return Priced {
        size: input.size
        pence: Coins(input.quantity * 250 + shop)
        wrapping: Wrapping.plain
    }
}

policy {
    actions: {
        price: [Access.public]
    }
}

routes Shop {
    prefix: /shops

    POST /{shop: ShopNumber}/price {
        authentication: RouteAuthentication.none
        input: OrderLine
        output: Priced
        run: price(shop, input)
    }
}
"#;

/// Each route has a typed entry (chunk 8.1p): its placeholder read from
/// text, its input's members by name in declared order with the members
/// it doesn't declare refused, its enum's variants by their wire names,
/// and its output written member by member; each of its types' codecs a
/// function of its own, the runtime's rules called by number.
#[test]
fn a_route_is_read_and_written_by_its_typed_entry() {
    let planned = plan(&[("main.jadpo", SHOP)]);
    let generated = generate(&planned, &options()).expect("it compiles");
    let entries = text(&generated, "src/entries.rs");
    assert!(
        entries.contains("pub(super) fn entry(n: u32) -> Option<&'static Entry>"),
        "{entries}"
    );
    assert!(
        entries.contains("static ROUTE_0: Entry = Entry {"),
        "{entries}"
    );
    assert!(entries.contains("params: &[p"), "{entries}");
    assert!(entries.contains("input: Some(i"), "{entries}");
    assert!(entries.contains("output: Some(w"), "{entries}");
    // The input's fields in declared order, then the members it doesn't
    // declare.
    let size = entries.find("At::Member(at, \"size\")").expect("size");
    let quantity = entries
        .find("At::Member(at, \"quantity\")")
        .expect("quantity");
    let note = entries.find("At::Member(at, \"note\")").expect("note");
    assert!(size < quantity && quantity < note, "{entries}");
    assert!(
        entries.contains("r.unknown(m, &[\"size\", \"quantity\", \"note\", \"extras\"], at);"),
        "{entries}"
    );
    assert!(entries.contains("r.list(item, at, r"), "{entries}");
    // The enum by its wire names, a payload variant by its tag.
    assert!(
        entries.contains("\"small\" => Some(Value::variant(\"Size\", 0, \"small\", Vec::new())),"),
        "{entries}"
    );
    assert!(entries.contains("let tag = r.tag(&m, at)?;"), "{entries}");
    // The output member by member, a variant's tag and fields.
    assert!(
        entries.contains("out.push_str(\"\\\"pence\\\":\");"),
        "{entries}"
    );
    assert!(
        entries.contains("out.push_str(\"{\\\"tag\\\":\\\"ribbon\\\"\");"),
        "{entries}"
    );
    let lib = text(&generated, "src/lib.rs");
    assert!(lib.contains("routes: entries::entry,"), "{lib}");
}

/// Areas 1 to 4 (chunk 8.1g): a loop, the standard library, an inline
/// function, a construction and its `attempt`, a `reject` and an outcome
/// `match` compile, each through the runtime's site functions.
#[test]
fn the_forms_of_areas_one_to_four_compile() {
    let planned = plan(&[(
        "main.jadpo",
        r#"type Count = Int {}

type CakeCount = Int {
    min: 1
    max: 12
}

type Coins = Int {
    min: 0
}

type OrderLine = Object {
    subtotal: Coins
}

type OrderEmpty = Failure {
    kind: FailureKind.invalid_value
}

type OrderTooLarge = Failure {
    kind: FailureKind.invalid_value
}

type LookupCode = Text {}

type LookupResult = Text {}

type LookupMissing = Failure {
    kind: FailureKind.not_found
}

function from_count(n: Count) fails OrderEmpty, OrderTooLarge -> CakeCount {
    var result: CakeCount = attempt CakeCount(n) {
        max: OrderTooLarge
        invalid: OrderEmpty
    }
    return result
}

function order_total(lines: List<OrderLine>) -> Coins {
    var mut total: Coins = 0
    for line in lines {
        total = Coins(total + line.subtotal)
    }
    return total
}

function dear_lines(lines: List<OrderLine>) -> Count {
    return Count(list.count(list.filter(lines, line -> line.subtotal > 100)))
}

function lookup(key: LookupCode) fails LookupMissing -> LookupResult {
    if (key == "missing") {
        reject LookupMissing
    }
    return "found"
}

function lookup_or_default(key: LookupCode) -> LookupResult {
    return match lookup(key) {
        success(value) -> value
        LookupMissing -> "not found"
    }
}
"#,
    )]);
    let generated = generate(&planned, &options()).expect("it compiles");
    let module = text(&generated, "src/app.rs");
    for expected in [
        "for item in items.iter()",
        "rt::standard(cx, p, jadpo_std::Op::ListCount",
        "rt::standard_each(",
        "rt::construct(",
        "rt::rule_arm(",
        "rt::raised(",
        "Err(Stop::Failed(failure)) if failure.key ==",
    ] {
        assert!(module.contains(expected), "{expected}:\n{module}");
    }
}

/// The test forms of area 13 (chunk 8.1h) compile: a test's clock moved
/// on, after which what came due is delivered, and a schedule run by
/// hand. The corpus runs the rest: callers, sessions, tokens and keys,
/// batches and an export's rows.
#[test]
fn the_test_forms_of_area_thirteen_compile() {
    let planned = plan(&[
        ("main.jadpo", PRICES),
        (
            "tidy.jadpo",
            r#"/// Tidies up.
action tidy() -> Unit {}

/// Tidies up every quarter of an hour.
schedule tidy_up {
    every: 15m
    run: tidy()
}
"#,
        ),
        (
            "main.test.jadpo",
            r#"test "an hour later" {
    clock.advance(1h)
    tidy_up.run()
    assert line_total(Basket {
        loaves: 1
        each: 2
    }).pence == 2
}
"#,
        ),
    ]);
    let generated = generate(&planned, &options()).expect("it compiles");
    let module = text(&generated, "src/app.rs");
    for expected in [
        "rt::advance(cx, by, 1)",
        "rt::step(cx, p, true, 1, ran)",
        "rt::schedule(cx, p, ",
    ] {
        assert!(module.contains(expected), "{expected}:\n{module}");
    }
}

/// A translation (area 10) is compiled as a call of the runtime's entry
/// point for it, which the interpreter calls too (chunk 8.1i), by its key,
/// its values in order with the locale first: the last form chunk 8.1h's
/// generator refused by name.
#[test]
fn a_translation_is_the_runtimes_to_write() {
    let planned = plan(&[
        (
            "app.jadpo",
            "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n",
        ),
        (
            "domains/orders/types/discount_rate.jadpo",
            "/// A discount off an order.\ntype DiscountRate = Percentage {}\n",
        ),
        (
            "domains/orders/translations/order_discount.jadpo",
            "use orders/types { DiscountRate }\n\n/// The discount on an order.\ntype OrderDiscount = Message {\n    reduction: DiscountRate\n}\n",
        ),
        (
            "domains/orders/translations/en_gb.jadpo",
            "translations {\n    order_discount: \"You saved {reduction, number, percent}.\"\n}\n",
        ),
        (
            "domains/orders/actions/confirm_discount.jadpo",
            r#"use orders/types { DiscountRate }

/// The order's discount, in the customer's language.
action confirm_discount(reduction: DiscountRate) -> PresentationText {
    return translations.orders.order_discount(reduction: reduction, locale: Locale.en_gb)
}
"#,
        ),
    ]);
    let generated = generate(&planned, &options()).expect("it compiles");
    let module = text(&generated, "src/orders.rs");
    assert!(
        module.contains("rt::translate(cx, p, \"orders.order_discount\", &[a0, a1], 1)"),
        "{module}"
    );
}
