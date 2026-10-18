//! `jadpo ai show` (area 21, "Prompts as files", chunk 5.5v2): what a
//! prompt sends, its placeholders in place, in the CLI design language;
//! and the errors around it.

use super::http::{jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/support/types/article_slug.jadpo",
        "/// A help article's address.\ntype ArticleSlug = Text {\n    max_length: 120\n}\n",
    ),
    (
        "domains/support/types/article_excerpt.jadpo",
        "use support/types { ArticleSlug }\n\n/// The part of a help article a question is answered from.\ntype ArticleExcerpt = Object {\n    slug: ArticleSlug\n    title: Text\n    excerpt: Text\n}\n",
    ),
    (
        "domains/support/prompts/answer_question.jadpo",
        "use support/types { ArticleExcerpt }\n\n/// Answers a customer's question using only the help articles found for it.\ntype AnswerQuestion = Prompt {\n    question: Text\n    articles: List<ArticleExcerpt>\n    reader_locale: Locale\n}\n",
    ),
    (
        "domains/support/prompts/answer_question.prompt",
        "# Instructions\n\nYou answer questions from customers of an online bakery, using only the\nhelp articles given with the question. If they don't answer it, set\nresolved_by_articles to false and say a person will reply. Write in the\nlanguage of {reader_locale}. Never promise refunds or delivery dates.\n\n# Request\n\n{question}\n\n{articles}\n",
    ),
    (
        "domains/support/prompts/chat_reply.jadpo",
        "/// Replies in a chat with a customer.\ntype ChatReply = Prompt {\n    history: List<ChatMessage>\n    latest: Text\n}\n",
    ),
    (
        "domains/support/prompts/chat_reply.prompt",
        "# Instructions\n\nReply kindly, in a sentence or two. Write {{\"ok\": true}} when done.\n\n# Request\n\n{latest}\n",
    ),
];

fn project(name: &str) -> (tempdir::Dir, std::path::PathBuf) {
    let dir = tempdir::Dir::new(name);
    let root = dir.path().join("support");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    (dir, root)
}

#[test]
fn show_prints_what_a_prompt_sends() {
    let (_dir, root) = project("ai-show");
    let done = jadpo(&root, &["ai", "show", "answer_question"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert_eq!(
        done.stdout,
        "  prompt: AnswerQuestion
  file: domains/support/prompts/answer_question.prompt
  version: answer_question@864ad76b

system  # Instructions
  You answer questions from customers of an online bakery, using only the
  help articles given with the question. If they don't answer it, set
  resolved_by_articles to false and say a person will reply. Write in the
  language of {reader_locale}. Never promise refunds or delivery dates.

user  # Request
  <question>
  {question}
  </question>

  <articles>
  {articles}
  </articles>

answer_question@864ad76b: 3 placeholders, 2 data blocks
"
    );
    // Its type's name finds it too.
    assert_eq!(
        jadpo(&root, &["ai", "show", "AnswerQuestion"]).stdout,
        done.stdout
    );
}

#[test]
fn a_conversation_is_shown_as_the_turns_and_braces_as_written() {
    let (_dir, root) = project("ai-show-chat");
    let done = jadpo(&root, &["ai", "show", "chat_reply"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains(
            "system  # Instructions\n  Reply kindly, in a sentence or two. Write {{\"ok\": true}} when done.\n\nturns\n  each message of `history`, in order\n\nuser  # Request\n  <latest>\n  {latest}\n  </latest>\n"
        ),
        "{}",
        done.stdout
    );
    let json = jadpo(&root, &["ai", "show", "chat_reply", "--json"]);
    let data: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(data["data"]["messages"][1]["role"], "turns");
    assert_eq!(data["data"]["messages"][1]["field"], "history");
    assert_eq!(data["data"]["placeholders"][0]["name"], "latest");
    assert_eq!(data["data"]["placeholders"][0]["written"], "data_block");
}

#[test]
fn show_names_a_prompt_the_project_has() {
    let (_dir, root) = project("ai-show-unknown");
    let done = jadpo(&root, &["ai", "show", "answer_questoin"]);
    assert_eq!(done.code, 2);
    assert_eq!(
        done.stderr,
        "error: `answer_questoin` isn't a prompt in this project
  reason: No prompt is called `answer_questoin`; the closest is `answer_question`.
  next: Run `jadpo ai show answer_question` (agent)
  rule: cli.ai_arguments
  help: docs/language/14-tooling.html#command-errors
"
    );
    let none = jadpo(&root, &["ai", "show"]);
    assert_eq!(none.code, 2);
    assert!(
        none.stderr
            .contains("The project's prompts are `answer_question` and `chat_reply`."),
        "{}",
        none.stderr
    );
    let other = jadpo(&root, &["ai", "shwo", "answer_question"]);
    assert!(
        other.stderr.contains("Use `jadpo ai show`"),
        "{}",
        other.stderr
    );
    let later = jadpo(&root, &["ai", "record", "answer_question"]);
    assert!(
        later.stderr.contains("rule: toolchain.not_built_yet"),
        "{}",
        later.stderr
    );
}

#[test]
fn a_prompt_with_errors_shows_them_instead() {
    let (_dir, root) = project("ai-show-broken");
    write(
        &root,
        "domains/support/prompts/answer_question.prompt",
        "# Instructions\n\nAnswer in {reader_locale}.\n\n# Request\n\n{questoin}\n\n{articles}\n",
    );
    let done = jadpo(&root, &["ai", "show", "answer_question"]);
    assert_eq!(done.code, 1, "{}", done.stdout);
    assert!(
        done.stdout
            .contains("error: `{questoin}` isn't a field of `AnswerQuestion`")
            && done
                .stdout
                .contains("--> domains/support/prompts/answer_question.prompt:7:1")
            && done.stdout.contains("ai show stopped: 1 error"),
        "{}",
        done.stdout
    );
}
