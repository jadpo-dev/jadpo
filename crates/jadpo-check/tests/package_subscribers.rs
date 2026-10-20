//! A package's subscribers on its own webhooks (area 11, "What package
//! code may do"; chunk 7.6b): a provider package subscribes to the events
//! its webhooks bring, public or not, and runs one of its own actions,
//! which calls its own API to enrich the event and emits the package's
//! own declared event for the app. An event a package action emits is the
//! package's own, not one its provider sends, so it needs no `wire_name:`;
//! a package subscriber listens only for its webhooks' events; and the
//! app subscribes only to a package's public events.

use crate::support;

use jadpo_db::Layout;

const HOME: &str = "shared/providers/acme_pay/v1";

const APP: &str = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        payments: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n            webhooks: {\n                secret: config.pay_webhook_secret\n            }\n        }\n    }\n    events: {\n        store: main\n    }\n}\n";

const CONFIG: &str = "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n    pay_webhook_secret: Text {\n        secret: true\n    }\n}\n";

const PROVIDER: &str = "/// Acme Pay's API, which signs its webhooks as Stripe does.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    webhooks: {\n        check: SignatureScheme.stripe\n        event_id: \"id\"\n        event_type: \"type\"\n        occurred_at: \"created\"\n    }\n}\n";

/// The raw webhook, kept to the package.
const INVOICE_PAID: &str = "/// An invoice was paid.\ntype InvoicePaid = Event {\n    wire_name: \"invoice.paid\"\n    occurred_at: Instant\n    subscription: Reference\n}\n";

/// The enriched event the package's action emits, for the app.
const RENEWED: &str = "/// A subscription was renewed, with its plan.\npublic type SubscriptionRenewed = Event {\n    subscription: Reference\n    plan: Reference\n    paid_at: Instant\n}\n";

/// Looks the subscription up, then announces the renewal.
const ANNOUNCE: &str = "/// Looks up the subscription an invoice paid for, and announces it.\naction announce_renewal(event: InvoicePaid) -> Unit {\n    var found: Subscription = subscriptions.retrieve(SubscriptionAsked {\n        id: event.subscription\n    })\n    emit SubscriptionRenewed {\n        subscription: found.id\n        plan: found.plan\n        paid_at: event.occurred_at\n    }\n}\n";

const RENEWAL: &str = "/// Turns a paid invoice into a renewal.\nsubscriber renewal {\n    on: InvoicePaid\n    run: announce_renewal(event)\n}\n";

/// Acme Pay's package, with `files` in place of its own.
fn package(files: &[(&str, Option<&str>)]) -> Vec<(String, String)> {
    let own: Vec<(&str, &str)> = vec![
        (
            "package.jadpo",
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        ("provider.jadpo", PROVIDER),
        (
            "types/reference.jadpo",
            "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n",
        ),
        (
            "types/subscription_asked.jadpo",
            "/// Which subscription to look up.\ntype SubscriptionAsked = Object {\n    id: Reference\n}\n",
        ),
        (
            "types/subscription.jadpo",
            "/// A subscription, as Acme Pay keeps it.\ntype Subscription = Object {\n    id: Reference\n    plan: Reference\n}\n",
        ),
        (
            "resources/subscriptions.jadpo",
            "resource subscriptions {\n    retrieve: {\n        method: HttpMethod.post\n        path: \"/v1/subscriptions/lookup\"\n        input: SubscriptionAsked\n        output: Subscription\n        source: PackageSource.written\n    }\n}\n",
        ),
        ("events/invoice_paid.jadpo", INVOICE_PAID),
        ("events/subscription_renewed.jadpo", RENEWED),
        ("actions/announce_renewal.jadpo", ANNOUNCE),
        ("subscribers/renewal.jadpo", RENEWAL),
    ];
    let mut out: Vec<(String, String)> = own
        .iter()
        .filter(|(path, _)| files.iter().all(|(changed, _)| changed != path))
        .map(|(path, text)| (format!("{HOME}/{path}"), text.to_string()))
        .collect();
    out.extend(
        files
            .iter()
            .filter_map(|(path, text)| Some((format!("{HOME}/{path}"), (*text)?.to_string()))),
    );
    out
}

/// The app's side: a renewal kept by `billing` when Acme Pay announces
/// one.
const BILLING: &[(&str, &str)] = &[
    (
        "domains/billing/entities/renewal.jadpo",
        "use shared/providers/acme_pay\n\nentity Renewal {\n    id: Uuid\n    subscription: AcmePay.Reference\n    plan: AcmePay.Reference\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n",
    ),
    (
        "domains/billing/actions/record_renewal.jadpo",
        "use billing/entities { Renewal }\nuse shared/providers/acme_pay\n\n/// Keeps a renewal Acme Pay announced.\naction record_renewal(event: AcmePay.SubscriptionRenewed) -> Renewal {\n    return Renewal.create {\n        set: {\n            subscription: event.subscription\n            plan: event.plan\n        }\n    }\n}\n",
    ),
    (
        "domains/billing/subscribers/renewed.jadpo",
        "use billing/actions { record_renewal }\nuse shared/providers/acme_pay\n\n/// Keeps each renewal.\nsubscriber renewed {\n    on: AcmePay.SubscriptionRenewed\n    run: record_renewal(event)\n}\n",
    ),
];

/// Each problem of the project with the package's `changes` and the app's
/// `more`, as `path rule: message`, but for unused imports and
/// declarations, migrations and records, which these projects don't keep.
fn problems(changes: &[(&str, Option<&str>)], more: &[(&str, &str)]) -> Vec<String> {
    let mut files = vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
    ];
    files.extend(package(changes));
    for (path, text) in BILLING.iter().chain(more) {
        files.retain(|(written, _)| written != path);
        files.push((path.to_string(), text.to_string()));
    }
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
fn a_package_subscriber_enriches_its_webhook_and_emits_for_the_app() {
    assert_eq!(problems(&[], &[]), Vec::<String>::new());
}

#[test]
fn the_app_subscribes_only_to_a_packages_public_events() {
    let subscriber = "use billing/actions { note_invoice }\nuse shared/providers/acme_pay\n\n/// Notes each paid invoice.\nsubscriber invoiced {\n    on: AcmePay.InvoicePaid\n    run: note_invoice(event.subscription)\n}\n";
    let action = "use shared/providers/acme_pay\n\n/// Notes a paid invoice.\naction note_invoice(subscription: AcmePay.Reference) -> AcmePay.Reference {\n    return subscription\n}\n";
    let found = problems(
        &[],
        &[
            ("domains/billing/subscribers/invoiced.jadpo", subscriber),
            ("domains/billing/actions/note_invoice.jadpo", action),
        ],
    );
    assert_eq!(
        found,
        [
            "domains/billing/subscribers/invoiced.jadpo package.not_public: `AcmePay.InvoicePaid` isn't public"
        ]
    );
}

#[test]
fn a_package_subscriber_listens_only_for_its_webhooks_events() {
    let renewal = RENEWAL
        .replace("on: InvoicePaid", "on: SubscriptionRenewed")
        .replace("announce_renewal(event)", "announce_again(event)");
    let again = "/// Announces a renewal again.\naction announce_again(event: SubscriptionRenewed) -> Unit {\n    assert event.plan == event.plan\n}\n";
    assert_eq!(
        problems(
            &[
                ("subscribers/renewal.jadpo", Some(&renewal)),
                ("actions/announce_again.jadpo", Some(again)),
            ],
            &[]
        ),
        [
            "shared/providers/acme_pay/v1/subscribers/renewal.jadpo subscriber.event_invalid: `on: SubscriptionRenewed` isn't one of `AcmePay.v1`'s webhooks"
        ]
    );
}

#[test]
fn a_package_without_webhooks_has_no_subscriber() {
    let provider = "/// Acme Pay's API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n";
    let app = APP.replace(
        "            webhooks: {\n                secret: config.pay_webhook_secret\n            }\n",
        "",
    );
    let found = problems(
        &[("provider.jadpo", Some(provider))],
        &[("app.jadpo", &app)],
    );
    assert_eq!(
        found,
        [
            "shared/providers/acme_pay/v1/subscribers/renewal.jadpo subscriber.event_invalid: `on: InvoicePaid` isn't one of `AcmePay.v1`'s webhooks"
        ]
    );
}

#[test]
fn a_package_action_never_emits_one_of_its_webhooks() {
    let announce = ANNOUNCE.replace(
        "    emit SubscriptionRenewed {",
        "    emit InvoicePaid {\n        subscription: found.id\n        occurred_at: event.occurred_at\n    }\n    emit SubscriptionRenewed {",
    );
    assert_eq!(
        problems(&[("actions/announce_renewal.jadpo", Some(&announce))], &[]),
        [
            "shared/providers/acme_pay/v1/actions/announce_renewal.jadpo package.action_invalid: `announce_renewal` emits `InvoicePaid`, which `AcmePay.v1`'s webhooks bring"
        ]
    );
}

#[test]
fn an_event_nothing_emits_still_names_its_webhook() {
    let announce = ANNOUNCE.replace(
        "    emit SubscriptionRenewed {\n        subscription: found.id\n        plan: found.plan\n        paid_at: event.occurred_at\n    }\n",
        "    assert found.plan == found.plan\n",
    );
    assert_eq!(
        problems(&[("actions/announce_renewal.jadpo", Some(&announce))], &[]),
        [
            "shared/providers/acme_pay/v1/events/subscription_renewed.jadpo package.wire_invalid: Webhook event `SubscriptionRenewed` doesn't declare `occurred_at: Instant`",
            "shared/providers/acme_pay/v1/events/subscription_renewed.jadpo package.wire_name_invalid: Webhook event `SubscriptionRenewed` doesn't give `wire_name:`"
        ]
    );
}

#[test]
fn a_public_package_action_still_never_emits() {
    let announce = ANNOUNCE.replace("action announce_renewal", "public action announce_renewal");
    let paid = INVOICE_PAID.replace("type InvoicePaid", "public type InvoicePaid");
    assert_eq!(
        problems(
            &[
                ("actions/announce_renewal.jadpo", Some(&announce)),
                ("events/invoice_paid.jadpo", Some(&paid)),
            ],
            &[]
        ),
        [
            "shared/providers/acme_pay/v1/actions/announce_renewal.jadpo package.action_invalid: `announce_renewal` is public and emits `SubscriptionRenewed`"
        ]
    );
}

/// The package's subscribers are in the plan's table, the events it emits
/// kept in the app's events store.
#[test]
fn a_package_subscriber_reads_cleanly() {
    let mut files = vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
    ];
    files.extend(package(&[]));
    files.extend(
        BILLING
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let subscribers = jadpo_check::events::subscribers(&db, project);
    let names: Vec<(&str, &str, &str)> = subscribers
        .iter()
        .map(|model| (model.key.as_str(), model.event.name(), model.run.name()))
        .collect();
    assert_eq!(
        names,
        [
            (
                "subscriber AcmePay.renewal",
                "AcmePay.InvoicePaid",
                "AcmePay.announce_renewal"
            ),
            (
                "subscriber renewed",
                "AcmePay.SubscriptionRenewed",
                "record_renewal"
            ),
        ]
    );
    assert_eq!(
        jadpo_check::events::outbox_store(&db, project, "AcmePay.announce_renewal").as_deref(),
        Some("main")
    );
}
