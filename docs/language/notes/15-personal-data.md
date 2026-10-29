---
title: Personal data, design notes
area: 15
---

# Personal data: design notes

Why [Personal data](../15-personal-data.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

- **Hard delete.** Area 5 says code cannot hard-delete a record with a
  lifecycle. Erasure does, as purge does; both are the compiler's own
  operations, which area 9 exempts from policy.
- **Two shapes for an end date.** `retention:` reuses purge's `after:` and
  `since:` but applies to visible records too. Purge stays the form for
  soft-deleted records.
- **Grants for erasure and export.** Area 9's grants are create, read, update
  and delete; this page maps erase to delete and export to read rather than
  adding effects.
- **`subject` already means something.** Area 8's principal has a `subject`
  (the sign-in identifier), so this page uses "person", not "data subject",
  in source.
- **Options as enum values.** `PersonalData.contact` and `Erasure.deleted`
  follow area 2's `Type.value` rule and its "values, not verbs" naming.

## Carried to later areas

| Item | Area |
|---|---|
| Hash every marked field in logs, whatever its type | 4 |
| Erasure and retention as compiler operations alongside purge; `retention:` beside `purge:` | 5 |
| Exports large enough to need paging or a snapshot read | 6, 12 |
| Erase maps to the delete grant and export to read; personal-data report changes in the approval review | 9 |
| Retention periods as settings | 10 |
| Integrations that receive marked fields; asking a provider to delete | 11 |
| Erasure of other stores by a generated subscriber; retention in the purge schedule; marked fields in event payloads | 12 |
| Generated tests: after `erase`, `export` returns nothing and no cache holds the person | 13 |
| The report's file and form | 14 |

## Decisions

| Topic | Decision |
|---|---|
| Setting options | Carried from area 2: options are enum values written `Type.value`, values rather than verbs |
| References | Carried from area 5: every field typed `X.id` is a reference with a required `on_delete:` |
| Lifecycle | Carried from area 5: `purge: { after: since: }` deletes hidden records; code cannot hard-delete a record with a lifecycle |
| Caches | Carried from area 5: caches are opt-in copies for display reads; erasing personal data wipes cached copies immediately |
| Logs | Carried from area 4: identifiers, numbers, enums, booleans and times as values; text as a keyed hash; secrets never |
| Stores | Carried from area 5: an action writes at most one store; stores declared in `app.jadpo` |
| Marking | `personal: PersonalData.category` from a fixed list of 14, eight sensitive |
| The person | `person: true` on the entity; ownership found by following references; several person entities allowed |
| Erasure outcomes | Generated `Person.erase(id)`; outcomes follow `on_delete:`; `erase: Erasure.deleted/.anonymised/.retained` required on restricted references |
| Kept records | Field-by-field erasure on the person too: retained fields stay until retention ends, the rest is cleared, the person cannot sign in; purge deletes the rest automatically |
| Backups | An erasure log replayed after restore; no per-person keys |
| Legal holds | Left to the application |
| Export | Generated `Person.export(id)` as JSON of everything belonging to the person, credentials left out |
| Retention | `retention: { after: since: }` on a field (clears) or entity (deletes); periods from config; applied by the purge schedule |
| Access records | Every export, erasure and read of a sensitive field recorded without values |
| Logs of marked fields | Every marked field hashed in logs, whatever its type (amends area 4) |
| Purpose and consent | Not in the language |
| Report and residency | `build/report/personal-data.md` and `.json`, shown in review when it changes; residency by deployment per region |
| Data requests | Endpoints only, no pages: `POST /privacy/requests`, `/confirm` and `/cancel` for self-service, the emails linking to the frontend's `link:` and `cancel_link:` paths; signed-in `POST /privacy/export` and `/privacy/erasure` for the person only; staff actions granted in policy; export by expiring link, erasure after a cooling-off period, events at every step; the emails are area 16's built-in emails, each taking `email:` or `send:` |
| Unreachable personal data | A marked field that belongs to no person is a compile error; every reference into a person's records has an erasure outcome |
| Erasure at providers | The package's `erasure: { call: by: }`, the connection's `erasure: { key: }`; the app erased first in one transaction, then each provider by a generated subscriber after commit, retried until confirmed; the completion email only when all confirm; parked deliveries shown; personal keys only in the outbox until delivered; a connection that cannot erase needs an acceptance stating why; OAuth grants deleted and revoked |
| Report additions | Erasure per connection, keys waiting in the outbox, multi-step step records and OAuth grants listed in the personal-data report |
| Coverage and gaps | The report states what the language covers, what is the app's choice and what is not code; breach-notification and transfer support to add |
