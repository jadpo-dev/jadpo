# `conform.refused`

The conformance suite wasn't sent to the provider's live API, because the app has no connection to the package, nothing sets the connection's credential or address, or `ai:` doesn't ask its models through it.

## Why Jadpo refuses it

`jadpo package conform <package> ai --live` sends the `ai` capability's
cases to the provider's own API, so a package is proved against what the
provider answers today rather than against its recordings. It sends them
through the app's own connection to the package, with that connection's
credential and address, and asks the models the app's `ai:` names, so
the run proves what the app itself will send.

Without those, the only thing it could send is a stand-in key to a
guessed address, and a failure would say nothing about the package.
Jadpo won't set a credential for you: the key decides which account at
the provider pays for the calls, and it is a secret only you hold.

## What it looks like

An app that asks its models through its own provider package, Acme
Models, whose key `ACME_KEY` gives:

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/package.jadpo hidden
/// Acme's models, asked as the `ai` capability.
package AcmeModels.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/provider.jadpo hidden
/// Acme's models API, each request carrying the account's key in
/// `x-api-key`. The connection gives its address in `host:`.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.header
    credential_header: "x-api-key"
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/resources/messages.jadpo hidden
public resource messages {
    create: {
        method: HttpMethod.post
        path: "/v1/messages"
        input: MessageRequest
        output: MessageResponse
        changes: false
        source: PackageSource.written
    }
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/provides/ai.jadpo hidden
/// Acme fills `ai` with `generate`: a message, the answer its text.
provide ai {
    generate: {
        call: messages.create
        input: {
            model: request.model
            max_tokens: request.max_output
            system: request.instructions
            messages: {
                each: request.messages
                as: {
                    role: item.role
                    content: item.content
                }
            }
        }
        answer: response.text
        usage: {
            input: response.usage.input_tokens
            output: response.usage.output_tokens
        }
        schema: {
            mode: AiSchema.json_only
            at: "response_format"
        }
    }
    models: {
        context: models.context_windows
    }
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/types/message_param.jadpo hidden
/// One turn of the conversation: who says it, and what.
public type MessageParam = Object {
    role: Text {
        max_length: 20
    }
    content: Text
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/types/message_request.jadpo hidden
/// What asking a model for a message takes.
public type MessageRequest = Object {
    model: Text {
        max_length: 200
    }
    max_tokens: Int {
        min: 1
        max: 1000000
    }
    system: Text
    messages: List<MessageParam>
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/types/message_response.jadpo hidden
/// The model's answer, and the tokens it took.
public type MessageResponse = Object {
    text: Text
    usage: MessageUsage
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/types/message_usage.jadpo hidden
/// The tokens a message took.
public type MessageUsage = Object {
    input_tokens: TokenCount
    output_tokens: TokenCount
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/types/model_context.jadpo hidden
/// A model's context window, in tokens.
type ModelContext = Object {
    model: Text
    tokens: TokenCount
}
```

```jadpo check project=e-conform-refused file=shared/providers/acme_models/v1/libraries/models.jadpo hidden
/// The models Acme serves.
library models {
    /// Each model's context window, in tokens.
    constant context_windows: List<ModelContext> = [
        ModelContext {
            model: "acme-small-1"
            tokens: 200000
        }
    ]
}
```

```jadpo check project=e-conform-refused file=domains/support/types/greeting.jadpo hidden
/// What the support page says first.
type Greeting = Text {
    max_length: 100
}
```

```jadpo check project=e-conform-refused file=domains/support/actions/greet.jadpo hidden
use support/types { Greeting }

/// The support page's greeting.
action greet() -> Greeting {
    return "Hello"
}
```

config.jadpo:

```jadpo check project=e-conform-refused file=config.jadpo
config {
    acme_host: Url {
        default: "https://api.acme-models.example"
    }
    acme_key: Text {
        secret: true
    }
    drafting_model: ModelId {
        default: "acme/acme-small-1"
    }
}
```

app.jadpo:

```jadpo check project=e-conform-refused file=app.jadpo migrate=initial
application {
    connections: {
        acme: {
            package: AcmeModels.v1
            domains: [support]
            credential: config.acme_key
            host: config.acme_host
        }
    }
    ai: {
        connection: acme
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

Nothing on this machine sets `ACME_KEY`, so the live run is refused
before anything is sent:

```sh run project=e-conform-refused expect=conform.refused
jadpo package conform acme_models ai --live
```

```text output
error: `acme` has nothing set for the provider's API
  reason: `--live` sends the suite to the provider's API with `acme`'s credential and address, and nothing sets `ACME_KEY`, so only a stand-in would be sent.
  next: Set `ACME_KEY` in `.env.local` to the provider's, then run it again (human)
  rule: conform.refused
  help: https://jadpo.dev/docs/errors/conform.refused
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides which of your accounts at Acme the suite's calls are made with.

## How to fix it

The question is whether you meant to call Acme's API.

**If you did**, set the key, then run the suite again:

```sh
jadpo config set acme_key
jadpo package conform acme_models ai --live
```

`jadpo config set` asks for the key without showing what you type and
saves it in `.env.local`, which is never committed. The suite's cases
then go to `https://api.acme-models.example` with it.

**If you didn't**, run the suite without `--live`:

```sh
jadpo package conform acme_models ai
```

It then runs against the package's recordings and a local fake, and
nothing leaves your machine.

## Related

- [`jadpo package conform`](../language/14-tooling.md#jadpo-package-conform)
- [The conformance suite](../language/21-ai.md#the-conformance-suite)
- [`jadpo config set` and `jadpo config check`](../language/14-tooling.md#jadpo-config-set-and-jadpo-config-check)
