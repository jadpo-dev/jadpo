# `ai.connection_required`

An action asks a model, but the application names no `ai:` connection to ask it through.

## Why Jadpo refuses it

Every `ai` call goes through the one connection `application { ai: }`
names, asking a model that `models:` lists, each a `ModelId` setting.
So which provider and which models an app uses are written in
`app.jadpo` and its settings, where a reviewer sees them, and changing
either changes nothing else. An app that names no `ai:` has no provider
to ask, so Jadpo refuses the call at `jadpo check` rather than let it
fail the first time it runs.

## What it looks like

```jadpo check project=e-ai-connection-required file=app.jadpo hidden
application {}
```

```jadpo check project=e-ai-connection-required file=domains/support/types/customer_question.jadpo hidden
/// What a customer asked the help desk.
type CustomerQuestion = Text {
    max_length: 2000
}
```

```jadpo check project=e-ai-connection-required file=domains/support/types/support_answer.jadpo hidden
/// What the assistant says back.
type SupportAnswer = Output {
    /// False when the help articles don't answer the question.
    resolved_by_articles: Bool
    /// The reply to show the customer, at most a few short paragraphs.
    reply: Text {
        max_length: 2000
    }
}
```

```jadpo check project=e-ai-connection-required file=domains/support/failures/assistant_unavailable.jadpo hidden
/// The assistant can't answer now; a person will reply.
type AssistantUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The assistant can't answer now; a person will reply."
}
```

```jadpo check project=e-ai-connection-required file=domains/support/prompts/answer_question.jadpo hidden
use support/types { CustomerQuestion }

/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: CustomerQuestion
}
```

```jadpo check project=e-ai-connection-required file=domains/support/prompts/answer_question.prompt hidden
# Instructions

You answer questions from customers of an online bakery.

# Request

{question}
```

domains/support/actions/answer_customer.jadpo:

```jadpo check project=e-ai-connection-required file=domains/support/actions/answer_customer.jadpo expect=ai.connection_required
use support/types { CustomerQuestion, SupportAnswer }
use support/failures { AssistantUnavailable }
use support/prompts { AnswerQuestion }

/// Answers a customer's question.
action answer_customer(
    customer_question: CustomerQuestion
) fails AssistantUnavailable -> SupportAnswer {
    return attempt ai.generate(
        AnswerQuestion {
            question: customer_question
        },
        as: SupportAnswer
    ) {
        refused: AssistantUnavailable
        malformed: AssistantUnavailable
        timed_out: AssistantUnavailable
        over_budget: AssistantUnavailable
        too_long: AssistantUnavailable
    }
}
```

```text output
error: `ai.generate` asks a model, and the application names none
 --> domains/support/actions/answer_customer.jadpo:9:20
  |
9 |     return attempt ai.generate(
  |                    ^^^^^^^^^^^ no `ai:` to ask through
  reason: Every `ai` call goes through the connection `application { ai: { connection: } }` names, asking a model `models:` lists, each a `ModelId` setting, so changing a model or a provider changes `app.jadpo` and nothing else.
  next: Add `ai: { connection: <name>  models: { default: config.<name> } }` to `application { }` in `app.jadpo`, naming a connection with `driver: Driver.openai_compatible` (agent)
  rule: ai.connection_required
  help: https://jadpo.dev/docs/errors/ai.connection_required

check failed: 1 error, 6 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the app should ask a model**, connect one in `app.jadpo`: a
connection the model is reached through, and an `ai:` block naming it,
with the model as a `ModelId` setting and a daily budget. Here it is a model served on this
machine through OpenAI's Chat Completions API. In `app.jadpo`:

```jadpo fragment
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
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

```jadpo check project=e-ai-connection-required-fix-1 file=app.jadpo hidden
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
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

and in `config.jadpo`:

```jadpo check project=e-ai-connection-required-fix-1 file=config.jadpo
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-connection-required-fix-1 file=domains/support/types/customer_question.jadpo hidden
/// What a customer asked the help desk.
type CustomerQuestion = Text {
    max_length: 2000
}
```

```jadpo check project=e-ai-connection-required-fix-1 file=domains/support/types/support_answer.jadpo hidden
/// What the assistant says back.
type SupportAnswer = Output {
    /// False when the help articles don't answer the question.
    resolved_by_articles: Bool
    /// The reply to show the customer, at most a few short paragraphs.
    reply: Text {
        max_length: 2000
    }
}
```

```jadpo check project=e-ai-connection-required-fix-1 file=domains/support/failures/assistant_unavailable.jadpo hidden
/// The assistant can't answer now; a person will reply.
type AssistantUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The assistant can't answer now; a person will reply."
}
```

```jadpo check project=e-ai-connection-required-fix-1 file=domains/support/prompts/answer_question.jadpo hidden
use support/types { CustomerQuestion }

/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: CustomerQuestion
}
```

```jadpo check project=e-ai-connection-required-fix-1 file=domains/support/prompts/answer_question.prompt hidden
# Instructions

You answer questions from customers of an online bakery.

# Request

{question}
```

```jadpo check project=e-ai-connection-required-fix-1 file=domains/support/actions/answer_customer.jadpo hidden migrate=initial
use support/types { CustomerQuestion, SupportAnswer }
use support/failures { AssistantUnavailable }
use support/prompts { AnswerQuestion }

/// Answers a customer's question.
action answer_customer(
    customer_question: CustomerQuestion
) fails AssistantUnavailable -> SupportAnswer {
    return attempt ai.generate(
        AnswerQuestion {
            question: customer_question
        },
        as: SupportAnswer
    ) {
        refused: AssistantUnavailable
        malformed: AssistantUnavailable
        timed_out: AssistantUnavailable
        over_budget: AssistantUnavailable
        too_long: AssistantUnavailable
    }
}
```

`answer_customer` now asks `meta-llama/Llama-3.1-8B-Instruct`, served
at `http://localhost:8000/v1`. Moving to another model or provider
changes `config.jadpo` or `app.jadpo` and nothing else. The app asks for
at most 2,000,000 tokens a day; past that, a call ends as `over_budget`,
which `answer_customer` turns into `AssistantUnavailable`.

## Related

- [Connecting a provider](../language/21-ai.md#connecting-a-provider)
- [A model on this machine](../language/21-ai.md#a-model-on-this-machine)
- [The `ai` capability](../language/21-ai.md#the-ai-capability)
