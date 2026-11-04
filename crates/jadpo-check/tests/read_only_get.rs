//! A `GET` that runs an action (area 7, "What a route runs"; chunk G2): it
//! runs a named query, or an action the graph proves writes nothing, emits
//! nothing and makes no changing call, itself and through every action it
//! calls. `route.run_kind` names what the action does and where, each way
//! of not being read-only on a project of its own, and each way of being
//! read-only compiles.

use crate::support;

use jadpo_db::{Database, Layout};

const HOME: &str = "shared/providers/acme_pay/v1";

fn base() -> Vec<(String, String)> {
    [
        (
            "app.jadpo",
            "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/shop/sheets\"\n        }\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [shop]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n            embedding: config.search_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n        embedding_dimensions: 3\n    }\n    events: {\n        store: main\n    }\n    steps: {\n        encryption_key: config.step_encryption_key\n    }\n}\n",
        ),
        (
            "config.jadpo",
            "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    search_model: ModelId {\n        default: \"local/nomic-embed-text\"\n    }\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n    step_encryption_key: SigningKey\n}\n",
        ),
        (
            "domains/shop/types/note.jadpo",
            "type Note = Text {\n    max_length: 40\n}\n",
        ),
        (
            "domains/shop/types/count.jadpo",
            "type Count = Int {\n    min: 0\n}\n",
        ),
        (
            "domains/shop/types/answer.jadpo",
            "use shop/types { Note }\n\ntype Answer = Output {\n    note: Note\n}\n",
        ),
        (
            "domains/shop/types/meaning.jadpo",
            "type Meaning = Vector {\n    dimensions: 3\n    distance: VectorDistance.cosine\n}\n",
        ),
        (
            "domains/shop/entities/shelf.jadpo",
            "use shop/types { Count, Note }\n\nentity Shelf {\n    id: Uuid\n    note: Note\n    visits: Count\n    closed: Bool\n\n    audit: {}\n\n    lifecycle: {\n        initial: {\n            closed: false\n        }\n        transitions: {\n            close: {\n                set: {\n                    closed: true\n                }\n            }\n        }\n    }\n\n    policy: {\n        create: [Access.public, Access.system]\n        read: [Access.public, Access.system]\n        update: [Access.public, Access.system]\n        delete: [Access.public, Access.system]\n    }\n}\n",
        ),
        (
            "domains/shop/events/shelf_seen.jadpo",
            "use shop/types { Note }\n\ntype ShelfSeen = Event {\n    note: Note\n}\n",
        ),
        (
            "domains/shop/failures/too_far.jadpo",
            "type TooFar = Failure {\n    kind: FailureKind.rejected\n    message: \"That is too far along.\"\n}\n",
        ),
        (
            "domains/shop/failures/no_model.jadpo",
            "type NoModel = Failure {\n    kind: FailureKind.unavailable\n    message: \"The model can't answer now.\"\n}\n",
        ),
        (
            "domains/shop/failures/refused.jadpo",
            "type Refused = Failure {\n    kind: FailureKind.rejected\n    message: \"Refused.\"\n}\n",
        ),
        (
            "domains/shop/prompts/name_it.jadpo",
            "use shop/types { Note }\n\n/// Names a shelf.\ntype NameIt = Prompt {\n    note: Note\n}\n",
        ),
        (
            "domains/shop/queries/open_shelves.jadpo",
            "use shop/entities { Shelf }\n\nquery open_shelves() -> List<Shelf> {\n    from: Shelf\n    order: [Shelf.id asc]\n    limit: 10\n}\n",
        ),
        (
            "domains/shop/types/shelf_page.jadpo",
            "type ShelfPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
        ),
        (
            "domains/shop/queries/paged_shelves.jadpo",
            "use shop/entities { Shelf }\nuse shop/types { ShelfPage }\n\nquery paged_shelves(page: ShelfPage) -> Page<Shelf> {\n    from: Shelf\n    order: [Shelf.id asc]\n    page: {\n        size: page.page_size\n        after: page.after\n    }\n}\n",
        ),
        (
            "domains/shop/actions/add_shelf.jadpo",
            "use shop/entities { Shelf }\nuse shop/types { Note }\n\naction add_shelf(note: Note) -> Shelf {\n    return Shelf.create {\n        set: {\n            note: note\n            visits: 0\n        }\n    }\n}\n",
        ),
        (
            "domains/shop/actions/add_through.jadpo",
            "use shop/actions { add_shelf }\nuse shop/entities { Shelf }\nuse shop/types { Note }\n\naction add_through(note: Note) -> Shelf {\n    return add_shelf(note)\n}\n",
        ),
        (
            "domains/shop/actions/read_through.jadpo",
            "use shop/entities { Shelf }\nuse shop/queries { open_shelves }\n\naction read_through() -> List<Shelf> {\n    return open_shelves()\n}\n",
        ),
        (
            "domains/shop/routes/kept.jadpo",
            "use shop/actions { keep }\nuse shop/entities { Shelf }\nuse shop/types { Answer }\n\nroutes Kept {\n    prefix: /kept\n\n    GET /{id: Shelf.id} {\n        authentication: RouteAuthentication.none\n        deadline: 60s\n        output: Answer\n        run: keep(id)\n    }\n}\n",
        ),
        (
            "policy.jadpo",
            "use shop\n\npolicy {\n    actions: {\n        shop.keep: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/shop/libraries/wording.jadpo",
            "use shop/types { Note }\n\nlibrary wording {\n    function shouted(note: Note) -> Note {\n        return note\n    }\n}\n",
        ),
        (
            &format!("{HOME}/package.jadpo"),
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            &format!("{HOME}/provider.jadpo"),
            "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n}\n",
        ),
        (
            &format!("{HOME}/types/pay_request.jadpo"),
            "/// What each call takes.\npublic type PayRequest = Object {\n    reference: Reference\n}\n",
        ),
        (
            &format!("{HOME}/types/pay_reply.jadpo"),
            "/// What each call gives.\npublic type PayReply = Object {\n    id: Reference\n}\n",
        ),
        (
            &format!("{HOME}/types/reference.jadpo"),
            "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n",
        ),
        (
            &format!("{HOME}/failures/card_declined.jadpo"),
            "/// The card was declined.\npublic type CardDeclined = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was declined.\"\n}\n",
        ),
        // A `GET`, a `POST` marked as changing nothing, a keyed change and
        // a change with no key.
        (
            &format!("{HOME}/resources/invoices.jadpo"),
            "public resource invoices {\n    look: {\n        method: HttpMethod.get\n        path: \"/v1/invoices\"\n        input: PayRequest\n        output: PayReply\n        failures: [CardDeclined]\n        source: PackageSource.written\n    }\n    price: {\n        method: HttpMethod.post\n        path: \"/v1/invoices/price\"\n        input: PayRequest\n        output: PayReply\n        changes: false\n        failures: [CardDeclined]\n        source: PackageSource.written\n    }\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/invoices\"\n        input: PayRequest\n        output: PayReply\n        changes: true\n        idempotent: true\n        failures: [CardDeclined]\n        source: PackageSource.written\n    }\n    send: {\n        method: HttpMethod.post\n        path: \"/v1/invoices/send\"\n        input: PayRequest\n        output: PayReply\n        changes: true\n        idempotent: false\n        failures: [CardDeclined]\n        source: PackageSource.written\n    }\n}\n",
        ),
        // The package's own actions: one only looks, one creates.
        (
            &format!("{HOME}/actions/look_twice.jadpo"),
            "/// Looks an invoice up twice.\npublic action look_twice(input: PayRequest) fails CardDeclined -> PayReply {\n    var first: PayReply = attempt invoices.look(input)\n    return attempt invoices.look(PayRequest {\n        reference: first.id\n    })\n}\n",
        ),
        (
            &format!("{HOME}/actions/start_invoice.jadpo"),
            "/// Starts an invoice.\npublic action start_invoice(input: PayRequest) fails CardDeclined -> PayReply {\n    return attempt invoices.create(input)\n}\n",
        ),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect()
}

const PROMPT: (&str, &str) = (
    "domains/shop/prompts/name_it.prompt",
    "# Instructions\n\nYou name shelves.\n\n# Request\n\n{note}\n",
);

const IMPORTS: &str = "use app { pay, sheets }\nuse shop/actions { add_shelf, add_through, read_through }\nuse shop/entities { Shelf, ShelfNotFound }\nuse shop/events { ShelfSeen }\nuse shop/failures { NoModel, Refused, TooFar }\nuse shop/libraries { wording }\nuse shop/prompts { NameIt }\nuse shop/queries { open_shelves, paged_shelves }\nuse shop/types { Answer, Count, Meaning, Note, ShelfPage }\nuse shared/providers/acme_pay\n\n";

/// `keep`, an action given nothing and returning an `Answer` that a `GET`
/// runs, with `body`.
fn keep(body: &str) -> String {
    format!(
        "{IMPORTS}action keep(id: Shelf.id) fails NoModel, Refused, ShelfNotFound, TooFar -> Answer {{\n{body}\n    return Answer {{\n        note: \"kept\"\n    }}\n}}\n"
    )
}

/// Each problem `files` gives, as `path rule: message`, but what these
/// projects don't keep: the records and what is left unused.
fn problems(files: &[(String, String)]) -> Vec<String> {
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project_with_migrated_records(&db, Layout::Project, &files, &[PROMPT]);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| {
            ![
                "import.unused",
                "layout.unused_declaration",
                "semantic.unused_variable",
                "record.out_of_date",
                // `keep` declares every failure any of its bodies raises.
                "failure.stale_declaration",
            ]
            .contains(&d.rule.id)
        })
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

fn with_keep(keep: &str) -> Vec<(String, String)> {
    let mut files = base();
    files.push((
        "domains/shop/actions/keep.jadpo".to_string(),
        keep.to_string(),
    ));
    files
}

#[track_caller]
fn refused(body: &str, says: &str) {
    let found: Vec<String> = problems(&with_keep(&keep(body)))
        .into_iter()
        .filter(|problem| problem.contains(" route.run_kind:"))
        .collect();
    assert_eq!(found.len(), 1, "{body}: {found:#?}");
    assert!(found[0].contains(says), "{says}: {found:#?}");
}

/// Nothing at all is wrong with it, so nothing went unread: a body that
/// didn't type-check would prove nothing.
#[track_caller]
fn proved(body: &str) {
    assert_eq!(
        problems(&with_keep(&keep(body))),
        Vec::<String>::new(),
        "{body}"
    );
}

#[test]
fn a_write_is_named() {
    refused(
        "    var made: Shelf = Shelf.create {\n        set: {\n            note: \"new\"\n            visits: 0\n        }\n    }\n",
        "`GET /kept/{id}` runs action `keep`, which writes with `Shelf.create`",
    );
}

#[test]
fn an_update_a_delete_a_counter_and_a_transition_are_writes() {
    for (write, name) in [
        (
            "attempt Shelf.update(id) {\n        set: {\n            note: \"new\"\n        }\n    }\n",
            "Shelf.update",
        ),
        ("attempt Shelf.delete(id)\n", "Shelf.delete"),
        (
            "attempt Shelf.update(id) {\n        set: {\n            visits: Count(Shelf.visits + 1)\n        }\n    }\n",
            "Shelf.update",
        ),
        ("attempt Shelf.close(id)\n", "Shelf.close"),
    ] {
        refused(
            &format!("    {write}"),
            &format!("which writes with `{name}`"),
        );
    }
}

#[test]
fn a_write_on_one_way_through_counts() {
    refused(
        "    var made: Bool = clock.now == clock.now\n    if (made) {\n        var shelf: Shelf = add_shelf(\"new\")\n    }\n",
        "through `add_shelf`",
    );
    refused(
        "    var shelves: List<Shelf> = open_shelves()\n    for shelf in shelves {\n        var made: Shelf = Shelf.create {\n            set: {\n                note: shelf.note\n                visits: 0\n            }\n        }\n    }\n",
        "which writes with `Shelf.create`",
    );
}

#[test]
fn a_write_through_calls_names_each_call_and_where_it_is() {
    refused(
        "    var shelf: Shelf = add_through(\"new\")\n",
        "which writes with `Shelf.create` through `add_through`",
    );
    let files = with_keep(&keep("    var shelf: Shelf = add_through(\"new\")\n"));
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project_with_migrated_records(&db, Layout::Project, &files, &[PROMPT]);
    let all = jadpo_check::check(&db, project);
    let problem = all
        .iter()
        .find(|problem| problem.rule.id == "route.run_kind")
        .expect("refused");
    let reason = problem.reason.as_deref().unwrap_or_default();
    assert!(
        reason.contains(
            "`keep` calls `add_through`, and `add_through` calls `add_shelf`, and `add_shelf` writes with `Shelf.create` at `domains/shop/actions/add_shelf.jadpo:5`"
        ),
        "{reason}"
    );
    assert!(
        problem
            .secondary
            .iter()
            .any(|label| label.span.path == "domains/shop/actions/add_shelf.jadpo"),
        "{problem:#?}"
    );
}

#[test]
fn an_emit_is_named_through_calls_too() {
    refused(
        "    emit ShelfSeen {\n        note: \"seen\"\n    }\n",
        "which emits `ShelfSeen`",
    );
}

#[test]
fn starting_or_cancelling_a_run_writes_and_its_status_reads() {
    refused(
        "    var run: BatchRun = batches.each(\n        from: paged_shelves(ShelfPage { page_size: 25 }),\n        each: shelf -> add_shelf(shelf.note),\n        key: id\n    )\n",
        "which writes with `batches.each`",
    );
    let mut files = base();
    files.push((
        "domains/shop/entities/sweep.jadpo".to_string(),
        "entity Sweep {\n    id: Uuid\n    run: BatchRun?\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n".to_string(),
    ));
    let with_call = |call: &str| {
        let mut files = files.clone();
        files.push((
            "domains/shop/actions/keep.jadpo".to_string(),
            format!(
                "{IMPORTS}use shop/entities {{ Sweep, SweepNotFound }}\n\naction keep(id: Sweep.id) fails Refused, SweepNotFound -> Answer {{\n    var sweep: Sweep = attempt Sweep.get(id)\n    if (sweep.run != none) {{\n        {call}\n    }}\n    return Answer {{\n        note: \"kept\"\n    }}\n}}\n"
            ),
        ));
        for (path, text) in &mut files {
            if path == "domains/shop/routes/kept.jadpo" {
                *text = "use shop/actions { keep }\nuse shop/entities { Sweep }\nuse shop/types { Answer }\n\nroutes Kept {\n    prefix: /kept\n\n    GET /{id: Sweep.id} {\n        authentication: RouteAuthentication.none\n        output: Answer\n        run: keep(id)\n    }\n}\n".to_string();
            }
        }
        problems(&files)
    };
    let cancelled = with_call("batches.cancel(sweep.run)");
    let status = with_call("var progress: BatchProgress = batches.status(sweep.run)");
    assert!(
        cancelled
            .iter()
            .any(|problem| problem.contains("which writes with `batches.cancel`")),
        "{cancelled:#?}"
    );
    assert!(
        !status
            .iter()
            .any(|problem| problem.contains(" route.run_kind:")),
        "{status:#?}"
    );
}

#[test]
fn a_write_inside_an_inline_function_counts() {
    let found = problems(&with_keep(&keep(
        "    var made: List<Shelf> = list.map(open_shelves(), shelf -> add_shelf(shelf.note))\n",
    )));
    assert!(
        found
            .iter()
            .any(|problem| problem.contains(" route.run_kind:")),
        "{found:#?}"
    );
}

#[test]
fn a_changing_call_is_named_keyed_or_not() {
    let call = |operation: &str| {
        format!(
            "    var reply: AcmePay.PayReply = attempt pay.invoices.{operation}(AcmePay.PayRequest {{\n        reference: \"a\"\n    }}) {{\n        card_declined: Refused\n    }}\n"
        )
    };
    for operation in ["create", "send"] {
        refused(
            &call(operation),
            &format!("which makes the changing call `pay.invoices.{operation}`"),
        );
    }
    for operation in ["look", "price"] {
        proved(&call(operation));
    }
}

#[test]
fn a_store_s_put_changes_and_its_read_doesn_t() {
    refused(
        "    var file: File = attempt sheets.put(encoding.to_utf8(\"a,b\"), FileType.csv) {\n        storage_full: NoModel\n    }\n",
        "which makes the changing call `sheets.put`",
    );
}

#[test]
fn a_package_action_is_followed() {
    let call = |action: &str| {
        format!(
            "    var reply: AcmePay.PayReply = attempt pay.{action}(AcmePay.PayRequest {{\n        reference: \"a\"\n    }}) {{\n        card_declined: Refused\n    }}\n"
        )
    };
    refused(
        &call("start_invoice"),
        "which makes the changing call `invoices.create` through `AcmePay.start_invoice`",
    );
    proved(&call("look_twice"));
}

#[test]
fn asking_a_model_changes_nothing() {
    for call in [
        "    var meaning: Meaning = attempt ai.embed(\"a shelf\", as: Meaning) {\n        timed_out: NoModel\n        over_budget: NoModel\n        too_long: NoModel\n    }\n",
        "    var named: Note = attempt ai.generate(NameIt {\n        note: \"a shelf\"\n    }, as: Note) {\n        timed_out: NoModel\n        over_budget: NoModel\n        too_long: NoModel\n        refused: NoModel\n        malformed: NoModel\n    }\n",
    ] {
        proved(call);
    }
}

#[test]
fn reading_working_out_and_failing_are_proved() {
    proved(
        "    var shelves: List<Shelf> = open_shelves()\n    var through: List<Shelf> = read_through()\n    if (list.count(shelves) > 5) {\n        reject TooFar {}\n    }\n    if (list.count(through) > 6) {\n        reject Refused {}\n    }\n    var note: Note = wording.shouted(\"a\")\n",
    );
    proved("    var shelf: Shelf = attempt Shelf.get(id)\n");
    proved("    var page: Page<AuditEntry> = Shelf.history(id)\n");
}

#[test]
fn another_method_still_runs_an_action_and_never_a_query() {
    let mut files = base();
    files.retain(|(path, _)| path != "domains/shop/routes/kept.jadpo");
    files.push((
        "domains/shop/actions/keep.jadpo".to_string(),
        "use shop/types { Answer }\n\naction keep() -> Answer {\n    return Answer {\n        note: \"kept\"\n    }\n}\n".to_string(),
    ));
    files.push((
        "domains/shop/routes/kept.jadpo".to_string(),
        "use shop/entities { Shelf }\nuse shop/queries { open_shelves }\n\nroutes Kept {\n    prefix: /kept\n\n    POST / {\n        authentication: RouteAuthentication.none\n        output: List<Shelf>\n        run: open_shelves()\n    }\n}\n".to_string(),
    ));
    let found: Vec<String> = problems(&files)
        .into_iter()
        .filter(|problem| problem.contains(" route.run_kind:"))
        .collect();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("`POST /kept` runs query `open_shelves`"),
        "{found:#?}"
    );
}

#[test]
fn a_read_only_action_sets_off_nothing_to_settle() {
    let mut files = base();
    files.retain(|(path, _)| path != "domains/shop/routes/kept.jadpo");
    files.push((
        "domains/shop/actions/keep.jadpo".to_string(),
        "use shop/types { Answer }\n\naction keep() -> Answer {\n    return Answer {\n        note: \"kept\"\n    }\n}\n".to_string(),
    ));
    files.push((
        "domains/shop/queries/an_answer.jadpo".to_string(),
        "use shop/entities { Shelf }\nuse shop/types { Answer }\n\nquery an_answer(note: Answer) -> List<Shelf> {\n    from: Shelf\n    order: [Shelf.id asc]\n    limit: 1\n}\n".to_string(),
    ));
    files.push((
        "domains/shop/routes/kept.jadpo".to_string(),
        "use shop/actions { keep }\nuse shop/entities { Shelf }\nuse shop/queries { an_answer }\n\nroutes Kept {\n    prefix: /kept\n\n    GET / {\n        authentication: RouteAuthentication.none\n        output: List<Shelf>\n        run: keep()\n        settle: {\n            answer: an_answer(result)\n            within: 1s\n        }\n    }\n}\n".to_string(),
    ));
    let found: Vec<String> = problems(&files)
        .into_iter()
        .filter(|problem| problem.contains(" route.settle_invalid:"))
        .collect();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("runs an action that only reads, which sets off no work"),
        "{found:#?}"
    );
}
