//! `jadpo check` (area 14): the project is found by walking up to
//! `app.jadpo`, a file or folder can be named instead, problems print in
//! the design language's error shape, and the last line is the summary.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

const APP: &str = "application {}\n";
const COUNT: &str = "domains/planning/types/count.jadpo";
const TALLY: &str = "domains/planning/actions/tally.jadpo";

/// A fresh folder for one test under cargo's temporary directory.
use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("check-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// A clean project of three files: a type in `types/` and an action using it.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", APP);
    write(&root, COUNT, "type Count = Int {\n    min: 0\n}\n");
    write(
        &root,
        TALLY,
        "use planning/types { Count }\n\naction tally(count: Count) -> Count {\n    return count\n}\n",
    );
    root
}

fn jadpo_in(env: Environment, cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    jadpo_in(Environment::empty(), cwd, args)
}

fn terminal() -> Environment {
    Environment::empty()
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn a_clean_project_is_one_line_from_any_subfolder() {
    let root = project("clean");
    let output = jadpo(&root.join("domains/planning/actions"), &["check"]);
    assert_eq!(output.stdout, "checked: 3 files\n");
    assert_eq!(output.stderr, "");
    assert_eq!(output.code, 0);
}

const MISTAKE: &str = "use planning/types { Count }\n\naction tally(count: Count) -> Count {\n    return dobule(count)\n}\n\nfunction double(count: Count) -> Count {\n    return count\n}\n";

#[test]
fn problems_print_in_the_error_shape_then_the_summary() {
    let root = project("problem");
    write(&root, TALLY, MISTAKE);
    let output = jadpo(&root, &["check"]);
    assert_eq!(
        output.stdout,
        "error: `dobule` isn't a function, action or query
 --> domains/planning/actions/tally.jadpo:4:12
  |
4 |     return dobule(count)
  |            ^^^^^^ not declared
  reason: Nothing in this project called `dobule` can be called. Did you mean `double`?
  next: Use `double` instead of `dobule` (agent)
  rule: semantic.unknown_callee
  help: https://jadpo.dev/docs/errors/semantic.unknown_callee

check failed: 1 error, 3 files
"
    );
    assert_eq!(output.code, 1);
}

#[test]
fn a_terminal_gets_the_rich_form() {
    let root = project("rich");
    write(&root, TALLY, MISTAKE);
    let output = jadpo_in(terminal(), &root, &["check"]);
    assert!(output.stdout.contains("\x1b["), "a terminal gets colour");
    assert_eq!(
        strip_ansi(&output.stdout),
        "✗ error  `dobule` isn't a function, action or query
  domains/planning/actions/tally.jadpo:4:12
    │
  4 │     return dobule(count)
    │            ^^^^^^ not declared
    │
  reason    Nothing in this project called `dobule` can be called. Did you mean
            `double`?
  next      Use `double` instead of `dobule` (agent)
  rule      semantic.unknown_callee
  help      https://jadpo.dev/docs/errors/semantic.unknown_callee

✗ check failed  1 error · 3 files
"
    );
}

#[test]
fn json_carries_the_problems_with_lines_and_edits() {
    let root = project("json");
    write(&root, TALLY, MISTAKE);
    let output = jadpo(&root, &["check", "--json", "--full"]);
    assert_eq!(output.code, 1);
    assert_eq!(output.stderr, "");
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["schemaVersion"], 3);
    assert_eq!(document["command"], "check");
    assert_eq!(document["ok"], false);
    assert_eq!(document["data"]["layout"], "project");
    assert_eq!(
        document["data"]["files"],
        serde_json::json!(["app.jadpo", TALLY, COUNT])
    );
    assert_eq!(document["data"]["errors"], 1);
    let problem = &document["diagnostics"][0];
    assert_eq!(problem["rule"], "semantic.unknown_callee");
    assert_eq!(problem["location"]["path"], TALLY);
    assert_eq!(problem["location"]["line"], 4);
    assert_eq!(problem["location"]["column"], 12);
    assert_eq!(problem["next"]["edits"][0]["replacement"], "double");
    let again = jadpo(&root, &["check", "--json", "--full"]);
    let timeless = |stdout: &str| -> String {
        stdout
            .lines()
            .filter(|line| !line.contains("\"durationMs\""))
            .collect()
    };
    assert_eq!(
        timeless(&again.stdout),
        timeless(&output.stdout),
        "the same source, the same bytes but for how long it took"
    );
}

#[test]
fn outside_a_project_a_named_file_is_checked_on_its_own() {
    let root = folder("loose");
    write(
        &root,
        "notes/sketch.jadpo",
        "entity Basket {\n    id: Uuid\n}\n",
    );
    write(&root, "notes/other.jadpo", "type Broken = Missing {}\n");
    let output = jadpo(&root, &["check", "notes/sketch.jadpo"]);
    assert_eq!(output.stdout, "checked: 1 file\n");
    let folder = jadpo(&root, &["check", "notes", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&folder.stdout).expect("JSON");
    assert_eq!(document["data"]["layout"], "loose");
    assert_eq!(document["diagnostics"][0]["rule"], "semantic.unknown_name");
}

#[test]
fn without_a_project_or_a_path_it_says_where_it_looked() {
    let root = folder("nowhere");
    let output = jadpo(&root, &["check"]);
    assert_eq!(
        output.stdout,
        "error: No Jadpo project here
  reason: Jadpo looks for `app.jadpo` in this folder and every folder above it, and found none.
  next: Run `jadpo check` inside a project, or give a file or folder: `jadpo check <path>` (agent)
  rule: toolchain.project_not_found
  help: https://jadpo.dev/docs/errors/toolchain.project_not_found
"
    );
    assert_eq!(output.code, 1);
}

#[test]
fn a_path_that_does_not_exist_is_a_command_line_mistake() {
    let root = folder("missing");
    let output = jadpo(&root, &["check", "nowhere"]);
    assert_eq!(output.code, 2);
    assert!(
        output.stderr.starts_with("error: `nowhere` doesn't exist"),
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains("rule: cli.check_arguments"));
}

#[test]
fn layout_problems_name_the_file_without_a_frame() {
    let root = project("layout");
    write(
        &root,
        "domains/planning/helpers/dates.jadpo",
        "type Day = Int {}\n",
    );
    let output = jadpo(&root, &["check"]);
    assert_eq!(
        output.stdout,
        "error: `helpers/` isn't a folder a domain has
 --> domains/planning/helpers/dates.jadpo
  reason: A domain holds only `entities/`, `types/`, `failures/`, `actions/`, `queries/`, `libraries/`, `routes/`, `schedules/`, `events/`, `subscribers/`, `emails/`, `translations/`, `prompts/` and `fixtures/`, each named after what its files declare.
  next: Move the file to `domains/planning/types/day.jadpo` (agent)
  rule: layout.unknown_location
  help: https://jadpo.dev/docs/errors/layout.unknown_location

check failed: 1 error, 4 files
"
    );
}

/// A domain folder that can't be listed fails the check rather than
/// passing a project with code missing (Phase 1 review, finding 22).
#[cfg(unix)]
#[test]
fn an_unreadable_folder_fails_the_check() {
    use std::os::unix::fs::PermissionsExt;
    let root = project("unreadable-folder");
    write(
        &root,
        "domains/billing/types/amount.jadpo",
        "type Amount = Int {}\n",
    );
    let locked = root.join("domains/billing");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");
    let listable = fs::read_dir(&locked).is_ok();
    let output = jadpo(&root, &["check"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("chmod back");
    if listable {
        // Running as root: every folder can be listed.
        return;
    }
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output
            .stdout
            .contains("`domains/billing/` couldn't be read"),
        "{}",
        output.stdout
    );
    assert!(output.stdout.contains("rule: toolchain.source_read_failed"));
}

/// A pipe named like a source file is reported, never read (it would block).
#[cfg(unix)]
#[test]
fn a_pipe_named_jadpo_is_not_read() {
    let root = project("fifo");
    let pipe = root.join("domains/planning/types/pipe.jadpo");
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .expect("mkfifo");
    assert!(made.success());
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output
            .stdout
            .contains("`domains/planning/types/pipe.jadpo` couldn't be read"),
        "{}",
        output.stdout
    );
    let formatted = jadpo(&root, &["fmt", "--check"]);
    assert!(
        formatted.stdout.contains("isn't a regular file"),
        "{}",
        formatted.stdout
    );
}

/// A 5,000-branch `else if` chain is one flat `if`: it checks and formats
/// without a depth error or a stack overflow (Phase 1 review, finding 18).
#[test]
fn a_long_else_if_chain_checks_and_formats() {
    let root = project("else-if-chain");
    let chain = " else if (count == 1) {\n        return count\n    }".repeat(5_000);
    write(
        &root,
        TALLY,
        &format!(
            "use planning/types {{ Count }}\n\naction tally(count: Count) -> Count {{\n    if (count == 0) {{\n        return count\n    }}{chain}\n    return count\n}}\n"
        ),
    );
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(
        output.stdout.starts_with("checked: 3 files"),
        "{}",
        output.stdout
    );
    let formatted = jadpo(&root, &["fmt", "--check"]);
    assert_eq!(formatted.code, 0, "{}", formatted.stdout);
}

/// A deployable built for Workers that reaches a connection with
/// `max_concurrent:` passes, and the check says the cap counts per
/// instance there (area 11's notes, Decisions, 2026-10-21; chunk 8.1o2).
#[test]
fn a_cap_on_calls_from_workers_is_said_to_count_per_instance() {
    let root = folder("per-instance");
    let files = [
        (
            "app.jadpo",
            "application {\n    connections: {\n        pdf: {\n            package: AcmePdf.v1\n            domains: [desk]\n            host: config.pdf_url\n            max_concurrent: 2\n        }\n        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }\n    }\n    rate_limits: {\n        store: limits\n    }\n    deployables: {\n        edge: {\n            serves: [main]\n            target: Target.wasm\n        }\n    }\n}\n",
        ),
        ("config.jadpo", "config {\n    pdf_url: Url\n}\n"),
        (
            "shared/providers/acme_pdf/v1/package.jadpo",
            "/// Acme's PDF service.\npackage AcmePdf.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            "shared/providers/acme_pdf/v1/provider.jadpo",
            "/// Acme's PDF service, run beside the app.\nprovider {\n    protocol: Protocol.json_http\n    runs: {\n        image: \"acme/pdf:1\"\n        port: 3000\n    }\n}\n",
        ),
        (
            "shared/providers/acme_pdf/v1/types/page_text.jadpo",
            "/// A page's text.\npublic type PageText = Text {\n    max_length: 200\n}\n",
        ),
        (
            "shared/providers/acme_pdf/v1/types/sheet.jadpo",
            "/// A sheet.\npublic type Sheet = Object {\n    text: PageText\n}\n",
        ),
        (
            "shared/providers/acme_pdf/v1/resources/documents.jadpo",
            "/// Rendered documents.\npublic resource documents {\n    render: {\n        method: HttpMethod.post\n        path: \"/render\"\n        input: Sheet\n        output: Sheet\n        source: PackageSource.written\n    }\n}\n",
        ),
        (
            "domains/desk/types/line.jadpo",
            "/// A line of text.\ntype Line = Text {\n    max_length: 200\n}\n",
        ),
        (
            "domains/desk/types/words.jadpo",
            "use desk/types { Line }\n\n/// What is said.\ntype Words = Input {\n    line: Line\n}\n",
        ),
        (
            "domains/desk/types/said.jadpo",
            "use desk/types { Line }\n\n/// What is said back.\ntype Said = Output {\n    line: Line\n}\n",
        ),
        (
            "domains/desk/actions/render.jadpo",
            "use app { pdf }\nuse desk/types { Said, Words }\nuse shared/providers/acme_pdf\n\n/// Renders the words.\naction render(input: Words) -> Said {\n    var sheet: AcmePdf.Sheet = pdf.documents.render(AcmePdf.Sheet {\n        text: AcmePdf.PageText(input.line)\n    })\n    assert sheet.text == sheet.text\n    return Said {\n        line: input.line\n    }\n}\n",
        ),
        (
            "policy.jadpo",
            "use desk\n\npolicy {\n    actions: {\n        desk.render: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/desk/routes/pages.jadpo",
            "use desk/types { Said, Words }\nuse desk/actions { render }\n\n/// Pages.\nroutes Pages {\n    prefix: /pages\n\n    /// Renders a page.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Words\n        output: Said\n        run: render(input)\n    }\n}\n",
        ),
    ];
    for (path, text) in files {
        write(&root, path, text);
    }
    jadpo(&root, &["records", "update"]);
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert!(output.stdout.contains("On Workers"), "{}", output.stdout);
    assert!(
        output
            .stdout
            .contains("pdf  2 at once in each instance of `edge`"),
        "{}",
        output.stdout
    );
    let json = jadpo(&root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(
        document["data"]["per_instance"],
        serde_json::json!([{ "connection": "pdf", "max_concurrent": 2, "deployable": "edge" }]),
        "{document}"
    );
}

/// A teaching example copied to a folder of its own.
fn example(name: &str, copy: &str) -> PathBuf {
    let root = folder(copy);
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    crate::deployable::copy_folder(&examples.join(name), &root);
    let _ = fs::remove_dir_all(root.join("build"));
    root
}

/// Area 1, "Tests and other files": a file that isn't Jadpo sits beside
/// the declaration that uses it, named after it, so one in a code folder
/// nothing refers to is `layout.asset_unreferenced`. An email's templates
/// and the shared layout, a partial's, a prompt's text and its recordings
/// are referred to; a template or a prompt's text beside no declaration is
/// its own rule's, and is reported once.
#[test]
fn a_file_that_isnt_jadpo_and_that_nothing_uses_is_an_error() {
    let rules = |output: &Output| -> Vec<String> {
        let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
        let mut found: Vec<String> = document["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .iter()
            .map(|d| {
                format!(
                    "{} {}",
                    d["rule"].as_str().unwrap_or(""),
                    d["location"]["path"].as_str().unwrap_or("")
                )
            })
            .collect();
        found.sort();
        found
    };
    let guest_house = example("27-guest-house", "assets-emails");
    let clean = jadpo(&guest_house, &["check"]);
    assert_eq!(clean.code, 0, "{}", clean.stdout);
    write(
        &guest_house,
        "domains/lodging/emails/old_welcome.html",
        "<p>Hello</p>\n",
    );
    write(&guest_house, "domains/lodging/actions/notes.md", "Notes.\n");
    let output = jadpo(&guest_house, &["check"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output.stdout.contains(
            "error: `notes.md` isn't used by any declaration\n --> domains/lodging/actions/notes.md\n  reason: A code folder holds Jadpo and the files its declarations use: an email's templates beside it in `emails/`, and a prompt's text and recordings beside it in `prompts/`. Nothing uses `notes.md`.\n  next: Move it out of `domains/`, or delete it if nothing needs it (agent)\n  rule: layout.asset_unreferenced\n"
        ),
        "{}",
        output.stdout
    );
    let output = jadpo(&guest_house, &["check", "--json"]);
    assert_eq!(
        rules(&output),
        [
            "email.template_missing domains/lodging/emails/old_welcome.html",
            "layout.asset_unreferenced domains/lodging/actions/notes.md",
        ],
        "{}",
        output.stdout
    );

    let helpline = example("22-plant-helpline", "assets-prompts");
    let clean = jadpo(&helpline, &["check"]);
    assert_eq!(clean.code, 0, "{}", clean.stdout);
    write(
        &helpline,
        "domains/helpline/prompts/old_answer.prompt",
        "# Instructions\n\nAnswer.\n\n# Request\n\nNothing.\n",
    );
    write(
        &helpline,
        "domains/helpline/prompts/recordings/nothing_here/first.json",
        "{}\n",
    );
    let output = jadpo(&helpline, &["check"]);
    assert!(
        output.stdout.contains(
            "error: `first.json` isn't used by any declaration\n --> domains/helpline/prompts/recordings/nothing_here/first.json\n  reason: A recording sits in `recordings/<prompt>/` beside the prompt it records, so `first.json` belongs to `NothingHere` in `domains/helpline/prompts/nothing_here.jadpo`, and no prompt there is called that.\n  next: Move it to the recordings of the prompt it records, or delete it if nothing will use it (agent)\n"
        ),
        "{}",
        output.stdout
    );
    let output = jadpo(&helpline, &["check", "--json"]);
    assert_eq!(
        rules(&output),
        [
            "layout.asset_unreferenced domains/helpline/prompts/recordings/nothing_here/first.json",
            "prompt.section_missing domains/helpline/prompts/old_answer.prompt",
        ],
        "{}",
        output.stdout
    );
}
