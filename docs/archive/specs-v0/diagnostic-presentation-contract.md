# Diagnostic presentation contract

This document is the executable presentation contract for DX2. A compiler
diagnostic is one semantic repair protocol projected into three audience views;
the CLI, agent protocol, and IDE must never maintain separate explanations.

## 1. Shared semantic object

Every public diagnostic must define, without generic fallback prose:

- a complete human summary that names the concrete problem;
- a short reason that explains the relevant language rule in source terms;
- the smallest responsible source range;
- typed, bounded context;
- exactly one recommended next step;
- zero or more bounded, semantically valid alternatives;
- `automatic_fix`, `guided_choice`, or `human_decision` repair ownership;
- behavioural and public-contract impact for every proposed edit;
- a stable lower-case dotted rule identifier and help identifier;
- a source revision for every edit; and
- legacy aliases only as secondary compatibility metadata.

Phrases such as “compiler-enforced invariant”, “unexpected token”, and “update
the source to satisfy this rule” are not acceptable final explanations. They
may identify an internal failure while developing the compiler, but they cannot
pass the public catalogue conformance suite.

Public copy is written in the author's vocabulary, not the compiler's internal
classification vocabulary:

- quote the exact authored name or value when it is safe to display;
- say what the concrete source role needs — for example, “This field needs a
  type” or “`missing:` needs a `NotFound` failure”;
- when the valid set is bounded, name representative valid values or the exact
  accepted syntax;
- suggest the nearest compatible visible name when one exists;
- make the recommended step usable on its own, such as “Define `CustomerID` as
  a type,” rather than “use the required declaration kind”; and
- reserve terms such as type, field, function, action, failure, input, output,
  route, and migration for concepts visible to the author. Internal terms such
  as callee, nominal sibling, operand shape, semantic declaration kind, and
  reachable problem set do not appear in public copy.

The plain-language conformance test applies these rules to every catalogue
entry. Scenario fixtures additionally assert exact contextual wording for
names, expected roles, received roles, examples, and compatible suggestions.

One authored mistake produces one root diagnostic. Parser recovery must
continue at the next valid boundary and must not publish dependent token, type,
or pipeline failures as separate user problems.

## 2. Audience projections

### 2.1 LLM and automation

LLMs receive the versioned JSON diagnostic object. The packet contains the
canonical `schemaVersion`, `diagnosticId`, `sourceRevision`, `summary`,
`reason`, `recommendedNextStep`, `alternatives`, `ruleId`, `severity`,
`location`, `context`, `impact`, and `helpId` fields. It contains no ANSI,
Markdown presentation, secret values, customer payloads, provider errors,
stacks, or terminal-only labels.

Edits are exact and revision-bound. Each edit previews behavioural and
public-contract effects. A protected decision may expose bounded candidate
edits, but an agent may not select one on the human's behalf.

### 2.2 Interactive terminal

The interactive terminal leads with the complete summary, shows an exact source
frame, then presents `Why`, the recommended next step, bounded alternatives,
impact, and decision ownership. The stable rule and legacy alias are visually
secondary. Rich colour and Unicode are presentation only; plain output carries
the same meaning without ANSI. The stable relative help identifier is rendered
as a full `https://jadpo.dev/docs/diagnostics/...` URL.

### 2.3 IDE

The Problems panel contains only the human summary. The squiggle covers the
smallest responsible range. Hover contains summary, reason, recommended step,
alternatives, owner, and bounded impact. Quick Fix lists a verified preferred
edit first only when the compiler can justify it; protected choices are shown
without an automatic preference. Details contain the complete impact and help
link as a clickable full URL. Raw JSON is available for tools, not used as the
normal IDE explanation.

## 3. Authentication-value golden scenario

Given:

```jadpo
route POST /registrations {
    auth: nonke
    input: RegisterCustomer
    output: RegistrationAccepted
    run: register_customer(input)
}
```

the compiler emits exactly one root diagnostic over `nonke`:

- summary: ``Route authentication value `nonke` is not valid``;
- reason: routes require authentication by default, and the only accepted
  explicit opt-out is the exact form `auth: none`;
- recommended next step: ask the human to choose the authentication boundary;
- owner and kind: `human` and `human_decision`;
- choice one: remove the `auth:` item to retain required authentication,
  reporting that target generation may then require authentication runtime
  support;
- choice two: replace `nonke` with `none`, explicitly reporting that the route
  becomes callable without authentication; and
- impact: the public route security boundary is unresolved, so compilation is
  blocked rather than guessed.

Neither choice is automatically preferred. `input`, `output`, and `run` remain
parsed and do not receive cascade diagnostics.

## 4. Conformance evidence

The diagnostic suite must enumerate every compiler-owned public code and fail
when any code lacks its own catalogue entry or fixture. For every code it checks
the JSON schema, human-readable content, repair classification, bounded
context/impact, source range, and absence of unsafe data. It also checks both
terminal modes and the IDE Problems/hover/details/Quick Fix projections.

Scenario fixtures additionally prove root-cause grouping, automatic edits,
guided alternatives, human decisions, edit-impact previews, stale-edit
rejection, related locations, multi-file edits, Unicode/UTF-16 ranges, and
legacy aliases. A generated snapshot may verify deterministic rendering, but a
snapshot derived from generic fallback prose does not count as semantic
evidence.
