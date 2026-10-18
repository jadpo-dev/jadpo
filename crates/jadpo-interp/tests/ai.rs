//! Area 19, the AI types (chunk 5.5s2), run: a vector built from decimals
//! keeps each as the nearest single-precision float, and is sent as a JSON
//! array of numbers that reads back as the same vector; a vector type's
//! length is checked where the running program builds one and where JSON is
//! read; numbers past what a float or a decimal holds, and JSON's missing
//! NaN and infinity, are refused; an embedding, a usage and a message are
//! sent as named fields; and each is shown to people. `ai.distance` gives
//! the distance its vectors' type compares them by, as pgvector works it
//! out (area 21, chunk 5.5v1).

use crate::support;

use jadpo_interp::Value;
use support::{decimal, failure, fault, int, list, main, text, value};

const AI: &str = "type Sent = Text {}

type Unreadable = Failure {
    kind: FailureKind.invalid_value
}

type WrongSize = Failure {
    kind: FailureKind.invalid_value
}

/// A help article's embedding, as the support site's model makes it.
type ArticleVector = Vector {
    dimensions: 3
}

/// The numbers a model sent.
type ModelNumbers = List<Decimal> {}

/// A vector written out, with more places than a float keeps.
function sample() -> ArticleVector {
    return ArticleVector([0.5, -0.25, 0.123456789])
}

/// A vector from a model's numbers.
function from_model(numbers: ModelNumbers) fails WrongSize -> ArticleVector {
    return attempt ArticleVector(numbers) {
        dimensions: WrongSize
    }
}

/// A vector as the search index is sent it.
function sent(vector: ArticleVector) -> Sent {
    return Sent(json.write(vector))
}

/// A vector from the search index.
function indexed(feed: Sent) fails Unreadable -> ArticleVector {
    return attempt json.read(feed, as: ArticleVector) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// An embedding from the model's answer.
function embedded(feed: Sent) fails Unreadable -> Embedding {
    return attempt json.read(feed, as: Embedding) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// An embedding as it is logged.
function embedding_sent(meaning: Embedding) -> Sent {
    return Sent(json.write(meaning))
}

/// A call's usage from the provider's answer.
function used(feed: Sent) fails Unreadable -> TokenUsage {
    return attempt json.read(feed, as: TokenUsage) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// A call's usage as it is billed.
function usage_sent(usage: TokenUsage) -> Sent {
    return Sent(json.write(usage))
}

/// The question a customer asked.
function asked() -> ChatMessage {
    return ChatMessage {
        role: ChatRole.user
        content: \"Which soups are vegan?\"
    }
}

/// A message as the model is sent it.
function message_sent(message: ChatMessage) -> Sent {
    return Sent(json.write(message))
}

/// A message from the model's answer.
function answered(feed: Sent) fails Unreadable -> ChatMessage {
    return attempt json.read(feed, as: ChatMessage) {
        malformed: Unreadable
        mismatch: Unreadable
    }
}

/// A message for the support team.
function message_shown(message: ChatMessage, reader: Locale) -> PresentationText {
    return text.format(message, locale: reader)
}

/// A usage for the billing page.
function usage_shown(usage: TokenUsage, reader: Locale) -> PresentationText {
    return text.format(usage, locale: reader)
}

/// A vector for a debugging page.
function vector_shown(vector: ArticleVector, reader: Locale) -> PresentationText {
    return text.format(vector, locale: reader)
}

/// A shop's position in a map of tastes, compared by straight-line
/// distance.
type TasteVector = Vector {
    dimensions: 3
    distance: VectorDistance.euclidean
}

/// A normalised score vector, compared by inner product.
type ScoreVector = Vector {
    dimensions: 3
    distance: VectorDistance.inner_product
}

/// How far apart two vectors are.
type Apart = Decimal {
    scale: 18
}

/// How far apart two articles are, by angle.
function articles_apart(a: ArticleVector, b: ArticleVector) -> Apart {
    return Apart(ai.distance(a, b))
}

/// How far apart two shops' tastes are.
function tastes_apart(a: TasteVector, b: TasteVector) -> Apart {
    return Apart(ai.distance(a, b))
}

/// How far apart two scores are.
function scores_apart(a: ScoreVector, b: ScoreVector) -> Apart {
    return Apart(ai.distance(a, b))
}

/// A count typed into the limits page.
function typed_limit(raw: Sent) fails Unreadable -> TokenCount {
    return attempt text.parse(raw, as: TokenCount) {
        malformed: Unreadable
    }
}
";

fn locale(name: &str) -> Value {
    let ordinal = jadpo_std::variant_ordinal(jadpo_std::LOCALE, name).expect("a locale");
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

#[test]
fn a_vector_keeps_each_item_as_the_nearest_float() {
    let program = main(AI).with_proof_checks();
    let sample = value(&program, "sample", vec![]);
    assert_eq!(
        sample,
        list(vec![
            decimal("0.5"),
            decimal("-0.25"),
            decimal("0.12345679")
        ])
    );
    let numbers = list(vec![
        decimal("-0.006929283495992422"),
        decimal("0"),
        decimal("1"),
    ]);
    assert_eq!(
        value(&program, "from_model", vec![numbers]),
        list(vec![decimal("-0.0069292835"), decimal("0"), decimal("1")])
    );
    assert_eq!(
        failure(
            &program,
            "from_model",
            vec![list(vec![decimal("0.5"), decimal("0.25")])]
        )
        .name,
        "WrongSize"
    );
}

#[test]
fn a_vector_round_trips_through_json_as_numbers() {
    let program = main(AI).with_proof_checks();
    let sample = value(&program, "sample", vec![]);
    let written = value(&program, "sent", vec![sample.clone()]);
    assert_eq!(written, text("[0.5,-0.25,0.12345679]"));
    assert_eq!(value(&program, "indexed", vec![written]), sample);
    // An f32 sent as a double, as most clients write one, reads as that
    // float, and is written back in its shortest form.
    let read = value(
        &program,
        "indexed",
        vec![text("[-0.006929283495992422,1e-5,2]")],
    );
    assert_eq!(
        value(&program, "sent", vec![read]),
        text("[-0.0069292835,0.00001,2]")
    );
}

#[test]
fn what_isn_t_a_vector_of_its_type_is_refused() {
    let program = main(AI).with_proof_checks();
    for feed in [
        "[0.5,-0.25]",
        "[0.5,-0.25,1,2]",
        "[]",
        r#"["0.5",-0.25,1]"#,
        "[0.5,null,1]",
        "[1e39,0,0]",
        "[-1e39,0,0]",
        "[NaN,0,0]",
        "[Infinity,0,0]",
        "0.5",
    ] {
        assert_eq!(
            failure(&program, "indexed", vec![text(feed)]).name,
            "Unreadable",
            "{feed}"
        );
    }
}

#[test]
fn an_embedding_usage_and_message_are_sent_as_named_fields() {
    let program = main(AI).with_proof_checks();
    let embedding = r#"{"model":"openai/text-embedding-3-small","vector":[0.5,-0.25,1]}"#;
    let read = value(&program, "embedded", vec![text(embedding)]);
    assert_eq!(
        value(&program, "embedding_sent", vec![read]),
        text(embedding)
    );
    assert_eq!(
        failure(
            &program,
            "embedded",
            vec![text(
                r#"{"model":"openai/text-embedding-3-small","vector":[]}"#
            )]
        )
        .name,
        "Unreadable"
    );
    let usage = r#"{"input":1200,"output":350,"cached":800}"#;
    let read = value(&program, "used", vec![text(usage)]);
    assert_eq!(value(&program, "usage_sent", vec![read]), text(usage));
    for refused in [
        r#"{"input":1200,"output":350,"cached":1500}"#,
        r#"{"input":-1,"output":350,"cached":null}"#,
        r#"{"input":1200,"output":350,"cached":null,"total":1550}"#,
    ] {
        assert_eq!(
            failure(&program, "used", vec![text(refused)]).name,
            "Unreadable",
            "{refused}"
        );
    }
    let asked = value(&program, "asked", vec![]);
    let written = value(&program, "message_sent", vec![asked.clone()]);
    assert_eq!(
        written,
        text(r#"{"role":"user","content":"Which soups are vegan?"}"#)
    );
    assert_eq!(value(&program, "answered", vec![written]), asked);
    assert_eq!(
        failure(
            &program,
            "answered",
            vec![text(r#"{"role":"developer","content":"Answer briefly."}"#)]
        )
        .name,
        "Unreadable"
    );
}

#[test]
fn each_is_shown_to_people() {
    let program = main(AI).with_proof_checks();
    let asked = value(&program, "asked", vec![]);
    assert_eq!(
        value(&program, "message_shown", vec![asked, locale("en_gb")]),
        text("user: Which soups are vegan?")
    );
    let usage = value(
        &program,
        "used",
        vec![text(r#"{"input":1200,"output":350,"cached":null}"#)],
    );
    assert_eq!(
        value(&program, "usage_shown", vec![usage, locale("en_gb")]),
        text("input 1,200, output 350")
    );
    let sample = value(&program, "sample", vec![]);
    assert_eq!(
        value(
            &program,
            "vector_shown",
            vec![sample.clone(), locale("en_gb")]
        ),
        text("[0.5; -0.25; 0.12345679]")
    );
    assert_eq!(
        value(&program, "vector_shown", vec![sample, locale("fr_fr")]),
        text("[0,5; -0,25; 0,12345679]")
    );
}

#[test]
fn a_typed_count_is_a_whole_number_from_0() {
    let program = main(AI).with_proof_checks();
    assert_eq!(
        value(&program, "typed_limit", vec![text("128000")]),
        int(128000)
    );
    for raw in ["-1", "1.5", "1,200", ""] {
        assert_eq!(
            failure(&program, "typed_limit", vec![text(raw)]).name,
            "Unreadable",
            "{raw}"
        );
    }
}

#[test]
fn a_distance_is_the_one_its_type_compares_by() {
    let program = main(AI).with_proof_checks();
    let vector = |items: [&str; 3]| list(items.iter().map(|item| decimal(item)).collect());
    let a = vector(["1", "2", "3"]);
    let b = vector(["4", "-5", "6"]);
    // One less 12 / sqrt(14 * 77), as pgvector's `<=>` gives it.
    let cosine = jadpo_std::standard::ai::distance_text(1.0 - 12.0 / 1078f64.sqrt());
    assert_eq!(
        value(&program, "articles_apart", vec![a.clone(), b.clone()]),
        decimal(&cosine)
    );
    assert_eq!(
        value(&program, "tastes_apart", vec![a.clone(), b.clone()]),
        decimal("8.18535277187245")
    );
    assert_eq!(
        value(&program, "scores_apart", vec![a.clone(), b.clone()]),
        decimal("-12")
    );
    assert_eq!(
        value(&program, "articles_apart", vec![a.clone(), a.clone()]),
        decimal("0")
    );
    // A vector of zeros has no direction, so no angle to another.
    let zero = vector(["0", "0", "0"]);
    assert_eq!(
        fault(&program, "articles_apart", vec![zero.clone(), a.clone()]).kind,
        jadpo_interp::FaultKind::Defect(
            "a cosine distance from a vector of zeros, which has no direction".into()
        )
    );
    assert_eq!(
        value(&program, "tastes_apart", vec![zero, a]),
        decimal(&jadpo_std::standard::ai::distance_text(14f64.sqrt()))
    );
}
