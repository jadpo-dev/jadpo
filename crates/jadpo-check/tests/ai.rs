//! Area 19, the AI types (chunk 5.5s2), in the checker: a vector type fixes
//! its length with `dimensions:`, proved for a list written out and broken
//! by name otherwise; `dimensions:` fits only a vector type, from 1 to
//! 16,000, and a type built on one keeps it; a usage's cached tokens are
//! within its input; a message is built from its role and its text; none of
//! the records has a single text form; and storing a vector or an embedding
//! waits on the AI area, while counts, usages and messages store today.

use crate::support;

use jadpo_db::{Database, Layout};

const NAMES: &str = "type Wire = Text {}\n\n/// The numbers a model sent.\ntype ModelNumbers = List<Decimal> {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype WrongSize = Failure {\n    kind: FailureKind.invalid_value\n}\n\n/// A help article's embedding, as the support site's model makes it.\ntype ArticleVector = Vector {\n    dimensions: 3\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_vector_type_s_length_is_proved_for_a_list_written_out() {
    assert_eq!(
        rules(
            "function sample() -> ArticleVector {\n    return ArticleVector([0.5, -0.25, 1.0])\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules("function sample() -> ArticleVector {\n    return ArticleVector([0.5, -0.25])\n}\n"),
        ["type.invalid_literal"],
        "two items aren't three"
    );
    // A list only the running program knows breaks `dimensions` by name.
    assert_eq!(
        rules(
            "function from_model(numbers: ModelNumbers) fails WrongSize -> ArticleVector {\n    return attempt ArticleVector(numbers) {\n        dimensions: WrongSize\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function from_model(numbers: ModelNumbers) -> ArticleVector {\n    return ArticleVector(numbers)\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // A plain vector keeps 1 to 16,000 items.
    assert_eq!(
        rules("function sample() -> Vector {\n    return Vector([0.5])\n}\n"),
        NONE
    );
    assert_eq!(
        rules(
            "function any(numbers: ModelNumbers) fails WrongSize -> Vector {\n    return attempt Vector(numbers) {\n        min_length: WrongSize\n        max_length: WrongSize\n    }\n}\n"
        ),
        NONE
    );
    // A vector of the type's own length needs no arm.
    assert_eq!(
        rules(
            "function same(vector: ArticleVector) -> ArticleVector {\n    return ArticleVector(vector)\n}\n"
        ),
        NONE
    );
    // An item is a decimal to code.
    assert_eq!(
        rules(
            "/// What a vector's items add up to.\ntype Weight = Decimal {\n    scale: 9\n}\n\nfunction weight(vector: ArticleVector) -> Weight {\n    return Weight(list.sum(vector))\n}\n"
        ),
        NONE
    );
}

#[test]
fn dimensions_fit_only_a_vector_type() {
    let declared = |block: &str| rules(&format!("/// A size.\ntype Sized = {block}\n"));
    assert_eq!(declared("Vector {\n    dimensions: 1536\n}"), NONE);
    assert_eq!(
        declared("Vector {\n    dimensions: 16000\n}"),
        NONE,
        "pgvector's most"
    );
    assert_eq!(
        declared("Vector {\n    dimensions: 16001\n}"),
        ["type.setting_value"]
    );
    assert_eq!(
        declared("Vector {\n    dimensions: 0\n}"),
        ["type.setting_value"]
    );
    assert_eq!(
        declared("Vector {\n    dimensions: 3.0\n}"),
        ["type.setting_value"]
    );
    assert_eq!(
        declared("List<Decimal> {\n    dimensions: 3\n}"),
        ["type.constraint_base"]
    );
    assert_eq!(
        declared("Text {\n    dimensions: 3\n}"),
        ["type.constraint_base"]
    );
    // Its lengths may narrow a vector's, never loosen them.
    assert_eq!(
        declared("Vector {\n    min_length: 256\n    max_length: 3072\n}"),
        NONE
    );
    assert_eq!(
        declared("Vector {\n    max_length: 20000\n}"),
        ["type.constraint_loosened"]
    );
    assert_eq!(
        declared("Vector {\n    dimensions: 3\n    max_length: 2\n}"),
        ["type.constraint_contradiction"]
    );
    // A type built on one keeps its length.
    assert_eq!(declared("ArticleVector {\n    dimensions: 3\n}"), NONE);
    assert_eq!(
        declared("ArticleVector {\n    dimensions: 4\n}"),
        ["type.constraint_contradiction"]
    );
}

#[test]
fn a_usage_s_cached_tokens_are_within_its_input() {
    let usage = |input: &str, cached: &str| {
        rules(&format!(
            "function used() -> TokenUsage {{\n    return TokenUsage {{\n        input: {input}\n        output: 350\n        cached: {cached}\n    }}\n}}\n"
        ))
    };
    assert_eq!(usage("1200", "800"), NONE);
    assert_eq!(usage("1200", "none"), NONE, "a provider that doesn't say");
    assert_eq!(usage("1200", "1200"), NONE);
    assert_eq!(usage("1200", "1500"), ["type.invalid_literal"]);
    assert_eq!(usage("-1", "none"), ["type.invalid_literal"]);
    assert_eq!(
        rules(
            "/// A call's tokens in all.\ntype Total = Int {}\n\nfunction total(used: TokenUsage) -> Total {\n    return Total(used.input + used.output)\n}\n"
        ),
        NONE,
        "the total is the sum, worked out where it is needed"
    );
}

#[test]
fn a_message_is_its_role_and_its_text() {
    assert_eq!(
        rules(
            "function asked() -> ChatMessage {\n    return ChatMessage {\n        role: ChatRole.user\n        content: \"Which soups are vegan?\"\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "function asked() -> ChatMessage {\n    return ChatMessage {\n        role: ChatRole.developer\n        content: \"Answer briefly.\"\n    }\n}\n"
        ),
        ["type.unknown_enum_variant"]
    );
}

#[test]
fn the_records_have_no_text_of_their_own() {
    for ty in ["Vector", "Embedding", "TokenUsage", "ChatMessage"] {
        assert_eq!(
            rules(&format!(
                "function sent(value: {ty}) -> Wire {{\n    return Wire(text.from(value))\n}}\n"
            )),
            ["type.mismatch"],
            "{ty}"
        );
    }
    // A count is a whole number, read and written as one.
    assert_eq!(
        rules("function sent(count: TokenCount) -> Wire {\n    return Wire(text.from(count))\n}\n"),
        NONE
    );
}

/// The rules a project of `source` gives, checked whole.
fn checked(source: &str) -> Vec<String> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

#[test]
fn a_vector_is_stored_in_pgvector() {
    for ty in ["Vector", "Embedding", "ArticleVector"] {
        assert_eq!(
            checked(&format!(
                "/// A help article's embedding.\ntype ArticleVector = Vector {{\n    dimensions: 3\n}}\n\nentity Article {{\n    id: Uuid\n    meaning: {ty}\n}}\n"
            )),
            NONE,
            "{ty}"
        );
    }
    assert_eq!(
        checked(
            "entity Reply {\n    id: Uuid\n    tokens: TokenCount\n    usage: TokenUsage\n    message: ChatMessage\n}\n"
        ),
        NONE
    );
}

/// A project whose `app.jadpo` declares a Redis connection, `drafts`, and
/// whose entity holds `field`.
fn on_redis(field: &str) -> Vec<String> {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &[
            (
                "app.jadpo",
                "application {\n    connections: {\n        drafts: {\n            driver: Driver.redis\n            url: config.redis_url\n        }\n    }\n}\n\nconfig {\n    redis_url: RedisUrl\n}\n",
            ),
            (
                "domains/search/types/query_meaning.jadpo",
                "/// A search's meaning, as the configured model makes it.\ntype QueryMeaning = Vector {\n    dimensions: 3\n}\n",
            ),
            (
                "domains/search/entities/recent_search.jadpo",
                &format!(
                    "use search/types {{ QueryMeaning }}\n\n/// A search someone made in the last hour.\nentity RecentSearch {{\n    id: Uuid\n    {field}\n\n    store: drafts\n}}\n"
                ),
            ),
        ],
    );
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

#[test]
fn a_vector_needs_postgresql() {
    assert_eq!(
        on_redis("meaning: QueryMeaning"),
        ["data.vector_store_unsupported"]
    );
    assert_eq!(
        on_redis("meaning: Embedding?\n    other: QueryMeaning"),
        ["data.vector_store_unsupported"],
        "said once, of the first"
    );
    assert_eq!(
        on_redis("words: Text\n    other: QueryMeaning?"),
        ["data.vector_store_unsupported"],
        "one that may be none"
    );
}

#[test]
fn a_vector_type_says_how_its_vectors_are_compared() {
    let declared = |block: &str| rules(&format!("/// A size.\ntype Sized = {block}\n"));
    for distance in ["cosine", "euclidean", "inner_product"] {
        assert_eq!(
            declared(&format!(
                "Vector {{\n    dimensions: 3\n    distance: VectorDistance.{distance}\n}}"
            )),
            NONE,
            "{distance}"
        );
    }
    assert_eq!(
        declared("Vector {\n    distance: VectorDistance.manhattan\n}"),
        ["type.setting_value"]
    );
    assert_eq!(
        declared("Vector {\n    distance: cosine\n}"),
        ["type.setting_value"]
    );
    assert_eq!(
        declared("List<Decimal> {\n    distance: VectorDistance.cosine\n}"),
        ["type.constraint_base"]
    );
    // A type built on one keeps it.
    assert_eq!(
        rules(
            "/// Compared by angle.\ntype ByAngle = Vector {\n    distance: VectorDistance.cosine\n}\n\n/// The same, narrowed.\ntype Narrowed = ByAngle {\n    dimensions: 3\n    distance: VectorDistance.cosine\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "/// Compared by angle.\ntype ByAngle = Vector {\n    distance: VectorDistance.cosine\n}\n\n/// Compared another way.\ntype ByLength = ByAngle {\n    distance: VectorDistance.euclidean\n}\n"
        ),
        ["type.constraint_contradiction"]
    );
}

#[test]
fn a_distance_is_between_two_vectors_of_one_type() {
    let measured = |params: &str, call: &str| {
        rules(&format!(
            "/// Another model's vector.\ntype OtherVector = Vector {{\n    dimensions: 3\n}}\n\n/// How far apart.\ntype Apart = Decimal {{\n    scale: 9\n}}\n\nfunction apart({params}) -> Apart {{\n    return Apart({call})\n}}\n"
        ))
    };
    assert_eq!(
        measured("a: ArticleVector, b: ArticleVector", "ai.distance(a, b)"),
        NONE
    );
    assert_eq!(
        measured("a: Vector, b: Vector", "ai.distance(a, b)"),
        NONE,
        "plain vectors, compared by cosine"
    );
    assert_eq!(
        measured("a: ArticleVector, b: OtherVector", "ai.distance(a, b)"),
        ["type.mismatch"],
        "another type may be another model's"
    );
    assert_eq!(
        measured("a: ArticleVector, b: Vector", "ai.distance(a, b)"),
        ["type.mismatch"]
    );
    assert_eq!(
        measured("a: ArticleVector?, b: ArticleVector", "ai.distance(a, b)"),
        ["type.mismatch"],
        "a vector that may be none"
    );
    assert_eq!(
        measured("a: ModelNumbers, b: ArticleVector", "ai.distance(a, b)"),
        ["type.mismatch"],
        "a list of numbers isn't a vector"
    );
}
