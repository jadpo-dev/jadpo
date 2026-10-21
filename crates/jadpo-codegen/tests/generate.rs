//! A checked project written as one Rust crate (roadmap chunk 8.1f;
//! docs/compiler-runtime.md §2.5, "Compiled code"): a module per domain,
//! each callable a function over the runtime's values, the plan's bytes
//! beside them, the same crate every time, and a body form not compiled
//! yet refused by name, so what is left shows.

use jadpo_codegen::{Error, Generated, Options, generate};
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

/// A project is one crate: its manifest, its plan's bytes with and without
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

type Pence = Int {
    min: 0
}

type OrderLine = Object {
    subtotal: Pence
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

function order_total(lines: List<OrderLine>) -> Pence {
    var mut total: Pence = 0
    for line in lines {
        total = Pence(total + line.subtotal)
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

/// A body form the generator doesn't compile yet is refused by its name
/// and place, every one of them, rather than written as code that faults
/// when a test reaches it.
#[test]
fn a_body_form_not_compiled_yet_is_refused_by_name() {
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
    let Err(Error::Refused(refusals)) = generate(&planned, &options()) else {
        panic!("refused");
    };
    let shown: Vec<String> = refusals.iter().map(ToString::to_string).collect();
    assert_eq!(
        shown,
        [
            "`translations.<message>` isn't compiled yet: domains/orders/actions/confirm_discount.jadpo:5, in action confirm_discount"
        ]
    );
}
