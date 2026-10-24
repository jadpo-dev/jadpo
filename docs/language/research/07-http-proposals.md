# Public API record and accepting reviewed changes

**Status:** research and proposal for [area 7](../07-http.md), question 10,
2026-10-12. Nothing here is decided.

## What is complicated today

Two things were carried to area 7:

1. **A committed record of the public API** (routes, fields, failure codes),
   so a breaking change is caught at review rather than by a client in
   production (area 4, question 1).
2. **One way to accept a reviewed change**, shared by everything that must not
   change silently: a breaking API change, a database drop or a move between
   stores (area 5), a policy weakening (area 9 and the
   approval protocol), and a new or renamed
   domain (area 1). The compiler flags the change; the build passes only once
   a person records an acceptance.

Today each of these is half-built in its own way:

| Change | What exists | Gap |
|---|---|---|
| API | `build/compatibility/public-failure-codes.json` (failure codes only) and `build/openapi/openapi.json` | Both live in `build/`, which is disposable, so there is no baseline: the file says `baseline_not_configured`. Routes and fields are not recorded at all. |
| Database | Area 5 decided migrations, two-step removal and "a drop needs explicit acceptance" | No acceptance form. |
| Policy | `build/approval/subject.json` and `jadpo approval --before`, "non-releasable" locally | Attestation is a design; nothing records a decision. |
| Domains | Area 1 decided "appears in the approval review" | Nothing yet. |

## How other ecosystems do it

**Committed record, check fails on drift.** Microsoft's API Extractor commits
an API report (`etc/<package>.api.md`). A local build updates it; a CI build
fails with "You have changed the public API signature" until the updated file
is committed, and a branch policy can require a named reviewer whenever that
file changes
([API Extractor](https://api-extractor.com/pages/overview/demo_api_report/)).
Kotlin's binary-compatibility-validator is the same: `apiDump` writes a
readable `.api` file into `api/`, and `apiCheck` (part of `check`) fails when
the code no longer matches it
([BCV](https://github.com/Kotlin/binary-compatibility-validator)). Django's
`makemigrations --check` exits non-zero when models changed without a
migration
([Django](https://docs.djangoproject.com/en/5.1/ref/django-admin/)). Strength:
the review is an ordinary diff of one readable file. Weakness: adding a field
and deleting one look equally important.

**Classified diff against a baseline.** `buf breaking` compares protobuf
schemas against a baseline such as `--against '.git#branch=main'`, with rule
sets of different strictness (`FILE`, `PACKAGE`, `WIRE_JSON`, `WIRE`);
exceptions are configured per file or rule with `ignore` and `ignore_only`
([buf](https://buf.build/docs/breaking/),
[buf.yaml](https://buf.build/docs/configuration/v2/buf-yaml/)). oasdiff does
the same for OpenAPI with hundreds of checks, using the rule "breaking if a
previously valid request can now be rejected, or a response can carry
something clients were not written to handle"
([oasdiff](https://www.oasdiff.com/)). Apollo GraphOS checks a proposed
GraphQL schema against real client operations, and a reviewer marks a flagged
change "safe" or "ignored" for each operation
([GraphOS checks](https://www.apollographql.com/docs/graphos/platform/schema-management/checks/reference)).
Strength: only real breaks stop the build. Weakness: the exception lives in a
config file that anyone, including an agent, can edit, and an `ignore` keeps
applying to future changes.

**Explicit acceptance at the site of the danger.** strong_migrations blocks a
dangerous migration (`remove_column`) until it is wrapped in
`safety_assured { ... }`, and recommends removing a column in two deploys:
`ignored_columns` first, then the drop
([strong_migrations](https://github.com/ankane/strong_migrations)). This is
exactly area 5's retire-then-drop. Strength: the acceptance is local and
one-off. Weakness: it is just code, so nothing shows a person approved it.

**Versioning instead of breaking.** Stripe pins each account to the dated
version of its first request, lets a request override it with
`Stripe-Version`, and keeps old versions working by transforming responses
backwards; adding endpoints and fields is safe, and existing fields keep
their names and types ([Stripe](https://stripe.com/blog/api-versioning)).
GitHub uses dated versions in `X-GitHub-Api-Version` and supports each for at
least 24 months
([GitHub](https://docs.github.com/en/rest/about-the-rest-api/api-versions)).
This is the answer for a large public API, and expensive; it only matters
once a break cannot be avoided.

## Options

### A. Committed record only

`jadpo check` fails while `api/main.api` disagrees with the source;
`jadpo api update` rewrites it. The pull request diff is the review.

- Simple, and the same shape as area 5's "check fails on drift".
- Does not tell a reviewer which lines break clients, and does nothing for
  drops, policy or domains.

### B. Record plus classified breaks, accepted in configuration

As A, plus the compiler classifies each difference. Breaking ones need an
entry in an `ignore` list in `app.jadpo`.

- Familiar from buf and oasdiff.
- An ignore list is standing configuration: it keeps hiding later changes,
  and it is a second mechanism next to migrations and policy approval.

### C. Records plus one acceptance per reviewed change (recommended)

Every surface that must not change silently has a committed record: the API
record in `api/`, the migrations in `database/`, policy and domains in the
approval subject. When a change to one of them needs a person, the compiler
reports it with a stable name, and the build passes only when a matching
acceptance exists:

```jadpo fragment
// acceptances/20261020T1400_remove_todo_priority.jadpo
/// Priority was never shown by any client; the mobile app stopped sending it in 3.2.
acceptance {
    change: output_field_removed
    subject: main.TodoView.priority
}
```

```jadpo fragment
// acceptances/20261021T0900_drop_legacy_flag.jadpo
/// Retired in 20261001T1000_retire_legacy_flag; no build reads it.
acceptance {
    change: column_dropped
    subject: Todo.legacy_flag
}
```

- One form for every kind of change: `change:` is a closed option from the
  compiler's list (`route_removed`, `input_field_required`,
  `output_field_removed`, `failure_code_changed`, `column_dropped`,
  `store_moved`, `policy_weakened`, `domain_created`, ...), `subject:` names
  the thing, and the reason is the `///` comment (area 1 already uses `///`
  for descriptions). The date is in the file name, as for migrations.
- Each acceptance is used once. When `jadpo api update` (or
  `jadpo migrate new`) records the change, it writes the acceptance's name
  beside it in the record, so removing a field, adding it back and removing
  it again needs a second acceptance. An acceptance that no record uses is an
  error, so stale ones cannot pile up.
- Additive changes (a new route, a new optional input field, a new output
  field) update the record and need no acceptance; the diff still shows them.
- The compiler's message names the change and says that a person writes the
  acceptance. An agent stops there; it does not write one.
- **Who accepted it** comes from the host's protected review: the
  `acceptances/` folder (and `policy.jadpo`) is owned in CODEOWNERS, so a
  pull request that adds one needs the owner's approval. This is the approval
  protocol's rule (use the review system, invent no cryptography); the
  acceptance file makes each decision visible and named, the review makes it
  attributable.

The API record is one readable text file per API, sorted, so a diff reads
like a changelog:

```text
// api/main.api — generated by `jadpo api update`; do not edit
version: 15

GET /todos/{todo_id: Uuid}
    output: TodoView
    fails: todo_not_found 404
PATCH /todos/{todo_id: Uuid}
    input: PatchTodo
    output: TodoView
    fails: todo_not_found 404, empty_patch 422, due_date_in_past 422

TodoView
    id: Uuid
    title: Text { min_length: 1, max_length: 200 }
    removed priority (20261020T1400_remove_todo_priority)

failure todo_not_found 404 "Todo not found."
```

`version:` rises by one on each update and becomes the OpenAPI
`info.version`, which is `0.0.0` today. OpenAPI stays generated in `build/`:
it is too verbose to review, and both are produced from the same facts.

What counts as breaking follows oasdiff's rule. For a request: a route or
method removed, a path placeholder's type changed, an input field added
without `optional`, a rule tightened (`max_length` lowered, an enum variant
removed from an input), authentication added. For a response: an output field
removed, renamed or made nullable, a type changed, an enum variant added to an
output (clients' exhaustive switches break; oasdiff warns on this), a success
status changed, a failure's code, status, message or client fields changed
(area 4's existing keys).

## Trade-offs

| | A | B | C |
|---|---|---|---|
| Reviewer sees which lines break clients | No | Yes | Yes |
| One mechanism for API, data, policy and domains | No | No | Yes |
| Exceptions stay one-off | n/a | No (ignore lists persist) | Yes |
| New root folders | `api/` | `api/` | `api/`, `acceptances/` |
| Cost | Low | Medium | Medium: one diff engine per surface, which the approval subject and migrations already need |

## Recommendation

Option C. It is A's committed, readable record (the cheapest proven
approach), buf's and oasdiff's classification so only real breaks need a
person, and strong_migrations' one-off acceptance, in one form shared by
areas 1, 5, 7 and 9. It adds two root folders, `api/` and `acceptances/`,
which area 1's fixed root list must include. Versioning (option "Stripe")
stays out until an application has outside clients it cannot coordinate
with; then a second API (`v2`) is declared, as in the page's question 7.
