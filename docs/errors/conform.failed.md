# `conform.failed`

A package's answers to `jadpo package conform` don't fit the capability it fills, or the suite couldn't run.

## Why Jadpo refuses it

`jadpo package conform <package> ai` runs the `ai` capability's
conformance suite through the package's `provide ai` mapping: a typed
answer, a refusal, a cut-off, a stream, an embedding and the usage
reported. Without `--live` it answers from the package's recordings,
served by a local fake, so nothing leaves the machine. A case that fails
means the app would misread what the provider sends, such as taking an
answer the model stopped short as whole. So the suite fails, says which
case and why, and a package isn't published until it passes.

## What it looks like

An app asking its models through its own provider package, Acme
Models, whose mapping says how a refusal looks but not a cut-off:

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/package.jadpo hidden
/// Acme's models, asked as the `ai` capability.
package AcmeModels.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/provider.jadpo hidden
/// Acme's models API, each request carrying the account's key in
/// `x-api-key`. The connection gives its address in `host:`.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.header
    credential_header: "x-api-key"
}
```

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/resources/messages.jadpo hidden
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

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/provides/ai.jadpo hidden
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
        refused_when: {
            path: "stop_reason"
            equals: "refusal"
        }
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

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/types/message_param.jadpo hidden
/// One turn of the conversation: who says it, and what.
public type MessageParam = Object {
    role: Text {
        max_length: 20
    }
    content: Text
}
```

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/types/message_request.jadpo hidden
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

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/types/message_response.jadpo hidden
/// The model's answer, and the tokens it took.
public type MessageResponse = Object {
    text: Text
    stop_reason: Text
    usage: MessageUsage
}
```

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/types/message_usage.jadpo hidden
/// The tokens a message took.
public type MessageUsage = Object {
    input_tokens: TokenCount
    output_tokens: TokenCount
}
```

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/types/model_context.jadpo hidden
/// A model's context window, in tokens.
type ModelContext = Object {
    model: Text
    tokens: TokenCount
}
```

```jadpo check project=e-conform-failed file=shared/providers/acme_models/v1/libraries/models.jadpo hidden
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

```jadpo check project=e-conform-failed file=domains/support/types/greeting.jadpo hidden
/// What the support page says first.
type Greeting = Text {
    max_length: 100
}
```

```jadpo check project=e-conform-failed file=domains/support/actions/greet.jadpo hidden
use support/types { Greeting }

/// The support page's greeting.
action greet() -> Greeting {
    return "Hello"
}
```

config.jadpo:

```jadpo check project=e-conform-failed file=config.jadpo
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

```jadpo check project=e-conform-failed file=app.jadpo migrate=initial
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

The package's recording of `messages.create` is in its `recordings/`
folder, which the local fake answers from:

```sh run project=e-conform-failed expect=conform.failed
mkdir -p shared/providers/acme_models/v1/recordings
cat > shared/providers/acme_models/v1/recordings/messages.create.json <<'EOF2'
{
  "stop_reason": "{Text}",
  "text": "{Text}",
  "usage": {
    "input_tokens": "{Int}",
    "output_tokens": "{Int}"
  }
}
EOF2
jadpo package conform acme_models ai
```

```text output
error: `AcmeModels.v1`'s cut-off doesn't conform to `ai`
  reason: The mapping says nothing of a cut-off, so an answer stopped at `max_output:` would be read as whole.
  next: Correct `provides/ai.jadpo` against the provider's documentation and the package's recordings, then run it again (agent)
  rule: conform.failed
  help: https://jadpo.dev/docs/errors/conform.failed

Conformance suite
  typed answer  passed                        decodes as ConformanceAnswer
  refusal       passed                        refused, for its mark
  cut-off       failed                        no cut_off_when:
  stream        skipped                       AcmeModels.v1 maps no stream:
  embedding     skipped                       AcmeModels.v1 maps no embed:
  usage         passed                        12 in, 7 out, 0 cached

ai doesn't conform: 1 failed, 3 passed, 2 skipped, its recordings and a local fake
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A case without a recording to answer from is refused the same way, and
its `next:` line says to record it against the provider's sandbox; a
suite that couldn't start, for lack of memory or of a free loopback
port, says what to free.

## How to fix it

**If the provider marks a cut-off in its answer**, as Acme's
`stop_reason` does, say how in the mapping:

```jadpo fragment
provide ai {
    generate: {
        ...
        refused_when: {
            path: "stop_reason"
            equals: "refusal"
        }
        cut_off_when: {
            path: "stop_reason"
            equals: "max_tokens"
        }
        ...
    }
    ...
}
```

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/provides/ai.jadpo hidden
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
        refused_when: {
            path: "stop_reason"
            equals: "refusal"
        }
        cut_off_when: {
            path: "stop_reason"
            equals: "max_tokens"
        }
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

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/package.jadpo hidden
/// Acme's models, asked as the `ai` capability.
package AcmeModels.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
```

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/provider.jadpo hidden
/// Acme's models API, each request carrying the account's key in
/// `x-api-key`. The connection gives its address in `host:`.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.header
    credential_header: "x-api-key"
}
```

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/resources/messages.jadpo hidden
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

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/types/message_param.jadpo hidden
/// One turn of the conversation: who says it, and what.
public type MessageParam = Object {
    role: Text {
        max_length: 20
    }
    content: Text
}
```

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/types/message_request.jadpo hidden
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

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/types/message_response.jadpo hidden
/// The model's answer, and the tokens it took.
public type MessageResponse = Object {
    text: Text
    stop_reason: Text
    usage: MessageUsage
}
```

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/types/message_usage.jadpo hidden
/// The tokens a message took.
public type MessageUsage = Object {
    input_tokens: TokenCount
    output_tokens: TokenCount
}
```

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/types/model_context.jadpo hidden
/// A model's context window, in tokens.
type ModelContext = Object {
    model: Text
    tokens: TokenCount
}
```

```jadpo check project=e-conform-fixed file=shared/providers/acme_models/v1/libraries/models.jadpo hidden
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

```jadpo check project=e-conform-fixed file=domains/support/types/greeting.jadpo hidden
/// What the support page says first.
type Greeting = Text {
    max_length: 100
}
```

```jadpo check project=e-conform-fixed file=domains/support/actions/greet.jadpo hidden
use support/types { Greeting }

/// The support page's greeting.
action greet() -> Greeting {
    return "Hello"
}
```

```jadpo check project=e-conform-fixed file=config.jadpo hidden
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

```jadpo check project=e-conform-fixed file=app.jadpo hidden migrate=initial
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

The suite's cut-off case then passes, and an answer the model stopped at
`max_output:` is read as cut off rather than whole.

## Related

- [`jadpo package conform`](../language/14-tooling.md#jadpo-package-conform)
- [The conformance suite](../language/21-ai.md#the-conformance-suite)
