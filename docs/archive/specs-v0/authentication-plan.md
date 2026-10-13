# AUTH-001 authentication implementation plan

**Status:** AUTH-P0–P3 core and scoped browser/API AUTH-P4/P7 completed; scoped AUTH-P5/P6 runtime extensions verified; broader principal mappings and full AUTH-P7/P8 exits remain open
**Prepared:** 2026-09-27
**Accepted-contract digest:** `sha256:5cf32778486586b4645d226f3e4c1ebcd636d0d868b443c2be768d492003fb0b` (section 2, recorded 2026-09-27)
**Scope:** browser, API, and service-to-service authentication plus
provider-independent principal acquisition for P11
**Out of scope:** authorization policy, configuration source/provenance, account
provisioning, human login mechanisms such as passwords or magic links, service
egress, and durable jobs

This plan turns the candidate golden-todo authentication contract into an
implementable sequence. The owner approved the architecture in section 2 after
reviewing session lookup cost, bounded versus immediate revocation, browser and
API transports, and service-to-service clients. Candidate source syntax still
requires fixture-first design; the security semantics do not.

## 1. Already accepted invariants

**Owner-selected syntax successor, 2026-10-04:** remove the authored name from
the sole `principal { ... }` block and its redundant application selector;
provide `Principal` and `current_principal` automatically. This is scheduled
under RM-222/RM-223, not a change to the currently implemented named grammar or
the frozen section-2 contract. Authentication provenance, closed variants and
policy boundaries remain mandatory. See the
[naming/principal decision](decision-register.md#naming-and-principal-direction--2026-10-04).

The following rules are not reopened by AUTH-001 implementation:

- every route requires authentication unless it contains the exact, conspicuous
  `auth: none` exception;
- omitting an authentication item never makes a route public;
- authentication and authorization remain distinct;
- provider credentials, token objects, cookies, trusted identity headers, and
  provider SDK types stop at a compiler-owned boundary;
- business actions, named queries, and policy receive one provider-independent,
  typed principal rather than raw claims;
- multiple strategies never merge identities, roles, permissions, or
  authentication strength;
- malformed, invalid, ambiguous, disabled, unavailable, or misconfigured
  authentication fails closed with safe public output and secret-free
  operational evidence;
- authentication completes before protected-route input decoding or business
  database work, except for the explicit authoritative principal operation;
- `auth: none` does not bypass validation, resource limits, policy, audit, or
  any non-authentication boundary; and
- target generation continues to reject protected routes until the selected
  authentication runtime is implemented and configured.

The normative security rules remain `AUTH-EXACTLY-ONE` and
`ROUTE-DEFAULT-AUTH` in the policy/proof kernel. The first implementation must
not claim that runtime validation is a static proof.

## 2. Approved v0.1 contract

The approved baseline is:

- authentication is defined for three first-class client classes: a browser,
  a user-operated API client, and a service-to-service client;
- credential transport and credential validation are independent choices;
  cookies do not imply stateful validation and bearer headers do not imply
  stateless validation;
- first-party browsers use secure `HttpOnly` cookies; API clients use one
  `Authorization: Bearer` credential; bearer credentials are never accepted
  from URLs;
- the ordinary developer-facing strategy choices are browser session, opaque
  bearer token, API key, and JWT bearer token. JWT describes a token format and
  bearer describes its request transport; the source language and tooling make
  that relationship clear without asking authors to choose cryptographic
  libraries or low-level validation policy;
- the runtime produces one typed `Principal`: either an authoritative user or
  an authoritative service. Authored code never receives cookies, keys, raw
  claims, provider objects, or token implementation types;
- exactly one credential may be presented. Cookie-plus-bearer, duplicate
  values, and invalid-beside-valid all reject rather than select or merge;
- application authentication has a configurable revocation guarantee:
  `immediate` performs an authoritative check on every authenticated request;
  `bounded` validates a short-lived signed credential locally and checks
  authority when refreshing or exchanging it;
- bounded mode declares its maximum revocation delay. It must never claim
  immediate disable, logout, permission-change, or key-revocation propagation;
- a protected route may require a fresh authoritative check even when the
  application default is bounded. This is intended for administration,
  permission changes, billing, export, and similarly sensitive operations;
- a cache may reduce authority load, but no security or availability guarantee
  depends on a cache being present;
- signed credentials contain only namespaced principal identity, principal
  kind, authentication strength, credential/session identifier, audience,
  issued time, expiry, and format/key version. They do not contain email,
  profile data, tenant authority, roles, capabilities, or permissions;
- service principals exist in v0.1. Each service has an identity, owner,
  lifecycle state, and separately rotatable, expiring, revocable credentials;
- a service can authenticate directly with a high-entropy opaque key for
  immediate revocation, or exchange its credential for a short-lived bearer
  credential for bounded revocation and lookup-free ordinary requests;
- raw service secrets are shown once and stored only as a verifier. Audit facts
  identify the service and credential record without recording secret material;
- permissions are deliberately not an authentication claim. POLICY-001 applies
  policy to the resulting user or service principal and owns freshness of
  authorization facts;
- missing credentials are `401 authentication_required`, invalid credentials
  are `401 invalid_credentials`, conflicting credentials are
  `401 ambiguous_credentials`, and a valid credential found to represent a
  disabled principal during an immediate, refresh, exchange, or fresh-authority
  check reaches the declared disabled-principal application failure; and
- `auth: none` skips credential inspection and never creates a principal.

Jadpo-issued cookie and opaque bearer envelopes use Bun's native cryptographic
primitives and require no application-installed authentication package. API
keys likewise use the compiler-owned runtime and authoritative store.

JWT bearer validation is an explicit opt-in capability. When and only when an
application declares it, Jadpo adds one compiler-selected, exactly pinned
`jose` dependency. The application author does not choose its package, version,
algorithms, parsing mode, or validation steps. The generated adapter owns the
issuer, audience, required-claim, algorithm-allowlist, token-size, JWKS, clock,
and failure policies. The selected `jose` release must have zero transitive
dependencies, and its version, integrity, provenance, license, advisories, and
test evidence appear in generated audit/build output.

Applications without JWT bearer validation emit no package manifest, lockfile,
external import, dormant JOSE code, or install step. The precise product claim
is therefore: Jadpo applications have zero package dependencies by default;
enabling JWT authentication adds one pinned dependency with zero transitive
dependencies.

These decisions supersede the earlier user-only, always-look-up-every-request
candidate. Changes to them require a new recorded decision rather than an
incidental implementation choice.

## 3. Compiler model after approval

The initial semantic model should contain:

- one application authentication default with an explicit revocation guarantee;
- one provider-independent `Principal` declaration with distinct user and
  service variants;
- a finite set of named authentication strategy declarations;
- cookie and bearer credential locations reserved from ordinary route inputs;
- opaque and signed credential validation modes independent of transport;
- service declarations and separately identifiable credential records;
- validated provider-to-intermediate claim mappings;
- authoritative user and service resolution operations;
- explicit inactive, missing, duplicate, invalid, ambiguous, unavailable, and
  misconfigured outcomes;
- a route requirement derived as `required`, fresh-authority-required, or the
  explicit `none` exception; and
- source spans connecting every audit fact and generated runtime operation back
  to the declaration that authorised it.

Recommended initial source shapes are the candidate `application`,
`authentication`, and `principal` declarations in the golden todo. Grammar
should be frozen only after minimal positive and negative fixtures demonstrate
that these facts cannot be expressed ambiguously. Authentication declarations
are application capabilities, not ordinary functions or actions.

## 4. Implementation sequence

### AUTH-P0 — Freeze and trace the decision

Deliverables:

- record the section 2 contract as the accepted AUTH-001 architecture;
- update the decision register and AUTH-001 issue status;
- update the golden source, acceptance cases, proof rule, threat model,
  expected audit, and TypeScript baseline together;
- add browser, user-API, and service-to-service cases before compiler changes;
- record a digest of the accepted contract; and
- record the Bun-native session/opaque/API-key boundary and the opt-in, exactly
  pinned `jose` JWT boundary with provenance, integrity, advisory, and manual
  update policy;
- update the generated dependency-closure rule so only the declared JWT
  capability can introduce the compiler-selected package and lockfile; and
- prove that a non-JWT application remains byte-for-byte free of external
  imports, package metadata, dormant JOSE code, and installation.

Exit: there is one internally consistent authentication contract and no open
choice can alter parser, principal, credential-selection, revocation, or
runtime architecture.

Implementation record: the accepted section-2 contract is digest-pinned; the
decision register, threat/proof boundaries, CONFIG-001 relationship, golden
source/audit/baseline, browser/API/service acceptance matrix, and non-JWT
dependency-closure evidence are aligned. The golden source records the closed
user/service principal, two credential slots, bounded revocation, and a
fresh-authority route as fixture-first design input. AUTH-P1 now owns turning
that shape into accepted grammar and semantic nodes; it must not create a
second competing model.

### AUTH-P1 — Syntax, AST, and semantic graph

Deliverables:

- parse the application default, principal declaration, configured transports,
  validation modes, service identities, and strategies;
- give principal variants, strategies, mappings, credential slots, authority
  operations, and route requirements stable semantic nodes;
- reject duplicate principals/strategies/credential slots and unknown mappings;
- reserve configured cookie/header locations from ordinary route input;
- reject bearer credentials configured in query or path locations;
- reject `auth: none` routes whose reachable behavior requires `principal`; and
- retain `JADPO_TARGET_AUTH_NOT_IMPLEMENTED` until the runtime gate is complete.

Exit: fixtures can express and inspect the contract, but no protected target is
generated yet.

Implementation record: `auth: fresh` is parsed as a distinct protected route
requirement and appears as `fresh_authority` in route audit output. The compiler
now also parses one application authentication default, immediate or bounded
revocation, and one closed principal with required user and service variants.
These declarations have stable application, principal, variant, and field graph
nodes; duplicates, missing variants, and invalid delay contracts are rejected.
AUTH-P1b adds named strategies with one reserved cookie or authorization-header
slot and one or more named validators. Each validator records one of the
`signed`, `opaque`, `api_key`, or `jwt` modes and resolves to the closed `user`
or `service` principal variant. Duplicate strategies, credential slots, and
validator names; empty validator sets; unknown modes or variants; and bearer
credentials in path or query locations fail with authored diagnostics. These
facts have stable semantic nodes and source spans. Protected target generation
remains fail-closed. Claim mappings, authoritative resolution declarations,
reserved route-input enforcement, and unauthenticated-principal reachability
are the remaining AUTH-P1 slice.

AUTH-P1c adds explicit credential-claim mappings and user/service authority
resolution declarations with stable claim, resolution, and principal-field
mapping nodes. It rejects duplicate or non-principal mapping targets, duplicate
resolutions, non-field authorities, and empty resolution mappings. The first
AUTH-P2 checks also require a unique authority key, a Boolean active-state
predicate, nominally compatible authority-to-principal fields, complete
principal construction, and authoritative rather than credential-owned identity
fields. Route reachability, exact claim schemas, and full lifecycle/cardinality
typing remain before AUTH-P1/P2 can exit.

### AUTH-P2 — Typed principal and authoritative resolution

Deliverables:

- type-check provider claim mappings into a constrained intermediate identity;
- type-check authoritative user and service resolution and exact principal
  construction;
- prohibit provider claims from populating undeclared or authoritative
  application-owned fields;
- require resolution cardinality of exactly one active principal when the
  configured mode or route requires authority;
- make `principal` available only in authenticated action/query/policy contexts;
- prevent principal values, credential material, and raw claims from entering
  ordinary outputs or failure context; and
- emit call/effect edges for resolution without permitting arbitrary raw
  persistence in the adapter.

Exit: every strategy produces the correct variant of the same closed principal
type, with negative fixtures for missing, duplicate, stale, disabled, and
provider-owned data.

### AUTH-P3 — Strategy-independent runtime selector

Deliverables:

- generate one pre-route authentication pipeline;
- inventory every configured reserved credential slot before selecting a
  strategy;
- implement the approved zero/one/multiple/invalid-beside-valid state machine;
- authenticate before protected-route body/path/query decoding and business
  work;
- skip the entire pipeline for `auth: none` routes;
- normalise all strategy results into one internal identity contract;
- enforce immediate, bounded, and per-route fresh-authority semantics;
- resolve the active user or service when required and construct the typed
  principal; and
- contain all adapter exceptions as compiler-owned authentication or
  operational failures.

Exit: a fake verifier harness passes the selector, principal-resolution, and
revocation-mode matrices without cryptographic implementation details.

### AUTH-P4 — First-party browser and API adapters

Deliverables:

- implement versioned signed cookie and bearer envelopes with Bun-native
  cryptographic primitives;
- use constant-time verification and compiler-owned cookie parsing;
- enforce audience, expiry, key/version selection, size limits, duplicate
  rejection, and safe rotation behavior supplied by CONFIG-001;
- in immediate mode, use opaque credentials and verify current authority on
  every request;
- in bounded mode, validate locally until expiry and verify authority during
  refresh; expose and audit the configured maximum revocation delay;
- support a route-level authoritative check in either mode;
- apply compiler-owned CSRF defenses to cookie-authenticated mutation requests;
- never place application permissions or authoritative profile fields in a
  cookie or bearer envelope; and
- emit no raw credential or session secret in logs, diagnostics, traces, audit
  events, or generated errors. Stable non-secret record identifiers are allowed
  only where required for security audit and revocation.

Exit: tampering, expiry, duplicate cookies, unknown key versions, disabled
users, malformed encoding, CSRF attempts, and oversized input fail closed;
disable and logout propagation match the declared revocation guarantee.

### AUTH-P5 — Service-to-service adapter

Deliverables:

- create first-class service principals and credential records with explicit
  ownership and lifecycle state;
- generate high-entropy opaque service secrets, reveal them once, and store
  only constant-time comparable verifiers;
- support overlapping credentials for zero-downtime rotation, explicit expiry,
  individual revocation, and whole-service disable;
- support direct opaque bearer authentication with immediate authority checks;
- support confidential-client credential exchange for short-lived Jadpo bearer
  envelopes with bounded revocation;
- ensure service principals enter the same policy boundary as users without
  pretending they are users or carrying permissions in credentials; and
- emit secret-free audit evidence naming the service and credential record.

Exit: creation, one-time reveal, use, overlap rotation, expiry, revocation,
service disable, exchange, and bounded-token expiry have black-box evidence.

### AUTH-P6 — Opt-in JWT bearer adapter

Deliverables:

- add the exact compiler-selected `jose` package only when a JWT bearer strategy
  is declared, with no author-controlled package or version selection;
- generate a compiler-owned manifest and frozen lock/integrity record, require
  an explicit install step, and keep Bun auto-install disabled;
- accept one bounded bearer credential from the reserved authorization header;
- enforce compiler-owned token-size limits, issuer, audience, algorithm
  allowlist, signature, temporal and required claims, subject, key selection,
  and clock skew;
- expose none of `jose`'s decode-only, unsecured-token, JWE, arbitrary key URL,
  or application-selected algorithm surfaces to authored code;
- implement bounded HTTPS discovery/JWKS loading and caching under CONFIG-001;
- distinguish invalid credentials from operational unavailability without
  exposing provider detail; and
- retain provider profile claims only long enough to complete allowlisted
  mapping.

Exit: non-JWT builds remain dependency-free; JWT builds contain exactly one
direct package and no transitive packages; algorithm confusion, `none`, wrong
issuer/audience, hostile key headers, unknown/stale key, expired/not-yet-valid
tokens, duplicate bearer values, oversized/malformed JWTs, and JWKS failure all
have explicit tests.

### AUTH-P7 — Route integration, artifacts, and tooling

Deliverables:

- remove `JADPO_TARGET_AUTH_NOT_IMPLEMENTED` only for fully configured protected
  applications;
- add `audit/authentication.json` with strategies, credential locations,
  principal kinds, transports, validation and revocation modes,
  mapping/resolution sources, route requirements, failure contracts, evidence
  classification, dependency provenance, and unsupported features;
- derive OpenAPI security schemes and per-route security requirements without
  exposing secrets or provider internals;
- add authentication facts to inspect, LSP hover, signatures, semantic tokens,
  diagnostics, and agent JSON;
- generate structural tests proving protected-by-default and the explicit
  public exception; and
- retain source mapping for every generated authentication operation.

Exit: audit, OpenAPI, route inventory, target behavior, IDE presentation, and
generated tests agree on every route.

### AUTH-P8 — Golden todo and adversarial exit run

Deliverables:

- migrate the frozen golden source without weakening its behavioral cases;
- run the original `AUTH-001` through `AUTH-007` cases plus the expanded
  browser/API/service adversarial matrix below;
- run both SQLite and live PostgreSQL principal resolution;
- compare the generated target with the frozen TypeScript baseline;
- record dependency/toolchain versions and raw results; and
- keep policy implementation evidence and P10R release claims distinct from
  authentication evidence.

Exit: authentication is executable evidence for P11, but authorization and
release-equivalent assurance remain blocked on their own gates.

## 5. Required fixture and runtime matrix

At minimum, implementation must cover:

| Area | Positive evidence | Negative/adversarial evidence |
|---|---|---|
| Route default | protected route with one valid strategy | missing credential; invalid `auth:` value; accidental public omission impossible |
| Public exception | liveness works without principal or dependency access | public behavior attempts to read `principal`; ambient invalid credential cannot affect liveness |
| Selection | one cookie; one bearer; one service credential | cookie plus bearer; duplicate values; invalid beside valid; same-principal dual credentials; conflicting principals |
| Browser cookie | valid signed or opaque cookie resolves a user | tampered, expired, disabled user, oversized, malformed, duplicate, unknown key version, CSRF attempt |
| User API | valid bearer resolves a user | bearer in URL, malformed header, expired or wrong-audience credential, disabled user, duplicate bearer values |
| Revocation | immediate mode checks authority; bounded mode avoids ordinary lookup; fresh route checks authority | cache absence changes correctness; bounded mode claims immediate effect; refresh succeeds after disable/revocation |
| Service direct | current opaque key resolves a service | guessed, malformed, expired, revoked, duplicate, disabled-service, or previously rotated key |
| Service exchange | valid client credential receives and uses a short-lived bearer | public/unregistered client, revoked credential, excessive lifetime, wrong audience, replay beyond expiry |
| JWT bearer | declared capability adds exact pinned package and valid allowlisted issuer/audience/algorithm resolves a principal | dependency appears without declaration; package/version override; transitive dependency; `alg:none`; algorithm confusion; attacker key URL/header; bad signature; wrong issuer/audience; expired, future, missing subject, unknown key, malformed/oversized token |
| Resolution | exactly one active authoritative user or service | no principal, duplicate mapping, disabled principal, authority unavailable, provider data conflicts with authoritative data |
| Principal | every strategy produces the declared user or service variant | kind confusion, unknown field, provider object escape, raw token/claim output, permission supplied by a credential |
| Operations | safe classified event with strategy name and source span | secrets, tokens, cookies, claims, provider response, connection data, or user profile appear in telemetry |
| Configuration | valid startup, declared revocation bound, and key refresh | missing/invalid bound, missing authority, insecure issuer URL, unreachable discovery/JWKS, rotation/reload failure |
| Generated contracts | audit/OpenAPI/inventory agree | unsupported strategy or incomplete mapping fails generation rather than weakening access |

Credential parser fuzzing and upstream JOSE conformance suites are required
for the accepted dependency boundary; hand-selected examples alone are not
sufficient security evidence.

## 6. Diagnostics to design fixture-first

The initial diagnostic families should include stable authored contracts for:

- missing or duplicate application authentication default;
- missing, duplicate, or invalid principal declaration or variant;
- duplicate strategy or credential location;
- ambiguous cookie/bearer selection or bearer-in-URL configuration;
- invalid or missing revocation guarantee and bound;
- fresh-authority route without an authority operation;
- invalid service ownership, credential lifecycle, or exchange configuration;
- unsupported strategy;
- invalid or incomplete claim mapping;
- unknown principal field or incompatible mapping type;
- invalid resolution query/cardinality/inactive mapping;
- provider claim used as an authoritative application field;
- principal access from a public or unauthenticated context;
- reserved credential header/cookie exposed as ordinary route input;
- conflicting or insufficient route authentication requirement;
- unavailable, substituted, unpinned, or integrity-mismatched JWT dependency;
- external package metadata or import emitted without a declared JWT strategy;
- incomplete CONFIG-001 binding; and
- target generation attempted before the authentication contract is complete.

Each security-boundary diagnostic is human-owned unless the repair can only
restore a stricter already-declared contract. No automatic repair may add
`auth: none`, loosen claim validation, or delete principal resolution.

## 7. Generated audit contract

`audit/authentication.json` should state facts rather than a single green flag:

- schema and compiler version;
- application default and explicit public exceptions;
- every strategy, transport, validation mode, credential location, and
  implementation provenance;
- whether JWT capability is enabled and, only then, the exact direct dependency
  version, integrity, provenance, license, advisory state, and zero-transitive
  closure;
- revocation guarantee, bounded-mode maximum delay, and fresh-authority routes;
- issuer/audience/algorithm/session and service-credential properties without
  secret values;
- claim-to-intermediate and authority-to-principal mappings;
- active-user and active-service resolution operations and source spans;
- exact selector/conflict/invalid-beside-valid behavior;
- route-to-requirement mapping;
- public and internal failure classification;
- configuration dependencies and reload/restart ownership;
- generated/runtime/test/proof evidence classification;
- unsupported features and unresolved decisions; and
- direct links to threat entries and acceptance cases.

The audit must never imply that authentication proves authorization, tenant
scope, ownership, field policy, deployment correctness, provider uptime, key
custody, or absence of dependency vulnerabilities.

## 8. Dependencies and parallel boundaries

- **CONFIG-001:** blocks real secret/key/issuer binding, revocation bounds,
  readiness, refresh, rotation, and reload/restart behavior. AUTH-P1 through
  most of AUTH-P3 can be implemented against the accepted section 2 contract;
  real adapters cannot exit without CONFIG-001.
- **POLICY-001:** its approved [policy contract](policy-plan.md) supplies the
  authorisation design, and the enforcement core is implemented; final
  approval/proof evidence remains gated. It does not block authentication, principal
  acquisition, or protected-by-default route generation once
  `AUTH-EXACTLY-ONE` is frozen as a runtime invariant.
- **DATA-007/QUERY-001:** the implemented named-query/entity foundation supplies
  authoritative user, service, session, and credential resolution and is no
  longer a blocker.
- **TIME-001/TEST-001:** the approved, digest-pinned
  [time/testing contract](time-testing-plan.md) supplies stable operation
  `Instant`, internal monotonic deadlines, deterministic clock control, typed
  capability fakes, and boundary-evidence separation. Authentication adapters
  must consume that contract rather than define their own clock, skew, or fake
  semantics.
- **P10R/DX2:** remain external evidence gates and cannot be satisfied by this
  implementation plan.

## 9. Stop conditions

Pause implementation and return to the owner if work would:

- change the approved section 2 contract;
- introduce a new credential location, strategy, provisioning path, or principal
  authority;
- implement JWT/OIDC verification without the approved dependency boundary;
- allow authored package/version/algorithm selection or add a JWT dependency
  to an application that does not declare JWT bearer validation;
- read secrets or configuration directly before CONFIG-001 fixes ownership;
- expose raw provider data or service secrets to authored business code;
- add optional authentication to public routes;
- treat authentication success as authorization;
- weaken a protected route to make a runtime fixture pass; or
- publish a proof/assurance claim beyond the named runtime and test evidence.

## 10. Approval handoff

Owner approval was recorded on 2026-09-27 and AUTH-P0 is complete. AUTH-P1
through AUTH-P3 may proceed against the frozen contract and implemented
CONFIG-001 core. No additional owner decision is required unless implementation
would cross a stop condition in section 9.

Implementation record: AUTH-P1 now includes reserved route-input enforcement,
transitive rejection of principal use from `auth: none` routes, and closed
principal input/output/failure boundaries. AUTH-P2 generates an exact tagged
user/service principal, validates authoritative values and resolution
cardinality envelopes, and prevents kind or subject substitution. AUTH-P3
generates and adversarially tests the exactly-one credential inventory,
validation, bounded/immediate/fresh resolution, and adapter-exception
containment pipeline. The original selector-only evidence did not open protected routes. The dated
first-party checkpoint below now opens fully configured supported user routes;
unsupported authentication remains fail-closed.


### 2026-09-29 first-party checkpoint

The owner requested completion of the first-party path and protected routes
before Wasm work. The [executable example](../examples/first-party-authentication/README.md)
records the supported source settings, exact key format, host integration,
credential lifecycle, boundaries and reproduction commands.

AUTH-P4 now supplies real signed and opaque credentials over cookie or bearer
transport, generated persistent session/user authority, expiry and revocation,
bounded signed refresh, overlapping keys and CSRF. AUTH-P7 supplies the protected
route runtime, principal-to-policy context, authentication audit and OpenAPI
security requirements. Validation precedes protected input decoding/business
work; public routes skip authentication. The normal request clock supplies one
stable operation instant.

The current supported principal is `subject`, UUID `user_id`, and optional
textual `authentication_strength`, with one user validator per strategy and a
persisted unique authority mapping. Profiles, service credentials and JWT are
still generation-gated. This narrow boundary prevents signed envelopes from
silently acquiring profile or permission claims. Typed validator settings are
`secret`, optional `previous_secret`, `audience`, and cookie `origin`; keys enter
only through CONFIG-001 secret sinks. Current/previous keys rotate via restart.

Trusted host integration may issue credentials only after authenticating the
human; no authored callable or login/refresh/logout HTTP endpoint is introduced.
The host lifecycle harness is not evidence for passwords, magic links or account
provisioning. The public API remains defined by authored routes.

Evidence: 20 first-party runtime cases cover all four signed/opaque and
cookie/bearer combinations, SQLite policy-scoped reads/writes, real local HTTP,
expiry, tampering, bounded versus fresh revocation, refresh, rotation, CSRF,
concurrent principal isolation, configuration errors, malformed credentials,
secret containment and OpenAPI/audit agreement. Compiler evidence covers literal
secret rejection and fail-closed unsupported/incomplete adapters. Live PostgreSQL,
long-running fuzzing, deployment readiness and P10R review remain open, so the
full AUTH-P4/P7/P8 exits are not claimed complete.


### 2026-09-29 browser/API milestone completion

The agreed first-party milestone is now complete for the documented
identity-only principal and trusted host integration boundary. This completes
the browser/API slice, not the full AUTH-001 programme or a release assurance
review. AUTH-P5 service credentials, AUTH-P6 JWT, richer principal mappings,
broader tooling/generated-test coverage and the full golden-todo AUTH-P8 exit
remain separate work.

- One 24-case suite passes in SQLite and PostgreSQL modes. Both modes compile
  and exercise signed/opaque cookie and bearer variants, including immediate
  revocation. Startup-failure subprocess cases deliberately use isolated SQLite
  databases in both runs.
- Persistent sessions and revocation survive a separate runtime process; a
  replaced authoritative identity cannot inherit another identity's credential.
- Concurrent initialization cannot let an older success overwrite a newer
  configuration failure. Session-schema creation and compatibility checks run
  inside the structured startup-failure boundary before the HTTP listener.
- The retired `auth: public explicitly` spelling produces one diagnostic with
  the complete value and two working, human-owned repair alternatives. Parser
  recovery preserves authentication by default.
- Reproduction commands and retained test output are linked from the
  [example](../examples/first-party-authentication/README.md).

The next phase is the broader comprehensive validation programme requested by
the owner; Wasm follows that phase. Extended fuzzing, production deployment
qualification and independent security review remain explicit validation gaps.

### 2026-09-30 service/JWT runtime extension

The through-Wasm continuation also implements AUTH-P5 service credentials and
AUTH-P6 external JWT validation before the experiment. The
[runtime extension guide](auth-runtime-extensions.md) records the supported
source, trusted lifecycle APIs, dependency installation, fixed validation limits,
query accounting, evidence and remaining exits. This dated entry supersedes the
earlier service/JWT generation-gated statement for those supported shapes.

The selector inventories credentials once and dispatches recognized formats in
the selected slot without retrying another verifier after rejection. Service
keys have an explicit owner binding and separate verifier-only records. Bounded
exchange preserves service kind/identity and originating authentication strength.
JWTs yield only a verified subject, resolved authoritatively once per request;
local IDs/profile/permissions cannot come from external claims. Non-JWT builds
retain the dependency-free runtime. JWT builds emit the exact compiler-pinned
package/lock and dated integrity, provenance, license and advisory evidence.

Review corrected two compiler restrictions: policy identity uses the declared
authority mapping rather than a name guess, and inactive failures can be
`NotPermitted` (403) as already authored in the golden app, as well as `Rejected`
(422). Golden assertions were not changed. Cookie JWTs are rejected to preserve
the separate browser CSRF boundary.

The original browser/API milestone plus comprehensive validation is the
experiment's prerequisite; the first-party implementation by itself was never
sufficient. These additional service/JWT suites strengthen that checkpoint.
Full P11 golden integration, broader principal mappings, complete tooling and
external assurance remain their separately named exits. The experiment uses
fixture principals and does not claim portable production authentication.
