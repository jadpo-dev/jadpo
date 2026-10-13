# Developer tooling and LLM presentation workstream

**Status:** DX0.5 and initial DX1 compiler-backed local tooling implemented;
generated-artifact navigation and renderer adapter sources implemented;
renderer-host previews and TOOL-005 formatter conformance remain open
**Timing:** thin highlighting may begin during P10–P11; DX0.5 is required
during P10.5 before P11 application authoring; the minimum tooling baseline
must be frozen before the P12 comparison

The language is intended for agent-primary authorship, but weak human tooling
would still distort review, debugging, adoption, and the eventual comparison
with TypeScript. Editor and LLM presentation support are therefore part of the
product hypothesis rather than optional polish.

## 1. One compiler-backed language service

The semantic compiler remains authoritative. Editor integrations must not grow
a second parser, type system, or diagnostic catalogue that can disagree with
`jadpo check`.

The language server uses standard LSP over stdio and reuses the
compiler's source ranges, syntax tree, semantic graph, inferred expression
types, and structured diagnostics. Its delivery order is:

1. live parse/type/failure/effect diagnostics with exact CLI parity;
2. document symbols, go-to-definition, references, and workspace symbols;
3. hover information for types, fields, actions, routes, failures, constraints,
   effects, and generated behaviour;
4. context-aware completion and signature help;
5. semantic tokens, rename, formatting, and conservative code actions;
6. explicit commands for choosing and opening any relevant generated audit or
   contract artifact without decorating source declarations or presenting
   generated target code as the normal review surface.

The VS Code extension's **Jadpo: Open Generated Artifact** command lists the
compiler artifacts present under the active project's `build/` directory. Its
curated paths cover all stable inventories, audits, validation and compatibility
contracts, OpenAPI, and diagnostic references; an authentication audit appears
only when that artifact exists.

The renderer adapters and canonical fenced examples live in
[`editors/rendering`](../editors/rendering/README.md). Shiki reuses the TextMate
grammar, while the Prism, Highlight.js, and Monaco lexical vocabularies are
derived from it. Cached Prism 1.30.0 and Shiki 4.4.3 engines have rendered all
six fixtures in an isolated local smoke check, including the expected lexical
categories and TextMate scopes. Highlight.js and Monaco still have only
registration-shape checks, and visual previews in the target renderer hosts
remain required before TOOL-003 can exit.

Hover documentation should be derived from the language reference and semantic
metadata where possible. Hand-copying descriptions into each editor extension
would create another source of truth.

## 2. Editor packaging

The first packaging target is VS Code and compatible editors because a TextMate
grammar, file association, snippets, and an LSP client provide a small portable
base. The same server should remain usable from Neovim, Helix, Emacs, and
editors with generic LSP support. JetBrains-specific packaging is justified
only after the protocol behaviour is stable.

The baseline editor package includes:

- a canonical source-file association;
- bracket, comment, indentation, and folding rules;
- a TextMate grammar for immediate lexical highlighting;
- LSP semantic-token refinement once available;
- formatter and `jadpo check` commands;
- links to the canonical language documentation; and
- extension tests using the Jadpo seed, persistence seed, and later the
  golden todo application.

The TextMate grammar is presentation-only. Its scopes must not be treated as
semantic evidence.

The package now lives in `editors/vscode`. It supplies `.jadpo` file
association, TextMate highlighting, brackets/comments/indentation, and snippets.
Its persistent client starts `jadpo lsp` and receives live diagnostics for
unsaved text, document/workspace symbols, cross-file definitions and references,
inferred-type hover, compiler-backed callable outcome hover and signature help,
contextual member completion, semantic tokens, conservative compiler-indexed
rename, and the canonical formatter. Callable declaration, reference, and call
hover shows the successful type, complete declared failures, and derived
completion/suspension semantics without exposing generated wrappers; attempted
call sites identify propagation, while exhaustive-match call sites identify
each statically known handled, mapped, or propagated failure. Inline actions
show their route-derived success and exact failure contract. Source
declarations also link to the relevant generated validation,
callable, failure-audit, or OpenAPI artifact. The extension contains no parser
or type checker.

## 3. Documentation in the editing loop

Documentation support has three layers:

- concise hover text for the declaration currently under the cursor;
- a searchable local language reference with canonical examples and diagnostic
  explanations; and
- stable documentation URLs or identifiers that diagnostics and generated
  audits can reference.

Every public diagnostic rule has a human-first summary, reason, recommended
next step, bounded alternatives, decision owner, likely causes, impact, and a
stable help identifier. Examples and repairs come from executable compile
fixtures so documentation cannot silently drift from accepted syntax.

### 3.1 Guided diagnostic contract

Diagnostic definitions live in one versioned compiler-owned catalogue. A
definition owns its lower-case dotted rule identifier, category, severity,
human templates, typed context schema, repair alternatives, decision ownership,
help identifier, fixtures, and any legacy upper-case aliases. Compiler call
sites supply typed semantic facts rather than constructing prose with arbitrary
strings.

Every error classifies the recommended next step as exactly one of:

- `automatic_fix`: a compiler-produced, revision-bound edit whose effect can be
  previewed and checked;
- `guided_choice`: a closed set of semantically valid alternatives that the
  compiler cannot choose safely; or
- `human_decision`: an intent, policy, disclosure, migration, or other protected
  choice that an implementation agent may not make.

The version-2 agent packet is concise but self-explanatory. Its canonical keys
are `schemaVersion`, `diagnosticId`, `sourceRevision`, `summary`, `reason`,
`recommendedNextStep`, `alternatives`, `ruleId`, `severity`, `location`,
`context`, `impact`, and `helpId`. Vague compiler-oriented keys such as
`primary`, `facts`, and `affected` are not part of the public contract. Context
and impact are bounded; large dependency sets return a summary, examples, and a
query identifier rather than flooding the agent context.

The human renderer leads with the complete summary, explains the cause, and
places the recommended step before alternatives. The stable rule identifier is
searchable but visually secondary. Pipeline summaries such as
`JADPO_TYPE_FAILED` become report status rather than duplicate errors, and
dependent parse/type cascades are grouped under their root diagnostic.

Interactive terminals receive a capability-aware rich renderer with restrained
severity colour, source frames, precise carets, repair ownership, exact edit
previews, alternatives, related locations, context, notes, and secondary rule
metadata. It never reformats the source excerpt. Non-interactive streams use a
stable ASCII plain renderer without ANSI escapes; `NO_COLOR` disables colour
even when colour was requested, `JADPO_ASCII=1` makes renderer-owned decoration
ASCII without altering source text, and terminal width is bounded from
`COLUMNS`. `--diagnostic-format=human|plain|json` and
`--color=auto|always|never` provide explicit control. JSON remains the
canonical LLM/automation protocol and is byte-independent of terminal styling;
`check`, `watch`, and `dev` accept diagnostic JSON explicitly, while existing
machine-oriented commands continue to emit their JSON contracts directly.

### 3.2 IDE presentation

The normative cross-audience shapes, catalogue quality gates, and the
human-owned invalid-authentication example are recorded in the
[diagnostic presentation contract](diagnostic-presentation-contract.md).

The LSP transports the same semantic diagnostic object, using standard
diagnostic `data` and code-action mechanisms rather than embedding a second
catalogue in the editor. Presentation is progressive:

- the inline squiggle covers the smallest responsible source range;
- the Problems panel shows only the plain human summary;
- hover shows the summary, reason, recommended step, bounded alternatives, and
  decision owner;
- Quick Fix places a compiler-verified preferred edit first only when that
  preference is justified, then lists other valid choices with their effects;
- automatic and multi-file fixes show a diff preview and are rejected when the
  diagnostic's source revision is stale;
- an expandable details view shows affected routes, actions, policies, and
  public contracts plus the stable help link; and
- raw diagnostic JSON remains available for tools and debugging, not as the
  normal human interface.

Protocol fixtures must cover automatic fixes, guided choices, human-decision
handoff, alternative ordering, source-revision rejection, related locations,
root-cause grouping, multi-file previews, and exact CLI/LSP semantic parity.

### 3.3 Audience and disclosure boundaries

`CompilerDiagnostic`, `PublicFailureResponse`, `OperationalLogEvent`, and
`AgentIncidentPacket` are separate compiler/runtime types. They cannot be
serialized as one another.

Public failure responses remain deliberately small: stable declared code,
static safe message, request identifier, and explicitly public detail only.
Operational events contain redacted semantic facts, correlation identifiers,
and stable operation/source-revision IDs. They support structured JSON,
OpenTelemetry, and constrained vendor adapters without exposing raw provider
errors, SQL, parameters, headers, bodies, credentials, stacks, or arbitrary
application values.

Diagnostic and logging APIs accept only compiler-approved safe value types.
Secret values are non-renderable, and an `internal` failure field is not assumed
safe for logging or an LLM. Third-party adapters apply the runtime disclosure
policy before export; vendor-side scrubbing is defence in depth rather than the
primary boundary.

For runtime investigation, a trusted local tool joins the safe event's stable
operation and source-revision IDs with the matching compiler manifest. It then
constructs a rich, bounded agent incident packet containing source location,
semantic context, impact, occurrence summary, and guided repairs without
passing customer data or secrets through the telemetry provider.

## 4. LLM and chat presentation

LLMs can already emit source in fenced Markdown blocks. Presentation quality is
controlled by the client renderer, not by the model alone. The project should
define a stable machine language identifier, separate from any eventual product
name, and use it consistently as the Markdown fence tag, file association, LSP
language ID, and grammar package ID.

The portable rendering deliverables are:

- TextMate grammar for editors and clients that consume TextMate scopes;
- adapters or generated grammars for Shiki, Prism, Highlight.js, and Monaco;
- fenced-snippet fixtures covering declarations, constraints, routes,
  persistence, failures, comments, strings, and malformed source; and
- a plain-text fallback that preserves code exactly when a host does not know
  the language identifier.

Generated-artifact navigation must not use source document links: VS Code
decorates those ranges with underlines that are visually indistinguishable from
or compete with diagnostics, and a route-sized link can underline an entire
declaration. Use CodeLens above the declaration or an explicit navigation
command instead. Ordinary source underlines are reserved for diagnostics.

Inline backtick spans are normally rendered as undifferentiated code. Rich
syntax colouring should be expected only for fenced blocks or file views whose
host has installed the grammar. An existing third-party or hosted chat client
cannot be forced to recognize the language merely by prompting the LLM.

Official Codex documentation currently states that the CLI highlights fenced
Markdown code blocks and file diffs, and documents custom themes. It does not
document custom language-grammar registration. Treat Codex/ChatGPT-specific
grammar support as an external integration request, not a compiler guarantee:
[Codex CLI customization](https://learn.chatgpt.com/docs/cli-customization).

## 5. Giving agents language-native context

Rendering is separate from comprehension. Before considering model training or
fine-tuning, provide agents with a compact, versioned language bundle:

- core grammar and accepted examples;
- semantic rules and diagnostic catalogue;
- the current compiler version and capability manifest;
- non-interactive `check`, `inspect`, `build`, and formatting commands; and
- small machine-readable queries for symbol information, hover documentation,
  definitions, references, and diagnostics.

The current packaged interface is
[`skills/jadpo-agent/SKILL.md`](../skills/jadpo-agent/SKILL.md), backed by the
CLI and standard LSP. A future plugin or MCP adapter may expose the same
compiler-backed queries, but it must not duplicate language semantics. These
interfaces should help an agent retrieve relevant context on demand rather than
inject the entire language manual into every prompt.

Agent-facing evaluation should measure whether a fresh model can:

1. identify the language and use its canonical fenced-block identifier;
2. make a requested edit using only retrieved relevant documentation;
3. use compiler diagnostics to repair invalid source; and
4. avoid editing compiler-owned generated files.

## 6. Delivery stages and gates

### DX0 — presentation baseline

- choose the stable machine language/file identifiers;
- ship file association, comments/brackets/folding, TextMate highlighting, and
  fenced-snippet fixtures;
- prove one editor and one renderer adapter display the canonical examples; and
- document plain-text fallback behaviour.

### DX0.5 — checked local development loop

DX0.5 turns the existing manual `check`, `build`, and Bun commands into the
canonical edit-feedback-run loop without adding a second semantic path:

- `jadpo check <project> --diagnostic-format=json` emits a versioned
  machine-readable envelope containing severity, stable code, message, source
  path, byte range, and notes; the existing human-readable format remains the
  default;
- `jadpo watch <project>` watches authored `.jadpo` files and other checked
  project inputs, coalesces filesystem events, and reruns the same authoritative
  check/build pipeline used by one-shot commands;
- watch output distinguishes checking, failure, successful generation, runtime
  restart, readiness, and shutdown. Lifecycle events use the same versioned
  structured envelope when JSON output is selected, so editors and agents do
  not need to infer state from prose;
- `jadpo dev <project>` performs an initial checked build, launches the
  generated Bun server, and automatically restarts or reloads it after each
  successful rebuild;
- generated output is staged and promoted atomically. A failed check or build
  never exposes a partial target and never replaces the last successful build;
- while a new edit is invalid, the last-known-good Bun server may continue
  serving, but the tool must visibly report that the running revision is stale;
- a successful revision becomes live only after Bun starts and reaches its
  configured readiness boundary. Failed startup preserves or restores the
  last-known-good process when possible and reports a structured runtime event;
- initial implementation may use a graceful Bun process restart. In-process
  hot module replacement is not required for DX0.5; observable server state
  must nevertheless advance automatically after a successful compile; and
- shutdown cleans up the watcher and child process without leaving an orphaned
  listener.

Configuration extends this same loop without adding a partial compiler check:

- `jadpo config set <field>` runs inside the project, resolves the checked
  field, and prompts the developer directly. Secret input has terminal echo
  disabled and is never accepted as an argument or printed;
- the command validates before atomically updating owner-only `.env.local`,
  preserves unrelated entries, refuses symbolic-link targets, and detects a
  concurrent change;
- `jadpo config check` reports only field names and `set`, `default`, `missing`,
  or `invalid` state;
- agents run the ordinary JSON `check` after coherent source edits, ask the
  user to run `config set` rather than paste a secret into chat, and need not
  repeat `check` immediately before `build`, `test`, or a running `dev`; and
- `dev` watches `.env.local`, validates a complete local snapshot, passes only
  declared bindings to Bun with automatic environment-file loading disabled,
  and preserves the last ready revision after an invalid replacement.

DX0.5 is implemented. Passing `--diagnostic-format=json` to
`jadpo check <project>` writes exactly one version-2 `diagnostic_report`
object to standard output and uses the ordinary success/failure exit code. The
envelope records `command`, `status`, the supplied project path, optional source/
declaration/node counts, and an ordered `diagnostics` array. Every diagnostic
contains `severity`, stable `code`, `message`, nullable `source` and byte
`range`, and `notes`. Failed frontend checks retain the same phase ordering as
the human CLI and include the terminal `JADPO_*_FAILED` diagnostic. Successful
checks may contain warnings. The default human format and compiler pipeline are
unchanged semantically; human output now resolves byte ranges to line/column,
prints the relevant source line and caret, and retains bounded repair notes.

`jadpo watch <project>` now performs an immediate checked build and then
fingerprints authored `.jadpo` files plus `schema.identities.json`. It ignores
`build/` and compiler staging/backup directories, polls every 100 ms, and waits
for a 75 ms quiet window before treating rapid writes as one revision. Each
revision uses the same check, target derivation, and artifact-writing functions
as one-shot commands. Output is completed in a sibling staging directory before
directory promotion; a staging or promotion failure preserves or restores the
last complete `build/` revision. Human events and version-2 JSON
`lifecycle_event` objects distinguish checking, build success, build failure,
watch-input failure, monotonic sequence/revision IDs, and whether the retained
build is stale. Source creation/deletion and registry edits participate in the
snapshot. Runtime launch/readiness and `jadpo dev` are now implemented for
the initial process loop. `jadpo dev <project>` layers Bun execution on
the same loop. `PORT` defaults to 3000 and is validated before the initial
build. Every generated target has a compiler-owned `GET /health` endpoint unless
an authored public route replaces it. Bun starts with package installation
disabled; readiness requires HTTP 200 from that endpoint within three seconds,
and unexpected runtime exit is detected while waiting for edits. Invalid source revisions leave an already-ready process
running and report the retained revision as stale. A successful revision emits
build success, stops the prior process, launches the new target, and emits
runtime starting/readiness or failure. JSON mode keeps lifecycle objects on
standard output and diverts Bun standard output to standard error. A real
generated server passed readiness and an HTTP request on Bun 1.2.20, and
terminal interruption left no listener. Generated runtime and startup faults
now use compact versioned JSON diagnostics; raw generated-target stacks are
available only through the explicit `JADPO_DEBUG_TARGET_STACKS=1` debugging
escape hatch. Before a ready runtime is replaced, the supervisor preserves its
generated revision. A candidate becomes current only after readiness; failed
startup atomically restores the preserved artifacts and proves the prior
runtime ready again. Portable interruption emits an explicit `shutdown` event
after terminating and waiting for the child process. Protocol tests cover
rapid-write coalescing, invalid-edit retention, recovery, source creation and
deletion, candidate startup failure, last-known-good restoration, and clean
listener shutdown.

The protocol-level acceptance suite covers a valid edit, an invalid edit,
recovery to valid source, rapid/coalesced writes, source creation/deletion, a
generated-target failure, Bun startup failure, last-known-good continuity, and
clean shutdown. The watcher must ignore compiler-owned `build/` output so its
own writes cannot create a rebuild loop.

DX0.5 is a prerequisite for P11 application authoring. The later VS Code client
and DX1 language server must consume the same structured diagnostics and state
model rather than scrape terminal prose or create an editor-only watcher.

### DX1 — compiler-backed LSP

- diagnostics have byte/range/code/message parity with the CLI;
- definitions, references, symbols, hover, and completion work across files;
- semantic tokens refine rather than contradict lexical scopes; and
- the seed applications are covered by protocol-level tests.

**Status: complete for the initial language surface.** `jadpo lsp` implements
standard Content-Length-framed JSON-RPC over stdio with full-document sync.
Compiler byte ranges are converted to LSP UTF-16 positions, including non-ASCII
source. Protocol tests cover live malformed unsaved text, tagged variants and
payload symbols, contextual enum/record completion, callable outcome hover and
signature help, handled/mapped/propagated call-site state, inline actions,
failure-arm navigation, rename/reference sets, semantic tokens, authored tests,
UTF-16 range conversion, and imported definitions across files. Schema-identity
and index-advice diagnostics reuse the same
compiler checks after a valid frontend pass. Initial declaration-to-artifact
links cover validation plans, callable inventories, failure audits, and OpenAPI.
Diagnostic documentation is generated from the compiler-owned DX2 catalogue;
the editor remains a presentation client rather than an independent authority.

### DX1 formatter conformance follow-up

The formatter ships through the compiler and LSP. Its conformance follow-up is
tracked as [`TOOL-005`](language-issues.md#active-issues), with current rules in
the [formatter rule matrix](formatter-rules.md):

- maintain a rule matrix that maps every production and syntactic alternative
  in the accepted grammar to its canonical layout and at least one unit case;
- cover all declaration families, type and expression forms, statements,
  route/auth/config syntax, comments, tests and fixtures, and bounded
  module/import syntax, updating the matrix whenever the grammar changes;
- define exact rules for indentation, token spacing, delimiter layout,
  blank-line grouping, line endings, and the final newline;
- for every syntax case, test valid whitespace variations including excess or
  trailing spaces, tabs and mixed indentation, extra/missing blank lines, and
  additional/missing line breaks, then compare byte-for-byte with canonical
  output; and
- assert idempotence, preserve parsed structure for valid files, and retain
  string-literal bytes plus comment text and order.

The unit suite should exercise every formatter syntax branch, not enumerate
every possible whitespace byte string. Whitespace perturbations should be
table-driven by rule and remain valid token separators where the grammar
requires them. The LSP protocol suite continues to cover transport and editor
integration; it does not substitute for this formatter rule matrix.

The first implementation slice normalizes token spacing while preserving
string/comment bytes and covers eight unit tests plus all 56 current compile-pass
and 127 compile-fail source fixtures. Positive cases receive tab, spacing,
indentation, excess blank-line, removed-line-break, and LF/CRLF perturbations;
negative cases verify token preservation and stable syntax diagnostic codes,
including unknown characters. A whitespace split across the route header also
verifies compact context-sensitive path tokens.

Canonical route-item reordering is implemented, including comments attached to
their items. The rule matrix names the EBNF productions and maps them to
canonical rules plus fixture or unit evidence. TOOL-005 remains open: for
removed-line-break cases, corpus checks preserve tokens, parseability, and
idempotence but do not compare exact canonical output. The owner selected syntax-derived canonical layout on 2026-10-01 in
[FMT-005](formatter-rules.md#canonical-output-rules), superseding the previous
source-line-preservation rule. AST-guided line reconstruction and exact-output
assertions remain RM-203 work; token/comment/string and syntax-tree preservation
must remain intact.

### DX2 — guided diagnostics, agent context, and documentation

- replace scattered message construction with the central versioned diagnostic
  catalogue and readable dotted rule identifiers;
- emit version-2 guided diagnostic packets with a human-first summary,
  recommended next step, alternatives, decision ownership, bounded semantic
  context, impact, help identifiers, and revision-bound verified repairs;
- give CLI and LSP human renderers the same guidance while presenting it through
  clean Problems, hover, Quick Fix, diff-preview, and details experiences;
- generate searchable diagnostic documentation and executable examples from
  the same catalogue and fixtures;
- emit audience-specific public failures and secret-safe operational events,
  with OpenTelemetry/structured-log compatibility and constrained third-party
  adapters;
- enrich runtime events locally into rich agent incident packets through stable
  operation/source-revision IDs and the compiler manifest;
- expose a compact skill/plugin or MCP interface for on-demand grammar,
  diagnostic, symbol, impact, repair-preview, and documentation queries;
- evaluate fenced code output and source-file links in supported clients; and
- degrade to exact readable plain text in unsupported clients.

**Status: fixture completion in progress.** The compiler emits
the version-2 object through CLI JSON and LSP `data`, generates catalogue JSON
and Markdown reference artifacts, supplies revision-bound Quick Fix edits and
bounded alternatives, and renders related locations without editor-owned
rules. Generated runtimes use separate public-failure and operational-event
schemas, safe OpenTelemetry/provider mappings, stable operation/source IDs, and
trusted local `jadpo incident` enrichment. `skills/jadpo-agent/SKILL.md` exposes
the bounded grammar, diagnostic, symbol, impact, repair-preview, documentation,
and incident workflow without duplicating compiler semantics. The exhaustive
cross-audience suite is recorded in
[`tests/diagnostics/README.md`](../tests/diagnostics/README.md). All 298 public
codes now have rule-specific authored summary, reason, and next-step copy, and
every code is projected through the version-2 agent, terminal, LSP, and VS Code
contracts. A catalogue-wide plain-language gate rejects compiler-internal
phrases, while contextual semantic-name fixtures prove concrete source roles,
compatible name suggestions, and usable advice such as defining a missing
name as a type. Terminal and IDE views render each stable help identifier as a
full guidance URL. The permanent catalogue-and-evidence gate is active and
green: language diagnostics have real malformed-source or semantic scenarios,
while operational and I/O faults have explicit emitter-contract coverage in
addition to the exhaustive audience projections. Fresh-agent and first-user
repair-cycle trials remain external evidence.

**Exit gate:** every public diagnostic is catalogue-backed and fixture-backed;
common mechanical errors offer a verified one-step repair; semantic ambiguity
offers bounded alternatives rather than vague advice; protected choices name
the required human decision; CLI, JSON, LSP, and documentation agree; stale
repairs fail closed; planted secrets and customer-value canaries do not appear
in browser responses, diagnostic packets, operational events, exporter buffers,
or third-party adapter captures; and fresh-agent plus first-user trials meet the
roadmap's diagnostic-quality and two-cycle repair thresholds.

### P12 tooling freeze gate

Before freezing the language-versus-TypeScript experiment, record exactly which
editor, LSP, formatter, grammar, agent-context bundle, and renderer support each
side receives. Missing language tooling is a real product cost and must not be
hidden, but the comparison must not change tooling halfway through a run.

## 7. Explicit non-goals for the first tooling slice

- maintaining editor-specific semantic implementations;
- claiming highlighting support in clients that cannot install the grammar;
- using a vaguely similar existing language tag and presenting the result as
  correct highlighting;
- model fine-tuning before retrieval plus compiler feedback is measured; or
- allowing IDE convenience actions to bypass compiler checks or policy review.
