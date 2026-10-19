//! `jadpo api update` (areas 7 and 18): rewrites each API's committed
//! record, `api/<api>.jadpo_record`, from the routes, removes the record
//! of an API left with no routes, and removes a record still under its
//! old name, `api/<api>.api`. Every change to the API is then a diff of
//! one file.
//!
//! The screen is a status line per record, as `jadpo migrate new` writes
//! its folder: `✓ Wrote api/main.jadpo_record` with the OpenAPI version it
//! gives and the entries that changed, or that it was already up to date.

use std::path::PathBuf;

use jadpo_check::routes::record;
use jadpo_db::record as record_format;
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::fmt::project_not_found;
use crate::migrate::blocked;
use crate::run::{Call, Reply};

pub fn api(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        Some("update") => match call.arguments.get(1) {
            Some(extra) => Reply::failed(arguments(
                format!("`jadpo api update` takes nothing after it, not `{extra}`"),
                "It rewrites every API's record at once.".to_string(),
                "Run `jadpo api update`".to_string(),
            )),
            None => update(call),
        },
        Some(other) => Reply::failed(arguments(
            format!("`{other}` isn't something `jadpo api` does"),
            "`jadpo api update` rewrites each API's record in `api/` from the routes.".to_string(),
            "Run `jadpo api update`".to_string(),
        )),
        None => Reply::failed(arguments(
            "`jadpo api` needs to know what to do".to_string(),
            "`jadpo api update` rewrites each API's record in `api/` from the routes.".to_string(),
            "Run `jadpo api update`".to_string(),
        )),
    }
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_API_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

fn update(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("api");
    };
    let loaded = jadpo_db::load(&root);
    let (db, project) = (&loaded.db, loaded.project);
    // A record is written from routes that read cleanly; any other problem
    // in them stops it, as in `jadpo check`.
    let problems: Vec<Diagnostic> = jadpo_check::check(db, project)
        .into_iter()
        .filter(|d| {
            (d.rule.area == 7 || d.rule.id.starts_with("syntax."))
                && d.rule.area != 18
        })
        .collect();
    if !problems.is_empty() || !jadpo_check::routes::routes_clean(db, project) {
        let problems = if problems.is_empty() {
            jadpo_check::check(db, project)
        } else {
            problems
        };
        return blocked(&loaded, &cwd, problems, "api update stopped");
    }
    let texts = record::record_texts(db, project);
    let written: Vec<(String, String)> = project
        .records(db)
        .iter()
        .filter(|r| {
            r.path.starts_with("api/")
                && (r.path.ends_with(record_format::EXTENSION) || r.path.ends_with(".api"))
        })
        .map(|r| (r.path.clone(), r.text.clone().unwrap_or_default()))
        .collect();
    let mut screen = Screen::new();
    let mut changed = 0usize;
    let mut entries = Vec::new();
    for (api, text) in texts {
        let path = record::path(api);
        let version = record::openapi_version(text);
        let before = written.iter().find(|(p, _)| *p == path).map(|(_, t)| t);
        let outcome = match before {
            Some(old) if record_format::normalise(old) == text.as_str() => "unchanged",
            Some(_) => "updated",
            None => "written",
        };
        let mut rows = vec![("version".to_string(), version.clone())];
        if let Some(old) = before
            && outcome == "updated"
        {
            for (change, heading) in
                jadpo_check::records::entry_changes(&record_format::API, old, text)
            {
                rows.push((change.word().to_string(), heading));
            }
        }
        if outcome != "unchanged" {
            if let Err(error) = write(&root.join(&path), text) {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                        format!("`{path}` couldn't be written"),
                    )
                    .reason(format!("Writing it failed: {error}."))
                    .next("Check that `api/` can be written, then run `jadpo api update` again"),
                );
            }
            changed += 1;
        }
        let shown = match outcome {
            "unchanged" => format!("{path} is up to date"),
            _ => format!("Wrote {path}"),
        };
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: shown,
            rows,
        });
        entries.push(Json::object([
            ("path", Json::from(path.as_str())),
            ("api", Json::from(api.as_str())),
            ("version", Json::from(version.as_str())),
            ("change", outcome.into()),
        ]));
    }
    for (path, _) in &written {
        let api = path
            .trim_start_matches("api/")
            .trim_end_matches(record_format::EXTENSION)
            .trim_end_matches(".api");
        let old_name = !path.ends_with(record_format::EXTENSION);
        if !old_name && texts.contains_key(api) {
            continue;
        }
        if let Err(error) = std::fs::remove_file(root.join(path)) {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{path}` couldn't be removed"),
                )
                .reason(format!("Removing it failed: {error}."))
                .next("Remove it by hand, then run `jadpo api update` again"),
            );
        }
        changed += 1;
        let reason = if old_name {
            "the record's old name"
        } else {
            "no route belongs to its API"
        };
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: format!("Removed {path}"),
            rows: vec![("reason".to_string(), reason.to_string())],
        });
        entries.push(Json::object([
            ("path", Json::from(path.as_str())),
            ("api", Json::from(api)),
            ("change", "removed".into()),
        ]));
    }
    if entries.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "nothing to record",
            [Fact::plain("the project has no routes")],
        ));
    } else if changed > 0 {
        screen.push(Block::hint(
            "git diff api/",
            "shows what changed for the API's clients",
        ));
    }
    Reply::screen(screen, Json::object([("records", Json::array(entries))]))
}

fn write(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(path, text)
}
