# `prompt.section_missing`

A prompt's file is its two sections, `# Instructions` then `# Request`,
each once and with text; a prompt type without its file, and a file
without its type, are the same mistake.

```jadpo check project=e-prompt-section-missing file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-section-missing file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-section-missing file=domains/support/prompts/answer_question.prompt expect=prompt.section_missing
# Instructions

Answer the customer's question in the language of {reader_locale}.
```

```text output
error: `answer_question.prompt` has no `# Request`
 --> domains/support/prompts/answer_question.prompt:1:1
  |
1 | # Instructions
  | ^^^^^^^^^^^^^^ its only section
  reason: A prompt's file is its two sections, each once: `# Instructions`, the fixed text sent as the system message, then `# Request`, what is asked each time. Every line is in one of them, so a reviewer reads exactly what a model is sent.
  next: Add `# Request` and what is asked each time after the instructions (agent)
  rule: prompt.section_missing
  help: docs/language/21-ai.html#prompt-section-missing

check failed: 1 error, 2 files
```
