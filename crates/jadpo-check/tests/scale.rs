//! What an edit rechecks in a project of 2,303 files (docs/compiler-runtime.md
//! §2.4): a body depends only on the declarations it reads, through the
//! per-name lookups, so an edit rechecks the file it is in and nothing that
//! does not read what changed. Entities and queries are read the same way:
//! each by name, into a model without positions, so a comment moves no
//! model and an entity's change reaches only what reads that entity.
//!
//! The project has one shared type, `Count`, used by 2,001 action files,
//! each importing it (area 1, Imports);
//! `act_0` also declares `Special`, which only it uses, and `act_1` a
//! helper only it calls. Beside them are 100 entities, `Item0` to `Item99`,
//! each with a query reading it and an action creating its records. Since
//! chunk 4.11 it has areas 7 to 9 too: a principal, `Member`, signing in
//! with a browser session; 10 entities with policies, `Note0` to `Note9`,
//! each with a field only its owner reads, a query, an action creating its
//! records and a route group running both, with two tests each, one of
//! which sends a request; and a route anyone may call, listed under root
//! `policy { actions: }`. Each edit is checked twice: by which per-file
//! and per-callable checks ran again, and by how long checking and
//! building the graph took (printed with `--nocapture`; the time is not
//! asserted, the counts are).

use crate::support;

use std::collections::BTreeMap;
use std::time::Instant;

use jadpo_db::{Database, File, Layout, Project, Record};
use salsa::Setter;

const ACTIONS: usize = 2001;
const ENTITIES: usize = 100;
const NOTES: usize = 10;

/// Every query that checks one file or one callable, or builds one file's
/// part of the graph.
const CHECKS: &[&str] = &[
    "typed_body",
    "layout_problems",
    "naming_problems",
    "resolve_file",
    "import_problems",
    "usage_problems",
    "type_problems",
    "failure_problems",
    "body_problems",
    "recursion_problems",
    "similar_problems",
    "store_problems",
    "data_problems",
    "query_problems",
    "file_part",
    "route_problems",
    "reach_problems",
    "policy_problems",
    "query_policy_problems",
    "flow_problems",
];

/// The queries that read one callable's body for areas 6 to 9: what it
/// reads, writes and calls, and what its values carry to a caller.
const CALLABLES: &[&str] = &["body_facts", "flow_summary"];

/// The queries that read entities and queries, by name or whole.
const DATA: &[&str] = &[
    "entity_read",
    "entity_model",
    "query_read",
    "query_model",
    "write_rules",
    "data_model",
    "query_models",
    "schema",
    "migration_problems",
];

fn action(k: usize) -> String {
    match k {
        0 => "type Special = Int {\n    min: 0\n}\n\naction act_0(x: Special) -> Special {\n    var y: Special = x\n    return y\n}\n".to_string(),
        1 => "use bench/types { Count }\n\naction act_1(x: Count) -> Count {\n    var y: Count = helper(x)\n    return y\n}\n\nfunction helper(x: Count) -> Count {\n    return x\n}\n".to_string(),
        k => format!(
            "use bench/types {{ Count }}\n\naction act_{k}(x: Count) -> Count {{\n    var y: Count = x\n    return y\n}}\n"
        ),
    }
}

fn entity(k: usize) -> String {
    format!(
        "entity Item{k} {{\n    id: Uuid\n    title: Text {{\n        max_length: 100\n    }}\n    rank: Int\n}}\n"
    )
}

fn query(k: usize) -> String {
    format!(
        "use bench/entities {{ Item{k} }}\n\nquery items_{k}(rank: Item{k}.rank) -> List<Item{k}> {{\n    from: Item{k}\n    where: Item{k}.rank == rank\n    order: [Item{k}.title asc]\n    limit: 50\n}}\n"
    )
}

fn writer(k: usize) -> String {
    format!(
        "use bench/entities {{ Item{k} }}\n\naction add_item_{k}(title: Item{k}.title) -> Item{k} {{\n    return Item{k}.create {{\n        set: {{\n            title: title\n            rank: 0\n        }}\n    }}\n}}\n"
    )
}

/// Areas 7 to 9's root files and shared declarations: the principal and
/// its sign-in, root `policy { }` listing `echo`, and the route anyone may
/// call, with a test that sends it a request.
fn identity_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "authentication.jadpo",
            "use bench/entities { Member }\n\nprincipal {\n    user: {\n        subject: Member.handle\n    }\n}\n\nauthentication {\n    browser_session: {\n        cookie: \"bench_session\"\n    }\n}\n",
        ),
        (
            "policy.jadpo",
            "use bench\n\npolicy {\n    actions: {\n        bench.echo: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/bench/types/handle.jadpo",
            "type Handle = Text {\n    max_length: 40\n}\n",
        ),
        (
            "domains/bench/types/note_text.jadpo",
            "type NoteText = Text {\n    max_length: 200\n}\n",
        ),
        (
            "domains/bench/types/new_note.jadpo",
            "use bench/types { NoteText }\n\ntype NewNote = Input {\n    text: NoteText\n}\n",
        ),
        (
            "domains/bench/types/echo_input.jadpo",
            "use bench/types { Count }\n\ntype EchoInput = Input {\n    count: Count\n}\n",
        ),
        (
            "domains/bench/types/echo_view.jadpo",
            "use bench/types { Count }\n\ntype EchoView = Output {\n    count: Count\n}\n",
        ),
        (
            "domains/bench/failures/handle_taken.jadpo",
            "type HandleTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That handle is taken.\"\n}\n",
        ),
        (
            "domains/bench/entities/member.jadpo",
            "use bench/types { Handle }\n\nentity Member {\n    id: Uuid\n    handle: Handle {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n    }\n}\n",
        ),
        (
            "domains/bench/actions/join.jadpo",
            "use bench/entities { Member }\nuse bench/types { Handle }\nuse bench/failures { HandleTaken }\n\naction join(handle: Handle) fails HandleTaken -> Member {\n    return attempt Member.create {\n        set: {\n            handle: handle\n        }\n        conflict: {\n            Member.handle: HandleTaken\n        }\n    }\n}\n",
        ),
        (
            "domains/bench/actions/echo.jadpo",
            "use bench/types { EchoInput, EchoView }\n\naction echo(input: EchoInput) -> EchoView {\n    return EchoView {\n        count: input.count\n    }\n}\n",
        ),
        (
            "domains/bench/routes/echoes.jadpo",
            "use bench/types { EchoInput, EchoView }\nuse bench/actions { echo }\n\nroutes Echoes {\n    prefix: /echoes\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: EchoInput\n        output: EchoView\n        run: echo(input)\n    }\n}\n",
        ),
        (
            "domains/bench/routes/echoes.test.jadpo",
            "use bench/types { EchoInput }\n\ntest \"an echo answers\" {\n    var echoed: Response = request {\n        method: HttpMethod.post\n        path: \"/echoes\"\n        input: EchoInput {\n            count: 1\n        }\n    }\n    assert echoed.status == 200\n}\n",
        ),
    ]
}

/// A note only its writer may create and change, whose `secret` only its
/// writer reads.
fn note(k: usize) -> String {
    format!(
        "use bench/entities {{ Member }}\nuse bench/types {{ NoteText }}\n\nentity Note{k} {{\n    id: Uuid\n    owner_id: Member.id {{\n        on_delete: OnDelete.cascade\n    }}\n    text: NoteText\n    secret: NoteText {{\n        policy: {{\n            read: [owner_id]\n        }}\n    }}\n\n    policy: {{\n        create: [owner_id]\n        read: [Access.authenticated]\n        update: [owner_id]\n    }}\n}}\n"
    )
}

fn note_view(k: usize) -> String {
    format!(
        "use bench/entities {{ Note{k} }}\n\ntype NoteView{k} = Output {{\n    pick: [Note{k}.id, Note{k}.text]\n}}\n"
    )
}

fn notes_query(k: usize) -> String {
    format!(
        "use bench/entities {{ Note{k} }}\nuse bench/types {{ NoteView{k} }}\n\nquery notes_{k}() -> List<NoteView{k}> {{\n    from: Note{k}\n    order: [Note{k}.text asc]\n    limit: 50\n}}\n"
    )
}

fn note_writer(k: usize) -> String {
    format!(
        "use bench/entities {{ Note{k} }}\nuse bench/types {{ NewNote, NoteView{k} }}\n\naction add_note_{k}(input: NewNote, writer: Principal.user) -> NoteView{k} {{\n    var note: Note{k} = Note{k}.create {{\n        set: {{\n            owner_id: writer.id\n            text: input.text\n            secret: input.text\n        }}\n    }}\n    return NoteView{k} {{\n        id: note.id\n        text: note.text\n    }}\n}}\n"
    )
}

fn note_routes(k: usize) -> String {
    format!(
        "use bench/types {{ NewNote, NoteView{k} }}\nuse bench/actions {{ add_note_{k} }}\nuse bench/queries {{ notes_{k} }}\n\nroutes Notes{k} {{\n    prefix: /notes{k}\n\n    GET / {{\n        output: List<NoteView{k}>\n        run: notes_{k}()\n    }}\n\n    POST / {{\n        input: NewNote\n        output: NoteView{k}\n        status: SuccessStatus.created\n        run: add_note_{k}(input, current_principal.user)\n    }}\n}}\n"
    )
}

/// Two tests: one sends its routes a request, the other doesn't.
fn note_tests(k: usize) -> String {
    format!(
        "use bench/entities {{ Member }}\nuse bench/types {{ NewNote }}\nuse bench/actions {{ join }}\n\ntest \"a member writes note {k}\" {{\n    var dee: Member = attempt join(\"dee\")\n    var written: Response = request {{\n        method: HttpMethod.post\n        path: \"/notes{k}\"\n        as: Principal.user(dee)\n        input: NewNote {{\n            text: \"hello\"\n        }}\n    }}\n    assert written.status == 201\n}}\n\ntest \"a member joins to write note {k}\" {{\n    var dee: Member = attempt join(\"dee\")\n    assert dee.handle == \"dee\"\n}}\n"
    )
}

fn files() -> Vec<(String, String)> {
    let mut files = vec![
        ("app.jadpo".to_string(), "application {}\n".to_string()),
        (
            "domains/bench/types/count.jadpo".to_string(),
            "type Count = Int {\n    min: 0\n}\n".to_string(),
        ),
    ];
    for k in 0..ACTIONS {
        files.push((format!("domains/bench/actions/act_{k}.jadpo"), action(k)));
    }
    for k in 0..ENTITIES {
        files.push((format!("domains/bench/entities/item{k}.jadpo"), entity(k)));
        files.push((format!("domains/bench/queries/items_{k}.jadpo"), query(k)));
        files.push((
            format!("domains/bench/actions/add_item_{k}.jadpo"),
            writer(k),
        ));
    }
    for (path, text) in identity_files() {
        files.push((path.to_string(), text.to_string()));
    }
    for k in 0..NOTES {
        files.push((format!("domains/bench/entities/note{k}.jadpo"), note(k)));
        files.push((
            format!("domains/bench/types/note_view{k}.jadpo"),
            note_view(k),
        ));
        files.push((
            format!("domains/bench/queries/notes_{k}.jadpo"),
            notes_query(k),
        ));
        files.push((
            format!("domains/bench/actions/add_note_{k}.jadpo"),
            note_writer(k),
        ));
        files.push((
            format!("domains/bench/routes/notes{k}.jadpo"),
            note_routes(k),
        ));
        files.push((
            format!("domains/bench/routes/notes{k}.test.jadpo"),
            note_tests(k),
        ));
    }
    files
}

struct Bench {
    db: Database,
    project: Project,
    /// Whether the entities have changed since the migration, which
    /// `jadpo check` reports and nothing else may.
    stale: bool,
    /// Whether the routes have changed since the API record, likewise.
    stale_api: bool,
}

/// How many times each query ran.
type Counts = BTreeMap<&'static str, usize>;

impl Bench {
    fn new() -> Bench {
        let db = Database::recording();
        let files = files();
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let project = support::project_in_path_order(&db, Layout::Project, &borrowed);
        Bench {
            db,
            project,
            stale: false,
            stale_api: false,
        }
    }

    fn file(&self, path: &str) -> File {
        self.project
            .files(&self.db)
            .iter()
            .copied()
            .find(|file| file.path(&self.db) == path)
            .expect("file")
    }

    /// Checks the project and builds its graph; how long each took, and
    /// how many times each check ran.
    fn check(&mut self, what: &str) -> Counts {
        self.db.take_executions();
        let start = Instant::now();
        let problems = jadpo_check::check(&self.db, self.project);
        let checked = start.elapsed();
        let start = Instant::now();
        jadpo_check::graph::graph(&self.db, self.project);
        let built = start.elapsed();
        let unexpected: Vec<_> = problems
            .iter()
            .filter(|d| !(self.stale && d.rule.id == "migration.out_of_date"))
            .filter(|d| !(self.stale_api && d.rule.id == "api.record_out_of_date"))
            .map(|d| (d.rule.id, d.message.clone()))
            .collect();
        assert!(unexpected.is_empty(), "{what}: {unexpected:?}");
        let log = self.db.take_executions();
        let counts: Counts = CHECKS
            .iter()
            .chain(DATA)
            .chain(CALLABLES)
            .map(|query| {
                let prefix = format!("{query}(");
                (
                    *query,
                    log.iter().filter(|key| key.starts_with(&prefix)).count(),
                )
            })
            .collect();
        let ran: Vec<String> = counts
            .iter()
            .filter(|(_, n)| **n > 0)
            .map(|(query, n)| format!("{query} {n}"))
            .collect();
        println!(
            "{what}: check {} ms, graph {} ms; {}",
            checked.as_millis(),
            built.as_millis(),
            ran.join(", ")
        );
        counts
    }

    fn edit(&mut self, path: &str, from: &str, to: &str) {
        let file = self.file(path);
        let text = file.text(&self.db).replace(from, to);
        assert_ne!(&text, file.text(&self.db), "{path}: no `{from}`");
        file.set_text(&mut self.db).to(text);
    }
}

/// The most times any one per-file check ran.
fn most(counts: &Counts) -> usize {
    CHECKS
        .iter()
        .filter(|query| **query != "typed_body")
        .map(|query| counts[query])
        .max()
        .unwrap_or(0)
}

/// How many callables were checked again.
fn bodies(counts: &Counts) -> usize {
    counts["typed_body"]
}

/// How many times each of `queries` ran.
fn ran(counts: &Counts, queries: &[&'static str]) -> Vec<(&'static str, usize)> {
    queries
        .iter()
        .map(|query| (*query, counts[query]))
        .collect()
}

#[test]
fn an_edit_rechecks_only_what_reads_it() {
    let mut bench = Bench::new();
    // The migration that creates the entities' tables and the API's
    // record, so the checks below aren't told either is out of date.
    let scratch = Database::default();
    let files = files();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let written = support::project(&scratch, Layout::Project, &borrowed);
    let mut records = written.records(&scratch).clone();
    for (api, body) in jadpo_check::routes::record::record_bodies(&scratch, written) {
        records.push(Record {
            path: jadpo_check::routes::record::path(api),
            text: Ok(jadpo_check::routes::record::file(body, 1)),
        });
    }
    bench.project.set_records(&mut bench.db).to(records);

    let cold = bench.check("cold");
    assert!(
        most(&cold) >= ACTIONS,
        "every file is checked once: {cold:?}"
    );

    // A body edit rechecks its own file.
    bench.edit(
        "domains/bench/actions/act_5.jadpo",
        "    return y\n",
        "    var z: Count = y\n    return z\n",
    );
    let body = bench.check("body edit");
    assert_eq!((most(&body), bodies(&body)), (1, 1), "{body:?}");

    // A rule on a type only one file uses rechecks that file.
    bench.edit(
        "domains/bench/actions/act_0.jadpo",
        "    min: 0\n",
        "    min: 0\n    max: 99\n",
    );
    let rule = bench.check("rule on a type one file uses");
    assert_eq!((most(&rule), bodies(&rule)), (1, 1), "{rule:?}");

    // The signature of a helper only its own file calls, with that call:
    // only its own file.
    bench.edit(
        "domains/bench/actions/act_1.jadpo",
        "helper(x)\n    return y\n}\n\nfunction helper(x: Count) -> Count {\n    return x\n}",
        "helper(x, x)\n    return y\n}\n\nfunction helper(x: Count, y: Count) -> Count {\n    return y\n}",
    );
    let signature = bench.check("signature of a helper one file calls");
    // Its file holds `act_1` and the helper.
    assert_eq!(
        (most(&signature), bodies(&signature)),
        (1, 2),
        "{signature:?}"
    );

    // A comment above an entity moves its positions and nothing in its
    // model: only its own file is checked again, and nothing that reads
    // the entity.
    bench.edit(
        "domains/bench/entities/item7.jadpo",
        "entity Item7",
        "// Kept for the audit.\nentity Item7",
    );
    let comment = bench.check("comment above an entity");
    assert_eq!((most(&comment), bodies(&comment)), (1, 0), "{comment:?}");
    assert_eq!(
        ran(
            &comment,
            &[
                "entity_read",
                "query_read",
                "write_rules",
                "data_model",
                "schema"
            ]
        ),
        [
            ("entity_read", 1),
            ("query_read", 0),
            ("write_rules", 0),
            ("data_model", 0),
            ("schema", 0)
        ],
        "{comment:?}"
    );

    // A comment above a query: only its own file.
    bench.edit(
        "domains/bench/queries/items_7.jadpo",
        "query items_7",
        "// The newest first would need an index of its own.\nquery items_7",
    );
    let comment = bench.check("comment above a query");
    assert_eq!((most(&comment), bodies(&comment)), (1, 0), "{comment:?}");
    assert_eq!(
        ran(
            &comment,
            &["entity_read", "query_read", "query_models", "schema"]
        ),
        [
            ("entity_read", 0),
            ("query_read", 1),
            ("query_models", 0),
            ("schema", 0)
        ],
        "{comment:?}"
    );

    // A body edit in an action that writes an entity rechecks that
    // action, and reads the entity's write rules from before.
    bench.edit(
        "domains/bench/actions/add_item_7.jadpo",
        "rank: 0",
        "rank: 1",
    );
    let write = bench.check("body edit in an action that writes");
    assert_eq!((most(&write), bodies(&write)), (1, 1), "{write:?}");
    assert_eq!(
        ran(&write, &["entity_read", "query_read", "write_rules"]),
        [("entity_read", 0), ("query_read", 0), ("write_rules", 0)],
        "{write:?}"
    );

    // A field's rule on an entity: the three files that name `Item7` (the
    // entity, its query and the action that creates its records, whose
    // signatures read its fields), the one body that writes it, and the
    // schema; no other entity's checks. The migrations are out of date
    // from here on.
    bench.edit(
        "domains/bench/entities/item7.jadpo",
        "max_length: 100",
        "max_length: 120",
    );
    bench.stale = true;
    let field = bench.check("field rule on an entity");
    assert_eq!((most(&field), bodies(&field)), (3, 1), "{field:?}");
    assert_eq!(
        ran(
            &field,
            &[
                "entity_read",
                "query_read",
                "write_rules",
                "data_problems",
                "query_problems",
                "schema"
            ]
        ),
        [
            ("entity_read", 1),
            ("query_read", 1),
            ("write_rules", 1),
            ("data_problems", 1),
            ("query_problems", 0),
            ("schema", 1)
        ],
        "{field:?}"
    );

    // Areas 7 to 9 (chunk 4.11). A comment above an action that writes a
    // note moves its positions and nothing it reads, writes or carries:
    // its own file's checks run again, flow included, and no route's,
    // query's or other action's. Before, the flow check walked every
    // action and function in the project again.
    const PHASE_4: &[&str] = &[
        "typed_body",
        "body_facts",
        "flow_summary",
        "flow_problems",
        "reach_problems",
        "query_policy_problems",
        "policy_problems",
        "route_problems",
    ];
    bench.edit(
        "domains/bench/actions/add_note_3.jadpo",
        "action add_note_3",
        "// The writer is the signed-in member.\naction add_note_3",
    );
    let comment = bench.check("comment above an action that writes a note");
    assert_eq!(most(&comment), 1, "{comment:?}");
    assert_eq!(
        ran(&comment, PHASE_4),
        [
            ("typed_body", 1),
            ("body_facts", 1),
            ("flow_summary", 0),
            ("flow_problems", 1),
            ("reach_problems", 0),
            ("query_policy_problems", 0),
            ("policy_problems", 0),
            ("route_problems", 0)
        ],
        "{comment:?}"
    );

    // A field's own policy on one entity: the checks that read that
    // entity's policy (its own file's, its query's, the flow of the
    // action that creates its records and the routes that reach it), and
    // no body. Before, every query in the project was checked again (110
    // files), with every route and every action's flow.
    bench.edit(
        "domains/bench/entities/note3.jadpo",
        "            read: [owner_id]\n",
        "            read: [owner_id]\n            update: [owner_id]\n",
    );
    let policy = bench.check("a field's policy on one entity");
    assert_eq!(most(&policy), 1, "{policy:?}");
    assert_eq!(
        ran(&policy, PHASE_4),
        [
            ("typed_body", 0),
            ("body_facts", 0),
            ("flow_summary", 0),
            ("flow_problems", 1),
            ("reach_problems", 1),
            ("query_policy_problems", 1),
            ("policy_problems", 1),
            ("route_problems", 0)
        ],
        "{policy:?}"
    );

    // A route no test sends a request to: its own file's route checks,
    // and no test body. Before, all 21 test bodies were checked again.
    bench.edit(
        "domains/bench/routes/notes5.jadpo",
        "    GET / {\n",
        "    /// Every note the caller may read.\n    GET / {\n",
    );
    bench.stale_api = true;
    let unrequested = bench.check("a route no test requests");
    assert_eq!(most(&unrequested), 1, "{unrequested:?}");
    assert_eq!(
        ran(
            &unrequested,
            &["typed_body", "route_problems", "reach_problems"]
        ),
        [
            ("typed_body", 0),
            ("route_problems", 1),
            ("reach_problems", 1)
        ],
        "{unrequested:?}"
    );

    // The route one test sends a request to, now needing a signed-in
    // caller: that test's body alone.
    bench.edit(
        "domains/bench/routes/echoes.jadpo",
        "        authentication: RouteAuthentication.none\n",
        "",
    );
    let requested = bench.check("the route one test requests");
    assert_eq!(most(&requested), 1, "{requested:?}");
    assert_eq!(
        ran(
            &requested,
            &["typed_body", "route_problems", "reach_problems"]
        ),
        [
            ("typed_body", 1),
            ("route_problems", 1),
            ("reach_problems", 1)
        ],
        "{requested:?}"
    );

    // A new file that reads nothing the others declare and that nothing
    // reads: only the new file is checked.
    let mut files = bench.project.files(&bench.db).clone();
    files.push(File::new(
        &bench.db,
        "domains/bench/actions/act_new.jadpo".to_string(),
        "type Fresh = Int {\n    min: 0\n}\n\naction act_new(x: Fresh) -> Fresh {\n    return x\n}\n"
            .to_string(),
    ));
    files.sort_by(|a, b| a.path(&bench.db).cmp(b.path(&bench.db)));
    bench.project.set_files(&mut bench.db).to(files);
    let added = bench.check("new unrelated file");
    assert_eq!((most(&added), bodies(&added)), (1, 1), "{added:?}");

    // And a rule on the type every file uses rechecks every body, which
    // shows the counts above are what the lookups decide, not luck.
    bench.edit(
        "domains/bench/types/count.jadpo",
        "    min: 0\n",
        "    min: 0\n    max: 99\n",
    );
    let shared = bench.check("rule on the type every file uses");
    assert!(bodies(&shared) >= ACTIONS, "{shared:?}");
}
