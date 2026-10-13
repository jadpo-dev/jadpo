# Behavioural-review comprehension study

**Status:** candidate P10R instrument  
**Question/scoring version:** `comprehension-v0.1`

## Purpose

Test whether reviewers and fresh agents can accurately understand behavior,
policy, effects, lifecycle, configuration, and uncertainty. Secure approval
identity is insufficient if the review surface creates false confidence or hides
transitive consequences.

## Compared surfaces

1. a normal pull-request code diff from the relevant stack;
2. a concise compiler-derived behavioral diff; and
3. a relationship/effect-aware decision view with drill-down evidence.

Participants receive only one surface per change before answering. Surface order
is counterbalanced with a balanced Latin-square assignment where sample size
permits. The same participant does not answer the same change from two surfaces.

## Administration

- Use artifacts built from the exact frozen subject digest.
- Do not coach terminology or point to relevant files/nodes.
- Record answer, evidence path, time, confidence, and clarification request
  before revealing the scoring key.
- Include locally plausible changes with unsafe transitive behavior.
- Separate implementers from evaluators; blind evaluators to stack where the
  answer artifact permits.
- Preserve unanswered and “cannot determine” responses. Do not infer a correct
  answer from discussion after time stops.

## Scoring

The machine-readable questions and point-level answer key are in
[`tests/assurance/comprehension-v0.1.json`](../tests/assurance/comprehension-v0.1.json).
Award a point only when the response states the named fact without a conflicting
claim. Partial credit is the sum of independently named facts; confidence never
changes correctness.

Report:

- overall and category accuracy;
- critical fact accuracy;
- false confidence: incorrect answer with confidence 4–5;
- critical risks noticed and missed;
- median time and clarification requests;
- approval/rejection quality for decision questions; and
- evidence-navigation paths and abandoned paths.

The continuation threshold remains at least 80% correct overall, with the
language/audit result no more than ten percentage points behind TypeScript in
any critical category. Individual critical misunderstandings are listed even
when the aggregate passes.

## Instrument changes

The candidate questions may be challenged once during P10R. After digest freeze,
wording, expected facts, points, category, criticality, and artifact assignment
cannot change in response to participant results. A necessary correction creates
a new instrument version and separate series.

