//! A project, as `jadpo check` and `jadpo test` read it: loaded by the
//! loader's own rules ([`jadpo_db::load_files`]), checked, and when
//! nothing blocks running, planned and prepared for the interpreter
//! ([`jadpo_interp::Prepared::load`]). Whatever the files hold, the
//! checker answers with diagnostics, never a panic.
//!
//! An input is one loose file, `main.jadpo`, or a project: files each
//! headed by a line `//// <path>`, the way [`project_text`] writes one.

use jadpo_db::FileSet;

/// The line that heads each file of a project input.
pub const HEADER: &str = "//// ";

pub fn run(text: &str) {
    let files = files(text);
    let loaded = jadpo_db::load_files("fuzzed".into(), &files);
    let _ = jadpo_interp::Prepared::load(&loaded.db, loaded.project);
}

/// The files an input holds.
pub fn files(text: &str) -> FileSet {
    if !text.starts_with(HEADER) {
        return FileSet::of([("main.jadpo", text)]);
    }
    let mut files = FileSet::default();
    for section in text.split(&format!("\n{HEADER}")) {
        let section = section.strip_prefix(HEADER).unwrap_or(section);
        let (path, body) = section.split_once('\n').unwrap_or((section, ""));
        files.insert(path, Ok(body.to_string()));
    }
    files
}

/// A project's files as one input: each headed by its path.
pub fn project_text(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, text)| format!("{HEADER}{path}\n{text}"))
        .collect::<Vec<_>>()
        .join("\n")
}
