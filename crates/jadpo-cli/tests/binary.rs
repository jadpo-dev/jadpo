//! The built `jadpo` binary end to end: streams, exit codes, the environment
//! it reads, and the completion scripts in real shells where they exist.

use std::path::Path;
use std::process::{Command, Output};

const JADPO: &str = env!("CARGO_BIN_EXE_jadpo");

/// Runs the binary with a clean environment, so the developer's terminal
/// settings never leak into the result.
fn jadpo(args: &[&str], vars: &[(&str, &str)]) -> Output {
    let mut command = Command::new(JADPO);
    command.args(args).env_clear();
    for (key, value) in vars {
        command.env(key, value);
    }
    command.output().expect("jadpo runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

#[test]
fn piped_help_is_plain_on_stdout() {
    let output = jadpo(&[], &[]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = text(&output.stdout);
    assert!(stdout.starts_with("jadpo "), "{stdout}");
    assert!(!stdout.contains('\x1b'), "no escape codes on a pipe");
    assert!(
        stdout.ends_with("next: jadpo help completions  shows how to turn on Tab completion\n")
    );
}

#[test]
fn force_color_colours_a_pipe_and_no_color_wins() {
    let coloured = text(&jadpo(&[], &[("FORCE_COLOR", "1")]).stdout);
    assert!(coloured.contains("\x1b[36m→"), "{coloured:?}");
    let plain = text(&jadpo(&[], &[("FORCE_COLOR", "1"), ("NO_COLOR", "1")]).stdout);
    assert!(!plain.contains("\x1b[36m"), "{plain:?}");
}

#[test]
fn a_usage_error_exits_two_on_stderr() {
    let output = jadpo(&["hlep"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).starts_with("error: `hlep` isn't a jadpo command\n"));
}

#[test]
fn json_errors_stay_on_stdout() {
    let output = jadpo(&["hlep", "--json"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(document["ok"], false);
}

fn shell(name: &str) -> Option<String> {
    let found = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .ok()?;
    found
        .status
        .success()
        .then(|| text(&found.stdout).trim().to_string())
}

#[test]
fn completion_scripts_parse_in_their_shells() {
    for name in ["zsh", "bash", "fish"] {
        let Some(path) = shell(name) else { continue };
        let script = text(&jadpo(&["completions", name], &[]).stdout);
        let check = Command::new(path)
            .args(["-n", "-c", &script])
            .output()
            .expect("shell runs");
        assert!(check.status.success(), "{name}: {}", text(&check.stderr));
    }
}

#[test]
fn bash_completes_commands_and_values() {
    let Some(bash) = shell("bash") else { return };
    let directory = Path::new(JADPO).parent().expect("binary directory");
    let complete = |words: &str, current: usize| -> Vec<String> {
        let program = format!(
            "eval \"$(jadpo completions bash)\"\nCOMP_WORDS=({words}); COMP_CWORD={current}; _jadpo; printf '%s\\n' \"${{COMPREPLY[@]}}\""
        );
        let output = Command::new(&bash)
            .args(["--noprofile", "--norc", "-c", &program])
            .env("PATH", format!("{}:/usr/bin:/bin", directory.display()))
            .output()
            .expect("bash runs");
        assert!(output.status.success(), "{}", text(&output.stderr));
        text(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        complete("jadpo ''", 1),
        [
            "check",
            "test",
            "dev",
            "build",
            "fmt",
            "package",
            "migrate",
            "sql",
            "routes",
            "ai",
            "calls",
            "email",
            "why",
            "api",
            "records",
            "declines",
            "fix",
            "suggest",
            "translations",
            "schedules",
            "deliveries",
            "runs",
            "config",
            "completions",
            "lsp",
            "help",
            "version"
        ]
    );
    assert_eq!(complete("jadpo ver", 1), ["version"]);
    assert_eq!(
        complete("jadpo test --kind ''", 3),
        ["pure", "action", "route", "schedule", "recording"]
    );
    assert_eq!(complete("jadpo test --f", 2), ["--failed"]);
    assert_eq!(complete("jadpo completions ''", 2), ["zsh", "bash", "fish"]);
    assert_eq!(complete("jadpo help --j", 2), ["--json"]);
}
