---
title: HTTP, design notes
area: 7
---

# HTTP: design notes

Why [HTTP](../07-http.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Consistency with the rest of the language

- **`routes` is plural** while other keywords are singular and their
  folders plural (`schedule` in `schedules/`). It names a group, so it is
  kept, and the folder `routes/` still matches it.
- **The default API is `main`**, like the default store, and not `public`,
  which already means "visible to other domains".

## Why some rules are the way they are

Moved here from the area page when it was rewritten to the writing guide.

- **A route entry, not a named route.** A route is an entry
  `GET /path { }` inside its group, keyed by method and path, which is how
  HTTP, OpenAPI, logs and every framework name a route; it is the colon
  rule's one exception (area 3). A named form
  (`route get_todo { method: ... path: ... }`) would add a third name to
  keep in step with the action and the URL. Routes are never `public`
  (area 1); who may call one is authentication's and policy's job (areas 8
  and 9).
- **A new version is another API.** Stripe's dated versions with response
  rewriting suit a large public API and are not worth their cost here.
- **`Idempotency-Key`** is the header of the IETF draft Stripe's design
  became.
- **Field paths are dotted.** A problem names its field as
  `lines.2.quantity`, the names the OpenAPI schema uses; JSON Pointer's
  `/lines/2/quantity` reads worse beside a form field.
- **The rate-limit headers** follow the IETF draft
  `draft-ietf-httpapi-ratelimit-headers`; its current version is still to
  check.
- **The public API record** catches a breaking change at review, not by a
  client in production, as Kotlin's API validator and Microsoft's API
  Extractor catch one.
- **Streaming** is server-sent events because area 21 chose them over
  WebSockets.

## Carried to later areas

| Item | Area |
|---|---|
| Route authentication: `authentication: RouteAuthentication.none` and `.fresh`, each API's default, how the caller is passed (`current_principal`) | 8 |
| Webhook signature checking over the raw body, declared with the provider's integration | 8, 11 |
| Policy on routes and the `not_permitted` answer | 9 |
| A root folder for records and acceptances (`api/`, `acceptances/`) in area 1's fixed root list | 1 |
| Database drops and store moves accepted with the shared `acceptance` form | 5 |
| Policy weakening and new domains accepted with the shared form, enforced through the approval protocol | 9, 1 |
| Integration timeouts inside a route's deadline | 11 |
| Background work after `status: SuccessStatus.accepted`; webhook events processed by subscribers | 12 |
| Calling routes in tests | 13 |
| `toolchain.target_liveness_not_local`, raised only by `jadpo build`; the `jadpo api update` command and CI's record check | 14 |
| `precondition_failed` for `If-Match` (from area 4); until then a concurrent change answers 409 through area 5's check (deferred) | 7 |
| Response headers: `Location` for created records, caching, `ETag` (deferred) | 7 |
| Per-route rate limits (`rate_limited`) (deferred) | 7 |

## Decisions

| Topic | Decision |
|---|---|
| Route groups | Carried from area 1: named route groups with a prefix the compiler checks, one group per file in the domain's `routes/` folder, named after the group |
| Triggers | Carried from area 1: routes call actions; routes are never `public` |
| Documentation comments | Carried from area 1: `///` comments feed OpenAPI |
| Route types | Carried from area 2: routes accept `Input` and return `Output` |
| Colon rule | Carried from area 3: every route item is `name: value`; a route entry, keyed by method and path, is the one exception |
| Failure responses | Carried from area 4: one envelope with `code`, `message`, `request_id` and client `details`; status from the kind; no status mapping on routes; `x-request-id` header |
| Invalid input | Carried from area 4: each problem as `{ field, rule }`, never the value; area 7 sets the field format |
| Shared acceptance | Carried from areas 1 and 5: new domains, policy changes and database drops need an explicit acceptance; area 7 designs the shared mechanism |
| Route groups and prefix check | `routes Todos { prefix: ... }`; file name from the prefix's fixed segments; prefixes may hold typed placeholders; every URL matches exactly one group |
| One input and typed placeholders | Placeholders typed in the path; one `input:` whose source follows the method; headers as `header:` on `Input` fields |
| `status:` and checked `output:` | `status: SuccessStatus.x`; `output:` must be an `Output` and exactly the action's result type, required unless `no_content` |
| What `run:` calls | A `GET` calls a named query, every other method a named action; no inline route actions; a `GET` also runs `sessions.tokens` and `<Service>.keys` (area 8) |
| Deadline and body limit | 10-second default deadline, `deadline:` up to 60 seconds; 1 MB body limit answering 413 |
| Health | Generated only: `/health/live` and `/health/ready`, readiness from the declared stores |
| APIs and versions | `apis: { }` in `application`, `main` at `/` implied; a group names its API with `api:`; one OpenAPI document and record per API; a new major version is another API |
| Webhooks | Provider webhooks are generated from the package and arrive as events (area 11): `POST /webhooks/<connection>` in the `webhooks` API, the provider's signature checked over the raw body, repeats dropped by the provider's event id, the work in a subscriber; no route is written |
| Field path format | The `Input` field's dotted path with list positions as numbers (`lines.2.quantity`) |
| API record and acceptance | Committed readable record per API (`api/<api>.jadpo_record`) compared by `jadpo check`; breaking changes need a person-written acceptance file, one per decision (area 18), the same form for drops, policy weakening and new domains; `acceptances/` owned in CODEOWNERS |
| Write revision header | `Jadpo-Revision` header on every write, a cookie for browsers; sending it back skips caches |
| Generated authentication routes in OpenAPI | Listed like any route, marked generated; every route shows its security scheme |
| Accepted authentication methods per API | `authentication: [...]` in each API's entry in `apis: { }`; groups and routes may only narrow it |
| Route URL shown | Hover shows a route's full method and path; `jadpo routes` lists every route with path, file, API, authentication and what it runs |
| Route test request and `Response` | Owned by area 13; this page links to it |
| Route declaration form | `GET /path { }` entries inside a `routes` group, keyed by method and path; the one exception to the colon rule |
| Idempotent writes | Every non-`GET` route honours `Idempotency-Key`, handled entirely by the runtime: stored with the response in the action's transaction for 24 hours, scoped to the caller; reuse with a different request is 422, in flight is 409; `unavailable` and internal failures not stored; `idempotency: Idempotency.required` per API |
| Expired idempotency keys | A lookup ignores a key older than 24 hours; the runtime deletes expired keys in batches from its own minute tick (area 12), so the table holds about a day of keys |
| Route readings | `output:` may also be a `List` or `Page` of an `Output`, as a query returns them; a placeholder or `input` a route never passes to what it runs is an error (`route.binding_unused`); a header field may not read `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` or `X-Jadpo-*`; besides a type's own rules, input problems name `required`, `unknown_field`, `type` (the wrong JSON kind), `variant` and `duplicate`, and text that isn't a UUID, an instant or a date breaks `format`; an `Int` is a JSON number only when its declared range has both ends within JavaScript's safe integers, a string otherwise, as area 2 decides, and a number is accepted for it either way |
| Runtime answers | Unmatched paths answer 404 `route_not_found`, a known path with another method 405 `method_not_allowed` with `Allow`; defects answer 500 `internal_fault` with "The server failed to answer the request."; a declared failure without a `message:` gets its kind's message; `request_id` is `req_` and 16 hexadecimal digits, on every response as `x-request-id` |
| OpenAPI and the record | The record is the wire contract: routes in path order, then each `input`, `output`, object and enum by name with each field's wire type and rules, then each failure code with its status, message and client fields; it holds no version, and an OpenAPI document's version is the first 12 hex digits of the record's SHA-256. An OpenAPI document's title is the project folder's name until `app.jadpo` names the application. A record out of date fails `jadpo check` but doesn't stop `jadpo test` or `jadpo dev` |
| Default rate limits | A signed-in caller's requests count against its own allowance, 600 a minute (area 8's Decisions); every other request counts against its address (the client's, as area 10's `client_address:` chooses it: the connection's own unless the application trusts a proxy's header): 120 a minute, in memory per process within 16 MB, the oldest counters dropped first; the native host sends each address's connections to one worker thread, so no lock is taken. The `jadpo check` warning about per-instance limits comes with the Redis store, since until then nothing could silence it |
| Request guarantees | A body over 1 MB answers 413 before routing; the host stops holding it there, reading and dropping up to 16 MB more so the client reads the answer. A route's deadline counts from when it is matched: when it passes, the running work is dropped with its open transaction, which the store rolls back, and a claimed key is given up; an action's commit runs after its body, outside the deadline, so a request that committed never answers 504, and its message is "The request ran out of time, and nothing it did was kept." A `GET` never takes part, whatever headers it sends |
| Health details | `/health/live` answers 200 `{"status":"live"}`; `/health/ready` runs `SELECT 1` on each PostgreSQL store an entity lives in, within two seconds in all, and answers 200 `{"status":"ready","stores":{"main":"ready"},"integrations":{}}`, or 503 with `not_ready` and each store that didn't answer as `unavailable`; provider connections appear by name as `unchecked`. Both answer `HEAD` and `OPTIONS`, any other method 405, with `Cache-Control: no-store`; they are answered before the rate limits, so probes are never refused; a route on either path is `route.path_overlap`. Each worker (natively a worker thread, on Workers an isolate) reuses its last readiness answer for a second, so probes cost a store at most one `SELECT 1` a second per worker |
| Idempotency keys | `Idempotency.optional` (the default) and `Idempotency.required`, which an API's record shows as `idempotency: required` and OpenAPI as a required header on every write; any other value is `api.idempotency_invalid`. A key is 1 to 255 printable ASCII characters, else 400 `invalid_request`, as is a missing one where it is required. A signed-in caller's keys are kept under `user <id>`, and a route anyone may call keeps `anonymous`. The request's digest covers its route and what it says, decoded, every secret value left out ("How failures reach the client"). A key is claimed in one statement of its own before the action runs, with no response and kept only for the route's deadline and five seconds, so a request that never finished leaves a claim that lapses; the response (status, the body exactly as sent and its code) is written over it in the action's transaction, or in one statement of its own for a failure the action returned, kept 24 hours from the request. A repeat answers with the stored response under its own `x-request-id`. Keys go in the store the action writes. An action that writes no store but emits keeps its keys beside its events, in the store `events: { store: }` names (area 12), and one that makes a changing call keeps them there too, beside the record of its steps, as OpenAPI says; one that does none of these keeps none, since running it again changes nothing, and its calls' keys are still derived from the client's key, never the request's id |
| `jadpo dev` | Listens on 127.0.0.1, port 3000 unless `--port`; the database is the one `--url` names, else each store's `url:` setting (area 10), else a disposable PostgreSQL for the session, and pending migrations are applied when it starts and after each save that adds one |
| Waiting for follow-on work | `settle: { answer: <query>(result...) within: <duration> }`: after commit the runtime delivers the action's events and their follow-ons at once and waits until each has succeeded, failed for good or been parked, then answers with the query and the route's status; past `within:` the same query answers 202; `output:` is the query's result; a repeat with the same `Idempotency-Key` reruns the query |
| Route authentication | A route's `authentication:` takes `RouteAuthentication.none` or `.fresh`; anything else is `route.authentication_invalid`, which offers both without an edit, since one opens the route. An API's `authentication:` is a list of `AuthenticationMethod`s `authentication.jadpo` declares, each once, else `api.authentication_invalid`; an API that names none accepts every declared method. Area 8's Decisions hold the rest |
| Failure wording for clients | Every failure a route can produce is worded in every supported locale in the app's language files; `jadpo build` writes `<api>.failures.<locale>.json` beside each OpenAPI document; dates and numbers stay values for the client to format; `message` stays English (area 10) |
| Generated routes | Endpoints only, never pages; `path:` where the feature is configured or `generate_routes: false` over generated operations that keep every protection; webhook routes change path but cannot be switched off; response shapes fixed |
| Rate-limited response | `429`, code `rate_limited`, `Retry-After` and the IETF `RateLimit` and `RateLimit-Policy` headers, wording from translations |
| Serving OpenAPI | Every API serves its OpenAPI document at `<prefix>/openapi.json` under its own default authentication, `serve_openapi: false` to turn it off; the route is listed, marked generated, in its own OpenAPI document, its API record and `jadpo routes` (area 11) |
| `x-jadpo` fields | Jadpo's OpenAPI documents carry `x-jadpo-changes`, `x-jadpo-idempotency`, `x-jadpo-failure` (`code`, `kind`), `x-jadpo-constraints` and `x-jadpo-events`; other tools ignore them; area 11's importer reads them to make an exact package (area 11) |
| Serving OpenAPI and its fields | The served route needs a signed-in caller by any method its API accepts, unless the API accepts no built method, when anyone may read it; it answers with `build/openapi/<api>.json`'s bytes and refuses input; the record lists it as `generated openapi`. `x-jadpo-changes` is `true` for a route that runs an action; `x-jadpo-idempotency` gives `header`, `required`, `keeps`, `window`, `max_length`, `in_use` and `reused`; `x-jadpo-failure` is on each declared failure's schema; `x-jadpo-constraints` is a field's type as the record writes it; each operation is tagged with its group. `x-jadpo-events` waits for publishing events outward. The document's title is the project folder's name, held by the project itself so the plan can serve it |
| API record name | `api/<api>.jadpo_record`: readable, starting with the line that says what wrote it, a hand edit an error (area 18) |
| Webhook senders without a package | A signing system with no package gets the app's own integration in `shared/providers/`; a partner with an agreement calls an ordinary route as a `Service` with a service key (area 11) |
| Request reading | A body is read only with a JSON `Content-Type` (`application/json` or `application/…+json`, any parameters, any case), else 400 `invalid_request` "A request body is JSON, sent with `Content-Type: application/json`."; a header value that isn't printable ASCII (spaces and tabs allowed) is 400 `invalid_request` "A header's value isn't printable ASCII.", whichever header it is, so a credential or key is never dropped; a route that reads no query string answers each key it gets as `{ field: <key>, rule: unknown_field }`, and one that reads no body answers a body that isn't blank as `{ field: "", rule: unknown_field }`, 422 `invalid_input`, before anything runs; a page's cursor that isn't one of its query's is 422 `invalid_input` with `{ field: <its path in the input>, rule: format }` |
| Deadlines on work that never waits | The runtime reads the host's monotonic clock against the route's deadline: the interpreter checks it every 4,096 steps and stops the body there, and any body that finishes past the deadline, stopped or not, is answered 504 `deadline_exceeded` with nothing committed and its claimed key given up. Each of those checks also gives the worker thread back, so the other requests it serves carry on while such a body computes |
| A concurrent change at commit | A change another transaction made, found as the action commits, runs the whole action again, as area 6 says, within the same three attempts and the route's deadline, then 503 `unavailable`; a claim that lapsed and was taken before the commit is `unavailable` too, with nothing kept |
| OpenAPI responses | Every route lists 400, 413, 422, 429, 500 and 504, since any request can be unreadable, too large or carry unknown input; 503 `unavailable` where what it runs uses a store; 403 where policy can refuse its action or one it calls; a write's 409 and 422 for a key only where a key is claimed, and its `Idempotency-Key` described by what is kept: the response, nothing for a response that starts or ends a session or carries a new credential, or nothing at all for an action that writes no store. A mixed enum's plain variant is its name as a string constant. There is no time of day (area 10); a `ZonedTime` is an object of its `instant`, a `date-time` string, and its `zone`, an IANA name, both required and nothing else; a `Zone` is sent as its IANA name (`Europe/London`) and a `Locale` as its BCP 47 tag (`en-GB`), and OpenAPI lists them so; a `Duration` is ISO 8601 in JSON, query strings, paths and headers alike |
| Sign-in methods in the record | Each API's record starts with `authentication:` and the methods it accepts, its own list or every declared one, when `authentication.jadpo` declares any; a route that selects one kind of caller says `caller: user` or `caller: service` |
| Native request limits | A client has 10 seconds to send a request's headers and 30 its body, else the connection is closed without an answer; the server holds at most 1,024 connections, closing one past that as it is accepted; each worker opens at most 10 connections per store, a transaction past that waiting for one, inside the route's deadline. Configuration doesn't set them |
| `Idempotency-Key` as input | An `Input` field may not read `Idempotency-Key`, which the runtime reads before anything runs: `route.header_credential`, "`Idempotency-Key` belongs to the runtime" |
| A map layer as output | Besides an `Output`, or a `List` or `Page` of one, a route may send a `FeatureCollection<G, P>` (area 20) bare, because mapping libraries fetch a layer as RFC 7946 GeoJSON; its properties type `P` is held to what an `Output` may hold, so only fields declared for clients leave the server. `route.output_not_output`'s reason names it |
| Streaming a model's answer | A route whose action returns an `AiStream` (area 21) names `output: AiStream` and sends it as server-sent events, `text/event-stream` with `Cache-Control: no-store`, once the action has committed and the first piece has come: each `piece` event `{"text": ...}` as it is read, then `done` (`{}`), or `error` with the failure envelope, `unavailable` (`internal_fault` for a defect), for anything that stops it part-way; one `event:` and one `data:` line of compact JSON per event; no `id:`, no `retry:`, `Last-Event-ID` not read, so a stream can't be resumed; a failure before the first piece answers as any route's does. The stream ends by the call's `timeout:` and the route's deadline, whichever is first, and a client that goes away closes the provider's connection. Only `SuccessStatus.ok` and no `settle:` (`route.stream_invalid`); the response is never kept for an `Idempotency-Key` repeat; OpenAPI's 200 is `text/event-stream` with each event's data in `x-jadpo-stream`; the record says `output AiStream (server-sent events)`; a route test's `Response` has `pieces` in place of `output`. The standard shape, since area 21 chose server-sent events |
