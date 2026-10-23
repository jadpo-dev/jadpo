//! Configuration (area 10, chunk 5.1) through the commands: `jadpo config
//! check` against the environment over `.env.local`, `jadpo config set`
//! checking and storing a value the safe way, `jadpo dev` refusing to start
//! with a missing or invalid required value and reloading when `.env.local`
//! is fixed, and `jadpo test` reading `test:` values and a test's own
//! `config:`. No value, a secret's above all, appears in anything printed.
//! The live test that opens a database from a setting skips when
//! `JADPO_SKIP_POSTGRES=1`.

use std::fs;
use std::io::{Read, Write as _};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jadpo_cli::config::{SetArgs, store};
use jadpo_cli::dev::{DevArgs, serve_to};
use jadpo_cli::{Output, run};
use jadpo_term::{ColorChoice, Environment, Screen, Style};

use crate::support;

const SECRET: &str = "0123456789abcdef0123456789abcdef0123";

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("config-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

/// Saves `text` to `path` in one step, as an editor's atomic save does:
/// written to a hidden file beside it, which no watch sees, then renamed
/// over it, so a running `jadpo dev` never reads `.env.local` half-written
/// (chunk 5.13b).
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    let folder = path.parent().expect("parent");
    fs::create_dir_all(folder).expect("folders");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a name");
    let saving = folder.join(format!(".{name}.saving"));
    fs::write(&saving, text).expect("write");
    fs::rename(&saving, &path).expect("save");
}

fn jadpo_with(env: Environment, cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    jadpo_with(Environment::empty(), cwd, args)
}

/// A project whose action greets with two settings, and a third that is a
/// signing key.
const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "config.jadpo",
        r#"use site/types { HelloText, ListSize }

config {
    greeting: HelloText {
        test: "hello from the tests"
    }
    page_size: ListSize {
        default: 25
    }
    business_zone: Zone {
        default: Zone.europe_london
    }
    session_signing_key: SigningKey
}
"#,
    ),
    (
        "policy.jadpo",
        "use site\n\npolicy {\n    actions: {\n        site.greet: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/site/types/hello_text.jadpo",
        "type HelloText = Text {\n    max_length: 200\n}\n",
    ),
    (
        "domains/site/types/list_size.jadpo",
        "type ListSize = Int {\n    min: 1\n    max: 100\n}\n",
    ),
    (
        "domains/site/types/welcome.jadpo",
        "use site/types { HelloText, ListSize }\n\ntype Welcome = Output {\n    text: HelloText\n    list_size: ListSize\n}\n",
    ),
    (
        "domains/site/actions/greet.jadpo",
        "use site/types { Welcome }\n\naction greet() -> Welcome {\n    return Welcome {\n        text: config.greeting\n        list_size: config.page_size\n    }\n}\n",
    ),
    (
        "domains/site/routes/hello.jadpo",
        "use site/actions { greet }\nuse site/types { Welcome }\n\nroutes Hello {\n    prefix: /hello\n\n    POST / {\n        authentication: RouteAuthentication.none\n        output: Welcome\n        run: greet()\n    }\n}\n",
    ),
];

fn project(name: &str) -> PathBuf {
    let root = folder(name);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let recorded = jadpo(&root, &["api", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

// --- jadpo config check ------------------------------------------------------

#[test]
fn check_lists_what_is_missing_and_what_the_fix_is() {
    let root = project("check-missing");
    let output = jadpo(&root, &["config", "check"]);
    assert_eq!(output.code, 1);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "Values
  greeting             missing  GREETING
  page_size            default  PAGE_SIZE
  business_zone        default  BUSINESS_ZONE
  session_signing_key  missing  SESSION_SIGNING_KEY

error: `GREETING` isn't set
  reason: `greeting` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set greeting` to store it in `.env.local`, or set `GREETING` in the environment (human)
  rule: configuration.value_missing
  help: docs/language/14-tooling.html#command-errors

error: `SESSION_SIGNING_KEY` isn't set
  reason: `session_signing_key` is required: it has no `default:`, so the app can't start without a value for it.
  next: Run `jadpo config set session_signing_key` to store it in `.env.local`, or set `SESSION_SIGNING_KEY` in the environment (human)
  rule: configuration.value_missing
  help: docs/language/14-tooling.html#command-errors

config incomplete: 2 problems, 4 settings

next: jadpo config set greeting  stores it in .env.local
"
    );
}

#[test]
fn check_reads_the_environment_over_the_local_file_and_prints_no_value() {
    let root = project("check-values");
    write(
        &root,
        ".env.local",
        &format!("GREETING=from the file\nSESSION_SIGNING_KEY={SECRET}\nPAGE_SIZE=500\n"),
    );
    let env = Environment::empty().with_var("PAGE_SIZE", "50");
    let output = jadpo_with(env, &root, &["config", "check"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    for text in [&output.stdout, &output.stderr] {
        assert!(
            !text.contains(SECRET) && !text.contains("from the file"),
            "{text}"
        );
    }
    assert!(
        output
            .stdout
            .contains("page_size            set      PAGE_SIZE")
    );
    assert!(
        output.stdout.ends_with("config ready: 4 settings\n"),
        "{}",
        output.stdout
    );
    // The file alone has a value out of range: `max`, never the value.
    let output = jadpo(&root, &["config", "check"]);
    assert_eq!(output.code, 1);
    assert!(
        output
            .stdout
            .contains("page_size            invalid  PAGE_SIZE")
    );
    assert!(output.stdout.contains("breaks `max`"), "{}", output.stdout);
    assert!(
        !output.stdout.contains("500") || output.stdout.contains("500 "),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains(SECRET));
}

#[test]
fn check_as_json_is_the_same_list() {
    let root = project("check-json");
    write(
        &root,
        ".env.local",
        &format!("GREETING=hello\nSESSION_SIGNING_KEY={SECRET}\n"),
    );
    let output = jadpo(&root, &["config", "check", "--json"]);
    assert_eq!(output.code, 0);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("json");
    assert_eq!(document["data"]["ready"], true);
    let settings = document["data"]["settings"].as_array().expect("settings");
    let states: Vec<(&str, &str, bool)> = settings
        .iter()
        .map(|s| {
            (
                s["name"].as_str().expect("name"),
                s["state"].as_str().expect("state"),
                s["secret"].as_bool().expect("secret"),
            )
        })
        .collect();
    assert_eq!(
        states,
        [
            ("greeting", "set", false),
            ("page_size", "default", false),
            ("business_zone", "default", false),
            ("session_signing_key", "set", true),
        ]
    );
    assert!(!output.stdout.contains(SECRET));
}

/// A database's address may not set TLS itself, since its connection in
/// `app.jadpo` decides, and the authority a connection names must be PEM
/// (area 5, "PostgreSQL over TLS"; chunk 8.1n).
#[test]
fn a_database_address_that_sets_tls_itself_and_an_authority_that_isnt_pem_are_refused() {
    let root = folder("check-tls");
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n            authority: config.database_authority\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    database_authority: Certificate\n    database_url: PostgresUrl\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let env = Environment::empty()
        .with_var(
            "DATABASE_URL",
            "postgresql://app:pw@db.example.test/app?sslmode=require",
        )
        .with_var("DATABASE_AUTHORITY", "not a certificate");
    let output = jadpo_with(env, &root, &["config", "check"]);
    assert_eq!(output.code, 1);
    assert!(
        output
            .stdout
            .contains("error: `DATABASE_URL` sets TLS itself")
            && output
                .stdout
                .contains("which the database's connection in `app.jadpo` decides"),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .contains("error: `DATABASE_AUTHORITY` isn't a valid value for `database_authority`"),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains("db.example.test") && !output.stdout.contains("pw@"));
}

#[test]
fn a_project_without_settings_says_so() {
    let root = folder("check-none");
    write(&root, "app.jadpo", "application {}\n");
    let output = jadpo(&root, &["config", "check"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    assert!(
        output.stdout.contains("No settings declared"),
        "{}",
        output.stdout
    );
}

#[test]
fn bad_arguments_are_cli_errors() {
    let root = project("arguments");
    for (args, needle) in [
        (vec!["config"], "needs an action"),
        (vec!["config", "chek"], "Did you mean `check`?"),
        (vec!["config", "set"], "needs a setting"),
    ] {
        let output = jadpo(&root, &args);
        assert_eq!(output.code, 2, "{args:?}");
        assert!(
            output.stderr.contains("rule: cli.config_arguments") && output.stderr.contains(needle),
            "{args:?}\n{}",
            output.stderr
        );
    }
    // `set` types its value at a prompt, so it needs a terminal; an unknown
    // setting is reported before that.
    let output = jadpo(&root, &["config", "set", "greetin"]);
    assert!(
        output.stderr.contains("Did you mean `greeting`?"),
        "{}",
        output.stderr
    );
    let output = jadpo(&root, &["config", "set", "greeting"]);
    assert_eq!(output.code, 2);
    assert!(
        output.stderr.contains("rule: cli.config_tty_required"),
        "{}",
        output.stderr
    );
}

// --- jadpo config set --------------------------------------------------------

fn set_args(root: &Path, field: &str) -> SetArgs {
    SetArgs {
        root: root.to_path_buf(),
        field: field.to_string(),
        choice: ColorChoice::Never,
    }
}

fn plain(screen: &Screen) -> String {
    screen.render(&Style::new(jadpo_term::Mode::Plain))
}

#[test]
fn a_value_is_checked_then_stored_in_the_local_file_only_its_owner_can_read() {
    let root = project("set");
    let screen = store(&set_args(&root, "greeting"), "hello there").expect("stored");
    assert_eq!(
        plain(&screen),
        "greeting configured locally
    variable   GREETING
    file       .env.local
    gitignore  added .env.local, so it isn't committed
"
    );
    assert_eq!(
        fs::read_to_string(root.join(".env.local")).expect("the file"),
        "GREETING=\"hello there\"\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).expect("the ignore file"),
        ".env.local\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(root.join(".env.local"))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    // Another value changes its line and keeps the rest, and the ignore
    // file isn't touched again.
    let screen = store(&set_args(&root, "session_signing_key"), SECRET).expect("stored");
    assert!(!plain(&screen).contains("gitignore"));
    assert!(!plain(&screen).contains(SECRET));
    assert_eq!(
        fs::read_to_string(root.join(".env.local")).expect("the file"),
        format!("GREETING=\"hello there\"\nSESSION_SIGNING_KEY={SECRET}\n")
    );
    store(&set_args(&root, "greeting"), "again").expect("changed");
    assert_eq!(
        fs::read_to_string(root.join(".env.local")).expect("the file"),
        format!("GREETING=again\nSESSION_SIGNING_KEY={SECRET}\n")
    );
    let checked = jadpo(&root, &["config", "check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
}

#[test]
fn a_value_that_breaks_the_type_is_refused_by_rule_and_nothing_is_written() {
    let root = project("set-invalid");
    for (field, value, rule) in [
        ("page_size", "many", "type"),
        ("page_size", "500", "max"),
        ("session_signing_key", "short", "format"),
        ("business_zone", "Nowhere/Land", "variant"),
    ] {
        let problem = store(&set_args(&root, field), value).expect_err("refused");
        let text = plain(&Screen::new().with(jadpo_term::Block::diagnostic(&problem, &())));
        assert!(text.contains("rule: configuration.value_invalid"), "{text}");
        assert!(text.contains(&format!("breaks `{rule}`")), "{text}");
        assert!(!text.contains(value), "{text}");
    }
    assert!(!root.join(".env.local").exists());
    store(&set_args(&root, "business_zone"), "America/New_York").expect("a zone");
}

#[test]
fn a_local_file_that_is_not_an_ordinary_file_is_not_written_through() {
    let root = project("set-unsafe");
    let elsewhere = root.join("elsewhere");
    fs::write(&elsewhere, "OTHER=1\n").expect("write");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&elsewhere, root.join(".env.local")).expect("a link");
        let problem = store(&set_args(&root, "greeting"), "x").expect_err("refused");
        let text = plain(&Screen::new().with(jadpo_term::Block::diagnostic(&problem, &())));
        assert!(
            text.contains("rule: configuration.local_unsafe_target"),
            "{text}"
        );
        assert_eq!(fs::read_to_string(&elsewhere).expect("read"), "OTHER=1\n");
    }
}

#[test]
fn a_local_file_that_does_not_read_is_reported_by_its_line() {
    let root = project("set-syntax");
    write(&root, ".env.local", "GREETING=ok\nthis is not a line\n");
    let problem = store(&set_args(&root, "greeting"), "x").expect_err("refused");
    let text = plain(&Screen::new().with(jadpo_term::Block::diagnostic(&problem, &())));
    assert!(text.contains("rule: configuration.local_syntax"), "{text}");
    assert!(text.contains("line 2"), "{text}");
    // `check` reads the same file the same way.
    let checked = jadpo(&root, &["config", "check"]);
    assert_eq!(checked.code, 1);
    assert!(
        checked.stderr.contains("configuration.local_syntax"),
        "{}",
        checked.stderr
    );
}

// --- jadpo dev ---------------------------------------------------------------------

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("buffer").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Buffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("buffer")).into_owned()
    }
}

struct Dev {
    out: Buffer,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<u8>>,
}

impl Dev {
    fn start(root: &Path, env: Environment) -> Dev {
        let out = Buffer::default();
        let stop = Arc::new(AtomicBool::new(false));
        let args = DevArgs {
            cwd: root.to_path_buf(),
            port: 0,
            url: None,
            no_run: false,
            json: true,
            choice: ColorChoice::Never,
            fake: Vec::new(),
            sandbox: Vec::new(),
        };
        let thread = {
            let mut out = out.clone();
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut err = Buffer::default();
                serve_to(&args, &env, &mut out, &mut err, &|| {
                    stop.load(Ordering::SeqCst)
                })
            })
        };
        Dev {
            out,
            stop,
            thread: Some(thread),
        }
    }

    /// Waits until the output has a line with `needle`; the port it listens
    /// on when that line says one.
    fn wait_for(&self, needle: &str) -> String {
        let started = Instant::now();
        loop {
            let text = self.out.text();
            if text.contains(needle) {
                return text;
            }
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "never saw {needle:?}:\n{text}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn port(&self) -> u16 {
        let text = self.wait_for("\"listening\"");
        let line = text
            .lines()
            .rev()
            .find(|line| line.contains("\"listening\""))
            .expect("a listening line");
        let document: serde_json::Value = serde_json::from_str(line).expect("json");
        document["data"]["url"]
            .as_str()
            .and_then(|url| url.rsplit(':').next())
            .and_then(|port| port.parse().ok())
            .expect("a port")
    }
}

impl Drop for Dev {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn post(port: u16, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("timeout");
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\ncontent-length: 0\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).expect("sends");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("reads");
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .expect("a status");
    (status, body.to_string())
}

#[test]
fn the_app_refuses_to_start_without_its_required_values_and_starts_once_they_are_set() {
    let root = project("dev");
    let dev = Dev::start(&root, Environment::empty());
    // Nothing listens: it says which settings and variables, once, and
    // waits for a save that fixes them.
    let text = dev.wait_for("configuration.value_missing");
    assert!(text.contains("configuration.value_missing"), "{text}");
    assert!(
        text.contains("GREETING") && text.contains("SESSION_SIGNING_KEY"),
        "{text}"
    );
    assert!(!text.contains("\"listening\""), "{text}");
    // A value that is there but invalid is not accepted either, and the
    // value is never in the output.
    write(
        &root,
        ".env.local",
        &format!("GREETING=hi there\nSESSION_SIGNING_KEY={SECRET}\nPAGE_SIZE=lots\n"),
    );
    let text = dev.wait_for("configuration.value_invalid");
    assert!(!text.contains("lots") && !text.contains(SECRET), "{text}");
    assert!(!text.contains("\"listening\""), "{text}");
    // Fixing the file starts it; the action reads its settings.
    write(
        &root,
        ".env.local",
        &format!("GREETING=hi there\nSESSION_SIGNING_KEY={SECRET}\n"),
    );
    let port = dev.port();
    let (status, body) = post(port, "/hello");
    assert_eq!(status, 200, "{body}");
    let body: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(body["text"], "hi there");
    assert_eq!(body["list_size"], 25);
    // A change to the file is a reload; one that is invalid keeps the last
    // good version serving.
    write(
        &root,
        ".env.local",
        &format!("GREETING=hi there\nSESSION_SIGNING_KEY={SECRET}\nPAGE_SIZE=500\n"),
    );
    let text = dev.wait_for("\"serving\":true");
    assert!(text.contains("PAGE_SIZE"), "{text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&post(port, "/hello").1).expect("json")["list_size"],
        25
    );
    write(
        &root,
        ".env.local",
        &format!("GREETING=changed\nSESSION_SIGNING_KEY={SECRET}\nPAGE_SIZE=40\n"),
    );
    let started = Instant::now();
    loop {
        let body: serde_json::Value = serde_json::from_str(&post(port, "/hello").1).expect("json");
        if body["text"] == "changed" {
            assert_eq!(body["list_size"], 40);
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "never reloaded"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let text = dev.out.text();
    assert!(!text.contains(SECRET), "{text}");
}

#[test]
fn the_process_environment_gives_the_values_too() {
    let root = project("dev-environment");
    let env = Environment::empty()
        .with_var("GREETING", "from the environment")
        .with_var("SESSION_SIGNING_KEY", SECRET);
    let dev = Dev::start(&root, env);
    let (status, body) = post(dev.port(), "/hello");
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("from the environment"), "{body}");
}

// --- jadpo test --------------------------------------------------------------------

const GREET_TESTS: &str = r#"use site/types { Welcome }

test "the greeting is the test value" {
    var welcome: Welcome = greet()
    assert welcome.text == "hello from the tests"
    assert welcome.list_size == 25
}

test "a test gives a setting its own value" {
    config: {
        greeting: "planning says hello"
        page_size: 7
    }
    var welcome: Welcome = greet()
    assert welcome.text == "planning says hello"
    assert welcome.list_size == 7
}
"#;

#[test]
fn tests_read_test_values_defaults_and_their_own_config() {
    let root = project("tests");
    write(&root, "domains/site/actions/greet.test.jadpo", GREET_TESTS);
    let formatted = jadpo(&root, &["fmt"]);
    assert_eq!(
        formatted.code, 0,
        "{}{}",
        formatted.stdout, formatted.stderr
    );
    let output = jadpo(&root, &["test"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    assert!(output.stdout.contains("2 passed"), "{}", output.stdout);
}

#[test]
fn a_setting_tested_code_reads_with_no_test_value_is_a_compile_error() {
    let root = project("tests-missing");
    write(
        &root,
        "config.jadpo",
        &FILES[1]
            .1
            .replace("        test: \"hello from the tests\"\n", ""),
    );
    write(
        &root,
        "domains/site/actions/greet.test.jadpo",
        "use site/types { Welcome }\n\ntest \"it greets\" {\n    var welcome: Welcome = greet()\n    assert welcome.list_size == 25\n}\n",
    );
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    assert!(
        output
            .stdout
            .contains("rule: configuration.test_value_missing"),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .contains("`config.greeting` has no value in tests")
    );
    // Giving the test its own value is enough.
    write(
        &root,
        "domains/site/actions/greet.test.jadpo",
        "use site/types { Welcome }\n\ntest \"it greets\" {\n    config: {\n        greeting: \"hi\"\n    }\n    var welcome: Welcome = greet()\n    assert welcome.list_size == 25\n}\n",
    );
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
}

#[test]
fn a_secret_is_made_for_a_run_and_never_written_in_a_test() {
    let root = project("tests-secret");
    write(
        &root,
        "domains/site/actions/greet.test.jadpo",
        "use site/types { Welcome }\n\ntest \"it greets\" {\n    config: {\n        session_signing_key: \"0123456789abcdef0123456789abcdef0123\"\n    }\n    var welcome: Welcome = greet()\n    assert welcome.list_size == 25\n}\n",
    );
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 1);
    assert!(
        output.stdout.contains("rule: configuration.secret_default"),
        "{}",
        output.stdout
    );
}

// --- a connection's address ---------------------------------------------------------

/// A connection the test server gives, as the URL a `PostgresUrl` holds: the
/// disposable server answers with `host=… dbname=…` pairs, an external one
/// with a URL.
fn as_url(connection: &str) -> String {
    if connection.starts_with("postgres") {
        return connection.to_string();
    }
    let part = |key: &str| {
        connection
            .split_whitespace()
            .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
            .map(|value| value.trim_matches('\'').to_string())
            .unwrap_or_default()
    };
    format!(
        "postgresql://{}@/{}?host={}&port={}",
        part("user"),
        part("dbname"),
        part("host"),
        part("port")
    )
}

#[test]
fn a_connection_opens_at_the_address_its_setting_holds() {
    if !jadpo_test::live() {
        return;
    }
    let root = folder("connection");
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    database_url: PostgresUrl\n}\n",
    );
    write(
        &root,
        "domains/planning/entities/todo.jadpo",
        "entity Todo {\n    id: Uuid\n    title: Text\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    // No address: the command says where it comes from.
    let output = jadpo(&root, &["migrate", "apply"]);
    assert_eq!(output.code, 2, "{}{}", output.stdout, output.stderr);
    assert!(
        output.stderr.contains("jadpo config set"),
        "{}",
        output.stderr
    );
    // The address in the environment is used, and never printed.
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = as_url(
        &server
            .create_database("config_connection")
            .expect("a database"),
    );
    let env = Environment::empty().with_var("DATABASE_URL", &url);
    let output = jadpo_with(env.clone(), &root, &["migrate", "apply"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    assert!(
        output.stdout.contains("Applied 1 migration to `main`"),
        "{}",
        output.stdout
    );
    assert!(!output.stdout.contains(&url) && !output.stderr.contains(&url));
    let output = jadpo_with(env, &root, &["migrate", "status"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    // Not an address at all.
    let env = Environment::empty().with_var("DATABASE_URL", "https://example.test");
    let output = jadpo_with(env, &root, &["migrate", "apply"]);
    assert_eq!(output.code, 1, "{}{}", output.stdout, output.stderr);
    assert!(
        output.stderr.contains("configuration.value_invalid"),
        "{}",
        output.stderr
    );
    assert!(!output.stderr.contains("https://example.test"));
}

// --- jadpo config move -------------------------------------------------------

/// Area 10's example: a library constant in a domain, read by an action.
const ACCOUNTS: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/accounts/types/login_attempts.jadpo",
        "type LoginAttempts = Int {\n    min: 1\n    max: 100\n}\n",
    ),
    (
        "domains/accounts/types/account_locked.jadpo",
        "type AccountLocked = Bool {}\n",
    ),
    (
        "domains/accounts/libraries/security.jadpo",
        "use accounts/types { LoginAttempts }\n\nlibrary security {\n    /// Failed sign-ins before an account is locked.\n    constant max_login_attempts: LoginAttempts = 5\n}\n",
    ),
    (
        "domains/accounts/actions/sign_in.jadpo",
        "use accounts/types { AccountLocked, LoginAttempts }\nuse accounts/libraries { security }\n\naction sign_in(failed_attempts: LoginAttempts) -> AccountLocked {\n    return AccountLocked(failed_attempts >= security.max_login_attempts)\n}\n",
    ),
];

fn accounts(name: &str) -> PathBuf {
    let root = folder(name);
    for (path, text) in ACCOUNTS {
        write(&root, path, text);
    }
    root
}

#[test]
fn move_makes_the_constant_a_setting_and_lists_what_changed() {
    let root = accounts("move");
    let output = jadpo(&root, &["config", "move", "security.max_login_attempts"]);
    assert_eq!(output.stderr, "");
    assert_eq!(
        output.stdout,
        "  config.jadpo
    + max_login_attempts  setting, default 5

  domains/accounts/libraries/security.jadpo
    - max_login_attempts  constant
    - security            library, nothing left in it
    - security.jadpo      file, nothing left in it

  domains/accounts/actions/sign_in.jadpo
    ~ line 5              reads config.max_login_attempts

Made security.max_login_attempts a setting
    setting   config.max_login_attempts
    variable  MAX_LOGIN_ATTEMPTS
"
    );
    assert_eq!(output.code, 0);
    assert_eq!(
        fs::read_to_string(root.join("config.jadpo")).expect("config"),
        "use accounts/types { LoginAttempts }\n\nconfig {\n    /// Failed sign-ins before an account is locked.\n    max_login_attempts: LoginAttempts {\n        default: 5\n    }\n}\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("domains/accounts/actions/sign_in.jadpo")).expect("action"),
        "use accounts/types { AccountLocked, LoginAttempts }\n\naction sign_in(failed_attempts: LoginAttempts) -> AccountLocked {\n    return AccountLocked(failed_attempts >= config.max_login_attempts)\n}\n"
    );
    assert!(!root.join("domains/accounts/libraries").exists());
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    assert_eq!(jadpo(&root, &["fmt", "--check"]).code, 0);
}

#[test]
fn move_as_json_names_the_setting_its_variable_and_each_read() {
    let root = accounts("move-json");
    let output = jadpo(
        &root,
        &["config", "move", "security.max_login_attempts", "--json"],
    );
    assert_eq!(output.code, 0);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let data = &document["data"];
    assert_eq!(data["moved"], true);
    assert_eq!(data["constant"], "security.max_login_attempts");
    assert_eq!(data["setting"], "max_login_attempts");
    assert_eq!(data["variable"], "MAX_LOGIN_ATTEMPTS");
    assert_eq!(data["default"], "5");
    assert_eq!(
        data["reads"],
        serde_json::json!([{"path": "domains/accounts/actions/sign_in.jadpo", "line": 5}])
    );
    let files: Vec<(&str, &str)> = data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap_or_default(),
                f["change"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        files,
        [
            ("config.jadpo", "added"),
            ("domains/accounts/libraries/security.jadpo", "removed"),
            ("domains/accounts/actions/sign_in.jadpo", "changed"),
        ]
    );
}

#[test]
fn move_refuses_a_name_a_setting_has_and_changes_nothing() {
    let root = accounts("move-taken");
    let config = "use accounts/types { LoginAttempts }\n\nconfig {\n    max_login_attempts: LoginAttempts {\n        default: 3\n    }\n}\n";
    write(&root, "config.jadpo", config);
    write(
        &root,
        "domains/accounts/actions/sign_in.jadpo",
        "use accounts/types { AccountLocked, LoginAttempts }\nuse accounts/libraries { security }\n\naction sign_in(failed_attempts: LoginAttempts) -> AccountLocked {\n    return AccountLocked(failed_attempts >= security.max_login_attempts or failed_attempts >= config.max_login_attempts)\n}\n",
    );
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    let output = jadpo(&root, &["config", "move", "security.max_login_attempts"]);
    assert_eq!(
        output.stdout,
        "error: A setting is already called `max_login_attempts`
 --> domains/accounts/libraries/security.jadpo:5:14
  |
5 |     constant max_login_attempts: LoginAttempts = 5
  |              ^^^^^^^^^^^^^^^^^^ a setting has this name
  reason: `config { }` declares `max_login_attempts` already, so `security.max_login_attempts` can't become `config.max_login_attempts` without two settings of one name.
  next: Rename the constant or the setting `max_login_attempts`, then run `jadpo config move` again (agent)
  rule: configuration.move_name_taken
  help: docs/language/14-tooling.html#command-errors

config move refused: nothing changed
"
    );
    assert_eq!(output.code, 1);
    assert_eq!(
        fs::read_to_string(root.join("config.jadpo")).expect("config"),
        config
    );
    assert!(
        root.join("domains/accounts/libraries/security.jadpo")
            .exists()
    );
}

#[test]
fn move_needs_a_constant_that_exists() {
    let root = accounts("move-unknown");
    let output = jadpo(&root, &["config", "move", "security.max_login_attempt"]);
    assert_eq!(output.code, 2);
    assert!(
        output
            .stderr
            .contains("`security.max_login_attempt` isn't a constant"),
        "{}",
        output.stderr
    );
    assert!(output.stderr.contains(
        "next: Use `security.max_login_attempts` instead of `security.max_login_attempt`"
    ));
    let missing = jadpo(&root, &["config", "move"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing
            .stderr
            .contains("`jadpo config move` needs a constant"),
        "{}",
        missing.stderr
    );
}

#[test]
fn move_stops_on_a_project_that_does_not_check() {
    let root = accounts("move-broken");
    write(
        &root,
        "domains/accounts/types/account_locked.jadpo",
        "type AccountLocked = Bool {\n    max: 3\n}\n",
    );
    let output = jadpo(&root, &["config", "move", "security.max_login_attempts"]);
    assert_eq!(output.code, 1);
    assert!(
        output.stdout.ends_with("config stopped: 1 error\n"),
        "{}",
        output.stdout
    );
    assert!(
        root.join("domains/accounts/libraries/security.jadpo")
            .exists()
    );
}

/// The files of `project`'s ```jadpo check blocks on a language page.
fn page_project(page: &str, project: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        let Some(attrs) = line.strip_prefix("```jadpo check ") else {
            continue;
        };
        if !attrs
            .split_whitespace()
            .any(|attr| attr == format!("project={project}"))
        {
            continue;
        }
        let file = attrs
            .split_whitespace()
            .find_map(|attr| attr.strip_prefix("file="))
            .unwrap_or("main.jadpo")
            .to_string();
        let mut text = String::new();
        for line in lines.by_ref() {
            if line.starts_with("```") {
                break;
            }
            text.push_str(line);
            text.push('\n');
        }
        out.push((file, text));
    }
    out
}

/// Area 10's example is real: `jadpo config move` turns the page's
/// "before" project into its "after" project, printing what the page
/// shows.
#[test]
fn move_does_what_area_10_shows() {
    let page = include_str!("../../../docs/language/10-configuration-and-time.md");
    let root = folder("move-page");
    let before = page_project(page, "move-before");
    assert!(!before.is_empty());
    for (path, text) in &before {
        write(&root, path, text);
    }
    let command = "$ jadpo config move security.max_login_attempts\n";
    let start = page.find(command).expect("the transcript") + command.len();
    let shown = &page[start..start + page[start..].find("```").expect("its end")];
    let output = jadpo(&root, &["config", "move", "security.max_login_attempts"]);
    assert_eq!(output.code, 0, "{}", output.stdout);
    assert_eq!(output.stdout, shown);
    let after = page_project(page, "move-after");
    let mut expected: Vec<(String, String)> = after;
    expected.sort();
    let mut actual: Vec<(String, String)> = jadpo_db::files_under(&root, true)
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(root.join(&path)).expect("read");
            (path, text)
        })
        .collect();
    actual.sort();
    assert_eq!(actual, expected);
}

/// A project with `Pence` and one action, `main` in `file`, in the shop
/// domain.
fn shop(name: &str, file: &str, main: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/shop/types/pence.jadpo",
        "type Pence = Int {\n    min: 0\n}\n",
    );
    write(&root, &format!("domains/shop/actions/{file}"), main);
    root
}

#[test]
fn move_refuses_every_reason_at_once_and_changes_nothing() {
    let main = "use shop/types { Pence }\n\naction first_fee() -> Pence {\n    for fee in first_fees {\n        return fee\n    }\n    return 0\n}\n\nconstant fees: List<Pence> = [250, 500]\ndecline constant.similar_exists: \"The first fees, which may differ\"\nconstant first_fees: List<Pence> = fees\n";
    let root = shop("move-refused", "first_fee.jadpo", main);
    assert_eq!(
        jadpo(&root, &["check"]).code,
        0,
        "{}",
        jadpo(&root, &["check"]).stdout
    );
    let output = jadpo(&root, &["config", "move", "fees"]);
    assert_eq!(output.code, 1);
    let rules: Vec<&str> = output
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  rule: "))
        .collect();
    assert_eq!(
        rules,
        [
            "configuration.move_type_unsupported",
            "configuration.move_read_unsupported",
        ]
    );
    assert!(
        output
            .stdout
            .ends_with("\nconfig move refused: nothing changed\n")
    );
    assert_eq!(
        fs::read_to_string(root.join("domains/shop/actions/first_fee.jadpo")).expect("action"),
        main
    );
    assert!(!root.join("config.jadpo").exists());
}

#[test]
fn move_fills_the_setting_whose_default_names_the_constant() {
    let root = shop(
        "move-fill",
        "page.jadpo",
        "use shop/types { Pence }\n\naction page() -> Pence {\n    return Pence(config.page_size + largest_page)\n}\n\nconstant largest_page: Pence = 100\n",
    );
    write(
        &root,
        "config.jadpo",
        "use shop/types { Pence }\n\nconfig {\n    page_size: Pence {\n        default: largest_page\n    }\n}\n",
    );
    let check = jadpo(&root, &["check"]);
    assert_eq!(check.code, 1);
    assert!(
        check
            .stdout
            .contains("next: Run `jadpo config move largest_page`"),
        "{}",
        check.stdout
    );
    let output = jadpo(&root, &["config", "move", "largest_page"]);
    assert_eq!(
        output.stdout,
        "  config.jadpo
    ~ page_size     default 100, was largest_page

  domains/shop/actions/page.jadpo
    - largest_page  constant
    ~ line 4        reads config.page_size

Made largest_page a setting
    setting   config.page_size
    variable  PAGE_SIZE
"
    );
    assert_eq!(output.code, 0);
    assert_eq!(
        fs::read_to_string(root.join("config.jadpo")).expect("config"),
        "use shop/types { Pence }\n\nconfig {\n    page_size: Pence {\n        default: 100\n    }\n}\n"
    );
    let after = jadpo(&root, &["check"]);
    assert_eq!(after.code, 0, "{}", after.stdout);
}

#[test]
fn every_invalid_connection_address_is_reported_without_its_credentials() {
    let root = folder("connection-structure");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "config.jadpo",
        "config { database_url: PostgresUrl\n issuer: Url\n }\n",
    );
    let output = jadpo_with(
        Environment::empty()
            .with_var("DATABASE_URL", "postgres://user:secret-password@")
            .with_var("ISSUER", "not-a-url"),
        &root,
        &["config", "check", "--json"],
    );
    assert_ne!(output.code, 0);
    let text = format!("{}{}", output.stdout, output.stderr);
    let json: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON report");
    assert_eq!(
        json["diagnostics"].as_array().expect("diagnostics").len(),
        2,
        "{text}"
    );
    assert!(!text.contains("secret-password"), "{text}");
}
