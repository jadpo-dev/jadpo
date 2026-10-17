//! The guard against holes in the checker's proofs (roadmap chunk 2.7).
//! Compiled code never checks a construction the checker proved, so a
//! proof that passes an invalid value would pass it silently. Here every
//! project of the specification corpus (the language pages' `check`
//! projects and the teaching examples) runs with proofs checked again
//! (`Program::with_proof_checks`): its tests as written, and every
//! callable whose parameters are numbers, text, UUIDs or flags, or lists
//! and sets of them, with values its types allow at and near the edges of
//! their rules. A proven value that
//! breaks a rule is a defect, and so is anything else the checker should
//! have ruled out, so any defect fails the gate.

use crate::syntax_examples as examples;

use jadpo_db::{Database, File, Layout, Project};
use jadpo_graph::Kind;
use jadpo_interp::{FaultKind, Outcome, Program, Value, on_large_stack};

/// At most this many sets of arguments per callable.
const PROBES: usize = 300;

fn load(files: &[(String, String)]) -> Option<Program> {
    let db = Database::default();
    let mut sorted = files.to_vec();
    sorted.sort();
    let inputs: Vec<File> = sorted
        .iter()
        .map(|(path, text)| File::new(&db, path.clone(), text.clone()))
        .collect();
    let layout = if files.iter().any(|(path, _)| path == "app.jadpo") {
        Layout::Project
    } else {
        Layout::Loose
    };
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        layout,
        Default::default(),
        Vec::new(),
    );
    // A project written to show an error doesn't run.
    Program::load(&db, project).ok()
}

/// The defect an outcome ends in, if any.
fn defect(program: &Program, outcome: &Outcome) -> Option<String> {
    let Outcome::Fault(fault) = outcome else {
        return None;
    };
    let FaultKind::Defect(what) = &fault.kind else {
        return None;
    };
    let place = fault
        .frames
        .first()
        .and_then(|frame| program.places().place(*frame))
        .map(|place| format!("{}:{}", place.file, place.line))
        .unwrap_or_default();
    Some(format!("{place} {what}"))
}

fn shown(args: &[Value]) -> String {
    args.iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every defect the project's callables reach, and how many calls ran.
fn defects(name: &str, program: &Program) -> (Vec<String>, usize) {
    let mut found = Vec::new();
    let mut calls = 0;
    let keys: Vec<String> = program.keys().map(|key| key.as_str().to_string()).collect();
    for key in keys {
        let runs: Vec<Vec<Value>> = if key.starts_with(&format!("{} ", Kind::Test.word())) {
            vec![Vec::new()]
        } else {
            program.probe_arguments(&key, PROBES).unwrap_or_default()
        };
        for args in runs {
            calls += 1;
            let Ok(outcome) = program.call(&key, args.clone()) else {
                continue;
            };
            if let Some(defect) = defect(program, &outcome) {
                found.push(format!("{name}: {key}({}): {defect}", shown(&args)));
            }
        }
    }
    (found, calls)
}

#[test]
fn every_proved_construction_in_the_corpus_holds_when_it_runs() {
    let (found, projects, calls) = on_large_stack(|| {
        let mut found = Vec::new();
        let mut projects = 0;
        let mut calls = 0;
        for (name, files) in examples::projects() {
            let Some(program) = load(&files) else {
                continue;
            };
            projects += 1;
            let (mut more, ran) = defects(&name, &program.with_proof_checks());
            found.append(&mut more);
            calls += ran;
        }
        (found, projects, calls)
    })
    .expect("a thread");
    assert!(projects > 40, "the corpus was read: {projects} projects");
    eprintln!("{projects} projects, {calls} calls with proofs checked again");
    assert!(calls > 1000, "the callables were probed: {calls} calls");
    assert!(found.is_empty(), "defects:\n{}", found.join("\n"));
}

#[test]
fn a_proof_hole_would_be_found() {
    // The checker proved `CakeCount(n)` from the condition before it; a
    // value that breaks its rules anyway shows up as a defect. A hole is
    // made here by giving the parameter a value its type refuses, which no
    // checked caller could.
    on_large_stack(|| {
        let files = vec![(
            "main.jadpo".to_string(),
            "type Count = Int {\n    max: 12\n}\n\ntype CakeCount = Int {\n    min: 1\n    max: 12\n}\n\ntype OrderEmpty = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction cakes(n: Count) fails OrderEmpty -> CakeCount {\n    if (n < 1) {\n        reject OrderEmpty\n    }\n    return CakeCount(n)\n}\n"
                .to_string(),
        )];
        let program = load(&files).expect("it checks").with_proof_checks();
        let outcome = program
            .call("cakes", vec![Value::Int(40)])
            .expect("a call");
        let found = defect(&program, &outcome).unwrap_or_default();
        assert!(
            found.contains("proved follows its type's rules, breaking `max: 12`"),
            "{outcome:?}"
        );
        let (none, calls) = defects("cakes", &program);
        assert!(none.is_empty(), "{none:?}");
        assert!(calls > 3, "probed with values `Count` allows: {calls}");
    })
    .expect("a thread");
}

#[test]
fn lists_and_sets_are_probed_at_the_edges_of_their_lengths() {
    on_large_stack(|| {
        let files = vec![(
            "main.jadpo".to_string(),
            "type Tag = Text {\n    max_length: 3\n}\n\ntype Tags = List<Tag> {\n    min_length: 1\n    max_length: 3\n}\n\ntype Labels = Set<Tag> {}\n\ntype Count = Int {}\n\nfunction few(items: Tags, labels: Labels) -> Count {\n    return Count(collection.count(items))\n}\n"
                .to_string(),
        )];
        let program = load(&files).expect("it checks");
        let probes = program.probe_arguments("few", PROBES).expect("probed");
        let lengths: std::collections::BTreeSet<usize> = probes
            .iter()
            .filter_map(|args| args.first()?.items().map(<[Value]>::len))
            .collect();
        assert_eq!(lengths, [1, 2, 3].into(), "{probes:?}");
        assert!(
            probes
                .iter()
                .all(|args| matches!(args.get(1), Some(Value::Set(_)))),
            "{probes:?}"
        );
    })
    .expect("a thread");
}

#[test]
fn named_collection_constructions_are_probed_and_checked_again() {
    // `NameSet(pair)` is proved from `Pair`'s `min_length: 2`, which
    // proves one distinct item, never two: the probes give lists of one
    // name over and over, and a value `Pair` refuses breaks the proof.
    on_large_stack(|| {
        let files = vec![(
            "main.jadpo".to_string(),
            "type Name = Text {\n    max_length: 3\n}\n\ntype Pair = List<Name> {\n    min_length: 2\n}\n\ntype NameSet = Set<Name> {\n    min_length: 1\n}\n\nfunction distinct(pair: Pair) -> NameSet {\n    return NameSet(pair)\n}\n"
                .to_string(),
        )];
        let program = load(&files).expect("it checks").with_proof_checks();
        let probes = program.probe_arguments("distinct", PROBES).expect("probed");
        assert!(
            probes.iter().any(|args| args.first().and_then(Value::items).is_some_and(
                |items| items.len() > 1 && items.iter().all(|item| *item == items[0])
            )),
            "{probes:?}"
        );
        let (none, calls) = defects("distinct", &program);
        assert!(none.is_empty(), "{none:?}");
        assert!(calls > 3, "probed with lists `Pair` allows: {calls}");
        let outcome = program
            .call("distinct", vec![Value::list(Vec::new())])
            .expect("a call");
        let found = defect(&program, &outcome).unwrap_or_default();
        assert!(
            found.contains("proved follows its type's rules, breaking `min_length: 1`"),
            "{outcome:?}"
        );
    })
    .expect("a thread");
}
