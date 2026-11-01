//! `jadpo migrate` (area 5, "Migrations"; area 14, "`migrate` commands"): `new`
//! compares the entities with the latest migration's snapshot and writes
//! the next migration's folder (`snapshot.json`, `postgres.sql`, and
//! `changes.jadpo` when a change needs a decision), or finishes the latest
//! one once its decisions are written; `apply` runs a database's pending
//! migrations, each in a transaction; `status` lists the migrations and,
//! given a database, which it has applied.
//!
//! The screens follow the CLI design language's drawn `jadpo migrate new`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_check::acceptances::{data as removed, words};
use jadpo_check::data::migrations::{
    self as records, Decide, Decision, DecisionValue, Migration, Migrations, example_backfill,
    field_type, read_decisions, store_steps,
};
use jadpo_check::data::{DataModel, data_model, schema};
use jadpo_db::Loaded;
use jadpo_diag::{Decider, Diagnostic, Span, rule};
use jadpo_graph::migration::{
    Needed, NeededKind, Refusal, Removal, RemovalKind, Sql, Undroppable, decisions_needed,
    migration_sql, next_snapshot, removals, retired, without_retired,
};
use jadpo_graph::schema::{Change, Holds, Schema};
use jadpo_term::{Block, ChangeMark, Fact, Json, Outcome, Screen};

use crate::database::Address;
use crate::fmt::project_not_found;
use crate::run::{Call, Reply};

pub fn migrate(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        Some("new") => new(call),
        Some(action @ ("apply" | "status")) => {
            if call.value("--drop").is_some() {
                return Reply::failed(arguments(
                    "`--drop` belongs to `jadpo migrate new`".to_string(),
                    format!("A drop is written as a migration of its own, which `jadpo migrate {action}` doesn't do."),
                    "Run `jadpo migrate new <purpose> --drop <name>`".to_string(),
                ));
            }
            if let Some(extra) = call.arguments.get(1) {
                return Reply::failed(arguments(
                    format!("`jadpo migrate {action}` takes no purpose"),
                    format!("Only `jadpo migrate new` takes a purpose; `{extra}` would be ignored."),
                    format!("Run `jadpo migrate {action}`"),
                ));
            }
            if action == "apply" {
                apply(call)
            } else {
                status(call)
            }
        }
        Some(other) => Reply::failed(arguments(
            format!("`{other}` isn't something `jadpo migrate` does"),
            "`jadpo migrate new` writes the next migration, `apply` runs a database's pending ones, and `status` lists them.".to_string(),
            "Use `jadpo migrate new`, `jadpo migrate apply` or `jadpo migrate status`".to_string(),
        )),
        None => Reply::failed(arguments(
            "`jadpo migrate` needs to know what to do".to_string(),
            "`jadpo migrate new` writes the next migration, `apply` runs a database's pending ones, and `status` lists them.".to_string(),
            "Add `new`, `apply` or `status`".to_string(),
        )),
    }
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_MIGRATE_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

fn load(call: &Call) -> Result<(PathBuf, Loaded), Reply> {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return Err(project_not_found("migrate"));
    };
    let loaded = jadpo_db::load(&root);
    Ok((cwd, loaded))
}

/// A screen of the problems that stop the command, as `jadpo check` shows
/// them, and the summary.
pub(crate) fn blocked(
    loaded: &Loaded,
    cwd: &Path,
    problems: Vec<Diagnostic>,
    headline: &str,
) -> Reply {
    let db = &loaded.db;
    let sources: BTreeMap<String, String> = jadpo_db::source_texts(db, loaded.project);
    let root = jadpo_db::shown_root(cwd, &loaded.root);
    let mut screen = Screen::new();
    for problem in &problems {
        screen.push(Block::diagnostic_from(problem, &sources, root.as_deref()));
    }
    let errors = problems.len();
    screen.push(Block::summary(
        Outcome::Failure,
        headline,
        [Fact::problem(format!(
            "{errors} {}",
            if errors == 1 { "error" } else { "errors" }
        ))],
    ));
    Reply::Report {
        screen,
        data: Json::object([("errors", errors.into())]),
        diagnostics: problems,
        sources,
    }
}

// --- new ----------------------------------------------------------------

/// The rules `migrate new` itself answers, and another record out of date
/// or edited, which has its own command; any other error stops it.
fn answers(diagnostic: &Diagnostic) -> bool {
    [
        rule::RECORD_OUT_OF_DATE.id,
        rule::RECORD_EDITED.id,
        rule::MIGRATION_OUT_OF_DATE.id,
        rule::MIGRATION_DECISION_REQUIRED.id,
        rule::MIGRATION_DECISION_INVALID.id,
        rule::MIGRATION_DECISION_UNKNOWN.id,
        rule::MIGRATION_UNFINISHED.id,
    ]
    .contains(&diagnostic.rule.id)
}

fn new(call: &Call) -> Reply {
    let (cwd, loaded) = match load(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let (db, project) = (&loaded.db, loaded.project);
    let problems: Vec<Diagnostic> = jadpo_check::check(db, project)
        .into_iter()
        .filter(|d| !answers(d))
        .collect();
    if !problems.is_empty() {
        return blocked(&loaded, &cwd, problems, "migrate new stopped");
    }
    let set = records::migrations(db, project);
    let model = data_model(db, project);
    let source = schema(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let drop = call.value("--drop");
    // The latest migration is finished once every decision it needs is
    // written and its SQL holds them; until then this finishes it.
    let latest = set.list.last();
    let (decisions, unfinished, changes) = match latest {
        Some(migration) if migration.changes => {
            let decisions = written_decisions(&loaded, migration);
            let before = set.before_latest();
            let after = set.latest_schema();
            let changes = loaded.file(&migration.changes_path());
            let decide = Decide {
                db,
                project,
                lookup: &lookup,
                model,
                decisions: &decisions,
                file: changes,
                before: &before,
            };
            let finished = finished(&decide, &before, &after, migration);
            (decisions, !finished, changes)
        }
        _ => (Vec::new(), false, None),
    };
    let (before, base, name) = if unfinished {
        let Some(migration) = latest else {
            return nothing(set);
        };
        if drop.is_some() {
            return Reply::failed(arguments(
                format!("`{}` waits for its decisions", migration.name),
                "A drop is a migration of its own, written once the latest is finished."
                    .to_string(),
                "Write its decisions and run `jadpo migrate new`, then ask for the drop"
                    .to_string(),
            ));
        }
        (
            set.before_latest(),
            set.latest_schema(),
            migration.name.clone(),
        )
    } else {
        let latest_schema = set.latest_schema();
        let kept = match drop.map(|name| without_retired(&latest_schema, name)) {
            Some(Ok(kept)) => kept,
            Some(Err(why)) => return Reply::failed(undroppable(&why)),
            None => latest_schema.clone(),
        };
        let next = next_snapshot(&kept, source);
        if next.same_but_reasons(&latest_schema) {
            return nothing(set);
        }
        let purpose = match call.arguments.get(1) {
            Some(purpose) => purpose.clone(),
            None => {
                let changes = records::readable_changes(&latest_schema, &next);
                let suggested = if set.list.is_empty() {
                    "initial".to_string()
                } else {
                    records::purpose(&changes, &next)
                };
                return Reply::failed(arguments(
                    "`jadpo migrate new` needs a purpose".to_string(),
                    "A migration's folder is named after what it does, so the history reads as a list of changes.".to_string(),
                    format!("Add one, such as `jadpo migrate new {suggested}`"),
                ));
            }
        };
        if !valid_purpose(&purpose) {
            return Reply::failed(arguments(
                format!("`{purpose}` isn't a purpose a folder can be named after"),
                "A purpose is lower_snake_case words, such as `add_rating`.".to_string(),
                format!("Write it as `{}`", snake(&purpose)),
            ));
        }
        (latest_schema, kept, folder_name(set, &purpose))
    };
    let after = next_snapshot(&base, source);
    let needed = decisions_needed(&before, &after);
    // A migration not written yet has no decisions but the drop asked for.
    let decisions = if unfinished {
        decisions
    } else {
        drop.map(dropped).into_iter().collect()
    };
    let decisions_of = Decide {
        db,
        project,
        lookup: &lookup,
        model,
        decisions: &decisions,
        file: changes,
        before: &before,
    };
    let decide = |need: &Needed| decisions_of.decided(need);
    let sql = match migration_sql(&before, &after, &decide) {
        Ok(sql) => sql,
        Err(refusals) => return refused(&refusals),
    };
    let folder = loaded.root.join(records::FOLDER).join(&name);
    let relative = format!("{}/{name}/", records::FOLDER);
    // A decision that may be left out (chunk 5.9c) is never pending.
    let pending: Vec<&Needed> = needed
        .iter()
        .filter(|need| !need.kind.optional() && decide(need).is_none())
        .collect();
    let misfits: BTreeMap<String, String> = pending
        .iter()
        .filter_map(|need| decisions_of.misfit(need).map(|why| (need.key(), why)))
        .collect();
    let written = match write(
        &folder,
        &name,
        &after,
        &sql,
        &needed,
        pending.len(),
        model,
        &lookup,
    ) {
        Ok(written) => written,
        Err(error) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{relative}` couldn't be written"),
                )
                .reason(format!("Writing it failed: {error}."))
                .next("Check the folder can be written, then run `jadpo migrate new` again"),
            );
        }
    };
    screen_for_new(
        set, latest, &before, &after, &relative, &name, &sql, &pending, &misfits, written,
        unfinished, model, &lookup,
    )
}

/// The decision `--drop <name>` makes, as `changes.jadpo` will say it.
fn dropped(name: &str) -> Decision {
    Decision {
        key: name.to_string(),
        span: Span::new("", 0, 0),
        value: if name.contains('.') {
            DecisionValue::Change(jadpo_std::FieldChange::Dropped)
        } else {
            DecisionValue::Entity(jadpo_std::EntityChange::Dropped)
        },
        decl: 0,
        member: 0,
    }
}

/// Why `--drop` can't drop what it names.
fn undroppable(why: &Undroppable) -> Diagnostic {
    match why {
        Undroppable::NotRetired(name) => arguments(
            format!("`{name}` isn't retired"),
            "Removing data takes two migrations: the first retires it, keeping its data while code stops using it, and a later one drops it.".to_string(),
            format!("Remove `{name}` from the source and run `jadpo migrate new` to retire it first"),
        ),
        Undroppable::Unknown(name) => arguments(
            format!("`{name}` is no retired field or entity"),
            "`--drop` takes a retired field or entity by the name its migration's `changes.jadpo` gives it, such as `Todo.legacy_flag` or `Todo`.".to_string(),
            "Run `jadpo migrate new` with nothing to migrate to list what is retired".to_string(),
        ),
    }
}

/// The decisions written in a migration's `changes.jadpo`.
fn written_decisions(loaded: &Loaded, migration: &Migration) -> Vec<Decision> {
    let path = migration.changes_path();
    let db = &loaded.db;
    let Some(file) = loaded.file(&path) else {
        return Vec::new();
    };
    let Some(source) = jadpo_syntax::ast::SourceFile::cast(jadpo_db::parse(db, file).syntax())
    else {
        return Vec::new();
    };
    read_decisions(&path, &source).0
}

use jadpo_syntax::ast::AstNode as _;

/// Whether every decision a migration needs is made and its SQL holds them.
fn finished(decide: &Decide<'_>, before: &Schema, after: &Schema, migration: &Migration) -> bool {
    let needed = decisions_needed(before, after);
    if needed
        .iter()
        .any(|need| !need.kind.optional() && decide.decided(need).is_none())
    {
        return false;
    }
    let decide = |need: &Needed| decide.decided(need);
    match migration_sql(before, after, &decide) {
        Ok(sql) => {
            migration
                .sql
                .as_ref()
                .and_then(|s| s.as_ref().ok())
                .map(String::as_str)
                == Some(sql.to_document(&migration.name).as_str())
        }
        Err(_) => false,
    }
}

fn nothing(set: &Migrations) -> Reply {
    let latest = set.list.last().map(|m| m.name.clone());
    let text = match &latest {
        Some(name) => format!("Nothing to migrate: the entities match {name}"),
        None => "Nothing to migrate: the project has no entities".to_string(),
    };
    // What still holds data no code reaches, and how it goes.
    let waiting = retired(&set.latest_schema());
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows: waiting
            .iter()
            .map(|(name, entity)| {
                (
                    name.clone(),
                    if *entity {
                        "retired: its table and records stay".to_string()
                    } else {
                        "retired: its column and data stay".to_string()
                    },
                )
            })
            .collect(),
    });
    if !waiting.is_empty() {
        screen.push(Block::hint(
            "jadpo migrate new <purpose> --drop <name>",
            "drops one for good, once a person accepts it",
        ));
    }
    Reply::screen(
        screen,
        Json::object([
            ("written", Json::Null),
            ("latest", latest.map_or(Json::Null, Json::from)),
            (
                "retired",
                Json::array(waiting.iter().map(|(name, _)| name.as_str())),
            ),
        ]),
    )
}

fn refused(refusals: &[Refusal]) -> Reply {
    let listed: Vec<String> = refusals.iter().map(|r| r.change.clone()).collect();
    let reason = refusals
        .first()
        .map(|r| r.reason.clone())
        .unwrap_or_default();
    Reply::failed(
        Diagnostic::new(
            &rule::TOOLCHAIN_NOT_BUILT_YET,
            "This migration isn't built yet",
        )
        .reason(format!("It would need {}. {reason}", and_list(&listed)))
        .next("Undo that change for now, then run `jadpo migrate new` again"),
    )
}

fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

pub(crate) fn valid_purpose(purpose: &str) -> bool {
    !purpose.is_empty()
        && purpose
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && purpose.starts_with(|c: char| c.is_ascii_lowercase())
        && !purpose.ends_with('_')
        && !purpose.contains("__")
}

pub(crate) fn snake(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// `20261020T1400_<purpose>`, now in UTC (or at `SOURCE_DATE_EPOCH`, the
/// reproducible-builds convention, when it is set), a minute after the
/// latest migration when the clock is behind it, so names keep the order.
fn folder_name(set: &Migrations, purpose: &str) -> String {
    let fixed = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|seconds| seconds.trim().parse::<i64>().ok())
        .map(|seconds| seconds.saturating_mul(1000));
    let millis = fixed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
    });
    let stamp = |millis: i64| -> String {
        jadpo_std::format_instant(millis)
            .chars()
            .take(16)
            .filter(|c| *c != '-' && *c != ':')
            .collect()
    };
    let mut at = millis - millis.rem_euclid(60_000);
    if let Some(latest) = set.list.last() {
        let latest_stamp = latest.name.get(..13).unwrap_or_default().to_string();
        while stamp(at) <= latest_stamp {
            at += 60_000;
        }
    }
    format!("{}_{purpose}", stamp(at))
}

/// What `migrate new` wrote, file by file, with what each is.
struct Written {
    files: Vec<(String, String)>,
    /// The line of each decision still to make in `changes.jadpo`.
    lines: BTreeMap<String, u32>,
}

#[allow(clippy::too_many_arguments)]
fn write(
    folder: &Path,
    name: &str,
    after: &Schema,
    sql: &Sql,
    needed: &[Needed],
    open: usize,
    model: &DataModel,
    lookup: &dyn jadpo_check::types::TypeView,
) -> std::io::Result<Written> {
    std::fs::create_dir_all(folder)?;
    std::fs::write(folder.join("snapshot.json"), after.to_document())?;
    std::fs::write(folder.join("postgres.sql"), sql.to_document(name))?;
    let count = sql.count();
    let summary = sql.summary().join(", ");
    let mut files = vec![
        (
            "snapshot.json".to_string(),
            "the whole schema at this point".to_string(),
        ),
        (
            "postgres.sql".to_string(),
            if count == 0 {
                // Only what the application checks changed, such as a
                // variant added: the snapshot records it, and the store
                // has nothing to do.
                "no statements: nothing in the database changes".to_string()
            } else {
                format!(
                    "{count} {}: {summary}",
                    if count == 1 {
                        "statement"
                    } else {
                        "statements"
                    }
                )
            },
        ),
    ];
    let mut lines = BTreeMap::new();
    if !needed.is_empty() {
        let path = folder.join("changes.jadpo");
        let existing = std::fs::read_to_string(&path).ok();
        let (text, _) = changes_file(existing.as_deref(), needed, model, lookup);
        std::fs::write(&path, &text)?;
        for (number, line) in (1u32..).zip(text.lines()) {
            for need in needed {
                if line.trim_start().starts_with(&format!("{}:", need.key())) {
                    lines.insert(need.key(), number);
                }
            }
        }
        let optional = needed.iter().all(|need| need.kind.optional());
        files.push((
            "changes.jadpo".to_string(),
            // A decision written but not one that fits is still open.
            match open {
                // Only a structured field's stored values to check, which
                // a `backfill:` rewrites instead (chunk 5.9c).
                0 if optional => {
                    "no decision needed: a `backfill:` rewrites the values its type no longer reads"
                        .to_string()
                }
                0 => "every decision made".to_string(),
                1 => "1 decision to make".to_string(),
                n => format!("{n} decisions to make"),
            },
        ));
    }
    Ok(Written { files, lines })
}

/// The `changes.jadpo` text: what was written already, with a line to fill
/// in for each decision it doesn't make yet; and how many are still open.
fn changes_file(
    existing: Option<&str>,
    needed: &[Needed],
    model: &DataModel,
    lookup: &dyn jadpo_check::types::TypeView,
) -> (String, usize) {
    let mut open = 0;
    let mut added = String::new();
    let mut seen: Vec<String> = Vec::new();
    for need in needed {
        let key = need.key();
        // A table left behind in two stores is one decision.
        if seen.contains(&key) {
            continue;
        }
        seen.push(key.clone());
        // A decision that may be left out (chunk 5.9c) is said in a
        // comment, once, and never open.
        if need.kind.optional() {
            if existing.is_some_and(|text| text.contains(&format!("`{key}"))) {
                continue;
            }
            added.push_str(&format!(
                "    // `{key}`'s type may not read every value stored before, so\n    // `jadpo migrate apply` checks each one and refuses the migration\n    // while one doesn't read. To rewrite those instead, write\n    // `{key}: {{ backfill: <value> }}` and run `jadpo migrate new`.\n"
            ));
            continue;
        }
        let present = existing.is_some_and(|text| {
            text.lines()
                .any(|line| line.trim_start().starts_with(&format!("{key}:")))
        });
        if present {
            let filled = existing.is_some_and(|text| {
                text.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with(&format!("{key}:")) && !line.ends_with("{}")
                })
            });
            if !filled {
                open += 1;
            }
            continue;
        }
        // `--drop` asked for it, so the line says it already.
        match need.kind {
            NeededKind::DropField => {
                added.push_str(&format!(
                    "    // Drops `{key}`'s column and its data for good, once a person\n    // accepts it.\n    {key}: FieldChange.dropped\n"
                ));
                continue;
            }
            NeededKind::DropEntity => {
                added.push_str(&format!(
                    "    // Drops `{key}`'s table and its records for good, once a person\n    // accepts it.\n    {key}: EntityChange.dropped\n"
                ));
                continue;
            }
            _ => open += 1,
        }
        let advice = match need.kind {
            NeededKind::Backfill => {
                let example = field_type(model, &need.entity, &need.field)
                    .and_then(|ty| example_backfill(lookup, ty))
                    .map_or_else(String::new, |e| format!(", such as `backfill: {e}`"));
                format!(
                    "    // `{key}` is required: give the `{}` records already\n    // stored a value with `backfill:`{example}.\n    {key}: {{}}\n",
                    need.entity
                )
            }
            NeededKind::Retire => format!(
                "    // `{key}` is gone. `FieldChange.retired` keeps its column\n    // and data while code stops using it.\n    {key}: {{}}\n"
            ),
            NeededKind::RetireEntity => format!(
                "    // `{key}` is gone. `EntityChange.retired` keeps its table and\n    // records, out of the app's reach, while code stops using them.\n    {key}: {{}}\n"
            ),
            NeededKind::Unreadable | NeededKind::DropField | NeededKind::DropEntity => continue,
            NeededKind::Refit => {
                let example = field_type(model, &need.entity, &need.field)
                    .and_then(|ty| example_backfill(lookup, ty))
                    .map_or_else(String::new, |e| format!(", such as `backfill: {e}`"));
                format!(
                    "    // `{key}` allows less than it did: give the `{}` records\n    // already stored with a value it no longer allows one it does\n    // with `backfill:`{example}.\n    {key}: {{}}\n",
                    need.entity
                )
            }
        };
        added.push_str(&advice);
    }
    let text = match existing {
        Some(text) => match text.rfind('}') {
            Some(end) if !added.is_empty() => {
                let (head, tail) = text.split_at(end);
                format!("{}\n{added}{tail}", head.trim_end())
            }
            _ => text.to_string(),
        },
        None => format!("migration {{\n{added}}}\n"),
    };
    (text, open)
}

#[allow(clippy::too_many_arguments)]
fn screen_for_new(
    set: &Migrations,
    latest: Option<&Migration>,
    before: &Schema,
    after: &Schema,
    relative: &str,
    name: &str,
    sql: &Sql,
    pending: &[&Needed],
    misfits: &BTreeMap<String, String>,
    written: Written,
    finishing: bool,
    model: &DataModel,
    lookup: &dyn jadpo_check::types::TypeView,
) -> Reply {
    let mut screen = Screen::new();
    let note = match (finishing, latest) {
        (true, _) => {
            let base = set
                .list
                .len()
                .checked_sub(2)
                .and_then(|at| set.list.get(at))
                .map_or_else(|| "an empty database".to_string(), |m| m.name.clone());
            format!("finished {name}, which migrates from {base}")
        }
        (false, Some(latest)) => format!("compared the entities with {}", latest.name),
        (false, None) => "compared the entities with an empty database".to_string(),
    };
    let groups = change_groups(before, after, model);
    screen.push(Block::Changes {
        note: Some(note),
        groups,
    });
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text: format!("Wrote {relative}"),
        rows: written.files.clone(),
    });
    let changes_path = format!("{relative}changes.jadpo");
    let mut decisions = Vec::new();
    for need in pending {
        let key = need.key();
        let (mut subject, next) = match need.kind {
            NeededKind::Backfill => {
                let example = field_type(model, &need.entity, &need.field)
                    .and_then(|ty| example_backfill(lookup, ty));
                (
                    format!("existing `{}` records need a `{}`", need.entity, need.field),
                    match example {
                        Some(example) => {
                            format!("Set `backfill:` for `{key}`, such as `{example}`")
                        }
                        None => format!("Set `backfill:` for `{key}`"),
                    },
                )
            }
            NeededKind::Retire => (
                format!("`{key}` is gone: retire its column"),
                format!("Write `{key}: FieldChange.retired`"),
            ),
            NeededKind::RetireEntity => (
                format!("`{key}` is gone: retire its table"),
                format!("Write `{key}: EntityChange.retired`"),
            ),
            NeededKind::DropField => (
                format!("this migration drops `{key}`'s column"),
                format!("Write `{key}: FieldChange.dropped`"),
            ),
            NeededKind::DropEntity => (
                format!("this migration drops `{key}`'s table"),
                format!("Write `{key}: EntityChange.dropped`"),
            ),
            NeededKind::Unreadable => (
                format!(
                    "existing `{}` records may hold a `{}` its type no longer reads",
                    need.entity, need.field
                ),
                format!("Set `backfill:` for `{key}`"),
            ),
            NeededKind::Refit => {
                let example = field_type(model, &need.entity, &need.field)
                    .and_then(|ty| example_backfill(lookup, ty));
                (
                    format!(
                        "existing `{}` records may hold a `{}` it no longer allows",
                        need.entity, need.field
                    ),
                    match example {
                        Some(example) => {
                            format!("Set `backfill:` for `{key}`, such as `{example}`")
                        }
                        None => format!("Set `backfill:` for `{key}`"),
                    },
                )
            }
        };
        if let Some(why) = misfits.get(&key) {
            // `This value can't fill …: why.` as a subject line.
            let why = why.trim_end_matches('.');
            let mut chars = why.chars();
            subject = chars.next().map_or_else(String::new, |first| {
                first.to_lowercase().chain(chars).collect::<String>()
            });
        }
        let line = written.lines.get(&key).copied();
        screen.push(Block::Decision {
            subject,
            location: Some(match line {
                Some(line) => format!("{changes_path}:{line}"),
                None => changes_path.clone(),
            }),
            next,
        });
        decisions.push(Json::object([
            ("key", key.as_str().into()),
            (
                "kind",
                match need.kind {
                    NeededKind::Backfill => "backfill",
                    NeededKind::Retire => "retire",
                    NeededKind::Refit => "refit",
                    NeededKind::Unreadable => "rewrite",
                    NeededKind::RetireEntity => "retire_entity",
                    NeededKind::DropField => "drop_field",
                    NeededKind::DropEntity => "drop_entity",
                }
                .into(),
            ),
            ("file", changes_path.as_str().into()),
            ("line", line.map_or(Json::Null, |l| i64::from(l).into())),
        ]));
    }
    let removed = removals(before, after);
    if !pending.is_empty() {
        screen.push(Block::hint(
            "jadpo migrate new",
            "finishes the migration once its decisions are written",
        ));
    } else if removed.is_empty() {
        screen.push(Block::hint(
            "jadpo check",
            "confirms the migration matches the entities",
        ));
    } else {
        // The change list above says exactly what goes; what is left is a
        // person's.
        screen.push(Block::Decision {
            subject: format!(
                "{} for a person's acceptance",
                crate::approval::waiting_changes(removed.len())
            ),
            location: None,
            next: format!(
                "A person accepts {}: `jadpo acceptance new <purpose>` writes the draft for them.",
                if removed.len() == 1 { "it" } else { "them" }
            ),
        });
    }
    let data = Json::object([
        ("migration", name.into()),
        ("folder", relative.into()),
        ("finished", pending.is_empty().into()),
        (
            "removes",
            Json::array(removed.iter().map(|removal| {
                Json::object([
                    ("subject", removal.subject().as_str().into()),
                    ("change", removal.change().into()),
                    ("said", words::removal(removal).as_str().into()),
                ])
            })),
        ),
        (
            "files",
            Json::array(written.files.iter().map(|(f, _)| f.as_str())),
        ),
        ("statements", sql.count().into()),
        ("decisions", Json::Array(decisions)),
    ]);
    Reply::screen(screen, data)
}

/// Whether table `table`, new in `store`, is an entity's that left its table
/// behind in another store.
fn moved_here(before: &Schema, after: &Schema, store: &str, table: &str) -> bool {
    let Some(Holds::Entity(entity)) = after
        .stores
        .get(store)
        .and_then(|s| s.tables.get(table))
        .map(|t| &t.holds)
    else {
        return false;
    };
    before.live_store_of(entity).is_some_and(|was| was != store)
}

/// The changes from `before` to `after`, grouped under the entity (or the
/// system tables) they change, each with its mark and columns.
fn change_groups(
    before: &Schema,
    after: &Schema,
    model: &DataModel,
) -> Vec<jadpo_term::ChangeGroup> {
    let now = |born: &str| {
        model
            .entities
            .iter()
            .find(|e| e.born == born)
            .map_or_else(|| born.to_string(), |e| e.name.clone())
    };
    let table = |schema: &Schema, store: &str, name: &str| {
        schema
            .stores
            .get(store)
            .and_then(|s| s.tables.get(name))
            .cloned()
    };
    type Rows = Vec<(ChangeMark, Vec<String>)>;
    let mut groups: BTreeMap<(bool, String), Rows> = BTreeMap::new();
    for change in records::readable_changes(before, after) {
        let (store, Some(name)) = change.table() else {
            continue;
        };
        let Some(found) = table(after, store, name).or_else(|| table(before, store, name)) else {
            continue;
        };
        let (system, heading) = match &found.holds {
            Holds::Entity(born) => (false, now(born)),
            Holds::System(_) => (true, "System tables".to_string()),
        };
        let column = |schema: &Schema, column: &str| {
            table(schema, store, name).and_then(|t| t.columns.get(column).cloned())
        };
        let row = match &change {
            Change::AddTable { table: t, .. } => (
                ChangeMark::Added,
                vec![
                    "table".into(),
                    t.clone(),
                    if system {
                        found.holds.to_string()
                    } else if moved_here(before, after, store, t) {
                        format!("in `{store}`: new and empty")
                    } else {
                        "new entity".into()
                    },
                ],
            ),
            Change::DropTable { table: t, .. } => (
                ChangeMark::Removed,
                vec![
                    "table".into(),
                    t.clone(),
                    "dropped: the table and its records go for good".into(),
                ],
            ),
            Change::RetireTable { table: t, .. } => (
                ChangeMark::Removed,
                vec![
                    "table".into(),
                    t.clone(),
                    if matches!(&found.holds, Holds::Entity(entity) if after.live_store_of(entity).is_some())
                    {
                        format!("in `{store}`: retired, its records stay and aren't copied")
                    } else {
                        "retired: the table and its records stay".into()
                    },
                ],
            ),
            // Refused before a screen is drawn.
            Change::ReviveTable { .. } => continue,
            Change::AddColumn { column: c, .. } => (
                ChangeMark::Added,
                vec![
                    c.clone(),
                    column(after, c)
                        .as_ref()
                        .map(|c| c.sql_type.clone())
                        .unwrap_or_default(),
                    // An entity's searchable words (area 16, chunk B12).
                    if column(after, c).is_some_and(|c| c.generated.is_some()) {
                        "new search".into()
                    } else {
                        "new field".into()
                    },
                ],
            ),
            Change::RemoveColumn { column: c, .. } => {
                let was = column(before, c);
                (
                    ChangeMark::Removed,
                    vec![
                        c.clone(),
                        was.as_ref().map(|c| c.sql_type.clone()).unwrap_or_default(),
                        if was.is_some_and(|c| c.generated.is_some()) {
                            "search removed".into()
                        } else {
                            "dropped: the column and its data go for good".into()
                        },
                    ],
                )
            }
            Change::AlterColumn { column: c, .. } => {
                let (was, now) = (column(before, c), column(after, c));
                let what = match (&was, &now) {
                    // An entity's searchable words, worked out again from
                    // its fields (area 16, chunk B12).
                    (Some(_), Some(now)) if now.generated.is_some() => {
                        "search made again from its fields".to_string()
                    }
                    (Some(was), Some(now)) if !was.retired && now.retired => {
                        "retired: the column and its data stay".to_string()
                    }
                    (Some(was), Some(now)) if was.sql_type != now.sql_type => {
                        format!("type was {}", was.sql_type)
                    }
                    (Some(was), Some(now)) if was.nullable && !now.nullable => {
                        "now required".into()
                    }
                    // Only its comment, the unit a quantity is held in
                    // (area 19, chunk 5.5w).
                    (Some(was), Some(now))
                        if was.comment != now.comment
                            && was.nullable == now.nullable
                            && was.allows == now.allows =>
                    {
                        match &now.comment {
                            Some(comment) => format!("holds {comment}"),
                            None => "comment removed".into(),
                        }
                    }
                    (Some(was), Some(now)) if was.nullable == now.nullable => {
                        if was.allows.is_none() {
                            "what it allows, recorded".into()
                        } else if jadpo_graph::migration::narrowed(was, now) {
                            "allows less".into()
                        } else {
                            "allows more".into()
                        }
                    }
                    (Some(_), Some(_)) => "can be none".into(),
                    _ => "changed".into(),
                };
                let mark = if now.as_ref().is_some_and(|n| n.retired) {
                    ChangeMark::Removed
                } else {
                    ChangeMark::Changed
                };
                (
                    mark,
                    vec![c.clone(), now.map(|c| c.sql_type).unwrap_or_default(), what],
                )
            }
            Change::AddUnique { name: n, .. } | Change::DropUnique { name: n, .. } => {
                let added = matches!(change, Change::AddUnique { .. });
                let rule = table(if added { after } else { before }, store, name)
                    .and_then(|t| t.unique.get(n).cloned());
                (
                    if added {
                        ChangeMark::Added
                    } else {
                        ChangeMark::Removed
                    },
                    vec![
                        "unique".into(),
                        rule.as_ref()
                            .map_or_else(String::new, |r| format!("({})", r.columns.join(", "))),
                        rule.map_or_else(|| n.clone(), |r| format!("for {}", r.rule)),
                    ],
                )
            }
            Change::AddCheck { name: n, .. } | Change::DropCheck { name: n, .. } => {
                let added = matches!(change, Change::AddCheck { .. });
                let check = table(if added { after } else { before }, store, name)
                    .and_then(|t| t.checks.get(n).cloned());
                (
                    if added {
                        ChangeMark::Added
                    } else {
                        ChangeMark::Removed
                    },
                    vec![
                        "check".into(),
                        check
                            .as_ref()
                            .map_or_else(String::new, |c| format!("({})", c.column)),
                        check
                            .map_or_else(|| n.clone(), |c| format!("is {}", c.values.join(" or "))),
                    ],
                )
            }
            Change::AddForeignKey { name: n, .. } | Change::DropForeignKey { name: n, .. } => {
                let added = matches!(change, Change::AddForeignKey { .. });
                let key = table(if added { after } else { before }, store, name)
                    .and_then(|t| t.foreign_keys.get(n).cloned());
                (
                    if added {
                        ChangeMark::Added
                    } else {
                        ChangeMark::Removed
                    },
                    vec![
                        "reference".into(),
                        key.as_ref().map_or_else(String::new, |k| {
                            format!("{} → {}", k.columns.join(", "), k.table)
                        }),
                        key.map_or_else(
                            || n.clone(),
                            |k| format!("on delete {}", k.on_delete.sql().to_lowercase()),
                        ),
                    ],
                )
            }
            Change::AddIndex { name: n, .. } | Change::DropIndex { name: n, .. } => {
                let added = matches!(change, Change::AddIndex { .. });
                let index = table(if added { after } else { before }, store, name)
                    .and_then(|t| t.indexes.get(n).cloned());
                (
                    if added {
                        ChangeMark::Added
                    } else {
                        ChangeMark::Removed
                    },
                    vec![
                        "index".into(),
                        index
                            .as_ref()
                            .map_or_else(String::new, |i| format!("({})", i.columns.join(", "))),
                        index.map_or_else(
                            || n.clone(),
                            |i| match i.reason.split_once(' ') {
                                Some((_, what)) => format!("for {what}"),
                                None => format!("for {}", i.reason),
                            },
                        ),
                    ],
                )
            }
            Change::AddStore { .. } => continue,
        };
        groups.entry((system, heading)).or_default().push(row);
    }
    groups
        .into_iter()
        .map(|((_, heading), rows)| (heading, rows))
        .collect()
}

// --- apply and status --------------------------------------------------------

/// The store `apply` and `status` run against, with its URL: `--url` if
/// given, otherwise the address the store's connection reads from its
/// setting (area 10), from the environment over `.env.local`, secured as
/// its connection says either way (chunk 8.1n). `--store` picks the store
/// when the project has several.
fn target(
    call: &Call,
    loaded: &Loaded,
    steps: &BTreeMap<String, Vec<jadpo_runtime::Migration>>,
) -> Result<Option<(String, Address)>, Reply> {
    let url = call.value("--url");
    let store = match call.value("--store") {
        Some(store) => store.to_string(),
        None => match steps.keys().collect::<Vec<_>>().as_slice() {
            [only] => (*only).clone(),
            [] => "main".to_string(),
            many => {
                let names: Vec<String> = many.iter().map(|s| format!("`{s}`")).collect();
                return Err(Reply::failed(arguments(
                    "Which store is this database?".to_string(),
                    format!(
                        "The project's migrations are for {}, and `--url` names one database.",
                        and_list(&names)
                    ),
                    "Add `--store <name>`".to_string(),
                )));
            }
        },
    };
    if let Some(url) = url {
        let address = Address::of(call, loaded, &store, url.to_string())?;
        return Ok(Some((store, address)));
    }
    configured_url(call, loaded, store)
}

/// The address the connection `store` reads from its setting, when a
/// variable gives one, secured as the connection says.
pub(crate) fn configured_url(
    call: &Call,
    loaded: &Loaded,
    store: String,
) -> Result<Option<(String, Address)>, Reply> {
    use jadpo_runtime::config::Variables;
    let urls = jadpo_check::config::connection_urls(&loaded.db, loaded.project);
    let Some(connection) = urls.url_of(&store) else {
        return Ok(None);
    };
    let model = jadpo_check::config::config(&loaded.db, loaded.project);
    let Some(setting) = model.setting(&connection.setting) else {
        return Ok(None);
    };
    let layered = crate::config::Layered::new(&loaded.root, &call.environment)
        .map_err(|problem| Reply::failed(*problem))?;
    let Some(url) = layered.variable(&setting.variable) else {
        return Ok(None);
    };
    if jadpo_std::check_builtin_text(jadpo_std::Builtin::PostgresUrl, &url).is_err() {
        let rule = if jadpo_std::database_tls::tls_parameter(&url).is_some() {
            jadpo_runtime::config::TLS_RULE
        } else {
            "format"
        };
        return Err(Reply::failed(crate::config::value_diagnostic(
            &setting.name,
            &setting.variable,
            true,
            "PostgresUrl",
            &jadpo_runtime::config::ProblemKind::Invalid {
                rule: rule.to_string(),
            },
        )));
    }
    let address = Address::of(call, loaded, &store, url)?;
    Ok(Some((store, address)))
}

/// Whether the migrations may be applied: their records read, the latest
/// finished, and each removal accepted by a person, however the project got
/// here (chunk V3). Code that doesn't check doesn't stop it.
fn migratable(loaded: &Loaded) -> Vec<Diagnostic> {
    jadpo_check::data::migrations::migration_problems(&loaded.db, loaded.project)
        .iter()
        .filter(|d| d.rule.id != rule::MIGRATION_OUT_OF_DATE.id)
        .cloned()
        .chain(removed::unaccepted(&loaded.db, loaded.project))
        .collect()
}

fn apply(call: &Call) -> Reply {
    let (cwd, loaded) = match load(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let problems = migratable(&loaded);
    if !problems.is_empty() {
        return blocked(&loaded, &cwd, problems, "migrate apply stopped");
    }
    let set = records::migrations(&loaded.db, loaded.project);
    let steps = store_steps(set);
    let (store, url) = match target(call, &loaded, &steps) {
        Ok(Some(found)) => found,
        Ok(None) => {
            return Reply::failed(arguments(
                "`jadpo migrate apply` needs the database's URL".to_string(),
                "No variable gives the address of the store's connection: a connection reads it from the setting its `url:` names (area 10), and `--url` names a database directly.".to_string(),
                "Run `jadpo config set <the connection's setting>`, or add `--url postgresql://…`".to_string(),
            ));
        }
        Err(reply) => return reply,
    };
    let list = steps.get(&store).map(Vec::as_slice).unwrap_or_default();
    let migrations = list.to_vec();
    // A backfill worked out in Jadpo runs the project's code (chunk
    // 5.6b2): the latest migration's, checked, which needs a project that
    // checks without errors. Whether one has rows to fill is known only
    // as it runs, so code errors stop it only then.
    let in_jadpo = migrations.iter().any(|migration| {
        migration
            .steps()
            .iter()
            .any(|step| matches!(step, jadpo_std::Step::Batches(b) if b.how == jadpo_std::IN_JADPO))
    });
    let (prepared, code_errors) = match in_jadpo
        .then(|| jadpo_interp::Prepared::with_backfills(&loaded.db, loaded.project))
    {
        Some(Ok(prepared)) => (Some(prepared), Vec::new()),
        Some(Err(errors)) => (None, errors),
        None => (None, Vec::new()),
    };
    let latest = set.list.last().map(|migration| migration.name.clone());
    // The interpreter's values never cross threads, so the program is made
    // on the thread that runs it, with the stack its recursion needs.
    let ran = jadpo_interp::on_large_stack(move || {
        let program = prepared.map(jadpo_interp::Program::new);
        with_backfills(program.as_ref(), |backfills| {
            let runtime = jadpo_host_native::Runtime::new()
                .map_err(|error| jadpo_runtime::StoreError::Fault(error.to_string()))?;
            let database = url.open()?;
            Ok(runtime.block_on(async {
                let mut filled: Vec<Filled> = Vec::new();
                let ran = jadpo_runtime::migrate_with(
                    &database,
                    &migrations,
                    backfills,
                    &mut |progress| {
                        track(&mut filled, progress);
                        std::ops::ControlFlow::Continue(())
                    },
                )
                .await;
                ran.map(|ran| (ran, filled))
            }))
        })
    });
    match ran {
        Ok(Ok(Ok((ran, filled)))) => {
            let removes = removed::removed_by(&loaded.db, loaded.project, &store);
            applied(&store, &ran, &filled, &removes)
        }
        // The latest migration's backfill can't run while the code doesn't
        // check: its errors are what to fix.
        Ok(Ok(Err(
            jadpo_runtime::MigrateError::Unworked { name, .. }
            | jadpo_runtime::MigrateError::Unchecked { name, .. },
        ))) if Some(&name) == latest.as_ref() && !code_errors.is_empty() => {
            blocked(&loaded, &cwd, code_errors, "migrate apply stopped")
        }
        Ok(Ok(Err(error))) => Reply::failed(migrate_failed(&store, error)),
        Ok(Err(error)) => Reply::failed(migrate_failed(
            &store,
            jadpo_runtime::MigrateError::Store(error),
        )),
        Err(error) => Reply::failed(migrate_failed(
            &store,
            jadpo_runtime::MigrateError::Store(jadpo_runtime::StoreError::Fault(error.to_string())),
        )),
    }
}

/// Runs `apply` with the latest migration's backfills that have no SQL
/// form worked out by the runtime with `program`'s interpreter (chunks
/// 5.6b2 and 6.1a), or with none when there is no program.
pub(crate) fn with_backfills<T>(
    program: Option<&jadpo_interp::Program>,
    apply: impl FnOnce(&dyn jadpo_runtime::Backfills) -> T,
) -> T {
    match program {
        Some(program) => apply(
            &jadpo_runtime::runner::InJadpo::new(program, program.backfills())
                .with_json(program.stored_json(), program.migration_json()),
        ),
        None => apply(&jadpo_runtime::NoBackfills),
    }
}

/// A backfill `apply` filled in batches (chunk 5.6b), as it reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Filled {
    migration: String,
    field: String,
    work: jadpo_std::Work,
    how: String,
    /// The rows left when this run began it.
    left: u64,
    rows: u64,
    batches: u64,
    resumed: bool,
}

/// Follows a backfill's batches, and says how far they have got on the
/// live line: `Filling Todo.priority  2000 of 5000 rows`.
fn track(filled: &mut Vec<Filled>, progress: &jadpo_runtime::Progress<'_>) {
    use jadpo_runtime::Progress;
    match progress {
        Progress::Filling {
            migration,
            field,
            work,
            how,
            left,
            resumed,
        } => {
            filled.push(Filled {
                migration: migration.to_string(),
                field: field.to_string(),
                work: *work,
                how: how.to_string(),
                left: *left,
                rows: 0,
                batches: 0,
                resumed: *resumed,
            });
            crate::run::say(&format!(
                "{} {field}{}  0 of {}",
                doing(*work),
                in_jadpo_words(how),
                rows(*left)
            ));
        }
        Progress::Filled {
            migration,
            field,
            rows: batch,
        } => {
            if let Some(found) = filled
                .iter_mut()
                .rev()
                .find(|f| f.migration == *migration && f.field == *field)
            {
                found.rows += batch;
                found.batches += 1;
                crate::run::say(&format!(
                    "{} {field}{}  {} of {}",
                    doing(found.work),
                    in_jadpo_words(&found.how),
                    found.rows,
                    rows(found.left.max(found.rows))
                ));
            }
        }
        Progress::Applied { .. } => crate::run::say("Applying migrations"),
    }
}

/// What the live line says a step in batches is doing.
fn doing(work: jadpo_std::Work) -> &'static str {
    match work {
        jadpo_std::Work::Fill => "Filling",
        jadpo_std::Work::Check => "Checking",
        jadpo_std::Work::Rewrite => "Rewriting",
    }
}

/// ` in Jadpo` on the live line for a backfill worked out in Jadpo (chunk
/// 5.6b2), so a slower run says why while it runs.
fn in_jadpo_words(how: &str) -> &'static str {
    if how == jadpo_std::IN_JADPO {
        " in Jadpo"
    } else {
        ""
    }
}

/// `1 row`, `2500 rows`.
fn rows(n: u64) -> String {
    if n == 1 {
        "1 row".to_string()
    } else {
        format!("{n} rows")
    }
}

/// What a removal did, under its migration on `apply`'s screen.
fn removal_word(removal: &Removal) -> &'static str {
    match removal.kind {
        RemovalKind::ColumnDropped | RemovalKind::TableDropped => "dropped",
        RemovalKind::EntityRemoved => "retired",
        RemovalKind::EntityMoved { .. } => "retired here, moved",
    }
}

/// The screen for migrations applied: each by name, and under it each
/// backfill it filled in batches and what it removed.
fn applied(
    store: &str,
    ran: &[String],
    filled: &[Filled],
    removes: &BTreeMap<String, Vec<Removal>>,
) -> Reply {
    let text = if ran.is_empty() {
        format!("`{store}` is up to date")
    } else {
        format!(
            "Applied {} {} to `{store}`",
            ran.len(),
            if ran.len() == 1 {
                "migration"
            } else {
                "migrations"
            }
        )
    };
    let mut lines = Vec::new();
    for name in ran {
        lines.push((name.clone(), "applied".to_string()));
        for fill in filled.iter().filter(|f| &f.migration == name) {
            let batches = if fill.batches == 1 {
                "1 batch".to_string()
            } else {
                format!("{} batches", fill.batches)
            };
            let done = match fill.work {
                jadpo_std::Work::Fill => "filled",
                jadpo_std::Work::Check => "checked",
                jadpo_std::Work::Rewrite => "rewritten",
            };
            let mut said = format!("{} {done} in {batches} {}", rows(fill.rows), fill.how);
            // Worked out row by row in Jadpo, which is slower (chunk
            // 5.6b2); a structured field's stored values are read only
            // that way (chunk 5.9c).
            if fill.how == jadpo_std::IN_JADPO && fill.work == jadpo_std::Work::Fill {
                said.push_str(" (slower than SQL)");
            }
            if fill.resumed {
                said.push_str(", carrying on from an earlier run");
            }
            lines.push((format!("  {}", fill.field), said));
        }
        for removal in removes.get(name).into_iter().flatten() {
            lines.push((
                format!("  {}", removal.subject()),
                removal_word(removal).to_string(),
            ));
        }
    }
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows: lines,
    });
    let backfills = filled.iter().map(|fill| {
        Json::object([
            ("migration", fill.migration.as_str().into()),
            ("field", fill.field.as_str().into()),
            ("how", fill.how.as_str().into()),
            (
                "work",
                match fill.work {
                    jadpo_std::Work::Fill => "fill",
                    jadpo_std::Work::Check => "check",
                    jadpo_std::Work::Rewrite => "rewrite",
                }
                .into(),
            ),
            ("rows", Json::from(fill.rows as i64)),
            ("batches", Json::from(fill.batches as i64)),
            ("resumed", fill.resumed.into()),
        ])
    });
    Reply::screen(
        screen,
        Json::object([
            ("store", store.into()),
            ("applied", Json::array(ran.iter().map(String::as_str))),
            ("backfills", Json::array(backfills)),
            (
                "removed",
                Json::array(ran.iter().flat_map(|name| {
                    removes.get(name).into_iter().flatten().map(move |removal| {
                        Json::object([
                            ("migration", name.as_str().into()),
                            ("subject", removal.subject().as_str().into()),
                            ("change", removal.change().into()),
                        ])
                    })
                })),
            ),
        ]),
    )
}

/// Runs `work` against the database at `url`, on a runtime of its own.
fn run<T>(
    url: &Address,
    work: impl for<'d> FnOnce(&'d dyn jadpo_runtime::Database) -> jadpo_runtime::LocalFuture<'d, T>,
) -> Result<T, jadpo_runtime::StoreError> {
    let runtime = jadpo_host_native::Runtime::new()
        .map_err(|error| jadpo_runtime::StoreError::Fault(error.to_string()))?;
    let database = url.open()?;
    Ok(runtime.block_on(work(&database)))
}

/// What an extension a migration creates is for, as an error about it
/// says first.
fn extension_purpose(extension: &str) -> String {
    match extension {
        "vector" => "`vector` is pgvector, which stores and searches the project's vectors; there is no fallback, since a search held anywhere else would behave differently from the one that was tested.".to_string(),
        "postgis" => "`postgis` is PostGIS, which keeps the project's geometries in `geography` columns; there is no fallback, since a geometry kept any other way would be measured and compared differently.".to_string(),
        other => format!("`{other}` is a PostgreSQL extension the project's columns need."),
    }
}

pub(crate) fn migrate_failed(store: &str, error: jadpo_runtime::MigrateError) -> Diagnostic {
    use jadpo_runtime::MigrateError;
    match error {
        MigrateError::Unknown { name } => Diagnostic::new(
            &rule::MIGRATION_STORE_DIVERGED,
            format!("`{store}` has applied `{name}`, which this project doesn't have"),
        )
        .reason("The database was migrated by another version of the project, so applying this one's migrations could undo or repeat its changes.")
        .next_for("Bring the project up to date with the code that migrated it, then apply again", Decider::Human),
        MigrateError::Changed { name } => Diagnostic::new(
            &rule::MIGRATION_STORE_DIVERGED,
            format!("`{name}` changed after `{store}` applied it"),
        )
        .reason("A migration's SQL is never changed once it has run anywhere: the database holds what the old SQL did.")
        .next_for(format!("Restore `{name}` as it was when it ran, and make the change in a new migration"), Decider::Human),
        MigrateError::OutOfOrder { name, missing } => Diagnostic::new(
            &rule::MIGRATION_STORE_DIVERGED,
            format!("`{store}` has applied `{name}` but not `{missing}`, which comes before it"),
        )
        .reason("Migrations apply in the order of their names; one added before a migration that already ran would run after it, on a schema it wasn't written for.")
        .next_for(format!("Make `{missing}`'s change in a new migration after `{name}`"), Decider::Human),
        // pgvector's `vector` (area 21, "Without pgvector") and PostGIS's
        // `postgis` (area 20, "A store with no PostGIS").
        MigrateError::ExtensionMissing { extension } => Diagnostic::new(
            &rule::MIGRATION_EXTENSION_MISSING,
            format!("`{store}` has no `{extension}` extension"),
        )
        .reason(format!(
            "{} The project's migrations create it, and its database server neither has it nor offers it, so nothing was applied.",
            extension_purpose(&extension)
        ))
        .next_for(
            match extension.as_str() {
                "vector" => "Install pgvector on the database server, or use a server that has it (the major hosted services do), then run the command again".to_string(),
                "postgis" => "Install PostGIS on the database server, or use a server that has it (the major hosted services do), then run the command again".to_string(),
                other => format!("Install `{other}` on the database server, or use a server that has it, then run the command again"),
            },
            Decider::Human,
        ),
        // Only the latest migration's backfills are checked against the
        // code that works them out (chunk 5.6b2).
        MigrateError::Unworked { name, field } => Diagnostic::new(
            &rule::MIGRATION_APPLY_FAILED,
            format!("Filling `{field}` for `{name}` needs the build it was written in"),
        )
        .reason(format!(
            "`{field}`'s backfill has an operation with no SQL form, so `jadpo migrate apply` works it out in Jadpo with the project's code, and only the latest migration's backfills are checked against that code; `{name}` isn't the latest, so the values it would give with this build are unchecked. Nothing of `{name}` past its last committed step was applied, and `{field}` stays optional."
        ))
        .next_for(
            format!("Run `jadpo migrate apply` with the build that wrote `{name}` (its commit), then with this one"),
            Decider::Human,
        ),
        // A structured field's stored values its type no longer reads
        // (chunk 5.9c), by count and rule, never a value.
        MigrateError::Unreadable {
            name,
            field,
            rows: unread,
            first,
        } => Diagnostic::new(
            &rule::MIGRATION_APPLY_FAILED,
            format!(
                "`{field}` holds {} its type no longer reads",
                if unread == 1 {
                    "1 stored value".to_string()
                } else {
                    format!("{unread} stored values")
                }
            ),
        )
        .reason(format!(
            "`{name}` changes `{field}`'s type so it may not read every value stored before, so `jadpo migrate apply` read each one with it first: {} {} read, the first breaking {first}. Nothing of `{name}` was applied.",
            unread,
            if unread == 1 { "doesn't" } else { "don't" },
        ))
        .next_for(
            format!(
                "Write `{field}: {{ backfill: <value> }}` in `{}/{name}/changes.jadpo` to rewrite them, run `jadpo migrate new`, then apply again",
                records::FOLDER
            ),
            Decider::Human,
        ),
        // Only the latest migration is checked against the code whose
        // types read a structured field's stored values (chunk 5.9c).
        MigrateError::Unchecked { name, field } => Diagnostic::new(
            &rule::MIGRATION_APPLY_FAILED,
            format!("Checking `{field}` for `{name}` needs the build it was written in"),
        )
        .reason(format!(
            "`{name}` changes `{field}`'s type so it may not read every value stored before, so `jadpo migrate apply` reads each one with that type as the project's code declares it, and only the latest migration is checked against that code; `{name}` isn't the latest. Nothing of `{name}` past its last committed step was applied."
        ))
        .next_for(
            format!("Run `jadpo migrate apply` with the build that wrote `{name}` (its commit), then with this one"),
            Decider::Human,
        ),
        MigrateError::BackfillStopped {
            name,
            field,
            record,
            reason,
        } => Diagnostic::new(
            &rule::MIGRATION_APPLY_FAILED,
            format!("Working out `{field}` in Jadpo stopped at record `{record}`"),
        )
        .reason(format!(
            "While `{name}` filled `{field}`, {reason}. That batch rolled back: every batch before it stays, `{field}` stays optional, and the database records how far it got."
        ))
        .next_for(
            "Fix what stopped it, then run `jadpo migrate apply` again",
            Decider::Human,
        ),
        // The server offers it, and the role can't create it (area 20,
        // chunk 5.5u4).
        MigrateError::ExtensionNotCreatable { extension } => Diagnostic::new(
            &rule::MIGRATION_EXTENSION_MISSING,
            format!("`{store}` has no `{extension}` extension, and its role can't create it"),
        )
        .reason(format!(
            "{} The project's migrations create it, and its database server offers it, but the role applying migrations isn't allowed to create it, so nothing was applied.{}",
            extension_purpose(&extension),
            if extension == "postgis" {
                " PostGIS isn't a trusted extension: a superuser, or the provider's own admin role, creates it."
            } else {
                ""
            }
        ))
        .next_for(
            format!("Have a role allowed to create it run `CREATE EXTENSION {extension}` once in this database, then run the command again"),
            Decider::Human,
        ),
        MigrateError::Stopped { name } => Diagnostic::new(
            &rule::MIGRATION_APPLY_FAILED,
            format!("Migrating `{store}` stopped during `{name}`"),
        )
        .reason("It stopped between two of its steps: every batch of its backfill that committed stays, the column stays optional until its last step, and the database records how far it got.")
        .next_for("Run `jadpo migrate apply` again, which carries on where it stopped", Decider::Human),
        // pgvector 0.8.0 for an HNSW index's queries (area 21, chunk
        // 5.5v1b).
        MigrateError::ExtensionOutdated {
            extension,
            found,
            least,
        } => {
            let (named, reason, next) = match extension.as_str() {
                "vector" => (
                    "pgvector".to_string(),
                    format!(
                        "The project's migrations build an HNSW index for a `nearest` query, which runs with pgvector's iterative index scans so a filtered search still fills its limit; they came in pgvector {least}, and the database server offers {found}, so nothing was applied."
                    ),
                    format!(
                        "Upgrade pgvector on the database server to {least} or later (the major hosted services offer it), then run the command again"
                    ),
                ),
                other => (
                    format!("`{other}`"),
                    format!(
                        "The project's migrations need `{other}` {least} or later, and the database server offers {found}, so nothing was applied."
                    ),
                    format!("Upgrade `{other}` on the database server to {least} or later, then run the command again"),
                ),
            };
            let needs = match extension.as_str() {
                "vector" => format!("its nearest-neighbour queries need {least}"),
                _ => format!("its migrations need {least}"),
            };
            Diagnostic::new(
                &rule::MIGRATION_EXTENSION_OUTDATED,
                format!("`{store}` has {named} {found}, and {needs}"),
            )
            .reason(reason)
            .next_for(next, Decider::Human)
        }
        MigrateError::Store(store_error) => {
            let why = crate::schedules::store_error(&store_error);
            let mut letters = why.chars();
            let why = letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect::<String>())
                .unwrap_or_default();
            Diagnostic::new(
                &rule::MIGRATION_APPLY_FAILED,
                format!("Migrating `{store}` failed"),
            )
            .reason(format!("{why}. The step that failed was rolled back; the migrations before it stay applied, as does every batch of a backfill that committed, and the next run carries on from there."))
            .next_for("Fix what stopped it, then run `jadpo migrate apply` again", Decider::Human)
        }
    }
}

fn status(call: &Call) -> Reply {
    let (cwd, loaded) = match load(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let set = records::migrations(&loaded.db, loaded.project);
    let steps = store_steps(set);
    let target = match target(call, &loaded, &steps) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let _ = cwd;
    let mut screen = Screen::new();
    let mut data = Vec::new();
    let mut states: BTreeMap<String, &str> = BTreeMap::new();
    if let Some((store, url)) = &target {
        let list = steps.get(store).map(Vec::as_slice).unwrap_or_default();
        let migrations = list.to_vec();
        let standing = run(url, |database| {
            Box::pin(async move { jadpo_runtime::standing(database, &migrations).await })
        });
        match standing {
            Ok(Ok(standing)) => {
                for name in &standing.applied {
                    states.insert(name.clone(), "applied");
                }
                for name in &standing.pending {
                    states.insert(name.clone(), "pending");
                }
            }
            Ok(Err(error)) => return Reply::failed(migrate_failed(store, error)),
            Err(error) => {
                return Reply::failed(migrate_failed(
                    store,
                    jadpo_runtime::MigrateError::Store(error),
                ));
            }
        }
    }
    let rows: Vec<(String, String)> = set
        .list
        .iter()
        .map(|migration| {
            let state = states
                .get(&migration.name)
                .copied()
                .unwrap_or(if target.is_some() {
                    "no change here"
                } else {
                    ""
                });
            data.push(Json::object([
                ("migration", migration.name.as_str().into()),
                (
                    "state",
                    if state.is_empty() {
                        Json::Null
                    } else {
                        state.into()
                    },
                ),
            ]));
            (migration.name.clone(), state.to_string())
        })
        .collect();
    let count = set.list.len();
    let text = match &target {
        Some((store, _)) => {
            let pending = states.values().filter(|s| **s == "pending").count();
            format!(
                "`{store}`: {} applied, {pending} pending",
                states.values().filter(|s| **s == "applied").count()
            )
        }
        None => format!(
            "{count} {}",
            if count == 1 {
                "migration"
            } else {
                "migrations"
            }
        ),
    };
    screen.push(Block::Status {
        outcome: Outcome::Success,
        text,
        rows,
    });
    if target.is_none() && count > 0 {
        screen.push(Block::hint(
            "jadpo migrate status --url <database>",
            "shows which ones a database has applied",
        ));
    }
    Reply::screen(screen, Json::object([("migrations", Json::Array(data))]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purposes_are_lower_snake_case() {
        assert!(valid_purpose("add_priority") && valid_purpose("initial"));
        assert!(!valid_purpose("AddPriority") && !valid_purpose("add-priority"));
        assert_eq!(snake("AddPriority"), "add_priority");
        assert_eq!(snake("add priority!"), "add_priority");
    }

    #[test]
    fn a_new_changes_file_has_a_line_to_fill_in() {
        let need = Needed {
            store: "main".into(),
            table: "todo".into(),
            column: "rank".into(),
            entity: "Todo".into(),
            field: "rank".into(),
            kind: NeededKind::Retire,
        };
        let model = DataModel::default();
        let table = jadpo_check::types::TypeTable::default();
        let (text, open) = changes_file(None, std::slice::from_ref(&need), &model, &table);
        assert_eq!(open, 1);
        assert!(text.starts_with("migration {\n") && text.contains("    Todo.rank: {}\n"));
        let (again, open) = changes_file(Some(&text), &[need], &model, &table);
        assert_eq!((again, open), (text, 1));
    }
}
