//! The AI types' baselines (area 19, "The standard types" and "AI: a
//! future area", chunk 5.5s2): token counts and a call's usage, a message
//! in a conversation with a model, and vectors and the embeddings a model
//! makes. Storing a vector in pgvector, similarity and `ai.generate` come
//! with the AI area (roadmap 5.5v); until then a vector's storage is
//! [`Storage::Vector`], which a field can't use yet. What the page left
//! open is area 19's "Chosen in chunk 5.5s2".

use super::across::{Order, StandardRule, in_order};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::Builtin;
use crate::read::SqlType;

/// `TokenCount`.
pub const TOKEN_COUNT: &str = "TokenCount";
/// `TokenUsage`.
pub const TOKEN_USAGE: &str = "TokenUsage";
/// `ChatRole`, who wrote a `ChatMessage`.
pub const CHAT_ROLE: &str = "ChatRole";
/// `ChatMessage`.
pub const CHAT_MESSAGE: &str = "ChatMessage";
/// `Vector`.
pub const VECTOR: &str = "Vector";
/// `Embedding`.
pub const EMBEDDING: &str = "Embedding";

/// The most items a vector has: pgvector's limit for its `vector` column,
/// so every vector can be stored once the AI area stores them.
pub const MOST_DIMENSIONS: u64 = 16_000;

/// The categories of these types that are personal data whatever field
/// holds them (area 15): a message to or from a model is free text that
/// can hold anything a person says, their health included, so it takes
/// the sensitive category that fits such text (decided by the lead
/// session, 2026-10-18).
pub const PERSONAL: &[(&str, &str)] = &[(CHAT_MESSAGE, "health")];

/// The most a token count goes to: JavaScript's largest safe integer, so
/// it is always sent as a JSON number a client reads exactly.
const MOST_TOKENS: i64 = crate::http::SAFE_INTEGER as i64;

const fn examples(
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> Examples {
    Examples {
        valid,
        format: None,
        malformed,
        of: None,
        settings: "",
    }
}

/// How many tokens a prompt, an answer or a call's input holds, by the
/// model's own tokenizer: a whole number from 0, sent as a JSON number and
/// shown in the locale's digits.
pub const TOKEN_COUNT_ENTRY: StandardType = StandardType {
    name: TOKEN_COUNT,
    holds: Holds::Whole {
        min: 0,
        max: MOST_TOKENS,
    },
    text: Some(TextForm::Whole),
    json: Json::Number,
    storage: Storage::Column(SqlType::Int8),
    format: Format {
        key: FormatKey::Suffixed(""),
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[("0", "0"), ("350", "350"), ("128000", "128,000")],
        &["-1", "1.5", "1,200", "+5", "05", "", "9007199254740992"],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `TokenUsage`'s parts: the tokens a call read and wrote, and of those it
/// read, how many the provider's cache served, where it says.
pub const TOKEN_USAGE_FIELDS: [(&str, Part); 3] = [
    ("input", Part::Standard(TOKEN_COUNT)),
    ("output", Part::Standard(TOKEN_COUNT)),
    ("cached", Part::Optional(&Part::Standard(TOKEN_COUNT))),
];

/// `TokenUsage`'s rule across its parts (chunk 5.5t2): the cached tokens
/// are some of the input's.
pub const TOKEN_USAGE_RULES: &[StandardRule] = &[StandardRule {
    name: "cached_within_input",
    written: "cached <= input, where it is given",
    label: "more than the input",
    reason: "A `TokenUsage`'s `cached` tokens are the input tokens the provider's cache served, counted within its `input`, so they are never more.",
    next: "Write a `cached` count at or below the `input`",
    check: in_order(&[Order::at_most("cached", "input")], "cached"),
}];

/// The tokens one call to a model used: sent as named fields and stored a
/// column each, its total `input + output`, which isn't kept.
pub const TOKEN_USAGE_ENTRY: StandardType = StandardType {
    name: TOKEN_USAGE,
    holds: Holds::Object(&TOKEN_USAGE_FIELDS),
    text: None,
    json: Json::Fields(&["input", "output", "cached"]),
    storage: Storage::Columns {
        parts: &[
            ("input", "bigint"),
            ("output", "bigint"),
            ("cached", "bigint"),
        ],
        fixed: None,
    },
    format: Format {
        key: FormatKey::ByLabel(&[
            ("input", "input"),
            ("output", "output"),
            ("cached", "cached"),
        ]),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"input":1200,"output":350,"cached":null}"#,
                "input 1,200, output 350",
            ),
            (
                r#"{"input":1200,"output":350,"cached":800}"#,
                "input 1,200, output 350, cached 800",
            ),
            (
                r#"{"input":0,"output":0,"cached":0}"#,
                "input 0, output 0, cached 0",
            ),
        ],
        &[
            r#"{"input":1200,"output":350,"cached":1500}"#,
            r#"{"input":-1,"output":350,"cached":null}"#,
            r#"{"input":1200,"output":350}"#,
            r#"{"input":1200,"output":350,"cached":null,"total":1550}"#,
            r#"{"input":1.5,"output":350,"cached":null}"#,
            r#""1200/350""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `ChatRole`'s variants, each its own name, as the providers' APIs write
/// a message's `role`.
pub const CHAT_ROLES: &[(&str, &str)] = &[
    ("system", "system"),
    ("user", "user"),
    ("assistant", "assistant"),
    ("tool", "tool"),
];

/// Who wrote a message in a conversation with a model: the application's
/// instructions, the person, the model, or a tool's result.
pub const CHAT_ROLE_ENTRY: StandardType = StandardType {
    name: CHAT_ROLE,
    holds: Holds::Enum(CHAT_ROLES),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("system", "system"),
            ("user", "user"),
            ("assistant", "assistant"),
            ("tool", "tool"),
        ],
        &["developer", "model", "human", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `ChatMessage`'s parts: who wrote it and what it says, as text.
pub const CHAT_MESSAGE_FIELDS: [(&str, Part); 2] = [
    ("role", Part::Standard(CHAT_ROLE)),
    ("content", Part::Of(Builtin::Text)),
];

/// One message in a conversation with a model, in the shape the major
/// providers' APIs share, `{"role": "user", "content": "..."}`: sent as
/// named fields, stored a column each, and shown after its role.
pub const CHAT_MESSAGE_ENTRY: StandardType = StandardType {
    name: CHAT_MESSAGE,
    holds: Holds::Object(&CHAT_MESSAGE_FIELDS),
    text: None,
    json: Json::Fields(&["role", "content"]),
    storage: Storage::Columns {
        parts: &[("role", "text"), ("content", "text")],
        fixed: None,
    },
    format: Format {
        key: FormatKey::Parts {
            parts: &["role", "content"],
            separator: ": ",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"role":"system","content":"Answer in British English."}"#,
                "system: Answer in British English.",
            ),
            (
                r#"{"role":"user","content":"Summarise this page.\nThanks"}"#,
                "user: Summarise this page.\nThanks",
            ),
            (r#"{"role":"assistant","content":""}"#, "assistant: "),
        ],
        &[
            r#"{"role":"developer","content":"Answer briefly."}"#,
            r#"{"role":"user"}"#,
            r#"{"role":"user","content":[{"type":"text","text":"Summarise this page."}]}"#,
            r#"{"role":"user","content":"Summarise this page.","name":"someone"}"#,
            r#""user: Summarise this page.""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A list of numbers, each a single-precision float, as a model's
/// embedding is: from 1 to [`MOST_DIMENSIONS`] of them, sent as a JSON
/// array of numbers and shown in the locale's digits. A type built on it
/// fixes how many with `dimensions:`.
pub const VECTOR_ENTRY: StandardType = StandardType {
    name: VECTOR,
    holds: Holds::List(Part::Float),
    text: None,
    json: Json::Array,
    storage: Storage::Vector,
    format: Format {
        key: FormatKey::Vector,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("[0.5,-0.25,1]", "[0.5; -0.25; 1]"),
            ("[0.0123,-0.00456,0.789]", "[0.0123; -0.00456; 0.789]"),
            ("[1250.5]", "[1,250.5]"),
        ],
        &[
            "[]",
            r#"["0.5"]"#,
            "[0.5,null]",
            "[1e39]",
            "[-1e39]",
            r#"{"vector":[0.5]}"#,
            "0.5",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Embedding`'s parts: the model that made it and its vector, since
/// vectors from different models can't be compared.
pub const EMBEDDING_FIELDS: [(&str, Part); 2] = [
    ("model", Part::Standard(super::records::MODEL_ID)),
    ("vector", Part::Standard(VECTOR)),
];

/// A vector with the model that made it: sent as named fields, its model
/// as `ModelId`'s text, and stored with its vector in pgvector once the AI
/// area does.
pub const EMBEDDING_ENTRY: StandardType = StandardType {
    name: EMBEDDING,
    holds: Holds::Object(&EMBEDDING_FIELDS),
    text: None,
    json: Json::Fields(&["model", "vector"]),
    storage: Storage::Vector,
    format: Format {
        key: FormatKey::Vector,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"model":"openai/text-embedding-3-small","vector":[0.5,-0.25,1]}"#,
                "openai/text-embedding-3-small [0.5; -0.25; 1]",
            ),
            (
                r#"{"model":"ollama/nomic-embed-text","vector":[0.0123]}"#,
                "ollama/nomic-embed-text [0.0123]",
            ),
        ],
        &[
            r#"{"model":"text-embedding-3-small","vector":[0.5]}"#,
            r#"{"model":"openai/text-embedding-3-small","vector":[]}"#,
            r#"{"model":"openai/text-embedding-3-small"}"#,
            r#"[0.5,-0.25,1]"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// The rules a `Vector` keeps as a type: from 1 to [`MOST_DIMENSIONS`]
/// items, each kept as the nearest single-precision float ([`float`]),
/// which its name as `standard` marks.
pub fn vector_rules() -> crate::Rules {
    crate::Rules {
        min_length: Some(1),
        max_length: Some(MOST_DIMENSIONS),
        standard: Some(VECTOR),
        ..crate::Rules::default()
    }
}

/// Whether a value of standard type `name` keeps each of its items as the
/// nearest single-precision float as it is built: a `Vector`'s, rounded
/// rather than refused, so building one never breaks `format`.
pub fn rounds(name: &str) -> bool {
    name == VECTOR
}

/// Below this size, a vector's item is held as 0: the nearest float has
/// more places than a decimal's 38 (chosen in chunk 5.5s2).
const SMALLEST: f32 = 1e-30;

/// A vector's item as it is kept, from a decimal number written as area 2
/// writes a `Decimal` (`-0.25`), or from JSON's number as Rust's `f64`
/// writes it: the single-precision float nearest it, rounded half to even
/// as IEEE 754 rounds, written as the shortest decimal that is that float,
/// with no exponent and `0` for either zero or anything below `1e-30`.
/// `None` when the nearest float is past a decimal's 38 digits (`1e38` or
/// more) or isn't finite.
pub fn float(text: &str) -> Option<String> {
    let nearest: f32 = text.parse().ok()?;
    if !nearest.is_finite() {
        return None;
    }
    if nearest.abs() < SMALLEST {
        return Some("0".to_string());
    }
    let written = format!("{nearest}");
    let digits = written
        .trim_start_matches('-')
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    let whole = written
        .trim_start_matches('-')
        .split('.')
        .next()
        .unwrap_or_default();
    if whole.len() > 38 || digits.trim_start_matches('0').len() > 38 {
        return None;
    }
    Some(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_is_the_nearest_single_precision_float() {
        assert_eq!(float("0.5").as_deref(), Some("0.5"));
        assert_eq!(float("-0.25").as_deref(), Some("-0.25"));
        assert_eq!(float("1").as_deref(), Some("1"));
        // More places than a float holds: rounded to the nearest.
        assert_eq!(float("0.123456789").as_deref(), Some("0.12345679"));
        // An f32 written out as an f64 is that f32 again.
        assert_eq!(
            float("-0.006929283495992422").as_deref(),
            Some("-0.0069292835")
        );
        assert_eq!(float("-0").as_deref(), Some("0"));
        assert_eq!(float("1e-31").as_deref(), Some("0"));
        assert_eq!(
            float("1e-29").as_deref(),
            Some("0.00000000000000000000000000001")
        );
        assert_eq!(float("1e39"), None);
        assert_eq!(float("4e38"), None);
        assert_eq!(float("NaN"), None);
        assert_eq!(float("inf"), None);
        // 1e38 has 39 digits, one past a decimal's.
        assert_eq!(float("1e38"), None);
        assert!(float("9e37").is_some());
    }
}
