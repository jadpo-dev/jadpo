//! Batches in the checker (area 12, "Batches: work over many items";
//! chunk 6.4): `batches.each(from:, each:, finished:, key:)` only in an
//! action, `from:` a call of a named query giving a `Page`, `each:` and
//! `finished:` one call of an action in the same domain whose arguments
//! are the item, a field inside it, or a value saved with the run, every
//! saved value one an entity field could hold; `batches.status` and
//! `batches.cancel` given a `BatchRun`; the batch types read but never
//! built; a `BatchRun` kept in an entity field; and a run's tables beside
//! the store its query reads.

use crate::support;

use jadpo_db::Layout;

const BASE: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/shop/types/shelf_code.jadpo",
        "type ShelfCode = Text {\n    min_length: 1\n    max_length: 40\n}\n",
    ),
    (
        "domains/shop/types/item_page.jadpo",
        "type ItemPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/shop/entities/item.jadpo",
        "use shop/types { ShelfCode }\n\nentity Item {\n    id: Uuid\n    code: ShelfCode\n    seen: Bool\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public, Access.system]\n        update: [Access.system]\n    }\n}\n",
    ),
    (
        "domains/shop/entities/sweep.jadpo",
        "entity Sweep {\n    id: Uuid\n    run: BatchRun?\n    seen: Int?\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public, Access.system]\n        update: [Access.public, Access.system]\n    }\n}\n",
    ),
    (
        "domains/shop/queries/items.jadpo",
        "use shop/entities { Item }\nuse shop/types { ItemPage }\n\nquery items(page: ItemPage) -> Page<Item> {\n    from: Item\n    order: [Item.code asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    ),
    (
        "domains/shop/queries/all_items.jadpo",
        "use shop/entities { Item }\n\nquery all_items() -> List<Item> {\n    from: Item\n    order: [Item.code asc]\n    limit: 10\n}\n",
    ),
    (
        "domains/shop/actions/see.jadpo",
        "use shop/entities { Item, Sweep }\n\naction see(item: Item, sweep_id: Sweep.id) -> Unit {\n    var seen: Item = Item.update(item) {\n        set: {\n            seen: true\n        }\n    }\n    assert seen.seen\n}\n",
    ),
    (
        "domains/shop/actions/see_code.jadpo",
        "use shop/types { ShelfCode }\n\naction see_code(code: ShelfCode) -> Unit {\n    assert code == code\n}\n",
    ),
    (
        "domains/shop/actions/swept.jadpo",
        "use shop/entities { Sweep, SweepNotFound }\n\naction swept(summary: ImportSummary, sweep_id: Sweep.id) fails SweepNotFound -> Unit {\n    attempt Sweep.update(sweep_id) {\n        set: {\n            seen: summary.items\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/progress.jadpo",
        "action progress(run: BatchRun) -> BatchProgress {\n    var at: BatchProgress = batches.status(run)\n    assert at.items_done >= at.problems or at.state != BatchState.finished\n    return at\n}\n",
    ),
    (
        "domains/shop/actions/stop.jadpo",
        "action stop(run: BatchRun) -> Unit {\n    batches.cancel(run)\n}\n",
    ),
];

/// The imports every `start` may use; one it doesn't use isn't reported.
const IMPORTS: &str = "use shop/actions { see, see_code, see_page, swept }\nuse shop/entities { Item, Sweep }\nuse shop/queries { all_items, items }\nuse shop/types { ItemPage, ShelfCode }\n\n";

/// An action given an item and the page it was found on.
const SEE_PAGE: (&str, &str) = (
    "domains/shop/actions/see_page.jadpo",
    "use shop/entities { Item }\nuse shop/types { ItemPage }\n\naction see_page(item: Item, page: ItemPage) -> Unit {\n    assert item.seen == item.seen\n}\n",
);

/// The rules the base project gives with `start` as its
/// `domains/shop/actions/start.jadpo`, and `extra` files besides, but the
/// imports [`IMPORTS`] gives that it doesn't use.
fn rules(start: &str, extra: &[(&str, &str)]) -> Vec<String> {
    let mut files: Vec<(&str, &str)> = BASE.to_vec();
    let start = format!("{IMPORTS}{start}");
    files.push(("domains/shop/actions/start.jadpo", &start));
    files.push(SEE_PAGE);
    // A batch that checks, so the application keeps runs in one store.
    let sweep_all = format!(
        "{IMPORTS}{}",
        GOOD.replace("action start(", "action sweep_all(")
    );
    files.push(("domains/shop/actions/sweep_all.jadpo", &sweep_all));
    files.extend_from_slice(extra);
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| d.rule.id != "import.unused")
        .map(|d| {
            format!(
                "{} {}",
                d.rule.id,
                d.primary
                    .as_ref()
                    .map(|l| l.text.as_str())
                    .unwrap_or_default()
            )
        })
        .collect()
}

/// Only the rules' ids.
fn ids(start: &str) -> Vec<String> {
    rules(start, &[])
        .into_iter()
        .map(|line| line.split(' ').next().unwrap_or_default().to_string())
        .collect()
}

const GOOD: &str = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> see(item, sweep_id),
        finished: summary -> swept(summary, sweep_id),
        key: sweep_id
    )
}
"#;

#[test]
fn a_batch_started_in_an_action_checks() {
    assert_eq!(rules(GOOD, &[]), Vec::<String>::new());
    // `finished:` and `key:` may be left out, and a field of the item is
    // the item's.
    assert_eq!(
        ids(
            "action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {\n    return batches.each(\n        from: items(page),\n        each: item -> see_code(item.code)\n    )\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn only_an_action_calls_the_batch_operations() {
    let source = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> Unit {
    assert sweep_id == sweep_id
}

function started(run: BatchRun) -> BatchProgress {
    return batches.status(run)
}
"#;
    // The function is called by nothing, which is said too.
    assert_eq!(
        ids(source),
        ["layout.unused_declaration", "type.batch_context"]
    );
}

#[test]
fn from_is_a_named_querys_page() {
    let list = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: all_items(),
        each: item -> see(item, sweep_id)
    )
}
"#;
    assert_eq!(ids(list), ["type.batch_from_invalid"]);
    let value = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    var found: Page<Item> = items(page)
    return batches.each(
        from: found,
        each: item -> see(item, sweep_id)
    )
}
"#;
    assert_eq!(ids(value), ["type.batch_from_invalid"]);
}

#[test]
fn each_is_one_call_of_an_action_in_its_domain() {
    let query = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> all_items()
    )
}
"#;
    assert_eq!(rules(query, &[]), ["type.batch_each_invalid not an action"]);
    let mixed = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> see_code(ShelfCode(item.code))
    )
}
"#;
    let found = rules(mixed, &[]);
    assert!(
        found
            .iter()
            .any(|r| r == "type.batch_each_invalid the item and more"),
        "{found:?}"
    );
    let not_inline = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: sweep_id
    )
}
"#;
    assert_eq!(ids(not_inline), ["type.batch_each_invalid"]);
    let two = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: (item, other) -> see(item, sweep_id)
    )
}
"#;
    assert_eq!(ids(two), ["type.batch_each_invalid"]);
}

#[test]
fn what_a_run_saves_is_what_an_entity_field_holds() {
    // An `Input` crosses a boundary; it isn't kept on a record.
    let captured = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> see_page(item, page)
    )
}
"#;
    let found = rules(captured, &[]);
    assert_eq!(
        found,
        ["type.batch_capture_not_storable `ItemPage` has no stored form"]
    );
    let key = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> see(item, sweep_id),
        key: page
    )
}
"#;
    assert_eq!(ids(key), ["type.batch_capture_not_storable"]);
}

#[test]
fn the_batch_operations_take_what_they_take() {
    let missing = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page)
    )
}
"#;
    assert_eq!(ids(missing), ["type.argument_count"]);
    let unknown = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> see(item, sweep_id),
        then: sweep_id
    )
}
"#;
    assert_eq!(ids(unknown), ["type.named_argument_unknown"]);
    let status = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchProgress {
    return batches.status()
}
"#;
    assert_eq!(ids(status), ["type.argument_count"]);
}

#[test]
fn only_the_runtime_builds_a_batch_type() {
    let built = r#"action start(page: ItemPage, sweep_id: Sweep.id) -> BatchRun {
    return BatchRun {
        id: sweep_id
    }
}
"#;
    assert_eq!(ids(built), ["type.not_record"]);
}

#[test]
fn an_application_with_tenants_runs_batches() {
    // A run is limited to the tenant its saved tenant id names (chunk
    // 6.4c), so a tenant no longer refuses `batches.each`.
    let tenancy = [(
        "policy.jadpo",
        "use shop/entities { Sweep }\n\npolicy {\n    tenant: Sweep\n}\n",
    )];
    let found = rules(GOOD, &tenancy);
    assert!(
        !found
            .iter()
            .any(|r| r.starts_with("toolchain.not_built_yet")),
        "{found:?}"
    );
}

/// The rules the base project gives with `start` as [`GOOD`], with
/// `entity`'s file replaced by `text`.
fn with_entity(entity: &str, text: &str) -> Vec<String> {
    let path = format!("domains/shop/entities/{entity}.jadpo");
    let mut files: Vec<(&str, &str)> = BASE.iter().filter(|(at, _)| *at != path).copied().collect();
    files.push((&path, text));
    let start = format!("{IMPORTS}{GOOD}");
    files.push(("domains/shop/actions/start.jadpo", &start));
    files.push(SEE_PAGE);
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| d.rule.id != "import.unused")
        .map(|d| {
            format!(
                "{} {} ({})",
                d.rule.id,
                d.message,
                d.primary
                    .as_ref()
                    .map(|l| l.text.as_str())
                    .unwrap_or_default()
            )
        })
        .collect()
}

#[test]
fn a_batch_reading_what_only_the_public_may_read_is_refused() {
    // The run reads `items` as the system (area 9, "Batches and
    // `Access.public`"; chunk 6.4c).
    let item = "use shop/types { ShelfCode }\n\nentity Item {\n    id: Uuid\n    code: ShelfCode\n    seen: Bool\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        update: [Access.system]\n    }\n}\n";
    assert_eq!(
        with_entity("item", item),
        [
            "policy.system_grant_missing `items` runs as the system, which only `Access.public` lets read `Item` (runs as `Access.system`)"
        ]
    );
}

#[test]
fn a_batch_whose_actions_reach_what_only_the_public_may_change_is_refused() {
    // `each:`'s `see` updates an `Item`, and `finished:`'s `swept` a
    // `Sweep`, each granted only to anyone at all.
    let item = "use shop/types { ShelfCode }\n\nentity Item {\n    id: Uuid\n    code: ShelfCode\n    seen: Bool\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public, Access.system]\n        update: [Access.public]\n    }\n}\n";
    assert_eq!(
        with_entity("item", item),
        [
            "policy.system_grant_missing `see` runs as the system, which only `Access.public` lets update `Item` (runs as `Access.system`)"
        ]
    );
    let sweep = "entity Sweep {\n    id: Uuid\n    run: BatchRun?\n    seen: Int?\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public, Access.system]\n        update: [Access.public]\n    }\n}\n";
    assert_eq!(
        with_entity("sweep", sweep),
        [
            "policy.system_grant_missing `swept` runs as the system, which only `Access.public` lets update `Sweep` (runs as `Access.system`)"
        ]
    );
}

#[test]
fn a_runs_tables_live_beside_the_store_its_query_reads() {
    let db = jadpo_db::Database::default();
    let mut files: Vec<(&str, &str)> = BASE.to_vec();
    let start = format!(
        "use shop/actions {{ see, swept }}\nuse shop/entities {{ Sweep }}\nuse shop/queries {{ items }}\nuse shop/types {{ ItemPage }}\n\n{GOOD}"
    );
    files.push(("domains/shop/actions/start.jadpo", &start));
    let project = support::project(&db, Layout::Project, &files);
    let schema = jadpo_check::data::schema(&db, project);
    let tables: Vec<&String> = schema.stores["main"]
        .tables
        .keys()
        .filter(|name| name.starts_with("jadpo_batch"))
        .collect();
    assert_eq!(
        tables,
        [
            "jadpo_batch_cancels",
            "jadpo_batch_problems",
            "jadpo_batch_runs"
        ]
    );
    // An application that starts no batch has none, and nowhere to find
    // a run.
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, BASE);
    let schema = jadpo_check::data::schema(&db, project);
    assert!(
        !schema.stores["main"]
            .tables
            .keys()
            .any(|name| name.starts_with("jadpo_batch"))
    );
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| format!("{} {}", d.rule.id, d.message))
        .collect();
    assert_eq!(
        found,
        [
            "toolchain.not_built_yet `batches.status` here isn't built yet",
            "toolchain.not_built_yet `batches.cancel` here isn't built yet",
        ]
    );
}

#[test]
fn a_runs_items_write_the_store_its_query_reads() {
    // `Tally` lives in another store than the items the run walks, so
    // one batch couldn't commit its items' writes and its position
    // together.
    let app = (
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        tallies: {\n            driver: Driver.postgres\n        }\n    }\n}\n",
    );
    let tally = (
        "domains/shop/entities/tally.jadpo",
        "entity Tally {\n    id: Uuid\n    seen: Bool\n\n    store: tallies\n\n    policy: {\n        create: [Access.public, Access.system]\n        read: [Access.public, Access.system]\n    }\n}\n",
    );
    let count = (
        "domains/shop/actions/count.jadpo",
        "use shop/entities { Item, Tally }\n\naction count(item: Item) -> Unit {\n    var tally: Tally = Tally.create {\n        set: {\n            seen: item.seen\n        }\n    }\n    assert tally.seen == item.seen\n}\n",
    );
    let start = r#"use shop/actions { count }
use shop/queries { items }
use shop/types { ItemPage }

action start(page: ItemPage) -> BatchRun {
    return batches.each(
        from: items(page),
        each: item -> count(item)
    )
}
"#;
    let mut files: Vec<(&str, &str)> = BASE
        .iter()
        .filter(|(path, _)| *path != "app.jadpo")
        .copied()
        .collect();
    files.extend([
        app,
        tally,
        count,
        ("domains/shop/actions/start.jadpo", start),
    ]);
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| format!("{} {}", d.rule.id, d.message))
        .collect();
    assert!(
        found
            .iter()
            .any(|line| line == "store.action_spans_stores `start` writes two stores"),
        "{found:?}"
    );
}

// A batch's summary under policy (area 12, "Chosen in chunk 6.4d"): in an
// application with a tenant, a run's `finished:` stores what its
// `ImportSummary` counts in a record of the tenant the run is limited to.

/// An allotment application whose societies are its tenants: a round of
/// invoices walks one society's plots, and `finished:` records how it went
/// in the round. `changes` replaces text in the files, by path.
fn societies(changes: &[(&str, &str, &str)]) -> Vec<String> {
    let mut files: Vec<(String, String)> = SOCIETIES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    for (path, from, to) in changes {
        let file = files
            .iter_mut()
            .find(|(at, _)| at == path)
            .unwrap_or_else(|| panic!("no file {path}"));
        assert!(file.1.contains(from), "{path} has no {from:?}");
        file.1 = file.1.replace(from, to);
    }
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|line| !line.contains("import.unused"))
        .collect();
    // The summary holds counts and problems, never a record's fields.
    for line in &found {
        for field in ["items", "items_with_problems", "problems"] {
            assert!(
                !line.contains(&format!(".{field}`")),
                "a problem names a field the summary's records don't have: {line}"
            );
        }
    }
    found
}

const SOCIETIES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "use plots/entities { Gardener }\n\napplication {}\n\nprincipal {\n    user: {\n        subject: Gardener.email\n    }\n}\n",
    ),
    (
        "authentication.jadpo",
        "authentication {\n    browser_session: {\n        cookie: \"allotments_session\"\n    }\n}\n",
    ),
    (
        "policy.jadpo",
        "use plots/entities { Society }\n\npolicy {\n    tenant: Society\n}\n",
    ),
    (
        "domains/plots/types/officer_role.jadpo",
        "type OfficerRole = Enum {\n    secretary\n    treasurer\n}\n",
    ),
    (
        "domains/plots/types/name.jadpo",
        "type Name = Text {\n    min_length: 1\n    max_length: 80\n}\n",
    ),
    (
        "domains/plots/types/plot_page.jadpo",
        "type PlotPage = Input {\n    page_size: PageSize {\n        default: 50\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/plots/failures/plot_unlet.jadpo",
        "type PlotUnlet = Failure {\n    kind: FailureKind.rejected\n    message: \"Nobody rents this plot.\"\n}\n",
    ),
    (
        "domains/plots/entities/gardener.jadpo",
        "use plots/types { Name }\n\nentity Gardener {\n    id: Uuid\n    email: Email {\n        unique: true\n    }\n    name: Name\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n        update: [id]\n        tenant: Tenancy.shared\n    }\n}\n",
    ),
    (
        "domains/plots/entities/society.jadpo",
        "use plots/types { Name, OfficerRole }\n\nentity Society {\n    id: Uuid\n    name: Name\n\n    policy: {\n        create: [Access.authenticated]\n        read: [OfficerRole.secretary, OfficerRole.treasurer, Access.system]\n        update: [OfficerRole.secretary]\n    }\n}\n",
    ),
    (
        "domains/plots/entities/officer.jadpo",
        "use plots/entities { Gardener, Society }\nuse plots/types { OfficerRole }\n\nentity Officer {\n    id: Uuid\n    society_id: Society.id {\n        on_delete: OnDelete.cascade\n    }\n    gardener_id: Gardener.id {\n        on_delete: OnDelete.cascade\n    }\n    role: OfficerRole\n\n    membership: {\n        scope: society_id\n        member: gardener_id\n        role: role\n    }\n\n    policy: {\n        create: [OfficerRole.secretary]\n        read: [OfficerRole.secretary, OfficerRole.treasurer]\n        delete: [OfficerRole.secretary]\n    }\n}\n",
    ),
    (
        // `lapsed` and `notes` only the secretary reads.
        "domains/plots/entities/plot.jadpo",
        "use plots/entities { Society }\nuse plots/types { Name, OfficerRole }\n\nentity Plot {\n    id: Uuid\n    society_id: Society.id {\n        on_delete: OnDelete.cascade\n    }\n    holder: Name?\n    rent: Int\n    lapsed: Bool {\n        policy: {\n            read: [OfficerRole.secretary, Access.system]\n        }\n    }\n    notes: Name {\n        policy: {\n            read: [OfficerRole.secretary, Access.system]\n        }\n    }\n\n    policy: {\n        create: [OfficerRole.secretary]\n        read: [OfficerRole.secretary, OfficerRole.treasurer, Access.system]\n        update: [OfficerRole.secretary, Access.system]\n    }\n}\n",
    ),
    (
        "domains/plots/entities/rent_round.jadpo",
        "use plots/entities { Society }\nuse plots/types { Name, OfficerRole }\n\nentity RentRound {\n    id: Uuid\n    society_id: Society.id {\n        on_delete: OnDelete.cascade\n    }\n    memo: Name\n    invoices: BatchRun?\n    invoiced: Int?\n    unlet: Int?\n\n    policy: {\n        create: [OfficerRole.treasurer]\n        read: [OfficerRole.treasurer, Access.system]\n        update: [OfficerRole.treasurer, Access.system]\n    }\n}\n",
    ),
    (
        "domains/plots/entities/invoice.jadpo",
        "use plots/entities { Plot, RentRound, Society }\nuse plots/types { Name, OfficerRole }\n\nentity Invoice {\n    id: Uuid\n    society_id: Society.id {\n        on_delete: OnDelete.cascade\n    }\n    plot_id: Plot.id {\n        on_delete: OnDelete.cascade\n    }\n    round_id: RentRound.id {\n        on_delete: OnDelete.cascade\n    }\n    amount: Int\n    memo: Name?\n\n    policy: {\n        create: [Access.system]\n        read: [OfficerRole.treasurer, Access.system]\n    }\n}\n",
    ),
    (
        "domains/plots/entities/round_note.jadpo",
        "use plots/entities { Society }\nuse plots/types { Name, OfficerRole }\n\nentity RoundNote {\n    id: Uuid\n    society_id: Society.id {\n        on_delete: OnDelete.cascade\n    }\n    memo: Name\n    invoiced: Int\n\n    policy: {\n        create: [Access.system]\n        read: [OfficerRole.treasurer, Access.system]\n    }\n}\n",
    ),
    (
        // Every society's officers read a tally.
        "domains/plots/entities/tally.jadpo",
        "entity Tally {\n    id: Uuid\n    unlet: Int\n\n    policy: {\n        create: [Access.system]\n        read: [Access.authenticated]\n        tenant: Tenancy.shared\n    }\n}\n",
    ),
    (
        "domains/plots/queries/society_plots.jadpo",
        "use plots/entities { Plot, Society }\nuse plots/types { PlotPage }\n\nquery society_plots(society_id: Society.id, page: PlotPage) -> Page<Plot> {\n    from: Plot\n    where: Plot.society_id == society_id\n    order: [Plot.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    ),
    (
        "domains/plots/actions/invoice_plot.jadpo",
        "use plots/entities { Invoice, Plot, RentRound, RentRoundNotFound }\nuse plots/failures { PlotUnlet }\n\naction invoice_plot(plot: Plot, round_id: RentRound.id) fails PlotUnlet, RentRoundNotFound -> Invoice {\n    if (plot.holder == none) {\n        reject PlotUnlet {}\n    }\n    return attempt Invoice.create {\n        set: {\n            society_id: plot.society_id\n            plot_id: plot.id\n            round_id: round_id\n            amount: plot.rent\n            memo: none\n        }\n    }\n}\n",
    ),
    (
        // Writes nothing: whether it fails is all it shows.
        "domains/plots/actions/check_plot.jadpo",
        "use plots/entities { Plot }\nuse plots/failures { PlotUnlet }\n\naction check_plot(plot: Plot) fails PlotUnlet -> Unit {\n    if (plot.holder == none) {\n        reject PlotUnlet {}\n    }\n}\n",
    ),
    (
        "domains/plots/actions/record_round.jadpo",
        "use plots/entities { RentRound, RentRoundNotFound }\n\naction record_round(summary: ImportSummary, round_id: RentRound.id) fails RentRoundNotFound -> Unit {\n    attempt RentRound.update(round_id) {\n        set: {\n            invoiced: summary.items\n            unlet: summary.items_with_problems\n        }\n    }\n}\n",
    ),
    (
        "domains/plots/actions/tally_round.jadpo",
        "use plots/entities { Tally }\n\naction tally_round(summary: ImportSummary) -> Unit {\n    var kept: Tally = Tally.create {\n        set: {\n            unlet: summary.items_with_problems\n        }\n    }\n    assert kept.unlet >= 0\n}\n",
    ),
    (
        "domains/plots/actions/note_round.jadpo",
        "use plots/entities { RoundNote, Society, SocietyNotFound }\nuse plots/types { Name }\n\naction note_round(summary: ImportSummary, society_id: Society.id, memo: Name) fails SocietyNotFound -> Unit {\n    var kept: RoundNote = attempt RoundNote.create {\n        set: {\n            society_id: society_id\n            memo: memo\n            invoiced: summary.items\n        }\n    }\n    assert kept.invoiced >= 0\n}\n",
    ),
    (
        "domains/plots/actions/invoice_round.jadpo",
        "use plots/actions { check_plot, invoice_plot, note_round, record_round, tally_round }\nuse plots/entities { RentRound }\nuse plots/queries { society_plots }\nuse plots/types { PlotPage }\n\naction invoice_round(rent_round: RentRound) -> RentRound {\n    var invoicing: BatchRun = batches.each(\n        from: society_plots(rent_round.society_id, PlotPage {}),\n        each: plot -> invoice_plot(plot, rent_round.id),\n        finished: summary -> record_round(summary, rent_round.id),\n        key: rent_round.id\n    )\n    return RentRound.update(rent_round) {\n        set: {\n            invoices: invoicing\n        }\n    }\n}\n",
    ),
    (
        // A round found by its `id`, whose memo `finished:` notes.
        "domains/plots/actions/note_by_id.jadpo",
        "use plots/actions { invoice_plot, note_round }\nuse plots/entities { RentRound, RentRoundNotFound, Society }\nuse plots/queries { society_plots }\nuse plots/types { PlotPage }\n\naction note_by_id(round_id: RentRound.id, society_id: Society.id) fails RentRoundNotFound -> BatchRun {\n    var rent_round: RentRound = attempt RentRound.get(round_id)\n    return batches.each(\n        from: society_plots(rent_round.society_id, PlotPage {}),\n        each: plot -> invoice_plot(plot, rent_round.id),\n        finished: summary -> note_round(summary, rent_round.society_id, rent_round.memo),\n        key: rent_round.id\n    )\n}\n",
    ),
];

const ROUND: &str = "domains/plots/actions/invoice_round.jadpo";
const INVOICE_PLOT: &str = "domains/plots/actions/invoice_plot.jadpo";
const CHECK_PLOT: &str = "domains/plots/actions/check_plot.jadpo";
const NOTE_BY_ID: &str = "domains/plots/actions/note_by_id.jadpo";

#[test]
fn a_tenanted_runs_finished_stores_its_counts_in_the_tenants_record() {
    // `invoiced` is how many plots the run walked, and `unlet` how many
    // of them `invoice_plot` refused, which `holder` decided: the run is
    // limited to the round's society, whose treasurer reads both.
    assert_eq!(societies(&[]), Vec::<String>::new());
}

#[test]
fn what_decided_a_runs_failures_goes_with_its_summary() {
    // `lapsed` decides which plots fail, so the count of those that did
    // shows it, and only the secretary reads it.
    let each = (
        ROUND,
        "each: plot -> invoice_plot(plot, rent_round.id)",
        "each: plot -> check_plot(plot)",
    );
    let lapsed = (CHECK_PLOT, "if (plot.holder == none)", "if (plot.lapsed)");
    assert_eq!(
        societies(&[each, lapsed]),
        [
            "domains/plots/actions/invoice_round.jadpo policy.output_field_unproved: `invoice_round` passes a value decided by `Plot.lapsed` to `record_round`, which stores it in `RentRound.unlet`, though not everyone who may read `unlet` may read it"
        ]
    );
    // How many plots the run walked shows nothing of them.
    let invoiced_only = (
        "domains/plots/actions/record_round.jadpo",
        "            unlet: summary.items_with_problems\n",
        "",
    );
    assert_eq!(
        societies(&[each, lapsed, invoiced_only]),
        Vec::<String>::new()
    );
}

#[test]
fn a_runs_summary_stored_where_every_tenant_reads_it_is_refused() {
    // A tally every society's officers read isn't the round's society's.
    let finished = (
        ROUND,
        "finished: summary -> record_round(summary, rent_round.id)",
        "finished: summary -> tally_round(summary)",
    );
    assert_eq!(
        societies(&[finished]),
        [
            "domains/plots/actions/invoice_round.jadpo policy.output_field_unproved: `invoice_round` passes a value decided by `Plot.holder` to `tally_round`, which stores it in `Tally.unlet`, though not everyone who may read `unlet` may read it"
        ]
    );
}

#[test]
fn what_a_runs_action_stores_from_its_item_is_followed() {
    // The item is one of the plots the run walked: its `notes`, which only
    // the secretary reads, can't go in an invoice the treasurer reads.
    let memo = (INVOICE_PLOT, "memo: none", "memo: plot.notes");
    assert_eq!(
        societies(&[memo]),
        [
            "domains/plots/actions/invoice_round.jadpo policy.output_field_unproved: `invoice_round` passes `Plot.notes` to `invoice_plot`, which stores it in `Invoice.memo`, though not everyone who may read `memo` may read it",
            "domains/plots/actions/note_by_id.jadpo policy.output_field_unproved: `note_by_id` passes `Plot.notes` to `invoice_plot`, which stores it in `Invoice.memo`, though not everyone who may read `memo` may read it",
        ]
    );
    // Its `holder` the treasurer reads, within the society.
    let holder = (INVOICE_PLOT, "memo: none", "memo: plot.holder");
    assert_eq!(societies(&[holder]), Vec::<String>::new());
}

#[test]
fn a_saved_value_is_the_runs_tenants_when_its_record_named_the_tenant() {
    // `note_by_id`'s run is limited to the round's own society, so its
    // memo, which the treasurer reads, may go in a note of that society.
    // With the society the request named, nothing says the memo is the
    // society's the run is limited to, and the run's society is one the
    // action never reached, at each value naming it (chunk 6.6c).
    let from = (
        NOTE_BY_ID,
        "from: society_plots(rent_round.society_id, PlotPage {})",
        "from: society_plots(society_id, PlotPage {})",
    );
    let note = (
        NOTE_BY_ID,
        "note_round(summary, rent_round.society_id, rent_round.memo)",
        "note_round(summary, society_id, rent_round.memo)",
    );
    assert_eq!(
        societies(&[from, note]),
        [
            "domains/plots/actions/note_by_id.jadpo policy.event_tenant_unproved: `note_by_id` starts a batch in a `Society` it never reached",
            "domains/plots/actions/note_by_id.jadpo policy.event_tenant_unproved: `note_by_id` starts a batch in a `Society` it never reached",
            "domains/plots/actions/note_by_id.jadpo policy.output_field_unproved: `note_by_id` passes `RentRound.memo` to `note_round`, which stores it in `RoundNote.memo`, though not everyone who may read `memo` may read it"
        ]
    );
}

#[test]
fn a_reference_to_the_tenant_names_a_runs_tenant() {
    // `rent_round.society_id` is of the tenant's `id` type, as declared
    // by `RentRound`'s field: a saved value of it limits the run to that
    // society (chunk 6.4d), and so does one of `Society.id` itself.
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, SOCIETIES);
    let registry = jadpo_check::graph::registry(&db, project);
    let field = |entity: &str, name: &str| {
        jadpo_graph::TypeRef::Field(jadpo_check::graph::entity_field_key(
            registry,
            entity,
            &[name.to_string()],
        ))
    };
    let saved = [
        field("RentRound", "society_id"),
        field("RentRound", "id"),
        jadpo_graph::TypeRef::Nullable(Box::new(field("Society", "id"))),
    ];
    assert_eq!(
        jadpo_check::batches::run_tenant(&db, project, &saved, Some(&field("Plot", "society_id"))),
        jadpo_std::batch::BatchTenant::Saved {
            captured: vec![0, 2],
            key: true
        }
    );
}

// The Phase 6 review's policy proofs (chunk 6.6b).

/// A diary application: each writer's diaries hold a secret only its
/// owner and background work read, and a board anyone signed in reads.
/// `changes` replaces text in the files, by path.
fn diaries(changes: &[(&str, &str, &str)]) -> Vec<String> {
    let mut files: Vec<(String, String)> = DIARIES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    for (path, from, to) in changes {
        match files.iter_mut().find(|(at, _)| at == path) {
            Some(file) => {
                assert!(file.1.contains(from), "{path} has no {from:?}");
                file.1 = file.1.replace(from, to);
            }
            None => files.push((path.to_string(), to.to_string())),
        }
    }
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|line| !line.contains("import.unused"))
        .collect()
}

const DIARIES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "authentication.jadpo",
        "use journal/entities { Writer }\n\nprincipal {\n    user: {\n        subject: Writer.handle\n    }\n}\n\nauthentication {\n    browser_session: {\n        cookie: \"journal_session\"\n    }\n}\n",
    ),
    (
        "domains/journal/types/line.jadpo",
        "type Line = Text {\n    min_length: 1\n    max_length: 200\n}\n",
    ),
    (
        "domains/journal/types/diary_page.jadpo",
        "type DiaryPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/journal/entities/writer.jadpo",
        "use journal/types { Line }\n\nentity Writer {\n    id: Uuid\n    handle: Line {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n    }\n}\n",
    ),
    (
        // Only its owner, and background work, read a diary's secret.
        "domains/journal/entities/diary.jadpo",
        "use journal/entities { Writer }\nuse journal/types { Line }\n\nentity Diary {\n    id: Uuid\n    owner_id: Writer.id {\n        on_delete: OnDelete.cascade\n    }\n    title: Line\n    secret: Line {\n        policy: {\n            read: [owner_id, Access.system]\n        }\n    }\n\n    policy: {\n        create: [owner_id]\n        read: [Access.authenticated, Access.system]\n    }\n}\n",
    ),
    (
        // Anyone signed in reads the board.
        "domains/journal/entities/board.jadpo",
        "use journal/types { Line }\n\nentity Board {\n    id: Uuid\n    text: Line\n\n    policy: {\n        create: [Access.system]\n        read: [Access.authenticated, Access.system]\n    }\n}\n",
    ),
    (
        "domains/journal/queries/diaries.jadpo",
        "use journal/entities { Diary }\nuse journal/types { DiaryPage }\n\nquery diaries(page: DiaryPage) -> Page<Diary> {\n    from: Diary\n    order: [Diary.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
    ),
    (
        "domains/journal/actions/post_item.jadpo",
        "use journal/entities { Board, Diary }\n\naction post_item(diary: Diary) -> Unit {\n    var posted: Board = Board.create {\n        set: {\n            text: diary.title\n        }\n    }\n    assert posted.text == posted.text\n}\n",
    ),
    (
        "domains/journal/actions/post_all.jadpo",
        "use journal/actions { post_item }\nuse journal/queries { diaries }\nuse journal/types { DiaryPage }\n\naction post_all(page: DiaryPage) -> BatchRun {\n    return batches.each(\n        from: diaries(page),\n        each: diary -> post_item(diary)\n    )\n}\n",
    ),
];

const POST_ITEM: &str = "domains/journal/actions/post_item.jadpo";

#[test]
fn a_runs_item_written_where_others_read_it_is_refused() {
    // A diary's title anyone signed in reads may go on the board.
    assert_eq!(diaries(&[]), Vec::<String>::new());
    // Its secret, which only its owner reads, may not: each item is one
    // of the diaries `from:` finds, followed into what `each:`'s action
    // stores, as a call's arguments are.
    let secret = (POST_ITEM, "text: diary.title", "text: diary.secret");
    assert_eq!(
        diaries(&[secret]),
        [
            "domains/journal/actions/post_all.jadpo policy.output_field_unproved: `post_all` passes `Diary.secret` to `post_item`, which stores it in `Board.text`, though not everyone who may read `text` may read it"
        ]
    );
    // The same when the secret is the item's field `each:` passes, or
    // when the whole diary is its owner's alone.
    let post_text = (
        "domains/journal/actions/post_text.jadpo",
        "",
        "use journal/entities { Board }\nuse journal/types { Line }\n\naction post_text(line: Line) -> Unit {\n    var posted: Board = Board.create {\n        set: {\n            text: line\n        }\n    }\n    assert posted.text == posted.text\n}\n",
    );
    let each = (
        "domains/journal/actions/post_all.jadpo",
        "use journal/actions { post_item }\n",
        "use journal/actions { post_item, post_text }\n",
    );
    let field = (
        "domains/journal/actions/post_all.jadpo",
        "each: diary -> post_item(diary)",
        "each: diary -> post_text(diary.secret)",
    );
    assert_eq!(
        diaries(&[post_text, each, field]),
        [
            "domains/journal/actions/post_all.jadpo policy.output_field_unproved: `post_all` passes `Diary.secret` to `post_text`, which stores it in `Board.text`, though not everyone who may read `text` may read it"
        ]
    );
    let owners = (
        "domains/journal/entities/diary.jadpo",
        "read: [Access.authenticated, Access.system]",
        "read: [owner_id, Access.system]",
    );
    assert_eq!(
        diaries(&[owners]),
        [
            "domains/journal/actions/post_all.jadpo policy.output_field_unproved: `post_all` passes `Diary.title` to `post_item`, which stores it in `Board.text`, though not everyone who may read `text` may read it"
        ]
    );
}

/// A shop where shoppers sign in, with `routes` as its routes file and
/// `types` among its types: the problems, each with its message.
fn requested(types: &str, routes: &str) -> Vec<String> {
    let auth = (
        "authentication.jadpo",
        "use shop/entities { Shopper }\n\nprincipal {\n    user: {\n        subject: Shopper.handle\n    }\n}\n\nauthentication {\n    browser_session: {\n        cookie: \"shop_session\"\n    }\n}\n",
    );
    let shopper = (
        "domains/shop/entities/shopper.jadpo",
        "use shop/types { ShelfCode }\n\nentity Shopper {\n    id: Uuid\n    handle: ShelfCode {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n    }\n}\n",
    );
    let types = format!("use shop/entities {{ Sweep }}\nuse shop/types {{ ShelfCode }}\n\n{types}");
    let halted = (
        "domains/shop/types/halted.jadpo",
        "use shop/types { ShelfCode }\n\ntype Halted = Output {\n    code: ShelfCode\n}\n",
    );
    // Neither reaches an entity, so its line says who may call it.
    let policy = (
        "policy.jadpo",
        "use shop\n\npolicy {\n    actions: {\n        shop.halt: [Access.authenticated]\n        shop.take: [Access.authenticated]\n    }\n}\n",
    );
    let halt = (
        "domains/shop/actions/halt.jadpo",
        "use shop/types { Halted }\n\naction halt(run: BatchRun) -> Halted {\n    batches.cancel(run)\n    return Halted {\n        code: \"halted\"\n    }\n}\n",
    );
    let take = (
        "domains/shop/actions/take.jadpo",
        "use shop/types { Asked, Halted }\n\naction take(asked: Asked) -> Halted {\n    return Halted {\n        code: \"taken\"\n    }\n}\n",
    );
    let stop_kept = (
        "domains/shop/actions/stop_kept.jadpo",
        "use shop/entities { Sweep, SweepNotFound }\n\naction stop_kept(sweep_id: Sweep.id) fails SweepNotFound -> Unit {\n    var sweep: Sweep = attempt Sweep.get(sweep_id)\n    match sweep.run {\n        some(run) -> {\n            batches.cancel(run)\n        }\n        none -> {}\n    }\n}\n",
    );
    let routes = format!(
        "use shop/actions {{ halt, stop_kept, take }}\nuse shop/entities {{ Sweep }}\nuse shop/types {{ Asked, Halted }}\n\nroutes Runs {{\n    prefix: /runs\n\n    {routes}\n}}\n"
    );
    let db = jadpo_db::Database::default();
    let mut files: Vec<(&str, &str)> = BASE.to_vec();
    let start = format!("{IMPORTS}{GOOD}");
    files.extend([
        auth,
        shopper,
        halt,
        halted,
        policy,
        take,
        stop_kept,
        SEE_PAGE,
        ("domains/shop/actions/start.jadpo", start.as_str()),
        ("domains/shop/types/asked.jadpo", types.as_str()),
        ("domains/shop/routes/runs.jadpo", routes.as_str()),
    ]);
    let project = support::project(&db, Layout::Project, &files);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| !matches!(d.rule.id, "import.unused" | "record.out_of_date"))
        .map(|d| format!("{} {}", d.rule.id, d.message))
        .collect()
}

const ASKED: &str = "type Asked = Input {\n    run: BatchRun\n}\n";

const POST_HALT: &str =
    "POST / {\n        input: Asked\n        output: Halted\n        run: halt(input.run)\n    }";

#[test]
fn no_request_carries_a_batch_type() {
    // A run comes from `batches.each` or a field the caller may read
    // (chunk 6.6b): in a body, a query string or a placeholder, anyone
    // could name any run, and read or stop it.
    assert_eq!(
        requested(ASKED, POST_HALT),
        ["type.batch_input `Asked.run` holds a `BatchRun`, which no request carries"]
    );
    let query = "DELETE / {\n        input: Asked\n        output: Halted\n        run: halt(input.run)\n    }";
    assert_eq!(
        requested(ASKED, query),
        ["type.batch_input `Asked.run` holds a `BatchRun`, which no request carries"]
    );
    let placeholder = "POST /{run: BatchRun} {\n        input: Asked\n        output: Halted\n        run: halt(run)\n    }";
    let unused = "type Asked = Input {\n    note: ShelfCode\n}\n";
    assert_eq!(
        requested(unused, placeholder),
        ["type.batch_input Placeholder `run` holds a `BatchRun`, which no request carries"]
    );
}

#[test]
fn a_batch_type_anywhere_in_an_input_is_refused() {
    for (field, batch) in [
        ("runs: List<BatchRun>", "BatchRun"),
        ("run: BatchRun?", "BatchRun"),
        ("at: BatchProgress", "BatchProgress"),
        ("summary: ImportSummary", "ImportSummary"),
        ("problems: List<RowProblem>", "RowProblem"),
        // An entity's field holding one, and an object holding one.
        ("run: Sweep.run", "BatchRun"),
        ("kept: Kept", "BatchRun"),
        // A type built on one.
        ("run: KeptRun", "BatchRun"),
    ] {
        let types = format!(
            "type Kept = Object {{\n    run: BatchRun\n}}\n\ntype KeptRun = BatchRun {{}}\n\ntype Asked = Input {{\n    {field}\n}}\n"
        );
        let route = "POST / {\n        input: Asked\n        output: Halted\n        run: take(input)\n    }";
        let found = requested(&types, route);
        let name = field.split(':').next().unwrap_or_default();
        assert!(
            found.contains(&format!(
                "type.batch_input `Asked.{name}` holds a `{batch}`, which no request carries"
            )),
            "{field}: {found:?}"
        );
    }
}

#[test]
fn a_batch_types_field_names_no_type() {
    // A run's `id` is read from the run; named as a type it would stand
    // for nothing, and let a request's value through where a run goes.
    let types = "type Asked = Input {\n    run: BatchRun.id\n}\n";
    assert_eq!(
        requested(types, POST_HALT),
        ["type.unknown_field `BatchRun.id` isn't a type"]
    );
    let types = "type Asked = Input {\n    run: InstantRange.begin\n}\n";
    assert_eq!(
        requested(types, POST_HALT),
        ["type.unknown_field `InstantRange` has no field `begin`"]
    );
}

#[test]
fn a_run_kept_where_the_caller_may_read_it_is_stopped() {
    // The run a sweep keeps: `stop_kept` finds the sweep under its
    // grants, and stops the run it holds.
    let route = "POST /{sweep_id: Sweep.id}/stop {\n        status: SuccessStatus.no_content\n        run: stop_kept(sweep_id)\n    }";
    let types = "type Asked = Input {\n    note: ShelfCode\n}\n";
    assert_eq!(requested(types, route), Vec::<String>::new());
}

/// The diary application with `run_count`, which tells its caller how
/// many diaries a run couldn't check, each item's `check_item` refusing a
/// diary whose `field` is a draft; `listed`, with a line letting in only
/// those signed in.
fn counted(field: &str, listed: bool) -> Vec<String> {
    let draft = (
        "domains/journal/failures/draft.jadpo",
        "",
        "type Draft = Failure {\n    kind: FailureKind.rejected\n    message: \"This diary is a draft.\"\n}\n",
    );
    let check_item = format!(
        "use journal/entities {{ Diary }}\nuse journal/failures {{ Draft }}\n\naction check_item(diary: Diary) fails Draft -> Unit {{\n    if (diary.{field} == \"draft\") {{\n        reject Draft {{}}\n    }}\n}}\n"
    );
    let check_item = (
        "domains/journal/actions/check_item.jadpo",
        "",
        check_item.as_str(),
    );
    let each = (
        "domains/journal/actions/post_all.jadpo",
        "use journal/actions { post_item }\n",
        "use journal/actions { check_item }\n",
    );
    let calls = (
        "domains/journal/actions/post_all.jadpo",
        "each: diary -> post_item(diary)",
        "each: diary -> check_item(diary)",
    );
    let run_count = (
        "domains/journal/actions/run_count.jadpo",
        "",
        "use journal/types { RunTally }\n\naction run_count(run: BatchRun) -> RunTally {\n    var at: BatchProgress = batches.status(run)\n    return RunTally {\n        problems: at.problems\n    }\n}\n",
    );
    let output = (
        "domains/journal/types/run_tally.jadpo",
        "",
        "type RunTally = Output {\n    problems: Int\n}\n",
    );
    let line = (
        "policy.jadpo",
        "",
        "use journal\n\npolicy {\n    actions: {\n        journal.run_count: [Access.authenticated]\n    }\n}\n",
    );
    let mut changes = vec![draft, check_item, each, calls, run_count, output];
    if listed {
        changes.push(line);
    }
    diaries(&changes)
}

#[test]
fn a_runs_status_is_held_to_what_decides_its_failures() {
    // How many items a run couldn't post shows what decided they failed
    // (chunk 6.6b): a diary's secret, which only its owner reads.
    assert_eq!(
        counted("secret", true),
        [
            "domains/journal/actions/run_count.jadpo policy.output_field_unproved: `journal.RunTally.problems` carries `Diary.secret`, which not every caller of `run_count` may read"
        ]
    );
    // Its title anyone signed in reads, and `run_count`'s line lets in
    // only those signed in.
    assert_eq!(counted("title", true), Vec::<String>::new());
    // A caller nothing is known of finds the run by its `id` alone, so it
    // may read only what anyone at all may.
    assert_eq!(
        counted("title", false),
        [
            "domains/journal/actions/run_count.jadpo policy.output_field_unproved: `journal.RunTally.problems` carries `Diary.title`, which not every caller of `run_count` may read"
        ]
    );
}

#[test]
fn a_status_read_by_a_runs_own_action_is_solved() {
    // Whether `check_after` fails is decided by another run's status,
    // which every batch's failures decide, its own among them: a cycle,
    // solved as a summary's is.
    let after = (
        "domains/journal/actions/check_after.jadpo",
        "",
        "use journal/entities { Diary }\nuse journal/failures { Draft }\n\naction check_after(diary: Diary, earlier: BatchRun) fails Draft -> Unit {\n    var at: BatchProgress = batches.status(earlier)\n    if (at.problems > 0) {\n        reject Draft {}\n    }\n}\n",
    );
    let draft = (
        "domains/journal/failures/draft.jadpo",
        "",
        "type Draft = Failure {\n    kind: FailureKind.rejected\n    message: \"This diary is a draft.\"\n}\n",
    );
    let start = (
        "domains/journal/actions/post_all.jadpo",
        "use journal/actions { post_item }\nuse journal/queries { diaries }\nuse journal/types { DiaryPage }\n\naction post_all(page: DiaryPage) -> BatchRun {\n    return batches.each(\n        from: diaries(page),\n        each: diary -> post_item(diary)\n    )\n}\n",
        "use journal/actions { check_after }\nuse journal/queries { diaries }\nuse journal/types { DiaryPage }\n\naction post_all(page: DiaryPage, earlier: BatchRun) -> BatchRun {\n    return batches.each(\n        from: diaries(page),\n        each: diary -> check_after(diary, earlier)\n    )\n}\n",
    );
    assert_eq!(diaries(&[after, draft, start]), Vec::<String>::new());
}
