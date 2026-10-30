# `prompt.placeholder_unknown`

A placeholder names a field of its prompt's type, as a translation's does;
a brace not doubled or never closed names nothing either.

```jadpo check project=e-prompt-placeholder-unknown file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-unknown file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-placeholder-unknown file=domains/support/prompts/answer_question.prompt expect=prompt.placeholder_unknown
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{questoin}
```

```text output
error: `{questoin}` isn't a field of `AnswerQuestion`
 --> domains/support/prompts/answer_question.prompt:7:1
  |
7 | {questoin}
  | ^^^^^^^^^^ no such field
  reason: A prompt's placeholders are its type's fields, written `{field}`, as a translation's are. `AnswerQuestion` has no field `questoin`; its text may write `{question}` and `{reader_locale}`.
  next: Write `{question}`, or add the field to `AnswerQuestion` (agent)
  rule: prompt.placeholder_unknown
  help: https://jadpo.dev/docs/errors/prompt.placeholder_unknown

check failed: 1 error, 2 files
```
