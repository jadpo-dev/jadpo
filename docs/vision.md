# Jadpo vision and design philosophy

## 1. Origin of the idea

The idea began before the current wave of coding agents: a programming language
specifically for website backends, with best practice and consistency built in.
A route or feature would keep its description, authentication needs, validation,
tests, data behaviour, and generated documentation together. Inputs, outputs,
and database writes would be validated. The system would prescribe how common
backend work is done rather than offer another collection of libraries.

At the time, writing a new language seemed disproportionate. TypeScript was
battle-tested and general purpose, and real applications eventually need
something unusual. The idea looked like a framework or CRUD generator with a
large implementation cost.

LLMs change both sides of that calculation:

- agents can help prototype a language and toolchain;
- the primary author may now be an agent rather than a human;
- humans are increasingly unable to review the volume of generated code;
- conventional stacks encode crucial safety knowledge in conventions, review,
  prompts, and developer memory;
- a narrow, predictable language can remove choices and reduce the context an
  agent must manage.

## 2. The problem is assurance, not code generation

Web tooling is fragmented, layered, and full of incidental decisions: language,
framework, ORM, validation library, folder architecture, middleware, error
style, background-job system, API client, migration strategy, test structure,
and many others. LLMs are already good at producing the resulting boilerplate.
Making boilerplate shorter is therefore not the central opportunity.

The deeper problem is that the normal software process assumed humans wrote and
reviewed code. Replacing the writer with an LLM while leaving assurance
unchanged produces a weak loop:

```text
LLM writes code
    -> LLM reviews its own code
    -> LLM runs an optional security skill
    -> LLM judges its own findings
    -> human glances at a large pull request
    -> ship
```

The same probabilistic system supplies both the implementation and the claim
that the implementation is safe. Better models do not remove this structural
problem. If a future model generates a 50,000-line application in minutes,
meaningful human line-by-line review becomes less viable through volume alone.

Assurance must move down a layer:

```text
human intent
    -> constrained language
    -> LLM implementation
    -> deterministic verification
    -> executable
```

The human reviews the smaller, higher-value surface: intent, permissions,
lifecycle decisions, and explicit exceptions.

## 3. Division of responsibility

### Human

The human owns decisions whose correctness depends on purpose rather than code:

- whether a route is public;
- which roles or owners may read or mutate data;
- destructive lifecycle behaviour such as reject, retain, anonymise, or cascade;
- which external effects are acceptable;
- security policy changes and escape hatches;
- business intent and exceptions.

An agent may help write the corresponding declaration, but it may not silently
weaken it to make an implementation compile.

### LLM

The LLM translates requirements into the constrained source representation,
responds to structured compiler diagnostics, and performs repetitive
implementation work. It may make local implementation choices inside the space
the language permits.

The environment should be model-provider independent. Project knowledge,
contracts, policies, and instructions belong in the repository and compiler
model, not in conventions specific to Claude Code, Codex, or another agent.

### Compiler and runtime

The compiler owns boring, deterministic decisions and rejects unresolved ones.
It should know enough to enforce and derive:

- route inputs, outputs, authentication, and authorisation;
- entity relationships and data lifecycles;
- database queries and mutations;
- external effects, retry behaviour, idempotency, and timeouts;
- secret flow and output exposure;
- migrations and constraints;
- runtime boundary validation;
- policy conformance, generated tests, documentation, and audit views.

The compiler should be deliberately uncreative. Its value is that the same
input produces the same conclusion every time.

## 4. The compiler as part of the reasoning loop

Conventional compiler and framework errors often expose implementation detail
and require an agent to infer the real unresolved decision. This project should
make diagnostics look like a small, structured task list:

```text
BUILD FAILED

3 unresolved requirements:

1. create_booking can be called anonymously.
   Choose: authenticated | public

2. Booking.user may be deleted.
   Choose lifecycle policy:
   cascade | retain | anonymize | reject

3. send_confirmation has an external effect.
   Choose delivery policy:
   once | at_least_once | best_effort
```

The diagnostic should distinguish choices the agent may resolve from decisions
that require explicit human approval.

## 5. Code, intent, policy, and audit

The unusually strong part of the proposal is not any individual keyword. It is
the combination of four related representations:

- **code** — what the application does;
- **intent** — why the behaviour exists;
- **policy** — what behaviour a human has approved;
- **audit** — the compiler-derived account of what the implementation permits.

Documentation becomes a view of the program rather than a separate artefact an
agent must remember to update. The repository can retain both behaviour and
rationale, reducing drift between requirements, architecture documents,
implementation, tests, and operational reality.

An early illustrative rule was:

```text
rule cancellation_window {
    intent:
        "Customers may cancel until 48 hours before their booking."

    applies Booking

    allow cancel
        when now < starts_at - 48h
}
```

The exact syntax is historical, but the requirement remains: an agent changing
the system should see both what it does and why.

## 6. Scope

The language is initially for conventional web/SaaS backends:

- typed HTTP APIs;
- Postgres persistence;
- authentication and record/role-based authorisation;
- validation and serialisation;
- CRUD plus real business logic;
- background jobs and events;
- declared third-party services;
- tests, migrations, docs, and audits.

The working hypothesis is that roughly ten to fifteen semantic primitives may
cover most of this domain: `app`, `entity`, `value`, `input`, `output`, `query`,
`action`, `route`, `policy`, `event`, `job`, `service`, `test`, and perhaps an
explicit `decision`/`rule` concept.

This is not a commitment that those boundaries are correct. Complete examples
must prove them.

## 7. Opinionation and escape hatches

The language only has a reason to exist if it removes choices and prevents
invalid architectures. It should not become a thin syntax over every option in
the JavaScript ecosystem.

Ordinary code therefore has:

- no raw SQL;
- no arbitrary HTTP calls;
- no hidden side effects;
- no undeclared secrets;
- no ambient exceptions for business flow;
- no `eval`, runtime code loading, raw sockets, untyped dynamic objects,
  reflection, or general metaprogramming;
- no five equivalent ways to branch, iterate, or report an error.

An unusual requirement may eventually cross a deliberate boundary into an
extension or escape hatch. Such a crossing must be visible, reviewable, and
present in the audit. If ordinary SaaS work constantly needs it, the language
model has failed.

## 8. Human readability

Agents may be the primary authors, but people must be able to step in. The
language should look conventional and boring: familiar blocks, descriptive
names, explicit roles, and limited punctuation. Novelty belongs in semantics and
guarantees rather than visual cleverness.

The target is not minimum characters. It is maximum meaning per token with
minimum ambiguity. Strict semantics can make source simpler because the
compiler carries infrastructure and assurance concerns that would otherwise
leak through every layer.

## 9. Safe by default

Protections should be inherited or enabled unless source explicitly removes
them. The normal case includes:

- authentication;
- input and output validation;
- database write validation;
- scoped data access;
- secret non-disclosure;
- declared network egress;
- rate limits, timeouts, transactions, and audit logging;
- retries/idempotency for asynchronous or external effects.

Unsafe or public behaviour should look conspicuous. Forgetting a declaration
must not silently remove protection.

## 10. What the compiler cannot guarantee

The proposal does not claim to solve all security or correctness:

- business requirements can be wrong;
- human-approved policy can be wrong;
- business logic can express the wrong rule;
- external providers and infrastructure can fail or be compromised;
- compiler and runtime bugs can exist;
- domain-specific vulnerabilities may lie outside the semantic model.

The goal is to make a large, mundane, recurring class of defects structurally
difficult or impossible: missing authentication, IDORs, unvalidated boundaries,
arbitrary queries, accidental private-field output, secret leakage, undeclared
egress, forgotten transaction boundaries, and inconsistent lifecycle handling.

## 11. Why the compilation target is not the point

Jadpo compiles to native code and WebAssembly, but the target is not the
product story. The reason to adopt the language would be that its semantic
model can reject unsafe programs that remain perfectly valid TypeScript.

Performance has to be good enough that developers don't avoid Jadpo for it,
but native performance does not prove the idea. Constraints, audits, safe
evolution, and agent ergonomics do.

## 12. Current posture

This is a research hypothesis worth a small, deliberately brutal experiment—not
yet a six-month language project or a strong near-term commercial opportunity.
The project should try to kill the idea quickly and continue only if the source
and guarantees are obviously better than a strong TypeScript baseline.
