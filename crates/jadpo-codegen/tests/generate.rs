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

/// A body form the generator doesn't compile yet is refused by its name
/// and place, every one of them, rather than written as code that faults
/// when a test reaches it.
#[test]
fn a_body_form_not_compiled_yet_is_refused_by_name() {
    let planned = plan(&[(
        "main.jadpo",
        r#"type Basket = Object {
    prices: List<Int>
}

type Bill = Object {
    pence: Int
}

function total(basket: Basket) -> Bill {
    var mut sum: Int = 0
    for price in basket.prices {
        sum = sum + price
    }
    return Bill {
        pence: sum
    }
}

function counted(basket: Basket) -> Bill {
    return Bill {
        pence: list.count(basket.prices)
    }
}
"#,
    )]);
    let Err(Error::Refused(refusals)) = generate(&planned, &options()) else {
        panic!("refused");
    };
    let shown: Vec<String> = refusals.iter().map(ToString::to_string).collect();
    assert_eq!(
        shown,
        [
            "`list.count` isn't compiled yet: main.jadpo:21, in function counted",
            "`for` isn't compiled yet: main.jadpo:11, in function total",
        ]
    );
}
