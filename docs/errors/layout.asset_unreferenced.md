# `layout.asset_unreferenced`

A file that isn't Jadpo sits in a code folder, and no declaration uses it.

## Why Jadpo refuses it

Everything in `domains/` and `shared/` is the app's code, or a file the
code uses: an email's templates, a prompt's text and its recordings. Each
sits beside the declaration that uses it, named after it, so a reader who
opens a folder sees only what the app runs. A file nothing uses is left
behind by a rename, or was never the app's, and a reviewer can't tell
which without asking.

## What it looks like

Notes about the planning domain are kept beside its actions.

```jadpo check project=e-asset file=app.jadpo hidden
application {}
```

```jadpo check project=e-asset file=domains/planning/actions/complete_todo.jadpo hidden
/// Marks a todo done.
action complete_todo() -> Unit {}
```

`domains/planning/actions/notes.md`:

```jadpo check project=e-asset file=domains/planning/actions/notes.md expect=layout.asset_unreferenced
Completing a todo should also clear its reminder.
```

```text output
error: `notes.md` isn't used by any declaration
 --> domains/planning/actions/notes.md
  reason: A code folder holds Jadpo and the files its declarations use: an email's templates beside it in `emails/`, and a prompt's text and recordings beside it in `prompts/`. Nothing uses `notes.md`.
  next: Move it out of `domains/`, or delete it if nothing needs it (agent)
  rule: layout.asset_unreferenced
  help: https://jadpo.dev/docs/errors/layout.asset_unreferenced

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A recording of a prompt's answer sits in `recordings/<prompt>/` beside the
prompt it records. One whose folder names no prompt is used by nothing,
here because the prompt was renamed from `answer_question` to
`answer_customer`:

```jadpo check project=e-asset-recording file=app.jadpo hidden
application {}
```

```jadpo check project=e-asset-recording file=domains/support/prompts/answer_customer.jadpo hidden
/// Answers a customer's question.
type AnswerCustomer = Prompt {
    question: Text
}
```

```jadpo check project=e-asset-recording file=domains/support/prompts/answer_customer.prompt hidden
# Instructions

Answer the customer's question in one sentence.

# Request

{question}
```

`domains/support/prompts/recordings/answer_question/sesame.json`:

```jadpo check project=e-asset-recording file=domains/support/prompts/recordings/answer_question/sesame.json expect=layout.asset_unreferenced
{
  "prompt_version": "1a2b3c4d",
  "answer": "Yes, the loaf has sesame seeds."
}
```

```text output
error: `sesame.json` isn't used by any declaration
 --> domains/support/prompts/recordings/answer_question/sesame.json
  reason: A recording sits in `recordings/<prompt>/` beside the prompt it records, so `sesame.json` belongs to `AnswerQuestion` in `domains/support/prompts/answer_question.jadpo`, and no prompt there is called that.
  next: Move it to the recordings of the prompt it records, or delete it if nothing will use it (agent)
  rule: layout.asset_unreferenced
  help: https://jadpo.dev/docs/errors/layout.asset_unreferenced

check failed: 1 error, 2 files
```

## How to fix it

**If the file is the app's**, put it beside the declaration that uses it,
named after it: here, move the recording to
`domains/support/prompts/recordings/answer_customer/sesame.json`.

```jadpo check project=e-asset-recording-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-asset-recording-fix file=domains/support/prompts/answer_customer.jadpo hidden
/// Answers a customer's question.
type AnswerCustomer = Prompt {
    question: Text
}
```

```jadpo check project=e-asset-recording-fix file=domains/support/prompts/answer_customer.prompt hidden
# Instructions

Answer the customer's question in one sentence.

# Request

{question}
```

`domains/support/prompts/recordings/answer_customer/sesame.json`:

```jadpo check project=e-asset-recording-fix file=domains/support/prompts/recordings/answer_customer/sesame.json
{
  "prompt_version": "1a2b3c4d",
  "answer": "Yes, the loaf has sesame seeds."
}
```

The recording is now one of `AnswerCustomer`'s, which a test's fake can
answer with.

**If it isn't**, move it out of the code folder, to the project's root or
`docs/`, or delete it if nothing needs it.

```jadpo check project=e-asset-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-asset-fix file=domains/planning/actions/complete_todo.jadpo
/// Marks a todo done. Completing a todo also clears its reminder.
action complete_todo() -> Unit {}
```

The note now lives where a reader looks for it: on the action it is about.

## Related

- [Tests and other files](../language/01-project-layout-and-modules.md#tests-and-other-files)
- [Email](../language/16-built-in-services.md#email)
- [Prompts as files](../language/21-ai.md#prompts-as-files)
