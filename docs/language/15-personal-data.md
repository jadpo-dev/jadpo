---
title: Personal data
area: 15
status: decided
checked-with: this checkout's jadpo on every gate run
rules: []
---

# Personal data

How a Jadpo application says which data is about a person, so the compiler
can export it, erase it, expire it and report where it lives. The aim is
that GDPR, UK GDPR and CCPA/CPRA requests work out of the box.

> Not built yet, except the reading of `personal:` that area 21 uses; see [the roadmap](../roadmap.md).

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
one category from a fixed list. The category decides how carefully the
compiler treats the value; the list follows GDPR's special categories and
the common groups used by privacy tools such as Fides. A bare
`personal: true` cannot tell health data from an email address, and there
are no custom categories, so reports stay comparable and the
sensitive-data rules exact.

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
  type, so a marked `date_of_birth: CalendarDate` is hashed too, and so is
  a marked field in failure context. This refines area 4, which logs dates
  and numbers as values: the marking is the classification. The
  hash covers the UTF-8 bytes of the value's canonical text (`text.from`,
  `2026-10-25` for a date, `GBP 10.50` for an amount), or of its contract
  JSON as `json.write` writes it for a type with no single text form (a
  `PostalAddress`), so one value hashes the same in every log and on both
  executors, as every boundary goes through area 19's contract.
- **Sensitive fields** are never put in a cache, and every read of one is
  recorded.
- **Records a person owns** need no marking on every field. A todo's title is
  the user's data because the todo belongs to them (next section), so it is
  exported and erased with them. Marking is for the fields whose value points
  at, or describes, a person.

## The person

The law calls the person data is about the *data subject*. An entity that
represents one says `person: true`. Most applications have one, `User`;
several are allowed, so a clinic might also have `Patient`, a shop a guest
`Customer`.

```jadpo fragment
entity User {
    ...
    person: true
}
```

A record **belongs to** a person when it refers to them, directly or through
a chain of references: a todo through `owner_id`, a todo's attachment through
the todo. The compiler follows every reference, so nothing has to be listed
twice (tagging each owning reference would repeat what references already
say), and the report shows the whole tree for review. A record that refers
to two people, such as a message from one user to another, belongs to both.

## Erasure

Erasing a person is a generated operation, `User.erase(user_id)`, called from
an action you write (for a "delete my account" route, or a support tool).
It erases the person's record and decides what happens to every record that
belongs to them:

| `erase:` on a reference to a person | What happens to the record | Needs |
|---|---|---|
| `Erasure.deleted` | Deleted, with everything that cascades from it | |
| `Erasure.anonymised` | Kept; the reference and every marked field are cleared | the reference and marked fields nullable |
| `Erasure.retained` | Kept as it is until its own retention ends; only the reference is cleared | the reference nullable, and `retention:` on the entity |

Most references need nothing written: `OnDelete.cascade` means
`Erasure.deleted` and `OnDelete.cleared` means `Erasure.anonymised`. A
reference that is `OnDelete.restricted` must say `erase:`, because "a user
with todos cannot be deleted" is the right rule for a mistaken click but the
wrong one for a legal request. Most apps write one or two `erase:` lines,
and the compiler refuses an erasure it cannot complete.

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

**The person's own record** is erased field by field, because it cannot
always go: an insurer must keep policy records for at least five years.
Where nothing must be kept, the record is deleted. Otherwise a field
marked `erase: Erasure.retained` (with the entity's `retention:`) stays and
every other marked field is cleared; the person can no longer sign in,
receives nothing, and is excluded from every query except those that serve
the retained purpose. When the retention ends the purge schedule deletes
the rest. A kept record of another entity keeps its own copy of the
details it needs, as the invoice does, never a stub of the person, and
must have its own retention.

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
  the UK regulator (ICO) calls putting data "beyond use". There are no
  per-person encryption keys: deleting a person's key ("crypto-shredding")
  breaks unique lookups, indexes and search on encrypted fields, and no
  regulator has said key deletion counts as erasure.
- **Logs** are not rewritten either. They hold identifiers and hashes, never
  marked values, and expire on their own retention.
- **Other companies.** Data already sent to a provider (an email provider,
  a payment provider) is outside the database. Providers that can erase
  are asked to, as "Erasure at providers" says; the report lists every
  connection that receives a marked field and whether its copy is erased.
- **Files** (area 16) attached to erased records are deleted with them.
- **OAuth grants** a person or record owns (area 11) are deleted with it,
  and revoked at the provider where the package declares how.
- **Legal holds** are application data: an action that checks a hold flag
  and refuses with its own failure before calling `User.erase`. The same
  goes for restricting use while a complaint is open (Article 18). Both are
  rare in the apps Jadpo targets, and a built-in one would also have to
  pause purge and retention.

### Erasure at providers

A provider package declares how its provider erases a person, in its
`provider.jadpo`; the app's connection names the field holding that
person's key there. Erasure emits a `PersonErased` event.

```jadpo fragment
// packages/stripe/v2024_06_20/provider.jadpo
provider {
    ...
    erasure: {
        call: customers.delete
        by: CustomerId
    }
}

// app.jadpo
stripe: {
    package: Stripe.v2024_06_20
    domains: [payments]
    credential: config.stripe_secret_key
    erasure: {
        key: User.stripe_customer_id
    }
}
```

- **The order is fixed.** When `User.erase` runs, Jadpo copies each
  connection's key (`stripe_customer_id`) into the erasure event, then
  erases the person in one transaction as above; after it commits, a
  generated subscriber calls each provider's erasure operation, retried
  until the provider confirms. The app first, then the providers, so a
  provider never holds the only record of someone the app still has.
- **The completion email waits for every provider.** The erasure-completed
  email (area 16's built-in emails) goes only once every provider has
  confirmed. A delivery that gives up is parked for an operator and shown
  in the privacy request's state and in `jadpo privacy requests`, never
  left quietly incomplete.
- **A copied key that is itself personal data** (an email address used as
  the provider's key) stays only in the outbox until it is delivered, and
  the personal-data report lists it there.
- **A connection that receives personal data but cannot erase it** (its
  package declares no `erasure:`, or the connection names no key) is a
  compile error, unless an acceptance (area 7's form) states why the data
  stays there (tax records at an accounting provider); the report lists
  the connection and the reason.
- The app may add its own `PersonErased` subscribers for anything else
  (a CRM it calls through its own integration).

This is a declaration on the package, not part of any capability (area
11), so Stripe and Postmark are erased the same way.

## Export

`User.export(user_id)` returns everything held about the person: their
record and every record that belongs to them, as JSON, one list per entity,
with the same field names as the API. That covers both the
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
purge, gives one:

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
  action reads is a declinable error (area 1): you are holding data you do
  not use (data minimisation, Article 5). It is declinable because a field
  kept for a legal reason may be read only by export.
- **Erasure is complete.** Every reference into a person's tree has an
  erasure outcome; `Erasure.anonymised` and `Erasure.retained` have the
  nullable fields and retention they need.
- **No personal data erasure cannot reach.** A marked field on an entity
  that is not a person and does not belong to one through references is a
  compile error ("erasure cannot find this: reference the person, or mark
  the entity `person: true`"). With the completeness check above, every
  stored copy of personal data is reachable.
- **Sensitive fields stay out of caches**: a cache on an entity that holds
  one leaves those fields out.

There is no `purpose:` setting and no consent model, because the compiler
can check neither. Purposes and lawful bases belong in the privacy notice,
which the team writes from the report; consent is ordinary application
data, such as a `Consent` entity with the purpose and the time it was given
and withdrawn.

## Data requests

`privacy:` in `app.jadpo` switches on generated routes for a person's
requests to export or erase their data. Jadpo generates endpoints only,
never pages: the app's frontend builds the page, and a recipe can show one.

- **Self-service by email.** `privacy: { self_service: {} }` turns it on.
  `POST /privacy/requests` (`address`) answers `202` for every address, so
  it never reveals who has an account, and sends the address-check email.
  Its single-use link opens the frontend path
  `self_service: { link: "/privacy/confirm" }` at the browser origin with a
  token; the frontend posts `POST /privacy/requests/confirm` (`token`,
  `kind`: `export` or `erasure`) to create the request. The erasure
  confirmation's cancel link opens
  `self_service: { cancel_link: "/privacy/cancel" }`, which posts
  `POST /privacy/requests/cancel` (`token`). Loading a frontend page
  consumes nothing, so mail scanners confirm and cancel nothing.
- **Signed in.** `POST /privacy/export` and `POST /privacy/erasure` need a
  recent sign-in (`RouteAuthentication.fresh`) and act only on the
  caller's own record: there is no person id to pass. Export answers `202`
  and delivers a download link when it is ready.
- **Staff.** Requests received by email or phone are created through
  generated actions, granted in `policy.jadpo`
  (`privacy.create_request: [StaffRole.support]`), after staff verify the
  person. Each records who created it and how identity was checked, and
  the export still goes to the person's own email, never to the staff
  member. A schedule or subscriber never makes a request; only retention
  purges erase on a schedule.
- **Export** is delivered as a single-use download link that expires after
  7 days, never as an attachment; the file is encrypted at rest, deleted on
  expiry, and every download is in the access records. Where the files
  are kept is set in `privacy: { exports: { store: keep: } }`.
- **Erasure** waits out a cooling-off period the app sets (default 14
  days), with a cancel link in the confirmation email. The completion
  email says what was kept and why, since the person must be told: an
  entity marked `Erasure.retained` needs a wording in its domain's language
  files, which the email lists.
- **Deadlines.** Every request is a record with its one-month legal
  deadline; `jadpo privacy requests` and the console list them, and one
  nearing its deadline raises an alert.
- **Events** at every step: `PrivacyRequestReceived`, `PrivacyExportReady`
  (with the download link), `PrivacyErasureScheduled`,
  `PrivacyErasureCompleted`, `PrivacyRequestOverdue`. The built-in emails
  are subscribers to these; an app can add its own (Slack, a CRM) or
  replace the emails. With no mail connection, the compiler requires a
  subscriber that delivers `PrivacyExportReady`.
- **Protection:** rate limits on the routes (area 16), links single-use and
  short-lived, every step in the access records.

These routes take `path:` and `routes: false` like every generated route
(area 7), over generated `privacy.request`, `privacy.confirm` and
`privacy.cancel` operations that keep every protection.

The privacy emails (the address check, export ready, erasure confirmed,
erasure completed) are built-in emails and change at area 16's five
levels ("Built-in emails"): Jadpo's own email, its words in the root
`translations/` under `jadpo: { }` (area 10), the app's shared layout, the
app's own `EmailMessage` (`email:`), or the app's own sending (`send:`).
They go through the app's one `email:` connection in `application { }`,
and need `application { display_name: }`. Each has a built-in type and a
request type for `send:`: `PrivacyAddressCheckEmail`, `PrivacyExportEmail`,
`PrivacyErasureConfirmedEmail` and `PrivacyErasureCompletedEmail`, set in
`privacy: { emails: { address_check: { } export_ready: { }
erasure_confirmed: { } erasure_completed: { } } }`, each entry taking
`email:` or `send:`, never both; their fields are in area 16.

## Access records

Every export and erasure is recorded: who asked, for which person `id`, and
when, never the values. So is every read of a sensitive field. These are
log events with a long retention, so an organisation can show a regulator
what it did. Reads of ordinary personal data are not recorded one by one;
GDPR asks for accountability and security, not a log of every read, and
that would log almost every request.

## Where data is stored

Each store is one database in one place. Data that must stay in the EU lives
in a deployment whose stores are in the EU; an application serving several
regions runs one deployment per region (area 5). There is no region setting
in source, since one the compiler cannot verify would give false comfort;
the report lists each store with the configuration setting that says where
it is.

## The personal-data report

`jadpo check` writes `build/report/personal-data.md` and a `.json` twin
(area 14's report form). It lists, for each person entity:

- every marked field, its category, entity, store and caches;
- who may read it, from policy;
- its retention, or "kept until erased";
- what erasure does to each record that belongs to the person;
- every connection a marked field is sent to, and whether its copy is
  erased by the provider's `erasure:` or kept for an accepted reason;
- keys copied into the outbox for erasure at providers, while they wait;
- the step records of actions with several changing calls (area 12),
  which hold providers' answers encrypted, a fault over 1 MB each, until the
  action commits;
- connections holding OAuth grants, and whose grants they are;
- sensitive fields, flagged.

That is most of a record of processing activities (Article 30). The team adds
what the compiler cannot know: the purpose of each use, the lawful basis, and
the organisation's details. Changes to the report appear in the approval
review (area 9), so a pull request that starts sending email addresses to a
new provider is seen by a person before it ships.

## Encrypted fields

An entity field may be stored encrypted,
`notes: ClinicalNote { encrypted: true }` (area 19), sealed with the application's one
`field_encryption_key:`, decrypted on load, and kept out of unique rules,
indexes, filters, ordering, search and logs. There are no per-person keys
(see Erasure). The full design is this area's, built with personal data,
together with keys held as data (a key per device or person in an
encrypted field, usable only as a `crypto` call's `key:`). Every field sent
to an AI model is listed in the report above (area 21).

## What this does not cover

The report says so plainly. Covered by the language: minimisation and
storage limitation, access, rectification (ordinary updates), erasure,
portability, privacy by design, records of processing (most of Article
30), security of processing. Supported but the application's choice:
restriction and objection, consent. Not code at all: lawful basis, privacy
notice, DPIAs, processor contracts, the DPO. Still to add: breach
notification (from the report and access records, list the people whose
marked data a breached store or provider held) and international transfers
(provider packages declare where data is processed, shown in the report).
This is not legal advice; a privacy professional reviews the report.

## What is built

Nothing on this page is built yet, but for one reading: an entity's and
an event's `personal: PersonalData.<category>` is read and checked against
the fourteen categories, for area 21's check of what reaches a model. The
rest is on [the roadmap](../roadmap.md).
