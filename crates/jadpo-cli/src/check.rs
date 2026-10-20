//! `jadpo check`: checks the project, found by walking up to `app.jadpo`,
//! or the file or folder named, and prints every problem then one summary
//! line. Outside a project, the files named are checked as loose files: one
//! implicit domain with the layout rules off (area 1). In a project it also
//! writes the derived files in `build/` ([`crate::derived`]).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use jadpo_db::Layout;
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::derived;
use crate::fmt::project_not_found;
use crate::run::{Call, Reply};

/// Elapsed time joins the summary only when someone would notice it.
const SHOW_TIME_FROM: Duration = Duration::from_millis(100);

pub fn check(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let target = match call.arguments.first() {
        Some(argument) => {
            let path = cwd.join(argument);
            if !path.exists() {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::CLI_CHECK_ARGUMENTS,
                        format!("`{argument}` doesn't exist"),
                    )
                    .reason("There is no file or folder at this path.")
                    .next("Give the path of a project, a `.jadpo` file or a folder"),
                );
            }
            if path.is_file() && path.extension().and_then(|e| e.to_str()) != Some("jadpo") {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::CLI_CHECK_ARGUMENTS,
                        format!("`{argument}` isn't a Jadpo file"),
                    )
                    .reason("`jadpo check` reads files whose names end in `.jadpo`.")
                    .next("Give a `.jadpo` file, or a folder to check every one in it"),
                );
            }
            path
        }
        None => match jadpo_db::find_root(&cwd) {
            Some(root) => root,
            None => return project_not_found("check"),
        },
    };
    let started = Instant::now();
    let loaded = jadpo_db::load(&target);
    let db = &loaded.db;
    let mut diagnostics: Vec<Diagnostic> = loaded
        .unreadable
        .iter()
        .map(|file| {
            Diagnostic::new(
                &rule::TOOLCHAIN_SOURCE_READ_FAILED,
                format!("`{}` couldn't be read", file.path),
            )
            .at(Span::new(&file.path, 0, 0), "")
            .reason(format!("Reading it failed: {}.", file.error))
            .next("Check it can be read, then run `jadpo check` again")
        })
        .collect();
    let checked = jadpo_check::check(db, loaded.project);
    if diagnostics.is_empty() {
        diagnostics = checked;
    } else {
        // Without every file, the other checks would report knock-on errors.
        diagnostics.extend(
            checked
                .into_iter()
                .filter(|diagnostic| diagnostic.rule.id.starts_with("syntax.")),
        );
    }
    let layout = loaded.project.layout(db);
    let mut written = Vec::new();
    if layout == Layout::Project && loaded.unreadable.is_empty() {
        let errors = diagnostics.len();
        let graph = jadpo_check::graph::graph(db, loaded.project);
        let openapi = derived::openapi(db, loaded.project, &title(&loaded.root));
        let permissions = jadpo_check::policy::policy_clean(db, loaded.project)
            .then(|| jadpo_check::policy::report::report(db, loaded.project));
        let surfaces = derived::surfaces(db, loaded.project);
        match derived::write(
            &loaded.root,
            graph,
            errors,
            &openapi,
            permissions.as_ref(),
            &surfaces,
        ) {
            Ok(paths) => written = paths,
            Err(problem) => diagnostics.push(*problem),
        }
    }
    let elapsed = started.elapsed();
    let files = loaded.project.files(db);
    let sources: BTreeMap<String, String> = jadpo_db::source_texts(db, loaded.project);
    let paths: Vec<&String> = files.iter().map(|file| file.path(db)).collect();
    let root = jadpo_db::shown_root(&cwd, &loaded.root);
    report(
        diagnostics,
        sources,
        root,
        &paths,
        loaded.unreadable.len(),
        (layout, written),
        elapsed,
    )
}

/// The application's name, which titles its OpenAPI documents: its
/// folder's name until `app.jadpo` gives one.
pub(crate) fn title(root: &std::path::Path) -> String {
    root.file_name().map_or_else(
        || "Jadpo application".to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn report(
    diagnostics: Vec<Diagnostic>,
    sources: BTreeMap<String, String>,
    root: Option<String>,
    paths: &[&String],
    unreadable: usize,
    (layout, written): (Layout, Vec<String>),
    elapsed: Duration,
) -> Reply {
    let errors = diagnostics.len();
    let mut facts = Vec::new();
    if errors > 0 {
        facts.push(Fact::problem(count(errors, "error", "errors")));
    }
    let total = paths.len() + unreadable;
    let mut files = count(total, "file", "files");
    if elapsed >= SHOW_TIME_FROM {
        files.push_str(&format!(" in {:.1} s", elapsed.as_secs_f64()));
    }
    facts.push(Fact::plain(files));
    let (outcome, headline) = if errors > 0 {
        (Outcome::Failure, "check failed")
    } else {
        (Outcome::Success, "checked")
    };
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic_from(
            diagnostic,
            &sources,
            root.as_deref(),
        ));
    }
    screen.push(Block::summary(outcome, headline, facts));
    let data = Json::object([
        (
            "layout",
            match layout {
                Layout::Project => "project",
                Layout::Loose => "loose",
            }
            .into(),
        ),
        ("root", Json::from(root.as_deref().unwrap_or("."))),
        ("files", Json::array(paths.iter().map(|path| path.as_str()))),
        ("errors", errors.into()),
        ("build", Json::array(written)),
    ]);
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
}
