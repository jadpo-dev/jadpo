//! Polls and `max_concurrent:` (area 11, "Polls" and "Calling"; area 12,
//! "Events from outside"; chunk 7.6c): a provider package's `polls:`
//! names an operation that takes nothing and changes nothing, how often it
//! is called, and the package's own public event made from its answer; a
//! connection may call it more or less often with `polls: { <name>:
//! <interval> }`. `max_concurrent:` in `provider.jadpo` caps the calls at
//! once, and a connection may lower it.

use crate::support;

use jadpo_db::Layout;

const HOME: &str = "shared/providers/fuel_prices/v1";

const APP: &str = "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        fuel: {\n            package: FuelPrices.v1\n            domains: [pricing]\n            credential: config.fuel_key\n            environment: config.fuel_environment\n            max_concurrent: 2\n            polls: {\n                latest: 6h\n            }\n        }\n    }\n    events: {\n        store: main\n    }\n}\n";

const CONFIG: &str = "config {\n    fuel_key: Text {\n        secret: true\n    }\n    fuel_environment: ProviderEnvironment\n}\n";

const PROVIDER: &str = "/// Acme Fuel's price list, which sends no webhooks.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.fuel-prices.example\"\n        sandbox: \"https://sandbox.fuel-prices.example\"\n    }\n    credential: CredentialScheme.bearer\n    max_concurrent: 4\n    polls: {\n        latest: {\n            call: prices.latest\n            every: 1d\n            emits: PricesPublished\n        }\n    }\n}\n";

const PRICE_LIST: &str = "/// The day's prices, as the API gives them.\ntype PriceList = Object {\n    region: Text\n    diesel_pence: Int\n    petrol_pence: Int\n    fetched_at: Instant\n}\n";

const PRICES: &str = "resource prices {\n    latest: {\n        method: HttpMethod.get\n        path: \"/v1/prices/latest\"\n        output: PriceList\n        source: PackageSource.written\n    }\n}\n";

const PUBLISHED: &str = "/// The day's pump prices, published.\npublic type PricesPublished = Event {\n    region: Text\n    diesel_pence: Int\n    petrol_pence: Int\n}\n";

/// Acme Fuel's package, with `files` in place of its own.
fn package(files: &[(&str, Option<&str>)]) -> Vec<(String, String)> {
    let own: Vec<(&str, &str)> = vec![
        (
            "package.jadpo",
            "/// Acme Fuel's price list.\npackage FuelPrices.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        ("provider.jadpo", PROVIDER),
        ("types/price_list.jadpo", PRICE_LIST),
        ("resources/prices.jadpo", PRICES),
        ("events/prices_published.jadpo", PUBLISHED),
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

/// The app's side: the pump prices kept by `pricing` each time they
/// change.
const PRICING: &[(&str, &str)] = &[
    (
        "domains/pricing/entities/pump_price.jadpo",
        "entity PumpPrice {\n    id: Uuid\n    region: Text\n    diesel_pence: Int\n    petrol_pence: Int\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n",
    ),
    (
        "domains/pricing/actions/keep_prices.jadpo",
        "use pricing/entities { PumpPrice }\nuse shared/providers/fuel_prices\n\n/// Keeps the prices Acme Fuel published.\naction keep_prices(event: FuelPrices.PricesPublished) -> PumpPrice {\n    return PumpPrice.create {\n        set: {\n            region: event.region\n            diesel_pence: event.diesel_pence\n            petrol_pence: event.petrol_pence\n        }\n    }\n}\n",
    ),
    (
        "domains/pricing/subscribers/prices_published.jadpo",
        "use pricing/actions { keep_prices }\nuse shared/providers/fuel_prices\n\n/// Keeps each change of price.\nsubscriber prices_published {\n    on: FuelPrices.PricesPublished\n    run: keep_prices(event)\n}\n",
    ),
];

/// The files of the project with the package's `changes` and the app's
/// `more`.
fn files(changes: &[(&str, Option<&str>)], more: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut files = vec![
        ("app.jadpo".to_string(), APP.to_string()),
        ("config.jadpo".to_string(), CONFIG.to_string()),
    ];
    files.extend(package(changes));
    for (path, text) in PRICING.iter().chain(more) {
        files.retain(|(written, _)| written != path);
        files.push((path.to_string(), text.to_string()));
    }
    files
}

/// Each problem of the project with the package's `changes` and the app's
/// `more`, as `path rule: message`, but for unused imports and
/// declarations, migrations and records, which these projects don't keep.
fn problems(changes: &[(&str, Option<&str>)], more: &[(&str, &str)]) -> Vec<String> {
    let files = files(changes, more);
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

/// The provider with its `polls:` block replaced by `polls`.
fn polls(polls: &str) -> String {
    let start = PROVIDER.find("    polls: {").expect("polls");
    let end = PROVIDER.rfind("}\n").expect("end");
    format!("{}{polls}{}", &PROVIDER[..start], &PROVIDER[end..])
}

const PROVIDER_FILE: &str = "shared/providers/fuel_prices/v1/provider.jadpo";

fn provider_problem(message: &str) -> String {
    format!("{PROVIDER_FILE} package.provider_invalid: {message}")
}

#[test]
fn a_poll_of_a_feed_the_app_subscribes_to_reads_cleanly() {
    assert_eq!(problems(&[], &[]), Vec::<String>::new());
}

#[test]
fn a_poll_gives_what_it_calls_how_often_and_what_it_emits() {
    let missing = polls(
        "    polls: {\n        latest: {\n            call: prices.latest\n            emits: PricesPublished\n        }\n    }\n",
    );
    assert_eq!(
        problems(&[("provider.jadpo", Some(&missing))], &[]),
        [provider_problem("Poll `latest` doesn't give `every:`")]
    );
    let unknown = polls(
        "    polls: {\n        latest: {\n            call: prices.latest\n            every: 1d\n            emits: PricesPublished\n            input: {}\n        }\n    }\n",
    );
    assert_eq!(
        problems(&[("provider.jadpo", Some(&unknown))], &[]),
        [provider_problem("Poll `latest` has no key `input:`")]
    );
    let flat = polls("    polls: {\n        latest: prices.latest\n    }\n");
    assert_eq!(
        problems(&[("provider.jadpo", Some(&flat))], &[]),
        [provider_problem("Poll `latest` isn't a block")]
    );
}

#[test]
fn a_poll_calls_an_operation_of_its_package_that_takes_and_changes_nothing() {
    let misspelt = PROVIDER.replace("call: prices.latest", "call: prices.lastest");
    assert_eq!(
        problems(&[("provider.jadpo", Some(&misspelt))], &[]),
        [provider_problem(
            "Poll `latest` calls `prices.lastest`, which isn't one of the package's operations"
        )]
    );
    let region = "/// Which region's prices.\ntype RegionAsked = Object {\n    region: Text\n}\n";
    let taking = PRICES.replace(
        "        output: PriceList\n",
        "        input: RegionAsked\n        output: PriceList\n",
    );
    assert_eq!(
        problems(
            &[
                ("resources/prices.jadpo", Some(&taking)),
                ("types/region_asked.jadpo", Some(region)),
            ],
            &[]
        ),
        [provider_problem(
            "Poll `latest` calls `prices.latest`, which takes an input"
        )]
    );
    let changing = PRICES.replace(
        "        output: PriceList\n",
        "        output: PriceList\n        changes: true\n",
    );
    assert_eq!(
        problems(&[("resources/prices.jadpo", Some(&changing))], &[]),
        [provider_problem(
            "Poll `latest` calls `prices.latest`, which changes something"
        )]
    );
}

#[test]
fn a_poll_runs_at_most_once_a_minute() {
    let often = PROVIDER.replace("every: 1d", "every: 30s");
    assert_eq!(
        problems(&[("provider.jadpo", Some(&often))], &[]),
        [provider_problem(
            "Poll `latest`'s `every:` must be a duration of at least a minute"
        )]
    );
}

#[test]
fn a_poll_emits_a_public_event_of_its_package_made_from_the_answer() {
    let unknown = PROVIDER.replace("emits: PricesPublished", "emits: PricePublished");
    assert_eq!(
        problems(&[("provider.jadpo", Some(&unknown))], &[]),
        [provider_problem(
            "Poll `latest` emits `PricePublished`, which isn't one of the package's events"
        )]
    );
    let kept = PUBLISHED.replace("public type", "type");
    let found = problems(&[("events/prices_published.jadpo", Some(&kept))], &[]);
    assert!(
        found.contains(&provider_problem(
            "Poll `latest` emits `PricesPublished`, which isn't public"
        )),
        "{found:?}"
    );
    let named = PUBLISHED.replace(
        "    region: Text\n",
        "    wire_name: \"prices.published\"\n    region: Text\n",
    );
    assert_eq!(
        problems(&[("events/prices_published.jadpo", Some(&named))], &[]),
        [provider_problem(
            "Poll `latest` emits `PricesPublished`, which gives `wire_name:`"
        )]
    );
    let more = PUBLISHED.replace(
        "    petrol_pence: Int\n",
        "    petrol_pence: Int\n    lpg_pence: Int\n",
    );
    assert_eq!(
        problems(&[("events/prices_published.jadpo", Some(&more))], &[]),
        [provider_problem(
            "Poll `latest` can't make `PricesPublished` from `prices.latest`'s answer"
        )]
    );
    let other = PUBLISHED.replace("    region: Text\n", "    region: Int\n");
    let found = problems(&[("events/prices_published.jadpo", Some(&other))], &[]);
    assert!(
        found.contains(&provider_problem(
            "Poll `latest` can't make `PricesPublished` from `prices.latest`'s answer"
        )),
        "{found:?}"
    );
}

/// A package that receives webhooks needs no `wire_name:` on the event a
/// poll emits: that is the package's own, as one its action emits is.
#[test]
fn a_polls_event_is_the_packages_own_beside_its_webhooks() {
    let provider = PROVIDER.replace(
        "    max_concurrent: 4\n",
        "    max_concurrent: 4\n    webhooks: {\n        check: SignatureScheme.stripe\n        event_id: \"id\"\n        event_type: \"type\"\n    }\n",
    );
    assert_eq!(
        problems(&[("provider.jadpo", Some(&provider))], &[]),
        Vec::<String>::new()
    );
}

#[test]
fn a_connection_changes_only_its_packages_polls() {
    let misspelt = APP.replace("latest: 6h", "lastest: 6h");
    assert_eq!(
        problems(&[], &[("app.jadpo", &misspelt)]),
        ["app.jadpo connection.key_invalid: `fuel`'s `polls:` has no poll `lastest`"]
    );
    let often = APP.replace("latest: 6h", "latest: 10s");
    assert_eq!(
        problems(&[], &[("app.jadpo", &often)]),
        [
            "app.jadpo connection.value_invalid: `fuel`'s `polls: { latest: }` must be a duration of at least a minute"
        ]
    );
    let provider = polls("");
    assert_eq!(
        problems(&[("provider.jadpo", Some(&provider))], &[]),
        [
            "app.jadpo connection.key_invalid: `fuel` gives `polls:`, which `FuelPrices.v1` doesn't take"
        ]
    );
}

#[test]
fn max_concurrent_is_a_whole_number_a_connection_may_only_lower() {
    let none = PROVIDER.replace("max_concurrent: 4", "max_concurrent: 0");
    let found = problems(&[("provider.jadpo", Some(&none))], &[]);
    assert!(
        found.contains(&provider_problem(
            "`max_concurrent:` must be a whole number of at least 1"
        )),
        "{found:?}"
    );
    let more = APP.replace("max_concurrent: 2", "max_concurrent: 8");
    assert_eq!(
        problems(&[], &[("app.jadpo", &more)]),
        [
            "app.jadpo connection.value_invalid: `fuel`'s `max_concurrent:` must be a whole number from 1 to 4"
        ]
    );
    let unlimited = PROVIDER.replace("    max_concurrent: 4\n", "");
    let words = APP.replace("max_concurrent: 2", "max_concurrent: \"two\"");
    assert_eq!(
        problems(
            &[("provider.jadpo", Some(&unlimited))],
            &[("app.jadpo", &words)]
        ),
        [
            "app.jadpo connection.value_invalid: `fuel`'s `max_concurrent:` must be a whole number of at least 1"
        ]
    );
}

/// A poll's events are kept where the package's webhooks would be: with
/// several stores and none named, there is nowhere to keep them.
#[test]
fn a_poll_needs_a_store_for_its_events() {
    let app = APP
        .replace(
            "        main: {\n            driver: Driver.postgres\n        }\n",
            "        main: {\n            driver: Driver.postgres\n        }\n        audit: {\n            driver: Driver.postgres\n        }\n",
        )
        .replace("    events: {\n        store: main\n    }\n", "");
    let ledger = "entity Ledger {\n    id: Uuid\n    note: Text\n\n    store: audit\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n";
    let found = problems(
        &[],
        &[
            ("app.jadpo", &app),
            ("domains/pricing/entities/ledger.jadpo", ledger),
        ],
    );
    assert!(
        found.contains(
            &"app.jadpo event.store_required: `fuel` polls `latest` but has no store to keep its events in"
                .to_string()
        ),
        "{found:?}"
    );
}

/// Each connection's polls are planned only while the app subscribes to
/// their event, at the connection's interval, beside the system table that
/// keeps what each last saw.
#[test]
fn a_poll_is_planned_only_while_something_subscribes() {
    let read = |more: &[(&str, &str)], without: &[&str]| {
        let files: Vec<(String, String)> = files(&[], more)
            .into_iter()
            .filter(|(path, _)| !without.contains(&path.as_str()))
            .collect();
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let db = jadpo_db::Database::default();
        let project = support::project(&db, Layout::Project, &files);
        let polls: Vec<(String, String, u64)> = jadpo_check::polls::polls(&db, project)
            .polls
            .iter()
            .map(|poll| (poll.connection.clone(), poll.name.clone(), poll.every_ms))
            .collect();
        let schema = jadpo_check::data::schema(&db, project);
        let kept = schema
            .stores
            .get("main")
            .is_some_and(|store| store.tables.contains_key("jadpo_polls"));
        (polls, kept)
    };
    assert_eq!(
        read(&[], &[]),
        (
            vec![("fuel".to_string(), "latest".to_string(), 6 * 60 * 60 * 1000)],
            true
        )
    );
    let (none, kept) = read(&[], &["domains/pricing/subscribers/prices_published.jadpo"]);
    assert_eq!(none, Vec::<(String, String, u64)>::new());
    assert!(!kept);
}
