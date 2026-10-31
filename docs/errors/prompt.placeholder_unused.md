# `prompt.placeholder_unused`

A prompt's type has a field that its prompt file never writes.

## Why Jadpo refuses it

A prompt is two files: a type whose fields are the values it is given,
and a `.prompt` file of text, where each `{field}` placeholder writes one
of them. The model is sent only that text, so what a reviewer reads in
the `.prompt` file is exactly what the model gets.

A field the text never writes is never sent. Either the text forgot it,
and the model is missing something the code meant it to have, or the
field isn't needed. Jadpo can't tell which, so it refuses the field
until the text writes it or the field is removed.

## What it looks like

```jadpo check project=e-prompt-placeholder-unused file=app.jadpo hidden
application {}
```

domains/support/prompts/answer_question.prompt:

```jadpo check project=e-prompt-placeholder-unused file=domains/support/prompts/answer_question.prompt
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```

domains/support/prompts/answer_question.jadpo:

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
  help: https://jadpo.dev/docs/errors/prompt.placeholder_unused

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the model should be given the field.

**If the model needs it**, write its placeholder in the prompt file. An
order reference is free text, so it goes under `# Request`, not in the
instructions. In `domains/support/prompts/answer_question.prompt`:

```text
{order_reference}
```

```jadpo check project=e-prompt-placeholder-unused-fix-1 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}

{order_reference}
```

```jadpo check project=e-prompt-placeholder-unused-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-unused-fix-1 file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
    order_reference: Text
}
```

The model is now sent the order reference with the question, as a data
block tagged `<order_reference>`.

**If it doesn't**, remove the field. `answer_question.jadpo` becomes:

```jadpo check project=e-prompt-placeholder-unused-fix-2 file=domains/support/prompts/answer_question.jadpo
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-placeholder-unused-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-unused-fix-2 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```

Code that builds an `AnswerQuestion` stops giving it an
`order_reference`, and the prompt's text is unchanged.

## Related

- [Prompts as files](../language/21-ai.md#prompts-as-files)
- [`prompt.free_text_in_instructions`](../errors/prompt.free_text_in_instructions.md)
