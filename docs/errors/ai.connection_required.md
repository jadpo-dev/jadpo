# `ai.connection_required`

Every `ai` call goes through the connection `application { ai: }` names,
so an action that asks a model in an application that names none has
nothing to ask through.

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
