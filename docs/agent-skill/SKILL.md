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

In an app that has code, run `jadpo explain` first. It prints a digest of
the app from its checked code, about 1,000 to 3,000 tokens: who calls and
what tenancy limits them to, each entity's fields, lifecycle and who may
create, read, update and delete it, each route with its input, output and
what it runs, the background work, the acceptances already given, and for
each kind of construct the app uses the one topic that explains it.
`jadpo help topic <id>` prints that topic offline. Read the digest instead
of every file, and a topic it names instead of a whole page.

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

## All of the records

When a request means every record that matches, such as "export all my
todos" or "clear all my completed todos", the query's bound is
`limit: every`: every match, up to 10,000, in one list. Don't ask your
user for a number. When more than 10,000 match, the query gives none of
them and the action stops with a fault, so nothing is cut short or half
changed. A change to every match is that query and a `for` loop of
writes in one action. Use `Page<T>` or a batch only when the request
says to page or the set can grow past 10,000. The details are in
https://jadpo.dev/docs/every-record-that-matches.md.

## Filtering by a related record

To keep the records that have, or lack, a related record, such as the
tickets with a tag kept in a link entity, write the test in the query's
own `where:`: `exists(TicketTag.ticket_id, TicketTag.tag_id == tag_id)`,
or `not exists(...)`. It works with `Page<T>` and `order:`, and the
related entity's policy and `visible:` apply. Don't read the related
records first and filter with `in` a list of their ids. The details are
in https://jadpo.dev/docs/filtering-by-a-related-record.md.

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

A check that passes can still print a `notice:` (for example what a first
commit holds, which describes the app as it stands rather than your
change). It is marked `(agent)`: carry on, and tell the user what it said
when you hand over. It is never a reason to stop.

`jadpo check` writes the API record, the other records and the next
migration itself when your change moved them on, and lists each file it
wrote (`updated` in `--json`). Commit them with the code. Only a migration
that needs a decision is left to `jadpo migrate new`, which the error
names.

In the JSON this is `next.decides`: `"agent"` or `"human"`. When it is
`"human"`, put the choices and what each changes to the user, and wait for
the answer.

## What you decide yourself

Stop and ask in three cases, once, with every open point in one question:

- an error or step whose `next:` ends `(human)`, and an acceptance
  (below);
- a request that changes or contradicts a rule the project writes down:
  in the README's rules, an entity's policy or its `about:`
  (`jadpo explain` opens with where they are);
- a case the request leaves open that the rules or the product would
  decide: who may do it and to which records, a deadline or time boundary
  (is a boundary inclusive?), what happens when related data is missing,
  and whether a limit can move. A rule that isn't written down is the
  owner's to decide, not yours.

Everything else is yours: presentation, such as wording, names and order.
Choose the reasonable reading, write it down where the owner will see it,
as a `///` line on the code that applies it and in your summary, and
carry on.

Before you stop to ask, leave the project passing `jadpo check` and
`jadpo test`. Back out or set aside the part that needs the owner, keep
the rest of the change that works, and say in your summary what is
waiting on them. Whatever comes next, yours or another agent's, starts
from the project you leave, and one that doesn't check fails every
change after it.

## Changes that need the owner's acceptance

Some changes need a person to accept them: removing what clients read,
widening who can do what, dropping stored data, a new outside host. `check`
fails with `acceptance.missing` until an acceptance lists them, and with
`acceptance.no_reason` until its `///` line says why. An acceptance is the
owner's decision, not yours:

1. Stop and ask your user whether to accept the change, saying what it
   allows and for whom, such as "the daily job would run as the system
   and read every workspace's replies". Ask about it on its own, so the
   answer is about it.
2. Go on only once they agree to that change: in their answer, or in a
   request that asks in so many words for what it does ("owners and
   admins only" agrees to listing the new action for those roles). A
   request that widens a written rule without its limits, such as "let
   agents delete customers", isn't enough on its own: ask who, which
   records and under what conditions before you draft. A
   general go-ahead, such as "do whatever it needs" or "carry on", agrees
   to nothing that widens access or breaks a client. If they don't
   agree, undo the change. While you wait for the answer, the change
   that needs it is backed out, so `jadpo check` and `jadpo test` pass.
3. Run `jadpo acceptance new <purpose>` and write their own words as the
   `///` reason, with who said them and where:

   ```text
   /// Sam Okafor (product owner), replying in QUESTIONS.md: "Yes, the daily job may read reply times."
   ```

   Never write a reason of your own, and never get `check` passing by
   inventing one.

## A bug in Jadpo

If the compiler crashes, rejects code the docs say is valid, or a doc is
wrong, don't work around it silently and don't change Jadpo. Tell the user
what you found, with the smallest project that shows it and the output of
`jadpo check --json`, and let them report it. How is in "Using Jadpo" in
https://github.com/jadpo-dev/jadpo/blob/main/AGENTS.md.
