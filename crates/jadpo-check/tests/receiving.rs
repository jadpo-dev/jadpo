//! Queues through `receiving` (area 12, "Events from outside"; area 11,
//! "Queues"; chunk 7.7c): `receiving:` names a connection to a package
//! that provides `receiving`, where each message's type and id are, and
//! the events it may carry, each with its `wire_name:`; the package maps
//! the capability's operations onto its own in `provides/receiving.jadpo`;
//! an AWS package names its signing service and its operations' prefix in
//! `aws:`, and its connection gives a key id and a secret. A queue on
//! Cloudflare Queues is a connection with `driver:
//! Driver.cloudflare_queues` naming the queue and its dead-letter queue
//! (chunk 7.7c2).

use crate::support;

use jadpo_db::Layout;

const HOME: &str = "shared/providers/acme_queue/v1";

const CONFIG: &str = "config {\n    queue_key_id: Text\n    queue_secret: Text {\n        secret: true\n    }\n    queue_endpoint: Url\n    queue_url: Text\n}\n";

/// The app: one queue connection, receiving `queue` as written.
fn app(credential: &str, queue: &str) -> String {
    format!(
        "use stock/events {{ OrderShipped, StockLow }}\n\napplication {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n        warehouse: {{\n            package: AcmeQueue.v1\n            domains: [stock]\n            credential: {credential}\n            host: config.queue_endpoint\n            queue_url: config.queue_url\n        }}\n    }}\n    events: {{\n        store: main\n    }}\n    receiving: {{\n{queue}    }}\n}}\n"
    )
}

const CREDENTIAL: &str = "{\n                key_id: config.queue_key_id\n                secret: config.queue_secret\n            }";

const QUEUE: &str = "        warehouse: {\n            type_at: \"type\"\n            id_at: \"message_id\"\n            events: [OrderShipped, StockLow]\n        }\n";

const PROVIDER: &str = "/// Acme Queue's JSON API, signed as AWS signs.\nprovider {\n    protocol: Protocol.aws_json\n    credential: CredentialScheme.aws_sigv4\n    aws: {\n        service: \"queue\"\n        target: \"AcmeQueue\"\n    }\n    error_code: \"__type\"\n    settings: {\n        queue_url: Text\n    }\n}\n";

const MAPPING: &str = "/// Reads ten messages at a time.\nprovide receiving {\n    receive: {\n        call: messages.receive_message\n        input: {\n            queue_url: settings.queue_url\n            max: 10\n        }\n        messages: response.messages\n        body: item.body\n        handle: item.handle\n    }\n    acknowledge: {\n        call: messages.delete_message\n        input: {\n            queue_url: settings.queue_url\n            handle: message.handle\n        }\n    }\n}\n";

const SHIPPED: &str = "/// An order left the warehouse.\ntype OrderShipped = Event {\n    wire_name: \"order.shipped\"\n    order: Text {\n        max_length: 40\n    }\n}\n";

const LOW: &str = "/// A product is running out.\ntype StockLow = Event {\n    wire_name: \"stock.low\"\n    sku: Text {\n        max_length: 40\n    }\n}\n";

fn package(provider: &str, mapping: &str) -> Vec<(String, String)> {
    vec![
        (
            format!("{HOME}/package.jadpo"),
            "/// Acme Queue.\npackage AcmeQueue.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".to_string(),
        ),
        (format!("{HOME}/provider.jadpo"), provider.to_string()),
        (
            format!("{HOME}/resources/messages.jadpo"),
            "public resource messages {\n    receive_message: {\n        method: HttpMethod.post\n        path: \"/\"\n        input: ReceiveRequest\n        output: Received\n        changes: false\n        source: PackageSource.written\n    }\n    delete_message: {\n        method: HttpMethod.post\n        path: \"/\"\n        input: DeleteRequest\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/receive_request.jadpo"),
            "/// Which queue to read.\npublic type ReceiveRequest = Object {\n    queue_url: Text {\n        wire_name: \"QueueUrl\"\n    }\n    max: Int {\n        wire_name: \"Max\"\n    }\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/received.jadpo"),
            "/// What a receive gives.\npublic type Received = Object {\n    messages: List<QueuedMessage>? {\n        wire_name: \"Messages\"\n    }\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/queued_message.jadpo"),
            "/// One message.\npublic type QueuedMessage = Object {\n    body: Text {\n        wire_name: \"Body\"\n    }\n    handle: Text {\n        wire_name: \"Handle\"\n    }\n    count: Int\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/delete_request.jadpo"),
            "/// The message to delete.\npublic type DeleteRequest = Object {\n    queue_url: Text {\n        wire_name: \"QueueUrl\"\n    }\n    handle: Text {\n        wire_name: \"Handle\"\n    }\n}\n".to_string(),
        ),
        (format!("{HOME}/provides/receiving.jadpo"), mapping.to_string()),
    ]
}

struct Project<'a> {
    credential: &'a str,
    queue: &'a str,
    provider: &'a str,
    mapping: &'a str,
    shipped: &'a str,
    low: &'a str,
}

const PROJECT: Project<'static> = Project {
    credential: CREDENTIAL,
    queue: QUEUE,
    provider: PROVIDER,
    mapping: MAPPING,
    shipped: SHIPPED,
    low: LOW,
};

/// Each problem as `path rule: message`, but for unused declarations,
/// migrations and records, which these projects don't keep.
fn problems(project: &Project<'_>) -> Vec<String> {
    let mut files = vec![
        ("app.jadpo".to_string(), app(project.credential, project.queue)),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/stock/events/order_shipped.jadpo".to_string(),
            project.shipped.to_string(),
        ),
        (
            "domains/stock/events/stock_low.jadpo".to_string(),
            project.low.to_string(),
        ),
        (
            "domains/stock/entities/shipment.jadpo".to_string(),
            "entity Shipment {\n    id: Uuid\n    order: Text {\n        max_length: 40\n    }\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n".to_string(),
        ),
    ];
    files.extend(package(project.provider, project.mapping));
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

#[test]
fn a_queue_of_events_named_by_their_wire_names_checks() {
    assert_eq!(problems(&PROJECT), Vec::<String>::new());
    // A queue of one kind needs no `type_at:`, nor its event a
    // `wire_name:`.
    let one = "        warehouse: {\n            id_at: \"message_id\"\n            events: [OrderShipped]\n        }\n";
    let plain = SHIPPED.replace("    wire_name: \"order.shipped\"\n", "");
    assert_eq!(
        problems(&Project {
            queue: one,
            shipped: &plain,
            ..PROJECT
        }),
        Vec::<String>::new()
    );
}

#[test]
fn a_queue_says_where_ids_and_types_are_and_names_each_event() {
    let cases: [(&str, &str); 4] = [
        (
            "        warehouse: {\n            type_at: \"type\"\n            events: [OrderShipped, StockLow]\n        }\n",
            "app.jadpo receiving.invalid: `warehouse` doesn't say where each message's id is",
        ),
        (
            "        warehouse: {\n            id_at: \"message_id\"\n            events: [OrderShipped, StockLow]\n        }\n",
            "app.jadpo receiving.invalid: `warehouse` lists 2 events but doesn't say where each message's type is",
        ),
        (
            "        warehouse: {\n            type_at: \"type\"\n            id_at: \"message_id\"\n            events: [OrderShipped, StockLow]\n            retries: 3\n        }\n",
            "app.jadpo receiving.invalid: A queue in `receiving:` has no key `retries:`",
        ),
        (
            "        main: {\n            id_at: \"message_id\"\n            events: [OrderShipped]\n        }\n",
            "app.jadpo receiving.invalid: `main` isn't a connection a queue is read through",
        ),
    ];
    for (queue, problem) in cases {
        assert_eq!(
            problems(&Project { queue, ..PROJECT }),
            [problem],
            "{queue}"
        );
    }
}

#[test]
fn each_event_of_a_queue_with_types_has_a_wire_name_of_its_own() {
    let unnamed = LOW.replace("    wire_name: \"stock.low\"\n", "");
    assert_eq!(
        problems(&Project {
            low: &unnamed,
            ..PROJECT
        }),
        ["app.jadpo receiving.invalid: `StockLow` doesn't give `wire_name:`"]
    );
    let same = LOW.replace("stock.low", "order.shipped");
    assert_eq!(
        problems(&Project {
            low: &same,
            ..PROJECT
        }),
        ["app.jadpo receiving.invalid: `StockLow` and `OrderShipped` are both `order.shipped`"]
    );
}

#[test]
fn a_package_maps_receiving_onto_its_own_operations_as_data() {
    let cases: [(&str, &str, &str); 4] = [
        (
            "messages.receive_message",
            "messages.recieve_message",
            "shared/providers/acme_queue/v1/provides/receiving.jadpo package.provider_invalid: `messages.recieve_message` isn't one of the package's operations",
        ),
        (
            "max: 10",
            "max: settings.max",
            "shared/providers/acme_queue/v1/provides/receiving.jadpo package.provider_invalid: `settings.max` isn't a value a mapping can give",
        ),
        (
            "body: item.body",
            "body: item.count",
            "shared/providers/acme_queue/v1/provides/receiving.jadpo package.provider_invalid: `item.count` isn't text",
        ),
        (
            "            max: 10\n",
            "",
            "shared/providers/acme_queue/v1/provides/receiving.jadpo package.provider_invalid: `messages.receive_message`'s input field `max` isn't mapped",
        ),
    ];
    for (from, to, problem) in cases {
        let mapping = MAPPING.replace(from, to);
        assert_eq!(
            problems(&Project {
                mapping: &mapping,
                ..PROJECT
            }),
            [problem],
            "{to}"
        );
    }
}

#[test]
fn an_aws_package_names_its_service_and_its_connection_gives_a_key_id_and_a_secret() {
    let untargeted = PROVIDER.replace("        target: \"AcmeQueue\"\n", "");
    assert_eq!(
        problems(&Project {
            provider: &untargeted,
            ..PROJECT
        }),
        [
            "shared/providers/acme_queue/v1/provider.jadpo package.provider_invalid: `provider` doesn't give `aws: { target: }`"
        ]
    );
    assert_eq!(
        problems(&Project {
            credential: "config.queue_secret",
            ..PROJECT
        }),
        [
            "app.jadpo connection.value_invalid: `warehouse`'s AWS credential needs a key id and a secret"
        ]
    );
    let shown = CREDENTIAL.replace("config.queue_secret", "config.queue_key_id");
    assert_eq!(
        problems(&Project {
            credential: &shown,
            ..PROJECT
        }),
        [
            "app.jadpo connection.value_invalid: `warehouse`'s secret access key must be a secret setting"
        ]
    );
}

const CLOUDFLARE: &str = "            driver: Driver.cloudflare_queues\n            queue: \"warehouse-events\"\n            dead_letter_queue: \"warehouse-events-dead\"\n";

/// Each problem of an app receiving `QUEUE` through Cloudflare Queues,
/// its connection `warehouse` holding `connection` and `extra` declaring
/// more connections.
fn cloudflare_problems(connection: &str, extra: &str) -> Vec<String> {
    let app = format!(
        "use stock/events {{ OrderShipped, StockLow }}\n\napplication {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n        warehouse: {{\n{connection}        }}\n{extra}    }}\n    events: {{\n        store: main\n    }}\n    receiving: {{\n{QUEUE}    }}\n}}\n"
    );
    let files = [
        ("app.jadpo", app.as_str()),
        ("domains/stock/events/order_shipped.jadpo", SHIPPED),
        ("domains/stock/events/stock_low.jadpo", LOW),
        (
            "domains/stock/entities/shipment.jadpo",
            "entity Shipment {\n    id: Uuid\n    order: Text {\n        max_length: 40\n    }\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n",
        ),
    ];
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

#[test]
fn a_queue_on_cloudflare_queues_names_the_queue_and_its_dead_letter_queue() {
    assert_eq!(cloudflare_problems(CLOUDFLARE, ""), Vec::<String>::new());
    let cases: [(String, &str); 5] = [
        (
            CLOUDFLARE.replace("            queue: \"warehouse-events\"\n", ""),
            "app.jadpo connection.key_missing: `warehouse` doesn't give `queue:`",
        ),
        (
            CLOUDFLARE.replace(
                "            dead_letter_queue: \"warehouse-events-dead\"\n",
                "",
            ),
            "app.jadpo connection.key_missing: `warehouse` doesn't give `dead_letter_queue:`",
        ),
        (
            CLOUDFLARE.replace("\"warehouse-events\"", "\"Warehouse_Events\""),
            "app.jadpo connection.value_invalid: `warehouse`'s `queue:` must be a queue's name",
        ),
        (
            CLOUDFLARE.replace("warehouse-events-dead", "warehouse-events"),
            "app.jadpo connection.value_invalid: `warehouse`'s dead-letter queue is the queue itself",
        ),
        (
            format!("{CLOUDFLARE}            retries: 5\n"),
            "app.jadpo connection.key_invalid: A connection with `driver: Driver.cloudflare_queues` takes no `retries:`",
        ),
    ];
    for (connection, problem) in cases {
        assert_eq!(
            cloudflare_problems(&connection, ""),
            [problem],
            "{connection}"
        );
    }
    let second = "        billing: {\n            driver: Driver.cloudflare_queues\n            queue: \"warehouse-events\"\n            dead_letter_queue: \"billing-dead\"\n        }\n";
    assert_eq!(
        cloudflare_problems(CLOUDFLARE, second),
        [
            "app.jadpo connection.value_invalid: `billing` and `warehouse` both name the queue `warehouse-events`"
        ]
    );
    // A dead-letter queue has a consumer of its own, which parks what it
    // hands back for its connection (chunk 8.4b): it is named once too.
    let shared = "        billing: {\n            driver: Driver.cloudflare_queues\n            queue: \"billing\"\n            dead_letter_queue: \"warehouse-events-dead\"\n        }\n";
    assert_eq!(
        cloudflare_problems(CLOUDFLARE, shared),
        [
            "app.jadpo connection.value_invalid: `billing`'s dead-letter queue `warehouse-events-dead` is named by `warehouse` already"
        ]
    );
    let dead_as_queue = "        billing: {\n            driver: Driver.cloudflare_queues\n            queue: \"warehouse-events-dead\"\n            dead_letter_queue: \"billing-dead\"\n        }\n";
    assert_eq!(
        cloudflare_problems(CLOUDFLARE, dead_as_queue),
        [
            "app.jadpo connection.value_invalid: `billing` and `warehouse` both name the queue `warehouse-events-dead`"
        ]
    );
}

#[test]
fn a_queue_name_is_lowercase_letters_digits_and_dashes() {
    use jadpo_check::receiving::queue_name_ok;
    for good in ["warehouse-events", "q", "2026-orders", &"a".repeat(63)] {
        assert!(queue_name_ok(good), "{good}");
    }
    for bad in [
        "",
        "-events",
        "events-",
        "Events",
        "warehouse_events",
        "a b",
        &"a".repeat(64),
    ] {
        assert!(!queue_name_ok(bad), "{bad}");
    }
}
