//! `jadpo ai usage` (area 21, "Budgets"; chunk 5.5v5): the tokens the
//! application's `ai` calls used today and this month (UTC), and their
//! estimated cost, by prompt and model, against the daily limit, from the
//! counts `ai: { limits: }` keeps in its store (`jadpo_ai_usage`). It reads
//! counts alone, never a prompt or an answer.
//!
//! The screen, designed before it was built (CLI design language, area 14):
//!
//! ```text
//!   day         2026-10-20 (UTC)
//!   today       4210 of 2000000 tokens · 3 calls
//!   cost        USD 0.01 today
//!
//!   prompt           model              calls  today  this month  cost
//!   answer_question  acme/acme-small        3   4210       61240  USD 0.11
//!   draft_reply      local/llama3.2         1      0         880  –
//!
//! ✓ 4210 tokens today  0% of the daily limit · 62120 this month
//! ```
//!
//! Calls and cost are this month's; a model with no price shows `–`. A
//! day whose tokens have reached the limit ends in a failure footer,
//! since every call is refused `over_budget` until midnight UTC. No hint:
//! nothing is left to run. The database is the one the store's
//! connection's setting gives, a running `jadpo dev`'s, or `--url`'s.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Table};

use crate::run::{Call, Reply};
use crate::schedules::{now, store_error, url_of};

fn arguments(message: impl Into<String>, reason: impl Into<String>, next: &str) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::CLI_AI_ARGUMENTS, message)
            .reason(reason)
            .next(next),
    )
}

/// One prompt and model's counts.
#[derive(Default)]
struct Counted {
    calls: u64,
    today: u64,
    month: u64,
    input: u64,
    output: u64,
    cached: u64,
    cost: u64,
    currency: Option<String>,
}

/// A row of `jadpo_ai_usage` as [`jadpo_runtime::ai_budget::READ`] reads
/// it.
struct Row {
    day: i32,
    scope: String,
    key: String,
    calls: u64,
    input: u64,
    output: u64,
    cached: u64,
    reserved: u64,
    cost: u64,
    currency: Option<String>,
}

fn count(value: Option<&jadpo_runtime::SqlValue>) -> u64 {
    use jadpo_runtime::SqlValue;
    match value {
        Some(SqlValue::Int8(n)) => u64::try_from(*n).unwrap_or(0),
        Some(SqlValue::Int4(n)) => u64::try_from(*n).unwrap_or(0),
        _ => 0,
    }
}

fn row(values: &[jadpo_runtime::SqlValue]) -> Option<Row> {
    use jadpo_runtime::SqlValue;
    let text = |at: usize| match values.get(at) {
        Some(SqlValue::Text(text)) => Some(text.clone()),
        _ => None,
    };
    Some(Row {
        day: match values.first() {
            Some(SqlValue::Date(day)) => *day,
            _ => return None,
        },
        scope: text(1)?,
        key: text(2)?,
        calls: count(values.get(3)),
        input: count(values.get(4)),
        output: count(values.get(5)),
        cached: count(values.get(6)),
        reserved: count(values.get(7)),
        cost: count(values.get(8)),
        currency: text(9),
    })
}

/// `micros` millionths of `currency` as its text form, rounded up to its
/// minor unit: `USD 0.11`.
fn money(micros: u64, currency: &str) -> String {
    let exponent = jadpo_std::money::exponent_of(currency).min(6);
    let unit = 10u64.pow(6 - exponent);
    let minor = micros.div_ceil(unit);
    if exponent == 0 {
        return format!("{currency} {minor}");
    }
    let scale = 10u64.pow(exponent);
    format!(
        "{currency} {}.{:0width$}",
        minor / scale,
        minor % scale,
        width = exponent as usize
    )
}

pub fn usage(call: &Call) -> Reply {
    if let Some(extra) = call.arguments.get(1) {
        return arguments(
            format!("`jadpo ai usage` takes no `{extra}`"),
            "It shows every prompt's usage today and this month.",
            "Run `jadpo ai usage`",
        );
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_needed("ai usage");
    };
    let loaded = jadpo_db::load(&root);
    let settings = jadpo_check::ai::settings(&loaded.db, loaded.project);
    if !settings.written {
        return arguments(
            "This project asks no model",
            "`jadpo ai usage` reads the counts `ai: { limits: }` keeps, and `app.jadpo` has no `ai:`.",
            "Add `ai:` to `application { }` in `app.jadpo`",
        );
    }
    let (Some(store), Some(per_day)) = (&settings.limits.store, settings.limits.per_day) else {
        return arguments(
            "`ai: { limits: }` doesn't check",
            "The counts are kept in the store `limits:` names, under the daily limit it sets.",
            "Run `jadpo check`, fix what it reports, then run this again",
        );
    };
    let url = match url_of(call, &root, &loaded, store) {
        Ok(Some(url)) => url,
        Ok(None) => {
            return arguments(
                format!("`jadpo ai usage` needs the address of the database `{store}`"),
                "No variable gives the address of its connection, and no `jadpo dev` is running with one: a connection reads its address from the setting its `url:` names (area 10), and `--url` names a database directly.",
                "Run `jadpo config set <the connection's setting>`, or add `--url postgresql://…`",
            );
        }
        Err(reply) => return reply,
    };
    let now = now(call);
    let today = jadpo_runtime::ai_budget::day_of(now);
    let (year, month, _) = jadpo_std::civil_date(today);
    let first = jadpo_std::epoch_day(year, month, 1).unwrap_or(today);
    let rows = match read(store, &url, first) {
        Ok(rows) => rows,
        Err(reply) => return reply,
    };
    shown(&rows, today, per_day)
}

/// Every count from day `first` on in the database at `url`.
fn read(store: &str, url: &str, first: i32) -> Result<Vec<Row>, Reply> {
    let failed = |error: String| {
        Reply::failed(
            Diagnostic::new(
                &rule::AI_USAGE_UNAVAILABLE,
                format!("The database `{store}` couldn't be read"),
            )
            .reason(format!(
                "Reading what the application's `ai` calls used failed: {error}. Its table comes with its migrations."
            ))
            .next("Check the database is running and its address is right"),
        )
    };
    let runtime = jadpo_host_native::Runtime::new().map_err(|error| failed(error.to_string()))?;
    let database = jadpo_host_native::postgres(url).map_err(|error| failed(store_error(&error)))?;
    use jadpo_runtime::Database;
    let rows = runtime
        .block_on(database.query_once(
            jadpo_runtime::ai_budget::READ,
            &[jadpo_runtime::SqlValue::Date(first)],
        ))
        .map_err(|error| failed(store_error(&error)))?;
    Ok(rows.iter().filter_map(|values| row(values)).collect())
}

/// The screen and data for the counts `rows` read, on day `today`.
fn shown(rows: &[Row], today: i32, per_day: u64) -> Reply {
    let mut prompts: BTreeMap<(String, String), Counted> = BTreeMap::new();
    let mut day = Row {
        day: today,
        scope: String::new(),
        key: String::new(),
        calls: 0,
        input: 0,
        output: 0,
        cached: 0,
        reserved: 0,
        cost: 0,
        currency: None,
    };
    for row in rows {
        match row.scope.as_str() {
            jadpo_runtime::ai_budget::APP if row.day == today => {
                day.calls = row.calls;
                day.input = row.input;
                day.output = row.output;
                day.reserved = row.reserved;
                day.cost = row.cost;
                day.currency = row.currency.clone();
            }
            jadpo_runtime::ai_budget::PROMPT => {
                let (prompt, model) = row.key.split_once(' ').unwrap_or((&row.key, ""));
                let counted = prompts
                    .entry((prompt.to_string(), model.to_string()))
                    .or_default();
                let tokens = row.input + row.output;
                counted.calls += row.calls;
                counted.month += tokens;
                counted.input += row.input;
                counted.output += row.output;
                counted.cached += row.cached;
                counted.cost += row.cost;
                if row.day == today {
                    counted.today += tokens;
                }
                if row.currency.is_some() {
                    counted.currency = row.currency.clone();
                }
            }
            _ => {}
        }
    }
    let used_today = day.input + day.output;
    let month: u64 = prompts.values().map(|counted| counted.month).sum();
    let day_text = jadpo_std::format_date(today);
    let cost_text = |micros: u64, currency: &Option<String>| match currency {
        Some(currency) => money(micros, currency),
        None => "–".to_string(),
    };
    let data = Json::object([
        ("day", Json::from(day_text.as_str())),
        (
            "today",
            Json::object([
                ("tokens", json_count(used_today)),
                ("calls", json_count(day.calls)),
                ("inUse", json_count(day.reserved)),
                ("limit", json_count(per_day)),
                (
                    "cost",
                    match &day.currency {
                        Some(currency) => Json::from(money(day.cost, currency).as_str()),
                        None => Json::Null,
                    },
                ),
            ]),
        ),
        ("monthTokens", json_count(month)),
        (
            "prompts",
            Json::array(prompts.iter().map(|((prompt, model), counted)| {
                Json::object([
                    ("prompt", Json::from(prompt.as_str())),
                    ("model", Json::from(model.as_str())),
                    ("calls", json_count(counted.calls)),
                    ("tokensToday", json_count(counted.today)),
                    ("tokensThisMonth", json_count(counted.month)),
                    ("inputTokens", json_count(counted.input)),
                    ("outputTokens", json_count(counted.output)),
                    ("cachedTokens", json_count(counted.cached)),
                    (
                        "cost",
                        match &counted.currency {
                            Some(currency) => Json::from(money(counted.cost, currency).as_str()),
                            None => Json::Null,
                        },
                    ),
                ])
            })),
        ),
    ]);
    let mut screen = Screen::new();
    if prompts.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "No model was asked this month",
            [Fact::plain(format!("limit {per_day} tokens a day"))],
        ));
        return Reply::screen(screen, data);
    }
    let mut fields = vec![
        ("day".to_string(), format!("{day_text} (UTC)")),
        (
            "today".to_string(),
            format!(
                "{used_today} of {per_day} tokens · {}",
                crate::schedules::plural(day.calls, "call", "calls")
            ),
        ),
    ];
    if day.currency.is_some() {
        fields.push((
            "cost".to_string(),
            format!("{} today", cost_text(day.cost, &day.currency)),
        ));
    }
    screen.push(Block::Fields(fields));
    let mut table = Table::new(["prompt", "model", "calls", "today", "this month", "cost"])
        .right(2)
        .right(3)
        .right(4)
        .cut(1);
    for ((prompt, model), counted) in &prompts {
        let cost = match &counted.currency {
            Some(currency) => Cell::text(money(counted.cost, currency)),
            None => Cell::muted("–"),
        };
        table = table.row(vec![
            Cell::text(prompt.clone()),
            Cell::muted(model.clone()),
            Cell::text(counted.calls.to_string()),
            Cell::text(counted.today.to_string()),
            Cell::text(counted.month.to_string()),
            cost,
        ]);
    }
    screen.push(Block::Table(table));
    let share = (u128::from(used_today) * 100 / u128::from(per_day.max(1))).min(100);
    let spent = used_today >= per_day;
    let facts = [
        if spent {
            Fact::problem("the daily limit is reached")
        } else {
            Fact::plain(format!("{share}% of the daily limit"))
        },
        Fact::plain(format!("{month} this month")),
    ];
    screen.push(
        Block::summary(
            if spent {
                Outcome::Failure
            } else {
                Outcome::Success
            },
            format!("{used_today} tokens today"),
            facts,
        )
        .with_plain_headline("ai usage"),
    );
    Reply::screen(screen, data)
}

fn json_count(n: u64) -> Json {
    Json::from(i64::try_from(n).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cost is shown in its currency's minor unit, rounded up, so a
    /// fraction of a cent shows as one.
    #[test]
    fn a_cost_is_shown_to_its_minor_unit_rounded_up() {
        assert_eq!(money(110_000, "USD"), "USD 0.11");
        assert_eq!(money(1, "USD"), "USD 0.01");
        assert_eq!(money(0, "GBP"), "GBP 0.00");
        assert_eq!(money(2_500_000, "JPY"), "JPY 3");
        assert_eq!(money(1_234_567_000, "BHD"), "BHD 1234.567");
    }
}
