//! `jadpo email` (area 16, "Email"; chunk 7.3c2): the dev inbox from the
//! command line, so an agent can follow a sign-up or sign-in flow end to
//! end, and the sending domain's records.
//!
//! - `jadpo email` (or `list`) lists the mail `jadpo dev` caught, oldest
//!   first, filtered by `--to`: each mail's id, its email, its address, its
//!   subject and its health.
//! - `jadpo email show <id>` prints one: who it is from and to, its text
//!   (`--html` for its HTML), its links numbered, its attachments, and
//!   what Jadpo's health checks found, each with how to fix it, then
//!   SpamAssassin's rules when it was run on the mail (chunk 7.3c3), or
//!   why it wasn't.
//! - `jadpo email links <id>` lists its links, numbered; `jadpo email open
//!   <id> <link>` opens one, by its number or written out, in the browser
//!   (`$BROWSER`, else the system's).
//! - `jadpo email clear` empties the inbox.
//! - `jadpo email check-domain [<domain>]` reads the sending domain's SPF,
//!   DKIM (with `--selector`) and DMARC records and says what each needs.
//!   Its lookups go through [`Resolver`], which its tests fake.
//!
//! Every screen that shows a health score says what it can't do
//! ([`jadpo_std::email_health::DISCLAIMER`]).

use std::path::{Path, PathBuf};

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::email_health::{self, Check, DISCLAIMER};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table};

use crate::dev_inbox::{self, Mail};
use crate::run::{Call, Reply};
use crate::spam::Spam;

pub(crate) const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "list",
        description: "List the mail jadpo dev caught",
    },
    crate::table::Value {
        name: "show",
        description: "Show one mail, its links and its health",
    },
    crate::table::Value {
        name: "links",
        description: "List one mail's links, numbered",
    },
    crate::table::Value {
        name: "open",
        description: "Open one of a mail's links in the browser",
    },
    crate::table::Value {
        name: "clear",
        description: "Empty the inbox",
    },
    crate::table::Value {
        name: "check-domain",
        description: "Check the sending domain's SPF, DKIM and DMARC",
    },
];

pub fn email(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_needed("email");
    };
    let arguments = &call.arguments;
    let one_id = |action: &str| -> Result<Mail, Reply> {
        match arguments.get(1) {
            Some(id) if arguments.len() == 2 => find(&root, id),
            _ => Err(Reply::failed(problem(
                format!("`jadpo email {action}` takes one mail's id"),
                "Each mail has an id, `mail_3`, which `jadpo email` lists.".to_string(),
                format!("Run `jadpo email {action} <id>` with an id `jadpo email` lists"),
            ))),
        }
    };
    match arguments.first().map(String::as_str) {
        None | Some("list") => {
            if arguments.len() > 1 {
                return Reply::failed(problem(
                    "`jadpo email` lists the mail and takes nothing more".to_string(),
                    format!("It was given `{}`.", arguments[1..].join(" ")),
                    "Filter with `--to <address>`".to_string(),
                ));
            }
            list(&root, call.value("--to"))
        }
        Some("show") => match one_id("show") {
            Ok(mail) => show(&mail, call.flag("--html")),
            Err(reply) => reply,
        },
        Some("links") => match one_id("links") {
            Ok(mail) => links(&mail),
            Err(reply) => reply,
        },
        Some("open") => match (arguments.get(1), arguments.get(2)) {
            (Some(id), Some(link)) if arguments.len() == 3 => match find(&root, id) {
                Ok(mail) => open(&mail, link, &call.environment),
                Err(reply) => reply,
            },
            _ => Reply::failed(problem(
                "`jadpo email open` takes a mail's id and one of its links".to_string(),
                "It opens a link the mail holds, by its number from `jadpo email links <id>` or written out.".to_string(),
                "Run `jadpo email open <id> <number>`, for example `jadpo email open mail_3 1`"
                    .to_string(),
            )),
        },
        Some("clear") => {
            if arguments.len() > 1 {
                return Reply::failed(problem(
                    "`jadpo email clear` empties the whole inbox and takes nothing more"
                        .to_string(),
                    format!("It was given `{}`.", arguments[1..].join(" ")),
                    "Run `jadpo email clear`".to_string(),
                ));
            }
            clear(&root)
        }
        Some("check-domain") => {
            if arguments.len() > 2 {
                return Reply::failed(problem(
                    "`jadpo email check-domain` takes at most one domain".to_string(),
                    format!("It was given `{}`.", arguments[1..].join(" ")),
                    "Run `jadpo email check-domain <domain>`".to_string(),
                ));
            }
            let domain = match arguments.get(1) {
                Some(domain) => domain.trim_end_matches('.').to_ascii_lowercase(),
                None => match sending_domain(&root, &call.environment) {
                    Ok(domain) => domain,
                    Err(problem) => return Reply::failed(*problem),
                },
            };
            let resolver = SystemResolver(jadpo_host_native::dns::system_server());
            check_domain(&domain, &call.all("--selector"), &resolver)
        }
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let near = jadpo_diag::nearest(other, names.iter().copied());
            Reply::failed(problem(
                format!("`{other}` isn't something `jadpo email` does"),
                format!(
                    "`jadpo email` reads the dev inbox and checks the sending domain: {}.{}",
                    crate::run::either(&crate::run::quoted(names.iter())),
                    near.map(|near| format!(" Did you mean `{near}`?"))
                        .unwrap_or_default()
                ),
                match near {
                    Some(near) => format!("Use `jadpo email {near}`"),
                    None => "Run `jadpo email` to list the mail".to_string(),
                },
            ))
        }
    }
}

fn problem(headline: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_EMAIL_ARGUMENTS, headline)
        .reason(reason)
        .next(next)
}

/// The mail `id` names, or the reply saying there is none.
fn find(root: &Path, id: &str) -> Result<Mail, Reply> {
    if let Some(mail) = dev_inbox::read(root, id) {
        return Ok(mail);
    }
    let all = dev_inbox::read_all(root);
    let near = jadpo_diag::nearest(id, all.iter().map(|mail| mail.id.as_str()));
    Err(Reply::failed(problem(
        format!("There is no mail `{id}`"),
        format!(
            "The inbox keeps the last {} mails `jadpo dev` caught{}.",
            dev_inbox::KEPT,
            match all.last() {
                Some(last) => format!(", the newest `{}`", last.id),
                None => ", and holds none now".to_string(),
            }
        ),
        match near {
            Some(near) => format!("Use `{near}`"),
            None => "Run `jadpo email` to see their ids".to_string(),
        },
    )))
}

// --- list ---------------------------------------------------------------------

fn list(root: &Path, to: Option<&str>) -> Reply {
    let all = dev_inbox::read_all(root);
    let shown: Vec<&Mail> = all
        .iter()
        .filter(|mail| to.is_none_or(|wanted| mail.to.eq_ignore_ascii_case(wanted)))
        .collect();
    let data = Json::object([(
        "mails",
        Json::array(shown.iter().map(|mail| summary_json(mail))),
    )]);
    let mut screen = Screen::new();
    if shown.is_empty() {
        if all.is_empty() {
            screen.push(Block::summary(
                Outcome::Success,
                "No mail yet".to_string(),
                Vec::new(),
            ));
            screen.push(Block::hint(
                "jadpo dev",
                "runs the app, catching each mail it sends here",
            ));
        } else {
            screen.push(Block::summary(
                Outcome::Success,
                "No mail matches".to_string(),
                to.map(|to| vec![Fact::plain(format!("--to {to}"))])
                    .unwrap_or_default(),
            ));
        }
        return Reply::screen(screen, data);
    }
    let mut table = Table::new(["mail", "email", "to", "subject", "health"]).cut(3);
    let mut with_problems = 0;
    for mail in &shown {
        let found = mail.problems();
        if found > 0 {
            with_problems += 1;
        }
        table = table.row(vec![
            Cell::text(mail.id.clone()),
            Cell::muted(mail.email_short()),
            Cell::text(mail.to.clone()),
            Cell::text(mail.subject.clone()),
            health_cell(found, matches!(mail.spam, Spam::Ran(_))),
        ]);
    }
    screen.push(Block::Table(table));
    screen.push(Block::fields(vec![("health", DISCLAIMER)]));
    let mut facts = Vec::new();
    if with_problems > 0 {
        facts.push(Fact {
            text: format!("{with_problems} with problems"),
            tone: Some(jadpo_term::Token::Warning),
        });
    }
    screen.push(
        Block::summary(Outcome::Success, count(shown.len(), "mail"), facts)
            .with_plain_headline("email"),
    );
    if let Some(last) = shown.last() {
        screen.push(Block::hint(
            format!("jadpo email show {}", last.id),
            "shows its words, links and health",
        ));
    }
    Reply::screen(screen, data)
}

/// A mail's health in the list: "clean" only when SpamAssassin's rules
/// were in its score too.
fn health_cell(found: usize, scored: bool) -> Cell {
    match found {
        0 if scored => Cell::state(Symbol::Success, "clean"),
        0 => Cell::state(Symbol::Success, "no problems found"),
        n => Cell::state(Symbol::Warning, count(n, "problem")),
    }
}

fn count(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn int(n: usize) -> Json {
    Json::from(i64::try_from(n).unwrap_or(i64::MAX))
}

/// A mail as `jadpo email --json` lists it.
fn summary_json(mail: &Mail) -> Json {
    Json::object([
        ("id", Json::from(mail.id.as_str())),
        ("at", Json::from(mail.at.as_str())),
        ("email", Json::from(mail.email.as_str())),
        ("from", Json::from(mail.from.as_str())),
        ("to", Json::from(mail.to.as_str())),
        ("subject", Json::from(mail.subject.as_str())),
        ("problems", int(mail.problems())),
    ])
}

// --- show ---------------------------------------------------------------------

fn show(mail: &Mail, html: bool) -> Reply {
    let findings = mail.health();
    let links = mail.links();
    let mut rows = vec![
        ("email".to_string(), mail.email.clone()),
        ("from".to_string(), mail.from.clone()),
        ("to".to_string(), mail.to.clone()),
        ("at".to_string(), mail.at.clone()),
    ];
    if let Some(trace) = &mail.trace {
        rows.push(("trace".to_string(), trace.clone()));
    }
    let parts: Vec<String> = [
        mail.html
            .as_ref()
            .map(|html| format!("html {}", size(html.len()))),
        mail.text
            .as_ref()
            .map(|text| format!("text {}", size(text.len()))),
    ]
    .into_iter()
    .flatten()
    .collect();
    rows.push(("parts".to_string(), parts.join(" · ")));
    for attachment in &mail.attachments {
        rows.push((
            "attached".to_string(),
            format!(
                "{} · {} · {}",
                attachment.name,
                attachment.content_type,
                size(attachment.size as usize)
            ),
        ));
    }
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("{}  {}", mail.id, mail.subject),
        rows,
    });
    match (html, &mail.html, &mail.text) {
        (true, Some(body), _) | (false, Some(body), None) => {
            screen.push(Block::section("html", body.lines()));
        }
        (_, _, Some(text)) => screen.push(Block::section("text", text.lines())),
        (_, None, None) => {}
    }
    if !links.is_empty() {
        screen.push(Block::section(
            "links",
            links
                .iter()
                .enumerate()
                .map(|(at, link)| format!("{:>2}  {link}", at + 1)),
        ));
    }
    health_blocks(&mut screen, &findings, &mail.spam);
    if !links.is_empty() {
        screen.push(Block::hint(
            format!("jadpo email open {} 1", mail.id),
            "opens its first link in the browser",
        ));
    }
    let mut data = full_json(mail, &links);
    if let Json::Object(members) = &mut data {
        members.push(("health".to_string(), health_json(&findings, &mail.spam)));
    }
    Reply::screen(screen, data)
}

/// What the health checks found: an alert for each problem, with what was
/// found and the fix, then SpamAssassin's rules or why there are none,
/// then the note every score carries.
fn health_blocks(screen: &mut Screen, findings: &[email_health::Finding], spam: &Spam) {
    let total = Check::ALL.len();
    if findings.is_empty() {
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: format!("Health: all {total} of Jadpo's checks pass"),
            rows: Vec::new(),
        });
    } else {
        for finding in findings {
            screen.push(Block::Alerts(vec![finding.check.title().to_string()]));
            screen.push(Block::fields(vec![
                ("found", finding.detail.as_str()),
                ("next", finding.check.fix()),
            ]));
        }
    }
    spam_blocks(screen, spam);
    let passing = total - findings.len();
    let problems = findings.len() + usize::from(spam.is_spam());
    screen.push(Block::fields(vec![(
        "health",
        format!(
            "{} · {passing} of {total} checks pass · {}. {DISCLAIMER}",
            if problems == 0 && matches!(spam, Spam::Ran(_)) {
                "clean".to_string()
            } else if problems == 0 {
                "no problems found".to_string()
            } else {
                count(problems, "problem")
            },
            spam.summary()
        ),
    )]));
}

/// SpamAssassin's part of a score: its verdict and the rules it matched,
/// the most points first, or why it has none, never that the mail is
/// clean.
fn spam_blocks(screen: &mut Screen, spam: &Spam) {
    match spam {
        Spam::Ran(report) => {
            if report.is_spam() {
                screen.push(Block::Alerts(vec![spam.summary()]));
            } else {
                screen.push(Block::Status {
                    outcome: Outcome::Success,
                    text: spam.summary(),
                    rows: Vec::new(),
                });
            }
            if !report.rules.is_empty() {
                let mut table = Table::new(["points", "rule", "description"])
                    .right(0)
                    .cut(2);
                for rule in &report.rules {
                    table = table.row(vec![
                        if rule.counts() {
                            Cell::state(Symbol::Warning, rule.points.clone())
                        } else {
                            Cell::muted(rule.points.clone())
                        },
                        Cell::text(rule.name.clone()),
                        Cell::text(rule.description.clone()),
                    ]);
                }
                screen.push(Block::Table(table));
            }
            if report.is_spam() {
                screen.push(Block::fields(vec![(
                    "next",
                    "Fix the rules with the most points first: each says what it matched",
                )]));
            }
        }
        Spam::Failed(_) => {
            if let Some((why, next)) = spam.absent() {
                screen.push(Block::Alerts(vec![spam.summary()]));
                screen.push(Block::fields(vec![("found", why), ("next", next)]));
            }
        }
        Spam::NotRun | Spam::NotInstalled => {
            if let Some((why, next)) = spam.absent() {
                screen.push(Block::fields(vec![("spam", why), ("next", next)]));
            }
        }
    }
}

fn health_json(findings: &[email_health::Finding], spam: &Spam) -> Json {
    Json::object([
        ("checks", int(Check::ALL.len())),
        (
            "problems",
            Json::array(findings.iter().map(|finding| {
                Json::object([
                    ("check", Json::from(finding.check.id())),
                    ("title", Json::from(finding.check.title())),
                    ("found", Json::from(finding.detail.as_str())),
                    ("next", Json::from(finding.check.fix())),
                ])
            })),
        ),
        ("spamassassin", spam.to_output()),
        ("note", Json::from(DISCLAIMER)),
    ])
}

/// A mail whole, as `show --json` gives it.
fn full_json(mail: &Mail, links: &[String]) -> Json {
    let text = |value: Option<&String>| value.map_or(Json::Null, |text| Json::from(text.as_str()));
    Json::object([
        ("id", Json::from(mail.id.as_str())),
        ("at", Json::from(mail.at.as_str())),
        ("email", Json::from(mail.email.as_str())),
        ("trace", text(mail.trace.as_ref())),
        ("from", Json::from(mail.from.as_str())),
        ("to", Json::from(mail.to.as_str())),
        ("subject", Json::from(mail.subject.as_str())),
        ("html", text(mail.html.as_ref())),
        ("text", text(mail.text.as_ref())),
        ("links", Json::array(links.iter().map(String::as_str))),
        (
            "attachments",
            Json::array(mail.attachments.iter().map(|attachment| {
                Json::object([
                    ("name", Json::from(attachment.name.as_str())),
                    ("contentType", Json::from(attachment.content_type.as_str())),
                    ("size", int(attachment.size as usize)),
                ])
            })),
        ),
    ])
}

/// A size in bytes, as people read it: `412 B`, `3.2 KB`.
pub(crate) fn size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

// --- links and open -----------------------------------------------------------

fn links(mail: &Mail) -> Reply {
    let links = mail.links();
    let data = Json::object([
        ("id", Json::from(mail.id.as_str())),
        ("links", Json::array(links.iter().map(String::as_str))),
    ]);
    if links.is_empty() {
        let screen = Screen::new().with(Block::summary(
            Outcome::Success,
            format!("`{}` has no links", mail.id),
            Vec::new(),
        ));
        return Reply::screen(screen, data);
    }
    let screen = Screen::new()
        .with(Block::section(
            format!("{}  {}", mail.id, mail.subject),
            links
                .iter()
                .enumerate()
                .map(|(at, link)| format!("{:>2}  {link}", at + 1)),
        ))
        .with(Block::hint(
            format!("jadpo email open {} 1", mail.id),
            "opens the first in the browser",
        ));
    Reply::screen(screen, data)
}

fn open(mail: &Mail, wanted: &str, environment: &jadpo_term::Environment) -> Reply {
    let links = mail.links();
    let chosen = match wanted.parse::<usize>() {
        Ok(number) => number
            .checked_sub(1)
            .and_then(|at| links.get(at))
            .map(|link| (number, link)),
        Err(_) => links
            .iter()
            .position(|link| link == wanted)
            .map(|at| (at + 1, &links[at])),
    };
    let Some((number, link)) = chosen else {
        return Reply::failed(problem(
            format!("`{wanted}` isn't one of `{}`'s links", mail.id),
            match links.len() {
                0 => format!("`{}` has no links.", mail.id),
                n => format!(
                    "It has {}, numbered from 1 as `jadpo email links {}` lists them; a link may also be written out whole.",
                    count(n, "link"),
                    mail.id
                ),
            },
            format!("Run `jadpo email links {}` to see them", mail.id),
        ));
    };
    let lower = link.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Reply::failed(problem(
            format!("Link {number} of `{}` isn't a web address", mail.id),
            format!(
                "`jadpo email open` opens `http://` and `https://` links in the browser, and this one is `{link}`."
            ),
            "Open a web link, or copy this one from `jadpo email show`".to_string(),
        ));
    }
    if let Err(error) = start_browser(link, environment) {
        let set = browser_var(environment).is_some();
        let (program, _) = opener(link, browser_var(environment), System::current());
        let who = if set {
            format!("`BROWSER` names `{program}` as the browser")
        } else {
            format!("`BROWSER` isn't set, so the system's `{program}` opens links")
        };
        let why = match error.kind() {
            std::io::ErrorKind::NotFound => "there's no program of that name".to_string(),
            std::io::ErrorKind::PermissionDenied => "this user may not run it".to_string(),
            _ => format!("starting it failed: {error}"),
        };
        return Reply::failed(
            Diagnostic::new(
                &rule::EMAIL_BROWSER_FAILED,
                "The browser couldn't be started",
            )
            .reason(format!("{who}, and {why}."))
            .next(format!(
                "Set `BROWSER` to your browser's command, or open `{link}` yourself"
            )),
        );
    }
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!("Opened link {number} of `{}` in the browser", mail.id),
        rows: vec![("link".to_string(), link.clone())],
    });
    Reply::screen(
        screen,
        Json::object([
            ("id", Json::from(mail.id.as_str())),
            ("link", Json::from(link.as_str())),
            ("number", int(number)),
        ]),
    )
}

/// Starts the browser on `link`: `$BROWSER` when set, else the system's
/// opener. It isn't waited for.
fn start_browser(link: &str, environment: &jadpo_term::Environment) -> std::io::Result<()> {
    let (program, args) = opener(link, browser_var(environment), System::current());
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}

/// `$BROWSER`, when it is set to something.
fn browser_var(environment: &jadpo_term::Environment) -> Option<String> {
    environment
        .var("BROWSER")
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The operating systems whose openers differ.
#[derive(Clone, Copy, Debug)]
enum System {
    Mac,
    Windows,
    Other,
}

impl System {
    /// The one this `jadpo` was built for.
    fn current() -> System {
        if cfg!(target_os = "macos") {
            System::Mac
        } else if cfg!(windows) {
            System::Windows
        } else {
            System::Other
        }
    }
}

/// The program that opens `link`, and its arguments: `browser` when set,
/// else `system`'s opener, the link always one argument of its own and
/// never read by a shell, so a `&` in it is only a character of the link
/// (on Windows, `url.dll`'s handler, not `cmd /C start`; Phase 7 review,
/// chunk 7.9c).
fn opener(link: &str, browser: Option<String>, system: System) -> (String, Vec<String>) {
    let (program, mut args) = match (browser, system) {
        (Some(browser), _) => (browser, Vec::new()),
        (None, System::Mac) => ("open".to_string(), Vec::new()),
        (None, System::Windows) => (
            "rundll32".to_string(),
            vec!["url.dll,FileProtocolHandler".to_string()],
        ),
        (None, System::Other) => ("xdg-open".to_string(), Vec::new()),
    };
    args.push(link.to_string());
    (program, args)
}

// --- clear --------------------------------------------------------------------

fn clear(root: &Path) -> Reply {
    match dev_inbox::clear(root) {
        Ok(cleared) => {
            let text = match cleared {
                0 => "The inbox was empty".to_string(),
                n => format!("Cleared {} from the inbox", count(n, "mail")),
            };
            Reply::screen(
                Screen::new().with(Block::Status {
                    outcome: Outcome::Success,
                    text,
                    rows: Vec::new(),
                }),
                Json::object([("cleared", int(cleared))]),
            )
        }
        Err(error) => Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                "The inbox couldn't be cleared",
            )
            .reason(format!(
                "Removing its mail from `{}` failed: {error}.",
                dev_inbox::INBOX
            ))
            .next("Check that the project's `build/` folder can be written, then run it again"),
        ),
    }
}

// --- check-domain -------------------------------------------------------------

/// Where `jadpo email check-domain` reads `TXT` records.
pub(crate) trait Resolver {
    /// Every `TXT` record at `name`, each one's strings joined: none when
    /// the name has none; why it couldn't be asked otherwise.
    fn txt(&self, name: &str) -> Result<Vec<String>, String>;
}

/// The system's name server.
struct SystemResolver(Option<std::net::SocketAddr>);

impl Resolver for SystemResolver {
    fn txt(&self, name: &str) -> Result<Vec<String>, String> {
        let server = self
            .0
            .ok_or_else(|| jadpo_host_native::dns::DnsError::NoServer.to_string())?;
        jadpo_host_native::dns::txt(server, name).map_err(|error| error.to_string())
    }
}

/// The domain `application { email: { from: } }` sends from: the address
/// written out, or the setting's value from the environment, `.env.local`
/// or its default.
fn sending_domain(
    root: &Path,
    environment: &jadpo_term::Environment,
) -> Result<String, Box<Diagnostic>> {
    let give = |reason: String| {
        Box::new(problem(
            "The sending domain can't be read".to_string(),
            reason,
            "Give the domain: `jadpo email check-domain <domain>`".to_string(),
        ))
    };
    let loaded = jadpo_db::load(root);
    let Ok(prepared) = jadpo_interp::Prepared::load(&loaded.db, loaded.project) else {
        return Err(give(
            "It is read from `email: { from: }` in `app.jadpo`, and the project has errors, so it can't be read.".to_string(),
        ));
    };
    let emails = &prepared.plan().events().emails;
    let address = match &emails.from {
        None => {
            return Err(give(
                "It is read from `email: { from: }` in `app.jadpo`, and the app names no address to send from.".to_string(),
            ));
        }
        Some(jadpo_std::email::Sender::Address(address)) => address.clone(),
        Some(jadpo_std::email::Sender::Setting(at)) => {
            let plan = prepared.plan().config();
            let Some(field) = plan.fields.get(*at as usize) else {
                return Err(give(
                    "`email: { from: }` names a setting this build doesn't have.".to_string(),
                ));
            };
            use jadpo_runtime::config::Variables as _;
            let layered = crate::config::Layered::new(root, environment)?;
            match layered
                .variable(&field.variable)
                .or_else(|| field.default.clone())
            {
                Some(value) => value,
                None => {
                    return Err(give(format!(
                        "`email: {{ from: }}` reads `config.{}`, and nothing gives it a value: `{}` isn't set and it has no default.",
                        field.name, field.variable
                    )));
                }
            }
        }
    };
    match address.rsplit_once('@') {
        Some((_, domain)) if !domain.is_empty() => Ok(domain
            .trim_end_matches(['>', ' '])
            .trim_end_matches('.')
            .to_ascii_lowercase()),
        _ => Err(give(format!("`{address}` isn't an email address."))),
    }
}

/// How one record fared.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Verdict {
    Passes,
    /// Works, but should change.
    Alert,
    Fails,
    /// Not looked up: what it needs first.
    Unchecked,
}

/// What one record check found.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DomainRecord {
    /// `SPF`, `DKIM`, `DMARC`.
    kind: &'static str,
    /// The name its record is at.
    at: String,
    verdict: Verdict,
    /// The record as found, when one was.
    record: Option<String>,
    /// What is wrong, as a headline, when something is.
    problem: Option<String>,
    /// The one change that fixes it.
    next: Option<String>,
}

/// Every record the domain needs to send mail that is trusted, read
/// through `resolver`.
fn domain_checks(domain: &str, selectors: &[&str], resolver: &dyn Resolver) -> Vec<DomainRecord> {
    let mut out = vec![spf(domain, resolver)];
    if selectors.is_empty() {
        out.push(DomainRecord {
            kind: "DKIM",
            at: format!("<selector>._domainkey.{domain}"),
            verdict: Verdict::Unchecked,
            record: None,
            problem: Some("DKIM needs the selector your provider signs with".to_string()),
            next: Some(
                "Run again with `--selector <name>`; your provider's DKIM set-up page names it"
                    .to_string(),
            ),
        });
    }
    for selector in selectors {
        out.push(dkim(domain, selector, resolver));
    }
    out.push(dmarc(domain, resolver));
    out
}

fn lookup_failed(kind: &'static str, at: String, why: String) -> DomainRecord {
    DomainRecord {
        kind,
        problem: Some(format!(
            "{kind}'s record at `{at}` couldn't be looked up: {why}"
        )),
        next: Some("Check this machine's connection, then run it again".to_string()),
        at,
        verdict: Verdict::Unchecked,
        record: None,
    }
}

fn spf(domain: &str, resolver: &dyn Resolver) -> DomainRecord {
    let at = domain.to_string();
    let records = match resolver.txt(&at) {
        Ok(records) => records,
        Err(why) => return lookup_failed("SPF", at, why),
    };
    let found: Vec<&String> = records
        .iter()
        .filter(|record| {
            let lower = record.trim().to_ascii_lowercase();
            lower == "v=spf1" || lower.starts_with("v=spf1 ")
        })
        .collect();
    let check = |verdict,
                 record: Option<&String>,
                 problem: Option<String>,
                 next: Option<String>| DomainRecord {
        kind: "SPF",
        at: at.clone(),
        verdict,
        record: record.cloned(),
        problem,
        next,
    };
    let [record] = found.as_slice() else {
        if found.is_empty() {
            return check(
                Verdict::Fails,
                None,
                Some(format!("No SPF record at `{at}`")),
                Some(format!(
                    "Add a TXT record at `{at}`: `v=spf1 include:<your provider's SPF domain> -all`, as your provider's set-up page gives it"
                )),
            );
        }
        return check(
            Verdict::Fails,
            found.first().copied(),
            Some(format!(
                "{} SPF records at `{at}`; receivers read that as none",
                found.len()
            )),
            Some("Merge them into one `v=spf1` record".to_string()),
        );
    };
    let terms: Vec<String> = record
        .split_whitespace()
        .skip(1)
        .map(str::to_ascii_lowercase)
        .collect();
    let all = terms
        .iter()
        .find(|term| term.trim_start_matches(['+', '-', '~', '?']) == "all");
    let lookups = terms
        .iter()
        .filter(|term| {
            let term = term.trim_start_matches(['+', '-', '~', '?']);
            term == "a"
                || term == "mx"
                || term == "ptr"
                || [
                    "a:",
                    "a/",
                    "mx:",
                    "mx/",
                    "include:",
                    "exists:",
                    "ptr:",
                    "redirect=",
                ]
                .iter()
                .any(|prefix| term.starts_with(prefix))
        })
        .count();
    match all.map(String::as_str) {
        Some("+all" | "all") => check(
            Verdict::Fails,
            Some(record),
            Some("SPF lets any server send as this domain (`+all`)".to_string()),
            Some("End the record with `-all` or `~all`".to_string()),
        ),
        _ if lookups > 10 => check(
            Verdict::Fails,
            Some(record),
            Some(format!(
                "SPF needs at least {lookups} DNS lookups; receivers stop at 10"
            )),
            Some(
                "Remove the includes your mail doesn't go through, or flatten them into addresses"
                    .to_string(),
            ),
        ),
        Some("-all" | "~all") => check(Verdict::Passes, Some(record), None, None),
        _ => check(
            Verdict::Alert,
            Some(record),
            Some("SPF doesn't say what to do with other servers".to_string()),
            Some("End the record with `-all` or `~all`".to_string()),
        ),
    }
}

fn dkim(domain: &str, selector: &str, resolver: &dyn Resolver) -> DomainRecord {
    let at = format!("{selector}._domainkey.{domain}");
    let records = match resolver.txt(&at) {
        Ok(records) => records,
        Err(why) => return lookup_failed("DKIM", at, why),
    };
    let found = records.iter().find(|record| {
        let lower = record.to_ascii_lowercase();
        lower.contains("p=") && (lower.starts_with("v=dkim1") || lower.contains("k="))
            || lower.trim_start().starts_with("p=")
    });
    let Some(record) = found else {
        return DomainRecord {
            kind: "DKIM",
            problem: Some(format!("No DKIM key at `{at}`")),
            next: Some(format!(
                "Add the record your provider gives for selector `{selector}`, or check the selector"
            )),
            at,
            verdict: Verdict::Fails,
            record: None,
        };
    };
    let key = record
        .split(';')
        .map(str::trim)
        .find_map(|tag| tag.strip_prefix("p=").or_else(|| tag.strip_prefix("P=")))
        .unwrap_or_default();
    if key.trim().is_empty() {
        return DomainRecord {
            kind: "DKIM",
            problem: Some(format!("The DKIM key at `{at}` is revoked (`p=` is empty)")),
            next: Some("Publish the key your provider signs with now".to_string()),
            at,
            verdict: Verdict::Fails,
            record: Some(record.clone()),
        };
    }
    DomainRecord {
        kind: "DKIM",
        at,
        verdict: Verdict::Passes,
        record: Some(record.clone()),
        problem: None,
        next: None,
    }
}

fn dmarc(domain: &str, resolver: &dyn Resolver) -> DomainRecord {
    let at = format!("_dmarc.{domain}");
    let records = match resolver.txt(&at) {
        Ok(records) => records,
        Err(why) => return lookup_failed("DMARC", at, why),
    };
    let found: Vec<&String> = records
        .iter()
        .filter(|record| record.trim().to_ascii_lowercase().starts_with("v=dmarc1"))
        .collect();
    let check = |verdict,
                 record: Option<&String>,
                 problem: Option<String>,
                 next: Option<String>| DomainRecord {
        kind: "DMARC",
        at: at.clone(),
        verdict,
        record: record.cloned(),
        problem,
        next,
    };
    let [record] = found.as_slice() else {
        if found.is_empty() {
            return check(
                Verdict::Fails,
                None,
                Some(format!("No DMARC record at `{at}`")),
                Some(format!(
                    "Add a TXT record at `{at}`: `v=DMARC1; p=quarantine; rua=mailto:dmarc@{domain}`"
                )),
            );
        }
        return check(
            Verdict::Fails,
            found.first().copied(),
            Some(format!(
                "{} DMARC records at `{at}`; receivers read that as none",
                found.len()
            )),
            Some("Keep one `v=DMARC1` record".to_string()),
        );
    };
    let policy = record.split(';').map(str::trim).find_map(|tag| {
        let (name, value) = tag.split_once('=')?;
        (name.trim().eq_ignore_ascii_case("p")).then(|| value.trim().to_ascii_lowercase())
    });
    match policy.as_deref() {
        Some("quarantine" | "reject") => check(Verdict::Passes, Some(record), None, None),
        Some("none") => check(
            Verdict::Alert,
            Some(record),
            Some("DMARC only monitors (`p=none`): mail that fails still gets through".to_string()),
            Some(
                "Once its reports show your mail passing, move to `p=quarantine`, then `p=reject`"
                    .to_string(),
            ),
        ),
        _ => check(
            Verdict::Fails,
            Some(record),
            Some("DMARC names no policy (`p=`)".to_string()),
            Some("Add `p=quarantine` or `p=reject`".to_string()),
        ),
    }
}

/// `jadpo email check-domain`'s screen for `domain`.
fn check_domain(domain: &str, selectors: &[&str], resolver: &dyn Resolver) -> Reply {
    let checks = domain_checks(domain, selectors, resolver);
    let mut screen = Screen::new();
    let mut table = Table::new(["record", "state", "at"]);
    for check in &checks {
        table = table.row(vec![
            Cell::text(check.kind),
            match check.verdict {
                Verdict::Passes => Cell::state(Symbol::Success, "passes"),
                Verdict::Alert => Cell::state(Symbol::Warning, "alert"),
                Verdict::Fails => Cell::state(Symbol::Failure, "fails"),
                Verdict::Unchecked => Cell::state(Symbol::Skipped, "not checked"),
            },
            Cell::muted(check.at.clone()),
        ]);
    }
    screen.push(Block::Table(table));
    for check in &checks {
        let (Some(problem), Some(next)) = (&check.problem, &check.next) else {
            continue;
        };
        let mut fields = Vec::new();
        if let Some(record) = &check.record {
            fields.push(("record".to_string(), record.clone()));
        }
        fields.push(("next".to_string(), next.clone()));
        match check.verdict {
            Verdict::Fails => screen.push(Block::Failed {
                subject: problem.clone(),
                fields,
            }),
            _ => {
                screen.push(Block::Alerts(vec![problem.clone()]));
                screen.push(Block::Fields(fields));
            }
        }
    }
    screen.push(Block::fields(vec![(
        "note",
        "Records in place are needed for mail to be trusted, never enough: they can't predict whether Gmail, Outlook or any other provider puts a mail in the inbox, since each uses its own filters and the sender's history.",
    )]));
    let tally = |verdict: Verdict| {
        checks
            .iter()
            .filter(|check| check.verdict == verdict)
            .count()
    };
    let (failed, alerts, passed, unchecked) = (
        tally(Verdict::Fails),
        tally(Verdict::Alert),
        tally(Verdict::Passes),
        tally(Verdict::Unchecked),
    );
    let mut facts = Vec::new();
    if failed > 0 {
        facts.push(Fact::problem(format!("{failed} failed")));
    }
    if alerts > 0 {
        facts.push(Fact {
            text: count(alerts, "alert"),
            tone: Some(jadpo_term::Token::Warning),
        });
    }
    facts.push(Fact::plain(format!("{passed} passed")));
    if unchecked > 0 {
        facts.push(Fact::plain(format!("{unchecked} not checked")));
    }
    let outcome = if failed > 0 {
        Outcome::Failure
    } else {
        Outcome::Success
    };
    screen.push(
        Block::summary(outcome, domain.to_string(), facts).with_plain_headline("check-domain"),
    );
    let data = Json::object([
        ("domain", Json::from(domain)),
        ("passed", Json::from(failed == 0)),
        (
            "records",
            Json::array(checks.iter().map(|check| {
                let text = |value: &Option<String>| value.as_deref().map_or(Json::Null, Json::from);
                Json::object([
                    ("kind", Json::from(check.kind)),
                    ("at", Json::from(check.at.as_str())),
                    (
                        "state",
                        Json::from(match check.verdict {
                            Verdict::Passes => "passes",
                            Verdict::Alert => "alert",
                            Verdict::Fails => "fails",
                            Verdict::Unchecked => "not_checked",
                        }),
                    ),
                    ("record", text(&check.record)),
                    ("problem", text(&check.problem)),
                    ("next", text(&check.next)),
                ])
            })),
        ),
    ]);
    Reply::screen(screen, data)
}

#[cfg(test)]
mod tests {
    /// Each system's opener is handed the link as one argument and no
    /// shell reads it, so a link's `&` runs nothing (Phase 7 review, chunk
    /// 7.9c).
    #[test]
    fn a_link_is_opened_as_one_argument_with_no_shell() {
        use super::{System, opener};
        let link = "https://example.test/verify?a=1&calc.exe&b=\"2\"";
        let opened = |system| opener(link, None, system);
        assert_eq!(
            opened(System::Windows),
            (
                "rundll32".to_string(),
                vec!["url.dll,FileProtocolHandler".to_string(), link.to_string()]
            )
        );
        assert_eq!(
            opened(System::Mac),
            ("open".to_string(), vec![link.to_string()])
        );
        assert_eq!(
            opened(System::Other),
            ("xdg-open".to_string(), vec![link.to_string()])
        );
        assert_eq!(
            opener(link, Some("firefox".into()), System::Windows),
            ("firefox".to_string(), vec![link.to_string()])
        );
    }

    use super::*;
    use std::collections::BTreeMap;

    /// Answers from a table, never a name server.
    struct Fake(BTreeMap<&'static str, Result<Vec<&'static str>, &'static str>>);

    impl Resolver for Fake {
        fn txt(&self, name: &str) -> Result<Vec<String>, String> {
            match self.0.get(name) {
                Some(Ok(records)) => Ok(records.iter().map(|record| record.to_string()).collect()),
                Some(Err(why)) => Err(why.to_string()),
                None => Ok(Vec::new()),
            }
        }
    }

    fn states(checks: &[DomainRecord]) -> Vec<(&str, Verdict)> {
        checks
            .iter()
            .map(|check| (check.kind, check.verdict.clone()))
            .collect()
    }

    #[test]
    fn a_domain_with_its_records_in_place_passes() {
        let fake = Fake(BTreeMap::from([
            (
                "notes.example",
                Ok(vec![
                    "google-site-verification=x",
                    "v=spf1 include:amazonses.com ~all",
                ]),
            ),
            (
                "resend._domainkey.notes.example",
                Ok(vec!["p=MIGfMA0GCSqGSIb3DQEB"]),
            ),
            (
                "_dmarc.notes.example",
                Ok(vec!["v=DMARC1; p=reject; rua=mailto:d@notes.example"]),
            ),
        ]));
        let checks = domain_checks("notes.example", &["resend"], &fake);
        assert_eq!(
            states(&checks),
            [
                ("SPF", Verdict::Passes),
                ("DKIM", Verdict::Passes),
                ("DMARC", Verdict::Passes)
            ]
        );
        let Reply::Done { data, .. } = check_domain("notes.example", &["resend"], &fake) else {
            panic!("a screen");
        };
        assert!(data.to_line().contains("\"passed\":true"));
    }

    #[test]
    fn each_missing_or_weak_record_says_what_to_add() {
        let fake = Fake(BTreeMap::from([
            ("notes.example", Ok(vec!["v=spf1 +all"])),
            ("_dmarc.notes.example", Ok(vec!["v=DMARC1; p=none"])),
        ]));
        let checks = domain_checks("notes.example", &[], &fake);
        assert_eq!(
            states(&checks),
            [
                ("SPF", Verdict::Fails),
                ("DKIM", Verdict::Unchecked),
                ("DMARC", Verdict::Alert)
            ]
        );
        assert_eq!(
            checks[0].problem.as_deref(),
            Some("SPF lets any server send as this domain (`+all`)")
        );
        let none = domain_checks("bare.example", &["s1"], &Fake(BTreeMap::new()));
        assert_eq!(
            states(&none),
            [
                ("SPF", Verdict::Fails),
                ("DKIM", Verdict::Fails),
                ("DMARC", Verdict::Fails)
            ]
        );
        assert_eq!(
            none[2].next.as_deref(),
            Some(
                "Add a TXT record at `_dmarc.bare.example`: `v=DMARC1; p=quarantine; rua=mailto:dmarc@bare.example`"
            )
        );
        let two = Fake(BTreeMap::from([
            (
                "two.example",
                Ok(vec!["v=spf1 -all", "v=spf1 include:x.example -all"]),
            ),
            ("s1._domainkey.two.example", Ok(vec!["v=DKIM1; k=rsa; p="])),
            (
                "_dmarc.two.example",
                Ok(vec!["v=DMARC1; rua=mailto:x@two.example"]),
            ),
        ]));
        let checks = domain_checks("two.example", &["s1"], &two);
        assert_eq!(
            checks
                .iter()
                .map(|check| check.problem.clone().unwrap_or_default())
                .collect::<Vec<_>>(),
            [
                "2 SPF records at `two.example`; receivers read that as none",
                "The DKIM key at `s1._domainkey.two.example` is revoked (`p=` is empty)",
                "DMARC names no policy (`p=`)"
            ]
        );
    }

    #[test]
    fn too_many_lookups_and_a_failed_lookup_are_reported() {
        let many = format!("v=spf1 {} -all", "include:a.example ".repeat(11));
        let fake = Fake(BTreeMap::from([
            ("notes.example", Ok(vec![many.leak() as &str])),
            (
                "_dmarc.notes.example",
                Err("the name server didn't answer: timed out"),
            ),
        ]));
        let checks = domain_checks("notes.example", &[], &fake);
        assert_eq!(
            checks[0].problem.as_deref(),
            Some("SPF needs at least 11 DNS lookups; receivers stop at 10")
        );
        assert_eq!(checks[2].verdict, Verdict::Unchecked);
        assert_eq!(
            checks[2].problem.as_deref(),
            Some(
                "DMARC's record at `_dmarc.notes.example` couldn't be looked up: the name server didn't answer: timed out"
            )
        );
    }

    #[test]
    fn the_screen_draws_each_record_and_its_fix() {
        let fake = Fake(BTreeMap::from([(
            "_dmarc.notes.example",
            Ok(vec!["v=DMARC1; p=none"]),
        )]));
        let Reply::Done {
            text: crate::run::Text::Screen(screen),
            ..
        } = check_domain("notes.example", &[], &fake)
        else {
            panic!("a screen");
        };
        let plain = screen.render(&jadpo_term::Style::new(jadpo_term::Mode::Plain));
        assert!(
            plain.contains("failed: No SPF record at `notes.example`"),
            "{plain}"
        );
        assert!(plain.contains("alert: DMARC only monitors"), "{plain}");
        assert!(plain.contains("can't predict whether Gmail"), "{plain}");
    }

    fn mail(spam: Spam) -> Mail {
        Mail {
            id: "mail_1".into(),
            number: 1,
            at: "2026-10-20T14:02:31Z".into(),
            email: "members.WelcomeEmail".into(),
            trace: None,
            from: "hello@notes.example".into(),
            to: "ada@notes.example".into(),
            subject: "Welcome, Ada".into(),
            html: None,
            text: Some("Hello, Ada.\n".into()),
            attachments: Vec::new(),
            spam,
        }
    }

    fn shown(mail: &Mail) -> (String, String) {
        let Reply::Done {
            text: crate::run::Text::Screen(screen),
            data,
            ..
        } = show(mail, false)
        else {
            panic!("a screen");
        };
        (
            screen.render(&jadpo_term::Style::new(jadpo_term::Mode::Plain)),
            data.to_line(),
        )
    }

    #[test]
    fn show_says_when_spamassassin_wasn_t_run_never_that_the_mail_is_clean() {
        let (plain, data) = shown(&mail(Spam::NotInstalled));
        assert!(
            plain.contains("SpamAssassin isn't installed, so its rules aren't in this score. That doesn't make the mail clean."),
            "{plain}"
        );
        assert!(
            plain.contains("no problems found · 7 of 7 checks pass · SpamAssassin wasn't run: it isn't installed."),
            "{plain}"
        );
        assert!(
            data.contains(
                "\"spamassassin\":{\"ran\":false,\"why\":\"not_installed\",\"detail\":null}"
            ),
            "{data}"
        );
        let (plain, data) = shown(&mail(Spam::Failed("it exited with status 2".into())));
        assert!(
            plain.contains("alert: SpamAssassin couldn't score it"),
            "{plain}"
        );
        assert!(plain.contains("spamassassin --lint"), "{plain}");
        assert!(data.contains("\"why\":\"failed\""), "{data}");
    }

    #[test]
    fn show_lists_spamassassin_s_rules_the_most_points_first() {
        let report = crate::spam::Report {
            score: "6.2".into(),
            required: "5.0".into(),
            rules: vec![
                crate::spam::Rule {
                    name: "MISSING_DATE".into(),
                    points: "1.4".into(),
                    description: "Missing Date: header".into(),
                },
                crate::spam::Rule {
                    name: "NO_RELAYS".into(),
                    points: "-0.0".into(),
                    description: "Informational: message was not relayed via SMTP".into(),
                },
            ],
        };
        let (plain, data) = shown(&mail(Spam::Ran(report)));
        assert!(
            plain.contains("alert: SpamAssassin calls it spam: 6.2 points, spam from 5.0"),
            "{plain}"
        );
        assert!(plain.contains("MISSING_DATE"), "{plain}");
        assert!(plain.contains("Missing Date: header"), "{plain}");
        assert!(plain.contains("1 problem · 7 of 7 checks pass"), "{plain}");
        assert!(
            data.contains("\"rules\":[{\"rule\":\"MISSING_DATE\",\"points\":\"1.4\",\"description\":\"Missing Date: header\"}"),
            "{data}"
        );
        assert!(data.contains("\"spam\":true"), "{data}");
    }
}
