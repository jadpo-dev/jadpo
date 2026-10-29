---
title: Failures, design notes
area: 4
---

# Failures: design notes

Why [Failures](../04-failures.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Carried to later areas

| Item | Area | Status |
|---|---|---|
| Data operations' mapping clauses (`missing:`, `conflict: { ... }`, `empty:`) in the `condition: Failure` form, and foreign-key conflicts | 6 | Input to area 6 |
| A route's error responses come from what its action can fail with; no route-level status mapping; validation details and envelope in OpenAPI | 7 | Input to area 7 |
| `precondition_failed` for HTTP preconditions such as `If-Match` | 7 | Input to area 7 |
| Authentication failures: `unauthenticated`, the `inactive:` mapping, principal values kept out of failure context (`type.auth_*` rules) | 8 | Input to area 8 |
| Concealment by policy instead of a declared `not_visible` failure | 9 | Decided (question 10) |
| Data classification of internal fields for log redaction, beyond hashing text | 9 | Input to area 9 |
| Failure text for people, keyed by failure code | 10 | Decided: keyed by code in the domain's language files and exported per locale (question 12) |
| Integration failure mapping as a `failures: { provider.x: F }` block; provider timeouts and `outcome_unknown` | 11 | Input to area 11 |
| Retry, dead-letter and alerting for schedules and subscribers by kind; `outcome_unknown` never retried blindly | 12 | Input to area 12 |
| Asserting that a call fails with a given failure in tests | 13 | Input to area 13 |
| Runtime diagnostics `runtime.unhandled_fault`, `runtime.outcome_unknown` and `runtime.startup_failed`, which `jadpo check` cannot trigger | 14 | Input to area 14 |
| Streaming responses that fail after headers are sent (ROUTE-002) | 7 | Deferred |
| Whether `event` should also become `type X = Event { ... }`, by the same reasoning as failures | 12 | Raised |
| A committed record of the public API (routes, fields, failure codes) so breaking changes are caught at review, accepted explicitly: the compiler flags the change and the build passes only once a person records an acceptance. One "accept a reviewed change" mechanism shared with database drops (area 5), policy (area 9) and domain changes (area 1) | 7 | Raised |

## Decisions

| Topic | Decision |
|---|---|
| Public code (question 1) | Defaults to the failure's name in lower_snake_case: `QuantityUnavailable` is `quantity_unavailable`. `code: "..."` overrides it, such as to keep a published code after a rename. Every code is lower_snake_case and belongs to one failure, because a client relies on a code always meaning the same status, message and fields. Detecting that a published code changed needs a committed record of the public API, carried to area 7. |
| `client` and `internal` (question 2) | `client: { }` holds the fields sent to the caller and `internal: { }` the fields that never leave the server, so the block names say who sees each field and `public` keeps its one meaning, visible to other domains. |
| Declarable kinds (question 3) | An application declares `invalid_value`, `not_found`, `not_permitted`, `conflict`, `rejected`, `rate_limited` and `unavailable`; the other eight are raised only by the runtime, and the standard kinds are never failure names in `fails` or `reject`. 422 serves both `invalid_value` and `rejected`, and 400 unreadable requests, as GitHub's API does; changing that would break every client. A timeout an application cares about is either a business rule with its own failure (`ReservationExpired` with `kind: FailureKind.conflict`) or a provider problem the integration maps to a declared `unavailable` failure, such as `timeout: 5s` and `timed_out: ChargeUnavailable` (area 11), handled like any other in a `match`. How long to wait is a setting on the integration or route (areas 7 and 11), and the runtime raises the timeout kind itself. |
| Invalid input detail (question 4) | Each problem is listed as `{ field, rule }` under `details`, never the value, so a form can show which field is wrong. Area 7 sets the field path format. |
| Faults in `fails` (question 5) | Never: `fails` lists domain failures only, faults go straight to the boundary, and code that must react to a provider problem gets a declared failure from the integration's mapping (area 11). One rule, and actions do not grow a list of infrastructure names, as Rust splits `Result` from `panic`. |
| Construction proven safe (question 6) | A construction the declared ranges, or an `if` condition (area 2, "Conditions as proof"), prove cannot break takes no `attempt`; the compiler decides. `Stock(per_pack * packs)` with both at least 0 and no maximum on `Stock` stays plain; `Quantity(quantity * 2)` with a maximum of 1000 needs `attempt` and an arm for `max`. Changing a range flips call sites, and each one is an error where it is written, `failure.attempt_required` or `failure.attempt_infallible`, which names the comparison that proves it when one does. |
| Mapping form (question 7) | One form everywhere: `condition: Failure` inside the operation's block, for data operations, constructions (keyed by rule), authentication and integrations, with the kind checked against the condition (the table in [Turning one failure into another](../04-failures.md#turning-one-failure-into-another)). `invalid:` is the catch-all only for a construction's rules, and every other condition has its own arm. `match` arms are as area 3 sets them, and each area keeps its condition names. |
| Cause on mapping (question 8) | Always kept, without syntax: the original failure and its internal fields become the internal cause of the new one, shown in the log and never to the client. The compiler knows every mapping, so it always does what Rust's `source()` chain and Java's `cause` do by convention. |
| Overflow and the recursion limit (question 9) | Defects: the operation ends with 500 `internal_fault`, its writes roll back, and the log names the cause (`int_overflow`, `recursion_limit`). They are not declarable or catchable, because a caller cannot do anything sensible with "the arithmetic overflowed"; Swift traps the same way, and wrapping silently is ruled out (area 2). |
| Concealment (question 10) | Policy reports a hidden record with the same `not_found` failure the code already maps, and the log records that policy concealed it; `not_visible` is never declared and no alias syntax is needed. GitHub answers 404 for private repositories the same way. Area 9 says how policy applies it. |
| Internal fields in logs (question 11) | Identifiers, numbers, enums, booleans and times as values; text-based values, which can hold names or email addresses, as a keyed hash, so equal values still match up across events; secrets can never be failure fields. Area 9's data classification can refine it. |
| Failure text (question 12) | `message` is fixed English for developers and logs, a stable part of the API contract. The text for people is written by the app, keyed by the failure's code in its domain's language files, with its client fields as placeholders, and never on the failure type. The compiler writes it beside each API's OpenAPI document as `<api>.failures.<locale>.json`, code to wording, saying which placeholders the client formats and how; dates and numbers are sent as values for the client to format. Every failure that can reach a response is worded in every supported locale (area 10). |
| Failure declaration form (question 13) | `type Name = Failure { ... }`, the same form as every other type, so people and agents write code one way. It matches how mainstream languages build errors on an error base (`class X extends Error` in TypeScript, `struct X: Error` in Swift). |
| Failure fields (question 14) | Fields exist only in `client: { }` and `internal: { }`, so nothing reaches the client unless it is listed under `client`; without `client` the caller gets only the code, message and request id. |
| Failure placement (question 15) | One failure per file, in the raising domain's own `failures/` folder, or in `shared/failures/` once two or more domains raise it. Failures are numerous, part of the public contract and looked for as a group, so they get a folder of their own rather than sitting in `types/`. A failure another domain only handles stays where it is raised and is marked `public`. Generic behaviour comes from `kind`, the runtime owns the built-in failures, and the compiler generates a catalogue of every failure for documentation and API consumers. |
| Request id | Every error response carries `request_id` (also the `x-request-id` header), and every failure, expected ones included, is logged with it, so a client can quote it and a developer can trace it. |
| Kind is an enum | `kind:` takes a `FailureKind` value written `FailureKind.not_found`, never a string (area 2, question 17). |
| Runtime construction | From area 2: a runtime `Type(value)` check needs `attempt` with a block mapping each rule that can break to an `invalid_value` failure, `invalid:` covering the rest; literals are checked while compiling. |
| Failure arms | From area 3: one `match` handles call outcomes; failure arms have no `failure` prefix, every failure has an arm, and an arm recovers, `reject`s or `propagate`s. |
| `attempt` placement | From area 3: only at the start of a statement, and only on a call that can fail. |
| Signature | From area 3: the whole signature on the header line, `fails` before `->`. |
| Colon rule | From area 3: failure members are `name: value`, so context blocks are written `internal: { ... }`. |
