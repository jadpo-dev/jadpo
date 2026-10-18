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

// --- nearest-neighbour queries (area 21, chunk 5.5v1b) ---------------------------

/// Two vector types of one length, an entity holding each kind of vector,
/// and a result type picking its title.
const SEARCH: &str = "/// A help article's meaning, as the configured embedding model makes it.\ntype ArticleMeaning = Vector {\n    dimensions: 8\n}\n\n/// A question's meaning, as the older model made it.\ntype QuestionMeaning = Vector {\n    dimensions: 8\n}\n\n/// A long embedding.\ntype LongMeaning = Vector {\n    dimensions: 3072\n    distance: VectorDistance.euclidean\n}\n\n/// An embedding too long for any index.\ntype VeryLongMeaning = Vector {\n    dimensions: 5000\n}\n\n/// A title.\ntype Title = Text {\n    max_length: 80\n}\n\n/// How many answers to show.\ntype AnswerCount = Int {\n    min: 1\n    max: 20\n}\n\n/// More answers than a nearest query gives.\ntype ManyAnswers = Int {\n    min: 1\n    max: 500\n}\n\nentity HelpArticle {\n    id: Uuid\n    title: Title\n    shown_publicly: Bool\n    body_meaning: ArticleMeaning?\n    long_meaning: LongMeaning?\n    very_long_meaning: VeryLongMeaning?\n    loose: Vector?\n    embedding: Embedding?\n}\n\ntype ArticleExcerpt = Output {\n    pick: [HelpArticle.id, HelpArticle.title]\n}\n\n";

fn searched(source: &str) -> Vec<String> {
    checked(&format!("{SEARCH}{source}"))
}

/// A query `closest(meaning: <param>) -> <result>` with `lines` in its block.
fn nearest_query(param: &str, result: &str, lines: &str) -> String {
    format!("query closest(meaning: {param}) -> {result} {{\n    from: HelpArticle\n{lines}}}\n")
}

#[test]
fn a_nearest_query_orders_by_a_vector_and_returns_its_distance() {
    let source = nearest_query(
        "ArticleMeaning",
        "List<Nearest<ArticleExcerpt>>",
        "    where: HelpArticle.shown_publicly == true\n    where: ai.distance(HelpArticle.body_meaning, meaning) <= 0.6\n    order: [HelpArticle.body_meaning nearest meaning]\n    limit: 5\n",
    );
    assert_eq!(searched(&source), NONE);
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{SEARCH}{source}"))],
    );
    let queries = jadpo_check::data::queries::query_models(&db, project);
    let query = queries.get("closest").expect("the query");
    let nearest = query.nearest.as_ref().expect("a nearest order");
    assert_eq!(nearest.field, "body_meaning");
    assert_eq!(nearest.metric, jadpo_std::VectorDistance::Cosine);
    assert_eq!(nearest.most, 5);
    assert!(
        query.order.is_empty(),
        "no other order, not even the tie-break"
    );
    assert_eq!(query.element.ty, "ArticleExcerpt");
    assert!(matches!(
        query.filter.last(),
        Some(jadpo_check::data::queries::Condition::Distance {
            op: jadpo_check::data::queries::Compare::LessEqual,
            ..
        })
    ));
    // A value whose type has a `max:` of at most 100 bounds it too.
    assert_eq!(
        searched(
            "query closest(meaning: ArticleMeaning, count: AnswerCount) -> List<Nearest<HelpArticle>> {\n    from: HelpArticle\n    order: [HelpArticle.body_meaning nearest meaning]\n    limit: count\n}\n"
        ),
        NONE
    );
    // Code reads each record and its distance.
    assert_eq!(
        searched(&format!(
            "{}\n/// Whether a found article is close.\nfunction close(found: Nearest<ArticleExcerpt>) -> Close {{\n    return Close(found.distance <= 0.2)\n}}\n\n/// Whether an article is close to a question.\ntype Close = Bool {{}}\n\n/// A found article's title.\nfunction title_of(found: Nearest<ArticleExcerpt>) -> Title {{\n    return found.item.title\n}}\n",
            nearest_query(
                "ArticleMeaning",
                "List<Nearest<ArticleExcerpt>>",
                "    order: [HelpArticle.body_meaning nearest meaning]\n    limit: 5\n"
            )
        )),
        NONE
    );
}

#[test]
fn nearest_needs_a_vector_of_fixed_length() {
    for field in ["loose", "embedding"] {
        assert_eq!(
            searched(&nearest_query(
                if field == "loose" {
                    "Vector"
                } else {
                    "Embedding"
                },
                "List<Nearest<HelpArticle>>",
                &format!("    order: [HelpArticle.{field} nearest meaning]\n    limit: 5\n"),
            )),
            ["query.nearest_unindexed"],
            "{field}"
        );
    }
}

/// The query model of `closest` in `source`, after `SEARCH`.
fn closest_model(source: &str) -> jadpo_check::data::queries::QueryModel {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{SEARCH}{source}"))],
    );
    let queries = jadpo_check::data::queries::query_models(&db, project);
    queries.get("closest").expect("the query").clone()
}

/// The `help_article` table's indexes `source` derives or declares.
fn article_indexes(source: &str) -> std::collections::BTreeMap<String, jadpo_graph::schema::Index> {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &format!("{SEARCH}{source}"))],
    );
    let schema = jadpo_check::data::schema(&db, project);
    schema.stores["main"].tables["help_article"].indexes.clone()
}

#[test]
fn a_longer_vector_is_searched_at_half_precision() {
    // Area 21, "The index" (chunk 5.5v1c): 2,001 to 4,000 dimensions are
    // indexed through a `halfvec` expression.
    let source = nearest_query(
        "LongMeaning",
        "List<Nearest<HelpArticle>>",
        "    order: [HelpArticle.long_meaning nearest meaning]\n    limit: 5\n",
    );
    assert_eq!(searched(&source), NONE);
    let query = closest_model(&source);
    let nearest = query.nearest.as_ref().expect("a nearest order");
    assert_eq!(nearest.dimensions, 3072);
    assert_eq!(
        nearest.search,
        jadpo_check::data::queries::VectorSearch::HalfPrecision(3072)
    );
    let indexes = article_indexes(&source);
    let index = &indexes["help_article_long_meaning_idx"];
    assert_eq!(index.columns, ["long_meaning"]);
    let method = index.method.as_ref().expect("an HNSW index");
    assert_eq!(method.name, "hnsw");
    assert_eq!(method.operator_class, "halfvec_l2_ops");
    assert_eq!(method.cast.as_deref(), Some("halfvec(3072)"));
    assert!(method.with.is_empty());
}

#[test]
fn a_vector_too_long_to_index_is_searched_exactly_once_declined() {
    // Over 4,000 dimensions: the cost is named, and a decline accepts it.
    let lines = "    order: [HelpArticle.very_long_meaning nearest meaning]\n    limit: 5\n";
    let source = nearest_query("VeryLongMeaning", "List<Nearest<HelpArticle>>", lines);
    assert_eq!(searched(&source), ["query.nearest_exact"]);
    // The decline stands above the `order:` the finding is on.
    let declined = nearest_query(
        "VeryLongMeaning",
        "List<Nearest<HelpArticle>>",
        &format!(
            "    decline query.nearest_exact: \"A few hundred articles, read in full\"\n{lines}"
        ),
    );
    assert_eq!(searched(&declined), NONE);
    let query = closest_model(&declined);
    let nearest = query.nearest.as_ref().expect("a nearest order");
    assert_eq!(
        nearest.search,
        jadpo_check::data::queries::VectorSearch::Exact
    );
    // No index is derived for it.
    let indexes = article_indexes(&declined);
    assert!(
        indexes.values().all(|index| index.method.is_none()),
        "{indexes:#?}"
    );
}

/// `SEARCH` with an `indexes:` section of `entries` on `HelpArticle`, and
/// a query searching `field`.
fn tuned(entries: &str, field: &str) -> String {
    let with_indexes = SEARCH.replace(
        "    embedding: Embedding?\n}\n",
        &format!("    embedding: Embedding?\n\n    indexes: {{\n{entries}    }}\n}}\n"),
    );
    let ty = match field {
        "long_meaning" => "LongMeaning",
        "very_long_meaning" => "VeryLongMeaning",
        _ => "ArticleMeaning",
    };
    format!(
        "{with_indexes}{}",
        nearest_query(
            ty,
            "List<Nearest<HelpArticle>>",
            &format!("    order: [HelpArticle.{field} nearest meaning]\n    limit: 5\n"),
        )
    )
}

/// An `indexes:` entry `by_meaning` on `field` with `settings`.
fn hnsw_entry(field: &str, settings: &str) -> String {
    format!(
        "        by_meaning: {{\n            fields: [{field}]\n            kind: IndexKind.hnsw\n{settings}        }}\n"
    )
}

#[test]
fn a_tuned_hnsw_index_sets_m_and_ef_construction() {
    let source = tuned(
        &hnsw_entry(
            "body_meaning",
            "            m: 24\n            ef_construction: 128\n",
        ),
        "body_meaning",
    );
    assert_eq!(checked(&source), NONE);
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let schema = jadpo_check::data::schema(&db, project);
    let indexes = &schema.stores["main"].tables["help_article"].indexes;
    let hnsw: Vec<(&String, &jadpo_graph::schema::Index)> = indexes
        .iter()
        .filter(|(_, index)| index.method.is_some())
        .collect();
    assert_eq!(
        hnsw.len(),
        1,
        "the declared index serves the query: {indexes:#?}"
    );
    let (name, index) = hnsw[0];
    assert_eq!(name, "help_article_by_meaning_idx");
    let method = index.method.as_ref().expect("a method");
    assert_eq!(method.operator_class, "vector_cosine_ops");
    assert_eq!(
        method.with,
        [("m".to_string(), 24), ("ef_construction".to_string(), 128)]
    );
    // At half precision too, and with neither setting.
    let source = tuned(
        &hnsw_entry("long_meaning", "            ef_construction: 200\n"),
        "long_meaning",
    );
    assert_eq!(checked(&source), NONE);
    let source = tuned(&hnsw_entry("body_meaning", ""), "body_meaning");
    assert_eq!(checked(&source), NONE);
    // The bounds pgvector takes.
    for (settings, why) in [
        (
            "            m: 2\n            ef_construction: 4\n",
            "the least",
        ),
        (
            "            m: 100\n            ef_construction: 1000\n",
            "the most",
        ),
        ("            m: 32\n", "twice 32 is the default 64"),
        (
            "            m: 16\n            ef_construction: 32\n",
            "exactly twice",
        ),
    ] {
        assert_eq!(
            checked(&tuned(
                &hnsw_entry("body_meaning", settings),
                "body_meaning"
            )),
            NONE,
            "{why}"
        );
    }
}

#[test]
fn an_hnsw_index_s_tuning_stays_within_pgvector_s_bounds() {
    for (settings, why) in [
        ("            m: 1\n", "m below 2"),
        (
            "            m: 101\n            ef_construction: 1000\n",
            "m above 100",
        ),
        (
            "            ef_construction: 3\n",
            "ef_construction below 4",
        ),
        (
            "            ef_construction: 1001\n",
            "ef_construction above 1,000",
        ),
        ("            m: 16.5\n", "not a whole number"),
        ("            m: \"16\"\n", "text"),
        (
            "            m: 16\n            ef_construction: 31\n",
            "less than twice m",
        ),
        (
            "            m: 33\n",
            "more than half the default ef_construction",
        ),
        (
            "            ef_construction: 20\n",
            "less than twice the default m",
        ),
    ] {
        assert_eq!(
            checked(&tuned(
                &hnsw_entry("body_meaning", settings),
                "body_meaning"
            )),
            ["type.setting_value"],
            "{why}"
        );
    }
    assert_eq!(
        checked(&tuned(
            &hnsw_entry("body_meaning", "            m: 16\n            m: 24\n"),
            "body_meaning"
        )),
        ["data.duplicate_rule"],
        "m twice"
    );
    // Only an HNSW index has them.
    assert_eq!(
        checked(&tuned(
            "        by_title: {\n            fields: [title]\n            m: 24\n        }\n",
            "body_meaning"
        )),
        ["type.unknown_setting"]
    );
    // On one vector field of fixed length, of at most 4,000 dimensions.
    for (field, why) in [
        ("title", "not a vector"),
        ("loose", "no fixed length"),
        ("embedding", "an embedding"),
        ("very_long_meaning", "too long"),
        ("body_meaning, long_meaning", "two fields"),
    ] {
        assert_eq!(
            checked(&tuned(&hnsw_entry(field, ""), "body_meaning")),
            ["type.setting_value"],
            "{why}"
        );
    }
}

#[test]
fn nearest_measures_only_from_a_vector_of_the_field_s_type() {
    let one =
        |param: &str, result: &str, lines: &str| searched(&nearest_query(param, result, lines));
    let order = "    order: [HelpArticle.body_meaning nearest meaning]\n    limit: 5\n";
    // Another vector type, even of the same length.
    assert_eq!(
        one("QuestionMeaning", "List<Nearest<HelpArticle>>", order),
        ["query.nearest_type"]
    );
    // A field that isn't a vector.
    assert_eq!(
        one(
            "Title",
            "List<Nearest<HelpArticle>>",
            "    order: [HelpArticle.title nearest meaning]\n    limit: 5\n"
        ),
        ["query.nearest_type"]
    );
    // Another order beside it.
    assert_eq!(
        one(
            "ArticleMeaning",
            "List<Nearest<HelpArticle>>",
            "    order: [HelpArticle.body_meaning nearest meaning, HelpArticle.title asc]\n    limit: 5\n"
        ),
        ["query.nearest_type"]
    );
    // A result that isn't `List<Nearest<T>>`, and `Nearest<T>` without it.
    assert_eq!(
        one("ArticleMeaning", "List<HelpArticle>", order),
        ["query.nearest_type"]
    );
    assert_eq!(
        one(
            "ArticleMeaning",
            "List<Nearest<HelpArticle>>",
            "    order: [HelpArticle.title asc]\n    limit: 5\n"
        ),
        ["query.nearest_type"]
    );
    // A value that may be absent.
    assert_eq!(
        one("ArticleMeaning?", "List<Nearest<HelpArticle>>", order),
        ["query.optional_value"]
    );
}

#[test]
fn nearest_records_come_in_one_list_of_at_most_100() {
    assert_eq!(
        searched(
            "query closest(meaning: ArticleMeaning, after: PageCursor?) -> Page<Nearest<HelpArticle>> {\n    from: HelpArticle\n    order: [HelpArticle.body_meaning nearest meaning]\n    page: {\n        size: 20\n        after: after\n    }\n}\n"
        ),
        ["query.nearest_page"]
    );
    assert_eq!(
        searched(&nearest_query(
            "ArticleMeaning",
            "List<Nearest<HelpArticle>>",
            "    order: [HelpArticle.body_meaning nearest meaning]\n    limit: 101\n",
        )),
        ["query.limit_range"]
    );
    assert_eq!(
        searched(
            "query closest(meaning: ArticleMeaning, count: ManyAnswers) -> List<Nearest<HelpArticle>> {\n    from: HelpArticle\n    order: [HelpArticle.body_meaning nearest meaning]\n    limit: count\n}\n"
        ),
        ["query.limit_range"]
    );
}

#[test]
fn a_distance_in_a_query_is_compared_with_a_bound() {
    let line = |condition: &str| {
        searched(&nearest_query(
            "ArticleMeaning",
            "List<HelpArticle>",
            &format!("    where: {condition}\n    order: [HelpArticle.title asc]\n    limit: 5\n"),
        ))
    };
    assert_eq!(
        line("ai.distance(HelpArticle.body_meaning, meaning) > 0.25"),
        NONE
    );
    assert_eq!(
        line("not (ai.distance(HelpArticle.body_meaning, meaning) <= 0.25)"),
        NONE
    );
    // Never for equality, field first, and of one type.
    assert_eq!(
        line("ai.distance(HelpArticle.body_meaning, meaning) == 0.25"),
        ["query.condition_unsupported"]
    );
    assert_eq!(
        line("ai.distance(meaning, HelpArticle.body_meaning) <= 0.25"),
        ["query.condition_unsupported"]
    );
    assert_eq!(
        searched(&nearest_query(
            "QuestionMeaning",
            "List<HelpArticle>",
            "    where: ai.distance(HelpArticle.body_meaning, meaning) <= 0.25\n    order: [HelpArticle.title asc]\n    limit: 5\n",
        )),
        ["type.mismatch"]
    );
    assert_eq!(
        searched(&nearest_query(
            "ArticleMeaning?",
            "List<HelpArticle>",
            "    where_present: ai.distance(HelpArticle.body_meaning, meaning) <= 0.25\n    order: [HelpArticle.title asc]\n    limit: 5\n",
        )),
        ["query.condition_unsupported"]
    );
}
