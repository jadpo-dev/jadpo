// CLI helpers return the complete structured diagnostic so every renderer sees
// the same repair protocol; keep that contract intact at command boundaries.
#![allow(clippy::result_large_err)]

use jadpo_core::{
    accept_index_recommendation, analyze_project, check_local_configuration,
    checked_source_revision, configuration_fields, create_project, derive_artifacts, derive_target,
    diff_schema_identities, discover_sources, format_source, index_recommendation_count,
    index_recommendations_json, initialize_schema_identities, local_configuration_environment,
    register_schema_additions, rename_schema_identity, set_local_configuration,
    snapshot_schema_identities, validate_schema_decisions, validate_schema_identities,
    write_artifacts, write_schema_decision_template, write_schema_migration_plan,
    write_schema_migration_sql_review, AnalyzedProject,
};
use jadpo_diagnostics::{catalogue_definition, json_string, Diagnostic, DiagnosticFact};
use jadpo_semantic::checked_manifest_json;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

mod experimental_build;
mod lsp;
mod terminal;

use terminal::{ColorChoice, LayoutChoice, Presentation};

// Bun 1.2.20 does not recognize --no-env-file. An explicit empty file
// disables ambient dotenv discovery while retaining the supplied environment.
const BUN_EMPTY_ENV_FILE: &str = if cfg!(windows) {
    "--env-file=NUL"
} else {
    "--env-file=/dev/null"
};

fn main() -> ExitCode {
    let raw_arguments = env::args().skip(1).collect::<Vec<_>>();
    let (arguments, presentation) = match presentation_arguments(raw_arguments) {
        Ok(result) => result,
        Err(diagnostic) => {
            terminal::configure(Presentation::default());
            print_human_diagnostic(&diagnostic);
            return ExitCode::from(1);
        }
    };
    terminal::configure(presentation);
    let requests_json = arguments
        .iter()
        .any(|argument| argument == "--diagnostic-format=json");
    if requests_json {
        match arguments.first().map(String::as_str) {
            Some("check") => return run_json_check(&arguments),
            Some("watch" | "dev") => {}
            _ => {
                print_human_diagnostic(
                    &Diagnostic::error("CLI_PRESENTATION_ARGUMENTS").with_note(
                        "--diagnostic-format=json is supported by check, watch, and dev; other machine-oriented commands already emit JSON",
                    ),
                );
                return ExitCode::from(1);
            }
        }
    }

    match run(arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(diagnostic) => {
            print_human_diagnostic(&diagnostic);
            ExitCode::from(1)
        }
    }
}

fn presentation_arguments(
    arguments: Vec<String>,
) -> Result<(Vec<String>, Presentation), Diagnostic> {
    let mut presentation = Presentation::default();
    let mut normalized = Vec::with_capacity(arguments.len());
    let mut diagnostic_format = None;
    let mut color_choice = None;

    for argument in arguments {
        if let Some(value) = argument.strip_prefix("--diagnostic-format=") {
            if diagnostic_format.replace(value.to_owned()).is_some() {
                return Err(Diagnostic::error("CLI_PRESENTATION_ARGUMENTS")
                    .with_note("choose exactly one diagnostic format"));
            }
            match value {
                "json" => normalized.push(argument),
                "plain" => presentation.layout = LayoutChoice::Plain,
                "human" => presentation.layout = LayoutChoice::Human,
                _ => {
                    return Err(Diagnostic::error("CLI_PRESENTATION_ARGUMENTS")
                        .with_note("expected --diagnostic-format=human|plain|json"));
                }
            }
            continue;
        }
        if let Some(value) = argument.strip_prefix("--color=") {
            if color_choice.replace(value.to_owned()).is_some() {
                return Err(Diagnostic::error("CLI_PRESENTATION_ARGUMENTS")
                    .with_note("choose exactly one colour mode"));
            }
            presentation.color = match value {
                "auto" => ColorChoice::Auto,
                "always" => ColorChoice::Always,
                "never" => ColorChoice::Never,
                _ => {
                    return Err(Diagnostic::error("CLI_PRESENTATION_ARGUMENTS")
                        .with_note("expected --color=auto|always|never"));
                }
            };
            continue;
        }
        normalized.push(argument);
    }
    Ok((normalized, presentation))
}

fn run(arguments: Vec<String>) -> Result<(), Diagnostic> {
    let Some(command) = arguments.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };

    if matches!(command, "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }

    if command == "schema" {
        return run_schema(&arguments);
    }

    if command == "lsp" {
        if arguments.len() != 1 {
            return Err(Diagnostic::error("CLI_LSP_ARGUMENTS"));
        }
        return lsp::run_stdio();
    }

    if command == "config" {
        return run_config(&arguments);
    }

    if command == "build" {
        return experimental_build::run(&arguments[1..]);
    }

    if command == "protected-attempt" {
        println!("{}", protected_attempt_export(&arguments[1..])?);
        return Ok(());
    }

    if command == "approval" {
        println!("{}", approval_export(&arguments[1..])?);
        return Ok(());
    }

    let project = arguments
        .get(1)
        .ok_or_else(|| Diagnostic::error("CLI_PROJECT_REQUIRED"))?;
    let project = Path::new(project);

    match command {
        "new" => {
            let files = create_project(project)?;
            print_success(
                &format!(
                    "created project {} with {} deterministic file(s)",
                    project.display(),
                    files.len()
                ),
                "Project created",
                &format!(
                    "{} · {}",
                    project.display(),
                    terminal::plural(files.len(), "file", "files")
                ),
            );
            Ok(())
        }
        "check" => {
            if arguments.len() != 2 {
                return Err(Diagnostic::error(
                    "CLI_CHECK_ARGUMENTS")
                .with_note("expected: jadpo check <project> [--diagnostic-format=json]"));
            }
            run_human_check(project)
        }
        "inspect" => {
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            validate_schema_identities(project, &analyzed)?;
            println!(
                "{}",
                checked_manifest_json(&analyzed.semantics, &analyzed.typing, &analyzed.failures)
            );
            Ok(())
        }
        "artifacts" => {
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            validate_schema_identities(project, &analyzed)?;
            let artifacts = derive_artifacts(project, &analyzed);
            let output = write_artifacts(project, &artifacts)?;
            print_success(
                &format!(
                    "generated {} deterministic artifact(s) beneath {}",
                    artifacts.len(),
                    output.display()
                ),
                "Artifacts generated",
                &format!(
                    "{} · {}",
                    terminal::plural(artifacts.len(), "artifact", "artifacts"),
                    output.display()
                ),
            );
            Ok(())
        }
        "incident" => {
            if arguments.len() != 3 {
                return Err(Diagnostic::error(
                    "CLI_INCIDENT_ARGUMENTS")
                .with_note("expected: jadpo incident <project> <event-json-file>"));
            }
            run_incident_enrichment(project, Path::new(&arguments[2]))
        }
        "test" => run_human_test(project),
        "fmt" => {
            let check = match arguments.get(2).map(String::as_str) {
                None => false,
                Some("--check") => true,
                Some(_) => {
                    return Err(Diagnostic::error(
                        "CLI_FMT_ARGUMENTS"));
                }
            };
            if arguments.len() > 3 {
                return Err(Diagnostic::error(
                    "CLI_FMT_ARGUMENTS"));
            }
            run_format(project, check)
        }
        "watch" => {
            if project == Path::new("--diagnostic-format=json") {
                return Err(Diagnostic::error(
                    "CLI_PROJECT_REQUIRED"));
            }
            let json = match arguments.get(2).map(String::as_str) {
                None => false,
                Some("--diagnostic-format=json") => true,
                Some(_) => {
                    return Err(Diagnostic::error(
                        "CLI_WATCH_ARGUMENTS")
                    .with_note("expected: jadpo watch <project> [--diagnostic-format=json]"));
                }
            };
            if arguments.len() > 3 {
                return Err(Diagnostic::error(
                    "CLI_WATCH_ARGUMENTS")
                .with_note("expected: jadpo watch <project> [--diagnostic-format=json]"));
            }
            run_watch(project, json)
        }
        "dev" => {
            if project == Path::new("--diagnostic-format=json") {
                return Err(Diagnostic::error(
                    "CLI_PROJECT_REQUIRED"));
            }
            let json = match arguments.get(2).map(String::as_str) {
                None => false,
                Some("--diagnostic-format=json") => true,
                Some(_) => {
                    return Err(Diagnostic::error(
                        "CLI_DEV_ARGUMENTS")
                    .with_note("expected: jadpo dev <project> [--diagnostic-format=json]"));
                }
            };
            if arguments.len() > 3 {
                return Err(Diagnostic::error(
                    "CLI_DEV_ARGUMENTS")
                .with_note("expected: jadpo dev <project> [--diagnostic-format=json]"));
            }
            run_dev(project, json)
        }
        _other => Err(Diagnostic::error(
            "CLI_UNKNOWN_COMMAND")
        .with_note(
            "expected one of: new, check, inspect, artifacts, build, test, fmt, watch, dev, config, schema, help",
        )),
    }
}

fn run_config(arguments: &[String]) -> Result<(), Diagnostic> {
    let subcommand = arguments.get(1).ok_or_else(|| {
        Diagnostic::error("CLI_CONFIG_ARGUMENTS")
            .with_note("expected: jadpo config <set <field>|check>")
    })?;
    let project_path =
        env::current_dir().map_err(|_| Diagnostic::error("JADPO_PROJECT_READ_FAILED"))?;
    let checked = checked_project(&project_path).map_err(|mut report| {
        report
            .diagnostics
            .pop()
            .expect("a failed configuration check must contain a diagnostic")
    })?;

    match subcommand.as_str() {
        "set" if arguments.len() == 3 => {
            let field_name = &arguments[2];
            let fields = configuration_fields(&checked.analyzed);
            let field = fields
                .iter()
                .find(|field| field.name == *field_name)
                .ok_or_else(|| {
                    Diagnostic::error("CONFIG_FIELD_UNKNOWN")
                        .with_fact(DiagnosticFact::Field(field_name.clone()))
                })?;
            let value = prompt_configuration_value(&field.binding, field.secret)?;
            let configured =
                set_local_configuration(&project_path, &checked.analyzed, field_name, &value)?;
            print_success(
                &format!("configured {} locally", configured.name),
                "Local configuration saved",
                &format!("{} · .env.local", configured.name),
            );
            Ok(())
        }
        "check" if arguments.len() == 2 => {
            let statuses = check_local_configuration(&project_path, &checked.analyzed)?;
            for status in &statuses {
                println!("{}: {}", status.field, status.state.as_str());
            }
            let invalid = statuses
                .iter()
                .filter(|status| !status.state.is_valid())
                .count();
            if invalid > 0 {
                return Err(
                    Diagnostic::error("CONFIG_LOCAL_CHECK_FAILED").with_note(format!(
                        "{invalid} configuration field(s) are missing or invalid"
                    )),
                );
            }
            print_success(
                &format!("validated {} local configuration field(s)", statuses.len()),
                "Local configuration valid",
                &terminal::plural(statuses.len(), "field", "fields"),
            );
            Ok(())
        }
        _ => Err(Diagnostic::error("CLI_CONFIG_ARGUMENTS")
            .with_note("expected: jadpo config <set <field>|check>")),
    }
}

fn prompt_configuration_value(binding: &str, secret: bool) -> Result<String, Diagnostic> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(Diagnostic::error("CLI_CONFIG_TTY_REQUIRED"));
    }
    eprint!("Enter {binding}: ");
    io::stderr()
        .flush()
        .map_err(|_| Diagnostic::error("CLI_CONFIG_PROMPT_FAILED"))?;
    if secret {
        set_terminal_echo(false)?;
    }
    let mut value = String::new();
    let read = io::stdin().read_line(&mut value);
    let restore = if secret {
        let result = set_terminal_echo(true);
        eprintln!();
        result
    } else {
        Ok(())
    };
    restore?;
    read.map_err(|_| Diagnostic::error("CLI_CONFIG_PROMPT_FAILED"))?;
    while value.ends_with(['\n', '\r']) {
        value.pop();
    }
    Ok(value)
}

#[cfg(unix)]
fn set_terminal_echo(enabled: bool) -> Result<(), Diagnostic> {
    let status = Command::new("stty")
        .arg(if enabled { "echo" } else { "-echo" })
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| Diagnostic::error("CLI_CONFIG_PROMPT_FAILED"))?;
    if status.success() {
        Ok(())
    } else {
        Err(Diagnostic::error("CLI_CONFIG_PROMPT_FAILED"))
    }
}

#[cfg(not(unix))]
fn set_terminal_echo(_enabled: bool) -> Result<(), Diagnostic> {
    Err(Diagnostic::error("CLI_CONFIG_PROMPT_FAILED"))
}

fn run_schema(arguments: &[String]) -> Result<(), Diagnostic> {
    let subcommand = arguments.get(1).ok_or_else(|| {
        Diagnostic::error(
            "CLI_SCHEMA_COMMAND_REQUIRED")
        .with_note("expected: jadpo schema <init|check|add|rename|snapshot|diff|decision-template|decision-check|plan|sql|index-recommend|index-accept> <project> ...")
    })?;
    let project = arguments
        .get(2)
        .ok_or_else(|| Diagnostic::error("CLI_PROJECT_REQUIRED"))?;
    let project = Path::new(project);
    match subcommand.as_str() {
        "init" => {
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let path = initialize_schema_identities(project, &analyzed)?;
            print_success(
                &format!("initialized schema identities at {}", path.display()),
                "Schema identities initialized",
                &path.display().to_string(),
            );
            Ok(())
        }
        "check" => {
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let path = validate_schema_identities(project, &analyzed)?.ok_or_else(|| {
                Diagnostic::error(
                    "MIG_IDENTITY_REGISTRY_MISSING")
                .with_note("run `jadpo schema init <project>` first")
            })?;
            print_success(
                &format!("schema identities match checked source: {}", path.display()),
                "Schema identities verified",
                &path.display().to_string(),
            );
            Ok(())
        }
        "add" => {
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let (path, count) = register_schema_additions(project, &analyzed)?;
            print_success(
                &format!(
                    "registered {count} additive schema identity entry(s) in {}",
                    path.display()
                ),
                "Schema additions registered",
                &format!(
                    "{} · {}",
                    terminal::plural(count, "identity", "identities"),
                    path.display()
                ),
            );
            Ok(())
        }
        "snapshot" => {
            let output = arguments.get(3).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_SNAPSHOT_ARGUMENTS")
                .with_note("expected: jadpo schema snapshot <project> <output>")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let path = snapshot_schema_identities(project, &analyzed, Path::new(output))?;
            print_success(
                &format!(
                    "wrote immutable schema identity snapshot to {}",
                    path.display()
                ),
                "Schema snapshot written",
                &path.display().to_string(),
            );
            Ok(())
        }
        "diff" => {
            if arguments.get(3).map(String::as_str) != Some("--against") {
                return Err(Diagnostic::error(
                    "CLI_SCHEMA_DIFF_ARGUMENTS")
                .with_note("expected: jadpo schema diff <project> --against <snapshot>"));
            }
            let previous = arguments.get(4).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_DIFF_ARGUMENTS")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            print!(
                "{}",
                diff_schema_identities(project, &analyzed, Path::new(previous))?
            );
            Ok(())
        }
        "decision-template" => {
            if arguments.get(3).map(String::as_str) != Some("--against") {
                return Err(Diagnostic::error(
                    "CLI_SCHEMA_DECISION_ARGUMENTS"));
            }
            let previous = arguments.get(4).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_DECISION_ARGUMENTS")
            })?;
            let output = arguments.get(5).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_DECISION_ARGUMENTS")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let (path, count) = write_schema_decision_template(
                project,
                &analyzed,
                Path::new(previous),
                Path::new(output),
            )?;
            print_success(
                &format!(
                    "wrote {count} unresolved schema decision(s) to {}",
                    path.display()
                ),
                "Decision template written",
                &format!(
                    "{} · {}",
                    terminal::plural(count, "decision", "decisions"),
                    path.display()
                ),
            );
            Ok(())
        }
        "decision-check" => {
            if arguments.get(3).map(String::as_str) != Some("--against") {
                return Err(Diagnostic::error(
                    "CLI_SCHEMA_DECISION_ARGUMENTS"));
            }
            let previous = arguments.get(4).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_DECISION_ARGUMENTS")
            })?;
            let decisions = arguments.get(5).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_DECISION_ARGUMENTS")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            print!(
                "{}",
                validate_schema_decisions(
                    project,
                    &analyzed,
                    Path::new(previous),
                    Path::new(decisions),
                )?
            );
            Ok(())
        }
        "plan" => {
            if arguments.get(3).map(String::as_str) != Some("--against")
                || arguments.get(5).map(String::as_str) != Some("--decisions")
                || arguments.get(7).map(String::as_str) != Some("--adapter")
            {
                return Err(Diagnostic::error(
                    "CLI_SCHEMA_PLAN_ARGUMENTS"));
            }
            let previous = &arguments[4];
            let decisions = &arguments[6];
            let adapter = arguments.get(8).ok_or_else(|| {
                Diagnostic::error("CLI_SCHEMA_PLAN_ARGUMENTS")
            })?;
            let output = arguments.get(9).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_PLAN_ARGUMENTS")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let (path, steps, irreversible) = write_schema_migration_plan(
                project,
                &analyzed,
                Path::new(previous),
                Path::new(decisions),
                adapter,
                Path::new(output),
            )?;
            print_success(
                &format!(
                    "wrote non-executable {adapter} migration plan with {steps} step(s), {irreversible} irreversible, to {}",
                    path.display()
                ),
                "Migration plan written",
                &format!(
                    "{adapter} · {} · {irreversible} irreversible · {}",
                    terminal::plural(steps, "step", "steps"),
                    path.display()
                ),
            );
            Ok(())
        }
        "sql" => {
            if arguments.get(3).map(String::as_str) != Some("--against")
                || arguments.get(5).map(String::as_str) != Some("--decisions")
                || arguments.get(7).map(String::as_str) != Some("--adapter")
            {
                return Err(Diagnostic::error(
                    "CLI_SCHEMA_SQL_ARGUMENTS"));
            }
            let previous = &arguments[4];
            let decisions = &arguments[6];
            let adapter = arguments.get(8).ok_or_else(|| {
                Diagnostic::error("CLI_SCHEMA_SQL_ARGUMENTS")
            })?;
            let output = arguments.get(9).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_SQL_ARGUMENTS")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let (path, forward, rollback) = write_schema_migration_sql_review(
                project,
                &analyzed,
                Path::new(previous),
                Path::new(decisions),
                adapter,
                Path::new(output),
            )?;
            print_success(
                &format!(
                    "wrote non-executable {adapter} SQL review with {forward} forward and {rollback} rollback statement(s) to {}",
                    path.display()
                ),
                "SQL review written",
                &format!(
                    "{adapter} · {forward} forward · {rollback} rollback · {}",
                    path.display()
                ),
            );
            Ok(())
        }
        "index-recommend" => {
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            print!("{}", index_recommendations_json(&analyzed));
            Ok(())
        }
        "index-accept" => {
            let path = arguments.get(3).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_INDEX_ACCEPT_ARGUMENTS")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let (source, registry) = accept_index_recommendation(project, &analyzed, path)?;
            print_success(
                &format!(
                    "accepted index recommendation `{path}` in {}; registered identity in {}",
                    source.display(),
                    registry.display()
                ),
                "Index recommendation accepted",
                &format!("{path} · {} · {}", source.display(), registry.display()),
            );
            Ok(())
        }
        "rename" => {
            let kind = arguments.get(3).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_RENAME_ARGUMENTS")
                .with_note("expected: jadpo schema rename <project> <entity|field> <old> <new>")
            })?;
            let old_path = arguments.get(4).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_RENAME_ARGUMENTS")
            })?;
            let new_path = arguments.get(5).ok_or_else(|| {
                Diagnostic::error(
                    "CLI_SCHEMA_RENAME_ARGUMENTS")
            })?;
            let analyzed = analyze_project(project)?;
            require_valid_frontend(&analyzed)?;
            let path = rename_schema_identity(project, &analyzed, kind, old_path, new_path)?;
            print_success(
                &format!(
                    "renamed schema {kind} identity `{old_path}` to `{new_path}` in {}",
                    path.display()
                ),
                "Schema identity renamed",
                &format!("{kind} · {old_path} -> {new_path} · {}", path.display()),
            );
            Ok(())
        }
        _other => Err(Diagnostic::error(
            "CLI_UNKNOWN_SCHEMA_COMMAND")
        .with_note(
            "expected: jadpo schema <init|check|add|rename|snapshot|diff|decision-template|decision-check|plan|sql|index-recommend|index-accept> <project> ...",
        )),
    }
}

fn print_help() {
    print!("{}", terminal::render_help(terminal::stdout_style()));
}

fn print_success(plain: &str, title: &str, detail: &str) {
    print!(
        "{}",
        terminal::render_success(plain, title, detail, terminal::stdout_style())
    );
}

const DIAGNOSTIC_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CheckSummary {
    source_files: usize,
    declarations: usize,
    semantic_nodes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckReport {
    passed: bool,
    diagnostics: Vec<Diagnostic>,
    summary: Option<CheckSummary>,
}

struct CheckedProject {
    analyzed: AnalyzedProject,
    diagnostics: Vec<Diagnostic>,
    summary: CheckSummary,
}

#[derive(Debug)]
struct BuildSuccess {
    analyzed: AnalyzedProject,
    diagnostics: Vec<Diagnostic>,
    summary: CheckSummary,
    artifact_count: usize,
    output: PathBuf,
}

fn run_json_check(arguments: &[String]) -> ExitCode {
    let project = match arguments {
        [command, project, format]
            if command == "check" && format == "--diagnostic-format=json" =>
        {
            Path::new(project)
        }
        [command, format] if command == "check" && format == "--diagnostic-format=json" => {
            let report = failed_report(Diagnostic::error("CLI_PROJECT_REQUIRED"));
            println!("{}", check_report_json(Path::new(""), &report));
            return ExitCode::from(1);
        }
        _ => {
            let report = failed_report(
                Diagnostic::error("CLI_CHECK_ARGUMENTS")
                    .with_note("expected: jadpo check <project> --diagnostic-format=json"),
            );
            println!("{}", check_report_json(Path::new(""), &report));
            return ExitCode::from(1);
        }
    };

    let report = check_project(project);
    println!("{}", check_report_json(project, &report));
    if report.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_human_check(project: &Path) -> Result<(), Diagnostic> {
    let mut report = check_project(project);
    if !report.passed {
        let terminal = report
            .diagnostics
            .pop()
            .expect("a failed check report must contain a diagnostic");
        for diagnostic in report.diagnostics {
            print_human_diagnostic(&diagnostic);
        }
        return Err(terminal);
    }

    for diagnostic in &report.diagnostics {
        print_human_diagnostic(diagnostic);
    }
    let summary = report
        .summary
        .expect("a successful check report must contain a summary");
    print!(
        "{}",
        terminal::render_check_success(
            summary.source_files,
            summary.declarations,
            summary.semantic_nodes,
            terminal::stdout_style(),
        )
    );
    Ok(())
}

fn check_project(project: &Path) -> CheckReport {
    match checked_project(project) {
        Ok(checked) => CheckReport {
            passed: true,
            diagnostics: checked.diagnostics,
            summary: Some(checked.summary),
        },
        Err(report) => report,
    }
}

fn checked_project(project: &Path) -> Result<CheckedProject, CheckReport> {
    let analyzed = match analyze_project(project) {
        Ok(analyzed) => analyzed,
        Err(diagnostic) => return Err(failed_report(diagnostic)),
    };
    let summary = CheckSummary {
        source_files: analyzed.syntax.sources.len(),
        declarations: analyzed.syntax.declaration_count(),
        semantic_nodes: analyzed.semantics.nodes.len(),
    };
    let source_revision = checked_source_revision(project, &analyzed);

    if let Some(mut diagnostics) = frontend_failure_diagnostics(&analyzed) {
        for diagnostic in &mut diagnostics {
            diagnostic.source_revision.clone_from(&source_revision);
        }
        return Err(CheckReport {
            passed: false,
            diagnostics,
            summary: Some(summary),
        });
    }
    if let Err(mut diagnostic) = validate_schema_identities(project, &analyzed) {
        diagnostic.source_revision.clone_from(&source_revision);
        return Err(CheckReport {
            passed: false,
            diagnostics: vec![diagnostic],
            summary: Some(summary),
        });
    }

    let mut diagnostics = Vec::new();
    let index_recommendations = index_recommendation_count(&analyzed);
    if index_recommendations > 0 {
        diagnostics.push(
            Diagnostic::warning("INDEX_RECOMMENDATION_AVAILABLE").with_note(format!(
                "run `jadpo schema index-recommend {}` for evidence",
                project.display()
            )),
        );
    }
    for diagnostic in &mut diagnostics {
        diagnostic.source_revision.clone_from(&source_revision);
    }

    Ok(CheckedProject {
        analyzed,
        diagnostics,
        summary,
    })
}

fn run_incident_enrichment(project_path: &Path, event_path: &Path) -> Result<(), Diagnostic> {
    let packet = incident_packet(project_path, event_path)?;
    println!(
        "{}",
        serde_json::to_string(&packet).expect("incident packet is serializable")
    );
    Ok(())
}

fn incident_packet(
    project_path: &Path,
    event_path: &Path,
) -> Result<serde_json::Value, Diagnostic> {
    let analyzed = analyze_project(project_path)?;
    require_valid_frontend(&analyzed)?;
    let event_text = fs::read_to_string(event_path)
        .map_err(|_error| Diagnostic::error("CLI_INCIDENT_READ_FAILED"))?;
    let event: serde_json::Value = serde_json::from_str(&event_text)
        .map_err(|_error| Diagnostic::error("CLI_INCIDENT_INVALID"))?;
    if event.get("kind").and_then(serde_json::Value::as_str) != Some("operational_log_event") {
        return Err(Diagnostic::error("CLI_INCIDENT_INVALID"));
    }
    let revision = checked_source_revision(project_path, &analyzed);
    let project_root = if project_path.is_dir() {
        project_path
    } else {
        project_path.parent().unwrap_or_else(|| Path::new("."))
    };
    let manifest_path = project_root.join("build/app.meta.json");
    let manifest_text = fs::read_to_string(manifest_path).map_err(|_error| {
        Diagnostic::error("CLI_INCIDENT_MANIFEST_MISSING")
            .with_note("run `jadpo build` for the exact source revision before enrichment")
    })?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text)
        .map_err(|_error| Diagnostic::error("CLI_INCIDENT_MANIFEST_INVALID"))?;
    if manifest
        .get("source_revision")
        .and_then(serde_json::Value::as_str)
        != Some(revision.as_str())
    {
        return Err(Diagnostic::error("CLI_INCIDENT_MANIFEST_STALE"));
    }
    let event_revision = event
        .get("sourceRevision")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if event_revision != revision {
        return Err(Diagnostic::error("CLI_INCIDENT_REVISION_MISMATCH")
            .with_fact(DiagnosticFact::EventRevision(event_revision.to_owned()))
            .with_fact(DiagnosticFact::LocalRevision(revision)));
    }
    let operation = bounded_identifier(&event, "semanticOperationId")?;
    let classification = event
        .get("classification")
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            matches!(
                *value,
                "RUNTIME_UNHANDLED_FAULT" | "RUNTIME_OUTCOME_UNKNOWN" | "RUNTIME_STARTUP_FAILED"
            )
        })
        .ok_or_else(|| Diagnostic::error("CLI_INCIDENT_INVALID"))?;
    let request_id = event
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| *value == "startup" || valid_generated_request_id(value))
        .ok_or_else(|| Diagnostic::error("CLI_INCIDENT_INVALID"))?;
    let operation_entry = manifest
        .get("operations")
        .and_then(serde_json::Value::as_array)
        .and_then(|operations| {
            operations.iter().find(|candidate| {
                candidate.get("id").and_then(serde_json::Value::as_str) == Some(operation)
            })
        });
    let (source, range) = if let Some(entry) = operation_entry {
        let source = entry
            .get("source")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unknown>")
            .to_owned();
        let start = entry
            .pointer("/range/start")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize;
        let end = entry
            .pointer("/range/end")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(start as u64) as usize;
        (source, jadpo_syntax::TextRange::new(start, end))
    } else if operation == "runtime:start" {
        ("<runtime>".to_owned(), jadpo_syntax::TextRange::new(0, 0))
    } else {
        return Err(Diagnostic::error("CLI_INCIDENT_OPERATION_UNKNOWN"));
    };
    let definition = catalogue_definition(classification);
    let packet = serde_json::json!({
        "schemaVersion": 1,
        "kind": "agent_incident_packet",
        "sourceRevision": revision,
        "semanticOperationId": operation,
        "occurrence": {
            "summary": "A secret-safe operational event was reported for this semantic operation.",
            "requestId": request_id,
            "classification": classification
        },
        "location": {
            "source": source,
            "range": { "start": range.start, "end": range.end }
        },
        "ruleId": definition.rule_id,
        "context": {
            "operation": operation,
            "classification": classification
        },
        "impact": {
            "affected": [operation],
            "summary": "The named semantic operation failed; customer values and provider payloads were intentionally omitted."
        },
        "recommendedNextStep": {
            "kind": definition.repair_kind.as_str(),
            "title": definition.recommended_title,
            "decisionOwner": definition.decision_owner.as_str()
        },
        "alternatives": [],
        "helpId": definition.help_id
    });
    Ok(packet)
}

fn bounded_identifier<'a>(event: &'a serde_json::Value, key: &str) -> Result<&'a str, Diagnostic> {
    let value = event
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 256
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'_' | b'-' | b':' | b'/' | b'{' | b'}' | b'.')
                })
        })
        .ok_or_else(|| Diagnostic::error("CLI_INCIDENT_INVALID"))?;
    Ok(value)
}

fn valid_generated_request_id(value: &str) -> bool {
    let Some(uuid) = value.strip_prefix("req_") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn run_human_build(project: &Path) -> Result<(), Diagnostic> {
    match build_project(project) {
        Ok(success) => {
            for diagnostic in &success.diagnostics {
                print_human_diagnostic(diagnostic);
            }
            print!(
                "{}",
                terminal::render_build_success(
                    success.artifact_count,
                    &success.output.display().to_string(),
                    terminal::stdout_style(),
                )
            );
            Ok(())
        }
        Err(mut report) => {
            let terminal = report
                .diagnostics
                .pop()
                .expect("a failed build must contain a diagnostic");
            for diagnostic in report.diagnostics {
                print_human_diagnostic(&diagnostic);
            }
            Err(terminal)
        }
    }
}

fn run_human_test(project: &Path) -> Result<(), Diagnostic> {
    let success = match build_project(project) {
        Ok(success) => success,
        Err(mut report) => {
            let terminal = report
                .diagnostics
                .pop()
                .expect("a failed test build must contain a diagnostic");
            for diagnostic in report.diagnostics {
                print_human_diagnostic(&diagnostic);
            }
            return Err(terminal);
        }
    };
    for diagnostic in &success.diagnostics {
        print_human_diagnostic(diagnostic);
    }
    let entrypoint = success.output.join("target/tests.ts");
    if !entrypoint.is_file() {
        return Err(Diagnostic::error("TEST_NO_TESTS"));
    }
    let status = Command::new("bun")
        .arg("--no-install")
        .arg(BUN_EMPTY_ENV_FILE)
        .arg(&entrypoint)
        .status()
        .map_err(|_error| Diagnostic::error("TEST_RUNTIME_START_FAILED"))?;
    if status.success() {
        Ok(())
    } else {
        Err(Diagnostic::error("TEST_FAILED"))
    }
}

fn run_format(project: &Path, check: bool) -> Result<(), Diagnostic> {
    let sources = discover_sources(project)?;
    let mut changed = Vec::new();
    for source in sources {
        let formatted = format_source(&source.text);
        if formatted != source.text {
            changed.push(source.path.clone());
            if !check {
                fs::write(&source.path, formatted)
                    .map_err(|_error| Diagnostic::error("FMT_WRITE_FAILED"))?;
            }
        }
    }
    if check && !changed.is_empty() {
        return Err(Diagnostic::error("FMT_CHANGES_REQUIRED").with_note(
            changed
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    let verb = if check { "checked" } else { "formatted" };
    print_success(
        &format!("{verb} {} source file(s)", changed.len()),
        if check {
            "Formatting verified"
        } else {
            "Formatting complete"
        },
        &terminal::plural(changed.len(), "changed source", "changed sources"),
    );
    Ok(())
}

fn build_project(project: &Path) -> Result<BuildSuccess, CheckReport> {
    let checked = checked_project(project)?;
    let mut outputs = derive_artifacts(project, &checked.analyzed);
    let target = derive_target(project, &checked.analyzed).map_err(|diagnostic| CheckReport {
        passed: false,
        diagnostics: vec![diagnostic],
        summary: Some(checked.summary),
    })?;
    outputs.extend(target);
    let artifact_count = outputs.len();
    let output = write_artifacts(project, &outputs).map_err(|diagnostic| CheckReport {
        passed: false,
        diagnostics: vec![diagnostic],
        summary: Some(checked.summary),
    })?;
    Ok(BuildSuccess {
        analyzed: checked.analyzed,
        diagnostics: checked.diagnostics,
        summary: checked.summary,
        artifact_count,
        output,
    })
}

type InputSnapshot = BTreeMap<PathBuf, u64>;

#[allow(clippy::too_many_arguments)]
fn emit_lifecycle_event(
    json: bool,
    command: &str,
    project: &Path,
    sequence: u64,
    revision: u64,
    event: &str,
    status: &str,
    stale: bool,
    summary: Option<CheckSummary>,
    diagnostics: &[Diagnostic],
    artifact_count: Option<usize>,
    output: Option<&Path>,
) -> Result<(), Diagnostic> {
    if json {
        println!(
            "{}",
            lifecycle_event_json(
                command,
                project,
                sequence,
                revision,
                event,
                status,
                stale,
                summary,
                diagnostics,
                artifact_count,
                output,
            )
        );
    } else {
        print!(
            "{}",
            terminal::render_lifecycle(
                command,
                event,
                revision,
                status,
                stale,
                terminal::stdout_style(),
            )
        );
        for diagnostic in diagnostics {
            print_human_diagnostic(diagnostic);
        }
        if let (Some(count), Some(output)) = (artifact_count, output) {
            print_success(
                &format!(
                    "{command}[built]: revision {revision}, {count} artifact(s) beneath {}",
                    output.display()
                ),
                "Build output ready",
                &format!(
                    "revision {revision} · {} · {}",
                    terminal::plural(count, "artifact", "artifacts"),
                    output.display()
                ),
            );
        }
    }
    io::stdout()
        .flush()
        .map_err(|_error| Diagnostic::error("CLI_WATCH_OUTPUT_FAILED"))
}

#[allow(clippy::too_many_arguments)]
fn lifecycle_event_json(
    command: &str,
    project: &Path,
    sequence: u64,
    revision: u64,
    event: &str,
    status: &str,
    stale: bool,
    summary: Option<CheckSummary>,
    diagnostics: &[Diagnostic],
    artifact_count: Option<usize>,
    output: Option<&Path>,
) -> String {
    let diagnostics = diagnostics_json(diagnostics);
    let summary = summary_json(summary);
    let artifact_count =
        artifact_count.map_or_else(|| "null".to_owned(), |count| count.to_string());
    let output = output.map_or_else(
        || "null".to_owned(),
        |path| json_string(&path.to_string_lossy()),
    );
    format!(
        "{{\"schemaVersion\":{DIAGNOSTIC_SCHEMA_VERSION},\"kind\":\"lifecycle_event\",\"command\":{},\"sequence\":{sequence},\"revision\":{revision},\"event\":{},\"status\":{},\"project\":{},\"stale\":{stale},\"summary\":{summary},\"artifactCount\":{artifact_count},\"output\":{output},\"diagnostics\":{diagnostics}}}",
        json_string(command),
        json_string(event),
        json_string(status),
        json_string(&project.to_string_lossy()),
    )
}

fn run_watch(project: &Path, json: bool) -> Result<(), Diagnostic> {
    let shutdown = install_shutdown_handler()?;
    let mut sequence = 0_u64;
    let mut revision = 0_u64;
    let mut has_successful_build = false;
    let mut snapshot = watch_input_snapshot(project).unwrap_or_default();

    while !shutdown.load(Ordering::SeqCst) {
        revision += 1;
        sequence += 1;
        emit_lifecycle_event(
            json,
            "watch",
            project,
            sequence,
            revision,
            "checking",
            "running",
            has_successful_build,
            None,
            &[],
            None,
            None,
        )?;

        match build_project(project) {
            Ok(success) => {
                has_successful_build = true;
                sequence += 1;
                emit_lifecycle_event(
                    json,
                    "watch",
                    project,
                    sequence,
                    revision,
                    "build_succeeded",
                    "succeeded",
                    false,
                    Some(success.summary),
                    &success.diagnostics,
                    Some(success.artifact_count),
                    Some(&success.output),
                )?;
            }
            Err(report) => {
                sequence += 1;
                emit_lifecycle_event(
                    json,
                    "watch",
                    project,
                    sequence,
                    revision,
                    "build_failed",
                    "failed",
                    has_successful_build,
                    report.summary,
                    &report.diagnostics,
                    None,
                    None,
                )?;
            }
        }

        if let Some(next) = wait_for_settled_change(
            project,
            &snapshot,
            "watch",
            json,
            &mut sequence,
            revision,
            &shutdown,
        )? {
            snapshot = next;
        } else {
            break;
        }
    }

    sequence += 1;
    emit_lifecycle_event(
        json,
        "watch",
        project,
        sequence,
        revision,
        "shutdown",
        "stopped",
        false,
        None,
        &[],
        None,
        None,
    )
}

fn run_dev(project: &Path, json: bool) -> Result<(), Diagnostic> {
    let shutdown = install_shutdown_handler()?;
    let port = dev_port()?;
    let deployment_port = TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|_| Diagnostic::error("CLI_DEV_BUN_START_FAILED"))?;
    let mut sequence = 0_u64;
    let mut revision = 0_u64;
    let mut runtime: Option<Child> = None;
    let mut has_ready_runtime = false;
    let mut active_environment = BTreeMap::new();
    let mut snapshot = watch_input_snapshot(project).unwrap_or_default();

    while !shutdown.load(Ordering::SeqCst) {
        revision += 1;
        sequence += 1;
        emit_lifecycle_event(
            json,
            "dev",
            project,
            sequence,
            revision,
            "checking",
            "running",
            has_ready_runtime,
            None,
            &[],
            None,
            None,
        )?;

        let rollback = if has_ready_runtime {
            Some(prepare_runtime_rollback(project, revision)?)
        } else {
            None
        };

        match build_project(project) {
            Err(report) => {
                if let Some(rollback) = rollback.as_ref() {
                    restore_runtime_rollback(rollback)?;
                }
                sequence += 1;
                emit_lifecycle_event(
                    json,
                    "dev",
                    project,
                    sequence,
                    revision,
                    "build_failed",
                    "failed",
                    has_ready_runtime,
                    report.summary,
                    &report.diagnostics,
                    None,
                    None,
                )?;
            }
            Ok(success) => {
                sequence += 1;
                emit_lifecycle_event(
                    json,
                    "dev",
                    project,
                    sequence,
                    revision,
                    "build_succeeded",
                    "succeeded",
                    has_ready_runtime,
                    Some(success.summary),
                    &success.diagnostics,
                    Some(success.artifact_count),
                    Some(&success.output),
                )?;

                let local_environment =
                    match local_configuration_environment(project, &success.analyzed) {
                        Ok(environment) => Some(environment),
                        Err(diagnostic) => {
                            if let Some(rollback) = rollback.as_ref() {
                                restore_runtime_rollback(rollback)?;
                            }
                            sequence += 1;
                            emit_lifecycle_event(
                                json,
                                "dev",
                                project,
                                sequence,
                                revision,
                                "configuration_failed",
                                "failed",
                                has_ready_runtime,
                                Some(success.summary),
                                &[diagnostic],
                                None,
                                Some(&success.output),
                            )?;
                            None
                        }
                    };

                if let Some(local_environment) = local_environment {
                    let replacing_ready_runtime = runtime.is_some();
                    if let Some(mut child) = runtime.take() {
                        sequence += 1;
                        emit_lifecycle_event(
                            json,
                            "dev",
                            project,
                            sequence,
                            revision,
                            "runtime_restarting",
                            "running",
                            true,
                            Some(success.summary),
                            &[],
                            None,
                            Some(&success.output),
                        )?;
                        stop_runtime(&mut child);
                        has_ready_runtime = false;
                    }

                    sequence += 1;
                    emit_lifecycle_event(
                        json,
                        "dev",
                        project,
                        sequence,
                        revision,
                        "runtime_starting",
                        "running",
                        false,
                        Some(success.summary),
                        &[],
                        None,
                        Some(&success.output),
                    )?;

                    match start_bun_runtime(project, port, deployment_port, json, &local_environment) {
                        Ok(mut child) => match wait_until_ready(&mut child, deployment_port) {
                            Ok(()) => {
                                if let Some(rollback) = rollback.as_ref() {
                                    if let Err(diagnostic) = discard_runtime_rollback(rollback) {
                                        stop_runtime(&mut child);
                                        return Err(diagnostic);
                                    }
                                }
                                has_ready_runtime = true;
                                sequence += 1;
                                emit_lifecycle_event(
                                    json,
                                    "dev",
                                    project,
                                    sequence,
                                    revision,
                                    "runtime_ready",
                                    "ready",
                                    false,
                                    Some(success.summary),
                                    &[],
                                    None,
                                    Some(&success.output),
                                )?;
                                runtime = Some(child);
                                active_environment = local_environment;
                            }
                            Err(diagnostic) => {
                                stop_runtime(&mut child);
                                sequence += 1;
                                emit_lifecycle_event(
                                    json,
                                    "dev",
                                    project,
                                    sequence,
                                    revision,
                                    "runtime_failed",
                                    "failed",
                                    false,
                                    Some(success.summary),
                                    &[diagnostic],
                                    None,
                                    Some(&success.output),
                                )?;
                                if replacing_ready_runtime {
                                    rollback_runtime(
                                        project,
                                        json,
                                        port,
                                        deployment_port,
                                        &mut sequence,
                                        revision,
                                        &mut runtime,
                                        &mut has_ready_runtime,
                                        rollback.as_ref(),
                                        &active_environment,
                                    )?;
                                }
                            }
                        },
                        Err(diagnostic) => {
                            sequence += 1;
                            emit_lifecycle_event(
                                json,
                                "dev",
                                project,
                                sequence,
                                revision,
                                "runtime_failed",
                                "failed",
                                false,
                                Some(success.summary),
                                &[diagnostic],
                                None,
                                Some(&success.output),
                            )?;
                            if replacing_ready_runtime {
                                rollback_runtime(
                                    project,
                                    json,
                                    port,
                                    deployment_port,
                                    &mut sequence,
                                    revision,
                                    &mut runtime,
                                    &mut has_ready_runtime,
                                    rollback.as_ref(),
                                    &active_environment,
                                )?;
                            }
                        }
                    }
                }
            }
        }

        let Some(next) = wait_for_dev_change(
            project,
            &snapshot,
            json,
            &mut sequence,
            revision,
            &mut runtime,
            &mut has_ready_runtime,
            &shutdown,
        )?
        else {
            break;
        };
        snapshot = next;
    }

    if let Some(mut child) = runtime.take() {
        stop_runtime(&mut child);
    }
    sequence += 1;
    emit_lifecycle_event(
        json,
        "dev",
        project,
        sequence,
        revision,
        "shutdown",
        "stopped",
        false,
        None,
        &[],
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn wait_for_dev_change(
    project: &Path,
    baseline: &InputSnapshot,
    json: bool,
    sequence: &mut u64,
    revision: u64,
    runtime: &mut Option<Child>,
    has_ready_runtime: &mut bool,
    shutdown: &AtomicBool,
) -> Result<Option<InputSnapshot>, Diagnostic> {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let exited = if let Some(child) = runtime.as_mut() {
            match child.try_wait() {
                Ok(Some(_status)) => Some(Diagnostic::error("CLI_DEV_RUNTIME_EXITED")),
                Ok(None) => None,
                Err(_error) => Some(Diagnostic::error("CLI_DEV_RUNTIME_STATUS_FAILED")),
            }
        } else {
            None
        };
        if let Some(diagnostic) = exited {
            if let Some(mut child) = runtime.take() {
                stop_runtime(&mut child);
            }
            *has_ready_runtime = false;
            *sequence += 1;
            emit_lifecycle_event(
                json,
                "dev",
                project,
                *sequence,
                revision,
                "runtime_failed",
                "failed",
                false,
                None,
                &[diagnostic],
                None,
                None,
            )?;
        }

        thread::sleep(Duration::from_millis(100));
        let mut candidate = match watch_input_snapshot(project) {
            Ok(snapshot) => snapshot,
            Err(diagnostic) => {
                *sequence += 1;
                emit_lifecycle_event(
                    json,
                    "dev",
                    project,
                    *sequence,
                    revision,
                    "watch_failed",
                    "failed",
                    *has_ready_runtime,
                    None,
                    &[diagnostic],
                    None,
                    None,
                )?;
                continue;
            }
        };
        if &candidate == baseline {
            continue;
        }
        loop {
            thread::sleep(Duration::from_millis(75));
            let next = match watch_input_snapshot(project) {
                Ok(snapshot) => snapshot,
                Err(_) => continue,
            };
            if next == candidate {
                return Ok(Some(candidate));
            }
            candidate = next;
        }
    }
}

fn dev_port() -> Result<u16, Diagnostic> {
    let value = env::var("PORT").unwrap_or_else(|_| "3000".to_owned());
    parse_dev_port(&value)
}

fn parse_dev_port(value: &str) -> Result<u16, Diagnostic> {
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| Diagnostic::error("CLI_DEV_PORT_INVALID"))
}

fn start_bun_runtime(
    project: &Path,
    port: u16,
    deployment_port: u16,
    json: bool,
    configuration: &BTreeMap<String, String>,
) -> Result<Child, Diagnostic> {
    let project_root = if project.is_dir() {
        project
    } else {
        project.parent().unwrap_or_else(|| Path::new("."))
    };
    let target = project_root.join("build/target/app.ts");
    let target = target
        .canonicalize()
        .map_err(|_error| Diagnostic::error("CLI_DEV_TARGET_MISSING"))?;
    let mut command = Command::new("bun");
    command
        .arg("--no-install")
        .arg(BUN_EMPTY_ENV_FILE)
        .arg(&target)
        .current_dir(project_root)
        .env("PORT", port.to_string())
        .envs(configuration)
        .env("JADPO_DEPLOYMENT_PORT", deployment_port.to_string())
        .stdin(Stdio::null())
        .stderr(Stdio::inherit());
    if json {
        command.stdout(Stdio::piped());
    } else {
        command.stdout(Stdio::inherit());
    }
    let mut child = command
        .spawn()
        .map_err(|_error| Diagnostic::error("CLI_DEV_BUN_START_FAILED"))?;
    if json {
        if let Some(mut output) = child.stdout.take() {
            thread::spawn(move || {
                let _ = io::copy(&mut output, &mut io::stderr());
            });
        }
    }
    Ok(child)
}

fn wait_until_ready(child: &mut Child, port: u16) -> Result<(), Diagnostic> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(_status) = child
            .try_wait()
            .map_err(|_error| Diagnostic::error("CLI_DEV_RUNTIME_STATUS_FAILED"))?
        {
            return Err(Diagnostic::error("CLI_DEV_RUNTIME_EXITED"));
        }
        if healthcheck_ready(port) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Diagnostic::error("CLI_DEV_READINESS_TIMEOUT"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn healthcheck_ready(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let timeout = Some(Duration::from_millis(250));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    if stream
        .write_all(b"GET /health/ready HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }

    let mut response = [0_u8; 64];
    let Ok(length) = stream.read(&mut response) else {
        return false;
    };
    response[..length].starts_with(b"HTTP/1.1 200")
        || response[..length].starts_with(b"HTTP/1.0 200")
}

fn stop_runtime(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Debug)]
struct RuntimeRollback {
    output_root: PathBuf,
    backup_root: PathBuf,
}

fn project_root(project: &Path) -> &Path {
    if project.is_dir() {
        project
    } else {
        project.parent().unwrap_or_else(|| Path::new("."))
    }
}

fn prepare_runtime_rollback(project: &Path, revision: u64) -> Result<RuntimeRollback, Diagnostic> {
    let project_root = project_root(project);
    let output_root = project_root.join("build");
    let backup_root = project_root.join(format!(
        ".jadpo-runtime-backup.{}.{revision}",
        std::process::id()
    ));
    if backup_root.exists() {
        fs::remove_dir_all(&backup_root)
            .map_err(|_error| Diagnostic::error("CLI_DEV_ROLLBACK_PREPARE_FAILED"))?;
    }
    fs::rename(&output_root, &backup_root)
        .map_err(|_error| Diagnostic::error("CLI_DEV_ROLLBACK_PREPARE_FAILED"))?;
    Ok(RuntimeRollback {
        output_root,
        backup_root,
    })
}

fn restore_runtime_rollback(rollback: &RuntimeRollback) -> Result<(), Diagnostic> {
    if rollback.output_root.exists() {
        fs::remove_dir_all(&rollback.output_root)
            .map_err(|_error| Diagnostic::error("CLI_DEV_ROLLBACK_RESTORE_FAILED"))?;
    }
    fs::rename(&rollback.backup_root, &rollback.output_root)
        .map_err(|_error| Diagnostic::error("CLI_DEV_ROLLBACK_RESTORE_FAILED"))
}

fn discard_runtime_rollback(rollback: &RuntimeRollback) -> Result<(), Diagnostic> {
    if rollback.backup_root.exists() {
        fs::remove_dir_all(&rollback.backup_root)
            .map_err(|_error| Diagnostic::error("CLI_DEV_ROLLBACK_CLEANUP_FAILED"))?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn rollback_runtime(
    project: &Path,
    json: bool,
    port: u16,
    deployment_port: u16,
    sequence: &mut u64,
    revision: u64,
    runtime: &mut Option<Child>,
    has_ready_runtime: &mut bool,
    rollback: Option<&RuntimeRollback>,
    configuration: &BTreeMap<String, String>,
) -> Result<(), Diagnostic> {
    let Some(rollback) = rollback else {
        return Ok(());
    };
    *sequence += 1;
    emit_lifecycle_event(
        json,
        "dev",
        project,
        *sequence,
        revision,
        "runtime_rollback_starting",
        "running",
        true,
        None,
        &[],
        None,
        Some(&rollback.backup_root),
    )?;
    restore_runtime_rollback(rollback)?;

    let result = start_bun_runtime(project, port, deployment_port, json, configuration).and_then(|mut child| {
        if let Err(diagnostic) = wait_until_ready(&mut child, deployment_port) {
            stop_runtime(&mut child);
            return Err(diagnostic);
        }
        *runtime = Some(child);
        Ok(())
    });

    match result {
        Ok(()) => {
            *has_ready_runtime = true;
            *sequence += 1;
            emit_lifecycle_event(
                json,
                "dev",
                project,
                *sequence,
                revision,
                "runtime_rollback_succeeded",
                "ready",
                true,
                None,
                &[],
                None,
                Some(&rollback.output_root),
            )
        }
        Err(diagnostic) => {
            *has_ready_runtime = false;
            *sequence += 1;
            emit_lifecycle_event(
                json,
                "dev",
                project,
                *sequence,
                revision,
                "runtime_rollback_failed",
                "failed",
                false,
                None,
                &[diagnostic],
                None,
                Some(&rollback.output_root),
            )
        }
    }
}

fn install_shutdown_handler() -> Result<Arc<AtomicBool>, Diagnostic> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let requested = Arc::clone(&shutdown);
    ctrlc::set_handler(move || requested.store(true, Ordering::SeqCst))
        .map_err(|_error| Diagnostic::error("CLI_SIGNAL_HANDLER_FAILED"))?;
    Ok(shutdown)
}

fn wait_for_settled_change(
    project: &Path,
    baseline: &InputSnapshot,
    command: &str,
    json: bool,
    sequence: &mut u64,
    revision: u64,
    shutdown: &AtomicBool,
) -> Result<Option<InputSnapshot>, Diagnostic> {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(100));
        let mut candidate = match watch_input_snapshot(project) {
            Ok(snapshot) => snapshot,
            Err(diagnostic) => {
                *sequence += 1;
                emit_lifecycle_event(
                    json,
                    command,
                    project,
                    *sequence,
                    revision,
                    "watch_failed",
                    "failed",
                    true,
                    None,
                    &[diagnostic],
                    None,
                    None,
                )?;
                continue;
            }
        };
        if &candidate == baseline {
            continue;
        }

        loop {
            thread::sleep(Duration::from_millis(75));
            let next = match watch_input_snapshot(project) {
                Ok(snapshot) => snapshot,
                Err(_) => continue,
            };
            if next == candidate {
                return Ok(Some(candidate));
            }
            candidate = next;
        }
    }
}

fn watch_input_snapshot(project: &Path) -> Result<InputSnapshot, Diagnostic> {
    let mut paths = Vec::new();
    if project.is_file() {
        paths.push(project.to_owned());
        if let Some(parent) = project.parent() {
            let registry = parent.join("schema.identities.json");
            if registry.is_file() {
                paths.push(registry);
            }
        }
    } else if project.is_dir() {
        collect_watch_inputs(project, &mut paths)?;
    }
    paths.sort();
    paths.dedup();

    let mut snapshot = BTreeMap::new();
    for path in paths {
        let bytes =
            fs::read(&path).map_err(|_error| Diagnostic::error("CLI_WATCH_INPUT_READ_FAILED"))?;
        snapshot.insert(path, stable_bytes_hash(&bytes));
    }
    Ok(snapshot)
}

fn collect_watch_inputs(path: &Path, output: &mut Vec<PathBuf>) -> Result<(), Diagnostic> {
    let entries =
        fs::read_dir(path).map_err(|_error| Diagnostic::error("CLI_WATCH_INPUT_READ_FAILED"))?;
    for entry in entries {
        let entry = entry.map_err(|_error| Diagnostic::error("CLI_WATCH_INPUT_READ_FAILED"))?;
        let entry_path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry_path.is_dir() {
            if name == "build"
                || name.starts_with(".jadpo-build-")
                || name.starts_with(".jadpo-runtime-")
            {
                continue;
            }
            collect_watch_inputs(&entry_path, output)?;
        } else if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("jadpo")
            || name == "schema.identities.json"
            || name == ".env.local"
        {
            output.push(entry_path);
        }
    }
    Ok(())
}

fn stable_bytes_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn frontend_failure_diagnostics(project: &AnalyzedProject) -> Option<Vec<Diagnostic>> {
    let stages = [
        project.syntax.diagnostics().cloned().collect::<Vec<_>>(),
        project.semantics.diagnostics.clone(),
        project.typing.diagnostics.clone(),
        project.failures.diagnostics.clone(),
    ];

    for diagnostics in stages {
        if diagnostics.is_empty() {
            continue;
        }
        return Some(diagnostics);
    }
    None
}

fn failed_report(diagnostic: Diagnostic) -> CheckReport {
    CheckReport {
        passed: false,
        diagnostics: vec![diagnostic],
        summary: None,
    }
}

fn check_report_json(project: &Path, report: &CheckReport) -> String {
    let status = if report.passed { "passed" } else { "failed" };
    let summary = summary_json(report.summary);
    let diagnostics = diagnostics_json(&report.diagnostics);
    format!(
        "{{\"schemaVersion\":{DIAGNOSTIC_SCHEMA_VERSION},\"kind\":\"diagnostic_report\",\"command\":\"check\",\"status\":\"{status}\",\"project\":{},\"summary\":{summary},\"diagnostics\":{diagnostics}}}",
        json_string(&project.to_string_lossy()),
    )
}

fn summary_json(summary: Option<CheckSummary>) -> String {
    summary.map_or_else(
        || "null".to_owned(),
        |summary| {
            format!(
                "{{\"source_files\":{},\"declarations\":{},\"semantic_nodes\":{}}}",
                summary.source_files, summary.declarations, summary.semantic_nodes
            )
        },
    )
}

fn diagnostics_json(diagnostics: &[Diagnostic]) -> String {
    let diagnostics = diagnostics
        .iter()
        .map(Diagnostic::to_json)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{diagnostics}]")
}

fn print_human_diagnostic(diagnostic: &Diagnostic) {
    eprint!(
        "{}",
        terminal::render_diagnostic(diagnostic, terminal::stderr_style())
    );
}

#[cfg(test)]
fn human_diagnostic(diagnostic: &Diagnostic) -> String {
    terminal::render_diagnostic(diagnostic, terminal::RenderStyle::plain())
}

fn require_valid_frontend(project: &AnalyzedProject) -> Result<(), Diagnostic> {
    if let Some(mut diagnostics) = frontend_failure_diagnostics(project) {
        let terminal = diagnostics
            .pop()
            .expect("a frontend failure must contain a summary diagnostic");
        for diagnostic in diagnostics {
            print_human_diagnostic(&diagnostic);
        }
        return Err(terminal);
    }

    Ok(())
}

fn protected_attempt_export(arguments: &[String]) -> Result<String, Diagnostic> {
    let invalid = || {
        Diagnostic::error("CLI_PRESENTATION_ARGUMENTS").with_note(
        "expected: jadpo protected-attempt <attempt-project> --before <checked-baseline> --request <closed-request.json> [--expected-before <sha256:digest>] [--expected-before-state <sha256:digest>] [--validate-export <artifact.json>]")
    };
    let path = arguments
        .first()
        .filter(|s| !s.starts_with('-'))
        .ok_or_else(invalid)?;
    let mut options = std::collections::BTreeMap::new();
    let mut index = 1;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        if !matches!(
            flag,
            "--before"
                | "--request"
                | "--expected-before"
                | "--expected-before-state"
                | "--validate-export"
        ) || options.contains_key(flag)
        {
            return Err(invalid());
        }
        let value = arguments
            .get(index + 1)
            .filter(|s| !s.starts_with('-'))
            .ok_or_else(invalid)?;
        options.insert(flag, value.as_str());
        index += 2;
    }
    let before_path = Path::new(*options.get("--before").ok_or_else(invalid)?);
    let request_path = options.get("--request").ok_or_else(invalid)?;
    let baseline = analyze_project(before_path)?;
    require_valid_frontend(&baseline)?;
    validate_schema_identities(before_path, &baseline)?;
    let attempt = analyze_project(Path::new(path))?;
    let request = fs::read_to_string(request_path)
        .map_err(|_| invalid().with_note("request file must be readable UTF-8"))?;
    let subject = jadpo_core::derive_protected_attempt(
        Path::new(path),
        &attempt,
        before_path,
        &baseline,
        &request,
        options.get("--expected-before").copied(),
        options.get("--expected-before-state").copied(),
    )
    .map_err(|message| invalid().with_note(message))?;
    if let Some(export_path) = options.get("--validate-export") {
        let exported = fs::read_to_string(export_path)
            .map_err(|_| invalid().with_note("export file must be readable UTF-8"))?;
        jadpo_core::validate_protected_attempt_export(&subject, &exported)
            .map_err(|message| invalid().with_note(message))?;
    }
    Ok(subject)
}

fn approval_export(arguments: &[String]) -> Result<String, Diagnostic> {
    let invalid = || {
        Diagnostic::error("CLI_PRESENTATION_ARGUMENTS").with_note(
        "expected: jadpo approval <project> [--before <source-project>] [--expected-before <sha256:digest>] [--expected-before-state <sha256:digest>] [--intent <text-file>] [--text]"
    )
    };
    let path = arguments
        .first()
        .filter(|s| !s.starts_with('-'))
        .ok_or_else(invalid)?;
    let mut before = None;
    let mut expected = None;
    let mut expected_state = None;
    let mut intent = None;
    let mut text = false;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--text" if !text => {
                text = true;
                index += 1;
                continue;
            }
            "--before" if before.is_none() => {
                before = Some(arguments.get(index + 1).ok_or_else(invalid)?.clone())
            }
            "--expected-before" if expected.is_none() => {
                expected = Some(arguments.get(index + 1).ok_or_else(invalid)?.clone())
            }
            "--expected-before-state" if expected_state.is_none() => {
                expected_state = Some(arguments.get(index + 1).ok_or_else(invalid)?.clone())
            }
            "--intent" if intent.is_none() => {
                intent = Some(arguments.get(index + 1).ok_or_else(invalid)?.clone())
            }
            _ => return Err(invalid()),
        }
        index += 2;
    }
    let analyzed = analyze_project(Path::new(path))?;
    require_valid_frontend(&analyzed)?;
    validate_schema_identities(Path::new(path), &analyzed)?;
    let baseline = before
        .as_ref()
        .map(|p| analyze_project(Path::new(p)))
        .transpose()?;
    if let (Some(path), Some(project)) = (&before, &baseline) {
        require_valid_frontend(project)?;
        validate_schema_identities(Path::new(path), project)?;
    }
    let intent = intent
        .map(|p| {
            fs::read_to_string(p)
                .map_err(|_| invalid().with_note("intent file must be readable UTF-8"))
        })
        .transpose()?;
    let subject = jadpo_core::derive_approval_subject_with_state_pin(
        Path::new(path),
        &analyzed,
        before
            .as_ref()
            .zip(baseline.as_ref())
            .map(|(p, a)| (Path::new(p), a)),
        intent.as_deref(),
        expected.as_deref(),
        expected_state.as_deref(),
    )
    .map_err(|message| invalid().with_note(message))?;
    Ok(if text {
        jadpo_core::approval_text(&subject)
    } else {
        subject
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_project, check_project, check_report_json, human_diagnostic, incident_packet,
        lifecycle_event_json, parse_dev_port, prepare_runtime_rollback, presentation_arguments,
        restore_runtime_rollback, watch_input_snapshot, CheckReport, CheckSummary,
    };
    use crate::terminal::{ColorChoice, LayoutChoice};
    use jadpo_diagnostics::{Diagnostic, SourceSpan};
    use std::fs;
    use std::path::Path;

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("CLI crate should be inside the repository")
    }

    #[test]
    fn emits_a_stable_versioned_check_envelope() {
        let report = CheckReport {
            passed: true,
            diagnostics: vec![
                Diagnostic::warning("TEST_WARNING").with_note("use the evidence command")
            ],
            summary: Some(CheckSummary {
                source_files: 2,
                declarations: 3,
                semantic_nodes: 5,
            }),
        };

        let json = check_report_json(Path::new("example"), &report);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid report JSON");
        assert_eq!(parsed["schemaVersion"], 2);
        assert_eq!(parsed["diagnostics"][0]["ruleId"], "test.warning");
        assert_eq!(parsed["diagnostics"][0]["summary"], "Warning");
        assert_eq!(
            parsed["diagnostics"][0]["recommendedNextStep"]["kind"],
            "guided_choice"
        );
    }

    #[test]
    fn separates_presentation_flags_from_command_arguments() {
        let (arguments, presentation) = presentation_arguments(vec![
            "check".to_owned(),
            "example".to_owned(),
            "--diagnostic-format=plain".to_owned(),
            "--color=never".to_owned(),
        ])
        .expect("valid presentation flags");

        assert_eq!(arguments, vec!["check", "example"]);
        assert_eq!(presentation.layout, LayoutChoice::Plain);
        assert_eq!(presentation.color, ColorChoice::Never);

        let (arguments, _) = presentation_arguments(vec![
            "check".to_owned(),
            "example".to_owned(),
            "--diagnostic-format=json".to_owned(),
        ])
        .expect("JSON remains part of the command protocol");
        assert_eq!(
            arguments,
            vec!["check", "example", "--diagnostic-format=json"]
        );
    }

    #[test]
    fn json_check_report_preserves_precise_frontend_diagnostics() {
        let fixture = repository_root().join("tests/compile/fail/52_immutable_reassignment.jadpo");
        let report = check_project(&fixture);

        assert!(!report.passed);
        assert!(report.summary.is_some());
        assert_eq!(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![
                "TYPE_ASSIGN_IMMUTABLE",
                "TYPE_ASSIGN_IMMUTABLE",
                "TYPE_MISMATCH"
            ]
        );
        let json = check_report_json(&fixture, &report);
        assert!(json.contains("\"status\":\"failed\""));
        assert!(json.contains("\"location\":{\"source\":"));
        assert!(json.contains("\"range\":{\"start\":132,\"end\":154}"));
    }

    #[test]
    fn empty_project_fails_check() {
        let root =
            std::env::temp_dir().join(format!("jadpo-cli-empty-project-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture should be removable");
        }
        fs::create_dir_all(&root).expect("fixture should be created");
        fs::write(root.join("app.jadpo"), "").expect("empty source should be written");

        let report = check_project(&root);

        assert!(!report.passed);
        assert_eq!(report.diagnostics[0].code, "JADPO_EMPTY_PROJECT");
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn check_summary_counts_persistence_declarations() {
        let root = std::env::temp_dir().join(format!(
            "jadpo-cli-persistence-declaration-count-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture should be removable");
        }
        fs::create_dir_all(&root).expect("fixture should be created");
        fs::write(
            root.join("app.jadpo"),
            "type Customer = Object { id: Uuid }\npersist Customer { identity: id }\n",
        )
        .expect("source should be written");

        let report = check_project(&root);

        assert!(report.passed, "{:#?}", report.diagnostics);
        assert_eq!(
            report.summary.expect("summary should exist").declarations,
            2
        );
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn human_diagnostics_show_source_line_column_and_caret() {
        let root =
            std::env::temp_dir().join(format!("jadpo-cli-human-diagnostic-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture should be removable");
        }
        fs::create_dir_all(&root).expect("fixture should be created");
        let source = root.join("app.jadpo");
        fs::write(&source, "type User = {\n    id: UUID\n}\n").expect("source should be written");
        let mut diagnostic =
            Diagnostic::error("TEST_SYNTAX").with_note("use an entity declaration");
        diagnostic.primary = Some(SourceSpan {
            source: source.to_string_lossy().into_owned(),
            start: 12,
            end: 13,
        });

        let rendered = human_diagnostic(&diagnostic);

        assert!(rendered.contains(":1:13\n"));
        assert!(rendered.contains("1 | type User = {"));
        assert!(rendered.contains("  |             ^"));
        assert!(rendered.contains("note: use an entity declaration"));
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn runnable_build_separates_public_liveness_from_native_deployment_readiness() {
        let root = std::env::temp_dir().join(format!("jadpo-cli-no-routes-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture should be removable");
        }
        fs::create_dir_all(&root).expect("fixture should be created");
        fs::write(root.join("app.jadpo"), "type Name = Text {}\n")
            .expect("source should be written");

        let success = build_project(&root).expect("type-only project should build");
        let target = fs::read_to_string(success.output.join("target/app.ts"))
            .expect("generated target should be readable");

        assert!(target.contains("url.pathname === \"/health\""));
        assert!(target.contains("export async function handleDeploymentRequest"));
        let application = target.split("export async function handleRequest").nth(1).expect("application handler should exist");
        assert!(application.contains("url.pathname === \"/health/live\""));
        assert!(application.contains("url.pathname === \"/health\") return json(404"));
        assert!(!application.contains("refreshDatabaseReadiness()"));
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn enriches_safe_runtime_ids_locally_without_copying_event_payloads() {
        let root = std::env::temp_dir().join(format!(
            "jadpo-cli-incident-enrichment-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture should be removable");
        }
        fs::create_dir_all(&root).expect("fixture should be created");
        fs::write(root.join("app.jadpo"), "type Name = Text {}\n")
            .expect("source should be written");
        build_project(&root).expect("project should build a matching manifest");
        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join("build/app.meta.json"))
                .expect("manifest should be readable"),
        )
        .expect("manifest should be JSON");
        let revision = manifest["source_revision"]
            .as_str()
            .expect("manifest should carry source revision");
        let canary = "customer-secret-canary";
        let event_path = root.join("event.json");
        fs::write(
            &event_path,
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "kind": "operational_log_event",
                "eventName": "operation.failed",
                "classification": "RUNTIME_STARTUP_FAILED",
                "requestId": "startup",
                "traceId": null,
                "semanticOperationId": "runtime:start",
                "sourceRevision": revision,
                "attributes": { "unsafe": canary },
                "rawError": canary
            }))
            .expect("event should serialize"),
        )
        .expect("event should be written");

        let packet = incident_packet(&root, &event_path).expect("event should enrich locally");
        let encoded = serde_json::to_string(&packet).expect("packet should serialize");
        assert!(!encoded.contains(canary));
        assert_eq!(packet["kind"], "agent_incident_packet");
        assert_eq!(packet["semanticOperationId"], "runtime:start");
        assert_eq!(packet["sourceRevision"], revision);
        assert_eq!(packet["location"]["source"], "<runtime>");
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn emits_versioned_watch_lifecycle_events() {
        let diagnostic = Diagnostic::warning("TEST_WARNING");
        let json = lifecycle_event_json(
            "watch",
            Path::new("example"),
            2,
            1,
            "build_succeeded",
            "succeeded",
            false,
            Some(CheckSummary {
                source_files: 1,
                declarations: 2,
                semantic_nodes: 3,
            }),
            &[diagnostic],
            Some(8),
            Some(Path::new("example/build")),
        );

        assert!(json.starts_with(
            "{\"schemaVersion\":2,\"kind\":\"lifecycle_event\",\"command\":\"watch\""
        ));
        assert!(json.contains("\"sequence\":2,\"revision\":1"));
        assert!(json.contains("\"event\":\"build_succeeded\""));
        assert!(json.contains("\"artifactCount\":8"));
        assert!(json.contains("\"output\":\"example/build\""));
        assert!(json.contains("\"ruleId\":\"test.warning\""));
    }

    #[test]
    fn watch_snapshot_tracks_checked_inputs_and_ignores_generated_output() {
        let root = std::env::temp_dir().join(format!("jadpo-cli-watch-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale watch fixture should be removable");
        }
        fs::create_dir_all(root.join("build/target"))
            .expect("watch fixture directories should be created");
        fs::write(root.join("app.jadpo"), "type Name = Text {}\n")
            .expect("source should be written");
        fs::write(root.join("schema.identities.json"), "{}\n").expect("registry should be written");
        fs::write(root.join("build/target/generated.jadpo"), "ignored\n")
            .expect("generated source should be written");

        let first = watch_input_snapshot(&root).expect("snapshot should succeed");
        assert_eq!(first.len(), 2);
        fs::write(root.join("build/target/generated.jadpo"), "still ignored\n")
            .expect("generated source should change");
        assert_eq!(
            watch_input_snapshot(&root).expect("second snapshot should succeed"),
            first
        );
        fs::write(
            root.join("app.jadpo"),
            "type Name = Text { max_length 10 }\n",
        )
        .expect("source should change");
        assert_ne!(
            watch_input_snapshot(&root).expect("changed snapshot should succeed"),
            first
        );

        fs::remove_dir_all(root).expect("watch fixture should be removable");
    }

    #[test]
    fn validates_the_dev_server_port() {
        assert_eq!(parse_dev_port("3000").expect("port should parse"), 3000);
        assert_eq!(
            parse_dev_port("0").expect_err("zero should fail").code,
            "CLI_DEV_PORT_INVALID"
        );
        assert_eq!(
            parse_dev_port("not-a-port")
                .expect_err("text should fail")
                .code,
            "CLI_DEV_PORT_INVALID"
        );
    }

    #[test]
    fn preserves_and_restores_the_last_known_good_runtime_revision() {
        let root =
            std::env::temp_dir().join(format!("jadpo-cli-runtime-rollback-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale rollback fixture should be removable");
        }
        fs::create_dir_all(root.join("build/target")).expect("build fixture should be created");
        fs::write(root.join("build/target/app.ts"), "known-good\n")
            .expect("known-good target should be written");

        let rollback =
            prepare_runtime_rollback(&root, 2).expect("known-good build should be preserved");
        assert!(!root.join("build").exists());
        assert!(rollback.backup_root.exists());
        fs::create_dir_all(root.join("build/target")).expect("candidate build should be created");
        fs::write(root.join("build/target/app.ts"), "broken-candidate\n")
            .expect("candidate target should be written");

        restore_runtime_rollback(&rollback).expect("known-good build should be restored");
        assert_eq!(
            fs::read_to_string(root.join("build/target/app.ts"))
                .expect("restored target should be readable"),
            "known-good\n"
        );
        assert!(!rollback.backup_root.exists());
        fs::remove_dir_all(root).expect("rollback fixture should be removable");
    }

    #[test]
    fn rollback_failures_have_stable_diagnostic_contracts() {
        for code in [
            "CLI_DEV_ROLLBACK_CLEANUP_FAILED",
            "CLI_DEV_ROLLBACK_PREPARE_FAILED",
            "CLI_DEV_ROLLBACK_RESTORE_FAILED",
            "CLI_SIGNAL_HANDLER_FAILED",
        ] {
            let diagnostic = Diagnostic::error(code);
            assert!(!diagnostic.message.is_empty(), "{code}");
            assert!(!diagnostic.reason.is_empty(), "{code}");
        }
    }

    #[test]
    fn configuration_cli_failures_have_stable_safe_copy() {
        for code in [
            "CLI_CONFIG_ARGUMENTS",
            "CLI_CONFIG_PROMPT_FAILED",
            "CLI_CONFIG_TTY_REQUIRED",
        ] {
            let diagnostic = Diagnostic::error(code);
            assert!(!diagnostic.message.is_empty(), "{code}");
            assert!(!diagnostic.reason.is_empty(), "{code}");
            assert!(!diagnostic.message.contains("value"), "{code}");
        }
    }
    #[test]
    fn protected_attempt_cli_requires_fresh_baseline_closed_request_and_exact_export() {
        let root = std::env::temp_dir().join(format!(
            "jadpo-protected-attempt-cli-{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("before")).unwrap();
        fs::create_dir_all(root.join("attempt")).unwrap();
        let baseline = format!("{}\nfailure Missing {{ kind: NotFound code: \"missing\" }}\nfailure Conflicted {{ kind: Conflict code: \"conflicted\" }}\n",
            fs::read_to_string(repository_root().join("tests/compile/pass/132_scoped_role_policy.jadpo")).unwrap());
        let attempt = format!("{baseline}\naction move_owner(row_id: Company.id, next_owner: User.id) fails Missing, Conflicted -> Company {{ return attempt update required Company {{ where: id == row_id set: {{ owner_id: Company.owner_id(next_owner) }} missing: Missing conflict: Conflicted }} }}");
        fs::write(root.join("before/app.jadpo"), &baseline).unwrap();
        fs::write(root.join("attempt/app.jadpo"), &attempt).unwrap();
        let request = serde_json::json!({"entity":"Company","field":"owner_id","callable":"move_owner",
            "row":{"parameter":"row_id","nominal_type":"Company.id","alias":"t1"},
            "from":{"nominal_type":"User.id","alias":"alice"},
            "to":{"parameter":"next_owner","nominal_type":"User.id","alias":"bob"}});
        fs::write(root.join("request.json"), request.to_string()).unwrap();
        let args = vec![
            root.join("attempt").to_string_lossy().into_owned(),
            "--before".into(),
            root.join("before").to_string_lossy().into_owned(),
            "--request".into(),
            root.join("request.json").to_string_lossy().into_owned(),
        ];
        let raw = super::protected_attempt_export(&args).unwrap();
        let subject: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            subject["canonical"]["disposition"],
            "requires_human_decision"
        );
        assert!(!super::check_project(&root.join("attempt")).passed);
        fs::write(root.join("export.json"), &raw).unwrap();
        let mut validated = args.clone();
        validated.extend([
            "--validate-export".into(),
            root.join("export.json").to_string_lossy().into_owned(),
        ]);
        assert_eq!(super::protected_attempt_export(&validated).unwrap(), raw);
        fs::write(root.join("export.json"), "{}").unwrap();
        assert!(super::protected_attempt_export(&validated).is_err());
        for extra in [
            vec!["--expected-before".into(), "sha256:foreign".into()],
            vec!["--expected-before-state".into(), "sha256:foreign".into()],
            vec!["--request".into(), "duplicate".into()],
            vec!["--text".into()],
        ] {
            let mut invalid = args.clone();
            invalid.extend(extra);
            assert!(super::protected_attempt_export(&invalid).is_err());
        }
        assert!(super::protected_attempt_export(&args[..3]).is_err());
        fs::write(
            root.join("before/app.jadpo"),
            "function bad() -> Text { return 1 }",
        )
        .unwrap();
        assert!(super::protected_attempt_export(&args).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approval_cli_compares_pinned_source_and_preserves_text_facts() {
        let root = std::env::temp_dir().join(format!("jadpo-approval-cli-{}", std::process::id()));
        fs::create_dir_all(root.join("before")).unwrap();
        fs::create_dir_all(root.join("after")).unwrap();
        let source = |value| {
            format!("type Greeting = Text {{}}\nfunction greet() -> Greeting {{ return Greeting(\"{value}\") }}\n")
        };
        fs::write(root.join("before/app.jadpo"), source("old")).unwrap();
        fs::write(root.join("after/app.jadpo"), source("new")).unwrap();
        fs::write(root.join("intent.txt"), "Change the greeting").unwrap();
        let before = root.join("before").to_string_lossy().into_owned();
        let after = root.join("after").to_string_lossy().into_owned();
        let baseline: serde_json::Value =
            serde_json::from_str(&super::approval_export(&[before.clone()]).unwrap()).unwrap();
        let mut args = vec![
            after,
            "--before".into(),
            before,
            "--expected-before".into(),
            baseline["canonical"]["after"]["source_digest"]
                .as_str()
                .unwrap()
                .into(),
            "--intent".into(),
            root.join("intent.txt").to_string_lossy().into_owned(),
        ];
        args.extend([
            "--expected-before-state".into(),
            baseline["canonical"]["after"]["state_digest"]
                .as_str()
                .unwrap()
                .into(),
        ]);
        let json = super::approval_export(&args).unwrap();
        args.push("--text".into());
        let text = super::approval_export(&args).unwrap();
        assert!(jadpo_core::validate_approval_export(&json, text.lines().last().unwrap()).is_ok());
        assert!(
            !root.join("after/build").exists(),
            "inspection must not replace a build"
        );
        let checked = jadpo_core::analyze_project(&root.join("before")).unwrap();
        let registry =
            jadpo_core::initialize_schema_identities(&root.join("before"), &checked).unwrap();
        assert!(
            super::approval_export(&args).is_err(),
            "registry-only changes invalidate the full state pin"
        );
        fs::remove_file(registry).unwrap();
        assert!(super::approval_export(&args).is_ok());
        fs::write(root.join("before/app.jadpo"), source("changed baseline")).unwrap();
        assert!(super::approval_export(&args).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
