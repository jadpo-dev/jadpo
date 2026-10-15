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
//! each with a query reading it and an action creating its records. Each
//! edit is checked twice: by which per-file and per-callable checks ran
//! again, and by how long checking and building the graph took (printed
//! with `--nocapture`; the time is not asserted, the counts are).

use crate::support;

use std::collections::BTreeMap;
use std::time::Instant;

use jadpo_db::{Database, File, Layout, Project};
use salsa::Setter;

const ACTIONS: usize = 2001;
const ENTITIES: usize = 100;

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
];

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
    files
}

struct Bench {
    db: Database,
    project: Project,
    /// Whether the entities have changed since the migration, which
    /// `jadpo check` reports and nothing else may.
    stale: bool,
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
            .map(|d| (d.rule.id, d.message.clone()))
            .collect();
        assert!(unexpected.is_empty(), "{what}: {unexpected:?}");
        let log = self.db.take_executions();
        let counts: Counts = CHECKS
            .iter()
            .chain(DATA)
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
    // The migration that creates the entities' tables, so the checks
    // below aren't told the migrations are out of date.
    let scratch = Database::default();
    let files = files();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let records = support::project(&scratch, Layout::Project, &borrowed)
        .records(&scratch)
        .clone();
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
