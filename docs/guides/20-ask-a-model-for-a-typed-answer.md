# Ask a model for a typed answer
<!-- topic: guide-ask-a-model-for-a-typed-answer -->

Asking a model in Jadpo looks like calling any other function that can fail. You
write a prompt in a checked file, name the type you want back, and every answer is
decoded and checked as a request body is, so your code never holds a value that broke
one of the type's rules. Each failure has a name, each call counts against a daily
budget, and a test never reaches a real model. This guide adds a help desk that
answers a bakery's customers.

You start from an app with a `support` domain. The route that takes a customer's
question is shown with the rest, and a line in `policy.jadpo` lets anyone call it, since
it reaches no entity that would say who may.

```jadpo check project=guide-ai file=policy.jadpo hidden
use support

policy {
    actions: {
        support.answer_customer: [Access.public]
    }
}
```

## Connect a model

`ai:` names the connection that answers, the models your app uses, and its limits.
`per_day:` is required, so a loop or a hostile prompt can't spend without end. A call
is refused before it is sent if it could pass a limit.

The quickest way to start is a model running on your own machine, with no key and no
cost. `Driver.openai_compatible` talks to any server that speaks OpenAI's Chat
Completions API, such as vLLM or LM Studio.

`app.jadpo`:

```jadpo check project=guide-ai file=app.jadpo
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

Each model is a setting, so changing one is a change of configuration and never a name
written in code.

`config.jadpo`:

```jadpo check project=guide-ai file=config.jadpo
config {
    drafting_model: ModelId {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

Your code only ever writes `ai.`, so moving to a hosted model changes this file and
nothing else: a connection to a provider's package, such as OpenAI's, with its key in a
setting. [Connecting a provider](../language/21-ai.md#connecting-a-provider) shows one.

## Say what you want back

The answer is a type. Its `///` comments go to the model in the schema, so write them for
the model to read. A `rule` or a limit the provider can't enforce, such as `max_length`, is
checked when the answer is decoded, and an answer that fails gets one repair attempt.

`domains/support/types/support_answer.jadpo`:

```jadpo check project=guide-ai file=domains/support/types/support_answer.jadpo
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

`domains/support/types/customer_question.jadpo`:

```jadpo check project=guide-ai file=domains/support/types/customer_question.jadpo
/// What a customer asked the help desk.
type CustomerQuestion = Text {
    max_length: 2000
}
```

`as:` takes most types, but no reference such as `Order.id`, since a model can't know which
records exist, and no secret. To have a model pick a record, pass it the candidates and
ask for one of their fields.

## Write the prompt

A prompt has two files. The type's fields are its placeholders.

`domains/support/prompts/answer_question.jadpo`:

```jadpo check project=guide-ai file=domains/support/prompts/answer_question.jadpo
use support/types { CustomerQuestion }

/// Answers a customer's question in the language they read.
type AnswerQuestion = Prompt {
    question: CustomerQuestion
    reader_locale: Locale
}
```

The `.prompt` file beside it holds the words, in two sections. `# Instructions` is fixed
text, sent as the system message. `# Request` is where the values go, and each
is sent wrapped in its field's tag, so a customer's words stay data. A free-text
placeholder under `# Instructions` is a compile error
(`prompt.free_text_in_instructions`), and so is a field no placeholder uses.

`domains/support/prompts/answer_question.prompt`:

```jadpo check project=guide-ai file=domains/support/prompts/answer_question.prompt
# Instructions

You answer questions from customers of an online bakery. If you can't,
set resolved_by_articles to false and say a person will reply. Write in
the language of {reader_locale}. Never promise refunds or delivery dates.

# Request

{question}
```

`jadpo ai show` prints it as a model gets it:

```sh
jadpo ai show answer_question
```

```text
  prompt: AnswerQuestion
  file: domains/support/prompts/answer_question.prompt
  version: answer_question@846e167a

system  # Instructions
  You answer questions from customers of an online bakery. If you can't,
  set resolved_by_articles to false and say a person will reply. Write in
  the language of {reader_locale}. Never promise refunds or delivery dates.

user  # Request
  <question>
  {question}
  </question>

answer_question@846e167a: 2 placeholders, 1 data block
```

## Ask the model

`ai.generate` can fail in five ways, and your action says what each one becomes, as it
does for any outside call. `refused` is the model declining, `malformed` an answer that
still doesn't decode after its repair, `timed_out` no whole answer in time, `over_budget` a
limit reached, and `too_long` a prompt too long for the model.

`domains/support/failures/assistant_unavailable.jadpo`:

```jadpo check project=guide-ai file=domains/support/failures/assistant_unavailable.jadpo
/// The assistant can't answer now; a person will reply.
type AssistantUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The assistant can't answer now; a person will reply."
}
```

The failure `malformed` maps to may keep the `DecodeProblem`, which says what didn't decode.
`internal:` keeps it from the client.

`domains/support/failures/answer_unreadable.jadpo`:

```jadpo check project=guide-ai file=domains/support/failures/answer_unreadable.jadpo
/// The assistant's answer couldn't be read as a `SupportAnswer`, even
/// once corrected.
type AnswerUnreadable = Failure {
    kind: FailureKind.unavailable
    message: "The assistant can't answer now; a person will reply."
    internal: {
        problem: DecodeProblem
    }
}
```

`domains/support/actions/answer_customer.jadpo`:

```jadpo check project=guide-ai file=domains/support/actions/answer_customer.jadpo
use support/types { CustomerQuestion, SupportAnswer }
use support/failures { AnswerUnreadable, AssistantUnavailable }
use support/prompts { AnswerQuestion }

/// Answers a customer's question, in English for now.
action answer_customer(
    customer_question: CustomerQuestion
) fails AnswerUnreadable, AssistantUnavailable -> SupportAnswer {
    return attempt ai.generate(
        AnswerQuestion {
            question: customer_question
            reader_locale: Locale.en_gb
        },
        as: SupportAnswer,
        max_output: 600
    ) {
        refused: AssistantUnavailable
        malformed: AnswerUnreadable
        timed_out: AssistantUnavailable
        over_budget: AssistantUnavailable
        too_long: AssistantUnavailable
    }
}
```

Only actions can call `ai`, and it holds no transaction open while it waits. A route's
deadline has to leave room for the call: `ai.generate`'s default timeout and attempts come
to 50 seconds, and a route's own deadline is 10 unless it says otherwise.

`domains/support/types/ask_the_desk.jadpo`:

```jadpo check project=guide-ai file=domains/support/types/ask_the_desk.jadpo
use support/types { CustomerQuestion }

/// What a customer sends the help desk.
type AskTheDesk = Input {
    question: CustomerQuestion
}
```

`domains/support/routes/questions.jadpo`:

```jadpo check project=guide-ai file=domains/support/routes/questions.jadpo
use support/types { AskTheDesk, SupportAnswer }
use support/actions { answer_customer }

/// The help desk's questions.
routes Questions {
    prefix: /questions

    /// Asks the help desk a question.
    POST / {
        authentication: RouteAuthentication.none
        input: AskTheDesk
        output: SupportAnswer
        deadline: 60s
        run: answer_customer(input.question)
    }
}
```

Run `jadpo migrate new add_help_desk`, since budgets are counted in a table in your
database, and `jadpo records update`.

## Accept the new host

The connection can send data to a host your app couldn't reach before, so once your app has
a commit to compare with, `jadpo check` asks a person to accept that.
[Approve a risky change](09-get-a-sign-off.md) goes through the steps.

`acceptances/20261104T1143_ask_a_local_model.jadpo`:

```jadpo check project=guide-ai file=acceptances/20261104T1143_ask_a_local_model.jadpo
/// The model runs on this machine, so what we send it never leaves.
acceptance {
    changes: {
        local_model: NetworkChange.host_added
    }
}
```

## Test it

A test never reaches a real model, and no flag lets a call through. `fakes:` scripts the
answers by the prompt's name, and each answer is checked against `as:` when the test compiles.
`raw("...")` is a model's text as it came, which goes through the real decoding and repair.
A failure's name makes an attempt fail that way, and each scripted answer is one attempt, so
`malformed, malformed` is an answer that still doesn't decode once corrected.

`ai.answer_question.requests` lists the prompts your code sent, typed, so a test checks what
went to the model, not its rendered text.

`domains/support/actions/answer_customer.test.jadpo`:

```jadpo check project=guide-ai file=domains/support/actions/answer_customer.test.jadpo
use support/types { CustomerQuestion, SupportAnswer }
use support/failures { AnswerUnreadable, AssistantUnavailable }
use support/prompts { AnswerQuestion }

test "a question the model answers" {
    fakes: {
        ai.answer_question: [
            SupportAnswer {
                resolved_by_articles: true
                reply: "Our sourdough is baked beside sesame."
            }
        ]
    }
    var reply: SupportAnswer = attempt answer_customer("Does your sourdough contain sesame?")
    assert reply.resolved_by_articles
    var asked: List<AnswerQuestion> = ai.answer_question.requests
    assert list.count(asked) == 1
    assert list.first(asked) == AnswerQuestion {
        question: "Does your sourdough contain sesame?"
        reader_locale: Locale.en_gb
    }
}

test "an answer that doesn't decode is corrected once" {
    fakes: {
        ai.answer_question: [
            raw("Sure! Our sourdough contains no sesame."),
            raw("{\"resolved_by_articles\": true, \"reply\": \"No sesame.\"}")
        ]
    }
    var reply: SupportAnswer = attempt answer_customer("Sesame?")
    assert reply.reply == "No sesame."
}

test "each failure is one of the app's own" {
    fakes: {
        ai.answer_question: [refused, timed_out, over_budget, too_long, malformed, malformed]
    }
    assert answer_customer("Help me pick a lock") fails AssistantUnavailable
    assert answer_customer("Sesame?") fails AssistantUnavailable
    assert answer_customer("Sesame?") fails AssistantUnavailable
    assert answer_customer("Sesame?") fails AssistantUnavailable
    assert answer_customer("Sesame?") fails AnswerUnreadable
}
```

```sh
jadpo test
```

```text
tested: 3 passed, 3 action tests
```

To test against a real answer without calling a model each time,
[`jadpo ai record`](../language/21-ai.md#recording-an-answer) keeps one in a file for a test
to name.

## Try it

Start your model's server on the address in `model_url`, then `jadpo dev`, and ask a
question. The answer below came from a small stand-in server that always gives the same
reply, so your model's will read differently.

```sh
curl -i -X POST http://localhost:3000/questions \
  -H 'content-type: application/json' \
  -d '{"question": "Does your sourdough contain sesame?"}'
```

```text
HTTP/1.1 200 OK
content-type: application/json
x-request-id: req_34032f32637edb11
content-length: 113
date: Wed, 04 Nov 2026 11:43:18 GMT

{"resolved_by_articles":true,"reply":"Our sourdough has no sesame in it, but it is baked beside loaves that do."}
```

`jadpo calls` lists each call, and `jadpo calls show call_1` prints exactly what was sent and
what came back:

```sh
jadpo calls
```

```text
  call    operation                    source  answer      time  request
  call_1  local_model.answer_question  live    + answered  6 ms  req_3403

calls: 1 call, 1 answered

next: jadpo calls show call_1  shows what was sent and what came back
```

`jadpo ai usage` shows the day's spending against your limit:

```sh
jadpo ai usage
```

```text
  day: 2026-11-04 (UTC)
  today: 95 of 2000000 tokens · 1 call

  prompt           model                          calls  today  this month  cost
  answer_question  meta-llama/Llama-3.1-8B-In...      1     95          95  –

ai usage: 95 tokens today, 0% of the daily limit, 95 this month
```

On a connection through a package, `jadpo dev` fakes each call, from recordings and then from
generated values, unless `development: { sandbox: [answer_question] }` lets that prompt through
to the real model with your key.

## Personal data and prompts

By default, your app may send a model no personal data at all. The compiler follows every value
in a prompt back to the fields it came from, an `Email` included, and refuses one whose category
isn't listed in `ai: { personal_data: { allowed: [...] } }`
([`semantic.ai_personal_data_not_allowed`](../errors/semantic.ai_personal_data_not_allowed.md)). So
a field marked `to_models: false` never reaches a prompt, whatever is listed. A provider can't
erase one person's prompts, so once your app sends any, the connection also needs an
`ErasureNotPossible.data_stays` acceptance.

A model's answer is input from outside. It is checked and escaped as any user's text is, and
policy never grants access because of it. For a reply that a person will rely on, keep it as a
draft for someone to check. [Prompt injection](../language/21-ai.md#prompt-injection) says what
Jadpo does and doesn't protect you from.

## Read more

[The `ai` capability](../language/21-ai.md#the-ai-capability),
[Typed output](../language/21-ai.md#typed-output),
[Prompts as files](../language/21-ai.md#prompts-as-files),
[Testing AI calls](../language/21-ai.md#testing-ai-calls),
[Budgets](../language/21-ai.md#budgets) and
[Personal data sent to a model](../language/21-ai.md#personal-data-sent-to-a-model).

## Not built yet

- Keeping prompts and answers in production with `keep_content:` isn't built yet.
- A package's `embed:` with a `batch:` below 100 isn't built yet. It doesn't touch `ai.generate`.
