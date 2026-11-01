//! `jadpo package conform <package> <capability>` (area 11, "Proving a
//! package works"; area 21, "How a provider fills it"; area 14; chunk
//! 5.5v7b): runs a capability's conformance suite against a package that
//! fills it. `ai`'s is the first built: a typed answer, a refusal, a
//! cut-off, a stream, an embedding of the declared length, and the usage
//! the answer reports.
//!
//! The suite is compiled as an app of its own: a scratch app with the
//! package installed as the app holds it, connected and named by `ai:`,
//! and a domain whose actions ask each case, so every case goes through
//! the very path an app's call takes: the package's mapping, its request,
//! the package driver's HTTP, its answer read through the package's own
//! types. By default it runs against a local fake on this machine's
//! loopback address ([`crate::conform_fake`]) that answers each case from
//! the package's own recording of the operation, marked where the mapping
//! says the provider marks it; nothing leaves the machine and no
//! credential is needed. With `--live` it runs against the provider's API
//! through the app's own connection to the package, with its credential
//! and `host:`, and asks the models the app's `ai:` names; it is refused
//! when the app has no such connection, nothing sets its credential, or
//! `ai:` doesn't go through it. A provider can't be made to refuse on
//! request, so the live run leaves the refusal to the run against the
//! recordings.
//!
//! The screen follows the publishing bar's (`jadpo package check`): the
//! failed cases' problems first, then the suite as a list, a row per case
//! with its state as a symbol and a word and a few words after, on the
//! list grid's third column, then the footer and one hint.
//!
//! ```text
//! Conformance suite
//! ✓ typed answer  passed                        decodes as ConformanceAnswer
//! ✓ refusal       passed                        refused, for each of its 2 marks
//! ✓ cut-off       passed                        over_budget at max_output: 16
//! ✓ stream        passed                        3 pieces, then whole
//! ✓ embedding     passed                        256 numbers, as declared
//! ✓ usage         passed                        12 in, 7 out, 3 cached
//!
//! ✓ ai conforms  OpenAi.v1 · 6 passed · its recordings and a local fake
//! → jadpo package conform openai ai --live  runs it against the provider's API with the app's key
//! ```
//!
//! A package that maps no `embed:` (Anthropic's) skips its row, as it
//! declares:
//!
//! ```text
//! – embedding     skipped                       Anthropic.v1 maps no embed:
//!
//! ✓ ai conforms  Anthropic.v1 · 5 passed · 1 skipped · its recordings and a local fake
//! ```
//!
//! A case that fails is an error, `conform.failed`, above the list:
//!
//! ```text
//! ✗ error  `OpenAi.v1`'s cut-off doesn't conform to `ai`
//!   reason  The local fake answers from the package's recordings, and the recording of
//!           `chat_completions.create` has no `choices.0.finish_reason`, where `cut_off_when:` looks.
//!   next    Record it with `jadpo package record <connection> <operation>` against the
//!           provider's sandbox, then run it again (human)
//!   rule    conform.failed
//!
//! Conformance suite
//! ✓ typed answer  passed                        decodes as ConformanceAnswer
//! ✗ cut-off       failed                        not in its recordings
//! …
//!
//! ✗ ai doesn't conform  1 failed · 5 passed · its recordings and a local fake
//! ```

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_check::surface;
use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_interp::Program;
use jadpo_runtime::ai::Done;
use jadpo_runtime::call_log::{CallLog, Seen};
use jadpo_runtime::stream::ServerEvent;
use jadpo_runtime::{Ctx, Env, FaultKind};
use jadpo_std::RuleKey;
use jadpo_std::ai::{AiDriver, AiMarker, PackageAi, SchemaMode};
use jadpo_std::provider::{Address, CredentialSource};
use jadpo_term::{Block, Environment, Fact, Json, Outcome, Row, RowState, Screen};
use jadpo_value::Value;
use serde_json::Value as J;

use crate::conform_fake::{self, Answer, LocalFake, Step};
use crate::fmt::project_needed;
use crate::package::{Scratch, and_list, arguments, plural};
use crate::package_record::NextMarked;
use crate::run::{Call, Reply, quoted};

/// The capability whose suite is built.
const AI: &str = "ai";

/// The scratch app's connection to the package.
const CONNECTION: &str = "provider";

/// How long the suite's embedding is: what its `embedding_dimensions:`
/// declares.
const DIMENSIONS: u64 = 256;

/// The `max_output:` the cut-off case asks with, too few for its answer.
const CUT_OFF_AT: u32 = 16;

/// The prompts the suite asks.
const QUESTION: &str = "conformance_question";
const ESSAY: &str = "conformance_long_answer";

/// What the local fake answers the typed case with, and what it decodes
/// as.
const TYPED: &str = "{\"city\":\"Lisbon\",\"letters\":6}";
const ANSWER_TYPE: &str = "ConformanceAnswer";

/// The pieces the local fake streams.
const PIECES: [&str; 3] = ["Lisbon", " is the", " capital."];

/// The tokens the local fake's answer says it used: input, output and
/// cached.
const USED: (u64, u64, u64) = (12, 7, 3);

pub fn conform(call: &Call) -> Reply {
    match jadpo_interp::on_large_stack(|| conformed(call)) {
        Ok(reply) => reply,
        Err(error) => Reply::failed(
            Diagnostic::new(&rule::CONFORM_FAILED, "The conformance suite couldn't run")
                .reason(format!(
                    "The interpreter runs on a thread with a large stack, and starting it failed: {error}."
                ))
                .next_marked("Free some memory, then run it again (human)"),
        ),
    }
}

/// Where the suite runs against.
enum Against {
    /// The local fake, from the package's recordings.
    Fake(LocalFake),
    /// The provider's API, at the app's connection's address, with its
    /// credential.
    Live { host: String, key: Option<String> },
}

impl Against {
    fn words(&self) -> String {
        match self {
            Against::Fake(_) => "its recordings and a local fake".to_string(),
            Against::Live { host, .. } => host.clone(),
        }
    }

    fn live(&self) -> bool {
        matches!(self, Against::Live { .. })
    }
}

fn conformed(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("package conform");
    };
    let (Some(wanted), Some(capability)) = (call.arguments.get(1), call.arguments.get(2)) else {
        return Reply::failed(arguments(
            "`jadpo package conform` needs a package and a capability".to_string(),
            "It runs the capability's conformance suite against a package that fills it: against the package's recordings and a local fake, or with `--live` against the provider's API.".to_string(),
            "Run `jadpo package conform <package> ai`".to_string(),
        ));
    };
    if let Some(extra) = call.arguments.get(3) {
        return Reply::failed(arguments(
            format!("`jadpo package conform` takes one capability, not `{extra}` as well"),
            "It runs one capability's suite against one package at a time.".to_string(),
            format!("Run `jadpo package conform {wanted} {capability}`"),
        ));
    }

    // The package, as `jadpo package show` names it.
    let loaded = jadpo_db::load_whole(&root);
    let (db, project) = (&loaded.db, loaded.project);
    let homes = surface::homes(db, project);
    let found: Vec<&jadpo_db::PackageHome> = homes
        .values()
        .filter(|home| {
            *wanted == home.name
                || wanted.eq_ignore_ascii_case(&home.code)
                || *wanted == home.versioned()
        })
        .collect();
    let home = match found.as_slice() {
        [one] => (*one).clone(),
        [first, .., last] => {
            let held: Vec<String> = found.iter().map(|home| home.versioned()).collect();
            return Reply::failed(arguments(
                format!("`{wanted}` names {} packages", found.len()),
                format!(
                    "The project holds {} side by side, so `jadpo package conform` needs the version too.",
                    and_list(&quoted(&held))
                ),
                format!(
                    "Use `jadpo package conform {} {capability}` or `jadpo package conform {} {capability}`",
                    first.versioned(),
                    last.versioned()
                ),
            ));
        }
        [] => {
            let names: Vec<String> = homes.values().map(|home| home.name.clone()).collect();
            let reason = if names.is_empty() {
                "The project holds no package: none in `packages/` or `shared/providers/`."
                    .to_string()
            } else {
                format!("The project holds {}.", and_list(&quoted(&names)))
            };
            return Reply::failed(match nearest(wanted, names.iter().map(String::as_str)) {
                Some(near) => arguments(
                    format!("`{wanted}` isn't a package in this project"),
                    format!("{reason} Did you mean `{near}`?"),
                    format!("Use `jadpo package conform {near} {capability}`"),
                ),
                None => arguments(
                    format!("`{wanted}` isn't a package in this project"),
                    reason,
                    "Install it with `jadpo package add`, then run it again".to_string(),
                ),
            });
        }
    };
    let package = home.versioned();
    let lock = std::fs::read_to_string(root.join(jadpo_db::LOCK))
        .ok()
        .and_then(|text| jadpo_db::parse_lock(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .find(|line| line.package == package);
    let source = match lock.as_ref().map(|line| &line.source) {
        Some(jadpo_db::Source::Link { path }) => root.join(path),
        _ => root.join(&home.folder),
    };

    // What it fills.
    let fills = filled_capabilities(&source);
    if capability != AI {
        if fills.iter().any(|filled| filled == capability) {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("`jadpo package conform` has no suite for `{capability}` yet"),
                )
                .reason("Area 11 gives every capability a conformance suite; `ai`'s is the first built (chunk 5.5v7b), and each other capability's comes with its own work.")
                .next(format!(
                    "Run `jadpo package conform <package> ai` for a package that fills `ai`; `{package}` is checked by `jadpo package check` meanwhile"
                )),
            );
        }
        return Reply::failed(doesnt_fill(&package, capability, &fills, &home.name));
    }
    if !fills.iter().any(|filled| filled == AI) {
        return Reply::failed(doesnt_fill(&package, AI, &fills, &home.name));
    }
    let Some(mapping) = jadpo_check::ai_mapping::mappings(db, project)
        .of
        .get(&home.folder)
        .cloned()
    else {
        return Reply::failed(
            Diagnostic::new(
                &rule::CONFORM_FAILED,
                format!("`{package}`'s `provide ai` doesn't read"),
            )
            .reason("The suite asks through the package's mapping, and `jadpo check` finds problems in it.")
            .next_marked("Run `jadpo check`, correct `provides/ai.jadpo`, then run it again (agent)"),
        );
    };
    let provider_text =
        std::fs::read_to_string(source.join(jadpo_db::PROVIDER_FILE)).unwrap_or_default();
    if keyed(&provider_text, "hosts") {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("`{package}` has fixed hosts, which the suite can't reach yet"),
            )
            .reason("The suite reaches the provider through its connection's `host:`, and a package with `hosts:` takes none; both first-party packages that fill `ai` take `host:` (area 21, \"The conformance suite\").")
            .next("Give the package no `hosts:`, so each connection gives `host:`, or leave it to `jadpo package check` meanwhile"),
        );
    }

    // The app, for its connection to the package and the models it asks.
    let ready = match crate::dev::prepare(&root, &call.environment, true) {
        Ok(ready) => ready,
        Err((diagnostics, sources)) => return not_run(diagnostics, sources),
    };
    let connection = ready
        .prepared
        .plan()
        .package_connections()
        .find(|(_, _, known)| known.package == package)
        .map(|(_, name, known)| (name, known));
    let through = ready
        .prepared
        .plan()
        .ai()
        .filter(|ai| connection.is_some_and(|(name, _)| name == ai.connection));
    let app_model = |name: &str| {
        through
            .and_then(|ai| ai.model(name))
            .and_then(|at| ready.config.text_at(at))
            .map(str::to_string)
    };
    let takes_key = match connection {
        Some((_, known)) => known.credential.is_some(),
        None => keyed(&provider_text, "credential"),
    };
    let live = call.flag("--live");
    let against = if live {
        match live_target(&ready, connection, through.is_some(), &package) {
            Ok(against) => against,
            Err(refused) => return Reply::failed(*refused),
        }
    } else {
        match LocalFake::start() {
            Ok(fake) => Against::Fake(fake),
            Err(error) => {
                return Reply::failed(
                    Diagnostic::new(&rule::CONFORM_FAILED, "The local fake couldn't start")
                        .reason(format!(
                            "The suite answers from a server on this machine's loopback address, and listening there failed: {error}."
                        ))
                        .next_marked("Check nothing stops programs listening on 127.0.0.1, then run it again (human)"),
                );
            }
        }
    };
    let table_model = mapping
        .contexts
        .first()
        .map_or_else(|| "conformance".to_string(), |(model, _)| model.clone());
    let table_model = format!("{}/{table_model}", home.name);
    let model = app_model(jadpo_std::ai::DEFAULT_MODEL).unwrap_or_else(|| table_model.clone());
    let embedding_model = app_model(jadpo_std::embed::DEFAULT_MODEL)
        .or_else(|| (!against.live()).then(|| table_model.clone()));
    let embeds = mapping.embed.is_some() && embedding_model.is_some();
    let streams = mapping.stream.is_some();

    // The suite, compiled as an app of its own.
    let scratch = match Scratch::new() {
        Ok(scratch) => scratch,
        Err(error) => {
            return crate::package::write_failed(
                "package conform",
                "a scratch folder",
                &error.to_string(),
            );
        }
    };
    let suite = Suite {
        package: &package,
        takes_key,
        streams,
        embeds,
    };
    if let Err(error) = place(&scratch.0, &source, &home, lock.as_ref(), &suite) {
        return crate::package::write_failed(
            "package conform",
            "the scratch app the suite runs in",
            &error,
        );
    }
    let mut env = Environment::empty()
        .with_var("CONFORMANCE_MODEL", &model)
        .with_var(
            "CONFORMANCE_HOST",
            &match &against {
                Against::Fake(fake) => fake.url.clone(),
                Against::Live { host, .. } => host.clone(),
            },
        );
    if let Some(embedding_model) = &embedding_model
        && embeds
    {
        env = env.with_var("CONFORMANCE_EMBEDDING_MODEL", embedding_model);
    }
    if takes_key {
        let key = match &against {
            Against::Fake(_) => Some("conformance-stand-in".to_string()),
            Against::Live { key, .. } => key.clone(),
        };
        if let Some(key) = key {
            env = env.with_var("CONFORMANCE_KEY", &key);
        }
    }
    // `ai:`'s day's counts are kept in a table of the main store, which
    // its first migration makes.
    let migrated = crate::run::run(
        &[
            "migrate".to_string(),
            "new".to_string(),
            "initial".to_string(),
        ],
        &Environment::empty().with_cwd(&scratch.0),
    );
    // One that couldn't be made shows its problems as the app is
    // prepared below.
    let _ = migrated;
    let scratch_ready = match crate::dev::prepare(&scratch.0, &env, true) {
        Ok(ready) => ready,
        Err((diagnostics, sources)) => return not_run(diagnostics, sources),
    };
    let program = Program::new(std::sync::Arc::clone(&scratch_ready.prepared))
        .with_config(&scratch_ready.config);
    let Some(ai) = program.ai() else {
        return defect("The suite's app asks no model");
    };
    let AiDriver::Package(filled) = &ai.plan.driver else {
        return defect("The suite's app asks no package");
    };
    let filled: PackageAi = (**filled).clone();
    let log = Rc::new(Log::default());
    let host = jadpo_host_native::NativeHost::new()
        .with_drivers(
            jadpo_host_native::Drivers::build(
                program.runtime_plan().connections,
                program.started(),
                &jadpo_host_native::supplies(),
            )
            .keeping(|asked| asked == CONNECTION),
        )
        .with_call_log(Rc::clone(&log) as Rc<dyn CallLog>);
    let runtime = match jadpo_host_native::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            return Reply::failed(
                Diagnostic::new(&rule::CONFORM_FAILED, "The conformance suite couldn't run")
                    .reason(format!(
                        "The runtime that sends its calls couldn't start: {error}."
                    ))
                    .next_marked("Free some memory, then run it again (human)"),
            );
        }
    };
    let run = Run {
        program: &program,
        host: &host,
        runtime: &runtime,
        log: &log,
        package: &package,
        filled: &filled,
        answers: Answers {
            json: program.stored_json(),
            program: &program,
            package: &filled,
            recordings: scratch
                .0
                .join(mount(&home))
                .join(jadpo_runtime::recorded::FOLDER),
        },
        against: &against,
    };
    let mut cases = Vec::new();
    let typed = run.typed();
    let answered = typed.1.clone();
    cases.push(typed.0);
    cases.push(run.refusal());
    cases.push(run.cut_off());
    cases.push(if streams {
        run.stream()
    } else {
        Case::skipped("stream", format!("{package} maps no stream:"))
    });
    cases.push(if embeds {
        run.embedding()
    } else if mapping.embed.is_none() {
        Case::skipped("embedding", format!("{package} maps no embed:"))
    } else {
        Case::skipped("embedding", "ai: names no embedding model")
    });
    cases.push(run.usage(answered.as_deref()));
    report(&package, wanted, &against, cases)
}

/// What the suite's app holds besides the package.
struct Suite<'a> {
    package: &'a str,
    takes_key: bool,
    streams: bool,
    embeds: bool,
}

/// The capabilities the package in `folder` fills: its `provides/` files.
fn filled_capabilities(folder: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(folder.join("provides"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".jadpo")?;
            (!stem.ends_with("_test") && !stem.contains('.')).then(|| stem.to_string())
        })
        .collect();
    out.sort();
    out
}

/// Whether `text`, a package's `provider.jadpo`, gives key `key` (`hosts`,
/// `credential`) at the top of its `provider { }`.
fn keyed(text: &str, key: &str) -> bool {
    let prefix = format!("{key}:");
    // The block's own keys, at its first level of indentation, as `jadpo
    // fmt` writes it.
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        line.len() - trimmed.len() == 4 && trimmed.starts_with(&prefix)
    })
}

fn doesnt_fill(package: &str, capability: &str, fills: &[String], name: &str) -> Diagnostic {
    let reason = if fills.is_empty() {
        format!("`{package}` fills no capability: it has no `provides/` file.")
    } else {
        format!(
            "`{package}` fills {}, each with its `provides/` file.",
            and_list(&quoted(fills))
        )
    };
    let next = match fills.first() {
        Some(first) if first == AI => format!("Run `jadpo package conform {name} ai`"),
        _ => "Name a package that fills it, such as one with `provides/ai.jadpo`".to_string(),
    };
    arguments(
        format!("`{package}` doesn't fill `{capability}`"),
        reason,
        next,
    )
}

/// The provider's API through the app's connection to the package, or
/// why the live run is refused.
fn live_target(
    ready: &crate::dev::Ready,
    connection: Option<(&str, &jadpo_std::provider::ConnectionPlan)>,
    ai_through: bool,
    package: &str,
) -> Result<Against, Box<Diagnostic>> {
    let refused = |headline: String, reason: String, next: String| {
        Box::new(
            Diagnostic::new(&rule::CONFORM_REFUSED, headline)
                .reason(reason)
                .next_marked(next),
        )
    };
    let Some((name, chosen)) = connection else {
        return Err(refused(
            format!("The app has no connection to `{package}`"),
            format!(
                "`--live` calls the provider's API through the app's own connection to `{package}`, with its credential and `host:`."
            ),
            format!(
                "Connect `{package}` in `app.jadpo`'s `connections:` with its credential and `host:`, then run it again (agent)"
            ),
        ));
    };
    if let Some(unset) = ready.unset.get(name)
        && !unset.is_empty()
    {
        let variables: Vec<String> = unset.iter().map(|name| format!("`{name}`")).collect();
        return Err(refused(
            format!("`{name}` has nothing set for the provider's API"),
            format!(
                "`--live` sends the suite to the provider's API with `{name}`'s credential and address, and nothing sets {}, so only a stand-in would be sent.",
                and_list(&variables)
            ),
            format!(
                "Set {} in `.env.local` to the provider's, then run it again (human)",
                and_list(&variables)
            ),
        ));
    }
    let key = match chosen.credential.map(|credential| credential.source) {
        Some(CredentialSource::Setting { secret, .. }) => match ready.config.text_at(secret) {
            Some(key) if !key.is_empty() => Some(key.to_string()),
            _ => {
                return Err(refused(
                    format!("`{name}` has no credential set"),
                    "`--live` sends the suite to the provider's API with the connection's credential, and its setting is empty.".to_string(),
                    "Set the connection's credential in `.env.local`, then run it again (human)"
                        .to_string(),
                ));
            }
        },
        None => None,
    };
    let host = match &chosen.address {
        Address::Setting(at) => ready.config.text_at(*at).map(str::to_string),
        _ => None,
    };
    let Some(host) = host.filter(|host| !host.is_empty()) else {
        return Err(refused(
            format!("`{name}` has no address to call"),
            "`--live` sends the suite to the address the connection's `host:` gives, and it gives none.".to_string(),
            format!("Give `{name}` a `host:` in `app.jadpo`, then run it again (agent)"),
        ));
    };
    if !ai_through {
        return Err(refused(
            format!("`ai:` doesn't ask its models through `{name}`"),
            "`--live` asks the models the app's `ai:` names, so it proves what the app will send, and `ai:` names another connection or none.".to_string(),
            format!(
                "Name `{name}` in `ai: {{ connection: }}` with the models to ask, then run it again (agent)"
            ),
        ));
    }
    Ok(Against::Live { host, key })
}

/// Writes the suite's app in `root`: the package in `packages/` as the
/// app installed it, locked to its digest there, its connection, `ai:`,
/// and the domain whose actions ask each case.
/// Where the suite's app holds the package. The app's own package, in
/// `shared/providers/`, is the suite's app's own too, with no lock line:
/// as an installed copy it would answer to an installed package's rules,
/// such as being proprietary only when installed from git.
fn mount(home: &jadpo_db::PackageHome) -> String {
    if home.installed {
        format!("{}/{}/{}", jadpo_db::PACKAGES, home.name, home.version)
    } else {
        home.folder.clone()
    }
}

fn place(
    root: &Path,
    source: &Path,
    home: &jadpo_db::PackageHome,
    locked: Option<&jadpo_db::Locked>,
    suite: &Suite<'_>,
) -> Result<(), String> {
    let own = !home.installed;
    let mount = mount(home);
    let write = |path: &str, text: &str| -> Result<(), String> {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, text).map_err(|error| error.to_string())
    };
    for file in crate::package::package_files(source) {
        let to = root.join(&mount).join(&file);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::copy(source.join(&file), to).map_err(|error| error.to_string())?;
    }
    for (path, text) in sources(suite) {
        write(&path, &text)?;
    }
    if own {
        return Ok(());
    }
    // Locked to the digest of the files copied, as `jadpo package add`
    // locks them.
    let loaded = jadpo_db::load(root);
    let copied = surface::homes(&loaded.db, loaded.project)
        .into_values()
        .find(|found| found.versioned() == suite.package)
        .ok_or_else(|| "the package wasn't found where it was copied".to_string())?;
    let digest = jadpo_check::packages::digest_of(&loaded.db, loaded.project, &copied)
        .map_err(|path| format!("`{path}` couldn't be read"))?;
    let version = match locked.map(|line| &line.source) {
        Some(
            jadpo_db::Source::Registry { version, .. }
            | jadpo_db::Source::Git { version, .. }
            | jadpo_db::Source::Path { version, .. },
        ) => version.clone(),
        _ => "0.0.0".to_string(),
    };
    let line = jadpo_db::Locked {
        package: suite.package.to_string(),
        source: jadpo_db::Source::Path {
            version,
            path: source.to_string_lossy().replace('\\', "/"),
            digest,
        },
    };
    write(jadpo_db::LOCK, &jadpo_db::write_lock(&[line]))
}

/// The suite's app's own files.
fn sources(suite: &Suite<'_>) -> Vec<(String, String)> {
    let credential = if suite.takes_key {
        "            credential: config.conformance_key\n"
    } else {
        ""
    };
    let (models, dimensions) = if suite.embeds {
        (
            "            default: config.conformance_model\n            embedding: config.conformance_embedding_model\n",
            format!("        embedding_dimensions: {DIMENSIONS}\n"),
        )
    } else {
        (
            "            default: config.conformance_model\n",
            String::new(),
        )
    };
    let mut out = vec![
        (
            "app.jadpo".to_string(),
            format!(
                "application {{\n    connections: {{\n        {CONNECTION}: {{\n            package: {}\n            domains: [conformance]\n{credential}            host: config.conformance_host\n        }}\n    }}\n    ai: {{\n        connection: {CONNECTION}\n        models: {{\n{models}        }}\n{dimensions}        limits: {{\n            per_day: {{\n                tokens: 2000000\n            }}\n        }}\n    }}\n}}\n",
                suite.package
            ),
        ),
        (
            "config.jadpo".to_string(),
            format!(
                "config {{\n    conformance_model: ModelId {{\n    }}\n{}    conformance_host: Url {{\n    }}\n{}}}\n",
                if suite.embeds {
                    "    conformance_embedding_model: ModelId {\n    }\n"
                } else {
                    ""
                },
                if suite.takes_key {
                    "    conformance_key: Text {\n        secret: true\n    }\n"
                } else {
                    ""
                }
            ),
        ),
    ];
    let mut actions = vec!["answer_typed", "write_essay"];
    if suite.streams {
        actions.push("streamed_conformance");
    }
    if suite.embeds {
        actions.push("embedded_conformance");
    }
    out.push((
        "policy.jadpo".to_string(),
        format!(
            "use conformance\n\npolicy {{\n    actions: {{\n{}    }}\n}}\n",
            actions
                .iter()
                .map(|action| format!("        conformance.{action}: [Access.public]\n"))
                .collect::<String>()
        ),
    ));
    let domain = |path: &str, text: &str| (format!("domains/conformance/{path}"), text.to_string());
    out.extend([
        domain(
            "types/conformance_text.jadpo",
            "/// What the suite asks a model.\ntype ConformanceText = Text {\n    max_length: 2000\n}\n",
        ),
        domain(
            "types/conformance_asked.jadpo",
            "use conformance/types { ConformanceText }\n\n/// One question the suite asks.\ntype ConformanceAsked = Input {\n    question: ConformanceText\n}\n",
        ),
        domain(
            "types/conformance_answer.jadpo",
            "/// A typed answer: a city, and how many letters its name has.\ntype ConformanceAnswer = Output {\n    /// The city the question asks about.\n    city: Text {\n        max_length: 200\n    }\n    /// How many letters the city's name has.\n    letters: Int\n}\n",
        ),
        domain(
            "types/conformance_essay.jadpo",
            "/// A long answer, cut off on purpose.\ntype ConformanceEssay = Text {\n    max_length: 100000\n}\n",
        ),
        domain(
            "failures/conformance_unavailable.jadpo",
            "/// The model gave no answer the suite can use.\ntype ConformanceUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The model gave no answer the suite can use.\"\n}\n",
        ),
        domain(
            "prompts/conformance_question.jadpo",
            "use conformance/types { ConformanceText }\n\n/// Asks a question with a short answer.\ntype ConformanceQuestion = Prompt {\n    question: ConformanceText\n}\n",
        ),
        domain(
            "prompts/conformance_question.prompt",
            "# Instructions\n\nYou answer questions about places in a few words.\n\n# Request\n\n{question}\n",
        ),
        domain(
            "prompts/conformance_long_answer.jadpo",
            "use conformance/types { ConformanceText }\n\n/// Asks for a long answer.\ntype ConformanceLongAnswer = Prompt {\n    question: ConformanceText\n}\n",
        ),
        domain(
            "prompts/conformance_long_answer.prompt",
            "# Instructions\n\nYou write long, detailed answers.\n\n# Request\n\n{question}\n",
        ),
    ]);
    let generate = |name: &str, prompt: &str, ty: &str, max: u32, timeout: &str| {
        format!(
            "use conformance/failures {{ ConformanceUnavailable }}\nuse conformance/prompts {{ {prompt} }}\nuse conformance/types {{ ConformanceAsked, {ty} }}\n\n/// Asks the model, as the suite's case does.\naction {name}(asked: ConformanceAsked) fails ConformanceUnavailable -> {ty} {{\n    return attempt ai.generate(\n        {prompt} {{\n            question: asked.question\n        }},\n        as: {ty},\n        max_output: {max},\n        timeout: {timeout}\n    ) {{\n        refused: ConformanceUnavailable\n        malformed: ConformanceUnavailable\n        timed_out: ConformanceUnavailable\n        over_budget: ConformanceUnavailable\n        too_long: ConformanceUnavailable\n    }}\n}}\n"
        )
    };
    out.push(domain(
        "actions/answer_typed.jadpo",
        &generate(
            "answer_typed",
            "ConformanceQuestion",
            ANSWER_TYPE,
            4000,
            "25s",
        ),
    ));
    out.push(domain(
        "actions/write_essay.jadpo",
        &generate(
            "write_essay",
            "ConformanceLongAnswer",
            "ConformanceEssay",
            CUT_OFF_AT,
            "25s",
        ),
    ));
    if suite.streams {
        out.push(domain(
            "actions/streamed_conformance.jadpo",
            "use conformance/failures { ConformanceUnavailable }\nuse conformance/prompts { ConformanceQuestion }\nuse conformance/types { ConformanceAsked }\n\n/// Streams the model's answer, as the suite's case does.\naction streamed_conformance(asked: ConformanceAsked) fails ConformanceUnavailable -> AiStream {\n    return attempt ai.stream(\n        ConformanceQuestion {\n            question: asked.question\n        },\n        max_output: 4000,\n        timeout: 25s\n    ) {\n        refused: ConformanceUnavailable\n        timed_out: ConformanceUnavailable\n        over_budget: ConformanceUnavailable\n        too_long: ConformanceUnavailable\n    }\n}\n",
        ));
    }
    if suite.embeds {
        out.extend([
            domain(
                "types/conformance_meaning.jadpo",
                &format!(
                    "/// A text's meaning, as the embedding model makes it.\ntype ConformanceMeaning = Vector {{\n    dimensions: {DIMENSIONS}\n}}\n"
                ),
            ),
            domain(
                "types/conformance_meant.jadpo",
                "use conformance/types { ConformanceMeaning }\n\n/// A text's meaning, given back.\ntype ConformanceMeant = Output {\n    meaning: ConformanceMeaning\n}\n",
            ),
            domain(
                "actions/embedded_conformance.jadpo",
                "use conformance/failures { ConformanceUnavailable }\nuse conformance/types { ConformanceAsked, ConformanceMeaning, ConformanceMeant }\n\n/// Embeds a text, as the suite's case does.\naction embedded_conformance(asked: ConformanceAsked) fails ConformanceUnavailable -> ConformanceMeant {\n    var meaning: ConformanceMeaning = attempt ai.embed(asked.question, as: ConformanceMeaning) {\n        timed_out: ConformanceUnavailable\n        over_budget: ConformanceUnavailable\n        too_long: ConformanceUnavailable\n    }\n    return ConformanceMeant {\n        meaning: meaning\n    }\n}\n",
            ),
        ]);
    }
    out
}

// --- the cases -------------------------------------------------------------

/// One row of the suite.
struct Case {
    name: &'static str,
    state: RowState,
    detail: String,
    problem: Option<Diagnostic>,
}

impl Case {
    fn passed(name: &'static str, detail: impl Into<String>) -> Case {
        Case {
            name,
            state: RowState::Done,
            detail: detail.into(),
            problem: None,
        }
    }

    fn skipped(name: &'static str, detail: impl Into<String>) -> Case {
        Case {
            name,
            state: RowState::Skipped,
            detail: detail.into(),
            problem: None,
        }
    }

    fn word(&self) -> &'static str {
        match self.state {
            RowState::Done => "passed",
            RowState::Failed => "failed",
            RowState::Skipped => "skipped",
        }
    }

    fn key(&self) -> String {
        self.name.replace([' ', '-'], "_")
    }
}

/// The requests the suite's calls sent and the answers that came back, as
/// the call log is handed them.
#[derive(Default)]
struct Log(RefCell<Vec<Exchange>>);

#[derive(Clone, Debug, Default)]
struct Exchange {
    body: Option<String>,
    answer: Option<String>,
}

impl CallLog for Log {
    fn seen(&self, seen: &Seen<'_>) {
        self.0.borrow_mut().push(Exchange {
            body: seen.body.map(str::to_string),
            answer: seen.answer.map(str::to_string),
        });
    }
}

/// What the suite runs with.
struct Run<'r> {
    program: &'r Program,
    host: &'r jadpo_host_native::NativeHost,
    runtime: &'r jadpo_host_native::Runtime,
    log: &'r Log,
    package: &'r str,
    filled: &'r PackageAi,
    answers: Answers<'r>,
    against: &'r Against,
}

impl Run<'_> {
    fn fake(&self) -> Option<&LocalFake> {
        match self.against {
            Against::Fake(fake) => Some(fake),
            Against::Live { .. } => None,
        }
    }

    fn failed(&self, name: &'static str, detail: String, reason: String, next: String) -> Case {
        let problem = Diagnostic::new(
            &rule::CONFORM_FAILED,
            format!("`{}`'s {name} doesn't conform to `ai`", self.package),
        )
        .reason(reason)
        .next_marked(next);
        Case {
            name,
            state: RowState::Failed,
            detail,
            problem: Some(problem),
        }
    }

    /// A case the mapping's reading failed, with what the call ended in.
    fn misread(&self, name: &'static str, detail: String, reason: String) -> Case {
        let next = if self.against.live() {
            "Check `provides/ai.jadpo` against the provider's documentation and its answer, then run it again (agent)"
        } else {
            "Correct `provides/ai.jadpo` against the provider's documentation and the package's recordings, then run it again (agent)"
        };
        self.failed(name, detail, reason, next.to_string())
    }

    /// A case the local fake has no recording to answer from.
    fn unrecorded(&self, name: &'static str, why: String) -> Case {
        self.failed(
            name,
            "not in its recordings".to_string(),
            format!("The local fake answers from the package's recordings, and {why}."),
            "Record it with `jadpo package record <connection> <operation>` against the provider's sandbox, then run it again (human)".to_string(),
        )
    }

    /// A value of prompt `prompt`'s type, asking `question`.
    fn question(&self, prompt: &str, question: &str) -> Value {
        let ty = self
            .program
            .ai()
            .zip(self.program.generate_site_of(prompt))
            .and_then(|(ai, site)| ai.prompts.entries.get(site.entry as usize))
            .map(|entry| entry.type_name.clone())
            .unwrap_or_default();
        Value::record(
            &ty,
            vec![(Rc::from("question"), Value::Text(Rc::from(question)))],
        )
    }

    fn ctx(&self) -> Ctx<'_> {
        let mut ctx = Ctx::of(self.program, self.host, Env::default());
        ctx.allow_standalone_calls();
        ctx
    }

    fn ask(&self, prompt: &str, question: &str) -> Option<Done> {
        self.log.0.borrow_mut().clear();
        let value = self.question(prompt, question);
        let mut ctx = self.ctx();
        self.runtime
            .block_on(self.program.ask_model(&mut ctx, prompt, value))
    }

    /// A typed answer: decoded as the call's type, its schema sent where
    /// the mapping says; and the answer as sent, for the usage case.
    fn typed(&self) -> (Case, Option<String>) {
        const NAME: &str = "typed answer";
        if let Some(fake) = self.fake() {
            match self.answers.generate() {
                Ok(answer) => fake.answer(Answer::json(answer.to_string())),
                Err(why) => return (self.unrecorded(NAME, why), None),
            }
        }
        let done = self.ask(
            QUESTION,
            "Which city is the capital of Portugal? Give the city, and how many letters its name has.",
        );
        let exchange = self.log.0.borrow().last().cloned().unwrap_or_default();
        let answer = exchange.answer.clone();
        let Some(done) = done else {
            return (
                self.misread(
                    NAME,
                    "not asked".into(),
                    "The suite's call wasn't found.".into(),
                ),
                None,
            );
        };
        let Done::Value(value) = &done else {
            let got = ended(&done);
            return (
                self.misread(
                    NAME,
                    "no typed answer".into(),
                    format!("Asked for a `{ANSWER_TYPE}`, the call ended in {got}."),
                ),
                answer,
            );
        };
        if self.fake().is_some() {
            let city = match value {
                Value::Record(record) => record.get("city").cloned(),
                _ => None,
            };
            if !matches!(city, Some(Value::Text(ref text)) if &**text == "Lisbon") {
                return (
                    self.misread(
                        NAME,
                        "not from the answer's text".into(),
                        "The answer decoded, but not as the text the local fake put where `answer:` reads it, so the text came from somewhere else.".into(),
                    ),
                    answer,
                );
            }
        }
        let detail = format!("decodes as {ANSWER_TYPE}");
        if self.answers.mode() == SchemaMode::StrictSubset
            && let Some(at) = &self.filled.schema_at
        {
            let sent = exchange
                .body
                .as_deref()
                .and_then(|body| serde_json::from_str::<J>(body).ok());
            if sent
                .as_ref()
                .and_then(|sent| conform_fake::get(sent, &conform_fake::dotted(at)))
                .is_none()
            {
                return (
                    self.misread(
                        NAME,
                        "no schema sent".into(),
                        format!(
                            "The answer decoded, but the request carried no schema at `{at}`, where `schema: {{ at: }}` says."
                        ),
                    ),
                    answer,
                );
            }
        }
        (Case::passed(NAME, detail), answer)
    }

    /// A refusal: each of the mapping's marks read as `refused`.
    fn refusal(&self) -> Case {
        const NAME: &str = "refusal";
        let Some(fake) = self.fake() else {
            return Case::skipped(NAME, "a provider can't be made to refuse");
        };
        let marks = &self.filled.refused_when;
        if marks.is_empty() {
            return self.misread(
                NAME,
                "no refused_when:".into(),
                "The mapping says nothing of a refusal, so a refused answer would be read as an answer.".into(),
            );
        }
        for mark in marks {
            let answer = match self.answers.generate().and_then(|mut answer| {
                self.answers.mark_refused(&mut answer, mark)?;
                Ok(answer)
            }) {
                Ok(answer) => answer,
                Err(why) => return self.unrecorded(NAME, why),
            };
            fake.answer(Answer::json(answer.to_string()));
            let done = self.ask(QUESTION, "Which city is the capital of Portugal?");
            match done {
                Some(Done::Broke {
                    rule: RuleKey::Refused,
                    ..
                }) => {}
                other => {
                    let got = other.as_ref().map_or_else(|| "nothing".to_string(), ended);
                    return self.misread(
                        NAME,
                        "not read as refused".into(),
                        format!(
                            "The answer marked as `refused_when:` says ({}) ended in {got}, not `refused`.",
                            mark_words(mark)
                        ),
                    );
                }
            }
        }
        Case::passed(
            NAME,
            if marks.len() == 1 {
                "refused, for its mark".to_string()
            } else {
                format!("refused, for each of its {} marks", marks.len())
            },
        )
    }

    /// A cut-off: an answer stopped at `max_output:` read as
    /// `over_budget`.
    fn cut_off(&self) -> Case {
        const NAME: &str = "cut-off";
        if let Some(fake) = self.fake() {
            let Some((path, equals)) = &self.filled.cut_off_when else {
                return self.misread(
                    NAME,
                    "no cut_off_when:".into(),
                    "The mapping says nothing of a cut-off, so an answer stopped at `max_output:` would be read as whole.".into(),
                );
            };
            let answer = match self.answers.generate().and_then(|mut answer| {
                let value = serde_json::from_str::<J>(equals).unwrap_or(J::Null);
                if !conform_fake::put(&mut answer, &conform_fake::dotted(path), &value) {
                    return Err(format!(
                        "the recording of `{}` has no `{path}`, where `cut_off_when:` looks",
                        self.filled.operation
                    ));
                }
                Ok(answer)
            }) {
                Ok(answer) => answer,
                Err(why) => return self.unrecorded(NAME, why),
            };
            fake.answer(Answer::json(answer.to_string()));
        }
        let done = self.ask(
            ESSAY,
            "Write a long essay, of at least a thousand words, about the history of Lisbon.",
        );
        match done {
            Some(Done::Broke {
                rule: RuleKey::OverBudget,
                ..
            }) => Case::passed(NAME, format!("over_budget at max_output: {CUT_OFF_AT}")),
            other => {
                let got = other.as_ref().map_or_else(|| "nothing".to_string(), ended);
                self.misread(
                    NAME,
                    "not read as over_budget".into(),
                    format!(
                        "Asked with `max_output: {CUT_OFF_AT}` for a long answer, the call ended in {got}, not `over_budget`."
                    ),
                )
            }
        }
    }

    /// A stream: its pieces read as they are sent, then whole.
    fn stream(&self) -> Case {
        const NAME: &str = "stream";
        if let Some(fake) = self.fake() {
            match self.answers.stream() {
                Ok(answer) => fake.answer(answer),
                Err(why) => return self.unrecorded(NAME, why),
            }
        }
        self.log.0.borrow_mut().clear();
        let value = self.question(QUESTION, "Which city is the capital of Portugal?");
        let mut ctx = self.ctx();
        let done = self
            .runtime
            .block_on(self.program.stream_model(&mut ctx, QUESTION, value));
        let opened = match &done {
            Some(Done::Value(value)) => jadpo_runtime::stream::take_opened(&mut ctx, value),
            _ => None,
        };
        let Some(opened) = opened else {
            let got = done.as_ref().map_or_else(|| "nothing".to_string(), ended);
            return self.misread(
                NAME,
                "ended before its first piece".into(),
                format!("The stream ended in {got} before its first piece."),
            );
        };
        let host: &dyn jadpo_runtime::Host = self.host;
        let (pieces, last) = self.runtime.block_on(async {
            let mut pieces = Vec::new();
            let mut last = None;
            while let Some(event) = opened.next_on(Some(host)).await {
                match event {
                    ServerEvent::Piece(text) => pieces.push(text),
                    other => {
                        last = Some(other);
                        break;
                    }
                }
            }
            (pieces, last)
        });
        let count = plural(pieces.len(), "piece", "pieces");
        if last != Some(ServerEvent::Done) {
            return self.misread(
                NAME,
                format!("{count}, then not whole"),
                format!(
                    "The stream sent {count}, then ended with an `error` event rather than whole: its end isn't read where `done_when:` or `data: [DONE]` says."
                ),
            );
        }
        if self.fake().is_some() && pieces != PIECES {
            return self.misread(
                NAME,
                format!("{count}, not the {} sent", PIECES.len()),
                format!(
                    "The local fake sent {} pieces where `pieces:` reads them, and {count} came through.",
                    PIECES.len()
                ),
            );
        }
        if pieces.is_empty() {
            return self.misread(
                NAME,
                "no pieces, then whole".into(),
                "The stream ended whole with no text in it.".into(),
            );
        }
        Case::passed(NAME, format!("{count}, then whole"))
    }

    /// An embedding as long as `embedding_dimensions:` declares.
    fn embedding(&self) -> Case {
        const NAME: &str = "embedding";
        if let Some(fake) = self.fake() {
            match self.answers.embed() {
                Ok(answer) => fake.answer(Answer::json(answer.to_string())),
                Err(why) => return self.unrecorded(NAME, why),
            }
        }
        self.log.0.borrow_mut().clear();
        let mut ctx = self.ctx();
        let done = self.runtime.block_on(
            self.program
                .embed_text(&mut ctx, "Lisbon is the capital of Portugal."),
        );
        match done {
            Some(Done::Value(value)) => match length(&value) {
                Some(n) if n as u64 == DIMENSIONS => {
                    Case::passed(NAME, format!("{DIMENSIONS} numbers, as declared"))
                }
                found => self.misread(
                    NAME,
                    format!("{} numbers, not {DIMENSIONS}", found.unwrap_or(0)),
                    format!(
                        "Asked for {DIMENSIONS} dimensions, the embedding read had {}.",
                        found.unwrap_or(0)
                    ),
                ),
            },
            other => {
                let got = other.as_ref().map_or_else(|| "nothing".to_string(), ended);
                self.misread(
                    NAME,
                    "no embedding".into(),
                    format!(
                        "Asked to embed one text in {DIMENSIONS} dimensions, the call ended in {got}."
                    ),
                )
            }
        }
    }

    /// The tokens the typed answer says it used, read as `usage:` says.
    fn usage(&self, answer: Option<&str>) -> Case {
        const NAME: &str = "usage";
        // Usage is read from the typed answer's case, whose own failure
        // says what to fix first.
        let Some(answer) = answer else {
            return self.failed(
                NAME,
                "no answer to read".into(),
                "The typed answer's case got no answer, so there is no usage to read.".into(),
                "Fix the typed answer's case first, then run it again (agent)".into(),
            );
        };
        let Some(ai) = self.program.ai() else {
            return self.misread(NAME, "no model".into(), "The suite asks no model.".into());
        };
        let mut ctx = self.ctx();
        let read = jadpo_runtime::ai::reported_usage(&mut ctx, &ai, answer);
        let Some((input, output, cached)) = read else {
            return self.misread(
                NAME,
                "none read".into(),
                "The answer's usage isn't read where `usage:` says.".into(),
            );
        };
        let words = format!("{input} in, {output} out, {cached} cached");
        let expected = self.fake().map(|_| {
            let cached = self
                .filled
                .usage
                .as_ref()
                .and_then(|usage| usage.cached.as_ref())
                .map_or(0, |_| USED.2);
            (USED.0, USED.1, cached)
        });
        let right = match expected {
            Some(expected) => (input, output, cached) == expected,
            None => input > 0 && output > 0,
        };
        if right {
            Case::passed(NAME, words)
        } else {
            let sent = expected.map_or_else(
                || "some input and output".to_string(),
                |(i, o, c)| format!("{i} in, {o} out, {c} cached"),
            );
            self.misread(
                NAME,
                format!("{words} read"),
                format!("The answer says it used {sent} tokens, and `usage:` read {words}."),
            )
        }
    }
}

/// How a call ended, in a few words, never its text.
fn ended(done: &Done) -> String {
    match done {
        Done::Value(_) => "an answer".to_string(),
        Done::Broke {
            rule: RuleKey::Malformed,
            problem,
        } => {
            let field = |name: &str| match problem {
                Some(Value::Record(record)) => match record.get(name) {
                    Some(Value::Text(text)) => text.to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            };
            match (field("field"), field("rule")) {
                (at, broke) if at.is_empty() => format!("`malformed` (`{broke}`)"),
                (at, broke) => format!("`malformed` (`{broke}` on `{at}`)"),
            }
        }
        Done::Broke { rule, .. } => format!("`{}`", rule.word()),
        Done::Failed(failure) => format!("the runtime's `{}`", failure.code),
        Done::Fault(FaultKind::Provider(why)) => format!("a fault reading the answer: {why}"),
        Done::Fault(kind) => format!("a `{}` fault", kind.classification()),
    }
}

/// A mark of `refused_when:`, as the mapping writes it.
fn mark_words(mark: &AiMarker) -> String {
    match mark {
        AiMarker::Holds(path) => format!("`{}` holds a refusal", path.join(".")),
        AiMarker::Equals(path, equals) => format!("`{path}` is {}", equals.trim()),
    }
}

/// How many numbers a vector holds.
fn length(value: &Value) -> Option<usize> {
    match value {
        Value::List(items) => Some(items.len()),
        Value::Record(record) => record.fields.iter().find_map(|(_, value)| match value {
            Value::List(items) => Some(items.len()),
            _ => None,
        }),
        _ => None,
    }
}

// --- the local fake's answers ---------------------------------------------

/// Makes each case's answer from the package's recordings.
struct Answers<'a> {
    json: jadpo_runtime::JsonCodecs<'a>,
    program: &'a Program,
    package: &'a PackageAi,
    recordings: PathBuf,
}

impl Answers<'_> {
    fn mode(&self) -> SchemaMode {
        self.program
            .plan()
            .ai()
            .map_or(SchemaMode::None, |ai| ai.schema)
    }

    /// Operation `operation`'s recording, its placeholders filled; and
    /// where its output is in the answer, and the output type's codec.
    fn recording(
        &self,
        operation: &str,
    ) -> Result<(J, Option<String>, Option<jadpo_std::http::CodecId>), String> {
        let plan = self.program.plan().providers();
        let table = self.program.plan().connections();
        let Some((_, op)) = plan.operation(table, CONNECTION, operation) else {
            return Err(format!("the package has no operation `{operation}`"));
        };
        let file = self.recordings.join(format!("{operation}.json"));
        let Ok(text) = std::fs::read_to_string(&file) else {
            return Err(format!(
                "it has no recording of `{operation}`, `{}`",
                jadpo_runtime::recorded::file_name(operation)
            ));
        };
        let mut next = 0u32;
        let mut uuid = || {
            next += 1;
            format!("00000000-0000-4000-8000-{next:012}")
        };
        let mut making = jadpo_runtime::fakes::Making {
            now: 1_784_505_600_000,
            uuid: &mut uuid,
        };
        let filled = jadpo_runtime::recorded::filled(&self.json, op, &text, &mut making)
            .and_then(|filled| serde_json::from_str::<J>(&filled).ok())
            .ok_or_else(|| {
                format!(
                    "its recording of `{operation}` isn't JSON, `{}`",
                    jadpo_runtime::recorded::file_name(operation)
                )
            })?;
        let codec = op.output.and_then(|number| self.json.codec_of(number));
        Ok((filled, op.output_at.clone(), codec))
    }

    /// The places, by field names in `codec`'s type, as places by
    /// spellings on the wire.
    fn wire(
        &self,
        codec: Option<jadpo_std::http::CodecId>,
        at: Option<&str>,
        path: &[String],
    ) -> Option<Vec<Step>> {
        let steps = conform_fake::by_fields(&self.json.codecs, codec?, path)?;
        Some(conform_fake::under(at, steps))
    }

    /// `generate:`'s answer: the typed answer's text where `answer:` reads
    /// it, nothing where a refusal would be, and the tokens used where
    /// `usage:` reads them.
    fn generate(&self) -> Result<J, String> {
        let operation = &self.package.operation;
        let (mut answer, at, codec) = self.recording(operation)?;
        let Some(place) = self.wire(codec, at.as_deref(), &self.package.answer) else {
            return Err(format!(
                "the recording of `{operation}` has no `{}`, where `answer:` reads the text",
                self.package.answer.join(".")
            ));
        };
        if !conform_fake::put(&mut answer, &place, &J::from(TYPED)) {
            return Err(format!(
                "the recording of `{operation}` has no `{}`, where `answer:` reads the text",
                self.package.answer.join(".")
            ));
        }
        for mark in &self.package.refused_when {
            if let AiMarker::Holds(path) = mark
                && let Some(place) = self.wire(codec, at.as_deref(), path)
                && conform_fake::get(&answer, &place).is_some()
            {
                conform_fake::put(&mut answer, &place, &J::Null);
            }
        }
        if let Some(usage) = &self.package.usage {
            for (path, count) in [
                (&usage.input, USED.0),
                (&usage.output, USED.1),
                (&usage.cached, USED.2),
            ] {
                if let Some(path) = path
                    && let Some(place) = self.wire(codec, at.as_deref(), path)
                {
                    conform_fake::put(&mut answer, &place, &J::from(count));
                }
            }
        }
        Ok(answer)
    }

    /// `answer` marked as refused by `mark`.
    fn mark_refused(&self, answer: &mut J, mark: &AiMarker) -> Result<(), String> {
        let operation = &self.package.operation;
        let put = match mark {
            AiMarker::Holds(path) => {
                let plan = self.program.plan().providers();
                let (codec, at) = plan
                    .operation(self.program.plan().connections(), CONNECTION, operation)
                    .map(|(_, op)| {
                        (
                            op.output.and_then(|number| self.json.codec_of(number)),
                            op.output_at.clone(),
                        )
                    })
                    .unwrap_or((None, None));
                self.wire(codec, at.as_deref(), path).is_some_and(|place| {
                    conform_fake::put_new(answer, &place, &J::from("I can't help with that."))
                })
            }
            AiMarker::Equals(path, equals) => {
                let value = serde_json::from_str::<J>(equals).unwrap_or(J::Null);
                conform_fake::put(answer, &conform_fake::dotted(path), &value)
            }
        };
        if put {
            Ok(())
        } else {
            Err(format!(
                "the recording of `{operation}` has no place where `refused_when:` looks ({})",
                mark_words(mark)
            ))
        }
    }

    /// `stream:`'s answer: [`PIECES`], each an event made from the
    /// recording with its text where `pieces:` reads it, then the event
    /// `done_when:` marks, or `data: [DONE]`.
    fn stream(&self) -> Result<Answer, String> {
        let Some(stream) = &self.package.stream else {
            return Err("the package maps no `stream:`".to_string());
        };
        let operation = &stream.operation;
        let (mut event, _, _) = self.recording(operation)?;
        let pieces = conform_fake::spelled(&stream.pieces);
        for mark in &stream.refused_when {
            if let AiMarker::Holds(path) = mark {
                let place = conform_fake::spelled(path);
                if conform_fake::get(&event, &place).is_some() {
                    conform_fake::put(&mut event, &place, &J::Null);
                }
            }
        }
        let mut events = Vec::new();
        for piece in PIECES {
            let mut one = event.clone();
            if !conform_fake::put(&mut one, &pieces, &J::from(piece)) {
                return Err(format!(
                    "the recording of `{operation}` has no `{}`, where `pieces:` reads the text",
                    stream.pieces.join(".")
                ));
            }
            events.push(one);
        }
        let done = match &stream.done_when {
            Some((path, equals)) => {
                let mut last = event.clone();
                conform_fake::put(&mut last, &pieces, &J::Null);
                let value = serde_json::from_str::<J>(equals).unwrap_or(J::Null);
                if !conform_fake::put(&mut last, &conform_fake::dotted(path), &value) {
                    return Err(format!(
                        "the recording of `{operation}` has no `{path}`, where `done_when:` looks"
                    ));
                }
                events.push(last);
                false
            }
            None => true,
        };
        Ok(Answer::events(&events, done))
    }

    /// `embed:`'s answer: one vector of [`DIMENSIONS`] numbers where
    /// `vector:` reads it, its place `0` where `index:` reads it.
    fn embed(&self) -> Result<J, String> {
        let Some(embed) = &self.package.embed else {
            return Err("the package maps no `embed:`".to_string());
        };
        let operation = &embed.operation;
        let (mut answer, _, _) = self.recording(operation)?;
        let list = conform_fake::spelled(&embed.vectors);
        let Some(J::Array(items)) = conform_fake::get(&answer, &list).cloned() else {
            return Err(format!(
                "the recording of `{operation}` has no list at `{}`, where `vectors:` reads them",
                embed.vectors.join(".")
            ));
        };
        let Some(mut item) = items.into_iter().next() else {
            return Err(format!(
                "the recording of `{operation}` has no item at `{}`",
                embed.vectors.join(".")
            ));
        };
        let numbers: Vec<J> = (0..DIMENSIONS)
            .map(|at| J::from(((at % 97) as f64 + 1.0) / 128.0))
            .collect();
        if !conform_fake::put(
            &mut item,
            &conform_fake::spelled(&embed.vector),
            &J::Array(numbers),
        ) {
            return Err(format!(
                "the recording of `{operation}` has no `{}` in its items, where `vector:` reads it",
                embed.vector.join(".")
            ));
        }
        if let Some(index) = &embed.index {
            conform_fake::put(&mut item, &conform_fake::spelled(index), &J::from(0));
        }
        conform_fake::put(&mut answer, &list, &J::Array(vec![item]));
        Ok(answer)
    }
}

// --- the screen --------------------------------------------------------------

fn report(package: &str, name: &str, against: &Against, cases: Vec<Case>) -> Reply {
    let mut screen = Screen::new();
    let mut diagnostics = Vec::new();
    for case in &cases {
        if let Some(problem) = &case.problem {
            screen.push(Block::diagnostic_from(
                problem,
                &std::collections::BTreeMap::new(),
                None,
            ));
            diagnostics.push(problem.clone());
        }
    }
    let rows: Vec<Row> = cases
        .iter()
        .map(|case| {
            Row::new(case.name, case.word())
                .state(case.state)
                .extra([case.detail.clone()])
        })
        .collect();
    screen.push(Block::list("Conformance suite", rows));
    let count = |state: RowState| cases.iter().filter(|case| case.state == state).count();
    let (failed, passed, skipped) = (
        count(RowState::Failed),
        count(RowState::Done),
        count(RowState::Skipped),
    );
    let mut facts = Vec::new();
    if failed == 0 {
        facts.push(Fact::plain(package));
    } else {
        facts.push(Fact::problem(format!("{failed} failed")));
    }
    facts.push(Fact::plain(format!("{passed} passed")));
    if skipped > 0 {
        facts.push(Fact::plain(format!("{skipped} skipped")));
    }
    facts.push(Fact::plain(against.words()));
    let conforms = failed == 0;
    let (outcome, headline) = if conforms {
        (Outcome::Success, "ai conforms")
    } else {
        (Outcome::Failure, "ai doesn't conform")
    };
    screen.push(Block::summary(outcome, headline, facts));
    if conforms && !against.live() {
        screen.push(Block::hint(
            format!("jadpo package conform {name} ai --live"),
            "runs it against the provider's API with the app's key",
        ));
    }
    let data = Json::object([
        ("package", Json::from(package)),
        ("capability", Json::from(AI)),
        (
            "against",
            Json::from(if against.live() { "live" } else { "recordings" }),
        ),
        (
            "host",
            match against {
                Against::Live { host, .. } => Json::from(host.as_str()),
                Against::Fake(_) => Json::Null,
            },
        ),
        ("conforms", conforms.into()),
        (
            "cases",
            Json::array(cases.iter().map(|case| {
                Json::object([
                    ("case", Json::from(case.key().as_str())),
                    ("state", Json::from(case.word())),
                    ("detail", Json::from(case.detail.as_str())),
                ])
            })),
        ),
    ]);
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources: std::collections::BTreeMap::new(),
    }
}

/// The project's, or the suite's app's, errors, which stop the suite.
fn not_run(
    diagnostics: Vec<Diagnostic>,
    sources: std::collections::BTreeMap<String, String>,
) -> Reply {
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic_from(diagnostic, &sources, None));
    }
    screen.push(Block::summary(
        Outcome::Failure,
        "suite not run",
        [Fact::problem(plural(diagnostics.len(), "error", "errors"))],
    ));
    Reply::Report {
        screen,
        data: Json::object([("conforms", false.into())]),
        diagnostics,
        sources,
    }
}

fn defect(headline: &str) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::CONFORM_FAILED, headline.to_string())
            .reason("The suite's own app didn't compile as it should.")
            .next_marked("Report it to Jadpo's maintainers with this report (human)"),
    )
}
