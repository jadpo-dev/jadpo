# `prompt.free_text_in_instructions`

Free text (text, `Markdown`, objects and lists) is only ever sent as data,
under `# Request`; only a closed value is written in the instructions.

```jadpo check project=e-prompt-free-text-in-instructions file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-free-text-in-instructions file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-free-text-in-instructions file=domains/support/prompts/answer_question.prompt expect=prompt.free_text_in_instructions
# Instructions

Answer this question in the language of {reader_locale}: {question}

# Request

{question}
```

```text output
error: `{question}` is free text in the instructions of `AnswerQuestion`
 --> domains/support/prompts/answer_question.prompt:3:58
  |
3 | Answer this question in the language of {reader_locale}: {question}
  |                                                          ^^^^^^^^^^ free text
  reason: `question` is free text, which can carry instructions of its own. Free text is only ever sent as data, under `# Request`, inside a block tagged `<question>` that it can't close, so it can't pass itself off as the application's instructions. A closed value (an enum, a number, a date, a `Locale`) may be written in the instructions.
  next: Move `{question}` under `# Request` (agent)
  rule: prompt.free_text_in_instructions
  help: https://jadpo.dev/docs/errors/prompt.free_text_in_instructions

check failed: 1 error, 2 files
```
