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
2. Read https://jadpo.dev/llms.txt once too. It lists the guides, one for
   each common job, and the reference's groups.
3. For a task, follow its guide if it has one. Otherwise open its group's
   page, which lists what is in the group, and fetch only the topics you
   need, each at `https://jadpo.dev/docs/<id>.md`.
4. The app declares its version in `app.jadpo`. Use the same addresses
   under `https://jadpo.dev/v/<version>/` to read the docs for that version.
5. Don't fetch everything. There is no single file of all the docs, and
   `docs/index.json` is a list for tools: never read it whole.

## Before you add or change code

`jadpo check` writes two catalogues of the project into `build/catalogue/`,
errors or not. `build/` is never committed, so in a fresh workspace run
`jadpo check` once before reading them.

- Before writing a helper or a constant, read
  `build/catalogue/libraries.md`: every library function with its
  signature, description and callers, and every library constant with its
  type, value and readers. Use one that fits instead of writing another. A
  helper too like an existing one is `library.similar_exists`, and two
  constants holding one value are `constant.similar_exists`; each names
  the one to use.
- Before adding a failure, read `build/catalogue/failures.md`: every
  failure with its code, kind, status and client fields.
- Before changing or removing anything shared or public, run
  `jadpo impact <name> --json`, while the project still checks without
  errors. It lists the domains that call or use it, the routes, schedules
  and subscribers it reaches, the tests that reach it and whether it is
  public. Change every place it reaches, and run the tests it names.

## Check your work

Run both after each connected change, a declaration and the first thing
that uses it, and read the result before writing more:

```text
jadpo check --json
jadpo test --json
```

Each prints one JSON document: `schemaVersion`, `command`, `ok`,
`durationMs`, `data` and `diagnostics`, kept short: add `--full` for
everything it leaves out. Stop when `ok` is `true`. Otherwise read each entry of
`diagnostics`:

- `rule` names the error, and `message` says what is wrong.
- `reason` says why Jadpo refuses it. An entry saying the same as an earlier
  one has `sameAs`, that entry's place in the list, instead.
- `next.text` is the step to take, and `next.edits` lists edits to apply
  when the fix is mechanical.
- `help` is the address of the rule's page, such as
  `https://jadpo.dev/docs/errors/failure.kind_required`. Open it when the
  reason and the step are not enough, and add `.md` for the markdown.

Fix the first error and run again; later errors often disappear.

## A fixed value

If a deployment could ever need a different value, make it a setting in
`config.jadpo`; otherwise make it a `constant`. A page size an operator
might tune is a setting. The number of pence in a pound is a constant.

A value is one or the other, never both, so a setting's `default:` holds
the value itself and never names a constant
(`configuration.default_literal_required`). A constant that turns out to
need a value per deployment is moved with `jadpo config move <constant>`,
which makes it a setting and rewrites every read to `config.<name>`. Don't
move it by hand. The reasons are in https://jadpo.dev/docs/constants.md.

## Working beside other agents

When other agents work on the same project at the same time, each in its
own workspace, take these steps before every push:

1. Merge `main` into your branch.
2. Run `jadpo check --json`. It builds nothing, so it is quick, and it
   finds a name another agent took since you started.
3. Fix any clash it reports, run `jadpo test --json`, then push.

A private function, type or constant needs a name unique only within its
domain, and everything else one unique across the project. So two
domains' private helpers never clash, but a name another agent took in
your domain does, and so does a public or shared name anywhere. A clash
is `semantic.duplicate_declaration`, which says whether to use the other
agent's declaration or rename yours. Two alike helpers or constants are
`library.similar_exists` or `constant.similar_exists`, which name the
other one to use.

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
