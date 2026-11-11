//! Files that aren't Jadpo in a code folder (area 1, "Tests and other
//! files"): each sits beside the declaration that uses it, named after it,
//! so a file in `domains/` or `shared/` that no declaration refers to is
//! `layout.asset_unreferenced`. What refers to one today: an email or an
//! email partial to its `.html` and `.txt` templates, and every email to
//! the shared layout (area 16, "Email"); a prompt to its `.prompt` text and
//! to its recordings in `recordings/<prompt>/` (area 21). A template or a
//! prompt's text beside no declaration of its name is reported by its own
//! rule (`email.template_missing`, `prompt.section_missing`), whose fix
//! declares the type, so it isn't reported again here. A package's files
//! are the package's own (area 11), records are Jadpo's, and a domain's
//! `fixtures/files/` holds the files its tests name with `fixture_file`
//! (area 13), which `semantic.fixture_file_invalid` checks from the test.
//!
//! Design (impeccable's shape pass against the CLI design language, run
//! unattended, so its assumptions are marked):
//!
//! - *Job.* An agent, or the person reviewing it, learns that a file in the
//!   code is read by nothing, a recording left behind by a prompt's rename
//!   or a file of a kind Jadpo never reads, and either puts it where its
//!   declaration is or moves it out. Mode: operate.
//! - The error has the one shape every error has. Its headline names the
//!   file by its own name, the location line its path, with no code frame,
//!   since the file isn't Jadpo and its text says nothing about the
//!   problem. For a recording the `reason` names the prompt its folder
//!   says it records and where that would be, so a misspelt name reads as
//!   one; any other file gets what a code folder may hold.
//! - `next` is the agent's. A recording moves to its prompt's recordings
//!   or goes (assumption: deleting it is safe to suggest, since it is in
//!   git and `jadpo ai record` makes another). A file of no kind Jadpo
//!   reads moves out of the code folder first, so notes aren't lost.

use std::collections::BTreeSet;

use jadpo_db::{Db, Layout, Project, to_upper_camel};
use jadpo_diag::{Diagnostic, Span, rule};

/// One problem for each file in a code folder that no declaration uses.
#[salsa::tracked(returns(ref))]
pub fn asset_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if project.layout(db) != Layout::Project {
        return Vec::new();
    }
    let recorded: BTreeSet<&String> = crate::prompt_recordings::recordings(db, project)
        .iter()
        .filter(|found| crate::prompt_recordings::beside_its_prompt(db, project, found))
        .map(|found| &found.file)
        .collect();
    let recordings = project
        .records(db)
        .iter()
        .map(|record| &record.path)
        .filter(|path| jadpo_db::is_prompt_recording(path) && !recorded.contains(path));
    let mut assets: Vec<&String> = project
        .other_files(db)
        .iter()
        .filter(|path| !jadpo_db::is_package_data(path) && !is_fixture_file(path))
        .chain(recordings)
        .collect();
    assets.sort();
    assets.dedup();
    assets.into_iter().map(|path| unreferenced(path)).collect()
}

/// Whether `path` is in a domain's `fixtures/files/`.
fn is_fixture_file(path: &str) -> bool {
    let mut parts = path.split('/');
    parts.next() == Some("domains")
        && parts.next().is_some()
        && parts.next() == Some("fixtures")
        && parts.next() == Some("files")
        && parts.next().is_some()
}

/// `layout.asset_unreferenced` for the file at `path`.
fn unreferenced(path: &str) -> Diagnostic {
    let file = path.rsplit_once('/').map_or(path, |(_, file)| file);
    let code = path.split('/').next().unwrap_or("domains");
    let (reason, next) = if let Some((prompts, prompt, _)) = jadpo_db::prompt_recording(path) {
        let owner = to_upper_camel(prompt);
        (
            format!(
                "A recording sits in `recordings/<prompt>/` beside the prompt it records, so `{file}` belongs to `{owner}` in `{prompts}/{prompt}.jadpo`, and no prompt there is called that."
            ),
            "Move it to the recordings of the prompt it records, or delete it if nothing will use it"
                .to_string(),
        )
    } else {
        (
            format!(
                "A code folder holds Jadpo and the files its declarations use: an email's templates beside it in `emails/`, and a prompt's text and recordings beside it in `prompts/`. Nothing uses `{file}`."
            ),
            format!("Move it out of `{code}/`, or delete it if nothing needs it"),
        )
    };
    Diagnostic::new(
        &rule::LAYOUT_ASSET_UNREFERENCED,
        format!("`{file}` isn't used by any declaration"),
    )
    .at(Span::new(path, 0, 0), "")
    .reason(reason)
    .next(next)
}
