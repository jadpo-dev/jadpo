//! Area 21, "Prompts as files" (chunk 5.5v2), in the checker: a prompt is
//! a `type X = Prompt { ... }` in a domain's `prompts/` folder (or
//! `shared/prompts/`) with its words in `<name>.prompt` beside it, in two
//! sections; its placeholders are checked as a translation's are, free
//! text only under `# Request`, a `List<ChatMessage>` field as the turns,
//! and `prompts.render` gives the messages; each prompt is in the manifest
//! with its digest.

use jadpo_db::{Database, Layout};

use crate::support;

const APP: (&str, &str) = ("app.jadpo", "application {}\n");

const SLUG: (&str, &str) = (
    "domains/support/types/article_slug.jadpo",
    "/// A help article's address.\ntype ArticleSlug = Text {\n    max_length: 120\n}\n",
);

const EXCERPT: (&str, &str) = (
    "domains/support/types/article_excerpt.jadpo",
    "use support/types { ArticleSlug }\n\n/// The part of a help article a question is answered from.\ntype ArticleExcerpt = Object {\n    slug: ArticleSlug\n    title: Text\n    excerpt: Text\n}\n",
);

const PROMPT_TYPE: &str = "use support/types { ArticleExcerpt }

/// Answers a customer's question using only the help articles found for it.
type AnswerQuestion = Prompt {
    question: Text
    articles: List<ArticleExcerpt>
    reader_locale: Locale
}
";

const PROMPT_TEXT: &str = "# Instructions

You answer questions from customers of an online bakery, using only the
help articles given with the question. If they don't answer it, set
resolved_by_articles to false and say a person will reply. Write in the
language of {reader_locale}. Never promise refunds or delivery dates.

# Request

{question}

{articles}
";

const TYPE_PATH: &str = "domains/support/prompts/answer_question.jadpo";
const TEXT_PATH: &str = "domains/support/prompts/answer_question.prompt";

/// The page's project with the prompt's type and text given.
fn diagnostics(
    prompt_type: &str,
    prompt_text: Option<&str>,
    more: &[(&str, &str)],
) -> Vec<jadpo_diag::Diagnostic> {
    let db = Database::default();
    let mut files = vec![APP, SLUG, EXCERPT, (TYPE_PATH, prompt_type)];
    files.extend_from_slice(more);
    let records: Vec<(&str, &str)> = prompt_text
        .map(|text| (TEXT_PATH, text))
        .into_iter()
        .collect();
    let project = support::project_with_records(&db, Layout::Project, &files, &records);
    jadpo_check::check(&db, project)
}

fn rules(prompt_type: &str, prompt_text: Option<&str>) -> Vec<String> {
    diagnostics(prompt_type, prompt_text, &[])
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

/// Each problem as `path:line rule: message`.
fn problems(prompt_type: &str, prompt_text: Option<&str>) -> Vec<String> {
    diagnostics(prompt_type, prompt_text, &[])
        .iter()
        .map(|d| {
            let at = d.primary.as_ref().map_or(String::new(), |label| {
                format!("{}@{}", label.span.path, label.span.start)
            });
            format!("{at} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

const NONE: &[&str] = &[];

#[test]
fn the_page_s_prompt_checks() {
    assert_eq!(rules(PROMPT_TYPE, Some(PROMPT_TEXT)), NONE);
}

#[test]
fn a_placeholder_names_a_field() {
    let text = PROMPT_TEXT.replace("{question}", "{questoin}");
    let found = problems(PROMPT_TYPE, Some(&text));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("prompt.placeholder_unknown") && found[0].contains("{questoin}"),
        "{found:#?}"
    );
    assert!(found[0].starts_with(TEXT_PATH), "{found:#?}");
    // A brace not doubled, or never closed, is a placeholder that names
    // nothing.
    let lone = PROMPT_TEXT.replace("delivery dates.", "delivery dates. }");
    assert_eq!(
        rules(PROMPT_TYPE, Some(&lone)),
        ["prompt.placeholder_unknown"]
    );
    let open = PROMPT_TEXT.replace("delivery dates.", "delivery dates. {");
    assert_eq!(
        rules(PROMPT_TYPE, Some(&open)),
        ["prompt.placeholder_unknown"]
    );
    // Doubled braces write one.
    let doubled = PROMPT_TEXT.replace("delivery dates.", "delivery dates. Reply {{as JSON}}.");
    assert_eq!(rules(PROMPT_TYPE, Some(&doubled)), NONE);
}

#[test]
fn every_field_is_written_somewhere() {
    let with_tone = PROMPT_TYPE.replace(
        "    reader_locale: Locale\n",
        "    reader_locale: Locale\n    reply_tone: Text\n",
    );
    let found = problems(&with_tone, Some(PROMPT_TEXT));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("prompt.placeholder_unused") && found[0].contains("reply_tone"),
        "{found:#?}"
    );
    assert!(found[0].starts_with(TYPE_PATH), "{found:#?}");
}

#[test]
fn a_prompt_has_its_file_and_two_sections() {
    // No `# Request`: one error, not one per field the request would write.
    let no_request = PROMPT_TEXT
        .split("# Request")
        .next()
        .unwrap_or_default()
        .to_string();
    assert_eq!(
        rules(PROMPT_TYPE, Some(&no_request)),
        ["prompt.section_missing"]
    );
    // No file at all.
    let found = problems(PROMPT_TYPE, None);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("prompt.section_missing") && found[0].starts_with(TYPE_PATH),
        "{found:#?}"
    );
    // Text before the instructions, the sections the wrong way round, a
    // section twice and an empty one.
    for broken in [
        format!("Draft two\n{PROMPT_TEXT}"),
        "# Request\n\n{question}\n\n{articles}\n\n# Instructions\n\nAnswer in {reader_locale}.\n"
            .to_string(),
        format!("{PROMPT_TEXT}\n# Request\n\nAgain.\n"),
        "# Instructions\n\n# Request\n\n{question} {articles} {reader_locale}\n".to_string(),
    ] {
        assert_eq!(
            rules(PROMPT_TYPE, Some(&broken)),
            ["prompt.section_missing"],
            "{broken}"
        );
    }
    // A prompt's file with no type.
    let db = Database::default();
    let project = support::project_with_records(
        &db,
        Layout::Project,
        &[APP],
        &[("domains/support/prompts/old_greeting.prompt", PROMPT_TEXT)],
    );
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    assert_eq!(found, ["prompt.section_missing"]);
}

#[test]
fn free_text_is_only_in_the_request() {
    let text = PROMPT_TEXT.replace(
        "delivery dates.",
        "delivery dates. The question: {question}",
    );
    let found = problems(PROMPT_TYPE, Some(&text));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("prompt.free_text_in_instructions") && found[0].contains("question"),
        "{found:#?}"
    );
    // An object or a list is free text too.
    let text = PROMPT_TEXT.replace("delivery dates.", "delivery dates. {articles}");
    assert_eq!(
        rules(PROMPT_TYPE, Some(&text)),
        ["prompt.free_text_in_instructions"]
    );
    // A closed value may be in either: an enum, a number, a date, a locale.
    let closed = "use support/types { ArticleExcerpt }

/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    articles: List<ArticleExcerpt>
    reader_locale: Locale
    reader_country: Country
    most_words: Int
    asked_on: CalendarDate
}
";
    let text = PROMPT_TEXT.replace(
        "delivery dates.",
        "delivery dates. They are in {reader_country}, asked on {asked_on}; use at most {most_words} words.",
    );
    assert_eq!(rules(closed, Some(&text)), NONE);
}

#[test]
fn a_secret_a_file_bytes_or_a_second_conversation_is_no_placeholder() {
    let with = |field: &str, written: &str| {
        let prompt_type = PROMPT_TYPE.replace(
            "    reader_locale: Locale\n",
            &format!("    reader_locale: Locale\n{field}"),
        );
        let text = PROMPT_TEXT.replace("{articles}\n", &format!("{{articles}}\n{written}"));
        rules(&prompt_type, Some(&text))
    };
    assert_eq!(
        with("    photo: Bytes\n", "{photo}\n"),
        ["prompt.placeholder_type"]
    );
    assert_eq!(
        with("    staff_password: Password\n", "{staff_password}\n"),
        ["prompt.placeholder_type"]
    );
    assert_eq!(
        with("    shown_note: Text?\n", "{shown_note}\n"),
        ["prompt.placeholder_type"]
    );
    // One conversation is the turns before the request, never written.
    assert_eq!(with("    history: List<ChatMessage>\n", ""), NONE);
    assert_eq!(
        with("    history: List<ChatMessage>\n", "{history}\n"),
        ["prompt.placeholder_type"]
    );
    assert_eq!(
        with(
            "    history: List<ChatMessage>\n    earlier: List<ChatMessage>\n",
            ""
        ),
        ["prompt.placeholder_type"]
    );
}

#[test]
fn a_prompt_lives_in_prompts() {
    let db = Database::default();
    let project = support::project_with_records(
        &db,
        Layout::Project,
        &[
            APP,
            SLUG,
            EXCERPT,
            ("domains/support/types/answer_question.jadpo", PROMPT_TYPE),
        ],
        &[("domains/support/types/answer_question.prompt", PROMPT_TEXT)],
    );
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    assert!(
        found.contains(&"layout.wrong_folder".to_string()),
        "{found:#?}"
    );
    // `shared/prompts/` holds a prompt several domains use.
    let db = Database::default();
    let project = support::project_with_records(
        &db,
        Layout::Project,
        &[
            APP,
            (
                "shared/prompts/summarise_text.jadpo",
                "/// Summarises a text in a few sentences.\npublic type SummariseText = Prompt {\n    source_text: Text\n}\n",
            ),
        ],
        &[(
            "shared/prompts/summarise_text.prompt",
            "# Instructions\n\nSummarise the text in three sentences.\n\n# Request\n\n{source_text}\n",
        )],
    );
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    assert_eq!(found, NONE);
}

#[test]
fn a_prompt_s_name_is_the_project_s_own() {
    let found = diagnostics(
        PROMPT_TYPE,
        Some(PROMPT_TEXT),
        &[
            (
                "domains/billing/prompts/answer_question.jadpo",
                "/// Answers a question about a bill.\ntype AnswerQuestion = Prompt {\n    question: Text\n}\n",
            ),
            (
                "domains/billing/actions/noop.jadpo",
                "/// Nothing.\naction noop() {\n}\n",
            ),
        ],
    );
    assert!(
        found
            .iter()
            .any(|d| d.rule.id == "semantic.duplicate_declaration"),
        "{:#?}",
        found.iter().map(|d| d.rule.id).collect::<Vec<_>>()
    );
}

const RENDERS: &str = "use support/prompts { AnswerQuestion }
use support/types { ArticleExcerpt }

/// What the model is sent for a question.
function sent(articles: List<ArticleExcerpt>) -> List<ChatMessage> {
    return prompts.render(AnswerQuestion {
        question: \"Do your loaves contain sesame?\"
        articles: articles
        reader_locale: Locale.en_gb
    })
}
";

#[test]
fn prompts_render_gives_the_messages() {
    let db = Database::default();
    let project = support::project_with_records(
        &db,
        Layout::Loose,
        &[
            (
                "main.jadpo",
                &format!(
                    "{}\n{}",
                    SLUG.1,
                    EXCERPT
                        .1
                        .replace("use support/types { ArticleSlug }\n\n", "")
                ),
            ),
            (
                "answer_question.jadpo",
                &PROMPT_TYPE.replace("use support/types { ArticleExcerpt }\n\n", ""),
            ),
            (
                "sending.jadpo",
                &RENDERS
                    .replace("use support/prompts { AnswerQuestion }\n", "")
                    .replace("use support/types { ArticleExcerpt }\n", ""),
            ),
        ],
        &[("answer_question.prompt", PROMPT_TEXT)],
    );
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(found, NONE as &[&str]);
    // Only a prompt can be rendered.
    let db = Database::default();
    let project = support::project_with_records(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            "/// A customer's question.\ntype QuestionText = Text {\n    max_length: 2000\n}\n\n/// Not a prompt.\nfunction sent(question: QuestionText) -> List<ChatMessage> {\n    return prompts.render(question)\n}\n",
        )],
        &[],
    );
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect();
    assert_eq!(found, ["type.mismatch"]);
}

#[test]
fn each_prompt_is_in_the_manifest_with_its_digest() {
    let files = [APP, SLUG, EXCERPT, (TYPE_PATH, PROMPT_TYPE)];
    let version = |text: &str, prompt_type: &str| {
        let db = Database::default();
        let mut files = files;
        files[3] = (TYPE_PATH, prompt_type);
        let project =
            support::project_with_records(&db, Layout::Project, &files, &[(TEXT_PATH, text)]);
        let graph = jadpo_check::graph::graph(&db, project);
        let node = graph
            .nodes()
            .iter()
            .find(|node| node.key.as_str() == "prompt answer_question")
            .unwrap_or_else(|| panic!("no prompt node"));
        match &node.detail {
            jadpo_graph::Detail::Prompt { version } => version.clone(),
            other => panic!("{other:?}"),
        }
    };
    let first = version(PROMPT_TEXT, PROMPT_TYPE);
    let (name, digest) = first.split_once('@').expect("name@digest");
    assert_eq!(name, "answer_question");
    assert!(
        digest.len() == 8 && digest.chars().all(|c| c.is_ascii_hexdigit()),
        "{first}"
    );
    // The digest is a SHA-256 over the two files, each with its length.
    assert_eq!(first, format!("answer_question@{}", expected_digest()));
    // Either file changing changes it.
    assert_ne!(
        version(&PROMPT_TEXT.replace("bakery", "bakehouse"), PROMPT_TYPE),
        first
    );
    assert_ne!(
        version(PROMPT_TEXT, &PROMPT_TYPE.replace("Answers", "Replies to")),
        first
    );
}

/// What the digest of the page's two files is, worked out apart (Python's
/// `hashlib.sha256` over each file's length as 8 bytes, little-endian,
/// then its bytes).
fn expected_digest() -> &'static str {
    "864ad76b"
}
