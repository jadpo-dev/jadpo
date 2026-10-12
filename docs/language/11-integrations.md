---
title: External services
area: 11
status: in-review
checked-with: jadpo 3ce094d0 (2026-10-12)
rules:
  - semantic.service_atomic_effect
  - semantic.service_call_context
  - semantic.service_configuration_missing
  - semantic.service_contract_invalid
  - semantic.service_egress_invalid
  - semantic.service_idempotency_invalid
  - semantic.service_import_digest_mismatch
  - semantic.service_import_not_found
  - semantic.service_import_pin_invalid
  - semantic.service_secret_sink_invalid
---

# External services

How a Jadpo program talks to third-party APIs: declaring an integration, its
credentials, time limits and retries, turning provider errors into the app's
failures, idempotency keys, when a call may happen, where the app may connect,
provider API versions, the provider's contract file, inbound webhooks and
fakes for tests.

> In review. Code outside the collapsed boxes applies the decisions from areas 1 to 5 and this page's recommendations; each recommendation names its open question. Today's compiled syntax is collapsed beneath each section that changes.

The rule behind all of it: **code calls a typed operation; the compiler owns
the HTTP request, the secret, the destination, the retries and the mapping of
what came back.** Application code never builds a URL, sets a header, reads a
credential or sees a raw provider response.

Today the compiler accepts exactly one integration: the golden app's
`ReminderMail`, compared line by line with a fixed copy. Everything below
describes the general version that replaces it.

## Declaring an integration

An integration is one third-party API at one version. It lives in its
domain's `integrations/` folder, with the provider's contract file beside it
(area 1). It names the contract, the address, the credential, the time limits
and the operations the app uses.

```jadpo fragment
// domains/planning/integrations/reminder_mail.jadpo
integration ReminderMail {
    contract: "reminder_mail.contract.json"
    base_url: "https://mail.example.invalid"
    credential: {
        scheme: bearer
        token: config.mail_api_key
    }
    timeout: 5s
    retry: {
        attempts: 3
        within: 30s
    }
    operations: {
        send_reminder: {
            endpoint: POST /v1/messages
            input: ReminderMessage
            output: ReminderReceipt
            idempotency: {
                header: "Idempotency-Key"
            }
            failures: {
                invalid_recipient: ReminderRecipientRejected
                rate_limited: ReminderDelayed
                timed_out: ReminderDelayed
            }
        }
    }
}

type ReminderMessage = Object {
    from: Email
    to: Email
    todo_title: Todo.title
    due_at: Todo.due_at
}

type ReminderReceipt = Object {
    accepted_at: Instant
}
```

- Every member follows the colon rule (area 3): settings are `name: value`,
  operations are named blocks inside `operations:`.
- `input` and `output` are ordinary types you write, usually in the same
  file. The compiler checks each one against the contract: every field must
  exist there with a compatible type. An output may leave out fields you do
  not use; they are dropped. Generating them instead is
  [option B in the proposals](research/11-integrations-proposals.md)
  (question 1).
- Only the operations listed exist. A provider with 400 endpoints costs
  nothing until you list one.

::: today

The keyword is `service`, an operation is a run of flat lines, and settings
use one-off grammar: `equals`, `exponential(...)` and `maps ... -> fault`.
Faults are listed in the caller's `fails`. The compiler compares every line
with a fixed list, so this is the only declaration that compiles:

```jadpo project=today-declare file=types.jadpo hidden
config TestConfiguration {
    mail_api_key: Text {
        binding: "MAIL_API_KEY"
        secret: true
    }
}

type ReminderIntentId = Uuid
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

value ReminderMessage {
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}

value ReminderReceipt {
    accepted_at: Instant
}

failure ReminderRecipientRejected {
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}

failure ReminderTemporarilyUnavailable {
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}
```

```jadpo project=today-declare file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=today-declare
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}

action deliver(mail: ReminderMessage)
    fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
    -> ReminderReceipt
{
    return attempt ReminderMail.send_overdue_reminder(mail)
}
```

:::

## Calling an operation

An operation is called through its integration's name, like an entity's
generated operations (`Todo.get`): the dot names the owner (area 1). It can
fail, so it needs `attempt` or a `match`, and its mapped failures go in the
caller's `fails`. Only actions may call one (question 2).

```jadpo fragment
// domains/planning/actions/send_reminder_email.jadpo
action send_reminder_email(todo_id: Todo.id) fails TodoNotFound, ReminderRecipientRejected, ReminderDelayed -> Unit {
    var todo: Todo = attempt Todo.get(todo_id)
    var message: ReminderMessage = reminder_message_for(todo)
    var receipt: ReminderReceipt = attempt ReminderMail.send_reminder(message)
    record_reminder_sent(todo.id, receipt.accepted_at)    // writes after the call (question 7)
}
```

::: today

Called as `ReminderMail.send_overdue_reminder(mail)` (see the box above). A
call from a function or query is `semantic.service_call_context`. The caller
must list `Misconfigured`, `Unavailable` and `OutcomeUnknown`, which area 4
removes.

:::

## Credentials

The credential is a `secret: true` setting from `config.jadpo` (area 10). The
compiler puts it in the request itself; code can never read it, and it never
reaches a URL, a body, a log or a fake's record (question 10).

| `scheme:` | Sends | Settings |
|---|---|---|
| `bearer` | `Authorization: Bearer <token>` | `token:` |
| `header` | the key in a named header | `name: "X-Api-Key"`, `token:` |
| `basic` | `Authorization: Basic` (Twilio's account SID and token) | `username:`, `password:` |

::: today

Only `scheme: bearer` with `header: "Authorization"` and
`secret: config.mail_api_key`; the setting must be called exactly
`mail_api_key` and be secret.

:::

## Time limits and retries

`timeout:` limits one attempt (default 10s). `retry:` is the budget for one
call: at most `attempts` tries, all finished `within` the time given
(default 3 attempts within 30s). Waits between attempts grow exponentially
with random jitter; that is fixed, not a setting (question 8).

The runtime retries only when a repeat is safe: the request never left, the
provider said it did nothing (`rate_limited`, honouring `Retry-After`), or
the operation has an idempotency key. A call also stops when the route's
deadline passes (area 7), so the budget must fit inside it; the compiler
warns when `timeout` times `attempts` exceeds the deadline of a route that
reaches the call.

::: today

Required and fixed at `timeout: 5s` and
`retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)`.

:::

## Provider failures

An operation's `failures:` block maps what went wrong to the app's own
failures, in area 4's single form `condition: Failure`. The conditions are the
provider's error responses named in the contract (`invalid_recipient`,
`rate_limited`) and one the runtime adds, `timed_out` (question 5).

| Condition | Comes from | May map to |
|---|---|---|
| A provider error (`invalid_recipient`) | The contract's error responses | Any failure you may declare |
| `timed_out` | No answer after every attempt | An `unavailable` failure, and only on an operation with an idempotency key |
| Anything not mapped | | A fault, never in `fails`: `misconfigured` for a rejected credential, `unavailable` when the provider surely did nothing, `outcome_unknown` when it may have acted |

`timed_out` needs an idempotency key because a request that timed out may
already have taken effect. With a key, retrying later with the same key is
safe, so callers can treat it as "try again". Without one, the honest answer
is `outcome_unknown`, which area 12 parks for a person to check.

```jadpo fragment
failures: {
    card_declined: PaymentDeclined
    rate_limited: PaymentDelayed
    timed_out: PaymentDelayed
}
```

::: today

Each mapping is a line, `maps provider.rate_limited -> ReminderTemporarilyUnavailable`
or `maps transport.possible_dispatch_timeout -> fault OutcomeUnknown`, and all
ten are required exactly as written.

:::

## Idempotency keys

A provider that supports idempotency keys ignores a repeated request with the
same key. The operation says where the key goes; the runtime supplies it
(question 6):

- in a subscriber or schedule, the delivery's key (event and subscriber,
  area 12), so every retry of that delivery sends the same key;
- in a route, the client's `Idempotency-Key` header if it sent one, otherwise
  the request id.

```jadpo fragment
idempotency: {
    header: "Idempotency-Key"     // or field: idempotency_key, for a key in the body
}
```

A new reason to call needs a new event: moving a todo's due date emits a new
`ReminderDue`, which gets its own key.

::: today

`idempotency_key: ReminderIntentId` names a type that must be a field of the
input, plus two sentence clauses, `request_key: input.idempotency_key equals
header "Idempotency-Key"` and `receipt_key: response header ... equals
request_key`. The golden app sends `todo.id`, which the service plan's review
showed cannot send a second reminder after a date change.

:::

## When a call may happen

A request to a provider cannot be part of a database transaction: it cannot be
rolled back. So in an action, a call that changes something comes before any
write; the writes after it commit in their own transaction once the call has
returned (question 7). An action makes at most one such call (area 12).

Work that must follow a write, such as charging for an order just placed,
emits an event; a subscriber's action makes the call after the order has
committed (area 12):

```jadpo fragment
// domains/planning/subscribers/send_reminder.jadpo
subscriber send_reminder {
    on: ReminderDue
    run: send_reminder_email(event.todo_id)
}
```

If the write after the call fails, the delivery is retried and the call
repeats with the same key, so the provider does nothing new and returns the
same result.

::: today

Any write in the same action, before or after the call, is
`semantic.service_atomic_effect`, so today's action cannot record the
receipt at all.

:::

## Where the app may connect

The app may connect only to the host in `base_url`, over HTTPS on port 443;
the compiler derives this from `base_url`, so nothing restates it. Redirects
and proxies are always refused, and code cannot choose a host, path or port
(question 9). A provider with a separate sandbox host lists both, and
configuration picks one:

```jadpo fragment
base_url: {
    live: "https://api-m.paypal.com"
    sandbox: "https://api-m.sandbox.paypal.com"
    use: config.paypal_environment
}
```

::: today

`egress: mail.example.invalid:443` restates the host unquoted, and
`proxy: prohibited` and `redirects: prohibited` are required settings with one
legal value.

:::

## Provider versions

Each provider API version is its own integration, with its own contract file,
named with the version: `StripeV2024` in `stripe_v2024.jadpo` beside
`stripe_v2024.contract.json` (question 3). A provider that versions by header
names the header, and the value comes from the contract's `info.version`:

```jadpo fragment
// domains/payments/integrations/stripe_v2024.jadpo
integration StripeV2024 {
    contract: "stripe_v2024.contract.json"
    base_url: "https://api.stripe.com"
    version_header: "Stripe-Version"
    ...
}
```

Upgrading is adding the next version beside the old one:

1. Add `StripeV2025` with the new contract (`jadpo integration add`,
   area 14). The compiler checks every listed type against it and lists what
   changed.
2. Move callers across one at a time; both versions run side by side. A type
   that fits both contracts can be shared.
3. Delete `StripeV2024` once the compiler reports nothing calls it (area 1).

Stripe signs and shapes webhook events in the version set on the endpoint, so
each webhook route names the integration version that reads it.

## The contract file

The contract is the provider's OpenAPI document (3.0 or 3.1, as JSON), kept in
the repository and never fetched during a build (question 4). The compiler
checks only the operations listed, so Stripe's full specification is fine,
and an unsupported OpenAPI feature is an error only inside an operation you
use. Changing the contract is reviewed like a policy change: its digest goes
in the approval record (area 7), so the source carries no hash.

::: today

`import { file: ... version: ... sha256: ... }` reuses the module-import
word, and the file must be exactly
`tests/assurance/service-reference-mail-v0.1.json` with one fixed digest.

:::

## Inbound webhooks

A webhook is an ordinary route in area 7's `webhooks` API. The provider's
signature check is declared on the integration and used as the route's
authentication (areas 7 and 8, question 11). The check runs over the raw body
before anything is parsed, and rejects old timestamps.

```jadpo fragment
// in domains/payments/integrations/stripe_v2024.jadpo
webhooks: {
    signature: stripe
    secret: config.stripe_webhook_secret
}

// domains/payments/routes/webhooks_stripe.jadpo
POST / {
    authentication: StripeV2024.signature
    input: StripeEvent
    status: no_content
    run: record_stripe_event(input)
}
```

Built-in schemes: `stripe`, `standard_webhooks` (the shared format used by
Svix, Twilio and others) and `hmac_sha256` with a named header for the rest.
The action stores the event under a unique rule on its id, so a redelivery
does nothing, and real work runs in a subscriber (area 12).

Today nothing supports webhooks.

## Fakes in tests

Tests never reach a provider: every reachable operation needs a fake listing
its responses, each an output value or a condition from the operation's
`failures:` block, such as `rate_limited`. Responses still pass through the
real retries and mapping. The syntax is area 13's.

## Errors

Today's rules check the one fixed declaration. They carry over to the general
compiler, with wording that no longer names the mail provider. Every example
below is today's syntax.

### `semantic.service_import_pin_invalid`

The contract import loses its digest:

```jadpo project=err-pin expect=semantic.service_import_pin_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_import_not_found`

The declaration is exact, but the contract file is missing from the project:

```jadpo project=err-not-found expect=semantic.service_import_not_found
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_import_digest_mismatch`

The contract file exists but its bytes have changed (here it holds `{}`):

```jadpo project=err-digest file=tests/assurance/service-reference-mail-v0.1.json hidden
{}
```

```jadpo project=err-digest expect=semantic.service_import_digest_mismatch
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_egress_invalid`

The allowed host no longer matches `base_url`:

```jadpo project=err-egress expect=semantic.service_egress_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.attacker.example:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_secret_sink_invalid`

The credential moves to another header:

```jadpo project=err-secret expect=semantic.service_secret_sink_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "X-Api-Key"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_idempotency_invalid`

The idempotency key is a plain `Uuid` instead of the named intent type:

```jadpo project=err-idempotency expect=semantic.service_idempotency_invalid
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: Uuid
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_contract_invalid`

Any other difference, here only the name, is outside the checked slice:

```jadpo project=err-contract expect=semantic.service_contract_invalid
service StripeV2024 {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_configuration_missing`

The project has no `config` declaration for the credential:

```jadpo project=err-config file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=err-config expect=semantic.service_configuration_missing
service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

### `semantic.service_call_context`

A function calls the operation:

```jadpo project=err-context file=types.jadpo hidden
config TestConfiguration {
    mail_api_key: Text {
        binding: "MAIL_API_KEY"
        secret: true
    }
}

type ReminderIntentId = Uuid
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

value ReminderMessage {
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}

value ReminderReceipt {
    accepted_at: Instant
}

failure ReminderRecipientRejected {
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}

failure ReminderTemporarilyUnavailable {
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}

service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

```jadpo project=err-context file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=err-context expect=semantic.service_call_context
function deliver(mail: ReminderMessage) -> ReminderReceipt {
    return attempt ReminderMail.send_overdue_reminder(mail)
}
```

### `semantic.service_atomic_effect`

An action calls the operation and then writes the receipt:

```jadpo project=err-atomic file=types.jadpo hidden
config TestConfiguration {
    mail_api_key: Text {
        binding: "MAIL_API_KEY"
        secret: true
    }
}

type ReminderIntentId = Uuid
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

value ReminderMessage {
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}

value ReminderReceipt {
    accepted_at: Instant
}

failure ReminderRecipientRejected {
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}

failure ReminderTemporarilyUnavailable {
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}

service ReminderMail {
    import {
        file: "tests/assurance/service-reference-mail-v0.1.json"
        version: "0.1.0"
        sha256: "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d"
    }
    credential {
        scheme: bearer
        header: "Authorization"
        secret: config.mail_api_key
    }
    base_url: "https://mail.example.invalid"
    egress: mail.example.invalid:443
    proxy: prohibited
    redirects: prohibited
    timeout: 5s
    retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)

    operation send_overdue_reminder
    POST /v1/messages
    idempotency_key: ReminderIntentId
    input ReminderMessage
    output ReminderReceipt
    request_key: input.idempotency_key equals header "Idempotency-Key"
    receipt_key: response header "Idempotency-Key" equals request_key
    maps provider.invalid_recipient -> ReminderRecipientRejected
    maps provider.rate_limited -> ReminderTemporarilyUnavailable
    maps provider.authentication -> fault Misconfigured
    maps provider.idempotency_conflict -> fault Misconfigured
    maps transport.pre_dispatch_refusal -> fault Unavailable
    maps transport.pre_dispatch_timeout -> fault Unavailable
    maps transport.possible_dispatch_timeout -> fault OutcomeUnknown
    maps transport.possible_dispatch_loss -> fault OutcomeUnknown
    maps transport.invalid_acknowledgement -> fault OutcomeUnknown
    maps transport.unexpected_response -> fault OutcomeUnknown
}
```

```jadpo project=err-atomic file=tests/assurance/service-reference-mail-v0.1.json hidden
{
  "openapi": "3.1.0",
  "info": {
    "title": "Jadpo local reference mail provider",
    "version": "0.1.0"
  },
  "servers": [
    { "url": "https://mail.example.invalid" }
  ],
  "paths": {
    "/v1/messages": {
      "post": {
        "operationId": "send_overdue_reminder",
        "parameters": [
          {
            "name": "Idempotency-Key",
            "in": "header",
            "required": true,
            "schema": { "type": "string", "format": "uuid" }
          }
        ],
        "security": [ { "providerCredential": [] } ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["idempotency_key", "from", "to", "todo_title", "due_at"],
                "properties": {
                  "idempotency_key": { "type": "string", "format": "uuid" },
                  "from": { "type": "string", "format": "email" },
                  "to": { "type": "string", "format": "email" },
                  "todo_title": { "type": "string", "minLength": 1, "maxLength": 200 },
                  "due_at": { "type": ["string", "null"], "format": "date-time" }
                }
              }
            }
          }
        },
        "responses": {
          "202": {
            "description": "Accepted or deduplicated by the same key and payload",
            "headers": {
              "Idempotency-Key": {
                "required": true,
                "schema": { "type": "string", "format": "uuid" }
              }
            },
            "content": {
              "application/json": {
                "schema": {
                  "type": "object",
                  "additionalProperties": false,
                  "required": ["accepted_at"],
                  "properties": {
                    "accepted_at": { "type": "string", "format": "date-time" }
                  }
                }
              }
            }
          },
          "400": { "$ref": "#/components/responses/InvalidRecipient" },
          "401": { "$ref": "#/components/responses/Authentication" },
          "409": { "$ref": "#/components/responses/IdempotencyConflict" },
          "429": { "$ref": "#/components/responses/RateLimited" }
        }
      }
    }
  },
  "components": {
    "securitySchemes": {
      "providerCredential": { "type": "http", "scheme": "bearer" }
    },
    "responses": {
      "InvalidRecipient": {
        "description": "Recipient rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/InvalidRecipient" } } }
      },
      "Authentication": {
        "description": "Credential rejected before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Authentication" } } }
      },
      "IdempotencyConflict": {
        "description": "Key reused with a different payload; no new acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/IdempotencyConflict" } } }
      },
      "RateLimited": {
        "description": "Request refused before acceptance",
        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/RateLimited" } } }
      }
    },
    "schemas": {
      "InvalidRecipient": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "invalid_recipient" } }
      },
      "Authentication": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "authentication" } }
      },
      "IdempotencyConflict": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "idempotency_conflict" } }
      },
      "RateLimited": {
        "type": "object", "additionalProperties": false, "required": ["code"],
        "properties": { "code": { "const": "rate_limited" } }
      }
    }
  }
}
```

```jadpo project=err-atomic expect=semantic.service_atomic_effect
entity Reminder {
    id: Uuid { generated: identity }
    accepted_at: Instant

    identity: id

    persistence {
        store: primary
        role: authority
    }

    action deliver(mail: ReminderMessage)
        fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
        -> ReminderReceipt
    {
        var receipt = attempt ReminderMail.send_overdue_reminder(mail)
        var saved = attempt create Reminder {
            accepted_at: receipt.accepted_at
        }
        return receipt
    }
}
```

## Review

### Status

Each row says how it was checked: compiled with `jadpo check`, built with
`jadpo build`, or read from the compiler source.

| Feature | Status |
|---|---|
| Declaring | **Gap:** one hard-coded declaration. `checked_service_effect` compares every line with a fixed list of 29 and the name with `ReminderMail` (read); renaming it, or adding a second service, is `semantic.service_contract_invalid` (compiled). The operation, method, path, types, host and limits in the result are constants, not read from the source (read). |
| Contract | Must be the fixed path and digest; a file outside the project is refused (compiled). The hand-written input and output types are checked field by field against the contract (read). |
| Calls | A function caller and any write in the same action are rejected (compiled); an action that only calls passes (compiled). |
| Generated adapter | `jadpo build` writes `build/target/service-adapter.ts` and `build/audit/services.json` (built). The service plan records 19 passing loopback HTTP tests; not rerun here. |
| Failures | Callers must list the faults `Misconfigured`, `Unavailable` and `OutcomeUnknown` (compiled), which area 4 forbids. |
| Credential | **Gap:** the setting must be named `mail_api_key` (read). |
| Messages | **Bug:** every rule's advice names the mail provider ("Declare the mail API key", "Use ReminderIntentId"), whatever the service (read from the catalogue). |
| Webhooks, versions, other schemes | None. |
| Fakes | Work (area 13). |

### Consistency with the rest of the language

- **Grammar used nowhere else** (syntax review #60–64): flat operation
  lines, `equals` for `==`, call-shaped `exponential(...)`, `import` for a
  contract, `maps ... -> fault`. The proposed form uses settings blocks only.
- **Faults in `fails`.** Today's callers list three faults; area 4 decided
  `fails` holds domain failures only.
- **`secret` means a fourth thing** in `credential { secret: }`; area 10
  keeps it for the config flag, so the credential setting becomes `token:`.
- **The policy file restates the service.** The golden `policy.jadpo` has an
  `external_effect ReminderMail.send_overdue_reminder` block repeating the
  secret, the key and the caller (area 9).
- **Area 4's `timed_out: ChargeUnavailable`** against the service plan's
  rule that a timeout after sending is `outcome_unknown`: question 5
  reconciles them.

### Carried to later areas

| Item | Area | Status |
|---|---|---|
| Who may call an integration, replacing the `external_effect` policy block | 9 | Input to area 9 |
| A webhook route's `authentication: StripeV2024.signature`, raw-body access, the `webhooks` API | 7, 8 | Input to areas 7 and 8 |
| Integration time limits inside a route's deadline: the warning in "Time limits and retries" | 7 | Answered here |
| The delivery key as the idempotency key; one changing call per action; `outcome_unknown` parked | 12 | Answered here, agreeing with area 12 |
| Fake syntax and `.requests`; fakes name the conditions in `failures:` | 13 | Input to area 13 |
| `jadpo integration add` and `update`, the contract diff, the approval record entry, adapter conformance runs, `runtime.outcome_unknown` | 14 | Input to area 14 |
| Readiness does not probe integrations, as area 7 proposes; a provider outage should not take the app out of service | 7 | Answered here |
| Magic links and OIDC code exchange (area 8) are ordinary integrations; no special feature | 8 | Answered here |
| Mail templates and translations for message bodies; the body is an ordinary field today | 10 | Deferred |

### Open questions

1. **How are request and response types written?** Today they are written
   by hand and checked against the contract. *Recommendation:* keep that,
   generalised: a type may leave out output fields it does not use, and one
   type can serve two provider versions if it fits both. Generating types
   from the contract (OpenAPI Generator, Ballerina's `bal openapi`) brings in
   hundreds of fields nobody reviews. See
   [the proposals](research/11-integrations-proposals.md).
2. **How is an integration named and called?** *Recommendation:*
   UpperCamelCase with the version in the name (`StripeV2024`), called
   `StripeV2024.create_payment_intent(...)` like `Todo.get`, only from
   actions. Encore's typed service clients read the same way.
3. **How are provider versions handled?** *Recommendation:* one integration
   per version with its own contract, both runnable at once, upgraded by
   adding the next version and moving callers; `version_header:` sends the
   contract's version. This is Stripe's own advice: pin the version, test the
   new one beside it.
4. **Does the source keep `version:` and `sha256:`?** *Recommendation:* no:
   `contract: "file"` only. The file is in the repository, and its digest
   goes in the approval record, so a contract change is still reviewed.
   `semantic.service_import_digest_mismatch` becomes an approval-record
   check.
5. **Which conditions can be mapped, and to what?** *Recommendation:* the
   contract's error responses to any declarable failure, plus `timed_out` to
   an `unavailable` failure only on operations with an idempotency key;
   anything unmapped is a fault. Temporal marks errors non-retryable the same
   way.
6. **Who supplies the idempotency key?** *Recommendation:* the runtime: the
   delivery key in subscribers and schedules, the client's key or request id
   in routes; the operation only says where it goes. Today's
   `ReminderIntentId` and its two clauses go.
7. **May one action call a provider and also write?** Today no write at all
   is allowed. *Recommendation:* yes, if the call comes first; the writes
   commit after it returns, and a failed commit retries the call with the
   same key. Writes before the call stay an error; that case emits an event.
8. **Which retry settings exist?** *Recommendation:* `timeout:` (default
   10s) and `retry: { attempts: within: }` (default 3 within 30s), the same
   words as area 12's subscriber retry; exponential backoff with full jitter
   fixed. Temporal exposes five knobs, mostly left at defaults.
9. **How is the outbound host controlled?** *Recommendation:* derived from
   `base_url`, HTTPS only, no redirects or proxies ever; a sandbox host is a
   second listed URL chosen by configuration. `egress:`, `proxy:` and
   `redirects:` go.
10. **Which credential schemes first?** *Recommendation:* `bearer`,
    `header` and `basic` (Twilio), each with `token:` or
    `username:`/`password:`. OAuth client credentials and request signing
    (AWS) wait for a real app that needs them.
11. **Where is a webhook's signature check declared?** *Recommendation:* on
    the integration (`webhooks: { signature: stripe  secret: ... }`), used by
    the route as `authentication: StripeV2024.signature`, with `stripe`,
    `standard_webhooks` and `hmac_sha256` built in and a five-minute
    timestamp tolerance, Stripe's library default.

### Decisions

Nothing is decided on this page yet.

| Topic | Decision | Date |
|---|---|---|
| Keyword and folder | Carried from area 1: `integration` in `integrations/`, one declaration per provider version, contract beside it | 2026-10-12 |
| Mapping form | Carried from area 4: `condition: Failure`, e.g. `timed_out: ChargeUnavailable`; time limits are integration settings | 2026-10-12 |
| Faults | Carried from area 4: faults are never listed in `fails`; integrations map provider problems to declared failures | 2026-10-12 |
| Calls | Carried from areas 1 and 3: the dot names the owner; `attempt` only on calls that can fail; actions have no settings | 2026-10-12 |
| Colon rule and options | Carried from areas 2 and 3: members are `name: value`; closed options are lower_snake_case names | 2026-10-12 |
| Types and fields | | |
| Naming and calls | | |
| Provider versions | | |
| Contract pin | | |
| Failure conditions | | |
| Idempotency keys | | |
| Call then write | | |
| Retry settings | | |
| Outbound host | | |
| Credential schemes | | |
| Webhook signatures | | |
