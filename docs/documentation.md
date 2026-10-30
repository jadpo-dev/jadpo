# Jadpo's documentation

How Jadpo's documentation is written, built and delivered, to people and to
coding agents. The site at jadpo.dev is built in its own repository from
the docs bundle this one publishes; this page is the contract between the
two. How the pages themselves are written is the
[writing guide](writing.md). Releases are switched off until the steps
under [Versions and releases](#versions-and-releases) are done.

## Who reads it

- **Agents writing Jadpo apps.** They read the least they can: a primer,
  then the few sections a task needs, then an error's page when the
  compiler reports one. No model has seen Jadpo, so the docs are their
  only source.
- **People new to Jadpo.** They start on the [Learn path](learn/README.md),
  which builds one app a page at a time in the order its ideas depend on
  each other, never on the area pages.
- **People writing Jadpo apps.** The same content as pages: the Learn
  path, then the reference by topic.
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
`help:` line is that page's address on the site
(`https://jadpo.dev/docs/errors/failure.kind_required`); its
area page lists it under `## Errors`.

| Content | Source |
|---|---|
| Primer: layout, syntax essentials, the constructs nearly every app uses, a project to run, where to look next; about 3,000 to 5,000 tokens | `docs/primer.md` |
| The Learn path: its outline, then one page per step of one app | `docs/learn/README.md`, `docs/learn/NN-*.md` |
| How user pages are written | `docs/writing.md` |
| The language, by topic | the area pages, `docs/language/NN-*.md` |
| One page per compiler error | `docs/errors/<rule.id>.md` (examples), with the rule's summary from the compiler's catalogue (`jadpo help --json`) |
| Why the language is the way it is | each area's design notes, `docs/language/notes/NN-*.md`, kept apart from what an app author reads |
| Commands | generated from `jadpo help --json` |
| Standard types | generated from the compiler's contract data (roadmap, after the first build, item 2) |
| Teaching projects | `examples/` |
| The agent skill: how a coding agent writes Jadpo apps | `docs/agent-skill/SKILL.md` |

## Topic IDs

Every section an app author reads has a topic ID: lowercase kebab case,
unique across the docs, independent of the area that holds it
(`translations`, `migrations`, `passkeys`). An ID is declared in the source
next to its heading and is never reused. Moving a section to another area,
or regrouping the areas, changes no ID and breaks no link. An ID changes
only when it reads badly, and then the old one stays in the declaration,
reaching the same page for good:

```markdown
## Capabilities the built-in services use
<!-- topic: built-in-service-capabilities (was capabilities-on-this-page) -->
```

Error pages use the rule ID itself (`data.migration_missing`), commands
`jadpo-<command>` (`jadpo-check`) and the primer `primer`. A Learn page
is one topic: its ID stands on the line after its `#` heading
(`learn-first-route`), and the outline's is `learn`.

The declaration is an HTML comment on the line after the heading, which
GitHub doesn't show and which leaves the heading's own anchor alone:

```markdown
## Declaring a failure
<!-- topic: declaring-a-failure -->
```

Each `#` and `##` heading of an area page is a topic, but `## Errors`,
which only lists the area's error pages; a topic runs to the next. The
docs check (`tools/docs/language_docs.py`) fails on a topic with no ID, an ID
or old ID that isn't kebab case or is used twice, an ID line anywhere else,
and a topic over 3,000 words as the bundle gives it, which is split. It
also fails an area page that names an area or a question by number, as
in "(area 8)", "area 8's" or "question 2", outside code. The design notes
are exempt, since that is where question numbers belong.

## The bundle

Each release, and each merge to `main`, produces a docs bundle: the
built form the site and `jadpo docs` both read, so neither parses the area
pages.

```text
bundle/
  index.json                   one entry per page
  primer.md
  learn/<id>.md                the Learn path: the outline, then each page
  topics/<id>.md               one per topic ID
  errors/<rule.id>.md          one per catalogue rule, example or not
  commands/jadpo-<command>.md  generated from jadpo help --json
  skills/jadpo/SKILL.md        the agent skill, outside index.json
```

`index.json` is `{ "version": "<jadpo's version>", "pages": [...] }`, in
the order above: the primer, the Learn path in reading order, topics in
the areas' order, errors, commands. Each entry has `id`, `kind`
(`primer`, `learn`, `topic`, `error`, `command`), `title`, a one-line
`summary` (a page's first sentence, a rule's catalogue summary, a
command's own), `tokens` (an estimate: a quarter of the page's
characters), `hash` (`sha256:` and the hex digest of the page's
markdown), the `area` it comes from (`null` for the primer and the Learn
path, 14 for a command), and the IDs it `links` to. A renamed topic's
entry also has `aliases`, its old IDs; no other entry has the field. A
page's path follows
from its kind and ID. `learn` pages are the user layer, and `topic`
pages the reference; each area's "What is built" topic is titled
`What is built: <area>`.

Pages are plain markdown. A topic's heading is its page's `#`, its hidden
blocks (`hidden`, the support code its examples compile with) are left
out, a Learn page's front matter too, each code fence keeps only its
language (`jadpo`, `text`), the docs check's attributes after it
(`check project=… file=…`, `fragment`, `output`) left out, so the site
shows the fence as it is, and its relative links are by ID (`passkeys.md`,
`../errors/failure.kind_required.md`), keeping an anchor within the page;
a link to anything else, design notes included, names the file on GitHub's
`main`. An error page leads with its rule's summary, area and who usually
decides the next step; a command's page is its usage, arguments, flags,
examples, the flags every command takes and what `--json` gives (the
same on each page), and errors. The primer is `docs/primer.md` with its hidden blocks
left out; the docs check compiles its every example, refuses a `fragment`
in it and keeps it to 3,000 to 5,000 tokens, its words times 1.3. A
Learn page shows only the files it adds or changes; the docs check
compiles and tests its project as the pages before it built it, less
what its front matter `removes:`, making the migrations and the API
record as the reader's commands do. The primer and the Learn path also
keep the [writing guide](writing.md)'s mechanical rules.

`python3 tools/docs/language_docs.py --bundle <dir>` builds it; the gate builds
it into `build/docs-bundle/`, and on `main` CI keeps that folder as the
`docs-bundle` artifact. A release attaches the bundle to its GitHub
Release.

## What the site serves

| Path | What |
|---|---|
| `/llms.txt` | A short index: what Jadpo is, the primer's URL, how to find a page by ID or by rule, the version |
| `/docs/primer`, `/docs/primer.md` | The primer, as HTML and as markdown |
| `/learn`, `/learn/<id>`, `.md` | The Learn path: the outline, then each page, with previous and next in the index's order |
| `/docs/<id>`, `/docs/<id>.md` | A topic |
| `/docs/<old-id>`, `.md` | A permanent redirect to the topic whose `aliases` hold it |
| `/docs/errors/<rule.id>`, `.md` | An error's page; reached from a compiler error, not listed in navigation |
| `/docs/index.json` | The bundle's index |
| `/.well-known/agent-skills/jadpo/SKILL.md`, `/.well-known/agent-skills/index.json` | The agent skill, and the index that lists it |
| `/v/<version>/…` | The same paths for one release; immutable, cached for good |
| `/…` without `/v/` | The latest release; short cache, `ETag` from the page's hash |

The markdown copy is the bundle's file served as `text/markdown`. There is
no single file of all the docs: no agent can use a million tokens. A
newcomer is sent to the Learn path first; the area pages are the
reference, in the areas' order. Task guides will come from the bundle
too, as a page kind of their own ([roadmap](roadmap.md), D11); until
then the site keeps its own task list.

## The agent skill

`docs/agent-skill/SKILL.md` is a skill in the standard `SKILL.md` format,
for a coding agent that writes Jadpo apps (it does not cover changing
Jadpo). Its front matter has a `name`, `jadpo`, and a one-line
`description`; its body tells the agent to read the primer first, find
pages through `index.json` or `llms.txt`, run `jadpo check --json` and
`jadpo test --json`, read the error document and what `(agent)` and
`(human)` mean, and report a bug in Jadpo through the user, as "Using
Jadpo" in `AGENTS.md` says. It is versioned with the language: a release's
bundle holds the skill of that release. Its links are full URLs, because
the bundle's text is served as it is.

The bundle holds it at `skills/jadpo/SKILL.md`, a byte-for-byte copy and
no entry in `index.json`. The site serves that file as it is (the
`ETag` from its hash) at `/.well-known/agent-skills/jadpo/SKILL.md`, and
builds `/.well-known/agent-skills/index.json` from the front matter: for
each skill its `name`, `description`, `type` (`skill-md`), `url` and
`digest` (`sha256:` and the hex digest of the file). `/v/<version>/…`
serves the release's own copy. The site's `llms.txt` links the skill.

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

`llms.txt` in this repository links the raw files on GitHub, for an agent
working from a checkout; some agents cannot fetch from GitHub, which is
why the site serves the bundle.

### An assistant that cannot fetch the site

In the Gemini web app on a computer, use **Add file, More uploads, Import
code** and enter `https://github.com/richardwillars/jadpo`. Google's
[GitHub import documentation](https://support.google.com/gemini/answer/16176929?hl=en)
says the GitHub app cannot read a repository merely from a URL in the
prompt. An import takes one repository per chat, up to 5,000 files and
100 MB, and is a snapshot that does not sync later changes.

The Gemini API's [URL context tool](https://ai.google.dev/gemini-api/docs/url-context)
fetches the public URLs you supply but does not follow their nested links.
Supply the individual pages the task needs and check that they were
retrieved. If neither works, attach the primer and the pages the task
needs, or use a local clone with a file-reading coding agent.

## Versions and releases

- One version for the language, compiler and runtime, in semantic
  versioning from 0.1.0: until 1.0, a breaking change raises the minor
  number.
- Each pull request's Conventional Commit type sets the change: `fix` a
  patch, `feat` a minor, `feat!` a breaking change. Claude chooses it and
  writes it as the pull request's title. A pull request merges with
  squash: one commit on `main`, its message the
  title, which is what release-please reads. CI
  (`tools/checks/commit_type.py`, in `.github/workflows/commit-type.yml`) fails a
  title that isn't a Conventional Commit and a language change under a title other than `feat` or `feat!`: a spec
  example giving a different result for the same source (its `expect=` or
  its output block, on any page under `docs/` or in `README.md`), or a
  rule entering or leaving the catalogue.
- release-please (`release-please-config.json`,
  `.github/workflows/release-please.yml`) keeps a release pull request with
  the next version and its notes; merging it tags `vX.Y.Z` and makes a
  draft GitHub Release holding the notes. Claude merges it, except 0.1.0,
  which a maintainer checks first. Release notes live on the Release, not in a
  changelog file.
- The release then gets the docs bundle as `docs-bundle.tar.gz`, and
  dist's workflow (`.github/workflows/release.yml`) builds the installers,
  attaches them, publishes the release, then the Homebrew formula and the
  npm package. The site publishes `/v/X.Y.Z/`.
- The VS Code extension (`editors/vscode`) is packaged at the release's
  version and attached to the release, then published to the VS Code
  Marketplace and Open VSX once the release is out
  (`.github/workflows/vscode-extension.yml`); release-please keeps its
  `package.json` version in step.
- An app declares the version it needs in `app.jadpo`, as
  `application { jadpo: "0.1" }`, and the compiler refuses an app needing a
  newer one (area 1, "The root").
- Releases need these, set by a maintainer; each part is off until its
  switch is set:
  - release-please runs only when the repository variable `JADPO_RELEASES`
    is `on`, with a `RELEASE_PLEASE_TOKEN` secret, so its pull requests run
    CI.
  - dist's workflow publishes the Homebrew formula (`jadpo.rb`) to the
    `jadpo-dev/homebrew-tap` repository with a `HOMEBREW_TAP_TOKEN`
    secret that can write to it (users run `brew install jadpo-dev/tap/jadpo`),
    and the npm package `@jadpo/jadpo` to npmjs.com
    through trusted publishing (OIDC), which needs no token: npmjs.com is
    configured to trust `jadpo-dev/jadpo`'s `release.yml` for the package.
    Both run after the release is published and skip prereleases.
  - The extension is published only when the repository variable
    `JADPO_VSCODE_PUBLISH` is `on`, with the Marketplace publisher `jadpo`
    and its `VSCE_PAT` secret, and the Open VSX namespace `jadpo` with a
    trusted publisher for `vscode-extension.yml`, which needs no secret.

## Installing

Each release carries prebuilt `jadpo` binaries for macOS (arm64 and
x86_64), Linux (x86_64 and arm64, glibc) and Windows (x86_64), built by
dist (`dist-workspace.toml`, its version pinned there) with OpenSSL
compiled in, so nobody installs Rust to check, test or run an app. Beside
them: a shell installer (`jadpo-cli-installer.sh`, for `curl | sh`), a
PowerShell one (`jadpo-cli-installer.ps1`), a Homebrew formula
(`jadpo.rb`) and an npm package (`@jadpo/jadpo`, for `npx @jadpo/jadpo`). `jadpo build`
fetches its own pinned Rust toolchain into its cache on first use.

The installed `jadpo` is meant to be a launcher that runs the release line
`app.jadpo` names, fetched into a per-user cache and verified first; it is
not built yet, and 0.1.0 is a plain binary (area 14, "Which jadpo runs").

Each archive has a SHA-256 beside it, and each of those files, like the
VS Code extension's `.vsix`, has a GitHub artifact attestation saying
which workflow run in `jadpo-dev/jadpo` built it, from which commit.
Check one before running it, with the GitHub CLI:

```sh
gh attestation verify jadpo-cli-aarch64-apple-darwin.tar.xz --repo jadpo-dev/jadpo
```

## Not decided yet

- How `jadpo` serves the bundle itself: `jadpo docs <id>` and
  `jadpo docs search`, the `jadpo mcp` tools, or both. Either serves the
  bundle above, so the site doesn't wait on it.

