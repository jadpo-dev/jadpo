//! What package code may do (area 11, "What package code may do"; chunk
//! 7.6): a provider package's actions call only its own API, by its
//! operations' bare names (`customers.create`), in any order, passing
//! answers between them; the app calls a public one through its
//! connection (`pay.start_subscription(input)`) from a listed domain,
//! mapping its package failures at the call as an operation's; and its
//! changing calls are steps of the calling action (area 12, "Several
//! changing calls in one action"), so a package action over 8 is refused
//! where it is declared, which is how publishing refuses it.

use crate::support;

use jadpo_db::Layout;

const APP: &str = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n    events: {\n        store: main\n    }\n}\n";

const CONFIG: &str = "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n";

const HOME: &str = "shared/providers/acme_pay/v1";

/// A resource `resource` of one keyed change, `create`, taking and giving
/// `PayRequest` and `PayReply`; `public` when `public`.
fn resource(resource: &str, public: bool) -> (String, String) {
    (
        format!("{HOME}/resources/{resource}.jadpo"),
        format!(
            "{}resource {resource} {{\n    create: {{\n        method: HttpMethod.post\n        path: \"/v1/{resource}\"\n        input: PayRequest\n        output: PayReply\n        changes: true\n        idempotent: true\n        failures: [CardDeclined]\n        source: PackageSource.written\n    }}\n}}\n",
            if public { "public " } else { "" }
        ),
    )
}

/// Acme Pay's package: `customers` and `cards` are its own, not public;
/// `invoices` is public.
fn package() -> Vec<(String, String)> {
    let mut out = vec![
        (
            format!("{HOME}/package.jadpo"),
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".to_string(),
        ),
        (
            format!("{HOME}/provider.jadpo"),
            "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/pay_request.jadpo"),
            "/// What each call takes.\npublic type PayRequest = Object {\n    reference: Reference\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/pay_reply.jadpo"),
            "/// What each call gives.\npublic type PayReply = Object {\n    id: Reference\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/reference.jadpo"),
            "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n".to_string(),
        ),
        (
            format!("{HOME}/failures/card_declined.jadpo"),
            "/// The card was declined.\npublic type CardDeclined = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was declined.\"\n}\n".to_string(),
        ),
    ];
    out.push(resource("customers", false));
    out.push(resource("cards", false));
    out.push(resource("invoices", true));
    out
}

/// `start_subscription`: the customer, then the card for it, each answer
/// passed to the next call, the card declined passed on.
const START: &str = "/// Creates the customer, then attaches the card.\npublic action start_subscription(input: PayRequest) fails CardDeclined -> PayReply {\n    var customer: PayReply = attempt customers.create(input)\n    return attempt cards.create(PayRequest {\n        reference: customer.id\n    })\n}\n";

const BILLING: &[(&str, &str)] = &[
    (
        "domains/billing/failures/payment_refused.jadpo",
        "/// The payment provider refused the card.\ntype PaymentRefused = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was refused.\"\n}\n",
    ),
    (
        "domains/billing/events/plan_chosen.jadpo",
        "use shared/providers/acme_pay\n\n/// A customer chose a plan.\ntype PlanChosen = Event {\n    plan: AcmePay.Reference\n}\n",
    ),
];

/// `subscribe(plan)` in the billing domain, calling `call` with the
/// plan's request and mapping a declined card.
fn subscribe(call: &str) -> String {
    format!(
        "use app {{ pay }}\nuse billing/failures {{ PaymentRefused }}\nuse shared/providers/acme_pay\n\n/// Starts a subscription.\naction subscribe(plan: AcmePay.Reference) fails PaymentRefused -> AcmePay.PayReply {{\n    return attempt {call}(AcmePay.PayRequest {{\n        reference: plan\n    }}) {{\n        card_declined: PaymentRefused\n    }}\n}}\n"
    )
}

/// A subscriber running `subscribe(event.plan)`.
const SUBSCRIBER: (&str, &str) = (
    "domains/billing/subscribers/start.jadpo",
    "use billing/actions { subscribe }\nuse billing/events { PlanChosen }\n\n/// Starts the subscription a customer chose.\nsubscriber start {\n    on: PlanChosen\n    run: subscribe(event.plan)\n}\n",
);

/// The project: the application, the package with `actions` in its
/// `actions/`, the billing domain with `subscribe` calling `call`, and
/// `more`.
fn files(actions: &[(&str, &str)], call: &str, more: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out = vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
    ];
    out.extend(package());
    out.extend(
        actions
            .iter()
            .map(|(name, text)| (format!("{HOME}/actions/{name}.jadpo"), text.to_string())),
    );
    out.extend(
        BILLING
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    out.push((
        "domains/billing/actions/subscribe.jadpo".to_string(),
        subscribe(call),
    ));
    out.extend(
        more.iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    out
}

/// Each problem, as `path rule: message`, but for unused imports and
/// declarations, migrations and records, which these projects don't keep.
fn problems(actions: &[(&str, &str)], call: &str, more: &[(&str, &str)]) -> Vec<String> {
    let files = files(actions, call, more);
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

// --- the package's side -----------------------------------------------------

#[test]
fn a_package_action_calls_its_own_operations_and_the_app_calls_it() {
    assert_eq!(
        problems(
            &[("start_subscription", START)],
            "pay.start_subscription",
            &[SUBSCRIBER]
        ),
        Vec::<String>::new()
    );
}

#[test]
fn an_own_operation_that_doesnt_exist_is_named_with_the_nearest() {
    let start = START.replace("customers.create", "customers.craete");
    assert_eq!(
        problems(&[("start_subscription", &start)], "pay.start_subscription", &[SUBSCRIBER]),
        [
            "shared/providers/acme_pay/v1/actions/start_subscription.jadpo semantic.unknown_callee: Resource `customers` has no operation `craete`"
        ]
    );
}

#[test]
fn an_own_operations_failure_is_passed_on_or_handled() {
    let start = START.replace("attempt customers.create", "customers.create");
    assert_eq!(
        problems(&[("start_subscription", &start)], "pay.start_subscription", &[SUBSCRIBER]),
        [
            "shared/providers/acme_pay/v1/actions/start_subscription.jadpo failure.attempt_required: `customers.create` can fail, and nothing handles it"
        ]
    );
}

#[test]
fn a_packages_function_cant_call_its_api() {
    let library = "/// Helpers.\nlibrary customer_opening {\n    /// Opens a customer.\n    function open(input: PayRequest) -> PayReply {\n        return customers.create(input)\n    }\n}\n";
    let mut more = vec![(
        "shared/providers/acme_pay/v1/libraries/customer_opening.jadpo",
        library,
    )];
    more.push(SUBSCRIBER);
    assert_eq!(
        problems(&[("start_subscription", START)], "pay.start_subscription", &more),
        [
            "shared/providers/acme_pay/v1/libraries/customer_opening.jadpo effect.function_persistence: A function can't call `customers.create`"
        ]
    );
}

#[test]
fn a_package_action_calls_no_other_action() {
    let open = "/// Opens the customer.\npublic action open(input: PayRequest) fails CardDeclined -> PayReply {\n    return attempt customers.create(input)\n}\n";
    let start = "/// Opens the customer, through another action.\npublic action start_subscription(input: PayRequest) fails CardDeclined -> PayReply {\n    return attempt open(input)\n}\n";
    assert_eq!(
        problems(
            &[("open", open), ("start_subscription", start)],
            "pay.start_subscription",
            &[SUBSCRIBER]
        ),
        [
            "shared/providers/acme_pay/v1/actions/start_subscription.jadpo package.action_invalid: `start_subscription` calls the package's action `open`"
        ]
    );
}

#[test]
fn a_resource_and_an_action_of_one_name_clash() {
    let customers = "/// Opens a customer.\npublic action invoices(input: PayRequest) fails CardDeclined -> PayReply {\n    return attempt customers.create(input)\n}\n";
    assert_eq!(
        problems(
            &[("start_subscription", START), ("invoices", customers)],
            "pay.start_subscription",
            &[SUBSCRIBER]
        ),
        [
            "shared/providers/acme_pay/v1/actions/invoices.jadpo package.action_invalid: `invoices` is both a resource and an action of `AcmePay.v1`"
        ]
    );
}

// --- the app's side ---------------------------------------------------------

#[test]
fn a_package_action_without_public_isnt_called_by_the_app() {
    let start = START.replace("public action", "action");
    assert_eq!(
        problems(&[("start_subscription", &start)], "pay.start_subscription", &[SUBSCRIBER]),
        [
            "domains/billing/actions/subscribe.jadpo package.not_public: `AcmePay.v1`'s action `start_subscription` isn't public"
        ]
    );
}

#[test]
fn a_package_actions_failures_are_mapped_at_the_call() {
    let subscribe = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Starts a subscription.\naction subscribe(plan: AcmePay.Reference) -> AcmePay.PayReply {\n    return pay.start_subscription(AcmePay.PayRequest {\n        reference: plan\n    })\n}\n";
    assert_eq!(
        problems(
            &[("start_subscription", START)],
            "pay.start_subscription",
            &[
                SUBSCRIBER,
                ("domains/billing/actions/subscribe.jadpo", subscribe)
            ]
        )
        .into_iter()
        .filter(|problem| problem.contains("failure."))
        .collect::<Vec<_>>(),
        [
            "domains/billing/actions/subscribe.jadpo failure.attempt_required: `pay.start_subscription` can fail, and nothing handles it"
        ]
    );
}

#[test]
fn an_unknown_name_through_the_connection_lists_its_actions_too() {
    assert_eq!(
        problems(&[("start_subscription", START)], "pay.start_subscriptoin", &[SUBSCRIBER]),
        [
            "domains/billing/actions/subscribe.jadpo semantic.unknown_callee: `pay` has no operation or action `start_subscriptoin`"
        ]
    );
}

// --- steps ------------------------------------------------------------------

/// A schedule running `renew()`, which calls the package action once.
fn nightly() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "domains/billing/schedules/nightly.jadpo",
            "use billing/actions { renew }\n\n/// Renews the night's subscriptions.\nschedule nightly {\n    every: 1d\n    run: renew()\n}\n",
        ),
        (
            "domains/billing/actions/renew.jadpo",
            "use app { pay }\nuse billing/failures { PaymentRefused }\nuse shared/providers/acme_pay\n\n/// Renews the night's subscriptions.\naction renew() fails PaymentRefused -> Unit {\n    var started: AcmePay.PayReply = attempt pay.start_subscription(AcmePay.PayRequest {\n        reference: \"night\"\n    }) {\n        card_declined: PaymentRefused\n    }\n    assert started.id == started.id\n}\n",
        ),
    ]
}

#[test]
fn a_package_actions_changing_calls_are_steps_of_the_calling_action() {
    assert_eq!(
        problems(&[("start_subscription", START)], "pay.start_subscription", &nightly()),
        [
            "domains/billing/schedules/nightly.jadpo step.context: `nightly` runs `renew`, which makes 2 changing calls, but a schedule's retry is its next run"
        ]
    );
}

#[test]
fn a_package_action_over_eight_steps_is_refused_where_it_is_declared() {
    let mut body = String::from(
        "/// Opens nine customers.\npublic action start_subscription(input: PayRequest) fails CardDeclined -> PayReply {\n    var c1: PayReply = attempt customers.create(input)\n",
    );
    for n in 2..=9 {
        body.push_str(&format!(
            "    var c{n}: PayReply = attempt customers.create(PayRequest {{\n        reference: c{}.id\n    }})\n",
            n - 1
        ));
    }
    body.push_str("    return c9\n}\n");
    assert_eq!(
        problems(&[("start_subscription", &body)], "pay.start_subscription", &[SUBSCRIBER]),
        [
            "shared/providers/acme_pay/v1/actions/start_subscription.jadpo step.too_many: `start_subscription` makes 9 changing calls in a row, more than 8"
        ]
    );
}
