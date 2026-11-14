//! Area 16's telemetry (chunk O1): the application's `telemetry:`
//! section, which says where traces, metrics and logs are sent and how
//! much of each request, and each domain's `metric` declarations in its
//! `monitoring.jadpo`, which its code records through
//! `telemetry.<metric>.add`, `record` and `set`.
//!
//! - [`section`] reads `telemetry:`: `endpoint:` a `Url` setting,
//!   `credential: { header: value: }` a header's name and a secret
//!   setting, `send:` and `detail:` the page's values. Left out,
//!   `endpoint:` and `credential:` are read by the runtime from the
//!   standard `OTEL_EXPORTER_OTLP_ENDPOINT` and
//!   `OTEL_EXPORTER_OTLP_HEADERS`, which the app's code can't read.
//!   Anything else is `telemetry.settings_invalid`.
//! - [`metrics`] reads every `metric`: a `Counter`, `Measure` or `Gauge`
//!   with `by:`, a list of label names, declared in a domain's
//!   `monitoring.jadpo`, or a single-file project's file. Anything else is
//!   `telemetry.metric_invalid`.
//!
//! What a call records is checked where it is written
//! (`types::body::calls`), against [`metrics`].

use std::collections::BTreeMap;
use std::sync::Arc;

use jadpo_db::{Db, ItemKind, Layout, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::telemetry::{
    Credential, Detail, Full, Level, MetricKind, MetricPlan, Send, TRACE_ENUM, TelemetryPlan,
};
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::config::{config, singletons};
use crate::lookups::Lookup;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, Prim, TypeView};

/// The section's name in `application { }`.
pub const SECTION: &str = "telemetry";

/// What `telemetry:` holds.
const KEYS: [&str; 4] = ["endpoint", "credential", "send", "detail"];
const CREDENTIAL_KEYS: [&str; 2] = ["header", "value"];
const SEND_KEYS: [&str; 3] = ["traces", "metrics", "logs"];
const DETAIL_KEYS: [&str; 2] = ["full_traces", "samples"];
const SAMPLE_KEYS: [&str; 2] = ["per_path", "every"];

/// What a `metric`'s block holds.
const METRIC_KEYS: [&str; 1] = ["by"];

/// The file a domain's metrics are declared in.
pub const FILE: &str = crate::thresholds::FILE;

const SECTION_REASON: &str = "`telemetry:` sends the app's traces, metrics and logs over OpenTelemetry's protocol: `endpoint:` is a `Url` setting, `credential:` a header's name and a secret setting, `send:` which signals go out and `detail:` how much of each request. Leave `endpoint:` and `credential:` out to read `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS`.";

const METRIC_REASON: &str = "A metric is declared in a domain's `monitoring.jadpo` as `metric name = Counter { by: [label] }`: a `Counter`, a count that only rises, a `Measure`, a distribution, or a `Gauge`, a value at a moment. `by:` names its labels, which code gives as enums or `Bool`, so the number of series stays bounded.";

/// `telemetry:` as written and checked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    /// Whether the section is written at all.
    pub written: bool,
    /// The setting `endpoint:` reads; `None` when it is left out.
    pub endpoint: Option<String>,
    /// `credential:`'s header and the setting its value reads.
    pub credential: Option<(String, String)>,
    pub send: Send,
    pub detail: Detail,
    /// Where it is written.
    pub at: Option<Span>,
    pub problems: Vec<Diagnostic>,
}

impl Section {
    /// The runtime's plan of it, its settings by their numbers in `config`:
    /// `None` when it isn't written.
    pub fn plan(&self, number: &dyn Fn(&str) -> Option<u32>) -> Option<TelemetryPlan> {
        if !self.written {
            return None;
        }
        Some(TelemetryPlan {
            endpoint: self.endpoint.as_deref().and_then(number),
            credential: self.credential.as_ref().and_then(|(header, setting)| {
                Some(Credential {
                    header: header.clone(),
                    value: number(setting)?,
                })
            }),
            send: self.send,
            detail: self.detail.clone(),
        })
    }
}

/// The application's `telemetry:`, read.
#[salsa::tracked(returns(ref))]
pub fn section(db: &dyn Db, project: Project) -> Arc<Section> {
    let mut out = Section::default();
    let lookup = Lookup::new(db, project);
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some(SECTION) {
                continue;
            }
            out.written = true;
            out.at = Some(node_span(&found.path, key.syntax()));
            let mut reader = Reader {
                db,
                project,
                lookup: &lookup,
                path: &found.path,
                out: &mut out,
            };
            reader.section(&member);
        }
    }
    Arc::new(out)
}

struct Reader<'a, 'l, 'o> {
    db: &'a dyn Db,
    project: Project,
    lookup: &'l Lookup<'a>,
    path: &'o str,
    out: &'o mut Section,
}

impl Reader<'_, '_, '_> {
    fn span(&self, node: &jadpo_syntax::SyntaxNode) -> Span {
        node_span(self.path, node)
    }

    fn value_span(&self, member: &ast::Member) -> Span {
        member.value().map_or_else(
            || self.span(member.syntax()),
            |value| self.span(value.syntax()),
        )
    }

    fn problem(&mut self, title: String, at: Span, label: &str, next: String) {
        self.out.problems.push(
            Diagnostic::new(&rule::TELEMETRY_SETTINGS_INVALID, title)
                .at(at, label.to_string())
                .reason(SECTION_REASON)
                .next(next),
        );
    }

    /// A key a block doesn't take, with the one it is nearest.
    fn unknown(&mut self, key: &ast::Name, block: &str, keys: &[&str]) {
        let written = name_text(key).unwrap_or_default();
        let at = self.span(key.syntax());
        let near = nearest(&written, keys.iter().copied());
        let list = crate::and_list(&keys.iter().map(|k| format!("`{k}:`")).collect::<Vec<_>>());
        let problem = Diagnostic::new(
            &rule::TELEMETRY_SETTINGS_INVALID,
            format!("`{block}` takes no `{written}:`"),
        )
        .at(at.clone(), "not a telemetry setting")
        .reason(format!("`{block}` holds {list}. {SECTION_REASON}"));
        self.out.problems.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}:` instead of `{written}:`"))
                .edit(at, near),
            None => problem.next(format!("Remove `{written}:`")),
        });
    }

    fn section(&mut self, member: &ast::Member) {
        if member.block().is_none() {
            let at = self.value_span(member);
            self.problem(
                "`telemetry:` must be a block".into(),
                at,
                "not a block",
                "Write `telemetry: { send: { traces: true } }`, or leave the section out".into(),
            );
            return;
        }
        for (key, option) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("endpoint") => self.endpoint(&option),
                Some("credential") => self.credential(&option),
                Some("send") => self.send(&option),
                Some("detail") => self.detail(&option),
                _ => self.unknown(&key, "telemetry:", &KEYS),
            }
        }
    }

    /// `config.<name>`'s setting, when `value` reads one that is declared:
    /// one that isn't is reported where it is read.
    fn setting(&self, value: &Expr) -> Option<Result<crate::config::Setting, ()>> {
        let (first, rest) = tree::dotted(value)?;
        if tree::name_ref_text(&first).as_deref() != Some(crate::config::CONFIG) || rest.len() != 1
        {
            return None;
        }
        let name = tree::name_ref_text(&rest[0])?;
        Some(
            config(self.db, self.project)
                .setting(&name)
                .cloned()
                .ok_or(()),
        )
    }

    fn endpoint(&mut self, option: &ast::Member) {
        let at = self.value_span(option);
        let read = option.value().and_then(|value| self.setting(&value));
        match read {
            Some(Ok(setting)) if self.lookup.base_prim(&setting.ty) == Some(Prim::Url) => {
                self.out.endpoint = Some(setting.name);
            }
            Some(Ok(setting)) => self.problem(
                "`telemetry:`'s `endpoint:` must be an address".into(),
                at,
                &format!("`{}` is `{}`", setting.name, setting.ty),
                format!("Declare `{}: Url` in `config.jadpo`", setting.name),
            ),
            Some(Err(())) => {}
            None => self.problem(
                "`telemetry:`'s `endpoint:` must be a setting".into(),
                at,
                "not a setting",
                "Write `endpoint: config.telemetry_endpoint`, declaring `telemetry_endpoint: Url` in `config.jadpo`, or leave `endpoint:` out to read `OTEL_EXPORTER_OTLP_ENDPOINT`".into(),
            ),
        }
    }

    fn credential(&mut self, option: &ast::Member) {
        let whole = self.span(option.syntax());
        if option.block().is_none() {
            let at = self.value_span(option);
            self.problem(
                "`telemetry:`'s `credential:` must be a block".into(),
                at,
                "not a block",
                "Write `credential: { header: \"x-api-key\" value: config.telemetry_key }`".into(),
            );
            return;
        }
        let mut header = None;
        let mut value = None;
        for (key, member) in tree::members(option.block()) {
            match name_text(&key).as_deref() {
                Some("header") => match member.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Str(text)) if header_name(&text) => header = Some(text),
                    _ => {
                        let at = self.value_span(&member);
                        self.problem(
                            "`credential:`'s `header:` must be a header's name".into(),
                            at,
                            "not a header's name",
                            "Write the header the service reads its key from, such as `header: \"x-honeycomb-team\"`".into(),
                        );
                        return;
                    }
                },
                Some("value") => {
                    let at = self.value_span(&member);
                    match member.value().and_then(|v| self.setting(&v)) {
                        Some(Ok(setting)) if setting.secret => value = Some(setting.name),
                        Some(Ok(setting)) => {
                            self.problem(
                                "`credential:`'s `value:` must be a secret setting".into(),
                                at,
                                &format!("`{}` isn't secret", setting.name),
                                format!("Mark `{}` `secret: true` in `config.jadpo`", setting.name),
                            );
                            return;
                        }
                        Some(Err(())) => return,
                        None => {
                            self.problem(
                                "`credential:`'s `value:` must be a secret setting".into(),
                                at,
                                "not a setting",
                                "Write `value: config.telemetry_key`, declaring `telemetry_key: SecretText { secret: true }` in `config.jadpo`".into(),
                            );
                            return;
                        }
                    }
                }
                _ => self.unknown(&key, "credential:", &CREDENTIAL_KEYS),
            }
        }
        match (header, value) {
            (Some(header), Some(value)) => self.out.credential = Some((header, value)),
            (None, _) => self.problem(
                "`credential:` doesn't say which header it is sent in".into(),
                whole,
                "no `header:`",
                "Add `header:`, the header the service reads its key from".into(),
            ),
            (_, None) => self.problem(
                "`credential:` doesn't say where its value is read".into(),
                whole,
                "no `value:`",
                "Add `value: config.telemetry_key`, a secret setting".into(),
            ),
        }
    }

    fn send(&mut self, option: &ast::Member) {
        if option.block().is_none() {
            let at = self.value_span(option);
            self.problem(
                "`telemetry:`'s `send:` must be a block".into(),
                at,
                "not a block",
                "Write `send: { traces: true metrics: true logs: LogLevel.warn }`".into(),
            );
            return;
        }
        for (key, member) in tree::members(option.block()) {
            let word = name_text(&key).unwrap_or_default();
            match word.as_str() {
                "traces" | "metrics" => match member.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Bool(on)) => {
                        if word == "traces" {
                            self.out.send.traces = on;
                        } else {
                            self.out.send.metrics = on;
                        }
                    }
                    _ => {
                        let at = self.value_span(&member);
                        self.problem(
                            format!("`send:`'s `{word}:` must be `true` or `false`"),
                            at,
                            "not a `Bool`",
                            format!("Write `{word}: true` or `{word}: false`"),
                        );
                    }
                },
                "logs" => match member.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Path(path))
                        if path.len() == 2
                            && path[0] == "LogLevel"
                            && Level::from_word(&path[1]).is_some() =>
                    {
                        self.out.send.logs = Level::from_word(&path[1]).unwrap_or(Level::Warn);
                    }
                    _ => {
                        let at = self.value_span(&member);
                        self.problem(
                            "`send:`'s `logs:` must be a `LogLevel`".into(),
                            at,
                            "not a `LogLevel`",
                            "Write the lowest level sent, such as `logs: LogLevel.warn`".into(),
                        );
                    }
                },
                _ => self.unknown(&key, "send:", &SEND_KEYS),
            }
        }
    }

    fn detail(&mut self, option: &ast::Member) {
        if option.block().is_none() {
            let at = self.value_span(option);
            self.problem(
                "`telemetry:`'s `detail:` must be a block".into(),
                at,
                "not a block",
                "Write `detail: { full_traces: [Trace.failures] }`".into(),
            );
            return;
        }
        for (key, member) in tree::members(option.block()) {
            match name_text(&key).as_deref() {
                Some("full_traces") => self.full_traces(&member),
                Some("samples") => self.samples(&member),
                _ => self.unknown(&key, "detail:", &DETAIL_KEYS),
            }
        }
    }

    fn full_traces(&mut self, member: &ast::Member) {
        let at = self.value_span(member);
        let listed = match member.value().map(|v| Const::from_expr(&v)) {
            Some(Const::List(items)) => items
                .into_iter()
                .map(|item| match item {
                    Const::Path(path) if path.len() == 2 && path[0] == TRACE_ENUM => {
                        Full::from_word(&path[1])
                    }
                    _ => None,
                })
                .collect::<Option<Vec<Full>>>(),
            _ => None,
        };
        match listed {
            Some(mut listed) => {
                listed.sort();
                listed.dedup();
                self.out.detail.full_traces = listed;
            }
            None => {
                let variants = crate::and_list(
                    &Full::ALL
                        .iter()
                        .map(|full| format!("`Trace.{}`", full.word()))
                        .collect::<Vec<_>>(),
                );
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_SETTINGS_INVALID,
                        "`detail:`'s `full_traces:` must list `Trace` variants",
                    )
                    .at(at, "not a list of `Trace` variants")
                    .reason(format!(
                        "`full_traces:` lists when a request's whole trace is sent: {variants}. {SECTION_REASON}"
                    ))
                    .next("Write `full_traces: [Trace.failures, Trace.faults]`"),
                );
            }
        }
    }

    fn samples(&mut self, member: &ast::Member) {
        if member.block().is_none() {
            let at = self.value_span(member);
            self.problem(
                "`detail:`'s `samples:` must be a block".into(),
                at,
                "not a block",
                "Write `samples: { per_path: 1 every: 1m }`".into(),
            );
            return;
        }
        for (key, option) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("per_path") => {
                    let read = match option.value().map(|v| Const::from_expr(&v)) {
                        Some(Const::Number { decimal: false, .. }) => option
                            .value()
                            .and_then(|v| tree::written(v.syntax()).parse::<u32>().ok()),
                        _ => None,
                    };
                    match read {
                        Some(count) => self.out.detail.per_path = count,
                        None => {
                            let at = self.value_span(&option);
                            self.problem(
                                "`samples:`'s `per_path:` must be a whole number".into(),
                                at,
                                "not a whole number",
                                "Write how many ordinary traces each path sends, such as `per_path: 1`".into(),
                            );
                        }
                    }
                }
                Some("every") => {
                    let read = option.value().and_then(|value| {
                        let Const::Duration(_) = Const::from_expr(&value) else {
                            return None;
                        };
                        let ms = jadpo_std::Dur::parse(&tree::written(value.syntax()))?.millis;
                        u64::try_from(ms).ok().filter(|ms| *ms > 0)
                    });
                    match read {
                        Some(ms) => self.out.detail.every_ms = ms,
                        None => {
                            let at = self.value_span(&option);
                            self.problem(
                                "`samples:`'s `every:` must be a duration above zero".into(),
                                at,
                                "not a duration",
                                "Write how long each window of samples is, such as `every: 1m`"
                                    .into(),
                            );
                        }
                    }
                }
                _ => self.unknown(&key, "samples:", &SAMPLE_KEYS),
            }
        }
    }
}

/// Whether `text` is an HTTP header's name: a token of letters, digits
/// and `-`.
fn header_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// One `metric` declaration that checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metric {
    /// Its domain: `None` in a single-file project.
    pub domain: Option<String>,
    pub name: String,
    pub kind: MetricKind,
    /// Its `by:`, in the order written.
    pub labels: Vec<String>,
    /// Its `///` line.
    pub doc: Option<String>,
    /// Where its name is.
    pub at: Span,
}

impl Metric {
    /// The name it is sent under: its domain's and its own.
    pub fn sent_as(&self) -> String {
        match &self.domain {
            Some(domain) => format!("{domain}.{}", self.name),
            None => self.name.clone(),
        }
    }
}

/// Every metric of a project, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metrics {
    pub metrics: Vec<Metric>,
    /// Every name declared, read cleanly or not, by domain, so a call to
    /// one that failed to read is not also reported as unknown.
    pub written: Vec<(Option<String>, String)>,
    pub problems: Vec<Diagnostic>,
}

impl Metrics {
    /// The metric `name` of `domain`.
    pub fn find(&self, domain: Option<&str>, name: &str) -> Option<&Metric> {
        self.metrics
            .iter()
            .find(|metric| metric.domain.as_deref() == domain && metric.name == name)
    }

    /// Whether `name` is declared in `domain`, read cleanly or not.
    pub fn declared(&self, domain: Option<&str>, name: &str) -> bool {
        self.written
            .iter()
            .any(|(of, written)| of.as_deref() == domain && written == name)
    }

    /// The metrics `domain` declares, by name.
    pub fn names_in(&self, domain: Option<&str>) -> Vec<&str> {
        self.metrics
            .iter()
            .filter(|metric| metric.domain.as_deref() == domain)
            .map(|metric| metric.name.as_str())
            .collect()
    }

    /// The runtime's plan of each, by the name it is sent under.
    pub fn plans(&self) -> Vec<MetricPlan> {
        let mut out: Vec<MetricPlan> = self
            .metrics
            .iter()
            .map(|metric| MetricPlan {
                name: metric.sent_as(),
                kind: metric.kind,
                labels: metric.labels.clone(),
                description: metric.doc.clone(),
                unit: None,
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

/// Every `metric` of `project`, checked.
#[salsa::tracked(returns(ref))]
pub fn metrics(db: &dyn Db, project: Project) -> Arc<Metrics> {
    let mut out = Metrics::default();
    let loose = project.layout(db) == Layout::Loose;
    let mut first: BTreeMap<(Option<String>, String), Span> = BTreeMap::new();
    for &file in project.files(db) {
        let items = item_tree(db, file);
        let indexes: Vec<u32> = items
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Metric)
            .map(|item| item.index)
            .collect();
        if indexes.is_empty() {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        let domain = crate::thresholds::domain_of(&path).map(str::to_string);
        for index in indexes {
            let Some(ast::Decl::MetricDecl(decl)) = tree::decl_at(&source, index) else {
                continue;
            };
            let Some(name_node) = decl.name() else {
                continue;
            };
            let Some(name) = name_text(&name_node) else {
                continue;
            };
            let at = node_span(&path, name_node.syntax());
            out.written.push((domain.clone(), name.clone()));
            if !loose && domain.is_none() {
                out.problems.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_METRIC_INVALID,
                        format!("`{name}` isn't in a domain's `{FILE}`"),
                    )
                    .at(at, "not in a monitoring file")
                    .reason(METRIC_REASON)
                    .next(format!("Move `{name}` to `domains/<domain>/{FILE}`")),
                );
                continue;
            }
            if let Some(earlier) = first.get(&(domain.clone(), name.clone())) {
                out.problems.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_METRIC_INVALID,
                        format!("`{name}` is declared twice"),
                    )
                    .at(at, "declared again")
                    .also(earlier.clone(), "first declared here")
                    .reason("A domain's code records a metric by its name, so each name is declared once in a domain.")
                    .next(format!("Rename one of them, or remove this `{name}`")),
                );
                continue;
            }
            first.insert((domain.clone(), name.clone()), at.clone());
            if let Some(metric) =
                read_metric(&path, &decl, domain.clone(), name, at, &mut out.problems)
            {
                out.metrics.push(metric);
            }
        }
    }
    Arc::new(out)
}

/// One `metric` declaration read.
fn read_metric(
    path: &str,
    decl: &ast::MetricDecl,
    domain: Option<String>,
    name: String,
    at: Span,
    problems: &mut Vec<Diagnostic>,
) -> Option<Metric> {
    let base = decl.base()?;
    let written = tree::written(base.syntax());
    let Some(kind) = MetricKind::from_word(&written) else {
        let near = nearest(&written, MetricKind::ALL.iter().map(|kind| kind.word()));
        let span = node_span(path, base.syntax());
        let problem = Diagnostic::new(
            &rule::TELEMETRY_METRIC_INVALID,
            format!("`{written}` isn't a kind of metric"),
        )
        .at(span.clone(), "not `Counter`, `Measure` or `Gauge`")
        .reason(METRIC_REASON);
        problems.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{written}`"))
                .edit(span, near),
            None => problem.next("Declare it a `Counter`, a `Measure` or a `Gauge`"),
        });
        return None;
    };
    let mut labels = Vec::new();
    let mut fine = true;
    for (key, member) in tree::members(decl.member_list()) {
        let word = name_text(&key).unwrap_or_default();
        if word != "by" {
            let span = node_span(path, key.syntax());
            let problem = Diagnostic::new(
                &rule::TELEMETRY_METRIC_INVALID,
                format!("A metric takes no `{word}:`"),
            )
            .at(span.clone(), "not a metric's setting")
            .reason(METRIC_REASON);
            problems.push(match nearest(&word, METRIC_KEYS.iter().copied()) {
                Some(near) => problem
                    .next(format!("Use `{near}:` instead of `{word}:`"))
                    .edit(span, near),
                None => problem.next(format!("Remove `{word}:`")),
            });
            fine = false;
            continue;
        }
        let span = member.value().map_or_else(
            || node_span(path, member.syntax()),
            |value| node_span(path, value.syntax()),
        );
        let read = match member.value().map(|v| Const::from_expr(&v)) {
            Some(Const::List(items)) => items
                .into_iter()
                .map(|item| match item {
                    Const::Path(path) if path.len() == 1 => Some(path[0].clone()),
                    _ => None,
                })
                .collect::<Option<Vec<String>>>(),
            _ => None,
        };
        match read {
            Some(read) => {
                let mut seen = std::collections::BTreeSet::new();
                for label in &read {
                    if !seen.insert(label.clone()) {
                        problems.push(
                            Diagnostic::new(
                                &rule::TELEMETRY_METRIC_INVALID,
                                format!("`{name}` lists `{label}` twice"),
                            )
                            .at(span.clone(), "a label repeated")
                            .reason(METRIC_REASON)
                            .next(format!("List `{label}` once")),
                        );
                        fine = false;
                    }
                }
                labels = read;
            }
            None => {
                problems.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_METRIC_INVALID,
                        format!("`{name}`'s `by:` must list its labels' names"),
                    )
                    .at(span, "not a list of names")
                    .reason(METRIC_REASON)
                    .next("Write the labels code gives it, such as `by: [plan]`"),
                );
                fine = false;
            }
        }
    }
    fine.then(|| Metric {
        domain,
        name,
        kind,
        labels,
        doc: crate::graph::text::doc(decl.syntax()),
        at,
    })
}

/// The problems with `telemetry:` and the metrics.
pub fn problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = section(db, project).problems.clone();
    out.extend(metrics(db, project).problems.iter().cloned());
    out
}
