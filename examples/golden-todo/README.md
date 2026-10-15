# Golden todo design contract

**Contract version:** `todo-v0.1`  
**Status:** candidate freeze; the behavioural target for roadmap Phase 10

The golden todo is the backend the new compiler must build end to end, on
both targets, with every contract case passing (roadmap Phase 10). This
folder holds the contract and its notes, not source: Phase 10 writes the
application in the decided language. The language pages win wherever these
notes use older syntax or process. The original source, written for the
archived v0 compiler, is at the tag `archive/compiler-v0`.

[`acceptance.json`](acceptance.json) is the authority: its 44 cases define
the application's externally observable behaviour. All of them are
unexecuted. The other documents are subordinate evidence:

1. [`expected-audit.md`](expected-audit.md) defines the review surface the
   compiler must derive.
2. [`adversarial-changes.md`](adversarial-changes.md) defines change-pressure
   cases and the compiler's required response to each.
3. [`typescript-baseline.md`](typescript-baseline.md) fixes the equivalent
   TypeScript stack that Phase 9's benchmarks and chunk 10.2's agent
   comparison measure against.
4. [`REVIEW.md`](REVIEW.md) records the contract's review history and the
   pending independent-review checklist.

If the source and the acceptance behaviour disagree, the contradiction blocks
the freeze until it is resolved. If acceptance behaviour exceeds the policy a
person approved, policy wins and the case is invalid until a human-reviewed
contract revision reconciles them. An agent may not edit policy to make an
implementation compile.

## Product behaviour

The application supports:

- browser sessions, user API bearers, service API keys/exchange, and opt-in JWT
  bearer validation normalised to one closed user/service principal;
- authenticated-by-default routes and one explicit public health route;
- fail-closed handling of invalid, conflicting, or ambiguous credentials;
- bounded revocation for ordinary traffic with fresh-authority checks on
  sensitive operations, without cache-dependent correctness;
- owner-scoped todo CRUD with omission-aware patches;
- stable cursor pagination and stable child ordering without N+1 reads;
- explicit owner relationship integrity and deletion lifecycle;
- optional due dates, overdue reminder scheduling, and a declared mail service;
- a soft-delete retention window followed by an idempotent purge job;
- typed configuration, secret handling, readiness, and startup preflight;
- generated structural tests plus authored business tests; and
- derived route, data, effect, failure, configuration, and policy evidence.

Sharing and administrator access are not present in the initial contract. They
enter through the frozen change sequence so the experiment can measure policy
evolution rather than beginning at the final answer.

## Freeze rule

This is a **candidate** freeze. Independent review may correct ambiguity once.
After it is marked frozen, changes need a new contract version and a
rationale, and the implementation must not weaken or rewrite the cases to fit
compiler limitations. A case may be classified `unsupported`, `cannot prove`,
or `requires human decision`, but it may not silently disappear.
