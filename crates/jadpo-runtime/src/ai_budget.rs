//! What `ai` calls may spend (area 21, "Budgets"; chunk 5.5v5): the
//! limits `ai: { limits: }` sets, checked before a call is sent and
//! charged what it used after.
//!
//! - **Before sending** ([`take`]), a call's most possible use, its
//!   prompt's estimated tokens and `max_output:` for each of its
//!   `attempts:`, is taken from every limit it counts against: the
//!   request's, kept by the invocation; and the day's, the application's
//!   and the caller's, each a row of `jadpo_ai_usage` in the limits'
//!   store, taken by one conditional statement, so concurrent calls on
//!   several instances never pass a limit between them. A call any limit
//!   can't cover is `over_budget` with nothing sent, and a store that
//!   can't be reached refuses it `unavailable` rather than spend
//!   unmetered.
//! - **After** ([`charge`]), it is charged what the provider says it used,
//!   or, for an answer that says nothing, the estimate of what was sent
//!   and answered, and what it took is given back; its prompt and model's
//!   row counts it too, for `jadpo ai usage`.
//! - **The cost** is estimated from the package's price table: each
//!   model's price per million input, cached and output tokens, in
//!   millionths of its currency. A model with no price costs nothing to
//!   the estimate (the built-in driver's included); a price in another
//!   currency than `per_day: { cost: }`'s can't be compared with it, so
//!   the call is refused `misconfigured`.
//!
//! Nothing here holds a prompt or an answer: only counts, the prompt's
//! name, the model's and a digest of who the caller is.

use jadpo_std::ai::{AiDriver, AiPrice, GenerateSite};
use jadpo_value::Value;

use crate::ai::Ai;
use crate::{Ctx, Database, SqlType, SqlValue};

/// How long the counts of a day are kept: two months, so `jadpo ai
/// usage` shows a whole month and the one before it.
pub const KEPT_MS: i64 = 62 * DAY_MS;

const DAY_MS: i64 = 86_400_000;

/// The application's window: one row a day.
pub const APP: &str = "app";
/// A caller's window: one row a day for each, keyed by a digest of who
/// it is.
pub const CALLER: &str = "caller";
/// A prompt and model's count, keyed `<prompt> <model>`, which `jadpo ai
/// usage` reads.
pub const PROMPT: &str = "prompt";

/// Takes `$4` tokens and `$5` millionths of a currency from window `$2`
/// `$3` of day `$1` at `$8`, unless its tokens, with what calls under way
/// still hold, would pass `$6`, or its cost would pass `$7` (`NULL` for no
/// cost limit): a row when taken, none when not. A first call of the day
/// makes the row, its use already checked against the limits.
pub const TAKE: &str = "INSERT INTO \"jadpo_ai_usage\" AS \"used\" (\"day\", \"scope\", \"key\", \"calls\", \"input_tokens\", \"output_tokens\", \"cached_tokens\", \"reserved_tokens\", \"cost_micros\", \"reserved_micros\", \"currency\", \"updated_at\") VALUES ($1, $2, $3, 0, 0, 0, 0, $4::bigint, 0, $5::bigint, NULL, $8) ON CONFLICT (\"day\", \"scope\", \"key\") DO UPDATE SET \"reserved_tokens\" = \"used\".\"reserved_tokens\" + $4::bigint, \"reserved_micros\" = \"used\".\"reserved_micros\" + $5::bigint, \"updated_at\" = $8 WHERE \"used\".\"input_tokens\" + \"used\".\"output_tokens\" + \"used\".\"reserved_tokens\" + $4::bigint <= $6::bigint AND ($7::bigint IS NULL OR \"used\".\"cost_micros\" + \"used\".\"reserved_micros\" + $5::bigint <= $7::bigint) RETURNING 1";

/// Charges window `$2` `$3` of day `$1` one call of `$4` input, `$5`
/// output and `$6` cached tokens costing `$8` millionths of `$10` (`NULL`
/// with no price), at `$11`, giving back the `$7` tokens and `$9`
/// millionths it took.
pub const CHARGE: &str = "INSERT INTO \"jadpo_ai_usage\" AS \"used\" (\"day\", \"scope\", \"key\", \"calls\", \"input_tokens\", \"output_tokens\", \"cached_tokens\", \"reserved_tokens\", \"cost_micros\", \"reserved_micros\", \"currency\", \"updated_at\") VALUES ($1, $2, $3, 1, $4::bigint, $5::bigint, $6::bigint, 0, $8::bigint, 0, $10, $11) ON CONFLICT (\"day\", \"scope\", \"key\") DO UPDATE SET \"calls\" = \"used\".\"calls\" + 1, \"input_tokens\" = \"used\".\"input_tokens\" + $4::bigint, \"output_tokens\" = \"used\".\"output_tokens\" + $5::bigint, \"cached_tokens\" = \"used\".\"cached_tokens\" + $6::bigint, \"reserved_tokens\" = GREATEST(\"used\".\"reserved_tokens\" - $7::bigint, 0), \"cost_micros\" = \"used\".\"cost_micros\" + $8::bigint, \"reserved_micros\" = GREATEST(\"used\".\"reserved_micros\" - $9::bigint, 0), \"currency\" = COALESCE($10, \"used\".\"currency\"), \"updated_at\" = $11";

/// Gives back to window `$2` `$3` of day `$1` the `$4` tokens and `$5`
/// millionths a call took and didn't use, at `$6`.
pub const GIVE_BACK: &str = "UPDATE \"jadpo_ai_usage\" SET \"reserved_tokens\" = GREATEST(\"reserved_tokens\" - $4::bigint, 0), \"reserved_micros\" = GREATEST(\"reserved_micros\" - $5::bigint, 0), \"updated_at\" = $6 WHERE \"day\" = $1 AND \"scope\" = $2 AND \"key\" = $3";

/// The application's and each prompt's counts from day `$1` on, for
/// `jadpo ai usage`.
pub const READ: &str = "SELECT \"day\", \"scope\", \"key\", \"calls\", \"input_tokens\", \"output_tokens\", \"cached_tokens\", \"reserved_tokens\", \"cost_micros\", \"currency\" FROM \"jadpo_ai_usage\" WHERE \"day\" >= $1 AND \"scope\" IN ('app', 'prompt') ORDER BY \"day\", \"scope\", \"key\"";

/// Counts last changed at or before `$1`, at most `$2`, which the tick
/// deletes.
pub const OLD_DAYS: &str = "DELETE FROM \"jadpo_ai_usage\" WHERE (\"day\", \"scope\", \"key\") IN (SELECT \"day\", \"scope\", \"key\" FROM \"jadpo_ai_usage\" WHERE \"updated_at\" <= $1 LIMIT $2) RETURNING 1";

/// Why a call isn't sent.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// A limit can't cover it.
    OverBudget,
    /// The limits' store can't be reached.
    Unavailable,
    /// Its price can't be compared with the day's cost limit.
    Misconfigured,
}

/// What a call took from the day's limits, to be charged after it.
#[derive(Debug)]
pub(crate) struct Taken {
    /// The store the counts are in, and the day taken from.
    store: String,
    day: i32,
    /// Each window it took from, with what it took.
    windows: Vec<(&'static str, String)>,
    tokens: u64,
    micros: u64,
    /// The prompt and model, for its own count.
    prompt: String,
    price: Option<AiPrice>,
}

/// What a call used, attempts included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Used {
    pub input: u64,
    pub output: u64,
    pub cached: u64,
}

impl Used {
    pub(crate) fn add(&mut self, other: Used) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cached = self.cached.saturating_add(other.cached);
    }

    fn tokens(self) -> u64 {
        self.input.saturating_add(self.output)
    }
}

/// The day `millis` falls on, in UTC, as days since 1970-01-01.
pub fn day_of(millis: i64) -> i32 {
    i32::try_from(millis.div_euclid(DAY_MS)).unwrap_or(i32::MAX)
}

/// What `tokens` cost at `per_million` millionths for each million,
/// rounded up.
fn cost(tokens: u64, per_million: u64) -> u64 {
    let micros = u128::from(tokens) * u128::from(per_million);
    u64::try_from(micros.div_ceil(1_000_000)).unwrap_or(u64::MAX)
}

/// What `used` cost at `price`.
fn priced(used: Used, price: &AiPrice) -> u64 {
    let cached = used.cached.min(used.input);
    cost(used.input - cached, price.input)
        .saturating_add(cost(cached, price.cached))
        .saturating_add(cost(used.output, price.output))
}

/// The day's cost limit in millionths of its currency, and its code,
/// from the `Money` setting's value.
fn cost_limit(value: &Value) -> Option<(u64, &'static str)> {
    let Value::Money(money) = value else {
        return None;
    };
    let exponent = jadpo_std::money::exponent(money.currency)?;
    let scale = 10u64.checked_pow(6u32.checked_sub(exponent)?)?;
    let minor = u64::try_from(money.minor.max(0)).ok()?;
    Some((minor.saturating_mul(scale), money.code()))
}

/// Who a call counts against: a digest of its signed-in caller, or of
/// the address it came from; `None` for the system's own work.
fn caller_key(ctx: &Ctx<'_>) -> Option<String> {
    let who = match (ctx.caller(), ctx.client()) {
        (Some(caller), _) => caller.key.clone(),
        (None, Some(address)) => format!("address {address}"),
        (None, None) => return None,
    };
    let digest = jadpo_value::hash::sha256(who.as_bytes());
    Some(
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// Takes from every limit the most a call to `site`'s prompt of `model`
/// (its `ModelId` text) can use, sending `input` estimated tokens a
/// time: `None` with no limits to keep (a program the checker didn't
/// make), the [`Taken`] to charge after it otherwise.
pub(crate) async fn take(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    site: &GenerateSite,
    model: &str,
    input: u64,
    settings: &[(u32, Value)],
) -> Result<Option<Taken>, Refused> {
    let Some(limits) = ai.plan.limits.as_ref() else {
        return Ok(None);
    };
    let attempts = u64::from(site.attempts.max(1));
    let most = Used {
        input: input.saturating_mul(attempts),
        output: u64::from(site.max_output).saturating_mul(attempts),
        cached: 0,
    };
    if let Some(per_request) = limits.per_request
        && ctx.ai_spent().saturating_add(most.tokens()) > per_request
    {
        return Err(Refused::OverBudget);
    }
    let day = day_of(ctx.now());
    let price = match &ai.plan.driver {
        AiDriver::Package(package) => package
            .price(crate::ai::model_name(model), &jadpo_std::format_date(day))
            .cloned(),
        AiDriver::OpenAiCompatible(_) => None,
    };
    let limit = match limits.cost {
        Some(at) => {
            let value = settings
                .iter()
                .find(|(number, _)| *number == at)
                .map(|(_, value)| value);
            match value.and_then(cost_limit) {
                Some(limit) => Some(limit),
                None => return Err(Refused::Misconfigured),
            }
        }
        None => None,
    };
    if let (Some((_, currency)), Some(price)) = (limit, &price)
        && price.currency != currency
    {
        return Err(Refused::Misconfigured);
    }
    let micros = price.as_ref().map_or(0, |price| priced(most, price));
    let tokens = most.tokens();
    let mut windows: Vec<(&'static str, String, u64)> = vec![(APP, String::new(), limits.per_day)];
    if let (Some(per_caller), Some(key)) = (limits.per_caller_per_day, caller_key(ctx)) {
        windows.push((CALLER, key, per_caller));
    }
    // A call over a limit by itself is refused without asking the store.
    if windows.iter().any(|(_, _, limit)| tokens > *limit)
        || limit.is_some_and(|(limit, _)| micros > limit)
    {
        return Err(Refused::OverBudget);
    }
    let Some(database) = ctx.host().database(&limits.store) else {
        return Err(Refused::Unavailable);
    };
    let now = crate::outbox::micros(ctx.now());
    let mut taken: Vec<(&'static str, String)> = Vec::new();
    for (scope, key, most_tokens) in windows {
        let cost_limit = match (scope, limit) {
            (APP, Some((limit, _))) => SqlValue::Int8(clamp(limit)),
            _ => SqlValue::Null(SqlType::Int8),
        };
        let params = [
            SqlValue::Date(day),
            SqlValue::Text(scope.to_string()),
            SqlValue::Text(key.clone()),
            SqlValue::Int8(clamp(tokens)),
            SqlValue::Int8(clamp(micros)),
            SqlValue::Int8(clamp(most_tokens)),
            cost_limit,
            now.clone(),
        ];
        let refused = match database.query_once(TAKE, &params).await {
            Ok(rows) if !rows.is_empty() => None,
            Ok(_) => Some(Refused::OverBudget),
            Err(_) => Some(Refused::Unavailable),
        };
        if let Some(refused) = refused {
            for (scope, key) in &taken {
                give_back(database, day, scope, key, (tokens, micros), ctx.now()).await;
            }
            return Err(refused);
        }
        taken.push((scope, key));
    }
    Ok(Some(Taken {
        store: limits.store.clone(),
        day,
        windows: taken,
        tokens,
        micros,
        prompt: format!("{} {model}", site.prompt),
        price,
    }))
}

fn clamp(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Gives back what a call took from one window and didn't use. What
/// can't be given back stays taken: the limit is kept the more strictly.
async fn give_back(
    database: &dyn Database,
    day: i32,
    scope: &str,
    key: &str,
    (tokens, micros): (u64, u64),
    now: i64,
) {
    let _ = database
        .query_once(
            GIVE_BACK,
            &[
                SqlValue::Date(day),
                SqlValue::Text(scope.to_string()),
                SqlValue::Text(key.to_string()),
                SqlValue::Int8(clamp(tokens)),
                SqlValue::Int8(clamp(micros)),
                crate::outbox::micros(now),
            ],
        )
        .await;
}

/// Charges a call what it `used`, giving back what it took, and counts it
/// against its request. A count that can't be written leaves what was
/// taken taken.
pub(crate) async fn charge(ctx: &mut Ctx<'_>, taken: Option<Taken>, used: Used) {
    ctx.add_ai_spent(used.tokens());
    let Some(taken) = taken else {
        return;
    };
    let Some(database) = ctx.host().database(&taken.store) else {
        return;
    };
    let micros = taken.price.as_ref().map_or(0, |price| priced(used, price));
    let currency = match &taken.price {
        Some(price) => SqlValue::Text(price.currency.clone()),
        None => SqlValue::Null(SqlType::Text),
    };
    let now = crate::outbox::micros(ctx.now());
    let prompt = (PROMPT, taken.prompt.clone(), 0, 0);
    let windows = taken
        .windows
        .iter()
        .map(|(scope, key)| (*scope, key.clone(), taken.tokens, taken.micros))
        .chain([prompt]);
    for (scope, key, tokens, reserved_micros) in windows {
        let _ = database
            .query_once(
                CHARGE,
                &[
                    SqlValue::Date(taken.day),
                    SqlValue::Text(scope.to_string()),
                    SqlValue::Text(key),
                    SqlValue::Int8(clamp(used.input)),
                    SqlValue::Int8(clamp(used.output)),
                    SqlValue::Int8(clamp(used.cached)),
                    SqlValue::Int8(clamp(tokens)),
                    SqlValue::Int8(clamp(micros)),
                    SqlValue::Int8(clamp(reserved_micros)),
                    currency.clone(),
                    now.clone(),
                ],
            )
            .await;
    }
}

/// [`charge`], then `done`: what a call that ends early gives.
pub(crate) async fn charged<T>(ctx: &mut Ctx<'_>, taken: Option<Taken>, used: Used, done: T) -> T {
    charge(ctx, taken, used).await;
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price() -> AiPrice {
        AiPrice {
            model: "acme-small".into(),
            from: "2026-01-01".into(),
            currency: "USD".into(),
            input: 400_000,
            cached: 100_000,
            output: 1_600_000,
        }
    }

    /// A call is charged each kind of token at its own price, rounded up
    /// to a millionth, cached tokens being part of the input.
    #[test]
    fn a_call_costs_its_tokens_at_the_models_prices() {
        let used = Used {
            input: 1_000,
            output: 500,
            cached: 200,
        };
        // 800 × 0.40 + 200 × 0.10 + 500 × 1.60 millionths a token.
        assert_eq!(priced(used, &price()), 320 + 20 + 800);
        assert_eq!(cost(1, 1), 1);
        assert_eq!(cost(0, 400_000), 0);
    }

    /// The day's cost limit is read in millionths of its currency's
    /// major unit, whatever its exponent.
    #[test]
    fn a_cost_limit_is_read_in_millionths() {
        let gbp = jadpo_std::money::CURRENCIES
            .iter()
            .position(|(_, code)| *code == "GBP")
            .expect("GBP") as u32;
        let jpy = jadpo_std::money::CURRENCIES
            .iter()
            .position(|(_, code)| *code == "JPY")
            .expect("JPY") as u32;
        let limit =
            |currency, minor| cost_limit(&Value::Money(jadpo_value::Money { currency, minor }));
        assert_eq!(limit(gbp, 2_000), Some((20_000_000, "GBP")));
        assert_eq!(limit(jpy, 500), Some((500_000_000, "JPY")));
        assert_eq!(cost_limit(&Value::Int(3)), None);
    }

    /// A day starts at midnight UTC, before 1970 too.
    #[test]
    fn a_day_starts_at_midnight_utc() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(DAY_MS - 1), 0);
        assert_eq!(day_of(DAY_MS), 1);
        assert_eq!(day_of(-1), -1);
    }
}
