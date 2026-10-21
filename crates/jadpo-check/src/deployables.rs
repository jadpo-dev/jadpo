//! Deployables (area 14, question 14; area 11, question 24; roadmap chunk
//! 8.1o2): `application { deployables: { } }` in `app.jadpo` declares
//! each program the app is built into, what it serves and runs and what
//! it is built for:
//!
//! ```text
//! deployables: {
//!     kiosk: {
//!         serves: [main, webhooks]
//!         runs: [Trigger.schedules, Trigger.subscribers]
//!         target: Target.native
//!     }
//! }
//! ```
//!
//! With none declared there is one deployable, `app`, which serves every
//! API and runs everything, and which has no target until a build asks
//! for one (Richard, 2026-10-21): `jadpo build` builds it natively, and
//! no driver is refused for it. Once declared, every API with routes must
//! be served by one, and the app's schedules and subscribers run by one
//! ([`unserved`]).
//!
//! [`reaches`] follows what each deployable reaches, from the routes of
//! the APIs it serves, the background work it runs and what Jadpo runs by
//! name (chunk 8.1o3: the actions `on_first_sign_in:` and `magic_link: {
//! send: }` name, the email templates and the provider packages' `decode:`
//! and `encode:`), through every callable those call, to the connections
//! they use, which is what its program holds (`jadpo_plan::Plan::
//! for_deployable`); a deployable that
//! declares a target is refused each one whose driver that target can't
//! run, naming the alternative ([`target_problems`]): the disk and a
//! service Jadpo runs (`runs:`) without `host:` on Workers, a Cloudflare
//! queue in a native deployable, and SMTP on port 25, which Workers
//! blocks; and one built for Workers that runs the schedules is refused
//! each schedule more often than once a minute, which its minute's cron
//! can't run (chunk 8.2f). Its `max_concurrent:` caps on Workers count per
//! instance, which [`per_instance`] lists for the network report.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::body::{Body, Callee};
use jadpo_graph::{Detail, EdgeKind, Key, Kind};
use jadpo_std::batch::BatchOp;
use jadpo_std::{DeployTarget, Runs, Trigger};
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::config::singletons;
use crate::data::ConnectionKind;
use crate::tree::{self, node_span};
use crate::types::Const;

/// The one deployable of an app that declares none.
pub const APP: &str = "app";

/// What a deployable's entry may hold.
pub const SETTINGS: [&str; 3] = ["serves", "runs", "target"];

/// The API the routes of every connection's webhooks are in (area 7,
/// "Webhooks").
const WEBHOOKS: &str = crate::webhooks::DEFAULT_PREFIX;

/// One deployable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deployable {
    pub name: String,
    /// Where its name is written; `None` for `app`, which nothing declares.
    pub at: Option<Span>,
    /// The APIs it serves, in the order written, each once.
    pub serves: Vec<String>,
    /// The background work it runs.
    pub runs: Runs,
    /// The target it declares, and where; `None` for one that declares
    /// none, which is built natively and holds no driver to a target.
    pub target: Option<(DeployTarget, Span)>,
}

impl Deployable {
    /// What `jadpo build` builds it for: its target, or native.
    pub fn builds_for(&self) -> DeployTarget {
        self.target
            .as_ref()
            .map_or(DeployTarget::Native, |(target, _)| *target)
    }

    /// Whether it serves API `api`.
    pub fn serves(&self, api: &str) -> bool {
        self.serves.iter().any(|served| served == api)
    }

    /// What it runs, as `runs:` writes it: `schedules, subscribers`.
    pub fn runs_words(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.runs.schedules {
            out.push(Trigger::Schedules.name());
        }
        if self.runs.subscribers {
            out.push(Trigger::Subscribers.name());
        }
        out
    }
}

/// The app's deployables and the problems in declaring them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Deployables {
    /// Whether `app.jadpo` declares `deployables:`.
    pub declared: bool,
    /// The declared ones in name order, or `app` alone.
    pub list: Vec<Deployable>,
    /// Where `deployables:` is written.
    pub at: Option<Span>,
    pub problems: Vec<Diagnostic>,
}

impl Deployables {
    pub fn get(&self, name: &str) -> Option<&Deployable> {
        self.list.iter().find(|deployable| deployable.name == name)
    }

    /// Their names, in order.
    pub fn names(&self) -> Vec<&str> {
        self.list
            .iter()
            .map(|deployable| deployable.name.as_str())
            .collect()
    }
}

/// The APIs a deployable can serve: each the app declares, or `main`,
/// and `webhooks` when connections receive webhooks and the app declares
/// no API of that name.
pub fn api_names(db: &dyn Db, project: Project) -> Vec<String> {
    let mut out: Vec<String> = crate::routes::api_table(db, project)
        .apis
        .iter()
        .map(|api| api.name.clone())
        .collect();
    if !crate::webhooks::webhooks(db, project).routes.is_empty()
        && !out.iter().any(|api| api == WEBHOOKS)
    {
        out.push(WEBHOOKS.to_string());
    }
    out.sort();
    out
}

/// The app's deployables, as `app.jadpo` declares them.
#[salsa::tracked(returns(ref))]
pub fn deployables(db: &dyn Db, project: Project) -> Deployables {
    let mut out = Deployables::default();
    let apis = api_names(db, project);
    let mut declared: BTreeMap<String, Deployable> = BTreeMap::new();
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if tree::name_text(&key).as_deref() != Some("deployables") {
                continue;
            }
            out.declared = true;
            let at = node_span(&found.path, key.syntax());
            out.at.get_or_insert_with(|| at.clone());
            let Some(block) = member.block() else {
                out.problems.push(
                    invalid("`deployables:` isn't a block", at, "a block goes here")
                        .reason("`deployables:` names each program the app is built into, with what it serves, what it runs and its target: `deployables: { api: { serves: [main] target: Target.native } }`.")
                        .next("Write `deployables: { <name>: { serves: [<api>] runs: [Trigger.<trigger>] target: Target.<target> } }`"),
                );
                continue;
            };
            let entries = tree::members(Some(block));
            if entries.is_empty() {
                out.problems.push(
                    invalid("`deployables:` names no deployable", at.clone(), "empty")
                        .reason("With `deployables:` declared, the app is built into exactly the deployables it names, so an empty one would build nothing; without it, the app has one deployable, `app`, which serves every API and runs everything.")
                        .next("Name a deployable, `deployables: { api: { serves: [main] } }`, or remove `deployables:`"),
                );
            }
            let mut reader = Reader {
                path: &found.path,
                apis: &apis,
                problems: &mut out.problems,
            };
            for (name, entry) in entries {
                let Some(text) = tree::name_text(&name) else {
                    continue;
                };
                let span = node_span(&found.path, name.syntax());
                if let Some(deployable) = reader.deployable(&text, span, &entry) {
                    declared.insert(text, deployable);
                }
            }
        }
    }
    out.list = if out.declared {
        declared.into_values().collect()
    } else {
        vec![Deployable {
            name: APP.to_string(),
            at: None,
            serves: apis,
            runs: Runs::EVERYTHING,
            target: None,
        }]
    };
    out
}

/// The problems in declaring the app's deployables, and those with what
/// they serve, run and reach.
#[salsa::tracked(returns(ref))]
pub fn deployable_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let deployables = deployables(db, project);
    let mut out = deployables.problems.clone();
    // What a deployable serves, runs and reaches is checked once every
    // one reads cleanly, so a misspelling isn't followed by its knock-on
    // errors.
    if deployables.declared && out.is_empty() {
        out.extend(unserved(db, project, deployables));
        out.extend(settle_problems(db, project, deployables));
        out.extend(target_problems(db, project));
    }
    out
}

struct Reader<'a> {
    path: &'a str,
    apis: &'a [String],
    problems: &'a mut Vec<Diagnostic>,
}

impl Reader<'_> {
    fn deployable(&mut self, name: &str, at: Span, entry: &ast::Member) -> Option<Deployable> {
        let Some(list) = entry.block() else {
            self.problems.push(
                invalid(format!("`{name}` isn't a block"), at, "a block goes here")
                    .reason("Each deployable says what it serves, what it runs and its target: `api: { serves: [main] target: Target.native }`.")
                    .next(format!("Write `{name}: {{ serves: [<api>] runs: [Trigger.<trigger>] target: Target.<target> }}`")),
            );
            return None;
        };
        let mut deployable = Deployable {
            name: name.to_string(),
            at: Some(at.clone()),
            serves: Vec::new(),
            runs: Runs {
                schedules: false,
                subscribers: false,
            },
            target: None,
        };
        let before = self.problems.len();
        for (key, member) in tree::members(Some(list)) {
            let Some(setting) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(self.path, key.syntax());
            match setting.as_str() {
                "serves" => deployable.serves = self.serves(name, &key_span, &member),
                "runs" => deployable.runs = self.runs(name, &key_span, &member),
                "target" => deployable.target = self.target(&key_span, &member),
                other => {
                    let near = nearest(other, SETTINGS);
                    let problem = invalid(
                        format!("`{other}` isn't a setting of a deployable"),
                        key_span.clone(),
                        "not a setting",
                    )
                    .reason(format!(
                        "A deployable takes `serves:`, the APIs it serves, `runs:`, the background work it runs, and `target:`, what it is built for.{}",
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ));
                    self.problems.push(match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{other}`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{other}:`")),
                    });
                }
            }
        }
        let idle = Runs {
            schedules: false,
            subscribers: false,
        };
        if self.problems.len() == before && deployable.serves.is_empty() && deployable.runs == idle
        {
            self.problems.push(
                Diagnostic::new(
                    &rule::DEPLOYABLE_EMPTY,
                    format!("`{name}` serves nothing and runs nothing"),
                )
                .at(at, "does nothing")
                .reason("A deployable serves APIs, `serves: [main]`, runs background work, `runs: [Trigger.schedules]`, or both; one that does neither would only answer its health checks.")
                .next(format!("Give `{name}` `serves:` or `runs:`, or remove it")),
            );
        }
        Some(deployable)
    }

    /// The items of `serves:` or `runs:`, each with where it is written;
    /// `None` when the value isn't a list, which is reported.
    fn items(
        &mut self,
        deployable: &str,
        setting: &str,
        key_span: &Span,
        member: &ast::Member,
        form: &str,
    ) -> Option<Vec<(Expr, Span)>> {
        let value = member.value();
        let Some(Expr::ListExpr(list)) = &value else {
            let span = value
                .as_ref()
                .map_or_else(|| key_span.clone(), |v| node_span(self.path, v.syntax()));
            self.problems.push(
                invalid(
                    format!("`{deployable}`'s `{setting}:` isn't a list"),
                    span,
                    "not a list",
                )
                .reason(format!("`{setting}:` is a list, `{setting}: [{form}]`."))
                .next(format!("Write `{setting}: [{form}]`")),
            );
            return None;
        };
        Some(
            list.exprs()
                .map(|item| {
                    let span = node_span(self.path, item.syntax());
                    (item, span)
                })
                .collect(),
        )
    }

    fn serves(&mut self, deployable: &str, key_span: &Span, member: &ast::Member) -> Vec<String> {
        let Some(items) = self.items(deployable, "serves", key_span, member, "<api>") else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for (item, span) in items {
            let name = match &item {
                Expr::NameRef(name) => tree::name_ref_text(name),
                _ => None,
            };
            let Some(api) = name else {
                self.problems.push(
                    invalid(
                        format!("`{deployable}`'s `serves:` lists something other than an API"),
                        span,
                        "not an API's name",
                    )
                    .reason("`serves:` lists the APIs the deployable serves by name, as `apis:` declares them: `serves: [main, webhooks]`.")
                    .next("Write each API's name, `serves: [<api>]`"),
                );
                continue;
            };
            if out.contains(&api) {
                self.problems.push(twice(&api, span, deployable, "serves"));
                continue;
            }
            if !self.apis.contains(&api) {
                let near = nearest(&api, self.apis.iter().map(String::as_str));
                let problem = Diagnostic::new(
                    &rule::DEPLOYABLE_API_UNKNOWN,
                    format!("`{api}` isn't one of the app's APIs"),
                )
                .at(span.clone(), "no such API")
                .reason(format!(
                    "`serves:` names APIs the app has: {}.{}",
                    listed(self.apis),
                    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                ));
                self.problems.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{api}`"))
                        .edit(span, near),
                    None => problem.next(format!(
                        "Serve one of the app's APIs, or declare `{api}` in `apis:`"
                    )),
                });
                continue;
            }
            out.push(api);
        }
        out
    }

    fn runs(&mut self, deployable: &str, key_span: &Span, member: &ast::Member) -> Runs {
        let mut out = Runs {
            schedules: false,
            subscribers: false,
        };
        let Some(items) = self.items(
            deployable,
            "runs",
            key_span,
            member,
            "Trigger.schedules, Trigger.subscribers",
        ) else {
            return out;
        };
        for (item, span) in items {
            let written = tree::written(item.syntax());
            let names = match Const::from_expr(&item) {
                Const::Path(names) => names,
                _ => Vec::new(),
            };
            let trigger = match names.as_slice() {
                [host, variant] if host == Trigger::NAME => Trigger::from_name(variant),
                _ => None,
            };
            let Some(trigger) = trigger else {
                let variant = match names.as_slice() {
                    [_, variant] => variant.as_str(),
                    _ => written.as_str(),
                };
                let near = nearest(variant, Trigger::NAMES.iter().copied());
                let problem = Diagnostic::new(
                    &rule::DEPLOYABLE_TRIGGER_UNKNOWN,
                    format!("`{written}` isn't background work a deployable runs"),
                )
                .at(span.clone(), "not a `Trigger`")
                .reason(format!(
                    "`runs:` lists `Trigger.schedules`, the schedules, purges and polls, and `Trigger.subscribers`, the deliveries of events and emails.{}",
                    near.map_or_else(String::new, |near| format!(" Did you mean `Trigger.{near}`?"))
                ));
                self.problems.push(match near {
                    Some(near) => problem
                        .next(format!("Use `Trigger.{near}` instead"))
                        .edit(span, format!("Trigger.{near}")),
                    None => problem.next("Use `Trigger.schedules` or `Trigger.subscribers`"),
                });
                continue;
            };
            let seen = match trigger {
                Trigger::Schedules => &mut out.schedules,
                Trigger::Subscribers => &mut out.subscribers,
            };
            if *seen {
                self.problems
                    .push(twice(&written, span, deployable, "runs"));
            }
            *seen = true;
        }
        out
    }

    fn target(&mut self, key_span: &Span, member: &ast::Member) -> Option<(DeployTarget, Span)> {
        let value = member.value();
        let span = value
            .as_ref()
            .map_or_else(|| key_span.clone(), |v| node_span(self.path, v.syntax()));
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let names = match value.as_ref().map(Const::from_expr) {
            Some(Const::Path(names)) => names,
            _ => Vec::new(),
        };
        if let [host, variant] = names.as_slice()
            && host == DeployTarget::NAME
            && let Some(target) = DeployTarget::from_name(variant)
        {
            return Some((target, span));
        }
        let variant = match names.as_slice() {
            [_, variant] => variant.as_str(),
            _ => written.as_str(),
        };
        // Workers is where WebAssembly runs (decision register,
        // 2026-10-13).
        let near = match variant {
            "workers" | "cloudflare" | "worker" | "webassembly" => Some("wasm"),
            _ => nearest(variant, DeployTarget::NAMES.iter().copied()),
        };
        let problem = Diagnostic::new(
            &rule::DEPLOYABLE_TARGET_UNKNOWN,
            format!("`{written}` isn't a target"),
        )
        .at(span.clone(), "not a `Target`")
        .reason(format!(
            "A deployable is built for `Target.native`, a server, or `Target.wasm`, WebAssembly on Cloudflare Workers.{}",
            near.map_or_else(String::new, |near| format!(" Did you mean `Target.{near}`?"))
        ));
        self.problems.push(match near {
            Some(near) => problem
                .next(format!("Use `Target.{near}` instead"))
                .edit(span, format!("Target.{near}")),
            None => problem.next("Use `Target.native` or `Target.wasm`"),
        });
        None
    }
}

fn invalid(message: impl Into<String>, at: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::DEPLOYABLE_INVALID, message).at(at, label)
}

fn twice(written: &str, span: Span, deployable: &str, setting: &str) -> Diagnostic {
    invalid(
        format!("`{deployable}`'s `{setting}:` lists `{written}` twice"),
        span.clone(),
        "listed again",
    )
    .reason(format!("Each entry of `{setting}:` is listed once."))
    .next(format!("Remove the second `{written}`"))
    .edit(span, "")
}

/// `a`, `` `a` and `b` ``, `` `a`, `b` and `c` ``.
fn listed(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("`{item}`")).collect();
    match quoted.as_slice() {
        [] => "none".to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

// --- what no deployable serves or runs ------------------------------------

/// The background work the app has, by the trigger that runs it: one
/// thing of each kind, named as a reader finds it, for the message.
fn background(db: &dyn Db, project: Project) -> (Option<String>, Option<String>) {
    let mut schedules = None;
    if let Some(schedule) = crate::schedules::schedules(db, project).first() {
        schedules = Some(match &schedule.run {
            crate::schedules::ScheduleRun::Action(_) => format!("the schedule `{}`", schedule.name),
            crate::schedules::ScheduleRun::Purge(entity) => format!("`{entity}`'s purge"),
        });
    }
    if schedules.is_none()
        && let Some(poll) = crate::polls::polls(db, project).polls.first()
    {
        schedules = Some(format!("`{}`'s poll `{}`", poll.connection, poll.name));
    }
    if schedules.is_none()
        && let Some(starter) = bodies(db, project)
            .iter()
            .find(|(_, body)| starts_batches(body))
            .map(|(key, _)| key.name().to_string())
    {
        schedules = Some(format!("the batch runs `{starter}` starts"));
    }
    let mut subscribers = crate::events::subscribers(db, project)
        .first()
        .map(|subscriber| format!("the subscriber `{}`", subscriber.name));
    if subscribers.is_none()
        && let Some(email) = crate::emails::analysis(db, project).emails.first()
    {
        subscribers = Some(format!("the email `{}`", email.type_name));
    }
    if subscribers.is_none()
        && let Some(queue) = crate::receiving::receiving(db, project).queues.first()
    {
        subscribers = Some(format!("the queue `{}`", queue.connection));
    }
    (schedules, subscribers)
}

/// Each API with routes no deployable serves, and the background work
/// none runs.
fn unserved(db: &dyn Db, project: Project, deployables: &Deployables) -> Vec<Diagnostic> {
    let Some(at) = deployables.at.clone() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut routed: BTreeMap<String, usize> = BTreeMap::new();
    for (group, _) in crate::routes::groups(db, project).routes() {
        *routed.entry(group.api.clone()).or_default() += 1;
    }
    let webhooks = crate::webhooks::webhooks(db, project).routes.len();
    if webhooks > 0 {
        *routed.entry(WEBHOOKS.to_string()).or_default() += webhooks;
    }
    for (api, routes) in routed {
        if deployables.list.iter().any(|d| d.serves(&api)) {
            continue;
        }
        out.push(
            Diagnostic::new(
                &rule::DEPLOYABLE_UNSERVED,
                format!("No deployable serves the API `{api}`"),
            )
            .at(at.clone(), "none serves it")
            .reason(format!(
                "Once `deployables:` is declared, an API's routes are served only by a deployable whose `serves:` names it, and `{api}` has {routes} {}.",
                if routes == 1 { "route" } else { "routes" }
            ))
            .next(format!("Add `{api}` to a deployable's `serves:`")),
        );
    }
    let (schedules, subscribers) = background(db, project);
    let runs_schedules: fn(Runs) -> bool = |runs| runs.schedules;
    let runs_subscribers: fn(Runs) -> bool = |runs| runs.subscribers;
    for (work, trigger, runs) in [
        (schedules, Trigger::Schedules, runs_schedules),
        (subscribers, Trigger::Subscribers, runs_subscribers),
    ] {
        let Some(work) = work else { continue };
        if deployables.list.iter().any(|d| runs(d.runs)) {
            continue;
        }
        out.push(
            Diagnostic::new(
                &rule::DEPLOYABLE_UNSERVED,
                format!("No deployable runs the app's {}", trigger.name()),
            )
            .at(at.clone(), "none runs them")
            .reason(format!(
                "Once `deployables:` is declared, background work runs only in a deployable whose `runs:` lists its trigger, and {work} needs `Trigger.{}`.",
                trigger.name()
            ))
            .next(format!(
                "Add `Trigger.{}` to a deployable's `runs:`",
                trigger.name()
            )),
        );
    }
    out
}

/// Each route that waits for follow-on work in a deployable that doesn't
/// run subscribers (area 7, "Waiting for follow-on work").
fn settle_problems(db: &dyn Db, project: Project, deployables: &Deployables) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for deployable in &deployables.list {
        if deployable.runs.subscribers {
            continue;
        }
        let Some(at) = &deployable.at else { continue };
        let waiting = crate::routes::groups(db, project)
            .routes()
            .find(|(group, route)| route.settle.is_some() && deployable.serves(&group.api));
        let Some((group, route)) = waiting else {
            continue;
        };
        out.push(
            Diagnostic::new(
                &rule::DEPLOYABLE_SETTLE_UNRUN,
                format!(
                    "`{}` waits for follow-on work `{}` doesn't run",
                    route.label(),
                    deployable.name
                ),
            )
            .at(at.clone(), "doesn't run subscribers")
            .reason(format!(
                "`{}` in `{}` has `settle:`, so it waits for the subscribers of what its action emits, and a deployable runs them only when its `runs:` lists `Trigger.subscribers`.",
                route.label(),
                group.api
            ))
            .next(format!(
                "Add `Trigger.subscribers` to `{}`'s `runs:`",
                deployable.name
            )),
        );
    }
    out
}

// --- what each deployable reaches ------------------------------------------

/// Every callable's typed body, by its key.
pub fn bodies(db: &dyn Db, project: Project) -> BTreeMap<Key, Arc<Body>> {
    let mut out = BTreeMap::new();
    for &file in project.files(db) {
        for callable in crate::types::callables_of(db, file) {
            let checked = crate::types::typed_body(db, project, callable);
            if let Some(key) = &checked.key {
                out.insert(key.clone(), Arc::clone(&checked.body));
            }
        }
    }
    out
}

/// Whether `body` starts a batch run, which runs where schedules run.
fn starts_batches(body: &Body) -> bool {
    body.calls().any(|(_, callee)| {
        matches!(
            callee,
            Callee::Batch(BatchOp::Each | BatchOp::Read(_) | BatchOp::Write(_))
        )
    })
}

/// How a deployable reaches a connection: the callable that calls
/// through it, or the part of the app that does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Via {
    /// A callable calls through it, by its key.
    Callable(Key),
    /// It receives the connection's webhooks.
    Webhooks,
    /// It calls the connection's polls.
    Polls,
    /// It receives the queue `receiving:` reads through it.
    Receiving,
    /// It sends the app's email through it.
    Email,
    /// The batch runs it carries on write or read files there.
    Batches,
}

impl Via {
    /// The end of a sentence saying how: "through `notes.save_sheet`".
    pub fn words(&self) -> String {
        match self {
            Via::Callable(key) => format!("through `{}`", key.written()),
            Via::Webhooks => "by receiving its webhooks".to_string(),
            Via::Polls => "by calling its polls".to_string(),
            Via::Receiving => "by receiving its queue".to_string(),
            Via::Email => "by sending the app's email".to_string(),
            Via::Batches => "in the batch runs it carries on".to_string(),
        }
    }
}

/// What one deployable reaches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reached {
    /// Every callable it reaches, by key: a query or an email template
    /// among them.
    pub callables: BTreeSet<Key>,
    /// Every connection it uses, by name, with the first way found.
    pub connections: BTreeMap<String, Via>,
}

/// What each of the app's deployables reaches, in their order.
#[salsa::tracked(returns(ref))]
pub fn reaches(db: &dyn Db, project: Project) -> Vec<Reached> {
    let bodies = reachable_bodies(db, project);
    deployables(db, project)
        .list
        .iter()
        .map(|deployable| reach(db, project, deployable, &bodies))
        .collect()
}

/// What `deployable` reaches: one of the app's, or one a test makes.
pub fn reached(db: &dyn Db, project: Project, deployable: &Deployable) -> Reached {
    if let Some(at) = deployables(db, project)
        .list
        .iter()
        .position(|declared| declared == deployable)
        && let Some(reached) = reaches(db, project).get(at)
    {
        return reached.clone();
    }
    reach(db, project, deployable, &reachable_bodies(db, project))
}

/// Every callable's typed body, and each email template's, which a
/// delivery renders (area 16), by key.
fn reachable_bodies(db: &dyn Db, project: Project) -> BTreeMap<Key, Arc<Body>> {
    let mut out = bodies(db, project);
    for path in crate::emails::template_paths(db, project) {
        let checked =
            crate::types::template_body(db, project, crate::lookups::Name::new(db, path.clone()));
        if let Some(key) = &checked.key {
            out.insert(key.clone(), Arc::clone(&checked.body));
        }
    }
    out
}

/// The callables the runtime runs by their key or number rather than
/// because a body calls them, which a deployable that serves `serves`
/// and runs `runs` reaches (roadmap chunk 8.1o3): the action
/// `on_first_sign_in:` names wherever an API is served, since redeeming a
/// magic link runs it; where subscribers run, the action `magic_link: {
/// send: }` names and every email template, which a delivery runs; and
/// every provider package's `decode:` and `encode:` functions, kept
/// whole, which its codecs name by key.
fn named_by_the_runtime(db: &dyn Db, project: Project, deployable: &Deployable) -> Vec<Key> {
    let lookup = crate::lookups::Lookup::new(db, project);
    let named = |name: &Option<String>| {
        name.as_ref()
            .and_then(|name| crate::graph::declaration_key(&lookup, name))
    };
    let mut out = Vec::new();
    if deployable.serves.iter().any(|api| api != WEBHOOKS) {
        out.extend(named(&crate::identity::first_sign_in_action(db, project).0));
    }
    if deployable.runs.subscribers {
        out.extend(named(&crate::identity::sign_in_send_action(db, project).0));
        for path in crate::emails::template_paths(db, project) {
            let checked =
                crate::types::template_body(db, project, crate::lookups::Name::new(db, path));
            out.extend(checked.key.clone());
        }
    }
    let operations = crate::providers::operations(db, project);
    for decoder in &operations.decoders {
        out.push(decoder.decode.clone());
        out.push(decoder.encode.clone());
    }
    out.extend(
        operations
            .list
            .iter()
            .filter_map(|operation| operation.decode.as_ref().map(|(_, key)| key.clone())),
    );
    out
}

fn reach(
    db: &dyn Db,
    project: Project,
    deployable: &Deployable,
    bodies: &BTreeMap<Key, Arc<Body>>,
) -> Reached {
    let mut out = Reached::default();
    let mut roots: Vec<Key> = Vec::new();
    let graph = crate::graph::graph(db, project);
    for id in graph.ids() {
        let node = graph.node(id);
        let Detail::Route { api, .. } = &node.detail else {
            continue;
        };
        if node.kind != Kind::Route || !deployable.serves(api) {
            continue;
        }
        roots.extend(
            graph
                .outgoing(id, EdgeKind::Runs)
                .map(|to| graph.node(to).key.clone()),
        );
    }
    if deployable.serves(WEBHOOKS) {
        for webhook in &crate::webhooks::webhooks(db, project).routes {
            connection(&mut out, &webhook.connection, Via::Webhooks);
            roots.extend(webhook.replies.iter().map(|(_, action)| action.clone()));
        }
    }
    if deployable.runs.schedules {
        roots.extend(
            crate::schedules::schedules(db, project)
                .iter()
                .filter_map(|schedule| schedule.run.action().cloned()),
        );
        for poll in &crate::polls::polls(db, project).polls {
            connection(&mut out, &poll.connection, Via::Polls);
        }
        // A batch run carries on where schedules run, calling what its
        // starter's inline functions call.
        roots.extend(
            bodies
                .iter()
                .filter(|(_, body)| starts_batches(body))
                .map(|(key, _)| key.clone()),
        );
    }
    let packages = &crate::connections::package_connections(db, project).connections;
    if deployable.runs.subscribers {
        for subscriber in crate::events::subscribers(db, project).iter() {
            roots.push(subscriber.run.clone());
            // A package's own subscriber hears each connection receiving
            // its package's webhooks, and calls through that one.
            if let Some(folder) = &subscriber.package {
                for package in packages.iter().filter(|p| &p.folder == folder) {
                    connection(&mut out, &package.name, Via::Webhooks);
                }
            }
        }
        for queue in &crate::receiving::receiving(db, project).queues {
            connection(&mut out, &queue.connection, Via::Receiving);
        }
        if !crate::emails::analysis(db, project).emails.is_empty()
            && let Some(email) = crate::emails::written_connection(db, project)
        {
            connection(&mut out, &email, Via::Email);
        }
    }
    roots.extend(named_by_the_runtime(db, project, deployable));
    let ai = crate::ai::settings(db, project).connection.clone();
    let files = crate::data::connections(db, project).files();
    let mut pending = roots;
    while let Some(key) = pending.pop() {
        if !out.callables.insert(key.clone()) {
            continue;
        }
        let Some(body) = bodies.get(&key) else {
            continue;
        };
        for (_, callee) in body.calls() {
            match callee {
                Callee::Function(next)
                | Callee::Action(next)
                | Callee::Query(next)
                | Callee::Fixture(next)
                | Callee::LibraryFunction(next) => pending.push(next.clone()),
                Callee::PackageAction {
                    connection: through,
                    action,
                } => {
                    connection(&mut out, through, Via::Callable(key.clone()));
                    pending.push(action.clone());
                }
                Callee::Provider {
                    connection: through,
                    ..
                }
                | Callee::Files {
                    connection: through,
                    ..
                } => connection(&mut out, through, Via::Callable(key.clone())),
                Callee::Generate { .. } | Callee::Stream { .. } | Callee::Embed { .. } => {
                    if let Some(ai) = &ai {
                        connection(&mut out, ai, Via::Callable(key.clone()));
                    }
                }
                // A file's rows are read and written by the batch run,
                // which carries on where schedules run.
                Callee::Batch(BatchOp::Read(_) | BatchOp::Write(_))
                    if deployable.runs.schedules =>
                {
                    for store in &files {
                        connection(&mut out, store, Via::Batches);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Records that `out` uses connection `name`, `via` the first way found;
/// a package action's own operations name none.
fn connection(out: &mut Reached, name: &str, via: Via) {
    if !name.is_empty() {
        out.connections.entry(name.to_string()).or_insert(via);
    }
}

// --- what a deployable's target can't run ----------------------------------

/// Each connection a deployable that declares a target reaches whose
/// driver that target can't run (area 11, question 24).
pub fn target_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let deployables = deployables(db, project);
    if deployables
        .list
        .iter()
        .all(|deployable| deployable.target.is_none())
    {
        return Vec::new();
    }
    let reaches = reaches(db, project);
    let kinds = crate::data::connections(db, project);
    let packages = &crate::connections::package_connections(db, project).connections;
    let smtp = &crate::emails::smtp_connections(db, project).list;
    let declared = connection_spans(db, project);
    let mut out = Vec::new();
    for (deployable, reached) in deployables.list.iter().zip(reaches) {
        let Some((target, at)) = &deployable.target else {
            continue;
        };
        for (name, via) in &reached.connections {
            let refusal = match (target, kinds.kind(name)) {
                (DeployTarget::Wasm, Some(ConnectionKind::Files)) => Some(Refusal {
                    message: format!(
                        "`{}` can't use `{name}` on Workers, which has no disk",
                        deployable.name
                    ),
                    label: "`Driver.disk`",
                    reason: format!(
                        "`{name}` keeps files on the server's own disk (`Driver.disk`), and a Worker has none; `{}` uses it {}.",
                        deployable.name,
                        via.words()
                    ),
                    next: format!(
                        "Keep `{name}`'s files through a package over S3's API, such as Cloudflare R2, or use them from a `Target.native` deployable"
                    ),
                }),
                (DeployTarget::Wasm, Some(ConnectionKind::Package)) => {
                    let Some(package) = packages.iter().find(|p| &p.name == name) else {
                        continue;
                    };
                    let (Some(image), None) = (&package.reach.runs, &package.host) else {
                        continue;
                    };
                    Some(Refusal {
                        message: format!(
                            "`{}` can't call `{name}` on Workers, which runs no containers",
                            deployable.name
                        ),
                        label: "a service",
                        reason: format!(
                            "`{}` runs `{image}` as a container beside the app (`runs:`); `{}` calls it {}.",
                            package.package,
                            deployable.name,
                            via.words()
                        ),
                        next: format!(
                            "Give `{name}` `host:`, a `Url` setting holding where the service is hosted, or call it from a `Target.native` deployable"
                        ),
                    })
                }
                (DeployTarget::Wasm, Some(ConnectionKind::Smtp)) => {
                    let blocked = smtp
                        .iter()
                        .any(|server| &server.name == name && server.port == 25);
                    blocked.then(|| Refusal {
                        message: format!(
                            "`{}` can't send email through `{name}` on port 25, which Workers blocks",
                            deployable.name
                        ),
                        label: "port 25",
                        reason: "A Worker can open TCP connections, but not to port 25; mail servers take mail from apps on port 587, submission.".to_string(),
                        next: format!(
                            "Give `{name}` `port: 587`, or send email from a `Target.native` deployable"
                        ),
                    })
                }
                (DeployTarget::Native, Some(ConnectionKind::CloudflareQueue)) => Some(Refusal {
                    message: format!(
                        "`{}` can't receive `{name}` natively: Cloudflare pushes it only to a Worker",
                        deployable.name
                    ),
                    label: "`Driver.cloudflare_queues`",
                    reason: format!(
                        "Cloudflare pushes a queue's batches to a Worker's consumer, and a native server has none; a deployable that runs `Trigger.subscribers` receives the app's queues, and `{}` reaches `{name}` {}.",
                        deployable.name,
                        via.words()
                    ),
                    next: format!(
                        "Receive `{name}` in a `Target.wasm` deployable that runs `Trigger.subscribers`, or read it through a package over Cloudflare's HTTP pull API"
                    ),
                }),
                _ => None,
            };
            let Some(refusal) = refusal else { continue };
            let mut problem =
                Diagnostic::new(&rule::DEPLOYABLE_TARGET_UNSUPPORTED, refusal.message).at(
                    at.clone(),
                    match target {
                        DeployTarget::Native => "built natively",
                        DeployTarget::Wasm => "built for Workers",
                    },
                );
            if let Some(span) = declared.get(name) {
                problem = problem.also(span.clone(), refusal.label);
            }
            out.push(problem.reason(refusal.reason).next(refusal.next));
        }
        if *target == DeployTarget::Wasm && deployable.runs.schedules {
            out.extend(too_often(db, project, deployable, at));
        }
    }
    out
}

/// How often Cloudflare's cron trigger runs a Worker at most: once a
/// minute (area 12, "The tick").
pub const CRON_EVERY_MS: u64 = 60_000;

/// Each schedule more often than once a minute that `deployable`, built
/// for Workers and running the schedules, would run: its tick is the
/// minute's cron, which can't run it (area 12, decided 2026-10-19), and
/// Jadpo has no warnings. Its error is at the schedule's `every:`, naming
/// the schedule (chunk 8.2f).
fn too_often(db: &dyn Db, project: Project, deployable: &Deployable, at: &Span) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &file in project.files(db).iter() {
        for read in crate::schedules::schedules_in(db, project, file) {
            let Some(model) = &read.model else { continue };
            if model.every_ms >= CRON_EVERY_MS {
                continue;
            }
            let how_often = match model.every_ms / 1000 {
                1 => "every second".to_string(),
                seconds => format!("every {seconds} seconds"),
            };
            let (name, schedule) = (&deployable.name, &model.name);
            out.push(
                Diagnostic::new(
                    &rule::DEPLOYABLE_TARGET_UNSUPPORTED,
                    format!(
                        "`{name}` can't run `{schedule}` {how_often} on Workers, whose cron runs once a minute"
                    ),
                )
                .at(
                    read.every.clone().unwrap_or_else(|| read.span.clone()),
                    "more often than once a minute",
                )
                .also(at.clone(), "built for Workers")
                .reason(format!(
                    "A Worker ticks when Cloudflare's cron trigger runs it, once a minute at most, so a schedule due {how_often} would run late; `{name}` runs `{schedule}` because its `runs:` lists `Trigger.schedules`."
                ))
                .next(format!(
                    "Give `{schedule}` `every: 1m` or longer, or run `Trigger.schedules` in a `Target.native` deployable"
                )),
            );
        }
    }
    out
}

/// What [`target_problems`] says of one connection.
struct Refusal {
    message: String,
    label: &'static str,
    reason: String,
    next: String,
}

/// Where each connection's name is written in `connections:`.
fn connection_spans(db: &dyn Db, project: Project) -> BTreeMap<String, Span> {
    let mut out = BTreeMap::new();
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if tree::name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, _) in tree::members(member.block()) {
                if let Some(text) = tree::name_text(&name) {
                    out.entry(text)
                        .or_insert_with(|| node_span(&found.path, name.syntax()));
                }
            }
        }
    }
    out
}

/// A connection's `max_concurrent:` cap that a deployable built for
/// Workers counts per instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerInstance {
    pub connection: String,
    pub cap: u32,
    pub deployable: String,
}

/// Each connection with a `max_concurrent:` cap that a deployable built
/// for Workers reaches, with the cap and that deployable: on Workers it
/// counts per instance, since Workers runs as many as traffic needs
/// (Richard, 2026-10-21; area 11, "Decisions").
pub fn per_instance(db: &dyn Db, project: Project) -> Vec<PerInstance> {
    let deployables = deployables(db, project);
    let workers =
        |deployable: &Deployable| matches!(deployable.target, Some((DeployTarget::Wasm, _)));
    if !deployables.list.iter().any(workers) {
        return Vec::new();
    }
    let packages = &crate::connections::package_connections(db, project).connections;
    let mut out = Vec::new();
    for (deployable, reached) in deployables.list.iter().zip(reaches(db, project)) {
        if !workers(deployable) {
            continue;
        }
        for package in packages {
            if !reached.connections.contains_key(&package.name) {
                continue;
            }
            if let Some(cap) = package.max_concurrent.or(package.reach.max_concurrent) {
                out.push(PerInstance {
                    connection: package.name.clone(),
                    cap,
                    deployable: deployable.name.clone(),
                });
            }
        }
    }
    out
}
