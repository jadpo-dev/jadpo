# External services, rethought

Status: proposal, 2026-10-13. It replaces the integration design on the
[area 11 page](../11-integrations.md), questions 1–5 and 9–12 included, if
Richard agrees. Questions 6–8 survive.

## What was wrong with the first design

An integration restated what the provider's contract already says: the
operation list, the method and path, the request and response types, the
idempotency header and the credential scheme. Every app wiring up Stripe
repeated the same work, and a person reviewed it as if it were the app's own
logic. The contract was also the wrong unit for the compiler. It read the
provider's OpenAPI file on every build, so cost grew with the provider, not
with what the app uses.

Sizes, checked on 2026-10-13 by `Content-Length` of the raw files:

| File | Size |
|---|---|
| Stripe `openapi/spec3.json` | 8.3 MB |
| AWS S3 Smithy model | 3.0 MB |
| AWS EC2 Smithy model | 6.7 MB |

AWS publishes one model per service, about 400 of them
([AWS API models](https://aws.amazon.com/blogs/aws/introducing-aws-api-models-and-publicly-available-resources-for-aws-api-definitions)).
Parsing an 8 MB file takes milliseconds. The real cost comes afterwards, on
every build and every keystroke in the editor: resolving thousands of
schemas, checking them, and generating code.

OpenAPI is also not tight enough as *the* contract. It has no standard place
to say:

- which operations change something (`POST` does not tell you);
- whether a call takes an idempotency key, and where;
- how a provider names its errors (Stripe puts the code in the body, not the
  status);
- pagination style or rate-limit headers;
- how webhooks are signed.

Specifications are often wrong, too. And AWS is not OpenAPI at all: it is
Smithy, with its own protocols and its own request signing.

## The idea: providers are packages, like libraries

A **provider package** is one external API at one version, described once
and used by every Jadpo app. It is to Stripe what a TypeScript `.d.ts` file is
to a JavaScript library, or crate metadata is to a Rust library.

- **Written in Jadpo.** A package is Jadpo source: types, operations,
  failures and events, one file per resource, plus an index. The compiler
  parses only Jadpo, and agents and people read packages like any other code.
- **Richer than OpenAPI.** It records what the provider's own specification
  leaves out:
  - which operations change something;
  - idempotency;
  - error codes, as failure types with a kind;
  - pagination;
  - rate limits;
  - webhook events and their signature scheme;
  - the sandbox address.
- **Imported, not hand-written.** `jadpo provider import openapi <file>` or
  `import smithy <file>` produces a package once, offline. An agent then fills
  the gaps (marking idempotent operations, naming errors) and a person reviews
  that diff. Corrections to a wrong specification live in the package, so they
  survive the next import: the importer merges, the way an
  [OpenAPI Overlay](https://spec.openapis.org/overlay/v1.0.0) does.
- **Where they come from.**
  - Jadpo's own registry of curated packages: Stripe, AWS (one package per
    service, as AWS's own SDK splits), Postmark or Resend, Twilio, Slack and
    GitHub. These are pinned in `jadpo.lock` with a digest, kept in a local
    cache, and never fetched during a build.
  - Anything else is imported into the repository's `providers/` folder and
    committed.
- **Protocol-neutral.** The package says how its calls travel: JSON over
  HTTP from OpenAPI, AWS's protocols and signing from Smithy, and GraphQL or
  gRPC later. OpenAPI becomes one input, not the contract.

## Compile time

The compiler reads the package index (names to files) and loads only the
resources the app calls. Each package is checked once per digest and the
result cached, so an unchanged package costs nothing on later builds. An app
using S3 and SES loads two small packages and a handful of files from each.
Cost scales with what you use.

## What an app writes

Connections are infrastructure, declared once in `app.jadpo` beside the
stores:

```text
providers: {
    stripe: {
        package: Stripe.v2024_06_20
        credential: config.stripe_secret_key
        environment: config.stripe_environment     // ProviderEnvironment.sandbox or .live
        webhooks: { secret: config.stripe_webhook_secret }
    }
    email: {
        package: Postmark.v1
        credential: config.postmark_token
    }
}
```

Calls read like the provider's own SDK, which agents already know from
training:

```text
action take_payment(order: Order) fails PaymentDeclined -> Order {
    var intent: Stripe.PaymentIntent = attempt stripe.payment_intents.create({
        amount: order.total
        currency: Currency.gbp
    }) {
        card_declined: PaymentDeclined
    }
    return attempt Order.update(order) { set: { payment_intent_id: intent.id } }
}
```

- **Types** come from the package (`Stripe.PaymentIntent`), and hover and
  `jadpo inspect` show them. The checker compares what you send with what the
  operation takes. At runtime a response is validated only for the fields your
  code reads.
- **Idempotency, retries, timeouts, rate limits and pagination** come from
  the package and the runtime. A provider entry may override `timeout:` and
  `retry:`.
- **Failures.** Rate limits, 5xx errors and timeouts get the standard kinds
  (area 12's retry table). The package's business failures
  (`Stripe.CardDeclined`) map to the app's own failures at the call, in the
  form area 4 decided.
- **Areas 11 and 12 rules stay:** calls only from actions, the changing call
  before the writes, and one changing call per action.

## Webhooks become events

A package declares the provider's events and how they are signed. Giving the
connection a webhook secret makes the runtime:

1. generate `POST /webhooks/stripe`;
2. verify the signature;
3. drop redeliveries by the provider's event id;
4. publish each event as an ordinary Jadpo event.

```text
subscriber payment_succeeded {
    on: Stripe.events.PaymentIntentSucceeded
    run: mark_order_paid(event)
}
```

No webhook route is written by hand. This replaces area 7 question 8 and area
11 question 11.

## Tests and review

- **Fakes.** Every package comes with a fake. In `jadpo test` a provider
  answers from contract-valid defaults, and a test scripts a response or a
  failure for one call. `jadpo test --kind contract` runs the same cases
  against the provider's sandbox in scheduled CI.
- **Review.** The approval report lists, for each change:
  - new operations called;
  - new data sent to a provider, with personal fields flagged (area 15);
  - package upgrades, as a diff of the operations the app uses.

## Your own services

Every Jadpo API (area 7) publishes a provider package from its API record. App
A calls app B as `orders.orders.create(...)`, with B's types, idempotency and
events, for free. Internal services and third parties work the same way.

## Costs and risks

- **A registry is real work.** Curating and maintaining packages for the top
  providers is ongoing effort. Start with three or four, kept in the Jadpo
  repository, and the importer for everything else.
- **Enrichment needs review.** An agent marking operations idempotent can be
  wrong. A wrong mark is caught by the sandbox contract run, not the compiler.
- **SDK-shaped names depend on the importer.** Names follow the provider's
  SDK only where the importer can map them (Stripe and AWS publish the data).
  Otherwise they come from `operationId`, renamed in the package.
- **Area 1 changes.** Integrations move out of each domain's `integrations/`
  folder into `app.jadpo` and `providers/`. Which domains may call a provider
  becomes visible in the approval report rather than in folders.

## Decisions this would reopen

| Area | Question | Change |
|---|---|---|
| 1 | Integrations folder | Connections in `app.jadpo`, imported packages in root `providers/` |
| 7 | Q8 webhooks as routes | Generated from the package |
| 11 | Q1 hand-written types | Types from the package |
| 11 | Q2 naming | `stripe.payment_intents.create`, package names `Stripe.v2024_06_20` |
| 11 | Q3 versions | A package version; two versions are two connections |
| 11 | Q4 contract pin | `jadpo.lock` digest, or the committed package |
| 11 | Q5 mapping | Business failures from the package, mapped at the call |
| 11 | Q9 host, Q10 credentials, Q11 signatures | From the package |
| 11 | Q12 fakes | Generated per package |
