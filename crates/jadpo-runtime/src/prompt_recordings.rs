//! Recordings of a prompt's answer (area 21, "Tests"; chunk 5.5v4):
//! `prompts/recordings/<prompt>/<name>.json`, an answer a model gave and
//! the version of the prompt it gave it to:
//!
//! ```json
//! {
//!   "answer": {
//!     "reply": "{Text}",
//!     "resolved_by_articles": false
//!   },
//!   "version": "answer_question@864ad76b"
//! }
//! ```
//!
//! The answer is a value of the type the prompt is asked for as, written
//! as `json.write` writes it, with the plain values a recording keeps out
//! of the repository as placeholders, as a provider's recording does
//! (area 11, chunk 7.2d2): each is filled with a generated value of its
//! type when the recording is read. [`check`] is what `jadpo test` runs
//! for each one ([`check_file`]): the answer decoded through every type the
//! prompt's calls ask for. [`answer`] is what a test's `recorded("<name>")` answers with.
//! [`recording_text`] is what `jadpo ai record` keeps (chunk 5.5v4b), asked
//! with a prompt's values `jadpo dev` logged ([`values_written`],
//! [`values_read`]).

use std::rc::Rc;

use jadpo_std::RuleKey;
use jadpo_std::ai::GenerateSite;
use jadpo_std::embed::{ANSWER, PromptRecording, VERSION};
use jadpo_value::Value;
use jadpo_value::json::{Problem, Unread as JsonUnread};

use crate::ai::Ai;
use crate::fakes::Making;
use crate::{Ctx, JsonCodecs};

/// Why a recording isn't one its prompt's calls read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unread {
    /// No prompt of its folder's name is beside it.
    NoPrompt,
    /// Not one JSON object.
    NotJson,
    /// No `answer` member.
    NoAnswer,
    /// No `version` member naming the prompt's version it was made from.
    NoVersion,
    /// No call asks the prompt for an answer, so nothing reads it.
    NotAsked,
    /// Its answer doesn't decode through a type a call asks for: the
    /// field, dotted from the answer (empty for the answer itself), and the
    /// rule it broke.
    Undecoded { field: String, rule: String },
}

/// The recording's text as a JSON object.
fn document(text: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    match serde_json::from_str::<serde_json::Value>(text).ok()? {
        serde_json::Value::Object(members) => Some(members),
        _ => None,
    }
}

/// The version of the prompt `text`, a recording, was made from:
/// `answer_question@864ad76b`.
pub fn recorded_version(text: &str) -> Option<String> {
    document(text)?.get(VERSION)?.as_str().map(str::to_string)
}

/// Whether the recording was made from another version of its prompt
/// than the one there now: its answer still decodes, but may no longer be
/// what the model says (area 21, "Tests").
pub fn stale(recording: &PromptRecording) -> bool {
    match (&recording.version, recorded_version(&recording.text)) {
        (Some(now), Some(then)) => *now != then,
        _ => false,
    }
}

/// The answer `text`, a recording, keeps, as JSON text with each
/// placeholder filled with a generated value of codec `answer`'s type;
/// `None` when it keeps none.
pub fn answer(
    json: &JsonCodecs<'_>,
    answer: u32,
    text: &str,
    making: &mut Making<'_>,
) -> Option<String> {
    let kept = document(text)?.remove(ANSWER)?;
    let filled = match json.codec_of(answer) {
        Some(codec) => crate::recorded::filled_value(&json.codecs, codec, &kept, making),
        None => kept,
    };
    Some(filled.to_string())
}

/// [`answer`], its placeholders' generated values made at `ctx`'s clock:
/// what a test's `recorded("<name>")` answers with.
pub fn answer_at(
    ctx: &mut Ctx<'_>,
    json: &JsonCodecs<'_>,
    number: u32,
    text: &str,
) -> Option<String> {
    let now = ctx.now();
    let mut uuid = || crate::fakes::uuid_text(ctx.new_id().unwrap_or_default());
    let mut making = Making {
        now,
        uuid: &mut uuid,
    };
    answer(json, number, text, &mut making)
}

/// Reads the recording of a prompt's answer at `file` in `plan` through
/// each type its prompt is asked for as ([`check`]), at `ctx`: what `jadpo
/// test` runs for each. `None` when the plan holds no such recording.
pub fn check_file(
    plan: &crate::Plan<'_>,
    ctx: &mut Ctx<'_>,
    file: &str,
) -> Option<Result<(), Unread>> {
    let recording = plan
        .prompt_recordings
        .iter()
        .find(|recording| recording.file == file)?;
    Some(check(ctx, &plan.json, recording))
}

/// Reads `recording` as `jadpo test` does (chunk 5.5v4): one JSON object
/// with its `version` and its `answer`, the answer decoded, its
/// placeholders filled at `ctx`, through each type a call asks its
/// prompt for as, every rule included.
pub fn check(
    ctx: &mut Ctx<'_>,
    json: &JsonCodecs<'_>,
    recording: &PromptRecording,
) -> Result<(), Unread> {
    if recording.version.is_none() {
        return Err(Unread::NoPrompt);
    }
    let members = document(&recording.text).ok_or(Unread::NotJson)?;
    if !members.contains_key(ANSWER) {
        return Err(Unread::NoAnswer);
    }
    if !members
        .get(VERSION)
        .is_some_and(serde_json::Value::is_string)
    {
        return Err(Unread::NoVersion);
    }
    if recording.answers.is_empty() {
        return Err(Unread::NotAsked);
    }
    let now = ctx.now();
    for number in &recording.answers {
        let Some(codec) = json.codec_of(*number) else {
            continue;
        };
        let mut uuid = || crate::fakes::uuid_text(ctx.new_id().unwrap_or_default());
        let mut making = Making {
            now,
            uuid: &mut uuid,
        };
        let Some(text) = answer(json, *number, &recording.text, &mut making) else {
            return Err(Unread::NoAnswer);
        };
        if let Err(unread) = json.codecs.read_value(codec, &Value::text(&text)) {
            return Err(match unread {
                JsonUnread::Malformed => Unread::Undecoded {
                    field: String::new(),
                    rule: jadpo_std::RuleKey::Malformed.word().to_string(),
                },
                JsonUnread::Mismatch(problem) => Unread::Undecoded {
                    field: problem.field,
                    rule: problem.rule,
                },
            });
        }
    }
    Ok(())
}

// --- making one (chunk 5.5v4b) -------------------------------------------------

/// A recording's text (`jadpo ai record`): `answer`, the value the model
/// answered a call of `site` with, kept with its plain values as
/// placeholders ([`crate::recorded::kept_answer`]), and `version`, the
/// prompt's version it was asked, members in name order and two spaces
/// deep; `None` when the answer can't be written.
pub fn recording_text(
    ctx: &mut Ctx<'_>,
    json: &JsonCodecs<'_>,
    site: &GenerateSite,
    answer: &Value,
    version: &str,
) -> Option<String> {
    let codec = json.codec_of(site.answer)?;
    let written = json.codecs.write_value(codec, answer);
    let now = ctx.now();
    let mut uuid = || crate::fakes::uuid_text(ctx.new_id().unwrap_or_default());
    let mut making = Making {
        now,
        uuid: &mut uuid,
    };
    let kept = crate::recorded::kept_answer(&json.codecs, codec, &written, 1, &mut making)?;
    Some(format!(
        "{{\n  \"{ANSWER}\": {kept},\n  \"{VERSION}\": {}\n}}\n",
        serde_json::Value::String(version.to_string())
    ))
}

/// `prompt`, a value of `site`'s prompt, as `jadpo dev`'s call log keeps
/// it beside the call (chunk 5.5v4b), for `jadpo ai record` to ask with
/// again: one JSON object, each field the prompt's text writes as its
/// type's JSON, and its conversation's turns as `{ content, role }`
/// objects; `None` when it isn't the prompt's.
pub fn values_written(ai: &Ai<'_>, site: &GenerateSite, prompt: &Value) -> Option<String> {
    let entry = ai.prompts.entries.get(site.entry as usize)?;
    let Value::Record(record) = prompt else {
        return None;
    };
    let mut members = serde_json::Map::new();
    for (at, field) in entry.fields.iter().enumerate() {
        let codec = ai.json.codec_of(*site.fields.get(at)?)?;
        let written = ai.json.codecs.write_value(codec, record.get(&field.name)?);
        members.insert(field.name.clone(), serde_json::from_str(&written).ok()?);
    }
    if let Some(conversation) = &entry.conversation {
        let Value::List(turns) = record.get(conversation)? else {
            return None;
        };
        let turns = turns
            .iter()
            .map(|turn| {
                let Value::Record(message) = turn else {
                    return None;
                };
                let Value::Variant(role) = message.get("role")? else {
                    return None;
                };
                let Value::Text(content) = message.get("content")? else {
                    return None;
                };
                Some(serde_json::json!({ "content": &**content, "role": &*role.name }))
            })
            .collect::<Option<Vec<_>>>()?;
        members.insert(conversation.clone(), serde_json::Value::Array(turns));
    }
    Some(serde_json::Value::Object(members).to_string())
}

/// The value of `site`'s prompt that `text`, as [`values_written`] writes
/// one, holds, each field read as its type with every rule; the first
/// field that doesn't read, and why, otherwise: the prompt may have
/// changed since the values were kept.
pub fn values_read(ai: &Ai<'_>, site: &GenerateSite, text: &str) -> Result<Value, Problem> {
    let problem = |field: &str, rule: &str| Problem {
        field: field.to_string(),
        rule: rule.to_string(),
    };
    let Some(entry) = ai.prompts.entries.get(site.entry as usize) else {
        return Err(problem("", "type"));
    };
    let Ok(serde_json::Value::Object(members)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        return Err(problem("", RuleKey::Malformed.word()));
    };
    let mut fields: Vec<(Rc<str>, Value)> = Vec::new();
    for (at, field) in entry.fields.iter().enumerate() {
        let codec = site
            .fields
            .get(at)
            .and_then(|number| ai.json.codec_of(*number))
            .ok_or_else(|| problem(&field.name, "type"))?;
        let given = members
            .get(&field.name)
            .ok_or_else(|| problem(&field.name, "required"))?;
        let mut problems = Vec::new();
        let read = ai.json.codecs.read(
            codec,
            given,
            &jadpo_value::json::Path::default(),
            &mut problems,
        );
        match (read, problems.into_iter().next()) {
            (Some(value), None) => fields.push((Rc::from(field.name.as_str()), value)),
            (_, Some(first)) => {
                let at = if first.field.is_empty() {
                    field.name.clone()
                } else {
                    format!("{}.{}", field.name, first.field)
                };
                return Err(problem(&at, &first.rule));
            }
            (None, None) => return Err(problem(&field.name, "type")),
        }
    }
    if let Some(conversation) = &entry.conversation {
        let turns = match members.get(conversation) {
            Some(serde_json::Value::Array(turns)) => turns,
            _ => return Err(problem(conversation, "required")),
        };
        let mut messages = Vec::new();
        for (n, turn) in turns.iter().enumerate() {
            let role = turn.get("role").and_then(serde_json::Value::as_str);
            let content = turn.get("content").and_then(serde_json::Value::as_str);
            match (role, content) {
                (Some(role), Some(content))
                    if jadpo_std::standard::ai::CHAT_ROLES
                        .iter()
                        .any(|(name, _)| *name == role) =>
                {
                    messages.push(jadpo_value::prompt::chat_message(
                        role,
                        Value::text(content),
                    ));
                }
                _ => return Err(problem(&format!("{conversation}.{n}"), "type")),
            }
        }
        fields.push((Rc::from(conversation.as_str()), Value::list(messages)));
    }
    Ok(Value::record(&entry.type_name, fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording(version: Option<&str>, text: &str) -> PromptRecording {
        PromptRecording {
            file: "domains/support/prompts/recordings/answer_question/sesame.json".into(),
            prompt: "answer_question".into(),
            name: "sesame".into(),
            version: version.map(str::to_string),
            answers: vec![0],
            text: text.into(),
        }
    }

    /// A recording made from another version of its prompt is stale; one
    /// from the version there now, or one that names none, isn't.
    #[test]
    fn a_recording_of_another_version_is_stale() {
        let text = "{\n  \"answer\": \"No.\",\n  \"version\": \"answer_question@1a2b3c4d\"\n}\n";
        assert!(stale(&recording(Some("answer_question@864ad76b"), text)));
        assert!(!stale(&recording(Some("answer_question@1a2b3c4d"), text)));
        assert!(!stale(&recording(Some("answer_question@1a2b3c4d"), "{}")));
        assert_eq!(
            recorded_version(text).as_deref(),
            Some("answer_question@1a2b3c4d")
        );
    }
}
