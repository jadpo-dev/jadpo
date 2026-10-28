//! The AI types' baselines (area 19, "The standard types" and "AI
//! types"): token counts and a call's usage, a message
//! in a conversation with a model, and vectors and the embeddings a model
//! makes. A vector is stored in a pgvector `vector` column
//! ([`Storage::Vector`], chunk 5.5v1, area 21), and compared as its type's
//! `distance:` says ([`distance`]). What the page left open is settled in
//! area 19's "AI types" and area 21's "Storing a vector".

use super::across::{Order, StandardRule, in_order};
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::Builtin;
use crate::read::SqlType;

/// The standard namespace of the `ai` capability's operations and
/// `ai.distance` (area 21).
pub const AI: &str = "ai";

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
/// so every vector can be stored.
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
        fixed: &[],
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
        fixed: &[],
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
/// as `ModelId`'s text, and stored in three columns, its model's provider
/// and name as `ModelId` is stored and its vector in a pgvector `vector`
/// (chunk 5.5v1).
pub const EMBEDDING_ENTRY: StandardType = StandardType {
    name: EMBEDDING,
    holds: Holds::Object(&EMBEDDING_FIELDS),
    text: None,
    json: Json::Fields(&["model", "vector"]),
    storage: Storage::Columns {
        parts: &[
            ("model_provider", "text"),
            ("model_name", "text"),
            ("vector", "vector"),
        ],
        fixed: &[],
    },
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

/// The most dimensions pgvector indexes as a `vector`; a type of more, up
/// to [`MOST_HALF_INDEXED`], is indexed through a half-precision `halfvec`
/// (area 21, "The index"; chunk 5.5v1c).
pub const MOST_INDEXED: u64 = 2_000;

/// The most dimensions pgvector indexes as a half-precision `halfvec`.
pub const MOST_HALF_INDEXED: u64 = 4_000;

/// The largest item a half-precision float holds, `65504`: an item of a
/// vector indexed at half precision is held to it, either way, so its
/// index takes every vector its type holds (chunk 5.8c).
pub const MOST_HALF: f32 = 65_504.0;

/// The rule as written that a vector type indexed at half precision holds
/// its items to, as `min` and `max` (chunk 5.8c).
pub const HALF_MIN_WRITTEN: &str = "min: -65504";
pub const HALF_MAX_WRITTEN: &str = "max: 65504";

/// The rule as written that a vector compared by cosine breaks when all
/// its items are zero, as `distance` (chunk 5.8c).
pub const COSINE_WRITTEN: &str = "distance: VectorDistance.cosine";

/// Whether a vector type of `dimensions` is indexed at half precision, so
/// holds each item within [`MOST_HALF`] (chunk 5.8c).
pub fn held_at_half(dimensions: Option<u64>) -> bool {
    dimensions.is_some_and(|n| n > MOST_INDEXED && n <= MOST_HALF_INDEXED)
}

/// What a vector's items break beyond its length (chunk 5.8c): an item
/// past [`MOST_HALF`] when its type is indexed at half precision
/// (`min` or `max`, with the item's place), then all items zero when its
/// type compares by cosine, which needs a direction (`distance`). Items
/// are the floats the vector keeps.
pub fn holding(
    items: &[f32],
    dimensions: Option<u64>,
    metric: crate::VectorDistance,
) -> Option<(crate::RuleKey, Option<usize>)> {
    if held_at_half(dimensions) {
        for (at, item) in items.iter().enumerate() {
            if *item > MOST_HALF {
                return Some((crate::RuleKey::Max, Some(at)));
            }
            if *item < -MOST_HALF {
                return Some((crate::RuleKey::Min, Some(at)));
            }
        }
    }
    if metric == crate::VectorDistance::Cosine && items.iter().all(|item| *item == 0.0) {
        return Some((crate::RuleKey::Distance, None));
    }
    None
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

/// The distance between two vectors of one type as pgvector works it out
/// for `metric` (area 21, "Storing a vector"; chunk 5.5v1), so
/// `ai.distance` in code gives what its operator gives in a query: each
/// item a single-precision float, the sums kept in single precision item
/// by item as pgvector's C does, and what follows in double precision.
/// Cosine is one less `dot / sqrt(|a|² · |b|²)`, kept between -1 and 1
/// first (`<=>`); euclidean the square root of the summed squared
/// differences (`<->`); inner product the sum of the products, negated
/// (`<#>`). Smaller is always nearer.
///
/// `None` when there is no such number: vectors of different lengths, a
/// cosine with a vector of zeros (which has no direction, and which
/// pgvector answers with `NaN`), or a sum past a float's range (which
/// pgvector answers with infinity).
///
/// pgvector's compiler may add its items in another order (it vectorises
/// the loop) or fuse a multiply and an add, so on items whose products and
/// running sums aren't exact floats the two can differ in a float's last
/// place; on items that are (small integers, halves, quarters), they
/// agree exactly, which is what the tests hold them to.
pub fn distance(a: &[f32], b: &[f32], metric: crate::VectorDistance) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let found = match metric {
        crate::VectorDistance::Euclidean => {
            let mut sum = 0.0f32;
            for (x, y) in a.iter().zip(b) {
                let diff = x - y;
                sum += diff * diff;
            }
            f64::from(sum).sqrt()
        }
        crate::VectorDistance::InnerProduct => {
            let mut sum = 0.0f32;
            for (x, y) in a.iter().zip(b) {
                sum += x * y;
            }
            -f64::from(sum)
        }
        crate::VectorDistance::Cosine => {
            let (mut dot, mut norm_a, mut norm_b) = (0.0f32, 0.0f32, 0.0f32);
            for (x, y) in a.iter().zip(b) {
                dot += x * y;
                norm_a += x * x;
                norm_b += y * y;
            }
            let similarity = f64::from(dot) / (f64::from(norm_a) * f64::from(norm_b)).sqrt();
            if similarity.is_nan() {
                return None;
            }
            1.0 - similarity.clamp(-1.0, 1.0)
        }
    };
    found.is_finite().then_some(found)
}

/// The double a distance is compared with in SQL so that it meets the
/// comparison exactly when its decimal ([`distance_text`]), which
/// `ai.distance` gives in code, meets `bound` (chunk 5.8c): the largest
/// double whose decimal is at most `bound`, or below it when `strict`. A
/// distance meets `<= bound` when it is at most that double, `< bound`
/// when it is at most the strict one, and `>` and `>=` when it is above
/// them, so PostgreSQL never rounds a longer literal its own way. `None`
/// when `bound` isn't a number.
pub fn distance_bound(bound: &str, strict: bool) -> Option<f64> {
    let limit = crate::Num::parse(bound)?;
    let meets = |found: f64| -> bool {
        if found.is_infinite() {
            return found < 0.0;
        }
        match crate::Num::parse(&distance_text(found)) {
            Some(written) => {
                let order = written.compare(limit);
                if strict { order.is_lt() } else { order.is_le() }
            }
            // Past a decimal's digits: above every bound, or below.
            None => found < 0.0,
        }
    };
    // Whether a double meets it only gets less likely as the double grows,
    // so search the doubles in order: by their bits, negatives reversed.
    let key = |found: f64| {
        let bits = found.to_bits() as i64;
        if bits < 0 { bits ^ i64::MAX } else { bits }
    };
    let double = |key: i64| f64::from_bits((if key < 0 { key ^ i64::MAX } else { key }) as u64);
    let (mut low, mut high) = (key(f64::NEG_INFINITY), key(f64::MAX));
    if meets(double(high)) {
        return Some(f64::MAX);
    }
    // `low` meets, `high` doesn't.
    while i128::from(high) - i128::from(low) > 1 {
        let halfway = ((i128::from(low) + i128::from(high)) / 2) as i64;
        if meets(double(halfway)) {
            low = halfway;
        } else {
            high = halfway;
        }
    }
    Some(double(low))
}

/// A double-precision distance as the decimal it is: the shortest decimal
/// that reads back as the same double, as PostgreSQL writes a `float8`,
/// or, for one with more than a decimal's 38 places (only a distance below
/// `1e-21`), the double rounded to 38 places.
pub fn distance_text(found: f64) -> String {
    let found = if found == 0.0 { 0.0 } else { found };
    let written = format!("{found}");
    let places = written
        .split_once('.')
        .map_or(0, |(_, places)| places.len());
    if places <= 38 {
        return written;
    }
    let rounded = format!("{found:.38}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    match trimmed {
        "-0" | "" => "0".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::VectorDistance::{Cosine, Euclidean, InnerProduct};

    #[test]
    fn distances_are_worked_out_as_pgvector_works_them_out() {
        let a = [1.0f32, 2.0, 3.0];
        let b = [4.0f32, -5.0, 6.0];
        // dot 12, |a|² 14, |b|² 77: one less 12 / sqrt(1078).
        assert_eq!(distance(&a, &b, Cosine), Some(1.0 - 12.0 / 1078f64.sqrt()));
        // Differences -3, 7, -3: sqrt(67).
        assert_eq!(distance(&a, &b, Euclidean), Some(67f64.sqrt()));
        assert_eq!(distance(&a, &b, InnerProduct), Some(-12.0));
        // The same vector is no distance away, its opposite as far as can be.
        assert_eq!(distance(&a, &a, Cosine), Some(0.0));
        assert_eq!(distance(&a, &[-1.0, -2.0, -3.0], Cosine), Some(2.0));
        // A vector of zeros has no direction, and lengths must agree.
        assert_eq!(distance(&[0.0, 0.0], &[1.0, 0.0], Cosine), None);
        assert_eq!(distance(&[0.0, 0.0], &[1.0, 0.0], Euclidean), Some(1.0));
        assert_eq!(distance(&[1.0], &[1.0, 2.0], Euclidean), None);
        // Past a float's range there is no number.
        assert_eq!(distance(&[3e38], &[-3e38], Euclidean), None);
        assert_eq!(distance(&[3e38, 3e38], &[3e38, 3e38], InnerProduct), None);
    }

    #[test]
    fn a_distance_is_written_as_postgresql_writes_a_double() {
        assert_eq!(distance_text(-12.0), "-12");
        assert_eq!(distance_text(0.5), "0.5");
        assert_eq!(distance_text(-0.0), "0");
        assert_eq!(distance_text(67f64.sqrt()), "8.18535277187245");
        assert_eq!(distance_text(1e-20), "0.00000000000000000001");
        // Past 38 places, rounded half to even.
        assert_eq!(
            distance_text(1.25e-37),
            "0.00000000000000000000000000000000000012"
        );
        assert_eq!(distance_text(1e-40), "0");
    }

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
    #[test]
    fn a_distance_bound_is_the_double_code_compares_the_same() {
        // `ai.distance(a, b) <= 0.6` in code compares the distance's
        // decimal with 0.6; the double that agrees is 0.6's own, and the
        // next one up writes as more than 0.6.
        assert_eq!(distance_bound("0.6", false), Some(0.6));
        assert_eq!(distance_bound("0.6", true), Some(0.6f64.next_down()));
        // A longer literal PostgreSQL would round up to the next double,
        // whose decimal, 0.6000000000000001, is above it.
        assert_eq!(distance_bound("0.60000000000000005", false), Some(0.6));
        assert_eq!(distance_text(0.6f64.next_up()), "0.6000000000000001");
        // Past a decimal's 38 digits a distance is no decimal at all.
        let most =
            distance_bound("99999999999999999999999999999999999999", false).expect("a double");
        assert!(most > 9.9e37 && most < 1.0e38, "{most}");
        let below =
            distance_bound("-99999999999999999999999999999999999999", true).expect("a double");
        assert!(below < -9.9e37, "{below}");
        assert_eq!(distance_bound("not a number", false), None);
        // Whole numbers too.
        assert_eq!(distance_bound("1", false), Some(1.0));
        // Below zero: a distance so small it writes as 0 isn't below it.
        let under = distance_bound("0", true).expect("a double");
        assert!(under < 0.0 && under > -1e-37, "{under}");
    }

    #[test]
    fn a_vector_holds_its_items_to_its_index_and_its_metric() {
        use crate::{RuleKey, VectorDistance};
        let cosine = VectorDistance::Cosine;
        assert_eq!(
            holding(&[0.0, 0.0], Some(2), cosine),
            Some((RuleKey::Distance, None))
        );
        assert_eq!(
            holding(&[0.0, 0.0], Some(2), VectorDistance::Euclidean),
            None
        );
        assert_eq!(holding(&[0.0, 1.0], Some(2), cosine), None);
        // Past a half-precision float, only where the index is one.
        assert_eq!(holding(&[1.0, 70_000.0], Some(2), cosine), None);
        let long = |at: usize, item: f32| {
            let mut items = vec![0.5f32; 3072];
            items[at] = item;
            items
        };
        assert_eq!(holding(&long(7, 65_504.0), Some(3072), cosine), None);
        assert_eq!(
            holding(&long(7, 65_505.0), Some(3072), cosine),
            Some((RuleKey::Max, Some(7)))
        );
        assert_eq!(
            holding(&long(9, -70_000.0), Some(3072), cosine),
            Some((RuleKey::Min, Some(9)))
        );
        assert_eq!(holding(&long(9, -70_000.0), Some(4001), cosine), None);
    }
}
