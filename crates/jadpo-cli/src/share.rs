//! `jadpo suggest share` (area 14, "Sharing with Jadpo HQ"; chunk 5.5y):
//! sends the pending entries' hashes to Jadpo HQ at once, shows exactly
//! what it would send of the snippets HQ asks for, and sends them only
//! with `--approve`, after a person said yes.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, matching `jadpo suggest ignore` and `jadpo migrate
//! new`'s decisions, run unattended, so its assumptions are marked):
//!
//! - *Job.* At the end of a suggest session the agent sends what was
//!   learned without asking anyone, and a person decides whether any code
//!   leaves the project. Mode: a report, then a decision. The agent reads
//!   every line and relays the decision; the person may only ever see what
//!   the agent relays, so the decision has to stand on its own.
//! - What happened first: one status line, `✓ Sent 3 hashes to Jadpo HQ`,
//!   with rows for the kinds sent, what HQ asked for, and a new token
//!   (with why it is committed). Then, when HQ asks for snippets, the
//!   bold heading ``What `jadpo suggest share --approve` sends`` and the
//!   request exactly as it will go, address and body, so nothing is
//!   summarised between what a person approves and what is sent
//!   (assumption: the payload alone, not a separate list of snippets,
//!   since the pretty-printed body is already readable and a second form
//!   could disagree with it). Last, the `◆ decision` with its `next`
//!   addressed to the agent: ask first, then one of two whole commands.
//! - The first share from a checkout adds one line under the bold heading
//!   `Sharing is on`, saying only hashes went and how to turn it off
//!   (assumption: per checkout, kept in `.jadpo/`, since nothing else is
//!   written outside the project).
//! - *States.* Hashes sent and nothing asked; snippets asked; asked but
//!   unattended (the row says skipped, no decision); nothing pending
//!   (`✓ Nothing to share`, nothing sent); sharing off (`✓ Sent nothing:
//!   sharing is turned off` and where, a success, since it did what it
//!   was told); a problem in `suggestions { }` (stopped on the
//!   diagnostics); HQ unreachable or refusing (`suggest.share_failed`,
//!   the entries kept). `--approve` is `✓ Sent 1 snippet to Jadpo HQ`;
//!   `--decline` is `✓ Sent nothing: dropped …`; either with nothing
//!   waiting is `cli.suggest_arguments`.
//! - `--json` carries the same, and an `approval` object an agent relays:
//!   the question, the payload, both commands and the rule.

use std::path::Path;

use jadpo_check::suggest::settings::{self, Settings};
use jadpo_check::suggest::{hash, without_comments};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_term::{Block, Json, Outcome, Screen, shown_path};
use serde_json::Value;

use crate::hq::{self, Address, Failure};
use crate::migrate::blocked;
use crate::run::{Call, Reply};
use crate::suggestions::{arguments, count, read_lines, text, write_failed};

/// The flags only `share` takes.
pub const FLAGS: [&str; 4] = ["--approve", "--decline", "--new-token", "--unattended"];

/// The variable that turns sharing off for a machine.
pub const SWITCH: &str = "JADPO_SUGGEST_SHARE";

const PENDING: &str = ".jadpo/suggest_pending";
const WANTED: &str = ".jadpo/suggest_wanted";
const SHARED: &str = ".jadpo/suggest_shared";

const HASHES: &str = "/v1/suggestions/hashes";
const SNIPPETS: &str = "/v1/suggestions/snippets";

/// The most entries one hashes request carries.
const BATCH: usize = 500;

const APPROVE: &str = "jadpo suggest share --approve";
const DECLINE: &str = "jadpo suggest share --decline";

const KINDS: [&str; 3] = ["accepted", "found", "ignored"];

/// Where sharing was turned off, for the screen, when it was.
pub fn turned_off(
    call: &Call,
    settings: &Settings,
    root: &Path,
    shown_root: Option<&str>,
) -> Option<String> {
    if let Some(value) = call.environment.var(SWITCH)
        && !value.is_empty()
        && value != "on"
    {
        return Some(format!("{SWITCH}={value} in the environment"));
    }
    match &settings.share {
        Some((false, span)) => Some(format!(
            "{}:{}  share: false",
            shown_path(shown_root, &span.path),
            line_of(root, span)
        )),
        _ => None,
    }
}

fn line_of(root: &Path, span: &Span) -> usize {
    std::fs::read_to_string(root.join(&span.path))
        .map(|text| {
            text.get(..span.start as usize)
                .unwrap_or("")
                .matches('\n')
                .count()
                + 1
        })
        .unwrap_or(1)
}

fn plural(n: usize, one: &str) -> String {
    count(n, one, &format!("{one}s"))
}

fn failed(message: impl Into<String>, reason: impl Into<String>, next: impl Into<String>) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::SUGGEST_SHARE_FAILED, message)
            .reason(reason)
            .next(next),
    )
}

/// A failed exchange as the command's error; `kept` says what waits.
fn exchange_failed(failure: Failure, kept: &str) -> Reply {
    let next = "Run the command again later; nothing was lost";
    match failure {
        Failure::Unreachable(why) => failed(
            "Jadpo HQ couldn't be reached",
            format!("{why} {kept}"),
            next,
        ),
        Failure::Refused {
            status,
            code,
            message,
        } => {
            let said = if message.is_empty() {
                String::new()
            } else {
                format!(": {message}")
            };
            let code = if code.is_empty() {
                String::new()
            } else {
                format!(" `{code}`")
            };
            failed(
                format!("Jadpo HQ refused what was shared ({status}{code})"),
                format!("HQ answered{said}. {kept}"),
                if status == 410 {
                    "Update Jadpo, then run the command again"
                } else {
                    next
                },
            )
        }
        Failure::Garbled(why) => failed(
            "Jadpo HQ's answer couldn't be read",
            format!("{why} {kept}"),
            next,
        ),
    }
}

fn address_of(call: &Call) -> Result<Address, Box<Reply>> {
    let written = call
        .environment
        .var(hq::VARIABLE)
        .filter(|v| !v.is_empty())
        .unwrap_or(hq::DEFAULT);
    hq::address(written).map_err(|why| {
        Box::new(failed(
            format!("`{}` can't be used", hq::VARIABLE),
            format!("{why} Nothing was sent, and the entries wait."),
            format!(
                "Set `{}` to HQ's `https://` address, or unset it for the default",
                hq::VARIABLE
            ),
        ))
    })
}

pub fn share(call: &Call, cwd: &Path, root: &Path) -> Reply {
    let approve = call.flag("--approve");
    let decline = call.flag("--decline");
    let new_token = call.flag("--new-token");
    let chosen: Vec<&str> = [
        ("--approve", approve),
        ("--decline", decline),
        ("--new-token", new_token),
    ]
    .into_iter()
    .filter_map(|(flag, on)| on.then_some(flag))
    .collect();
    if chosen.len() > 1 {
        return arguments(
            format!("`{}` and `{}` can't be given at once", chosen[0], chosen[1]),
            "`--approve` sends the snippets HQ asked for, `--decline` drops them, and `--new-token` replaces the token before sharing.",
            "Run one of them at a time",
        );
    }
    let unattended =
        call.flag("--unattended") || call.environment.var("CI").is_some_and(|v| !v.is_empty());
    if approve && unattended {
        return arguments(
            "An unattended run can't approve",
            "`--approve` sends code, and only a person who has seen the payload can say yes; an unattended run (`--unattended`, or `CI` set) sends only hashes.",
            "Ask the person, then run `jadpo suggest share --approve` where they can see it",
        );
    }
    let shown_root = jadpo_db::shown_root(cwd, root);
    // The environment turns sharing off before anything is read.
    if let Some(place) = turned_off(call, &Settings::default(), root, None) {
        return off(place);
    }
    let loaded = jadpo_db::load(root);
    let syntax: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|d| {
            d.rule.id.starts_with("syntax.") && !d.rule.id.starts_with("syntax.suggestions_")
        })
        .collect();
    if !syntax.is_empty() {
        return blocked(&loaded, cwd, syntax, "share stopped");
    }
    let settings = settings::settings(&loaded.db, loaded.project).clone();
    if let Some(place) = turned_off(call, &settings, root, shown_root.as_deref()) {
        return off(place);
    }
    if !settings.problems.is_empty() {
        return blocked(&loaded, cwd, settings.problems.clone(), "share stopped");
    }
    if approve {
        return send_snippets(root);
    }
    if decline {
        return drop_snippets(root);
    }
    send_hashes(call, root, &settings, new_token, unattended)
}

/// Sharing is turned off: nothing is sent or written.
fn off(place: String) -> Reply {
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: "Sent nothing: sharing is turned off".to_string(),
        rows: vec![("where".to_string(), place.clone())],
    });
    let data = Json::object([
        ("sent", 0usize.into()),
        ("turnedOff", Json::from(place.as_str())),
    ]);
    Reply::screen(screen, data)
}

/// One pending entry.
struct Entry {
    hash: String,
    kind: String,
    signal: String,
    suggestion: String,
    text: String,
    reason: String,
}

fn pending(root: &Path) -> Vec<Entry> {
    read_lines(root, PENDING)
        .iter()
        .map(|object| Entry {
            hash: text(object, "textHash"),
            kind: text(object, "kind"),
            signal: text(object, "signal"),
            suggestion: text(object, "suggestion"),
            text: text(object, "text"),
            reason: text(object, "reason"),
        })
        .filter(|entry| {
            entry.hash.len() == 16
                && KINDS.contains(&entry.kind.as_str())
                && !entry.signal.is_empty()
                && !entry.suggestion.is_empty()
        })
        .collect()
}

/// A new token: 32 hex digits from the operating system's generator.
fn new_token() -> Option<String> {
    let mut bytes = [0u8; settings::TOKEN_DIGITS / 2];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Writes `token` into `suggestions { }`, or a new block at the end of
/// `app.jadpo`; the file it went into.
fn write_token(root: &Path, settings: &Settings, token: &str) -> Result<String, Box<Diagnostic>> {
    let literal = format!("\"{token}\"");
    let file = match (&settings.token, &settings.keyword) {
        (Some((_, span)), _) => span.path.clone(),
        (None, Some(keyword)) => keyword.path.clone(),
        _ => "app.jadpo".to_string(),
    };
    let path = root.join(&file);
    let text = std::fs::read_to_string(&path).map_err(|error| write_failed(&file, &error))?;
    let edited = match (&settings.token, settings.open) {
        // Replaced where it is.
        (Some((_, span)), _) => format!(
            "{}{literal}{}",
            &text[..span.start as usize],
            &text[span.end as usize..]
        ),
        // The block's first member; `suggestions {}` gains a line for its
        // closing brace.
        (None, Some(open)) if settings.keyword.is_some() => {
            let (before, after) = text.split_at(open as usize);
            let (close, after) = if after.trim_start().starts_with('}') {
                ("\n", after.trim_start())
            } else {
                ("", after)
            };
            format!("{before}\n    project_token: {literal}{close}{after}")
        }
        // A new block at the end of the file.
        _ => {
            let mut out = text.trim_end().to_string();
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&format!(
                "suggestions {{\n    project_token: {literal}\n}}\n"
            ));
            out
        }
    };
    std::fs::write(&path, edited).map_err(|error| write_failed(&file, &error))?;
    Ok(file)
}

fn local_write(root: &Path, file: &str, text: &str) -> Result<(), Box<Diagnostic>> {
    crate::suggestions::local_folder(root)
        .and_then(|()| std::fs::write(root.join(file), text))
        .map_err(|error| write_failed(file, &error))
}

fn send_hashes(
    call: &Call,
    root: &Path,
    settings: &Settings,
    rotate: bool,
    unattended: bool,
) -> Reply {
    let entries = pending(root);
    if entries.is_empty() && !rotate {
        let screen = Screen::new().with(Block::Status {
            outcome: Outcome::Success,
            text: "Nothing to share".to_string(),
            rows: Vec::new(),
        });
        return Reply::screen(screen, Json::object([("sent", 0usize.into())]));
    }
    let address = match address_of(call) {
        Ok(address) => address,
        Err(reply) => return *reply,
    };

    // The token: written when missing, replaced when asked.
    let mut token_file = None;
    let token = match (&settings.token, rotate) {
        (Some((token, _)), false) => token.clone(),
        _ => {
            let Some(token) = new_token() else {
                return failed(
                    "A project token couldn't be made",
                    "The operating system's random number generator didn't answer. Nothing was sent, and the entries wait.",
                    "Run the command again",
                );
            };
            match write_token(root, settings, &token) {
                Ok(file) => token_file = Some(file),
                Err(problem) => return Reply::failed(*problem),
            }
            token
        }
    };
    if entries.is_empty() {
        let file = token_file.unwrap_or_default();
        let screen = Screen::new().with(Block::Status {
            outcome: Outcome::Success,
            text: format!("Wrote a new project token to {file}"),
            rows: vec![("sent".to_string(), "nothing: no entries wait".to_string())],
        });
        return Reply::screen(
            screen,
            Json::object([("sent", 0usize.into()), ("tokenWritten", true.into())]),
        );
    }

    // The hashes, each entry once.
    let mut sent: Vec<(&str, &str, &str, &str)> = Vec::new();
    for entry in &entries {
        let key = (
            entry.hash.as_str(),
            entry.kind.as_str(),
            entry.signal.as_str(),
            entry.suggestion.as_str(),
        );
        if !sent.contains(&key) {
            sent.push(key);
        }
    }
    let mut asked: Vec<(String, String)> = Vec::new();
    for batch in sent.chunks(BATCH) {
        let body = serde_json::json!({
            "token": token,
            "entries": batch.iter().map(|(hash, kind, signal, suggestion)| serde_json::json!({
                "hash": hash,
                "kind": kind,
                "signal": signal,
                "suggestion": suggestion,
            })).collect::<Vec<_>>(),
        });
        let answer = match hq::post(&address, HASHES, &body.to_string()) {
            Ok(answer) => answer,
            Err(failure) => {
                return exchange_failed(failure, &format!("Every entry waits in `{PENDING}`."));
            }
        };
        // Only what was sent can be asked for.
        for wanted in answer
            .get("wanted")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let field = |name: &str| wanted.get(name).and_then(Value::as_str).unwrap_or("");
            let pair = (field("hash").to_string(), field("kind").to_string());
            if batch
                .iter()
                .any(|(h, k, _, _)| *h == pair.0 && *k == pair.1)
                && !asked.contains(&pair)
            {
                asked.push(pair);
            }
        }
    }

    // The snippets HQ asked for, exactly as `--approve` would send them.
    let mut snippets: Vec<Value> = Vec::new();
    for (wanted_hash, wanted_kind) in &asked {
        for entry in entries
            .iter()
            .filter(|e| e.hash == *wanted_hash && e.kind == *wanted_kind)
        {
            let text = without_comments(&entry.text);
            if hash(&text) != entry.hash {
                continue;
            }
            let mut snippet = serde_json::Map::new();
            snippet.insert("kind".into(), Value::from(entry.kind.as_str()));
            if entry.kind == "ignored" && !entry.reason.is_empty() {
                snippet.insert("reason".into(), Value::from(entry.reason.as_str()));
            }
            snippet.insert("signal".into(), Value::from(entry.signal.as_str()));
            snippet.insert("suggestion".into(), Value::from(entry.suggestion.as_str()));
            snippet.insert("text".into(), Value::from(text));
            let snippet = Value::Object(snippet);
            if !snippets.contains(&snippet) {
                snippets.push(snippet);
            }
        }
    }
    let payload = serde_json::json!({ "snippets": snippets });
    let body = serde_json::to_string_pretty(&payload).unwrap_or_default();
    let url = address.url(SNIPPETS);

    // Afterwards: the pending file is wiped, wanted or not; what waits for
    // approval replaces anything an earlier share left.
    let _ = std::fs::remove_file(root.join(PENDING));
    let _ = std::fs::remove_file(root.join(WANTED));
    let ask = !snippets.is_empty() && !unattended;
    if ask {
        let wanted = serde_json::json!({ "url": url, "count": snippets.len(), "body": body });
        if let Err(problem) = local_write(root, WANTED, &format!("{wanted}\n")) {
            return Reply::failed(*problem);
        }
    }
    let first = !root.join(SHARED).exists();
    if first && let Err(problem) = local_write(root, SHARED, "shared\n") {
        return Reply::failed(*problem);
    }

    // The screen.
    let kinds: Vec<(&str, usize)> = KINDS
        .iter()
        .map(|kind| (*kind, sent.iter().filter(|(_, k, _, _)| k == kind).count()))
        .collect();
    let mut rows = vec![(
        "kinds".to_string(),
        kinds
            .iter()
            .filter(|(_, n)| *n > 0)
            .map(|(kind, n)| format!("{n} {kind}"))
            .collect::<Vec<_>>()
            .join(", "),
    )];
    let wanted_count = plural(snippets.len(), "snippet");
    rows.push((
        "asked".to_string(),
        if snippets.is_empty() {
            "nothing more".to_string()
        } else if unattended {
            format!("{wanted_count}, skipped: an unattended run sends only hashes")
        } else {
            wanted_count.clone()
        },
    ));
    if let Some(file) = &token_file {
        rows.push((
            "token".to_string(),
            format!("new, in {file}: commit it so every copy counts as one project"),
        ));
    }
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Sent {} to Jadpo HQ", count(sent.len(), "hash", "hashes")),
        rows,
    });
    if ask {
        let mut lines = vec![format!("POST {url}")];
        lines.extend(body.lines().map(str::to_string));
        screen.push(Block::Section {
            heading: format!("What `{APPROVE}` sends"),
            lines,
        });
    }
    if first {
        screen.push(Block::Section {
            heading: "Sharing is on".to_string(),
            lines: vec![
                "Only hashes went. To stop sharing, add `share: false` to `suggestions { }`."
                    .to_string(),
            ],
        });
    }
    let them = if snippets.len() == 1 { "it" } else { "them" };
    let subject = format!("Jadpo HQ asks for {wanted_count} of this project's code");
    let next = format!(
        "Ask the person whether to send {them}, showing them the payload above. Run `{APPROVE}` only after they say yes; otherwise run `{DECLINE}`."
    );
    if ask {
        screen.push(Block::Decision {
            subject: subject.clone(),
            location: None,
            next: next.clone(),
        });
    }
    let approval = if ask {
        Json::object([
            (
                "question",
                Json::from(format!("{subject}. Send {them}?").as_str()),
            ),
            ("url", Json::from(url.as_str())),
            ("payload", json_of(&payload)),
            ("approve", Json::from(APPROVE)),
            ("decline", Json::from(DECLINE)),
            (
                "rule",
                Json::from(
                    format!("Run `{APPROVE}` only after the person said yes; the command can't check that they did. Otherwise run `{DECLINE}`.")
                        .as_str(),
                ),
            ),
        ])
    } else {
        Json::Null
    };
    let data = Json::object([
        ("sent", sent.len().into()),
        (
            "kinds",
            Json::object(kinds.iter().map(|(kind, n)| (*kind, Json::from(*n)))),
        ),
        (
            "asked",
            Json::array(asked.iter().map(|(hash, kind)| {
                Json::object([
                    ("hash", Json::from(hash.as_str())),
                    ("kind", Json::from(kind.as_str())),
                ])
            })),
        ),
        ("skipped", (unattended && !snippets.is_empty()).into()),
        ("tokenWritten", token_file.is_some().into()),
        ("firstShare", first.into()),
        ("approval", approval),
    ]);
    Reply::screen(screen, data)
}

/// A JSON value in the envelope's form, its keys in their order.
fn json_of(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Number(n) => n.as_i64().map_or(Json::Null, Json::Int),
        Value::String(s) => Json::from(s.as_str()),
        Value::Array(items) => Json::Array(items.iter().map(json_of).collect()),
        Value::Object(map) => Json::object(map.iter().map(|(k, v)| (k.as_str(), json_of(v)))),
    }
}

/// What waits for approval: the address, how many snippets, the body.
fn waiting(root: &Path) -> Option<(String, usize, String)> {
    let text = std::fs::read_to_string(root.join(WANTED)).ok()?;
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    Some((
        value.get("url")?.as_str()?.to_string(),
        value.get("count")?.as_u64()? as usize,
        value.get("body")?.as_str()?.to_string(),
    ))
}

fn nothing_waiting(flag: &str) -> Reply {
    arguments(
        format!("Nothing waits for `{flag}`"),
        "`--approve` and `--decline` answer the snippets Jadpo HQ asked for at the last `jadpo suggest share`, and none are waiting.",
        "Run `jadpo suggest share`",
    )
}

fn send_snippets(root: &Path) -> Reply {
    let Some((url, n, body)) = waiting(root) else {
        return nothing_waiting("--approve");
    };
    let Some(base) = url.strip_suffix(SNIPPETS) else {
        return nothing_waiting("--approve");
    };
    let address = match hq::address(base) {
        Ok(address) => address,
        Err(why) => {
            return failed(
                "The address the snippets were shown for can't be used",
                format!("{why} Nothing was sent."),
                "Run `jadpo suggest share --decline`, then `jadpo suggest share`",
            );
        }
    };
    let answer = match hq::post(&address, SNIPPETS, &body) {
        Ok(answer) => answer,
        Err(failure) => {
            return exchange_failed(
                failure,
                "The snippets still wait for `--approve`, unchanged.",
            );
        }
    };
    let _ = std::fs::remove_file(root.join(WANTED));
    let refused = answer.get("refused").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut rows = Vec::new();
    if refused > 0 {
        rows.push((
            "refused".to_string(),
            format!("{refused}, no longer wanted by HQ"),
        ));
    }
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Sent {} to Jadpo HQ", plural(n, "snippet")),
        rows,
    });
    Reply::screen(
        screen,
        Json::object([("sent", n.into()), ("refused", refused.into())]),
    )
}

fn drop_snippets(root: &Path) -> Reply {
    let Some((_, n, _)) = waiting(root) else {
        return nothing_waiting("--decline");
    };
    if let Err(error) = std::fs::remove_file(root.join(WANTED)) {
        return Reply::failed(*write_failed(WANTED, &error));
    }
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!(
            "Sent nothing: dropped the {} Jadpo HQ asked for",
            plural(n, "snippet")
        ),
        rows: Vec::new(),
    });
    Reply::screen(
        screen,
        Json::object([("sent", 0usize.into()), ("dropped", n.into())]),
    )
}
