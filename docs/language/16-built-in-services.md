---
title: Built-in services
area: 16
status: decided
checked-with: jadpo 3ce094d0 (2026-10-12)
rules: []
---

# Built-in services

Which common backend features Jadpo should provide itself, rather than
leaving each application to install a library or write its own: feature
flags, rate limits, files, audit trails, tracing, email and search.

> Decided on 2026-10-13. Nothing on this page is built yet; the [status table](#status) says when each part comes. Examples apply the decisions from areas 1 to 16.

**The test for "native".** A feature is built in only when the compiler
gains something by knowing about it: a mistake becomes a compile error, a
test or a report is generated, policy or erasure reaches it, or the runtime
does the work with nothing for the author to write. Saving a library install
is not enough. Where the language can reasonably own a feature it does, rather
than depending on a third-party service (the rule area 8 applied to OAuth).

| Feature | Native? | Why | Where it lives |
|---|---|---|---|
| Feature flags | Yes | Misspelt flags are errors; both branches tested; expired flags reported; one-step removal | Root `flags.jadpo`; read as `flags.name` |
| Rate limits | Yes | The route knows the caller; 429 is already a failure kind; limits appear in OpenAPI | `rate_limit:` on an API or route |
| Files | Yes | Size and type checked; policy and erasure reach the bytes; signed links generated | `File` fields; a `files` connection in `app.jadpo` (area 11 capabilities) |
| Audit trail | Yes | The compiler sees every write and its caller; personal fields hashed | `audit:` section on an entity |
| Tracing and metrics | Yes, no syntax | Every route, query, action, job and integration call is known | Generated runtime |
| Path IDs and monitoring thresholds | Proposed | Every path is known, so it can be named, sampled and given limits | Generated runtime; a monitoring file per domain; questions 13–16 |
| Email | Yes, through the `email` capability | Templates typed against translations; delivered after commit; any provider without code changes | `type X = EmailMessage` in `emails/`; an `email` connection in `app.jadpo` |
| Search | Yes | Index, query and migration generated from one declaration | `search:` section on an entity |
| Outgoing webhooks | Yes | Payloads typed from events; signing and retries generated | Later area; question 8 |
| Field encryption | Yes | Encrypted fields kept out of logs, indexes and filters | Area 15; question 8 |
| Realtime, admin screens, payments, CMS, analytics, quotas | No, or later | The compiler gains little, or the feature is not a backend's | Not built in |

## Feature flags

A feature flag lets a team ship code switched off, then turn it on for some
users, a share of users, or everyone, without a deploy. Every flag is
declared once, in a new root file `flags.jadpo`, with its type, default,
owner and expiry (question 1). Its description is the `///` comment above
it, as for every declaration.

```jadpo fragment
// flags.jadpo
flags {
    /// The one-page checkout replacing the three-step form.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2027-01-31"
    }

    /// Which layout the checkout uses while we compare two.
    checkout_layout: CheckoutLayout {
        default: CheckoutLayout.classic
        owner: "payments"
        expires: "2026-12-15"
    }
}
```

A flag is a `Bool`, or an `Enum` for more than two variants. Code reads it as
`flags.new_checkout`, in actions and in a route's `run:` arguments; functions
and queries take the value as a parameter. As with `clock.now`, the value is
fixed for the whole operation, so one request never sees a flag change
halfway through.

```jadpo fragment
// domains/ordering/actions/place_order.jadpo
action place_order(user_id: User.id, input: PlaceOrder) -> OrderView {
    if (flags.new_checkout) {
        return place_order_in_one_step(user_id, input)
    }
    ...
}
```

**Changing a flag.** A flag's current state lives in the main store, in a
table the language owns, and is changed at runtime with `jadpo flags`
against an environment (area 14's operator commands, question 2). Each
change is logged with who made it and applies within ten seconds. If the
database cannot be read, each instance keeps the last values it had, or the
defaults; reading a flag never fails a request.

```text
jadpo flags set new_checkout --env production --users 0192... --percent 10
jadpo flags set checkout_layout --env production --split classic=50,compact=50
jadpo flags list --env production
```

**Who gets what.** The first rule that matches wins (question 3):

1. Named users get a fixed value.
2. Named tenants get a fixed value, once area 9 decides tenancy.
3. A percentage of users, chosen by a stable hash of the flag's name and the
   user's `id`: the same user always gets the same answer, raising 10% to
   20% keeps the first 10% in, and each flag picks a different 10%.
4. Everyone else gets the flag's current value, at first its `default`.

The caller comes from the runtime, as for policy. Schedules and subscribers
have no caller (area 12), so they get the everyone value; an action that
needs a user's variant later puts it in the event it emits.

**What the compiler checks** (question 4):

- `flags.new_chekout` is a compile error, like any unknown name.
- Every flag read needs tests with each value: a test chooses with
  `flags: { new_checkout: true }` beside `clock:` and `config:` (area 13), and
  `jadpo check` fails until tests reaching each read cover both `true` and
  `false`, or every variant.
- A flag past its `expires:` date is a warning naming its owner. Extending it
  means editing the date, so keeping a flag is a reviewed choice.
- Whether a flag has sat at 100% or 0% is runtime state the compiler cannot
  see, so `jadpo flags list` marks those flags "ready to remove".

**Removing a flag.** `jadpo flags remove new_checkout --keep true` rewrites
the source: each read becomes `true`, the dead branch goes, the declaration
goes, and test overrides for the removed value are deleted. Uber's Piranha
does this for Java and Swift; Jadpo can do it exactly because every read is
known.

**Flags in browsers and apps.** A flag with `client: true` is also returned,
for the caller, by a generated `GET /flags` route in the OpenAPI document
(question 5).

Flags are for releasing code. What a customer's plan includes is application
data with policy, and a setting that differs per environment is
configuration (area 10).

## Rate limits

A rate limit caps how many requests a caller may make in a period. It is set
on an API in `app.jadpo` (the default for its routes) or on one route
(question 6), and refused requests get the built-in `rate_limited` failure:
429, with a `Retry-After` header.

```jadpo fragment
// domains/sales/routes/demo_requests.jadpo
POST / {
    authentication: RouteAuthentication.none
    rate_limit: {
        requests: 5
        within: 1h
        by: RateLimitKey.ip
    }
    input: RequestDemo
    status: SuccessStatus.no_content
    run: request_demo(input)
}
```

- `by:` is `RateLimitKey.caller` (the default: the signed-in principal, or
  the address when there is none) or `RateLimitKey.ip`.
- Counters live in the Redis store named by `rate_limits: { store: cache }`
  in `application { }`. If it cannot be reached, each instance counts in its
  own memory, so limits stay roughly right and no request fails, the same
  rule as caches (area 5). With no Redis store, counting is in memory.
- Area 8's generated sign-in, sign-up and password routes get a limit by
  address without anything written.
- The limit appears in the route's OpenAPI entry and in `jadpo routes`.

## Files

A file is a field of type `File` on an entity. The bytes live in a
connection that provides the `files` capability (area 11): the built-in S3
driver for any S3-compatible service, or a package that is a preset over it
(Cloudflare R2, Amazon S3). The record holds the file's key, name, size and
type (question 7, revised 2026-10-13).

```jadpo fragment
// app.jadpo
application {
    connections: {
        avatars: {
            package: CloudflareR2.v1
            account_id: config.cloudflare_account
            bucket: "avatars"
            credential: config.r2_credential
        }
    }
}

// domains/accounts/entities/user.jadpo
entity User {
    ...
    avatar: File? {
        store: avatars
        max_size: 5mb
        types: [FileType.png, FileType.jpeg, FileType.webp]
    }
}
```

- `store:` names the connection, as for entities and caches. Two buckets
  are two connections.
- In `jadpo dev` and tests, files go to local disk with locally signed
  links, so nothing leaves the machine; the local driver does not exist in
  production builds.
- A file the application makes itself (a generated PDF) is written with
  `attempt avatars.put(bytes, FileType.pdf)`, which returns a `File` to
  store in a field. It is the action's one provider call, made before its
  writes (area 11 question 7); bytes never attached are deleted after a day,
  like abandoned uploads.

**Uploading.** Files go straight from the client to the store, not through
the application, so the 1 MB body limit (area 7) is not in the way:

1. An action calls the generated `User.avatar_upload()`, which returns an
   `UploadSlot`: a signed address valid for 15 minutes that accepts only
   `max_size` bytes.
2. The client sends the file to that address.
3. The client sends the slot's token in an ordinary create or update, in an
   `Input` field of type `FileUpload`. Before the write commits, the runtime
   checks the file is there, its size, and its type from its first bytes
   rather than what the client claimed. A mismatch is `invalid_value`.

Asking for a slot needs `create` or `update` on the entity (area 9).
Uploads never attached are deleted after a day.

**Downloading.** An `Output` that picks a `File` field sends a signed link
valid for five minutes, with name, size and type. The read has already passed
policy, including field restrictions, so there is no download operation to
protect separately.

**Deleting and erasing.** Deleting the record, or clearing the field,
deletes the bytes after commit through a generated subscriber, because an
action writes one store (area 5). A `File` field can be marked `personal:`,
erasure and retention reach it like any field (area 15), and
`User.export` includes it as a link.

## Audit trail

An entity with an `audit:` section keeps a history of every change to it:
who made it, when, through which action and request, and each changed
field's value before and after (question 9).

```jadpo fragment
// domains/billing/entities/invoice.jadpo
entity Invoice {
    ...
    audit: {
        retention: {
            after: config.audit_retention
        }
    }
}
```

- Entries are written in the same transaction as the change, in the same
  store, so a change cannot happen without its entry.
- "Who" is the principal; for schedules and subscribers it is the
  application, with the request that emitted the event, so a change made
  later is traced back to the click that caused it.
- Fields marked `personal:` are stored as keyed hashes, the same as logs
  (area 15): the history shows that an email changed, not what it was.
  Server-only fields are left out.
- `Invoice.history(invoice_id)` is a generated query returning a page of
  entries. It needs `read` on the record, and fields the caller cannot read
  are left out.
- Entries are deleted when their retention ends, and with their record on
  erasure. The permissions report lists which entities are audited.

This is a record of changes to data. Who exported or erased a person, and
who read sensitive fields, is area 15's access record, kept as log events.

## Tracing and metrics

The generated runtime reports what it does as OpenTelemetry traces and
metrics, the open standard most monitoring tools accept, with nothing
written in source (question 10).

- **Traces.** One span per route, action, query, integration call, cache
  read, schedule run and event delivery, named after the declaration
  (`POST /todos`, `create_todo`, `todos_for_owner`). Incoming and outgoing
  `traceparent` headers are honoured, and a trace continues from the action
  that emitted an event into its subscriber.
- **Metrics.** Requests and durations per route, failures per code, query
  durations, integration calls and retries, cache hits, rate-limit refusals,
  queue size and the age of the oldest waiting delivery.
- **Values never.** Spans carry names, identifiers and failure codes, never
  field values, following area 4's log rule; marked personal fields are
  hashed.
- **Where they go.** The runtime reads the standard
  `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_SERVICE_NAME` variables; with none
  set it sends nothing. Application code cannot read them.

There are no custom spans: every action and query is already one.

## Path IDs

Proposed 2026-10-13 (question 13). Every request records the path it took
through the application: the route, each action and query, and each branch
taken inside them. The compiler knows every possible path, so a path can be
named by a number rather than described:

- **`path`** is a number that decodes against that build's graph, using
  Ball–Larus path numbering (each acyclic path through a callable gets a
  unique small integer, with minimal instrumentation). `jadpo path <build>
  <path>` draws the route from the number alone, with no recorded data.
- **`path_key`** is a hash of the stable identities on the path, so the
  same path can be compared across builds ("p95 of this path since
  Tuesday's deploy"). An edit inside a declaration changes its key, which
  is the truth: the path changed.
- **Loops and recursion.** A `for` body records which inner paths ran, not
  how many times; a recursive call records the path of its first entry.

Both go on the root span and on every failure and fault event. A path ID
says which way a request went, not how long it took or what it carried:
timings, the request and the response come from the trace (area 17).

Two failures at the same line reached from different routes keep different
paths, and a path shows the branches that ran and finished before the
failure, which a stack no longer holds.

Path IDs need control flow inside each callable in the compiler's graph.

## Monitoring thresholds

Proposed 2026-10-13 (questions 14–16). Thresholds are declared in the
application, reviewed with it and versioned with each build, not configured
in a monitoring tool. There are two kinds, because only one can be judged
by a single process.

**Per request** (duration, query count, failure). The runtime keeps each
request's spans in memory until it ends, then sends:

- the full trace when a threshold was crossed, the request failed or
  faulted, or its path is new to this process;
- otherwise metrics only, plus one sample trace per path per minute, so
  there is always a normal request to compare a bad one with.

A process crash loses its unsent requests; long jobs send on a timeout.
This keeps volume low without losing anything structural: the compiler
knows every path, so per-path metrics cannot grow without bound.

**Across requests** (p95 over a window, error rate, oldest waiting
delivery). One instance cannot judge these for a fleet, so the compiler
generates the rules for the monitoring tool into `build/monitoring/`:
OpenSLO and Prometheus rules first, other tools only when asked. The build
manifest carries the same thresholds, so a tool that reads it needs no
generated rules. Rules are applied at deploy; a change made in the tool is
overwritten, as with Terraform.

- **Starting values** come from what the compiler knows (queries per path
  from area 6's query report), then from production once it has run. Local
  timings are not production timings; locally the compiler reports
  relative change on the same fixtures ("30% slower", "2 more queries").
- **An agent proposes changes** from observed data as an ordinary source
  diff, through the MCP server (area 17).
- **Loosening needs a person.** Raising or removing a threshold is how an
  agent silences an alert, so it needs an acceptance file (area 7's form)
  and appears in the approval report; tightening does not.
- **Per build and per flag value.** Thresholds belong to a build, and each
  flag value takes its own branch, so its own path and metrics, without
  extra tagging.

OpenTelemetry stays the transport; only what is sent changes.

## Email

Applications send a handful of messages: confirm your address, reset your
password, an invoice. A message is a type, `type X = EmailMessage`, whose fields are
the values its text uses, in its domain's `emails/` folder beside its HTML
and text templates (question 11). Its subject line is its member in the
domain's language files, with its fields as placeholders. The templates'
words come from the domain's translations (area 10): a template calls
`{{ translations.<domain>.<message>(...) }}`, which takes the email's
locale, and the compiler checks every call and placeholder against the
message types and the email's fields.

```jadpo fragment
// domains/accounts/emails/welcome_email.jadpo
/// Sent once a user has signed up.
type WelcomeEmail = EmailMessage {
    display_name: DisplayName
    sign_in_link: Url
}

// domains/accounts/translations/welcome_heading.jadpo
/// The heading of the welcome email.
type WelcomeHeading = Message {
    name: DisplayName
}

// domains/accounts/translations/en_gb.jadpo
translations {
    welcome_email: "Welcome, {display_name}"      // the subject line
    welcome_heading: "Welcome, {name}"
}
```

```text
<!-- domains/accounts/emails/welcome_email.html -->
<html dir="{{ locale.direction }}">
<h1>{{ translations.accounts.welcome_heading(name: display_name) }}</h1>
<a href="{{ sign_in_link }}">{{ translations.accounts.sign_in() }}</a>
</html>
```

**Templates are strict** (decided 2026-10-16, built with email in chunk
7.3). These are compile errors: a name that is not a field of the email's
type; a message that does not exist; a placeholder missing or of the wrong
type; a partial that does not exist or is called with the wrong fields;
`if` on something that is not `Bool`; `for` over something that is not a
list; and any call other than translations and formatting. A field the
template never uses is a warning.

**Only text and numbers are written out**, in templates as in
placeholders: `{{ value }}` takes `Text`, `PresentationText`, `Int` or
`Decimal` (numbers written the locale's way), or an enum whose values have
wording (area 10). Writing anything else out is a compile error that names
how to format it: an `Instant` or `Money` with `temporal.format` or the
money formatter, a `Bool` as a condition for `if`, a list walked with
`for`, an object's fields one by one, an enum without wording by adding
it. Values are escaped; HTML goes in unescaped only as the safe-HTML type,
which the standard library defines (input to chunk 5.4).

Sending is emitting: the mail goes after the action commits, retried like
any delivery (area 12), so a rolled-back sign-up never sends a welcome. Every
`EmailMessage` has `to:` and `locale:`.

```jadpo fragment
emit WelcomeEmail {
    to: user.email
    locale: user.locale
    display_name: user.display_name
    sign_in_link: config.app_url
}
```

- **Where it goes** is the `email` capability (area 11, revised
  2026-10-13): `email: { connection: postmark  from: config.mail_sender }` in
  `application { }` names a connection, and every email, the app's and
  Jadpo's built-in ones, goes through it (see "Built-in emails"). SMTP is one built-in driver
  (`driver: Driver.smtp`, port 587 on Cloudflare Workers, which blocks 25);
  a mail package (Postmark, Amazon SES, Resend) is another. The `EmailMessage`
  types, translations and `emit` are the same whichever is chosen, so
  changing provider changes `app.jadpo` only.
- **Bounces and complaints** come back as the standard events
  `EmailBounced` and `EmailComplained` when the package maps its webhooks
  onto them; with SMTP there are none.
- **Provider features** (Postmark's message streams, SES configuration
  sets) are typed settings on the connection; the provider's other
  operations (SES's suppression list) stay callable as an ordinary package
  (area 11).
- Tests capture mail instead of sending it, and read it as `email.sent`.
- **`jadpo dev` catches every email in a local inbox** (Richard, 2026-10-13).
  Nothing leaves the machine. A person opens it in the browser from the dev
  console (area 17): each mail rendered as HTML and as text, in its locale,
  with its links working (so a magic link signs in locally) and a link to
  the trace of the request that sent it. An agent uses the same inbox from
  the command line: `jadpo email list`, `jadpo email show <id> --json`,
  `jadpo email links <id>` and `jadpo email open <id> <link>`, so it can follow
  a sign-up or privacy-request flow end to end; `jadpo email clear` empties
  it. Phoenix's dev mailbox and Mailpit offer the browser half; the
  agent commands and the trace link are new.
- **A mail health score: an early indication, not a guarantee** (Richard,
  2026-10-13). Every mail in the dev inbox, and `jadpo email show <id>`,
  shows what Jadpo's own checks found and how to fix each: no plain-text
  part, image-only content, broken HTML, links that are not absolute HTTPS,
  link shorteners, an all-capitals subject, a missing unsubscribe header on
  bulk mail, more than 102 KB (Gmail clips it). `jadpo check` warns about
  templates that would fail. When SpamAssassin is installed its open rules
  are added. `jadpo email check-domain` compares the sending domain's SPF,
  DKIM and DMARC records with what the mail connection needs. What it does
  not do, said on every screen that shows a score: it cannot predict whether
  Gmail, Outlook or any other provider puts a mail in the inbox, because they
  use their own filters and the sender's history, which change constantly.
  A clean score means the common mistakes are absent, nothing more; it must
  not be relied on. Testing real inbox placement needs seed-account services
  outside Jadpo.
- The personal-data report lists the mail connection as receiving every
  marked field a mail carries; a package that maps an `erase` operation
  gets area 15's `PersonErased` subscriber generated.

### Built-in emails

> Decided on 2026-10-16 (Richard). Built with email (chunk 7.3) and the
> features that send them: magic-link sign-in (area 8, not yet on the
> roadmap) and data requests (area 15, after the first build).

Jadpo sends some emails itself: the magic-link email (area 8) and the
privacy emails (area 15). Each works with nothing written, and an app can
change as much of it as it needs, in five levels. Each level keeps the ones
below it.

**Delivery.** Every email, built in or the app's own, goes through the
app's one `email:` setting in `application { }`, which names a connection
providing `email` (area 11): the built-in SMTP driver or any package that
provides it. (The SMTP driver's own connection settings are area 11's.)

```jadpo fragment
// app.jadpo
application {
    connections: {
        postmark: {
            package: Postmark.v1
            credential: config.postmark_token
        }
    }
    email: {
        connection: postmark
        from: config.mail_sender
    }
}
```

`magic_link:` with no `email:` setting is a compile error, unless it sends
its own email (level 5).

**Level 1: nothing.** Jadpo renders its own email, HTML and text, in the
person's language, with Jadpo's wording. Jadpo ships its wording in
`en_gb` and `en_us` at first, and releases add locales. An app that
supports a locale Jadpo does not ship must supply the wording (level 2):
until it does, `jadpo check` fails with one error listing every Jadpo
message the locale is missing, unless the locale is `in_progress:`, which
falls back as any translation does (area 10).

**Level 2: change the words.** The app writes Jadpo's messages in the root
`translations/<locale>.jadpo`, under `jadpo: { }` (area 10, "Jadpo's and
packages' text"):

```jadpo fragment
// translations/en_gb.jadpo
translations {
    jadpo: {
        magic_link_email: "Your link to sign in to {app_name}"
        magic_link_intro: "Someone asked to sign in to {app_name} as {address}."
        sign_in_codes_disabled: "Too many wrong codes today. Use the link in your email instead."
    }
}
```

Rewording a message of a feature the app has not switched on (magic-link
wording without `magic_link:`) is an error saying the feature is off, and
`jadpo translations builtin` lists only the messages of the features the
app has on, with their fields. Every Jadpo message may also use
`{app_name}`, the application's `display_name:` (below); an app that needs
the name translated rewords the messages. The magic-link email and its
confirm page use these messages:

| Message | Fields | `en_gb` wording |
|---|---|---|
| `magic_link_email` (the subject) | | "Sign in to {app_name}" |
| `magic_link_heading` | | "Sign in" |
| `magic_link_intro` | `address: Email` | "Someone asked to sign in as {address}." |
| `magic_link_button` | | "Sign in" |
| `magic_link_code_line` | `code: SignInCode` | "Or enter this code: {code}" |
| `magic_link_expiry_line` | `expires_in: PresentationText` | "The link and the code work for {expires_in}." |
| `magic_link_ignore_line` | | "If you didn't ask to sign in, you can ignore this email." |
| `magic_link_confirm_heading` | `address: Email` | "Sign in as {address}?" |
| `magic_link_confirm_button` | | "Sign in" |
| `sign_in_link_expired` | | "This link has expired or has been used. Ask for a new one." |
| `sign_in_link_other_browser` | | "Open this link in the browser where you asked for it, or enter the code instead." |
| `sign_in_code_invalid` | | "That code isn't right. Check it, or ask for a new one." |
| `sign_in_codes_disabled` | | "Too many wrong codes today. Use the link in your email instead." |
| `sign_in_refused` | | "We couldn't sign you in." |

`expires_in` is `expires_after:` as Jadpo formats it in the email's
language ("15 minutes"). The failures' wording also goes into the
per-locale failure export for client apps (area 10).

**Level 3: branding.** Every email, built in or the app's own, is wrapped
in the app's shared email layout, so built-in emails carry the app's
branding with no other change. The layout is optional, one per app, in
`shared/emails/layout.html` and `shared/emails/layout.txt`; without it
Jadpo's plain layout is used. It is checked under the strict template
rules and may call translations, and it receives:

| In the layout | What it is |
|---|---|
| `{{ content }}` | The rendered body: the safe-HTML type in `layout.html`, text in `layout.txt` |
| `{{ subject }}` | The email's subject line, `PresentationText` |
| `{{ locale.direction }}` | `ltr` or `rtl` (area 10) |
| `{{ app.display_name }}` | The application's `display_name:` |

```text
<!-- shared/emails/layout.html -->
<html dir="{{ locale.direction }}">
<head><title>{{ subject }}</title></head>
<body>
  <header>{{ app.display_name }}</header>
  {{ content }}
</body>
</html>
```

**The application's name.** `application { display_name: "Acme" }` in
`app.jadpo`, plain text, is required once any built-in email or page is
switched on (magic links, the privacy pages); without it `jadpo check`
names the feature that needs it. Templates read it as
`{{ app.display_name }}`, and Jadpo's own wording as `{app_name}`.

**Level 4: the app's own email.** `magic_link: { email: accounts.SignInEmail }`
names an `EmailMessage` type of the app's that Jadpo fills and sends instead
of its own. The type takes its fields from the built-in `MagicLinkEmail`
with `pick:`, and may have no others, since Jadpo fills it:

| `MagicLinkEmail` field | Type | Holds |
|---|---|---|
| `address` | `Email` | The address the link was asked for; also the email's `to:` |
| `sign_in_link` | `Url` | The link to the confirm page |
| `code` | `SignInCode` | The six-digit code, text so a leading `0` stays |
| `expires_in` | `PresentationText` | How long both work, formatted by Jadpo in the email's language |

Every `EmailMessage` also has `to:` and `locale:` (above); Jadpo sets the
locale as for its own email (area 8).

```jadpo fragment
// domains/accounts/emails/sign_in_email.jadpo
/// The sign-in email, in the app's own words and design.
type SignInEmail = EmailMessage {
    pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code, MagicLinkEmail.expires_in]
}

// domains/accounts/translations/en_gb.jadpo
translations {
    sign_in_email: "Your Notes sign-in link"       // the subject line
    sign_in_heading: "Welcome back to Notes"
    sign_in_button: "Open Notes"
    sign_in_code_line: "Or type {code} on the sign-in page."
    sign_in_expiry: "Both work for {expires_in}."
}

// authentication.jadpo
authentication {
    ...
    sign_in: {
        magic_link: {
            email: accounts.SignInEmail
        }
    }
}
```

```text
<!-- domains/accounts/emails/sign_in_email.html -->
<h1>{{ translations.accounts.sign_in_heading() }}</h1>
<p><a href="{{ sign_in_link }}">{{ translations.accounts.sign_in_button() }}</a></p>
<p>{{ translations.accounts.sign_in_code_line(code: code) }}</p>
<p>{{ translations.accounts.sign_in_expiry(expires_in: expires_in) }}</p>
```

The messages it calls (`SignInHeading`, `SignInCodeLine` ...) are the
domain's own `Message` types, one per file in `translations/`. Its `.html`
and `.txt` templates sit beside it in `emails/`, its subject is
its member in the domain's language files, and all of it follows the strict
template rules above; it is wrapped in the shared layout like any of the
app's emails. `MagicLinkEmail` is never emitted by app code.

**Level 5: the app's own sending.** `magic_link: { send: accounts.send_sign_in_link }`
names an action that sends the email itself, through any package, instead
of Jadpo sending one. Its signature is fixed: it takes one built-in
`MagicLinkRequest` and returns `Unit`.

| `MagicLinkRequest` field | Type |
|---|---|
| `address` | `Email` |
| `sign_in_link` | `Url` |
| `code` | `SignInCode` |
| `expires_at` | `Instant` |
| `locale` | `Locale` |

```jadpo fragment
// domains/accounts/actions/send_sign_in_link.jadpo
action send_sign_in_link(request: MagicLinkRequest) fails MessageNotSent -> Unit {
    attempt messages.send(...)          // a package of the app's choice
}
```

- It runs after the request commits, like a subscriber, and is retried
  under area 12's rules; its failures are a subscriber's.
- In development its provider calls are faked and logged, as every
  provider call is (area 11), and the local inbox does not show it, since
  Jadpo sends no email.
- Levels 4 and 5 are exclusive: `email:` and `send:` on one email is a
  compile error.

**Every built-in email.** The same five levels apply to each email Jadpo
sends. A feature with one email takes `email:` or `send:` directly; a
feature with several takes `emails: { }`, one entry per email, each taking
`email:` or `send:`, never both. Delivery is always the one `email:`
connection.

| Email | Built-in types | Sent when | Configured in |
|---|---|---|---|
| Magic link | `MagicLinkEmail`, `MagicLinkRequest` | `POST /auth/magic-link` (area 8) | `authentication { sign_in: { magic_link: { email: / send: } } }` |
| Address check | `PrivacyAddressCheckEmail`, `PrivacyAddressCheckRequest` | A person asks on the `/privacy` page (area 15) | `privacy: { emails: { address_check: { } } }` |
| Export ready | `PrivacyExportEmail`, `PrivacyExportRequest` | `PrivacyExportReady` (area 15) | `privacy: { emails: { export_ready: { } } }` |
| Erasure confirmed, with its cancel link | `PrivacyErasureConfirmedEmail`, `PrivacyErasureConfirmedRequest` | `PrivacyErasureScheduled` (area 15) | `privacy: { emails: { erasure_confirmed: { } } }` |
| Erasure completed, with what was kept | `PrivacyErasureCompletedEmail`, `PrivacyErasureCompletedRequest` | `PrivacyErasureCompleted` (area 15) | `privacy: { emails: { erasure_completed: { } } }` |

```jadpo fragment
// app.jadpo
application {
    display_name: "Acme"
    privacy: {
        self_service: {}
        emails: {
            export_ready: {
                email: privacy_mail.ExportEmail
            }
            erasure_confirmed: {
                send: privacy_mail.send_erasure_confirmation
            }
        }
    }
}
```

The privacy emails' fields, which Jadpo fills: every one has `address:
Email` (also its `to:`) and the `locale:` of the request, and each request
type has the same fields with times as `Instant` in place of text.

| Email type | Its other fields | Request type's other fields |
|---|---|---|
| `PrivacyAddressCheckEmail` | `confirm_link: Url`, `expires_in: PresentationText` | `confirm_link: Url`, `expires_at: Instant` |
| `PrivacyExportEmail` | `download_link: Url`, `expires_in: PresentationText` (7 days) | `download_link: Url`, `expires_at: Instant` |
| `PrivacyErasureConfirmedEmail` | `erases_on: PresentationText`, `cancel_link: Url` | `erases_at: Instant`, `cancel_link: Url` |
| `PrivacyErasureCompletedEmail` | `kept: List<PrivacyKeptRecord>` | `kept: List<PrivacyKeptRecord>` |

`PrivacyKeptRecord` has `name: PresentationText`, the kept record's wording
(its entity's member in its domain's language files, required for every
entity marked `Erasure.retained`), and `kept_until: PresentationText`, the
end of its retention in the email's language. A template walks `kept`
with `for`. The address-check link lasts 15 minutes, as a magic link
does.

## Search

An entity with a `search:` section can be searched by words, with PostgreSQL's
full-text search (question 12). The compiler generates the search column,
its index and its migration, and a query uses `matches`.

```jadpo fragment
// domains/planning/entities/todo.jadpo
entity Todo {
    ...
    search: {
        fields: [title, notes]
        language: SearchLanguage.english
    }
}

// domains/planning/queries/search_todos.jadpo
query search_todos(owner_id: User.id, text: SearchText) -> Page<TodoView> {
    from: Todo
    where: Todo.owner_id == owner_id
    where: Todo matches text
}
```

- Fields listed first count most when ordering results.
- With no `order:`, best matches come first, then by `id`, in keyset pages
  like every other list (area 6).
- `text` takes what people type into search boxes: words, quoted phrases and
  `-word`, never raw query syntax.
- This replaces area 5's `kind:` full-text index: one declaration, not an
  index plus hand-written conditions.

Typo tolerance, facets and search across entities need a search service;
that is an integration, as Laravel Scout's other engines are.

## Not built in

| Feature | Why not, or not yet |
|---|---|
| Realtime subscriptions | Policy on a live stream of changes is a large design; later, after the golden todo. Until then, clients poll with the revision header (area 7) |
| Generated admin screens | Jadpo makes backends, not interfaces. Operators use `jadpo` commands (area 14); a screen can come later on the same operations |
| Payments | Money moves at the provider; an integration plus webhooks (areas 7 and 11) is the right shape, and the compiler gains nothing more |
| Content management | Content is application data |
| Product analytics | Events and metrics above feed whichever tool a team uses |
| Quotas and plan limits | A plan's limits are application data, checked by an action; counting uses area 6's counters |
| Background job dashboards | Already area 14's operator commands (question 13 there) |

## Review

### Status

Nothing on this page is built yet. Email comes with chunk 7.3, and default
rate limits with routes (chunk 4.1); flags, files, audit, search and
monitoring thresholds come after the first build (roadmap).

### Consistency with the rest of the language

- **A new root file.** `flags.jadpo` joins area 1's fixed root list, as an
  unnamed `flags { }` singleton like `config { }`.
- **Reading flags without a caller parameter.** `current_principal` exists
  only in routes (area 8), yet flags are evaluated for the caller inside
  actions. This follows policy, which also gets the caller from the runtime;
  the value is fixed per operation like `clock.now`.
- **Graceful degradation.** Flags and rate limits follow area 5's cache
  rule: an unreachable store never fails a request.
- **One store per action.** File bytes are written by the client and deleted
  by a generated subscriber, so no action writes two stores.
- **Mail as an event.** Reusing `emit` and `events/` adds no new statement or
  folder; `EmailMessage` is a new type base beside `Event` and `Failure`.
- **Capabilities.** Files and email follow area 11's proposed capabilities
  (questions 19–25 there): the provider is a connection in `app.jadpo`,
  and `stores:` becomes `connections:`. Search, flags, rate limits and
  audit stay in the main store and need no capability.
- **Size literals.** `max_size: 5mb` is a new literal form, following the
  duration spelling of area 10 question 4.
- **Retention shape.** Audit retention reuses area 15's
  `retention: { after: }`; `since:` is always the entry's own time.
- **Environment variables.** Area 10 says every setting is declared in
  `config.jadpo`; the OpenTelemetry variables are read by the runtime under
  their standard names instead, listed in the configuration report.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| `flags.jadpo` in the fixed root list; `EmailMessage` types in `emails/` | 1 | Questions 1 and 11 |
| `5mb` size literals; `File`, `FileUpload`, `UploadSlot`, `SearchText` built-in types | 2 | Questions 7 and 12 |
| File stores as `files` connections; `search:` replacing the full-text index `kind:` | 5, 11 | Questions 7 and 12 |
| `matches` in queries; relevance order in keyset pages | 6 | Question 12 |
| `rate_limit:` on APIs and routes; `GET /flags`; rate limits in OpenAPI | 7 | Questions 5 and 6 |
| Default rate limits on generated authentication routes | 8 | Question 6 |
| Per-tenant flag targeting depends on tenancy; slot and history grants | 9 | Questions 3, 7 and 9 |
| Translations format for mail (area 10 question 9, revised 2026-10-16: domain language files read as `translations.<domain>.<message>`); OpenTelemetry variables | 10 | Questions 10 and 11 |
| The safe-HTML type that templates write out unescaped; the money formatter | Standard library (chunk 5.4) | Question 11 |
| Mail delivery and retries as an event; trace context through the outbox | 12 | Questions 10 and 11 |
| `flags:` in tests and the both-values check; `email.sent`; an in-memory file store in tests | 13 | Questions 4, 7 and 11 |
| `jadpo flags` commands and source removal; the `syntax.expected_declaration` message | 14 | Question 2; Status |
| File erasure and export; audit entries on erasure; field encryption | 15 | Questions 7, 8 and 9 |
| Where threshold declarations live | 1 | Question 15 |
| Loop and recursion rule for path numbering | 3 | Question 13 |
| Stable identity for branches as well as declarations | 5 | Question 13 |
| Loosened thresholds need an acceptance | 7 | Question 16 |
| `jadpo path`; path IDs in `jadpo trace` and `incident`; agent-proposed thresholds through MCP | 17 | Questions 13 and 15 |

### Open questions

1. **Are flags declared in a root `flags.jadpo`, with type, default, owner
   and expiry?** *Recommendation:* yes, `Bool` or an `Enum`, with `expires:`
   required. Laravel Pennant defines flags in code with no list or expiry,
   and teams lose track of them; one file is the list a reviewer reads. *Decided 2026-10-13:* yes, as recommended.
2. **Is flag state stored in the application's database and changed with
   `jadpo flags`?** *Recommendation:* yes, no flag service. Pennant also
   stores flags in the app's database; LaunchDarkly or Unleash would be a
   third-party dependency for something small enough to own. A web screen
   can come later on the same operations.
   *Revised 2026-10-13 (Richard):* the store is named, never assumed:
   `flags: { store: main }` in `app.jadpo`; without it, using a flag is a
   compile error (the area 5 rule for every system feature). *Decided 2026-10-13:* as revised.
3. **Which targeting rules?** *Recommendation:* named users, named tenants
   (once area 9 decides tenancy), a percentage by a hash of flag name and
   user `id`, then everyone; first match wins. No attribute rules
   ("country is FR") until an app needs them; Firebase and Unleash start
   from the same percentage rule. *Decided 2026-10-13:* yes, as recommended.
4. **What does the compiler check?** *Recommendation:* unknown flags are
   errors; tests must cover every value of each flag read; expired flags are
   warnings; "fully rolled out" comes from `jadpo flags list`, because the
   compiler cannot see production; `jadpo flags remove --keep` rewrites the
   source. *Decided 2026-10-13:* yes, as recommended.
5. **Should Jadpo support OpenFeature?** *Recommendation:* not now. Jadpo
   evaluates its own flags, so it needs no OpenFeature provider. Client
   flags go out on a plain generated `GET /flags`; adopt OpenFeature's remote
   evaluation protocol (OFREP) for that route once it is stable, which its
   authors say it is not yet.
   *Decided 2026-10-13 (Richard):* yes, and the client sees its own state:
   `GET /flags` returns every flag marked `client: true`, evaluated for the
   caller (their user, tenant and rollout bucket), so a frontend shows
   exactly what this user has. Flags without `client: true` never leave the
   server. *Decided 2026-10-13:* as revised.
6. **Are rate limits `rate_limit: { requests: within: by: }` on an API or
   route, counted in Redis with an in-memory fallback?** *Recommendation:*
   yes, keyed by caller or address, with built-in limits on the generated
   authentication routes. This is Rails 8's `rate_limit to: within: by:` in
   Jadpo's form.
   *Revised 2026-10-13 (Richard): on by default for every route.* With no
   setting, every API gets sensible limits: per signed-in caller 600 a
   minute, per address 120 a minute on public routes, and 10 a minute per
   address on the generated sign-in and privacy routes; `rate_limit:`
   loosens or tightens them per API or route, and turning one off needs an
   acceptance. Counts live in Redis when a cache connection is named
   (`rate_limits: { store: cache }`); without one they are kept in memory
   with a fixed allowance (`memory: 16MB` by default, oldest counters
   dropped first), and `jadpo check` warns that limits are then per
   instance, multiplied by the number running, and recommends Redis. On
   Workers there is no lasting memory, so a Redis connection (or
   Cloudflare's rate-limiting binding as a driver) is required. *Decided 2026-10-13:* as revised.
7. **Are files a `File` field with a file store, direct uploads through a
   signed slot, and signed links in outputs?** *Recommendation:* yes, S3
   protocol only. It is how Rails Active Storage and Laravel work, and the
   compiler then applies policy, erasure and retention to the bytes.
   Image resizing and virus scanning wait for a real app.
   *Revised 2026-10-13 (capabilities, area 11 questions 19–24):* the store
   is a connection providing `files`: the built-in S3 driver, or a package
   that is a preset over it (R2's upload slots still need S3 keys, even on
   Workers); local disk in development and tests; `store.put(bytes, type)`
   for files the application makes. *Decided 2026-10-13:* as revised.
8. **Should outgoing webhooks and field encryption be built in?**
   *Recommendation:* yes, both later. Outgoing webhooks are events with
   signed, retried delivery to a customer's address, which the compiler can
   type from the event and document; field encryption (Rails has
   `encrypts`) belongs in area 15, which knows sensitive fields. Neither is
   needed by the golden todo. *Decided 2026-10-13:* yes, as recommended.
9. **Is the audit trail an opt-in `audit:` section, written in the same
   transaction, with personal fields hashed?** *Recommendation:* yes, opt-in
   per entity, history readable with `read` on the record. Rails and Django
   need a library (PaperTrail, django-simple-history) for this, and those
   cannot hash personal fields or know the request. *Decided 2026-10-13:* yes, as recommended.
10. **Is OpenTelemetry generated for everything, configured by the standard
    variables?** *Recommendation:* yes, traces and metrics with no syntax and
    no values in spans. Phoenix generates telemetry for every request and
    query the same way; using the standard variables means any monitoring
    tool works. *Decided 2026-10-13:* yes, with its settings declared in `config.jadpo` like any setting, bound to the standard `OTEL_*` variable names (area 10).
11. **Is email built in as `type X = EmailMessage`, emitted after commit and sent
    over SMTP?** *Recommendation:* yes, templates from `translations/`, SMTP
    only, provider APIs as integrations. This replaces the golden app's
    `ReminderMail` integration; Rails, Django and Laravel all ship mail with
    SMTP and a test inbox.
    *Revised 2026-10-13 (capabilities, area 11 questions 19–24):* email is
    no longer SMTP only. Sending goes through the `email` capability: SMTP is
    one built-in driver and a mail package (Postmark, SES) another, chosen
    by `email: { connection: }` with no code change; bounces and complaints
    come back as standard events; provider features are connection
    settings or the package's own operations. Laravel's mailers and Rails'
    delivery methods work the same way.
   *Revised 2026-10-13 (Richard):* mails live in their domain's `emails/`
   folder: the type (`welcome_email.jadpo`), an HTML template
   (`welcome_email.html`) and a text template (`welcome_email.txt`), plus a
   shared layout. No MJML: agents write plain HTML, which is safer and more
   flexible, and the health score flags what mail clients mishandle.
   Templates use a small template language, checked by the compiler against
   the email's type: `{{ display_name }}` (escaped), `{{ t.welcome_heading }}`
   for translated text, conditions, loops, partials and the standard
   formatting functions (`{{ temporal.format(due_at) }}`); the syntax for
   conditions and loops is the open point below.
   Nothing else: values are prepared in the mail's fields, not computed in
   the template.
    *Template syntax (Richard asked, 2026-10-13):* recommendation, use
    Jadpo's own syntax inside `{{ }}` rather than a second mini-language:
    `{{ if (due_at != none) }} ... {{ else if (...) }} ... {{ else }} ...
    {{ end }}`, `{{ for todo in todos }} ... {{ end }}`, expressions exactly
    as in Jadpo code, and partials as typed calls,
    `{{ include button(label: t.open_app, url: app_url) }}`. Templates may
    only read fields, translations and formatting functions: no variables,
    no `attempt`, no other calls. Razor does the same with C#; one syntax
    means one thing for agents to learn and one parser and type checker. *Decided 2026-10-13:* the revisions above, with Jadpo's own syntax inside `{{ }}` for conditions, loops and typed partials.
    *Revised 2026-10-16 (Richard, area 10 translations):* templates read
    translated text as `{{ translations.<domain>.<message>(field: value) }}`
    with the email's locale, not `t.`, and partials pass it the same way
    (`{{ include button(label: translations.accounts.open_app(), url: app_url) }}`).
    Templates are strict: a name that is not a field of the email's type, a
    message that does not exist, a placeholder missing or of the wrong type,
    an unknown partial or a partial called with the wrong fields, `if` on a
    non-`Bool`, `for` over a non-list and any call other than translations
    and formatting are compile errors, and an unused field is a warning.
    Only text and numbers are written out (`Text`, `PresentationText`,
    `Int`, `Decimal`, and enums with wording); anything else is an error
    naming how to format it (`temporal.format`, the money formatter, `if`,
    `for`, field by field). Values are escaped, and raw HTML goes in only as
    the safe-HTML type (input to chunk 5.4). An email's subject line is
    its member in its domain's language files (`welcome_email: "Welcome,
    {display_name}"`), its fields usable as placeholders; the body stays
    the `.html` and `.txt` templates. See "Email" above.
    *Revised 2026-10-16 (Richard, built-in emails):* every email Jadpo
    sends (the magic-link email, the privacy emails) can be changed in five
    levels, from nothing to the app's own sending; see "Built-in emails".
12. **Is search a `search:` section using PostgreSQL full-text search?**
    *Recommendation:* yes, one language per entity, with search services as
    integrations. Django's `SearchVector` and Laravel Scout's database engine
    do the same with more hand-written setup. *Decided 2026-10-13:* yes, as recommended.
13. **Does every request carry a path ID?** *Recommendation:* yes, two:
    a Ball–Larus `path` number that decodes against the build's graph, and
    a `path_key` hash of stable identities for comparing builds; loops
    record which inner paths ran, recursion its first entry. Sentry groups
    errors by guessing from stacks; the compiler can name the path exactly.
    Needs control flow in the compiler's graph. *Decided 2026-10-13:* yes, as recommended.
14. **Does the runtime decide how much to send per request?**
    *Recommendation:* yes: buffer each request, send the full trace on a
    crossed threshold, failure, fault or new path, otherwise metrics plus
    one sample per path per minute. This is tail sampling done in the
    process, which generic tools cannot do because they do not know what a
    new path is. *Decided 2026-10-13:* yes, as recommended.
15. **Are thresholds declared in source, and are monitoring rules
    generated?** *Recommendation:* yes, one monitoring file per domain, so
    a reviewer sees the set; per-request thresholds act in the runtime,
    across-request ones become OpenSLO and Prometheus rules in
    `build/monitoring/` and in the build manifest. OpenSLO and Sloth keep
    SLOs as code beside the app, where a renamed route breaks them silently;
    here the compiler checks every threshold names something that exists. *Decided 2026-10-13:* yes, as recommended.
16. **Does loosening a threshold need a person?** *Recommendation:* yes,
    an acceptance file and a line in the approval report; tightening does
    not. Otherwise an agent fixes a noisy alert by raising its limit. *Decided 2026-10-13:* yes, as recommended.

### Decisions

| Topic | Decision | Date |
|---|---|---|
| Setting options | Carried from area 2: options are enum values written `Type.value` | 2026-10-12 |
| Colon rule | Carried from area 3: every member is `name: value`; sections are `name: { }` | 2026-10-12 |
| Cache outages | Carried from area 5: an unreachable cache never fails a request | 2026-10-12 |
| Stores | Carried from area 5: stores declared in `app.jadpo`; an action writes at most one store | 2026-10-12 |
| Reading the caller | Carried from area 8: `current_principal` in routes only; policy gets the caller from the runtime | 2026-10-12 |
| Third-party services | Carried from area 8: the language owns a feature rather than relying on a third-party service where it reasonably can | 2026-10-12 |
| Policy model | Carried from area 9: grants keyed by `create`, `read`, `update`, `delete` | 2026-10-12 |
| Flag declaration | Root `flags.jadpo`: `Bool` or enum, default, owner, required `expires:` | 2026-10-13 |
| Flag storage and changes | In the app's own database, named explicitly (`flags: { store: main }`); changed with `jadpo flags`; no flag service | 2026-10-13 |
| Flag targeting | Named users, named tenants, percentage by hash of flag and user id, then everyone; first match wins | 2026-10-13 |
| Flag checks | Unknown flags are errors; tests cover every value read; expired flags warn; `jadpo flags list` shows rollout; `jadpo flags remove --keep` | 2026-10-13 |
| OpenFeature | Not now; `GET /flags` returns `client: true` flags evaluated for the caller (Richard) | 2026-10-13 |
| Rate limits | On by default for every route with sensible limits; `rate_limit:` adjusts; turning off needs an acceptance; Redis when named, otherwise bounded memory with a warning; Redis or a platform binding required on Workers (Richard) | 2026-10-13 |
| Files | A connection providing `files` (built-in S3 driver or a preset package); signed upload slots and links; local disk in development and tests | 2026-10-13 |
| Outgoing webhooks and field encryption | Both later; encryption with area 15's sensitive fields | 2026-10-13 |
| Audit trail | Opt-in `audit:` per entity, same transaction, personal fields hashed, readable with `read` on the record | 2026-10-13 |
| Tracing and metrics | OpenTelemetry generated for everything, no values in spans; settings in `config.jadpo` bound to `OTEL_*` names | 2026-10-13 |
| Email | `emails/` folder per domain: type, HTML and text templates, shared layout; no MJML; templates use Jadpo syntax inside `{{ }}` (values, translations, `if`, `for`, typed `include`, formatting), read-only; delivered through the `email` capability (Richard) | 2026-10-13 |
| Email templates | Subject line from the email's member in its domain's language files, its fields as placeholders; translated text through `translations.<domain>.<message>(...)` with the email's locale, not `t.`; strict checks (unknown names, messages and partials, wrong placeholders or partial fields, `if` on non-`Bool`, `for` over non-lists and other calls are errors; an unused field warns); only `Text`, `PresentationText`, `Int`, `Decimal` and enums with wording are written out, anything else an error naming how to format it; values escaped, raw HTML only as the safe-HTML type (Richard, area 10) | 2026-10-16 |
| Search | `search:` section with PostgreSQL full-text search, one language per entity; search services as packages | 2026-10-13 |
| Path IDs | Every request carries a path number and a stable path key | 2026-10-13 |
| Detail by threshold | Full traces on threshold, failure, fault or new path; otherwise metrics plus one sample per path per minute | 2026-10-13 |
| Thresholds in source | One monitoring file per domain, checked by the compiler; rules generated into `build/monitoring/` | 2026-10-13 |
| Loosening thresholds | Needs an acceptance; tightening does not | 2026-10-13 |
| Local inbox in development | `jadpo dev` catches all mail: browser view in the dev console (rendered, locale, working links, trace link) and `jadpo email list/show/links/open/clear` for agents (Richard) | 2026-10-13 |
| Mail health score | Deterministic content checks per mail in the dev inbox and `jadpo email show`, optional SpamAssassin rules, `jadpo email check-domain` for SPF/DKIM/DMARC; every score states it is an early indication, cannot predict inbox placement and must not be relied on (Richard) | 2026-10-13 |
| Email naming | Folder `emails/`, capability `email`, base type `EmailMessage` (`Email` stays the address type), commands `jadpo email ...`, app setting `email: { connection: }` (Richard) | 2026-10-13 |
| Shared email layout and display name | Optional `shared/emails/layout.html` and `layout.txt`, one per app, wrapping every email, receiving `content` (safe HTML in the HTML layout), `subject`, `locale.direction` and `app.display_name`, strict template rules; Jadpo's plain layout otherwise. `application { display_name: }` required once a built-in email or page is on, read as `{{ app.display_name }}` and `{app_name}` (decided for Richard) | 2026-10-16 |
| Privacy emails | Built-in `PrivacyAddressCheckEmail`, `PrivacyExportEmail`, `PrivacyErasureConfirmedEmail` and `PrivacyErasureCompletedEmail`, each with a `...Request` type for `send:`; configured in `privacy: { emails: { address_check: export_ready: erasure_confirmed: erasure_completed: } }`, each `email:` or `send:`; a feature with one email takes them directly (decided for Richard) | 2026-10-16 |
| Built-in emails | Five levels for every email Jadpo sends: nothing (Jadpo's email and wording, `en_gb` and `en_us` at first, other supported locales an error until the app words them); the words, in the root `translations/` under `jadpo: { }`, only for features switched on; branding, through the app's shared layout; the app's own `EmailMessage` (`email:`), picking from the built-in type's fields; the app's own sending (`send:`), an action taking the built-in request, run after commit; `email:` and `send:` together an error; all through the app's one `email:` connection (Richard) | 2026-10-16 |
