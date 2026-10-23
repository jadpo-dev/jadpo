//! What `ai` calls may spend (area 21, "Budgets"; chunk 5.5v5): the
//! limits `ai: { limits: }` sets, checked before a call is sent and
//! charged what it used after.
//!
//! - **Before sending** ([`take`]), a call's most possible use, what
//!   each of its `attempts:` sends, each repair resending the
//!   conversation so far ([`most`]), and `max_output:` for each answer, is
//!   taken from every limit it counts against: the
//!   request's, kept by the invocation; and the day's, the application's
//!   and the caller's, each a row of `jadpo_ai_usage` in the limits'
//!   store, taken by one conditional statement, so concurrent calls on
//!   several instances never pass a limit between them. A call any limit
//!   can't cover is `over_budget` with nothing sent, and a store that
//!   can't be reached refuses it `unavailable` rather than spend
//!   unmetered. A send it didn't count, a send again after the provider
//!   couldn't answer or a repair longer than counted, takes what it sends
//!   before it goes ([`top_up`]), and isn't made when a limit can't cover
//!   it (chunk 7.9c).
//! - **After** ([`debit`]), it is charged what the provider says it used,
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

use jadpo_std::ai::{AiDriver, AiPrice};
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

/// Reserves APP alone when no caller window applies. One conditional UPSERT
/// is already atomic: a skipped row means over budget without changing it.
/// The first eight parameters have the same meanings as [`TAKE`]; the caller
/// prechecks amounts against their limits before a first INSERT.
pub const TAKE_APP: &str = "INSERT INTO \"jadpo_ai_usage\" AS \"used\" (\"day\", \"scope\", \"key\", \"calls\", \"input_tokens\", \"output_tokens\", \"cached_tokens\", \"reserved_tokens\", \"cost_micros\", \"reserved_micros\", \"currency\", \"touched_at\") VALUES ($1, $2, $3, 0, 0, 0, 0, $4::bigint, 0, $5::bigint, NULL, $8) ON CONFLICT (\"day\", \"scope\", \"key\") DO UPDATE SET \"reserved_tokens\" = \"used\".\"reserved_tokens\" + $4::bigint, \"reserved_micros\" = \"used\".\"reserved_micros\" + $5::bigint, \"touched_at\" = $8 WHERE \"used\".\"input_tokens\" + \"used\".\"output_tokens\" + \"used\".\"reserved_tokens\" + $4::bigint <= $6::bigint AND ($7::bigint IS NULL OR \"used\".\"cost_micros\" + \"used\".\"reserved_micros\" + $5::bigint <= $7::bigint) RETURNING 1";

/// Reserves every applicable window together. The first eight parameters
/// retain the APP row's amounts, limits and time; `$9` is the optional
/// caller digest and `$10` its token limit. APP precedes CALLER, so calls
/// sharing either window lock their rows in the same order.
///
/// Each UPSERT checks the *locked current row*, not a statement snapshot.
/// A conditional UPSERT may skip a row; a dependent insert then deliberately
/// conflicts with APP's private primary key to roll back the WHOLE statement,
/// including new rows and timestamps. APP either existed (and refused) or
/// was inserted by `taken`, so this insert cannot succeed. No compensation
/// writes, snapshot prechecks or second updates of the same row are used.
/// Only that exact private constraint is a budget refusal; other errors
/// remain unavailable. The caller checks that each amount fits alone first.
pub const TAKE: &str = r#"WITH windows AS (
    SELECT $2::text AS scope, $3::text AS key
    UNION ALL SELECT 'caller', $9::text WHERE $9::text IS NOT NULL
), taken AS (
    INSERT INTO "jadpo_ai_usage" AS used
        (day, scope, key, calls, input_tokens, output_tokens, cached_tokens,
         reserved_tokens, cost_micros, reserved_micros, currency, touched_at)
    SELECT $1::date, scope, key, 0, 0, 0, 0, $4::bigint, 0, $5::bigint, NULL, $8::timestamptz
    FROM windows ORDER BY scope, key
    ON CONFLICT (day, scope, key) DO UPDATE SET
        reserved_tokens = used.reserved_tokens + $4::bigint,
        reserved_micros = used.reserved_micros + $5::bigint,
        touched_at = $8::timestamptz
    WHERE used.input_tokens + used.output_tokens + used.reserved_tokens + $4::bigint
            <= CASE WHEN used.scope = 'app' THEN $6::bigint ELSE $10::bigint END
        AND (used.scope <> 'app' OR $7::bigint IS NULL
             OR used.cost_micros + used.reserved_micros + $5::bigint <= $7::bigint)
    RETURNING 1
), refused AS (
    INSERT INTO "jadpo_ai_usage"
        (day, scope, key, calls, input_tokens, output_tokens, cached_tokens,
         reserved_tokens, cost_micros, reserved_micros, currency, touched_at)
    SELECT $1::date, $2::text, $3::text, 0, 0, 0, 0, 0, 0, 0, NULL, $8::timestamptz
    FROM (SELECT count(*) AS count FROM taken) AS tally
    WHERE tally.count <> (SELECT count(*) FROM windows)
    RETURNING 1
)
SELECT * FROM taken UNION ALL SELECT * FROM refused"#;

/// Charges APP, optional CALLER (`$12`) and PROMPT (`$13`) together,
/// releasing the full reservation in each budget window and none in
/// PROMPT. A statement error leaves every window held. Parameters 1–11
/// retain their previous meanings (day, APP scope/key, usage, releases,
/// cost, currency and timestamp), so accounting and rounding stay exact.
pub const DEBIT: &str = r#"INSERT INTO "jadpo_ai_usage" AS used
    (day, scope, key, calls, input_tokens, output_tokens, cached_tokens,
     reserved_tokens, cost_micros, reserved_micros, currency, touched_at)
SELECT $1::date, scope, key, 1, $4::bigint, $5::bigint, $6::bigint,
       0, $8::bigint, 0, $10::text, $11::timestamptz
FROM (SELECT $2::text AS scope, $3::text AS key
      UNION ALL SELECT 'caller', $12::text WHERE $12::text IS NOT NULL
      UNION ALL SELECT 'prompt', $13::text) AS windows
ORDER BY scope, key
ON CONFLICT (day, scope, key) DO UPDATE SET
    calls = used.calls + 1,
    input_tokens = used.input_tokens + $4::bigint,
    output_tokens = used.output_tokens + $5::bigint,
    cached_tokens = used.cached_tokens + $6::bigint,
    reserved_tokens = GREATEST(used.reserved_tokens - CASE WHEN used.scope = 'prompt' THEN 0 ELSE $7::bigint END, 0),
    cost_micros = used.cost_micros + $8::bigint,
    reserved_micros = GREATEST(used.reserved_micros - CASE WHEN used.scope = 'prompt' THEN 0 ELSE $9::bigint END, 0),
    currency = COALESCE($10::text, used.currency),
    touched_at = $11::timestamptz"#;

/// The application's and each prompt's counts from day `$1` on, for
/// `jadpo ai usage`.
pub const READ: &str = "SELECT \"day\", \"scope\", \"key\", \"calls\", \"input_tokens\", \"output_tokens\", \"cached_tokens\", \"reserved_tokens\", \"cost_micros\", \"currency\" FROM \"jadpo_ai_usage\" WHERE \"day\" >= $1 AND \"scope\" IN ('app', 'prompt') ORDER BY \"day\", \"scope\", \"key\"";

/// Counts last changed at or before `$1`, at most `$2`, which the tick
/// deletes.
pub const OLD_DAYS: &str = "DELETE FROM \"jadpo_ai_usage\" WHERE (\"day\", \"scope\", \"key\") IN (SELECT \"day\", \"scope\", \"key\" FROM \"jadpo_ai_usage\" WHERE \"touched_at\" <= $1 LIMIT $2) RETURNING 1";

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
    /// Each window it took from, with its limit.
    windows: Vec<(&'static str, String, u64)>,
    /// The day's cost limit, and the request's, which more taken keeps
    /// to.
    cost_limit: Option<u64>,
    per_request: Option<u64>,
    /// What it took, more taken since included.
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
fn cost_of(used: Used, price: &AiPrice) -> u64 {
    let cached = used.cached.min(used.input);
    cost(used.input - cached, price.input)
        .saturating_add(cost(cached, price.cached))
        .saturating_add(cost(used.output, price.output))
}

/// The settings a call's limits read: the day's cost limit, and each
/// price the application sets (chunk 5.5v7).
pub(crate) fn settings_read(plan: &jadpo_std::ai::AiPlan) -> Vec<u32> {
    let mut read: Vec<u32> = plan
        .limits
        .as_ref()
        .and_then(|limits| limits.cost)
        .into_iter()
        .collect();
    for price in &plan.prices {
        read.extend([price.input, price.output]);
        read.extend(price.cached);
    }
    read
}

/// The price `set` gives `model` (its `ModelId` text), read from its
/// `Money` settings in `settings`, each in millionths of their currency
/// for a million tokens: `None` when one has no value, or they are in
/// more than one currency.
fn set_price(
    set: &jadpo_std::ai::AiPriceSetting,
    model: &str,
    settings: &[(u32, Value)],
) -> Option<AiPrice> {
    let read = |at: u32| {
        settings
            .iter()
            .find(|(number, _)| *number == at)
            .and_then(|(_, value)| cost_limit(value))
    };
    let (input, currency) = read(set.input)?;
    let (output, output_in) = read(set.output)?;
    let (cached, cached_in) = match set.cached {
        Some(at) => read(at)?,
        None => (input, currency),
    };
    (output_in == currency && cached_in == currency).then(|| AiPrice {
        model: crate::ai::model_name(model).to_string(),
        from: String::new(),
        currency: currency.to_string(),
        input,
        cached,
        output,
    })
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
/// the address it came from, an IPv6 one by its /64 as the default rate
/// limits count it; `None` for the system's own work.
fn caller_key(ctx: &Ctx<'_>) -> Option<String> {
    let who = match (ctx.caller(), ctx.client()) {
        (Some(caller), _) => caller.key.clone(),
        (None, Some(address)) => format!("address {}", crate::http::address::counted_as(address)),
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

/// What a call asks, as its limits count it: the prompt it is counted
/// under (an embedding's `embed`), the most tokens each answer may take
/// (none for an embedding) and how many answers it may ask for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Asked<'a> {
    pub prompt: &'a str,
    /// The model's name in `ai: { models: }`, `default`.
    pub model: &'a str,
    pub max_output: u32,
    pub attempts: u32,
}

impl<'a> Asked<'a> {
    /// What an `ai.generate` or `ai.stream` call at `site` asks.
    pub(crate) fn of(site: &'a jadpo_std::ai::GenerateSite) -> Asked<'a> {
        Asked {
            prompt: &site.prompt,
            model: &site.model,
            max_output: site.max_output,
            attempts: site.attempts,
        }
    }
}

/// Takes from every limit the most a call to `site`'s prompt of `model`
/// (its `ModelId` text) can use, sending `input` estimated tokens a
/// time: `None` with no limits to keep (a program the checker didn't
/// make), the [`Taken`] to charge after it otherwise.
pub(crate) async fn take(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    site: Asked<'_>,
    model: &str,
    input: u64,
    settings: &[(u32, Value)],
) -> Result<Option<Taken>, Refused> {
    let Some(limits) = ai.plan.limits.as_ref() else {
        return Ok(None);
    };
    let most = most(input, site);
    if let Some(per_request) = limits.per_request
        && ctx.ai_spent().saturating_add(most.tokens()) > per_request
    {
        return Err(Refused::OverBudget);
    }
    let day = day_of(crate::steps::wall(ctx));
    // The application's own price for the model, else its package's
    // (chunk 5.5v7).
    let price = match ai
        .plan
        .prices
        .iter()
        .find(|price| price.model == site.model)
    {
        Some(set) => match set_price(set, model, settings) {
            Some(price) => Some(price),
            None => return Err(Refused::Misconfigured),
        },
        None => match &ai.plan.driver {
            AiDriver::Package(package) => package
                .price(crate::ai::model_name(model), &jadpo_std::format_date(day))
                .cloned(),
            AiDriver::OpenAiCompatible(_) => None,
        },
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
    let micros = price.as_ref().map_or(0, |price| cost_of(most, price));
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
    reserve(
        database,
        day,
        &windows,
        limit.map(|(limit, _)| limit),
        tokens,
        micros,
        ctx.now(),
    )
    .await?;
    Ok(Some(Taken {
        store: limits.store.clone(),
        day,
        windows,
        cost_limit: limit.map(|(limit, _)| limit),
        per_request: limits.per_request,
        tokens,
        micros,
        prompt: format!("{} {model}", site.prompt),
        price,
    }))
}

/// The tokens a repair's correction is counted at, beside the answer it
/// follows: its fixed words and a path and rule of usual length. A longer
/// one takes the rest before it is sent ([`top_up`]).
pub(crate) const CORRECTION_TOKENS: u64 = 64;

/// The most attempt number `attempt` of a call whose prompt is `input`
/// estimated tokens sends: the prompt, and each earlier answer, at most
/// `max_output` tokens, with the correction asking again (chunk 7.9c).
pub(crate) fn sent_most(input: u64, max_output: u32, attempt: u32) -> u64 {
    let asked_again = u64::from(max_output).saturating_add(CORRECTION_TOKENS);
    let earlier = u64::from(attempt.max(1) - 1);
    input.saturating_add(asked_again.saturating_mul(earlier))
}

/// A call's most possible use: what each of its `attempts:` sends, the
/// conversation growing with each repair, and `max_output:` for each
/// answer. A send again after a provider's error takes what it sends
/// before it goes ([`top_up`]).
pub(crate) fn most(input: u64, site: Asked<'_>) -> Used {
    let attempts = site.attempts.max(1);
    Used {
        input: (1..=attempts).fold(0, |sum: u64, attempt| {
            sum.saturating_add(sent_most(input, site.max_output, attempt))
        }),
        output: u64::from(site.max_output).saturating_mul(u64::from(attempts)),
        cached: 0,
    }
}

/// Takes `tokens` more input from every limit `taken` took from, before a
/// send it didn't count is made: a send again after the provider couldn't
/// answer, or a repair longer than counted (chunk 7.9c). Whether the
/// limits cover it; a store that can't be reached doesn't. With no limits
/// to keep, it always does.
pub(crate) async fn top_up(ctx: &Ctx<'_>, taken: &mut Option<Taken>, tokens: u64) -> bool {
    let Some(taken) = taken.as_mut() else {
        return true;
    };
    if tokens == 0 {
        return true;
    }
    let more = Used {
        input: tokens,
        ..Used::default()
    };
    let micros = taken.price.as_ref().map_or(0, |price| cost_of(more, price));
    if let Some(per_request) = taken.per_request
        && ctx
            .ai_spent()
            .saturating_add(taken.tokens)
            .saturating_add(tokens)
            > per_request
    {
        return false;
    }
    let Some(database) = ctx.host().database(&taken.store) else {
        return false;
    };
    if reserve(
        database,
        taken.day,
        &taken.windows,
        taken.cost_limit,
        tokens,
        micros,
        ctx.now(),
    )
    .await
    .is_err()
    {
        return false;
    }
    taken.tokens = taken.tokens.saturating_add(tokens);
    taken.micros = taken.micros.saturating_add(micros);
    true
}

/// What a send again is counted with: what its call took from the
/// limits, what it used so far, and the input each send of it sends.
pub(crate) struct Resend<'m> {
    pub taken: &'m mut Option<Taken>,
    pub used: &'m mut Used,
    pub input: u64,
}

fn clamp(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// One atomic reservation, shared by the initial call, repair and retry.
async fn reserve(
    database: &dyn Database,
    day: i32,
    windows: &[(&'static str, String, u64)],
    cost_limit: Option<u64>,
    tokens: u64,
    micros: u64,
    now: i64,
) -> Result<(), Refused> {
    // Also protects a first row on a top-up if housekeeping removed an
    // old day: ON CONFLICT's predicate is not run for a new INSERT.
    if windows.iter().any(|(_, _, limit)| tokens > *limit)
        || cost_limit.is_some_and(|limit| micros > limit)
    {
        return Err(Refused::OverBudget);
    }
    let Some((_, _, app_limit)) = windows.first() else {
        return Err(Refused::Unavailable);
    };
    let caller = windows.iter().find(|(scope, _, _)| *scope == CALLER);
    let params = [
        SqlValue::Date(day),
        SqlValue::Text(APP.into()),
        SqlValue::Text(String::new()),
        SqlValue::Int8(clamp(tokens)),
        SqlValue::Int8(clamp(micros)),
        SqlValue::Int8(clamp(*app_limit)),
        cost_limit.map_or(SqlValue::Null(SqlType::Int8), |limit| {
            SqlValue::Int8(clamp(limit))
        }),
        crate::outbox::micros(now),
        caller.map_or(SqlValue::Null(SqlType::Text), |(_, key, _)| {
            SqlValue::Text(key.clone())
        }),
        caller.map_or(SqlValue::Null(SqlType::Int8), |(_, _, limit)| {
            SqlValue::Int8(clamp(*limit))
        }),
    ];
    let (statement, params) = if caller.is_some() {
        (TAKE, params.as_slice())
    } else {
        (TAKE_APP, &params[..8])
    };
    match database.query_once(statement, params).await {
        Ok(rows)
            if rows.len() == windows.len()
                && rows.iter().all(|row| row.as_slice() == [SqlValue::Int4(1)]) =>
        {
            Ok(())
        }
        Ok(rows) if caller.is_none() && rows.is_empty() => Err(Refused::OverBudget),
        Err(crate::StoreError::Conflict { constraint })
            if caller.is_some() && constraint == "jadpo_ai_usage_pkey" =>
        {
            Err(Refused::OverBudget)
        }
        // Empty multiwindow or malformed results cannot prove success.
        // Only the multiwindow statement deliberately raises the private conflict.
        _ => Err(Refused::Unavailable),
    }
}

/// Charges a call what it `used`, giving back what it took, and counts it
/// against its request. A count that can't be written leaves what was
/// taken taken.
pub(crate) async fn debit(ctx: &mut Ctx<'_>, taken: Option<Taken>, used: Used) {
    ctx.add_ai_spent(used.tokens());
    let Some(taken) = taken else {
        return;
    };
    let Some(database) = ctx.host().database(&taken.store) else {
        return;
    };
    let micros = taken.price.as_ref().map_or(0, |price| cost_of(used, price));
    let currency = match &taken.price {
        Some(price) => SqlValue::Text(price.currency.clone()),
        None => SqlValue::Null(SqlType::Text),
    };
    let caller = taken.windows.iter().find(|(scope, _, _)| *scope == CALLER);
    let _ = database
        .query_once(
            DEBIT,
            &[
                SqlValue::Date(taken.day),
                SqlValue::Text(APP.into()),
                SqlValue::Text(String::new()),
                SqlValue::Int8(clamp(used.input)),
                SqlValue::Int8(clamp(used.output)),
                SqlValue::Int8(clamp(used.cached)),
                SqlValue::Int8(clamp(taken.tokens)),
                SqlValue::Int8(clamp(micros)),
                SqlValue::Int8(clamp(taken.micros)),
                currency,
                crate::outbox::micros(ctx.now()),
                caller.map_or(SqlValue::Null(SqlType::Text), |(_, key, _)| {
                    SqlValue::Text(key.clone())
                }),
                SqlValue::Text(taken.prompt),
            ],
        )
        .await;
}

/// [`debit`], then `done`: what a call that ends early gives.
pub(crate) async fn debited<T>(ctx: &mut Ctx<'_>, taken: Option<Taken>, used: Used, done: T) -> T {
    debit(ctx, taken, used).await;
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        Clock, Env, Host, Isolation, JsonCodecs, LocalFuture, Places, StoreError, Transaction,
    };
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Metered {
        calls: RefCell<Vec<(String, Vec<SqlValue>)>>,
        answers: RefCell<VecDeque<Result<Vec<crate::Row>, StoreError>>>,
        missing: Cell<bool>,
    }
    impl Database for Metered {
        fn begin_at(
            &self,
            _: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            Box::pin(async { Err(StoreError::Unavailable) })
        }
        fn query_once<'a>(
            &'a self,
            sql: &'a str,
            params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<crate::Row>, StoreError>> {
            self.calls.borrow_mut().push((sql.into(), params.to_vec()));
            let answer = self.answers.borrow_mut().pop_front().unwrap_or_else(|| {
                Ok(vec![
                    vec![SqlValue::Int4(1)];
                    if params
                        .get(8)
                        .is_some_and(|v| matches!(v, SqlValue::Text(_)))
                    {
                        2
                    } else {
                        1
                    }
                ])
            });
            Box::pin(async { answer })
        }
    }
    impl Host for Metered {
        fn random(&self, bytes: &mut [u8]) -> bool {
            bytes.fill(7);
            true
        }
        fn database(&self, _: &str) -> Option<&dyn Database> {
            (!self.missing.get()).then_some(self)
        }
    }
    fn plan() -> jadpo_std::ai::AiPlan {
        jadpo_std::ai::AiPlan {
            connection: "local".into(),
            driver: AiDriver::OpenAiCompatible(jadpo_std::ai::AiUrl::Written(
                "http://localhost/v1".into(),
            )),
            models: vec![],
            schema: jadpo_std::ai::SchemaMode::StrictSubset,
            limits: Some(jadpo_std::ai::AiLimits {
                store: "main".into(),
                per_request: Some(1000),
                per_day: 1000,
                per_caller_per_day: Some(500),
                cost: None,
            }),
            prices: vec![],
        }
    }
    fn ai(plan: &jadpo_std::ai::AiPlan) -> Ai<'_> {
        static PROMPTS: jadpo_std::prompt::PromptPlan =
            jadpo_std::prompt::PromptPlan { entries: vec![] };
        Ai {
            plan,
            prompts: &PROMPTS,
            json: JsonCodecs::NONE,
            providers: None,
        }
    }
    const ASKED: Asked<'static> = Asked {
        prompt: "answer",
        model: "default",
        max_output: 20,
        attempts: 1,
    };

    #[test]
    fn reserve_topup_and_debit_each_send_every_window_in_one_statement() {
        let host = Metered::default();
        let places = Places::new();
        let mut ctx = Ctx::new(
            &places,
            &host,
            Env {
                seed: Some(1),
                clock: Clock::TEST,
            },
        );
        ctx.set_client(Some("2001:db8::1234".into()));
        let plan = plan();
        let ai = ai(&plan);
        let mut taken = crate::block_on(take(&mut ctx, &ai, ASKED, "local/model", 30, &[]))
            .unwrap()
            .unwrap();
        assert_eq!(
            host.calls.borrow().len(),
            1,
            "APP and CALLER reserved together"
        );
        assert_eq!(taken.as_ref().unwrap().windows.len(), 2);
        assert!(crate::block_on(top_up(&ctx, &mut taken, 15)).unwrap());
        assert_eq!(host.calls.borrow().len(), 2, "one top-up for both windows");
        crate::block_on(debit(
            &mut ctx,
            taken,
            Used {
                input: 31,
                output: 7,
                cached: 9,
            },
        ))
        .unwrap();
        let calls = host.calls.borrow();
        assert_eq!(calls.len(), 3, "one charge for APP, CALLER and PROMPT");
        assert_eq!(calls[0].0, TAKE);
        assert_eq!(calls[1].0, TAKE);
        assert_eq!(calls[2].0, DEBIT);
        assert_eq!(calls[2].1[6], SqlValue::Int8(65));
        assert_eq!(ctx.ai_spent(), 38);
        assert_eq!(calls[2].1[12], SqlValue::Text("answer local/model".into()));
        let SqlValue::Text(caller) = &calls[0].1[8] else {
            panic!("caller digest");
        };
        assert_eq!(caller.len(), 32);
        assert!(!caller.contains(':'));
        assert_eq!(calls[2].1[11], SqlValue::Text(caller.clone()));
    }

    #[test]
    fn refused_and_failed_topups_leave_taken_exactly_held_without_compensation() {
        for error in [
            StoreError::Conflict {
                constraint: "jadpo_ai_usage_pkey".into(),
            },
            StoreError::Conflict {
                constraint: "another_key".into(),
            },
            StoreError::Statement("injected".into()),
            StoreError::Unavailable,
        ] {
            let host = Metered::default();
            let places = Places::new();
            let mut ctx = Ctx::new(
                &places,
                &host,
                Env {
                    seed: Some(1),
                    clock: Clock::TEST,
                },
            );
            ctx.set_client(Some("192.0.2.1".into()));
            let plan = plan();
            let ai = ai(&plan);
            let mut taken = crate::block_on(take(&mut ctx, &ai, ASKED, "local/model", 30, &[]))
                .unwrap()
                .unwrap();
            host.calls.borrow_mut().clear();
            host.answers.borrow_mut().push_back(Err(error));
            assert!(!crate::block_on(top_up(&ctx, &mut taken, 15)).unwrap());
            let held = taken.as_ref().unwrap();
            assert_eq!((held.tokens, held.micros), (50, 0));
            assert_eq!(host.calls.borrow().len(), 1, "no compensating statement");
            host.missing.set(true);
            assert!(!crate::block_on(top_up(&ctx, &mut taken, 15)).unwrap());
            crate::block_on(debit(
                &mut ctx,
                taken,
                Used {
                    input: 4,
                    ..Used::default()
                },
            ))
            .unwrap();
            assert_eq!(
                ctx.ai_spent(),
                4,
                "request usage still counted when store disappears"
            );
            assert_eq!(host.calls.borrow().len(), 1);
        }
    }

    #[test]
    fn app_only_take_and_topup_use_the_registered_single_window_statement() {
        let host = Metered::default();
        let places = Places::new();
        let mut ctx = Ctx::new(
            &places,
            &host,
            Env {
                seed: Some(1),
                clock: Clock::TEST,
            },
        );
        let plan = plan();
        let ai = ai(&plan);
        let mut taken = crate::block_on(take(&mut ctx, &ai, ASKED, "local/model", 30, &[]))
            .unwrap()
            .unwrap();
        assert!(crate::block_on(top_up(&ctx, &mut taken, 15)).unwrap());
        let calls = host.calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0].1.len(),
            8,
            "APP-only reservation needs only its own eight parameters"
        );
        let registered = crate::statements::system_statements()
            .into_iter()
            .find(|(name, _)| *name == "ai_budget::TAKE_APP")
            .expect("APP-only statement registered")
            .1;
        assert_eq!(calls[0].0, registered);
        assert_eq!(calls[1].0, registered);
        assert_eq!(taken.as_ref().unwrap().tokens, 65);
    }

    #[test]
    fn app_only_empty_is_refusal_but_malformed_or_any_store_fault_is_unavailable() {
        for (answer, expected) in [
            (Ok(vec![]), Refused::OverBudget),
            (Ok(vec![vec![SqlValue::Int4(0)]]), Refused::Unavailable),
            (Ok(vec![vec![SqlValue::Int8(1)]]), Refused::Unavailable),
            (Ok(vec![vec![SqlValue::Int4(1)]; 2]), Refused::Unavailable),
            (
                Err(StoreError::Conflict {
                    constraint: "jadpo_ai_usage_pkey".into(),
                }),
                Refused::Unavailable,
            ),
            (
                Err(StoreError::Conflict {
                    constraint: "other".into(),
                }),
                Refused::Unavailable,
            ),
            (Err(StoreError::ConcurrentChange), Refused::Unavailable),
            (
                Err(StoreError::Statement("injected".into())),
                Refused::Unavailable,
            ),
            (Err(StoreError::Unavailable), Refused::Unavailable),
        ] {
            let host = Metered::default();
            let places = Places::new();
            host.answers.borrow_mut().push_back(answer.clone());
            let mut ctx = Ctx::new(
                &places,
                &host,
                Env {
                    seed: Some(1),
                    clock: Clock::TEST,
                },
            );
            let plan = plan();
            let ai = ai(&plan);
            assert_eq!(
                crate::block_on(take(&mut ctx, &ai, ASKED, "local/model", 30, &[]))
                    .unwrap()
                    .unwrap_err(),
                expected
            );
            assert_eq!(ctx.ai_spent(), 0);
            assert_eq!(host.calls.borrow().len(), 1);
            let mut taken = crate::block_on(take(&mut ctx, &ai, ASKED, "local/model", 30, &[]))
                .unwrap()
                .unwrap();
            host.calls.borrow_mut().clear();
            host.answers.borrow_mut().push_back(answer);
            assert!(!crate::block_on(top_up(&ctx, &mut taken, 15)).unwrap());
            assert_eq!(
                (
                    taken.as_ref().unwrap().tokens,
                    taken.as_ref().unwrap().micros
                ),
                (50, 0)
            );
            assert_eq!(host.calls.borrow().len(), 1);
        }
    }

    #[test]
    fn app_only_first_row_and_topup_precheck_tokens_and_cost_without_sql() {
        let host = Metered::default();
        let windows = vec![(APP, String::new(), 100)];
        for (tokens, micros) in [(101, 0), (1, 101)] {
            assert_eq!(
                crate::block_on(reserve(&host, 1, &windows, Some(100), tokens, micros, 0)).unwrap(),
                Err(Refused::OverBudget)
            );
        }
        assert!(
            host.calls.borrow().is_empty(),
            "cannot INSERT an oversized first row"
        );
    }

    #[test]
    fn only_the_private_refusal_constraint_means_over_budget() {
        for (error, expected) in [
            (
                StoreError::Conflict {
                    constraint: "jadpo_ai_usage_pkey".into(),
                },
                Refused::OverBudget,
            ),
            (
                StoreError::Conflict {
                    constraint: "other".into(),
                },
                Refused::Unavailable,
            ),
            (StoreError::ConcurrentChange, Refused::Unavailable),
            (
                StoreError::Statement("injected".into()),
                Refused::Unavailable,
            ),
        ] {
            let host = Metered::default();
            let places = Places::new();
            host.answers.borrow_mut().push_back(Err(error));
            let mut ctx = Ctx::new(
                &places,
                &host,
                Env {
                    seed: Some(1),
                    clock: Clock::TEST,
                },
            );
            ctx.set_client(Some("192.0.2.1".into()));
            let plan = plan();
            let ai = ai(&plan);
            let answer =
                crate::block_on(take(&mut ctx, &ai, ASKED, "local/model", 30, &[])).unwrap();
            assert_eq!(answer.unwrap_err(), expected);
            assert_eq!(host.calls.borrow().len(), 1);
            assert_eq!(ctx.ai_spent(), 0);
        }
    }

    #[test]
    fn a_reservation_requires_every_expected_success_row() {
        for rows in [
            vec![],
            vec![vec![SqlValue::Int4(0)]],
            vec![vec![SqlValue::Int4(1)]; 3],
        ] {
            let host = Metered::default();
            let places = Places::new();
            host.answers.borrow_mut().push_back(Ok(rows));
            let mut ctx = Ctx::new(
                &places,
                &host,
                Env {
                    seed: Some(1),
                    clock: Clock::TEST,
                },
            );
            ctx.set_client(Some("192.0.2.1".into()));
            let plan = plan();
            let ai = ai(&plan);
            assert_eq!(
                crate::block_on(take(&mut ctx, &ai, ASKED, "local/model", 30, &[]))
                    .unwrap()
                    .unwrap_err(),
                Refused::Unavailable
            );
            assert_eq!(
                host.calls.borrow().len(),
                1,
                "no compensation after uncertain result"
            );
        }
    }

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
        assert_eq!(cost_of(used, &price()), 320 + 20 + 800);
        assert_eq!(cost(1, 1), 1);
        assert_eq!(cost(0, 400_000), 0);
    }

    /// A call's most possible use counts the conversation each repair
    /// sends: the prompt, and every answer before it at `max_output:` with
    /// the correction asking again (Phase 7 review, chunk 7.9c).
    #[test]
    fn each_repair_is_counted_with_the_conversation_it_sends() {
        let asked = |attempts| Asked {
            prompt: "answer_question",
            model: "default",
            max_output: 300,
            attempts,
        };
        assert_eq!(
            most(1_000, asked(1)),
            Used {
                input: 1_000,
                output: 300,
                cached: 0
            }
        );
        // The repair resends the prompt, the answer and the correction.
        let asked_again = 300 + CORRECTION_TOKENS;
        assert_eq!(most(1_000, asked(2)).input, 1_000 + 1_000 + asked_again);
        assert_eq!(most(1_000, asked(2)).output, 600);
        assert_eq!(
            most(1_000, asked(3)).input,
            1_000 + (1_000 + asked_again) + (1_000 + 2 * asked_again)
        );
        // The most an attempt sends, which a repair is topped up past.
        assert_eq!(sent_most(1_000, 300, 1), 1_000);
        assert_eq!(sent_most(1_000, 300, 3), 1_000 + 2 * asked_again);
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
