//! Projects built in memory, checked through the real pipeline, and run.

#![allow(dead_code)]

use jadpo_db::{Database, File, Layout, Project};
use jadpo_interp::{Failure, Fault, FaultKind, Outcome, Program, Value};

/// The program of `files` as loose files; panics with the check's errors
/// when it doesn't check.
pub fn program(files: &[(&str, &str)]) -> Program {
    let db = Database::default();
    let inputs: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(&db, path.to_string(), text.to_string()))
        .collect();
    let project = Project::new(&db, inputs, Vec::new(), Layout::Loose, Default::default());
    match Program::load(&db, project) {
        Ok(program) => program,
        Err(errors) => panic!(
            "the example doesn't check: {:#?}",
            errors
                .iter()
                .map(|d| format!("{}: {}", d.rule.id, d.message))
                .collect::<Vec<_>>()
        ),
    }
}

/// The program of one file, `main.jadpo`.
pub fn main(source: &str) -> Program {
    program(&[("main.jadpo", source)])
}

pub fn run(program: &Program, name: &str, args: Vec<Value>) -> Outcome {
    program
        .call(name, args)
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// The value a call gives; panics with its report otherwise.
pub fn value(program: &Program, name: &str, args: Vec<Value>) -> Value {
    match run(program, name, args) {
        Outcome::Value(value) => value,
        other => panic!(
            "{name} didn't give a value:\n{}",
            program
                .report(&other)
                .map(|r| r.plain())
                .unwrap_or_default()
        ),
    }
}

/// The failure a call gives; panics otherwise.
pub fn failure(program: &Program, name: &str, args: Vec<Value>) -> Box<Failure> {
    match run(program, name, args) {
        Outcome::Failed(failure) => failure,
        other => panic!("{name} didn't fail: {other:?}"),
    }
}

/// The fault a call ends in; panics otherwise.
pub fn fault(program: &Program, name: &str, args: Vec<Value>) -> Box<Fault> {
    match run(program, name, args) {
        Outcome::Fault(fault) => fault,
        other => panic!("{name} didn't fault: {other:?}"),
    }
}

/// The plain report of a call's outcome.
pub fn report(program: &Program, name: &str, args: Vec<Value>) -> String {
    let outcome = run(program, name, args);
    program
        .report(&outcome)
        .map(|report| report.plain())
        .unwrap_or_else(|| panic!("{name} gave a value: {outcome:?}"))
}

/// The fault's kind and the `file:line` of each frame, innermost first.
pub fn where_fault(program: &Program, fault: &Fault) -> (FaultKind, Vec<String>) {
    let frames = fault
        .frames
        .iter()
        .filter_map(|frame| program.places().place(*frame))
        .map(|place| format!("{}:{}", place.file, place.line))
        .collect();
    (fault.kind.clone(), frames)
}

pub fn int(n: i64) -> Value {
    Value::Int(n)
}

pub fn text(text: &str) -> Value {
    Value::text(text)
}

pub fn decimal(text: &str) -> Value {
    Value::decimal(text).unwrap_or_else(|| panic!("{text} isn't a decimal"))
}

pub fn list(items: Vec<Value>) -> Value {
    Value::list(items)
}
