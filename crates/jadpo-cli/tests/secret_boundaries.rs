//! Area 19, "Where a secret can go", run (decided 2026-11-08): a `Password`
//! a person signed off reaches its provider as itself, as any other secret
//! given to an operation does, and nowhere else: not the development call
//! log, not a step's fingerprint; a request whose secret is refused is
//! answered without its value; and a person signs off the decline.
//!
//! Each provider test compiles one project whose connection reaches a
//! provider started in the test on this machine. Nothing reaches a
//! network.

use std::path::Path;
use std::rc::Rc;

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use jadpo_value::Value;

use super::http::{Dev, write};
use crate::support::fake_provider::FakeProvider;

const HOME: &str = "shared/providers/acme_directory/v1";

/// The text a test sends as a secret, which nothing but its provider
/// should see.
const SPOKEN: &str = "correct horse battery staple";

fn files_of(sandbox: &str, method: &str, path: &str) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{HOME}/{path}"), text.to_string());
    let app = |path: &str, text: &str| (path.to_string(), text.to_string());
    vec![
        app(
            "app.jadpo",
            "application {\n    connections: {\n        directory: {\n            package: AcmeDirectory.v1\n            domains: [accounts]\n            credential: config.directory_key\n            environment: config.directory_environment\n        }\n    }\n}\n",
        ),
        app(
            "config.jadpo",
            "config {\n    directory_key: Text {\n        secret: true\n    }\n    directory_environment: ProviderEnvironment\n}\n",
        ),
        package(
            "package.jadpo",
            "/// Acme's directory.\npackage AcmeDirectory.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            format!("{HOME}/provider.jadpo"),
            format!(
                "/// How Acme's directory is reached.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://directory.acme.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n}}\n"
            ),
        ),
        package(
            "types/bind_request.jadpo",
            "/// A sign-in to check against the directory.\npublic type BindRequest = Object {\n    user: Text\n    password: Password\n    key: ApiKey?\n}\n",
        ),
        package(
            "types/bind_result.jadpo",
            "/// What the directory says of it.\npublic type BindResult = Object {\n    bound: Bool\n}\n",
        ),
        (
            format!("{HOME}/resources/binds.jadpo"),
            format!(
                "public resource binds {{\n    create: {{\n        method: HttpMethod.{method}\n        path: \"{path}\"\n        input: BindRequest\n        output: BindResult\n        changes: false\n        source: PackageSource.written\n    }}\n}}\n"
            ),
        ),
        app(
            "domains/accounts/actions/bind.jadpo",
            "use app { directory }\nuse shared/providers/acme_directory\n\n/// Checks a sign-in against the directory.\ndecline type.password_sent_to_provider: \"Old system, during the migration off it\"\naction bind(request: AcmeDirectory.BindRequest) -> AcmeDirectory.BindResult {\n    return directory.binds.create(request)\n}\n",
        ),
    ]
}

/// The project, checked, with the directory's sandbox at `sandbox`.
fn program(root: &Path, files: Vec<(String, String)>) -> Program {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:#?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "DIRECTORY_KEY" => Some("dk_test_123".into()),
            "DIRECTORY_ENVIRONMENT" => Some("sandbox".into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    Program::new(prepared).with_config(&values)
}

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "jadpo-secret-boundaries-{name}-{}",
        std::process::id()
    ))
}

fn request() -> Value {
    Value::record(
        "AcmeDirectory.BindRequest",
        vec![
            ("user".into(), Value::text("ada")),
            ("password".into(), Value::secret("Password", SPOKEN)),
            (
                "key".into(),
                Value::secret("ApiKey", "example_key_0123456789"),
            ),
        ],
    )
}

fn run(runtime: &Runtime, program: &Program) -> Outcome {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(MemoryHost::new()).with("directory", driver);
    let mut ctx = Ctx::of(
        program,
        &host,
        Env {
            seed: Some(7),
            clock: Clock::TEST,
        },
    );
    ctx.set_call_key("user:ada", "client-key-1");
    let id = program.plan().find("bind").expect("the action");
    runtime.block_on(program.run(&mut ctx, id, vec![request()]))
}

/// A signed-off password goes out as itself, or it couldn't check
/// anything; so does any other secret given to an operation.
#[test]
fn a_signed_off_password_reaches_the_provider_as_itself() {
    let provider = FakeProvider::start();
    let program = program(&root("body"), files_of(&provider.url, "post", "/v1/binds"));
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"bound": true}"#);
    let outcome = run(&runtime, &program);
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].json(),
        serde_json::json!({
            "user": "ada",
            "password": SPOKEN,
            "key": "example_key_0123456789",
        })
    );
}

/// In a path or a query string, what it says goes through the same
/// writer.
#[test]
fn a_signed_off_password_goes_into_the_url_it_was_declined_for() {
    let provider = FakeProvider::start();
    let program = program(
        &root("query"),
        files_of(&provider.url, "get", "/v1/binds/{user}"),
    );
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"bound": true}"#);
    let outcome = run(&runtime, &program);
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].path.contains("correct%20horse%20battery%20staple"),
        "{}",
        sent[0].path
    );
    assert!(!sent[0].path.contains("redacted"), "{}", sent[0].path);
}

/// `jadpo dev`'s call log shows what is sent and never a secret: it is
/// kept on the developer's disk and shown back by `jadpo calls`.
#[test]
fn the_call_log_never_shows_a_secret_a_provider_was_sent() {
    let dir = super::http::tempdir::Dir::new("secret-call-log");
    let root = dir.path();
    for (path, text) in files_of("https://sandbox.acme.example", "post", "/v1/binds") {
        write(root, &path, &text);
    }
    write(
        root,
        "config.jadpo",
        "config {\n    directory_key: Text {\n        secret: true\n    }\n    directory_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n}\n",
    );
    write(
        root,
        "policy.jadpo",
        "use accounts\n\npolicy {\n    actions: {\n        accounts.answer_bind: [Access.public]\n    }\n}\n",
    );
    write(
        root,
        "domains/accounts/actions/answer_bind.jadpo",
        "use accounts/actions { bind }\nuse accounts/types { BindForm, BindAnswer }\nuse shared/providers/acme_directory\n\n/// What a request's sign-in is told.\naction answer_bind(form: BindForm) -> BindAnswer {\n    var result: AcmeDirectory.BindResult = bind(AcmeDirectory.BindRequest {\n        user: form.user\n        password: form.password\n        key: none\n    })\n    return BindAnswer {\n        bound: result.bound\n    }\n}\n",
    );
    write(
        root,
        "domains/accounts/types/bind_form.jadpo",
        "/// What the form sends.\ntype BindForm = Input {\n    user: Text\n    password: Password\n}\n",
    );
    write(
        root,
        "domains/accounts/types/bind_answer.jadpo",
        "/// What it is told.\ntype BindAnswer = Output {\n    bound: Bool\n}\n",
    );
    write(
        root,
        "domains/accounts/routes/binds.jadpo",
        "use accounts/actions { answer_bind }\nuse accounts/types { BindForm, BindAnswer }\n\nroutes Binds {\n    prefix: /binds\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: BindForm\n        output: BindAnswer\n        run: answer_bind(input)\n    }\n}\n",
    );
    let dev = Dev::start_without_database(root);
    let reply = dev.send(
        "POST",
        "/binds",
        &[],
        &format!(r#"{{"user": "ada", "password": "{SPOKEN}"}}"#),
    );
    assert_eq!(reply.status, 200, "{}", reply.text());
    drop(dev);
    for args in [&["calls", "show", "call_1"][..], &["calls", "--json"]] {
        let shown = super::http::jadpo(root, args);
        assert!(
            !shown.stdout.contains("correct horse"),
            "{args:?}: {}",
            shown.stdout
        );
    }
    let shown = super::http::jadpo(root, &["calls", "show", "call_1"]);
    assert!(
        shown.stdout.contains("<redacted Password>"),
        "{}",
        shown.stdout
    );
}

/// A password that breaks a rule is answered by naming the rule, and the
/// value doesn't come back.
#[test]
fn a_refused_secret_isnt_echoed() {
    let dir = super::http::tempdir::Dir::new("secret-refused");
    let root = dir.path();
    write(root, "app.jadpo", "application {}\n");
    write(
        root,
        "domains/accounts/types/sign_up_form.jadpo",
        "/// What the form sends.\ntype SignUpForm = Input {\n    password: Password\n    key: ApiKey\n}\n",
    );
    write(
        root,
        "domains/accounts/types/answer.jadpo",
        "/// What it is told.\ntype Answer = Output {\n    ok: Bool\n}\n",
    );
    write(
        root,
        "domains/accounts/actions/sign_up.jadpo",
        "use accounts/types { SignUpForm, Answer }\n\n/// Signs up.\naction sign_up(form: SignUpForm) -> Answer {\n    return Answer {\n        ok: form.password == \"anything\"\n    }\n}\n",
    );
    write(
        root,
        "policy.jadpo",
        "use accounts\n\npolicy {\n    actions: {\n        accounts.sign_up: [Access.public]\n    }\n}\n",
    );
    write(
        root,
        "domains/accounts/routes/sign_ups.jadpo",
        "use accounts/actions { sign_up }\nuse accounts/types { SignUpForm, Answer }\n\nroutes SignUps {\n    prefix: /sign_ups\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: SignUpForm\n        output: Answer\n        run: sign_up(input)\n    }\n}\n",
    );
    let dev = Dev::start_without_database(root);
    // Too short, and a key with a character a key can't hold.
    let reply = dev.send(
        "POST",
        "/sign_ups",
        &[],
        r#"{"password": "short7!", "key": "bad key here"}"#,
    );
    assert_eq!(reply.status, 422, "{}", reply.text());
    assert!(
        reply.text().contains("\"rule\":\"format\""),
        "{}",
        reply.text()
    );
    for rejected in ["short7!", "bad key here"] {
        assert!(!reply.text().contains(rejected), "{}", reply.text());
    }
    // Sent as the wrong kind of JSON, and as not JSON at all.
    let reply = dev.send(
        "POST",
        "/sign_ups",
        &[],
        r#"{"password": 12345678, "key": ["example_key_01234"]}"#,
    );
    assert_eq!(reply.status, 422, "{}", reply.text());
    assert!(!reply.text().contains("12345678"), "{}", reply.text());
    assert!(
        !reply.text().contains("example_key_01234"),
        "{}",
        reply.text()
    );
    let reply = dev.send(
        "POST",
        "/sign_ups",
        &[],
        r#"{"password": "correct horse battery staple",}"#,
    );
    assert_eq!(reply.status, 400, "{}", reply.text());
    assert!(!reply.text().contains("correct horse"), "{}", reply.text());
    let log = dev.log();
    drop(dev);
    for rejected in ["short7!", "bad key here", "correct horse"] {
        assert!(!log.contains(rejected), "{log}");
    }
}

// --- the differential run ------------------------------------------------------

/// A sign-up form that checks its password against the old directory,
/// signed off, for `compiled.rs`: the password goes out through the same
/// call on both executors.
pub fn files() -> Vec<(String, String)> {
    let mut out = files_of("http://directory.example", "post", "/v1/binds");
    out.retain(|(path, _)| path != "domains/accounts/actions/bind.jadpo");
    out.extend(
        [
            (
                "domains/accounts/actions/bind.jadpo",
                "use app { directory }\nuse shared/providers/acme_directory\n\n/// Checks a sign-in against the directory.\ndecline type.password_sent_to_provider: \"Old system, during the migration off it\"\naction bind(request: AcmeDirectory.BindRequest) -> AcmeDirectory.BindResult {\n    return directory.binds.create(request)\n}\n",
            ),
            (
                "domains/accounts/actions/bind.test.jadpo",
                "use app { directory }\nuse shared/providers/acme_directory\n\ntest \"a signed-off password goes out as the fake is asked\" {\n    fakes: {\n        directory.binds.create: [AcmeDirectory.BindResult { bound: true }]\n    }\n    var answer: AcmeDirectory.BindResult = bind(AcmeDirectory.BindRequest {\n        user: \"ada\"\n        password: \"correct horse battery staple\"\n        key: none\n    })\n    assert answer.bound\n}\n",
            ),
        ]
        .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    out
}

// --- a person signs it off ------------------------------------------------------

use crate::acceptances::{acceptance_files, edit, git, jadpo, library, write as write_file};

/// The problems `jadpo check` reports, as rule and message.
fn checked(root: &Path) -> Vec<String> {
    let output = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", output.stdout, output.stderr));
    document["diagnostics"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|d| {
            format!(
                "{}: {}",
                d["rule"].as_str().unwrap_or(""),
                d["message"].as_str().unwrap_or("")
            )
        })
        .collect()
}

#[test]
fn declining_the_rule_needs_a_person_to_accept_it() {
    let root = library("secret-accept");
    edit(
        &root,
        "app.jadpo",
        "            url: config.database_url\n        }\n",
        "            url: config.database_url\n        }\n        directory: {\n            package: AcmeDirectory.v1\n            domains: [lending]\n            credential: config.directory_key\n            environment: config.directory_environment\n        }\n",
    );
    edit(
        &root,
        "config.jadpo",
        "    database_url: PostgresUrl\n",
        "    database_url: PostgresUrl\n    directory_key: Text {\n        secret: true\n    }\n    directory_environment: ProviderEnvironment\n",
    );
    for (path, text) in files_of("https://sandbox.acme.example", "post", "/v1/binds")
        .iter()
        .filter(|(path, _)| path.starts_with("shared/"))
    {
        write_file(&root, path, text);
    }
    write_file(
        &root,
        "domains/lending/actions/bind_old_directory.jadpo",
        "use app { directory }\nuse shared/providers/acme_directory\n\n/// Checks a member's password against the old directory.\ndecline type.password_sent_to_provider: \"Old system, during the migration off it\"\naction bind_old_directory(request: AcmeDirectory.BindRequest) -> AcmeDirectory.BindResult {\n    return directory.binds.create(request)\n}\n",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    assert_eq!(
        checked(&root)
            .into_iter()
            .filter(|problem| problem.contains("declining"))
            .collect::<Vec<_>>(),
        [
            "acceptance.missing: declining a security rule needs a person: lending.bind_old_directory"
        ]
    );
    // The app's access record lists the decline.
    let access = std::fs::read_to_string(root.join("access.jadpo_record")).expect("the record");
    assert!(
        access.contains("decline type.password_sent_to_provider lending.bind_old_directory"),
        "{access}"
    );
    // An agent drafts it, a person says why, and only then it passes.
    assert_eq!(
        jadpo(&root, &["acceptance", "new", "old_directory_bind"]).code,
        0
    );
    let files = acceptance_files(&root);
    assert_eq!(files.len(), 1, "{files:?}");
    let text = std::fs::read_to_string(&files[0]).expect("read");
    assert!(
        text.contains(
            "lending.bind_old_directory: PolicyChange.password_sent_to_provider_declined"
        ),
        "{text}"
    );
    std::fs::write(
        &files[0],
        format!("/// The migration needs it until March.\n{text}"),
    )
    .expect("written");
    assert_eq!(checked(&root), Vec::<String>::new());
    git(&root, &["add", "-A"]);
}

// --- the publishing bar ----------------------------------------------------------

/// A package's own repository: a provider that takes a `Password`, its own
/// public action sending one on, and a `sample/` app whose recipe calls
/// the operation as `recipe` says.
fn repository(name: &str, recipe: &str) -> super::http::tempdir::Dir {
    let dir = super::http::tempdir::Dir::new(name);
    let root = dir.path();
    write(
        root,
        "package.jadpo",
        "/// Acme's directory.\npackage AcmeDirectory.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        root,
        "provider.jadpo",
        "/// How Acme's directory is reached.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://directory.acme.example\"\n        sandbox: \"https://sandbox.acme.example\"\n    }\n    credential: CredentialScheme.bearer\n    reviewed: [BindRequest, BindResult]\n}\n",
    );
    write(
        root,
        "types/bind_request.jadpo",
        "/// A sign-in to check against the directory.\npublic type BindRequest = Object {\n    user: Text\n    password: Password\n}\n",
    );
    write(
        root,
        "types/bind_result.jadpo",
        "/// What the directory says of it.\npublic type BindResult = Object {\n    bound: Bool\n}\n",
    );
    write(
        root,
        "resources/binds.jadpo",
        "public resource binds {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/binds\"\n        input: BindRequest\n        output: BindResult\n        source: PackageSource.written\n    }\n}\n",
    );
    write(
        root,
        "actions/bind.jadpo",
        "/// Binds, through the package's own operation.\npublic action bind(request: BindRequest) -> BindResult {\n    return binds.create(request)\n}\n",
    );
    write(
        root,
        "sample/app.jadpo",
        "application {\n    connections: {\n        directory: {\n            package: AcmeDirectory.v1\n            domains: [accounts]\n            credential: config.directory_key\n            environment: config.directory_environment\n        }\n    }\n}\n",
    );
    write(
        root,
        "sample/config.jadpo",
        "config {\n    directory_environment: ProviderEnvironment\n    directory_key: Text {\n        secret: true\n    }\n}\n",
    );
    write(
        root,
        "sample/domains/accounts/types/note.jadpo",
        "/// A note.\ntype Note = Text {\n    max_length: 200\n}\n",
    );
    write(
        root,
        "sample/domains/accounts/actions/noted.jadpo",
        "use accounts/types { Note }\n\n/// Gives a note back as it was written.\naction noted(note: Note) -> Note {\n    return note\n}\n",
    );
    write(
        root,
        "recipes/check_password/actions/check_password.jadpo",
        recipe,
    );
    dir
}

/// The publishing bar's row `item`.
fn bar_row(root: &Path, item: &str) -> serde_json::Value {
    let checked = jadpo(root, &["package", "check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", checked.stdout, checked.stderr));
    document["data"]["bar"]
        .as_array()
        .and_then(|bar| bar.iter().find(|row| row["item"] == item))
        .cloned()
        .unwrap_or_else(|| panic!("a {item} row: {document}"))
}

const RECIPE: &str = "use app { directory }\nuse shared/providers/acme_directory\n\n/// Checks a password against the directory.\n{above}action check_password(request: AcmeDirectory.BindRequest) -> AcmeDirectory.BindResult {\n    return directory.binds.create(request)\n}\n";

/// A package describing an operation that takes a password publishes:
/// nothing is sent until something calls it, and its surface says so. The
/// recipe it ships is app code, answered in the app it is copied into, so
/// the bar holds the recipe to a decline as an app is.
#[test]
fn a_package_with_such_an_operation_passes_the_bar_and_says_so() {
    let dir = repository("secret-package-bar", &RECIPE.replace("{above}", ""));
    let root = dir.path();
    let compiles = bar_row(root, "compiles");
    assert_eq!(compiles["state"], "passed", "{compiles}");
    // It is written in the package's own surface, what reviewers read.
    assert!(jadpo(root, &["package", "check"]).code <= 1);
    let surface = std::fs::read_to_string(root.join("surface.jadpo_record")).expect("the surface");
    assert!(surface.contains("password: Password"), "{surface}");
    // The recipe is the app's, so the bar names the rule there.
    let recipes = bar_row(root, "recipes");
    assert_eq!(recipes["state"], "failed", "{recipes}");
    let checked = jadpo(root, &["package", "check"]);
    assert!(
        checked
            .stdout
            .contains("`directory.binds.create` sends a `Password` to a provider"),
        "{}",
        checked.stdout
    );
    let declined = RECIPE.replace(
        "{above}",
        "decline type.password_sent_to_provider: \"Shows how to bind the directory\"\n",
    );
    let dir = repository("secret-package-bar-declined", &declined);
    let recipes = bar_row(dir.path(), "recipes");
    assert_eq!(recipes["state"], "passed", "{recipes}");
    // The package's own action isn't the package's to sign off, so a
    // decline in it is unused.
    let dir = repository("secret-package-own-decline", &declined);
    let root = dir.path();
    write(
        root,
        "actions/bind.jadpo",
        "/// Binds, through the package's own operation.\ndecline type.password_sent_to_provider: \"Our bind is safe\"\npublic action bind(request: BindRequest) -> BindResult {\n    return binds.create(request)\n}\n",
    );
    let compiles = bar_row(root, "compiles");
    assert_eq!(compiles["state"], "failed", "{compiles}");
    let checked = jadpo(root, &["package", "check"]);
    assert!(
        checked.stdout.contains("decline.unused"),
        "{}",
        checked.stdout
    );
}
