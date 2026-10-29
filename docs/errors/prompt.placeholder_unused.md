# `prompt.placeholder_unused`

Every field of a prompt is written in its text, so a model is sent only
what a reviewer reads in the file.

```jadpo check project=e-prompt-placeholder-unused file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-unused file=domains/support/prompts/answer_question.jadpo expect=prompt.placeholder_unused
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
    order_reference: Text
}
```

```text output
error: `AnswerQuestion.order_reference` is written nowhere in `answer_question.prompt`
 --> domains/support/prompts/answer_question.jadpo:5:5
  |
5 |     order_reference: Text
  |     ^^^^^^^^^^^^^^^ never written
  reason: Each field of a prompt is a placeholder its text writes, so a model is sent only what a reviewer reads in `answer_question.prompt`.
  next: Write `{order_reference}` in `answer_question.prompt`, or remove the field (agent)
  rule: prompt.placeholder_unused
  help: docs/errors/prompt.placeholder_unused.md

check failed: 1 error, 2 files
```

```jadpo check project=e-prompt-placeholder-unused file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```
