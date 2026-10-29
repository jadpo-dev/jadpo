# `route.stream_invalid`

A route whose action returns an `AiStream` sends the model's answer as
it is written, answering 200 once the first piece has come; it takes no
other `status:` and no `settle:`:

```jadpo check project=e-route-stream-invalid file=app.jadpo hidden
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

```jadpo check project=e-route-stream-invalid file=config.jadpo hidden migrate=initial
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-route-stream-invalid file=policy.jadpo hidden
use support

policy {
    actions: {
        support.stream_answer: [Access.public]
    }
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/types/customer_question.jadpo hidden
/// What a customer asked the help desk.
type CustomerQuestion = Text {
    max_length: 2000
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/types/asked.jadpo hidden
use support/types { CustomerQuestion }

/// A customer's question, as the help desk sends it.
type Asked = Input {
    customer_question: CustomerQuestion
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/failures/assistant_unavailable.jadpo hidden
/// The assistant can't answer now; a person will reply.
type AssistantUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The assistant can't answer now; a person will reply."
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/prompts/answer_question.jadpo hidden
use support/types { CustomerQuestion }

/// Answers a customer's question.
type AnswerQuestion = Prompt {
    question: CustomerQuestion
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/prompts/answer_question.prompt hidden
# Instructions

You answer questions from customers of an online bakery.

# Request

{question}
```

```jadpo check project=e-route-stream-invalid file=domains/support/actions/stream_answer.jadpo hidden
use support/types { Asked }
use support/failures { AssistantUnavailable }
use support/prompts { AnswerQuestion }

/// Streams an answer to a customer's question as it is written.
action stream_answer(asked: Asked) fails AssistantUnavailable -> AiStream {
    return attempt ai.stream(
        AnswerQuestion {
            question: asked.customer_question
        },
        max_output: 600,
        timeout: 8s
    ) {
        refused: AssistantUnavailable
        timed_out: AssistantUnavailable
        over_budget: AssistantUnavailable
        too_long: AssistantUnavailable
    }
}
```

```jadpo check project=e-route-stream-invalid file=domains/support/routes/help_desk.jadpo expect=route.stream_invalid
use support/types { Asked }
use support/actions { stream_answer }

/// Customers' questions.
routes HelpDesk {
    prefix: /help_desk

    /// Streams the assistant's reply as it is written.
    POST / {
        authentication: RouteAuthentication.none
        input: Asked
        output: AiStream
        status: SuccessStatus.created
        run: stream_answer(input)
    }
}
```

```text output
error: `POST /help_desk` streams a model's answer with `SuccessStatus.created`
 --> domains/support/routes/help_desk.jadpo:13:9
   |
13 |         status: SuccessStatus.created
   |         ^^^^^^ not `SuccessStatus.ok`
  reason: A route whose action returns an `AiStream` sends the model's answer as server-sent events as it is written, answering 200 once its first piece has come; it has no other success to report, and nothing to wait for with `settle:`.
  next: Remove `status:`; a stream answers 200 (agent)
  rule: route.stream_invalid
  help: docs/errors/route.stream_invalid.md

check failed: 1 error, 9 files
```
