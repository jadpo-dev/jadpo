//! `jadpo package preview` (area 11, "Developing for the registry"; chunk
//! R5): serves the package's registry page on this machine, so publishing
//! holds no surprises.
//!
//! The page is the release as the registry would show it: its facts, what
//! changed since the last tag, the publishing bar, the public surface, the
//! changelog and the guide, all from `jadpo package verify`'s bar, which
//! writes nothing. It is read again when the package's files change. What only
//! the registry can add (the model's written account, the Jadpo releases it
//! works with, its contract runs) is named, not imitated. The CLI design
//! language's tokens and symbols are carried into the browser, as the dev
//! inbox's are.
//!
//! ```text
//! jadpo package preview  Postcodes.v2 · nothing is published · ctrl-c stops it
//!
//!   page  http://127.0.0.1:50321/
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jadpo_diag::Diagnostic;
use jadpo_registry::http::{Fetched, Incoming, Server};
use jadpo_std::email::escape_html as esc;
use jadpo_term::{Block, ColorChoice, Environment, Json, Screen, Stream, Style};
use serde_json::Value;

use crate::package::{self, Mode, arguments};
use crate::run::{Call, Reply, Session, finish};

/// What `jadpo package preview` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewArgs {
    pub folder: PathBuf,
    pub json: bool,
    pub choice: ColorChoice,
}

pub(crate) fn preview(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo package preview` takes one folder, not `{extra}`"),
            "It serves the page of the package in one folder.".to_string(),
            "Run `jadpo package preview [<folder>]`".to_string(),
        ));
    }
    let folder = match call.arguments.get(1) {
        Some(argument) => cwd.join(argument),
        None => cwd,
    };
    if !has_package(&folder) {
        return package::no_package("package preview");
    }
    Reply::Session(Session::Preview(Box::new(PreviewArgs {
        folder,
        json: false,
        choice: ColorChoice::Auto,
    })))
}

fn has_package(folder: &Path) -> bool {
    std::fs::read_to_string(folder.join(jadpo_db::PACKAGE_INDEX))
        .ok()
        .and_then(|index| package::index_name(&index))
        .is_some()
}

/// A page being served, stopped when dropped.
#[derive(Debug)]
pub struct Served {
    server: Server,
}

impl Served {
    pub fn url(&self) -> String {
        format!("http://{}/", self.server.address())
    }
}

/// The page last made, and the files' digest it was made from.
type Made = Arc<Mutex<Option<(Option<String>, String)>>>;

/// Serves the page on a port the system picks, on this machine only.
pub fn start(args: &PreviewArgs) -> std::io::Result<Served> {
    let folder = args.folder.clone();
    let made: Made = Arc::new(Mutex::new(None));
    let server = Server::start(Arc::new(move |incoming: Incoming| {
        answer(&folder, &made, &incoming)
    }))?;
    Ok(Served { server })
}

/// Serves until the process is stopped.
pub fn serve(args: &PreviewArgs, env: &Environment) -> u8 {
    let served = match start(args) {
        Ok(served) => served,
        Err(error) => {
            let output = finish(
                Some("package"),
                package::write_failed(
                    "package preview",
                    "a port on this machine",
                    &error.to_string(),
                ),
                env,
                args.json,
                args.choice,
                &crate::run::Asked::default(),
            );
            let _ = std::io::stderr().write_all(output.stderr.as_bytes());
            let _ = std::io::stdout().write_all(output.stdout.as_bytes());
            return output.code;
        }
    };
    let url = served.url();
    let name = std::fs::read_to_string(args.folder.join(jadpo_db::PACKAGE_INDEX))
        .ok()
        .and_then(|index| package::index_name(&index))
        .unwrap_or_default();
    let style = Style::detect(env, args.json, args.choice, Stream::Stdout);
    let started = if args.json {
        Json::object([
            ("package", Json::from(name.as_str())),
            ("url", Json::from(url.as_str())),
        ])
        .to_document()
    } else {
        Screen::new()
            .with(Block::header(
                "jadpo package preview",
                [
                    name,
                    "nothing is published".to_string(),
                    "ctrl-c stops it".to_string(),
                ],
            ))
            .with(Block::fields(vec![("page", url)]))
            .render(&style)
    };
    let _ = std::io::stdout().write_all(started.as_bytes());
    let _ = std::io::stdout().flush();
    loop {
        std::thread::park();
    }
}

fn answer(folder: &Path, made: &Made, incoming: &Incoming) -> Fetched {
    if incoming.method != "GET" && incoming.method != "HEAD" {
        return plain(405, "The preview is read-only.");
    }
    let path = incoming.target.split('?').next().unwrap_or("");
    if path != "/" {
        return plain(404, "The preview is one page.");
    }
    let digest = crate::install::folder_digest(folder);
    let mut held = made.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let html = match held.as_ref() {
        Some((was, html)) if *was == digest && digest.is_some() => html.clone(),
        _ => {
            let html = page(folder);
            *held = Some((digest, html.clone()));
            html
        }
    };
    Fetched {
        status: 200,
        content_type: Some("text/html; charset=utf-8".to_string()),
        body: html.into_bytes(),
    }
}

fn plain(status: u16, text: &str) -> Fetched {
    Fetched {
        status,
        content_type: Some("text/plain; charset=utf-8".to_string()),
        body: format!("{text}\n").into_bytes(),
    }
}

/// The page of the package in `folder` as it stands.
fn page(folder: &Path) -> String {
    let name = std::fs::read_to_string(folder.join(jadpo_db::PACKAGE_INDEX))
        .ok()
        .and_then(|index| package::index_name(&index));
    let Some(name) = name else {
        return document("No package", &absent("There is no package here any more."));
    };
    let tag = std::fs::read_to_string(folder.join(jadpo_db::record::SURFACE_FILE))
        .ok()
        .and_then(|text| crate::install::surface_version(&jadpo_db::record::normalise(&text)))
        .map_or_else(
            || "v999999.0.0".to_string(),
            |version| format!("v{version}"),
        );
    let call = Call {
        arguments: Vec::new(),
        flags: Vec::new(),
        values: Vec::new(),
        cwd: Some(folder.to_path_buf()),
        environment: Environment::empty().with_cwd(folder),
    };
    let reply = package::bar(
        &call,
        folder,
        Mode::Verify {
            previous: crate::package_publish::previous_release(folder, &name, &tag).map(Box::new),
            run: true,
            publisher: None,
        },
    );
    let (data, problems): (Value, Vec<Diagnostic>) = match reply {
        Reply::Report {
            data, diagnostics, ..
        } => (
            serde_json::from_str(&data.to_document()).unwrap_or(Value::Null),
            diagnostics,
        ),
        Reply::Failed(diagnostic) => (Value::Null, vec![*diagnostic]),
        _ => (Value::Null, Vec::new()),
    };
    document(&name, &body(folder, &name, &data, &problems))
}

fn absent(text: &str) -> String {
    format!("<main><p class=\"absent\">{}</p></main>", esc(text))
}

/// The `///` lines above `package` in the index.
fn description(folder: &Path) -> String {
    let index = std::fs::read_to_string(folder.join(jadpo_db::PACKAGE_INDEX)).unwrap_or_default();
    index
        .lines()
        .map_while(|line| line.trim().strip_prefix("///"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" ")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// One fact's row: its value in code, a line for each.
fn fact(label: &str, lines: &[String]) -> String {
    let shown = if lines.is_empty() {
        "<span class=\"muted\">none</span>".to_string()
    } else {
        lines
            .iter()
            .map(|line| format!("<code>{}</code>", esc(line)))
            .collect::<Vec<_>>()
            .join("<br>")
    };
    format!("<dt>{}</dt><dd>{shown}</dd>", esc(label))
}

fn facts_section(facts: &Value) -> String {
    let mut rows = String::new();
    let provider = facts["kind"] == "provider";
    let kind = facts["kind"].as_str().unwrap_or("");
    rows.push_str(&format!(
        "<dt>kind</dt><dd>{}{}</dd>",
        esc(kind),
        if facts["pure"] == true { ", pure" } else { "" }
    ));
    rows.push_str(&format!(
        "<dt>publisher</dt><dd>{}</dd>",
        match facts["publisher"].as_str() {
            Some(publisher) => esc(publisher),
            None => "none, and publishing needs one".to_string(),
        }
    ));
    if provider {
        let hosts: Vec<String> = facts["hosts"]
            .as_array()
            .map(|hosts| {
                hosts
                    .iter()
                    .filter_map(|host| host["host"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        rows.push_str(&fact("hosts", &hosts));
        rows.push_str(&fact("changes", &strings(&facts["changing_calls"])));
        let sent: Vec<String> = facts["personal_data"]
            .as_array()
            .map(|sent| {
                sent.iter()
                    .map(|fact| {
                        format!(
                            "{} to {}",
                            fact["field"].as_str().unwrap_or(""),
                            fact["host"].as_str().unwrap_or("")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        rows.push_str(&fact("personal data", &sent));
        rows.push_str(&format!(
            "<dt>erasure</dt><dd>{}</dd>",
            if facts["erasure"] == true {
                "declared"
            } else {
                "not declared"
            }
        ));
    }
    rows.push_str(&fact("storable", &strings(&facts["storable_types"])));
    let failures: Vec<String> = facts["failures"]
        .as_array()
        .map(|failures| {
            failures
                .iter()
                .map(|fact| {
                    format!(
                        "{}: {}",
                        fact["declaration"].as_str().unwrap_or(""),
                        strings(&fact["failures"]).join(", ")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    rows.push_str(&fact("failures", &failures));
    if provider {
        rows.push_str(&fact("confirmed", &strings(&facts["confirmed_operations"])));
    }
    let standard: Vec<String> = facts["standard_candidates"]
        .as_array()
        .map(|candidates| {
            candidates
                .iter()
                .map(|fact| {
                    format!(
                        "{}: {}",
                        fact["declaration"].as_str().unwrap_or(""),
                        fact["standard"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if !standard.is_empty() {
        rows.push_str(&fact("standard", &standard));
    }
    format!("<section><h2>Facts</h2><dl class=\"facts\">{rows}</dl></section>")
}

/// `new_host` as a screen says it.
fn change_word(change: &str) -> String {
    match change {
        "personal_data_sent" => "personal data".to_string(),
        other => other.replace('_', " "),
    }
}

fn changes_section(data: &Value) -> String {
    let Some(record) = data["change_record"].as_array() else {
        return String::new();
    };
    let since = data["previous"]["since"].as_str();
    let heading = match since {
        Some(since) => format!("Changes since {}", esc(since)),
        None => "Changes".to_string(),
    };
    if record.is_empty() {
        let said = if !data["facts"].is_object() {
            "They come once the package compiles."
        } else if since.is_some() {
            "Nothing that changes what an app sees."
        } else {
            "The first release."
        };
        return format!("<section><h2>{heading}</h2><p class=\"muted\">{said}</p></section>");
    }
    let rows: String = record
        .iter()
        .map(|entry| {
            let change = entry["change"].as_str().unwrap_or("");
            let (mark, class) = match change {
                "added" | "new_host" | "personal_data_sent" => ("+", "added"),
                "removed" => ("-", "removed"),
                _ => ("~", "changed"),
            };
            let detail = entry["detail"].as_str().unwrap_or("");
            let why = entry["why"]
                .as_str()
                .map(|why| format!("<p class=\"why\">{}</p>", esc(why)))
                .unwrap_or_default();
            format!(
                "<tr><td class=\"mark {class}\">{mark}</td><td>{}</td><td><code>{}</code>{why}</td><td class=\"muted\">{}</td></tr>",
                esc(&change_word(change)),
                esc(entry["declaration"].as_str().unwrap_or("")),
                esc(detail)
            )
        })
        .collect();
    format!(
        "<section><h2>{heading}</h2><table class=\"changes\"><tbody>{rows}</tbody></table></section>"
    )
}

fn bar_section(data: &Value) -> String {
    let rows: String = data["bar"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|row| {
            let (symbol, word, class) = match row["state"].as_str().unwrap_or("") {
                "passed" => ("✓", "passed", "passed"),
                "failed" => ("✗", "failed", "failed"),
                _ => ("–", "not checked", "later"),
            };
            format!(
                "<tr class=\"{class}\"><td class=\"state\"><span aria-hidden=\"true\">{symbol}</span> {word}</td><th scope=\"row\">{}</th><td>{}</td></tr>",
                esc(row["item"].as_str().unwrap_or("")),
                esc(row["detail"].as_str().unwrap_or(""))
            )
        })
        .collect();
    format!(
        "<section><h2>Publishing bar</h2><table class=\"bar-rows\"><tbody>{rows}</tbody></table></section>"
    )
}

fn problems_section(problems: &[Diagnostic]) -> String {
    if problems.is_empty() {
        return String::new();
    }
    let items: String = problems
        .iter()
        .map(|problem| {
            let place = problem
                .primary
                .as_ref()
                .map(|label| format!("<code>{}</code> ", esc(&label.span.path)))
                .unwrap_or_default();
            format!(
                "<li>{place}{} <span class=\"muted\">{}</span></li>",
                code_spans(&problem.message),
                esc(problem.rule.id)
            )
        })
        .collect();
    format!("<section><h2>What stops it</h2><ul class=\"problems\">{items}</ul></section>")
}

/// Text with its backticked names as code.
fn code_spans(line: &str) -> String {
    let mut out = String::new();
    for (at, part) in line.split('`').enumerate() {
        if at % 2 == 1 {
            out.push_str(&format!("<code>{}</code>", esc(part)));
        } else {
            out.push_str(&esc(part));
        }
    }
    out
}

/// A package's Markdown as the registry would read it: headings, lists,
/// paragraphs and code spans, everything else as text.
fn markdown(text: &str) -> String {
    let spans = code_spans;
    let mut out = String::new();
    let mut in_list = false;
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_list {
                out.push_str("</ul>");
                in_list = false;
            }
            out.push_str(if in_code { "</pre>" } else { "<pre>" });
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push_str(&esc(line));
            out.push('\n');
            continue;
        }
        let item = line.strip_prefix("- ");
        if in_list && item.is_none() {
            out.push_str("</ul>");
            in_list = false;
        }
        if let Some(item) = item {
            if !in_list {
                out.push_str("<ul>");
                in_list = true;
            }
            out.push_str(&format!("<li>{}</li>", spans(item)));
        } else if let Some(heading) = line.strip_prefix('#') {
            let heading = heading.trim_start_matches('#').trim();
            out.push_str(&format!("<h3>{}</h3>", spans(heading)));
        } else if !line.trim().is_empty() {
            out.push_str(&format!("<p>{}</p>", spans(line)));
        }
    }
    if in_list {
        out.push_str("</ul>");
    }
    if in_code {
        out.push_str("</pre>");
    }
    out
}

fn body(folder: &Path, name: &str, data: &Value, problems: &[Diagnostic]) -> String {
    let version = data["version"].as_str();
    let failed = data["bar"].as_array().map_or(0, |bar| {
        bar.iter().filter(|row| row["state"] == "failed").count()
    });
    let (verdict, next) = if failed == 0 && problems.is_empty() {
        (
            "<span class=\"passed\">✓</span> It passes the bar here.".to_string(),
            "<code>jadpo package publish --dry-run</code> runs the registry's verifier on the pushed tag.".to_string(),
        )
    } else {
        (
            format!(
                "<span class=\"failed\">✗</span> The registry would refuse it as it stands: {}.",
                if failed == 1 {
                    "1 row of the bar fails".to_string()
                } else if failed > 1 {
                    format!("{failed} rows of the bar fail")
                } else {
                    "it can't be checked".to_string()
                }
            ),
            "<code>jadpo package check</code> says what to change, and <code>jadpo package publish --dry-run</code> runs the registry's verifier once it passes.".to_string(),
        )
    };
    let lede = description(folder);
    let mut main = format!(
        "<header class=\"title\"><h1><code>{}</code>{}</h1>{}<p class=\"verdict\">{verdict}</p><p class=\"next\">{next}</p></header>",
        esc(name),
        version
            .map(|version| format!(" <span class=\"version\">{}</span>", esc(version)))
            .unwrap_or_default(),
        if lede.is_empty() {
            String::new()
        } else {
            format!("<p class=\"lede\">{}</p>", esc(&lede))
        },
    );
    main.push_str(&problems_section(problems));
    if data["facts"].is_object() {
        main.push_str(&facts_section(&data["facts"]));
    } else {
        main.push_str(
            "<section><h2>Facts</h2><p class=\"muted\">They come once the package compiles.</p></section>",
        );
    }
    main.push_str(&changes_section(data));
    main.push_str(&bar_section(data));
    let surface = data["surface"]
        .as_str()
        .map(str::to_string)
        .or_else(|| std::fs::read_to_string(folder.join(jadpo_db::record::SURFACE_FILE)).ok());
    main.push_str(&match surface {
        Some(surface) => format!(
            "<section><h2>Public surface</h2><pre class=\"surface\">{}</pre></section>",
            esc(&surface)
        ),
        None => "<section><h2>Public surface</h2><p class=\"muted\">It comes once the package compiles.</p></section>".to_string(),
    });
    for (heading, file) in [("Changelog", "CHANGELOG.md"), ("Guide", "GUIDE.md")] {
        main.push_str(&match std::fs::read_to_string(folder.join(file)) {
            Ok(text) => format!(
                "<section><h2>{heading}</h2><div class=\"prose\">{}</div></section>",
                markdown(&text)
            ),
            Err(_) => format!(
                "<section><h2>{heading}</h2><p class=\"muted\">It has no <code>{file}</code>.</p></section>"
            ),
        });
    }
    main.push_str(
        "<section class=\"later\"><h2>What the registry adds</h2><p>At release it writes a model's account of the package, marked as one, and adds whether its publisher is verified, the Jadpo releases it works with and the last contract run. Those can't be shown from here.</p></section>",
    );
    format!("<main>{main}</main>")
}

fn document(title: &str, main: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en-GB\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"light dark\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\"><title>{} · jadpo package preview</title><style>{STYLE}</style></head><body><div class=\"bar\"><strong>Preview</strong><span>Nothing is published. This is what a release from this folder would show, read again when its files change.</span></div>{main}</body></html>\n",
        esc(title)
    )
}

const STYLE: &str = r#"
:root{color-scheme:light dark;--bg:#ffffff;--panel:#f4f4f5;--line:#e2e3e6;--text:#1d1f23;--muted:#5d6470;--accent:#0b649f;--success:#0f7a55;--warning:#8b5400;--failure:#c3262f;--select:#e6f0f7;--code:#eceef1;
--sans:system-ui,-apple-system,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;--mono:ui-monospace,SFMono-Regular,Menlo,Consolas,"Liberation Mono",monospace}
@media (prefers-color-scheme:dark){:root{--bg:#1e1e1e;--panel:#252628;--line:#34363a;--text:#e4e5e7;--muted:#969da8;--accent:#5cc3f0;--success:#4fd69c;--warning:#ebb54a;--failure:#ff7b70;--select:#1f3340;--code:#2c2e31}}
*{box-sizing:border-box}
html{scrollbar-color:var(--line) transparent}
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.55 var(--sans);-webkit-font-smoothing:antialiased}
::selection{background:var(--select)}
.bar{display:flex;flex-wrap:wrap;gap:4px 16px;align-items:baseline;padding:10px 24px;background:var(--panel);border-bottom:1px solid var(--line);color:var(--muted);font-size:13px}
.bar strong{color:var(--text);font-weight:650}
main{max-width:76ch;margin:0 auto;padding:40px 24px 72px}
.title{margin-bottom:8px}
h1{font-size:26px;line-height:1.25;font-weight:650;margin:0 0 12px;letter-spacing:-0.02em;text-wrap:balance}
h1 code{font-size:inherit;background:none;padding:0;font-weight:650;letter-spacing:-0.02em}
.version{font-family:var(--mono);font-size:17px;font-weight:400;color:var(--muted);letter-spacing:0;margin-left:6px;font-variant-numeric:tabular-nums}
.lede{font-size:17px;line-height:1.5;margin:0 0 20px}
.verdict{margin:0 0 2px;font-weight:600}
.next{margin:0;color:var(--muted)}
span.passed,tr.passed .state{color:var(--success)}
span.failed,tr.failed .state{color:var(--failure)}
tr.later .state,.muted{color:var(--muted)}
section{margin-top:44px}
h2{font-size:16px;line-height:1.3;font-weight:650;margin:0 0 12px;padding-bottom:8px;border-bottom:1px solid var(--line)}
h3{font-size:15px;font-weight:650;margin:22px 0 6px}
h3:first-child{margin-top:0}
code,pre{font-family:var(--mono);font-size:13px;font-variant-numeric:tabular-nums}
code{background:var(--code);padding:1px 5px;border-radius:4px;overflow-wrap:anywhere}
pre{margin:0;padding:14px 16px;background:var(--panel);border-radius:8px;overflow:auto;line-height:1.5}
.facts{display:grid;grid-template-columns:max-content minmax(0,1fr);gap:8px 24px;margin:0}
.facts dt{color:var(--muted)}
.facts dd{margin:0;min-width:0}
.facts dd code{display:inline-block;margin-bottom:3px}
table{border-collapse:collapse;width:100%}
td,th{text-align:left;font-weight:400;vertical-align:baseline;padding:7px 16px 7px 0;border-bottom:1px solid var(--line)}
td:last-child,th:last-child{padding-right:0}
tr:last-child>td,tr:last-child>th{border-bottom:0}
.bar-rows .state{white-space:nowrap;width:1%}
.bar-rows th{font-weight:600;white-space:nowrap;width:1%}
.changes .mark{font-family:var(--mono);width:1%}
.mark.added{color:var(--success)}
.mark.removed{color:var(--failure)}
.mark.changed{color:var(--warning)}
.why{margin:4px 0 0;color:var(--muted)}
.problems{margin:0;padding:0 0 0 18px}
.problems li{margin-bottom:6px}
.prose p{margin:0 0 10px}
.prose ul{margin:0 0 10px;padding-left:20px}
.prose pre{margin:0 0 10px}
.absent{color:var(--muted)}
section.later p{margin:0;color:var(--muted)}
@media (max-width:560px){main{padding:24px 16px 48px}.bar{padding:10px 16px}.facts{grid-template-columns:minmax(0,1fr);gap:2px 0}.facts dd{margin-bottom:10px}h1{font-size:22px}}
"#;
