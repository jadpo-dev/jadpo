# External services: declaring integrations simply and generally

**Status:** proposals for [area 11](../11-integrations.md), 2026-10-12.
Decides nothing.

## What is complicated today

The compiler accepts one integration, the golden app's `ReminderMail`, by
comparing its 29 lines with a fixed list. Behind that sit real guarantees,
frozen in the [service plan](../../service-plan.md): a pinned contract, a
secret that only reaches one header, a fixed host, bounded retries, a stable
idempotency key, and a split between "the provider surely did nothing" and
"it may have acted". The declaration that carries them is the least
consistent in the language: flat operation lines, `equals`,
`exponential(...)`, `maps transport.possible_dispatch_loss -> fault OutcomeUnknown`
(ten such lines), and faults listed in callers' `fails`.

Making it general raises four questions: where request and response types
come from, how provider versions coexist, how retries and keys are declared,
and how much of the transport detail an author writes at all.

## What others do

| Tool | Declaring a provider | Types | Versions | Retries and keys |
|---|---|---|---|---|
| OpenAPI Generator | Generated client classes from a spec | Every schema becomes a model | Regenerate from a new spec | Left to the caller |
| Stripe SDKs | A library per language | Hand-maintained, per release | Each SDK release pins an API version; override per request; webhook endpoints have their own version | Built-in retries with idempotency keys |
| Ballerina | `bal openapi --mode client` generates a client, records and utilities; can limit to listed operations | Generated records | Regenerate | Client configuration (retry and timeout records) |
| Encore | Typed calls between services, `import { hello } from "~encore/clients"` | The callee's own types | Not covered | Not covered |
| Temporal | An activity wraps each outside call | Your own | Not covered | Retry policy per activity; activities must tolerate repeats |

Sources: [Stripe versioning](https://docs.stripe.com/api/versioning),
[Stripe idempotent requests](https://docs.stripe.com/api/idempotent_requests),
[Stripe webhooks](https://docs.stripe.com/webhooks),
[OpenAPI Generator usage](https://openapi-generator.tech/docs/usage/),
[Ballerina OpenAPI tool](https://ballerina.io/learn/openapi-tool/),
[Encore API calls](https://encore.dev/docs/ts/primitives/api-calls),
[Temporal retry policies](https://docs.temporal.io/encyclopedia/retry-policies),
[Standard Webhooks](https://www.standardwebhooks.com/).

What they agree on:

1. **Pin the version and test the next one beside it.** Stripe's typed SDKs
   (Java, Go, .NET) fix the API version per SDK release; the docs advise
   testing a new version before upgrading the account.
2. **Idempotency keys belong to the logical operation, not the attempt.**
   Stripe keeps a key for at least 24 hours and rejects the same key with
   different parameters; Temporal asks activities to tolerate repeats.
3. **Generated clients are large.** Ballerina added `--operations` to
   generate only what is used, because whole-spec generation is unwieldy.
4. **Retry policy is mostly defaults.** Temporal has five settings and its
   docs recommend bounding total time rather than counting attempts.
5. **Webhooks are verified over the raw body with a timestamp tolerance**
   (Stripe: five minutes by default), and handlers dedupe by event id.

## Option A: hand-written types checked against the contract (recommended)

The integration lists the operations used; you write their input and output
types; the compiler checks each field against the pinned contract.

```jadpo fragment
integration StripeV2024 {
    contract: "stripe_v2024.contract.json"
    base_url: "https://api.stripe.com"
    version_header: "Stripe-Version"
    credential: {
        scheme: bearer
        token: config.stripe_secret_key
    }
    operations: {
        create_payment_intent: {
            endpoint: POST /v1/payment_intents
            input: NewPaymentIntent
            output: PaymentIntent
            idempotency: {
                header: "Idempotency-Key"
            }
            failures: {
                card_declined: PaymentDeclined
                timed_out: PaymentDelayed
            }
        }
    }
}

type PaymentIntent = Object {
    id: PaymentIntentId
    status: PaymentIntentStatus
    amount: Pence
}
```

- An output type may leave out fields (Stripe's payment intent has dozens);
  unknown fields are dropped. An input type may leave out optional fields.
- A type that fits two contracts can serve `StripeV2024` and `StripeV2025`,
  so an upgrade touches only what changed.
- Upgrading re-checks the same types against the new contract and lists each
  mismatch.

**For:** explicit for reviewers; small; works with huge contracts; today's
compiler already checks field parity this way. **Against:** someone writes
the types (cheap for an agent); a field missing from the type is silently
unavailable until added.

## Option B: types generated from the contract

The compiler generates named types for each listed operation's request and
response from the contract, scoped to the integration:
`StripeV2024.PaymentIntent`.

```jadpo fragment
var intent: StripeV2024.PaymentIntent = attempt StripeV2024.create_payment_intent(request)
```

**For:** nothing to write; always exactly the contract. **Against:** reviewers
see types they never wrote, with hundreds of fields; every version has its own
copies, so an upgrade changes every signature that mentions them; generated
names collide with area 1's rule that a name finds exactly one file.

## Option C: a generated client per provider, no declaration

Like OpenAPI Generator or an SDK: every operation in the contract becomes
callable, with settings in `config.jadpo`.

**For:** least to write. **Against:** the compiler can no longer say which
operations the app uses, so failure mapping, egress review and fakes have
nothing to attach to; the whole of a provider's contract becomes the app's surface.

## Retries, keys and transport detail

The same in all options, because they are runtime behaviour, not authoring:

- `timeout:` per attempt and `retry: { attempts: within: }` for the call,
  matching area 12's subscriber `retry:`. Exponential backoff with full
  jitter is fixed.
- The runtime retries only when a repeat is safe: nothing was sent, the
  provider guaranteed no effect, or the operation has an idempotency key.
- The key comes from the runtime (the subscriber's delivery key, or the
  route request's), so an author cannot derive it from an attempt by mistake.
  The operation only says where it goes (`header:` or `field:`).
- The ten `maps transport.* -> fault` lines disappear: those outcomes are
  what "unmapped" means, and the runtime classifies them as the service plan
  already specifies.
- Egress comes from `base_url`; redirects and proxies are always refused.

## Trade-offs

| | A: checked hand-written types | B: generated types | C: generated client |
|---|---|---|---|
| Lines an author writes | Medium | Low | Lowest |
| What a reviewer reads | Only fields used | Everything in the contract | Nothing per operation |
| Two versions at once | Share fitting types | Separate copies | Separate clients |
| Failure mapping and fakes | Per listed operation | Per listed operation | No anchor |
| Change from today | Generalises today's parity check | New generator | New model |

## Recommendation

Option A. It keeps the guarantees the service plan froze, removes all of
today's one-off grammar, and suits code written by agents and reviewed by
people: the types say exactly which provider fields the app depends on, and
an upgrade reports precisely which of them changed. Option B's generator can
be added later as a `jadpo integration types` command that writes Option A
types for you, without changing the language.
