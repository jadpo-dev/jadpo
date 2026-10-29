# Writing Jadpo's docs

How the pages people and coding agents read are written. Both read the
same prose, and both do best with the same thing: plain words, one idea
at a time, and an example they can copy.

## Two layers

- **The user layer** teaches: the [Learn path](learn/README.md), the
  [primer](primer.md) and task guides. A newcomer starts on the Learn
  path, never on an area page. Area pages are organised by part of the
  language and each assumes the others, so no order of them teaches. The
  Learn path is ordered by what each idea needs, for a backend developer
  who knows TypeScript, SQL and HTTP.
- **The reference** is the area pages in `docs/language/` and the error
  pages in `docs/errors/`. They are the specification: where a user page
  and an area page disagree, the area page wins and the user page is
  fixed.

A user page never restates the reference in full. It shows the common
case and links the topic that has the rest.

## The rules

Each rule has a pair where it helps: a sentence from the docs as they
were, then the same thing written to the rule.

### Put ideas in the order they depend on each other

Never use a term before the page, or an earlier page, has introduced
it. Introduce a new idea through one the reader already has: an entity
is like a table with types, plus the compiler knowing who may read it.
When you must mention something early, say in a few plain words what it
is and which later page covers it. The Learn path's
[outline](learn/README.md) lists what each page introduces and the
pages it needs; check a new page against it.

> Bad: a page on reading records that opens with named queries and
> entities, when neither has been explained.
>
> Good: "An entity, a kind of stored record that page 4 introduces, says
> who may read and change its records."

### Say what something is for, then show it, then the rule

Lead with why the reader wants it, then the smallest code that works,
then what it shows. A rule with no example is a rule nobody can check
they followed.

> Bad: `subject:` names a unique field of a stored entity, one every
> tenant shares when the application has a tenant (area 9).
>
> Good: Jadpo needs to know which record a signed-in person is. `subject:`
> names the field it finds them by, such as `User.email`.

### One idea per paragraph, and sentences that read

A paragraph makes one point. A sentence carries one fact, or two joined
by "and" or "so". Don't stack clauses, and don't cut words to save
space: a telegraphic sentence costs the reader more time, not less.

> Bad: Calls are bare in their own domain; another domain's `public`
> actions are called through it, `billing.charge(...)`.
>
> Good: Inside a domain, call an action by its name: `charge(order)`. To
> call another domain's action, put the domain's name first:
> `billing.charge(order)`.

A page that brings in more than about eight new ideas is two pages.

### Use everyday words, and define each term the first time

Write "stored record", not "persisted aggregate"; "refuses", not
"rejects with a diagnostic". When a Jadpo term is the right word, define
it in the sentence that first uses it, or link the topic that does.
Where TypeScript has a near equivalent, name it: `var` is like `const`.

### Link by name, never by number

Readers can't follow "area 8" or "question 11". Link the topic by its
title. Design history and rationale belong in the design notes.

### Say what isn't built in one note at the end

A user page describes what works today, in the present tense. What isn't
built goes in one section at the end, titled exactly `## Not built yet`,
saying what to do instead. Nothing else on the page talks about build
status.

> Bad: (first line of the page) Not built yet: `providers:` is
> `toolchain.not_built_yet`.
>
> Good: (last section) Signing in with Google or GitHub isn't built yet.
> Use magic links or passkeys for now.

### Write to the reader, in the present tense

"You", "your app", "Jadpo checks". Not "the app author" or "will be".
Say what happens, not what was decided.

### Name the task or the thing in a heading

A heading is something a reader would search for: a thing ("Magic
links") or a task ("Store and change a record"). Never a position
("Capabilities on this page") or an internal label.

### Common case first, limits last

The setting most apps use, then the one most apps change, then the
limits, the security edge cases and the rare options.

### Compare only to explain

Mention another tool when it helps a reader map what they know ("like a
Zod schema, but checked when you compile"). Don't cite other frameworks
to justify a choice; that belongs in the design notes.

## Code and commands

### Every example compiles

Code in a user page is a `jadpo check` block, compiled and, when it has
tests, run by the gate. A user page has no `fragment`. Show a command
with the output it gives, copied from a real run. If you can't make an
example compile, the feature isn't ready for a user page.

### Put each file's path above its code

Above every block of a file, write its path as plain text, relative to
the project's folder: `domains/todos/actions/add_todo.jadpo`:. A block's
fence attributes (`project=`, `file=`) are for the docs check and aren't
shown, so they never count as telling the reader.

### Mark what changed when a file is shown again

When a page shows a file the reader already has, say above it which
lines are new or changed, or "replace the whole file". Never make the
reader compare two copies by eye.

### Say who writes a generated file

When a block shows a file a command writes, say so beside its path, and
that it isn't to be copied: `api/main.jadpo_record` (written by
`jadpo api update`). "Never edit this by hand" always names who does
write it.

### State a repeated step once

When the same fix follows every change, as `jadpo api update` does after
a route changes, say it once as a standing rule and refer to it after
that, rather than explaining it on every page.

### Examples that still work next year

A date in an example request must not pass: use one far ahead, such as
2099, or a test's `clock:`. When the reader must use a value of their
own, such as an id, a token or a password, say so beside it.

### No secrets on a command line

An example never puts a password or a key on a command line, where a
shell's history keeps it. Show a prompt that asks for it, or a file only
the reader can read.

### Send readers somewhere they can open

A user page links pages of the docs, by topic, never a path in this
repository (`examples/...`), which a reader in their own project can't
open. Where the compiler prints such a path, as in an error's `help:`
line, say what to open instead: `https://jadpo.dev/docs/errors/<rule>`.

## Kinds of user page

- **Learn path** (`docs/learn/`): one todo app, built a page at a time
  from an empty folder. Each page adds one idea, ends with a project that
  checks and runs, and shows the commands and what they print. A page
  shows only the files it adds or changes; the docs check builds its
  project on the pages before it, and front matter `removes:` names a
  file the page deletes.
- **Guide**: one job, such as "Add sign-in": where you start, the steps,
  the result, and links to the reference topics.
- **Primer** (`docs/primer.md`): the one page an agent reads before
  writing an app, 3,000 to 5,000 tokens.

Each user page declares its topic ID on the line after its `#` heading,
as area pages do.

## Area pages

Area pages keep every rule, limit and edge case. Where an area page is
the only source for something, its author-facing topics read like a
user page: what the thing is for and an example first, then the rules,
then the edge cases. Other areas are topic links by title, "question N"
goes to the design notes, and build status is only in the page's "What
is built" table.

## What the docs check enforces

`python3 tools/docs/language_docs.py --check`, part of the gate, fails a
user page that:

- mentions an area or question by number;
- talks about build status anywhere but a final `## Not built yet`
  section;
- uses spec vocabulary ("Decided.", "carried to");
- links a file in this repository rather than a page of the docs;
- holds a `fragment`, or an example that doesn't compile or whose tests
  fail.

The rest of this guide is for the writer and the reviewer.
