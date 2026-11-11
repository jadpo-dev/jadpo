//! `jadpo test --interactive` (area 13, "Interactive mode"; CLI design
//! language, "Keys"): the keyboard view of every test, grouped by domain
//! and file with each one's last result. Every screen is compared exactly
//! through `View`, driven by keys with no terminal or clock; the session
//! loop runs once on scripted keys; and the command refuses `--json`, a
//! missing terminal and a selection of its own.

use std::path::{Path, PathBuf};

use jadpo_cli::interactive::{Effect, InteractiveArgs, View, serve_to};
use jadpo_cli::keys::{Key, Keys};
use jadpo_cli::{Session, run};
use jadpo_term::{ColorChoice, Environment, Mode, Style};
use jadpo_test::Options;
use jadpo_test::watch::{Edit, EditKind, Pick};

use crate::affected::{edit, folder, project, write};
use crate::support;

const NORTH_TESTS: &str = "domains/north/actions/north_add.test.jadpo";
const SOUTH_TESTS: &str = "domains/south/actions/south_add.test.jadpo";
const SOUTH: &str = "domains/south/actions/south_add.jadpo";

/// Two domains, north's file with two tests, south's with one that fails,
/// and a run of them all, so each test has a last result.
fn fixture(name: &str) -> PathBuf {
    let root = folder(&format!("interactive-{name}"));
    project(&root);
    write(
        &root,
        NORTH_TESTS,
        "use shared/types { Count }\n\ntest \"north adds\" {\n    var got: Count = north_add(1, 2)\n    assert got == 3\n}\n\ntest \"north adds zero\" {\n    var got: Count = north_add(0, 2)\n    assert got == 2\n}\n",
    );
    edit(&root, SOUTH_TESTS, "got == 3", "got == 4");
    let ran = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(ran.code, 1, "{}", ran.stdout);
    root
}

fn jadpo_in(env: Environment, cwd: &Path, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &env.with_cwd(cwd))
}

fn jadpo(cwd: &Path, args: &[&str]) -> jadpo_cli::Output {
    jadpo_in(Environment::empty(), cwd, args)
}

fn terminal() -> Environment {
    Environment::empty()
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
}

fn args(root: &Path) -> InteractiveArgs {
    InteractiveArgs {
        options: Options {
            cwd: root.to_path_buf(),
            seed: Some(1),
            ..Options::default()
        },
        watch: false,
        choice: ColorChoice::Auto,
    }
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The frame 14 rows by 60 columns, without colour or run times, one
/// line per row with its trailing spaces.
fn frame(view: &mut View) -> String {
    let lines: Vec<String> = view
        .lines(14, 60)
        .iter()
        .map(|line| support::without_run_time(&strip_ansi(line)))
        .collect();
    lines.join("\n")
}

/// What a key does, carried out as the session would.
fn press(view: &mut View, key: Key) -> Effect {
    let effect = view.press(key);
    if let Effect::Run(pick) = &effect {
        view.begin(pick);
        view.run(pick.clone(), Vec::new());
    }
    effect
}

fn open(root: &Path) -> View {
    View::open(&args(root), Style::new(Mode::Rich)).expect("opens")
}

#[test]
fn the_list_groups_every_test_by_domain_and_file_with_its_last_result() {
    let root = fixture("list");
    let mut view = open(&root);
    assert_eq!(
        frame(&mut view),
        "\
jadpo test --interactive · 3 tests · 2 files

  north
› actions/north_add.test.jadpo                      2 passed
    ✓ north adds
    ✓ north adds zero
  south
  actions/south_add.test.jadpo                      1 failed
    ✗ south adds



○ 3 tests  1 failed last time
  / search  enter run file  a run all  f run failed  w watch  q quit"
    );
}

#[test]
fn keys_run_a_test_a_file_everything_and_the_failures() {
    let root = fixture("keys");
    let mut view = open(&root);
    // Down to the first test, which runs alone.
    view.press(Key::Down);
    let effect = view.press(Key::Enter);
    let Effect::Run(pick) = effect else {
        panic!("enter runs: {effect:?}");
    };
    view.begin(&pick);
    assert_eq!(
        frame(&mut view),
        "\
jadpo test --interactive · 3 tests · 2 files

  north
  actions/north_add.test.jadpo          1 passed · 1 running
›   ● north adds
    ✓ north adds zero
  south
  actions/south_add.test.jadpo                      1 failed
    ✗ south adds



● Running 1 test
  / search  enter run test  a run all  f run failed  w watch  q quit"
    );
    view.run(pick, Vec::new());
    assert!(
        frame(&mut view).ends_with("\n✓ 1 passed  1 of 3 tests\n  / search  enter run test  a run all  f run failed  w watch  q quit"),
        "{}",
        frame(&mut view)
    );
    // `f` runs the failing test, whose report `o` then opens.
    assert_eq!(press(&mut view, Key::Char('f')), Effect::Run(Pick::Failing));
    assert!(
        frame(&mut view).contains("\n✗ 1 failed  1 of 3 tests\n"),
        "{}",
        frame(&mut view)
    );
    view.press(Key::Down);
    view.press(Key::Down);
    view.press(Key::Down);
    view.press(Key::Down);
    assert!(
        frame(&mut view).ends_with(
            "  / search  enter run test  a run all  f run failed  o open report  w watch  q quit"
        ),
        "{}",
        frame(&mut view)
    );
    assert_eq!(view.press(Key::Char('o')), Effect::Redraw);
    let report = frame(&mut view);
    assert!(
        report.starts_with("jadpo test --interactive · south adds\n\n✗ failed  south adds\n"),
        "{report}"
    );
    assert!(
        report.contains("  expected  4\n  actual    3\n"),
        "{report}"
    );
    assert!(
        report.ends_with("\n  enter run again  esc back  q quit"),
        "{report}"
    );
    // Fixed, `enter` on the report runs it again, and passing, it is
    // back on the list.
    edit(&root, SOUTH_TESTS, "got == 4", "got == 3");
    let effect = view.press(Key::Enter);
    let Effect::Run(pick) = effect else {
        panic!("enter runs again: {effect:?}");
    };
    view.begin(&pick);
    view.run(pick, vec![Edit::new(SOUTH_TESTS, EditKind::Saved)]);
    let list = frame(&mut view);
    assert!(list.contains("\n›   ✓ south adds\n"), "{list}");
    assert!(
        list.ends_with(
            "\n✓ 1 passed  1 of 3 tests\n  / search  enter run test  a run all  w watch  q quit"
        ),
        "{list}"
    );
    // `a` runs everything; a file's `enter` runs its tests.
    assert_eq!(press(&mut view, Key::Char('a')), Effect::Run(Pick::All));
    assert!(frame(&mut view).contains("\n✓ 3 passed  3 action tests\n"));
    view.press(Key::Up);
    view.press(Key::Up);
    view.press(Key::Up);
    view.press(Key::Up);
    view.press(Key::Up);
    press(&mut view, Key::Enter);
    assert!(frame(&mut view).contains("\n✓ 2 passed  2 of 3 tests\n"));
    // `f` with nothing failing says so.
    assert_eq!(view.press(Key::Char('f')), Effect::Redraw);
    assert!(frame(&mut view).contains("\n  No test is failing\n"));
    assert_eq!(view.press(Key::Char('q')), Effect::Quit);
}

#[test]
fn typing_searches_files_and_tests() {
    let root = fixture("search");
    let mut view = open(&root);
    view.press(Key::Char('/'));
    for c in "zero".chars() {
        view.press(Key::Char(c));
    }
    assert_eq!(
        frame(&mut view),
        "\
jadpo test --interactive · 3 tests · 2 files · 1 match

  north
› actions/north_add.test.jadpo                      2 passed
    ✓ north adds zero







○ 3 tests  1 failed last time
/ zero  enter run  esc clear"
    );
    // A file's path matches all its tests; `enter` runs the selection
    // and keeps the search.
    for _ in 0.."zero".len() {
        view.press(Key::Backspace);
    }
    for c in "south_add".chars() {
        view.press(Key::Char(c));
    }
    press(&mut view, Key::Enter);
    let found = frame(&mut view);
    assert!(
        found.starts_with("jadpo test --interactive · 3 tests · 2 files · 1 match\n\n  south\n› actions/south_add.test.jadpo"),
        "{found}"
    );
    assert!(found.contains("\n✗ 1 failed  1 of 3 tests\n"), "{found}");
    // Nothing matching says so; `esc` clears the search.
    view.press(Key::Char('/'));
    view.press(Key::Char('x'));
    assert!(frame(&mut view).contains("\n  Nothing matches \"south_addx\"\n"));
    view.press(Key::Escape);
    assert!(
        frame(&mut view).starts_with("jadpo test --interactive · 3 tests · 2 files\n\n  north\n")
    );
}

#[test]
fn watching_runs_what_a_save_reaches_and_shows_errors() {
    let root = fixture("watching");
    let mut view = open(&root);
    assert_eq!(view.press(Key::Char('w')), Effect::Watch(true));
    assert!(
        frame(&mut view).starts_with("jadpo test --interactive · 3 tests · 2 files · watching\n")
    );
    assert!(frame(&mut view).ends_with("  w stop  q quit"));
    view.saved(Vec::new(), "14:02:31");
    edit(&root, SOUTH, "(a + b)", "(b + a)");
    view.saved(vec![Edit::new(SOUTH, EditKind::Saved)], "14:02:31");
    let saved = frame(&mut view);
    assert!(
        saved.contains("\n  14:02:31 · saved domains/south/actions/south_add.jadpo · 1 test reaches it\n✗ 1 failed  1 of 3 tests\n"),
        "{saved}"
    );
    // A save that breaks the build shows its errors; `esc` goes back.
    edit(&root, SOUTH, "(b + a)", "(b + nothing)");
    view.saved(vec![Edit::new(SOUTH, EditKind::Saved)], "14:03:05");
    let broken = frame(&mut view);
    assert!(
        broken.starts_with("jadpo test --interactive · tests not run · watching\n\n✗ error  "),
        "{broken}"
    );
    assert!(broken.contains("\n✗ tests not run  "), "{broken}");
    assert!(
        broken.ends_with("\n  a run all  esc back  q quit"),
        "{broken}"
    );
    // The fix runs what changed since the last run, back on the list.
    edit(&root, SOUTH, "(b + nothing)", "(a + b)");
    view.saved(vec![Edit::new(SOUTH, EditKind::Saved)], "14:03:40");
    let fixed = frame(&mut view);
    assert!(
        fixed.starts_with("jadpo test --interactive · 3 tests · 2 files · watching\n\n  north\n"),
        "{fixed}"
    );
    assert!(fixed.contains("\n✗ 1 failed  1 of 3 tests\n"), "{fixed}");
    assert_eq!(view.press(Key::Char('w')), Effect::Watch(false));
    assert!(frame(&mut view).ends_with("  w watch  q quit"));
}

#[test]
fn the_session_runs_on_keys_and_leaves_the_last_footer() {
    let root = fixture("session");
    let keys = Keys::script([Key::Char('a'), Key::Char('q')]);
    let mut out = Vec::new();
    let code = serve_to(
        &args(&root),
        Style::new(Mode::Rich),
        &keys,
        &mut out,
        &|| (14, 60),
        &|| "12:00:00".to_string(),
    );
    assert_eq!(code, 0);
    let out = String::from_utf8(out).expect("text");
    assert!(out.starts_with("\x1b[?1049h\x1b[?25l\x1b[H"), "{out:?}");
    assert!(out.contains("● Running 3 tests"), "{out:?}");
    let closing = out
        .rsplit_once("\x1b[?25h\x1b[?1049l")
        .expect("the alternate screen is left")
        .1;
    assert_eq!(
        support::without_run_time(&strip_ansi(closing)),
        "✗ 1 failed  2 passed · 3 action tests\n"
    );
}

#[test]
fn the_command_needs_a_terminal_and_no_selection_of_its_own() {
    let root = fixture("command");
    let piped = jadpo(&root, &["test", "--interactive"]);
    assert_eq!(piped.code, 2);
    assert_eq!(
        piped.stderr,
        "\
error: `jadpo test --interactive` needs a terminal
  reason: It is a keyboard view redrawn on every key, so its input and output must be a terminal it can draw in, not a pipe or plain output.
  next: Use `jadpo test` with a path, `--name`, `--kind` or `--failed`, or `jadpo test --watch`, which work anywhere (agent)
  rule: cli.test_arguments
  help: https://jadpo.dev/docs/errors/cli.test_arguments
"
    );
    let json = jadpo_in(terminal(), &root, &["test", "--interactive", "--json"]);
    assert_eq!(json.code, 2);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("json");
    assert_eq!(
        document["diagnostics"][0]["message"],
        "`jadpo test --interactive` can't write JSON"
    );
    let named = jadpo_in(
        terminal(),
        &root,
        &["test", "--interactive", "--name", "adds"],
    );
    assert_eq!(named.code, 2);
    assert!(
        strip_ansi(&named.stderr).contains("`--interactive` takes no `--name`"),
        "{}",
        named.stderr
    );
    let session = jadpo_in(terminal(), &root, &["test", "--interactive", "--watch"]);
    assert_eq!(session.code, 0, "{}", session.stderr);
    let Some(Session::Interactive(args)) = session.session else {
        panic!("a session: {:?}", session.session);
    };
    assert!(args.watch);
    // Help offers the flag.
    let help = jadpo(&root, &["test", "--help"]);
    assert!(help.stdout.contains("--interactive"), "{}", help.stdout);
    assert!(help.stdout.contains("--affected"), "{}", help.stdout);
}
