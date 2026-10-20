//! `jadpo package conform <package> ai` (area 11, "Proving a package
//! works"; area 21, "How a provider fills it"; chunk 5.5v7b): the `ai`
//! suite (a typed answer, a refusal, a cut-off, a stream, an embedding of
//! the declared length, the usage reported) run against both first-party
//! packages, installed as an app installs them, against their recordings
//! and the local fake Jadpo starts on this machine's loopback address.
//!
//! The live run is never made here: no real provider, no network and no
//! key. It is refused without `--live` and a set credential, and without
//! `--live` nothing goes to the connection's address even when a key is
//! set; the run against the provider's API is left to Richard.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("package-conform-5.5v7b-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// A first-party package as this repository holds it.
fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages")
        .join(name)
}

/// An app asking its models through `name`'s package, installed from
/// `from` with `jadpo package add`; `ai:` goes through it when `ai` is
/// true.
fn app(test: &str, name: &str, from: &Path, ai: bool) -> PathBuf {
    let root = folder(test);
    let (connection, package, model, embedding) = match name {
        "openai" => (
            "openai",
            "OpenAi.v1",
            "openai/gpt-6-luna",
            Some("openai/text-embedding-3-small"),
        ),
        _ => (
            "anthropic",
            "Anthropic.v1",
            "anthropic/claude-haiku-5-5",
            None,
        ),
    };
    let models = match embedding {
        Some(_) => {
            "            default: config.drafting_model\n            embedding: config.search_model\n        }\n        embedding_dimensions: 3\n"
        }
        None => "            default: config.drafting_model\n        }\n",
    };
    let asks = if ai {
        format!(
            "    ai: {{\n        connection: {connection}\n        models: {{\n{models}        limits: {{\n            per_day: {{\n                tokens: 2000000\n            }}\n        }}\n    }}\n"
        )
    } else {
        String::new()
    };
    write(
        &root,
        "app.jadpo",
        &format!(
            "application {{\n    connections: {{\n        {connection}: {{\n            package: {package}\n            domains: [support]\n            credential: config.provider_key\n            host: config.provider_host\n        }}\n    }}\n{asks}}}\n"
        ),
    );
    let search = embedding.map_or_else(String::new, |model| {
        format!("    search_model: ModelId {{\n        default: \"{model}\"\n    }}\n")
    });
    write(
        &root,
        "config.jadpo",
        &format!(
            "config {{\n    drafting_model: ModelId {{\n        default: \"{model}\"\n    }}\n    provider_host: Url {{\n        default: \"https://provider.invalid\"\n    }}\n    provider_key: Text {{\n        secret: true\n    }}\n{search}}}\n"
        ),
    );
    write(
        &root,
        "domains/support/types/asked.jadpo",
        "/// What a customer asked.\ntype Asked = Input {\n    question: Text {\n        max_length: 4000\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/types/echoed.jadpo",
        "/// The question, sent back.\ntype Echoed = Output {\n    question: Text {\n        max_length: 4000\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/actions/echo.jadpo",
        "use support/types { Asked, Echoed }\n\n/// Sends the question back.\naction echo(asked: Asked) -> Echoed {\n    return Echoed {\n        question: asked.question\n    }\n}\n",
    );
    write(
        &root,
        "policy.jadpo",
        "use support\n\npolicy {\n    actions: {\n        support.echo: [Access.public]\n    }\n}\n",
    );
    let added = jadpo(&root, &["package", "add", from.to_str().expect("a path")]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    if ai {
        let migrated = jadpo(&root, &["migrate", "new", "initial"]);
        assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    }
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

/// What `out` printed, on either stream.
fn said(out: &Output) -> String {
    format!("{}{}", out.stdout, out.stderr)
}

/// Each case's state in `out`'s JSON, by its key.
fn cases(out: &Output) -> Vec<(String, String)> {
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("JSON");
    json["data"]["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("no cases: {}", out.stdout))
        .iter()
        .map(|case| {
            (
                case["case"].as_str().unwrap_or_default().to_string(),
                case["state"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn states(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(case, state)| (case.to_string(), state.to_string()))
        .collect()
}

#[test]
fn openai_conforms_against_its_recordings_and_a_local_fake() {
    let root = app("openai", "openai", &package("openai"), true);
    let out = jadpo(&root, &["package", "conform", "openai", "ai"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "Conformance suite\n  \
         typed answer  passed                        decodes as ConformanceAnswer\n  \
         refusal       passed                        refused, for each of its 2 marks\n  \
         cut-off       passed                        over_budget at max_output: 16\n  \
         stream        passed                        3 pieces, then whole\n  \
         embedding     passed                        256 numbers, as declared\n  \
         usage         passed                        12 in, 7 out, 3 cached\n\
         \n\
         ai conforms: OpenAi.v1, 6 passed, its recordings and a local fake\n\
         \n\
         next: jadpo package conform openai ai --live  runs it against the provider's API with the app's key\n",
    );
    let json = jadpo(&root, &["package", "conform", "openai", "ai", "--json"]);
    assert_eq!(json.code, 0, "{}", json.stdout);
    assert_eq!(
        cases(&json),
        states(&[
            ("typed_answer", "passed"),
            ("refusal", "passed"),
            ("cut_off", "passed"),
            ("stream", "passed"),
            ("embedding", "passed"),
            ("usage", "passed"),
        ])
    );
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["data"]["against"], "recordings");
    assert_eq!(document["data"]["conforms"], true);
}

#[test]
fn anthropic_conforms_and_skips_the_embedding_it_declares_it_has_none_of() {
    let root = app("anthropic", "anthropic", &package("anthropic"), true);
    let out = jadpo(&root, &["package", "conform", "anthropic", "ai", "--json"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        cases(&out),
        states(&[
            ("typed_answer", "passed"),
            ("refusal", "passed"),
            ("cut_off", "passed"),
            ("stream", "passed"),
            ("embedding", "skipped"),
            ("usage", "passed"),
        ])
    );
    let plain = jadpo(&root, &["package", "conform", "Anthropic.v1", "ai"]);
    assert_eq!(plain.code, 0, "{}", plain.stdout);
    assert!(
        plain.stdout.contains(
            "  embedding     skipped                       Anthropic.v1 maps no embed:\n"
        ),
        "{}",
        plain.stdout
    );
    assert!(
        plain.stdout.contains(
            "ai conforms: Anthropic.v1, 5 passed, 1 skipped, its recordings and a local fake\n"
        ),
        "{}",
        plain.stdout
    );
}

/// Without `--live`, the suite runs against the recordings and the local
/// fake, never the connection's address, even with a key set for it.
#[test]
fn without_the_live_flag_nothing_goes_to_the_connections_address() {
    let api = FakeProvider::start();
    let root = app("no-flag", "openai", &package("openai"), true);
    write(
        &root,
        ".env.local",
        &format!("PROVIDER_KEY=test_key_5_5v7b\nPROVIDER_HOST={}\n", api.url),
    );
    let out = jadpo(&root, &["package", "conform", "openai", "ai"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout.contains("its recordings and a local fake"),
        "{}",
        out.stdout
    );
    assert!(api.requests().is_empty(), "{:?}", api.requests());
}

#[test]
fn the_live_run_is_refused_without_a_set_credential() {
    let root = app("live-no-key", "openai", &package("openai"), true);
    let out = jadpo(&root, &["package", "conform", "openai", "ai", "--live"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(said(&out).contains("conform.refused"), "{}", said(&out));
    assert!(
        said(&out).contains("`openai` has nothing set for the provider's API"),
        "{}",
        out.stdout
    );
    assert!(said(&out).contains("`PROVIDER_KEY`"), "{}", said(&out));
}

#[test]
fn the_live_run_is_refused_when_ai_asks_through_another_connection() {
    let root = app("live-no-ai", "anthropic", &package("anthropic"), false);
    write(
        &root,
        ".env.local",
        "PROVIDER_KEY=test_key_5_5v7b\nPROVIDER_HOST=https://provider.invalid\n",
    );
    // Against the recordings it asks a model of the package's own table.
    let recorded = jadpo(&root, &["package", "conform", "anthropic", "ai"]);
    assert_eq!(recorded.code, 0, "{}", said(&recorded));
    let out = jadpo(&root, &["package", "conform", "anthropic", "ai", "--live"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(said(&out).contains("conform.refused"), "{}", said(&out));
    assert!(
        said(&out).contains("`ai:` doesn't ask its models through `anthropic`"),
        "{}",
        out.stdout
    );
}

/// A recording that doesn't carry what the mapping reads fails the cases
/// that need it, each with its problem.
#[test]
fn a_recording_without_the_cut_off_mark_fails_its_cases() {
    let copy = folder("broken-package");
    let source = package("openai");
    for entry in walk(&source) {
        let relative = entry.strip_prefix(&source).expect("inside");
        let to = copy.join(relative);
        fs::create_dir_all(to.parent().expect("parent")).expect("folders");
        fs::copy(&entry, &to).expect("copy");
    }
    let recording = copy.join("recordings/chat_completions.create.json");
    let text = fs::read_to_string(&recording).expect("recording");
    fs::write(
        &recording,
        text.replace("      \"finish_reason\": \"{Text}\",\n", ""),
    )
    .expect("write");
    let root = app("broken", "openai", &copy, true);
    let out = jadpo(&root, &["package", "conform", "openai", "ai", "--json"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        cases(&out),
        states(&[
            ("typed_answer", "passed"),
            ("refusal", "failed"),
            ("cut_off", "failed"),
            ("stream", "passed"),
            ("embedding", "passed"),
            ("usage", "passed"),
        ])
    );
    let plain = jadpo(&root, &["package", "conform", "openai", "ai"]);
    assert!(
        plain
            .stdout
            .contains("`OpenAi.v1`'s cut-off doesn't conform to `ai`"),
        "{}",
        plain.stdout
    );
    assert!(plain.stdout.contains("conform.failed"), "{}", plain.stdout);
    assert!(
        plain
            .stdout
            .contains("ai doesn't conform: 2 failed, 4 passed, its recordings and a local fake\n"),
        "{}",
        plain.stdout
    );
}

#[test]
fn a_capability_the_package_doesnt_fill_or_a_package_the_app_doesnt_hold_is_named() {
    let root = app("arguments", "openai", &package("openai"), true);
    let email = jadpo(&root, &["package", "conform", "openai", "email"]);
    assert_eq!(email.code, 2, "{}", said(&email));
    assert!(
        said(&email).contains("`OpenAi.v1` doesn't fill `email`"),
        "{}",
        said(&email)
    );
    let unknown = jadpo(&root, &["package", "conform", "opneai", "ai"]);
    assert_eq!(unknown.code, 2, "{}", said(&unknown));
    assert!(
        said(&unknown).contains("Did you mean `open_ai`?"),
        "{}",
        said(&unknown)
    );
    let bare = jadpo(&root, &["package", "conform", "openai"]);
    assert_eq!(bare.code, 2, "{}", said(&bare));
    assert!(
        said(&bare).contains("`jadpo package conform` needs a package and a capability"),
        "{}",
        bare.stdout
    );
}

/// Every file under `folder`, hidden ones left out.
fn walk(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("folder").flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}
