//! Recordings of a prompt's answer (area 21, "Tests"; chunk 5.5v4):
//! `prompts/recordings/<prompt>/<name>.json` beside the prompt, each an
//! answer a model gave and the version of the prompt it gave it to. A
//! test's `fakes: { ai.<prompt>: [recorded("<name>")] }` answers with one,
//! and `jadpo test` decodes every one through each type the prompt is
//! asked for as.

use jadpo_db::{Db, Project};

/// One recording, found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Its path in the project.
    pub file: String,
    /// The prompt it records, by its folder's name: `answer_question`.
    pub prompt: String,
    /// Its own name, from its file's: `sesame`.
    pub name: String,
    /// The prompts folder it is in, `domains/support/prompts`.
    pub folder: String,
    /// Its text, or why it couldn't be read.
    pub text: Result<String, String>,
}

/// Where the recording called `name` of the prompt `prompt`, declared in
/// `type_path`, is kept: `domains/support/prompts/recordings/answer_question/sesame.json`.
pub fn path(type_path: &str, prompt: &str, name: &str) -> String {
    let folder = type_path.rsplit_once('/').map_or("", |(folder, _)| folder);
    format!(
        "{folder}/{}/{prompt}/{name}.json",
        jadpo_std::embed::RECORDINGS
    )
}

/// Every recording in the project, in path order.
#[salsa::tracked(returns(ref))]
pub fn recordings(db: &dyn Db, project: Project) -> Vec<Found> {
    let mut out: Vec<Found> = project
        .records(db)
        .iter()
        .filter_map(|record| {
            let (folder, prompt, name) = jadpo_db::prompt_recording(&record.path)?;
            Some(Found {
                file: record.path.clone(),
                prompt: prompt.to_string(),
                name: name.to_string(),
                folder,
                text: record.text.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

/// The names of the recordings of the prompt `prompt`, declared in
/// `type_path`, in name order: what `recorded(...)` may name.
pub fn names_of(db: &dyn Db, project: Project, type_path: &str, prompt: &str) -> Vec<String> {
    let folder = type_path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut names: Vec<String> = recordings(db, project)
        .iter()
        .filter(|found| found.prompt == prompt && found.folder == folder)
        .map(|found| found.name.clone())
        .collect();
    names.sort();
    names
}

/// Whether the recording at `found` sits beside the prompt it is named
/// after: the prompt's own `prompts/` folder.
pub fn beside_its_prompt(db: &dyn Db, project: Project, found: &Found) -> bool {
    crate::prompts::analysis(db, project)
        .prompt(&found.prompt)
        .is_some_and(|facts| {
            facts
                .path
                .rsplit_once('/')
                .is_some_and(|(folder, _)| folder == found.folder)
        })
}
