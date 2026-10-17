---
title: Personal data
area: 15
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules: []
---

# Personal data

How a Jadpo application says which data is about a person, so the compiler
can export it, erase it, expire it and report where it lives. The aim is
that GDPR, UK GDPR and CCPA/CPRA requests work out of the box.

> Decided on 2026-10-13. Nothing on this page is built yet; it comes after the first build (roadmap). Examples apply the decisions from areas 1 to 15. The legal research and the options considered are in [the proposals](research/15-personal-data-proposals.md).

**What the law asks of an application.** Keep only the personal data you
need, for as long as you need it (GDPR Article 5). Give a person a copy of
everything you hold about them, in a machine-readable form, within a month
(Articles 12, 15 and 20). Delete it when they ask, unless the law makes you
keep some of it, such as invoices (Article 17). Build this in from the start
(Article 25), keep a record of what you hold and why (Article 30), and
protect it (Article 32). CCPA/CPRA gives Californians the same rights to
know and to delete. Every one of these needs the application to know which
data is personal and whose it is; that is the one fact Jadpo asks you to
write down.

| Capability | What you write | What the compiler does |
|---|---|---|
| Marking | `personal:` on a field | Hashes it in logs, lists it in the report |
| The person | `person: true` on an entity | Finds every record that belongs to them |
| Erasure | `erase:` on references it cannot work out | Generates `User.erase(id)` across every store and cache |
| Export | nothing | Generates `User.export(id)` |
| Retention | `retention:` on a field or entity | Clears or deletes it when the time is up |
| Report | nothing | Writes `build/report/personal-data.md` |

## Marking personal fields

A field that identifies or describes a person says so in its block, with
one category from a fixed list (question 1). The category decides how
carefully the compiler treats the value; the list follows GDPR's special
categories and the common groups used by privacy tools such as Fides.

```jadpo fragment
// domains/accounts/entities/user.jadpo
entity User {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }
    display_name: DisplayName {
        personal: PersonalData.identifying
    }
    ...
}
```

| `PersonalData.` | For | Sensitive |
|---|---|---|
| `identifying` | Names, usernames, account numbers | |
| `contact` | Email, phone, postal address | |
| `financial` | Bank and card details, income, purchases | |
| `online` | IP address, device and cookie identifiers, browsing | |
| `demographic` | Date of birth, gender, nationality | |
| `government_id` | Passport, national insurance, driving licence | yes |
| `location` | Precise location | yes |
| `health` | Physical or mental health, including disability | yes |
| `biometric` | Fingerprints, face data used to identify | yes |
| `genetic` | Genetic data | yes |
| `beliefs` | Religion, political opinions, trade union membership | yes |
| `ethnicity` | Racial or ethnic origin | yes |
| `sex_life` | Sex life or sexual orientation | yes |
| `criminal` | Criminal convictions and offences | yes |

"Sensitive" covers GDPR's special categories (Articles 9 and 10) and the
extra ones CCPA/CPRA lists, government IDs and precise location.

- **Logs.** A marked field is written to logs as a keyed hash whatever its
  type, so a marked `date_of_birth: CalendarDate` is hashed too (question
  10). This refines area 4, which logs dates and numbers as values.
- **Sensitive fields** are never put in a cache, and every read of one is
  recorded (question 9).
- **Records a person owns** need no marking on every field. A todo's title is
  the user's data because the todo belongs to them (next section), so it is
  exported and erased with them. Marking is for the fields whose value points
  at, or describes, a person.

## The person

The law calls the person data is about the *data subject*. An entity that
represents one says `person: true` (question 2). Most applications have one,
`User`; a clinic might also have `Patient`, a shop a guest `Customer`.

```jadpo fragment
entity User {
    ...
    person: true
}
```

A record **belongs to** a person when it refers to them, directly or through
a chain of references: a todo through `owner_id`, a todo's attachment through
the todo. The compiler follows every reference, so nothing has to be listed
twice, and the report shows the whole tree for review. A record that refers
to two people, such as a message from one user to another, belongs to both.

## Erasure

Erasing a person is a generated operation, `User.erase(user_id)`, called from
an action you write (for a "delete my account" route, or a support tool).
It removes the person's record and decides what happens to every record that
belongs to them (question 3):

| `erase:` on a reference to a person | What happens to the record | Needs |
|---|---|---|
| `Erasure.deleted` | Deleted, with everything that cascades from it | |
| `Erasure.anonymised` | Kept; the reference and every marked field are cleared | the reference and marked fields nullable |
| `Erasure.retained` | Kept as it is until its own retention ends; only the reference is cleared | the reference nullable, and `retention:` on the entity |

Most references need nothing written: `OnDelete.cascade` means
`Erasure.deleted` and `OnDelete.cleared` means `Erasure.anonymised`. A
reference that is `OnDelete.restricted` must say `erase:`, because "a user
with todos cannot be deleted" is the right rule for a mistaken click but the
wrong one for a legal request.

```jadpo fragment
// domains/planning/entities/todo.jadpo
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        role: TodoRole.owner
        erase: Erasure.deleted          // erasing the owner deletes their todos
    }
    title: TodoTitle
    ...
}
```

Invoices are the usual case for `Erasure.retained`: tax law requires them
for several years, which GDPR Article 17(3) allows. The invoice keeps its own
copy of the billing name and address, so it no longer needs the user:

```jadpo fragment
// domains/billing/entities/invoice.jadpo
entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained         // kept for tax; the link to the user goes
    }
    billing_name: PersonName {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention // six years in the UK
        since: issued_at
    }
}
```

What erasure reaches:

- **Every store and cache.** The main store's records go in one
  transaction. Cached copies are wiped at once (decided in area 5). Records
  in another store are erased by a generated subscriber straight after,
  because an action writes at most one store.
- **Records with a lifecycle** are removed outright, not soft-deleted:
  erasure, like purge, is the compiler's own operation.
- **Backups** are not rewritten. Each erasure leaves an entry in an erasure
  log (the person's `id` and the time, nothing else), and restoring a backup
  replays the log before the application starts. Backups themselves expire
  on the hosting platform's schedule, which the report states. This is what
  the UK regulator (ICO) calls putting data "beyond use" (question 5).
- **Logs** are not rewritten either. They hold identifiers and hashes, never
  marked values, and expire on their own retention.
- **Other companies.** Data already sent to an integration (an email
  provider, a payment provider) is outside the database; the report lists
  every integration that receives a marked field, so the team knows whom to
  ask (question 12).
- **Legal holds** are application data: an action that checks a hold flag
  and refuses with its own failure before calling `User.erase` (question 6).
  The same goes for restricting use while a complaint is open (Article 18).

## Export

`User.export(user_id)` returns everything held about the person: their
record and every record that belongs to them, as JSON, one list per entity,
with the same field names as the API (question 7). That covers both the
right of access and the right to take your data elsewhere ("portability",
Article 20). Credentials, such as token hashes, are left out. The export
also says how long each kind of data is kept and which companies receive it,
from the report, because Article 15 asks for that too.

```jadpo fragment
// domains/accounts/actions/export_my_data.jadpo
/// Gives the signed-in user a copy of everything held about them.
/// The route passes the caller in: `run: export_my_data(current_principal.id)`.
action export_my_data(user_id: User.id) -> PersonalDataExport {
    return User.export(user_id)
}
```

Who may call `erase` and `export` is policy (area 9): `export` needs read
access to the person's record and `erase` needs delete access. Once allowed,
both reach every record that belongs to the person, as purge does, because a
partial export or erasure is worse than none.

## Retention

GDPR's storage limitation means personal data needs an end date. A
`retention:` block, in the same `after:` and `since:` shape as lifecycle
purge, gives one (question 8):

- **On a field**, the value is cleared when the time is up; the field must be
  nullable. Good for an IP address kept for 30 days for fraud checks.
- **On an entity**, the record is deleted, with whatever cascades from it.
  Good for invoices, or accounts inactive for two years.

```jadpo fragment
entity User {
    ...
    signup_ip: IpAddress? {
        personal: PersonalData.online
        retention: {
            after: config.signup_ip_retention
            since: created_at
        }
    }
}
```

The periods are settings in `config.jadpo` (area 10), so they are set per
environment and appear in the report. The compiler's built-in purge schedule
(area 12) applies retention too, in small batches, each logged. A person
entity, or an entity holding marked fields, with no retention and no purge
is listed in the report as "kept until erased".

## What the compiler checks

- **Marked fields nothing reads.** A personal field that no query, output or
  action reads is a warning: you are holding data you do not use (data
  minimisation, Article 5). *Amended 2026-10-17 (Richard, area 1, question 11):* a declinable error,
  since a field kept for a legal reason may be read only by export.
- **Erasure is complete.** Every reference into a person's tree has an
  erasure outcome; `Erasure.anonymised` and `Erasure.retained` have the
  nullable fields and retention they need.
- **Sensitive fields stay out of caches**: a cache on an entity that holds
  one leaves those fields out.

There is no `purpose:` setting and no consent model (question 11). Purposes
and lawful bases belong in the privacy notice, which the team writes from the
report; consent is ordinary application data, such as a `Consent` entity with
the purpose and the time it was given and withdrawn.

## Access records

Every export and erasure is recorded: who asked, for which person `id`, and
when, never the values. So is every read of a sensitive field (question 9). These are log events with a long retention, so an organisation can show a
regulator what it did. Reads of ordinary personal data are not recorded one by
one; that would log almost every request.

## Where data is stored

Each store is one database in one place. Data that must stay in the EU lives
in a deployment whose stores are in the EU; an application serving several
regions runs one deployment per region (decided in area 5's research, P6).
There is no region setting in source (question 12); the report lists each
store with the configuration setting that says where it is.

## The personal-data report

`jadpo check` writes `build/report/personal-data.md` and a `.json` twin
(area 14's report form). It lists, for each person entity:

- every marked field, its category, entity, store and caches;
- who may read it, from policy;
- its retention, or "kept until erased";
- what erasure does to each record that belongs to the person;
- every integration a marked field is sent to;
- sensitive fields, flagged.

That is most of a record of processing activities (Article 30). The team adds
what the compiler cannot know: the purpose of each use, the lawful basis, and
the organisation's details. Changes to the report appear in the approval
review (area 9), so a pull request that starts sending email addresses to a
new provider is seen by a person before it ships.

## Review

### Status

Nothing on this page is built yet. It is first on the roadmap's list of
work after the first build.

### Consistency with the rest of the language

- **Area 4's log rule.** Area 4 logs dates, numbers and enums as values and
  hashes only text. A marked `date_of_birth` or `health_status` enum would be
  logged in the clear; question 10 proposes hashing every marked field, which
  area 4 left open ("area 9's data classification can refine it later").
- **Hard delete.** Area 5 says code cannot hard-delete a record with a
  lifecycle. Erasure does, as purge does; both are the compiler's own
  operations, which area 9 already exempts from policy.
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

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Hash every marked field in logs, whatever its type | 4 | Question 10 |
| Erasure and retention as compiler operations alongside purge; `retention:` beside `purge:` | 5 | Questions 3 and 8 |
| Exports large enough to need paging or a snapshot read | 6, 12 | Question 7 |
| Erase maps to the delete grant and export to read; personal-data report changes in the approval review; answers area 9 question 11 | 9 | Questions 7 and 12 |
| Retention periods as settings | 10 | Question 8 |
| Integrations that receive marked fields; asking a provider to delete | 11 | Question 12 |
| Erasure of other stores by a generated subscriber; retention in the purge schedule; marked fields in event payloads | 12 | Questions 3 and 8 |
| Generated tests: after `erase`, `export` returns nothing and no cache holds the person | 13 | Input to area 13 |
| The report's file and form; the `Expected {expected}` bug in parse errors | 14 | Question 12; Status |

### Open questions

1. **Is a personal field marked `personal: PersonalData.category`, from a
   fixed list?** A bare `personal: true` cannot tell health data from an
   email address, and the law treats them differently. *Recommendation:* yes,
   one category per field from the fourteen above, no custom categories.
   Fides lets teams extend its taxonomy; a fixed list keeps reports
   comparable and the sensitive-data rules exact. *Decided 2026-10-13:* yes, as recommended.
2. **Is the person declared `person: true` on the entity, with ownership
   found by following references?** *Recommendation:* yes, several person
   entities allowed. The alternative, tagging each owning reference, repeats
   what references already say; Fides and Transcend both walk relationships
   from an identity in the same way. *Decided 2026-10-13:* yes, as recommended.
3. **Does erasure follow `on_delete:`, with `erase:` required only on
   restricted references?** *Recommendation:* yes, with three outcomes,
   `Erasure.deleted`, `Erasure.anonymised` and `Erasure.retained`.
   Inferring from `on_delete:` means most apps write one or two `erase:`
   lines, and the compiler refuses an erasure it cannot complete. *Decided 2026-10-13:* yes, as recommended.
4. **When a record is kept, does it keep its own copy of the details it
   needs, rather than a stub of the person?** *Recommendation:* yes: the
   person's record is always deleted, and kept records (`Erasure.retained`)
   must have their own retention. A half-erased user record breaks unique
   fields and required types; invoices copy billing details anyway.
   *Revised 2026-10-13 after Richard's Waggel case* (insurance records kept
   at least five years): the person's own record cannot always go. Revised
   recommendation: erasure works field by field on the person too. A field
   marked `erase: Erasure.retained` (with the entity's `retention:`) stays;
   every other marked field is cleared; the person can no longer sign in,
   receives nothing, and is excluded from every query except those that
   serve the retained purpose; when retention ends the purge schedule deletes
   the rest automatically. Where nothing must be kept, the record is deleted
   as before. *Decided 2026-10-13:* the revised recommendation.
5. **Are backups handled by an erasure log replayed after restore, not by
   per-person encryption keys?** *Recommendation:* yes. Deleting a person's
   key ("crypto-shredding") reaches backups but breaks unique lookups,
   indexes and search on encrypted fields and needs key management; the ICO
   accepts backups being put beyond use and expiring, and no regulator has
   said key deletion counts as erasure. django-gdpr-assist replays erasures
   after a restore the same way. Revisit for sensitive fields if a real app
   needs it. See the
   [proposals](research/15-personal-data-proposals.md). *Decided 2026-10-13:* yes, as recommended.
6. **Are legal holds and restriction (Article 18) left to the
   application?** *Recommendation:* yes: a hold is a field and a failure the
   erase action checks. Both are rare in the apps Jadpo targets; a built-in
   one would also have to pause purge and retention. *Decided 2026-10-13:* yes, as recommended.
7. **Is export generated JSON of every record that belongs to the person,
   gated by read access to the person's record?** *Recommendation:* yes,
   with credentials left out. One generated operation covers access and
   portability; Rails and Django leave this to hand-written serialisers,
   which miss new tables. *Decided 2026-10-13:* yes, as recommended.
8. **Is retention a `retention: { after: since: }` block on a field (clears
   it) or an entity (deletes it)?** *Recommendation:* yes, periods from
   `config`, applied by the purge schedule. Field retention covers IP
   addresses and logs of activity; entity retention covers invoices and
   inactive accounts. *Decided 2026-10-13:* yes, as recommended.
9. **Which access is recorded?** *Recommendation:* every export and
   erasure, and every read of a sensitive field, with no values; not
   ordinary reads. GDPR asks for accountability and security, not a log of
   every read; health apps are the case that needs read records. *Decided 2026-10-13:* yes, as recommended.
10. **Are marked fields always hashed in logs and failure context?**
    *Recommendation:* yes, whatever the type. This refines area 4's rule and
    answers area 9's question 11: the marking is the classification. *Decided 2026-10-13:* yes, as recommended.
11. **Should the language model purpose and consent?** *Recommendation:*
    no. Neither can be checked by the compiler; purposes go in the privacy
    notice built from the report, and consent is application data. Fides and
    OneTrust sell consent as a separate product for the same reason. *Decided 2026-10-13:* yes, as recommended.
12. **Is the report a build file that lists stores, integrations and
    retention, with no region setting in source?** *Recommendation:* yes:
    `build/report/personal-data.md` and `.json`, shown in the approval review
    when it changes. Residency stays a deployment per region; a region
    setting the compiler cannot verify would give false comfort. *Decided 2026-10-13:* yes, as recommended.
13. **Are there generated routes for data requests?** Raised by Richard
    (2026-10-13). *Recommendation:* yes, switched on with
    `privacy: { routes: true }` in `app.jadpo`. `POST /privacy/export`
    (signed in, recent sign-in required, `RouteAuthentication.fresh`)
    answers 202 and delivers a download link when the export is ready;
    `POST /privacy/erasure` (same checks) records an erasure request and
    carries it out after a cooling-off period the app sets (default 14
    days, cancellable by the person). Requests received another way (email,
    support) are created by staff through generated actions after they
    verify the person. Every request is a record with its one-month legal
    deadline; `jadpo privacy requests` and the console list them, and one
    nearing its deadline raises an alert.
    *Revised 2026-10-13 after Richard asked for it to work with no frontend
    built:* `privacy: { self_service: { email: <connection> } }` in
    `app.jadpo` switches on the whole flow.
    - **Out of the box:** a small generated privacy page at `/privacy`
      (accessible, translated, branded from configuration, designed with the
      design skills before it is built). A person enters their email; the
      reply is always "if an account exists, we have emailed you", so it
      never reveals who has an account. A single-use link proves they control
      the address; they choose export or erasure and confirm.
    - **Export** is delivered by email as a single-use download link that
      expires after 7 days, never as an attachment; the file is encrypted at
      rest, deleted on expiry, and every download is in the access records.
    - **Erasure** waits out the cooling-off period, with a cancel link in the
      confirmation email; the completion email says what was kept and why
      (the law allows keeping records such as insurance policies, and the
      person must be told).
    - **Events** at every step: `PrivacyRequestReceived`,
      `PrivacyExportReady` (with the download link), `PrivacyErasureScheduled`,
      `PrivacyErasureCompleted`, `PrivacyRequestOverdue`. The built-in emails
      are subscribers to these; an app can add its own (Slack, a CRM) or
      replace the emails. With no mail connection, the compiler requires a
      subscriber that delivers `PrivacyExportReady`.
    - **Who may ask, three ways:** the person themselves, through the page
      (email verified) or the API (`POST /privacy/export` and
      `/privacy/erasure`, signed in recently, acting only on their own
      record: there is no person id to pass); staff, through generated actions
      for requests received by email or phone, granted in `policy.jadpo`
      (`privacy.create_request: [StaffRole.support]`), recording who created
      it and how identity was checked, with the export still going to the
      person's own email, never to the staff member; and never a schedule or
      subscriber, except retention purges.
    - **Protection:** rate limits on the page and API (area 16), links
      single-use and short-lived, every step in the access records. *Decided 2026-10-13:* the revised recommendation.
14. **How is personal data that erasure cannot reach prevented?** Raised by
    Richard (2026-10-13): no siloed data left behind. *Recommendation:* a
    compile error for any marked personal field on an entity that is not a
    person and does not belong to one through references ("erasure cannot
    find this: reference the person, or mark the entity `person: true`").
    With area 15's completeness check (every reference into a person's
    records has an erasure outcome), every stored copy of personal data is
    reachable. Files (area 16) attached to erased records are deleted with
    them; caches are wiped (area 5); logs only hold hashes (area 4); backups
    replay the erasure log (question 5). *Decided 2026-10-13:* yes, as recommended.
15. **How are copies held by providers erased?** *Recommendation:* erasure
    emits a `PersonErased` event; a provider package can declare its
    deletion operation (Stripe's customer delete, an email provider's
    contact delete), and the compiler requires a subscriber for every
    connection that receives marked fields, or an accepted reason it is not
    needed (area 18). The personal-data report lists each provider and
    whether its copy is erased automatically. *Decided 2026-10-13:* yes, as recommended.
16. **What does this not cover?** *Recommendation:* say so plainly in the
    report. Covered by the language: minimisation and storage limitation,
    access, rectification (ordinary updates), erasure, portability,
    privacy by design, records of processing (most of Article 30), security
    of processing. Supported but the application's choice: restriction and
    objection, consent. Not code at all: lawful basis, privacy notice,
    DPIAs, processor contracts, the DPO. Added support worth building:
    breach notification (from the report and access records, list the
    people whose marked data a breached store or provider held) and
    international transfers (provider packages declare where data is
    processed, shown in the report). Not legal advice; a privacy
    professional reviews the report. *Decided 2026-10-13:* yes, as recommended.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Setting options | Carried from area 2: options are enum values written `Type.value`, values rather than verbs | 2026-10-12 |
| References | Carried from area 5: every field typed `X.id` is a reference with a required `on_delete:` | 2026-10-12 |
| Lifecycle | Carried from area 5: `purge: { after: since: }` deletes hidden records; code cannot hard-delete a record with a lifecycle | 2026-10-12 |
| Caches | Carried from area 5: caches are opt-in copies for display reads; erasing personal data wipes cached copies immediately | 2026-10-12 |
| Logs | Carried from area 4: identifiers, numbers, enums, booleans and times as values; text as a keyed hash; secrets never | 2026-10-12 |
| Stores | Carried from area 5: an action writes at most one store; stores declared in `app.jadpo` | 2026-10-12 |
| Marking | `personal: PersonalData.category` from a fixed list of 14, eight sensitive | 2026-10-13 |
| The person | `person: true` on the entity; ownership found by following references; several person entities allowed | 2026-10-13 |
| Erasure outcomes | Generated `Person.erase(id)`; outcomes follow `on_delete:`; `erase: Erasure.deleted/.anonymised/.retained` required on restricted references | 2026-10-13 |
| Kept records | Field-by-field erasure on the person too: retained fields stay until retention ends, the rest is cleared, the person cannot sign in; purge deletes the rest automatically | 2026-10-13 |
| Backups | An erasure log replayed after restore; no per-person keys for now | 2026-10-13 |
| Legal holds | Left to the application | 2026-10-13 |
| Export | Generated `Person.export(id)` as JSON of everything belonging to the person, credentials left out | 2026-10-13 |
| Retention | `retention: { after: since: }` on a field (clears) or entity (deletes); periods from config; applied by the purge schedule | 2026-10-13 |
| Access records | Every export, erasure and read of a sensitive field recorded without values | 2026-10-13 |
| Logs of marked fields | Every marked field hashed in logs, whatever its type (amends area 4) | 2026-10-13 |
| Purpose and consent | Not in the language | 2026-10-13 |
| Report and residency | `build/report/personal-data.md` and `.json`, shown in review when it changes; residency by deployment per region | 2026-10-13 |
| Data request routes | `privacy: { self_service: { email: } exports: { store: keep: } }`: a generated `/privacy` page with email verification, export by expiring link, erasure after a cooling-off period, events at every step, API routes for the person only, staff actions granted in policy | 2026-10-13 |
| Unreachable personal data | A marked field that belongs to no person is a compile error; every reference into a person's records has an erasure outcome | 2026-10-13 |
| Provider copies | Erasure emits `PersonErased`; every connection receiving marked fields needs a deleting subscriber or an accepted reason | 2026-10-13 |
| Coverage and gaps | The report states what the language covers, what is the app's choice and what is not code; breach-notification and transfer support to add | 2026-10-13 |
