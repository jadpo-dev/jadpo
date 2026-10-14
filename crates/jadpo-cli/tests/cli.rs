//! The `jadpo` command shell: help generated from the command table, JSON
//! help, synonyms, nearest-command suggestions, command errors and `--json`.

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn piped() -> Environment {
    Environment::empty()
}

fn terminal() -> Environment {
    Environment::empty()
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
}

fn jadpo_in(env: &Environment, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, env)
}

fn jadpo(args: &[&str]) -> Output {
    jadpo_in(&piped(), args)
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.stderr.is_empty(),
        "--json writes nothing on stderr: {}",
        output.stderr
    );
    assert!(output.stdout.ends_with('\n'));
    serde_json::from_str(&output.stdout).expect("one JSON document")
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

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn top_level_plain() -> String {
    format!(
        "jadpo {VERSION}, a programming environment for coding agents

Build and check
  check         Check the project for errors
  test          Run tests
  fmt           Format source files

Data
  migrate       Write and apply migrations    new, apply, status
  sql           Show the SQL a query sends

Set up
  completions   Turn on Tab completion        zsh, bash, fish
  lsp           Run the language server for editors

Help
  help          Show commands, flags and examples
  version       Print the version

On every command
  --json        Print data for tools instead of text
  --help        Show every flag, with examples
  --color       Choose when to use colour     auto, always, never

next: jadpo help completions  shows how to turn on Tab completion
"
    )
}

#[test]
fn jadpo_alone_lists_every_built_command_by_job() {
    let output = jadpo(&[]);
    assert_eq!(output.code, 0);
    assert_eq!(output.stderr, "");
    assert_eq!(output.stdout, top_level_plain());
}

#[test]
fn help_and_dash_dash_help_print_the_same_screen() {
    assert_eq!(jadpo(&["help"]).stdout, top_level_plain());
    assert_eq!(jadpo(&["--help"]).stdout, top_level_plain());
    assert_eq!(jadpo(&["-h"]).stdout, top_level_plain());
}

#[test]
fn rich_help_uses_symbols_and_the_same_grid() {
    let output = jadpo_in(&terminal(), &[]);
    assert!(output.stdout.contains("\x1b["), "a terminal gets colour");
    assert_eq!(
        strip_ansi(&output.stdout),
        format!(
            "jadpo {VERSION} · a programming environment for coding agents

Build and check
  check         Check the project for errors
  test          Run tests
  fmt           Format source files

Data
  migrate       Write and apply migrations    new · apply · status
  sql           Show the SQL a query sends

Set up
  completions   Turn on Tab completion        zsh · bash · fish
  lsp           Run the language server for editors

Help
  help          Show commands, flags and examples
  version       Print the version

On every command
  --json        Print data for tools instead of text
  --help        Show every flag, with examples
  --color       Choose when to use colour     auto · always · never

→ jadpo help completions  shows how to turn on Tab completion
"
        )
    );
}

#[test]
fn no_color_keeps_symbols_and_drops_colour() {
    let output = jadpo_in(&terminal().with_var("NO_COLOR", "1"), &[]);
    assert!(
        !output.stdout.contains("\x1b[3"),
        "no colour codes: {:?}",
        output.stdout
    );
    assert!(output.stdout.contains("→ jadpo help completions"));
    let output = jadpo_in(&terminal(), &["--color=never"]);
    assert!(
        !output.stdout.contains("\x1b[3"),
        "no colour codes: {:?}",
        output.stdout
    );
}

#[test]
fn color_always_colours_a_pipe() {
    let output = jadpo(&["--color", "always"]);
    assert!(
        output.stdout.contains("\x1b[36m→ jadpo help completions"),
        "{:?}",
        output.stdout
    );
}

fn completions_plain() -> &'static str {
    r#"jadpo completions, also called completion
Print the script that makes Tab complete jadpo's commands, flags and values,
each with a one-line description. Load it from your shell's start-up file.

Usage
  jadpo completions <shell>

Arguments
  <shell>       The shell to complete in      zsh, bash, fish

On every command
  --json        Print data for tools instead of text
  --help        Show every flag, with examples
  --color       Choose when to use colour     auto, always, never

Examples
  Turn on Tab completion in zsh
    echo 'source <(jadpo completions zsh)' >> ~/.zshrc

  Turn on Tab completion in bash
    echo 'eval "$(jadpo completions bash)"' >> ~/.bashrc

  Turn on Tab completion in fish
    jadpo completions fish > ~/.config/fish/completions/jadpo.fish
"#
}

#[test]
fn command_help_shows_usage_arguments_flags_and_examples() {
    assert_eq!(jadpo(&["help", "completions"]).stdout, completions_plain());
    assert_eq!(
        jadpo(&["completions", "--help"]).stdout,
        completions_plain()
    );
    assert_eq!(
        jadpo(&["help", "completion"]).stdout,
        completions_plain(),
        "synonyms reach the same help"
    );
}

#[test]
fn every_command_has_help_with_an_example() {
    for command in ["help", "version", "completions", "fmt"] {
        let output = jadpo(&["help", command]);
        assert_eq!(output.code, 0, "{command}");
        assert!(
            output.stdout.starts_with(&format!("jadpo {command}")),
            "{command}: {}",
            output.stdout
        );
        assert!(output.stdout.contains("\nUsage\n"), "{command}");
        assert!(output.stdout.contains("\nExamples\n"), "{command}");
        assert!(
            output
                .stdout
                .lines()
                .all(|line| line.chars().count() <= 80 || line.starts_with("    ")),
            "{command}"
        );
    }
}

#[test]
fn synonyms_run_the_canonical_command() {
    assert_eq!(
        jadpo(&["completion", "zsh"]).stdout,
        jadpo(&["completions", "zsh"]).stdout
    );
}

#[test]
fn version_prints_the_version() {
    assert_eq!(jadpo(&["version"]).stdout, format!("jadpo {VERSION}\n"));
    assert_eq!(jadpo(&["--version"]).stdout, format!("jadpo {VERSION}\n"));
    let document = json(&jadpo(&["version", "--json"]));
    assert_eq!(document["command"], "version");
    assert_eq!(document["data"], serde_json::json!({ "version": VERSION }));
}

#[test]
fn help_json_is_the_whole_command_reference() {
    let output = jadpo(&["help", "--json"]);
    assert_eq!(output.code, 0);
    let document = json(&output);
    assert_eq!(document["schemaVersion"], 2);
    assert_eq!(document["command"], "help");
    assert_eq!(document["ok"], true);
    let data = &document["data"];
    assert_eq!(data["name"], "jadpo");
    assert_eq!(data["version"], VERSION);
    let names: Vec<&str> = data["commands"]
        .as_array()
        .expect("commands")
        .iter()
        .map(|c| c["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        [
            "check",
            "test",
            "fmt",
            "migrate",
            "sql",
            "completions",
            "lsp",
            "help",
            "version"
        ]
    );
    let completions = &data["commands"][5];
    assert_eq!(completions["synonyms"], serde_json::json!(["completion"]));
    assert_eq!(completions["group"], "Set up");
    assert_eq!(completions["usage"], "jadpo completions <shell>");
    assert_eq!(completions["arguments"][0]["name"], "shell");
    assert_eq!(completions["arguments"][0]["required"], true);
    let shells: Vec<&str> = completions["arguments"][0]["values"]
        .as_array()
        .expect("values")
        .iter()
        .map(|v| v["name"].as_str().expect("name"))
        .collect();
    assert_eq!(shells, ["zsh", "bash", "fish"]);
    assert!(
        completions["examples"]
            .as_array()
            .expect("examples")
            .iter()
            .all(|e| e["command"].is_string() && e["description"].is_string())
    );
    assert_eq!(
        completions["rules"],
        serde_json::json!(["cli.completions_arguments"])
    );
    let flags: Vec<&str> = data["flags"]
        .as_array()
        .expect("flags")
        .iter()
        .map(|f| f["name"].as_str().expect("name"))
        .collect();
    assert_eq!(flags, ["--json", "--help", "--color"]);
    assert_eq!(data["flags"][2]["value"], "when");
    let rules = data["rules"].as_array().expect("rules");
    assert!(rules.iter().any(|rule| rule["id"] == "cli.unknown_command"
        && rule["area"] == 14
        && rule["severity"] == "error"
        && rule["decides"] == "agent"));
    assert_eq!(rules.len(), jadpo_diag::CATALOGUE.len());
    // `jadpo test` lists every code a run can end in, beside its own
    // argument errors.
    let test = &data["commands"][1];
    assert_eq!(test["usage"], "jadpo test [path]");
    assert_eq!(
        test["rules"],
        serde_json::json!([
            "cli.test_arguments",
            "test.assertion_failed",
            "test.unexpected_failure",
            "test.fault",
            "test.failed",
            "test.no_tests",
            "test.runtime_start_failed",
            "toolchain.not_built_yet",
            "toolchain.project_not_found"
        ])
    );
    let flags: Vec<(&str, &serde_json::Value)> = test["flags"]
        .as_array()
        .expect("flags")
        .iter()
        .map(|f| (f["name"].as_str().expect("name"), &f["value"]))
        .collect();
    assert_eq!(
        flags,
        [
            ("--name", &serde_json::json!("text")),
            ("--kind", &serde_json::json!("kind")),
            ("--failed", &serde_json::Value::Null),
            ("--seed", &serde_json::json!("n")),
            ("--watch", &serde_json::Value::Null),
            ("--keep-failed", &serde_json::Value::Null),
        ]
    );
}

#[test]
fn help_json_for_one_command_is_that_command() {
    let document = json(&jadpo(&["help", "completions", "--json"]));
    assert_eq!(document["data"]["name"], "completions");
    assert_eq!(json(&jadpo(&["completions", "--help", "--json"])), document);
    assert_eq!(json(&jadpo(&["--json"])), json(&jadpo(&["help", "--json"])));
}

#[test]
fn hidden_commands_never_appear_in_help() {
    let reference = json(&jadpo(&["help", "--json"]));
    assert!(!reference.to_string().contains("__complete"));
    assert!(!jadpo(&[]).stdout.contains("__complete"));
}

fn unknown_command_plain(word: &str, reason: &str, next: &str) -> String {
    format!(
        "error: `{word}` isn't a jadpo command
  reason: {reason}
  next: {next} (agent)
  rule: cli.unknown_command
  help: docs/language/14-tooling.html#command-errors
"
    )
}

#[test]
fn an_unknown_command_suggests_the_nearest_one() {
    let output = jadpo(&["hlep"]);
    assert_eq!(output.code, 2);
    assert_eq!(output.stdout, "");
    assert_eq!(
        output.stderr,
        unknown_command_plain(
            "hlep",
            "Jadpo has no command with this name. Did you mean `help`?",
            "Use `help` instead of `hlep`"
        )
    );
    assert!(
        jadpo(&["verison"])
            .stderr
            .contains("Did you mean `version`?")
    );
    assert!(
        jadpo(&["complet"])
            .stderr
            .contains("Did you mean `completion`?")
    );
    assert!(
        jadpo(&["completoin"])
            .stderr
            .contains("Did you mean `completion`?"),
        "synonyms are suggested as written"
    );
}

#[test]
fn an_unknown_command_with_nothing_near_points_at_help() {
    let output = jadpo(&["frobnicate"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        unknown_command_plain(
            "frobnicate",
            "Jadpo has no command with this name.",
            "Run `jadpo help` to see every command"
        )
    );
}

#[test]
fn commands_that_are_not_built_yet_are_unknown() {
    let output = jadpo(&["build"]);
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .starts_with("error: `build` isn't a jadpo command"),
        "{}",
        output.stderr
    );
}

#[test]
fn help_for_an_unknown_command_is_the_same_error() {
    let output = jadpo(&["help", "hlep"]);
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .starts_with("error: `hlep` isn't a jadpo command\n"),
        "{}",
        output.stderr
    );
}

#[test]
fn errors_in_a_terminal_are_rich() {
    let output = jadpo_in(&terminal().with_var("NO_COLOR", "1"), &["hlep"]);
    assert_eq!(
        strip_ansi(&output.stderr),
        "✗ error  `hlep` isn't a jadpo command
  reason    Jadpo has no command with this name. Did you mean `help`?
  next      Use `help` instead of `hlep` (agent)
  rule      cli.unknown_command
  help      docs/language/14-tooling.html#command-errors
"
    );
}

#[test]
fn json_errors_go_to_stdout_with_the_same_exit_code() {
    let output = jadpo(&["hlep", "--json"]);
    assert_eq!(output.code, 2);
    let document = json(&output);
    assert_eq!(document["ok"], false);
    assert_eq!(document["command"], serde_json::Value::Null);
    assert_eq!(document["data"], serde_json::Value::Null);
    assert_eq!(document["diagnostics"][0]["rule"], "cli.unknown_command");
    assert_eq!(
        document["diagnostics"][0]["reason"],
        "Jadpo has no command with this name. Did you mean `help`?"
    );
}

#[test]
fn an_unknown_flag_suggests_the_nearest_flag() {
    let output = jadpo(&["help", "--jsno"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "error: `--jsno` isn't a flag of `jadpo help`
  reason: `jadpo help` has no flag with this name. Did you mean `--json`?
  next: Use `--json` instead of `--jsno` (agent)
  rule: cli.unknown_flag
  help: docs/language/14-tooling.html#command-errors
"
    );
    let output = jadpo(&["--frob"]);
    assert_eq!(
        output.stderr,
        "error: `--frob` isn't a jadpo flag
  reason: Jadpo has no flag with this name.
  next: Run `jadpo --help` to see every flag (agent)
  rule: cli.unknown_flag
  help: docs/language/14-tooling.html#command-errors
"
    );
}

#[test]
fn a_bad_color_value_lists_the_valid_ones() {
    let output = jadpo(&["--color=sometimes"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "error: `sometimes` isn't a colour setting
  reason: `--color` takes auto, always or never.
  next: Use `--color=auto`, `--color=always` or `--color=never` (agent)
  rule: cli.presentation_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    assert!(
        jadpo(&["--color=alway"])
            .stderr
            .contains("Did you mean `always`?")
    );
    let output = jadpo(&["--color"]);
    assert!(
        output
            .stderr
            .starts_with("error: `--color` needs a value\n"),
        "{}",
        output.stderr
    );
}

#[test]
fn completions_without_a_shell_lists_the_shells() {
    let output = jadpo(&["completions"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "error: `jadpo completions` needs a shell
  reason: Each shell gets its own script: zsh, bash or fish.
  next: Add the shell, for example `jadpo completions zsh` (agent)
  rule: cli.completions_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
}

#[test]
fn completions_for_an_unknown_shell_suggests_one() {
    let output = jadpo(&["completions", "zhs"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "error: `zhs` isn't a shell jadpo completes
  reason: Jadpo writes completion scripts for zsh, bash and fish. Did you mean `zsh`?
  next: Use `zsh` instead of `zhs` (agent)
  rule: cli.completions_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    assert!(
        jadpo(&["completions", "tcsh"])
            .stderr
            .contains("next: Use `zsh`, `bash` or `fish` (agent)")
    );
}

#[test]
fn extra_arguments_are_refused() {
    let output = jadpo(&["version", "extra"]);
    assert_eq!(output.code, 2);
    assert_eq!(
        output.stderr,
        "error: `jadpo version` takes no arguments
  reason: It was given `extra`.
  next: Run `jadpo version` (agent)
  rule: cli.version_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    let output = jadpo(&["completions", "zsh", "bash"]);
    assert!(
        output
            .stderr
            .starts_with("error: `jadpo completions` takes one argument\n"),
        "{}",
        output.stderr
    );
    assert!(
        output
            .stderr
            .contains("next: Run `jadpo completions zsh` (agent)"),
        "{}",
        output.stderr
    );
    let output = jadpo(&["help", "version", "help"]);
    assert!(
        output.stderr.contains("rule: cli.help_arguments"),
        "{}",
        output.stderr
    );
}

#[test]
fn completions_print_a_script_for_each_shell() {
    for shell in ["zsh", "bash", "fish"] {
        let output = jadpo(&["completions", shell]);
        assert_eq!(output.code, 0, "{shell}");
        assert!(
            output.stdout.contains("jadpo __complete --"),
            "{shell}: {}",
            output.stdout
        );
        let document = json(&jadpo(&["completions", shell, "--json"]));
        assert_eq!(document["data"]["shell"], shell);
        assert_eq!(document["data"]["script"], output.stdout);
    }
    assert!(
        jadpo(&["completions", "zsh"])
            .stdout
            .starts_with("#compdef jadpo\n")
    );
    assert!(
        jadpo(&["completions", "bash"])
            .stdout
            .contains("complete -o default -F _jadpo jadpo")
    );
    assert!(
        jadpo(&["completions", "fish"])
            .stdout
            .contains("complete -c jadpo -f -a '(__jadpo_complete)'")
    );
}

fn complete(words: &[&str]) -> Vec<String> {
    let mut args = vec!["__complete", "--"];
    args.extend_from_slice(words);
    let output = jadpo(&args);
    assert_eq!(output.code, 0);
    output.stdout.lines().map(str::to_string).collect()
}

#[test]
fn tab_at_the_top_level_lists_every_command_with_its_description() {
    assert_eq!(
        complete(&[""]),
        [
            "check\tCheck the project for errors",
            "test\tRun tests",
            "fmt\tFormat source files",
            "migrate\tWrite and apply migrations",
            "sql\tShow the SQL a query sends",
            "completions\tTurn on Tab completion",
            "lsp\tRun the language server for editors",
            "help\tShow commands, flags and examples",
            "version\tPrint the version",
        ]
    );
    assert_eq!(complete(&["ver"]), ["version\tPrint the version"]);
}

#[test]
fn tab_completes_flags_with_descriptions_and_values() {
    assert_eq!(
        complete(&["help", "--"]),
        [
            "--json\tPrint data for tools instead of text",
            "--help\tShow every flag, with examples",
            "--color\tChoose when to use colour",
        ]
    );
    assert_eq!(
        complete(&["--color="]),
        [
            "--color=auto\tDecide from the terminal",
            "--color=always\tAlways use colour",
            "--color=never\tNever use colour"
        ]
    );
    assert_eq!(
        complete(&["--color", "a"]),
        [
            "auto\tDecide from the terminal",
            "always\tAlways use colour"
        ]
    );
}

#[test]
fn tab_completes_argument_values() {
    assert_eq!(
        complete(&["completions", ""]),
        [
            "zsh\tThe Z shell, macOS's default",
            "bash\tThe Bourne-again shell",
            "fish\tThe friendly interactive shell"
        ]
    );
    assert_eq!(
        complete(&["completion", "f"]),
        ["fish\tThe friendly interactive shell"]
    );
    assert_eq!(
        complete(&["help", "c"]),
        [
            "check\tCheck the project for errors",
            "completions\tTurn on Tab completion"
        ]
    );
    assert!(complete(&["version", ""]).is_empty());
    assert!(complete(&["completions", "zsh", ""]).is_empty());
}
