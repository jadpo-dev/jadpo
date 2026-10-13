# Threat model and trusted computing base

**Status:** candidate P10R model for review  
**System:** compiler, generated Bun runtime, database adapters, policy/approval
artifacts, CI gate, and deployment configuration

## 1. Security objective

For the named supported properties, an agent-generated change must not gain
release-equivalent success when it exceeds human-owned policy, bypasses a typed
boundary, discloses a protected value, introduces undeclared egress, or performs
an unresolved destructive lifecycle transition.

Deterministic behaviour is not assumed correct. The compiler and runtime are
inside the trusted computing base (TCB), not outside the threat model.

## 2. Assets and adversaries

Protected assets are tenant/user records, authentication identity, policy
authority, secrets, provider credentials, lifecycle/data integrity, derived
evidence, approval records, deployment state, and availability bounds.

Threat sources include:

- a well-intentioned agent making a plausible but unsafe change;
- an agent following an ambiguous, stale, or adversarial prompt;
- hostile request, credential, database, or provider data;
- a compromised user crossing owner or tenant boundaries;
- a reviewer approving without understanding transitive behaviour;
- a malicious or compromised implementation contributor;
- compiler, generator, runtime, adapter, or policy-engine defects;
- CI bypass, stale artifacts, deployment misconfiguration, or secret leakage;
- dependency/provider failure or contract drift; and
- misuse, replay, or normalisation of an escape hatch.

## 3. Trust boundaries

```text
human policy authority -> protected policy/approval system -> CI release gate
untrusted source/agent -> compiler semantic graph -> generated artifacts
hostile HTTP/auth data -> generated validators/auth adapters -> typed actor/input
typed operations -> generated persistence/service adapters -> database/providers
generated release -> automatic deployment/startup validation -> readiness -> serving runtime
```

The TCB includes the compiler semantic passes, proof kernel implementation,
artifact canonicalizer, generated validators and serializers, auth adapters,
persistence/service adapters, runtime failure boundary, database constraints,
policy and approval stores, CI identity/configuration, artifact provenance,
deployment/startup validation, runtime built-ins, and the reviewed external contracts
used for code generation.

Application source, agents, prompts, boundary data, provider responses, ordinary
repository files, generated prose, and cached approvals are not trusted.

## 4. Threat register

| ID | Threat and asset | Prevent/detect boundary | Enforcement | Required evidence | Residual risk |
|---|---|---|---|---|---|
| TM-01 | missing authentication exposes data | route default + generated auth adapter | static + runtime | `ROUTE-DEFAULT-AUTH`; unauthenticated black-box cases | adapter/compiler defect |
| TM-02 | mixed credentials merge identity/privilege | strategy selector | runtime generated | `AUTH-EXACTLY-ONE`; conflict cases | provider claim equivalence bug |
| TM-03 | cross-user/tenant query | proof kernel + scoped adapter | static + runtime DB predicate | `QUERY-SCOPE`; adversarial load-then-check fixture | proof/SQL lowering bug |
| TM-04 | unauthorized mutation or mass update | proof kernel + transaction adapter | static + runtime DB predicate | `MUTATION-SCOPE`; zero/many-row fixtures | database isolation/configuration |
| TM-05 | private/internal field disclosure | closed projection/serializer | static + runtime validation | `PROJECTION-SUBSET`; output fuzz tests | serializer/runtime defect |
| TM-06 | hostile malformed boundary data | generated validators | runtime | malformed IDs, unknown fields, provider fuzz cases | validator discrepancy |
| TM-07 | secret in output, log, diagnostic, or service payload | information-flow classification + sinks | static + runtime redaction | external-effect fixtures; log/output scans | side channel, novel sink |
| TM-08 | undeclared network egress | service declarations + runtime/deployment network policy | static + operational | egress manifest matched to sandbox/firewall | compromised runtime/host |
| TM-09 | raw database/provider error disclosure | adapter normalisation + boundary mapper | runtime | fault-injection cases | unknown driver/provider shape |
| TM-10 | unsafe destructive migration/lifecycle | migration identity + lifecycle rule + approval | static + human gate | `LIFECYCLE-TRANSITION`; existing-data drill | policy itself harmful |
| TM-11 | duplicate/irreversible external effect | idempotency/retry contract | static + runtime/provider | replay/failure injection | provider violates idempotency |
| TM-12 | agent weakens policy to make code compile | protected policy authority + semantic diff | CI + human | `PUBLIC-WEAKENING`; approval attack cases | reviewer collusion/account compromise |
| TM-13 | agent manufactures/replays approval | external attestation and digest binding | CI | stale, self-issued, replay, scope mismatch cases | approval service compromise |
| TM-14 | generated target diverges from semantic graph | deterministic generation + artifact digest + integration tests | build + CI | reproducibility check; black-box suite | common-mode compiler/generator bug |
| TM-15 | CI/deployment uses stale artifacts | provenance manifest and digest verification | CI + deployment | rebuild/verify from clean checkout | privileged CI bypass |
| TM-16 | invalid configuration starts serving | automatic deployment/startup validation + readiness | runtime + operational | configuration/deployment acceptance cases | platform ignores readiness |
| TM-17 | dependency outage causes restart storm | separate liveness/readiness | runtime + operational | outage recovery test | operator misconfiguration |
| TM-18 | unbounded query/job exhausts resources | explicit bounds + generated plan | static + runtime | plan metadata; load/boundary tests | allowed bound still too costly |
| TM-19 | escape hatch bypasses guarantees | typed capability envelope + audit + approval | static + human + operational | adversarial escape cases | approved code remains unsafe |
| TM-20 | reviewer misses transitive unsafe effect | compiler-derived behavioural/graph review | human process + UI | comprehension experiment | fatigue, misleading presentation |
| TM-21 | stale cache/graph data preserves revoked access or violates an invariant | authoritative policy/invariant check + freshness proof | static + runtime | revocation during projection lag; stale-candidate revalidation | authority outage may reduce availability |
| TM-22 | projection update is lost, duplicated, reordered, or silently diverges | authority-atomic change record + revisioned idempotent delivery + reconciliation | runtime + operational | crash-window, duplicate, reorder, replay, rebuild, and drift fixtures | compiler/adapter/common-store defect |
| TM-23 | multi-authority retry duplicates an irreversible effect or compensation hides ambiguity | persisted workflow + stable step idempotency + explicit outcome/intervention state | static + runtime + operational | timeout-at-each-boundary and reconciliation fixtures | provider violates contract; manual resolution error |
| TM-24 | stale signed credential outlives the promised revocation guarantee | declared immediate/bounded mode + per-route fresh authority | static + runtime | disable/logout/key-revoke matrices at the bound and fresh routes | clock/authority adapter defect |
| TM-25 | service credential is confused with a user or carries permissions | closed user/service principal + authoritative resolution + policy boundary | static + runtime | kind-confusion, disabled-service, exchange, and policy fixtures | principal-lowering defect |
| TM-26 | JWT validation widens algorithms, issuer, key source, or dependency surface | compiler-owned `jose` adapter + pinned closure | build + runtime + supply-chain review | non-JWT zero-footprint proof; JOSE adversarial/conformance suite | upstream/runtime or key-infrastructure compromise |

## 5. Escape hatches

No general arbitrary TypeScript, raw SQL, raw network, untyped serializer, or
ambient-secret escape exists in ordinary source. Any future escape must declare
its input/output types, exact data access, egress, secrets, failures, resource
bounds, and policy capability; appear in every audit; be independently approved;
and run behind the narrowest runtime/deployment capability available.

An escape hatch is residual trusted code, not compiler-proved behaviour. The
product must never relabel it as proved because its declaration compiled.

## 6. Assurance-claim publication rule

A claim may appear in product documentation only when it names:

1. the policy/proof or validation rule;
2. the threat-register entry;
3. the enforcing TCB components;
4. positive, negative, and adversarial executable evidence;
5. the relevant escape/bypass conditions; and
6. the residual risk in plain language.

Until then, the project may describe an intended property or experiment, not a
guarantee.

The candidate [assurance evidence map](../tests/assurance/evidence-map-v0.1.json)
links every `todo-v0.1` acceptance case and every threat entry to named proof or
validation rules, existing candidate evidence, and explicitly planned evidence.
Planned entries do not satisfy publication or phase-exit requirements.
