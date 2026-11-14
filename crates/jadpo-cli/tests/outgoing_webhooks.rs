//! Outgoing webhooks to the addresses customers register, against live
//! PostgreSQL and a receiver on this machine (area 16, "Outgoing
//! webhooks"; chunk C1b): a market whose `Shop` lists `OrderPlaced` in
//! `webhooks:`. A shop's owner registers addresses through
//! `Shop.register_webhook`, each given a secret of its own, shown once and
//! kept sealed; once an order's action commits, its delivery posts the
//! event to each of that shop's addresses, signed with that address's own
//! secret, through the runtime's guarded client. An address that took it
//! isn't sent it again when another is retried. A private address is
//! refused when it is registered, and one whose name resolves somewhere
//! private by the time it is sent is refused then, nothing connected to.
//!
//! The same project runs compiled: its `jadpo test` both ways in the
//! gate's compiled step (`compiled.rs`, "addresses customers register"),
//! where a test's host catches each post.
//!
//! Nothing reaches a network: the guarded client (`Outward::reaching`)
//! takes one public-looking host to a receiver on this machine
//! (`support::fake_hooks`), every other name is looked up as it would be
//! in production, and `localhost` resolves from the system's own hosts
//! file. Every secret is made as the test runs. The database is the one
//! `JADPO_TEST_DATABASE_URL` names or one started from the machine's
//! PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::Fetch;
use jadpo_drivers::webhook::Outgoing;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::Outward;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::runner::{run_deferred, sweep};
use jadpo_runtime::{Caller, Executor, Host, Outcome, SqlValue};
use jadpo_std::http::PrincipalKind;
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support::fake_hooks::{FakeHooks, Received};

/// When the test starts: 2026-11-14T09:00:00Z.
const T0: i64 = 1_794_646_800_000;
/// An hour, past the first retry's wait.
const HOUR: i64 = 3_600_000;

/// The host the guarded client takes to the receiver on this machine.
const HOOKS: &str = "hooks.corner-books.example";

/// The market.
pub(crate) fn files() -> Vec<(String, String)> {
    let file = |path: &str, text: &str| (path.to_string(), text.to_string());
    vec![
        file(
            "app.jadpo",
            "use market/entities { Seller }

application {
    connections: {
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    events: {
        store: main
    }
    field_encryption_key: config.field_encryption_key
}

principal {
    user: {
        subject: Seller.handle
    }
}
",
        ),
        file(
            "authentication.jadpo",
            "authentication {
    api_token: {}
}
",
        ),
        file(
            "config.jadpo",
            "config {
    /// The market's database.
    database_url: PostgresUrl
    /// What each webhook address's secret is kept sealed with.
    field_encryption_key: EncryptionKey
}
",
        ),
        file(
            "domains/market/types/handle.jadpo",
            "/// What a seller signs in as.
public type Handle = Text {
    min_length: 3
    max_length: 40
}
",
        ),
        file(
            "domains/market/types/shop_name.jadpo",
            "/// What a shop is called.
public type ShopName = Text {
    min_length: 1
    max_length: 60
}
",
        ),
        file(
            "domains/market/types/order_reference.jadpo",
            "/// An order's reference, as its buyer sees it.
public type OrderReference = Text {
    min_length: 1
    max_length: 20
}
",
        ),
        file(
            "domains/market/types/webhook_id.jadpo",
            "/// One of a shop's webhook addresses.
type WebhookId = Uuid {}
",
        ),
        file(
            "domains/market/types/new_webhook.jadpo",
            "use market/entities { Shop }

/// Where a shop wants its orders sent.
type NewWebhook = Input {
    shop_id: Shop.id
    url: Url
}
",
        ),
        file(
            "domains/market/types/webhook_added.jadpo",
            "/// A new address's secret, which its receiver checks each event with.
type WebhookAdded = Output {
    secret: WebhookSecret
}
",
        ),
        file(
            "domains/market/types/webhook_list.jadpo",
            "/// A shop's addresses, never their secrets.
type WebhookList = Output {
    webhooks: List<RegisteredWebhook>
}
",
        ),
        file(
            "domains/market/types/shop_view.jadpo",
            "use market/entities { Shop }

/// A shop.
type ShopView = Output {
    pick: [Shop.id, Shop.name]
}
",
        ),
        file(
            "domains/market/types/seller_view.jadpo",
            "use market/entities { Seller }

/// A seller.
type SellerView = Output {
    pick: [Seller.id, Seller.handle]
}
",
        ),
        file(
            "domains/market/failures/handle_taken.jadpo",
            "/// Another seller has the handle.
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: \"That handle is taken.\"
}
",
        ),
        file(
            "domains/market/entities/seller.jadpo",
            "use market/types { Handle }

/// Someone who sells on the market.
public entity Seller {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
    }
}
",
        ),
        file(
            "domains/market/entities/shop.jadpo",
            "use market/entities { Seller }
use market/types { ShopName }
use market/events { OrderPlaced }

/// A seller's shop, whose own systems hear of each order placed with it.
public entity Shop {
    id: Uuid
    owner_id: Seller.id {
        on_delete: OnDelete.restricted
    }
    name: ShopName

    webhooks: {
        events: [OrderPlaced]
    }

    policy: {
        create: [owner_id]
        read: [owner_id]
        update: [owner_id]
    }
}
",
        ),
        file(
            "domains/market/events/order_placed.jadpo",
            "use market/entities { Shop }
use market/types { OrderReference }

/// An order was placed with a shop.
public type OrderPlaced = Event {
    wire_name: \"order.placed\"
    shop_id: Shop.id
    reference: OrderReference
    placed_at: Instant
}
",
        ),
        file(
            "domains/market/actions/join.jadpo",
            "use market/entities { Seller }
use market/types { Handle, SellerView }
use market/failures { HandleTaken }

/// Signs a seller up.
action join(handle: Handle) fails HandleTaken -> SellerView {
    var seller: Seller = attempt Seller.create {
        set: {
            handle: handle
        }
        conflict: {
            Seller.handle: HandleTaken
        }
    }
    return SellerView {
        id: seller.id
        handle: seller.handle
    }
}
",
        ),
        file(
            "domains/market/actions/open_shop.jadpo",
            "use market/entities { Shop }
use market/types { ShopName, ShopView }

/// Opens a shop for the seller calling.
action open_shop(name: ShopName, owner: Principal.user) -> ShopView {
    var shop: Shop = Shop.create {
        set: {
            owner_id: owner.id
            name: name
        }
    }
    return ShopView {
        id: shop.id
        name: shop.name
    }
}
",
        ),
        file(
            "domains/market/actions/add_webhook.jadpo",
            "use market/entities { Shop }
use market/types { NewWebhook, WebhookAdded }

/// Registers an address a shop's orders are sent to, and gives its
/// secret, the one time anyone sees it.
action add_webhook(input: NewWebhook) -> WebhookAdded {
    return WebhookAdded {
        secret: Shop.register_webhook(input.shop_id, input.url)
    }
}
",
        ),
        file(
            "domains/market/actions/list_webhooks.jadpo",
            "use market/entities { Shop }
use market/types { WebhookList }

/// A shop's addresses.
action list_webhooks(shop_id: Shop.id) -> WebhookList {
    return WebhookList {
        webhooks: Shop.webhooks(shop_id)
    }
}
",
        ),
        file(
            "domains/market/actions/drop_webhook.jadpo",
            "use market/entities { Shop }
use market/types { WebhookId }

/// Removes one of a shop's addresses.
action drop_webhook(webhook: WebhookId) -> Unit {
    Shop.remove_webhook(webhook)
}
",
        ),
        file(
            "domains/market/actions/place_order.jadpo",
            "use market/entities { Shop }
use market/types { OrderReference }
use market/events { OrderPlaced }

/// Places an order with a shop, which its addresses hear of.
action place_order(shop_id: Shop.id, reference: OrderReference) -> Unit {
    emit OrderPlaced {
        shop_id: shop_id
        reference: reference
        placed_at: clock.now
    }
}
",
        ),
        file(
            "domains/market/actions/add_webhook.test.jadpo",
            "use market/entities { Seller }
use market/types { NewWebhook, SellerView, ShopView, WebhookAdded, WebhookId, WebhookList }
use market/actions { drop_webhook, join, list_webhooks, open_shop, place_order }

test \"a shop's address is registered, hears of its orders and is removed\" {
    var joined: SellerView = attempt join(\"corner-books\")
    var seller: Seller = attempt Seller.get(joined.id)
    var shop: ShopView = open_shop(\"Corner Books\", Principal.user(seller))
    var added: WebhookAdded = add_webhook(
        NewWebhook {
            shop_id: shop.id
            url: \"https://hooks.corner-books.example/orders\"
        }
    )
    assert text.starts_with(added.secret, \"whsec_\")
    var listed: WebhookList = list_webhooks(shop.id)
    assert list.count(listed.webhooks) == 1
    place_order(shop.id, \"A-1001\")
    for webhook in listed.webhooks {
        drop_webhook(WebhookId(webhook.id))
    }
    var after: WebhookList = list_webhooks(shop.id)
    assert list.count(after.webhooks) == 0
}
",
        ),
    ]
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1794646800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

/// The market, written into a folder of its own, migrated and recorded.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("outgoing-webhooks-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    root
}

struct Loaded {
    _root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The market's program, started with a field key.
fn load(name: &str) -> Option<Loaded> {
    let root = project(name);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let database = crate::deployable::as_url(&url);
    let variables = move |name: &str| -> Option<String> {
        match name {
            "DATABASE_URL" => Some(database.clone()),
            "FIELD_ENCRYPTION_KEY" => Some(crate::encrypted_fields::FIRST_KEY.to_string()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    let program = Program::new(prepared).with_config(&values);
    Some(Loaded {
        _root: root,
        program,
        _databases: databases,
        url,
    })
}

/// The exception's driver over the guarded client, which takes [`HOOKS`]
/// to `hooks`, a receiver on this machine, and looks every other name up.
fn outgoing(hooks: &FakeHooks) -> Rc<dyn Capability> {
    let at: SocketAddr = hooks
        .url
        .trim_start_matches("http://")
        .parse()
        .expect("the receiver's address");
    Rc::new(Outgoing::new(
        Rc::new(Outward::reaching(HOOKS, at)) as Rc<dyn Fetch>
    ))
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::Text(text) | Value::Uuid(text) => text.to_string(),
        other => panic!("not text: {other:?}"),
    }
}

/// The caller a seller signs in as.
fn signed_in(seller: &Value) -> Caller {
    Caller {
        value: jadpo_runtime::http::identity::principal(
            PrincipalKind::User,
            field(seller, "id"),
            field(seller, "handle"),
        ),
        key: "user".to_string(),
        session: None,
        sign_in: None,
        multi_factor: false,
    }
}

/// Runs `action` with `args` as `seller`, when one is given, and hands
/// what it set off to the runner.
async fn call(
    program: &Program,
    harness: &Harness<jadpo_host_native::Postgres>,
    host: &dyn Host,
    action: &str,
    args: Vec<Value>,
    seller: Option<&Value>,
) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = harness.ctx_on(program, host);
    ctx.set_caller(seller.map(signed_in));
    let outcome = program.run(&mut ctx, id, args).await;
    let work = ctx.take_deferred();
    drop(ctx);
    run_deferred(program, host, work).await;
    outcome
}

fn value(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

/// The code a failed outcome raised.
fn failed(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Failed(failure) => failure.code.clone(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// A seller called `handle`, as the record a caller is made from.
async fn seller(
    program: &Program,
    harness: &Harness<jadpo_host_native::Postgres>,
    host: &dyn Host,
    handle: &str,
) -> Value {
    value(
        call(
            program,
            harness,
            host,
            "join",
            vec![Value::text(handle)],
            None,
        )
        .await,
    )
}

/// A shop `owner` opens, its view.
async fn shop(
    program: &Program,
    harness: &Harness<jadpo_host_native::Postgres>,
    host: &dyn Host,
    owner: &Value,
    name: &str,
) -> Value {
    let caller = signed_in(owner).value;
    value(
        call(
            program,
            harness,
            host,
            "open_shop",
            vec![Value::text(name), caller],
            Some(owner),
        )
        .await,
    )
}

/// `url` registered for `shop` as `seller`.
async fn register(
    program: &Program,
    harness: &Harness<jadpo_host_native::Postgres>,
    host: &dyn Host,
    seller: &Value,
    shop: &Value,
    url: &str,
) -> Outcome {
    let input = Value::record(
        "NewWebhook",
        vec![
            (Rc::from("shop_id"), field(shop, "id")),
            (Rc::from("url"), Value::text(url)),
        ],
    );
    call(
        program,
        harness,
        host,
        "add_webhook",
        vec![input],
        Some(seller),
    )
    .await
}

/// The secret a registration gave.
fn secret(outcome: Outcome) -> String {
    text(&field(&value(outcome), "secret"))
}

/// The order delivery's state, attempts and last failure's code.
async fn deliveries(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> Vec<(String, i32, Option<String>)> {
    harness
        .rows(
            "SELECT \"status\", \"attempts\", \"last_failure\" ->> 'code' FROM \"jadpo_deliveries\" WHERE \"subscriber\" LIKE 'webhooks %' ORDER BY \"status\"",
        )
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(status), SqlValue::Int4(attempts), code] => (
                status.clone(),
                *attempts,
                match code {
                    SqlValue::Text(code) => Some(code.clone()),
                    _ => None,
                },
            ),
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

/// Whether `received` checks under `secret` at `now`, as a Jadpo app
/// receiving it with `SignatureScheme.standard_webhooks` checks it.
fn signed_with(received: &Received, secret: &str, now: i64) -> bool {
    matches!(
        jadpo_runtime::webhooks::check(
            &jadpo_std::provider::SignatureScheme::StandardWebhooks,
            &[secret],
            &received.as_request(),
            now,
        ),
        Ok(Some(_))
    )
}

/// Checks `received` is the shop's `OrderPlaced` for `reference`, and
/// gives its id.
fn order(received: &Received, shop_id: &str, reference: &str) -> String {
    assert_eq!(received.method, "POST");
    assert_eq!(received.header("content-type"), Some("application/json"));
    let body: serde_json::Value = serde_json::from_slice(&received.body).expect("JSON");
    let id = received.header("webhook-id").expect("an id").to_string();
    assert_eq!(body["id"], serde_json::json!(id), "{body}");
    assert_eq!(body["type"], "order.placed", "{body}");
    assert_eq!(body["data"]["shop_id"], shop_id, "{body}");
    assert_eq!(body["data"]["reference"], reference, "{body}");
    assert!(body["data"]["placed_at"].is_string(), "{body}");
    id
}

/// Two addresses a shop registers each get its orders, each signed with
/// that address's own secret and no other; a third shop's order reaches
/// neither. An address that answered `503` is sent the order again after
/// the backoff, under the same id, and the one that took it isn't. Each
/// secret is shown once and kept only sealed.
#[test]
fn an_order_reaches_each_address_signed_with_its_own_secret() {
    let hooks = FakeHooks::start(&[503]);
    let Some(loaded) = load("delivered") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = Faked::new(&harness).with(jadpo_std::outgoing::CONNECTION, outgoing(&hooks));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let program = &loaded.program;
        let ann = seller(program, &harness, &host, "corner-books").await;
        let bob = seller(program, &harness, &host, "bob-prints").await;
        let books = shop(program, &harness, &host, &ann, "Corner Books").await;
        let prints = shop(program, &harness, &host, &bob, "Bob's Prints").await;
        let first = secret(
            register(program, &harness, &host, &ann, &books, &format!("https://{HOOKS}/a")).await,
        );
        // A second later, so the addresses are in the order registered.
        harness.set_now(T0 + 1_000);
        let second = secret(
            register(program, &harness, &host, &ann, &books, &format!("https://{HOOKS}/b")).await,
        );
        harness.set_now(T0);
        assert!(first.starts_with("whsec_") && second.starts_with("whsec_"));
        assert_ne!(first, second, "each address has a secret of its own");
        let raw = jadpo_value::encoding::from_base64(first.trim_start_matches("whsec_"))
            .expect("Base64");
        assert_eq!(raw.len(), 32);

        // Kept sealed: the column holds neither secret.
        let kept = harness
            .rows("SELECT \"secret\" FROM \"jadpo_webhook_addresses\" ORDER BY \"registered_at\", \"id\"")
            .await;
        assert_eq!(kept.len(), 2);
        for row in &kept {
            let [SqlValue::Bytes(sealed)] = row.as_slice() else {
                panic!("not sealed bytes: {row:?}");
            };
            for secret in [&first, &second] {
                assert!(
                    !sealed
                        .windows(secret.len())
                        .any(|window| window == secret.as_bytes()),
                    "a secret kept as it is"
                );
                assert!(
                    !sealed.windows(raw.len()).any(|window| window == raw.as_slice()),
                    "a secret's bytes kept as they are"
                );
            }
        }

        // Listed by their owner without their secrets.
        let listed = value(
            call(
                program,
                &harness,
                &host,
                "list_webhooks",
                vec![field(&books, "id")],
                Some(&ann),
            )
            .await,
        );
        let Value::List(addresses) = field(&listed, "webhooks") else {
            panic!("not a list");
        };
        let urls: Vec<String> = addresses
            .iter()
            .map(|address| text(&field(address, "url")))
            .collect();
        assert_eq!(
            urls,
            vec![format!("https://{HOOKS}/a"), format!("https://{HOOKS}/b")]
        );
        assert!(!format!("{listed:?}").contains("whsec_"));

        // Another shop's order goes to none of these addresses.
        let ordered = call(
            program,
            &harness,
            &host,
            "place_order",
            vec![field(&prints, "id"), Value::text("P-7")],
            Some(&bob),
        )
        .await;
        value(ordered);
        assert_eq!(hooks.received().len(), 0, "{:?}", hooks.received());

        // The shop's order: posted to both once its action commits, the
        // first answering `503`, the second `204`.
        let shop_id = text(&field(&books, "id"));
        value(
            call(
                program,
                &harness,
                &host,
                "place_order",
                vec![field(&books, "id"), Value::text("A-1001")],
                Some(&ann),
            )
            .await,
        );
        let sent = hooks.received();
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!((sent[0].path.as_str(), sent[1].path.as_str()), ("/a", "/b"));
        let id = order(&sent[0], &shop_id, "A-1001");
        assert_eq!(order(&sent[1], &shop_id, "A-1001"), id, "one id for both");
        assert!(signed_with(&sent[0], &first, T0));
        assert!(!signed_with(&sent[0], &second, T0), "never the other's");
        assert!(signed_with(&sent[1], &second, T0));
        assert!(!signed_with(&sent[1], &first, T0), "never the other's");
        let outbox = harness
            .rows("SELECT \"id\"::text FROM \"jadpo_outbox\" WHERE \"event\" LIKE '%OrderPlaced'")
            .await;
        assert!(outbox.contains(&vec![SqlValue::Text(id.clone())]), "{outbox:?}");
        let mut states = deliveries(&harness).await;
        states.sort();
        assert_eq!(
            states,
            vec![
                ("pending".to_string(), 1, Some("unavailable".to_string())),
                ("succeeded".to_string(), 1, None),
            ],
            "the other shop's, which had no address, and this one's"
        );

        // An hour on, only the address that didn't take it is sent it
        // again, under the same id, signed with its secret at the new time.
        harness.set_now(T0 + HOUR);
        sweep(program, &host, "main", T0 + HOUR)
            .await
            .expect("the sweep runs");
        let again = hooks.received();
        assert_eq!(again.len(), 3, "{again:?}");
        assert_eq!(again[2].path, "/a");
        assert_eq!(order(&again[2], &shop_id, "A-1001"), id);
        assert!(signed_with(&again[2], &first, T0 + HOUR));
        let mut states = deliveries(&harness).await;
        states.sort();
        assert_eq!(
            states,
            vec![
                ("succeeded".to_string(), 1, None),
                ("succeeded".to_string(), 2, None),
            ]
        );
        harness.set_now(T0 + 2 * HOUR);
        sweep(program, &host, "main", T0 + 2 * HOUR)
            .await
            .expect("the sweep runs");
        assert_eq!(hooks.received().len(), 3, "never sent again");
    });
}

/// An address that isn't HTTPS to a public host is refused as it is
/// registered: a private, loopback or link-local address, IPv4 or IPv6,
/// and `localhost`. Only the shop's owner registers, lists and removes its
/// addresses, at most ten. An address whose name resolves only somewhere
/// private when its event is sent is refused then, before anything
/// connects, and its delivery fails as `misconfigured`.
#[test]
fn a_private_address_is_refused() {
    let hooks = FakeHooks::start(&[]);
    let Some(loaded) = load("refused") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = Faked::new(&harness).with(jadpo_std::outgoing::CONNECTION, outgoing(&hooks));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let program = &loaded.program;
        let ann = seller(program, &harness, &host, "corner-books").await;
        let bob = seller(program, &harness, &host, "bob-prints").await;
        let books = shop(program, &harness, &host, &ann, "Corner Books").await;
        for address in [
            "http://hooks.corner-books.example/orders",
            "https://localhost/orders",
            "https://127.0.0.1/orders",
            "https://10.1.2.3/orders",
            "https://172.16.0.9/orders",
            "https://192.168.1.20/orders",
            "https://169.254.169.254/latest/meta-data",
            "https://100.64.0.1/orders",
            "https://[::1]/orders",
            "https://[fe80::1]/orders",
            "https://[fd12:3456::1]/orders",
            "https://[::ffff:127.0.0.1]/orders",
            "https://127.1/orders",
            "https://user:password@hooks.corner-books.example/orders",
        ] {
            let refused = register(program, &harness, &host, &ann, &books, address).await;
            assert_eq!(failed(&refused), "webhook_address_invalid", "{address}");
        }
        assert_eq!(
            harness
                .rows("SELECT count(*) FROM \"jadpo_webhook_addresses\"")
                .await,
            vec![vec![SqlValue::Int8(0)]],
            "none kept"
        );

        // Only the shop's owner may register, see or remove its addresses.
        let elsewhere = register(
            program,
            &harness,
            &host,
            &bob,
            &books,
            &format!("https://{HOOKS}/bob"),
        )
        .await;
        assert_eq!(failed(&elsewhere), "not_permitted");
        for at in 0..10 {
            secret(
                register(program, &harness, &host, &ann, &books, &format!("https://{HOOKS}/{at}"))
                    .await,
            );
        }
        let full = register(program, &harness, &host, &ann, &books, &format!("https://{HOOKS}/10")).await;
        assert_eq!(failed(&full), "webhook_addresses_full");
        let theirs = value(
            call(
                program,
                &harness,
                &host,
                "list_webhooks",
                vec![field(&books, "id")],
                Some(&bob),
            )
            .await,
        );
        assert_eq!(field(&theirs, "webhooks"), Value::list(Vec::new()));
        let listed = value(
            call(
                program,
                &harness,
                &host,
                "list_webhooks",
                vec![field(&books, "id")],
                Some(&ann),
            )
            .await,
        );
        let Value::List(addresses) = field(&listed, "webhooks") else {
            panic!("not a list");
        };
        assert_eq!(addresses.len(), 10);
        let ids: Vec<Value> = addresses.iter().map(|address| field(address, "id")).collect();
        value(
            call(program, &harness, &host, "drop_webhook", vec![ids[0].clone()], Some(&bob)).await,
        );
        assert_eq!(
            harness
                .rows("SELECT count(*) FROM \"jadpo_webhook_addresses\"")
                .await,
            vec![vec![SqlValue::Int8(10)]],
            "another seller removes nothing"
        );
        for id in &ids[1..] {
            value(call(program, &harness, &host, "drop_webhook", vec![id.clone()], Some(&ann)).await);
        }

        // The one left now resolves only to this machine, as a name
        // rebound since it was registered would: refused as it is sent,
        // nothing connected to.
        let listener = TcpListener::bind("127.0.0.1:0").expect("a listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("its address").port();
        harness
            .rows(&format!(
                "UPDATE \"jadpo_webhook_addresses\" SET \"url\" = 'https://localhost:{port}/orders' RETURNING 1"
            ))
            .await;
        value(
            call(
                program,
                &harness,
                &host,
                "place_order",
                vec![field(&books, "id"), Value::text("A-2002")],
                Some(&ann),
            )
            .await,
        );
        assert!(listener.accept().is_err(), "nothing connected");
        assert_eq!(hooks.received().len(), 0);
        assert_eq!(
            deliveries(&harness).await,
            vec![("failed".to_string(), 1, Some("misconfigured".to_string()))]
        );
    });
}
