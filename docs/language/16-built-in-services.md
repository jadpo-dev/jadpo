---
title: Built-in services
area: 16
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules: []
---

# Built-in services

Which common backend features Jadpo should provide itself, rather than
leaving each application to install a library or write its own: feature
flags, rate limits, files, audit trails, tracing, email and search.

> In review. Nothing on this page exists in the compiler yet: every example is a proposal, written with the decisions from areas 1 to 9, and each names its open question. What other frameworks ship, and the options considered, are in [the proposals](research/16-built-in-services-proposals.md).

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
| Email | Yes, through the `mail` capability | Templates typed against translations; delivered after commit; any provider without code changes | `type X = Mail` in `events/`; a `mail` connection in `app.jadpo` |
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

Path IDs need control flow inside each callable in the compiler's graph,
which today records only declarations and calls.

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
password, an invoice. A message is a type, `type X = Mail`, whose fields are
the values its text uses, kept with events in the `events/` folder (question
11). Its subject and body are messages in `translations/` (area 10), under
the type's name, and the compiler checks every placeholder against the
fields and every locale against the default.

```jadpo fragment
// domains/accounts/events/welcome_mail.jadpo
/// Sent once a user has signed up.
type WelcomeMail = Mail {
    display_name: DisplayName
    sign_in_link: Url
}

// translations/en_gb.json
{
    "welcome_mail.subject": "Welcome, {display_name}",
    "welcome_mail.body": "Sign in at {sign_in_link} to get started."
}
```

Sending is emitting: the mail goes after the action commits, retried like
any delivery (area 12), so a rolled-back sign-up never sends a welcome. Every
`Mail` has `to:` and `locale:`.

```jadpo fragment
emit WelcomeMail {
    to: user.email
    locale: user.locale
    display_name: user.display_name
    sign_in_link: config.app_url
}
```

- **Where it goes** is the `mail` capability (area 11, revised
  2026-10-13): `mail: { connection: postmark  from: config.mail_sender }` in
  `application { }` names a connection. SMTP is one built-in driver
  (`driver: Driver.smtp`, port 587 on Cloudflare Workers, which blocks 25);
  a mail package (Postmark, Amazon SES, Resend) is another. The `Mail`
  types, translations and `emit` are the same whichever is chosen, so
  changing provider changes `app.jadpo` only.
- **Bounces and complaints** come back as the standard events
  `MailBounced` and `MailComplained` when the package maps its webhooks
  onto them; with SMTP there are none.
- **Provider features** (Postmark's message streams, SES configuration
  sets) are typed settings on the connection; the provider's other
  operations (SES's suppression list) stay callable as an ordinary package
  (area 11).
- Tests capture mail instead of sending it, and read it as `mail.sent`.
- **`jadpo dev` catches every email in a local inbox** (Richard, 2026-10-13).
  Nothing leaves the machine. A person opens it in the browser from the dev
  console (area 17): each mail rendered as HTML and as text, in its locale,
  with its links working (so a magic link signs in locally) and a link to
  the trace of the request that sent it. An agent uses the same inbox from
  the command line: `jadpo mail list`, `jadpo mail show <id> --json`,
  `jadpo mail links <id>` and `jadpo mail open <id> <link>`, so it can follow
  a sign-up or privacy-request flow end to end; `jadpo mail clear` empties
  it. Phoenix's dev mailbox and Mailpit offer the browser half; the
  agent commands and the trace link are new.
- **A mail health score: an early indication, not a guarantee** (Richard,
  2026-10-13). Every mail in the dev inbox, and `jadpo mail show <id>`,
  shows what Jadpo's own checks found and how to fix each: no plain-text
  part, image-only content, broken HTML, links that are not absolute HTTPS,
  link shorteners, an all-capitals subject, a missing unsubscribe header on
  bulk mail, more than 102 KB (Gmail clips it). `jadpo check` warns about
  templates that would fail. When SpamAssassin is installed its open rules
  are added. `jadpo mail check-domain` compares the sending domain's SPF,
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
| Realtime subscriptions | Policy on a live stream of changes is a large design; later, after the golden todo. Polling with the revision header (area 7) works today |
| Generated admin screens | Jadpo makes backends, not interfaces. Operators use `jadpo` commands (area 14); a screen can come later on the same operations |
| Payments | Money moves at the provider; an integration plus webhooks (areas 7 and 11) is the right shape, and the compiler gains nothing more |
| Content management | Content is application data |
| Product analytics | Events and metrics above feed whichever tool a team uses |
| Quotas and plan limits | A plan's limits are application data, checked by an action; counting uses area 6's counters |
| Background job dashboards | Already area 14's operator commands (question 13 there) |

## Review

### Status

| Feature | Status |
|---|---|
| Feature flags | **Not built (run):** `flag new_checkout { }` fails with `syntax.expected_declaration`. |
| Rate limits | **Not built (read):** `rate_limited` exists as a declarable failure kind (area 4) and integrations map a provider's 429 to it; nothing limits incoming requests. |
| Files | **Not built (run):** `type Avatar = File {}` fails with `semantic.unknown_name`. |
| Audit trail | **Not built (run):** `audit {}` in an entity fails with `syntax.unexpected_token`. Today's `build/audit/` folder is compiler output about the source, not a history of data. |
| Tracing and metrics | **Partly (read, generated `app.ts`):** the runtime writes JSON log events; `operationalEventToOpenTelemetry` is generated but never called, and `traceId` is always `null`. |
| Path IDs, monitoring thresholds | **Not built (read):** proposals from 2026-10-13. The semantic graph (`jadpo/crates/semantic/src/lib.rs`) holds declarations and call edges, no control flow. |
| Email, search | **Not built (read):** the only mail is the golden app's `ReminderMail` integration (area 11). |
| Error message | **Bug (run):** the `syntax.expected_declaration` reason lists `persist`, `module` and `import` and leaves out `entity`, `query` and `job`, which compile. Carried to area 14. |

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
  folder; `Mail` is a new type base beside `Event` and `Failure`.
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
| `flags.jadpo` in the fixed root list; `Mail` types in `events/` | 1 | Questions 1 and 11 |
| `5mb` size literals; `File`, `FileUpload`, `UploadSlot`, `SearchText` built-in types | 2 | Questions 7 and 12 |
| File stores as `files` connections; `search:` replacing the full-text index `kind:` | 5, 11 | Questions 7 and 12 |
| `matches` in queries; relevance order in keyset pages | 6 | Question 12 |
| `rate_limit:` on APIs and routes; `GET /flags`; rate limits in OpenAPI | 7 | Questions 5 and 6 |
| Default rate limits on generated authentication routes | 8 | Question 6 |
| Per-tenant flag targeting depends on tenancy; slot and history grants | 9 | Questions 3, 7 and 9 |
| Translations format for mail (area 10 question 9); OpenTelemetry variables | 10 | Questions 10 and 11 |
| Mail delivery and retries as an event; trace context through the outbox | 12 | Questions 10 and 11 |
| `flags:` in tests and the both-values check; `mail.sent`; an in-memory file store in tests | 13 | Questions 4, 7 and 11 |
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
   and teams lose track of them; one file is the list a reviewer reads.
2. **Is flag state stored in the application's database and changed with
   `jadpo flags`?** *Recommendation:* yes, no flag service. Pennant also
   stores flags in the app's database; LaunchDarkly or Unleash would be a
   third-party dependency for something small enough to own. A web screen
   can come later on the same operations.
3. **Which targeting rules?** *Recommendation:* named users, named tenants
   (once area 9 decides tenancy), a percentage by a hash of flag name and
   user `id`, then everyone; first match wins. No attribute rules
   ("country is FR") until an app needs them; Firebase and Unleash start
   from the same percentage rule.
4. **What does the compiler check?** *Recommendation:* unknown flags are
   errors; tests must cover every value of each flag read; expired flags are
   warnings; "fully rolled out" comes from `jadpo flags list`, because the
   compiler cannot see production; `jadpo flags remove --keep` rewrites the
   source.
5. **Should Jadpo support OpenFeature?** *Recommendation:* not now. Jadpo
   evaluates its own flags, so it needs no OpenFeature provider. Client
   flags go out on a plain generated `GET /flags`; adopt OpenFeature's remote
   evaluation protocol (OFREP) for that route once it is stable, which its
   authors say it is not yet.
6. **Are rate limits `rate_limit: { requests: within: by: }` on an API or
   route, counted in Redis with an in-memory fallback?** *Recommendation:*
   yes, keyed by caller or address, with built-in limits on the generated
   authentication routes. This is Rails 8's `rate_limit to: within: by:` in
   Jadpo's form.
7. **Are files a `File` field with a file store, direct uploads through a
   signed slot, and signed links in outputs?** *Recommendation:* yes, S3
   protocol only. It is how Rails Active Storage and Laravel work, and the
   compiler then applies policy, erasure and retention to the bytes.
   Image resizing and virus scanning wait for a real app.
   *Revised 2026-10-13 (capabilities, area 11 questions 19–24):* the store
   is a connection providing `files`: the built-in S3 driver, or a package
   that is a preset over it (R2's upload slots still need S3 keys, even on
   Workers); local disk in development and tests; `store.put(bytes, type)`
   for files the application makes.
8. **Should outgoing webhooks and field encryption be built in?**
   *Recommendation:* yes, both later. Outgoing webhooks are events with
   signed, retried delivery to a customer's address, which the compiler can
   type from the event and document; field encryption (Rails has
   `encrypts`) belongs in area 15, which knows sensitive fields. Neither is
   needed by the golden todo.
9. **Is the audit trail an opt-in `audit:` section, written in the same
   transaction, with personal fields hashed?** *Recommendation:* yes, opt-in
   per entity, history readable with `read` on the record. Rails and Django
   need a library (PaperTrail, django-simple-history) for this, and those
   cannot hash personal fields or know the request.
10. **Is OpenTelemetry generated for everything, configured by the standard
    variables?** *Recommendation:* yes, traces and metrics with no syntax and
    no values in spans. Phoenix generates telemetry for every request and
    query the same way; using the standard variables means any monitoring
    tool works.
11. **Is email built in as `type X = Mail`, emitted after commit and sent
    over SMTP?** *Recommendation:* yes, templates from `translations/`, SMTP
    only, provider APIs as integrations. This replaces the golden app's
    `ReminderMail` integration; Rails, Django and Laravel all ship mail with
    SMTP and a test inbox.
    *Revised 2026-10-13 (capabilities, area 11 questions 19–24):* email is
    no longer SMTP only. Sending goes through the `mail` capability: SMTP is
    one built-in driver and a mail package (Postmark, SES) another, chosen
    by `mail: { connection: }` with no code change; bounces and complaints
    come back as standard events; provider features are connection
    settings or the package's own operations. Laravel's mailers and Rails'
    delivery methods work the same way.
12. **Is search a `search:` section using PostgreSQL full-text search?**
    *Recommendation:* yes, one language per entity, with search services as
    integrations. Django's `SearchVector` and Laravel Scout's database engine
    do the same with more hand-written setup.
13. **Does every request carry a path ID?** *Recommendation:* yes, two:
    a Ball–Larus `path` number that decodes against the build's graph, and
    a `path_key` hash of stable identities for comparing builds; loops
    record which inner paths ran, recursion its first entry. Sentry groups
    errors by guessing from stacks; the compiler can name the path exactly.
    Needs control flow in the compiler's graph.
14. **Does the runtime decide how much to send per request?**
    *Recommendation:* yes: buffer each request, send the full trace on a
    crossed threshold, failure, fault or new path, otherwise metrics plus
    one sample per path per minute. This is tail sampling done in the
    process, which generic tools cannot do because they do not know what a
    new path is.
15. **Are thresholds declared in source, and are monitoring rules
    generated?** *Recommendation:* yes, one monitoring file per domain, so
    a reviewer sees the set; per-request thresholds act in the runtime,
    across-request ones become OpenSLO and Prometheus rules in
    `build/monitoring/` and in the build manifest. OpenSLO and Sloth keep
    SLOs as code beside the app, where a renamed route breaks them silently;
    here the compiler checks every threshold names something that exists.
16. **Does loosening a threshold need a person?** *Recommendation:* yes,
    an acceptance file and a line in the approval report; tightening does
    not. Otherwise an agent fixes a noisy alert by raising its limit.

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
| Flag declaration | | |
| Flag storage and changes | | |
| Flag targeting | | |
| Flag checks | | |
| OpenFeature | | |
| Rate limits | | |
| Files | | |
| Outgoing webhooks and field encryption | | |
| Audit trail | | |
| Tracing and metrics | | |
| Email | | |
| Search | | |
| Path IDs | | |
| Detail by threshold | | |
| Thresholds in source | | |
| Loosening thresholds | | |
| Local inbox in development | `jadpo dev` catches all mail: browser view in the dev console (rendered, locale, working links, trace link) and `jadpo mail list/show/links/open/clear` for agents (Richard) | 2026-10-13 |
| Mail health score | Deterministic content checks per mail in the dev inbox and `jadpo mail show`, optional SpamAssassin rules, `jadpo mail check-domain` for SPF/DKIM/DMARC; every score states it is an early indication, cannot predict inbox placement and must not be relied on (Richard) | 2026-10-13 |
