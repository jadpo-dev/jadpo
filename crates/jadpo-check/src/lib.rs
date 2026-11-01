//! Checking a Jadpo project (docs/compiler-runtime.md §2.2): the rules of
//! each language area as salsa queries over `jadpo-db`. Areas 1 to 6 are
//! built: layout, naming, visibility and resolution, then values and types
//! and the logic of bodies ([`types`]), failures (`failures`, with the
//! body rules beside the others in [`types`]), and data, entities, queries
//! and migrations ([`data`]). [`graph`] builds the application graph from
//! these queries.
//!
//! [`check`] assembles every diagnostic for a project, sorted by file and
//! position so the same source always gives the same output.

pub mod acceptances;
pub mod ai;
pub mod ai_limits;
pub mod ai_mapping;
pub mod ai_personal;
pub mod application;
pub mod batches;
mod case;
pub mod client_address;
pub mod completion;
pub mod config;
pub mod connections;
pub mod constants;
pub mod data;
pub mod databases;
pub mod declines;
pub mod deployables;
pub mod disk;
pub mod emails;
pub mod erasure;
pub mod events;
mod failures;
pub mod fixes;
pub mod flag_remove;
pub mod flags;
pub mod formatting;
pub mod graph;
pub mod identity;
pub mod imports;
pub mod jadpo_version;
mod layout;
pub mod locales;
pub mod lookups;
pub mod mail;
pub mod mail_health;
mod naming;
mod not_type;
mod not_value;
pub mod package_code;
pub mod packages;
pub mod personal_data;
pub mod persons;
pub mod policy;
pub mod polls;
pub mod prompt_recordings;
pub mod prompts;
pub mod providers;
pub mod publishing;
pub mod rate_limits;
pub mod receiving;
pub mod records;
mod resolve;
pub mod review;
pub mod routes;
pub mod schedules;
pub mod setting_move;
pub mod sign_in_providers;
mod std_lib;
pub mod suggest;
pub mod surface;
pub mod translations;
mod tree;
pub mod types;
mod usage;
mod visibility;
pub mod webhooks;

pub use failures::{
    DECLARABLE_KINDS, FailureInfo, FailureTable, MEMBERS as FAILURE_MEMBERS, failure_problems,
    failure_table,
};
pub use imports::{apply_edits, fix_imports, import_edits, import_problems};
pub use layout::layout_problems;
pub use naming::{domain_problems, naming_problems};
pub use resolve::resolve_file;
pub use usage::{mentions, usage_problems, users};

use jadpo_db::{Db, Project, item_tree, parse};
use jadpo_diag::{Diagnostic, rule};

pub use std_lib::{StandardStub, StdOperation, StubSignature};

/// Every standard-library operation the stubs declare, with its
/// parameters' kinds and its result's: each must match its entry in
/// `jadpo-std`'s operation table, which binds it to one Rust function in
/// `jadpo-value`.
pub fn standard_operations() -> Vec<StubSignature> {
    std_lib::StdNames::get().signatures()
}

/// Every standard-library operation the stubs declare, with its parameters
/// and result as written and its description, for the type reference.
pub fn documented_operations() -> Vec<StdOperation> {
    std_lib::StdNames::get().documented()
}

/// The standard types' stub declarations (`std/standard.jadpo`), by name,
/// for the test that holds them to their data entries in `jadpo-std`
/// (area 19).
pub fn standard_stubs() -> std::collections::BTreeMap<String, StandardStub> {
    std_lib::StdNames::get().standard().clone()
}

/// Every problem in `project`, in a stable order. Syntax errors come alone,
/// since a file that does not parse would only add knock-on errors, unless
/// every one is a habit from another language that parsed as its Jadpo
/// form (`Parse::checkable`): then the rest of the project is checked too,
/// and no other problem is reported on a line with a syntax error. A
/// finding a `decline` line answers is left out ([`declines`]). A project
/// whose `application { jadpo: }` needs a newer Jadpo gets that error alone
/// ([`jadpo_version`]).
pub fn check(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    // A project needing a newer Jadpo is refused with that alone: what a
    // later version added would only read as mistakes here.
    let version = jadpo_version::version_problems(db, project);
    if version.iter().any(jadpo_version::refuses) {
        let mut out: Vec<Diagnostic> = version
            .iter()
            .filter(|d| jadpo_version::refuses(d))
            .cloned()
            .collect();
        sort(&mut out);
        return out;
    }
    let files = project.files(db);
    // A domain folder named with a reserved word is reported at the
    // folder, and the word's uses as the domain's name are left out: they
    // are that one mistake again (area 1, "Reserved words").
    let reserved = naming::reserved_word_domains(db, project);
    let syntax: Vec<Diagnostic> = files
        .iter()
        .flat_map(|file| {
            let path = file.path(db);
            let text = file.text(db);
            parse(db, *file)
                .errors()
                .iter()
                .filter(|error| {
                    error.rule.id != rule::SYNTAX_EXPECTED_NAME.id
                        || !reserved.iter().any(|(domain, _)| {
                            text.get(error.start as usize..error.end as usize) == Some(domain)
                                && text
                                    .get(error.end as usize..)
                                    .is_some_and(|rest| rest.starts_with('.'))
                        })
                })
                .map(|error| error.to_diagnostic(path))
                .chain(acceptances::files::quoted_keys(db, *file))
                .collect::<Vec<_>>()
        })
        .collect();
    let checkable = files
        .iter()
        .all(|file| parse(db, *file).checkable_reading_names());
    let mut out = Vec::new();
    if syntax.is_empty() || checkable {
        if files
            .iter()
            .all(|file| item_tree(db, *file).items.is_empty())
        {
            out.push(empty_project(files.len()));
        }
        out.extend(domain_problems(db, project).iter().cloned());
        // An installed package's file this compiler already checked with no
        // problem at the package's digest isn't checked again (the package
        // cache, chunk 7.1c2): a package's code sees only its own names, so
        // its check doesn't depend on the app.
        let cached = project.checked(db);
        for &file in files {
            if cached.contains(file.path(db)) {
                continue;
            }
            // An acceptance names what changed, not declarations to
            // resolve: [`acceptances`] reads it (area 18).
            if project.layout(db) == jadpo_db::Layout::Project
                && acceptances::files::is_acceptance(file.path(db))
            {
                continue;
            }
            out.extend(layout_problems(db, project, file).iter().cloned());
            out.extend(
                packages::unbuilt_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                package_code::action_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(naming_problems(db, project, file).iter().cloned());
            out.extend(resolve_file(db, project, file).iter().cloned());
            out.extend(import_problems(db, project, file).iter().cloned());
            out.extend(usage_problems(db, project, file).iter().cloned());
            out.extend(
                visibility::public_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                visibility::public_reach_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(config::use_problems(db, project, file).iter().cloned());
            out.extend(types::type_problems(db, project, file).iter().cloned());
            out.extend(
                constants::constant_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(failure_problems(db, project, file).iter().cloned());
            out.extend(types::body_problems(db, project, file).iter().cloned());
            out.extend(types::recursion_problems(db, project, file).iter().cloned());
            out.extend(
                types::call_order_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(types::step_problems(db, project, file).iter().cloned());
            out.extend(types::store_problems(db, project, file).iter().cloned());
            out.extend(
                types::batch_store_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(types::file_run_problems(db, project, file).iter().cloned());
            out.extend(graph::similar_problems(db, project, file).iter().cloned());
            out.extend(data::data_problems(db, project, file).iter().cloned());
            out.extend(
                persons::personal_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                data::retention::retention_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(erasure::erasure_problems(db, project, file).iter().cloned());
            out.extend(
                data::audit::audit_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                data::queries::query_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(routes::route_problems(db, project, file).iter().cloned());
            out.extend(
                events::subscriber_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(events::emit_problems(db, project, file).iter().cloned());
            out.extend(
                schedules::schedule_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                batches::batch_grant_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                policy::reach::reach_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(policy::policy_problems(db, project, file).iter().cloned());
            out.extend(
                policy::calls::ungranted_call_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                policy::outputs::query_policy_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                policy::flow::flow_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                policy::flow::subscriber_store_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                policy::flow::model_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
        }
        out.extend(config::config_problems(db, project).iter().cloned());
        out.extend(flags::problems(db, project).iter().cloned());
        out.extend(version.iter().cloned());
        out.extend(config::connection_problems(db, project).iter().cloned());
        out.extend(application::section_problems(db, project).iter().cloned());
        out.extend(config::test_value_problems(db, project).iter().cloned());
        out.extend(identity::identity_problems(db, project).iter().cloned());
        out.extend(identity::sign_in_problems(db, project).iter().cloned());
        out.extend(
            client_address::client_address_problems(db, project)
                .iter()
                .cloned(),
        );
        out.extend(locales::locale_problems(db, project).iter().cloned());
        out.extend(
            suggest::settings::settings_problems(db, project)
                .iter()
                .cloned(),
        );
        out.extend(
            translations::translation_problems(db, project)
                .iter()
                .cloned(),
        );
        out.extend(prompts::prompt_problems(db, project).iter().cloned());
        out.extend(emails::email_problems(db, project).iter().cloned());
        out.extend(disk::disk_problems(db, project).iter().cloned());
        out.extend(data::held::counting_problems(db, project).iter().cloned());
        out.extend(ai::ai_problems(db, project).iter().cloned());
        out.extend(routes::api_problems(db, project).iter().cloned());
        out.extend(
            events::events_settings(db, project)
                .problems
                .iter()
                .cloned(),
        );
        out.extend(
            schedules::schedules_settings(db, project)
                .problems
                .iter()
                .cloned(),
        );
        out.extend(types::steps_settings(db, project).problems.iter().cloned());
        out.extend(routes::table_problems(db, project).iter().cloned());
        out.extend(routes::signin_problems(db, project).iter().cloned());
        out.extend(records::record_problems(db, project).iter().cloned());
        out.extend(acceptances::problems(db, project).iter().cloned());
        out.extend(packages::package_problems(db, project).iter().cloned());
        out.extend(providers::operation_problems(db, project).iter().cloned());
        out.extend(
            connections::connection_problems(db, project)
                .iter()
                .cloned(),
        );
        out.extend(webhooks::webhooks(db, project).problems.iter().cloned());
        out.extend(events::system_run_problems(db, project).iter().cloned());
        out.extend(receiving::mappings(db, project).problems.iter().cloned());
        out.extend(mail::mappings(db, project).problems.iter().cloned());
        out.extend(ai_mapping::mappings(db, project).problems.iter().cloned());
        out.extend(receiving::receiving(db, project).problems.iter().cloned());
        out.extend(polls::declared(db, project).problems.iter().cloned());
        out.extend(polls::polls(db, project).problems.iter().cloned());
        out.extend(rate_limits::rate_limits_problems(db, project));
        out.extend(
            receiving::cloudflare_queues(db, project)
                .problems
                .iter()
                .cloned(),
        );
        out.extend(types::call_problems(db, project).iter().cloned());
        out.extend(
            deployables::deployable_problems(db, project)
                .iter()
                .cloned(),
        );
        out.extend(
            data::migrations::migration_problems(db, project)
                .iter()
                .cloned(),
        );
        out = declines::answer(db, project, out, syntax.is_empty());
    }
    if !syntax.is_empty() {
        let line = |path: &str, offset: u32| {
            files
                .iter()
                .find(|file| file.path(db) == path)
                .map(|file| line_of(file.text(db), offset))
        };
        let broken: std::collections::HashSet<(String, Option<usize>)> = syntax
            .iter()
            .filter_map(|d| d.primary.as_ref())
            .map(|label| {
                let path = label.span.path.clone();
                let at = line(&path, label.span.start);
                (path, at)
            })
            .collect();
        out.retain(|d| {
            d.primary.as_ref().is_none_or(|label| {
                let path = label.span.path.clone();
                let at = line(&path, label.span.start);
                !broken.contains(&(path, at))
            })
        });
        out.extend(syntax);
    }
    out.extend(reserved.iter().map(|(_, problem)| problem.clone()));
    sort(&mut out);
    out
}

/// Whether `diagnostic` stops a program running (`jadpo test`, `jadpo
/// dev`): every error but a record out of date or edited, which is a
/// promise that `jadpo check` and CI hold to (area 18), not a reason the
/// program can't run, and an acceptance a change still needs.
pub fn blocks_running(diagnostic: &Diagnostic) -> bool {
    diagnostic.rule.id != rule::RECORD_OUT_OF_DATE.id
        && diagnostic.rule.id != rule::RECORD_EDITED.id
        // A change waiting for a person's acceptance still runs (area 18).
        && !diagnostic.rule.id.starts_with("acceptance.")
}

/// The zero-based line `offset` is on.
fn line_of(text: &str, offset: u32) -> usize {
    let end = (offset as usize).min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// By file, then position, then rule, so output never depends on the
/// order checks ran in; repeats are dropped.
fn sort(diagnostics: &mut Vec<Diagnostic>) {
    let key = |d: &Diagnostic| {
        let (path, start, end) = d.primary.as_ref().map_or((String::new(), 0, 0), |label| {
            (label.span.path.clone(), label.span.start, label.span.end)
        });
        (path, start, end, d.rule.id, d.message.clone())
    };
    diagnostics.sort_by_key(key);
    diagnostics.dedup();
}

fn empty_project(files: usize) -> Diagnostic {
    let reason = match files {
        0 => "Jadpo found no `.jadpo` files.".to_string(),
        1 => "Its one `.jadpo` file holds no declarations, only comments or nothing at all."
            .to_string(),
        n => {
            format!("Its {n} `.jadpo` files hold no declarations, only comments or nothing at all.")
        }
    };
    Diagnostic::new(
        &rule::TOOLCHAIN_EMPTY_PROJECT,
        "This project declares nothing",
    )
    .reason(reason)
    .next("Add a declaration, such as a `type`, `function` or `action`")
}

/// `type.foreign_value`: `null`, `None` or `True` written for a value Jadpo
/// spells `none`, `true` or `false`, when
/// `written` is one; the caller has checked the name means nothing else.
pub(crate) fn foreign_value(written: &str, span: jadpo_diag::Span) -> Option<Diagnostic> {
    let (meaning, jadpo) = jadpo_syntax::habits::foreign_value(written)?;
    let habit = &jadpo_syntax::habits::FOREIGN_VALUE;
    Some(
        Diagnostic::new(habit.rule, habit.message(written, jadpo))
            .at(span.clone(), habit.label_for(written, jadpo))
            .reason(jadpo_syntax::habits::foreign_reason(
                written,
                meaning,
                jadpo,
                "writes it",
            ))
            .next(habit.next_for(written, jadpo))
            .edit(span, jadpo),
    )
}

pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

pub(crate) fn or_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
    }
}
