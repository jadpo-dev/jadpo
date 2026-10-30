# 28 · A lost property office

A railway's lost property office keeps what staff hand in, and answers
passengers who report a loss. Each item is placed by an embedding model,
as a vector of what its description means, once it is on the shelves. A
passenger's report is placed the same way, the unclaimed items that mean
much the same are found, and a language model drafts the reply from
them, for someone in the office to check and send. The office's website
also answers questions about its rules, streaming the answer as it is
written.

The model is OpenAI's, reached through its first-party package, but no
code names OpenAI: every call is `ai.`, so moving to another provider
changes `app.jadpo` alone. Every call has a budget; the personal data
that may reach a model is listed, and the rest can't; and no test ever
reaches a model, since one fake answers every `ai` call.

## What it teaches

1. **A model package.** `jadpo package add ../../packages/openai` installs OpenAI's package in `packages/open_ai/v1/`, locked in `jadpo.lock`; it fills the `ai` capability, with its models' context windows and prices. See `jadpo.lock`, `packages/open_ai/v1/provides/ai.jadpo` and [area 21, How a provider fills it](../../docs/language/21-ai.md#how-a-provider-fills-it).
2. **Connecting it.** `ai:` names the connection, its models, each a `ModelId` setting, and `embedding_dimensions:`, which every vector type an embedding is read as must match. See `app.jadpo`, `config.jadpo`, `domains/lost_property/types/item_meaning.jadpo` and [area 21, Connecting a provider](../../docs/language/21-ai.md#connecting-a-provider).
3. **Budgets.** `limits:` caps the tokens one request may use, the tokens and the estimated cost a day, and each caller's tokens a day; `per_day:` is required. A call that could pass a limit is refused before it is sent, as `over_budget`. See `app.jadpo` and [area 21, Budgets](../../docs/language/21-ai.md#budgets).
4. **Embeddings.** `ai.embed(text, as: V)` gives a text's vector, in an action and never in the request that saved the text: a subscriber places each item handed in. See `domains/lost_property/actions/hand_in.jadpo`, `domains/lost_property/subscribers/place_handed_in_items.jadpo`, `domains/lost_property/actions/place_item.jadpo` and [area 21, Asking for an embedding](../../docs/language/21-ai.md#asking-for-an-embedding).
5. **Many at once.** `ai.embed_all(texts, as: V)` gives up to 100 texts' vectors in one call, in order. See `domains/lost_property/actions/place_unplaced_items.jadpo`.
6. **Searching by meaning.** `ai.distance` in a query's `where:` keeps the items close to a loss's meaning; with no `nearest`, it needs no vector index. See `domains/lost_property/queries/found_like.jadpo`.
7. **Asking a model.** `ai.generate(prompt, as: T)` sends a checked prompt and decodes the answer into `T`, every rule included, asking once more if it doesn't decode; each of its five failures is mapped to the office's own. Its calls come before the action's write. See `domains/lost_property/actions/draft_reply.jadpo`, `domains/lost_property/types/passenger_reply.jadpo` and [area 21, Typed output](../../docs/language/21-ai.md#typed-output).
8. **Personal data sent to a model.** A placeholder is followed back to the fields it was read from: the passenger's name is `identifying`, which `ai: { personal_data: }` lists, and their address is `to_models: false`, so it never reaches a prompt. The model's answer carries what its prompt was built from, so the fields its reply is stored in are marked `identifying` too. See `domains/lost_property/entities/loss_report.jadpo`, `domains/lost_property/prompts/reply_to_passenger.jadpo` and [area 21, Personal data sent to a model](../../docs/language/21-ai.md#personal-data-sent-to-a-model).
9. **Work that waits.** The report's route answers 202 at once, and the reply is drafted in a subscriber, so no passenger waits on a model. See `domains/lost_property/routes/losses.jadpo` and `domains/lost_property/subscribers/draft_replies.jadpo`.
10. **Streaming to a route.** `ai.stream(prompt)` gives an `AiStream`, which a route sends as server-sent events as it is written; it maps four failures, all before its first piece, and its route gives itself a `deadline:` past the stream's 25 seconds. See `domains/lost_property/actions/ask_the_office.jadpo`, `domains/lost_property/routes/enquiries.jadpo` and [area 7, Streaming a model's answer](../../docs/language/07-http.md#streaming-a-models-answer).
11. **The fake.** In a test every `ai` call answers through one fake: embeddings from each text's digest, so the same text always means the same; scripted answers by prompt, `raw(...)` text run through the real decoding, and failures by name; and `ai.<prompt>.requests`, what was asked. See `domains/lost_property/actions/hand_in.test.jadpo`, `domains/lost_property/actions/report_loss.test.jadpo`, `domains/lost_property/actions/draft_reply.test.jadpo` and [Testing AI calls](../../docs/language/21-ai.md#testing-ai-calls).
12. **Recordings.** `recordings/reply_to_passenger/nothing_found.json` keeps an answer a model gave, with the version of the prompt it answered; `recorded("nothing_found")` answers with it, and every run of `jadpo test` decodes it. See `domains/lost_property/prompts/recordings/reply_to_passenger/nothing_found.json` and [area 21, Recorded answers](../../docs/language/21-ai.md#recorded-answers).
13. **A stream in a route test.** A streaming route's `Response` has its `pieces` and the `code` of an `error` event that ended it; `partial("...", <failure>)` stops one part-way. See `domains/lost_property/routes/enquiries.test.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/28-lost-property
target/release/jadpo test examples/28-lost-property
target/release/jadpo fmt --check examples/28-lost-property
cd examples/28-lost-property && ../../target/release/jadpo ai show reply_to_passenger
```

- `check` ends with `checked: 70 files` and no errors.
- `test` needs PostgreSQL with pgvector, and ends with `tested: 13 passed, 8 action tests, 4 route tests, 1 recording test`. Nothing reaches OpenAI, and no key is needed.
- `ai show` prints the messages the reply prompt sends, and its version, which the recording names.

## Try next

- In `app.jadpo`, take `PersonalData.identifying` out of `allowed:`. `jadpo check` reports `semantic.ai_personal_data_not_allowed`: the passenger's name reaches the reply prompt.
  <!-- try {"file": "app.jadpo", "replace": "allowed: [PersonalData.identifying]", "with": "allowed: []", "check": ["semantic.ai_personal_data_not_allowed"]} -->
- In `loss_report.jadpo`, keep the passenger's name from models too, with `to_models: false`. `jadpo check` reports `semantic.ai_field_refused` at the prompt that sends it, and `semantic.ai_field_unmarked` where the reply drafted from it is stored, in fields that aren't kept from models.
  <!-- try {"file": "domains/lost_property/entities/loss_report.jadpo", "replace": "    passenger_name: PassengerName {\n        personal: PersonalData.identifying\n", "with": "    passenger_name: PassengerName {\n        personal: PersonalData.identifying\n        to_models: false\n", "check": ["semantic.ai_field_refused", "semantic.ai_field_unmarked"]} -->
- In `draft_reply.jadpo`, take out the `malformed:` arm. `jadpo check` reports `failure.rule_unmapped`: every way a model's answer can fail is the office's to decide.
  <!-- try {"file": "domains/lost_property/actions/draft_reply.jadpo", "replace": "        malformed: ReplyUnavailable\n", "with": "", "check": ["failure.rule_unmapped"]} -->
- In `enquiries.jadpo`, take the `deadline:` out. `jadpo check` reports `route.calls_past_deadline`: the stream may take 25 seconds, longer than the 10-second default.
  <!-- try {"file": "domains/lost_property/routes/enquiries.jadpo", "replace": "        deadline: 30s\n", "with": "", "check": ["route.calls_past_deadline"]} -->
- That is the last example so far. The [language pages](../../docs/language/README.md) hold the full specification.
