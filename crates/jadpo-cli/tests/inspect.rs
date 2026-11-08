//! `jadpo inspect` and `jadpo impact` (areas 1, 2, 14 and 18): the project
//! map, one node, and what a change to a name reaches, each read from the
//! graph `jadpo check` builds.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

/// A shop: a shared library function called three times from `ordering`
/// and once from `payments`, through two routes and a schedule, and a
/// type built from another.
const SHOP: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "policy.jadpo",
        "use ordering\nuse payments\n\npolicy {\n    actions: {\n        ordering.place_order: [Access.public]\n        payments.refund: [Access.public]\n    }\n}\n",
    ),
    (
        "shared/types/quantity.jadpo",
        "/// How many of one item.\ntype Quantity = Int {\n    min: 0\n    max: 1000\n}\n",
    ),
    (
        "shared/libraries/tallies.jadpo",
        "use shared/types { Quantity }\n\nlibrary tallies {\n    /// The larger of two tallies.\n    function larger(first: Quantity, second: Quantity) -> Quantity {\n        if (first > second) {\n            return first\n        }\n        return second\n    }\n}\n",
    ),
    (
        "domains/ordering/types/new_order.jadpo",
        "use shared/types { Quantity }\n\n/// What a customer asks for, and how many a box holds.\ntype NewOrder = Input {\n    asked: Quantity\n    per_box: Quantity\n}\n",
    ),
    (
        "domains/ordering/types/order_total.jadpo",
        "use shared/types { Quantity }\nuse ordering/types { NewOrder }\n\n/// What an order sends, with what was asked for.\ntype OrderTotal = Output {\n    include: [NewOrder]\n    per_box: Quantity\n    sent: Quantity\n}\n",
    ),
    (
        "domains/ordering/actions/place_order.jadpo",
        "use shared/types { Quantity }\nuse shared/libraries { tallies }\nuse ordering/types { NewOrder, OrderTotal }\n\n/// Sends at least a box, and a box holds one at least.\naction place_order(input: NewOrder) -> OrderTotal {\n    var per_box: Quantity = tallies.larger(input.per_box, 1)\n    return OrderTotal {\n        asked: input.asked\n        per_box: per_box\n        sent: tallies.larger(input.asked, per_box)\n    }\n}\n",
    ),
    (
        "domains/ordering/actions/place_order.test.jadpo",
        "use ordering/types { NewOrder, OrderTotal }\n\ntest \"an order sends at least a box\" {\n    var total: OrderTotal = place_order(\n        NewOrder {\n            asked: 2\n            per_box: 6\n        }\n    )\n    assert total.sent == 6\n}\n",
    ),
    (
        "domains/ordering/actions/send_invoices.jadpo",
        "use shared/types { Quantity }\nuse shared/libraries { tallies }\n\n/// Counts the night's invoices, one at least.\naction send_invoices() -> Quantity {\n    return tallies.larger(0, 1)\n}\n",
    ),
    (
        "domains/ordering/schedules/nightly_invoices.jadpo",
        "use ordering/actions { send_invoices }\n\n/// Sends the day's invoices each night.\nschedule nightly_invoices {\n    every: 1d\n    run: send_invoices()\n}\n",
    ),
    (
        "domains/ordering/routes/basket.jadpo",
        "use ordering/types { NewOrder, OrderTotal }\nuse ordering/actions { place_order }\n\n/// Basket.\nroutes Basket {\n    prefix: /basket\n\n    /// Prices an order, for anyone.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: NewOrder\n        output: OrderTotal\n        run: place_order(input)\n    }\n}\n",
    ),
    (
        "domains/payments/types/refund_request.jadpo",
        "use shared/types { Quantity }\n\n/// The items a customer sends back.\ntype RefundRequest = Input {\n    returned: Quantity\n}\n",
    ),
    (
        "domains/payments/types/refund_view.jadpo",
        "use shared/types { Quantity }\n\n/// The items a refund pays for.\ntype RefundView = Output {\n    paid_for: Quantity\n}\n",
    ),
    (
        "domains/payments/actions/refund.jadpo",
        "use shared/libraries { tallies }\nuse payments/types { RefundRequest, RefundView }\n\n/// Refunds the items sent back, one at least.\naction refund(input: RefundRequest) -> RefundView {\n    return RefundView {\n        paid_for: tallies.larger(input.returned, 1)\n    }\n}\n",
    ),
    (
        "domains/payments/actions/refund.test.jadpo",
        "use payments/types { RefundRequest, RefundView }\n\ntest \"a refund pays for at least one item\" {\n    var sent: RefundView = refund(\n        RefundRequest {\n            returned: 0\n        }\n    )\n    assert sent.paid_for == 1\n}\n",
    ),
    (
        "domains/payments/routes/payments.jadpo",
        "use payments/types { RefundRequest, RefundView }\nuse payments/actions { refund }\n\n/// Payments.\nroutes Payments {\n    prefix: /payments\n\n    /// Sends a refund, for anyone.\n    POST /refunds {\n        authentication: RouteAuthentication.none\n        input: RefundRequest\n        output: RefundView\n        run: refund(input)\n    }\n}\n",
    ),
];

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("inspect-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    root
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// The shop, with its migration and records made as a reader makes them,
/// so it checks cleanly.
fn shop(name: &str) -> PathBuf {
    let root = project(name, SHOP);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(
        checked.code, 0,
        "the shop checks cleanly: {}",
        checked.stdout
    );
    root
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_str(&output.stdout).expect("one JSON document")
}

#[test]
fn impact_lists_the_calling_domains_the_entry_points_and_the_tests() {
    let root = shop("impact");
    let output = jadpo(&root, &["impact", "tallies.larger"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert_eq!(
        output.stdout,
        "function tallies.larger\n  at: shared/libraries/tallies.jadpo:5\n  callers: ordering (3 calls), payments (1 call)\n  triggers: POST /basket, POST /payments/refunds, schedule nightly_invoices\n  tests: domains/ordering/actions/place_order.test.jadpo\n         domains/payments/actions/refund.test.jadpo\n  public: no\n"
    );
}

#[test]
fn impact_as_json_names_each_user_entry_point_and_test() {
    let root = shop("impact-json");
    let document = json(&jadpo(&root, &["impact", "tallies.larger", "--json"]));
    assert_eq!(document["ok"], true);
    let data = &document["data"];
    assert_eq!(data["id"], "function tallies.larger");
    assert_eq!(data["public"], false);
    let users: Vec<(String, i64)> = data["users"]
        .as_array()
        .expect("users")
        .iter()
        .map(|user| {
            (
                user["id"].as_str().expect("id").to_string(),
                user["calls"].as_i64().expect("calls"),
            )
        })
        .collect();
    assert_eq!(
        users,
        [
            ("action place_order".to_string(), 2),
            ("action refund".to_string(), 1),
            ("action send_invoices".to_string(), 1),
        ]
    );
    let entries: Vec<&str> = data["triggers"]
        .as_array()
        .expect("triggers")
        .iter()
        .map(|entry| entry["id"].as_str().expect("id"))
        .collect();
    assert_eq!(
        entries,
        [
            "route POST /basket",
            "route POST /payments/refunds",
            "schedule nightly_invoices"
        ]
    );
    assert_eq!(data["tests"].as_array().expect("tests").len(), 2);
}

#[test]
fn impact_of_a_type_or_a_field_counts_what_uses_it() {
    let root = shop("impact-field");
    let output = jadpo(&root, &["impact", "NewOrder.per_box"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output.stdout.starts_with("field NewOrder.per_box\n"),
        "{}",
        output.stdout
    );
    assert!(
        output.stdout.contains("  used in: ordering ("),
        "{}",
        output.stdout
    );
    assert!(
        output.stdout.contains("  triggers: POST /basket\n"),
        "a change to a field reaches what uses its type: {}",
        output.stdout
    );
    let output = jadpo(&root, &["impact", "RefundView"]);
    assert!(
        output
            .stdout
            .contains("  triggers: POST /payments/refunds\n"),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .contains("  tests: domains/payments/actions/refund.test.jadpo\n"),
        "{}",
        output.stdout
    );
}

#[test]
fn impact_of_a_library_is_its_functions() {
    let root = shop("impact-library");
    let output = jadpo(&root, &["impact", "tallies"]);
    assert!(
        output
            .stdout
            .contains("  callers: ordering (3 calls), payments (1 call)\n"),
        "{}",
        output.stdout
    );
}

#[test]
fn inspect_prints_the_project_map() {
    let root = shop("map");
    let output = jadpo(&root, &["inspect"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert_eq!(
        output.stdout,
        "ordering
  type NewOrder                 domains/ordering/types/new_order.jadpo:4
  type OrderTotal               domains/ordering/types/order_total.jadpo:5
  action place_order            domains/ordering/actions/place_order.jadpo:6
    calls    tallies.larger
  action send_invoices          domains/ordering/actions/send_invoices.jadpo:5
    calls    tallies.larger
  route POST /basket            domains/ordering/routes/basket.jadpo:9
    runs     place_order
  schedule nightly_invoices     domains/ordering/schedules/nightly_invoices.jadpo:4
    runs     send_invoices

payments
  type RefundRequest            domains/payments/types/refund_request.jadpo:4
  type RefundView               domains/payments/types/refund_view.jadpo:4
  action refund                 domains/payments/actions/refund.jadpo:5
    calls    tallies.larger
  route POST /payments/refunds  domains/payments/routes/payments.jadpo:9
    runs     refund

shared
  type Quantity                 shared/types/quantity.jadpo:2
  function tallies.larger       shared/libraries/tallies.jadpo:5

12 declarations: 2 domains, 2 tests
"
    );
}

#[test]
fn inspect_as_json_is_the_manifest() {
    let root = shop("map-json");
    let document = json(&jadpo(&root, &["inspect", "--json"]));
    assert_eq!(document["command"], "inspect");
    assert_eq!(document["data"]["format"], 2);
    let nodes = document["data"]["nodes"].as_array().expect("nodes");
    assert!(
        nodes
            .iter()
            .any(|node| node["id"] == "function tallies.larger"
                && node["file"] == "shared/libraries/tallies.jadpo")
    );
    // The command writes nothing: the manifest is `jadpo check`'s.
    let checked = jadpo(&root, &["check", "--json"]);
    assert_eq!(checked.code, 0);
    let written: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/manifest.json")).expect("manifest"),
    )
    .expect("JSON");
    assert_eq!(written["nodes"], document["data"]["nodes"]);
}

#[test]
fn inspect_shows_a_built_types_final_fields_and_where_each_came_from() {
    let root = shop("built-type");
    let output = jadpo(&root, &["inspect", "OrderTotal"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert_eq!(
        output.stdout,
        "type OrderTotal
  at: domains/ordering/types/order_total.jadpo:5
  about: What an order sends, with what was asked for.
  base: Output
  fields: asked  from NewOrder.asked
          per_box: Quantity  written, replaces NewOrder.per_box
          sent: Quantity  written
  uses: NewOrder
  used by: place_order, POST /basket
"
    );
}

#[test]
fn inspect_shows_one_node_and_its_edges() {
    let root = shop("one");
    let output = jadpo(&root, &["inspect", "place_order"]);
    assert_eq!(
        output.stdout,
        "action place_order
  at: domains/ordering/actions/place_order.jadpo:6
  about: Sends at least a box, and a box holds one at least.
  type: (input: NewOrder) -> OrderTotal
  calls: tallies.larger
  uses: Quantity, NewOrder, OrderTotal
  run by: POST /basket
"
    );
    let output = jadpo(&root, &["inspect", "tallies"]);
    assert!(
        output.stdout.contains("  members: tallies.larger\n"),
        "{}",
        output.stdout
    );
    let route = jadpo(&root, &["inspect", "POST /basket"]);
    assert!(
        route.stdout.starts_with("route POST /basket\n"),
        "{}",
        route.stdout
    );
    assert!(
        route.stdout.contains("  runs: place_order\n"),
        "{}",
        route.stdout
    );
}

#[test]
fn inspect_a_name_as_json_is_its_manifest_node_and_fields() {
    let root = shop("one-json");
    let document = json(&jadpo(&root, &["inspect", "OrderTotal", "--json"]));
    let data = &document["data"];
    assert_eq!(data["node"]["id"], "type ordering.OrderTotal");
    assert_eq!(data["node"]["kind"], "type");
    let fields: Vec<(&str, Option<&str>, Option<&str>)> = data["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .map(|field| {
            (
                field["name"].as_str().expect("name"),
                field["copiedFrom"].as_str(),
                field["replaces"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            (
                "OrderTotal.asked",
                Some("field ordering.NewOrder.asked"),
                None
            ),
            (
                "OrderTotal.per_box",
                None,
                Some("field ordering.NewOrder.per_box")
            ),
            ("OrderTotal.sent", None, None),
        ]
    );
}

#[test]
fn a_name_the_project_doesnt_declare_is_refused_with_the_nearest() {
    let root = shop("unknown");
    let output = jadpo(&root, &["impact", "tallies.larg"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "error: `tallies.larg` isn't a declaration
  reason: This project declares nothing called `tallies.larg`. Did you mean `tallies.larger`?
  next: Run `jadpo impact tallies.larger` (agent)
  rule: cli.impact_arguments
  help: https://jadpo.dev/docs/errors/cli.impact_arguments
"
    );
    let output = jadpo(&root, &["inspect", "Quantty"]);
    assert_eq!(output.code, 2);
    assert!(
        output.stderr.contains("rule: cli.inspect_arguments"),
        "{}",
        output.stderr
    );
    let output = jadpo(&root, &["impact"]);
    assert_eq!(output.code, 2);
    assert!(
        output.stderr.contains("rule: cli.impact_arguments"),
        "{}",
        output.stderr
    );
}

#[test]
fn two_domains_private_helpers_of_one_name_are_named_with_their_domain() {
    // Each domain's own `at_least_one`, private to it, beside the action
    // that calls it.
    let helper = "\n/// `count`, or one when it is none.\nfunction at_least_one(count: Quantity) -> Quantity {\n    if (count > 0) {\n        return count\n    }\n    return 1\n}\n";
    let files: Vec<(&str, String)> = SHOP
        .iter()
        .map(|&(path, text)| {
            let text = match path {
                "domains/ordering/actions/send_invoices.jadpo" => format!(
                    "{}{helper}",
                    text.replace("tallies.larger(0, 1)", "at_least_one(0)")
                        .replace("use shared/libraries { tallies }\n", "")
                ),
                "domains/payments/actions/refund.jadpo" => format!(
                    "use shared/types {{ Quantity }}\n{}{helper}",
                    text.replace(
                        "tallies.larger(input.returned, 1)",
                        "at_least_one(input.returned)"
                    )
                    .replace("use shared/libraries { tallies }\n", "")
                ),
                _ => text.to_string(),
            };
            (path, text)
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect();
    let root = project("ambiguous", &borrowed);
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let output = jadpo(&root, &["impact", "at_least_one"]);
    assert_eq!(output.code, 2, "{}", output.stdout);
    assert_eq!(
        output.stderr,
        "error: `at_least_one` names more than one declaration
  reason: More than one declaration goes by that name: `ordering.at_least_one` and `payments.at_least_one`.
  next: Name one as it is listed, for example `jadpo impact ordering.at_least_one` (agent)
  rule: cli.impact_arguments
  help: https://jadpo.dev/docs/errors/cli.impact_arguments
"
    );
    let output = jadpo(&root, &["impact", "payments.at_least_one"]);
    assert!(
        output
            .stdout
            .starts_with("function at_least_one\n  at: domains/payments/actions/refund.jadpo:")
            && output.stdout.contains("  callers: payments (1 call)\n")
            && output
                .stdout
                .contains("  triggers: POST /payments/refunds\n"),
        "{}",
        output.stdout
    );
}

#[test]
fn a_project_with_errors_shows_them_and_nothing_else() {
    let mut files = SHOP.to_vec();
    files.push((
        "domains/ordering/types/stray.jadpo",
        "/// Nothing uses this.\ntype Stray = Text {\n    max_length: 10\n}\n",
    ));
    let root = project("errors", &files);
    for command in ["inspect", "impact"] {
        let output = jadpo(&root, &[command, "tallies.larger"]);
        assert_eq!(output.code, 1, "{}", output.stdout);
        assert!(
            output.stdout.contains(&format!("{command} stopped: ")),
            "{}",
            output.stdout
        );
        assert!(!output.stdout.contains("called from"), "{}", output.stdout);
    }
}

#[test]
fn tab_completes_the_projects_declarations() {
    let root = shop("complete");
    let output = jadpo(&root, &["__complete", "--", "impact", "tall"]);
    assert!(
        output.stdout.contains("tallies.larger"),
        "{}",
        output.stdout
    );
}
