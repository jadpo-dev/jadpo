//! Outgoing webhooks to the addresses customers register (area 16,
//! "Outgoing webhooks"; chunk C1b): the customer's entity lists in
//! `webhooks:` the events its records' addresses receive, each a `public`
//! event with one field typed `<Entity>.id` naming the record whose
//! addresses get it. The entity's generated `register_webhook`, `webhooks`
//! and `remove_webhook` are typed, only an action calls them, and each is
//! limited by the record's grants. The secrets are sealed, so the app
//! names a field key, and the one runtime-owned exception is listed once
//! in the network report and kept in `network.jadpo_record`.

use crate::support;

use jadpo_db::Layout;

const APP: &str = "use market/entities { Seller }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n    }\n    events: {\n        store: main\n    }\n    field_encryption_key: config.field_encryption_key\n}\n\nprincipal {\n    user: {\n        subject: Seller.handle\n    }\n}\n";

const AUTHENTICATION: &str = "authentication {\n    api_token: {}\n}\n";

const CONFIG: &str = "config {\n    field_encryption_key: EncryptionKey\n}\n";

const HANDLE: &str = "/// What a seller signs in as.\npublic type Handle = Text {\n    min_length: 3\n    max_length: 40\n}\n";

const SELLER: &str = "use market/types { Handle }\n\n/// Someone who sells.\npublic entity Seller {\n    id: Uuid\n    handle: Handle {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n        update: [id]\n    }\n}\n";

const SHOP: &str = "use market/entities { Seller }\nuse market/events { OrderPlaced }\n\n/// A seller's shop.\npublic entity Shop {\n    id: Uuid\n    owner_id: Seller.id {\n        on_delete: OnDelete.restricted\n    }\n\n    webhooks: {\n        events: [OrderPlaced]\n    }\n\n    policy: {\n        create: [owner_id]\n        read: [owner_id]\n        update: [owner_id]\n    }\n}\n";

const PLACED: &str = "use market/entities { Shop }\n\n/// An order was placed with a shop.\npublic type OrderPlaced = Event {\n    wire_name: \"order.placed\"\n    shop_id: Shop.id\n    placed_at: Instant\n}\n";

const NEW_WEBHOOK: &str = "use market/entities { Shop }\n\n/// Where a shop's orders go.\ntype NewWebhook = Input {\n    shop_id: Shop.id\n    url: Url\n}\n";

const ADDED: &str =
    "/// A new address's secret.\ntype WebhookAdded = Output {\n    secret: WebhookSecret\n}\n";

const ADD: &str = "use market/entities { Shop }\nuse market/types { NewWebhook, WebhookAdded }\n\n/// Registers an address.\naction add_webhook(input: NewWebhook) -> WebhookAdded {\n    return WebhookAdded {\n        secret: Shop.register_webhook(input.shop_id, input.url)\n    }\n}\n";

const PLACE: &str = "use market/entities { Shop }\nuse market/events { OrderPlaced }\n\n/// Places an order.\naction place_order(shop_id: Shop.id) -> Unit {\n    emit OrderPlaced {\n        shop_id: shop_id\n        placed_at: clock.now\n    }\n}\n";

/// The market's files, with `changes` in place of the file at each path
/// (an empty text leaves the file out).
fn files(changes: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = [
        ("app.jadpo", APP),
        ("authentication.jadpo", AUTHENTICATION),
        ("config.jadpo", CONFIG),
        ("domains/market/types/handle.jadpo", HANDLE),
        ("domains/market/types/new_webhook.jadpo", NEW_WEBHOOK),
        ("domains/market/types/webhook_added.jadpo", ADDED),
        ("domains/market/entities/seller.jadpo", SELLER),
        ("domains/market/entities/shop.jadpo", SHOP),
        ("domains/market/events/order_placed.jadpo", PLACED),
        ("domains/market/actions/add_webhook.jadpo", ADD),
        ("domains/market/actions/place_order.jadpo", PLACE),
    ]
    .iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect();
    for (path, text) in changes {
        out.retain(|(known, _)| known != path);
        if !text.is_empty() {
            out.push((path.to_string(), text.to_string()));
        }
    }
    out
}

/// Each problem as `path rule: message`, but for unused declarations,
/// migrations and records, which these projects don't keep.
fn problems(changes: &[(&str, &str)]) -> Vec<String> {
    let files = files(changes);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            !["import.unused", "layout.unused_declaration"]
                .iter()
                .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
                && !problem.contains(" record.")
        })
        .collect()
}

fn with_rule(found: &[String], rule: &str) -> Vec<String> {
    found
        .iter()
        .filter(|problem| problem.contains(&format!(" {rule}:")))
        .cloned()
        .collect()
}

#[test]
fn an_entity_whose_records_register_addresses_checks() {
    // Listing the event in `webhooks:` offers it outside the app, so its
    // `public` is used.
    assert_eq!(problems(&[]), Vec::<String>::new());
}

#[test]
fn each_event_names_the_one_record_whose_addresses_receive_it() {
    let none =
        "/// An order was placed.\npublic type OrderPlaced = Event {\n    placed_at: Instant\n}\n";
    let place = "use market/events { OrderPlaced }\n\n/// Places an order.\naction place_order() -> Unit {\n    emit OrderPlaced {\n        placed_at: clock.now\n    }\n}\n";
    let found = problems(&[
        ("domains/market/events/order_placed.jadpo", none),
        ("domains/market/actions/place_order.jadpo", place),
    ]);
    assert_eq!(
        with_rule(&found, "publishing.invalid"),
        [
            "domains/market/entities/shop.jadpo publishing.invalid: `OrderPlaced` names no `Shop` to send to"
        ],
        "{found:?}"
    );
    let two = "use market/entities { Shop }\n\n/// An order moved between shops.\npublic type OrderPlaced = Event {\n    from_id: Shop.id\n    to_id: Shop.id\n}\n";
    let found = problems(&[("domains/market/events/order_placed.jadpo", two)]);
    assert!(
        found.contains(&"domains/market/entities/shop.jadpo publishing.invalid: `OrderPlaced` names two `Shop` records, `from_id` and `to_id`".to_string()),
        "{found:?}"
    );
    let optional = "use market/entities { Shop }\n\n/// An order was placed.\npublic type OrderPlaced = Event {\n    shop_id: Shop.id?\n}\n";
    let found = problems(&[("domains/market/events/order_placed.jadpo", optional)]);
    assert!(
        found.contains(&"domains/market/entities/shop.jadpo publishing.invalid: `OrderPlaced.shop_id` may be `none`, so it names no `Shop` to send to".to_string()),
        "{found:?}"
    );
}

#[test]
fn only_public_events_of_the_apps_domains_go_to_customers() {
    let private = PLACED.replace("public type OrderPlaced", "type OrderPlaced");
    let found = problems(&[("domains/market/events/order_placed.jadpo", &private)]);
    assert!(
        found.contains(
            &"domains/market/entities/shop.jadpo publishing.invalid: `OrderPlaced` isn't public"
                .to_string()
        ),
        "{found:?}"
    );
    let misspelt = SHOP.replace(
        "        events: [OrderPlaced]",
        "        event: [OrderPlaced]",
    );
    let found = problems(&[("domains/market/entities/shop.jadpo", &misspelt)]);
    assert!(
        found.contains(
            &"domains/market/entities/shop.jadpo publishing.invalid: `webhooks:` has no key `event:`"
                .to_string()
        ),
        "{found:?}"
    );
}

#[test]
fn the_secrets_are_sealed_so_the_app_names_a_key() {
    let app = APP.replace(
        "    field_encryption_key: config.field_encryption_key\n",
        "",
    );
    let found = problems(&[("app.jadpo", &app), ("config.jadpo", "config {\n}\n")]);
    assert_eq!(
        with_rule(&found, "data.field_encryption_key_required"),
        [
            "domains/market/entities/shop.jadpo data.field_encryption_key_required: `Shop`'s webhook addresses keep their secrets sealed, but `application { }` names no key to seal them with"
        ],
        "{found:?}"
    );
}

#[test]
fn the_operations_belong_to_the_webhooks_entity_and_an_action() {
    let elsewhere = ADD.replace("Shop.register_webhook", "Seller.register_webhook");
    let found = problems(&[("domains/market/actions/add_webhook.jadpo", &elsewhere)]);
    assert!(
        found.iter().any(|problem| problem
            .contains("semantic.unknown_callee: `Seller` has no operation `register_webhook`")),
        "{found:?}"
    );
    let test = "use market/entities { Shop }\n\ntest \"lists a shop's addresses\" {\n    var shop: Shop = make.Shop {}\n    assert list.count(Shop.webhooks(shop.id)) == 0\n}\n";
    let found = problems(&[("domains/market/actions/place_order.test.jadpo", test)]);
    assert!(
        found.iter().any(|problem| problem
            .contains("type.auth_sessions_context: A test can't call `Shop.webhooks`")),
        "{found:?}"
    );
    let stored = SHOP.replace(
        "    owner_id: Seller.id {",
        "    secret: WebhookSecret?\n    owner_id: Seller.id {",
    );
    let found = problems(&[("domains/market/entities/shop.jadpo", &stored)]);
    assert!(
        found.iter().any(|problem| problem
            .contains("type.auth_credential_stored: `Shop.secret` keeps a webhook's secret")),
        "{found:?}"
    );
    let count = ADD.replace("input.shop_id, input.url", "input.shop_id");
    let found = problems(&[("domains/market/actions/add_webhook.jadpo", &count)]);
    assert!(
        found.iter().any(|problem| problem.contains("type.argument_count: `Shop.register_webhook` takes the record's `Shop.id` and the address, a `Url`")),
        "{found:?}"
    );
}

#[test]
fn one_entitys_records_register_addresses() {
    let second = SELLER.replace(
        "    policy: {",
        "    webhooks: {\n        events: [OrderPlaced]\n    }\n\n    policy: {",
    );
    let second = second.replace(
        "use market/types { Handle }",
        "use market/events { OrderPlaced }\nuse market/types { Handle }",
    );
    let found = problems(&[("domains/market/entities/seller.jadpo", &second)]);
    assert!(
        found.iter().any(|problem| problem.contains(
            "domains/market/entities/shop.jadpo toolchain.not_built_yet: `Seller` and `Shop` both register addresses for outgoing webhooks"
        )),
        "{found:?}"
    );
    let clashing = APP.replace(
        "        main: {\n            driver: Driver.postgres\n        }\n",
        "        main: {\n            driver: Driver.postgres\n        }\n        outgoing_webhooks: {\n            driver: Driver.postgres\n        }\n",
    );
    let found = problems(&[("app.jadpo", &clashing)]);
    assert!(
        found.iter().any(|problem| problem
            .contains("publishing.invalid: A connection can't be called `outgoing_webhooks`")),
        "{found:?}"
    );
}

/// The exception is listed once in the network report and kept in the
/// network record, so the change that adds it needs a person.
#[test]
fn the_exception_is_listed_once_in_the_network_report() {
    let files = files(&[]);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let report = jadpo_check::connections::report(&db, project);
    let rows: Vec<&str> = report
        .markdown
        .lines()
        .filter(|line| line.starts_with("| `outgoing_webhooks`"))
        .collect();
    assert_eq!(
        rows,
        [
            "| `outgoing_webhooks` | any public HTTPS address a `Shop` registers | always | runtime outgoing webhooks: public HTTPS addresses only, checked as each is reached, no redirects | `market` |"
        ],
        "{}",
        report.markdown
    );
    let hosts = report.json.get("hosts").expect("hosts");
    let text = format!("{hosts:?}");
    assert!(text.contains("registered_by"), "{text}");
    let record = jadpo_check::acceptances::network::record_text(&db, project)
        .clone()
        .flatten()
        .expect("a record");
    assert!(
        record.contains(
            "connection outgoing_webhooks\n    for outgoing webhooks\n    host addresses Shop registers\n"
        ),
        "{record}"
    );
}

/// Personal data in an event customers' addresses receive leaves through
/// the exception, which can't be asked to erase it.
#[test]
fn personal_data_in_a_webhooks_event_is_reported_where_it_leaves() {
    let placed = "use market/entities { Shop }\n\n/// An order was placed with a shop.\npublic type OrderPlaced = Event {\n    shop_id: Shop.id\n    buyer: Email\n}\n";
    let place = "use market/entities { Shop }\nuse market/events { OrderPlaced }\n\n/// Places an order.\naction place_order(shop_id: Shop.id, buyer: Email) -> Unit {\n    emit OrderPlaced {\n        shop_id: shop_id\n        buyer: buyer\n    }\n}\n";
    let files = files(&[
        ("domains/market/events/order_placed.jadpo", placed),
        ("domains/market/actions/place_order.jadpo", place),
    ]);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let sent: Vec<String> = jadpo_check::personal_data::sent(&db, project)
        .iter()
        .map(|sent| format!("{} {}", sent.connection, sent.through.named()))
        .collect();
    assert!(
        sent.contains(&"outgoing_webhooks published OrderPlaced".to_string()),
        "{sent:?}"
    );
    let unerased: Vec<String> = jadpo_check::provider_erasure::unerased(&db, project)
        .iter()
        .map(|unerased| unerased.connection.clone())
        .collect();
    assert_eq!(unerased, ["outgoing_webhooks"]);
}

/// Each API's OpenAPI document lists the events customers' addresses
/// receive in `x-jadpo-events`.
#[test]
fn the_openapi_document_lists_the_events_customers_receive() {
    let routes = "use market/actions { add_webhook }\nuse market/types { NewWebhook, WebhookAdded }\n\n/// A shop's webhooks.\nroutes Webhooks {\n    prefix: /webhooks\n\n    /// Registers an address.\n    POST / {\n        input: NewWebhook\n        output: WebhookAdded\n        status: SuccessStatus.created\n        run: add_webhook(input)\n    }\n}\n";
    let files = files(&[("domains/market/routes/webhooks.jadpo", routes)]);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "market", "1")
        .expect("a document");
    let events = document.get("x-jadpo-events").expect("x-jadpo-events");
    assert_eq!(
        events
            .get("events")
            .and_then(|events| events.get("order.placed"))
            .and_then(|event| event.get("$ref")),
        Some(&jadpo_graph::json::Value::from(
            "#/components/schemas/OrderPlaced"
        )),
        "{document:?}"
    );
}
