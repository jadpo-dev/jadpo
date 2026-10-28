# Candidate contract review log

**Contract:** `todo-v0.1`  
**Review type:** author-side consistency review  
**Independent review:** pending

This log records contradictions found before the independent P10R review. It is
not external validation and cannot satisfy the P10R review or first-user gates.

## 2026-10-02 lifecycle/service semantic successor freeze

Independent RM-205 and RM-301 reviews accept their separate semantic contracts
for implementation; the lifecycle and
service plans linked exact candidate provenance and findings.
The service successor has a scoped policy identity amendment and a separately
pinned JOB-001 replacement, preserving pressure app/policy/acceptance bytes and
the other 43 case IDs. No HTTP/job/lifecycle execution or final independent
P10R experiment approval is supplied by these contract reviews.

## 2026-10-02 owner-authorised UserWithTodos email revision

The owner selected: keep `User.email` provisioning-only and omit it from the
self-service todo-list response. `policy.jadpo` is unchanged. The predecessor
acceptance bytes are preserved in
`acceptance-before-rm107-email-decision.json`
(SHA-256 `acfc317db191764ebef1714f22af2f37e6ae1b93c2c609c195527197ef252ab0`).
The successor digest is
`3bed090059de86e35d72fb7a61b0fab365990d909e118b4f5f39b7b10ba56608`.

AUTH-007 now requires email to be absent even when the JWT carries an email
claim. REL-001 retains the `UserWithTodos` response, two-query ceiling, ordered
child bound and cross-parent isolation, and also requires email to be absent.
The projection remains `user_id` plus `List<TodoView>`. Both cases remain
`not_executed`; this is a contract correction, not runtime evidence or final
independent P10R approval.

**Focused RM-107 evidence, 2026-10-02:** the migrated UserWithTodos route now
returns that shape through one self-scoped User lookup and one indexed Todo page
query. The SQLite HTTP suite covers zero, one and more than 100 children,
counts exactly two application data reads, and proves owner/deleted filtering,
ordering and private-field exclusion. PostgreSQL 16.3 checks the same capped
projection and partial-index plan. A real HTTP JWT test supplies a conflicting
email claim and confirms that neither the stored nor provider email appears in
the response. These focused tests do not execute the source-bound 44-case
harness; AUTH-007 and REL-001 remain `not_executed` there, and independent P10R
review remains pending.

## 2026-09-25 consistency pass

| Finding | Resolution |
|---|---|
| Acceptance was ranked above human-owned policy, allowing a conflicting test to appear authoritative. | Split authority: policy owns permissions/effects/lifecycle; acceptance owns observable behaviour inside the policy envelope; any contradiction blocks freeze. |
| The `overdue_reminders` job selected future todos within a lookahead window. | Removed the unused lookahead configuration and changed eligibility to `due_at < at`. |
| `TodoAlreadyCompleted` was declared but unreachable. | Removed the failure rather than inventing lifecycle behaviour. |
| Create-time omission of nullable `due_at` was unspecified. | Added an explicit `none` input default and black-box case `CREATE-004`; logged missing input-default semantics. |
| The validation plan required invalid-owner, reassignment, and parent-deletion cases, but only invalid-owner existed. | Added `REL-003` for prohibited reassignment and `REL-004` for parent hard-delete restriction. |
| Changing a due date after a reminder could leave the todo permanently ineligible. | The patch now clears `reminder_sent_at` whenever `due_at` is supplied; added `PATCH-005` and logged the derived-update requirement. |
| Authentication declarations mapped claims but did not state active-user resolution. | Added explicit resolution through `User.authentication_subject` with active status. |
| Patch-derived reminder clearing contradicted the one-line `job_only` field policy. | Expanded the field policy: the job may write delivery time, a due-date patch may only clear it, and clients can never write it directly. |
| Provider email claims could become application identity even though the database owns user email. | Removed provider-email mapping; actor user ID and email now come from the active-user resolution. |
| Readiness was modelled as an operator route without an operator identity or policy. | Moved readiness to the deployment plane; it is no longer an application route or a third implicit authentication scheme. |
| Several service/job types were referenced but undeclared, and a pure helper was unused. | Declared service message/receipt values and mapped failures; simplified job actions to `None`; removed the unused helper. |
| The acceptance contract required `201`/`204`, but source relied on an unspecified success-status convention. | Added typed `created` and `no_content` route success declarations and logged the missing route capability. |
| List query parameters allowed both omission and `none`, although URL query decoding needs only omission. | Made `due_before` and `after` non-nullable optional fields. |

## 2026-09-27 authentication/configuration alignment

| Finding | Resolution |
|---|---|
| The candidate treated authentication as user-only OIDC/session mapping and omitted API/service clients. | Expanded the frozen behavior matrix to browser, opaque/JWT user API, and service API-key/exchange paths that all produce a closed user/service principal. |
| The old actor shape mixed provider subject with authoritative profile data. | Kept provider claims only as intermediate resolution input; authoritative user/service records own application identity and profile data. |
| Revocation semantics implicitly required a lookup on every request. | Selected bounded mode for ordinary traffic, explicit maximum delay, and fresh-authority checks for sensitive routes; correctness never depends on a cache. |
| Configuration used positional `secret required restart/reloadable` modifiers and a second lifecycle choice. | Migrated the golden configuration to structured in-source binding bodies; all v0.1 values are startup-bound and secrets have no defaults. |

## 2026-09-30 authentication reconciliation

Authority: approved AUTH-001 section 2, plus the owner's explicit clarification
that a fresh route's budget is **one principal lookup, with credential/session
checks counted separately**. This is an author-side candidate correction, not
an independent P10R approval or a claim that any golden case executed.

The pre-reconciliation candidate is preserved in commit `8160b94` (acceptance
SHA-256 `a2e06d9622c2e212bbfa0f9a3e9e2adc496888cfa555dade96013bc7b67f5606`). The revised acceptance SHA-256 is
`acfc317db191764ebef1714f22af2f37e6ae1b93c2c609c195527197ef252ab0`. All 44 case IDs remain; non-authentication cases other than the
explicit user-disable follow-ups retain their exact content. `policy.jadpo`
and the approved AUTH section-2 contract are unchanged by this reconciliation.

| Case | Correction and preserved obligation |
|---|---|
| AUTH-005 | Two presented credentials yield 401 `ambiguous_credentials` before validation, even if one is invalid. Zero database queries remains required. |
| AUTH-006 | Pin issuance, disablement, request and expiry times. The existing fresh `DELETE /users/alice` must return 403 `user_disabled` for an otherwise valid unexpired cookie, with valid browser/CSRF context and no business writes. |
| AUTH-008 | Call the credential a short-lived signed bearer, matching both the source validator and bounded behavior. Cache outage succeeds without either principal- or credential-authority queries. This does not pretend that an opaque credential can skip authoritative validation. |
| AUTH-010 | Use declared `DELETE /users/alice` and its 204 result; the previous POST route did not exist. Assert exactly one principal-authority query and record credential checks separately, as the owner chose. No exact total-authentication-query budget is inferred. |
| USER-001 | Keep disabled status, 204 and retained todos. Add explicit unexpired fresh rejection, failed refresh with no credential issuance, and ordinary rejection at original credential expiry within the five-minute bound. An ordinary request need not succeed during the grace interval; earlier denial is allowed. |

`Principal.user.email` was also inconsistent with lookup-free signed validation:
AUTH-001 excludes profile data from signed credentials. No business expression
in this candidate used that principal field. Remove it and its two resolution
mappings; application-owned email remains on `User` and its projections and is
read through application data operations. Never copy provider email into a
credential or profile field to satisfy a zero-query assertion.

Query counters now have explicit non-overlapping categories. A combined
credential/principal SQL query is counted once as principal authority; additional
credential/session queries are counted separately. Application queries remain
separate from those authentication counters. This records physical queries,
not a single logical resolver call that could conceal multiple database reads.

These corrections do not implement the golden app. Service/JWT adapters, the
candidate exchange endpoint, canonical entity/policy migration, route success
and input/query bindings, lifecycle behavior and job/service contracts remain
tracked implementation or design work. Refresh follow-up names the compiler-owned
operation and named `UserDisabled` failure instead of inventing an HTTP endpoint
or treating a host operation as an HTTP response. A future executable harness
must execute every timed follow-up; structural candidate validation is not
runtime evidence.

## 2026-10-01 browser-origin configuration revision

Owner-authorised RM-103 revision: the implemented successor requires
`browser_origin: Url { binding: "BROWSER_ORIGIN" }` and binds the browser
validator's `origin` to it. No default or production origin is invented.
Missing/invalid configuration must fail before listening; exact origin and
session-bound CSRF proof remain required. Bearer transport stays separate.

The original candidate `app.jadpo`, acceptance and human-owned policy remain
unchanged. The addition is in `examples/golden-todo-migration`; this is an
explicit successor configuration revision, not a silent frozen-source edit or
independent approval. Baseline candidate/migration digests and historical
diagnostics are retained in
provenance.

| Changed successor file | Before SHA-256 | After SHA-256 |
|---|---|---|
| `examples/golden-todo-migration/config.jadpo` | `25be093084592a2f324fa958d96b6fae2155bf072403e96b908ebcdab625e593` | `37e5e694bfcef51f19dfcc9a23f6835ea617ad497bbca5bc86dee7381eab2d80` |
| `examples/golden-todo-migration/authentication.jadpo` | `8e8223032908ce024ad70d31fd6596d1e465258884da92de8888fa164971d143` | `7b58c592fcc6d58de350fbfbb39b013101f1f6501c6302beadda6ca304315c82` |

Runtime proof against the full golden application and independent authentication
review remain pending RM-102/RM-103; source checks cannot satisfy those gates.

## Verification

Run:

```text
python3 tools/verify-p10r.py
```

The verifier checks the required package, parses and structurally validates the
acceptance contract, rejects duplicate/malformed case IDs, verifies required
case families and local documentation links, and prints deterministic SHA-256
digests. Printed digests are informational until independent review has resolved
the candidate and a separate freeze decision records them.

## Independent-review checklist

- Verify each acceptance case is permitted by policy and represented by source.
- Identify unspecified observable behaviour rather than filling it in from
  framework convention.
- Challenge exactly-one authentication and invalid-beside-valid handling.
- Challenge concealment, timing, and enumeration behavior.
- Review patch omission, status transitions, reminder replay, due-date changes,
  and job concurrency/failure windows.
- Review user disablement, retained children, hard purge, and existing-data
  migration behavior.
- Trace every audit claim to a proof rule, threat entry, test, or explicit
  residual risk.
- Verify TypeScript receives an expert baseline and equal agent context.
- Record disagreements and disconfirming evidence; do not edit the candidate
  silently during review.
