# Writing Jadpo's docs

How the pages people and coding agents read are written. Both read the
same prose, and both do best with the same thing: plain words, one idea
at a time, and an example they can copy.

## Two layers

- **The user layer** teaches: the [Learn path](learn/README.md), the
  [primer](primer.md) and the [guides](guides/README.md). A newcomer starts on the Learn
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
open. An error's `help:` line is its page's address on the site,
`https://jadpo.dev/docs/errors/<rule>`, which a reader opens as it is.

## Kinds of user page

- **Learn path** (`docs/learn/`): one todo app, built a page at a time
  from an empty folder. Each page adds one idea, ends with a project that
  checks and runs, and shows the commands and what they print. A page
  shows only the files it adds or changes; the docs check builds its
  project on the pages before it, and front matter `removes:` names a
  file the page deletes.
- **Guide** (`docs/guides/`): one job, such as "Add sign-in": where you
  start, the steps, the result, and links to the reference topics. Each
  guide is a whole project of its own, built on no other page. It shows
  the files the job adds or changes, and the files it starts from are
  `hidden` blocks, which the docs check compiles and the bundle leaves
  out; its first paragraph after the opening says in words what those
  are. Guides don't depend on each other, so each says what it starts
  from rather than "as the last guide did".
- **Primer** (`docs/primer.md`): the one page an agent reads before
  writing an app, 3,000 to 5,000 tokens.

Each user page declares its topic ID on the line after its `#` heading,
as area pages do.

## How a reference topic reads

Area pages keep every rule, limit and edge case. Each `##` topic on them
is still read on its own, by someone who arrived from search or from a
compiler error, so its author-facing text reads like a user page. The
standard is the topic "Routes: signed in by default" on the
[Identity](language/08-identity.md#routes-signed-in-by-default) page:

- **Say first what happens by default.** "Every route needs a signed-in
  caller unless it says otherwise. You don't write anything for that."
  The reader learns what they get for free before any option.
- **One section per option**, as a `###` heading naming it: when to use
  it, then the example, then what it does. "Use it for routes people
  call before they have an account, such as a health check", then the
  route, then what changes.
- **Explain every term where it's used, or link it.** A
  `revocation_delay` gets a sentence saying what it is and what it costs
  before the topic relies on it.
- **Explanations go in prose, not in code comments.** A comment beside a
  line is easy to skip and can't hold a link. Keep comments to a file's
  path, or a note the code itself needs.
- **Link by name.** "[Policy](language/09-policy.md) still decides",
  never "area 9 still decides". "Question N" and design history go to the
  design notes in `docs/language/notes/`.
- **Say constraints in plain words.** "A route can narrow its API's list
  but never add to it" says what a reader can and can't do; "narrow"
  alone doesn't.
- **Check every fact against the page.** A rewrite keeps every rule as it
  is. Where the plain version would change what a rule means, keep the
  rule's wording.
- **Build status** is in the page's "What is built" section, or one note
  at the end of the topic, never in its first lines.

The design notes keep the Decisions table, the open questions and why
the language is the way it is; nothing is deleted on its way there.

## The `(human)` and `(agent)` markers

Every page that explains an error's `next:` line uses the same words:

> `(human)` at the end of an error's `next:` line tells your coding agent
> to stop and ask you which fix you want, because the choice is yours to
> make (who can see data, what a setting holds). `(agent)` means your
> agent can make the fix itself.

The marker is guidance that agents follow, and nothing enforces it yet.
The primer says so once; other pages don't repeat it.

## How an error page reads

A page in `docs/errors/` is reached from a compiler error, by someone in
the middle of a change. It has these parts, in this order:

1. **What happened**, under the rule's heading, in one plain sentence:
   "A route that anyone may call, with no sign-in, reads or changes a
   record that only signed-in people are allowed to see."
2. **`## Why Jadpo refuses it`**: the two things that disagree, or the
   harm the rule prevents, and why Jadpo won't pick the fix for you when
   it won't.
3. **`## What it looks like`**: the smallest example that raises the
   error, compiled by the gate, and its output.
4. **What the marker means**: after the output, what the next step's
   `(human)` or `(agent)` means for this error, in the standard wording
   above and with what the choice decides here ("the choice decides who
   can see your data").
5. **`## How to fix it`**: the question that picks the fix, if there is
   one, then each fix as "**If …**", its code (only the lines that
   change), and what it changes once made ("Callers who aren't signed in
   now get a 401").
6. **`## Related`**: the topics it touches, linked by title.

The sentence saying what happened is the rule's summary in the compiler's
catalogue (`crates/jadpo-diag`), word for word, with a full stop; the
catalogue keeps it without one. It starts with a capital letter and says
what the reader did, not the rule's name: "An API's `serve_openapi:`
setting holds something other than `true` or `false`." The bundle and
`jadpo help <rule>` give it once, in the page's head.

A rule raised in more than one way shows each further case after the
first one's output and marker, each introduced by a sentence saying how
it differs, with its own example and output.

The marker is explained in one of these two sentences, then what the
choice decides here, as in "…yours to make: it decides who may call
`check_health`.":

- "`(human)` at the end of the `next:` line tells your coding agent to
  stop and ask you which fix you want, because the choice is yours to
  make"
- "`(agent)` at the end of the `next:` line means your coding agent can
  make the fix itself"

A fix's code is the lines that change, as a `fragment`, followed by a
`hidden` check block of the fixed file, holding those lines, and hidden
blocks for the project's other files, so the gate compiles every fix.
The fragment parses on its own: it keeps the lines that open and close
what the change is in, and `...` stands for the rest. A
file of a few lines can instead be shown whole, as a `check` block. A fix
that is a command shows it as `sh`. What a fix changes is only what the
area page or a checked example shows; a page never describes behaviour
that isn't built.

An error only a command raises, never `jadpo check` or `jadpo test`,
shows the command as a `sh run` block and what it printed as the `text
output` block after it. The docs check runs the block's lines with
`sh -e` in a fresh copy of its project, the `check` blocks with the same
`project=` (usually `hidden`), and compares what they print; `--bless`
writes it. `expect=<rule>` says the commands end failing with that
rule's diagnostic and no other; without it they succeed. `tty` runs them
in a terminal and types `typed=<value>`, or nothing, then Enter, at the
first prompt. `database` gives the commands a new, empty database of
their own on the docs check's PostgreSQL, its address in `DATABASE_URL`,
which the project's connection reads through
`url: config.database_url`. The run has this checkout's `jadpo`, an empty home folder
and nothing of the machine's environment, so it prints the same
everywhere: the project's folder prints as `.`, the home folder as `~`,
and a run's duration is left out. What the error needs, such as a link,
a read-only folder or a file that isn't UTF-8, is made by the lines
before the command, where the reader sees it. A fix that needs the
error's state first is shown as a plain `sh` block, without output.

A rule no run can raise on demand, because it needs a race, a network
that fails, a machine that can't build, a database server without an
extension or `jadpo dev` running beside the command, or whose run can't
print the same twice, because it shows a random id or a build id that
changes with the compiler, shows the command as a plain `sh` block and
output copied from a real run as a plain `text` block. The project's
files are still `check` blocks, which the gate compiles.
The sentence before them says the output was copied rather than checked
and what the run was, such as "a build on a Mac with no C compiler",
with any state written into the database by hand, and the copy has the run's folders written as `.` and `~`, as the check
writes them. Nothing else in it is edited.

A run whose error needs a read-only file or folder, made with `chmod`,
shows nothing true as root, who can read and write anything, so the
docs check refuses to run it as root and asks for an ordinary user.

## What the docs check enforces

An error page has the parts above, in order: its sections, the catalogue
summary as its first sentence, an example of the error with its output,
the marker in the standard wording, each fix as "**If …**" with its code,
a fragment compiled by the hidden block after it, and what the fix
changes, and at least one related topic. Pages whose rule a person
decides, or that the user layer or a teaching example names, must have
them; the others are being brought to the format. It reads like a user
page, so the rules below hold for it too, but for the fix's fragments.

`python3 tools/docs/language_docs.py --check`, part of the gate, fails a
user page that:

- mentions an area or question by number;
- talks about build status anywhere but a final `## Not built yet`
  section;
- uses spec vocabulary ("Decided.", "carried to");
- links a file in this repository rather than a page of the docs;
- holds a `fragment`, or an example that doesn't compile or whose tests
  fail, or a `sh run` block that doesn't print its output block.

The rest of this guide is for the writer and the reviewer.
