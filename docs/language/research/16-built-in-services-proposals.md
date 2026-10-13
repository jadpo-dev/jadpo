# Built-in services: research and proposals

Research behind [area 16](../16-built-in-services.md). It asks which common
backend features Jadpo should provide itself. A feature is native only when
the compiler gains something by knowing about it (a compile error, a
generated test, report, policy or erasure, or runtime work the author no
longer writes), not merely because it saves a library install.

## What frameworks ship

"Ships" means first-party: part of the framework, or an official package from
the same team.

| Feature | Rails | Django | Laravel | Phoenix | Supabase | Firebase |
|---|---|---|---|---|---|---|
| Feature flags | No (Flipper gem) | No (django-waffle) | Pennant | No (FunWithFlags) | No | Remote Config |
| Rate limiting | `rate_limit` (7.2+) | No (django-ratelimit) | `RateLimiter`, `throttle` | No (Hammer) | Auth endpoints only | App Check (abuse, not limits) |
| Files / object storage | Active Storage | Storage API (S3 via django-storages) | `Storage` (Flysystem, S3) | LiveView uploads only | Storage | Cloud Storage |
| Audit trail | No (PaperTrail, Audited) | No (django-simple-history) | No (laravel-auditing) | No | `supa_audit` extension | No |
| Telemetry | `ActiveSupport::Notifications` | No (OpenTelemetry contrib) | Pulse, Telescope | `:telemetry` + LiveDashboard, generated | Dashboard logs | Performance Monitoring |
| Email | Action Mailer | `django.core.mail` | Mail, Notifications | Swoosh, generated | Auth emails only | Trigger Email extension |
| Full-text search | No | `contrib.postgres.search` | Scout (database engine) | No | Postgres FTS | No (Algolia) |
| Realtime | Action Cable | Channels (separate project) | Reverb | Channels, LiveView | Realtime | Firestore listeners |
| Admin screens | No | Admin | No (Nova is paid) | LiveDashboard (ops only) | Studio | Console |
| Payments | No | No | Cashier | No | No | Stripe extension |

Sources:
[Laravel Pennant](https://laravel.com/docs/12.x/pennant),
[Rails `rate_limit`](https://www.rubydoc.info/docs/rails/8.0.1/ActionController/RateLimiting/ClassMethods),
[Rails 8 multiple limits](https://blog.saeloun.com/2024/11/25/rails-adds-ability-to-use-multiple-rate-limits-per-controller/),
[Firebase percentage rollouts](https://firebase.google.com/docs/remote-config/use-cases/percentage-rollouts),
[Firebase rollouts](https://firebase.google.com/docs/remote-config/rollouts/about),
[Active Storage](https://api.rubyonrails.org/classes/ActiveStorage.html),
[Laravel Scout](https://laravel.com/docs/12.x/scout),
[Django full-text search](https://docs.djangoproject.com/en/stable/ref/contrib/postgres/search/),
[Django email](https://docs.djangoproject.com/en/6.1/topics/email/),
[Phoenix telemetry](https://phoenix.hexdocs.pm/telemetry.html),
[Firebase Trigger Email](https://firebase.google.com/docs/extensions/official/firestore-send-email),
[supa_audit](https://database.dev/kiwicopple/supa_audit),
[PaperTrail](https://github.com/paper-trail-gem/paper_trail).
Rows without a link are from the frameworks' own documentation, known
rather than re-fetched for this note.

**What it shows.** Email and file storage are in almost every framework.
Rate limiting arrived in Rails and Laravel recently. Feature flags, audit
trails and search are usually third-party add-ons, so every team picks and
wires its own. Those are the gaps where a compiler that sees the whole
program gains most.

**What SaaS backends hand-roll.** From practitioner experience, an inference
rather than a survey: feature flags (or a paid service), rate limits on
sign-in and public forms, audit logs for business customers who ask "who
changed this?", direct-to-S3 uploads, transactional email, outgoing webhooks
for customers' integrations, per-plan quotas, and field encryption for
tokens and national ID numbers. Jadpo already covers idempotency keys (area
7), API keys (area 8), soft delete and purge (area 5), pagination (area 6)
and job operator tools (area 14).

## Feature flags

### What is hard

- **Flags never get removed.** Uber built
  [Piranha](https://www.uber.com/blog/piranha/) to delete stale flags
  automatically; it has removed about two thousand from Uber's mobile code.
  Martin Fowler's
  [feature toggles article](https://martinfowler.com/articles/feature-toggles.html)
  treats release toggles as short-lived and costly to keep, because every
  live flag doubles the paths to test.
- **Untested branches.** A flag at 0% hides code that may not work when
  switched on.
- **Stable percentages.** A user must keep their answer, and raising the
  percentage must keep existing users in. Unleash hashes the user id with
  MurmurHash into 0 to 100
  ([stickiness](https://docs.getunleash.io/concepts/stickiness)). Its
  [rollout puzzle post](https://www.getunleash.io/blog/hashing-it-right-solving-a-gradual-rollout-puzzle)
  shows the trap: reusing one hash for rollout and for variants made the
  variant split badly uneven, fixed by seeding each hash differently. Jadpo
  seeds with the flag's name, so each flag also picks a different 10%.

### Options

**A. Flags as configuration.** A `Bool` in `config.jadpo` per environment.
No new concept, but a change needs a restart and there is no per-user or
percentage targeting. This is what teams do before they adopt a flag tool;
it is not a flag system.

**B. Declared flags, stored in the app's database (recommended).** A root
`flags.jadpo` lists every flag with default, owner and expiry; the state
(per-user values, percentages) lives in a language-owned table and is
changed with `jadpo flags`. Pennant stores flags in the app database the same
way, but its flags are defined in PHP closures, so nothing lists them or
expires them. Jadpo adds the compile-time list, the both-values test rule and
the source rewrite on removal.

```jadpo fragment
flags {
    /// The one-page checkout.
    new_checkout: Bool {
        default: false
        owner: "payments"
        expires: "2027-01-31"
    }
}
```

**C. A third-party flag service through OpenFeature.**
[OpenFeature](https://openfeature.dev/specification/) is a vendor-neutral
API; a provider connects it to LaunchDarkly, Unleash, Flipt and others.
Jadpo would declare flags and let the service evaluate them. That makes
every request depend on another company for something small enough to own,
the reason Richard rejected Auth0 for OAuth (area 8).

**Recommendation: B.** For browsers and mobile apps, flags marked
`client: true` come from a generated `GET /flags`. OpenFeature's
[remote evaluation protocol](https://openfeature.dev/docs/reference/other-technologies/ofrep/)
(OFREP) standardises that kind of route so any OpenFeature client SDK can
read it, but its
[repository](https://github.com/open-feature/protocol) still warns of
breaking changes. Adopt its shape for `GET /flags` once it is stable.

### Where flags are read

Evaluation needs the caller. `current_principal` exists only in routes, so
flags take the caller from the runtime, as policy does, and fix the value for
the operation as `clock.now` is fixed. Schedules and subscribers have no
caller and get the everyone value. Considered and rejected: carrying the
request's flag values with each emitted event. A subscriber would then keep
using a flag after an operator switched it off in an incident, which defeats
a kill switch.

### The compiler's checks

| Check | How | Compile or runtime |
|---|---|---|
| Unknown flag | Name lookup | Compile error |
| Both values tested | Tests set `flags: { x: true }`; the call graph shows which tests reach each read | `jadpo check` error |
| Expired | `expires:` before today | Warning naming the owner |
| Fully rolled out | Flag state in the environment | `jadpo flags list` |
| Removal | `jadpo flags remove x --keep true` rewrites reads, branches and tests | Command |

Making the expiry an error was considered: it would break a build on the day
a hotfix is needed, so it stays a warning.

## Rate limits

Rails 8: `rate_limit to: 10, within: 3.minutes, by: -> { ... }, store: ...`,
by IP by default, answering 429. Laravel: named limiters with `by()` keys and
`Retry-After` headers ([guide](https://kinsta.com/blog/laravel-throttle/)).
Both rely on a cache store. Jadpo's form is the same three facts,
`rate_limit: { requests: within: by: }`, on an API or route. When Redis is
unreachable, each instance counts in memory: limits loosen by the number of
instances but requests keep working, matching area 5's cache rule. Jadpo adds
default limits on its generated authentication routes and lists limits in
OpenAPI, which neither framework does.

## Files

Active Storage and Laravel `Storage` both do direct uploads: the server signs
a short-lived URL, the client uploads to S3, then submits a token the server
attaches to the record. Supabase Storage checks access with row-level
security on its objects table. Jadpo proposes the Active Storage flow with
three additions the compiler makes possible:

- The `Output` that picks a `File` field sends a signed download link, so
  downloads are protected by the read's existing policy.
- The file's type is checked from its bytes before the write commits.
- Erasure, retention and export (area 15) reach the bytes, because the
  compiler knows which fields are files and whose records they are on.

Only the S3 protocol is supported: S3, R2, MinIO and Google Cloud Storage's
interoperability mode all speak it.

## Audit trail

PaperTrail and Audited (Rails), django-simple-history and laravel-auditing
write a version row from model callbacks and set "who" from a controller
hook. `supa_audit` uses database triggers and has no idea who the
application's user was. Jadpo writes entries itself in the action's
transaction, knows the principal and the request (and, for subscribers, the
request that emitted the event), hashes marked personal fields, and applies
erasure and retention. Opt-in per entity, because every audited write costs
an extra row; supa_audit advises against tables above about 3,000 writes a
second.

## Tracing and metrics

Phoenix generates a telemetry module that measures every request and Ecto
query. OpenTelemetry's semantic conventions name HTTP server spans by method
and route, and database spans by operation, with the query text kept out of
the name. Jadpo can go further than Phoenix because it knows every action,
integration call and event delivery, and can carry trace context through the
outbox so a subscriber's work joins the trace of the request that caused it.
Today's runtime generates an `operationalEventToOpenTelemetry` mapping that
nothing calls, with `traceId` always `null`.

## Email

Rails, Django and Laravel each ship a mailer with SMTP and a test inbox.
Firebase's Trigger Email extension writes a document that a function turns
into an SMTP message after the write. Jadpo's proposal combines the two:
a `Mail` is an event, emitted in an action and sent after commit with area
12's retries, and its text comes from `translations/` with placeholders
checked at compile time. SMTP only, because every provider (SES, Postmark,
Resend, Mailgun) accepts it; provider-specific features go through an area 11
integration.

## Search

Django's `SearchVector` and Laravel Scout's database engine both use
PostgreSQL's full-text search but leave the index, the weighting and the
migration to the author; Scout's database engine falls back to `LIKE`
without them. A `search:` section lets Jadpo generate the column, the GIN
index, the migration and the query condition from one declaration. Scout's
other engines (Algolia, Meilisearch, Typesense) show where the line is: typo
tolerance and facets need a search service, which is an integration.

## Considered and left out

- **Realtime subscriptions** (Action Cable, Phoenix Channels, Supabase
  Realtime) would gain from policy and from knowing every write, but need
  long-lived connections and policy over a stream of changes. Revisit after
  the golden todo.
- **Admin screens** (Django admin, Supabase Studio): Jadpo generates no
  interface. Operators use `jadpo` commands.
- **Payments** (Laravel Cashier): provider-specific, and money moves at the
  provider; an integration plus webhooks covers it.
- **Quotas**: a plan's limits are application data, checked in an action.
- **Outgoing webhooks and field encryption** pass the test and are proposed
  for later: webhooks as events delivered with signatures and retries; field
  encryption (Rails' `encrypts`) inside area 15.
