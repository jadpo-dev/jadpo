# `prompt.placeholder_unknown`

A prompt's text writes a placeholder that names no field of its type, or a brace on its own.

## Why Jadpo refuses it

A prompt is two files: a type whose fields are the values it is given,
and a `.prompt` file of text, where each `{field}` writes one of them,
as a translation's placeholders do. A placeholder that names no field
has no value to write, so the model would be sent the braces and the
misspelt name, or nothing. A brace on its own, a `}` not doubled or a
`{` never closed on its line, is read the same way, since `{{` and `}}`
are how the text writes a brace. Jadpo refuses it rather than send the
model text the reviewer didn't mean.

## What it looks like

```jadpo check project=e-prompt-placeholder-unknown file=app.jadpo hidden
application {}
```

domains/support/prompts/answer_question.jadpo:

```jadpo check project=e-prompt-placeholder-unknown file=domains/support/prompts/answer_question.jadpo
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

domains/support/prompts/answer_question.prompt:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A brace never closed is the same error:

```jadpo check project=e-prompt-placeholder-unknown-brace file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-unknown-brace file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

```jadpo check project=e-prompt-placeholder-unknown-brace file=domains/support/prompts/answer_question.prompt expect=prompt.placeholder_unknown
# Instructions

Answer the customer's question in the language of {reader_locale.

# Request

{question}
```

```text output
error: A `{` in `answer_question.prompt` that never closes
 --> domains/support/prompts/answer_question.prompt:3:51
  |
3 | Answer the customer's question in the language of {reader_locale.
  |                                                   ^^^^^^^^^^^^^^^ no `}` on this line
  reason: In a prompt's text `{field}` writes a field of its type, and a brace is written doubled: `{{` writes `{` and `}}` writes `}`, as in a translation.
  next: Write `{{` for a brace, or close the placeholder with `}` (agent)
  rule: prompt.placeholder_unknown
  help: https://jadpo.dev/docs/errors/prompt.placeholder_unknown

check failed: 1 error, 2 files
```

## How to fix it

The question is whether the field exists under another name.

**If the placeholder is misspelt**, write the field's name. In
`domains/support/prompts/answer_question.prompt`:

```text
{question}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-1 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-1 file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-1 file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

The customer's question is now sent as a data block tagged
`<question>`.

**If the placeholder is for a value the prompt isn't given yet**, add a
field with its name to the type. For a placeholder `{order_reference}`
written under `# Request`, in
`domains/support/prompts/answer_question.jadpo`:

```jadpo fragment
type AnswerQuestion = Prompt {
    ...
    order_reference: Text
}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-2 file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
    order_reference: Text
}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-2 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}

{order_reference}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-2 file=app.jadpo hidden
application {}
```

Code that builds an `AnswerQuestion` now gives it an `order_reference`,
which the model is sent after the question.

**If a placeholder was left open**, close it; to write a brace itself,
double it, `{{` or `}}`. In
`domains/support/prompts/answer_question.prompt`:

```text
Answer the customer's question in the language of {reader_locale}.
```

```jadpo check project=e-prompt-placeholder-unknown-fix-3 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

Answer the customer's question in the language of {reader_locale}.

# Request

{question}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-3 file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-unknown-fix-3 file=domains/support/prompts/answer_question.jadpo hidden
/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: Text
    reader_locale: Locale
}
```

The instructions now name the reader's locale, written as its text,
such as `en-GB`.

## Related

- [Prompts as files](../language/21-ai.md#prompts-as-files)
- [What the compiler checks of translations](../language/10-configuration-and-time.md#what-the-compiler-checks-of-translations)
- [`prompt.placeholder_unused`](../errors/prompt.placeholder_unused.md)
