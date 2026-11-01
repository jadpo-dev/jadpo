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

## Why the area page is the way it is

- **A fixed list of categories.** The fourteen `PersonalData.` categories
  follow GDPR's special categories and the common groups used by privacy
  tools such as Fides. A bare `personal: true` can't tell health data from
  an email address, and there are no custom categories, so reports stay
  comparable and the sensitive-data rules exact.
- **The marking is the classification.** Hashing every marked field in
  logs, whatever its type, refines area 4, which logs dates and numbers as
  values.
- **Ownership by references, not tags.** The compiler follows every
  reference to find a person's records, because tagging each owning
  reference would repeat what references already say.
- **No crypto-shredding.** There are no per-person encryption keys:
  deleting a person's key breaks unique lookups, indexes and search on
  encrypted fields, and no regulator has said key deletion counts as
  erasure. Backups are handled by the erasure log instead.
- **Legal holds left to the app.** Holds and restriction while a complaint
  is open are rare in the apps Jadpo targets, and a built-in one would
  also have to pause purge and retention.
- **No purpose or consent in the language.** The compiler can check
  neither, so purposes and lawful bases belong in the privacy notice, and
  consent is ordinary application data.
- **Access records only for sensitive reads.** Reads of ordinary personal
  data aren't recorded one by one: GDPR asks for accountability and
  security, not a log of every read, and that would log almost every
  request.
- **No region setting.** One the compiler can't verify would give false
  comfort, so residency is by deployment, and the report names the setting
  that says where each store is.
- **Encrypted fields.** The full design of `encrypted: true` belongs to
  this area and is built with personal data, together with keys held as
  data (a key per device or person in an encrypted field, usable only as a
  `crypto` call's `key:`).

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
| The person | `person: true` on the entity, and only `true`; ownership found by following references, through other people's records too, a chain ending where it comes back to an entity it passed; several person entities allowed |
| Erasure outcomes | Generated `Person.erase(id)`; outcomes follow `on_delete:`; `erase: Erasure.deleted/.anonymised/.retained` required on restricted references |
| Kept records | Field-by-field erasure on the person too: retained fields stay until retention ends, the rest is cleared, the person cannot sign in; purge deletes the rest automatically |
| Backups | An erasure log replayed after restore; no per-person keys |
| Legal holds | Left to the application |
| Export | Generated `Person.export(id)` as JSON of everything belonging to the person, credentials left out |
| Retention | `retention: { after: since: }` on a field (clears) or entity (deletes); periods from config; applied by the purge schedule |
| Retention, as built (chunk P2) | `after:` is a `Duration` setting, never a duration written in place, since the page puts every period in `config.jadpo` (`data.retention_period`); `since:` an `Instant` field, a record whose `since:` is `none` not counted yet. A field's retention needs a nullable field no lifecycle owns or reads (`data.retention_field_required`, `data.retention_lifecycle_field`), since clearing a lifecycle's field would show a deleted record again; clearing is a change, so `Generated.on_change` fields take the run's time. An entity's deletes hidden and visible records with their cascades, and a record a `restricted` reference holds, directly or through a cascade, is a compile error (`data.retention_held`) rather than purge's quiet wait, so no record outlives its retention unseen. Someone who signs in is deleted with their sessions, keys, passkeys and sign-in links in one statement; one reached only through a cascade, and any `File` a retention would reach, are `toolchain.not_built_yet` until erasure (P4) deletes those. One purge schedule per entity applies its retention, then its purge, then its fields' retentions, sharing 20 batches of 500 a run; `items` counts records deleted and values cleared. No application test runs it, as for purge |
| Erasure, as built (chunk P3) | A record's outcome comes from the reference linking it to a record erasure deletes: its `erase:`, which may be written on any reference into a person's records and overrides `on_delete:`, or else cascade as deleted and cleared as anonymised. Erasure walks on only through what it deletes; a kept record's own children stay with it, as when an ordinary delete clears a reference. `erase:` is required on a `restricted` reference into records erasure deletes (`data.erase_required`), not on one into records it only keeps, where it would never be read; a field it clears must be nullable (`data.erase_field_required`), `Erasure.retained` needs the entity's `retention:` (`data.erase_retention_required`), and an `erase:` on a field that is no reference, on a reference to records no person reaches, or on an event's field is refused (`data.erase_unreached`). `User.erase` takes the person's `id`, no block and no `conflict:`, needs `delete` on the person's record, reaches them hidden or not, and fails with their not-found failure; an action calling it runs at repeatable read, so a record added since its snapshot reruns or fails it rather than being cleared unseen. One statement does it all, the sign-in state of each deleted entity someone signs in as included. Refused as `toolchain.not_built_yet` where `erase` is called until decided or built: a `File`'s bytes, and anonymising another person's own record (`User.invited_by_id` cleared), since its marked fields are its own person's. The person's own field's `erase:` is P4's |
| Access records | Every export, erasure and read of a sensitive field recorded without values |
| Logs of marked fields | Every marked field hashed in logs, whatever its type (amends area 4) |
| Purpose and consent | Not in the language |
| Report and residency | `build/report/personal-data.md` and `.json`, shown in review when it changes; residency by deployment per region |
| Data requests | Endpoints only, no pages: `POST /privacy/requests`, `/confirm` and `/cancel` for self-service, the emails linking to the frontend's `link:` and `cancel_link:` paths; signed-in `POST /privacy/export` and `/privacy/erasure` for the person only; staff actions granted in policy; export by expiring link, erasure after a cooling-off period, events at every step; the emails are area 16's built-in emails, each taking `email:` or `send:` |
| Unreachable personal data | A marked field that belongs to no person is a compile error (`data.personal_unreachable`, reported once per entity at its first marked field); every reference into a person's records has an erasure outcome. Only `personal:` marks a field for this: a standard type that is personal data whatever holds it is treated as marked in logs, caches and access records (area 19), not here, so a business's own phone number doesn't make it a person |
| Erasure at providers | The package's `erasure: { call: by: }`, the connection's `erasure: { key: }`; the app erased first in one transaction, then each provider by a generated subscriber after commit, retried until confirmed; the completion email only when all confirm; parked deliveries shown; personal keys only in the outbox until delivered; a connection that cannot erase needs an acceptance stating why; OAuth grants deleted and revoked |
| Report additions | Erasure per connection, keys waiting in the outbox, multi-step step records and OAuth grants listed in the personal-data report |
| Coverage and gaps | The report states what the language covers, what is the app's choice and what is not code; breach-notification and transfer support to add |
