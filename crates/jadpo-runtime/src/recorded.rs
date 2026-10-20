//! A provider package's recordings (area 11, "Provenance and
//! confirmation"; chunk 7.2d2): a real answer from the provider's sandbox,
//! kept in the package as `recordings/<resource>.<operation>.json`, that
//! confirms the operation's types and that every decoder reads, in `jadpo
//! test` and in the publishing bar; and what `jadpo dev`'s fake answers
//! the operation with. (Not the recording area 17 replays a request from,
//! [`crate::capability::Recording`], which keeps a run's effects.)
//!
//! A recording keeps the answer as the provider sent it with its values
//! replaced by placeholders ([`kept`]): only the path to `output_at:` and
//! the members its output type declares, each text or number read as a
//! plain value replaced by its built-in type's name in braces,
//! `"{Text}"`, `"{Instant}"`. What a generated value can't stand in for
//! is kept as sent: a field the provider sends in an encoding of its own
//! (`wire:`, or its `decode:` and `encode:`), and the whole answer of an
//! operation with `wire_output:`, which its decoders read; a value whose
//! type's rules no generated value keeps (a pattern); booleans, `null`,
//! an enum's spellings and a map's keys. Reading one fills each
//! placeholder with a generated value of its type ([`filled`]), as the
//! fake's generated default makes one, and reads it as any answer is.
//!
//! [`check`] is what `jadpo test` runs for each recording of the app's own
//! integrations: the recording filled and read as its operation's output,
//! whole, through the package's own decoders at the call's site, with each
//! field's `decode:` and `encode:` checked to give back what was recorded.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;

use jadpo_std::http::{Codec, CodecId, WireCoding};
use jadpo_std::provider::OperationPlan;
use jadpo_value::Value;
use jadpo_value::json::{Codecs, Numbers, Path};
use serde_json::Value as J;

use crate::fakes::{Making, enveloped, generated};
use crate::provider::{Functions, SiteCoder, at_path};
use crate::{Ctx, JsonCodecs};

/// The folder a package keeps its recordings in.
pub const FOLDER: &str = "recordings";

/// Where operation `operation`'s recording is in its package:
/// `recordings/pickups.create.json`.
pub fn file_name(operation: &str) -> String {
    format!("{FOLDER}/{operation}.json")
}

/// Whether `text` is a placeholder: a name in braces, `{Text}`.
pub fn is_placeholder(text: &str) -> bool {
    text.strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        })
}

/// Why an answer, or a recording, isn't one of its operation's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unread {
    /// The package has no operation of the recording's name.
    NoOperation,
    /// Not one JSON document.
    NotJson,
    /// Nothing at the operation's `output_at:`.
    NothingAt(String),
    /// A value its output type, or a field's decoder, doesn't read: the
    /// field, dotted from the output (empty for the output itself), and
    /// the rule it broke.
    Undecoded { field: String, rule: String },
    /// The operation's own `decode:` didn't give its output: how it ended.
    DecoderFailed(String),
    /// A field's `decode:` and `encode:` don't give back what was recorded:
    /// the two functions by name, and the value recorded and the value
    /// written back.
    RoundTrip {
        decode: String,
        encode: String,
        recorded: String,
        back: String,
    },
}

/// `answer`, operation `op`'s answer as its provider sent it, as its
/// recording keeps it ([the module's](self) placeholders), written with
/// two spaces of indentation and a closing newline; why it can't be kept
/// otherwise. That it reads as the operation's output is [`check`]'s.
pub fn kept(
    json: &JsonCodecs<'_>,
    op: &OperationPlan,
    answer: &str,
    making: &mut Making<'_>,
) -> Result<String, Unread> {
    let (document, numbers) = jadpo_value::json::parse(answer.as_bytes()).ok_or(Unread::NotJson)?;
    let at = match &op.output_at {
        Some(path) => at_path(&document, path).ok_or_else(|| Unread::NothingAt(path.clone()))?,
        None => &document,
    };
    let codec = op
        .decode
        .as_ref()
        .map_or(op.output, |(wire, _)| Some(*wire))
        .and_then(|number| json.codec_of(number));
    let value = match (codec, &op.decode) {
        // An operation's own decoder reads the whole answer as sent.
        (Some(_), Some(_)) => at.clone(),
        (Some(codec), None) => placeholders(&json.codecs, codec, at, making),
        (None, _) => J::Null,
    };
    // Kept where it was sent, so each number kept stands where the
    // answer's own digits for it were read.
    let kept = enveloped(op.output_at.as_deref(), value);
    let mut out = String::new();
    write(&kept, &numbers, &mut Vec::new(), 0, &mut out);
    out.push('\n');
    Ok(out)
}

/// [`kept`], its generated stand-ins made at `ctx`'s clock.
pub fn keep(
    ctx: &mut Ctx<'_>,
    json: &JsonCodecs<'_>,
    op: &OperationPlan,
    answer: &str,
) -> Result<String, Unread> {
    let now = ctx.now();
    let mut uuid = || crate::fakes::uuid_text(ctx.new_id().unwrap_or_default());
    let mut making = Making {
        now,
        uuid: &mut uuid,
    };
    kept(json, op, answer, &mut making)
}

/// `text`, operation `op`'s recording, with each placeholder filled with a
/// generated value of its type: the answer as the provider would send it.
/// `None` when it isn't JSON.
pub fn filled(
    json: &JsonCodecs<'_>,
    op: &OperationPlan,
    text: &str,
    making: &mut Making<'_>,
) -> Option<String> {
    let (mut document, numbers) = jadpo_value::json::parse(text.as_bytes())?;
    let codec = op
        .decode
        .as_ref()
        .map_or(op.output, |(wire, _)| Some(*wire))
        .and_then(|number| json.codec_of(number));
    if let Some(codec) = codec {
        let at = match &op.output_at {
            Some(path) => at_path_mut(&mut document, path),
            None => Some(&mut document),
        };
        if let Some(at) = at {
            *at = fill(&json.codecs, codec, at, making);
        }
    }
    let mut out = String::new();
    write(&document, &numbers, &mut Vec::new(), 0, &mut out);
    Some(out)
}

/// Reads `text`, a recording of operation `op`, as the operation's whole
/// output (chunk 7.2d2): its placeholders filled, read with its output
/// type's codec through the package's own decoders, run by `functions`
/// at `ctx`, with each field's `decode:` and `encode:` checked to give back
/// the value recorded. The first thing that doesn't hold.
pub fn check(
    ctx: &mut Ctx<'_>,
    json: &JsonCodecs<'_>,
    op: &OperationPlan,
    text: &str,
    functions: &dyn Functions,
) -> Result<(), Unread> {
    let Some(output) = op.output else {
        return Ok(());
    };
    let now = ctx.now();
    let answer = {
        let mut uuid = || crate::fakes::uuid_text(ctx.new_id().unwrap_or_default());
        let mut making = Making {
            now,
            uuid: &mut uuid,
        };
        filled(json, op, text, &mut making).ok_or(Unread::NotJson)?
    };
    let (document, numbers) = jadpo_value::json::parse(answer.as_bytes()).ok_or(Unread::NotJson)?;
    let (at, numbers) = match &op.output_at {
        Some(path) => {
            let at = at_path(&document, path).ok_or_else(|| Unread::NothingAt(path.clone()))?;
            let prefix: Vec<String> = path.split('.').map(str::to_string).collect();
            (at, numbers.within(&prefix))
        }
        None => (&document, numbers),
    };
    let read_as = op.decode.as_ref().map_or(output, |(wire, _)| *wire);
    let Some(codec) = json.codec_of(read_as) else {
        return Err(Unread::Undecoded {
            field: String::new(),
            rule: "type".to_string(),
        });
    };
    let pairs: HashMap<String, String> = json
        .codecs
        .codecs
        .iter()
        .filter_map(|codec| match codec {
            Codec::Wired {
                coding: WireCoding::Functions { decode, encode, .. },
                ..
            } => Some((decode.clone(), encode.clone())),
            _ => None,
        })
        .collect();
    let coder = RoundTrip {
        site: SiteCoder {
            ctx: RefCell::new(&mut *ctx),
            functions,
            at: crate::Frame::ENTRY,
            ended: RefCell::new(None),
        },
        pairs,
        broken: RefCell::new(None),
    };
    let codecs = Codecs {
        coder: Some(&coder),
        ..json.codecs
    };
    let mut problems = Vec::new();
    let path = Path::default().numbered(&numbers);
    let read = codecs.read(codec, at, &path, &mut problems);
    if let Some(broken) = coder.broken.borrow_mut().take() {
        return Err(broken);
    }
    let value = match (read, problems.into_iter().next()) {
        (Some(value), None) => value,
        (_, Some(problem)) => {
            return Err(Unread::Undecoded {
                field: problem.field,
                rule: problem.rule,
            });
        }
        (None, None) => {
            return Err(Unread::Undecoded {
                field: String::new(),
                rule: coder
                    .site
                    .ended
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| "type".to_string()),
            });
        }
    };
    if let Some((_, function)) = &op.decode
        && coder.site.run(function, value).is_none()
    {
        return Err(Unread::DecoderFailed(
            coder.site.ended.into_inner().unwrap_or_default(),
        ));
    }
    Ok(())
}

/// A package function's name as its package writes it, from its identity
/// key: its library and its name, `slots.parse_window`.
pub fn function_name(key: &str) -> String {
    let last = key.rsplit(['/', ' ']).next().unwrap_or(key);
    let parts: Vec<&str> = last.split('.').collect();
    parts
        .get(parts.len().saturating_sub(2)..)
        .unwrap_or_default()
        .join(".")
}

/// The package's field decoders run at a recording's reading, each
/// `decode:` checked against its `encode:`.
struct RoundTrip<'c, 'x> {
    site: SiteCoder<'c, 'x>,
    /// Each field's `decode:` and the `encode:` beside it, by key.
    pairs: HashMap<String, String>,
    /// The first pair that didn't give back what was recorded.
    broken: RefCell<Option<Unread>>,
}

impl std::fmt::Debug for RoundTrip<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoundTrip").finish_non_exhaustive()
    }
}

impl jadpo_value::json::Coder for RoundTrip<'_, '_> {
    fn decode(&self, function: &str, wire: Value) -> Option<Value> {
        let value = self.site.run(function, wire.clone())?;
        if let Some(encode) = self.pairs.get(function)
            && self.broken.borrow().is_none()
        {
            let back = self.site.run(encode, value.clone());
            if back.as_ref() != Some(&wire) {
                *self.broken.borrow_mut() = Some(Unread::RoundTrip {
                    decode: function_name(function),
                    encode: function_name(encode),
                    recorded: wire.to_string(),
                    back: back.map_or_else(
                        || {
                            self.site.ended.borrow().clone().map_or_else(
                                || "nothing".to_string(),
                                |how| format!("nothing, ending with {how}"),
                            )
                        },
                        |back| back.to_string(),
                    ),
                });
            }
        }
        Some(value)
    }

    fn encode(&self, function: &str, value: Value) -> Option<Value> {
        self.site.run(function, value)
    }

    fn unwritten(&self, rule: &str) {
        jadpo_value::json::Coder::unwritten(&self.site, rule);
    }
}

// --- placeholders -------------------------------------------------------------

/// The placeholder for a value of a built-in type called `name`.
fn placeholder(name: &str) -> J {
    J::String(format!("{{{name}}}"))
}

/// Whether a generated value of codec `id`'s type reads as one, so a
/// placeholder can stand in for the value sent.
fn stands_in(codecs: &Codecs<'_>, id: CodecId, making: &mut Making<'_>) -> bool {
    let candidate = generated(codecs, id, making);
    let mut problems = Vec::new();
    codecs
        .read(id, &candidate, &Path::default(), &mut problems)
        .is_some()
        && problems.is_empty()
}

/// `json`, a value of codec `id`'s type as sent, with its plain values
/// replaced by placeholders and the members its type doesn't declare left
/// out.
fn placeholders(codecs: &Codecs<'_>, id: CodecId, json: &J, making: &mut Making<'_>) -> J {
    let Some(codec) = codecs.codecs.get(id.0 as usize) else {
        return json.clone();
    };
    match (codec, json) {
        (Codec::Nullable(_), J::Null) => J::Null,
        (Codec::Nullable(inner), _) => placeholders(codecs, *inner, json, making),
        (Codec::Scalar { builtin, .. }, J::String(_) | J::Number(_)) => {
            if stands_in(codecs, id, making) {
                placeholder(builtin.name())
            } else {
                json.clone()
            }
        }
        (Codec::Standard { ty }, J::String(_)) => {
            if stands_in(codecs, id, making) {
                placeholder(ty)
            } else {
                json.clone()
            }
        }
        (Codec::Float, J::Number(_)) => placeholder("Float"),
        (Codec::Object { shape, fields, .. }, J::Object(members)) => {
            let Some(shape) = codecs.shapes.get(shape.0 as usize) else {
                return json.clone();
            };
            let mut out = serde_json::Map::new();
            for (slot, field) in fields.iter().enumerate() {
                let Some(name) = shape.json_name(slot) else {
                    continue;
                };
                if let Some(value) = members.get(name) {
                    out.insert(
                        name.to_string(),
                        placeholders(codecs, *field, value, making),
                    );
                }
            }
            J::Object(out)
        }
        (Codec::Enum { variants, .. }, J::Object(members)) => {
            let tag = members.get("tag").and_then(J::as_str);
            let Some(variant) = variants
                .iter()
                .find(|variant| Some(variant.wire.as_str()) == tag)
            else {
                return json.clone();
            };
            let mut out = serde_json::Map::new();
            out.insert("tag".to_string(), J::String(variant.wire.clone()));
            if let Some((shape, fields)) = &variant.fields
                && let Some(shape) = codecs.shapes.get(shape.0 as usize)
            {
                for (slot, field) in fields.iter().enumerate() {
                    if let Some(name) = shape.json_name(slot)
                        && let Some(value) = members.get(name)
                    {
                        out.insert(
                            name.to_string(),
                            placeholders(codecs, *field, value, making),
                        );
                    }
                }
            }
            J::Object(out)
        }
        (Codec::List { item, .. } | Codec::Set { item, .. }, J::Array(items)) => J::Array(
            items
                .iter()
                .map(|value| placeholders(codecs, *item, value, making))
                .collect(),
        ),
        (Codec::Map { value, .. }, J::Object(members)) => J::Object(
            members
                .iter()
                .map(|(key, item)| (key.clone(), placeholders(codecs, *value, item, making)))
                .collect(),
        ),
        (Codec::Page { item }, J::Object(members)) => {
            let mut out = members.clone();
            if let Some(J::Array(items)) = members.get("items") {
                out.insert(
                    "items".to_string(),
                    J::Array(
                        items
                            .iter()
                            .map(|value| placeholders(codecs, *item, value, making))
                            .collect(),
                    ),
                );
            }
            J::Object(out)
        }
        // As sent: a field the provider encodes its own way, which its
        // decoders read, and anything else.
        _ => json.clone(),
    }
}

/// `answer`, a value of codec `id`'s type as `json.write` writes it, as a
/// prompt's recording keeps its answer (area 21; chunk 5.5v4b): its plain
/// values placeholders, as [`kept`] keeps an operation's, written two
/// spaces deep from `depth` with each number's digits; `None` when it
/// isn't JSON.
pub fn kept_answer(
    codecs: &Codecs<'_>,
    id: CodecId,
    answer: &str,
    depth: usize,
    making: &mut Making<'_>,
) -> Option<String> {
    let (document, numbers) = jadpo_value::json::parse(answer.as_bytes())?;
    let kept = placeholders(codecs, id, &document, making);
    let mut out = String::new();
    write(&kept, &numbers, &mut Vec::new(), depth, &mut out);
    Some(out)
}

/// `json`, a recorded value of codec `id`'s type, with each placeholder
/// filled as [`filled`] fills an operation's: what a prompt's recording
/// gives as its answer (area 21; chunk 5.5v4).
pub fn filled_value(codecs: &Codecs<'_>, id: CodecId, json: &J, making: &mut Making<'_>) -> J {
    fill(codecs, id, json, making)
}

/// `json`, a recorded value of codec `id`'s type, with each placeholder
/// replaced by a generated value of its type.
fn fill(codecs: &Codecs<'_>, id: CodecId, json: &J, making: &mut Making<'_>) -> J {
    let Some(codec) = codecs.codecs.get(id.0 as usize) else {
        return json.clone();
    };
    match (codec, json) {
        (Codec::Nullable(_), J::Null) => J::Null,
        (Codec::Nullable(inner), _) => fill(codecs, *inner, json, making),
        (Codec::Scalar { .. } | Codec::Standard { .. } | Codec::Float, J::String(text))
            if is_placeholder(text) =>
        {
            generated(codecs, id, making)
        }
        (Codec::Object { shape, fields, .. }, J::Object(members)) => {
            let Some(shape) = codecs.shapes.get(shape.0 as usize) else {
                return json.clone();
            };
            let mut out = members.clone();
            for (slot, field) in fields.iter().enumerate() {
                if let Some(name) = shape.json_name(slot)
                    && let Some(value) = members.get(name)
                {
                    out.insert(name.to_string(), fill(codecs, *field, value, making));
                }
            }
            J::Object(out)
        }
        (Codec::Enum { variants, .. }, J::Object(members)) => {
            let tag = members.get("tag").and_then(J::as_str);
            let Some(variant) = variants
                .iter()
                .find(|variant| Some(variant.wire.as_str()) == tag)
            else {
                return json.clone();
            };
            let mut out = members.clone();
            if let Some((shape, fields)) = &variant.fields
                && let Some(shape) = codecs.shapes.get(shape.0 as usize)
            {
                for (slot, field) in fields.iter().enumerate() {
                    if let Some(name) = shape.json_name(slot)
                        && let Some(value) = members.get(name)
                    {
                        out.insert(name.to_string(), fill(codecs, *field, value, making));
                    }
                }
            }
            J::Object(out)
        }
        (Codec::List { item, .. } | Codec::Set { item, .. }, J::Array(items)) => J::Array(
            items
                .iter()
                .map(|value| fill(codecs, *item, value, making))
                .collect(),
        ),
        (Codec::Map { value, .. }, J::Object(members)) => J::Object(
            members
                .iter()
                .map(|(key, item)| (key.clone(), fill(codecs, *value, item, making)))
                .collect(),
        ),
        (Codec::Page { item }, J::Object(members)) => {
            let mut out = members.clone();
            if let Some(J::Array(items)) = members.get("items") {
                out.insert(
                    "items".to_string(),
                    J::Array(
                        items
                            .iter()
                            .map(|value| fill(codecs, *item, value, making))
                            .collect(),
                    ),
                );
            }
            J::Object(out)
        }
        _ => json.clone(),
    }
}

/// The value at `path`, dotted, to change in place.
fn at_path_mut<'j>(json: &'j mut J, path: &str) -> Option<&'j mut J> {
    path.split('.').try_fold(json, |at, step| match at {
        J::Object(members) => members.get_mut(step),
        J::Array(items) => items.get_mut(step.parse::<usize>().ok()?),
        _ => None,
    })
}

// --- writing --------------------------------------------------------------------

/// Writes `json` with two spaces of indentation, each number as `numbers`
/// holds its text where it was read from one (the provider's digits,
/// chunk 7.2c2), at `at` from the top.
fn write(json: &J, numbers: &Numbers, at: &mut Vec<String>, depth: usize, out: &mut String) {
    let indent = |depth: usize| "  ".repeat(depth);
    match json {
        J::Number(number) => {
            // A number read from the answer keeps its digits.
            match numbers.get(at) {
                Some(text) => out.push_str(text),
                None => out.push_str(&number.to_string()),
            }
        }
        J::Array(items) if items.is_empty() => out.push_str("[]"),
        J::Array(items) => {
            out.push_str("[\n");
            for (n, item) in items.iter().enumerate() {
                out.push_str(&indent(depth + 1));
                at.push(n.to_string());
                write(item, numbers, at, depth + 1, out);
                at.pop();
                if n + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push(']');
        }
        J::Object(members) if members.is_empty() => out.push_str("{}"),
        J::Object(members) => {
            out.push_str("{\n");
            for (n, (name, value)) in members.iter().enumerate() {
                out.push_str(&indent(depth + 1));
                let _ = write!(out, "{}: ", J::String(name.clone()));
                at.push(name.clone());
                write(value, numbers, at, depth + 1, out);
                at.pop();
                if n + 1 < members.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placeholder_is_a_name_in_braces() {
        assert!(is_placeholder("{Text}"));
        assert!(is_placeholder("{Instant}"));
        assert!(!is_placeholder("{}"));
        assert!(!is_placeholder("Text"));
        assert!(!is_placeholder("{a b}"));
        assert_eq!(
            file_name("pickups.create"),
            "recordings/pickups.create.json"
        );
    }

    #[test]
    fn a_recording_is_written_with_its_numbers_digits() {
        let text = r#"{"data": {"amount": 2.50, "items": [1, 2], "empty": {}}}"#;
        let (json, numbers) = jadpo_value::json::parse(text.as_bytes()).expect("JSON");
        let mut out = String::new();
        write(&json, &numbers, &mut Vec::new(), 0, &mut out);
        assert_eq!(
            out,
            "{\n  \"data\": {\n    \"amount\": 2.50,\n    \"empty\": {},\n    \"items\": [\n      1,\n      2\n    ]\n  }\n}"
        );
    }

    #[test]
    fn a_function_is_named_as_its_package_writes_it() {
        assert_eq!(function_name("money.parse"), "money.parse");
        assert_eq!(
            function_name("AcmeCouriers.slots.parse_window"),
            "slots.parse_window"
        );
    }
}
