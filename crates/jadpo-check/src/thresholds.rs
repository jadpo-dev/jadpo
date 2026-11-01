//! Area 16's monitoring thresholds (chunk B13): a domain's `thresholds {
//! }` in its `monitoring.jadpo`, each threshold `name: { }` watching one
//! route, action, query, schedule or subscriber of the domain, with
//! limits that are upper bounds: `duration:` and `queries:` judged per
//! request, the percentiles, `server_failures:` and `oldest_waiting:`
//! judged across requests over `within:` (decided 2026-11-01).
//!
//! [`written`] reads a block as text, with no project around it, so the
//! before side's monitoring files read from git are compared with the
//! same reading ([`loosened`]); [`thresholds`] checks today's against the
//! project, which the graph, the runtime's plan and the rules in
//! `build/monitoring/` read ([`rules`]). A disk store's
//! `free_space_floor:` is a threshold too ([`floors`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use jadpo_db::{Db, ItemKind, Layout, Ownership, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Key;
use jadpo_std::HttpMethod;
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use crate::lookups::{Lookup, NameView};
use crate::tree::{self, name_text, node_span};
use crate::types::Const;

/// The word `thresholds { }` is declared with.
pub const THRESHOLDS: &str = "thresholds";

/// The file a domain's thresholds are in.
pub const FILE: &str = "monitoring.jadpo";

/// `within:` when it is left out: five minutes.
pub const DEFAULT_WITHIN_MS: u64 = 300_000;

const MINUTE_MS: u64 = 60_000;
const DAY_MS: u64 = 86_400_000;

/// What a threshold watches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Watches {
    Route,
    Action,
    Query,
    Schedule,
    Subscriber,
}

impl Watches {
    pub const ALL: [Watches; 5] = [
        Watches::Route,
        Watches::Action,
        Watches::Query,
        Watches::Schedule,
        Watches::Subscriber,
    ];

    /// The key it is written with, and the word messages use.
    pub fn word(self) -> &'static str {
        match self {
            Watches::Route => "route",
            Watches::Action => "action",
            Watches::Query => "query",
            Watches::Schedule => "schedule",
            Watches::Subscriber => "subscriber",
        }
    }

    /// With its article: `an action`.
    pub fn with_a(self) -> &'static str {
        match self {
            Watches::Route => "a route",
            Watches::Action => "an action",
            Watches::Query => "a query",
            Watches::Schedule => "a schedule",
            Watches::Subscriber => "a subscriber",
        }
    }

    fn item(self) -> Option<ItemKind> {
        match self {
            Watches::Route => None,
            Watches::Action => Some(ItemKind::Action),
            Watches::Query => Some(ItemKind::Query),
            Watches::Schedule => Some(ItemKind::Schedule),
            Watches::Subscriber => Some(ItemKind::Subscriber),
        }
    }
}

/// One of a threshold's limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Limit {
    Duration,
    Queries,
    P50,
    P90,
    P95,
    P99,
    ServerFailures,
    OldestWaiting,
}

/// What a limit's value is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Milliseconds, written as a duration.
    Duration,
    /// A whole number.
    Count,
    /// Millionths, written as a percentage.
    Share,
}

impl Limit {
    pub const ALL: [Limit; 8] = [
        Limit::Duration,
        Limit::Queries,
        Limit::P50,
        Limit::P90,
        Limit::P95,
        Limit::P99,
        Limit::ServerFailures,
        Limit::OldestWaiting,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Limit::Duration => "duration",
            Limit::Queries => "queries",
            Limit::P50 => "p50",
            Limit::P90 => "p90",
            Limit::P95 => "p95",
            Limit::P99 => "p99",
            Limit::ServerFailures => "server_failures",
            Limit::OldestWaiting => "oldest_waiting",
        }
    }

    fn from_word(word: &str) -> Option<Limit> {
        Limit::ALL.into_iter().find(|l| l.word() == word)
    }

    /// Whether one process judges it, one request at a time.
    pub fn per_request(self) -> bool {
        matches!(self, Limit::Duration | Limit::Queries)
    }

    pub fn unit(self) -> Unit {
        match self {
            Limit::Queries => Unit::Count,
            Limit::ServerFailures => Unit::Share,
            _ => Unit::Duration,
        }
    }

    /// The share a percentile names, `0.95`; `None` for the others.
    pub fn quantile(self) -> Option<&'static str> {
        match self {
            Limit::P50 => Some("0.5"),
            Limit::P90 => Some("0.9"),
            Limit::P95 => Some("0.95"),
            Limit::P99 => Some("0.99"),
            _ => None,
        }
    }

    /// Whether it can watch `watches`.
    pub fn applies(self, watches: Watches) -> bool {
        match self {
            Limit::Queries => watches != Watches::Query,
            Limit::OldestWaiting => watches == Watches::Subscriber,
            _ => true,
        }
    }

    fn watched_by(self) -> &'static str {
        match self {
            Limit::Queries => "a route, an action, a schedule or a subscriber",
            Limit::OldestWaiting => "a subscriber",
            _ => "anything",
        }
    }
}

/// A threshold as written, read without the project: what the before
/// side is compared by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Written {
    pub name: String,
    /// What it watches, as written: `route POST /hello`, `action greet`.
    pub watches: Option<String>,
    /// Each limit that reads as a value of its unit.
    pub limits: BTreeMap<Limit, u64>,
    /// `within:` when given and valid.
    pub within_ms: Option<u64>,
}

impl Written {
    /// Whether any limit is judged across requests.
    pub fn across(&self) -> bool {
        self.limits.keys().any(|limit| !limit.per_request())
    }

    pub fn within(&self) -> u64 {
        self.within_ms.unwrap_or(DEFAULT_WITHIN_MS)
    }
}

/// What a checked threshold watches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub watches: Watches,
    /// Its node's key.
    pub key: Key,
    /// A route's method and path as `jadpo routes` prints it, for its
    /// metric's attributes.
    pub route: Option<(HttpMethod, String)>,
    /// What the runtime's metrics name it by: `hello.greet`, or the
    /// route's label.
    pub name: String,
}

/// One threshold that checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Threshold {
    pub domain: Option<String>,
    pub name: String,
    /// As an acceptance names it: `hello.greeting`.
    pub subject: String,
    /// Its `///` line.
    pub doc: Option<String>,
    pub target: Target,
    pub limits: BTreeMap<Limit, u64>,
    pub within_ms: u64,
    /// The file it is declared in and its name's place there.
    pub path: String,
    pub at: Span,
}

impl Threshold {
    pub fn across(&self) -> bool {
        self.limits.keys().any(|limit| !limit.per_request())
    }
}

/// Every threshold of a project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thresholds {
    /// Each that checks, by subject.
    pub thresholds: Vec<Threshold>,
    pub problems: Vec<Diagnostic>,
}

/// A disk store's `free_space_floor:`, in millionths, by connection.
pub fn floors(text: &str) -> BTreeMap<String, u32> {
    let parse = jadpo_syntax::parse_file(text);
    let mut out = BTreeMap::new();
    for decl in parse
        .syntax()
        .children()
        .filter_map(ast::SingletonDecl::cast)
        .filter(|decl| decl.keyword().is_some_and(|w| w.text() == "application"))
    {
        for (key, member) in tree::members(decl.member_list()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block()) != crate::data::ConnectionKind::Files
                {
                    continue;
                }
                let disk = tree::members(entry.block())
                    .into_iter()
                    .any(|(setting, option)| {
                        name_text(&setting).as_deref() == Some("driver")
                            && option.value().is_some_and(|v| {
                                tree::written(v.syntax()).replace(char::is_whitespace, "")
                                    == "Driver.disk"
                            })
                    });
                if !disk {
                    continue;
                }
                let floor = tree::members(entry.block())
                    .into_iter()
                    .find(|(setting, _)| name_text(setting).as_deref() == Some("free_space_floor"))
                    .and_then(
                        |(_, option)| match option.value().map(|v| Const::from_expr(&v)) {
                            Some(Const::Percentage(written)) => crate::disk::parse_floor(&written),
                            _ => None,
                        },
                    )
                    .unwrap_or(jadpo_std::files::DEFAULT_FREE_SPACE_FLOOR);
                out.insert(connection, floor);
            }
        }
    }
    out
}

/// Each `thresholds { }` in `text`, read without the project: `None` when
/// the file doesn't parse, so nothing about it is assumed.
pub fn written(text: &str) -> Option<Vec<Written>> {
    let parse = jadpo_syntax::parse_file(text);
    if parse.has_errors() {
        return None;
    }
    let mut out = Vec::new();
    for decl in parse
        .syntax()
        .children()
        .filter_map(ast::SingletonDecl::cast)
        .filter(|decl| decl.keyword().is_some_and(|w| w.text() == THRESHOLDS))
    {
        let mut reader = Reader::quiet("");
        out.extend(
            reader
                .block(decl.member_list())
                .into_iter()
                .map(|read| read.written),
        );
    }
    Some(out)
}

/// Each change from `before` to `after`, one domain's thresholds, that
/// needs a person: a subject (`hello.greeting.p95`, `hello.greeting`)
/// and `ThresholdChange.raised` or `.removed`.
pub fn loosened(
    domain: Option<&str>,
    before: &[Written],
    after: &[Written],
) -> Vec<(String, &'static str)> {
    let subject = |name: &str| match domain {
        Some(domain) => format!("{domain}.{name}"),
        None => name.to_string(),
    };
    let mut out = Vec::new();
    for was in before {
        let Some(now) = after.iter().find(|now| now.name == was.name) else {
            out.push((subject(&was.name), REMOVED));
            continue;
        };
        if now.watches != was.watches {
            out.push((subject(&was.name), REMOVED));
            continue;
        }
        for (limit, value) in &was.limits {
            match now.limits.get(limit) {
                None => out.push((format!("{}.{}", subject(&was.name), limit.word()), REMOVED)),
                Some(now) if now > value => {
                    out.push((format!("{}.{}", subject(&was.name), limit.word()), RAISED));
                }
                Some(_) => {}
            }
        }
        if was.across() && now.across() && now.within() > was.within() {
            out.push((format!("{}.within", subject(&was.name)), RAISED));
        }
    }
    out
}

/// A disk store's floor lowered: `uploads.free_space_floor`.
pub fn floors_lowered(
    before: &BTreeMap<String, u32>,
    after: &BTreeMap<String, u32>,
) -> Vec<String> {
    before
        .iter()
        .filter(|(connection, was)| after.get(*connection).is_some_and(|now| now < was))
        .map(|(connection, _)| format!("{connection}.free_space_floor"))
        .collect()
}

pub const RAISED: &str = "ThresholdChange.raised";
pub const REMOVED: &str = "ThresholdChange.removed";
pub const LOWERED: &str = "ThresholdChange.lowered";

/// The domain a project-relative path's thresholds belong to:
/// `domains/<domain>/monitoring.jadpo`.
pub fn domain_of(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("domains/")?;
    let (domain, file) = rest.split_once('/')?;
    (file == FILE).then_some(domain)
}

/// The thresholds of `project`, checked.
#[salsa::tracked(returns(ref))]
pub fn thresholds(db: &dyn Db, project: Project) -> Arc<Thresholds> {
    let mut out = Thresholds::default();
    let loose = project.layout(db) == Layout::Loose;
    let mut first_in: BTreeMap<String, Span> = BTreeMap::new();
    for found in crate::config::singletons(db, project, THRESHOLDS) {
        let domain = domain_of(&found.path).map(str::to_string);
        if !loose && domain.is_none() {
            out.problems.push(
                Diagnostic::new(
                    &rule::THRESHOLD_DECLARATION_INVALID,
                    format!("`thresholds {{ }}` isn't in a domain's `{FILE}`"),
                )
                .at(found.keyword.clone(), "not in a monitoring file")
                .reason(format!(
                    "A domain's thresholds are reviewed as one list, in `domains/<domain>/{FILE}`, beside the code they watch."
                ))
                .next(format!("Move these thresholds to `domains/<domain>/{FILE}`")),
            );
            continue;
        }
        if let Some(first) = first_in.get(&found.path) {
            out.problems.push(
                Diagnostic::new(
                    &rule::THRESHOLD_DECLARATION_INVALID,
                    "`thresholds { }` is declared twice",
                )
                .at(found.keyword.clone(), "declared again")
                .also(first.clone(), "first declared here")
                .reason("A domain's thresholds are one `thresholds { }`, so a reviewer reads every threshold that watches it in one place.")
                .next("Move the thresholds this one holds into the first, and remove it"),
            );
            continue;
        }
        first_in.insert(found.path.clone(), found.keyword.clone());
        let ownership = match &domain {
            Some(domain) => Ownership::Domain(domain.clone()),
            None => Ownership::Loose,
        };
        let lookup = Lookup::within(db, project, &ownership);
        let mut reader = Reader::quiet(&found.path);
        reader.loud = true;
        let read = reader.block(found.list.clone());
        out.problems.append(&mut reader.problems);
        for one in read {
            let mut problems = Vec::new();
            if let Some(threshold) = check(
                db,
                project,
                &lookup,
                domain.as_deref(),
                &found.path,
                one,
                &mut problems,
            ) {
                out.thresholds.push(threshold);
            }
            out.problems.extend(problems);
        }
    }
    out.thresholds.sort_by(|a, b| a.subject.cmp(&b.subject));
    Arc::new(out)
}

/// The problems with the project's thresholds.
pub fn problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    thresholds(db, project).problems.clone()
}

/// Whether every threshold read without a problem: what the rules and
/// the comparison with the before side are made from.
pub fn clean(db: &dyn Db, project: Project) -> bool {
    thresholds(db, project).problems.is_empty()
}

/// One threshold read from its block, with where its parts are written.
struct Read {
    written: Written,
    doc: Option<String>,
    at: Span,
    /// What it watches, as written, and where.
    subject: Option<(Subject, Span)>,
    /// Each limit and where its value is written.
    spans: BTreeMap<Limit, Span>,
}

#[derive(Clone, Debug)]
enum Subject {
    Route {
        method: HttpMethod,
        path: String,
        path_at: Span,
    },
    Name(String),
}

struct Reader<'a> {
    path: &'a str,
    /// Whether problems are kept: the before side's reading keeps none.
    loud: bool,
    problems: Vec<Diagnostic>,
}

const LIMIT_WORDS: &str = "`duration:`, `queries:`, `p50:`, `p90:`, `p95:`, `p99:`, `server_failures:` and `oldest_waiting:`";

impl<'a> Reader<'a> {
    fn quiet(path: &'a str) -> Reader<'a> {
        Reader {
            path,
            loud: false,
            problems: Vec::new(),
        }
    }

    fn report(&mut self, diagnostic: Diagnostic) {
        if self.loud {
            self.problems.push(diagnostic);
        }
    }

    fn invalid(&mut self, message: String, at: Span, label: &str, reason: &str, next: String) {
        self.report(
            Diagnostic::new(&rule::THRESHOLD_DECLARATION_INVALID, message)
                .at(at, label.to_string())
                .reason(reason.to_string())
                .next(next),
        );
    }

    fn span(&self, node: &jadpo_syntax::SyntaxNode) -> Span {
        node_span(self.path, node)
    }

    fn block(&mut self, list: Option<ast::MemberList>) -> Vec<Read> {
        let mut out = Vec::new();
        let mut seen: Vec<(String, Span)> = Vec::new();
        for item in list.iter().flat_map(|list| list.member_items()) {
            let MemberItem::Member(member) = item else {
                if !matches!(item, MemberItem::Elision(_)) {
                    let at = self.span(item.syntax());
                    let text = tree::written(item.syntax());
                    self.invalid(
                        format!("`{text}` isn't a threshold"),
                        at,
                        "not `name: { }`",
                        "`thresholds { }` holds thresholds, each `name: { }` giving what it watches and its limits.",
                        "Write the threshold as `name: { }`, or remove this line".to_string(),
                    );
                }
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let mut parts = key.names();
            let (Some(name), None) = (parts.next(), parts.next()) else {
                let at = self.span(key.syntax());
                let text = tree::written(key.syntax());
                self.invalid(
                    format!("`{text}` isn't a threshold's name"),
                    at,
                    "not a name",
                    "Each threshold has a name of its own, which an acceptance and the generated rules name it by.",
                    "Give the threshold one name, such as `todo_list:`".to_string(),
                );
                continue;
            };
            let Some(text) = name_text(&name) else {
                continue;
            };
            let at = self.span(name.syntax());
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == text) {
                let first = first.clone();
                self.report(
                    Diagnostic::new(
                        &rule::THRESHOLD_DECLARATION_INVALID,
                        format!("`{text}` is declared twice"),
                    )
                    .at(at, "declared again")
                    .also(first, "first declared here")
                    .reason("Each threshold has one declaration, so an acceptance and the generated rules name one threshold.")
                    .next(format!("Remove one `{text}`, or rename it")),
                );
                continue;
            }
            seen.push((text.clone(), at.clone()));
            if member.block().is_none() {
                self.invalid(
                    format!("`{text}` isn't a threshold's block"),
                    member.value().map_or_else(|| at.clone(), |v| self.span(v.syntax())),
                    "not `{ }`",
                    "A threshold is a block giving the one thing it watches, `route:` or `watches:`, and its limits.",
                    format!("Write `{text}: {{ }}` holding what it watches and its limits"),
                );
                continue;
            }
            let doc = ast::doc_comments(member.syntax())
                .iter()
                .map(|token| token.text().trim_start_matches("///").trim().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            out.push(self.threshold(&text, at, (!doc.is_empty()).then_some(doc), &member));
        }
        out
    }

    fn threshold(
        &mut self,
        name: &str,
        at: Span,
        doc: Option<String>,
        member: &ast::Member,
    ) -> Read {
        let mut read = Read {
            written: Written {
                name: name.to_string(),
                watches: None,
                limits: BTreeMap::new(),
                within_ms: None,
            },
            doc,
            at: at.clone(),
            subject: None,
            spans: BTreeMap::new(),
        };
        let mut seen: Vec<(String, Span)> = Vec::new();
        let mut within: Option<Span> = None;
        let mut subjects = 0;
        let mut broken = false;
        for (key, option) in tree::members(member.block()) {
            let Some(word) = name_text(&key) else {
                continue;
            };
            let key_at = self.span(key.syntax());
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == word) {
                let first = first.clone();
                broken = true;
                self.report(
                    Diagnostic::new(
                        &rule::THRESHOLD_DECLARATION_INVALID,
                        format!("`{word}:` is given twice on `{name}`"),
                    )
                    .at(key_at, "given again")
                    .also(first, "first given here")
                    .reason("A threshold gives each limit once, so there is one bound to judge.")
                    .next(format!("Remove one `{word}:`")),
                );
                continue;
            }
            seen.push((word.clone(), key_at.clone()));
            let value_at = option
                .value()
                .map_or_else(|| key_at.clone(), |v| self.span(v.syntax()));
            if word == "route" || word == "watches" {
                subjects += 1;
                if subjects > 1 {
                    broken = true;
                    self.invalid(
                        format!("`{name}` watches more than one thing"),
                        key_at,
                        "a second subject",
                        "A threshold watches one thing, a route with `route:` or an action, query, schedule or subscriber with `watches:`, so what crossed it is never in doubt.",
                        format!("Give `{word}:` a threshold of its own"),
                    );
                    continue;
                }
                match self.subject(name, &word, &option, &value_at) {
                    Some(subject) => {
                        read.written.watches = Some(match &subject {
                            Subject::Route { method, path, .. } => {
                                format!("route {} {path}", method.word())
                            }
                            Subject::Name(target) => format!("watches {target}"),
                        });
                        read.subject = Some((subject, value_at));
                    }
                    None => broken = true,
                }
                continue;
            }
            if word == "within" {
                within = Some(value_at.clone());
                match duration(&option) {
                    Some(ms) if ms.is_multiple_of(MINUTE_MS) && (MINUTE_MS..=DAY_MS).contains(&ms) => {
                        read.written.within_ms = Some(ms);
                    }
                    _ => {
                        broken = true;
                        self.invalid(
                            format!("`within:` on `{name}` isn't a window from `1m` to `1d`"),
                            value_at,
                            "not a window",
                            "`within:` is how long a window the limits judged across requests are judged over: a whole number of minutes or hours, or a day, from `1m` to `1d`; `5m` when left out.",
                            "Write a window such as `within: 5m`, or leave it out".to_string(),
                        );
                    }
                }
                continue;
            }
            let Some(limit) = Limit::from_word(&word) else {
                broken = true;
                let mut known: Vec<&str> = Limit::ALL.iter().map(|l| l.word()).collect();
                known.extend(["route", "watches", "within"]);
                let next = match nearest(&word, known) {
                    Some(near) => format!("Write `{near}:`, or remove `{word}:`"),
                    None => format!("Remove `{word}:`"),
                };
                self.invalid(
                    format!("A threshold takes no `{word}:`"),
                    key_at,
                    "not a limit",
                    &format!(
                        "A threshold gives what it watches, `route:` or `watches:`, its limits, {LIMIT_WORDS}, and `within:`."
                    ),
                    next,
                );
                continue;
            };
            let value = match limit.unit() {
                Unit::Duration => duration(&option).filter(|ms| *ms > 0),
                Unit::Count => match option.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Number { decimal: false, .. }) => option
                        .value()
                        .and_then(|v| tree::written(v.syntax()).parse::<u64>().ok()),
                    _ => None,
                },
                Unit::Share => match option.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Percentage(written)) => crate::disk::parse_floor(&written)
                        .filter(|share| *share > 0)
                        .map(u64::from),
                    _ => None,
                },
            };
            match value {
                Some(value) => {
                    read.written.limits.insert(limit, value);
                    read.spans.insert(limit, value_at);
                }
                None => {
                    broken = true;
                    let (what, example) = match limit.unit() {
                        Unit::Duration => ("a duration above zero", "300ms"),
                        Unit::Count => ("a whole number", "3"),
                        Unit::Share => ("a percentage above 0% and below 100%", "1%"),
                    };
                    let word = limit.word();
                    self.invalid(
                        format!("`{word}:` on `{name}` isn't {what}"),
                        value_at,
                        &format!("not {what}"),
                        &limit_reason(limit),
                        format!("Write `{word}: {example}`, or the bound you mean"),
                    );
                }
            }
        }
        if subjects == 0 {
            broken = true;
            self.invalid(
                format!("`{name}` doesn't say what it watches"),
                at.clone(),
                "watches nothing",
                "A threshold watches one route of its domain, `route: { method: path: }`, or one action, query, schedule or subscriber, `watches:` and its name.",
                format!("Add what `{name}` watches, such as `watches: <action>`"),
            );
        }
        if read.written.limits.is_empty() && !broken {
            self.invalid(
                format!("`{name}` has no limit"),
                at.clone(),
                "no limit",
                &format!("A threshold is its limits: {LIMIT_WORDS}."),
                format!("Give `{name}` a limit, such as `p95: 300ms`"),
            );
            broken = true;
        }
        if let Some(within) = within
            && !read.written.limits.is_empty()
            && !read.written.across()
        {
            self.invalid(
                format!("`within:` on `{name}` judges nothing"),
                within,
                "no limit across requests",
                "`within:` is the window the limits judged across requests are judged over, the percentiles, `server_failures:` and `oldest_waiting:`; `duration:` and `queries:` judge each request alone.",
                "Remove `within:`, or add a limit judged across requests".to_string(),
            );
            broken = true;
        }
        // A higher percentile is never shorter than a lower one.
        let percentiles: Vec<(Limit, u64)> = [Limit::P50, Limit::P90, Limit::P95, Limit::P99]
            .into_iter()
            .filter_map(|limit| Some((limit, *read.written.limits.get(&limit)?)))
            .collect();
        for pair in percentiles.windows(2) {
            let [(low, low_ms), (high, high_ms)] = pair else {
                continue;
            };
            if high_ms < low_ms {
                let at = read.spans.get(high).cloned().unwrap_or_else(|| at.clone());
                self.invalid(
                    format!("`{}:` on `{name}` is shorter than its `{}:`", high.word(), low.word()),
                    at,
                    "shorter than a lower percentile",
                    "A percentile counts the requests done within its time, so a higher one takes at least as long as a lower one: no `p99:` can be met while a shorter one beside it isn't.",
                    format!("Make `{}:` at least `{}:`'s, or remove one", high.word(), low.word()),
                );
                broken = true;
            }
        }
        if broken {
            read.subject = None;
        }
        read
    }

    fn subject(
        &mut self,
        name: &str,
        word: &str,
        option: &ast::Member,
        at: &Span,
    ) -> Option<Subject> {
        if word == "route" {
            return self.route(name, option, at);
        }
        match option.value() {
            Some(Expr::NameRef(target)) => tree::name_ref_text(&target).map(Subject::Name),
            _ => {
                self.invalid(
                    format!("`watches:` on `{name}` isn't a name"),
                    at.clone(),
                    "not a name",
                    "`watches:` names the action, query, schedule or subscriber the threshold watches, imported like any name the file uses; a route is `route: { method: path: }`.",
                    "Write `watches: <name>`".to_string(),
                );
                None
            }
        }
    }

    fn route(&mut self, name: &str, option: &ast::Member, at: &Span) -> Option<Subject> {
        let reason = "`route:` names the route a threshold watches as a route test does: `route: { method: HttpMethod.get path: \"/todos\" }`, its path as `jadpo routes` prints it, placeholders by name.";
        if option.block().is_none() {
            self.invalid(
                format!("`route:` on `{name}` isn't a route's method and path"),
                at.clone(),
                "not `{ method: path: }`",
                reason,
                "Write `route: { method: HttpMethod.<method> path: \"<path>\" }`".to_string(),
            );
            return None;
        }
        let mut method = None;
        let mut path = None;
        let mut ok = true;
        for (key, member) in tree::members(option.block()) {
            let word = name_text(&key).unwrap_or_default();
            let value_at = member
                .value()
                .map_or_else(|| self.span(key.syntax()), |v| self.span(v.syntax()));
            match word.as_str() {
                "method" => {
                    let read = member.value().and_then(|value| {
                        let (first, rest) = tree::dotted(&value)?;
                        let [variant] = rest.as_slice() else {
                            return None;
                        };
                        (tree::name_ref_text(&first)? == HttpMethod::NAME).then_some(())?;
                        HttpMethod::from_name(&tree::name_ref_text(variant)?)
                    });
                    match read {
                        Some(read) => method = Some(read),
                        None => {
                            ok = false;
                            self.invalid(
                                format!("`method:` on `{name}` isn't an `HttpMethod`"),
                                value_at,
                                "not an `HttpMethod`",
                                reason,
                                "Write the method as `HttpMethod.<method>`".to_string(),
                            );
                        }
                    }
                }
                "path" => match member.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Str(text)) => path = Some((text, value_at)),
                    _ => {
                        ok = false;
                        self.invalid(
                            format!("`path:` on `{name}` isn't the route's path in quotes"),
                            value_at,
                            "not text",
                            reason,
                            "Write the route's path in quotes, as `jadpo routes` prints it"
                                .to_string(),
                        );
                    }
                },
                other => {
                    ok = false;
                    self.invalid(
                        format!("`route:` takes no `{other}:`"),
                        self.span(key.syntax()),
                        "not `method:` or `path:`",
                        reason,
                        format!("Remove `{other}:`"),
                    );
                }
            }
        }
        match (method, path) {
            (Some(method), Some((path, path_at))) if ok => Some(Subject::Route {
                method,
                path,
                path_at,
            }),
            (method, path) => {
                if ok {
                    let missing = if method.is_none() { "method:" } else { "path:" };
                    let _ = path;
                    self.invalid(
                        format!("`route:` on `{name}` has no `{missing}`"),
                        at.clone(),
                        "missing its method or path",
                        reason,
                        format!("Add `{missing}` to `route:`"),
                    );
                }
                None
            }
        }
    }
}

fn limit_reason(limit: Limit) -> String {
    match limit {
        Limit::Duration => "`duration:` is the longest one request or run may take, a duration above zero.".to_string(),
        Limit::Queries => "`queries:` is the most queries one request or run may make, a whole number.".to_string(),
        Limit::ServerFailures => "`server_failures:` is the share of requests within `within:` that may end in a fault or a failure answered with a 5xx, a percentage above `0%` and below `100%`.".to_string(),
        Limit::OldestWaiting => "`oldest_waiting:` is the age a subscriber's oldest waiting delivery may reach, a duration above zero.".to_string(),
        percentile => format!(
            "`{}:` is how long that share of the requests within `within:` may take, a duration above zero.",
            percentile.word()
        ),
    }
}

/// A member's value as a duration in milliseconds, when it is one.
fn duration(option: &ast::Member) -> Option<u64> {
    let value = option.value()?;
    let Const::Duration(_) = Const::from_expr(&value) else {
        return None;
    };
    let millis = jadpo_std::Dur::parse(&tree::written(value.syntax()))?.millis;
    u64::try_from(millis).ok()
}

/// One read threshold checked against the project.
fn check(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    domain: Option<&str>,
    path: &str,
    read: Read,
    out: &mut Vec<Diagnostic>,
) -> Option<Threshold> {
    let (subject, at) = read.subject.clone()?;
    let name = &read.written.name;
    let target = match subject {
        Subject::Route {
            method,
            path: route_path,
            path_at,
        } => route_target(
            db,
            project,
            domain,
            name,
            method,
            &route_path,
            path_at,
            &read,
            out,
        )?,
        Subject::Name(target) => named_target(lookup, domain, name, &target, at, out)?,
    };
    let watches = target.watches;
    // Each limit must apply to what it watches.
    let mut ok = true;
    for (limit, span) in &read.spans {
        if !limit.applies(watches) {
            ok = false;
            out.push(
                Diagnostic::new(
                    &rule::THRESHOLD_DECLARATION_INVALID,
                    format!("`{}:` can't watch {}", limit.word(), watches.with_a()),
                )
                .at(span.clone(), format!("not for {}", watches.with_a()))
                .reason(format!(
                    "{} It watches {}.",
                    limit_reason(*limit),
                    limit.watched_by()
                ))
                .next(format!("Remove `{}:` from `{name}`", limit.word())),
            );
        }
    }
    if !ok {
        return None;
    }
    let subject = match domain {
        Some(domain) => format!("{domain}.{name}"),
        None => name.clone(),
    };
    Some(Threshold {
        domain: domain.map(str::to_string),
        name: name.clone(),
        subject,
        doc: read.doc,
        target,
        limits: read.written.limits.clone(),
        within_ms: read.written.within(),
        path: path.to_string(),
        at: read.at,
    })
}

#[allow(clippy::too_many_arguments)]
fn route_target(
    db: &dyn Db,
    project: Project,
    domain: Option<&str>,
    name: &str,
    method: HttpMethod,
    route_path: &str,
    path_at: Span,
    read: &Read,
    out: &mut Vec<Diagnostic>,
) -> Option<Target> {
    let groups = crate::routes::groups(db, project);
    let label = format!("{} {route_path}", method.word());
    let found = groups.routes().find(|(_, route)| route.label() == label);
    let Some((group, route)) = found else {
        let ours: Vec<String> = groups
            .routes()
            .filter(|(group, route)| group.domain.as_deref() == domain && route.method == method)
            .map(|(_, route)| format!("`{}`", route.path()))
            .collect();
        out.push(
            Diagnostic::new(
                &rule::THRESHOLD_SUBJECT_UNKNOWN,
                format!("No route answers `{label}`"),
            )
            .at(path_at, "no such route")
            .reason(if ours.is_empty() {
                format!("A threshold watches a route of its own domain by its method and its path as `jadpo routes` prints it, and {} has no `{}` route.", domain_words(domain), method.word())
            } else {
                format!(
                    "A threshold watches a route of its own domain by its method and its path as `jadpo routes` prints it, placeholders by name; {}'s `{}` routes are {}.",
                    domain_words(domain),
                    method.word(),
                    crate::and_list(&ours)
                )
            })
            .next("Write the path of the route this threshold watches, as `jadpo routes` lists it"),
        );
        return None;
    };
    if group.domain.as_deref() != domain {
        let theirs = group.domain.clone().unwrap_or_default();
        out.push(
            Diagnostic::new(
                &rule::THRESHOLD_SUBJECT_UNKNOWN,
                format!("`{label}` is a route of `{theirs}`"),
            )
            .at(path_at, "another domain's route")
            .reason("A domain's monitoring file watches its own routes, actions, queries, schedules and subscribers, so each domain's thresholds are the whole of what watches it.")
            .next(format!("Move `{name}` to `domains/{theirs}/{FILE}`")),
        );
        return None;
    }
    if let Some(ms) = read.written.limits.get(&Limit::Duration)
        && *ms >= u64::from(route.deadline_ms)
    {
        let at = read.spans.get(&Limit::Duration).cloned().unwrap_or(path_at);
        out.push(
            Diagnostic::new(
                &rule::THRESHOLD_DECLARATION_INVALID,
                format!("`duration:` on `{name}` isn't shorter than `{label}`'s deadline"),
            )
            .at(at, format!("its deadline is {}", jadpo_std::Dur { millis: i128::from(route.deadline_ms) }))
            .reason("A request still running at its route's deadline is stopped and answered `timed_out`, so a `duration:` at or past the deadline can never be crossed.")
            .next(format!("Make `duration:` shorter than {}, or raise the route's `deadline:`", jadpo_std::Dur { millis: i128::from(route.deadline_ms) })),
        );
        return None;
    }
    Some(Target {
        watches: Watches::Route,
        key: route.key.clone(),
        route: Some((method, route.path())),
        name: route.label(),
    })
}

fn domain_words(domain: Option<&str>) -> String {
    match domain {
        Some(domain) => format!("`{domain}`"),
        None => "the project".to_string(),
    }
}

fn named_target(
    lookup: &Lookup<'_>,
    domain: Option<&str>,
    name: &str,
    target: &str,
    at: Span,
    out: &mut Vec<Diagnostic>,
) -> Option<Target> {
    let declared = lookup.declared(target);
    let found = declared
        .iter()
        .filter(|symbol| symbol.library.is_none())
        .find_map(|symbol| {
            let watches = Watches::ALL
                .into_iter()
                .find(|w| w.item().as_ref() == Some(&symbol.kind))?;
            Some((symbol, watches))
        });
    let Some((symbol, watches)) = found else {
        let reason = match declared.iter().find(|symbol| symbol.library.is_none()) {
            Some(other) => format!(
                "`watches:` names an action, query, schedule or subscriber of the threshold's own domain, and `{target}` is {}.",
                other.kind.with_a()
            ),
            None => format!(
                "`watches:` names an action, query, schedule or subscriber of the threshold's own domain, and {} declares none called `{target}`.",
                domain_words(domain)
            ),
        };
        out.push(
            Diagnostic::new(
                &rule::THRESHOLD_SUBJECT_UNKNOWN,
                format!("Nothing called `{target}` to watch"),
            )
            .at(at, "nothing to watch")
            .reason(reason)
            .next(format!(
                "Name the action, query, schedule or subscriber of {} that `{name}` watches",
                domain_words(domain)
            )),
        );
        return None;
    };
    let word = watches.word();
    if symbol.ownership.domain() != domain {
        let theirs = symbol.ownership.domain().unwrap_or("the root").to_string();
        out.push(
            Diagnostic::new(
                &rule::THRESHOLD_SUBJECT_UNKNOWN,
                format!("`{target}` is {} of `{theirs}`", symbol.kind.with_a()),
            )
            .at(at, format!("another domain's {word}"))
            .reason("A domain's monitoring file watches its own routes, actions, queries, schedules and subscribers, so each domain's thresholds are the whole of what watches it.")
            .next(format!("Move `{name}` to `domains/{theirs}/{FILE}`")),
        );
        return None;
    }
    let key = crate::graph::declaration_key(lookup, target)?;
    let qualified = match domain {
        Some(domain) => format!("{domain}.{target}"),
        None => target.to_string(),
    };
    Some(Target {
        watches,
        key,
        route: None,
        name: qualified,
    })
}

// --- the rules in `build/monitoring/` ------------------------------------------

/// A double-quoted YAML scalar: JSON's escapes, which YAML reads the same.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A PromQL string literal.
fn promql(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A duration as Prometheus and OpenSLO write one: `5m`, `1h`, `1d`.
fn window(ms: u64) -> String {
    if ms.is_multiple_of(DAY_MS) {
        format!("{}d", ms / DAY_MS)
    } else if ms.is_multiple_of(3_600_000) {
        format!("{}h", ms / 3_600_000)
    } else if ms.is_multiple_of(MINUTE_MS) {
        format!("{}m", ms / MINUTE_MS)
    } else {
        format!("{}s", ms / 1000)
    }
}

/// Seconds, written as briefly as they are exact: `0.3`, `600`.
fn seconds(ms: u64) -> String {
    if ms.is_multiple_of(1000) {
        (ms / 1000).to_string()
    } else {
        let text = format!("{}.{:03}", ms / 1000, ms % 1000);
        text.trim_end_matches('0').to_string()
    }
}

/// A share in millionths as a fraction: `0.01`.
fn fraction(millionths: u64) -> String {
    let text = format!("{}.{:06}", millionths / 1_000_000, millionths % 1_000_000);
    let text = text.trim_end_matches('0');
    text.trim_end_matches('.').to_string()
}

/// A limit's value as source writes it: `300ms`, `3`, `1%`.
pub fn value_text(limit: Limit, value: u64) -> String {
    match limit.unit() {
        Unit::Duration => jadpo_std::Dur {
            millis: i128::from(value),
        }
        .to_string(),
        Unit::Count => value.to_string(),
        Unit::Share => percent(value),
    }
}

/// `within:` as source writes it: `5m`.
pub fn within_text(ms: u64) -> String {
    jadpo_std::Dur {
        millis: i128::from(ms),
    }
    .to_string()
}

/// The disk stores' floors today, from `app.jadpo`: empty for loose
/// files, or while the disk stores' settings have problems.
pub fn project_floors(db: &dyn Db, project: Project) -> BTreeMap<String, u32> {
    if project.layout(db) != Layout::Project || !crate::disk::disk_problems(db, project).is_empty()
    {
        return BTreeMap::new();
    }
    project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE)
        .map(|file| floors(file.text(db)))
        .unwrap_or_default()
}

/// The member a threshold is declared by, for its node's place.
pub fn member_of(db: &dyn Db, project: Project, threshold: &Threshold) -> Option<ast::Member> {
    crate::config::singletons(db, project, THRESHOLDS)
        .into_iter()
        .filter(|found| found.path == threshold.path)
        .find_map(|found| {
            tree::members(found.list)
                .into_iter()
                .find(|(key, _)| name_text(key).as_deref() == Some(threshold.name.as_str()))
                .map(|(_, member)| member)
        })
}

/// A share in millionths as written: `1%`, `0.5%`.
pub fn percent(millionths: u64) -> String {
    let text = format!("{}.{:04}", millionths / 10_000, millionths % 10_000);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}%")
}

/// A threshold's name in the rules, as a Prometheus name: `hello_greeting_p95`.
fn rule_name(subject: &str, limit: &str) -> String {
    format!("{subject}_{limit}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The selector of what a threshold watches, and whether it is a route's.
fn selector(target: &Target, extra: &str) -> (String, &'static str) {
    match &target.route {
        Some((method, path)) => (
            format!(
                "http_request_method=\"{}\", http_route=\"{}\"{extra}",
                method.word(),
                promql(path)
            ),
            "http_server_request_duration_seconds",
        ),
        None => (
            format!(
                "jadpo_kind=\"{}\", jadpo_name=\"{}\"{extra}",
                target.watches.word(),
                promql(&target.name)
            ),
            "jadpo_run_duration_seconds",
        ),
    }
}

/// The PromQL that holds while `limit` is crossed.
fn alert_expr(threshold: &Threshold, limit: Limit, value: u64) -> String {
    let range = window(threshold.within_ms);
    let target = &threshold.target;
    if let Some(quantile) = limit.quantile() {
        let (labels, metric) = selector(target, "");
        return format!(
            "histogram_quantile({quantile}, sum by (le) (rate({metric}_bucket{{{labels}}}[{range}]))) > {}",
            seconds(value)
        );
    }
    match limit {
        Limit::ServerFailures => {
            let failed = match target.route {
                Some(_) => ", http_response_status_code=~\"5..\"",
                None => ", jadpo_outcome=\"server_failure\"",
            };
            let (bad, metric) = selector(target, failed);
            let (all, _) = selector(target, "");
            format!(
                "sum(rate({metric}_count{{{bad}}}[{range}])) / sum(rate({metric}_count{{{all}}}[{range}])) > {}",
                fraction(value)
            )
        }
        Limit::OldestWaiting => format!(
            "max(jadpo_delivery_oldest_waiting_seconds{{jadpo_name=\"{}\"}}) > {}",
            promql(&target.name),
            seconds(value)
        ),
        _ => String::new(),
    }
}

/// A limit in words, for an alert's description: `p95 above 300ms`.
fn limit_words(limit: Limit, value: u64, within_ms: u64) -> String {
    let within = window(within_ms);
    match limit {
        Limit::ServerFailures => {
            format!("server failures above {} within {within}", percent(value))
        }
        Limit::OldestWaiting => format!(
            "oldest waiting delivery older than {}",
            jadpo_std::Dur {
                millis: i128::from(value)
            }
        ),
        other => format!(
            "{} above {} within {within}",
            other.word(),
            jadpo_std::Dur {
                millis: i128::from(value)
            }
        ),
    }
}

/// An objective in words, for an SLO: `95% done within 300ms`.
fn objective_words(limit: Limit, value: u64) -> String {
    match limit {
        Limit::ServerFailures => format!("at most {} failing on the server", percent(value)),
        other => {
            let share = match other {
                Limit::P50 => "50%",
                Limit::P90 => "90%",
                Limit::P95 => "95%",
                _ => "99%",
            };
            format!(
                "{share} done within {}",
                jadpo_std::Dur {
                    millis: i128::from(value)
                }
            )
        }
    }
}

/// `build/monitoring/prometheus.yaml` and `openslo.yaml` for `thresholds`
/// and the disk stores' `floors`; empty when there is nothing to judge
/// across requests.
pub fn rules(thresholds: &[Threshold], floors: &BTreeMap<String, u32>) -> Vec<(String, String)> {
    let across: Vec<&Threshold> = thresholds.iter().filter(|t| t.across()).collect();
    if across.is_empty() && floors.is_empty() {
        return Vec::new();
    }
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for threshold in &across {
        let group = format!("jadpo.{}", threshold.domain.as_deref().unwrap_or("app"));
        for (limit, value) in threshold
            .limits
            .iter()
            .filter(|(limit, _)| !limit.per_request())
        {
            let summary = threshold
                .doc
                .clone()
                .unwrap_or_else(|| threshold.subject.clone());
            let subject = format!("{}.{}", threshold.subject, limit.word());
            groups.entry(group.clone()).or_default().push(format!(
                "      - alert: {}\n        expr: {}\n        labels:\n          jadpo_threshold: {}\n        annotations:\n          summary: {}\n          description: {}\n",
                rule_name(&threshold.subject, limit.word()),
                quoted(&alert_expr(threshold, *limit, *value)),
                quoted(&subject),
                quoted(&summary),
                quoted(&format!("{} {}", threshold.target.name, limit_words(*limit, *value, threshold.within_ms))),
            ));
        }
    }
    for (connection, floor) in floors {
        let floor = u64::from(*floor);
        groups.entry("jadpo.files".to_string()).or_default().push(format!(
            "      - alert: {}\n        expr: {}\n        labels:\n          jadpo_threshold: {}\n        annotations:\n          summary: {}\n          description: {}\n",
            rule_name(connection, "free_space_floor"),
            quoted(&format!(
                "min(jadpo_files_free_ratio{{jadpo_connection=\"{}\"}}) < {}",
                promql(connection),
                fraction(floor)
            )),
            quoted(&format!("{connection}.free_space_floor")),
            quoted(&format!("`{connection}` has less free than its floor, so puts answer storage_full")),
            quoted(&format!("{connection}'s disk below {} free", percent(floor))),
        ));
    }
    let mut prometheus = String::from(
        "# Generated by Jadpo from the thresholds in each domain's monitoring.jadpo; never edit by hand.\ngroups:\n",
    );
    for (group, rules) in &groups {
        prometheus.push_str(&format!("  - name: {}\n    rules:\n", quoted(group)));
        for rule in rules {
            prometheus.push_str(rule);
        }
    }
    let mut documents: Vec<String> = Vec::new();
    let mut services: Vec<String> = across
        .iter()
        .filter(|t| {
            t.limits
                .keys()
                .any(|l| l.quantile().is_some() || *l == Limit::ServerFailures)
        })
        .map(|t| t.domain.clone().unwrap_or_else(|| "app".to_string()))
        .collect();
    services.dedup();
    for service in &services {
        documents.push(format!(
            "apiVersion: openslo/v1\nkind: Service\nmetadata:\n  name: {}\nspec:\n  description: {}\n",
            quoted(&slo_name(service)),
            quoted(&format!("The {service} domain's thresholds"))
        ));
    }
    for threshold in &across {
        let service = slo_name(threshold.domain.as_deref().unwrap_or("app"));
        for (limit, value) in &threshold.limits {
            let range = window(threshold.within_ms);
            let (good, total, target) = match (limit.quantile(), limit) {
                (Some(quantile), _) => {
                    let le = format!(", le=\"{}\"", seconds(*value));
                    let (good, metric) = selector(&threshold.target, &le);
                    let (all, _) = selector(&threshold.target, "");
                    (
                        format!("sum(rate({metric}_bucket{{{good}}}[{range}]))"),
                        format!("sum(rate({metric}_count{{{all}}}[{range}]))"),
                        quantile.to_string(),
                    )
                }
                (None, Limit::ServerFailures) => {
                    let failed = match threshold.target.route {
                        Some(_) => ", http_response_status_code!~\"5..\"",
                        None => ", jadpo_outcome!=\"server_failure\"",
                    };
                    let (good, metric) = selector(&threshold.target, failed);
                    let (all, _) = selector(&threshold.target, "");
                    (
                        format!("sum(rate({metric}_count{{{good}}}[{range}]))"),
                        format!("sum(rate({metric}_count{{{all}}}[{range}]))"),
                        fraction(1_000_000 - *value),
                    )
                }
                _ => continue,
            };
            let name = slo_name(&format!("{}.{}", threshold.subject, limit.word()));
            let source = |query: &str| {
                format!(
                    "        metricSource:\n          type: Prometheus\n          spec:\n            query: {}\n",
                    quoted(query)
                )
            };
            documents.push(format!(
                "apiVersion: openslo/v1\nkind: SLO\nmetadata:\n  name: {}\nspec:\n  description: {}\n  service: {}\n  indicator:\n    metadata:\n      name: {}\n    spec:\n      ratioMetric:\n        counter: true\n        good:\n{}        total:\n{}  timeWindow:\n    - duration: {}\n      isRolling: true\n  budgetingMethod: Occurrences\n  objectives:\n    - displayName: {}\n      target: {}\n",
                quoted(&name),
                quoted(&threshold.doc.clone().unwrap_or_else(|| threshold.subject.clone())),
                quoted(&service),
                quoted(&name),
                indent(&source(&good)),
                indent(&source(&total)),
                window(threshold.within_ms),
                quoted(&format!("{} {}", threshold.target.name, objective_words(*limit, *value))),
                target,
            ));
        }
    }
    let mut out = vec![("prometheus.yaml".to_string(), prometheus)];
    if !documents.is_empty() {
        let mut openslo = String::from(
            "# Generated by Jadpo from the thresholds in each domain's monitoring.jadpo; never edit by hand.\n",
        );
        for document in documents {
            openslo.push_str("---\n");
            openslo.push_str(&document);
        }
        out.push(("openslo.yaml".to_string(), openslo));
    }
    out
}

/// Two more spaces before each line of a metric source, which sits under
/// `good:` and `total:`.
fn indent(text: &str) -> String {
    text.lines().map(|line| format!("  {line}\n")).collect()
}

/// An OpenSLO name: lower-case letters, digits and `-`, as DNS labels are.
fn slo_name(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Vec<Written> {
        written(text).expect("parses")
    }

    #[test]
    fn a_raised_limit_and_a_longer_window_are_raised_and_a_tightened_one_needs_nothing() {
        let before = read(
            "thresholds {\n    list: {\n        watches: list_todos\n        duration: 200ms\n        p95: 100ms\n        queries: 3\n    }\n}\n",
        );
        let after = read(
            "thresholds {\n    list: {\n        watches: list_todos\n        duration: 150ms\n        p95: 120ms\n        queries: 3\n        within: 10m\n    }\n}\n",
        );
        assert_eq!(
            loosened(Some("planning"), &before, &after),
            [
                ("planning.list.p95".to_string(), RAISED),
                ("planning.list.within".to_string(), RAISED)
            ]
        );
        let tighter = read(
            "thresholds {\n    list: {\n        watches: list_todos\n        duration: 150ms\n        p95: 90ms\n        queries: 2\n        within: 1m\n    }\n}\n",
        );
        assert_eq!(loosened(Some("planning"), &before, &tighter), []);
    }

    #[test]
    fn a_limit_gone_and_a_threshold_gone_or_watching_something_else_are_removed() {
        let before = read(
            "thresholds {\n    a: {\n        watches: x\n        p95: 1s\n        duration: 2s\n    }\n    b: {\n        watches: y\n        p99: 1s\n    }\n    c: {\n        watches: z\n        p50: 1s\n    }\n}\n",
        );
        let after = read(
            "thresholds {\n    a: {\n        watches: x\n        p95: 1s\n    }\n    c: {\n        watches: w\n        p50: 1s\n    }\n}\n",
        );
        assert_eq!(
            loosened(Some("d"), &before, &after),
            [
                ("d.a.duration".to_string(), REMOVED),
                ("d.b".to_string(), REMOVED),
                ("d.c".to_string(), REMOVED),
            ]
        );
    }

    #[test]
    fn a_floor_lowered_needs_a_person_and_a_store_gone_takes_its_floor() {
        let before: BTreeMap<String, u32> =
            [("uploads".to_string(), 50_000), ("old".to_string(), 50_000)].into();
        let after: BTreeMap<String, u32> = [("uploads".to_string(), 20_000)].into();
        assert_eq!(
            floors_lowered(&before, &after),
            ["uploads.free_space_floor"]
        );
        assert_eq!(floors_lowered(&after, &before), Vec::<String>::new());
    }

    #[test]
    fn values_are_written_as_the_rules_read_them() {
        assert_eq!(seconds(300), "0.3");
        assert_eq!(seconds(600_000), "600");
        assert_eq!(seconds(1_250), "1.25");
        assert_eq!(fraction(10_000), "0.01");
        assert_eq!(fraction(950_000), "0.95");
        assert_eq!(percent(5_000), "0.5%");
        assert_eq!(percent(50_000), "5%");
        assert_eq!(window(300_000), "5m");
        assert_eq!(window(3_600_000), "1h");
        assert_eq!(window(DAY_MS), "1d");
    }
}
