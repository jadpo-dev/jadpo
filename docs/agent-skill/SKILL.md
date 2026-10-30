---
name: jadpo
description: Write or change an app in the Jadpo programming language. Use when a project holds an app.jadpo file, when the user asks for Jadpo code, or when jadpo check or jadpo test reports an error with a rule such as failure.kind_required.
---

# Writing Jadpo apps

No model has seen Jadpo before, so don't write it from memory of other
languages. The docs are the only source, and the compiler is the check.

## Read first

1. Read the primer once per session: https://jadpo.dev/docs/primer.md. It
   covers the project layout, the syntax and the constructs nearly every
   app uses.
2. For the task, read the bundle's index, https://jadpo.dev/docs/index.json
   (or the shorter https://jadpo.dev/llms.txt), pick pages by `title` and
   `summary`, and fetch only their `.md`. The app declares its version in
   `app.jadpo`; use `https://jadpo.dev/v/<version>/docs/index.json` and the
   same prefix on every page to read the docs for that version.
3. Don't fetch everything. There is no single file of all the docs.

## Check your work

Run both after every change, and read the result before writing more:

```text
jadpo check --json
jadpo test --json
```

Each prints one JSON document: `schemaVersion`, `command`, `ok`, `data` and
`diagnostics`. Stop when `ok` is `true`. Otherwise read each entry of
`diagnostics`:

- `rule` names the error, and `message` says what is wrong.
- `reason` says why Jadpo refuses it.
- `next.text` is the step to take, and `next.edits` lists edits to apply
  when the fix is mechanical.
- `help` is the address of the rule's page, such as
  `https://jadpo.dev/docs/errors/failure.kind_required`. Open it when the
  reason and the step are not enough, and add `.md` for the markdown.

Fix the first error and run again; later errors often disappear.

## `(agent)` and `(human)`

`(human)` at the end of an error's `next:` line tells your coding agent to
stop and ask you which fix you want, because the choice is yours to make
(who can see data, what a setting holds). `(agent)` means your agent can
make the fix itself.

In the JSON this is `next.decides`: `"agent"` or `"human"`. When it is
`"human"`, put the choices and what each changes to the user, and wait for
the answer.

## A bug in Jadpo

If the compiler crashes, rejects code the docs say is valid, or a doc is
wrong, don't work around it silently and don't change Jadpo. Tell the user
what you found, with the smallest project that shows it and the output of
`jadpo check --json`, and let them report it. How is in "Using Jadpo" in
https://github.com/jadpo-dev/jadpo/blob/main/AGENTS.md.
