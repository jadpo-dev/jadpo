# Jadpo v0 language charter

**Status:** working charter  
**Purpose:** preserve the smallest set of beliefs that currently define the idea

## Thesis

Build a backend programming environment in which:

- the **human owns intent, policy, and exceptions**;
- the **LLM performs implementation work** within a constrained language;
- the **compiler deterministically decides what is valid and permitted**;
- the runtime executes a program whose declared types and policies remain true
  at real system boundaries.

The LLM is useful, but it is not a source of assurance. It must not be asked to
write the implementation, review its own work, run an optional security prompt,
and then decide whether its own findings matter.

## Accepted foundations

1. The language targets web backends, initially ordinary SaaS applications.
2. It is opinionated and deliberately smaller than a general-purpose language.
3. Humans must be able to read and write it, even if agents are the primary
   authors.
4. It optimises for minimum ambiguity per token, not minimum characters.
5. Common tasks have one canonical representation; compiler/runtime defaults
   minimise routine architectural decisions for application-writing LLMs.
6. Semantics are strict; formatting is forgiving and canonicalised.
7. Bindings use `var`; they are immutable unless declared with `var mut`.
8. There is no language `undefined` and no ambient `null`.
9. Absence is explicit through `T?` and `none`.
10. Field omission is a property of an input/object shape, not a runtime value.
11. A concrete typed object always contains all of its declared fields.
12. Types are executable contracts, not annotations.
13. Values are validated whenever they cross an untrusted boundary.
14. Identity-bearing entities are first-class domain subjects independently of
    persistence; persistence is an optional explicit capability.
15. Named queries own reads, entity actions own entity mutations, and
    application actions compose multi-entity workflows and consistency intent.
16. Every mutable fact has one authority; caches, graph views, search indexes,
    and other copies are declared derived representations.
17. The compiler distinguishes atomic transactions, durable projections, and
    compensating workflows and never silently weakens one into another.
18. Database access and mutation are compiler-understood language operations.
19. Database and durable-job semantics are backend-neutral; SQLite/Postgres
    adapters must satisfy the same contract without requiring a hosted provider.
20. Raw SQL and arbitrary network calls are outside ordinary application code.
21. Expected domain failures are typed and declared.
22. Infrastructure failures are handled by declared operational policy rather
    than retry/timeout plumbing in business logic. Safe transient retries default
    on with bounded exponential backoff and jitter; uncertain commits and
    non-repeatable effects are not automatically replayed.
23. Authentication, validation, rate limits, safe output, and other protections
    default on or to safe behaviour.
24. Removing protection is explicit and conspicuous.
25. Human-owned policy is distinct from the derived audit.
26. CI refuses to build when policy and implementation disagree.
27. The compiler generates or derives schemas, migrations, validators, OpenAPI,
    documentation, tests, route inventory, and security audit information.
28. Generated target code is an implementation detail and is never the normal
    debugging surface.
29. Compiler diagnostics are structured work items suitable for an agent's
    reasoning loop.
30. Generated applications require only the selected runtime's built-in
    capabilities; ordinary builds and execution do not install packages.

## Success condition

The project succeeds only if the constrained model is materially safer and
easier for agents to evolve than an excellent conventional TypeScript backend.
Fewer lines are not enough. The important result is that meaningful defects—
especially security, data lifecycle, boundary, and consistency defects—become
unrepresentable or compilation failures.

## Current test

Write a complete todo backend as if the language already existed, followed by a
more difficult order/payment backend. Use them to discover the semantic model
before building a compiler.

## Reasons to stop

Abandon or substantially change the idea if:

- ordinary SaaS work continually requires arbitrary general-purpose escape
  hatches;
- agents perform materially worse because the language is unfamiliar and its
  constraints do not compensate;
- an opinionated TypeScript framework, generated schemas, linting, and an agent
  layer can provide essentially the same guarantees without losing the
  ecosystem;
- the fictional source does not feel dramatically clearer and safer than the
  TypeScript equivalent.

## Explicit restraint

Do not name the language yet. Do not mistake an appealing syntax for evidence
that the semantic model works. Do not build a compiler before the golden
programs force the important decisions into view.
