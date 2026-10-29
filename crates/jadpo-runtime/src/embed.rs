//! The `ai` capability's embeddings (area 21, "The `ai` capability" and
//! "Embeddings and search"; chunk 5.5v4): what both executors run at an
//! `ai.embed(text, as: V)` or `ai.embed_all(texts, as: V)` call's site.
//!
//! [`embed`] asks the model the call's `model:` names, through the
//! connection `ai:` names, for each text's embedding, on the one
//! outside-call path ([`crate::outside`]) as `ai.generate` does: refused
//! in an open transaction, logged without its values, recorded and
//! replayed with the other effects, held to its connection's
//! `max_concurrent:`, and its answer kept by its step and request for a
//! rerun. Each vector is read through `V`'s codec, `json.read`'s
//! decoding, so one of another length, or with no direction where `V`
//! compares by cosine, never becomes a `V`.
//!
//! - **The built-in driver** sends OpenAI's embeddings request, `POST
//!   <url>/embeddings` with `model` and the texts as `input`, and reads
//!   `data[i].embedding` in `index` order. It doesn't send `dimensions`: a
//!   local model makes vectors of its own length, which `V`'s codec checks.
//! - **Its three failures**: `timed_out` (no answer within `timeout:`),
//!   `too_long` (a text longer than the model takes, as OpenAI answers one,
//!   or more than [`MOST_TEXTS`] texts, sent nowhere) and `over_budget`
//!   (the limits chunk 5.5v5 builds, and a test's script). A vector that
//!   isn't a `V` is `misconfigured`: the model isn't the one `ai:` says.
//! - **The fake** answers a call nothing scripted with vectors made from
//!   each text's SHA-256 and the test's seed ([`seeded`]), so the same text
//!   always gives the same vector, unit length so no cosine is undefined.

use std::rc::Rc;

use jadpo_std::HttpMethod;
use jadpo_std::RuleKey;
use jadpo_std::ai::{AiDriver, AiUrl};
use jadpo_std::embed::{EmbedSite, MOST_TEXTS, OPERATION};
use jadpo_std::subscriber::Repeat;
use jadpo_value::Value;

use crate::Ctx;
use crate::FaultKind;
use crate::ai::{Ai, CONTEXT_LENGTH_EXCEEDED, Done, Kept};
use crate::capability::{Answer, Call, Source};
use crate::outside::{Asking, Comes, Form, Rerun, Retry, Sending};
use crate::provider::{Reading, Request};

/// The settings a call reads, by number, for its model and its address.
pub fn settings_read(ai: &Ai<'_>, site: &EmbedSite) -> Vec<u32> {
    let mut read: Vec<u32> = ai.plan.model(&site.model).into_iter().collect();
    // The day's cost limit, a `Money` setting (chunk 5.5v5).
    read.extend(crate::ai_budget::settings_read(ai.plan));
    match &ai.plan.driver {
        AiDriver::OpenAiCompatible(AiUrl::Setting(at)) => read.push(*at),
        AiDriver::OpenAiCompatible(AiUrl::Written(_)) => {}
        AiDriver::Package(package) => {
            if let Some(embed) = &package.embed {
                read.extend(crate::ai::package_reads(ai, embed.called, &embed.input));
            }
        }
    }
    read.sort_unstable();
    read.dedup();
    read
}

/// The texts `given` holds: one for `ai.embed`, a list for `embed_all`.
fn texts_of(given: &Value, all: bool) -> Option<Vec<String>> {
    let text = |value: &Value| match value {
        Value::Text(text) => Some(text.to_string()),
        other => match jadpo_value::standard::from(other) {
            Ok(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        },
    };
    if !all {
        return text(given).map(|one| vec![one]);
    }
    match given {
        Value::List(items) => items.iter().map(text).collect(),
        _ => None,
    }
}

/// A condition, for the call's `attempt` to map.
fn broke(rule: RuleKey) -> Done {
    Done::Broke {
        rule,
        problem: None,
    }
}

/// Asks the model for the embedding of each text `given` holds, as
/// `calling.site`'s vector type: what both executors run at an `ai.embed`
/// or `ai.embed_all` call's site.
pub async fn embed(
    ctx: &mut Ctx<'_>,
    calling: crate::ai::Calling<'_, EmbedSite>,
    given: Value,
) -> Done {
    let started = ctx.started();
    let (ai, settings) = match calling.ai(
        ctx,
        &started,
        |ai| settings_read(ai, calling.site),
        "an embedding asked in a program that names no model",
    ) {
        Ok(made) => made,
        Err(done) => return done,
    };
    embedded(ctx, &ai, calling.site, given, &settings).await
}

/// [`embed`], with `ai` and the values `settings` gives the settings
/// [`settings_read`] names.
async fn embedded(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    site: &EmbedSite,
    given: Value,
    settings: &[(u32, Value)],
) -> Done {
    let Some(texts) = texts_of(&given, site.all) else {
        return Done::Fault(FaultKind::Defect(
            "an embedding asked of what isn't text".into(),
        ));
    };
    // Nothing to embed is nothing to ask.
    if texts.is_empty() {
        return Done::Value(Value::list(Vec::new()));
    }
    // More texts than one call takes is sent nowhere.
    if texts.len() > MOST_TEXTS {
        return broke(RuleKey::TooLong);
    }
    let setting = |number: u32| {
        settings
            .iter()
            .find(|(at, _)| *at == number)
            .map(|(_, value)| value)
    };
    let model = ai
        .plan
        .model(&site.model)
        .and_then(setting)
        .and_then(|value| match jadpo_value::standard::from(value) {
            Ok(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        });
    let made = Call::new(&ai.plan.connection, OPERATION, given, Repeat::Safe);
    let refused = |ctx: &mut Ctx<'_>| {
        let now = ctx.now();
        ctx.capabilities_mut().refused(&made, now);
        Done::Failed(Box::new(crate::misconfigured()))
    };
    let Some(model) = model else {
        return refused(ctx);
    };
    // Through the built-in driver's `<url>/embeddings`, or a package's
    // `embed:` (chunk 5.5v7).
    let mut package = None;
    let request = match &ai.plan.driver {
        AiDriver::OpenAiCompatible(url) => {
            let base = match url {
                AiUrl::Written(text) => Some(text.clone()),
                AiUrl::Setting(at) => match setting(*at) {
                    Some(Value::Text(text)) => Some(text.to_string()),
                    _ => None,
                },
            };
            let Some(base) = base.and_then(|base| crate::provider::sendable(&base)) else {
                return refused(ctx);
            };
            Request {
                method: HttpMethod::Post,
                url: format!("{}/embeddings", base.trim_end_matches('/')),
                headers: vec![
                    ("Accept".to_string(), "application/json".to_string()),
                    ("Content-Type".to_string(), "application/json".to_string()),
                    ("user-agent".to_string(), "jadpo".to_string()),
                ],
                body: Some(request_body(&model, &texts)),
                timeout_ms: site.timeout_ms,
                reading: Reading {
                    error_code: Some("error.code".to_string()),
                    ..Reading::default()
                },
                // Set when the call takes it.
                repeatable: false,
            }
        }
        AiDriver::Package(filled) => {
            let (Some(embed), Some(providers)) = (&filled.embed, ai.providers) else {
                return Done::Fault(FaultKind::Defect(
                    "an embedding through a package that maps no `embed:`".into(),
                ));
            };
            let name = crate::ai::model_name(&model);
            // Checked before sending, from the package's table: a text
            // longer than the model takes costs nothing (area 21).
            if let Some(context) = filled.context(name)
                && texts
                    .iter()
                    .any(|text| jadpo_std::ai::estimated_tokens(text) > context)
            {
                return broke(RuleKey::TooLong);
            }
            let given = crate::ai::Given {
                model: name,
                max_output: 0,
                messages: &[],
                instructions_apart: false,
                texts: &texts,
                dimensions: site.dimensions,
            };
            let Some(json) = crate::ai::mapped_input(&embed.input, &given, settings) else {
                return refused(ctx);
            };
            // Resolved from its connection's source before the request
            // is made (chunk 10.3f).
            let credential = match crate::provider::credential_of(&providers, embed.called) {
                Ok(credential) => credential,
                Err(crate::provider::Unready::Misconfigured) => return refused(ctx),
                Err(crate::provider::Unready::Fault(kind)) => return Done::Fault(kind),
            };
            match crate::ai::package_request(
                ctx,
                &providers,
                embed.called,
                &json,
                settings,
                credential.as_ref(),
                None,
                OPERATION,
            ) {
                Ok(request) => {
                    package = Some((embed, providers));
                    Request {
                        timeout_ms: site.timeout_ms,
                        ..request
                    }
                }
                Err(crate::provider::Unready::Misconfigured) => return refused(ctx),
                Err(crate::provider::Unready::Fault(kind)) => return Done::Fault(kind),
            }
        }
    };
    let body = request.body.clone().unwrap_or_default();
    let digest = jadpo_value::hash::sha256(body.as_bytes());
    // Given back on a rerun: from the record of the action's steps, or
    // from the first run's answers kept by the invocation.
    let name = format!("{}.{OPERATION}", ai.plan.connection);
    let comes = crate::outside::comes_from(
        ctx,
        Asking {
            step: Some(crate::steps::Proposed {
                operation: &name,
                fields: vec![("request".to_string(), body.clone().into_bytes())],
                repeat: Repeat::Safe,
                changes: false,
                window_ms: None,
                did_nothing: &[],
            }),
            rerun: Some(Rerun::Answer(digest)),
        },
    )
    .await;
    let place = match comes {
        Ok(Comes::Send(place)) => place,
        Ok(Comes::Recorded { answered, at }) => {
            let kept = match Kept::recorded(answered) {
                Ok(kept) => kept,
                Err(what) => return Done::Fault(FaultKind::Defect(what.into())),
            };
            // Logged as the step it is (chunk 7.5f).
            let mut made = made;
            made.step = at;
            return replayed(ctx, ai, site, texts.len(), made, kept);
        }
        Ok(Comes::Reused(kept)) => return replayed(ctx, ai, site, texts.len(), made, kept),
        Ok(Comes::Opened(_)) => {
            return Done::Fault(FaultKind::Defect(
                "an embedding given back as a stream".into(),
            ));
        }
        Err(refused) => return crate::ai::unanswered(refused),
    };
    // What the call may use, taken from every limit before it is sent:
    // its texts, since an embedding answers with no tokens of its own
    // (area 21, "Budgets"; chunk 5.5v5).
    let input: u64 = texts
        .iter()
        .map(|text| jadpo_std::ai::estimated_tokens(text))
        .sum();
    let asked = crate::ai_budget::Asked {
        prompt: OPERATION,
        model: &site.model,
        max_output: 0,
        attempts: 1,
    };
    let mut taken = match crate::ai_budget::take(ctx, ai, asked, &model, input, settings).await {
        Ok(taken) => taken,
        Err(crate::ai_budget::Refused::OverBudget) => {
            let now = ctx.now();
            ctx.capabilities_mut().refused(&made, now);
            return broke(RuleKey::OverBudget);
        }
        Err(crate::ai_budget::Refused::Unavailable) => {
            return Done::Failed(Box::new(crate::unavailable()));
        }
        Err(crate::ai_budget::Refused::Misconfigured) => {
            return Done::Failed(Box::new(crate::misconfigured()));
        }
    };
    let began = ctx.host().monotonic_ms();
    let mut call = made.clone();
    call.step = place.step;
    // The sends a provider's error asked for again, each charged what it
    // sent (chunk 7.9c).
    let mut sent_again = crate::ai_budget::Used::default();
    let logged: crate::outside::Log<'_> =
        &|ctx, call, answer| crate::ai::log(ctx, call, None, answer);
    let sending = Sending {
        retry: Retry::timed(began, site.timeout_ms),
        form: Form::Whole,
        max_concurrent: crate::ai::max_concurrent_of(ai),
        resend: Some(crate::ai_budget::Resend {
            taken: &mut taken,
            used: &mut sent_again,
            input,
        }),
        log: Some(logged),
    };
    let sent = crate::outside::send(ctx, &call.with_request(request), sending)
        .await
        .map(|sent| {
            let source = sent.source;
            (sent.whole(), source)
        })
        .map_err(crate::ai::unanswered);
    let texts_used = |tokens: u64| {
        let mut used = texts_used(tokens);
        used.add(sent_again);
        used
    };
    let (answer, source) = match sent {
        Ok(sent) => sent,
        Err(done) => {
            return crate::ai_budget::debited(ctx, taken, texts_used(input), done).await;
        }
    };
    // A fake's own: the vectors made from each text and the seed.
    let answer = match answer {
        #[cfg(feature = "testing")]
        Answer::Generated => {
            let seed = ctx.env().seed.unwrap_or_default();
            Answer::Wire(fake_answer(&texts, seed, site.dimensions))
        }
        other => other,
    };
    // A package's own answer, read where its `embed:` says and kept in the
    // built-in driver's form; a fake answers in that form already.
    let answer = match (answer, package) {
        (Answer::Wire(text), Some((embed, providers)))
            if matches!(source, Source::Live | Source::Sandbox) =>
        {
            match package_vectors(ctx, &providers, &ai.plan.connection, embed, &text) {
                Ok(vectors) => Answer::Wire(vectors),
                Err(done) => {
                    return crate::ai_budget::debited(ctx, taken, texts_used(input), done).await;
                }
            }
        }
        (answer, _) => answer,
    };
    // Charged what the answer says it used, or what was sent.
    let used = match &answer {
        Answer::Wire(text) => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|json| json.get("usage")?.get("prompt_tokens")?.as_u64())
            .map_or_else(|| texts_used(input), texts_used),
        _ => texts_used(input),
    };
    crate::ai_budget::debit(ctx, taken, used).await;
    let kept = match &answer {
        Answer::Wire(text) => Kept::Answer(text.clone()),
        Answer::TimedOut => Kept::Condition(RuleKey::TimedOut),
        Answer::Failed(code) if code == CONTEXT_LENGTH_EXCEEDED => {
            Kept::Condition(RuleKey::TooLong)
        }
        Answer::Failed(code) if code == RuleKey::OverBudget.word() => {
            Kept::Condition(RuleKey::OverBudget)
        }
        Answer::Failed(_) | Answer::Misconfigured => {
            return Done::Failed(Box::new(crate::misconfigured()));
        }
        Answer::Unavailable | Answer::RetryAfter(_) | Answer::Unknown => {
            return Done::Failed(Box::new(crate::unavailable()));
        }
        Answer::Value(_) => {
            return Done::Fault(FaultKind::Defect(
                "an answer to `ai.embed` that isn't an embeddings answer".into(),
            ));
        }
        #[cfg(feature = "testing")]
        Answer::Generated | Answer::Recorded => {
            return Done::Fault(FaultKind::Defect(
                "an answer to `ai.embed` that isn't an embeddings answer".into(),
            ));
        }
    };
    let done = read(ai, site, texts.len(), &kept);
    if let Err(refused) = crate::outside::keep(ctx, place, kept).await {
        return crate::ai::unanswered(refused);
    }
    done
}

/// An embedding's use: `input` tokens, and none answered.
fn texts_used(input: u64) -> crate::ai_budget::Used {
    crate::ai_budget::Used {
        input,
        output: 0,
        cached: 0,
    }
}

/// What the call gives when its answer is given back without asking
/// again: logged as replayed, then read as any answer is.
fn replayed(
    ctx: &mut Ctx<'_>,
    ai: &Ai<'_>,
    site: &EmbedSite,
    count: usize,
    made: Call,
    kept: Kept,
) -> Done {
    crate::outside::given_back(ctx, &made, &kept.logged());
    read(ai, site, count, &kept)
}

/// What an answer kept gives: each of its `count` vectors read as `V`,
/// one or a list; or the condition it ended in.
fn read(ai: &Ai<'_>, site: &EmbedSite, count: usize, kept: &Kept) -> Done {
    let text = match kept {
        Kept::Answer(text) => text,
        Kept::Condition(rule) => return broke(*rule),
        Kept::Malformed(_) => return broke(RuleKey::Malformed),
    };
    let Some(vectors) = vectors_of(text).filter(|vectors| vectors.len() == count) else {
        return Done::Fault(FaultKind::Provider(format!(
            "`{}`'s answer to `{OPERATION}` isn't an embedding of each text",
            ai.plan.connection
        )));
    };
    let Some(codec) = ai.json.codec_of(site.vector) else {
        return Done::Fault(FaultKind::Defect(
            "an embedding of a type with no codec".into(),
        ));
    };
    let mut read = Vec::with_capacity(vectors.len());
    for vector in vectors {
        match ai.json.codecs.read_value(codec, &Value::text(&vector)) {
            Ok(value) => read.push(value),
            // Another length, or no direction: not the model `ai:` says.
            Err(_) => return Done::Failed(Box::new(crate::misconfigured())),
        }
    }
    if site.all {
        Done::Value(Value::list(read))
    } else {
        read.pop().map_or_else(
            || Done::Fault(FaultKind::Defect("an embedding of nothing".into())),
            Done::Value,
        )
    }
}

/// A package's answer to its `embed:` operation (chunk 5.5v7), decoded by
/// its types and read on the answer as sent where the mapping says, as
/// the built-in driver's form keeps it; a fault naming the connection
/// when it isn't one, never its values.
fn package_vectors(
    ctx: &mut Ctx<'_>,
    providers: &crate::provider::Providers<'_>,
    connection: &str,
    embed: &jadpo_std::ai::PackageEmbed,
    text: &str,
) -> Result<Rc<str>, Done> {
    let Some((_, op, _)) = providers.numbered(embed.called) else {
        return Err(Done::Fault(FaultKind::Defect(format!(
            "`{connection}.{}`, which the plan doesn't have",
            embed.operation
        ))));
    };
    let call = jadpo_std::provider::CallSite::default();
    let name = format!("{connection}.{}", embed.operation);
    let output = match crate::provider::decode(
        ctx,
        providers,
        op,
        &name,
        text,
        crate::provider::Site {
            call: &call,
            functions: &crate::ai::NoFunctions,
            at: 0,
        },
    ) {
        crate::provider::Done::Value(value) => value,
        crate::provider::Done::Failed(failure) => return Err(Done::Failed(failure)),
        crate::provider::Done::Fault(kind) => return Err(Done::Fault(kind)),
    };
    let not_one = || {
        Done::Fault(FaultKind::Provider(format!(
            "`{connection}`'s answer to `{OPERATION}` isn't an embedding of each text"
        )))
    };
    let sent: serde_json::Value = serde_json::from_str(text).map_err(|_| not_one())?;
    let items = crate::provider::at_path(&sent, &embed.vectors.join("."))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(not_one)?;
    let mut data = Vec::with_capacity(items.len());
    for (at, item) in items.iter().enumerate() {
        let vector = crate::provider::at_path(item, &embed.vector.join("."))
            .filter(|vector| vector.is_array())
            .ok_or_else(not_one)?;
        let index = match &embed.index {
            Some(path) => crate::provider::at_path(item, &path.join("."))
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(not_one)?,
            None => at as u64,
        };
        data.push(serde_json::json!({"object": "embedding", "index": index, "embedding": vector}));
    }
    // The tokens it used, where the mapping says, as the built-in
    // driver's form keeps them (chunk 5.5v5's budgets read them).
    let mut kept = serde_json::Map::new();
    kept.insert("object".into(), "list".into());
    kept.insert("data".into(), serde_json::Value::Array(data));
    if let Some(tokens) = embed
        .usage
        .as_ref()
        .and_then(|usage| usage.input.as_ref())
        .and_then(|path| crate::ai::count_at(&output, path))
    {
        kept.insert(
            "usage".into(),
            serde_json::json!({ "prompt_tokens": tokens }),
        );
    }
    Ok(Rc::from(serde_json::Value::Object(kept).to_string()))
}

/// OpenAI's embeddings request's JSON body.
fn request_body(model: &str, texts: &[String]) -> String {
    let mut body = serde_json::Map::new();
    body.insert("model".into(), model.into());
    body.insert(
        "input".into(),
        serde_json::Value::Array(texts.iter().map(|text| text.as_str().into()).collect()),
    );
    serde_json::Value::Object(body).to_string()
}

/// Each vector an embeddings answer holds, as its JSON text, in its
/// texts' order (`data[i].index`); `None` for anything else.
fn vectors_of(text: &str) -> Option<Vec<String>> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut data: Vec<(u64, String)> = json
        .get("data")?
        .as_array()?
        .iter()
        .enumerate()
        .map(|(at, item)| {
            let index = item
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(at as u64);
            let vector = item.get("embedding")?;
            vector.is_array().then(|| (index, vector.to_string()))
        })
        .collect::<Option<_>>()?;
    data.sort_by_key(|(index, _)| *index);
    Some(data.into_iter().map(|(_, vector)| vector).collect())
}

/// An embeddings answer with `vectors`, as an OpenAI-compatible server
/// sends one: how a fake answers, in the built-in driver's own form.
pub fn embeddings_answer(vectors: &[Vec<f32>]) -> Rc<str> {
    let data: Vec<serde_json::Value> = vectors
        .iter()
        .enumerate()
        .map(|(index, vector)| {
            let items: Vec<String> = vector.iter().map(|item| format!("{item:?}")).collect();
            let embedding: serde_json::Value =
                serde_json::from_str(&format!("[{}]", items.join(","))).unwrap_or_default();
            serde_json::json!({"object": "embedding", "index": index, "embedding": embedding})
        })
        .collect();
    Rc::from(serde_json::json!({"object": "list", "data": data}).to_string())
}

/// The fake's answer: the vector [`seeded`] makes for each text.
#[cfg(feature = "testing")]
fn fake_answer(texts: &[String], seed: u64, dimensions: u64) -> Rc<str> {
    let vectors: Vec<Vec<f32>> = texts
        .iter()
        .map(|text| seeded(text, seed, dimensions))
        .collect();
    embeddings_answer(&vectors)
}

/// The fake's embedding of `text` (area 21, "Tests"): `dimensions`
/// numbers drawn from SHA-256 over the text's own SHA-256, the seed and a
/// counter, each from -1 to 1, then made unit length, so the same text
/// under the same seed always gives the same vector and its cosine with
/// another is defined.
#[cfg(feature = "testing")]
pub fn seeded(text: &str, seed: u64, dimensions: u64) -> Vec<f32> {
    let of_text = jadpo_value::hash::sha256(text.as_bytes());
    let wanted = usize::try_from(dimensions).unwrap_or(0);
    let mut items: Vec<f32> = Vec::with_capacity(wanted);
    let mut block: u32 = 0;
    while items.len() < wanted {
        let mut input = Vec::with_capacity(44);
        input.extend_from_slice(&of_text);
        input.extend_from_slice(&seed.to_le_bytes());
        input.extend_from_slice(&block.to_le_bytes());
        let digest = jadpo_value::hash::sha256(&input);
        let (chunks, _) = digest.as_chunks::<4>();
        for chunk in chunks {
            if items.len() == wanted {
                break;
            }
            let bits = u32::from_le_bytes(*chunk);
            // The top 24 bits, which a single-precision float holds
            // exactly, from -1 up to 1.
            items.push((bits >> 8) as f32 / 8_388_608.0 - 1.0);
        }
        block = block.wrapping_add(1);
    }
    let length = items.iter().map(|item| item * item).sum::<f32>().sqrt();
    if length > 0.0 && length.is_finite() {
        for item in &mut items {
            *item /= length;
        }
    } else if let Some(first) = items.first_mut() {
        *first = 1.0;
    }
    items
}

/// Gives the `ai` fake the next answer to an embedding call: one of its
/// three failures, scripted by the test at the current frame of `ctx`.
pub fn script(ctx: &mut Ctx<'_>, ai: &Ai<'_>, condition: RuleKey) -> Result<(), &'static str> {
    let Some(from) = ctx.frames().first().copied() else {
        return Err("a scripted answer outside a test");
    };
    let answer = match condition {
        RuleKey::TimedOut => Answer::TimedOut,
        RuleKey::TooLong => Answer::Failed(CONTEXT_LENGTH_EXCEEDED.to_string()),
        RuleKey::OverBudget => Answer::Failed(RuleKey::OverBudget.word().to_string()),
        _ => return Err("a scripted failure an embedding has none of"),
    };
    let scripted = ctx
        .host()
        .capability(&ai.plan.connection)
        .is_some_and(|fake| fake.script(&ai.plan.connection, OPERATION, answer, from));
    if scripted {
        Ok(())
    } else {
        Err("a test's `fakes:` on a host with no `ai` fake")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same text under the same seed gives the same unit vector; a
    /// different text or seed gives another.
    #[test]
    fn a_fake_embedding_is_the_texts_own_under_the_seed() {
        let first = seeded("Does your sourdough contain sesame?", 7, 1536);
        assert_eq!(first.len(), 1536);
        assert_eq!(
            first,
            seeded("Does your sourdough contain sesame?", 7, 1536)
        );
        assert_ne!(first, seeded("Does your rye contain sesame?", 7, 1536));
        assert_ne!(
            first,
            seeded("Does your sourdough contain sesame?", 8, 1536)
        );
        let length: f32 = first.iter().map(|item| item * item).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1e-5, "{length}");
        assert!(first.iter().all(|item| (-1.0..=1.0).contains(item)));
        assert_eq!(seeded("x", 0, 3).len(), 3);
    }

    /// An embeddings answer is read in its texts' order, whatever order
    /// its items come in; anything else isn't one.
    #[test]
    fn an_embeddings_answer_is_read_by_index() {
        let answer = r#"{"data":[{"index":1,"embedding":[0.5,1]},{"index":0,"embedding":[1,0]}]}"#;
        assert_eq!(
            vectors_of(answer),
            Some(vec!["[1,0]".to_string(), "[0.5,1]".to_string()])
        );
        assert_eq!(vectors_of(r#"{"data":[{"index":0}]}"#), None);
        assert_eq!(vectors_of("{}"), None);
        let made = embeddings_answer(&[vec![0.25, -1.0]]);
        assert_eq!(vectors_of(&made), Some(vec!["[0.25,-1.0]".to_string()]));
    }

    /// The request is OpenAI's: the model and the texts, nothing else.
    #[test]
    fn the_request_names_the_model_and_the_texts() {
        assert_eq!(
            request_body("local/nomic-embed-text", &["Rye?".to_string()]),
            r#"{"input":["Rye?"],"model":"local/nomic-embed-text"}"#
        );
    }
}
