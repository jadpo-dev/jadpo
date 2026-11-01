//! `jadpo check`: checks the project, found by walking up to `app.jadpo`,
//! or the file or folder named, and prints every problem then one summary
//! line. Outside a project, the files named are checked as loose files: one
//! implicit domain with the layout rules off (area 1). In a project it also
//! writes the derived files in `build/` ([`crate::derived`]).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use jadpo_check::deployables::PerInstance;
use jadpo_db::Layout;
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::derived;
use crate::fmt::project_not_found;
use crate::run::{Call, Reply};

/// Elapsed time joins the summary only when someone would notice it.
pub(crate) const SHOW_TIME_FROM: Duration = Duration::from_millis(100);

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
    let checked = match checked_before(call, &target, JudgedOn::Today) {
        Ok(checked) => checked,
        Err(reply) => return *reply,
    };
    reported(&cwd, checked, started.elapsed(), "check failed")
}

/// The date a rule triggered by time passing is judged at (area 1, "Rules
/// triggered by time passing").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JudgedOn {
    /// Today: what `jadpo check` judges at, so CI fails a new change.
    Today,
    /// The commit's date: what `jadpo build` judges at, so rebuilding a
    /// merged revision is never blocked by a date passing.
    Commit,
}

/// The day `on` names for the project at `root`, in days since
/// 1970-01-01, in UTC: `SOURCE_DATE_EPOCH` when it is set, as everything a
/// build stamps; else today, or for [`JudgedOn::Commit`] the time of the
/// commit checked out, when there is one.
pub(crate) fn judged_day(call: &Call, root: &std::path::Path, on: JudgedOn) -> i32 {
    let seconds = call
        .environment
        .var("SOURCE_DATE_EPOCH")
        .and_then(|epoch| epoch.trim().parse::<i64>().ok())
        .or_else(|| match on {
            JudgedOn::Commit => crate::build::commit_seconds(root),
            JudgedOn::Today => None,
        })
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
        });
    i32::try_from(seconds.div_euclid(86_400)).unwrap_or(0)
}

/// [`checked`], with the before side read from git first, so acceptances
/// are judged (area 18, chunk A8), and the date rules triggered by time
/// passing are judged at, `on`: what `jadpo check` and `jadpo build` run.
/// `--before` names the branch or commit to compare with.
pub(crate) fn checked_before(
    call: &Call,
    target: &std::path::Path,
    on: JudgedOn,
) -> Result<Checked, Box<Reply>> {
    checked_before_as(call, target, on, &rule::CLI_CHECK_ARGUMENTS)
}

/// [`checked_before`], refusing a `--before` git doesn't know with the
/// arguments rule of the command that runs it, `arguments`.
pub(crate) fn checked_before_as(
    call: &Call,
    target: &std::path::Path,
    on: JudgedOn,
    arguments: &'static jadpo_diag::Rule,
) -> Result<Checked, Box<Reply>> {
    let with = call
        .values
        .iter()
        .find(|(flag, _)| *flag == "--before")
        .map(|(_, value)| value.as_str());
    checked_with(call, target, |loaded| {
        let day = judged_day(call, &loaded.root, on);
        loaded.set_judged_on(Some(day));
        if loaded.project.layout(&loaded.db) != Layout::Project {
            return Ok(());
        }
        let has_acceptances = loaded
            .project
            .files(&loaded.db)
            .iter()
            .any(|file| jadpo_check::acceptances::files::is_acceptance(file.path(&loaded.db)));
        match crate::before::read(&loaded.root, with, has_acceptances) {
            Ok(before) => {
                loaded.set_before(before);
                Ok(())
            }
            Err(crate::before::UnknownRef(reference)) => Err(Box::new(Reply::failed(
                Diagnostic::new(
                    arguments,
                    format!("`{reference}` isn't a branch or commit"),
                )
                .reason("`--before` names the branch or commit the change is compared with, and git knows none by this name.")
                .next("Name a branch or commit git knows, such as `--before main`"),
            ))),
        }
    })
}

/// What `jadpo check` shows of `checked`, run from `cwd` in `elapsed`:
/// each diagnostic, then the summary, whose headline is `failed` when
/// there are errors.
pub(crate) fn reported(
    cwd: &std::path::Path,
    checked: Checked,
    elapsed: Duration,
    failed: &str,
) -> Reply {
    let Checked {
        loaded,
        mut diagnostics,
        written,
    } = checked;
    let db = &loaded.db;
    let layout = loaded.project.layout(db);
    let files = loaded.project.files(db);
    let mut sources: BTreeMap<String, String> = jadpo_db::source_texts(db, loaded.project);
    // A linked package's problems are shown in its checkout, where they
    // are fixed.
    if !loaded.links.is_empty() {
        jadpo_db::show_in_checkouts(&loaded.links, &mut diagnostics, &mut sources);
    }
    let paths: Vec<&String> = files.iter().map(|file| file.path(db)).collect();
    let root = jadpo_db::shown_root(cwd, &loaded.root);
    // The scoped review and the unconfirmed calls (chunk 7.2e2), for a
    // project only.
    let reviews: &[jadpo_check::review::ProviderReview] = if layout == Layout::Project {
        jadpo_check::review::review(db, loaded.project)
    } else {
        &[]
    };
    // Each cap on calls a deployable built for Workers counts per instance
    // (area 11's notes, "Decisions", 2026-10-21; chunk 8.1o2).
    let per_instance = if layout == Layout::Project && diagnostics.is_empty() {
        jadpo_check::deployables::per_instance(db, loaded.project)
    } else {
        Vec::new()
    };
    // What the change was compared with (area 18, chunk A8): said only
    // when there was nothing to compare with, in one line.
    let before = match (layout, loaded.project.before(db)) {
        (Layout::Project, jadpo_db::Before::At(side)) => Some(Json::object([(
            "compared_with",
            side.described.as_str().into(),
        )])),
        (Layout::Project, jadpo_db::Before::Nothing(reason)) => {
            Some(Json::object([("not_judged", reason.as_str().into())]))
        }
        _ => None,
    };
    let not_judged = match loaded.project.before(db) {
        jadpo_db::Before::Nothing(reason) if layout == Layout::Project => {
            Some(format!("none, so acceptances aren't judged: {reason}"))
        }
        jadpo_db::Before::At(side) if layout == Layout::Project => {
            let introduced = jadpo_check::acceptances::introduced(db, loaded.project);
            (!introduced.is_empty()).then(|| {
                let names: Vec<String> =
                    introduced.iter().map(|path| format!("`{path}`")).collect();
                format!(
                    "{} {} new since {}, so the changes in {} aren't judged",
                    names.join(" and "),
                    if names.len() == 1 { "is" } else { "are" },
                    side.described,
                    if names.len() == 1 { "it" } else { "them" },
                )
            })
        }
        _ => None,
    };
    report(
        diagnostics,
        sources,
        root,
        &paths,
        loaded.unreadable.len(),
        (layout, written, reviews, &per_instance),
        (elapsed, failed, before, not_judged),
    )
}

/// What `jadpo check` says once it passes of each `max_concurrent:` a
/// deployable built for Workers reaches: Workers runs as many instances as
/// traffic needs, and each counts the cap on its own.
fn per_instance_block(caps: &[PerInstance]) -> Block {
    let mut lines = vec![
        "Workers runs as many instances as traffic needs, and each counts".to_string(),
        "`max_concurrent:` on its own; only the provider's rate limit caps the total.".to_string(),
    ];
    let width = caps
        .iter()
        .map(|cap| cap.connection.len())
        .max()
        .unwrap_or(0);
    for PerInstance {
        connection,
        cap,
        deployable,
    } in caps
    {
        lines.push(format!(
            "  {connection:width$}  {cap} at once in each instance of `{deployable}`"
        ));
    }
    Block::Section {
        heading: "On Workers".to_string(),
        lines,
    }
}

/// A project, or the files and folders named, checked as `jadpo check`
/// checks them, with the derived files written for a project.
pub(crate) struct Checked {
    pub loaded: jadpo_db::Loaded,
    pub diagnostics: Vec<Diagnostic>,
    /// The derived files written, by path from the project's root.
    pub written: Vec<String>,
}

/// Checks `target` as `jadpo check` does, once `prepare` has set what it
/// needs, and writes a project's derived files in `build/`: what `jadpo
/// build` starts from too.
fn checked_with(
    call: &Call,
    target: &std::path::Path,
    prepare: impl FnOnce(&mut jadpo_db::Loaded) -> Result<(), Box<Reply>>,
) -> Result<Checked, Box<Reply>> {
    let mut loaded = jadpo_db::load(target);
    prepare(&mut loaded)?;
    let cache = crate::package_cache::apply(&call.environment, &mut loaded);
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
    let mut checked = jadpo_check::check(db, loaded.project);
    if let Some(cache) = &cache
        && loaded.unreadable.is_empty()
    {
        cache.record(&loaded, &checked);
    }
    if crate::run::in_ci(&call.environment) {
        checked.extend(jadpo_check::packages::links_refused(db, loaded.project));
    }
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
        let openapi = derived::openapi(
            db,
            loaded.project,
            &jadpo_check::routes::openapi::title(db, loaded.project),
        );
        let permissions = jadpo_check::policy::policy_clean(db, loaded.project)
            .then(|| jadpo_check::policy::report::report(db, loaded.project));
        let mut surfaces = derived::surfaces(db, loaded.project);
        surfaces.extend(derived::network(db, loaded.project));
        surfaces.extend(derived::providers(db, loaded.project));
        // Only a project with no errors: where a body doesn't check, the
        // walk can't say what it sends.
        if errors == 0 {
            surfaces.extend(derived::personal_data(db, loaded.project));
        } else {
            derived::remove_personal_data(&loaded.root);
        }
        let monitoring = derived::monitoring(db, loaded.project);
        if let Some(rules) = &monitoring {
            derived::remove_stale_monitoring(&loaded.root, rules);
            surfaces.extend(rules.iter().cloned());
        }
        let stamp = derived::stamp(db, loaded.project, &loaded.root, errors);
        match derived::write(
            &loaded.root,
            graph,
            &stamp,
            &openapi,
            permissions.as_ref(),
            &surfaces,
        ) {
            Ok(paths) => written = paths,
            Err(problem) => diagnostics.push(*problem),
        }
    }
    Ok(Checked {
        loaded,
        diagnostics,
        written,
    })
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
    (layout, written, reviews, per_instance): (
        Layout,
        Vec<String>,
        &[jadpo_check::review::ProviderReview],
        &[PerInstance],
    ),
    (elapsed, failed, before, not_judged): (Duration, &str, Option<Json>, Option<String>),
) -> Reply {
    let errors = diagnostics.len();
    let mut facts = Vec::new();
    if errors > 0 {
        facts.push(Fact::problem(count(errors, "error", "errors")));
    } else {
        // What is left to review and confirm is the agent's work, not a
        // problem: shown only once the check passes.
        facts.extend(crate::provider_review::facts(reviews));
    }
    let total = paths.len() + unreadable;
    let mut files = count(total, "file", "files");
    if elapsed >= SHOW_TIME_FROM {
        files.push_str(&format!(" in {:.1} s", elapsed.as_secs_f64()));
    }
    facts.push(Fact::plain(files));
    let (outcome, headline) = if errors > 0 {
        (Outcome::Failure, failed)
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
    if errors == 0 {
        for block in crate::provider_review::check_blocks(reviews) {
            screen.push(block);
        }
        if !per_instance.is_empty() {
            screen.push(per_instance_block(per_instance));
        }
    }
    if let Some(said) = &not_judged {
        screen.push(Block::Fields(vec![("before".to_string(), said.clone())]));
    }
    screen.push(Block::summary(outcome, headline, facts));
    let mut data = Json::object([
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
        ("providers", crate::provider_review::json(reviews)),
        (
            "per_instance",
            Json::array(per_instance.iter().map(|cap| {
                Json::object([
                    ("connection", cap.connection.as_str().into()),
                    ("max_concurrent", cap.cap.into()),
                    ("deployable", cap.deployable.as_str().into()),
                ])
            })),
        ),
    ]);
    if let (Some(before), Json::Object(fields)) = (before, &mut data) {
        fields.push(("before".to_string(), before));
    }
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_term::Environment;

    fn call(environment: Environment) -> Call {
        Call {
            arguments: Vec::new(),
            flags: Vec::new(),
            values: Vec::new(),
            cwd: None,
            environment,
        }
    }

    fn today() -> i32 {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        i32::try_from(seconds / 86_400).unwrap_or(0)
    }

    #[test]
    fn check_judges_at_today_and_build_at_the_commits_date() {
        let root = std::env::temp_dir().join(format!("jadpo-judged-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("folder");
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .env("GIT_AUTHOR_DATE", "2026-03-01T12:00:00Z")
                .env("GIT_COMMITTER_DATE", "2026-03-01T12:00:00Z")
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.test")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.test")
                .output()
                .expect("git runs")
        };
        let plain = call(Environment::empty());
        // Outside git, a build judges at today too.
        assert_eq!(judged_day(&plain, &root, JudgedOn::Commit), today());
        git(&["init", "-q"]);
        std::fs::write(root.join("app.jadpo"), "application {}\n").expect("write");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "first"]);
        let commit = jadpo_std::date_number("2026-03-01").expect("a date");
        assert_eq!(judged_day(&plain, &root, JudgedOn::Commit), commit);
        assert_eq!(judged_day(&plain, &root, JudgedOn::Today), today());
        // `SOURCE_DATE_EPOCH` fixes both, as it fixes what a build stamps.
        let fixed = call(Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800"));
        let day = jadpo_std::date_number("2026-10-20").expect("a date");
        assert_eq!(judged_day(&fixed, &root, JudgedOn::Commit), day);
        assert_eq!(judged_day(&fixed, &root, JudgedOn::Today), day);
        let _ = std::fs::remove_dir_all(&root);
    }
}
