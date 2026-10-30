---
title: Personal data
area: 15
status: decided
checked-with: this checkout's jadpo on every gate run
rules: []
---

# Personal data
<!-- topic: personal-data -->

Your app says which data is about a person, and Jadpo does the rest: it
exports that data when the person asks for a copy, erases it when they ask
to be forgotten, deletes it when its time is up, and reports where it
lives. The aim is that requests under GDPR, UK GDPR and CCPA/CPRA work out
of the box.

The law asks a few things of an app:

- Keep only the personal data you need, for as long as you need it (GDPR
  Article 5).
- Give a person a copy of everything you hold about them, in a
  machine-readable form, within a month (Articles 12, 15 and 20).
- Delete it when they ask, unless the law makes you keep some of it, such
  as invoices (Article 17).
- Build this in from the start (Article 25), keep a record of what you
  hold and why (Article 30), and protect it (Article 32).

CCPA/CPRA gives Californians the same rights to know and to delete. Each of
these needs the app to know which data is personal and whose it is. That
is the one fact Jadpo asks you to write down.

| Capability | What you write | What the compiler does |
|---|---|---|
| Marking | `personal:` on a field | Hashes it in logs, lists it in the report |
| The person | `person: true` on an entity | Finds every record that belongs to them |
| Erasure | `erase:` on references it can't work out | Generates `User.erase(id)` across every store and cache |
| Export | nothing | Generates `User.export(id)` |
| Retention | `retention:` on a field or entity | Clears or deletes it when the time is up |
| Report | nothing | Writes `build/report/personal-data.md` |

## Marking personal fields
<!-- topic: marking-personal-fields -->

A field that identifies or describes a person says so in its block, with
`personal:` and one category from a fixed list. The category decides how
carefully the compiler treats the value. Here a user's email is contact
data and their display name identifies them:

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

Every marked field needs a category: there is no bare `personal: true`, and
you can't add categories of your own. These are the fourteen:

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
extra ones CCPA/CPRA lists: government IDs and precise location.

### Marked fields in logs

A marked field is written to logs as a keyed hash, whatever its type. So a
marked `date_of_birth: CalendarDate` is hashed, though
[logs](04-failures.md#logging) otherwise show dates and numbers as
values, and so is a marked field in a failure's context. The marking, not
the type, decides.

The hash covers the UTF-8 bytes of the value's canonical text, as
`text.from` gives it: `2026-10-25` for a date, `GBP 10.50` for an amount.
A type with no single text form, such as a `PostalAddress`, is hashed from
its JSON as `json.write` writes it. So one value hashes the same in every
log and on both executors, because every boundary goes through
[the standard types' contract](19-standard-library.md#every-standard-type-meets-one-contract).

### Sensitive fields

A sensitive field is never put in a cache, and every read of one is
recorded in the [access records](#access-records).

### Records a person owns

You don't mark every field of a record a person owns. A todo's title is
the user's data because the todo belongs to them (see
[The person](#the-person)), so it is exported and erased with them. Mark
only the fields whose value points at, or describes, a person.

## The person
<!-- topic: person-entities (was the-person) -->

An entity that represents a person says `person: true`. The law calls this
person the *data subject*: the one the data is about. Most apps have one
such entity, `User`. You can have several, so a clinic might also have
`Patient`, and a shop a guest `Customer`.

```jadpo fragment
entity User {
    ...
    person: true
}
```

You don't list which records belong to the person. A record **belongs
to** a person when it refers to them, directly or through a chain of
references: a todo through its `owner_id`, a todo's attachment through the
todo. The compiler follows every reference to find them, and the
[report](#the-personal-data-report) shows the whole tree for you to review.

A record that refers to two people, such as a message from one user to
another, belongs to both.

## Erasure
<!-- topic: erasure -->

Erasing a person is a generated operation, `User.erase(user_id)`. You call
it from an action you write, for example behind a "delete my account"
route or in a support tool. It erases the person's record and decides what
happens to every record that belongs to them.

What happens to each of those records follows from the `on_delete:`
setting of the [reference](05-data.md#references-and-on_delete) that
links it to the person, so most references need nothing more:

- `OnDelete.cascade` means the record is deleted (`Erasure.deleted`).
- `OnDelete.cleared` means the record is kept and anonymised
  (`Erasure.anonymised`).
- `OnDelete.restricted` must say what erasure does, with `erase:`.

A restricted reference must say, because "a user with todos can't be
deleted" is the right rule for a mistaken click, but not for a person's
legal request to be forgotten. Most apps write one or two `erase:` lines,
and the compiler refuses an erasure it can't complete.

| `erase:` on a reference to a person | What happens to the record | Needs |
|---|---|---|
| `Erasure.deleted` | Deleted, with everything that cascades from it | |
| `Erasure.anonymised` | Kept; the reference and every marked field are cleared | the reference and marked fields nullable |
| `Erasure.retained` | Kept as it is until its own retention ends; only the reference is cleared | the reference nullable, and `retention:` on the entity |

A marked field is one with a `personal:` setting, which says it holds
personal data ([Marking personal fields](#marking-personal-fields)). A
nullable field is one whose type ends in `?`, so it can hold no value
([Values that may be missing](02-values-and-types.md#values-that-may-be-missing)).

### Deleting the person's records: `Erasure.deleted`

Use it for records that are only the person's, such as their todos.
Here `Todo`'s reference to its owner is restricted, so ordinary code can't
delete a user who still has todos, but erasing the owner deletes their
todos:

```jadpo fragment
// domains/planning/entities/todo.jadpo
entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        role: TodoRole.owner
        erase: Erasure.deleted
    }
    title: TodoTitle
    ...
}
```

### Keeping a record without the person: `Erasure.retained`

Use it for records the law makes you keep. Invoices are the usual case:
tax law requires them for several years, which GDPR Article 17(3) allows.
The invoice keeps its own copy of the billing name and address, so it no
longer needs the user. Erasing the user clears `customer_id` and keeps the
invoice:

```jadpo fragment
// domains/billing/entities/invoice.jadpo
entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }
    billing_name: PersonName {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}
```

The invoice is then deleted when its own [retention](#retention) ends,
here `config.invoice_retention` after it was issued (six years in the UK).

A record you keep this way holds its own copy of the details it needs,
as the invoice holds the billing name. It never points at a stub left in
place of the person. It also needs a `retention:` of its own, which says
when it is deleted.

### The person's own record

The person's own record is erased field by field, because it can't always
go: an insurer, for example, must keep policy records for at least five
years.

- Where nothing must be kept, the record is deleted.
- Otherwise a field marked `erase: Erasure.retained` stays, which needs
  the entity's `retention:`, and every other marked field is cleared. The
  person can no longer sign in, receives nothing, and is left out of every
  query except those that serve the retained purpose. When the retention
  ends, the purge schedule deletes the rest.

### What erasure reaches

- **Every store and cache.** The main store's records go in one
  transaction, and cached copies are wiped at once
  ([Caches](05-data.md#caches)). An action can write to only one store,
  so records your app keeps in another store are erased straight after,
  by an event [subscriber](12-jobs-and-events.md#subscribers) that Jadpo
  generates.
- **Records with a lifecycle** are removed outright, not soft-deleted.
  Your own code can't do that, but erasure, like purge, is an operation
  the compiler generates, so it can
  ([Lifecycle](05-data.md#lifecycle)).
- **Files** attached to erased records are deleted with them
  ([Files](16-built-in-services.md#files)).
- **OAuth grants**, the permissions a person gave your app to act for them
  at a provider, are deleted with the person or record that owns them
  ([OAuth grants](11-integrations.md#acting-for-each-user-oauth-grants)).
  They are revoked at the provider where its package declares how.
- **Other companies.** Data already sent to a provider, such as an email
  or payment provider, is outside your database. Providers that can erase
  are asked to (see "Erasure at providers" below). The
  [personal-data report](#the-personal-data-report) lists every connection that receives a marked field, and whether its
  copy is erased.

### What erasure doesn't rewrite

- **Backups** aren't rewritten. Each erasure leaves an entry in an erasure
  log, holding the person's `id` and the time and nothing else. Restoring a
  backup replays the log before the app starts. Backups themselves expire
  on the hosting platform's schedule, which the report states. This is
  what the UK regulator, the ICO, calls putting data "beyond use".
- **There are no per-person encryption keys**, so erasure never works by
  deleting a person's key ("crypto-shredding").
- **Logs** aren't rewritten either. They hold identifiers and hashes,
  never marked values, and expire on their own retention.

### Legal holds and restricted use

A legal hold, which stops a person's data being erased while a case is
open, is application data that you write. An action checks a hold flag and
refuses with its own failure before it calls `User.erase`. Restricting use
of someone's data while a complaint is open (Article 18) works the same
way.

### Erasure at providers

When your app sends a person's data to another company, such as a payment
provider, erasing the person also asks that provider to erase them. Two
declarations make that work. The provider's package declares, in its
`provider.jadpo`, which call erases a person and what it takes. Your app's
connection names the field that holds the person's key at that provider:

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

Erasure emits a `PersonErased` event. It is a declaration on the package,
not part of any [capability](11-integrations.md#capabilities), so Stripe
and Postmark are erased the same way.

- **Your app first, then the providers.** When `User.erase` runs, Jadpo
  copies each connection's key, here `stripe_customer_id`, into the
  erasure event. It then erases the person in one transaction, as above.
  After that commits, a generated subscriber calls each provider's erasure
  operation, retried until the provider confirms. So a provider never
  holds the only record of someone your app still has.
- **The completion email waits for every provider.** The
  erasure-completed email, one of the
  [built-in emails](16-built-in-services.md#built-in-emails), goes only
  once every provider has confirmed. A delivery that gives up is parked
  for an operator and shown in the privacy request's state and in
  `jadpo privacy requests`. It is never left quietly incomplete.
- **A copied key that is itself personal data**, such as an email address
  used as the provider's key, stays only in the outbox until it is
  delivered. The personal-data report lists it there.
- **A connection that receives personal data but can't erase it** is a
  compile error. That is a connection whose package declares no
  `erasure:`, or which names no key. An
  [acceptance](18-records-and-manifest.md#acceptances), a committed file
  in which a person states why the data stays there (tax records at an
  accounting provider, say), allows it, and the report lists the
  connection and the reason.
- **Anything else** your app sends data to, such as a CRM it calls
  through its own integration, can be erased by your own `PersonErased`
  subscribers.

## Export
<!-- topic: personal-data-export (was export) -->

`User.export(user_id)` returns everything held about the person: their
record and every record that belongs to them, as JSON, one list per
entity, with the same field names as the API. You write nothing for it
beyond calling it:

```jadpo fragment
// domains/accounts/actions/export_my_data.jadpo
/// Gives the signed-in user a copy of everything held about them.
/// The route passes the caller in: `run: export_my_data(current_principal.id)`.
action export_my_data(user_id: User.id) -> PersonalDataExport {
    return User.export(user_id)
}
```

The export covers both the right of access and the right to take your
data elsewhere ("portability", Article 20). Credentials, such as token
hashes, are left out. It also says how long each kind of data is kept and
which companies receive it, taken from the report, because Article 15
asks for that too.

### Who may erase and export

[Policy](09-policy.md) decides who may call `erase` and `export`.
`export` needs read access to the person's record, and `erase` needs
delete access ([Grants](09-policy.md#grants)). Once a caller is allowed,
both reach every record that belongs to the person, as purge does,
because a partial export or erasure is worse than none.

## Retention
<!-- topic: retention -->

Personal data needs an end date: GDPR's storage limitation means you keep
it only as long as you need it. A `retention:` block gives one, in the
same `after:` and `since:` shape as a lifecycle's `purge:`
([Lifecycle](05-data.md#lifecycle)).

### On a field

The value is cleared when the time is up, so the field must be nullable.
Use it for a value you need only for a while, such as an IP address kept
for 30 days for fraud checks:

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

### On an entity

The record is deleted, with whatever cascades from it. Use it for
invoices, as in [Erasure](#keeping-a-record-without-the-person-erasureretained),
or for accounts inactive for two years.

### Where the periods come from

The periods are settings in `config.jadpo`
([Settings](10-configuration-and-time.md#settings-configjadpo)), so they
are set per environment and appear in the report. The compiler's built-in
[purge schedule](12-jobs-and-events.md#the-purge-schedule) applies
retention too, in small batches, each logged.

A person entity, or an entity holding marked fields, with no retention
and no purge is listed in the report as "kept until erased".

## What the compiler checks
<!-- topic: personal-data-checks (was what-the-compiler-checks) -->

- **Marked fields nothing reads.** A personal field that no query, output
  or action reads is an error, because you are holding data you don't use
  (data minimisation, Article 5). You can
  [decline](01-project-layout-and-modules.md#declining-a-judgement-call)
  it, with a reason, because a field kept for a legal reason may be read
  only by export.
- **Erasure is complete.** Every reference into a person's records has an
  erasure outcome. `Erasure.anonymised` and `Erasure.retained` have the
  nullable fields and retention they need.
- **No personal data erasure can't reach.** A marked field on an entity
  that isn't a person, and doesn't belong to one through references, is a
  compile error: "erasure cannot find this: reference the person, or mark
  the entity `person: true`". With the completeness check above, every
  stored copy of personal data can be reached.
- **Sensitive fields stay out of caches.** A cache on an entity that holds
  one leaves those fields out.

There is no `purpose:` setting and no consent model in the language.
Purposes and lawful bases belong in your privacy notice, which you write
from the report. Consent is ordinary application data, such as a `Consent`
entity holding the purpose and when it was given and withdrawn.

## Data requests
<!-- topic: data-requests -->

`privacy:` in `app.jadpo` turns on generated routes for a person's
requests to export or erase their data. Jadpo generates the endpoints
only, never the pages: your frontend builds the page, and a recipe can
show you one. There are three ways a request starts.

### Self-service by email

`privacy: { self_service: {} }` turns it on. Anyone can ask by email
address, without signing in:

1. Your frontend posts the address to `POST /privacy/requests`
   (`address`). It answers `202` for every address, so it never reveals
   who has an account, and sends an email to check the address.
2. The email's single-use link opens your frontend's path, set as
   `self_service: { link: "/privacy/confirm" }`, at the browser origin,
   with a token.
3. That page posts `POST /privacy/requests/confirm` (`token`, and `kind`:
   `export` or `erasure`) to create the request.
4. The erasure confirmation email has a cancel link, which opens
   `self_service: { cancel_link: "/privacy/cancel" }`. That page posts
   `POST /privacy/requests/cancel` (`token`).

Loading a frontend page uses up nothing, so a mail scanner that follows
the links confirms and cancels nothing.

### Signed in

`POST /privacy/export` and `POST /privacy/erasure` need a recent sign-in
([`RouteAuthentication.fresh`](08-identity.md#check-the-caller-again-routeauthenticationfresh))
and act only on the caller's own record: there is no person id to pass.
Export answers `202` and delivers a download link when it is ready.

### Made by staff

Requests received by email or phone are made through generated actions,
after staff have checked who the person is. You grant them in
`policy.jadpo`, as `privacy.create_request: [StaffRole.support]`. Each
request records who created it and how the person's identity was
checked. The export still goes to the person's own email, never to the
staff member.

A schedule or subscriber never makes a request. The only erasure that
happens on a schedule is retention's.

### Exports

An export is delivered as a single-use download link that expires after 7
days, never as an attachment. The file is encrypted at rest and deleted
when the link expires, and every download is in the access records. You
set where the files are kept in `privacy: { exports: { store: keep: } }`.

### Erasures

An erasure waits out a cooling-off period your app sets, 14 days by
default, and the confirmation email has a cancel link. The completion
email says what was kept and why, since the person must be told. So an
entity marked `Erasure.retained` needs a wording in its domain's language
files, which the email lists.

### Deadlines and events

Every request is a record with its one-month legal deadline.
`jadpo privacy requests` and the console list them, and one nearing its
deadline raises an alert.

Each step emits an event: `PrivacyRequestReceived`, `PrivacyExportReady`
(with the download link), `PrivacyErasureScheduled`,
`PrivacyErasureCompleted` and `PrivacyRequestOverdue`. The built-in
emails are subscribers to these events. Your app can add its own, to
post to Slack or a CRM, say, or replace the emails. With no mail
connection, the compiler requires a subscriber that delivers
`PrivacyExportReady`.

### Protection

The routes have [rate limits](16-built-in-services.md#rate-limits), their
links are single-use and short-lived, and every step is in the access
records.

### Changing the routes

These routes take `path:` and `routes: false` like every
[generated route](07-http.md#generated-routes). They run over generated
`privacy.request`, `privacy.confirm` and `privacy.cancel` operations,
which keep every protection.

### The privacy emails

The privacy emails are the address check, export ready, erasure confirmed
and erasure completed. They are [built-in emails](16-built-in-services.md#built-in-emails),
which you can change as much as you need, in five levels:

1. Jadpo's own email, as it comes.
2. Its words, changed in the root `translations/` folder under
   `jadpo: { }` ([Translations](10-configuration-and-time.md#jadpos-and-packages-text)).
3. Your app's shared email layout.
4. Your app's own `EmailMessage`, with `email:`.
5. Your app's own sending, with `send:`.

They go through your app's one `email:` connection in `application { }`,
and need `application { display_name: }`.

Each email has a built-in type, and a request type for `send:`:
`PrivacyAddressCheckEmail`, `PrivacyExportEmail`,
`PrivacyErasureConfirmedEmail` and `PrivacyErasureCompletedEmail`. You set
them in `privacy: { emails: { address_check: { } export_ready: { }
erasure_confirmed: { } erasure_completed: { } } }`. Each entry takes
`email:` or `send:`, never both. Their fields are on the
[Built-in services](16-built-in-services.md#built-in-emails) page.

## Access records
<!-- topic: access-records -->

Every export and erasure is recorded: who asked, for which person's `id`,
and when, never the values. So is every read of a sensitive field. These
are log events kept for a long time, so your organisation can show a
regulator what it did.

Reads of ordinary personal data aren't recorded one by one.

## Where data is stored
<!-- topic: where-data-is-stored -->

Each store is one database in one place. Data that must stay in the EU
lives in a deployment whose stores are in the EU. An app serving several
regions runs one deployment per region ([Stores](05-data.md#stores)).

There is no region setting in source. Instead, the report lists each
store with the configuration setting that says where it is.

## The personal-data report
<!-- topic: the-personal-data-report -->

`jadpo check` writes `build/report/personal-data.md` and a `.json` twin,
in the same form as its other reports ([The report](14-tooling.md#the-report)).
For each person entity it lists:

- every marked field, with its category, entity, store and caches;
- who may read it, from policy;
- its retention, or "kept until erased";
- what erasure does to each record that belongs to the person;
- every connection a marked field is sent to, and whether its copy is
  erased by the provider's `erasure:` or kept for an accepted reason;
- keys copied into the outbox for erasure at providers, while they wait;
- the step records of actions with
  [several changing calls](12-jobs-and-events.md#several-changing-calls-in-one-action),
  which hold providers' answers, encrypted, until the action commits (an
  answer over 1 MB is a fault);
- connections holding OAuth grants, and whose grants they are;
- sensitive fields, flagged.

That is most of a record of processing activities (Article 30). Your team
adds what the compiler can't know: the purpose of each use, the lawful
basis, and the organisation's details.

Changes to the report appear in the approval review, the summary of a
change a person signs off before it ships
([Review and sign-off](14-tooling.md#review-and-sign-off)). So a pull
request that starts sending email addresses to a new provider is seen by
a person before it ships.

## Encrypted fields
<!-- topic: encrypted-fields -->

An entity field can be stored encrypted:
`notes: ClinicalNote { encrypted: true }`. It is sealed with your app's
one `field_encryption_key:` and decrypted when it is loaded. An encrypted
field is kept out of unique rules, indexes, filters, ordering, search and
logs.

There are no per-person keys (see [Erasure](#what-erasure-doesnt-rewrite)).
Keys can also be held as data: a key per device or person in an
encrypted field, usable only as a `crypto` call's `key:`
([Hashing and cryptography](19-standard-library.md#hashing-and-cryptography)).

Every field sent to an AI model is listed in the report above
([Following personal data](21-ai.md#following-personal-data)).

## What this does not cover
<!-- topic: what-this-does-not-cover -->

The report says plainly what Jadpo covers and what it doesn't:

- **Covered by the language:** minimisation and storage limitation,
  access, rectification (by ordinary updates), erasure, portability,
  privacy by design, records of processing (most of Article 30), and
  security of processing.
- **Supported, but your app's choice:** restriction and objection, and
  consent.
- **Not code at all:** the lawful basis, the privacy notice, data
  protection impact assessments (DPIAs), processor contracts, and the data
  protection officer (DPO).
- **Still to add:** breach notification, which lists the people whose
  marked data a breached store or provider held, from the report and
  access records; and international transfers, where provider packages
  declare where data is processed and the report shows it.

This is not legal advice. A privacy professional reviews the report.

## What is built
<!-- topic: personal-data-built -->

| Feature | Status |
|---|---|
| An entity's and an event's `personal: PersonalData.<category>` | Built: it is read and checked against the fourteen categories, for the [AI](21-ai.md) page's check of what reaches a model. |
| Everything else on this page | Not built yet: it is on [the roadmap](../roadmap.md). |
