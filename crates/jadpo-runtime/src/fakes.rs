//! A package's fake (area 11, "Tests and fakes"; area 13, "Provider calls
//! and capabilities in tests"; chunk 7.2d): what answers a provider's
//! operations in `jadpo test`, and in `jadpo dev` unless a call is let
//! through to the sandbox.
//!
//! [`ProviderFake`] stands in a connection's driver ([`crate::Faked`]
//! gives it in the connection's place). Each call of an operation a test's
//! `fakes:` scripted takes the next scripted answer; every other call is
//! answered [`Answer::Generated`], which the call's site makes from the
//! operation's output type ([`generated`]), as the provider would send it,
//! and reads back through the operation's real decoding, as any answer is.
//! A scripted value is written the same way, as the provider sends it,
//! so a fake can't give a shape the provider never would, and what the
//! code reads went through the same reading as a real answer.
//!
//! The fake counts each scripted operation's calls, so the test runner
//! reports a script that ran out (`test.fake_exhausted`) or has answers
//! left (`test.fake_unused`). It keeps no state between calls beyond its
//! script: a flow that creates and then reads one object scripts the read.

use std::cell::RefCell;
use std::collections::VecDeque;

use jadpo_std::http::{Codec, CodecId};
use jadpo_value::json::{Codecs, Path};

use crate::LocalFuture;
use crate::capability::{Answer, Call, Capability, Source};
use crate::places::Frame;

/// One operation's script: the answers left, in order, and how many were
/// scripted and how many calls came.
#[derive(Debug)]
struct Scripts {
    /// `connection.resource.operation`.
    name: String,
    /// Where the test scripted it.
    from: Frame,
    answers: VecDeque<Answer>,
    scripted: usize,
    called: usize,
}

/// How a scripted operation was used, once the test ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    /// `couriers.pickups.create`.
    pub operation: String,
    /// Where the test's `fakes:` scripted it.
    pub from: Frame,
    pub scripted: usize,
    pub called: usize,
}

impl Usage {
    /// Called more times than it was scripted.
    pub fn exhausted(&self) -> bool {
        self.called > self.scripted
    }

    /// Called fewer times than it was scripted.
    pub fn unused(&self) -> bool {
        self.called < self.scripted
    }
}

/// A package's fake: scripted answers per operation, the generated
/// default otherwise.
#[derive(Debug, Default)]
pub struct ProviderFake {
    scripts: RefCell<Vec<Scripts>>,
}

impl ProviderFake {
    pub fn new() -> ProviderFake {
        ProviderFake::default()
    }

    /// How each scripted operation was used, in the order first scripted.
    pub fn usage(&self) -> Vec<Usage> {
        self.scripts
            .borrow()
            .iter()
            .map(|script| Usage {
                operation: script.name.clone(),
                from: script.from,
                scripted: script.scripted,
                called: script.called,
            })
            .collect()
    }
}

impl Capability for ProviderFake {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        let name = call.name();
        let answer = {
            let mut scripts = self.scripts.borrow_mut();
            match scripts.iter_mut().find(|script| script.name == name) {
                Some(script) => {
                    script.called += 1;
                    // Past its script, the generated default: the test
                    // fails as `test.fake_exhausted` once it ends.
                    script.answers.pop_front().unwrap_or(Answer::Generated)
                }
                None => Answer::Generated,
            }
        };
        Box::pin(async move { answer })
    }

    fn source(&self) -> Source {
        Source::Generated
    }

    fn source_of(&self, call: &Call) -> Source {
        let name = call.name();
        let scripted = self
            .scripts
            .borrow()
            .iter()
            .any(|script| script.name == name && !script.answers.is_empty());
        if scripted {
            Source::Scripted
        } else {
            Source::Generated
        }
    }

    fn script(&self, connection: &str, operation: &str, answer: Answer, from: Frame) -> bool {
        let name = format!("{connection}.{operation}");
        let mut scripts = self.scripts.borrow_mut();
        match scripts.iter_mut().find(|script| script.name == name) {
            Some(script) => {
                script.answers.push_back(answer);
                script.scripted += 1;
            }
            None => scripts.push(Scripts {
                name,
                from,
                answers: VecDeque::from([answer]),
                scripted: 1,
                called: 0,
            }),
        }
        true
    }
}

/// One answer a test's `fakes:` scripts (area 13).
#[derive(Clone, Debug, PartialEq)]
pub enum Script {
    /// A value of the operation's output type.
    Value(jadpo_value::Value),
    /// One of its failures, by its code.
    Failed(String),
    /// It surely did nothing: `unavailable`, or `in_use`.
    Unavailable,
    /// It went and no answer came back: `lost`.
    Lost,
    Misconfigured,
}

/// Gives the fake in connection `connection`'s place the next answer to
/// `operation` (area 13, `fakes:`; chunk 7.2d), scripted by the test at
/// the current frame of `ctx`: a value written as the provider sends it,
/// where the operation's `output_at:` reads it, so it is read back through
/// the operation's real decoding.
pub fn script(
    ctx: &mut crate::Ctx<'_>,
    providers: &crate::provider::Providers<'_>,
    (connection, operation): (&str, &str),
    given: Script,
) -> Result<(), &'static str> {
    let answer = match given {
        Script::Value(value) => match as_sent(providers, connection, operation, &value) {
            Some(sent) => Answer::Wire(sent),
            None => return Err("a scripted value its operation can't write"),
        },
        Script::Failed(code) => Answer::Failed(code),
        Script::Unavailable => Answer::Unavailable,
        Script::Lost => Answer::Unknown,
        Script::Misconfigured => Answer::Misconfigured,
    };
    let Some(from) = ctx.frames().first().copied() else {
        return Err("a scripted answer outside a test");
    };
    let scripted = ctx
        .host()
        .capability(connection)
        .is_some_and(|fake| fake.script(connection, operation, answer, from));
    if scripted {
        Ok(())
    } else {
        Err("a test's `fakes:` on a host with no fake in the connection's place")
    }
}

// --- generated answers ------------------------------------------------------

/// What a generated answer's values that can't be fixed are made from:
/// the invocation's clock, and a new UUID each time one is needed.
pub struct Making<'m> {
    /// Milliseconds since 1970-01-01T00:00:00Z.
    pub now: i64,
    pub uuid: &'m mut dyn FnMut() -> String,
}

/// Operation `op`'s generated default, as its provider would send it:
/// its output type's [`generated`] value where its `output_at:` reads it,
/// `null` for an operation that gives nothing.
pub fn default_answer(
    ctx: &mut crate::Ctx<'_>,
    providers: &crate::provider::Providers<'_>,
    op: &jadpo_std::provider::OperationPlan,
) -> std::rc::Rc<str> {
    let Some(codec) = op.output.and_then(|number| providers.json.codec_of(number)) else {
        return std::rc::Rc::from("null");
    };
    let now = ctx.now();
    let mut uuid = || uuid_text(ctx.new_id().unwrap_or_default());
    let mut making = Making {
        now,
        uuid: &mut uuid,
    };
    let json = generated(&providers.json.codecs, codec, &mut making);
    std::rc::Rc::from(enveloped(op.output_at.as_deref(), json).to_string())
}

/// `value`, of operation `operation`'s output type, as connection
/// `connection`'s provider would send it: written with the type's codec,
/// its `wire_name:` spellings included, where the operation's `output_at:`
/// reads it. `None` when the plan has no such operation, or it gives
/// nothing.
pub fn as_sent(
    providers: &crate::provider::Providers<'_>,
    connection: &str,
    operation: &str,
    value: &jadpo_value::Value,
) -> Option<std::rc::Rc<str>> {
    let (_, op) = providers.plan.operation(connection, operation)?;
    let codec = providers.json.codec_of(op.output?)?;
    let written = providers.json.codecs.write_value(codec, value);
    let json = serde_json::from_str::<serde_json::Value>(&written).ok()?;
    Some(std::rc::Rc::from(
        enveloped(op.output_at.as_deref(), json).to_string(),
    ))
}

/// A UUID's text: lower case, in its five groups.
pub(crate) fn uuid_text(bytes: [u8; 16]) -> String {
    let hex = jadpo_value::encoding::to_hex(&bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

impl std::fmt::Debug for Making<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Making")
            .field("now", &self.now)
            .finish_non_exhaustive()
    }
}

/// How deep a generated answer goes before a recursive type is cut off
/// with `null`.
const DEEPEST: u32 = 12;

/// The JSON a provider would send for a value of codec `id`'s type that
/// keeps its rules (area 11, "Tests and fakes"): text of its least length,
/// a number at its least, the first plain variant, empty lists and maps,
/// `null` for what is optional, an instant at the invocation's clock and a
/// fresh UUID. A type whose rules no candidate keeps, such as a pattern,
/// gives its first candidate, which reading then refuses, naming the field
/// and the rule, so a test scripts that operation instead.
pub fn generated(codecs: &Codecs<'_>, id: CodecId, making: &mut Making<'_>) -> serde_json::Value {
    generate(codecs, id, making, 0)
}

/// `json` where an operation's `output_at:` reads its answer from, as the
/// provider sends it: `{"data": json}` for `data`.
pub fn enveloped(output_at: Option<&str>, json: serde_json::Value) -> serde_json::Value {
    let Some(path) = output_at else {
        return json;
    };
    path.rsplit('.').fold(json, |inner, step| {
        let mut members = serde_json::Map::new();
        members.insert(step.to_string(), inner);
        serde_json::Value::Object(members)
    })
}

fn generate(
    codecs: &Codecs<'_>,
    id: CodecId,
    making: &mut Making<'_>,
    depth: u32,
) -> serde_json::Value {
    use serde_json::Value as J;
    if depth > DEEPEST {
        return J::Null;
    }
    let Some(codec) = codecs.codecs.get(id.0 as usize) else {
        return J::Null;
    };
    match codec {
        Codec::Nullable(_) => J::Null,
        Codec::Scalar {
            builtin,
            validator,
            int_as_number,
        } => {
            let rules: Vec<&jadpo_std::Rules> = codecs
                .validators
                .get(validator.0 as usize)
                .map(|validator| validator.rules().collect())
                .unwrap_or_default();
            // An instant or a date as the type's own codec writes it, at
            // the invocation's clock.
            let written = |value: jadpo_value::Value| {
                serde_json::from_str::<serde_json::Value>(&codecs.write_value(id, &value))
                    .unwrap_or(serde_json::Value::Null)
            };
            let now = making.now;
            let candidates = match builtin {
                jadpo_std::Builtin::Instant => vec![written(jadpo_value::Value::Instant(now))],
                jadpo_std::Builtin::CalendarDate => {
                    let day = i32::try_from(now.div_euclid(86_400_000)).unwrap_or(0);
                    vec![written(jadpo_value::Value::Date(day))]
                }
                _ => scalar_candidates(*builtin, &rules, *int_as_number, making),
            };
            first_read(codecs, id, candidates)
        }
        Codec::Object { shape, fields, .. } => {
            let Some(shape) = codecs.shapes.get(shape.0 as usize) else {
                return J::Null;
            };
            let mut members = serde_json::Map::new();
            for (slot, field) in fields.iter().enumerate() {
                let Some(name) = shape.json_name(slot) else {
                    continue;
                };
                members.insert(
                    name.to_string(),
                    generate(codecs, *field, making, depth + 1),
                );
            }
            J::Object(members)
        }
        Codec::Enum { variants, .. } => {
            if let Some(plain) = variants.iter().find(|variant| variant.fields.is_none()) {
                return J::String(plain.wire.clone());
            }
            let Some(variant) = variants.first() else {
                return J::Null;
            };
            let mut members = serde_json::Map::new();
            members.insert("tag".to_string(), J::String(variant.wire.clone()));
            if let Some((shape, fields)) = &variant.fields
                && let Some(shape) = codecs.shapes.get(shape.0 as usize)
            {
                for (slot, field) in fields.iter().enumerate() {
                    if let Some(name) = shape.json_name(slot) {
                        members.insert(
                            name.to_string(),
                            generate(codecs, *field, making, depth + 1),
                        );
                    }
                }
            }
            J::Object(members)
        }
        Codec::List { item, .. } | Codec::Set { item, .. } => {
            let one = generate(codecs, *item, making, depth + 1);
            first_read(codecs, id, vec![J::Array(Vec::new()), J::Array(vec![one])])
        }
        Codec::Map { .. } => J::Object(serde_json::Map::new()),
        Codec::Page { .. } => serde_json::json!({ "items": [], "next": null }),
        Codec::Standard { .. } => first_read(
            codecs,
            id,
            [
                "09:30:00",
                "2026-01-15",
                "P1D",
                "GBP",
                "en-GB",
                "GB",
                "1.0.0",
                "Europe/London",
                "example",
            ]
            .into_iter()
            .map(|text| J::String(text.to_string()))
            .collect(),
        ),
        Codec::Float => J::from(0),
        _ => J::Null,
    }
}

/// The first of `candidates` codec `id` reads without a problem; the first
/// candidate when none is.
fn first_read(
    codecs: &Codecs<'_>,
    id: CodecId,
    candidates: Vec<serde_json::Value>,
) -> serde_json::Value {
    for candidate in &candidates {
        let mut problems = Vec::new();
        if codecs
            .read(id, candidate, &Path::default(), &mut problems)
            .is_some()
            && problems.is_empty()
        {
            return candidate.clone();
        }
    }
    candidates
        .into_iter()
        .next()
        .unwrap_or(serde_json::Value::Null)
}

/// The values to try, in order, for a scalar built on `builtin` with
/// `rules`.
fn scalar_candidates(
    builtin: jadpo_std::Builtin,
    rules: &[&jadpo_std::Rules],
    int_as_number: bool,
    making: &mut Making<'_>,
) -> Vec<serde_json::Value> {
    use jadpo_std::Builtin as B;
    use serde_json::Value as J;
    let text = |text: &str| J::String(text.to_string());
    let least_length = rules
        .iter()
        .filter_map(|rules| rules.min_length)
        .max()
        .unwrap_or(0);
    let least = rules
        .iter()
        .filter_map(|rules| rules.min)
        .max_by(|a, b| a.compare(*b));
    let most = rules
        .iter()
        .filter_map(|rules| rules.max)
        .min_by(|a, b| a.compare(*b));
    match builtin {
        B::Bool => vec![J::Bool(false), J::Bool(true)],
        B::Int | B::PageSize | B::PluralCount => {
            let mut numbers: Vec<i128> = Vec::new();
            if let Some(least) = least {
                numbers.push(whole_at_least(least));
            }
            numbers.extend([0, 1]);
            if let Some(most) = most {
                numbers.push(most.mantissa / 10i128.pow(most.scale));
            }
            numbers
                .into_iter()
                .map(|n| {
                    if int_as_number && let Ok(n) = i64::try_from(n) {
                        J::from(n)
                    } else {
                        J::String(n.to_string())
                    }
                })
                .collect()
        }
        B::Decimal | B::Percentage => {
            let mut numbers: Vec<String> = Vec::new();
            if let Some(least) = least {
                numbers.push(least.to_string());
            }
            numbers.extend(["0".to_string(), "1".to_string()]);
            if let Some(most) = most {
                numbers.push(most.to_string());
            }
            numbers.into_iter().map(J::String).collect()
        }
        B::Uuid => vec![J::String((making.uuid)())],
        B::Email => vec![text("someone@example.com")],
        B::Url => vec![text("https://example.com")],
        B::IpAddress => vec![text("192.0.2.1")],
        B::ZonedTime => {
            vec![serde_json::json!({ "instant": "2000-01-01T00:00:00Z", "zone": "Europe/London" })]
        }
        B::Duration => vec![text("PT1M"), text("1m"), J::from(60_000)],
        B::Money => vec![
            serde_json::json!({ "amount": "0.00", "currency": "GBP" }),
            text("0.00 GBP"),
        ],
        B::Quantity(_) => vec![text("0"), J::from(0)],
        _ => {
            let length = usize::try_from(least_length.max(1)).unwrap_or(1);
            let mut texts = vec![
                text("example"),
                J::String("a".repeat(length)),
                J::String("A".repeat(length)),
                J::String("1".repeat(length)),
            ];
            if least_length == 0 {
                texts.push(text(""));
            }
            texts
        }
    }
}

/// The least whole number at or above `least`.
fn whole_at_least(least: jadpo_std::Num) -> i128 {
    let unit = 10i128.pow(least.scale);
    let whole = least.mantissa.div_euclid(unit);
    if least.mantissa.rem_euclid(unit) == 0 {
        whole
    } else {
        whole + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_value::Value;

    fn frame() -> Frame {
        Frame {
            callable: 0,
            site: Frame::ENTRY,
        }
    }

    fn call(operation: &str) -> Call {
        Call::new(
            "couriers",
            operation,
            Value::Unit,
            crate::capability::Repeat::Keyed,
        )
    }

    fn answer(fake: &ProviderFake, operation: &str) -> Answer {
        crate::block_on(fake.call(&call(operation))).expect("nothing waits")
    }

    /// A scripted operation takes its answers in order, then the generated
    /// default; every other operation the generated default. Each scripted
    /// one's calls are counted against its script.
    #[test]
    fn a_fake_answers_from_its_script_then_with_the_default() {
        let fake = ProviderFake::new();
        assert!(fake.script(
            "couriers",
            "bookings.create",
            Answer::Failed("slot_taken".into()),
            frame()
        ));
        assert!(fake.script("couriers", "bookings.create", Answer::Unavailable, frame()));
        assert_eq!(fake.source_of(&call("bookings.create")), Source::Scripted);
        assert_eq!(fake.source_of(&call("bookings.get")), Source::Generated);
        assert_eq!(
            answer(&fake, "bookings.create"),
            Answer::Failed("slot_taken".into())
        );
        assert_eq!(answer(&fake, "bookings.get"), Answer::Generated);
        assert_eq!(answer(&fake, "bookings.create"), Answer::Unavailable);
        let used = fake.usage();
        assert_eq!(used.len(), 1);
        assert!(!used[0].exhausted() && !used[0].unused());
        assert_eq!(answer(&fake, "bookings.create"), Answer::Generated);
        assert!(fake.usage()[0].exhausted());
    }

    #[test]
    fn an_answer_is_put_where_output_at_reads_it() {
        let json = serde_json::json!({ "id": "b1" });
        assert_eq!(enveloped(None, json.clone()), json);
        assert_eq!(
            enveloped(Some("data.booking"), json),
            serde_json::json!({ "data": { "booking": { "id": "b1" } } })
        );
    }

    #[test]
    fn the_least_whole_number_keeps_a_minimum() {
        let num = |mantissa, scale| jadpo_std::Num { mantissa, scale };
        assert_eq!(whole_at_least(num(5, 0)), 5);
        assert_eq!(whole_at_least(num(15, 1)), 2);
        assert_eq!(whole_at_least(num(-15, 1)), -1);
    }
}
