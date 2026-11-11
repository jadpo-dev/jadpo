# `prompt.section_missing`

A prompt's `.prompt` file doesn't hold `# Instructions` then `# Request`, each once and with text, or a prompt type and its file aren't both there.

## Why Jadpo refuses it

A prompt's file is its two sections, in this order: `# Instructions`,
the fixed text sent as the system message, then `# Request`, what is
asked each time, sent as the user message. Every line of the file is in
one of them, so a reviewer reads exactly what a model is sent and which
part a user's text can reach. Without one of them, or with text before
the first, the file doesn't say what goes where. A prompt type with no
file has no text to send, and a file with no type beside it is never
sent, so each is the same mistake.

## What it looks like

```jadpo check project=e-prompt-section-missing file=app.jadpo hidden
application {}
```

domains/support/prompts/answer_question.jadpo:

```jadpo check project=e-prompt-section-missing file=domains/support/prompts/answer_question.jadpo
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

domains/support/prompts/answer_question.prompt:

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
  help: https://jadpo.dev/docs/errors/prompt.section_missing

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `.prompt` file with no prompt type beside it is the same error:

```jadpo check project=e-prompt-section-missing-type file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-section-missing-type file=domains/support/prompts/answer_question.prompt expect=prompt.section_missing
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```

```text output
error: `answer_question.prompt` belongs to no prompt
 --> domains/support/prompts/answer_question.prompt:1:1
  |
1 | # Instructions
  | ^^^^^^^^^^^^^^ no type beside it
  reason: A prompt's text sits beside its `type X = Prompt { ... }`, named after it, so nothing sends `answer_question.prompt` until `AnswerQuestion` is declared in `domains/support/prompts/answer_question.jadpo`.
  next: Declare `type AnswerQuestion = Prompt { ... }` beside it, or remove the file (agent)
  rule: prompt.section_missing
  help: https://jadpo.dev/docs/errors/prompt.section_missing

check failed: 1 error, 1 file
```

## How to fix it

**If the file has no `# Request`**, add it after the instructions, with
what is asked each time. In
`domains/support/prompts/answer_question.prompt`:

```text
# Request

{question}
```

```jadpo check project=e-prompt-section-missing-fix-1 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```

```jadpo check project=e-prompt-section-missing-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-section-missing-fix-1 file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

The instructions go as the system message, and the customer's question
as the user message, in a data block tagged `<question>`.

**If the file has no type**, declare one beside it, named after the
file, with a field for each placeholder. In
`domains/support/prompts/answer_question.jadpo`:

```jadpo check project=e-prompt-section-missing-fix-2 file=domains/support/prompts/answer_question.jadpo
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-section-missing-fix-2 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```

```jadpo check project=e-prompt-section-missing-fix-2 file=app.jadpo hidden
application {}
```

Code can now ask a model with an `AnswerQuestion`, and the file's text
is what it sends.

## Related

- [Prompts as files](../language/21-ai.md#prompts-as-files)
- [`prompt.placeholder_unknown`](../errors/prompt.placeholder_unknown.md)
