//! Provider calls in `jadpo dev` (area 11, "Provider calls in
//! development"; area 17, "Provider calls in development: `jadpo calls`";
//! chunk 7.2d): every provider operation faked by default, answered with
//! what `jadpo calls script` scripted for it, else from its package's
//! recording with its placeholders filled (chunk 7.2d2), else its
//! generated default, so nothing leaves the machine and no credential is
//! needed;
//! the operations a connection's `development: { sandbox: }` names, or
//! `--sandbox` names for one session, let through to the provider's
//! sandbox, never live; and every call logged with its values in full in
//! `build/dev/calls.jsonl`, which `jadpo calls` lists. A package filling
//! `ai` is faked the same way (chunk 5.5v3b): each prompt answered with a
//! generated value of the call's type or the condition `jadpo calls
//! script` scripted, and the prompts its `sandbox:` names let through.
//!
//! The log keeps the last [`KEPT`] calls. Scripted answers wait in
//! `build/dev/script/`, one file each, in the order they were scripted;
//! the call that takes one removes it, so each answers one call. Both
//! live in `build/`, which is never committed.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::body::FakeCondition;
use jadpo_interp::Prepared;
use jadpo_runtime::LocalFuture;
use jadpo_runtime::call_log::{CallLog, Seen};
use jadpo_runtime::capability::{Answer, Call, Capability, Ended, Source};
use jadpo_runtime::config::ConfigValues;
use jadpo_std::provider::Address;

/// The call log, relative to the project.
pub(crate) const CALLS: &str = "build/dev/calls.jsonl";
/// The answers `jadpo calls script` scripted, waiting for their calls.
pub(crate) const SCRIPT: &str = "build/dev/script";
/// How many calls the log keeps (area 11: "the last 500").
pub(crate) const KEPT: usize = 500;

// --- what each connection does --------------------------------------------

/// What `jadpo dev` does with each provider connection's calls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Setup {
    pub connections: Vec<ConnectionSetup>,
    /// The operations whose package keeps a recording, which the fake
    /// answers from (chunk 7.2d2): `couriers.pickups.create`.
    pub recorded: Vec<String>,
    /// The prompts asked through a package's connection that fills `ai`,
    /// as their calls are named, `acme.answer_question` (chunk 5.5v3b):
    /// faked with a generated value of the call's type, or a condition
    /// `jadpo calls script` scripted.
    pub prompts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectionSetup {
    pub name: String,
    /// The operations let through to the sandbox, `pickups.create`.
    pub sandbox: Vec<String>,
    /// The operations `development: { sandbox: }` or `--sandbox` names
    /// that stay faked because a setting they need has no value: the
    /// variables to set.
    pub unset: Vec<String>,
}

impl Setup {
    /// Whether `call` goes to the sandbox.
    fn lets_through(&self, call: &Call) -> bool {
        self.connections.iter().any(|connection| {
            connection.name == call.connection && connection.sandbox.contains(&call.operation)
        })
    }

    /// Whether `call`'s operation keeps a recording to answer from.
    fn recorded(&self, call: &Call) -> bool {
        self.recorded.contains(&call.name())
    }

    /// Whether `call` asks a model through a package.
    fn prompt(&self, call: &Call) -> bool {
        self.prompts.contains(&call.name())
    }
}

/// What the session asked for besides the connections' own
/// `development:` (area 11): `--fake <connection>` keeps all its calls
/// faked, `--sandbox <operation>` lets one through, for this session only.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Session {
    pub fake: Vec<String>,
    pub sandbox: Vec<String>,
}

/// The settings each provider connection reads that no variable or
/// default gives, by connection: a stand-in lets the app start, and its
/// calls stay faked (area 11: "no credential is needed").
pub(crate) type Unset = BTreeMap<String, Vec<String>>;

/// What `jadpo dev` does with each provider connection, for `prepared`
/// with `values`; refused when a let-through connection's environment
/// is live, or a session flag names nothing the app has.
pub(crate) fn setup(
    prepared: &Prepared,
    values: &ConfigValues,
    unset: &Unset,
    session: &Session,
) -> Result<Setup, Box<Diagnostic>> {
    let plan = prepared.plan().providers();
    let config = prepared.plan().config();
    for named in &session.fake {
        if !plan.connections.iter().any(|known| known.name == *named) {
            return Err(Box::new(arguments(
                format!("`--fake {named}` names no connection to a provider"),
                format!(
                    "`--fake` keeps every call of one of the app's provider connections faked: {}.",
                    listed(plan.connections.iter().map(|known| known.name.as_str()))
                ),
            )));
        }
    }
    let mut asked: Vec<(String, String)> = Vec::new();
    for named in &session.sandbox {
        let found: Vec<(String, String)> = plan
            .operations
            .iter()
            .filter_map(|op| {
                let connection = plan.connections.get(op.connection as usize)?;
                let full = format!("{}.{}", connection.name, op.name);
                let resource = op.name.split('.').next().unwrap_or_default();
                (full == *named || format!("{}.{resource}", connection.name) == *named)
                    .then(|| (connection.name.clone(), op.name.clone()))
            })
            .collect();
        if found.is_empty() {
            return Err(Box::new(arguments(
                format!("`--sandbox {named}` names no provider operation"),
                format!(
                    "`--sandbox` lets one of the app's provider operations, or a resource's, through to its sandbox: {}.",
                    listed(
                        plan.operations
                            .iter()
                            .filter_map(|op| {
                                let connection = plan.connections.get(op.connection as usize)?;
                                Some(format!("{}.{}", connection.name, op.name))
                            })
                            .collect::<Vec<_>>()
                            .iter()
                            .map(String::as_str)
                    )
                ),
            )));
        }
        asked.extend(found);
    }
    // A package filling `ai` is faked as its other calls are: each prompt
    // asked through it (area 21, "Tests"; chunk 5.5v3b).
    let prompts = match prepared.plan().ai() {
        Some(ai) if matches!(ai.driver, jadpo_std::ai::AiDriver::Package(_)) => prepared
            .plan()
            .prompts()
            .entries
            .iter()
            .map(|entry| format!("{}.{}", ai.connection, entry.key))
            .collect(),
        _ => Vec::new(),
    };
    let mut out = Setup {
        prompts,
        recorded: plan
            .operations
            .iter()
            .filter(|op| op.recording.is_some())
            .filter_map(|op| {
                let connection = plan.connections.get(op.connection as usize)?;
                Some(format!("{}.{}", connection.name, op.name))
            })
            .collect(),
        ..Setup::default()
    };
    for connection in &plan.connections {
        let mut sandbox: Vec<String> = Vec::new();
        if !session.fake.contains(&connection.name) {
            sandbox.extend(connection.sandbox.iter().cloned());
            for (named, op) in &asked {
                if *named == connection.name && !sandbox.contains(op) {
                    sandbox.push(op.clone());
                }
            }
        }
        let missing = unset.get(&connection.name).cloned().unwrap_or_default();
        if !sandbox.is_empty() && missing.is_empty() {
            // Never live (area 11): the environment the connection's
            // setting chooses must be the sandbox.
            if let Address::Hosts { environment, .. } = &connection.address {
                let chosen = values.text_at(*environment);
                if chosen.is_some_and(|chosen| chosen.eq_ignore_ascii_case("live")) {
                    let variable = config
                        .fields
                        .get(*environment as usize)
                        .map_or_else(String::new, |field| field.variable.clone());
                    return Err(Box::new(
                        Diagnostic::new(
                            &rule::CLI_DEV_START_FAILED,
                            format!(
                                "`{}` lets calls through to its provider, and its environment is live",
                                connection.name
                            ),
                        )
                        .reason(format!(
                            "`jadpo dev` lets the operations `development: {{ sandbox: }}` names through to the provider's sandbox, never its live API, and `{variable}` chooses `live`."
                        ))
                        .next(format!(
                            "Set `{variable}` to `sandbox` in `.env.local`, or run `jadpo dev --fake {}`",
                            connection.name
                        )),
                    ));
                }
            }
        }
        let (sandbox, unset) = if missing.is_empty() {
            (sandbox, Vec::new())
        } else if sandbox.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            (Vec::new(), missing)
        };
        out.connections.push(ConnectionSetup {
            name: connection.name.clone(),
            sandbox,
            unset,
        });
    }
    Ok(out)
}

fn listed<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let names: Vec<String> = names.map(|name| format!("`{name}`")).collect();
    if names.is_empty() {
        "the app has none".to_string()
    } else {
        crate::run::either(&names)
    }
}

fn arguments(headline: String, reason: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_DEV_ARGUMENTS, headline)
        .reason(reason)
        .next("Name one of them, or leave the flag out")
}

/// The settings each provider connection of `prepared` reads, by
/// connection: its credential's, its environment's or its address's, each
/// by its number in the configuration's plan.
pub(crate) fn provider_settings(prepared: &Prepared) -> BTreeMap<String, Vec<u32>> {
    let mut out: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for connection in &prepared.plan().providers().connections {
        let mut read = Vec::new();
        if let Some((_, at)) = connection.credential {
            read.push(at);
        }
        match &connection.address {
            Address::Hosts { environment, .. } => read.push(*environment),
            Address::Setting(at) => read.push(*at),
            Address::Unknown => {}
        }
        out.insert(connection.name.clone(), read);
    }
    out
}

/// What stands in for a provider connection's setting no variable or
/// default gives, so the app starts with its calls faked: a credential's
/// placeholder, the sandbox, an address nothing answers at.
pub(crate) fn stand_in(type_name: &str) -> String {
    match type_name {
        "ProviderEnvironment" => "sandbox".to_string(),
        "Url" => "https://provider.invalid".to_string(),
        _ => "jadpo-dev-unset".to_string(),
    }
}

// --- the driver -------------------------------------------------------------

/// Every provider connection's driver in `jadpo dev`: the sandbox for
/// what is let through, the package's fake otherwise.
pub(crate) struct DevProviders {
    setup: Arc<Mutex<Setup>>,
    sandbox: jadpo_host_native::JsonHttp<jadpo_host_native::fetch::NativeFetch>,
    script: PathBuf,
    /// What the faked provider keeps under each key, as it would (chunk
    /// 7.5d): a client's retry under its key is given the first answer.
    keys: jadpo_runtime::fakes::KeptKeys,
}

impl DevProviders {
    pub(crate) fn new(root: &Path, setup: Arc<Mutex<Setup>>) -> DevProviders {
        DevProviders {
            setup,
            sandbox: jadpo_host_native::JsonHttp::new(jadpo_host_native::fetch::NativeFetch),
            script: root.join(SCRIPT),
            keys: jadpo_runtime::fakes::KeptKeys::new(),
        }
    }

    fn lets_through(&self, call: &Call) -> bool {
        match self.setup.lock() {
            Ok(setup) => setup.lets_through(call),
            Err(poisoned) => poisoned.into_inner().lets_through(call),
        }
    }

    fn recorded(&self, call: &Call) -> bool {
        match self.setup.lock() {
            Ok(setup) => setup.recorded(call),
            Err(poisoned) => poisoned.into_inner().recorded(call),
        }
    }

    fn prompt(&self, call: &Call) -> bool {
        match self.setup.lock() {
            Ok(setup) => setup.prompt(call),
            Err(poisoned) => poisoned.into_inner().prompt(call),
        }
    }
}

impl Capability for DevProviders {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        if self.lets_through(call) {
            return self.sandbox.call(call);
        }
        if let Some((answer, _)) = self.keys.given(call) {
            return Box::pin(async move { answer });
        }
        let source = self.source_of(call);
        // Scripted, else from the operation's recording, else its
        // generated default (chunk 7.2d2).
        let answer = match take(&self.script, &call.name()) {
            // A prompt's condition, in the `ai` fake's own form (chunk
            // 5.5v3b).
            Some(word) if self.prompt(call) => match jadpo_std::RuleKey::from_word(&word) {
                Some(rule) => {
                    jadpo_runtime::ai::scripted(jadpo_runtime::ai::Script::Condition(rule))
                }
                None => Answer::Generated,
            },
            Some(word) => scripted_answer(&word),
            None if self.recorded(call) => Answer::Recorded,
            None => Answer::Generated,
        };
        self.keys.keep(call, &answer, source);
        Box::pin(async move { answer })
    }

    fn source(&self) -> Source {
        Source::Generated
    }

    fn source_of(&self, call: &Call) -> Source {
        if self.lets_through(call) {
            Source::Sandbox
        } else if let Some(source) = self.keys.source_of(call) {
            source
        } else if waiting(&self.script, &call.name()) {
            Source::Scripted
        } else if self.recorded(call) {
            Source::Recording
        } else {
            Source::Generated
        }
    }
}

/// The answer a script's word gives: a condition, or the operation's
/// failure with that code.
fn scripted_answer(word: &str) -> Answer {
    match FakeCondition::named(word) {
        Some(FakeCondition::Unavailable | FakeCondition::InUse) => Answer::Unavailable,
        Some(FakeCondition::Lost) => Answer::Unknown,
        Some(FakeCondition::Misconfigured) => Answer::Misconfigured,
        None => Answer::Failed(word.to_string()),
    }
}

// --- scripts ----------------------------------------------------------------

/// The scripts waiting in `folder`, oldest first: each file and the
/// operation and answer it holds.
fn scripts(folder: &Path) -> Vec<(PathBuf, String, String)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let operation = json.get("operation")?.as_str()?.to_string();
            let answer = json.get("answer")?.as_str()?.to_string();
            Some((path, operation, answer))
        })
        .collect()
}

/// Whether an answer is scripted for `operation`.
fn waiting(folder: &Path, operation: &str) -> bool {
    scripts(folder)
        .iter()
        .any(|(_, scripted, _)| scripted == operation)
}

/// Takes the oldest answer scripted for `operation`, so it answers one
/// call: claimed by renaming its file, which only one worker can do.
fn take(folder: &Path, operation: &str) -> Option<String> {
    for (path, scripted, answer) in scripts(folder) {
        if scripted != operation {
            continue;
        }
        let claimed = path.with_extension(format!("taken-{}", std::process::id()));
        if std::fs::rename(&path, &claimed).is_ok() {
            let _ = std::fs::remove_file(&claimed);
            return Some(answer);
        }
    }
    None
}

/// Scripts `answer` for the next call of `operation` that takes none
/// before it (`jadpo calls script`).
pub(crate) fn script(root: &Path, operation: &str, answer: &str) -> std::io::Result<()> {
    let folder = root.join(SCRIPT);
    std::fs::create_dir_all(&folder)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let name = format!("{nanos:024}-{}.json", std::process::id());
    let document = serde_json::json!({ "operation": operation, "answer": answer });
    let partial = folder.join(format!("{name}.partial"));
    std::fs::write(&partial, format!("{document}\n"))?;
    std::fs::rename(partial, folder.join(name))
}

/// The answers scripted and not yet taken, oldest first: each operation
/// and answer.
pub(crate) fn scripted(root: &Path) -> Vec<(String, String)> {
    scripts(&root.join(SCRIPT))
        .into_iter()
        .map(|(_, operation, answer)| (operation, answer))
        .collect()
}

// --- the log ------------------------------------------------------------------

/// The call log's file, shared by every worker.
#[derive(Debug)]
pub(crate) struct LogFile {
    path: PathBuf,
    /// The next call's number.
    next: u64,
    /// How many calls the file holds.
    held: usize,
}

impl LogFile {
    /// The log in `root`, carrying on from the calls it already holds.
    pub(crate) fn open(root: &Path) -> LogFile {
        let path = root.join(CALLS);
        let entries = read(root);
        let next = entries
            .iter()
            .filter_map(|entry| entry.id.strip_prefix("call_")?.parse::<u64>().ok())
            .max()
            .map_or(1, |last| last + 1);
        LogFile {
            path,
            next,
            held: entries.len(),
        }
    }

    fn keep(&mut self, mut line: serde_json::Value) {
        let id = format!("call_{}", self.next);
        self.next += 1;
        if let Some(object) = line.as_object_mut() {
            object.insert("id".to_string(), serde_json::Value::from(id));
        }
        if let Some(folder) = self.path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let appended = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
        if appended.is_ok() {
            self.held += 1;
        }
        // Past the limit by a fifth, the oldest go.
        if self.held > KEPT + KEPT / 5
            && let Ok(text) = std::fs::read_to_string(&self.path)
        {
            let lines: Vec<&str> = text.lines().collect();
            let kept = &lines[lines.len().saturating_sub(KEPT)..];
            let mut out = kept.join("\n");
            out.push('\n');
            let partial = self.path.with_extension("jsonl.partial");
            if std::fs::write(&partial, out).is_ok()
                && std::fs::rename(&partial, &self.path).is_ok()
            {
                self.held = kept.len();
            }
        }
    }
}

/// `jadpo dev`'s call log: each provider call as it ends, its values in
/// full, never its headers.
pub(crate) struct DevLog(pub(crate) Arc<Mutex<LogFile>>);

impl CallLog for DevLog {
    fn seen(&self, seen: &Seen<'_>) {
        let (ended, code) = ended_words(seen.ended);
        let mut line = serde_json::json!({
            "at": jadpo_std::format_instant(seen.at),
            "connection": seen.connection,
            "operation": seen.operation,
            "source": seen.source.word(),
            "requestId": seen.request_id,
            "ms": seen.took_ms,
            "method": seen.method,
            "url": seen.url,
            "body": seen.body,
            "answer": seen.answer,
            "ended": ended,
            "code": code,
        });
        if let (Some(values), Some(members)) = (seen.values, line.as_object_mut()) {
            members.insert("values".to_string(), serde_json::Value::from(values));
        }
        match self.0.lock() {
            Ok(mut file) => file.keep(line),
            Err(poisoned) => poisoned.into_inner().keep(line),
        }
    }
}

/// How a call ended, as the log writes it, and a failure's code.
fn ended_words(ended: &Ended) -> (&'static str, Option<&str>) {
    match ended {
        Ended::InFlight => ("in_flight", None),
        Ended::Answered => ("answered", None),
        Ended::Failed(code) => ("failed", Some(code.as_str())),
        Ended::Unavailable => ("unavailable", None),
        Ended::Unknown => ("outcome_unknown", None),
        Ended::Misconfigured => ("misconfigured", None),
        Ended::Refused => ("refused", None),
    }
}

/// One call as the log holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub id: String,
    pub at: String,
    pub connection: String,
    pub operation: String,
    pub source: String,
    pub request_id: Option<String>,
    pub ms: Option<u64>,
    pub method: String,
    pub url: String,
    pub body: Option<String>,
    pub answer: Option<String>,
    pub ended: String,
    pub code: Option<String>,
    /// For a prompt asked of a model, its values as the code gave them
    /// (chunk 5.5v4b): what `jadpo ai record` asks with again.
    pub values: Option<String>,
    /// The line as written, for `--json`.
    pub json: serde_json::Value,
}

impl Entry {
    /// `couriers.pickups.create`.
    pub fn name(&self) -> String {
        format!("{}.{}", self.connection, self.operation)
    }
}

/// The calls the log in `root` holds, oldest first.
pub(crate) fn read(root: &Path) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(root.join(CALLS)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let json: serde_json::Value = serde_json::from_str(line).ok()?;
            let text = |key: &str| {
                json.get(key)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            };
            Some(Entry {
                id: text("id")?,
                at: text("at").unwrap_or_default(),
                connection: text("connection")?,
                operation: text("operation")?,
                source: text("source").unwrap_or_default(),
                request_id: text("requestId"),
                ms: json.get("ms").and_then(serde_json::Value::as_u64),
                method: text("method").unwrap_or_default(),
                url: text("url").unwrap_or_default(),
                body: text("body"),
                answer: text("answer"),
                ended: text("ended").unwrap_or_default(),
                code: text("code"),
                values: text("values"),
                json: json.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("jadpo-7.2d-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a folder");
        root
    }

    /// Scripted answers wait in order and each answers one call of its
    /// operation.
    #[test]
    fn a_scripted_answer_answers_one_call() {
        let root = root("script");
        script(&root, "couriers.pickups.create", "slot_taken").expect("scripted");
        script(&root, "couriers.pickups.get", "lost").expect("scripted");
        script(&root, "couriers.pickups.create", "unavailable").expect("scripted");
        let folder = root.join(SCRIPT);
        assert!(waiting(&folder, "couriers.pickups.create"));
        assert_eq!(
            take(&folder, "couriers.pickups.create").as_deref(),
            Some("slot_taken")
        );
        assert_eq!(
            take(&folder, "couriers.pickups.create").as_deref(),
            Some("unavailable")
        );
        assert_eq!(take(&folder, "couriers.pickups.create"), None);
        assert_eq!(
            scripted(&root),
            [("couriers.pickups.get".to_string(), "lost".to_string())]
        );
        assert_eq!(scripted_answer("lost"), Answer::Unknown);
        assert_eq!(
            scripted_answer("slot_taken"),
            Answer::Failed("slot_taken".into())
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// The log numbers its calls on from the last it holds, and keeps the
    /// last 500.
    #[test]
    fn the_log_keeps_the_last_calls() {
        let root = root("log");
        let mut file = LogFile::open(&root);
        for n in 0..(KEPT + KEPT / 5 + 1) {
            file.keep(
                serde_json::json!({ "connection": "couriers", "operation": format!("op{n}") }),
            );
        }
        let held = read(&root);
        assert_eq!(held.len(), KEPT);
        assert_eq!(held.last().map(|entry| entry.id.as_str()), Some("call_601"));
        let again = LogFile::open(&root);
        assert_eq!(again.next, 602);
        let _ = std::fs::remove_dir_all(root);
    }
}
