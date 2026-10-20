//! The `ai` capability's embeddings (area 21, "The `ai` capability" and
//! "Embeddings and search"; chunk 5.5v4): what both executors run at an
//! `ai.embed(text, as: V)` or `ai.embed_all(texts, as: V)` call's site.
//!
//! [`embed`] asks the model the call's `model:` names, through the
//! connection `ai:` names, for each text's embedding, on the capability
//! seam ([`crate::effects::exchange`]) as `ai.generate` does: refused in an
//! open transaction, logged without its values, recorded and replayed
//! with the other effects, and its answer kept by its step and request
//! for a rerun. Each vector is read through `V`'s codec, `json.read`'s
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
use crate::ai::{Ai, CONTEXT_LENGTH_EXCEEDED, Done, Kept, Reused};
use crate::capability::{Answer, Call, Source};
use crate::effects::Unanswered;
use crate::provider::{Reading, Request};

/// The settings a call reads, by number, for its model and its address.
pub fn settings_read(ai: &Ai<'_>, site: &EmbedSite) -> Vec<u32> {
    let mut read: Vec<u32> = ai.plan.model(&site.model).into_iter().collect();
    // The day's cost limit, a `Money` setting (chunk 5.5v5).
    read.extend(ai.plan.limits.as_ref().and_then(|limits| limits.cost));
    if let AiDriver::OpenAiCompatible(AiUrl::Setting(at)) = &ai.plan.driver {
        read.push(*at);
    }
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
/// `site`'s vector type; `settings` gives the values of the settings
/// [`settings_read`] names.
pub async fn embed(
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
    // A package's `provide ai` maps no `embed:` yet (chunk 5.5v7), which
    // the checker refuses.
    let base = match &ai.plan.driver {
        AiDriver::OpenAiCompatible(AiUrl::Written(text)) => Some(text.clone()),
        AiDriver::OpenAiCompatible(AiUrl::Setting(at)) => match setting(*at) {
            Some(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        },
        AiDriver::Package(_) => None,
    };
    let made = Call::new(&ai.plan.connection, OPERATION, given, Repeat::Safe);
    let (Some(model), Some(base)) = (
        model,
        base.and_then(|base| crate::provider::sendable(&base)),
    ) else {
        let now = ctx.now();
        ctx.capabilities_mut().refused(&made, now);
        return Done::Failed(Box::new(crate::misconfigured()));
    };
    let url = format!("{}/embeddings", base.trim_end_matches('/'));
    let body = request_body(&model, &texts);
    let digest = jadpo_value::hash::sha256(body.as_bytes());
    // Given back on a rerun: from the record of the action's steps, or
    // from the first run's answers kept by the invocation.
    let name = format!("{}.{OPERATION}", ai.plan.connection);
    let stepped = crate::steps::step(
        ctx,
        crate::steps::Proposed {
            operation: &name,
            fields: vec![("request".to_string(), body.clone().into_bytes())],
            repeat: Repeat::Safe,
            changes: false,
            window_ms: None,
        },
    )
    .await;
    let step_at = match stepped {
        Ok(crate::steps::Stepped::Untracked) => None,
        Ok(crate::steps::Stepped::Send(at)) => Some(at),
        Ok(crate::steps::Stepped::Replay(answered)) => {
            let kept = match answered {
                crate::steps::Answered::Wire(text) => Kept::Answer(text),
                crate::steps::Answered::Failed(word) => match Kept::from_word(&word) {
                    Some(kept) => kept,
                    None => {
                        return Done::Fault(FaultKind::Defect(
                            "a recorded answer of no condition".into(),
                        ));
                    }
                },
                crate::steps::Answered::Done => {
                    return Done::Fault(FaultKind::Defect("a recorded answer with nothing".into()));
                }
            };
            return replayed(ctx, ai, site, texts.len(), made, kept);
        }
        Err(Unanswered::Failure(failure)) => return Done::Failed(failure),
        Err(Unanswered::Fault(kind)) => return Done::Fault(kind),
        Err(Unanswered::Failed(_)) => return Done::Failed(Box::new(crate::unavailable())),
    };
    let step = if step_at.is_none() {
        ctx.next_step()
    } else {
        None
    };
    if let Some(step) = step
        && let Some(kept) = ctx.reused(step, &digest)
    {
        return replayed(ctx, ai, site, texts.len(), made, kept);
    }
    // What the call may use, taken from every limit before it is sent:
    // its texts, since an embedding answers with no tokens of its own
    // (area 21, "Budgets"; chunk 5.5v5).
    let input: u64 = texts
        .iter()
        .map(|text| jadpo_std::ai::estimated_tokens(text))
        .sum();
    let asked = crate::ai_budget::Asked {
        prompt: OPERATION,
        max_output: 0,
        attempts: 1,
    };
    let taken = match crate::ai_budget::take(ctx, ai, asked, &model, input, settings).await {
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
    call.step = step_at;
    let request = Request {
        method: HttpMethod::Post,
        url: url.clone(),
        headers: vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
            ("user-agent".to_string(), "jadpo".to_string()),
        ],
        body: Some(body.clone()),
        timeout_ms: site.timeout_ms,
        reading: Reading {
            error_code: Some("error.code".to_string()),
            ..Reading::default()
        },
    };
    let (answer, _) = match crate::ai::sent(ctx, call, request, began, site.timeout_ms).await {
        Ok(sent) => sent,
        Err(done) => return crate::ai_budget::charged(ctx, taken, estimate(input), done).await,
    };
    // A fake's own: the vectors made from each text and the seed.
    let answer = match answer {
        Answer::Generated => {
            let seed = ctx.env().seed.unwrap_or_default();
            Answer::Wire(fake_answer(&texts, seed, site.dimensions))
        }
        other => other,
    };
    // Charged what the answer says it used, or what was sent.
    let used = match &answer {
        Answer::Wire(text) => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|json| json.get("usage")?.get("prompt_tokens")?.as_u64())
            .map_or_else(|| estimate(input), estimate),
        _ => estimate(input),
    };
    crate::ai_budget::charge(ctx, taken, used).await;
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
        Answer::Value(_) | Answer::Generated | Answer::Recorded => {
            return Done::Fault(FaultKind::Defect(
                "an answer to `ai.embed` that isn't an embeddings answer".into(),
            ));
        }
    };
    let done = read(ai, site, texts.len(), &kept);
    if let Some(at) = step_at {
        let answered = match &kept {
            Kept::Answer(text) => crate::steps::Answered::Wire(text.clone()),
            condition => crate::steps::Answered::Failed(condition.word().unwrap_or_default()),
        };
        match crate::steps::answered(ctx, at, answered).await {
            Ok(()) => {}
            Err(Unanswered::Failure(failure)) => return Done::Failed(failure),
            Err(Unanswered::Fault(kind)) => return Done::Fault(kind),
            Err(Unanswered::Failed(_)) => return Done::Failed(Box::new(crate::unavailable())),
        }
    }
    if let Some(step) = step {
        ctx.keep_reused(Reused { step, digest, kept });
    }
    done
}

/// An embedding's use: `input` tokens, and none answered.
fn estimate(input: u64) -> crate::ai_budget::Used {
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
    let now = ctx.now();
    let answer = match &kept {
        Kept::Answer(text) => Answer::Wire(text.clone()),
        Kept::Condition(rule) => Answer::Failed(rule.word().to_string()),
        Kept::Malformed(_) => Answer::Failed(RuleKey::Malformed.word().to_string()),
    };
    let capabilities = ctx.capabilities_mut();
    let place = capabilities.sent(&made, Source::Replayed, now);
    capabilities.ended(place, &answer, Some(0));
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
