# Jadpo's documentation

How Jadpo's documentation is written, built and delivered, to people and to
coding agents. The site at jadpo.dev is built in its own repository from
what this one publishes; this page is the contract between the two.

Decided. The structure and the bundle are built; the primer, releases,
installers and the site are not yet, and the [roadmap](roadmap.md) says
when each comes.

## Who reads it

- **Agents writing Jadpo apps.** They read the least they can: a primer,
  then the few sections a task needs, then an error's page when the
  compiler reports one. No model has seen Jadpo, so the docs are their
  only source.
- **People writing Jadpo apps.** The same content as pages, navigated by
  task.
- **People and agents changing the language.** The area pages in full,
  with their design notes.

## Where the content lives

In this repository, versioned with the compiler. The gate compiles every
example on every page, a future `jadpo docs` ships exactly its own
version's docs, and compiler errors name rule IDs that must match the error
pages; none of that holds if the content lives anywhere else. The site
repository renders and hosts; it never edits content.

An area page keeps every rule an app author needs, and a short "What is
built" table near its end; its notes hold the Decisions table, how the area
fits the rest of the language, what it carried to other areas and the
questions still open. Every compiler rule has its page, one only a command
raises saying "No example yet." until it has one, and a diagnostic's
`help:` line names that page (`docs/errors/failure.kind_required.md`); its
area page lists it under `## Errors`.

| Content | Source |
|---|---|
| Primer: layout, syntax essentials, the constructs nearly every app uses, what is built, where to look next; about 3,000 to 5,000 tokens | `docs/primer.md` |
| The language, by topic | the area pages, `docs/language/NN-*.md` |
| One page per compiler error | `docs/errors/<rule.id>.md` (examples), with the rule's summary from the compiler's catalogue (`jadpo help --json`) |
| Why the language is the way it is | each area's design notes, `docs/language/notes/NN-*.md`, kept apart from what an app author reads |
| Commands | generated from `jadpo help --json` |
| Standard types | generated from the compiler's contract data (roadmap, after the first build, item 2) |
| Teaching projects | `examples/` |

## Topic IDs

Every section an app author reads has a topic ID: lowercase kebab case,
unique across the docs, independent of the area that holds it
(`translations`, `migrations`, `passkeys`). An ID is declared in the source
next to its heading, never changes, and is never reused. Moving a section to
another area, or regrouping the areas, changes no ID and breaks no link.
Error pages use the rule ID itself (`data.migration_missing`), commands
`jadpo-<command>` (`jadpo-check`) and the primer `primer`.

The declaration is an HTML comment on the line after the heading, which
GitHub doesn't show and which leaves the heading's own anchor alone:

```markdown
## Declaring a failure
<!-- topic: declaring-a-failure -->
```

Each `#` and `##` heading of an area page is a topic, but `## Errors`,
which only lists the area's error pages; a topic runs to the next. The
docs check (`tools/language_docs.py`) fails on a topic with no ID, an ID
that isn't kebab case or is used twice, an ID line anywhere else, and a
topic over 3,000 words as the bundle gives it, which is split.

## The bundle

Each release, and each merge to `main`, produces a docs bundle: the
built form the site and `jadpo docs` both read, so neither parses the area
pages.

```text
bundle/
  index.json                   one entry per page
  primer.md
  topics/<id>.md               one per topic ID
  errors/<rule.id>.md          one per catalogue rule, example or not
  commands/jadpo-<command>.md  generated from jadpo help --json
```

`index.json` is `{ "version": "<jadpo's version>", "pages": [...] }`, in
the order above, topics in the areas' order. Each entry has `id`, `kind`
(`primer`, `topic`, `error`, `command`), `title`, a one-line `summary` (a
topic's first sentence, a rule's catalogue summary, a command's own),
`tokens` (an estimate: a quarter of the page's characters), `hash`
(`sha256:` and the hex digest of the page's markdown), the `area` it comes
from (`null` for the primer, 14 for a command), and the IDs it `links` to.
A page's path follows from its kind and ID.

Pages are plain markdown. A topic's heading is its page's `#`, its hidden
blocks (`hidden`, the support code its examples compile with) are left
out, and its relative links are by ID (`passkeys.md`,
`../errors/failure.kind_required.md`), keeping an anchor within the page;
a link to anything else, design notes included, names the file on GitHub's
`main`. An error page leads with its rule's summary, area and who usually
decides the next step; a command's page is its usage, arguments, flags,
examples and errors. The primer joins the bundle once `docs/primer.md`
exists.

`python3 tools/language_docs.py --bundle <dir>` builds it; the gate builds
it into `build/docs-bundle/`, and on `main` CI keeps that folder as the
`docs-bundle` artifact. A release attaches the bundle to its GitHub
Release.

## What the site serves

| Path | What |
|---|---|
| `/llms.txt` | A short index: what Jadpo is, the primer's URL, how to find a page by ID or by rule, the version |
| `/docs/primer`, `/docs/primer.md` | The primer, as HTML and as markdown |
| `/docs/<id>`, `/docs/<id>.md` | A topic |
| `/docs/errors/<rule.id>`, `.md` | An error's page; reached from a compiler error, not listed in navigation |
| `/docs/index.json` | The bundle's index |
| `/v/<version>/…` | The same paths for one release; immutable, cached for good |
| `/…` without `/v/` | The latest release; short cache, `ETag` from the page's hash |

The markdown copy is the bundle's file served as `text/markdown`. There is
no single file of all the docs: no agent can use a million tokens. People
navigate by task (build an API, store data, sign people in, call another
service, run background work, test, deploy), built over topic IDs; the area
order stays the source's.

## How an agent uses it

1. A project made by `jadpo new` holds a short section in its `AGENTS.md`:
   what Jadpo is, the version the app declares in `app.jadpo`, and where the
   docs are. Agents load that file on their own.
2. The agent reads the primer once per session; it is the same every time,
   so prompt caching keeps it cheap.
3. For a task it reads `index.json` (or `llms.txt`), picks the topics by
   title and summary, and fetches only their `.md`.
4. A compiler error names its rule; the agent reads that rule's page only if
   the error's own reason and fix are not enough.
5. It reads the version its app declares, under `/v/<version>/`, and skips
   pages whose hash it already has.

Until the site is live, `llms.txt` in this repository links the raw files on
GitHub; some agents cannot fetch from GitHub, which is why the site and
`jadpo docs` exist.

## Versions and releases

- One version for the language, compiler and runtime, in semantic
  versioning from 0.x: until 1.0, a breaking change raises the minor
  number.
- Each pull request's Conventional Commit type sets the change: `fix` a
  patch, `feat` a minor, `feat!` a breaking change. Claude chooses it, and
  CI fails a pull request whose language behaviour changes (a spec
  example's result, or a rule entering or leaving the catalogue) under a
  type that says it doesn't.
- release-please keeps a release pull request with the next version and
  its notes; merging it tags `vX.Y.Z` and makes the GitHub Release. Claude
  merges it, except 0.1.0, which Richard sees first. Release notes live on
  the Release, not in a changelog file.
- A tag builds the installers and the bundle, and the site publishes
  `/v/X.Y.Z/`.
- An app declares the version it needs in `app.jadpo`, and the compiler
  refuses an app needing a newer one.

## Installing

Prebuilt `jadpo` binaries, with a `curl | sh` installer, Homebrew,
PowerShell and an npm wrapper (`npx jadpo`), so nobody installs Rust to
check, test or run an app. `jadpo build` fetches its own pinned Rust
toolchain into its cache on first use.

## Not decided yet

- How `jadpo` serves the bundle itself: `jadpo docs <id>` and
  `jadpo docs search`, the `jadpo mcp` tools, or both. Either serves the
  bundle above, so the site doesn't wait on it.
- Task guides beyond the primer, after the roadmap's measurement of how
  easily agents write Jadpo shows whether they are needed.
- Hosting, which the site's own repository chooses.
