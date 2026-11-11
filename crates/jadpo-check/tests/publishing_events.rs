//! Events published outward (area 12, "Where events travel"; area 16,
//! "Outgoing webhooks" and "Sending audit entries elsewhere"):
//! `publishing:` names a connection with `driver: Driver.webhook`, whose
//! `url:` is the one address it posts to and whose `secret:` a secret
//! setting each event is signed with, and lists the events it publishes:
//! each a `public` event of one of the app's domains, or
//! `AuditEntryRecorded` while an entity keeps an audit trail, holding no
//! file, named outside by its `wire_name:` or its domain and name, no two
//! alike.

use crate::support;

use jadpo_db::Layout;

const CONFIG: &str = "config {\n    partner_hook_url: Url\n    partner_hook_secret: Text {\n        secret: true\n    }\n    partner_hook_name: Text\n}\n";

const CONNECTION: &str = "        partner_hooks: {\n            driver: Driver.webhook\n            url: config.partner_hook_url\n            secret: config.partner_hook_secret\n        }\n";

const PUBLISHING: &str = "    publishing: {\n        partner_hooks: {\n            events: [ParcelPacked]\n        }\n    }\n";

const PACKED: &str = "/// A parcel was packed.\npublic type ParcelPacked = Event {\n    label: Text {\n        max_length: 40\n    }\n    packed_at: Instant\n}\n";

const PARCEL: &str = "/// A parcel.\nentity Parcel {\n    id: Uuid\n    label: Text {\n        max_length: 40\n    }\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n";

struct Project<'a> {
    connection: &'a str,
    publishing: &'a str,
    packed: &'a str,
    parcel: &'a str,
}

const PROJECT: Project<'static> = Project {
    connection: CONNECTION,
    publishing: PUBLISHING,
    packed: PACKED,
    parcel: PARCEL,
};

fn app(project: &Project<'_>) -> String {
    format!(
        "use dispatch/entities {{ Parcel }}\nuse dispatch/events {{ ParcelPacked }}\n\napplication {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n{}    }}\n{}}}\n",
        project.connection, project.publishing
    )
}

/// Each problem as `path rule: message`, but for unused declarations, a
/// public event no case publishes, migrations and records, which these
/// projects don't keep.
fn problems(project: &Project<'_>) -> Vec<String> {
    let files = [
        ("app.jadpo".to_string(), app(project)),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/dispatch/events/parcel_packed.jadpo".to_string(),
            project.packed.to_string(),
        ),
        (
            "domains/dispatch/entities/parcel.jadpo".to_string(),
            project.parcel.to_string(),
        ),
    ];
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
                "visibility.public_unused",
            ]
            .iter()
            .any(|rule| problem.contains(&format!(" {rule}:")))
                && !problem.contains(" migration.")
                && !problem.contains(" record.")
        })
        .collect()
}

#[test]
fn a_public_event_published_through_a_webhook_checks() {
    assert_eq!(problems(&PROJECT), Vec::<String>::new());
    // Publishing it is a use of `public` outside its domain.
    let files = [
        ("app.jadpo".to_string(), app(&PROJECT)),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/dispatch/events/parcel_packed.jadpo".to_string(),
            PACKED.to_string(),
        ),
        (
            "domains/dispatch/entities/parcel.jadpo".to_string(),
            PARCEL.to_string(),
        ),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let unused: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| problem.contains(" visibility.public_unused:"))
        .collect();
    assert_eq!(unused, Vec::<String>::new());
    // An address written out, HTTPS, is one too.
    let written = CONNECTION.replace(
        "config.partner_hook_url",
        "\"https://hooks.partner.example/jadpo\"",
    );
    assert_eq!(
        problems(&Project {
            connection: &written,
            ..PROJECT
        }),
        Vec::<String>::new()
    );
}

#[test]
fn publishing_is_no_longer_refused_as_not_built() {
    assert!(
        jadpo_check::application::SECTIONS.contains(&"publishing"),
        "{:?}",
        jadpo_check::application::SECTIONS
    );
    assert!(
        !jadpo_check::application::NOT_BUILT
            .iter()
            .any(|(section, _)| *section == "publishing")
    );
}

#[test]
fn only_public_events_of_the_apps_domains_leave() {
    let private = PACKED.replace("public type", "type");
    assert_eq!(
        problems(&Project {
            packed: &private,
            ..PROJECT
        }),
        ["app.jadpo publishing.invalid: `ParcelPacked` isn't public"]
    );
    let not_event = "    publishing: {\n        partner_hooks: {\n            events: [Parcel]\n        }\n    }\n";
    let found = problems(&Project {
        publishing: not_event,
        ..PROJECT
    });
    assert_eq!(
        found,
        ["app.jadpo publishing.invalid: `Parcel` isn't an event"],
        "{found:?}"
    );
}

#[test]
fn a_connection_in_publishing_is_a_webhook_listing_its_events() {
    let cases: [(&str, &str); 5] = [
        (
            "    publishing: {\n        main: {\n            events: [ParcelPacked]\n        }\n    }\n",
            "app.jadpo publishing.invalid: `main` isn't a connection events are published through",
        ),
        (
            "    publishing: {\n        partner_hooks: {}\n    }\n",
            "app.jadpo publishing.invalid: `partner_hooks` doesn't list the events it publishes",
        ),
        (
            "    publishing: {\n        partner_hooks: {\n            events: []\n        }\n    }\n",
            "app.jadpo publishing.invalid: `events:` lists no event",
        ),
        (
            "    publishing: {\n        partner_hooks: {\n            events: [ParcelPacked, ParcelPacked]\n        }\n    }\n",
            "app.jadpo publishing.invalid: `events:` lists `ParcelPacked` twice",
        ),
        (
            "    publishing: {\n        partner_hooks: {\n            events: [ParcelPacked]\n            retries: 3\n        }\n    }\n",
            "app.jadpo publishing.invalid: A connection in `publishing:` has no key `retries:`",
        ),
    ];
    for (publishing, problem) in cases {
        assert_eq!(
            problems(&Project {
                publishing,
                ..PROJECT
            }),
            [problem],
            "{publishing}"
        );
    }
}

#[test]
fn a_webhook_connection_gives_an_address_and_a_secret_setting() {
    let cases: [(String, &str); 5] = [
        (
            CONNECTION.replace("            secret: config.partner_hook_secret\n", ""),
            "app.jadpo connection.key_missing: `partner_hooks` doesn't say what its events are signed with",
        ),
        (
            CONNECTION.replace("            url: config.partner_hook_url\n", ""),
            "app.jadpo connection.key_missing: `partner_hooks` doesn't say where its events go",
        ),
        (
            CONNECTION.replace("config.partner_hook_secret", "config.partner_hook_name"),
            "app.jadpo connection.value_invalid: `partner_hooks`'s `secret:` must be a secret setting",
        ),
        (
            CONNECTION.replace(
                "config.partner_hook_url",
                "\"http://hooks.partner.example/jadpo\"",
            ),
            "app.jadpo connection.value_invalid: `partner_hooks`'s `url:` is plain HTTP to another machine",
        ),
        (
            CONNECTION.replace(
                "            secret: config.partner_hook_secret\n",
                "            secret: config.partner_hook_secret\n            timeout: 5s\n",
            ),
            "app.jadpo connection.key_invalid: A connection with `driver: Driver.webhook` takes no `timeout:`",
        ),
    ];
    for (connection, problem) in &cases {
        assert_eq!(
            problems(&Project {
                connection,
                ..PROJECT
            }),
            [*problem],
            "{connection}"
        );
    }
}

#[test]
fn two_events_through_one_connection_are_named_apart() {
    let files = [
        (
            "app.jadpo",
            "use dispatch/events { ParcelPacked, ParcelSent }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        partner_hooks: {\n            driver: Driver.webhook\n            url: config.partner_hook_url\n            secret: config.partner_hook_secret\n        }\n    }\n    publishing: {\n        partner_hooks: {\n            events: [ParcelPacked, ParcelSent]\n        }\n    }\n}\n",
        ),
        ("config.jadpo", CONFIG),
        (
            "domains/dispatch/events/parcel_packed.jadpo",
            "/// A parcel was packed.\npublic type ParcelPacked = Event {\n    wire_name: \"parcel.moved\"\n    label: Text\n}\n",
        ),
        (
            "domains/dispatch/events/parcel_sent.jadpo",
            "/// A parcel was sent.\npublic type ParcelSent = Event {\n    wire_name: \"parcel.moved\"\n    label: Text\n}\n",
        ),
        ("domains/dispatch/entities/parcel.jadpo", PARCEL),
    ];
    let found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| problem.contains("publishing."))
        .collect();
    assert_eq!(
        found,
        [
            "app.jadpo publishing.invalid: `ParcelSent` and `ParcelPacked` are both published as `parcel.moved`"
        ]
    );
}

#[test]
fn an_event_holding_a_file_stays_in_the_app() {
    let files = [
        (
            "app.jadpo",
            "use dispatch/events { LabelPrinted }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        labels: {\n            driver: Driver.disk\n            path: \"/var/labels\"\n        }\n        partner_hooks: {\n            driver: Driver.webhook\n            url: config.partner_hook_url\n            secret: config.partner_hook_secret\n        }\n    }\n    publishing: {\n        partner_hooks: {\n            events: [LabelPrinted]\n        }\n    }\n}\n",
        ),
        ("config.jadpo", CONFIG),
        (
            "domains/dispatch/events/label_printed.jadpo",
            "/// A parcel's label was printed.\npublic type LabelPrinted = Event {\n    label: File\n}\n",
        ),
    ];
    let found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| problem.starts_with("app.jadpo"))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`LabelPrinted`"), "{found:?}");
}

#[test]
fn audit_entries_are_published_only_while_an_entity_is_audited() {
    let audit = "    publishing: {\n        partner_hooks: {\n            events: [AuditEntryRecorded]\n        }\n    }\n";
    let unaudited = Project {
        publishing: audit,
        ..PROJECT
    };
    let found: Vec<String> = problems(&unaudited)
        .into_iter()
        .filter(|problem| problem.contains("publishing."))
        .collect();
    assert_eq!(
        found,
        ["app.jadpo publishing.invalid: Nothing emits `AuditEntryRecorded`"]
    );
    let audited = PARCEL.replace("\n    policy: {", "\n    audit: {}\n\n    policy: {");
    let found: Vec<String> = problems(&Project {
        publishing: audit,
        parcel: &audited,
        ..PROJECT
    })
    .into_iter()
    .filter(|problem| problem.contains("publishing."))
    .collect();
    assert_eq!(found, Vec::<String>::new());
}

/// Each API's OpenAPI document carries `x-jadpo-events`: how the events
/// the app publishes are signed, and each by the type it is named by
/// outside, its payload a component schema with its fields' rules.
#[test]
fn the_openapi_document_lists_the_events_the_app_publishes() {
    let route = "use dispatch/types { PackRequest }\nuse dispatch/actions { pack }\n\n/// The desk's parcels.\nroutes Parcels {\n    prefix: /parcels\n\n    /// Packs a parcel.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: PackRequest\n        status: SuccessStatus.no_content\n        run: pack(input)\n    }\n}\n";
    let request = "/// A parcel to pack.\ntype PackRequest = Input {\n    label: Text {\n        max_length: 40\n    }\n}\n";
    let action = "use dispatch/entities { Parcel }\nuse dispatch/events { ParcelPacked }\nuse dispatch/types { PackRequest }\n\n/// Packs a parcel, and announces it.\naction pack(request: PackRequest) -> Unit {\n    var parcel: Parcel = Parcel.create {\n        set: {\n            label: request.label\n        }\n    }\n    emit ParcelPacked {\n        label: parcel.label\n        packed_at: clock.now\n    }\n}\n";
    let parcel = PARCEL.replace("create: [Access.system]", "create: [Access.public]");
    let app = app(&PROJECT);
    let files = [
        ("app.jadpo", app.as_str()),
        ("config.jadpo", CONFIG),
        ("domains/dispatch/events/parcel_packed.jadpo", PACKED),
        ("domains/dispatch/entities/parcel.jadpo", parcel.as_str()),
        ("domains/dispatch/types/pack_request.jadpo", request),
        ("domains/dispatch/actions/pack.jadpo", action),
        ("domains/dispatch/routes/parcels.jadpo", route),
    ];
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "desk", "1")
        .expect("a document");
    let events = document.get("x-jadpo-events").expect("x-jadpo-events");
    assert_eq!(
        events.get("signature"),
        Some(&jadpo_graph::json::Value::from("standard_webhooks"))
    );
    assert_eq!(
        events
            .get("events")
            .and_then(|events| events.get("dispatch.ParcelPacked"))
            .and_then(|event| event.get("$ref")),
        Some(&jadpo_graph::json::Value::from(
            "#/components/schemas/ParcelPacked"
        ))
    );
    let label = document
        .get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(|schemas| schemas.get("ParcelPacked"))
        .and_then(|schema| schema.get("properties"))
        .and_then(|properties| properties.get("label"))
        .and_then(|label| label.get("x-jadpo-constraints"));
    assert_eq!(
        label,
        Some(&jadpo_graph::json::Value::from("Text { max_length: 40 }")),
        "{document:?}"
    );
}

/// Personal data in a published event leaves the app through its
/// connection, which can't be asked to erase it: the personal-data report
/// lists it, and it stays only with a person's `ErasureNotPossible`.
#[test]
fn personal_data_in_a_published_event_is_reported_where_it_leaves() {
    let packed =
        "/// A parcel was packed.\npublic type ParcelPacked = Event {\n    contact: Email\n}\n";
    let action = "use dispatch/events { ParcelPacked }\n\n/// Announces a parcel.\naction announce(contact: Email) -> Unit {\n    emit ParcelPacked {\n        contact: contact\n    }\n}\n";
    let app = app(&PROJECT).replace(
        "    publishing: {",
        "    events: {\n        store: main\n    }\n    publishing: {",
    );
    let files = [
        ("app.jadpo", app.as_str()),
        ("config.jadpo", CONFIG),
        ("domains/dispatch/events/parcel_packed.jadpo", packed),
        ("domains/dispatch/entities/parcel.jadpo", PARCEL),
        ("domains/dispatch/actions/announce.jadpo", action),
    ];
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let sent: Vec<String> = jadpo_check::personal_data::sent(&db, project)
        .iter()
        .map(|sent| format!("{} {}", sent.connection, sent.through.named()))
        .collect();
    assert!(
        sent.contains(&"partner_hooks published ParcelPacked".to_string()),
        "{sent:?}"
    );
    let unerased: Vec<String> = jadpo_check::provider_erasure::unerased(&db, project)
        .iter()
        .map(|unerased| unerased.connection.clone())
        .collect();
    assert_eq!(unerased, ["partner_hooks"]);
}
