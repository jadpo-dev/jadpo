//! `ai: { limits: }` (area 21, "Budgets"; chunk 5.5v5): what the
//! application's `ai` calls may use, in tokens, and the database their
//! counts are kept in.
//!
//! - **`per_day: { tokens: cost: }`** is required, so a loop or an
//!   injection can't spend without end (`semantic.ai_budget_missing`):
//!   `tokens:` a whole number, `cost:` a `Money` setting the day's
//!   estimated cost may not pass.
//! - **`per_request:`** and **`per_caller_per_day:`**, whole numbers of
//!   tokens, may be left out.
//! - **`store:`** names the database the counts are kept in; left out, it
//!   is `main` when that is a database, or the application's only one
//!   (`ai.settings_invalid` when neither says which).
//!
//! Every other mistake in it is `ai.settings_invalid`.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode};

use crate::config::config;
use crate::data::ConnectionKind;
use crate::lookups::Lookup;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, Prim, TypeView};

/// The keys `limits:` takes.
const KEYS: [&str; 4] = ["per_request", "per_day", "per_caller_per_day", "store"];

/// The keys `per_day:` takes.
const DAY_KEYS: [&str; 2] = ["tokens", "cost"];

/// The most tokens a limit may be: a trillion.
const MOST_TOKENS: u64 = 1_000_000_000_000;

const REASON: &str = "`limits:` caps what the application's `ai` calls use, in tokens: `per_day: { tokens: cost: }`, required, the day's tokens and the estimated cost a `Money` setting gives; `per_request:` and `per_caller_per_day:`; and `store:`, the database the counts are kept in.";

/// `ai: { limits: }`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LimitsRead {
    /// The database the counts are kept in, once it is known.
    pub store: Option<String>,
    pub per_request: Option<u64>,
    /// `per_day: { tokens: }`; `None` until it reads.
    pub per_day: Option<u64>,
    /// `per_day: { cost: }`, the `Money` setting it reads.
    pub cost: Option<String>,
    pub per_caller_per_day: Option<u64>,
}

/// Reads `limits:` from `member`, the entry under `ai:` in `path`, or
/// says it is missing at `head`, `ai:`'s key, when `member` is `None`.
pub(crate) fn read(
    db: &dyn Db,
    project: Project,
    path: &str,
    head: Span,
    member: Option<(Span, &ast::Member)>,
    out: &mut Vec<Diagnostic>,
) -> LimitsRead {
    let mut read = LimitsRead::default();
    let Some((key_span, member)) = member else {
        out.push(missing(
            head,
            "`ai:` sets no daily limit",
            "no `limits:`",
            "Add `limits: { per_day: { tokens: 2000000 } }` to `ai:`",
        ));
        return read;
    };
    let Some(block) = member.block() else {
        out.push(
            invalid(
                "`limits:` holds no block",
                node_span(path, member.syntax()),
                "not a block",
            )
            .next("Write `limits: { per_day: { tokens: 2000000 } }`"),
        );
        return read;
    };
    let mut given: Vec<String> = Vec::new();
    let mut store: Option<(String, Span)> = None;
    for (name, entry) in tree::members(Some(block)) {
        let word = name_text(&name).unwrap_or_default();
        let span = node_span(path, name.syntax());
        if !KEYS.contains(&word.as_str()) {
            let mut problem = invalid(
                format!("`{word}` isn't one of `limits:`'s settings"),
                span.clone(),
                "not a limit",
            );
            problem = match nearest(&word, KEYS.iter().copied()) {
                Some(near) => problem
                    .next(format!("Use `{near}:` instead of `{word}:`"))
                    .edit(span, near),
                None => problem.next(format!("Remove `{word}:`")),
            };
            out.push(problem);
            continue;
        }
        if given.contains(&word) {
            out.push(
                invalid(
                    format!("`limits:` gives `{word}:` twice"),
                    span,
                    "given again",
                )
                .next(format!("Remove the second `{word}:`")),
            );
            continue;
        }
        given.push(word.clone());
        let value_span = entry.value().map_or_else(
            || node_span(path, entry.syntax()),
            |value| node_span(path, value.syntax()),
        );
        match word.as_str() {
            "per_request" => read.per_request = tokens(&entry, value_span, &word, out),
            "per_caller_per_day" => {
                read.per_caller_per_day = tokens(&entry, value_span, &word, out)
            }
            "store" => {
                let written = entry
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                store = Some((written, value_span));
            }
            _ => day(db, project, path, &entry, span, value_span, &mut read, out),
        }
    }
    if !given.iter().any(|word| word == "per_day") {
        out.push(missing(
            key_span.clone(),
            "`limits:` sets no daily limit",
            "no `per_day:`",
            "Add `per_day: { tokens: 2000000 }` to `limits:`",
        ));
    }
    read.store = counted_in(db, project, store, key_span, out);
    read
}

/// `per_day: { tokens: cost: }`.
#[allow(clippy::too_many_arguments)]
fn day(
    db: &dyn Db,
    project: Project,
    path: &str,
    entry: &ast::Member,
    key_span: Span,
    value_span: Span,
    read: &mut LimitsRead,
    out: &mut Vec<Diagnostic>,
) {
    let Some(block) = entry.block() else {
        out.push(
            invalid("`per_day:` holds no block", value_span, "not a block")
                .next("Write `per_day: { tokens: 2000000 }`"),
        );
        return;
    };
    let mut given: Vec<String> = Vec::new();
    for (name, inner) in tree::members(Some(block)) {
        let word = name_text(&name).unwrap_or_default();
        let span = node_span(path, name.syntax());
        if !DAY_KEYS.contains(&word.as_str()) || given.contains(&word) {
            let problem = if given.contains(&word) {
                invalid(
                    format!("`per_day:` gives `{word}:` twice"),
                    span,
                    "given again",
                )
                .next(format!("Remove the second `{word}:`"))
            } else {
                invalid(
                    format!("`{word}` isn't one of `per_day:`'s settings"),
                    span,
                    "not a daily limit",
                )
                .next("`per_day:` takes `tokens:` and `cost:` alone")
            };
            out.push(problem);
            continue;
        }
        given.push(word.clone());
        let inner_span = inner.value().map_or_else(
            || node_span(path, inner.syntax()),
            |value| node_span(path, value.syntax()),
        );
        if word == "tokens" {
            read.per_day = tokens(&inner, inner_span, "per_day: { tokens: }", out);
        } else {
            read.cost = cost(db, project, &inner, inner_span, out);
        }
    }
    if !given.iter().any(|word| word == "tokens") {
        out.push(missing(
            key_span,
            "`per_day:` sets no daily tokens",
            "no `tokens:`",
            "Add `tokens: 2000000` to `per_day:`",
        ));
    }
}

/// A limit in tokens: a whole number from 1 to a trillion.
fn tokens(entry: &ast::Member, span: Span, what: &str, out: &mut Vec<Diagnostic>) -> Option<u64> {
    let value = entry
        .value()
        .as_ref()
        .map_or(Const::Other, Const::from_expr);
    let read = match value {
        Const::Number {
            value,
            decimal: false,
        } if value.scale == 0 => u64::try_from(value.mantissa)
            .ok()
            .filter(|tokens| (1..=MOST_TOKENS).contains(tokens)),
        _ => None,
    };
    if read.is_none() {
        out.push(
            invalid(
                format!("`{what}` must be a number of tokens"),
                span,
                "not a whole number of tokens",
            )
            .next("Write a whole number from 1 to 1000000000000, such as `20000`"),
        );
    }
    read
}

/// `per_day: { cost: }`: a `Money` setting.
fn cost(
    db: &dyn Db,
    project: Project,
    entry: &ast::Member,
    span: Span,
    out: &mut Vec<Diagnostic>,
) -> Option<String> {
    let lookup = Lookup::new(db, project);
    let field = entry.value().as_ref().and_then(crate::ai::reads_setting);
    let Some(field) = field else {
        out.push(
            invalid("`cost:` must be a setting", span, "not a setting")
                .reason("The day's estimated cost may not pass a `Money` setting, `config.<name>`, so each deployment sets its own and changing it is reviewed like any other setting.")
                .next("Declare `ai_daily_spend: Money { default: \"20.00 GBP\" }` in `config.jadpo`, and write `cost: config.ai_daily_spend`"),
        );
        return None;
    };
    // An unknown setting is reported where it is read.
    let setting = config(db, project).setting(&field)?;
    if lookup.base_prim(&setting.ty) != Some(Prim::Money) {
        out.push(
            invalid(
                "`cost:` must be an amount of money",
                span,
                &format!("`{field}` is `{}`", setting.ty),
            )
            .reason("The day's estimated cost is compared with an amount in one currency, which a `Money` setting holds.")
            .next(format!("Declare `{field}: Money` in `config.jadpo`")),
        );
        return None;
    }
    Some(field)
}

/// The database the counts are kept in: the one `store:` names, `main`
/// when that is a database, or the only one.
fn counted_in(
    db: &dyn Db,
    project: Project,
    named: Option<(String, Span)>,
    head: Span,
    out: &mut Vec<Diagnostic>,
) -> Option<String> {
    let connections = crate::data::connections(db, project);
    let databases = connections.databases();
    let listed = crate::and_list(
        &databases
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>(),
    );
    if let Some((written, span)) = named {
        if connections.kind(&written) == Some(ConnectionKind::Postgres) {
            return Some(written);
        }
        out.push(
            invalid(format!("`store: {written}` isn't a database"), span, "not a database")
                .reason(format!(
                    "The day's counts are kept in a database every instance shares, so none is lost; this application's databases are {listed}."
                ))
                .next("Name one of the application's database connections"),
        );
        return None;
    }
    if databases.iter().any(|name| name == crate::data::MAIN) {
        return Some(crate::data::MAIN.to_string());
    }
    if let [only] = databases.as_slice() {
        return Some(only.clone());
    }
    out.push(
        invalid("`limits:` doesn't say which database keeps its counts", head, "no `store:`")
            .reason(format!(
                "The day's counts are kept in a database every instance shares, so none is lost; this application has {listed}, none of them `main`."
            ))
            .next(match databases.first() {
                Some(first) => format!("Add `store: {first}` to `limits:`"),
                None => "Add a PostgreSQL connection under `connections:` in `app.jadpo`".to_string(),
            }),
    );
    None
}

fn invalid(title: impl Into<String>, span: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::AI_SETTINGS_INVALID, title)
        .at(span, label.to_string())
        .reason(REASON)
}

/// `semantic.ai_budget_missing`.
fn missing(span: Span, title: &str, label: &str, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::SEMANTIC_AI_BUDGET_MISSING, title)
        .at(span, label.to_string())
        .reason("Nothing an application asks a model is unlimited: `per_day: { tokens: }` caps the day's tokens, so a loop, or a prompt an injection makes run on, can't spend without end.")
        .next(next.to_string())
}
