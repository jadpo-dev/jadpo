//! Several changing calls in one action (area 12, "Several changing calls
//! in one action"; area 11, "Several changing calls in one action"; area
//! 3, "No outside calls inside `for`"; chunk 7.5): what `jadpo check`
//! keeps of them.
//!
//! An action may make up to 8 changing calls on one way through it,
//! counting those of the actions it calls, each taking its package's key
//! or safe to repeat as it is (`step.too_many`); a changing call with no
//! key is the only changing call in its action (`step.unkeyed`); several
//! are made only where the action's key survives a retry, a subscriber or
//! a route whose API requires `Idempotency-Key`, never a schedule, a
//! route whose API doesn't, or a run's item (`step.context`); none of
//! them is an operation marked `large_output: true`
//! (`step.large_output`); and a store keeps their record
//! (`step.store_required`). No action makes an outside call inside `for`
//! (`effect.call_in_loop`), so every way through it has a length the
//! compiler knows.

use crate::support;

use jadpo_db::{Database, Layout};

/// The application: a database, the payment provider's connection, a
/// store of files, and an API whose writes require `Idempotency-Key`.
fn app(events: bool) -> String {
    app_with(
        events,
        "    steps: {\n        encryption_key: config.step_encryption_key\n    }\n",
    )
}

/// [`app`], with `steps` in place of its `steps:` block.
fn app_with(events: bool, steps: &str) -> String {
    let events = if events {
        format!("    events: {{\n        store: main\n    }}\n{steps}")
    } else {
        steps.to_string()
    };
    format!(
        "application {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n        pay: {{\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }}\n        sheets: {{\n            driver: Driver.disk\n            path: \"/var/lib/billing/sheets\"\n        }}\n    }}\n    apis: {{\n        main: {{\n            prefix: /\n        }}\n        partners: {{\n            prefix: /partners\n            idempotency: Idempotency.required\n        }}\n    }}\n{events}}}\n"
    )
}

/// A provider operation's resource, `resource.operation`, sent as
/// `method` and taking `PayRequest`, with `keys` after `output:`.
fn resource(resource: &str, operation: &str, method: &str, keys: &str) -> (String, String) {
    (
        format!("shared/providers/acme_pay/v1/resources/{resource}.jadpo"),
        format!(
            "public resource {resource} {{\n    {operation}: {{\n        method: HttpMethod.{method}\n        path: \"/v1/{resource}\"\n        input: PayRequest\n        output: PayReply\n{keys}        source: PackageSource.written\n    }}\n}}\n"
        ),
    )
}

/// Acme Pay's package: three keyed changes, a read, a change with no key
/// and a keyed change whose answer is large.
fn package() -> Vec<(String, String)> {
    let keyed =
        "        changes: true\n        idempotent: true\n        failures: [CardDeclined]\n";
    let mut out = vec![
        (
            "shared/providers/acme_pay/v1/package.jadpo".to_string(),
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/provider.jadpo".to_string(),
            "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n        max_length: 255\n    }\n    error_code: \"error.code\"\n}\n".to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/pay_request.jadpo".to_string(),
            "/// What each call takes.\npublic type PayRequest = Object {\n    reference: Reference\n}\n".to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/pay_reply.jadpo".to_string(),
            "/// What each call gives.\npublic type PayReply = Object {\n    id: Reference\n}\n".to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/reference.jadpo".to_string(),
            "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n".to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/failures/card_declined.jadpo".to_string(),
            "/// The card was declined.\npublic type CardDeclined = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was declined.\"\n}\n".to_string(),
        ),
    ];
    out.push(resource("customers", "create", "post", keyed));
    out.push(resource("cards", "attach", "post", keyed));
    out.push(resource("subscriptions", "create", "post", keyed));
    out.push(resource(
        "invoices",
        "get",
        "get",
        "        changes: false\n",
    ));
    out.push(resource(
        "receipts",
        "send",
        "post",
        "        changes: true\n",
    ));
    out.push(resource(
        "statements",
        "export",
        "post",
        "        changes: true\n        idempotent: true\n        large_output: true\n",
    ));
    out
}

/// The billing domain's types, failures, event, entity and query.
const BILLING: &[(&str, &str)] = &[
    (
        "config.jadpo",
        "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n    step_encryption_key: SigningKey\n}\n",
    ),
    (
        "domains/billing/failures/payment_refused.jadpo",
        "/// The payment provider refused the card.\ntype PaymentRefused = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was refused.\"\n}\n",
    ),
    (
        "domains/billing/failures/no_room.jadpo",
        "/// The sheets store had no room.\ntype NoRoom = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the sheet.\"\n}\n",
    ),
    (
        "domains/billing/types/early.jadpo",
        "/// Whether to stop early.\ntype Early = Bool {}\n",
    ),
    (
        "domains/billing/types/plan_choice.jadpo",
        "use shared/providers/acme_pay\n\n/// A plan a customer chooses.\ntype PlanChoice = Input {\n    plan: AcmePay.Reference\n}\n",
    ),
    (
        "domains/billing/types/account_page.jadpo",
        "/// A page of accounts.\ntype AccountPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/billing/events/plan_chosen.jadpo",
        "use shared/providers/acme_pay\n\n/// A customer chose a plan.\ntype PlanChosen = Event {\n    plan: AcmePay.Reference\n}\n",
    ),
    (
        "domains/billing/entities/account.jadpo",
        "use shared/providers/acme_pay\n\n/// A customer's account and the subscription it pays for.\nentity Account {\n    id: Uuid\n    subscription: AcmePay.Reference\n\n    policy: {\n        create: [Access.public, Access.system]\n        read: [Access.public, Access.system]\n    }\n}\n",
    ),
    (
        "domains/billing/queries/accounts.jadpo",
        "use billing/entities { Account }\nuse billing/types { AccountPage }\n\n/// The accounts, a page at a time.\nquery accounts(page: AccountPage) -> Page<Account> {\n    from: Account\n    order: [Account.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    ),
    // One keyed change, then the account: what most actions do.
    (
        "domains/billing/actions/open_customer.jadpo",
        "use app { pay }\nuse billing/failures { PaymentRefused }\nuse shared/providers/acme_pay\n\n/// Opens the customer at Acme Pay.\naction open_customer(plan: AcmePay.Reference) fails PaymentRefused -> AcmePay.PayReply {\n    return attempt pay.customers.create(AcmePay.PayRequest {\n        reference: plan\n    }) {\n        card_declined: PaymentRefused\n    }\n}\n",
    ),
];

/// The imports every `keep` may use; one it doesn't use isn't reported.
const IMPORTS: &str = "use app { pay, sheets }\nuse billing/actions { open_customer }\nuse billing/entities { Account }\nuse billing/events { PlanChosen }\nuse billing/failures { NoRoom, PaymentRefused }\nuse billing/types { Early, PlanChoice }\nuse shared/providers/acme_pay\n\n";

const KEEP: &str = "domains/billing/actions/keep.jadpo";

/// The project: the application, with `events: { store: }` when
/// `events`, the package, the billing domain, `keep` as an action file of
/// its own, and `more` beside it.
fn files(events: bool, keep: &str, more: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out = vec![("app.jadpo".to_string(), app(events))];
    out.extend(package());
    out.extend(
        BILLING
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    out.push((KEEP.to_string(), format!("{IMPORTS}{keep}")));
    out.extend(
        more.iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    out
}

/// Each problem the project gives, as `path rule: message`, but for
/// unused imports, declarations and variables, migrations and records,
/// which these projects don't keep, and a failure after a changing call
/// and a route's deadline, which [`late_problems`] keeps.
fn problems_of(events: bool, keep: &str, more: &[(&str, &str)]) -> Vec<String> {
    all_problems(events, keep, more)
        .into_iter()
        .filter(|problem| {
            !problem.contains(" step.late_failure:")
                && !problem.contains(" route.calls_past_deadline:")
        })
        .collect()
}

/// [`problems_of`] with `events: { store: }`, a failure after a changing
/// call and a route's deadline kept (chunk 7.5e).
fn late_problems(keep: &str, more: &[(&str, &str)]) -> Vec<String> {
    all_problems(true, keep, more)
}

fn all_problems(events: bool, keep: &str, more: &[(&str, &str)]) -> Vec<String> {
    let files = files(events, keep, more);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            ![
                "import.unused",
                "layout.unused_declaration",
                "semantic.unused_variable",
            ]
            .iter()
            .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
                && !problem.contains(" record.")
        })
        .collect()
}

/// [`problems_of`], with `events: { store: }` naming the database, so
/// the record has a store whatever `keep` writes.
fn problems(keep: &str, more: &[(&str, &str)]) -> Vec<String> {
    problems_of(true, keep, more)
}

/// The diagnostics the project gives under `rule`.
fn found(keep: &str, more: &[(&str, &str)], rule: &str) -> Vec<jadpo_diag::Diagnostic> {
    let files = files(true, keep, more);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .into_iter()
        .filter(|diagnostic| diagnostic.rule.id == rule)
        .collect()
}

/// The text `label` points at in `text`.
fn pointed(text: &str, label: &jadpo_diag::Label) -> String {
    text[label.span.start as usize..label.span.end as usize].to_string()
}

/// A keyed call of `operation` with `reference`, kept as `name`.
fn keyed(name: &str, operation: &str, reference: &str) -> String {
    format!(
        "    var {name}: AcmePay.PayReply = attempt pay.{operation}(AcmePay.PayRequest {{\n        reference: {reference}\n    }}) {{\n        card_declined: PaymentRefused\n    }}\n"
    )
}

/// `keep(plan)`: the customer, the card, then the subscription, then the
/// account that keeps it.
fn subscribe() -> String {
    format!(
        "/// Starts a subscription.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}{}{}    var account: Account = Account.create {{\n        set: {{\n            subscription: subscription.id\n        }}\n    }}\n    assert account.subscription == subscription.id\n}}\n",
        keyed("customer", "customers.create", "plan"),
        keyed("card", "cards.attach", "customer.id"),
        keyed("subscription", "subscriptions.create", "card.id"),
    )
}

/// A subscriber running `keep(event.plan)`.
const SUBSCRIBER: (&str, &str) = (
    "domains/billing/subscribers/start_subscription.jadpo",
    "use billing/actions { keep }\nuse billing/events { PlanChosen }\n\n/// Starts the subscription a customer chose.\nsubscriber start_subscription {\n    on: PlanChosen\n    run: keep(event.plan)\n}\n",
);

/// A route of the website's API, which doesn't require `Idempotency-Key`,
/// running `keep(input.plan)`.
const WEBSITE: (&str, &str) = (
    "domains/billing/routes/plans.jadpo",
    "use billing/actions { keep }\nuse billing/types { PlanChoice }\n\n/// What the website calls.\nroutes Plans {\n    prefix: /plans\n\n    /// Starts a subscription straight away.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: PlanChoice\n        status: SuccessStatus.no_content\n        run: keep(input.plan)\n    }\n}\n",
);

/// Root policy letting anyone call `keep`, for a route running a `keep`
/// that reaches no entity.
const POLICY: (&str, &str) = (
    "policy.jadpo",
    "use billing\n\npolicy {\n    actions: {\n        billing.keep: [Access.public]\n    }\n}\n",
);

/// The same route in the partners' API, which requires it.
const PARTNERS: (&str, &str) = (
    "domains/billing/routes/partners_plans.jadpo",
    "use billing/actions { keep }\nuse billing/types { PlanChoice }\n\n/// What a partner's systems call.\nroutes PartnersPlans {\n    api: partners\n    prefix: /partners/plans\n\n    /// Starts a subscription straight away.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: PlanChoice\n        status: SuccessStatus.no_content\n        run: keep(input.plan)\n    }\n}\n",
);

/// A schedule running `renew()`, an action of no arguments.
const NIGHTLY: (&str, &str) = (
    "domains/billing/schedules/nightly.jadpo",
    "use billing/actions { renew }\n\n/// Renews the night's subscriptions.\nschedule nightly {\n    every: 1d\n    run: renew()\n}\n",
);

/// `renew()`, making two keyed changes.
fn renew() -> (&'static str, String) {
    (
        "domains/billing/actions/renew.jadpo",
        format!(
            "use app {{ pay }}\nuse billing/failures {{ PaymentRefused }}\nuse shared/providers/acme_pay\n\n/// Renews the night's subscriptions.\naction renew() fails PaymentRefused -> Unit {{\n{}{}    assert renewed.id == card.id\n}}\n",
            keyed("card", "cards.attach", "\"night\""),
            keyed("renewed", "subscriptions.create", "card.id"),
        ),
    )
}

// --- where several changing calls may be made ----------------------------------

#[test]
fn several_keyed_changing_calls_from_a_subscriber_check() {
    assert_eq!(problems(&subscribe(), &[SUBSCRIBER]), Vec::<String>::new());
}

#[test]
fn several_from_a_route_whose_api_requires_the_key_check() {
    assert_eq!(problems(&subscribe(), &[PARTNERS]), Vec::<String>::new());
}

#[test]
fn several_from_a_route_whose_api_doesnt_require_the_key_are_refused_at_the_route() {
    assert_eq!(
        problems(&subscribe(), &[WEBSITE]),
        [
            "domains/billing/routes/plans.jadpo step.context: `POST /plans` runs `keep`, which makes 3 changing calls, but API `main` doesn't require `Idempotency-Key`"
        ]
    );
    let [diagnostic] = found(&subscribe(), &[WEBSITE], "step.context")
        .try_into()
        .expect("one");
    let primary = diagnostic.primary.as_ref().expect("at the route");
    assert_eq!(pointed(WEBSITE.1, primary), "POST /");
    assert_eq!(primary.text, "a retry can't resume them");
    assert_eq!(
        diagnostic.next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Require the key with `idempotency: Idempotency.required` on API `main`, or answer `202` and make the calls in a subscriber"
        )
    );
    assert!(!diagnostic.rule.declinable, "a plain mistake");
}

#[test]
fn several_from_a_schedule_are_refused_at_the_schedule() {
    let (path, text) = renew();
    assert_eq!(
        problems(&subscribe(), &[NIGHTLY, (path, &text)]),
        [
            "domains/billing/schedules/nightly.jadpo step.context: `nightly` runs `renew`, which makes 2 changing calls, but a schedule's retry is its next run"
        ]
    );
}

#[test]
fn several_from_a_runs_item_are_refused_at_the_run() {
    let keep = format!(
        "/// Renews one account.\naction keep(account: Account) fails PaymentRefused -> Unit {{\n{}{}    assert renewed.id == account.subscription\n}}\n",
        keyed("card", "cards.attach", "account.subscription"),
        keyed("renewed", "subscriptions.create", "card.id"),
    );
    let each = "use billing/actions { keep }\nuse billing/queries { accounts }\nuse billing/types { AccountPage }\n\n/// Renews each account.\naction renew_each(page: AccountPage) -> BatchRun {\n    return batches.each(from: accounts(page), each: account -> keep(account))\n}\n";
    assert_eq!(
        problems(&keep, &[("domains/billing/actions/renew_each.jadpo", each)]),
        [
            "domains/billing/actions/renew_each.jadpo step.context: Each item of `batches.each` runs `keep`, which makes 2 changing calls, more than a run's item may"
        ]
    );
    // A run's `finished:` the same.
    let finished = "use billing/actions { count, keep_last }\nuse billing/queries { accounts }\nuse billing/types { AccountPage }\n\n/// Counts each account, then renews the last.\naction renew_each(page: AccountPage) -> BatchRun {\n    return batches.each(from: accounts(page), each: account -> count(account), finished: summary -> keep_last(summary))\n}\n";
    let count = "use billing/entities { Account }\n\n/// Counts an account.\naction count(account: Account) -> Unit {\n    assert account.id == account.id\n}\n";
    let last = format!(
        "use app {{ pay }}\nuse billing/failures {{ PaymentRefused }}\nuse shared/providers/acme_pay\n\n/// Renews the last account.\naction keep_last(summary: ImportSummary) fails PaymentRefused -> Unit {{\n{}{}    assert summary.items == summary.items\n    assert renewed.id == card.id\n}}\n",
        keyed("card", "cards.attach", "\"last\""),
        keyed("renewed", "subscriptions.create", "card.id"),
    );
    assert_eq!(
        problems(
            &keep,
            &[
                ("domains/billing/actions/renew_each.jadpo", finished),
                ("domains/billing/actions/count.jadpo", count),
                ("domains/billing/actions/keep_last.jadpo", &last),
            ]
        ),
        [
            "domains/billing/actions/renew_each.jadpo step.context: The end of `batches.each` runs `keep_last`, which makes 2 changing calls, more than a run's end may"
        ]
    );
}

#[test]
fn one_changing_call_on_each_way_checks_anywhere() {
    for keep in [
        // One keyed change, and reads around it.
        format!(
            "/// One change.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    var invoice: AcmePay.PayReply = pay.invoices.get(AcmePay.PayRequest {{\n        reference: plan\n    }})\n{}    var after: AcmePay.PayReply = pay.invoices.get(AcmePay.PayRequest {{\n        reference: customer.id\n    }})\n    assert after.id == invoice.id\n}}\n",
            keyed("customer", "customers.create", "plan"),
        ),
        // One on each way through an `if`.
        format!(
            "/// One change either way.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    if (plan == \"early\") {{\n{}        assert customer.id == plan\n    }} else {{\n{}        assert card.id == plan\n    }}\n}}\n",
            keyed("customer", "customers.create", "plan")
                .replace("\n    ", "\n        ")
                .replacen("    var", "        var", 1),
            keyed("card", "cards.attach", "plan")
                .replace("\n    ", "\n        ")
                .replacen("    var", "        var", 1),
        ),
        // A way that ends with its call, before another.
        format!(
            "/// One change, then stop; or another.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    if (plan == \"early\") {{\n{}        assert customer.id == plan\n        return\n    }}\n{}    assert card.id == plan\n}}\n",
            keyed("customer", "customers.create", "plan")
                .replace("\n    ", "\n        ")
                .replacen("    var", "        var", 1),
            keyed("card", "cards.attach", "plan"),
        ),
    ] {
        assert_eq!(
            problems(&keep, &[WEBSITE, POLICY]),
            Vec::<String>::new(),
            "{keep}"
        );
    }
}

#[test]
fn an_actions_calls_count_as_its_callers() {
    // One change through `open_customer`, then one of its own.
    let keep = format!(
        "/// Opens the customer, then the subscription.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    var customer: AcmePay.PayReply = attempt open_customer(plan)\n{}    assert subscription.id == customer.id\n}}\n",
        keyed("subscription", "subscriptions.create", "customer.id"),
    );
    assert_eq!(
        problems(&keep, &[WEBSITE, POLICY]),
        [
            "domains/billing/routes/plans.jadpo step.context: `POST /plans` runs `keep`, which makes 2 changing calls, but API `main` doesn't require `Idempotency-Key`"
        ]
    );
}

// --- how many ---------------------------------------------------------------

/// `count` keyed changes in a row, each from the last.
fn in_a_row(count: usize) -> String {
    let mut out = keyed("call_1", "cards.attach", "plan");
    for at in 2..=count {
        out.push_str(&keyed(
            &format!("call_{at}"),
            "cards.attach",
            &format!("call_{}.id", at - 1),
        ));
    }
    out
}

#[test]
fn eight_changing_calls_in_a_row_check() {
    let keep = format!(
        "/// Eight changes.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}    assert call_8.id == plan\n}}\n",
        in_a_row(8)
    );
    assert_eq!(problems(&keep, &[SUBSCRIBER]), Vec::<String>::new());
}

#[test]
fn a_ninth_changing_call_in_a_row_is_refused_at_the_ninth() {
    let keep = format!(
        "/// Nine changes.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}    assert call_9.id == plan\n}}\n",
        in_a_row(9)
    );
    assert_eq!(
        problems(&keep, &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.too_many: `keep` makes 9 changing calls in a row, more than 8"
        )]
    );
    let [diagnostic] = found(&keep, &[SUBSCRIBER], "step.too_many")
        .try_into()
        .expect("one");
    let primary = diagnostic.primary.as_ref().expect("at the ninth");
    let text = format!("{IMPORTS}{keep}");
    assert_eq!(pointed(&text, primary), "pay.cards.attach");
    assert_eq!(primary.text, "the 9th changing call");
    let ninth = text.find("var call_9").expect("ninth");
    assert!(primary.span.start as usize > ninth, "{primary:?}");
}

#[test]
fn calls_on_separate_ways_count_apart_and_through_actions_together() {
    // Five on each way through an `if`: five at most.
    let indent = |text: String| {
        text.lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>()
    };
    let keep = format!(
        "/// Five either way.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    if (plan == \"early\") {{\n{}        assert call_5.id == plan\n    }} else {{\n{}        assert call_5.id == plan\n    }}\n}}\n",
        indent(in_a_row(5)),
        indent(in_a_row(5)),
    );
    assert_eq!(
        problems(&keep, &[SUBSCRIBER]),
        Vec::<String>::new(),
        "{keep}"
    );
    // Eight of its own, and one through `open_customer` after them.
    let keep = format!(
        "/// Eight changes, then the customer.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}    var customer: AcmePay.PayReply = attempt open_customer(call_8.id)\n    assert customer.id == plan\n}}\n",
        in_a_row(8)
    );
    assert_eq!(
        problems(&keep, &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.too_many: `keep` makes 9 changing calls in a row, more than 8"
        )]
    );
    let [diagnostic] = found(&keep, &[SUBSCRIBER], "step.too_many")
        .try_into()
        .expect("one");
    let primary = diagnostic.primary.as_ref().expect("at the call");
    assert_eq!(
        pointed(&format!("{IMPORTS}{keep}"), primary),
        "open_customer"
    );
}

// --- which ------------------------------------------------------------------

#[test]
fn a_changing_call_with_no_key_beside_another_is_refused_at_it() {
    let receipt = "    var receipt: AcmePay.PayReply = pay.receipts.send(AcmePay.PayRequest {\n        reference: plan\n    })\n";
    // On its own: one changing call, kept as before.
    let alone = format!(
        "/// The receipt.\naction keep(plan: AcmePay.Reference) -> Unit {{\n{receipt}    assert receipt.id == plan\n}}\n"
    );
    assert_eq!(problems(&alone, &[SUBSCRIBER]), Vec::<String>::new());
    // Beside a keyed change, on the same way or another.
    for keep in [
        format!(
            "/// The customer, then the receipt.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}{receipt}    assert receipt.id == customer.id\n}}\n",
            keyed("customer", "customers.create", "plan")
        ),
        format!(
            "/// The receipt, or the customer.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    if (plan == \"early\") {{\n    {}        assert receipt.id == plan\n        return\n    }}\n{}    assert customer.id == plan\n}}\n",
            receipt.replace("\n    ", "\n        "),
            keyed("customer", "customers.create", "plan"),
        ),
    ] {
        assert_eq!(
            problems(&keep, &[SUBSCRIBER]),
            [format!(
                "{KEEP} step.unkeyed: `pay.receipts.send` takes no idempotency key, but `keep` makes another changing call, `pay.customers.create`"
            )],
            "{keep}"
        );
        let [diagnostic] = found(&keep, &[SUBSCRIBER], "step.unkeyed")
            .try_into()
            .expect("one");
        let primary = diagnostic.primary.as_ref().expect("at the call");
        assert_eq!(
            pointed(&format!("{IMPORTS}{keep}"), primary),
            "pay.receipts.send"
        );
        assert_eq!(primary.text, "no key");
    }
}

#[test]
fn a_large_answer_among_several_changing_calls_is_refused() {
    let statement = "    var statement: AcmePay.PayReply = pay.statements.export(AcmePay.PayRequest {\n        reference: plan\n    })\n";
    let alone = format!(
        "/// The statement.\naction keep(plan: AcmePay.Reference) -> Unit {{\n{statement}    assert statement.id == plan\n}}\n"
    );
    assert_eq!(problems(&alone, &[SUBSCRIBER]), Vec::<String>::new());
    let keep = format!(
        "/// The customer, then the statement.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}{statement}    assert statement.id == customer.id\n}}\n",
        keyed("customer", "customers.create", "plan")
    );
    assert_eq!(
        problems(&keep, &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.large_output: `pay.statements.export` gives a large answer, and `keep` makes several changing calls"
        )]
    );
}

// --- where the record is kept ----------------------------------------------------

#[test]
fn several_changing_calls_need_a_store_for_their_record() {
    let keep = format!(
        "/// The customer and the card, keeping nothing.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}{}    assert card.id == customer.id\n}}\n",
        keyed("customer", "customers.create", "plan"),
        keyed("card", "cards.attach", "customer.id"),
    );
    assert_eq!(
        problems_of(false, &keep, &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.store_required: `keep` makes 2 changing calls but writes no store to keep their record in"
        )]
    );
    // `events: { store: }` names one.
    assert_eq!(
        problems_of(true, &keep, &[SUBSCRIBER]),
        Vec::<String>::new()
    );
}

// --- loops ------------------------------------------------------------------

#[test]
fn an_outside_call_inside_for_is_refused_however_it_is_made() {
    let calls = [
        (
            "        var receipt: AcmePay.PayReply = pay.receipts.send(AcmePay.PayRequest {\n            reference: plan\n        })\n        assert receipt.id == plan\n",
            "`pay.receipts.send` is called once for every item of the loop",
            "pay.receipts.send",
        ),
        (
            "        var sheet: File = attempt sheets.put(encoding.to_utf8(plan), FileType.csv) {\n            storage_full: NoRoom\n        }\n        assert sheet == sheet\n",
            "`sheets.put` is called once for every item of the loop",
            "sheets.put",
        ),
        (
            "        var customer: AcmePay.PayReply = attempt open_customer(plan)\n        assert customer.id == plan\n",
            "`open_customer` calls `pay.customers.create` once for every item of the loop",
            "open_customer",
        ),
    ];
    for (call, message, at) in calls {
        let keep = format!(
            "/// A call for each plan.\naction keep(plans: List<AcmePay.Reference>) fails PaymentRefused, NoRoom -> Unit {{\n    for plan in plans {{\n{call}    }}\n}}\n"
        );
        let found_here: Vec<String> = problems(&keep, &[])
            .into_iter()
            .filter(|problem| !problem.contains(" failure.stale_declaration:"))
            .collect();
        assert_eq!(
            found_here,
            [format!("{KEEP} effect.call_in_loop: {message}")],
            "{keep}"
        );
        let [diagnostic] = found(&keep, &[], "effect.call_in_loop")
            .try_into()
            .expect("one");
        let primary = diagnostic.primary.as_ref().expect("at the call");
        assert_eq!(pointed(&format!("{IMPORTS}{keep}"), primary), at);
        assert_eq!(primary.text, "inside `for`");
        assert_eq!(
            diagnostic.next.as_ref().map(|next| next.text.as_str()),
            Some("Make the calls one item at a time with `batches.each`")
        );
    }
    // A loop with no outside call in it checks.
    let keep = "/// Counts the plans.\naction keep(plans: List<AcmePay.Reference>) -> Unit {\n    for plan in plans {\n        assert plan == plan\n    }\n}\n";
    assert_eq!(problems(keep, &[]), Vec::<String>::new());
}

#[test]
fn an_action_over_the_limit_is_reported_in_it_not_where_it_is_called() {
    let nine = format!(
        "use app {{ pay }}\nuse billing/failures {{ PaymentRefused }}\nuse shared/providers/acme_pay\n\n/// Nine changes.\naction nine(plan: AcmePay.Reference) fails PaymentRefused -> AcmePay.PayReply {{\n{}    return call_9\n}}\n",
        in_a_row(9)
    );
    let keep = "/// Nine changes through `nine`, then one more.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {\n    var reply: AcmePay.PayReply = attempt nine(plan)\n    var customer: AcmePay.PayReply = attempt open_customer(reply.id)\n    assert customer.id == plan\n}\n";
    let imports = IMPORTS.replace("{ open_customer }", "{ nine, open_customer }");
    let files: Vec<(String, String)> = files(
        true,
        keep,
        &[("domains/billing/actions/nine.jadpo", &nine), SUBSCRIBER],
    )
    .into_iter()
    .map(|(path, text)| {
        if path == KEEP {
            (path, format!("{imports}{keep}"))
        } else {
            (path, text)
        }
    })
    .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| problem.contains(" step.") && !problem.contains(" step.late_failure:"))
        .collect();
    assert_eq!(
        found,
        [
            "domains/billing/actions/nine.jadpo step.too_many: `nine` makes 9 changing calls in a row, more than 8"
        ]
    );
}

#[test]
fn a_call_with_no_key_through_an_action_is_named_with_it() {
    let receipt = "use app { pay }\nuse shared/providers/acme_pay\n\n/// Sends a receipt.\naction send_receipt(plan: AcmePay.Reference) -> AcmePay.PayReply {\n    return pay.receipts.send(AcmePay.PayRequest {\n        reference: plan\n    })\n}\n";
    let keep = format!(
        "/// The receipt, then the customer.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    var receipt: AcmePay.PayReply = send_receipt(plan)\n{}    assert customer.id == receipt.id\n}}\n",
        keyed("customer", "customers.create", "plan")
    );
    let imports = IMPORTS.replace("{ open_customer }", "{ open_customer, send_receipt }");
    let files: Vec<(String, String)> = files(
        true,
        &keep,
        &[
            ("domains/billing/actions/send_receipt.jadpo", receipt),
            SUBSCRIBER,
        ],
    )
    .into_iter()
    .map(|(path, text)| {
        if path == KEEP {
            (path, format!("{imports}{keep}"))
        } else {
            (path, text)
        }
    })
    .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| problem.contains(" step.") && !problem.contains(" step.late_failure:"))
        .collect();
    assert_eq!(
        found,
        [format!(
            "{KEEP} step.unkeyed: `send_receipt` calls `pay.receipts.send`, which takes no idempotency key, but `keep` makes another changing call, `pay.customers.create`"
        )]
    );
}

#[test]
fn a_read_inside_for_is_an_outside_call_too() {
    let keep = "/// Looks each plan up.\naction keep(plans: List<AcmePay.Reference>) -> Unit {\n    for plan in plans {\n        var invoice: AcmePay.PayReply = pay.invoices.get(AcmePay.PayRequest {\n            reference: plan\n        })\n        assert invoice.id == plan\n    }\n}\n";
    assert_eq!(
        problems(keep, &[]),
        [format!(
            "{KEEP} effect.call_in_loop: `pay.invoices.get` is called once for every item of the loop"
        )]
    );
}

// --- the record (chunk 7.5b) -----------------------------------------------------

/// The project's problems with `steps` in place of `app.jadpo`'s `steps:`
/// block.
fn problems_with_steps(steps: &str, keep: &str, more: &[(&str, &str)]) -> Vec<String> {
    let mut files = files(true, keep, more);
    files[0].1 = app_with(true, steps);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            ![
                "import.unused",
                "layout.unused_declaration",
                "semantic.unused_variable",
                "step.late_failure",
                "route.calls_past_deadline",
            ]
            .iter()
            .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
                && !problem.contains(" record.")
        })
        .collect()
}

/// The answers of an action with several changing calls are kept
/// encrypted, so `app.jadpo` names the key: one error, at the first such
/// action's first changing call (area 12's decisions, 2026-10-20).
#[test]
fn several_changing_calls_need_a_key_to_encrypt_their_answers() {
    let (path, text) = renew();
    assert_eq!(
        problems_with_steps("", &subscribe(), &[SUBSCRIBER, (path, &text)]),
        [
            "domains/billing/actions/keep.jadpo step.encryption_key_required: `keep` makes 3 changing calls, but `app.jadpo` names no key to encrypt their answers with"
        ]
    );
    // One changing call on each way keeps no answers.
    assert_eq!(
        problems_with_steps(
            "",
            "/// Opens the customer.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {\n    var opened: AcmePay.PayReply = attempt open_customer(plan)\n    assert opened.id == plan\n}\n",
            &[SUBSCRIBER],
        ),
        Vec::<String>::new()
    );
}

/// `steps:` holds one setting, `encryption_key:`, reading a `SigningKey`
/// setting.
#[test]
fn the_steps_key_is_a_signing_key_setting() {
    let steps = |inner: &str| format!("    steps: {{\n        {inner}\n    }}\n");
    let cases = [
        (
            steps("encryption_key: config.pay_api_key"),
            "app.jadpo step.settings_invalid: `encryption_key:` reads `pay_api_key`, which isn't a `SigningKey`",
        ),
        (
            steps("encryption_key: \"kept in the source\""),
            "app.jadpo step.settings_invalid: An encryption key is written in the source",
        ),
        (
            steps("key: config.step_encryption_key"),
            "app.jadpo step.settings_invalid: `key` isn't a steps setting",
        ),
    ];
    for (block, expected) in cases {
        let found = problems_with_steps(&block, &subscribe(), &[SUBSCRIBER]);
        assert!(found.contains(&expected.to_string()), "{block}: {found:?}");
    }
}

/// An action a subscriber runs making several changing calls has its
/// record planned in the store it writes, and the store keeps the
/// record's tables; a single keyed change instead plans dispatch metadata.
#[test]
fn the_record_is_planned_in_the_store_its_action_writes() {
    let files = files(true, &subscribe(), &[SUBSCRIBER]);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let plans = jadpo_check::types::record_plans(&db, project);
    let planned: Vec<(&str, &str, usize)> = plans
        .iter()
        .map(|(key, plan)| (key.name(), plan.store.as_str(), plan.shape.len()))
        .collect();
    assert_eq!(planned, [("keep", "main", 64)]);
    let schema = jadpo_check::data::schema(&db, project);
    let tables = &schema.stores.get("main").expect("main").tables;
    assert!(
        tables.contains_key("jadpo_step_records"),
        "{:?}",
        tables.keys()
    );
    assert!(tables.contains_key("jadpo_steps"));
    // The shape follows the calls: another order is another shape.
    let swapped = subscribe()
        .replace("customers.create", "TEMP")
        .replace("cards.attach", "customers.create")
        .replace("TEMP", "cards.attach");
    let files = self::files(true, &swapped, &[SUBSCRIBER]);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let other = Database::default();
    let project = support::project(&other, Layout::Project, &files);
    let shape = &jadpo_check::types::record_plans(&other, project)
        .values()
        .next()
        .expect("planned")
        .shape;
    assert_ne!(shape, &plans.values().next().expect("planned").shape);
}

// --- ending for good after a changing call (chunk 7.5e) ------------------------

/// After the first changing call, a point that can end the action for good
/// is reported once for the action, at the first, the others beside it:
/// the provider keeps what it did while the action's writes roll back.
#[test]
fn a_failure_after_a_changing_call_is_reported_once_at_the_first() {
    assert_eq!(
        late_problems(&subscribe(), &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.late_failure: `keep` can fail for good at `pay.cards.attach`, after `pay.customers.create` took effect"
        )]
    );
    let [diagnostic] = found(&subscribe(), &[SUBSCRIBER], "step.late_failure")
        .try_into()
        .expect("one");
    let text = format!("{IMPORTS}{}", subscribe());
    let primary = diagnostic.primary.as_ref().expect("at the point");
    assert_eq!(pointed(&text, primary), "pay.cards.attach");
    assert_eq!(primary.text, "can fail with `PaymentRefused`");
    let also: Vec<(String, &str)> = diagnostic
        .secondary
        .iter()
        .map(|label| (pointed(&text, label), label.text.as_str()))
        .collect();
    assert_eq!(
        also,
        [
            ("pay.customers.create".to_string(), "takes effect first"),
            (
                "pay.subscriptions.create".to_string(),
                "can fail for good too"
            ),
        ]
    );
    assert!(diagnostic.rule.declinable, "a judgement call");
    assert_eq!(
        diagnostic.next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Check the condition before `pay.customers.create`, record what happened and finish in a subscriber of an event `keep` emits, or undo `pay.customers.create` as another step in a `match` arm, or write `decline step.late_failure: \"<why>\"` above it if this is deliberate"
        )
    );
}

#[test]
fn a_decline_answers_it() {
    let declined = subscribe().replace(
        "action keep(",
        "decline step.late_failure: \"Acme Pay declines a card at the first call, never later\"\naction keep(",
    );
    assert_eq!(
        late_problems(&declined, &[SUBSCRIBER]),
        Vec::<String>::new()
    );
}

#[test]
fn the_first_changing_calls_own_failure_and_a_retried_one_after_it_arent_reported() {
    let keep = format!(
        "/// Opens the customer, then keeps a sheet.\naction keep(plan: AcmePay.Reference) fails PaymentRefused, NoRoom -> Unit {{\n{}    var sheet: File = attempt sheets.put(encoding.to_utf8(customer.id), FileType.csv) {{\n        storage_full: NoRoom\n    }}\n    assert sheet == sheet\n}}\n",
        keyed("customer", "customers.create", "plan"),
    );
    assert_eq!(late_problems(&keep, &[SUBSCRIBER]), Vec::<String>::new());
}

#[test]
fn a_reject_a_refusable_write_and_a_called_actions_failure_are_each_points() {
    let reject = format!(
        "/// Opens the customer, then may refuse.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}    if (customer.id == plan) {{\n        reject PaymentRefused\n    }}\n}}\n",
        keyed("customer", "customers.create", "plan"),
    );
    assert_eq!(
        late_problems(&reject, &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.late_failure: `keep` can fail for good at `reject PaymentRefused`, after `pay.customers.create` took effect"
        )]
    );
    let write = "/// Opens the customer, then keeps the plan on the account.\naction keep(plan: AcmePay.Reference, account_id: Account.id) fails PaymentRefused, AccountNotFound -> Unit {\n    var customer: AcmePay.PayReply = attempt pay.customers.create(AcmePay.PayRequest {\n        reference: plan\n    }) {\n        card_declined: PaymentRefused\n    }\n    var account: Account = attempt Account.update(account_id) {\n        set: {\n            subscription: plan\n        }\n    }\n    assert account.subscription == customer.id\n}\n";
    let files: Vec<(String, String)> = files(true, write, &[])
        .into_iter()
        .map(|(path, text)| {
            let text = if path == KEEP {
                text.replace("{ Account }", "{ Account, AccountNotFound }")
            } else {
                text
            };
            (path, text)
        })
        .collect();
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| problem.contains(" step."))
        .collect();
    assert_eq!(
        found,
        [format!(
            "{KEEP} step.late_failure: `keep` can fail for good at `Account.update`, after `pay.customers.create` took effect"
        )]
    );
    let called = format!(
        "/// Opens the card, then the customer.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}    var customer: AcmePay.PayReply = attempt open_customer(card.id)\n    assert customer.id == card.id\n}}\n",
        keyed("card", "cards.attach", "plan"),
    );
    assert_eq!(
        late_problems(&called, &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.late_failure: `keep` can fail for good at `open_customer`, after `pay.cards.attach` took effect"
        )]
    );
}

#[test]
fn a_failure_handled_with_match_isnt_reported_but_a_reject_in_its_arm_is() {
    let handled = |arm: &str| {
        format!(
            "/// Opens the card, then the customer if it can.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}    var opened: AcmePay.Reference = match open_customer(card.id) {{\n        success(customer) -> customer.id\n        PaymentRefused -> {arm}\n    }}\n    assert opened == card.id\n}}\n",
            keyed("card", "cards.attach", "plan"),
        )
    };
    assert_eq!(
        late_problems(&handled("card.id"), &[SUBSCRIBER]),
        Vec::<String>::new()
    );
    assert_eq!(
        late_problems(&handled("reject PaymentRefused"), &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.late_failure: `keep` can fail for good at `reject PaymentRefused`, after `pay.cards.attach` took effect"
        )]
    );
}

/// A failure on another way than the changing call's comes before it, so
/// it isn't reported; where two ways meet, a changing call on either may
/// have been made.
#[test]
fn only_a_way_that_made_a_changing_call_counts() {
    let before = format!(
        "/// Refuses first, then opens the customer.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    if (plan == \"closed\") {{\n        reject PaymentRefused\n    }}\n{}    assert customer.id == plan\n}}\n",
        keyed("customer", "customers.create", "plan"),
    );
    assert_eq!(late_problems(&before, &[SUBSCRIBER]), Vec::<String>::new());
    let either = format!(
        "/// Opens the customer on one way, then may refuse.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    if (plan == \"new\") {{\n{}        assert customer.id == plan\n    }}\n    if (plan == \"closed\") {{\n        reject PaymentRefused\n    }}\n}}\n",
        keyed("customer", "customers.create", "plan")
            .replace("\n    ", "\n        ")
            .replacen("    var", "        var", 1),
    );
    assert_eq!(
        late_problems(&either, &[SUBSCRIBER]),
        [format!(
            "{KEEP} step.late_failure: `keep` can fail for good at `reject PaymentRefused`, after `pay.customers.create` took effect"
        )]
    );
}

// --- a route's deadline (chunk 7.5e) -------------------------------------------

/// A route whose API requires the key, running `keep`.
fn partners_route(deadline: &str) -> String {
    PARTNERS.1.replace(
        "        run: keep(input.plan)\n",
        &format!("{deadline}        run: keep(input.plan)\n"),
    )
}

/// The project's files, the connection's `timeout:` and attempts set by
/// `settings`, written as its members are.
fn timed_files(settings: &str, keep: &str, more: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut files = files(true, keep, more);
    files[0].1 = files[0].1.replace(
        "            environment: config.pay_environment\n",
        &format!("            environment: config.pay_environment\n{settings}"),
    );
    files
}

/// The deadline diagnostics `files` give.
fn past_deadline(files: &[(String, String)]) -> Vec<jadpo_diag::Diagnostic> {
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .into_iter()
        .filter(|diagnostic| diagnostic.rule.id == "route.calls_past_deadline")
        .collect()
}

/// Each outside call can wait its `timeout:` on each of its attempts (3s
/// and 3 by default, Richard 2026-10-20), and a route's deadline covers
/// every call on the longest way through its action.
#[test]
fn a_routes_calls_are_summed_against_its_deadline() {
    let declined = subscribe().replace(
        "action keep(",
        "decline step.late_failure: \"Acme Pay declines a card at the first call, never later\"\naction keep(",
    );
    // Three calls of 3s, each tried 3 times: 27s, past the 10s default.
    assert_eq!(
        late_problems(&declined, &[PARTNERS]),
        [
            "domains/billing/routes/partners_plans.jadpo route.calls_past_deadline: `POST /partners/plans` runs `keep`, whose outside calls can take 27s, longer than its 10s deadline"
        ]
    );
    let [diagnostic] = found(&declined, &[PARTNERS], "route.calls_past_deadline")
        .try_into()
        .expect("one");
    let primary = diagnostic.primary.as_ref().expect("at the route");
    assert_eq!(pointed(PARTNERS.1, primary), "POST /");
    assert_eq!(primary.text, "deadline 10s");
    assert!(!diagnostic.rule.declinable, "a plain mistake");
    assert_eq!(
        diagnostic.next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Give the route `deadline: 27s`, lower the `timeout:` or the attempts of the calls (a connection's `timeout:` and `retry: { attempts: }`, an `ai` call's `timeout:` and `attempts:`), or answer `202` and make the calls in a subscriber"
        )
    );
    // A deadline that leaves room for them.
    let room = partners_route("        deadline: 30s\n");
    assert_eq!(
        late_problems(&declined, &[(PARTNERS.0, &room)]),
        Vec::<String>::new()
    );
    // Calls of 10s: 90s, past the most a route may wait.
    let [diagnostic] = past_deadline(&timed_files(
        "            timeout: 10s\n",
        &declined,
        &[PARTNERS],
    ))
    .try_into()
    .expect("one");
    assert_eq!(
        diagnostic.message,
        "`POST /partners/plans` runs `keep`, whose outside calls can take 90s, longer than its 10s deadline"
    );
    assert_eq!(
        diagnostic.next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Lower the `timeout:` or the attempts of the calls (a connection's `timeout:` and `retry: { attempts: }`, an `ai` call's `timeout:` and `attempts:`), or answer `202` and make the calls in a subscriber, since a route's deadline is at most 60s"
        )
    );
    // One call, through an action, fits the default deadline: 9s.
    let one = "/// Opens the customer.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {\n    var customer: AcmePay.PayReply = attempt open_customer(plan)\n    assert customer.id == plan\n}\n";
    let route = partners_route("");
    assert_eq!(
        late_problems(one, &[(PARTNERS.0, &route), POLICY]),
        Vec::<String>::new()
    );
}

/// Only the longest way counts, and a connection's `timeout:` and
/// `retry:` set each call's share.
#[test]
fn the_longest_way_counts_with_its_connections_settings() {
    let either = format!(
        "/// Opens the customer, or the card.\naction keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n    if (plan == \"card\") {{\n{}        assert card.id == plan\n        return\n    }}\n{}    assert customer.id == plan\n}}\n",
        keyed("card", "cards.attach", "plan")
            .replace("\n    ", "\n        ")
            .replacen("    var", "        var", 1),
        keyed("customer", "customers.create", "plan"),
    );
    // Each way makes one call of 9s; both together would be 18s.
    assert_eq!(
        late_problems(&either, &[PARTNERS, POLICY]),
        Vec::<String>::new()
    );
    // 4s, three times, is 12s: past the default 10s.
    let messages = |settings: &str| -> Vec<String> {
        past_deadline(&timed_files(settings, &either, &[PARTNERS, POLICY]))
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    };
    assert_eq!(
        messages("            timeout: 4s\n"),
        [
            "`POST /partners/plans` runs `keep`, whose outside calls can take 12s, longer than its 10s deadline"
        ]
    );
    // 4s, twice, is 8s: within it.
    assert_eq!(
        messages(
            "            timeout: 4s\n            retry: {\n                attempts: 2\n            }\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn single_keyed_calls_plan_only_dispatch_metadata_and_add_no_configuration_requirement() {
    let keep = format!(
        "action keep(plan: AcmePay.Reference) fails PaymentRefused -> Unit {{\n{}    assert customer.id != \"\"\n}}\n",
        keyed("customer", "customers.create", "plan")
    );
    for events in [true, false] {
        let mut source = files(events, &keep, &[PARTNERS, POLICY]);
        source[0].1 = app_with(events, "");
        let source: Vec<(&str, &str)> = source
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let db = Database::default();
        let project = support::project(&db, Layout::Project, &source);
        let problems = jadpo_check::check(&db, project);
        assert!(
            !problems.iter().any(|p| matches!(
                p.rule.id,
                "step.store_required" | "step.encryption_key_required"
            )),
            "{problems:?}"
        );
        let plans = jadpo_check::types::record_plans(&db, project);
        if events {
            let plan = plans.values().next().expect("dispatch metadata");
            assert_eq!(plan.store, "main");
            assert!(!plan.replay);
        } else {
            assert!(
                plans.is_empty(),
                "storeless routes keep their accepted behavior"
            );
        }
    }
}
