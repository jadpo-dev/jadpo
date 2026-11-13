//! The tests the compiler generates for each route (area 13, "Generated
//! tests"; chunk W2), end to end: a shelf's two routes, one that stocks an
//! item from a body and declares a failure with a client field, and one
//! that reads an item by a placeholder and fails when there is none. Each
//! gets a test refusing bad input with `{ field, rule }`, one sending each
//! failure it declares in its envelope, and one sending only its output's
//! fields, run with the project's own test, counted apart and listed at
//! the route; and the route's place, a name or `--kind route` picks them.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use crate::http::jadpo;

/// The shelf's project.
const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {\n}\n"),
    (
        "domains/shop/actions/look_up.jadpo",
        "use shop/entities { Item, ItemNotFound }\nuse shop/types { ItemView }\n\n/// An item by its id.\naction look_up(item_id: Item.id) fails ItemNotFound -> ItemView {\n    var item: Item = attempt Item.get(item_id)\n    return ItemView {\n        id: item.id\n        name: item.name\n        quantity: item.quantity\n    }\n}\n",
    ),
    (
        "domains/shop/actions/stock.jadpo",
        "use shop/entities { Item }\nuse shop/failures { ShelfFull }\nuse shop/types { ItemView, NewItem }\n\n/// Puts an item on the shelf, if there is room.\naction stock(input: NewItem) fails ShelfFull -> ItemView {\n    if (input.quantity > 50) {\n        reject ShelfFull {\n            room: 50\n        }\n    }\n    var item: Item = Item.create {\n        set: {\n            name: input.name\n            quantity: input.quantity\n        }\n    }\n    return ItemView {\n        id: item.id\n        name: item.name\n        quantity: item.quantity\n    }\n}\n",
    ),
    (
        "domains/shop/entities/item.jadpo",
        "use shop/types { ItemName, Quantity }\n\n/// An item on the shelf.\nentity Item {\n    id: Uuid\n    name: ItemName\n    quantity: Quantity\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/shop/failures/shelf_full.jadpo",
        "use shop/types { Quantity }\n\n/// The shelf has no room for that many.\ntype ShelfFull = Failure {\n    kind: FailureKind.conflict\n    message: \"The shelf is full.\"\n    client: {\n        room: Quantity\n    }\n}\n",
    ),
    (
        "domains/shop/routes/items.jadpo",
        "use shop/actions { look_up, stock }\nuse shop/entities { Item }\nuse shop/types { ItemView, NewItem }\n\n/// The shelf's items.\nroutes Items {\n    prefix: /items\n\n    /// Stocks an item.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NewItem\n        output: ItemView\n        status: SuccessStatus.created\n        run: stock(input)\n    }\n\n    /// One item.\n    GET /{item_id: Item.id} {\n        authentication: RouteAuthentication.none\n        output: ItemView\n        run: look_up(item_id)\n    }\n}\n",
    ),
    (
        "domains/shop/routes/items.test.jadpo",
        "use shop/types { NewItem }\n\ntest \"an item is stocked\" {\n    var response: Response = request {\n        method: HttpMethod.post\n        path: \"/items\"\n        input: NewItem {\n            name: \"Rye\"\n            quantity: 2\n        }\n    }\n    assert response.status == 201\n}\n",
    ),
    (
        "domains/shop/types/item_name.jadpo",
        "/// What an item is called.\ntype ItemName = Text {\n    min_length: 1\n    max_length: 40\n}\n",
    ),
    (
        "domains/shop/types/item_view.jadpo",
        "use shop/types { ItemName, Quantity }\n\n/// An item as a client sees it.\ntype ItemView = Output {\n    id: Uuid\n    name: ItemName\n    quantity: Quantity\n}\n",
    ),
    (
        "domains/shop/types/new_item.jadpo",
        "use shop/types { ItemName, Quantity }\n\n/// An item to stock.\ntype NewItem = Input {\n    name: ItemName\n    quantity: Quantity\n    note: ItemName {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/shop/types/quantity.jadpo",
        "/// How many of an item.\ntype Quantity = Int {\n    min: 1\n    max: 99\n}\n",
    ),
    (
        "policy.jadpo",
        "use shop\n\npolicy {\n    actions: {\n        shop.stock: [Access.public]\n        shop.look_up: [Access.public]\n    }\n}\n",
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("generated-tests-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

fn tested(root: &Path, args: &[&str]) -> (String, u8) {
    let mut all = vec!["test", "--seed", "1"];
    all.extend_from_slice(args);
    let output = jadpo(root, &all);
    (
        crate::support::without_run_time(&output.stdout),
        output.code,
    )
}

/// Each test that ran, as `--json --full` lists it: its name, file and
/// lines, and whether it was generated.
fn listed(root: &Path, args: &[&str]) -> Vec<serde_json::Value> {
    let mut all = vec!["test", "--seed", "1", "--json", "--full"];
    all.extend_from_slice(args);
    let output = jadpo(root, &all);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let mut tests = document["data"]["tests"].as_array().expect("tests").clone();
    tests.sort_by_key(|test| test["key"].as_str().unwrap_or_default().to_string());
    tests
}

#[test]
fn each_route_gets_tests_generated_from_its_api_and_shown_at_it() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("shown");
    assert_eq!(
        tested(&root, &[]),
        (
            "tested: 7 passed, 1 route test, 6 generated tests\n".to_string(),
            0
        )
    );
    let tests = listed(&root, &[]);
    let shown: Vec<(String, String, u64, bool)> = tests
        .iter()
        .map(|test| {
            (
                test["name"].as_str().unwrap_or_default().to_string(),
                test["file"].as_str().unwrap_or_default().to_string(),
                test["line"].as_u64().unwrap_or_default(),
                test["generated"].as_bool().unwrap_or(false),
            )
        })
        .collect();
    let routes = "domains/shop/routes/items.jadpo".to_string();
    let at = |name: &str, line: u64| (name.to_string(), routes.clone(), line, true);
    assert_eq!(
        shown,
        [
            at(
                "GET /items/{item_id} refuses bad input with { field, rule }",
                19
            ),
            at("POST /items refuses bad input with { field, rule }", 10),
            at(
                "GET /items/{item_id} sends each failure it declares in its envelope",
                19
            ),
            at(
                "POST /items sends each failure it declares in its envelope",
                10
            ),
            at(
                "GET /items/{item_id} sends only the fields its output declares",
                19
            ),
            at("POST /items sends only the fields its output declares", 10),
            (
                "an item is stocked".to_string(),
                "domains/shop/routes/items.test.jadpo".to_string(),
                3,
                false
            ),
        ]
    );
    // What each sent, and what each answer was to be.
    let sent: Vec<String> = tests
        .iter()
        .flat_map(|test| test["requests"].as_array().cloned().unwrap_or_default())
        .map(|request| {
            format!(
                "{} {} {}",
                request["request"].as_str().unwrap_or_default(),
                request["does"].as_str().unwrap_or_default(),
                request["expected"]
            )
        })
        .collect();
    assert_eq!(
        sent,
        [
            r#"GET /items/00000000-0000-4000-8000-000001000000?not_a_field=jadpo-generated-value sends `not_a_field` in its query string {"code":"invalid_input","field":"not_a_field","rule":"unknown_field","status":422}"#,
            r#"GET /items/jadpo-generated-value gives `item_id` text it can't read {"code":"invalid_input","field":"item_id","rule":"format","status":422}"#,
            r#"POST /items?not_a_field=jadpo-generated-value sends `not_a_field` in its query string {"code":"invalid_input","field":"not_a_field","rule":"unknown_field","status":422}"#,
            r#"POST /items sends `not_a_field` in its body {"code":"invalid_input","field":"not_a_field","rule":"unknown_field","status":422}"#,
            r#"POST /items leaves out `name` {"code":"invalid_input","field":"name","rule":"required","status":422}"#,
            r#"POST /items leaves out `quantity` {"code":"invalid_input","field":"quantity","rule":"required","status":422}"#,
            r#"POST /items gives `name` a value of another kind {"code":"invalid_input","field":"name","rule":"type","status":422}"#,
            r#"POST /items gives `name` a value past its `min_length` {"code":"invalid_input","field":"name","rule":"min_length","status":422}"#,
            r#"POST /items gives `name` a value past its `max_length` {"code":"invalid_input","field":"name","rule":"max_length","status":422}"#,
            r#"POST /items gives `note` a value of another kind {"code":"invalid_input","field":"note","rule":"type","status":422}"#,
            r#"POST /items gives `note` a value past its `min_length` {"code":"invalid_input","field":"note","rule":"min_length","status":422}"#,
            r#"POST /items gives `note` a value past its `max_length` {"code":"invalid_input","field":"note","rule":"max_length","status":422}"#,
            r#"POST /items gives `quantity` a value of another kind {"code":"invalid_input","field":"quantity","rule":"type","status":422}"#,
            r#"POST /items gives `quantity` a value past its `min` {"code":"invalid_input","field":"quantity","rule":"min","status":422}"#,
            r#"POST /items gives `quantity` a value past its `max` {"code":"invalid_input","field":"quantity","rule":"max","status":422}"#,
            r#"GET /items/00000000-0000-4000-8000-000001000001 is answered with `item_not_found` {"code":"item_not_found","details":[],"status":404}"#,
            r#"POST /items is answered with `shelf_full` {"code":"shelf_full","details":["room"],"status":409}"#,
            r#"GET /items/00000000-0000-4000-8000-000001000001 is answered with a value of its output {"status":200}"#,
            r#"POST /items is answered with a value of its output {"status":201}"#,
        ]
    );
}

#[test]
fn a_routes_place_a_name_or_its_kind_picks_its_generated_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("picked");
    assert_eq!(
        tested(&root, &["domains/shop/routes/items.jadpo:20"]),
        ("tested: 3 passed, 3 of 7 tests\n".to_string(), 0)
    );
    assert_eq!(
        tested(&root, &["--name", "refuses bad input"]),
        ("tested: 2 passed, 2 of 7 tests\n".to_string(), 0)
    );
    assert_eq!(
        tested(&root, &["--kind", "route"]),
        (
            "tested: 7 passed, 1 route test, 6 generated tests\n".to_string(),
            0
        )
    );
    let names: Vec<String> = listed(&root, &["--name", "its output"])
        .iter()
        .map(|test| test["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        names,
        [
            "GET /items/{item_id} sends only the fields its output declares",
            "POST /items sends only the fields its output declares",
        ]
    );
}
