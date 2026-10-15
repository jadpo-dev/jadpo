//! What an edit rechecks in a project of 2,003 files (docs/compiler-runtime.md
//! §2.4): a body depends only on the declarations it reads, through the
//! per-name lookups, so an edit rechecks the file it is in and nothing that
//! does not read what changed.
//!
//! The project has one shared type, `Count`, used by 2,001 action files,
//! each importing it (area 1, Imports);
//! `act_0` also declares `Special`, which only it uses, and `act_1` an
//! uncalled helper. Each edit is checked twice: by which per-file and
//! per-callable checks ran again, and by how long the whole check took
//! (printed with `--nocapture`; the time is not asserted, the counts are).

mod support;

use std::time::Instant;

use jadpo_db::{Database, File, Layout, Project};
use salsa::Setter;

const ACTIONS: usize = 2001;

/// Every query that checks one file or one callable.
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
];

fn action(k: usize) -> String {
    match k {
        0 => "type Special = Int {\n    min: 0\n}\n\naction act_0(x: Special) -> Special {\n    var y: Special = x\n    return y\n}\n".to_string(),
        1 => "use bench/types { Count }\n\naction act_1(x: Count) -> Count {\n    var y: Count = x\n    return y\n}\n\nfunction helper(x: Count) -> Count {\n    return x\n}\n".to_string(),
        k => format!(
            "use bench/types {{ Count }}\n\naction act_{k}(x: Count) -> Count {{\n    var y: Count = x\n    return y\n}}\n"
        ),
    }
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
    files
}

struct Bench {
    db: Database,
    project: Project,
}

impl Bench {
    fn new() -> Bench {
        let db = Database::recording();
        let files = files();
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let project = support::project_in_path_order(&db, Layout::Project, &borrowed);
        Bench { db, project }
    }

    fn file(&self, path: &str) -> File {
        self.project
            .files(&self.db)
            .iter()
            .copied()
            .find(|file| file.path(&self.db) == path)
            .expect("file")
    }

    /// Checks the project; how long it took, and how many times each
    /// check ran.
    fn check(&mut self, what: &str) -> Vec<(&'static str, usize)> {
        self.db.take_executions();
        let start = Instant::now();
        let problems = jadpo_check::check(&self.db, self.project);
        let took = start.elapsed();
        assert!(
            problems.is_empty(),
            "{what}: {:?}",
            problems
                .iter()
                .map(|d| (d.rule.id, d.message.clone()))
                .collect::<Vec<_>>()
        );
        let log = self.db.take_executions();
        let counts: Vec<(&'static str, usize)> = CHECKS
            .iter()
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
            .filter(|(_, n)| *n > 0)
            .map(|(query, n)| format!("{query} {n}"))
            .collect();
        println!("{what}: {} ms; {}", took.as_millis(), ran.join(", "));
        counts
    }

    fn edit(&mut self, path: &str, from: &str, to: &str) {
        let file = self.file(path);
        let text = file.text(&self.db).replace(from, to);
        file.set_text(&mut self.db).to(text);
    }
}

/// The most times any one per-file check ran.
fn most(counts: &[(&str, usize)]) -> usize {
    counts
        .iter()
        .filter(|(query, _)| *query != "typed_body")
        .map(|(_, n)| *n)
        .max()
        .unwrap_or(0)
}

/// How many callables were checked again.
fn bodies(counts: &[(&str, usize)]) -> usize {
    counts
        .iter()
        .find(|(query, _)| *query == "typed_body")
        .map_or(0, |(_, n)| *n)
}

#[test]
fn an_edit_rechecks_only_what_reads_it() {
    let mut bench = Bench::new();
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

    // An uncalled function's signature: only its own file.
    bench.edit(
        "domains/bench/actions/act_1.jadpo",
        "function helper(x: Count) -> Count {\n    return x\n}",
        "function helper(x: Count, y: Count) -> Count {\n    return y\n}",
    );
    let signature = bench.check("uncalled function's signature");
    // Its file holds `act_1` and the helper.
    assert_eq!(
        (most(&signature), bodies(&signature)),
        (1, 2),
        "{signature:?}"
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
